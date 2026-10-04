//! Explicit raster pass graph shared by native display and offscreen captures.
use crate::{
    asset::{self, Vertex},
    asset_format::AssetDocument,
    gpu_resources::{GpuContext, Texture},
    map_cache::BakedMapCache,
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
    inverse: Option<Vec<Texture>>,
    pub teleport: Texture,
    pub teleport_destination: Option<Texture>,
    pub split_teleport: bool,
    pub compact_warp: bool,
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
    inverse_pipelines: Vec<wgpu::RenderPipeline>,
    edge_compute: Option<(wgpu::ComputePipeline, wgpu::BindGroup)>,
    indirect_edgefill: bool,
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
    pipeline_format(
        ctx,
        layout,
        pass,
        count,
        first,
        wgpu::TextureFormat::Rgba32Float,
    )
}

fn pipeline_format(
    ctx: &GpuContext,
    layout: &wgpu::PipelineLayout,
    pass: &str,
    count: u32,
    first: u32,
    target_format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let source = shaders::source(pass, ctx.policy.filtered, count, first);
    let module = ctx
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(pass),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
    let buffers = if pass == "edgefill" || pass == "inverse" {
        vec![]
    } else {
        vec![Some(vertex_layout())]
    };
    let targets = vec![
        Some(wgpu::ColorTargetState {
            format: target_format,
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
    auxiliary: [&Texture; 5],
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
    entries.extend(
        auxiliary
            .iter()
            .enumerate()
            .map(|(index, texture)| wgpu::BindGroupEntry {
                binding: 10 + index as u32,
                resource: wgpu::BindingResource::TextureView(&texture.view),
            }),
    );
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

    pub async fn headless_with_asset(
        s: &Settings,
        root: &Path,
        asset: AssetDocument,
        manual: bool,
        planes: u32,
    ) -> Result<Self> {
        Self::headless_with_asset_and_cache(s, root, asset, manual, planes, None).await
    }

    pub async fn headless_with_asset_and_cache(
        s: &Settings,
        root: &Path,
        asset: AssetDocument,
        manual: bool,
        planes: u32,
        cache_path: Option<&Path>,
    ) -> Result<Self> {
        let ctx = GpuContext::new(GpuContext::instance(), None, manual, planes).await?;
        Self::with_asset_and_cache(ctx, s, root, asset, cache_path)
    }

    pub fn new(ctx: GpuContext, s: &Settings, root: &Path) -> Result<Self> {
        s.validate()?;
        let start = Instant::now();
        let mesh = asset::procedural(s.mesh, s.around, s.long, s.length, s.radius, s.thickness)?;
        let material = material::load(root, s.texture_set)?;
        Self::build(ctx, s, mesh, material, start, None)
    }

    pub fn with_asset(
        ctx: GpuContext,
        s: &Settings,
        root: &Path,
        asset: AssetDocument,
    ) -> Result<Self> {
        Self::with_asset_and_cache(ctx, s, root, asset, None)
    }

    pub fn with_asset_and_cache(
        ctx: GpuContext,
        s: &Settings,
        root: &Path,
        asset: AssetDocument,
        cache_path: Option<&Path>,
    ) -> Result<Self> {
        s.validate()?;
        asset.validate_pose_topology(&[glam::Mat4::IDENTITY; 4])?;
        let start = Instant::now();
        let mesh = asset.into_mesh();
        let material = material::load(root, s.texture_set)?;
        Self::build(ctx, s, mesh, material, start, cache_path)
    }

    fn build(
        ctx: GpuContext,
        s: &Settings,
        mesh: asset::Mesh,
        material: material::Material,
        start: Instant,
        cache_path: Option<&Path>,
    ) -> Result<Self> {
        let warp_format = if s.compact_warp {
            let features = ctx
                .adapter
                .get_texture_format_features(wgpu::TextureFormat::Rgba16Float);
            ensure!(
                features.allowed_usages.contains(
                    wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING
                        | wgpu::TextureUsages::COPY_SRC
                        | wgpu::TextureUsages::COPY_DST
                ),
                "adapter cannot render/sample/read back the RGBA16F warp atlas"
            );
            ensure!(
                !ctx.policy.filtered
                    || features
                        .flags
                        .contains(wgpu::TextureFormatFeatureFlags::FILTERABLE),
                "adapter cannot linearly sample the RGBA16F warp atlas; use manual filtering"
            );
            wgpu::TextureFormat::Rgba16Float
        } else {
            wgpu::TextureFormat::Rgba32Float
        };
        let make_warp_texture = |label: &str| {
            if s.compact_warp {
                Texture::float16(&ctx, label, s.atlas, s.atlas)
            } else {
                Texture::float(&ctx, label, s.atlas, s.atlas)
            }
        };
        let raw = (0..4)
            .map(|i| make_warp_texture(&format!("raw inverse plane {i}")))
            .collect::<Result<Vec<_>>>()?;
        let warp = (0..4)
            .map(|i| {
                if s.compute_edgefill {
                    Texture::float_storage(
                        &ctx,
                        &format!("gutter inverse plane {i}"),
                        s.atlas,
                        s.atlas,
                    )
                } else if s.compact_warp {
                    Texture::float16(&ctx, &format!("gutter inverse plane {i}"), s.atlas, s.atlas)
                } else {
                    Texture::float(&ctx, &format!("gutter inverse plane {i}"), s.atlas, s.atlas)
                }
            })
            .collect::<Result<Vec<_>>>()?;
        let inverse = if s.stored_inverse {
            Some(
                (0..3)
                    .map(|i| {
                        Texture::float(
                            &ctx,
                            &format!("stored object basis plane {i}"),
                            s.atlas,
                            s.atlas,
                        )
                    })
                    .collect::<Result<Vec<_>>>()?,
            )
        } else {
            None
        };
        let (teleport, teleport_destination) = if s.split_teleport {
            for format in [
                wgpu::TextureFormat::R32Float,
                wgpu::TextureFormat::Rg32Float,
            ] {
                let features = ctx.adapter.get_texture_format_features(format);
                ensure!(
                    features.allowed_usages.contains(
                        wgpu::TextureUsages::COPY_DST
                            | wgpu::TextureUsages::COPY_SRC
                            | wgpu::TextureUsages::TEXTURE_BINDING
                    ),
                    "adapter cannot sample and transfer split teleport texture format {format:?}"
                );
                ensure!(
                    !ctx.policy.filtered
                        || features
                            .flags
                            .contains(wgpu::TextureFormatFeatureFlags::FILTERABLE),
                    "adapter cannot linearly sample split teleport texture format {format:?}; use manual filtering"
                );
            }
            let usage = wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING;
            (
                Texture::new(
                    &ctx,
                    "split seam distance",
                    s.atlas,
                    s.atlas,
                    wgpu::TextureFormat::R32Float,
                    usage,
                )?,
                Some(Texture::new(
                    &ctx,
                    "nearest seam destination",
                    s.atlas,
                    s.atlas,
                    wgpu::TextureFormat::Rg32Float,
                    usage,
                )?),
            )
        } else {
            (
                Texture::float(&ctx, "static seam destination/distance", s.atlas, s.atlas)?,
                None,
            )
        };
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
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT
                            | wgpu::ShaderStages::COMPUTE,
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
                visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE,
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
            visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        });
        entries.extend((10..15).map(|binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float {
                    filterable: ctx.policy.filtered,
                },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        }));
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
            [&raw[0], &raw[1], &raw[2], &teleport, &teleport],
            &sampler,
        );
        let trace_inverse = inverse.as_deref().unwrap_or(&warp[..3]);
        let trace_warp = if s.indirect_edgefill { &raw } else { &warp };
        let seam_aux = if let Some(destination) = &teleport_destination {
            [destination, &teleport]
        } else {
            [&teleport, &teleport]
        };
        let trace_auxiliary = [
            &trace_inverse[0],
            &trace_inverse[1],
            &trace_inverse[2],
            seam_aux[0],
            seam_aux[1],
        ];
        let trace_group = bindings(
            &ctx,
            &sampled_layout,
            trace_warp,
            [&teleport, &edgefill],
            [&height, &albedo, &normal],
            trace_auxiliary,
            &sampler,
        );
        let edge_compute = if s.compute_edgefill {
            let format_features = ctx
                .adapter
                .get_texture_format_features(wgpu::TextureFormat::Rgba32Float);
            ensure!(
                format_features
                    .allowed_usages
                    .contains(wgpu::TextureUsages::STORAGE_BINDING),
                "adapter cannot use RGBA32F as a writable storage texture for U8 compute edgefill"
            );
            let storage_layout =
                ctx.device
                    .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                        label: Some("edgefill storage outputs"),
                        entries: &(0..4)
                            .map(|binding| wgpu::BindGroupLayoutEntry {
                                binding,
                                visibility: wgpu::ShaderStages::COMPUTE,
                                ty: wgpu::BindingType::StorageTexture {
                                    access: wgpu::StorageTextureAccess::WriteOnly,
                                    format: wgpu::TextureFormat::Rgba32Float,
                                    view_dimension: wgpu::TextureViewDimension::D2,
                                },
                                count: None,
                            })
                            .collect::<Vec<_>>(),
                    });
            let storage_group = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("edgefill storage outputs"),
                layout: &storage_layout,
                entries: &warp
                    .iter()
                    .enumerate()
                    .map(|(binding, texture)| wgpu::BindGroupEntry {
                        binding: binding as u32,
                        resource: wgpu::BindingResource::TextureView(&texture.view),
                    })
                    .collect::<Vec<_>>(),
            });
            let compute_layout =
                ctx.device
                    .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                        label: Some("compute gutter/trace layout"),
                        bind_group_layouts: &[
                            Some(&uniform_layout),
                            Some(&sampled_layout),
                            Some(&storage_layout),
                        ],
                        immediate_size: 0,
                    });
            let module = ctx
                .device
                .create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("full-atlas compute edgefill"),
                    source: wgpu::ShaderSource::Wgsl(
                        shaders::edgefill_compute(ctx.policy.filtered).into(),
                    ),
                });
            let pipeline = ctx
                .device
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("full-atlas compute edgefill"),
                    layout: Some(&compute_layout),
                    module: &module,
                    entry_point: Some("cs_main"),
                    compilation_options: Default::default(),
                    cache: None,
                });
            Some((pipeline, storage_group))
        } else {
            None
        };
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
            .map(|first| pipeline_format(&ctx, &deform_layout, "deform", width, first, warp_format))
            .collect();
        let edge_pipelines = (0..4)
            .step_by(width as usize)
            .map(|first| pipeline_format(&ctx, &full_layout, "edgefill", width, first, warp_format))
            .collect();
        let inverse_pipelines = if inverse.is_some() {
            let mut pipelines = Vec::new();
            let mut first = 0;
            while first < 3 {
                let count = width.min(3 - first);
                pipelines.push(pipeline(&ctx, &full_layout, "inverse", count, first));
                first += count;
            }
            pipelines
        } else {
            Vec::new()
        };
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
            inverse,
            teleport,
            teleport_destination,
            split_teleport: s.split_teleport,
            compact_warp: s.compact_warp,
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
            inverse_pipelines,
            edge_compute,
            indirect_edgefill: s.indirect_edgefill,
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
        let occupancy_data = if renderer.compact_warp {
            renderer.ctx.read_half_float(&renderer.raw[0].texture)?
        } else {
            renderer.ctx.read_float(&renderer.raw[0].texture)?
        };
        let occupancy: Vec<_> = occupancy_data.iter().map(|p| p[3] > 0.5).collect();
        let maps = if let Some(path) = cache_path {
            if path.exists() {
                let cache = BakedMapCache::load(path)?;
                cache.validate_for(&mesh, s.atlas, &occupancy)?;
                cache.maps
            } else {
                let cache = BakedMapCache::bake(&mesh, s.atlas, &occupancy)?;
                cache.save(path)?;
                cache.maps
            }
        } else {
            topology::bake(&mesh, s.atlas, &occupancy)?
        };
        if let Some(destination) = &renderer.teleport_destination {
            let distance: Vec<_> = maps.teleport.iter().map(|value| value[2]).collect();
            let uv: Vec<_> = maps
                .teleport
                .iter()
                .map(|value| [value[0], value[1]])
                .collect();
            renderer
                .teleport
                .upload(&renderer.ctx, bytemuck::cast_slice(&distance), 4);
            destination.upload(&renderer.ctx, bytemuck::cast_slice(&uv), 8);
        } else {
            renderer
                .teleport
                .upload(&renderer.ctx, bytemuck::cast_slice(&maps.teleport), 16);
        }
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
            s.stored_inverse == self.inverse.is_some(),
            "stored inverse setting must match renderer resources created at initialization"
        );
        ensure!(
            s.compute_edgefill == self.edge_compute.is_some(),
            "compute edgefill setting must match renderer resources created at initialization"
        );
        ensure!(
            s.indirect_edgefill == self.indirect_edgefill,
            "trace-time edgefill setting must match renderer resources created at initialization"
        );
        ensure!(
            s.compact_warp == self.compact_warp,
            "compact warp setting must match renderer resources created at initialization"
        );
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
        if s.indirect_edgefill {
            // U10 reads source transforms through edge_map in the trace shader.
        } else if let Some((pipeline, storage_group)) = &self.edge_compute {
            let index = names.len() as u32 * 2;
            if query.is_some() {
                names.push("edgefill-compute".to_owned());
            }
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("gutter and distortion compute"),
                timestamp_writes: query.as_ref().map(|query_set| {
                    wgpu::ComputePassTimestampWrites {
                        query_set,
                        beginning_of_pass_write_index: Some(index),
                        end_of_pass_write_index: Some(index + 1),
                    }
                }),
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &self.uniform_group, &[]);
            pass.set_bind_group(1, &self.edge_group, &[]);
            pass.set_bind_group(2, storage_group, &[]);
            pass.dispatch_workgroups(
                self.warp[0].texture.width().div_ceil(8),
                self.warp[0].texture.height().div_ceil(8),
                1,
            );
        } else {
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
        }
        if let Some(inverse) = &self.inverse {
            let width = self.ctx.policy.planes as usize;
            let mut first = 0usize;
            for (i, pipeline) in self.inverse_pipelines.iter().enumerate() {
                let count = (3 - first).min(width);
                let colors = attachments(&inverse[first..first + count], wgpu::Color::TRANSPARENT);
                let timestamps = timestamp(query.as_ref(), &mut names, format!("inverse-{i}"));
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("stored inverse basis"),
                    color_attachments: &colors,
                    timestamp_writes: timestamps,
                    ..Default::default()
                });
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, &self.uniform_group, &[]);
                pass.set_bind_group(1, &self.edge_group, &[]);
                pass.draw(0..3, 0..1);
                first += count;
            }
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
                        r: s.background[0] as f64,
                        g: s.background[1] as f64,
                        b: s.background[2] as f64,
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
            .map(|p| {
                if self.compact_warp {
                    self.ctx.read_half_float(&p.texture)
                } else {
                    self.ctx.read_float(&p.texture)
                }
            })
            .collect()
    }
    pub fn raw_planes(&self) -> Result<Vec<Vec<[f32; 4]>>> {
        self.raw
            .iter()
            .map(|p| {
                if self.compact_warp {
                    self.ctx.read_half_float(&p.texture)
                } else {
                    self.ctx.read_float(&p.texture)
                }
            })
            .collect()
    }
    pub fn stored_inverse_planes(&self) -> Result<Option<Vec<Vec<[f32; 4]>>>> {
        self.inverse
            .as_ref()
            .map(|planes| {
                planes
                    .iter()
                    .map(|plane| self.ctx.read_float(&plane.texture))
                    .collect()
            })
            .transpose()
    }
    pub fn render_capture(&mut self, s: &Settings) -> Result<Vec<Vec<[f32; 4]>>> {
        let ticket = self.submit(s, true)?;
        let mut result = self
            .frame
            .planes
            .iter()
            .map(|p| self.ctx.read_float(&p.texture))
            .collect::<Result<Vec<_>>>()?;
        // Diagnostic failed shell fragments carry alpha zero; composite their
        // color against the requested clear color, as the normal discard path does.
        for pixel in &mut result[0] {
            if pixel[3] <= 0. {
                pixel[..3].copy_from_slice(&s.background);
            }
        }
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
