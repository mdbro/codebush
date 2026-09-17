use crate::{camera::Camera, font::Atlas, geometry::*, scene::Scene};
use anyhow::{Context, Result};
use bytemuck::{Pod, Zeroable};
use std::{
    ops::Range,
    path::Path,
    sync::{Arc, Mutex},
};
use wgpu::util::DeviceExt;
#[path = "hyper_gpu.rs"]
mod hyper;
pub use hyper::HyperGpu;
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniform {
    viewport: [f32; 2],
    center: [f32; 2],
    offset: [f32; 2],
    zoom: f32,
    dpi: f32,
}
struct CachedTile {
    bounds: Rect,
    uv: [f32; 4],
    _texture: wgpu::Texture,
    group: wgpu::BindGroup,
    quad: wgpu::Buffer,
    quad_count: u32,
}
struct CachedScene {
    tiles: Vec<CachedTile>,
    scale: f32,
}
pub struct GpuScene {
    cache: Option<CachedScene>,
    buffers: Vec<wgpu::Buffer>,
    per_buffer: u32,
    pub bytes: u64,
}
impl GpuScene {
    pub fn cache_scale(&self) -> f32 {
        self.cache.as_ref().map_or(0., |c| c.scale)
    }
    pub fn empty() -> Self {
        Self {
            buffers: Vec::new(),
            per_buffer: 1,
            bytes: 0,
            cache: None,
        }
    }
}
pub struct Gpu {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub info: wgpu::AdapterInfo,
    pub errors: Arc<Mutex<Vec<String>>>,
    pipeline: wgpu::RenderPipeline,
    cache_pipeline: wgpu::RenderPipeline,
    cache_display_pipeline: wgpu::RenderPipeline,
    world_uniform: wgpu::Buffer,
    ui_uniform: wgpu::Buffer,
    world_group: wgpu::BindGroup,
    ui_group: wgpu::BindGroup,
    ui_buffer: wgpu::Buffer,
    ui_capacity: u64,
    pub format: wgpu::TextureFormat,
    pub atlas_bytes: u64,
    queries: Vec<wgpu::QuerySet>,
    resolve: Option<wgpu::Buffer>,
    timing: Option<wgpu::Buffer>,
    pub query_slot: u32,
}
impl Gpu {
    pub fn clear_color(hex: u32) -> wgpu::Color {
        let c = rgb(hex);
        let linear = |v: f32| {
            let v = v as f64;
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        wgpu::Color {
            r: linear(c[0]),
            g: linear(c[1]),
            b: linear(c[2]),
            a: 1.,
        }
    }
    pub async fn new(
        atlas: &Atlas,
        window: Option<Arc<winit::window::Window>>,
        allow_other: bool,
    ) -> Result<(Self, Option<wgpu::Surface<'static>>)> {
        let backends = wgpu::Backends::from_env().unwrap_or(if cfg!(windows) {
            wgpu::Backends::DX12
        } else {
            wgpu::Backends::VULKAN
        });
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends,
            flags: wgpu::InstanceFlags::VALIDATION,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let surface = window.map(|w| instance.create_surface(w)).transpose()?;
        Self::with_surface(atlas, instance, surface, allow_other).await
    }
    /// The OS window handle must be acquired on the window thread. All device,
    /// atlas, and source preparation after surface creation can run on a worker.
    pub async fn with_surface(
        atlas: &Atlas,
        instance: wgpu::Instance,
        surface: Option<wgpu::Surface<'static>>,
        allow_other: bool,
    ) -> Result<(Self, Option<wgpu::Surface<'static>>)> {
        let mut adapters = instance.enumerate_adapters(wgpu::Backends::all()).await;
        let names: Vec<_> = adapters.iter().map(|a| a.get_info().name).collect();
        adapters.sort_by_key(|a| !a.get_info().name.contains("5090"));
        let adapter=adapters.into_iter().find(|a|(allow_other||a.get_info().name.contains("5090"))&&surface.as_ref().is_none_or(|s|a.is_surface_supported(s))).with_context(||format!("RTX 5090 graphics adapter not available. Found: {names:?}. On this WSL workstation run the Windows build via scripts/run.sh."))?;
        let info = adapter.get_info();
        eprintln!(
            "GPU: {} / {:?} / {}",
            info.name, info.backend, info.driver_info
        );
        let features = adapter.features() & wgpu::Features::TIMESTAMP_QUERY;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("CodeBush · RTX 5090"),
                required_features: features,
                required_limits: adapter.limits(),
                ..Default::default()
            })
            .await?;
        let errors = Arc::new(Mutex::new(Vec::new()));
        let capture = errors.clone();
        device.on_uncaptured_error(Arc::new(move |e: wgpu::Error| {
            eprintln!("GPU ERROR: {e}");
            capture.lock().unwrap().push(e.to_string());
        }));
        let format = surface
            .as_ref()
            .and_then(|s| {
                s.get_capabilities(&adapter).formats.into_iter().find(|f| {
                    *f == wgpu::TextureFormat::Bgra8UnormSrgb
                        || *f == wgpu::TextureFormat::Rgba8UnormSrgb
                })
            })
            .unwrap_or(wgpu::TextureFormat::Rgba8UnormSrgb);
        let mip_count = atlas.width.max(atlas.height).ilog2() + 1;
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Shared glyph atlas"),
            size: wgpu::Extent3d {
                width: atlas.width,
                height: atlas.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: mip_count,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let mut pixels = atlas.pixels.clone();
        let mut w = atlas.width;
        let mut h = atlas.height;
        let mut atlas_bytes = 0;
        for mip in 0..mip_count {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &tex,
                    mip_level: mip,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &pixels,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(w),
                    rows_per_image: Some(h),
                },
                wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
            );
            atlas_bytes += pixels.len() as u64;
            let nw = (w / 2).max(1);
            let nh = (h / 2).max(1);
            let mut next = vec![0u8; (nw * nh) as usize];
            for y in 0..nh {
                for x in 0..nw {
                    let mut sum = 0;
                    for dy in 0..2 {
                        for dx in 0..2 {
                            sum += pixels
                                [((y * 2 + dy).min(h - 1) * w + (x * 2 + dx).min(w - 1)) as usize]
                                as u32;
                        }
                    }
                    next[(y * nw + x) as usize] = (sum / 4) as u8;
                }
            }
            pixels = next;
            w = nw;
            h = nh;
        }
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Filtered code"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("atlas.wgsl"));
        // Explicit layouts can be shared by the tree and 12D projection pipelines.
        let shared_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Shared atlas camera and image"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let shared_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Shared 2D source layout"),
                bind_group_layouts: &[Some(&shared_layout)],
                immediate_size: 0,
            });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("2D instanced code and interface"),
            layout: Some(&shared_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Quad>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![0=>Float32x4,1=>Float32x4,2=>Float32x4],
                }],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let cache_pipeline_for = |entry_point, vertex_entry| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(entry_point),
            layout: Some(&shared_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some(vertex_entry),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Quad>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![0=>Float32x4,1=>Float32x4,2=>Float32x4],
                }],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some(entry_point),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        })
        };
        let cache_pipeline = cache_pipeline_for("fs_cache", "vs");
        let cache_display_pipeline = cache_pipeline_for("fs_cache_display", "vs_cache_display");
        let world_uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Camera"),
            size: 32,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let ui_uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Interface camera"),
            size: 32,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let view = tex.create_view(&Default::default());
        let make_group = |buffer: &wgpu::Buffer| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &pipeline.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                ],
            })
        };
        let world_group = make_group(&world_uniform);
        let ui_group = make_group(&ui_uniform);
        let ui_capacity = 4 * 1024 * 1024;
        let ui_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Interface geometry"),
            size: ui_capacity,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let queries = if features.contains(wgpu::Features::TIMESTAMP_QUERY) {
            vec![device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("Frame timestamps"),
                ty: wgpu::QueryType::Timestamp,
                count: 2,
            })]
        } else {
            Vec::new()
        };
        let resolve = queries.first().map(|_| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: 256,
                usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            })
        });
        let timing = queries.first().map(|_| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: 16,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            })
        });
        Ok((
            Self {
                instance,
                adapter,
                device,
                queue,
                info,
                errors,
                pipeline,
                cache_pipeline,
                cache_display_pipeline,
                world_uniform,
                ui_uniform,
                world_group,
                ui_group,
                ui_buffer,
                ui_capacity,
                format,
                atlas_bytes,
                queries,
                resolve,
                timing,
                query_slot: 0,
            },
            surface,
        ))
    }
    pub fn upload(&self, scene: &Scene) -> GpuScene {
        let per_buffer =
            ((self.device.limits().max_buffer_size.min(256 * 1024 * 1024)) / 48) as u32;
        let buffers = scene
            .quads
            .chunks(per_buffer as usize)
            .map(|q| {
                self.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Resident source geometry"),
                        contents: bytemuck::cast_slice(q),
                        usage: wgpu::BufferUsages::VERTEX,
                    })
            })
            .collect();
        GpuScene {
            cache: None,
            buffers,
            per_buffer,
            bytes: scene.quads.len() as u64 * 48,
        }
    }
    pub fn cache_scene(
        &mut self,
        scene: &Scene,
        resident: &mut GpuScene,
        budget: u64,
    ) -> Result<()> {
        let b = scene.bounds;
        let border = 64u32;
        let edge = self.device.limits().max_texture_dimension_2d.min(16384) - border * 2;
        let mut scale = ((edge * 4) as f32 / b.w.max(b.h)).min(1.);
        // At most 4x4 texture tiles. Reserve even the worst-case instance
        // allocation before assigning the rest of the VRAM budget to images.
        let budget = budget.saturating_sub((scene.files.len() as u64 + 1) * 16 * 48);
        // Tile image detail across the available VRAM, beyond DX12's 16K texture limit.
        // Exact mip allocation includes every tile's filter gutter.
        let plan = |scale: f32| {
            let w = (b.w * scale).ceil().max(1.) as u32;
            let h = (b.h * scale).ceil().max(1.) as u32;
            let mut tiles = Vec::new();
            let mut bytes = 0u64;
            for y in (0..h).step_by(edge as usize) {
                for x in (0..w).step_by(edge as usize) {
                    let iw = edge.min(w - x);
                    let ih = edge.min(h - y);
                    let tw = iw + border * 2;
                    let th = ih + border * 2;
                    for mip in 0..=tw.max(th).ilog2() {
                        bytes += (tw >> mip).max(1) as u64 * (th >> mip).max(1) as u64 * 4;
                    }
                    tiles.push((x, y, iw, ih, tw, th));
                }
            }
            (tiles, bytes)
        };
        anyhow::ensure!(
            budget >= 1_048_576,
            "Insufficient VRAM budget for the source image cache"
        );
        while plan(scale).1 > budget {
            scale *= ((budget as f64 / plan(scale).1 as f64).sqrt() as f32 * 0.99).min(0.99);
        }
        let (tiles, total_bytes) = plan(scale);
        let mut cached_tiles = Vec::with_capacity(tiles.len());
        for (x, y, iw, ih, w, h) in tiles {
            let levels = w.max(h).ilog2() + 1;
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Resident multiresolution source image"),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: levels,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let base = texture.create_view(&wgpu::TextureViewDescriptor {
                base_mip_level: 0,
                mip_level_count: Some(1),
                ..Default::default()
            });
            let (sub, _) = self.render(
                &base,
                [w, h],
                1.,
                Rect::new(0., 0., w as f32, h as f32),
                Camera {
                    center: [
                        b.x + (x as f32 - border as f32 + w as f32 / 2.) / scale,
                        b.y + (y as f32 - border as f32 + h as f32 / 2.) / scale,
                    ],
                    zoom: scale,
                },
                scene,
                resident,
                &[],
                false,
            );
            self.wait(Some(sub))?;
            let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::MipmapFilterMode::Linear,
                ..Default::default()
            });
            let bind = |view: &wgpu::TextureView| {
                self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Cached source image"),
                    layout: &self.cache_pipeline.get_bind_group_layout(0),
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: self.world_uniform.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Sampler(&sampler),
                        },
                    ],
                })
            };
            for mip in 1..levels {
                let mw = (w >> mip).max(1);
                let mh = (h >> mip).max(1);
                let src = texture.create_view(&wgpu::TextureViewDescriptor {
                    base_mip_level: mip - 1,
                    mip_level_count: Some(1),
                    ..Default::default()
                });
                let dst = texture.create_view(&wgpu::TextureViewDescriptor {
                    base_mip_level: mip,
                    mip_level_count: Some(1),
                    ..Default::default()
                });
                let group = bind(&src);
                let q = Quad {
                    rect: [0., 0., mw as f32, mh as f32],
                    uv: [0., 0., 1., 1.],
                    color: [1.; 4],
                };
                let buffer = self
                    .device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: None,
                        contents: bytemuck::bytes_of(&q),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                let uniform = Uniform {
                    viewport: [mw as f32, mh as f32],
                    center: [0., 0.],
                    offset: [0., 0.],
                    zoom: 1.,
                    dpi: 1.,
                };
                self.queue
                    .write_buffer(&self.world_uniform, 0, bytemuck::bytes_of(&uniform));
                let mut encoder = self.device.create_command_encoder(&Default::default());
                {
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("Code mipmap"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &dst,
                            depth_slice: None,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                        multiview_mask: None,
                    });
                    pass.set_pipeline(&self.cache_pipeline);
                    pass.set_bind_group(0, &group, &[]);
                    pass.set_vertex_buffer(0, buffer.slice(..));
                    pass.draw(0..6, 0..1);
                }
                self.queue.submit([encoder.finish()]);
            }
            self.wait(None)?;
            let image_view = texture.create_view(&Default::default());
            let group = bind(&image_view);
            let bounds = Rect::new(
                b.x + x as f32 / scale,
                b.y + y as f32 / scale,
                iw as f32 / scale,
                ih as f32 / scale,
            );
            let q = Quad {
                rect: [bounds.x, bounds.y, bounds.w, bounds.h],
                uv: [
                    border as f32 / w as f32,
                    border as f32 / h as f32,
                    iw as f32 / w as f32,
                    ih as f32 / h as f32,
                ],
                color: [1., 1., 1., -1.],
            };
            let mut quads = vec![q];
            // Instanced overlays carry each light panel's exact paper color and
            // source scale. This keeps minified ink visible without guessing
            // backgrounds, changing directory decoration, or touching glyphs
            // at reading size. Every overlay samples the original source image.
            for f in scene.files.iter().filter(|f| f.theme % 2 == 0) {
                let inset = 2. * f.text_scale;
                let top = (crate::scene::HEADER + 1.) * f.text_scale;
                let body = Rect::new(
                    f.bounds.x + inset,
                    f.bounds.y + top,
                    (f.bounds.w - inset * 2.).max(0.),
                    (f.bounds.h - top - inset).max(0.),
                );
                if !body.intersects(bounds) {
                    continue;
                }
                let r = Rect::new(
                    body.x.max(bounds.x),
                    body.y.max(bounds.y),
                    (body.x + body.w).min(bounds.x + bounds.w) - body.x.max(bounds.x),
                    (body.y + body.h).min(bounds.y + bounds.h) - body.y.max(bounds.y),
                );
                let mut color = rgb(file_background(f.theme));
                color[3] = crate::scene::CODE_SIZE * f.text_scale;
                quads.push(Quad {
                    rect: [r.x, r.y, r.w, r.h],
                    uv: [
                        q.uv[0] + (r.x - bounds.x) / bounds.w * q.uv[2],
                        q.uv[1] + (r.y - bounds.y) / bounds.h * q.uv[3],
                        r.w / bounds.w * q.uv[2],
                        r.h / bounds.h * q.uv[3],
                    ],
                    color,
                });
            }
            let quad_count = quads.len() as u32;
            resident.bytes += (quads.len() * std::mem::size_of::<Quad>()) as u64;
            let quad = self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Atlas image rectangle"),
                    contents: bytemuck::cast_slice(&quads),
                    usage: wgpu::BufferUsages::VERTEX,
                });
            cached_tiles.push(CachedTile {
                bounds,
                uv: q.uv,
                _texture: texture,
                group,
                quad,
                quad_count,
            });
        }
        eprintln!(
            "Resident source image: {} tiles, {:.1} MiB",
            cached_tiles.len(),
            total_bytes as f64 / 1048576.
        );
        resident.bytes += total_bytes;
        resident.cache = Some(CachedScene {
            tiles: cached_tiles,
            scale,
        });
        Ok(())
    }
    pub fn target(&self, width: u32, height: u32) -> wgpu::Texture {
        self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Offscreen evidence / benchmark"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        })
    }
    pub fn render(
        &mut self,
        view: &wgpu::TextureView,
        size: [u32; 2],
        dpi: f32,
        viewport: Rect,
        camera: Camera,
        scene: &Scene,
        resident: &GpuScene,
        ui: &[Quad],
        timed: bool,
    ) -> (wgpu::SubmissionIndex, usize) {
        self.render_frame(
            view,
            size,
            dpi,
            viewport,
            camera,
            Some(scene),
            resident,
            ui,
            timed,
        )
    }
    pub fn render_ui(&mut self, view: &wgpu::TextureView, size: [u32; 2], dpi: f32, ui: &[Quad]) {
        self.render_frame(
            view,
            size,
            dpi,
            Rect::new(0., 0., size[0] as f32 / dpi, size[1] as f32 / dpi),
            Camera::default(),
            None,
            &GpuScene::empty(),
            ui,
            false,
        );
    }
    fn render_frame(
        &mut self,
        view: &wgpu::TextureView,
        size: [u32; 2],
        dpi: f32,
        viewport: Rect,
        camera: Camera,
        scene: Option<&Scene>,
        resident: &GpuScene,
        ui: &[Quad],
        timed: bool,
    ) -> (wgpu::SubmissionIndex, usize) {
        let world = Uniform {
            viewport: [size[0] as f32, size[1] as f32],
            center: camera.center,
            offset: viewport.center(),
            zoom: camera.zoom,
            dpi,
        };
        let screen = Uniform {
            viewport: world.viewport,
            center: [0., 0.],
            offset: [0., 0.],
            zoom: 1.,
            dpi,
        };
        self.queue
            .write_buffer(&self.world_uniform, 0, bytemuck::bytes_of(&world));
        self.queue
            .write_buffer(&self.ui_uniform, 0, bytemuck::bytes_of(&screen));
        let bytes = bytemuck::cast_slice(ui);
        if bytes.len() as u64 > self.ui_capacity {
            self.ui_capacity = (bytes.len() as u64).next_power_of_two();
            self.ui_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Interface geometry"),
                size: self.ui_capacity,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        if !bytes.is_empty() {
            self.queue.write_buffer(&self.ui_buffer, 0, bytes);
        }
        let cached = resident
            .cache
            .as_ref()
            .filter(|c| camera.zoom * dpi <= c.scale);
        let bounds = camera.view(viewport);
        let mut ranges: Vec<Range<u32>> = Vec::new();
        let visible_chunks = if cached.is_none() {
            scene.map_or_else(Vec::new, |s| s.visible_chunks(bounds))
        } else {
            Vec::new()
        };
        for i in visible_chunks {
            let c = &scene.unwrap().chunks[i];
            // Directory decoration belongs to overview scale. Enlarging its
            // world-space strokes while reading a tiny file creates huge bands;
            // the breadcrumb retains directory context at reading scale.
            if c.file.is_none() && camera.zoom > 0.65 {
                continue;
            }
            if !c.range.is_empty() {
                if let Some(last) = ranges.last_mut().filter(|r| r.end == c.range.start) {
                    last.end = c.range.end;
                } else {
                    ranges.push(c.range.clone());
                }
            }
        }
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Atlas frame"),
            });
        let mut calls = 0;
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Source and interface"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(Self::clear_color(crate::geometry::BG)),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: if timed {
                    self.queries
                        .get((self.query_slot / (wgpu::QUERY_SET_MAX_QUERIES / 2)) as usize)
                        .map(|q| wgpu::RenderPassTimestampWrites {
                            query_set: q,
                            beginning_of_pass_write_index: Some(
                                (self.query_slot * 2) % wgpu::QUERY_SET_MAX_QUERIES,
                            ),
                            end_of_pass_write_index: Some(
                                (self.query_slot * 2) % wgpu::QUERY_SET_MAX_QUERIES + 1,
                            ),
                        })
                } else {
                    None
                },
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.world_group, &[]);
            let sx = (viewport.x * dpi).max(0.) as u32;
            let sy = (viewport.y * dpi).max(0.) as u32;
            let sw = (viewport.w * dpi) as u32;
            let sh = (viewport.h * dpi) as u32;
            if sw > 0 && sh > 0 {
                pass.set_scissor_rect(sx, sy, sw.min(size[0] - sx), sh.min(size[1] - sy));
                if let Some(cache) = cached {
                    pass.set_pipeline(&self.cache_display_pipeline);
                    for tile in cache.tiles.iter().filter(|t| t.bounds.intersects(bounds)) {
                        pass.set_bind_group(0, &tile.group, &[]);
                        pass.set_vertex_buffer(0, tile.quad.slice(..));
                        pass.draw(0..6, 0..tile.quad_count);
                        calls += 1;
                    }
                }
                for r in ranges {
                    let mut start = r.start;
                    while start < r.end {
                        let bi = start / resident.per_buffer;
                        let offset = bi * resident.per_buffer;
                        let end = r.end.min(offset + resident.per_buffer);
                        pass.set_vertex_buffer(0, resident.buffers[bi as usize].slice(..));
                        pass.draw(0..6, start - offset..end - offset);
                        calls += 1;
                        start = end;
                    }
                }
            }
            pass.set_pipeline(&self.pipeline);
            pass.set_scissor_rect(0, 0, size[0], size[1]);
            pass.set_bind_group(0, &self.ui_group, &[]);
            pass.set_vertex_buffer(0, self.ui_buffer.slice(..));
            pass.draw(0..6, 0..ui.len() as u32);
        }
        if timed {
            if let (Some(q), Some(r), Some(t)) = (
                self.queries
                    .get((self.query_slot / (wgpu::QUERY_SET_MAX_QUERIES / 2)) as usize),
                &self.resolve,
                &self.timing,
            ) {
                let slot = self.query_slot;
                let local = (slot * 2) % wgpu::QUERY_SET_MAX_QUERIES;
                encoder.resolve_query_set(q, local..local + 2, r, slot as u64 * 256);
                encoder.copy_buffer_to_buffer(r, slot as u64 * 256, t, slot as u64 * 16, 16);
            }
        }
        (self.queue.submit([encoder.finish()]), calls)
    }
    pub fn finish_frame(&self) -> Result<()> {
        use std::sync::atomic::{AtomicBool, Ordering};
        let done = Arc::new(AtomicBool::new(false));
        let notify = done.clone();
        self.queue
            .on_submitted_work_done(move || notify.store(true, Ordering::Release));
        let start = std::time::Instant::now();
        while !done.load(Ordering::Acquire) {
            self.device.poll(wgpu::PollType::Poll)?;
            std::hint::spin_loop();
            anyhow::ensure!(
                start.elapsed().as_secs() < 30,
                "GPU frame completion timed out"
            );
        }
        Ok(())
    }
    pub fn wait(&self, index: Option<wgpu::SubmissionIndex>) -> Result<()> {
        self.device.poll(wgpu::PollType::Wait {
            submission_index: index,
            timeout: Some(std::time::Duration::from_secs(30)),
        })?;
        Ok(())
    }
    pub fn configure_timing(&mut self, slots: u32) -> Result<()> {
        anyhow::ensure!(
            slots > 0 && slots as u64 * 256 <= 128 * 1024 * 1024,
            "Benchmark timing exceeds the 128 MiB diagnostic allocation reserve"
        );
        if !self.queries.is_empty() {
            let per_set = wgpu::QUERY_SET_MAX_QUERIES / 2;
            self.queries = (0..slots)
                .step_by(per_set as usize)
                .map(|start| {
                    self.device.create_query_set(&wgpu::QuerySetDescriptor {
                        label: Some("Batched frame timestamps"),
                        ty: wgpu::QueryType::Timestamp,
                        count: (slots - start).min(per_set) * 2,
                    })
                })
                .collect();
            self.resolve = Some(self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Batched query resolve"),
                size: slots as u64 * 256,
                usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            }));
            self.timing = Some(self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Batched query readback"),
                size: slots as u64 * 16,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            }));
        }
        Ok(())
    }
    pub fn gpu_ms(&self) -> Result<Option<f64>> {
        Ok(self.gpu_frame_times()?.first().copied())
    }
    pub fn gpu_frame_times(&self) -> Result<Vec<f64>> {
        let Some(b) = &self.timing else {
            return Ok(Vec::new());
        };
        let (tx, rx) = std::sync::mpsc::channel();
        b.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.wait(None)?;
        rx.recv()??;
        let data = b.slice(..).get_mapped_range();
        let v: &[u64] = bytemuck::cast_slice(&data);
        let times = v
            .chunks_exact(2)
            .map(|v| (v[1] - v[0]) as f64 * self.queue.get_timestamp_period() as f64 / 1e6)
            .collect();
        drop(data);
        b.unmap();
        Ok(times)
    }
    pub fn save_png(&self, texture: &wgpu::Texture, path: &Path) -> Result<()> {
        let width = texture.width();
        let height = texture.height();
        let stride = (width * 4).div_ceil(256) * 256;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Screenshot readback"),
            size: stride as u64 * height as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(stride),
                    rows_per_image: Some(height),
                },
            },
            texture.size(),
        );
        let submission = self.queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.wait(Some(submission))?;
        rx.recv()??;
        let mapped = buffer.slice(..).get_mapped_range();
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        for row in mapped.chunks(stride as usize) {
            pixels.extend_from_slice(&row[..(width * 4) as usize]);
        }
        if matches!(
            self.format,
            wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
        ) {
            for p in pixels.chunks_exact_mut(4) {
                p.swap(0, 2);
            }
        }
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p)?;
        }
        let file = std::io::BufWriter::new(std::fs::File::create(path)?);
        let mut enc = png::Encoder::new(file, width, height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header()?;
        writer.write_image_data(&pixels)?;
        drop(mapped);
        buffer.unmap();
        Ok(())
    }
}
