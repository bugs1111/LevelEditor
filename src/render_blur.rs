//! Composites each inactive layer before the Heaps Gaussian and the complete
//! canvas before the reference picker's CSS sepia filter.
use eframe::{
    egui::{
        self, Painter, Rect,
        epaint::{ClippedPrimitive, Primitive},
    },
    egui_wgpu::{
        self, CallbackResources, CallbackTrait, RenderState, Renderer, RendererOptions,
        ScreenDescriptor,
    },
};
use std::{collections::HashMap, sync::OnceLock};
use wgpu::util::DeviceExt;
static GPU: OnceLock<RenderState> = OnceLock::new();
pub(super) fn configure(state: Option<&RenderState>) {
    if let Some(state) = state {
        let _ = GPU.set(state.clone());
    }
}
pub(super) fn gpu() -> Option<&'static RenderState> {
    GPU.get()
}
pub(super) fn available() -> bool {
    GPU.get().is_some()
}
pub(super) fn begin(painter: &Painter) -> usize {
    painter.ctx().graphics(|g| {
        g.get(painter.layer_id())
            .map_or(0, |p| p.all_entries().len())
    })
}
#[derive(Clone, Copy)]
pub(super) struct LayerRaster {
    pub origin: egui::Pos2,
    pub points_per_pixel: egui::Vec2,
    pub opacity: f32,
}

pub(super) fn finish(
    painter: &Painter,
    start: usize,
    rect: Rect,
    intensity: f32,
    key: u64,
    raster: LayerRaster,
) {
    let pixels = (8. * intensity.clamp(0., 1.)).round() as u32;
    let radius = if pixels == 0 {
        0.
    } else {
        pixels.next_power_of_two() as f32
    };
    let filtered = capture(
        painter,
        start,
        rect,
        Some((raster, radius)),
        |scene, format| BlurCallback {
            scene,
            intensity,
            radius,
            opacity: raster.opacity,
            key,
            format,
        },
    );
    if !filtered && raster.opacity < 1. {
        painter.ctx().graphics_mut(|g| {
            let list = g.entry(painter.layer_id());
            let end = list.all_entries().len();
            for i in start..end {
                list.mutate_shape(egui::layers::ShapeIdx(i), |shape| {
                    egui::epaint::shape_transform::adjust_colors(&mut shape.shape, move |color| {
                        if *color != egui::Color32::PLACEHOLDER {
                            *color = color.gamma_multiply(raster.opacity);
                        }
                    });
                });
            }
        });
    }
}

pub(super) fn finish_picker(painter: &Painter, start: usize, rect: Rect) {
    capture(painter, start, rect, None, |scene, format| PickerCallback {
        scene,
        format,
    });
}

/// The blur target is aligned to the layer's own grid. Its output uses the
/// visible part of this grid, which may begin inside a texel at the viewport edge.
fn layer_region(
    raster: LayerRaster,
    content: Rect,
    viewport: Rect,
    radius: f32,
    ppp: f32,
) -> (Rect, Rect, [u32; 2]) {
    let scale = raster.points_per_pixel;
    let visible = content.expand2(scale * (2. * radius)).intersect(viewport);
    let left = ((visible.min - raster.origin) / scale + egui::Vec2::splat(1e-4)).floor();
    let right = ((visible.max - raster.origin) / scale - egui::Vec2::splat(1e-4)).ceil();
    let raster_rect =
        Rect::from_min_max(raster.origin + left * scale, raster.origin + right * scale);
    let screen_round = |point: egui::Pos2| ((point.to_vec2() * ppp).round() / ppp).to_pos2();
    let output = Rect::from_min_max(screen_round(visible.min), screen_round(visible.max));
    (
        raster_rect,
        output,
        [(right.x - left.x) as u32, (right.y - left.y) as u32],
    )
}

/// Missing freshly uploaded textures keep the direct draw for this frame.
fn capture<C: CallbackTrait + 'static>(
    painter: &Painter,
    start: usize,
    rect: Rect,
    layer: Option<(LayerRaster, f32)>,
    callback: impl FnOnce(CapturedScene, wgpu::TextureFormat) -> C,
) -> bool {
    let Some(state) = GPU.get() else {
        return false;
    };
    let shapes = painter.ctx().graphics(|g| {
        g.get(painter.layer_id())
            .map(|p| p.all_entries().skip(start).cloned().collect::<Vec<_>>())
            .unwrap_or_default()
    });
    if shapes.is_empty() {
        return false;
    }
    let screen_ppp = painter.ctx().pixels_per_point();
    let (region, output, size, capture_ppp, scale) = if let Some((raster, radius)) = layer {
        let content = shapes.iter().fold(Rect::NOTHING, |bounds, s| {
            let visible = s.shape.visual_bounding_rect().intersect(s.clip_rect);
            if visible.is_positive() {
                bounds.union(visible)
            } else {
                bounds
            }
        });
        if !content.is_positive() {
            return false;
        }
        let (region, output, size) = layer_region(raster, content, rect, radius, screen_ppp);
        (region, output, size, 1., raster.points_per_pixel)
    } else {
        let px = (rect.size() * screen_ppp).ceil();
        (
            rect,
            rect,
            [px.x as u32, px.y as u32],
            screen_ppp,
            egui::Vec2::splat(1.),
        )
    };
    if size
        .into_iter()
        .any(|n| n == 0 || n > state.device.limits().max_texture_dimension_2d)
        || !output.is_positive()
    {
        return false;
    }
    let to_local = |point: egui::Pos2| ((point - region.min) / scale).to_pos2();
    let local_rect = |r: Rect| Rect::from_min_max(to_local(r.min), to_local(r.max));
    let mut primitives = painter.ctx().tessellate(shapes, screen_ppp);
    for p in &mut primitives {
        p.clip_rect = local_rect(p.clip_rect.intersect(region));
        match &mut p.primitive {
            Primitive::Mesh(mesh) => {
                for vertex in &mut mesh.vertices {
                    vertex.pos = to_local(vertex.pos);
                }
            }
            Primitive::Callback(callback) => callback.rect = local_rect(callback.rect),
        }
    }
    let renderer = state.renderer.read();
    let mut textures = HashMap::new();
    for p in &primitives {
        if let Primitive::Mesh(m) = &p.primitive {
            let Some(t) = renderer
                .texture(&m.texture_id)
                .and_then(|t| t.texture.clone())
            else {
                painter.ctx().request_repaint();
                return false;
            };
            textures.entry(m.texture_id).or_insert(t);
        }
    }
    drop(renderer);
    painter.ctx().graphics_mut(|g| {
        let list = g.entry(painter.layer_id());
        let end = list.all_entries().len();
        for i in start..end {
            list.reset_shape(egui::layers::ShapeIdx(i));
        }
    });
    painter.add(egui_wgpu::Callback::new_paint_callback(
        output,
        callback(
            CapturedScene {
                primitives,
                textures,
                size,
                ppp: capture_ppp,
                output_uv: Rect::from_min_max(
                    ((output.min - region.min) / region.size()).to_pos2(),
                    ((output.max - region.min) / region.size()).to_pos2(),
                ),
            },
            state.target_format,
        ),
    ));
    true
}
// Same paired bilinear taps as h3d.pass.Blur.calcValues (quality=1, gain=1).
pub(super) fn kernel(radius: f32) -> Vec<[f32; 2]> {
    let width = if radius > 0. {
        ((radius - 1.).max(1.) / 2.).ceil() as usize
    } else {
        0
    };
    let gauss = |x: f32| {
        if radius == 0. {
            if x == 0. { 1. } else { 0. }
        } else {
            (-x * x / (2. * radius)).exp() / (std::f32::consts::TAU * radius).sqrt()
        }
    };
    let mut taps = Vec::new();
    let mut total = 0.;
    for i in 0..=width {
        let a = (i * 2) as f32;
        let b = if i == 0 { 0. } else { a - 1. };
        let ga = gauss(a);
        let gb = gauss(b);
        let g = ga + gb;
        taps.push([if i == 0 { 0. } else { (ga * a + gb * b) / g }, g]);
        total += g * if i == 0 { 1. } else { 2. };
    }
    let minimum = taps[0][1] * 0.01;
    while taps.len() > 2 && taps.last().unwrap()[1] <= minimum {
        total -= taps.pop().unwrap()[1] * 2.;
    }
    for t in &mut taps {
        t[1] /= total;
    }
    taps
}
struct CapturedScene {
    primitives: Vec<ClippedPrimitive>,
    textures: HashMap<egui::TextureId, wgpu::Texture>,
    size: [u32; 2],
    ppp: f32,
    output_uv: Rect,
}
struct BlurCallback {
    scene: CapturedScene,
    intensity: f32,
    radius: f32,
    opacity: f32,
    key: u64,
    format: wgpu::TextureFormat,
}
struct Cache {
    entries: HashMap<u64, Target>,
    pipeline: Pipeline,
    scratch: HashMap<[u32; 2], Scratch>,
}
struct Scratch {
    views: [wgpu::TextureView; 2],
    frame: u64,
}
struct Pipeline {
    layout: wgpu::BindGroupLayout,
    color_matrix: wgpu::RenderPipeline,
    blur: wgpu::RenderPipeline,
    composite: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    nearest: wgpu::Sampler,
}
struct Target {
    size: [u32; 2],
    renderer: SceneRenderer,
    views: [wgpu::TextureView; 3],
    uniforms: [wgpu::Buffer; 4],
    bind: [wgpu::BindGroup; 4],
    frame: u64,
}
fn uniforms(step: [f32; 2], intensity: f32, taps: &[[f32; 2]]) -> Vec<u8> {
    let mut values = vec![step[0], step[1], intensity, taps.len() as f32];
    for i in 0..5 {
        let t = taps.get(i).copied().unwrap_or([0., 0.]);
        values.extend([t[0], t[1], 0., 0.]);
    }
    values.into_iter().flat_map(f32::to_ne_bytes).collect()
}
const SHADER: &str = r#"
struct Params{step:vec2<f32>,amount:f32,count:f32,taps:array<vec4<f32>,5>};
@group(0) @binding(0) var image:texture_2d<f32>;
@group(0) @binding(1) var linear_sampler:sampler;
@group(0) @binding(2) var<uniform> params:Params;
struct VertexOut{@builtin(position) position:vec4<f32>,@location(0) uv:vec2<f32>};
@vertex fn vs(@builtin(vertex_index) index:u32)->VertexOut{
 let uv=vec2<f32>(f32((index<<1u)&2u),f32(index&2u));
 var out:VertexOut;out.position=vec4<f32>(uv.x*2.-1.,1.-uv.y*2.,0.,1.);out.uv=uv;return out;
}
fn sample_color(uv:vec2<f32>)->vec4<f32>{
 if any(uv<vec2<f32>(0.))||any(uv>vec2<f32>(1.)){return vec4<f32>(0.);}
 return textureSample(image,linear_sampler,uv);
}
@fragment fn color_matrix(in:VertexOut) -> @location(0) vec4<f32> {
 let c = textureSample(image,linear_sampler,in.uv);
 let tint = vec3<f32>(140.,153.,193.) / 255.;
 let color = c.rgb * (1. - params.amount) + tint * params.amount * (c.r + c.g + c.b);
 return vec4<f32>(min(color,vec3<f32>(c.a)),c.a);
}
@fragment fn fs(in:VertexOut)->@location(0) vec4<f32>{
 var out=sample_color(in.uv)*params.taps[0].y;
 for(var i=1u;i<u32(params.count);i++){let offset=params.step*params.taps[i].x;out+=(sample_color(in.uv-offset)+sample_color(in.uv+offset))*params.taps[i].y;}
 return out;
}
@fragment fn copy(in:VertexOut)->@location(0) vec4<f32>{
 let uv = params.taps[0].xy + in.uv * params.step;
 return textureSample(image,linear_sampler,uv) * params.amount;
}

// CSS sepia(0.4) in gamma-encoded sRGB. The scene texture is premultiplied.
fn picker_color(uv: vec2<f32>) -> vec4<f32> {
    let c = textureSample(image, linear_sampler, uv);
    let rgb = vec3<f32>(
        dot(c.rgb, vec3<f32>(0.7572, 0.3076, 0.0756)),
        dot(c.rgb, vec3<f32>(0.1396, 0.8744, 0.0672)),
        dot(c.rgb, vec3<f32>(0.1088, 0.2136, 0.6524))
    );
    return vec4<f32>(min(rgb, vec3<f32>(c.a)), c.a);
}
@fragment fn picker(in: VertexOut) -> @location(0) vec4<f32> {
    return picker_color(in.uv);
}
@fragment fn picker_srgb(in: VertexOut) -> @location(0) vec4<f32> {
    let c = picker_color(in.uv);
    let linear = select(c.rgb / 12.92, pow((c.rgb + 0.055) / 1.055, vec3<f32>(2.4)), c.rgb > vec3<f32>(0.04045));
    return vec4<f32>(linear, c.a);
}
"#;
fn texture_bindings() -> [wgpu::BindGroupLayoutEntry; 2] {
    [
        wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 1,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        },
    ]
}
fn fullscreen_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    module: &wgpu::ShaderModule,
    entry: &str,
    format: wgpu::TextureFormat,
    blend: Option<wgpu::BlendState>,
) -> wgpu::RenderPipeline {
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[layout],
        push_constant_ranges: &[],
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(entry),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some("vs"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some(entry),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        multiview: None,
        cache: None,
    })
}

fn filter_shader(device: &wgpu::Device) -> wgpu::ShaderModule {
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("LDtk canvas filters"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    })
}

impl Pipeline {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let [texture, sampler] = texture_bindings();
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("LDtk Gaussian"),
            entries: &[
                texture,
                sampler,
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let module = filter_shader(device);
        let pipeline = |entry, format, blend| {
            fullscreen_pipeline(device, &layout, &module, entry, format, blend)
        };
        let blur = pipeline("fs", wgpu::TextureFormat::Rgba8Unorm, None);
        let color_matrix = pipeline("color_matrix", wgpu::TextureFormat::Rgba8Unorm, None);
        let composite = pipeline(
            "copy",
            format,
            Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
        );
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            layout,
            color_matrix,
            blur,
            composite,
            sampler,
            nearest: device.create_sampler(&wgpu::SamplerDescriptor::default()),
        }
    }
}
fn surface(device: &wgpu::Device, size: [u32; 2]) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("LDtk filter surface"),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        })
        .create_view(&Default::default())
}
impl Target {
    fn new(
        device: &wgpu::Device,
        size: [u32; 2],
        pipeline: &Pipeline,
        scratch: &[wgpu::TextureView; 2],
    ) -> Self {
        let views = [
            scratch[0].clone(),
            scratch[1].clone(),
            surface(device, size),
        ];
        let uniforms = std::array::from_fn(|_| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: &[0; 96],
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            })
        });
        let bind = std::array::from_fn(|i| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &pipeline.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&views[i.min(2)]),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(if i == 2 {
                            &pipeline.nearest
                        } else {
                            &pipeline.sampler
                        }),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: uniforms[i].as_entire_binding(),
                    },
                ],
            })
        });
        Self {
            size,
            renderer: SceneRenderer::new(device),
            views,
            uniforms,
            bind,
            frame: 0,
        }
    }
}
struct SceneRenderer {
    renderer: Renderer,
    textures: HashMap<egui::TextureId, egui::TextureId>,
}
impl SceneRenderer {
    fn new(device: &wgpu::Device) -> Self {
        let mut renderer = Renderer::new(
            device,
            wgpu::TextureFormat::Rgba8Unorm,
            RendererOptions::default(),
        );
        renderer
            .callback_resources
            .insert(super::outline::OutputFormat(
                wgpu::TextureFormat::Rgba8Unorm,
            ));
        Self {
            renderer,
            textures: HashMap::new(),
        }
    }

    fn render(
        &mut self,
        scene: &CapturedScene,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
    ) -> Vec<wgpu::CommandBuffer> {
        self.textures.retain(|old, id| {
            if scene.textures.contains_key(old) {
                true
            } else {
                self.renderer.free_texture(id);
                false
            }
        });
        for (old, texture) in &scene.textures {
            let view = texture.create_view(&Default::default());
            if let Some(id) = self.textures.get(old) {
                self.renderer.update_egui_texture_from_wgpu_texture(
                    device,
                    &view,
                    wgpu::FilterMode::Nearest,
                    *id,
                );
            } else {
                let id =
                    self.renderer
                        .register_native_texture(device, &view, wgpu::FilterMode::Nearest);
                self.textures.insert(*old, id);
            }
        }
        let mut primitives = scene.primitives.clone();
        for p in &mut primitives {
            if let Primitive::Mesh(m) = &mut p.primitive {
                m.texture_id = self.textures[&m.texture_id];
            }
        }
        let screen = ScreenDescriptor {
            size_in_pixels: scene.size,
            pixels_per_point: scene.ppp,
        };
        let commands = self
            .renderer
            .update_buffers(device, queue, encoder, &primitives, &screen);
        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("LDtk scene composite"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                })
                .forget_lifetime();
            self.renderer.render(&mut pass, &primitives, &screen);
        }
        commands
    }
}

impl CallbackTrait for BlurCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen: &ScreenDescriptor,
        encoder: &mut wgpu::CommandEncoder,
        resources: &mut CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        if resources.get::<Cache>().is_none() {
            let format = resources
                .get::<super::outline::OutputFormat>()
                .map_or(self.format, |f| f.0);
            resources.insert(Cache {
                entries: HashMap::new(),
                pipeline: Pipeline::new(device, format),
                scratch: HashMap::new(),
            });
        }
        let cache = resources.get_mut::<Cache>().unwrap();
        // Frame age bounds retention when switching projects or resizing a viewport.
        let frame = egui_frame();
        cache
            .entries
            .retain(|_, e| frame.saturating_sub(e.frame) < 3);
        cache
            .scratch
            .retain(|_, s| frame.saturating_sub(s.frame) < 3);
        let scratch = cache
            .scratch
            .entry(self.scene.size)
            .or_insert_with(|| Scratch {
                views: std::array::from_fn(|_| surface(device, self.scene.size)),
                frame,
            });
        scratch.frame = frame;
        if cache
            .entries
            .get(&self.key)
            .is_none_or(|t| t.size != self.scene.size)
        {
            cache.entries.insert(
                self.key,
                Target::new(device, self.scene.size, &cache.pipeline, &scratch.views),
            );
        }
        let target = cache.entries.get_mut(&self.key).unwrap();
        target.frame = frame;
        let commands =
            target
                .renderer
                .render(&self.scene, device, queue, encoder, &target.views[0]);
        let taps = kernel(self.radius);
        let uv = self.scene.output_uv;
        queue.write_buffer(
            &target.uniforms[2],
            0,
            &uniforms(uv.size().into(), self.opacity, &[uv.min.into()]),
        );
        let passes = [
            (
                0,
                2,
                &cache.pipeline.color_matrix,
                uniforms([0.; 2], self.intensity, &[]),
            ),
            (
                3,
                1,
                &cache.pipeline.blur,
                uniforms([1. / self.scene.size[0] as f32, 0.], 0., &taps),
            ),
            (
                1,
                2,
                &cache.pipeline.blur,
                uniforms([0., 1. / self.scene.size[1] as f32], 0., &taps),
            ),
        ];
        for (input, output, pipeline, params) in passes {
            queue.write_buffer(&target.uniforms[input], 0, &params);
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("LDtk layer filter pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target.views[output],
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &target.bind[input], &[]);
            pass.draw(0..3, 0..1);
        }
        commands
    }
    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        pass: &mut wgpu::RenderPass<'static>,
        resources: &CallbackResources,
    ) {
        let Some(cache) = resources.get::<Cache>() else {
            return;
        };
        let Some(target) = cache.entries.get(&self.key) else {
            return;
        };
        pass.set_pipeline(&cache.pipeline.composite);
        pass.set_bind_group(0, &target.bind[2], &[]);
        pass.draw(0..3, 0..1);
    }
}

struct PickerCallback {
    scene: CapturedScene,
    format: wgpu::TextureFormat,
}

struct PickerSurface {
    size: [u32; 2],
    view: wgpu::TextureView,
    bind: wgpu::BindGroup,
}

struct PickerCache {
    renderer: SceneRenderer,
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    surface: Option<PickerSurface>,
}

impl CallbackTrait for PickerCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen: &ScreenDescriptor,
        encoder: &mut wgpu::CommandEncoder,
        resources: &mut CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        if resources.get::<PickerCache>().is_none() {
            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("LDtk picker filter"),
                entries: &texture_bindings(),
            });
            let pipeline = fullscreen_pipeline(
                device,
                &layout,
                &filter_shader(device),
                if self.format.is_srgb() {
                    "picker_srgb"
                } else {
                    "picker"
                },
                self.format,
                Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
            );
            resources.insert(PickerCache {
                renderer: SceneRenderer::new(device),
                pipeline,
                layout,
                sampler: device.create_sampler(&wgpu::SamplerDescriptor::default()),
                surface: None,
            });
        }
        let cache = resources.get_mut::<PickerCache>().unwrap();
        let size = self.scene.size;
        if cache.surface.as_ref().is_none_or(|s| s.size != size) {
            let view = surface(device, size);
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("LDtk picker filter input"),
                layout: &cache.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&cache.sampler),
                    },
                ],
            });
            cache.surface = Some(PickerSurface { size, view, bind });
        }
        cache.renderer.render(
            &self.scene,
            device,
            queue,
            encoder,
            &cache.surface.as_ref().unwrap().view,
        )
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        pass: &mut wgpu::RenderPass<'static>,
        resources: &CallbackResources,
    ) {
        if let Some(cache) = resources.get::<PickerCache>()
            && let Some(surface) = &cache.surface
        {
            pass.set_pipeline(&cache.pipeline);
            pass.set_bind_group(0, &surface.bind, &[]);
            pass.draw(0..3, 0..1);
        }
    }
}

pub(super) fn release_picker() {
    if let Some(gpu) = GPU.get() {
        gpu.renderer
            .write()
            .callback_resources
            .remove::<PickerCache>();
    }
}

static FRAME: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub(super) fn next_frame(enabled: bool) {
    FRAME.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if !enabled && let Some(gpu) = GPU.get() {
        gpu.renderer.write().callback_resources.remove::<Cache>();
    }
}
pub(super) fn egui_frame() -> u64 {
    FRAME.load(std::sync::atomic::Ordering::Relaxed)
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cropped_layer_keeps_its_texel_grid_at_fractional_viewport_edges() {
        let raster = LayerRaster {
            origin: egui::pos2(5., 4.),
            points_per_pixel: egui::vec2(2., 3.),
            opacity: 1.,
        };
        let content = Rect::from_min_max(egui::pos2(9., 10.), egui::pos2(15., 19.));
        let viewport = Rect::from_min_max(egui::pos2(10.5, 12.), egui::pos2(15., 26.));
        let (region, output, size) = layer_region(raster, content, viewport, 0., 2.);
        assert_eq!(region, content);
        assert_eq!(output, content.intersect(viewport));
        assert_eq!(size, [3, 3]);
    }

    #[test]
    fn original_heaps_gaussian_has_paired_offsets_and_unit_gain() {
        let taps = kernel(8.);
        assert_eq!(taps.len(), 5);
        assert!((taps[0][1] + 2. * taps[1..].iter().map(|t| t[1]).sum::<f32>() - 1.).abs() < 1e-6);
        assert!((taps[1][0] - 1.4532619).abs() < 1e-6);
        assert!(taps.iter().all(|t| t[1] > 0.));
    }
}

#[cfg(test)]
mod gpu_tests {
    use super::*;
    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        struct Wake(std::thread::Thread);
        impl std::task::Wake for Wake {
            fn wake(self: std::sync::Arc<Self>) {
                self.0.unpark();
            }
        }
        let waker = std::task::Waker::from(std::sync::Arc::new(Wake(std::thread::current())));
        let mut context = std::task::Context::from_waker(&waker);
        let mut future = std::pin::pin!(future);
        loop {
            match future.as_mut().poll(&mut context) {
                std::task::Poll::Ready(v) => return v,
                std::task::Poll::Pending => std::thread::park(),
            }
        }
    }
    #[test]
    #[ignore = "requires a real GPU device"]
    fn gaussian_composites_original_layer_then_blurs_on_gpu() {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter =
            block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).unwrap();
        eprintln!("GPU adapter: {:?}", adapter.get_info());
        let (device, queue) =
            block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            texture.as_image_copy(),
            &[255, 255, 255, 255],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        let mut mesh = egui::Mesh::with_texture(egui::TextureId::Managed(0));
        mesh.add_rect_with_uv(
            Rect::from_min_max(egui::pos2(24., 24.), egui::pos2(40., 40.)),
            Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
            egui::Color32::RED,
        );
        let callback = BlurCallback {
            scene: CapturedScene {
                primitives: vec![ClippedPrimitive {
                    clip_rect: Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(64., 64.)),
                    primitive: Primitive::Mesh(mesh),
                }],
                textures: HashMap::from([(egui::TextureId::Managed(0), texture)]),
                size: [64, 64],
                ppp: 1.,
                output_uv: Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
            },
            intensity: 0.75,
            radius: 8.,
            opacity: 1.,
            key: 1,
            format: wgpu::TextureFormat::Rgba8Unorm,
        };
        let mut encoder = device.create_command_encoder(&Default::default());
        let mut resources = CallbackResources::default();
        let commands = callback.prepare(
            &device,
            &queue,
            &ScreenDescriptor {
                size_in_pixels: [64, 64],
                pixels_per_point: 1.,
            },
            &mut encoder,
            &mut resources,
        );
        let target = &resources.get::<Cache>().unwrap().entries[&1];
        let output = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 64 * 64 * 4,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            target.views[2].texture().as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &output,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(64),
                },
            },
            wgpu::Extent3d {
                width: 64,
                height: 64,
                depth_or_array_layers: 1,
            },
        );
        queue.submit(commands.into_iter().chain([encoder.finish()]));
        let (tx, rx) = std::sync::mpsc::channel();
        output.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            tx.send(r).unwrap();
        });
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(std::time::Duration::from_secs(10)),
            })
            .unwrap();
        rx.recv().unwrap().unwrap();
        let bytes = output.slice(..).get_mapped_range();
        let pixel = |x: usize, y: usize| &bytes[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4];
        assert_eq!(pixel(0, 0), [0, 0, 0, 0]);
        // Full interior follows Color.getColorizeFilterH2d, not multiplicative tint.
        let middle = pixel(32, 32);
        assert!(middle[0] > 160 && middle[0] < 175, "{middle:?}");
        assert!(middle[1] > 105 && middle[1] < 125, "{middle:?}");
        assert!(middle[2] > 135 && middle[2] < 155, "{middle:?}");
        assert!(middle[3] > 245);
        assert!(
            pixel(22, 32)[3] > 0 && pixel(22, 32)[3] < 100,
            "blur must cross the original sprite boundary"
        );
        image::save_buffer(
            std::env::temp_dir().join("ldtk-gaussian-gpu.png"),
            &bytes,
            64,
            64,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
}
