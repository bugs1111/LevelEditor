//! Native counterpart of LevelRender.getSingleLayerModeFilter: the original
//! color matrix followed by Heaps' separable Gaussian, applied after compositing
//! each layer (blurring sprites separately gives visibly incorrect seams).
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
/// Capture only the just-painted layer. Missing freshly uploaded textures leave
/// the normal shapes intact for the first frame and schedule a repaint.
pub(super) fn finish(painter: &Painter, start: usize, rect: Rect, intensity: f32, key: u64) {
    let Some(state) = GPU.get() else {
        return;
    };
    let mut shapes = painter.ctx().graphics(|g| {
        g.get(painter.layer_id())
            .map(|p| p.all_entries().skip(start).cloned().collect::<Vec<_>>())
            .unwrap_or_default()
    });
    if shapes.is_empty() {
        return;
    }
    for s in &mut shapes {
        s.clip_rect = s
            .clip_rect
            .translate(-rect.min.to_vec2())
            .intersect(Rect::from_min_size(egui::Pos2::ZERO, rect.size()));
        s.shape.translate(-rect.min.to_vec2());
    }
    let ppp = painter.ctx().pixels_per_point();
    let primitives = painter.ctx().tessellate(shapes, ppp);
    let renderer = state.renderer.read();
    let mut textures = HashMap::new();
    for p in &primitives {
        if let Primitive::Mesh(m) = &p.primitive {
            let Some(t) = renderer
                .texture(&m.texture_id)
                .and_then(|t| t.texture.clone())
            else {
                painter.ctx().request_repaint();
                return;
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
        rect,
        BlurCallback {
            primitives,
            textures,
            size: [
                (rect.width() * ppp).ceil().max(1.) as u32,
                (rect.height() * ppp).ceil().max(1.) as u32,
            ],
            ppp,
            intensity,
            key,
            format: state.target_format,
        },
    ));
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
struct BlurCallback {
    primitives: Vec<ClippedPrimitive>,
    textures: HashMap<egui::TextureId, wgpu::Texture>,
    size: [u32; 2],
    ppp: f32,
    intensity: f32,
    key: u64,
    format: wgpu::TextureFormat,
}
struct Cache {
    entries: HashMap<u64, Target>,
    pipeline: Pipeline,
    scratch: Option<([u32; 2], [wgpu::TextureView; 2])>,
}
struct Pipeline {
    layout: wgpu::BindGroupLayout,
    blur: wgpu::RenderPipeline,
    composite: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
}
struct Target {
    size: [u32; 2],
    renderer: Renderer,
    textures: HashMap<egui::TextureId, egui::TextureId>,
    views: [wgpu::TextureView; 3],
    uniforms: [wgpu::Buffer; 3],
    bind: [wgpu::BindGroup; 3],
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
struct Params{step:vec2<f32>,colorize:f32,count:f32,taps:array<vec4<f32>,5>};
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
 var c=textureSample(image,linear_sampler,uv);
 if params.colorize>0.{let tint=vec3<f32>(140.,153.,193.)/255.;c=vec4<f32>(clamp(c.rgb*(1.-params.colorize)+tint*params.colorize*(c.r+c.g+c.b),vec3<f32>(0.),vec3<f32>(c.a)),c.a);}
 return c;
}
@fragment fn fs(in:VertexOut)->@location(0) vec4<f32>{
 var out=sample_color(in.uv)*params.taps[0].y;
 for(var i=1u;i<u32(params.count);i++){let offset=params.step*params.taps[i].x;out+=(sample_color(in.uv-offset)+sample_color(in.uv+offset))*params.taps[i].y;}
 return out;
}
@fragment fn copy(in:VertexOut)->@location(0) vec4<f32>{return textureSample(image,linear_sampler,in.uv);}
"#;
impl Pipeline {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("LDtk Gaussian"),
            entries: &[
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
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("LDtk original Gaussian"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let pipeline = |entry: &str, format, blend| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &module,
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
        };
        let blur = pipeline("fs", wgpu::TextureFormat::Rgba8Unorm, None);
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
            blur,
            composite,
            sampler,
        }
    }
}
fn surface(device: &wgpu::Device, size: [u32; 2]) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("LDtk inactive layer"),
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
                        resource: wgpu::BindingResource::TextureView(&views[i]),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&pipeline.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: uniforms[i].as_entire_binding(),
                    },
                ],
            })
        });
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
            size,
            renderer,
            textures: HashMap::new(),
            views,
            uniforms,
            bind,
            frame: 0,
        }
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
            resources.insert(Cache {
                entries: HashMap::new(),
                pipeline: Pipeline::new(device, self.format),
                scratch: None,
            });
        }
        let cache = resources.get_mut::<Cache>().unwrap();
        // Frame age bounds retention when switching projects or resizing a viewport.
        let frame = egui_frame();
        cache
            .entries
            .retain(|_, e| frame.saturating_sub(e.frame) < 3);
        if cache
            .scratch
            .as_ref()
            .is_none_or(|(size, _)| *size != self.size)
        {
            cache.scratch = Some((
                self.size,
                std::array::from_fn(|_| surface(device, self.size)),
            ));
            cache.entries.clear();
        }
        if cache
            .entries
            .get(&self.key)
            .is_none_or(|t| t.size != self.size)
        {
            cache.entries.insert(
                self.key,
                Target::new(
                    device,
                    self.size,
                    &cache.pipeline,
                    &cache.scratch.as_ref().unwrap().1,
                ),
            );
        }
        let target = cache.entries.get_mut(&self.key).unwrap();
        target.frame = frame;
        target.textures.retain(|old, id| {
            if self.textures.contains_key(old) {
                true
            } else {
                target.renderer.free_texture(id);
                false
            }
        });
        for (old, texture) in &self.textures {
            let view = texture.create_view(&Default::default());
            if let Some(id) = target.textures.get(old) {
                target.renderer.update_egui_texture_from_wgpu_texture(
                    device,
                    &view,
                    wgpu::FilterMode::Nearest,
                    *id,
                );
            } else {
                let id = target.renderer.register_native_texture(
                    device,
                    &view,
                    wgpu::FilterMode::Nearest,
                );
                target.textures.insert(*old, id);
            }
        }
        let mut primitives = self.primitives.clone();
        for p in &mut primitives {
            if let Primitive::Mesh(m) = &mut p.primitive {
                m.texture_id = target.textures[&m.texture_id];
            }
        }
        let screen = ScreenDescriptor {
            size_in_pixels: self.size,
            pixels_per_point: self.ppp,
        };
        let commands = target
            .renderer
            .update_buffers(device, queue, encoder, &primitives, &screen);
        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("LDtk layer composite"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &target.views[0],
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
            target.renderer.render(&mut pass, &primitives, &screen);
        }
        let rounded = (8. * self.intensity.clamp(0., 1.)).round() as u32;
        let radius = rounded.max(1).next_power_of_two() as f32;
        let taps = kernel(radius);
        for axis in 0..2 {
            let step = if axis == 0 {
                [1. / self.size[0] as f32, 0.]
            } else {
                [0., 1. / self.size[1] as f32]
            };
            queue.write_buffer(
                &target.uniforms[axis],
                0,
                &uniforms(step, if axis == 0 { self.intensity } else { 0. }, &taps),
            );
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("LDtk Gaussian pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target.views[axis + 1],
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
            pass.set_pipeline(&cache.pipeline.blur);
            pass.set_bind_group(0, &target.bind[axis], &[]);
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
            primitives: vec![ClippedPrimitive {
                clip_rect: Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(64., 64.)),
                primitive: Primitive::Mesh(mesh),
            }],
            textures: HashMap::from([(egui::TextureId::Managed(0), texture)]),
            size: [64, 64],
            ppp: 1.,
            intensity: 0.75,
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
            "/private/tmp/ldtk-gaussian-gpu.png",
            &bytes,
            64,
            64,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
}
