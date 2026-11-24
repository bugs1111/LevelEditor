//! Heaps Outline2D used by field flows: quality .6 means 60 radial samples,
//! alpha is 1, and Outline.apply runs the pass twice. Static flows cache their
//! GPU result; only a cache miss renders the flow and runs the two passes.
use eframe::{
    egui::{
        self, Color32, Painter, Pos2, Rect,
        epaint::{ClippedPrimitive, Primitive},
    },
    egui_wgpu::{
        self, CallbackResources, CallbackTrait, Renderer, RendererOptions, ScreenDescriptor,
    },
};
use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
};
use wgpu::util::DeviceExt;
pub(super) struct OutputFormat(pub wgpu::TextureFormat);
const BUDGET: u64 = 64 * 1024 * 1024;
pub(super) fn available() -> bool {
    super::blur::available()
}
pub(super) fn finish(
    painter: &Painter,
    start: usize,
    color: Color32,
    opacity: f32,
    origin: Pos2,
    scale: f32,
) {
    let fallback = || {
        if opacity >= 1. {
            return;
        }
        painter.ctx().graphics_mut(|g| {
            let list = g.entry(painter.layer_id());
            let end = list.all_entries().len();
            for i in start..end {
                list.mutate_shape(egui::layers::ShapeIdx(i), |shape| {
                    egui::epaint::shape_transform::adjust_colors(&mut shape.shape, move |color| {
                        if *color != Color32::PLACEHOLDER {
                            *color = color.gamma_multiply(opacity);
                        }
                    })
                });
            }
        });
    };
    let Some(state) = super::blur::gpu() else {
        return;
    };
    if !scale.is_finite() || scale <= 0. {
        fallback();
        return;
    }
    let shapes = painter.ctx().graphics(|g| {
        g.get(painter.layer_id())
            .map(|p| p.all_entries().skip(start).cloned().collect::<Vec<_>>())
            .unwrap_or_default()
    });
    if shapes.is_empty() {
        return;
    }
    let ppp = painter.ctx().pixels_per_point();
    let bounds = shapes.iter().fold(Rect::NOTHING, |b, s| {
        b.union(s.shape.visual_bounding_rect())
    });
    let margin = 2. * scale / ppp;
    let visible = bounds
        .expand(margin)
        .intersect(painter.clip_rect().expand(margin));
    if !visible.is_finite() || !visible.intersects(painter.clip_rect()) {
        return;
    }
    let density = ppp / scale;
    // Capture at the field flow's own texel density and phase, even if its
    // projected position or display scale is fractional.
    let rect = Rect::from_min_max(
        origin + ((visible.min - origin) * density).floor() / density,
        origin + ((visible.max - origin) * density).ceil() / density,
    );
    let viewport = rect.intersect(painter.ctx().viewport_rect());
    let viewport = Rect::from_min_max(
        ((viewport.min.to_vec2() * ppp).floor() / ppp).to_pos2(),
        ((viewport.max.to_vec2() * ppp).ceil() / ppp).to_pos2(),
    );
    if !rect.is_positive() || !viewport.is_positive() {
        return;
    }
    let mapping = [
        viewport.width() / rect.width(),
        viewport.height() / rect.height(),
        (viewport.left() - rect.left()) / rect.width(),
        (viewport.top() - rect.top()) / rect.height(),
    ];
    let mut shapes = shapes;
    for shape in &mut shapes {
        shape.clip_rect = shape
            .clip_rect
            .translate(-rect.min.to_vec2())
            .intersect(Rect::from_min_size(egui::Pos2::ZERO, rect.size()));
        shape.shape.translate(-rect.min.to_vec2());
    }
    let primitives = painter.ctx().tessellate(shapes, density);
    let mut textures = HashMap::new();
    let renderer = state.renderer.read();
    for p in &primitives {
        if let Primitive::Mesh(m) = &p.primitive {
            let Some(t) = renderer
                .texture(&m.texture_id)
                .and_then(|t| t.texture.clone())
            else {
                painter.ctx().request_repaint();
                fallback();
                return;
            };
            textures.entry(m.texture_id).or_insert(t);
        } else {
            fallback();
            return;
        }
    }
    drop(renderer);
    let size = [
        (rect.width() * density).round().max(1.) as u32,
        (rect.height() * density).round().max(1.) as u32,
    ];
    if size
        .iter()
        .any(|n| *n > state.device.limits().max_texture_dimension_2d)
    {
        fallback();
        return;
    }
    let key = cache_key(&primitives, size, density, color, opacity, mapping);
    painter.ctx().graphics_mut(|g| {
        let list = g.entry(painter.layer_id());
        let end = list.all_entries().len();
        for i in start..end {
            list.reset_shape(egui::layers::ShapeIdx(i));
        }
    });
    painter.add(egui_wgpu::Callback::new_paint_callback(
        viewport,
        OutlineCallback {
            primitives,
            textures,
            size,
            ppp: density,
            color,
            opacity,
            mapping,
            key,
            format: state.target_format,
        },
    ));
}
fn cache_key(
    primitives: &[ClippedPrimitive],
    size: [u32; 2],
    ppp: f32,
    color: Color32,
    opacity: f32,
    mapping: [f32; 4],
) -> u64 {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    size.hash(&mut hash);
    ppp.to_bits().hash(&mut hash);
    color.hash(&mut hash);
    opacity.to_bits().hash(&mut hash);
    mapping.map(f32::to_bits).hash(&mut hash);
    for p in primitives {
        for n in [
            p.clip_rect.min.x,
            p.clip_rect.min.y,
            p.clip_rect.max.x,
            p.clip_rect.max.y,
        ] {
            n.to_bits().hash(&mut hash);
        }
        if let Primitive::Mesh(m) = &p.primitive {
            m.texture_id.hash(&mut hash);
            m.indices.hash(&mut hash);
            for v in &m.vertices {
                for n in [v.pos.x, v.pos.y, v.uv.x, v.uv.y] {
                    n.to_bits().hash(&mut hash);
                }
                v.color.hash(&mut hash);
            }
        }
    }
    hash.finish()
}
struct OutlineCallback {
    primitives: Vec<ClippedPrimitive>,
    textures: HashMap<egui::TextureId, wgpu::Texture>,
    size: [u32; 2],
    ppp: f32,
    color: Color32,
    opacity: f32,
    mapping: [f32; 4],
    key: u64,
    format: wgpu::TextureFormat,
}
struct Entry {
    _view: wgpu::TextureView,
    bind: wgpu::BindGroup,
    frame: u64,
    bytes: u64,
}
struct Cache {
    pipeline: Pipeline,
    renderer: Renderer,
    textures: HashMap<egui::TextureId, (egui::TextureId, u64)>,
    entries: HashMap<u64, Entry>,
    bytes: u64,
}
struct Pipeline {
    layout: wgpu::BindGroupLayout,
    outline: wgpu::RenderPipeline,
    composite: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
}
const SHADER: &str = r#"
struct Params { size:vec2<f32>, opacity:f32, padding:f32, color:vec4<f32>, mapping:vec4<f32> };
@group(0) @binding(0) var image:texture_2d<f32>;
@group(0) @binding(1) var linear_sampler:sampler;
@group(0) @binding(2) var<uniform> params:Params;
struct Out{@builtin(position) position:vec4<f32>,@location(0) uv:vec2<f32>};
@vertex fn vs(@builtin(vertex_index) i:u32)->Out{let uv=vec2<f32>(f32((i<<1u)&2u),f32(i&2u));var out:Out;out.position=vec4<f32>(uv.x*2.-1.,1.-uv.y*2.,0.,1.);out.uv=uv;return out;}
fn sample_color(uv:vec2<f32>)->vec4<f32>{if any(uv<vec2<f32>(0.))||any(uv>vec2<f32>(1.)){return vec4<f32>(0.);}return textureSampleLevel(image,linear_sampler,uv,0.);}
@fragment fn fs(input:Out)->@location(0) vec4<f32>{let own=sample_color(input.uv);var maximum=0.;var angle=0.;for(var i=0u;i<60u;i++){angle+=6.283185307179586/60.;let a=sample_color(input.uv+vec2<f32>(cos(angle),sin(angle))/params.size).a;maximum=max(maximum,a);}let alpha=max(maximum,own.a);let rgb=(own.rgb+params.color.rgb*(1.-own.a))*alpha;return vec4<f32>(rgb,alpha);}
@fragment fn copy(input:Out)->@location(0) vec4<f32>{
    let texel = vec2<i32>(floor((input.uv * params.mapping.xy + params.mapping.zw) * params.size));
    if any(texel < vec2<i32>(0)) || any(texel >= vec2<i32>(params.size)) { return vec4<f32>(0.); }
    return textureLoad(image, texel, 0) * params.opacity;
}
"#;
impl Pipeline {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("LDtk field outline"),
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
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Heaps Outline2D"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let pipeline = |entry: &str, format, blend| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(&pl),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
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
        Self {
            outline: pipeline("fs", wgpu::TextureFormat::Rgba8Unorm, None),
            composite: pipeline(
                "copy",
                format,
                Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
            ),
            layout,
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            }),
        }
    }
    fn bind(
        &self,
        device: &wgpu::Device,
        view: &wgpu::TextureView,
        buffer: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: buffer.as_entire_binding(),
                },
            ],
        })
    }
}
fn surface(device: &wgpu::Device, size: [u32; 2]) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("LDtk field flow"),
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
impl CallbackTrait for OutlineCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen: &ScreenDescriptor,
        _encoder: &mut wgpu::CommandEncoder,
        resources: &mut CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        if resources.get::<Cache>().is_none() {
            let format = resources.get::<OutputFormat>().map_or(self.format, |f| f.0);
            resources.insert(Cache {
                pipeline: Pipeline::new(device, format),
                renderer: Renderer::new(
                    device,
                    wgpu::TextureFormat::Rgba8Unorm,
                    RendererOptions::default(),
                ),
                textures: HashMap::new(),
                entries: HashMap::new(),
                bytes: 0,
            });
        }
        let cache = resources.get_mut::<Cache>().unwrap();
        let frame = super::blur::egui_frame();
        if let Some(entry) = cache.entries.get_mut(&self.key) {
            entry.frame = frame;
            return vec![];
        }
        cache.entries.retain(|_, entry| {
            let keep = frame.saturating_sub(entry.frame) < 3;
            if !keep {
                cache.bytes -= entry.bytes;
            }
            keep
        });
        while cache.bytes > BUDGET {
            let oldest = cache
                .entries
                .iter()
                .filter(|(_, e)| e.frame != frame)
                .min_by_key(|(_, e)| e.frame)
                .map(|(k, _)| *k);
            let Some(key) = oldest else {
                break;
            };
            cache.bytes -= cache.entries.remove(&key).unwrap().bytes;
        }
        cache.textures.retain(|_, (id, used)| {
            if frame.saturating_sub(*used) < 3 {
                true
            } else {
                cache.renderer.free_texture(id);
                false
            }
        });
        for (old, texture) in &self.textures {
            if let Some((_, used)) = cache.textures.get_mut(old) {
                *used = frame;
            } else {
                let id = cache.renderer.register_native_texture(
                    device,
                    &texture.create_view(&Default::default()),
                    wgpu::FilterMode::Nearest,
                );
                cache.textures.insert(*old, (id, frame));
            }
        }
        let mut primitives = self.primitives.clone();
        for p in &mut primitives {
            if let Primitive::Mesh(m) = &mut p.primitive {
                m.texture_id = cache.textures[&m.texture_id].0;
            }
        }
        let views = [
            surface(device, self.size),
            surface(device, self.size),
            surface(device, self.size),
        ];
        let c = self.color;
        let params = [
            self.size[0] as f32,
            self.size[1] as f32,
            self.opacity,
            0.,
            c.r() as f32 / 255.,
            c.g() as f32 / 255.,
            c.b() as f32 / 255.,
            1.,
        ]
        .into_iter()
        .chain(self.mapping)
        .flat_map(f32::to_ne_bytes)
        .collect::<Vec<_>>();
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: &params,
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let bind = views
            .iter()
            .map(|v| cache.pipeline.bind(device, v, &buffer))
            .collect::<Vec<_>>();
        let screen = ScreenDescriptor {
            size_in_pixels: self.size,
            pixels_per_point: self.ppp,
        };
        let mut encoder = device.create_command_encoder(&Default::default());
        let commands =
            cache
                .renderer
                .update_buffers(device, queue, &mut encoder, &primitives, &screen);
        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("LDtk field composite"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &views[0],
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
            cache.renderer.render(&mut pass, &primitives, &screen);
        }
        for i in 0..2 {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Heaps Outline2D pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &views[i + 1],
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
            pass.set_pipeline(&cache.pipeline.outline);
            pass.set_bind_group(0, &bind[i], &[]);
            pass.draw(0..3, 0..1);
        }
        // Submit cache misses now, before the shared egui renderer's vertex buffers
        // can be reused by another flow. Later parent passes consume these textures.
        queue.submit(commands.into_iter().chain([encoder.finish()]));
        let bytes = self.size[0] as u64 * self.size[1] as u64 * 4;
        cache.bytes += bytes;
        cache.entries.insert(
            self.key,
            Entry {
                _view: views[2].clone(),
                bind: bind[2].clone(),
                frame,
                bytes,
            },
        );
        vec![]
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
        let Some(entry) = cache.entries.get(&self.key) else {
            return;
        };
        pass.set_pipeline(&cache.pipeline.composite);
        pass.set_bind_group(0, &entry.bind, &[]);
        pass.draw(0..3, 0..1);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        struct Wake(std::thread::Thread);
        impl std::task::Wake for Wake {
            fn wake(self: std::sync::Arc<Self>) {
                self.0.unpark();
            }
        }
        let waker = std::task::Waker::from(std::sync::Arc::new(Wake(std::thread::current())));
        let mut cx = std::task::Context::from_waker(&waker);
        let mut future = std::pin::pin!(future);
        loop {
            match future.as_mut().poll(&mut cx) {
                std::task::Poll::Ready(v) => return v,
                std::task::Poll::Pending => std::thread::park(),
            }
        }
    }
    #[test]
    #[ignore = "requires Metal/Vulkan GPU"]
    fn field_outline_uses_two_sixty_sample_passes_and_reuses_cached_flow() {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter =
            block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).unwrap();
        eprintln!("GPU adapter: {:?}", adapter.get_info());
        let (device, queue) =
            block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
        let tex = device.create_texture(&wgpu::TextureDescriptor {
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
            tex.as_image_copy(),
            &[255; 4],
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
            Color32::WHITE,
        );
        let callback = OutlineCallback {
            primitives: vec![ClippedPrimitive {
                clip_rect: Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(64., 64.)),
                primitive: Primitive::Mesh(mesh),
            }],
            textures: HashMap::from([(egui::TextureId::Managed(0), tex)]),
            size: [64, 64],
            ppp: 1.,
            color: Color32::from_rgb(80, 20, 10),
            opacity: 1.,
            mapping: [0.5, 0.5, 0.25, 0.25],
            key: 1,
            format: wgpu::TextureFormat::Bgra8Unorm,
        };
        let mut encoder = device.create_command_encoder(&Default::default());
        let mut resources = CallbackResources::default();
        resources.insert(OutputFormat(wgpu::TextureFormat::Rgba8Unorm));
        let screen = ScreenDescriptor {
            size_in_pixels: [64, 64],
            pixels_per_point: 1.,
        };
        callback.prepare(&device, &queue, &screen, &mut encoder, &mut resources);
        callback.prepare(&device, &queue, &screen, &mut encoder, &mut resources);
        let cache = resources.get::<Cache>().unwrap();
        assert_eq!(cache.entries.len(), 1);
        assert_eq!(cache.bytes, 64 * 64 * 4);
        let composited = surface(&device, [64, 64]);
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("nested field callback"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &composited,
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
            pass.set_pipeline(&cache.pipeline.composite);
            pass.set_bind_group(0, &cache.entries[&1].bind, &[]);
            pass.draw(0..3, 0..1);
        }
        let output = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 64 * 64 * 4,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            composited.texture().as_image_copy(),
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
        queue.submit([encoder.finish()]);
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
        assert_eq!(pixel(32, 32), [255; 4]);
        // The projected field is at 2×. A local outline texel occupies two
        // identical destination pixels, and the adjacent texel stays clear.
        assert_eq!(pixel(12, 32), [80, 20, 10, 255]);
        assert_eq!(pixel(13, 32), pixel(12, 32));
        assert_eq!(pixel(11, 32), [0; 4]);
        assert!(pixel(12, 12)[3] > 0 && pixel(12, 12)[3] < 255);
        image::save_buffer(
            std::env::temp_dir().join("ldtk-outline-gpu.png"),
            &bytes,
            64,
            64,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
}
