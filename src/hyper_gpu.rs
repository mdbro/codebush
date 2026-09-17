use super::*;
use crate::{
    projection::{self, Matrix},
    references::Graph,
};
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Node {
    p: [f32; 12],
    bounds: [f32; 4],
    meta: [f32; 4],
    mask: [u32; 4],
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Card {
    quad: Quad,
    id: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ProjectionUniform {
    u: [f32; 12],
    v: [f32; 12],
    weights: [f32; 68],
    flags: [u32; 4],
}
struct Layer {
    group: wgpu::BindGroup,
    cards: Vec<(wgpu::Buffer, u32)>,
    file_cards: Vec<Vec<(usize, u32)>>,
    edges: wgpu::Buffer,
    edge_count: u32,
    nodes: Vec<Node>,
    chunks: Vec<Vec<usize>>,
}
pub struct HyperGpu {
    source: wgpu::RenderPipeline,
    cache: wgpu::RenderPipeline,
    edge: wgpu::RenderPipeline,
    uniform: wgpu::Buffer,
    file_group: wgpu::BindGroup,
    layers: [Layer; 2],
    pub bytes: u64,
}
pub struct HyperFrame {
    pub submission: wgpu::SubmissionIndex,
    pub detail_instances: u64,
}
impl HyperGpu {
    pub fn rotation_bounds(&self, mode: usize, rotation: &projection::Rotation) -> Rect {
        // Include every connected file, even those absent at both endpoints.
        let files: Vec<_> = self.layers[mode]
            .nodes
            .iter()
            .filter(|n| n.mask[..3].iter().any(|&m| m != 0))
            .map(|n| {
                (
                    n.p.map(f64::from),
                    [n.bounds[2] * n.meta[0], n.bounds[3] * n.meta[0]],
                )
            })
            .collect();
        rotation.swept_bounds(&files)
    }

    pub fn new(gpu: &Gpu, graph: &Graph, scenes: &[Scene; 2], residents: &[GpuScene; 2]) -> Self {
        let d = &gpu.device;
        let uniform = d.create_buffer(&wgpu::BufferDescriptor {
            label: Some("12D orthonormal projection"),
            size: 384,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Immutable 12D nodes"),
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
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let fl = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Source file selection"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(16),
                },
                count: None,
            }],
        });
        let mut ids = vec![0u32; graph.positions.len().max(1) * 64];
        for i in 0..graph.positions.len() {
            ids[i * 64] = i as u32;
        }
        let fb = d.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Immutable source indices"),
            contents: bytemuck::cast_slice(&ids),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let file_group = d.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &fl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &fb,
                    offset: 0,
                    size: wgpu::BufferSize::new(16),
                }),
            }],
        });
        let shader = d.create_shader_module(wgpu::include_wgsl!("hyper.wgsl"));
        let build = |vs: &str,
                     fs: &str,
                     stride: u64,
                     attrs: &[wgpu::VertexAttribute],
                     base: &wgpu::BindGroupLayout,
                     with_file: bool| {
            let layouts = if with_file {
                vec![Some(base), Some(&layout), Some(&fl)]
            } else {
                vec![Some(base), Some(&layout)]
            };
            let pl = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &layouts,
                immediate_size: 0,
            });
            d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(vs),
                layout: Some(&pl),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vs),
                    compilation_options: Default::default(),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: stride,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: attrs,
                    }],
                },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: gpu.format,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let source = build(
            "source_vs",
            "source_fs",
            48,
            &wgpu::vertex_attr_array![0=>Float32x4,1=>Float32x4,2=>Float32x4],
            &gpu.pipeline.get_bind_group_layout(0),
            true,
        );
        let cache = build(
            "card_vs",
            "card_fs",
            52,
            &wgpu::vertex_attr_array![0=>Float32x4,1=>Float32x4,2=>Float32x4,3=>Uint32],
            &gpu.cache_pipeline.get_bind_group_layout(0),
            false,
        );
        let edge = build(
            "edge_vs",
            "edge_fs",
            16,
            &wgpu::vertex_attr_array![0=>Uint32x4],
            &gpu.pipeline.get_bind_group_layout(0),
            false,
        );
        let mut bytes = (ids.len() * 4 + 384) as u64;
        let layers = std::array::from_fn(|mode| {
            let scene = &scenes[mode];
            let resident = &residents[mode];
            let mut nodes = vec![Node::zeroed(); graph.positions.len().max(1)];
            for f in &scene.files {
                let b = f.bounds;
                nodes[f.file] = Node {
                    p: graph.positions[f.file].map(|v| v as f32),
                    bounds: [b.x, b.y, b.w, b.h],
                    meta: [
                        720. / b.w.max(b.h).max(1.),
                        resident.cache_scale(),
                        f.text_scale,
                        0.,
                    ],
                    mask: [
                        graph.masks[mode][f.file][0],
                        graph.masks[mode][f.file][1],
                        graph.masks[mode][f.file][2],
                        0,
                    ],
                };
            }
            let nb = d.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Resident 12D file anchors"),
                contents: bytemuck::cast_slice(&nodes),
                usage: wgpu::BufferUsages::STORAGE,
            });
            bytes += (nodes.len() * 96) as u64;
            let group = d.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: uniform.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: nb.as_entire_binding(),
                    },
                ],
            });
            let edges: Vec<[u32; 4]> = graph
                .edges
                .iter()
                .map(|e| {
                    [
                        e.from as u32,
                        e.to as u32,
                        e.plane as u32,
                        if mode == 0 && e.test_only { 0 } else { 1 },
                    ]
                })
                .collect();
            let edge_count = edges.len() as u32;
            bytes += (edges.len() * 16) as u64;
            let edges = d.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Resident directed references"),
                contents: if edges.is_empty() {
                    bytemuck::bytes_of(&[0u32; 4])
                } else {
                    bytemuck::cast_slice(&edges)
                },
                usage: wgpu::BufferUsages::VERTEX,
            });
            let mut file_cards = vec![Vec::new(); graph.positions.len()];
            let cards = resident
                .cache
                .as_ref()
                .unwrap()
                .tiles
                .iter()
                .enumerate()
                .map(|(tile_id, tile)| {
                    let mut cards = Vec::new();
                    for f in &scene.files {
                        let b = f.bounds;
                        let t = tile.bounds;
                        if !b.intersects(t) {
                            continue;
                        }
                        let r = Rect::new(
                            b.x.max(t.x),
                            b.y.max(t.y),
                            (b.x + b.w).min(t.x + t.w) - b.x.max(t.x),
                            (b.y + b.h).min(t.y + t.h) - b.y.max(t.y),
                        );
                        let uv = [
                            tile.uv[0] + (r.x - t.x) / t.w * tile.uv[2],
                            tile.uv[1] + (r.y - t.y) / t.h * tile.uv[3],
                            r.w / t.w * tile.uv[2],
                            r.h / t.h * tile.uv[3],
                        ];
                        file_cards[f.file].push((tile_id, cards.len() as u32));
                        cards.push(Card {
                            quad: Quad {
                                rect: [r.x, r.y, r.w, r.h],
                                uv,
                                color: rgb(file_background(f.theme)),
                            },
                            id: f.file as u32,
                        });
                    }
                    bytes += (cards.len() * 52) as u64;
                    let count = cards.len() as u32;
                    let empty_card = Card::zeroed();
                    let b = d.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Resident source panel fragments"),
                        contents: if cards.is_empty() {
                            bytemuck::bytes_of(&empty_card)
                        } else {
                            bytemuck::cast_slice(&cards)
                        },
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                    (b, count)
                })
                .collect();
            let mut chunks = vec![Vec::new(); graph.positions.len()];
            for (i, c) in scene.chunks.iter().enumerate() {
                if let Some(id) = c.file {
                    chunks[id].push(i);
                }
            }
            Layer {
                group,
                cards,
                file_cards,
                edges,
                edge_count,
                nodes,
                chunks,
            }
        });
        Self {
            source,
            cache,
            edge,
            uniform,
            file_group,
            layers,
            bytes,
        }
    }
    pub fn scale(&self, mode: usize, id: usize) -> f32 {
        self.layers[mode].nodes[id].meta[0]
    }
    pub fn reading_scale(&self, mode: usize, id: usize) -> f32 {
        let meta = self.layers[mode].nodes[id].meta;
        meta[0] * meta[2]
    }
    pub fn bounds(&self, mode: usize, id: usize, r: &Matrix) -> Rect {
        let n = &self.layers[mode].nodes[id];
        let p = projection::project(r, n.p.map(|v| v as f64));
        let w = n.bounds[2] * n.meta[0];
        let h = n.bounds[3] * n.meta[0];
        Rect::new(p[0] - w / 2., p[1] - h / 2., w, h)
    }
    pub fn cache_scale(&self, mode: usize, id: usize) -> f32 {
        self.layers[mode].nodes[id].meta[1]
    }
    pub fn render(
        &self,
        gpu: &mut Gpu,
        view: &wgpu::TextureView,
        size: [u32; 2],
        dpi: f32,
        vp: Rect,
        camera: Camera,
        r: &Matrix,
        mode: usize,
        scene: &Scene,
        resident: &GpuScene,
        active: &[(usize, f32)],
        selected: Option<usize>,
        ui: &[Quad],
        timed: bool,
    ) -> HyperFrame {
        let world = Uniform {
            viewport: [size[0] as f32, size[1] as f32],
            center: camera.center,
            offset: vp.center(),
            zoom: camera.zoom,
            dpi,
        };
        gpu.queue
            .write_buffer(&gpu.world_uniform, 0, bytemuck::bytes_of(&world));
        gpu.queue.write_buffer(
            &gpu.ui_uniform,
            0,
            bytemuck::bytes_of(&Uniform {
                viewport: world.viewport,
                center: [0.; 2],
                offset: [0.; 2],
                zoom: 1.,
                dpi,
            }),
        );
        let w = projection::weights(r);
        let mut weights = [0.; 68];
        weights[..66].copy_from_slice(&w);
        gpu.queue.write_buffer(
            &self.uniform,
            0,
            bytemuck::bytes_of(&ProjectionUniform {
                u: r[0].map(|v| v as f32),
                v: r[1].map(|v| v as f32),
                weights,
                flags: [selected.map_or(u32::MAX, |id| id as u32), 0, 0, 0],
            }),
        );
        let bytes = bytemuck::cast_slice(ui);
        if bytes.len() as u64 > gpu.ui_capacity {
            gpu.ui_capacity = (bytes.len() as u64).next_power_of_two();
            gpu.ui_buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: gpu.ui_capacity,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        if !bytes.is_empty() {
            gpu.queue.write_buffer(&gpu.ui_buffer, 0, bytes);
        }
        let layer = &self.layers[mode];
        let world_view = camera.view(vp);
        let mut detailed = Vec::new();
        for &(id, _) in active {
            let n = &layer.nodes[id];
            if camera.zoom * dpi * n.meta[0] <= n.meta[1] {
                continue;
            }
            let b = self.bounds(mode, id, r);
            if !b.intersects(world_view) {
                continue;
            }
            let local = Rect::new(
                (world_view.x - b.x) / n.meta[0] + n.bounds[0],
                (world_view.y - b.y) / n.meta[0] + n.bounds[1],
                world_view.w / n.meta[0],
                world_view.h / n.meta[0],
            );
            for &i in &layer.chunks[id] {
                let c = &scene.chunks[i];
                if c.bounds.intersects(local) {
                    detailed.push((id, c.range.clone()));
                }
            }
        }
        let detail_instances = detailed.iter().map(|(_, r)| (r.end - r.start) as u64).sum();
        let mut enc = gpu.device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Orthographic 12D source projection"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(Gpu::clear_color(0x080c11)),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: if timed {
                    gpu.queries
                        .get((gpu.query_slot / (wgpu::QUERY_SET_MAX_QUERIES / 2)) as usize)
                        .map(|q| wgpu::RenderPassTimestampWrites {
                            query_set: q,
                            beginning_of_pass_write_index: Some(
                                gpu.query_slot * 2 % wgpu::QUERY_SET_MAX_QUERIES,
                            ),
                            end_of_pass_write_index: Some(
                                gpu.query_slot * 2 % wgpu::QUERY_SET_MAX_QUERIES + 1,
                            ),
                        })
                } else {
                    None
                },
                occlusion_query_set: None,
                multiview_mask: None,
            });
            let sx = (vp.x * dpi) as u32;
            let sy = (vp.y * dpi) as u32;
            pass.set_scissor_rect(
                sx,
                sy,
                ((vp.w * dpi) as u32).min(size[0] - sx),
                ((vp.h * dpi) as u32).min(size[1] - sy),
            );
            pass.set_pipeline(&self.edge);
            pass.set_bind_group(0, &gpu.world_group, &[]);
            pass.set_bind_group(1, &layer.group, &[]);
            pass.set_vertex_buffer(0, layer.edges.slice(..));
            pass.draw(0..9, 0..layer.edge_count);
            pass.set_pipeline(&self.cache);
            pass.set_bind_group(1, &layer.group, &[]);
            for ((buffer, count), tile) in layer
                .cards
                .iter()
                .zip(&resident.cache.as_ref().unwrap().tiles)
            {
                pass.set_bind_group(0, &tile.group, &[]);
                pass.set_vertex_buffer(0, buffer.slice(..));
                pass.draw(0..6, 0..*count);
            }
            // Selection changes annotation stacking only. The fixed 12D anchors,
            // links and their projected endpoints remain exactly unchanged.
            for foreground in [false, true] {
                if foreground {
                    if let Some(id) = selected {
                        pass.set_pipeline(&self.cache);
                        pass.set_bind_group(1, &layer.group, &[]);
                        for &(tile, instance) in &layer.file_cards[id] {
                            pass.set_bind_group(
                                0,
                                &resident.cache.as_ref().unwrap().tiles[tile].group,
                                &[],
                            );
                            pass.set_vertex_buffer(0, layer.cards[tile].0.slice(..));
                            pass.draw(0..6, instance..instance + 1);
                        }
                    }
                }
                pass.set_pipeline(&self.source);
                pass.set_bind_group(0, &gpu.world_group, &[]);
                pass.set_bind_group(1, &layer.group, &[]);
                for (id, range) in &detailed {
                    if (Some(*id) == selected) != foreground {
                        continue;
                    }
                    pass.set_bind_group(2, &self.file_group, &[(*id * 256) as u32]);
                    let mut start = range.start;
                    while start < range.end {
                        let bi = start / resident.per_buffer;
                        let off = bi * resident.per_buffer;
                        let end = range.end.min(off + resident.per_buffer);
                        pass.set_vertex_buffer(0, resident.buffers[bi as usize].slice(..));
                        pass.draw(0..6, start - off..end - off);
                        start = end;
                    }
                }
            }
            pass.set_pipeline(&gpu.pipeline);
            pass.set_bind_group(0, &gpu.ui_group, &[]);
            pass.set_scissor_rect(0, 0, size[0], size[1]);
            pass.set_vertex_buffer(0, gpu.ui_buffer.slice(..));
            pass.draw(0..6, 0..ui.len() as u32);
        }
        if timed {
            if let (Some(q), Some(resolve), Some(timing)) = (
                gpu.queries
                    .get((gpu.query_slot / (wgpu::QUERY_SET_MAX_QUERIES / 2)) as usize),
                &gpu.resolve,
                &gpu.timing,
            ) {
                let slot = gpu.query_slot;
                let local = slot * 2 % wgpu::QUERY_SET_MAX_QUERIES;
                enc.resolve_query_set(q, local..local + 2, resolve, slot as u64 * 256);
                enc.copy_buffer_to_buffer(resolve, slot as u64 * 256, timing, slot as u64 * 16, 16);
            }
        }
        HyperFrame {
            submission: gpu.queue.submit([enc.finish()]),
            detail_instances,
        }
    }
}
