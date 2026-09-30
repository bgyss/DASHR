//! Explicit raster pass graph shared by native display and offscreen captures.
use crate::{
    asset::{self, Vertex},
    gpu_resources::{GpuContext, Texture},
    material,
    settings::{Settings, Uniforms},
    shaders, topology,
};
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::{path::Path, time::Instant};

pub struct Frame {
    pub planes: Vec<Texture>,
    pub depth: Texture,
    pub width: u32,
    pub height: u32,
}
impl Frame {
    pub fn new(ctx: &GpuContext, width: u32, height: u32) -> Result<Self> {
        let planes = (0..4)
            .map(|i| Texture::float(ctx, &format!("frame plane {i}"), width, height))
            .collect::<Result<_>>()?;
        let depth = Texture::new(
            ctx,
            "reverse-Z depth",
            width,
            height,
            wgpu::TextureFormat::Depth32Float,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        )?;
        Ok(Self {
            planes,
            depth,
            width,
            height,
        })
    }
}
pub struct TimingTicket {
    buffer: wgpu::Buffer,
    names: Vec<String>,
}
impl TimingTicket {
    pub fn read(self, ctx: &GpuContext) -> Result<Value> {
        let data = ctx.map_buffer(&self.buffer)?;
        let stamps: Vec<_> = data
            .chunks_exact(8)
            .map(|p| u64::from_le_bytes(p.try_into().unwrap()))
            .collect();
        let mut result = serde_json::Map::new();
        for (i, name) in self.names.iter().enumerate() {
            result.insert(
                name.clone(),
                json!(
                    (stamps[2 * i + 1].saturating_sub(stamps[2 * i]) as f64)
                        * ctx.queue.get_timestamp_period() as f64
                        / 1e6
                ),
            );
        }
        Ok(Value::Object(result))
    }
}

pub struct Renderer {
    pub ctx: GpuContext,
    pub frame: Frame,
    pub raw: Vec<Texture>,
    pub warp: Vec<Texture>,
    pub teleport: Texture,
    pub edgefill: Texture,
    pub provenance: material::MaterialProvenance,
    pub bake_ms: f64,
    pub seam_edges: usize,
    vertex: wgpu::Buffer,
    index: wgpu::Buffer,
    index_count: u32,
    uniform: wgpu::Buffer,
    uniform_group: wgpu::BindGroup,
    edge_group: wgpu::BindGroup,
    trace_group: wgpu::BindGroup,
    deform_pipelines: Vec<wgpu::RenderPipeline>,
    edge_pipelines: Vec<wgpu::RenderPipeline>,
    trace_pipelines: Vec<wgpu::RenderPipeline>,
    display_pipeline: wgpu::RenderPipeline,
    pub last_timings: Option<Value>,
}
const ATTRS: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![0=>Float32x3,1=>Float32x3,2=>Float32x4,3=>Float32x3,4=>Float32x3,5=>Float32x3];
fn vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<Vertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &ATTRS,
    }
}
pub(crate) fn pipeline(
    ctx: &GpuContext,
    layout: &wgpu::PipelineLayout,
    pass: &str,
    count: u32,
    first: u32,
) -> wgpu::RenderPipeline {
    let source = shaders::source(pass, ctx.policy.filtered, count, first);
    let module = ctx
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(pass),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
    let buffers = if pass == "edgefill" {
        vec![]
    } else {
        vec![Some(vertex_layout())]
    };
    let targets = vec![
        Some(wgpu::ColorTargetState {
            format: wgpu::TextureFormat::Rgba32Float,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL
        });
        count as usize
    ];
    ctx.device
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(pass),
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &buffers,
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &targets,
            }),
            primitive: wgpu::PrimitiveState {
                front_face: wgpu::FrontFace::Cw,
                cull_mode: if pass == "trace" {
                    Some(wgpu::Face::Back)
                } else {
                    None
                },
                ..Default::default()
            },
            depth_stencil: if pass == "trace" {
                Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::GreaterEqual),
                    stencil: Default::default(),
                    bias: Default::default(),
                })
            } else {
                None
            },
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        })
}
pub(crate) fn bindings(
    ctx: &GpuContext,
    layout: &wgpu::BindGroupLayout,
    warp: &[Texture],
    maps: [&Texture; 2],
    materials: [&Texture; 3],
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    let views = [
        &warp[0].view,
        &warp[1].view,
        &warp[2].view,
        &warp[3].view,
        &maps[0].view,
        &maps[1].view,
        &materials[0].view,
        &materials[1].view,
        &materials[2].view,
    ];
    let mut entries: Vec<_> = views
        .iter()
        .enumerate()
        .map(|(i, v)| wgpu::BindGroupEntry {
            binding: i as u32,
            resource: wgpu::BindingResource::TextureView(v),
        })
        .collect();
    entries.push(wgpu::BindGroupEntry {
        binding: 9,
        resource: wgpu::BindingResource::Sampler(sampler),
    });
    ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("DASHR sampled resources"),
        layout,
        entries: &entries,
    })
}
pub(crate) fn attachments(
    textures: &[Texture],
    color: wgpu::Color,
) -> Vec<Option<wgpu::RenderPassColorAttachment<'_>>> {
    textures
        .iter()
        .map(|t| {
            Some(wgpu::RenderPassColorAttachment {
                view: &t.view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(color),
                    store: wgpu::StoreOp::Store,
                },
            })
        })
        .collect()
}
impl Renderer {
    pub async fn headless(s: &Settings, root: &Path, manual: bool, planes: u32) -> Result<Self> {
        let ctx = GpuContext::new(GpuContext::instance(), None, manual, planes).await?;
        Self::new(ctx, s, root)
    }
    pub fn new(ctx: GpuContext, s: &Settings, root: &Path) -> Result<Self> {
        s.validate()?;
        let start = Instant::now();
        let mesh = asset::procedural(s.mesh, s.around, s.long, s.length, s.radius, s.thickness)?;
        let material = material::load(root, s.texture_set)?;
        let raw = (0..4)
            .map(|i| Texture::float(&ctx, &format!("raw inverse plane {i}"), s.atlas, s.atlas))
            .collect::<Result<Vec<_>>>()?;
        let warp = (0..4)
            .map(|i| Texture::float(&ctx, &format!("gutter inverse plane {i}"), s.atlas, s.atlas))
            .collect::<Result<Vec<_>>>()?;
        let teleport = Texture::float(&ctx, "static seam destination/distance", s.atlas, s.atlas)?;
        let edgefill = Texture::float(&ctx, "static gutter source", s.atlas, s.atlas)?;
        let usage = wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING;
        let height = Texture::new(
            &ctx,
            "height byte UNORM",
            material.height.width,
            material.height.height,
            wgpu::TextureFormat::Rgba8Unorm,
            usage,
        )?;
        height.upload(&ctx, &material.height.rgba, 4);
        let albedo = Texture::new(
            &ctx,
            "albedo byte UNORM",
            material.albedo.width,
            material.albedo.height,
            wgpu::TextureFormat::Rgba8Unorm,
            usage,
        )?;
        albedo.upload(&ctx, &material.albedo.rgba, 4);
        let normal = Texture::new(
            &ctx,
            "reference SNORM normals",
            material.height.width,
            material.height.height,
            wgpu::TextureFormat::Rgba8Snorm,
            usage,
        )?;
        normal.upload(&ctx, &material.normal, 4);
        let sampler = ctx.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("reference linear clamp"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let uniform = ctx.buffer(
            "544-byte column-major uniform contract",
            bytemuck::bytes_of(&s.uniforms(false)),
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        );
        let uniform_layout =
            ctx.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("uniform layout"),
                    entries: &[wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: wgpu::BufferSize::new(
                                std::mem::size_of::<Uniforms>() as u64,
                            ),
                        },
                        count: None,
                    }],
                });
        let uniform_group = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("uniform bind group"),
            layout: &uniform_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        let mut entries: Vec<_> = (0..9)
            .map(|i| wgpu::BindGroupLayoutEntry {
                binding: i,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float {
                        filterable: i >= 6 || ctx.policy.filtered,
                    },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            })
            .collect();
        entries.push(wgpu::BindGroupLayoutEntry {
            binding: 9,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        });
        let sampled_layout =
            ctx.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("sampled texture layout"),
                    entries: &entries,
                });
        let edge_group = bindings(
            &ctx,
            &sampled_layout,
            &raw,
            [&teleport, &edgefill],
            [&height, &albedo, &normal],
            &sampler,
        );
        let trace_group = bindings(
            &ctx,
            &sampled_layout,
            &warp,
            [&teleport, &edgefill],
            [&height, &albedo, &normal],
            &sampler,
        );
        let deform_layout = ctx
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("deformation layout"),
                bind_group_layouts: &[Some(&uniform_layout)],
                immediate_size: 0,
            });
        let full_layout = ctx
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("gutter/trace layout"),
                bind_group_layouts: &[Some(&uniform_layout), Some(&sampled_layout)],
                immediate_size: 0,
            });
        let width = ctx.policy.planes;
        let deform_pipelines = (0..4)
            .step_by(width as usize)
            .map(|first| pipeline(&ctx, &deform_layout, "deform", width, first))
            .collect();
        let edge_pipelines = (0..4)
            .step_by(width as usize)
            .map(|first| pipeline(&ctx, &full_layout, "edgefill", width, first))
            .collect();
        let trace_pipelines = (0..4)
            .step_by(width as usize)
            .map(|first| pipeline(&ctx, &full_layout, "trace", width, first))
            .collect();
        let display_pipeline = pipeline(&ctx, &full_layout, "trace", 1, 0);
        ctx.check()?;
        let vertex = ctx.buffer(
            "metric vertices",
            bytemuck::cast_slice(&mesh.vertices),
            wgpu::BufferUsages::VERTEX,
        );
        let index = ctx.buffer(
            "procedural triangle indices",
            bytemuck::cast_slice(&mesh.indices),
            wgpu::BufferUsages::INDEX,
        );
        let frame = Frame::new(&ctx, s.width, s.height)?;
        let mut renderer = Self {
            ctx,
            frame,
            raw,
            warp,
            teleport,
            edgefill,
            provenance: material.provenance,
            bake_ms: 0.,
            seam_edges: 0,
            vertex,
            index,
            index_count: mesh.indices.len() as u32,
            uniform,
            uniform_group,
            edge_group,
            trace_group,
            deform_pipelines,
            edge_pipelines,
            trace_pipelines,
            display_pipeline,
            last_timings: None,
        };
        // Reproduce the original occupancy bake on the real GPU using identity bones.
        let mut bake = s.uniforms(false);
        bake.bones = [glam::Mat4::IDENTITY.to_cols_array_2d(); 4];
        renderer
            .ctx
            .queue
            .write_buffer(&renderer.uniform, 0, bytemuck::bytes_of(&bake));
        let mut encoder = renderer
            .ctx
            .device
            .create_command_encoder(&Default::default());
        renderer.encode_deform(&mut encoder, None, &mut Vec::new());
        renderer.ctx.queue.submit([encoder.finish()]);
        let occupancy: Vec<_> = renderer
            .ctx
            .read_float(&renderer.raw[0].texture)?
            .iter()
            .map(|p| p[3] > 0.5)
            .collect();
        let maps = topology::bake(&mesh, s.atlas, &occupancy)?;
        renderer
            .teleport
            .upload(&renderer.ctx, bytemuck::cast_slice(&maps.teleport), 16);
        renderer
            .edgefill
            .upload(&renderer.ctx, bytemuck::cast_slice(&maps.edgefill), 16);
        renderer.seam_edges = maps.seam_edges;
        renderer.bake_ms = start.elapsed().as_secs_f64() * 1000.;
        let _ = renderer.submit(s, true)?;
        renderer.ctx.wait()?;
        Ok(renderer)
    }
    fn geometry<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) {
        pass.set_vertex_buffer(0, self.vertex.slice(..));
        pass.set_index_buffer(self.index.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.index_count, 0, 0..1);
    }
    fn encode_deform(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        query: Option<&wgpu::QuerySet>,
        names: &mut Vec<String>,
    ) {
        let width = self.ctx.policy.planes as usize;
        for (i, pipeline) in self.deform_pipelines.iter().enumerate() {
            let colors = attachments(
                &self.raw[i * width..(i + 1) * width],
                wgpu::Color::TRANSPARENT,
            );
            let timestamps = timestamp(query, names, format!("deform-{i}"));
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("UV deformation"),
                color_attachments: &colors,
                timestamp_writes: timestamps,
                ..Default::default()
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &self.uniform_group, &[]);
            self.geometry(&mut pass);
        }
    }
    pub fn submit(&self, s: &Settings, diagnostics: bool) -> Result<Option<TimingTicket>> {
        s.validate()?;
        ensure!(
            s.width == self.frame.width && s.height == self.frame.height,
            "resize frame resources before rendering"
        );
        self.ctx.queue.write_buffer(
            &self.uniform,
            0,
            bytemuck::bytes_of(&s.uniforms(diagnostics)),
        );
        let query = if self
            .ctx
            .device
            .features()
            .contains(wgpu::Features::TIMESTAMP_QUERY)
        {
            Some(self.ctx.device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("asynchronous frame pass timestamps"),
                ty: wgpu::QueryType::Timestamp,
                count: 24,
            }))
        } else {
            None
        };
        let mut names = Vec::new();
        let mut encoder = self.ctx.device.create_command_encoder(&Default::default());
        self.encode_deform(&mut encoder, query.as_ref(), &mut names);
        let width = self.ctx.policy.planes as usize;
        for (i, pipeline) in self.edge_pipelines.iter().enumerate() {
            let colors = attachments(
                &self.warp[i * width..(i + 1) * width],
                wgpu::Color::TRANSPARENT,
            );
            let timestamps = timestamp(query.as_ref(), &mut names, format!("edgefill-{i}"));
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("gutter and distortion"),
                color_attachments: &colors,
                timestamp_writes: timestamps,
                ..Default::default()
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &self.uniform_group, &[]);
            pass.set_bind_group(1, &self.edge_group, &[]);
            pass.draw(0..3, 0..1);
        }
        let pipelines = if diagnostics {
            self.trace_pipelines.iter().collect::<Vec<_>>()
        } else {
            vec![&self.display_pipeline]
        };
        for (i, pipeline) in pipelines.into_iter().enumerate() {
            let slice = if diagnostics {
                &self.frame.planes[i * width..(i + 1) * width]
            } else {
                &self.frame.planes[..1]
            };
            let colors = attachments(
                slice,
                if i == 0 {
                    wgpu::Color {
                        r: 0.02,
                        g: 0.025,
                        b: 0.035,
                        a: 0.,
                    }
                } else {
                    wgpu::Color::TRANSPARENT
                },
            );
            let mut colors = colors;
            for (slot, attachment) in colors.iter_mut().enumerate() {
                let logical = if diagnostics { i * width + slot } else { 0 };
                if logical != 0 {
                    attachment.as_mut().unwrap().ops.load =
                        wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT);
                }
            }
            let timestamps = timestamp(query.as_ref(), &mut names, format!("trace-{i}"));
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shell and DASHR tracing"),
                color_attachments: &colors,
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.frame.depth.view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0.),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: timestamps,
                ..Default::default()
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &self.uniform_group, &[]);
            pass.set_bind_group(1, &self.trace_group, &[]);
            self.geometry(&mut pass);
        }
        let ticket = if let Some(query) = query {
            let size = (names.len() * 16) as u64;
            let resolve = self.ctx.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("timestamp resolve"),
                size,
                usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });
            let buffer = self.ctx.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("asynchronous timestamp readback"),
                size,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            encoder.resolve_query_set(&query, 0..(names.len() * 2) as u32, &resolve, 0);
            encoder.copy_buffer_to_buffer(&resolve, 0, &buffer, 0, size);
            Some(TimingTicket { buffer, names })
        } else {
            None
        };
        self.ctx.queue.submit([encoder.finish()]);
        self.ctx.check()?;
        Ok(ticket)
    }
    pub fn resize(&mut self, width: u32, height: u32) -> Result<()> {
        self.frame = Frame::new(&self.ctx, width, height)?;
        Ok(())
    }
    pub fn atlas_planes(&self) -> Result<Vec<Vec<[f32; 4]>>> {
        self.warp
            .iter()
            .map(|p| self.ctx.read_float(&p.texture))
            .collect()
    }
    pub fn render_capture(&mut self, s: &Settings) -> Result<Vec<Vec<[f32; 4]>>> {
        let ticket = self.submit(s, true)?;
        let result = self
            .frame
            .planes
            .iter()
            .map(|p| self.ctx.read_float(&p.texture))
            .collect::<Result<Vec<_>>>()?;
        self.last_timings = ticket.map(|t| t.read(&self.ctx)).transpose()?;
        self.ctx.check()?;
        Ok(result)
    }
}
fn timestamp<'a>(
    query: Option<&'a wgpu::QuerySet>,
    names: &mut Vec<String>,
    name: String,
) -> Option<wgpu::RenderPassTimestampWrites<'a>> {
    query.map(|q| {
        let index = (names.len() * 2) as u32;
        names.push(name);
        wgpu::RenderPassTimestampWrites {
            query_set: q,
            beginning_of_pass_write_index: Some(index),
            end_of_pass_write_index: Some(index + 1),
        }
    })
}
