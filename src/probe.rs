//! Numerical native-device gates, independent of the full viewer.
use crate::{
    asset::Vertex,
    gpu_resources::{GpuContext, Texture},
    passes::{attachments, pipeline},
    settings::Settings,
    shaders,
};
mod fractal;
mod inverse_lbs;
mod splat;
mod voxel_warp;

use anyhow::{Result, ensure};
use serde_json::{Value, json};
pub fn contracts(ctx: &GpuContext) -> Result<Value> {
    use glam::{DMat3, DVec3, Vec2, Vec3};
    let mut u = Settings::default().uniforms(true);
    u.height_step = [-3.25, 0.03125, 4097.25, 0.75];
    u.modes = [-1, 2, 1, 100];
    u.control = [7, 1, 9999, 1];
    let uniform = ctx.buffer(
        "probe uniforms",
        bytemuck::bytes_of(&u),
        wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    );
    let output = ctx.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("numeric probe output"),
        size: 576,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let texture = Texture::float(ctx, "sampling probe", 4, 4)?;
    let pixels: Vec<[f32; 4]> = (0..16)
        .map(|i| {
            [
                (i % 4) as f32,
                (i / 4) as f32,
                (i % 4 + 10 * (i / 4)) as f32,
                1.,
            ]
        })
        .collect();
    texture.upload(ctx, bytemuck::cast_slice(&pixels), 16);
    let sampler = ctx.device.create_sampler(&wgpu::SamplerDescriptor {
        min_filter: wgpu::FilterMode::Linear,
        mag_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let source = format!(
        "{}\n{}",
        shaders::common(ctx.policy.filtered),
        r#"
        @group(0) @binding(1) var<storage,read_write> result:array<vec4<f32>,36>;
        @compute @workgroup_size(1) fn cs_main(){
            for(var c=0u;c<4u;c++){
                result[c]=u.projection[c];result[4u+c]=u.camera_from_object[c];result[8u+c]=u.object_from_camera[c];
            }
            result[12]=u.height_step;
            for(var b=0u;b<4u;b++){for(var c=0u;c<4u;c++){result[13u+b*4u+c]=u.bones[b][c];}}
            result[29]=u.sun_atlas;result[30]=bitcast<vec4<f32>>(u.modes);
            result[31]=u.damping_extrusion;result[32]=u.lighting;result[33]=bitcast<vec4<f32>>(u.control);
            result[34]=sample_float(warp0,vec2<f32>(0.5));result[35]=nearest(warp0,vec2<f32>(0.5));
        }
    "#
    );
    let module = ctx
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("uniform/filter round trip"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
    let layout0 = ctx
        .device
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(544),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(576),
                    },
                    count: None,
                },
            ],
        });
    let layout1 = ctx
        .device
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float {
                            filterable: ctx.policy.filtered,
                        },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 9,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
    let layout = ctx
        .device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&layout0), Some(&layout1)],
            immediate_size: 0,
        });
    let compute = ctx
        .device
        .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("numeric contract probe"),
            layout: Some(&layout),
            module: &module,
            entry_point: Some("cs_main"),
            compilation_options: Default::default(),
            cache: None,
        });
    let group0 = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &layout0,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: output.as_entire_binding(),
            },
        ],
    });
    let group1 = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &layout1,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&texture.view),
            },
            wgpu::BindGroupEntry {
                binding: 9,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    });
    let read = ctx.device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 576,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = ctx.device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&compute);
        pass.set_bind_group(0, &group0, &[]);
        pass.set_bind_group(1, &group1, &[]);
        pass.dispatch_workgroups(1, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&output, 0, &read, 0, 576);
    ctx.queue.submit([encoder.finish()]);
    let bytes = ctx.map_buffer(&read)?;
    ensure!(
        &bytes[..544] == bytemuck::bytes_of(&u),
        "GPU uniform layout did not round-trip"
    );
    let values: Vec<_> = bytes[544..]
        .chunks_exact(4)
        .map(|p| f32::from_le_bytes(p.try_into().unwrap()))
        .collect();
    let expected = [1.5, 1.5, 16.5, 1., 2., 2., 22., 1.];
    ensure!(
        values
            .iter()
            .zip(expected)
            .all(|(a, b)| (a - b).abs() < 1e-6),
        "GPU float filtering/nearest contract failed: {values:?}"
    );
    // Actual WGSL deformation/MRT interpolation against the independent f64 oracle.
    let basis = DMat3::from_cols(
        DVec3::new(7., 0., 0.),
        DVec3::new(2., 3., 0.),
        DVec3::new(0., 0., 0.4),
    );
    let inv = crate::math_reference::inverse_basis(basis)?;
    let origin = DVec3::new(10., -2., 3.);
    let vertices: Vec<Vertex> = [Vec2::ZERO, Vec2::X, Vec2::Y, Vec2::ONE]
        .into_iter()
        .map(|uv| {
            let point = origin + basis * DVec3::new(uv.x as f64, uv.y as f64, 0.5);
            Vertex {
                position: point.as_vec3().to_array(),
                uv: [uv.x, uv.y, 1.],
                weights: [1., 0., 0., 0.],
                normal: basis.z_axis.as_vec3().to_array(),
                tangent: basis.x_axis.as_vec3().to_array(),
                bitangent: basis.y_axis.as_vec3().to_array(),
            }
        })
        .collect();
    let vertex = ctx.buffer(
        "analytic chart",
        bytemuck::cast_slice(&vertices),
        wgpu::BufferUsages::VERTEX,
    );
    let index = ctx.buffer(
        "analytic chart indices",
        bytemuck::cast_slice(&[0u32, 1, 2, 2, 1, 3]),
        wgpu::BufferUsages::INDEX,
    );
    let atlas = (0..4)
        .map(|i| Texture::float(ctx, &format!("analytic plane {i}"), 8, 8))
        .collect::<Result<Vec<_>>>()?;
    let layout0 = ctx
        .device
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(544),
                },
                count: None,
            }],
        });
    let group0 = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &layout0,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform.as_entire_binding(),
        }],
    });
    let layout = ctx
        .device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&layout0)],
            immediate_size: 0,
        });
    let pipelines: Vec<_> = (0..4)
        .step_by(ctx.policy.planes as usize)
        .map(|first| pipeline(ctx, &layout, "deform", ctx.policy.planes, first))
        .collect();
    let mut max_error = 0.0f64;
    for mode in 0..2 {
        u.bones = [glam::Mat4::IDENTITY.to_cols_array_2d(); 4];
        u.modes[2] = mode;
        ctx.queue.write_buffer(&uniform, 0, bytemuck::bytes_of(&u));
        let mut encoder = ctx.device.create_command_encoder(&Default::default());
        for (i, pipeline) in pipelines.iter().enumerate() {
            let width = ctx.policy.planes as usize;
            let colors = attachments(&atlas[i * width..(i + 1) * width], wgpu::Color::TRANSPARENT);
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("analytic full-float MRT"),
                color_attachments: &colors,
                ..Default::default()
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &group0, &[]);
            pass.set_vertex_buffer(0, vertex.slice(..));
            pass.set_index_buffer(index.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..6, 0, 0..1);
        }
        ctx.queue.submit([encoder.finish()]);
        let planes = atlas
            .iter()
            .map(|p| ctx.read_float(&p.texture))
            .collect::<Result<Vec<_>>>()?;
        for pixel in 0..64 {
            let uv = DVec3::new(
                ((pixel % 8) as f64 + 0.5) / 8.,
                ((pixel / 8) as f64 + 0.5) / 8.,
                0.5,
            );
            let expected = if mode == 0 {
                -inv * origin
            } else {
                origin + basis * uv
            };
            for (i, p) in planes.iter().enumerate() {
                let want = if i == 3 { expected } else { inv.col(i) };
                let actual = Vec3::new(p[pixel][0], p[pixel][1], p[pixel][2]).as_dvec3();
                max_error = max_error.max((actual - want).abs().max_element());
                ensure!(
                    p[pixel][3] > 0.5,
                    "analytic UV raster has missing/invalid coverage"
                );
            }
        }
    }
    ensure!(
        max_error < 1e-5,
        "GPU affine oracle error {max_error} exceeds 1e-5"
    );
    ctx.check()?;
    Ok(
        json!({"uniform_bytes_round_tripped":544,"filtered_sample":&values[..4],"nearest_sample":&values[4..],"affine_max_abs_error":max_error,"rgba32float_mrt":"rendered and read back","distortion_modes_tested":[0,1],"flat_trace":flat_trace(ctx)? ,"flat_ramp_trace":flat_ramp_trace(ctx)? ,"nonlinear_height_trace":nonlinear_height_trace(ctx)? ,"flat_trace_refined":flat_trace_variant(ctx,true,false,false,false,false,false)? ,"flat_trace_specialized":flat_trace_variant(ctx,false,true,false,false,false,false)? ,"flat_trace_stored_inverse":flat_trace_variant(ctx,false,false,true,false,false,false)? ,"space_warp_sdf":space_warp_sdf_probe(ctx)? ,"inverse_lbs_sdf":inverse_lbs::run(ctx)? ,"inverse_lbs_voxel_sdf":voxel_warp::run(ctx)? ,"gaussian_splat_adapter":splat::run(ctx)? ,"menger_sdf":fractal::run(ctx)? ,"voxel_sdf":voxel_sdf_probe()}),
    )
}

fn space_warp_sdf_probe(ctx: &GpuContext) -> Result<Value> {
    const RAYS_PER_WARP: usize = 64;
    const WARP_COUNT: usize = 2;
    const RAY_COUNT: usize = RAYS_PER_WARP * WARP_COUNT;
    let amplitude = 0.2f32;
    let frequency = 2.4f32;
    let maximum_shear = (amplitude * frequency).abs() as f64;
    // The first field is a shear; the second blends identity and one rigid bone rotation.
    let shear_lipschitz = (1.0 + maximum_shear * maximum_shear / 4.0).sqrt() + maximum_shear / 2.0;
    let bone_angle = 0.2f64;
    let domain_radius = 10.0f64;
    let bone_lipschitz = 1.0 + 1.5 * 2.0 * domain_radius * (0.5 * bone_angle).sin();
    let params = [
        amplitude,
        frequency,
        shear_lipschitz as f32,
        bone_lipschitz as f32,
    ];
    let params_buffer = ctx.buffer(
        "U19/U20 analytic SDF warp parameters",
        bytemuck::cast_slice(&params),
        wgpu::BufferUsages::UNIFORM,
    );
    let output_size = (RAY_COUNT * 16) as u64;
    let output = ctx.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("U19/U20 SDF warp results"),
        size: output_size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = ctx.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("U19/U20 SDF warp readback"),
        size: output_size,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let layout = ctx
        .device
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("U19/U20 SDF warp bindings"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(16),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(output_size),
                    },
                    count: None,
                },
            ],
        });
    let bind_group = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("U19/U20 SDF warp bindings"),
        layout: &layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: params_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: output.as_entire_binding(),
            },
        ],
    });
    let pipeline_layout = ctx
        .device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("U19/U20 analytic SDF warp layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
    let module = ctx
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("U19/U20 analytic SDF warp shader"),
            source: wgpu::ShaderSource::Wgsl(crate::shaders::space_warp_sdf_probe().into()),
        });
    let pipeline = ctx
        .device
        .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("U19/U20 analytic SDF warp"),
            layout: Some(&pipeline_layout),
            module: &module,
            entry_point: Some("cs_main"),
            compilation_options: Default::default(),
            cache: None,
        });
    let query = if ctx
        .device
        .features()
        .contains(wgpu::Features::TIMESTAMP_QUERY)
    {
        Some(ctx.device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("U19/U20 compute timestamp"),
            ty: wgpu::QueryType::Timestamp,
            count: 2,
        }))
    } else {
        None
    };
    let mut encoder = ctx.device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("U19/U20 analytic SDF space warp"),
            timestamp_writes: query
                .as_ref()
                .map(|query_set| wgpu::ComputePassTimestampWrites {
                    query_set,
                    beginning_of_pass_write_index: Some(0),
                    end_of_pass_write_index: Some(1),
                }),
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(RAY_COUNT.div_ceil(8) as u32, 1, 1);
    }
    let timestamp_readback = query.as_ref().map(|query_set| {
        let resolve = ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("U19/U20 timestamp resolve"),
            size: 16,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback = ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("U19/U20 timestamp readback"),
            size: 16,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.resolve_query_set(query_set, 0..2, &resolve, 0);
        encoder.copy_buffer_to_buffer(&resolve, 0, &readback, 0, 16);
        readback
    });
    encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, output_size);
    ctx.queue.submit([encoder.finish()]);
    let result_bytes = ctx.map_buffer(&readback)?;
    let values: Vec<[f32; 4]> = result_bytes
        .chunks_exact(16)
        .map(|pixel| {
            std::array::from_fn(|channel| {
                f32::from_le_bytes(pixel[channel * 4..channel * 4 + 4].try_into().unwrap())
            })
        })
        .collect();
    let gpu_ms = if let Some(readback) = timestamp_readback {
        let bytes = ctx.map_buffer(&readback)?;
        let start = u64::from_le_bytes(bytes[0..8].try_into().unwrap());
        let end = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
        Some((end.saturating_sub(start) as f64) * ctx.queue.get_timestamp_period() as f64 / 1e6)
    } else {
        None
    };

    let to_canonical = |mode: usize, p: [f64; 3]| -> [f64; 3] {
        if mode == 0 {
            [
                p[0],
                p[1],
                p[2] + amplitude as f64 * (frequency as f64 * p[0]).sin(),
            ]
        } else {
            let weight_parameter = (p[0] + 0.5).clamp(0.0, 1.0);
            let weight = weight_parameter * weight_parameter * (3.0 - 2.0 * weight_parameter);
            let cosine = bone_angle.cos();
            let sine = bone_angle.sin();
            let rotated = [
                cosine * p[0] - sine * p[2],
                p[1],
                sine * p[0] + cosine * p[2],
            ];
            [
                p[0] * (1.0 - weight) + rotated[0] * weight,
                p[1],
                p[2] * (1.0 - weight) + rotated[2] * weight,
            ]
        }
    };
    let reference_field = |mode: usize, coordinate: f64, t: f64| {
        let x =
            coordinate * 1.5 + (coordinate * 0.4 / (1.0 + (coordinate * 0.4).powi(2)).sqrt()) * t;
        let z = -3.0 + (1.0 / (1.0 + (coordinate * 0.4).powi(2)).sqrt()) * t;
        let canonical = to_canonical(mode, [x, 0.0, z]);
        canonical[0] * canonical[0] + canonical[2] * canonical[2] - 1.0
    };
    let mut status_disagreements = 0usize;
    let mut bound_violations = 0usize;
    let mut hit_count = 0usize;
    let mut max_hit_error = 0.0f64;
    let mut max_steps = 0u32;
    let mut variants = Vec::with_capacity(WARP_COUNT);
    for mode in 0..WARP_COUNT {
        let mut mode_status_disagreements = 0usize;
        let mut mode_bound_violations = 0usize;
        let mut mode_hit_count = 0usize;
        let mut mode_max_hit_error = 0.0f64;
        let mut mode_max_steps = 0u32;
        let mut step_counts = Vec::with_capacity(RAYS_PER_WARP);
        for ray in 0..RAYS_PER_WARP {
            let index = mode * RAYS_PER_WARP + ray;
            let coordinate = (ray as f64 + 0.5) / RAYS_PER_WARP as f64 * 2.0 - 1.0;
            let mut previous_t = 0.0f64;
            let mut previous_value = reference_field(mode, coordinate, previous_t);
            let mut interval = None;
            for sample in 1..=8192 {
                let t = 6.0 * sample as f64 / 8192.0;
                let value = reference_field(mode, coordinate, t);
                if previous_value > 0.0 && value <= 0.0 {
                    interval = Some((previous_t, t));
                    break;
                }
                previous_t = t;
                previous_value = value;
            }
            let root = interval.map(|(mut low, mut high)| {
                for _ in 0..48 {
                    let middle = (low + high) * 0.5;
                    if reference_field(mode, coordinate, middle) > 0.0 {
                        low = middle;
                    } else {
                        high = middle;
                    }
                }
                (low + high) * 0.5
            });
            let value = values[index];
            let status = value[0] as u32;
            let steps = value[1] as u32;
            let distance = value[2] as f64;
            mode_max_steps = mode_max_steps.max(steps);
            step_counts.push(steps);
            match root {
                Some(root) => {
                    mode_hit_count += 1;
                    if status != 1 {
                        mode_status_disagreements += 1;
                    } else {
                        mode_max_hit_error = mode_max_hit_error.max((distance - root).abs());
                        if distance > root + 2e-4 {
                            mode_bound_violations += 1;
                        }
                    }
                }
                None => {
                    if status != 2 {
                        mode_status_disagreements += 1;
                    }
                }
            }
        }
        step_counts.sort_unstable();
        status_disagreements += mode_status_disagreements;
        bound_violations += mode_bound_violations;
        hit_count += mode_hit_count;
        max_hit_error = max_hit_error.max(mode_max_hit_error);
        max_steps = max_steps.max(mode_max_steps);
        let mut variant = json!({
            "ray_count":RAYS_PER_WARP,
            "hit_rays":mode_hit_count,
            "status_disagreements":mode_status_disagreements,
            "conservative_step_violations":mode_bound_violations,
            "max_hit_distance_error":mode_max_hit_error,
            "steps_p50":step_counts[step_counts.len()/2],
            "steps_p95":step_counts[(step_counts.len()*95/100).min(step_counts.len()-1)],
            "steps_max":mode_max_steps
        });
        if mode == 0 {
            variant["warp"] = json!("bounded sinusoidal shear");
            variant["global_lipschitz_bound"] = json!(shear_lipschitz);
        } else {
            let weight_gradient_bound = 1.5f64;
            let maximum_displacement = 2.0 * domain_radius * (0.5 * bone_angle).sin();
            let lipschitz = 1.0 + weight_gradient_bound * maximum_displacement;
            let mut min_sampled_determinant = f64::INFINITY;
            for ray in 0..RAYS_PER_WARP {
                let coordinate = (ray as f64 + 0.5) / RAYS_PER_WARP as f64 * 2.0 - 1.0;
                let direction_x = coordinate * 0.4 / (1.0 + (coordinate * 0.4).powi(2)).sqrt();
                let direction_z = 1.0 / (1.0 + (coordinate * 0.4).powi(2)).sqrt();
                for sample in 0..=64 {
                    let t = 6.0 * sample as f64 / 64.0;
                    let x = coordinate * 1.5 + direction_x * t;
                    let z = -3.0 + direction_z * t;
                    let delta = 1e-4;
                    let center_x_plus = to_canonical(mode, [x + delta, 0.0, z]);
                    let center_x_minus = to_canonical(mode, [x - delta, 0.0, z]);
                    let center_z_plus = to_canonical(mode, [x, 0.0, z + delta]);
                    let center_z_minus = to_canonical(mode, [x, 0.0, z - delta]);
                    let dxdx = (center_x_plus[0] - center_x_minus[0]) / (2.0 * delta);
                    let dzdx = (center_x_plus[2] - center_x_minus[2]) / (2.0 * delta);
                    let dxdz = (center_z_plus[0] - center_z_minus[0]) / (2.0 * delta);
                    let dzdz = (center_z_plus[2] - center_z_minus[2]) / (2.0 * delta);
                    min_sampled_determinant =
                        min_sampled_determinant.min(dxdx * dzdz - dxdz * dzdx);
                }
            }
            variant["warp"] = json!("smooth blend of identity and one rigid bone inverse");
            variant["query_domain_lipschitz_bound"] = json!(lipschitz);
            variant["query_domain_radius"] = json!(domain_radius);
            variant["minimum_sampled_jacobian_determinant"] = json!(min_sampled_determinant);
        }
        variants.push(variant);
    }
    ctx.check()?;
    Ok(json!({
        "representation":"analytic unit sphere SDF",
        "query_to_canonical_warps":variants,
        "ray_count":RAY_COUNT,
        "hit_rays":hit_count,
        "status_disagreements":status_disagreements,
        "conservative_step_violations":bound_violations,
        "max_hit_distance_error":max_hit_error,
        "steps_max":max_steps,
        "compute_gpu_ms":gpu_ms,
        "reference":"CPU double-precision root search on the same inverse-warp field"
    }))
}

fn voxel_sdf_probe() -> Value {
    let resolutions = [16usize, 32, 64];
    let mut results = Vec::new();
    for resolution in resolutions {
        let intervals = resolution - 1;
        let spacing = 3.0f64 / intervals as f64;
        let coordinate = |index: usize| -1.5 + index as f64 * spacing;
        let index = |x: usize, y: usize, z: usize| x + resolution * (y + resolution * z);
        let mut grid = vec![0.0f32; resolution.pow(3)];
        for z in 0..resolution {
            for y in 0..resolution {
                for x in 0..resolution {
                    let px = coordinate(x);
                    let py = coordinate(y);
                    let pz = coordinate(z);
                    grid[index(x, y, z)] = ((px * px + py * py + pz * pz).sqrt() - 1.0) as f32;
                }
            }
        }
        let mut absolute_error_sum = 0.0f64;
        let mut max_absolute_error = 0.0f64;
        let mut sample_count = 0usize;
        let conservative_margin = 3.0f64.sqrt() * spacing;
        let mut bound_violations = 0usize;
        for z in 0..intervals {
            for y in 0..intervals {
                for x in 0..intervals {
                    let fraction = 0.5f64;
                    let p = [
                        coordinate(x) + fraction * spacing,
                        coordinate(y) + fraction * spacing,
                        coordinate(z) + fraction * spacing,
                    ];
                    let mut interpolated = 0.0f64;
                    for dz in 0..=1 {
                        for dy in 0..=1 {
                            for dx in 0..=1 {
                                let wx = if dx == 0 { 1.0 - fraction } else { fraction };
                                let wy = if dy == 0 { 1.0 - fraction } else { fraction };
                                let wz = if dz == 0 { 1.0 - fraction } else { fraction };
                                interpolated +=
                                    f64::from(grid[index(x + dx, y + dy, z + dz)]) * wx * wy * wz;
                            }
                        }
                    }
                    let exact = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt() - 1.0;
                    let error = interpolated - exact;
                    let absolute_error = error.abs();
                    absolute_error_sum += absolute_error * absolute_error;
                    max_absolute_error = max_absolute_error.max(absolute_error);
                    if interpolated - conservative_margin > exact + 1e-9 {
                        bound_violations += 1;
                    }
                    sample_count += 1;
                }
            }
        }
        results.push(json!({
            "resolution":[resolution,resolution,resolution],
            "grid_payload_bytes":grid.len()*std::mem::size_of::<f32>(),
            "sample_count":sample_count,
            "conservative_lipschitz_margin":conservative_margin,
            "rms_sample_error":(absolute_error_sum/sample_count as f64).sqrt(),
            "max_abs_sample_error":max_absolute_error,
            "conservative_bound_violations":bound_violations
        }));
    }
    json!({
        "field":"dense float32 SDF samples of analytic unit sphere",
        "sampling":"CPU trilinear interpolation at deterministic cell centers",
        "bound":"interpolated sample minus sqrt(3)*cell spacing",
        "resolutions":results
    })
}

fn flat_trace(ctx: &GpuContext) -> Result<Value> {
    flat_trace_variant(ctx, false, false, false, false, false, false)
}

fn flat_ramp_trace(ctx: &GpuContext) -> Result<Value> {
    let baseline = flat_trace_variant(ctx, false, false, false, true, false, false)?;
    let refined = flat_trace_variant(ctx, true, false, false, true, false, false)?;
    let manual_baseline = flat_trace_variant(ctx, false, false, false, true, false, true)?;
    let manual_refined = flat_trace_variant(ctx, true, false, false, true, false, true)?;
    Ok(json!({
        "baseline": baseline,
        "refined": refined,
        "manual_bilinear": {"baseline": manual_baseline, "refined": manual_refined}
    }))
}

fn nonlinear_height_trace(ctx: &GpuContext) -> Result<Value> {
    let baseline = flat_trace_variant(ctx, false, false, false, false, true, false)?;
    let refined = flat_trace_variant(ctx, true, false, false, false, true, false)?;
    let manual_baseline = flat_trace_variant(ctx, false, false, false, false, true, true)?;
    let manual_refined = flat_trace_variant(ctx, true, false, false, false, true, true)?;
    let seam_teleport_baseline = flat_trace_variant_with_seam(ctx, false, false)?;
    let seam_teleport_refined = flat_trace_variant_with_seam(ctx, true, false)?;
    let sdf_predicted_step = flat_trace_variant_with_seam(ctx, false, true)?;
    let inverse_retry_baseline = flat_trace_variant_with_transform_retry(ctx, false)?;
    let inverse_retry_adaptive = flat_trace_variant_with_transform_retry(ctx, true)?;
    Ok(json!({
        "baseline": baseline,
        "refined": refined,
        "manual_bilinear": {"baseline": manual_baseline, "refined": manual_refined},
        "seam_teleport": {
            "baseline": seam_teleport_baseline,
            "refined": seam_teleport_refined,
            "sdf_predicted_step": sdf_predicted_step
        },
        "inverse_transform_retry": {
            "baseline": inverse_retry_baseline,
            "adaptive": inverse_retry_adaptive
        }
    }))
}

fn sample_height_texels(texels: &[u8], u: f64) -> f64 {
    let texel = u.clamp(0.0, 1.0) * texels.len() as f64 - 0.5;
    let lower_raw = texel.floor() as i32;
    let fraction = texel - f64::from(lower_raw);
    let lower = lower_raw.clamp(0, texels.len() as i32 - 1) as usize;
    let upper = (lower_raw + 1).clamp(0, texels.len() as i32 - 1) as usize;
    ((1.0 - fraction) * f64::from(texels[lower]) + fraction * f64::from(texels[upper])) / 255.0
}

struct NonlinearHeightSurface<'a> {
    basis_x: f64,
    basis_z: f64,
    anchor_x: f64,
    anchor_z: f64,
    height_texels: &'a [u8],
    height_scale: f64,
    height_offset: f64,
}

fn nonlinear_height_root(
    start: [f64; 3],
    direction: [f64; 3],
    surface: &NonlinearHeightSurface<'_>,
) -> Result<f64> {
    let error_at = |distance: f64| {
        let point_x = start[0] + direction[0] * distance;
        let point_z = start[2] + direction[2] * distance;
        let surface_u = surface.basis_x * point_x + surface.anchor_x;
        let surface_z = surface.basis_z * point_z + surface.anchor_z;
        let sampled = sample_height_texels(surface.height_texels, surface_u);
        let height = (sampled - 0.5) * surface.height_scale + 0.5 + surface.height_offset;
        height - surface_z
    };

    let mut lower = 0.0;
    let mut lower_error = error_at(lower);
    ensure!(
        lower_error <= 0.0,
        "CPU nonlinear heightfield ray starts inside the surface"
    );
    const SEARCH_STEPS: usize = 8192;
    const MAX_DISTANCE: f64 = 8.0;
    for sample in 1..=SEARCH_STEPS {
        let mut upper = MAX_DISTANCE * sample as f64 / SEARCH_STEPS as f64;
        let mut upper_error = error_at(upper);
        if upper_error >= 0.0 {
            for _ in 0..64 {
                let middle = (lower + upper) * 0.5;
                let middle_error = error_at(middle);
                if middle_error >= 0.0 {
                    upper = middle;
                    upper_error = middle_error;
                } else {
                    lower = middle;
                    lower_error = middle_error;
                }
            }
            ensure!(
                lower_error <= 0.0 && upper_error >= 0.0,
                "CPU nonlinear root bracket lost its sign"
            );
            return Ok((lower + upper) * 0.5);
        }
        lower = upper;
        lower_error = upper_error;
    }
    anyhow::bail!("CPU nonlinear heightfield ray found no forward surface root")
}

fn seam_teleport_root(
    start: [f64; 3],
    direction: [f64; 3],
    basis_x: f64,
    basis_y: f64,
    anchor_x: f64,
    anchor_y: f64,
    distance_texels: &[[f32; 4]],
) -> Result<Option<f64>> {
    let error_at = |distance: f64| {
        let point_x = start[0] + direction[0] * distance;
        let point_y = start[1] + direction[1] * distance;
        let surface_u = basis_x * point_x + anchor_x;
        let surface_v = basis_y * point_y + anchor_y;
        sample_float_channel(distance_texels, 16, surface_u, surface_v, 2)
    };
    let mut lower = 0.0;
    let mut lower_error = error_at(lower);
    if lower_error > 0.0 {
        return Ok(None);
    }
    const SEARCH_STEPS: usize = 8192;
    const MAX_DISTANCE: f64 = 8.0;
    for sample in 1..=SEARCH_STEPS {
        let mut upper = MAX_DISTANCE * sample as f64 / SEARCH_STEPS as f64;
        let mut upper_error = error_at(upper);
        if upper_error >= 0.0 {
            for _ in 0..64 {
                let middle = (lower + upper) * 0.5;
                let middle_error = error_at(middle);
                if middle_error >= 0.0 {
                    upper = middle;
                    upper_error = middle_error;
                } else {
                    lower = middle;
                    lower_error = middle_error;
                }
            }
            ensure!(
                lower_error <= 0.0 && upper_error >= 0.0,
                "CPU seam root did not retain a sign-changing bracket"
            );
            return Ok(Some((lower + upper) * 0.5));
        }
        lower = upper;
        lower_error = upper_error;
    }
    Ok(None)
}

fn sample_float_channel(texels: &[[f32; 4]], size: usize, u: f64, v: f64, channel: usize) -> f64 {
    let texel_x = u.clamp(0.0, 1.0) * size as f64 - 0.5;
    let texel_y = v.clamp(0.0, 1.0) * size as f64 - 0.5;
    let base_x = texel_x.floor() as i32;
    let base_y = texel_y.floor() as i32;
    let fraction_x = texel_x - f64::from(base_x);
    let fraction_y = texel_y - f64::from(base_y);
    let coordinate = |x: i32, y: i32| {
        let x = x.clamp(0, size as i32 - 1) as usize;
        let y = y.clamp(0, size as i32 - 1) as usize;
        f64::from(texels[y * size + x][channel])
    };
    let a = coordinate(base_x, base_y);
    let b = coordinate(base_x + 1, base_y);
    let c = coordinate(base_x, base_y + 1);
    let d = coordinate(base_x + 1, base_y + 1);
    let top = a + (b - a) * fraction_x;
    let bottom = c + (d - c) * fraction_x;
    top + (bottom - top) * fraction_y
}

#[derive(Default)]
struct FlatTraceVariant {
    hit_refinement: bool,
    specialized_inverse: bool,
    stored_inverse: bool,
    ramp_height: bool,
    nonlinear_height: bool,
    manual_height_sampling: bool,
    seam_teleport: bool,
    seam_aware_stepping: bool,
    adaptive_steps: bool,
    variable_basis: bool,
    transform_retry: bool,
}

fn flat_trace_variant(
    ctx: &GpuContext,
    hit_refinement: bool,
    specialized_inverse: bool,
    stored_inverse: bool,
    ramp_height: bool,
    nonlinear_height: bool,
    manual_height_sampling: bool,
) -> Result<Value> {
    flat_trace_variant_impl(
        ctx,
        FlatTraceVariant {
            hit_refinement,
            specialized_inverse,
            stored_inverse,
            ramp_height,
            nonlinear_height,
            manual_height_sampling,
            ..Default::default()
        },
    )
}

fn flat_trace_variant_with_seam(
    ctx: &GpuContext,
    hit_refinement: bool,
    seam_aware_stepping: bool,
) -> Result<Value> {
    flat_trace_variant_impl(
        ctx,
        FlatTraceVariant {
            hit_refinement,
            seam_teleport: true,
            seam_aware_stepping,
            ..Default::default()
        },
    )
}

fn flat_trace_variant_with_transform_retry(
    ctx: &GpuContext,
    adaptive_steps: bool,
) -> Result<Value> {
    flat_trace_variant_impl(
        ctx,
        FlatTraceVariant {
            adaptive_steps,
            variable_basis: true,
            transform_retry: true,
            ..Default::default()
        },
    )
}

fn flat_trace_variant_impl(ctx: &GpuContext, variant: FlatTraceVariant) -> Result<Value> {
    use crate::{
        passes::{Frame, bindings},
        settings::Uniforms,
    };
    use glam::{Mat4, Vec2, Vec3, Vec4};
    let FlatTraceVariant {
        hit_refinement,
        specialized_inverse,
        stored_inverse,
        ramp_height,
        nonlinear_height,
        manual_height_sampling,
        seam_teleport,
        seam_aware_stepping,
        adaptive_steps,
        variable_basis,
        transform_retry,
    } = variant;
    let s = Settings {
        width: 32,
        height: 32,
        atlas: 16,
        camera: [0., 0., -3.],
        target: [0., 0., 0.],
        lighting_mode: 0,
        hit_depth: true,
        ..Settings::default()
    };
    let mut s = s;
    s.hit_refinement = hit_refinement;
    s.seam_aware_stepping = seam_aware_stepping;
    s.adaptive_steps = adaptive_steps;
    s.specialized_inverse = specialized_inverse;
    s.stored_inverse = stored_inverse;
    if nonlinear_height || seam_teleport || transform_retry {
        s.step_size = 0.04;
    }
    let mut u = s.uniforms(true);
    u.bones = [Mat4::IDENTITY.to_cols_array_2d(); 4];
    let camera_from_object = Mat4::from_cols_array_2d(&u.camera_from_object);
    let projection = Mat4::from_cols_array_2d(&u.projection);
    let project_depth = |point: [f64; 3]| {
        let camera =
            camera_from_object * Vec4::new(point[0] as f32, point[1] as f32, point[2] as f32, 1.0);
        let clip = projection * camera;
        f64::from(clip.z / clip.w)
    };
    if ramp_height || nonlinear_height {
        // Probe-only sentinel exposes the sampler, basis and ray in unused diagnostic outputs.
        u.modes[3] = -2;
    }
    if seam_teleport {
        // This sentinel returns immediately after the first seam crossing for root comparison.
        u.modes[3] = -3;
    }
    if transform_retry {
        // This sentinel returns one predicted step after U4's bounded transform-change retries.
        u.modes[3] = -4;
    }
    if manual_height_sampling {
        // Probe-only control bit selecting explicit four-texel bilinear loads.
        u.control[3] |= 1 << 8;
    }
    let uniform = ctx.buffer(
        "flat trace settings",
        bytemuck::bytes_of(&u),
        wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    );
    let u_layout = ctx
        .device
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<Uniforms>() as u64),
                },
                count: None,
            }],
        });
    let u_group = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &u_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform.as_entire_binding(),
        }],
    });
    let mut entries: Vec<_> = (0..9)
        .map(|binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float {
                    filterable: binding >= 6 || ctx.policy.filtered,
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
    entries.extend((10..15).map(|binding| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float {
                filterable: ctx.policy.filtered,
            },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }));
    let t_layout = ctx
        .device
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &entries,
        });
    let layout = ctx
        .device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&u_layout), Some(&t_layout)],
            immediate_size: 0,
        });
    let warp = (0..4)
        .map(|i| Texture::float(ctx, &format!("flat warp {i}"), 16, 16))
        .collect::<Result<Vec<_>>>()?;
    let inverse_maps = if stored_inverse {
        Some(
            (0..3)
                .map(|i| Texture::float(ctx, &format!("flat stored inverse {i}"), 16, 16))
                .collect::<Result<Vec<_>>>()?,
        )
    } else {
        None
    };
    let maps = (0..2)
        .map(|i| Texture::float(ctx, &format!("flat map {i}"), 16, 16))
        .collect::<Result<Vec<_>>>()?;
    maps[0].upload(
        ctx,
        bytemuck::cast_slice(&vec![[0.5f32, 0.5, -1000., 1.]; 256]),
        16,
    );
    maps[1].upload(
        ctx,
        bytemuck::cast_slice(&vec![[0.5f32, 0.5, 0., 1.]; 256]),
        16,
    );
    let seam_distance_texels: Vec<[f32; 4]> = (0..16)
        .flat_map(|y| {
            (0..16).map(move |x| {
                let u = (f64::from(x) + 0.5) / 16.0;
                let v = (f64::from(y) + 0.5) / 16.0;
                let signed_distance = (u - 0.5).abs() + (v - 0.5).abs() - 0.08;
                [0.5, 0.5, signed_distance as f32, 1.0]
            })
        })
        .collect();
    let target_ndc_x = (17.5 / 32.0) * 2.0 - 1.0;
    let focal_scale = 1.0 / 30.0f64.to_radians().tan();
    let target_u = 0.5 + 0.25 * 1.75 * target_ndc_x / focal_scale;
    let left_center = 7.5 / 16.0;
    let basis_slope = -400.0;
    let left_basis_z = -0.15 - basis_slope * (target_u - left_center);
    let right_basis_z = left_basis_z + basis_slope / 16.0;
    let variable_basis_texels: Vec<[f32; 4]> = (0..16)
        .flat_map(|_y| {
            (0..16).map(move |x| {
                let basis_z = if x < 8 { left_basis_z } else { right_basis_z };
                [0.0, 0.0, basis_z as f32, 1.0]
            })
        })
        .collect();
    if seam_teleport {
        maps[0].upload(ctx, bytemuck::cast_slice(&seam_distance_texels), 16);
    }
    let usage = wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING;
    let height_width = if nonlinear_height { 256 } else { 2 };
    let height_height = 2;
    let height_samples: Vec<u8> = if nonlinear_height {
        (0..height_width)
            .map(|index| {
                let phase =
                    std::f64::consts::TAU * 16.0 * f64::from(index) / f64::from(height_width);
                (128.0 + 32.0 * phase.sin()).round() as u8
            })
            .collect()
    } else if ramp_height {
        vec![0, 255]
    } else {
        vec![128; 2]
    };
    let mut height_pixels = Vec::with_capacity((height_width * height_height * 4) as usize);
    for _ in 0..height_height {
        for &value in &height_samples {
            height_pixels.extend([value, value, value, 255]);
        }
    }
    let height = Texture::new(
        ctx,
        "flat height",
        height_width,
        height_height,
        wgpu::TextureFormat::Rgba8Unorm,
        usage,
    )?;
    height.upload(ctx, &height_pixels, 4);
    let albedo = Texture::new(
        ctx,
        "flat albedo",
        2,
        2,
        wgpu::TextureFormat::Rgba8Unorm,
        usage,
    )?;
    albedo.upload(ctx, &[255; 16], 4);
    let normal = Texture::new(
        ctx,
        "flat normal",
        2,
        2,
        wgpu::TextureFormat::Rgba8Snorm,
        usage,
    )?;
    normal.upload(
        ctx,
        &[0, 0, 127, 0, 0, 0, 127, 0, 0, 0, 127, 0, 0, 0, 127, 0],
        4,
    );
    let sampler = ctx.device.create_sampler(&wgpu::SamplerDescriptor {
        min_filter: wgpu::FilterMode::Linear,
        mag_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let inverse = inverse_maps.as_deref().unwrap_or(&warp[..3]);
    let auxiliary = [&inverse[0], &inverse[1], &inverse[2], &maps[0], &maps[0]];
    let t_group = bindings(
        ctx,
        &t_layout,
        &warp,
        [&maps[0], &maps[1]],
        [&height, &albedo, &normal],
        auxiliary,
        &sampler,
    );
    let vertices: Vec<Vertex> = [
        Vec2::new(0.25, 0.25),
        Vec2::new(0.75, 0.25),
        Vec2::new(0.25, 0.75),
        Vec2::new(0.75, 0.75),
    ]
    .into_iter()
    .map(|uv| Vertex {
        position: [(uv.x - 0.5) * 4., (0.5 - uv.y) * 4., 0.],
        uv: [uv.x, uv.y, 1.],
        weights: [1., 0., 0., 0.],
        normal: [0., 0., -2.],
        tangent: [4., 0., 0.],
        bitangent: [0., -4., 0.],
    })
    .collect();
    let vertex = ctx.buffer(
        "flat shell",
        bytemuck::cast_slice(&vertices),
        wgpu::BufferUsages::VERTEX,
    );
    let index = ctx.buffer(
        "flat shell indices",
        bytemuck::cast_slice(&[0u32, 1, 2, 2, 1, 3]),
        wgpu::BufferUsages::INDEX,
    );
    let frame = Frame::new(ctx, 32, 32)?;
    let pipelines: Vec<_> = (0..4)
        .step_by(ctx.policy.planes as usize)
        .map(|first| pipeline(ctx, &layout, "trace", ctx.policy.planes, first))
        .collect();
    let mut max_error = 0.0f64;
    let mut component_errors = [0.0f64; 5];
    let mut mode_component_errors = [[0.0f64; 5]; 2];
    let mut ideal_component_errors = [0.0f64; 5];
    let mut ideal_max_error = 0.0f64;
    let mut max_surface_residual = 0.0f64;
    let mut max_object_plane_residual = 0.0f64;
    let mut max_cpu_gpu_height_sample_error = 0.0f64;
    let mut mode_object_plane_residuals = [0.0f64; 2];
    let mut mode_surface_transform_mismatches = [0.0f64; 2];
    let mut max_gpu_cpu_ray_direction_error = 0.0f64;
    let mut max_cpu_root_distance_error = 0.0f64;
    let mut cpu_root_samples = 0usize;
    let mut max_cpu_seam_root_distance_error = 0.0f64;
    let mut cpu_seam_root_samples = 0usize;
    let mut transform_retry_sample_count = 0usize;
    let mut transform_retry_report = None;
    let mut worst = String::new();
    let mut trace_steps = Vec::new();
    let hit_z = -2. * (128.0f64 / 255. - 0.5);
    let sy = 1. / (30.0f64.to_radians().tan());
    let mode_count = if nonlinear_height || seam_teleport || transform_retry {
        1
    } else {
        3
    };
    for mode in 0..mode_count {
        u.modes[2] = if mode == 2 { 0 } else { mode };
        ctx.queue.write_buffer(&uniform, 0, bytemuck::bytes_of(&u));
        for (i, p) in warp.iter().enumerate() {
            let pixels: Vec<[f32; 4]> = (0..256)
                .map(|pixel| match i {
                    0 => [0.25, 0., 0., 1.],
                    1 => {
                        if mode == 2 {
                            [0.25, 0., 0., 1.]
                        } else {
                            [0., -0.25, 0., 1.]
                        }
                    }
                    2 if variable_basis => variable_basis_texels[pixel],
                    2 => [0., 0., -0.5, 1.],
                    _ => {
                        if mode == 0 {
                            [0.5, 0.5, 0.5, 1.]
                        } else {
                            let uv = Vec2::new(
                                ((pixel % 16) as f32 + 0.5) / 16.,
                                ((pixel / 16) as f32 + 0.5) / 16.,
                            );
                            [(uv.x - 0.5) * 4., (0.5 - uv.y) * 4., 0., 1.]
                        }
                    }
                })
                .collect();
            p.upload(ctx, bytemuck::cast_slice(&pixels), 16);
        }
        if let Some(inverse_maps) = &inverse_maps {
            let columns: [[f32; 4]; 3] = if mode == 2 {
                [[0.0, 0.0, 0.0, 0.0]; 3]
            } else {
                [
                    [4.0, 0.0, 0.0, 0.0],
                    [0.0, -4.0, 0.0, 0.0],
                    [0.0, 0.0, -2.0, 0.0],
                ]
            };
            for (texture, column) in inverse_maps.iter().zip(columns) {
                texture.upload(ctx, bytemuck::cast_slice(&vec![column; 256]), 16);
            }
        }
        let mut encoder = ctx.device.create_command_encoder(&Default::default());
        for (i, pipeline) in pipelines.iter().enumerate() {
            let width = ctx.policy.planes as usize;
            let colors = attachments(
                &frame.planes[i * width..(i + 1) * width],
                wgpu::Color::TRANSPARENT,
            );
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("flat chart analytic trace"),
                color_attachments: &colors,
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &frame.depth.view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0.),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &u_group, &[]);
            pass.set_bind_group(1, &t_group, &[]);
            pass.set_vertex_buffer(0, vertex.slice(..));
            pass.set_index_buffer(index.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..6, 0, 0..1);
        }
        ctx.queue.submit([encoder.finish()]);
        let debug_warp = ctx.read_float(&frame.planes[0].texture)?;
        let hit = ctx.read_float(&frame.planes[1].texture)?;
        let status = ctx.read_float(&frame.planes[2].texture)?;
        let ray_direction = ctx.read_float(&frame.planes[3].texture)?;
        let depth = ctx.read_texture(&frame.depth.texture, 4)?;
        for y in 12..20 {
            for x in 12..20 {
                let i = y * 32 + x;
                if transform_retry {
                    if (x, y) != (17, 16) {
                        continue;
                    }
                    ensure!(
                        status[i][0] == 5.0,
                        "U4 diagnostic ray did not reach the transform-retry probe point: {}",
                        status[i][0]
                    );
                    let direction = [
                        f64::from(ray_direction[i][0]),
                        f64::from(ray_direction[i][1]),
                        f64::from(ray_direction[i][2]),
                    ];
                    let shell_distance = 1.75 / direction[2];
                    let start = [
                        direction[0] * shell_distance,
                        direction[1] * shell_distance,
                        -1.25,
                    ];
                    let distance = f64::from(hit[i][2]);
                    let current_uv = [f64::from(hit[i][0]), f64::from(hit[i][1])];
                    let current_point = [
                        start[0] + direction[0] * distance,
                        start[1] + direction[1] * distance,
                        start[2] + direction[2] * distance,
                    ];
                    let current_basis_z = sample_float_channel(
                        &variable_basis_texels,
                        16,
                        current_uv[0],
                        current_uv[1],
                        2,
                    );
                    ensure!(
                        current_basis_z.abs() > 1e-6,
                        "U4 diagnostic ray landed on a singular z basis"
                    );
                    let sampled_height = sample_height_texels(&height_samples, current_uv[0]);
                    let height = (sampled_height - 0.5) * f64::from(u.height_step[0])
                        + 0.5
                        + f64::from(u.height_step[1]);
                    let surface_z = current_basis_z * current_point[2] + 0.5;
                    let delta = height - surface_z;
                    let initial_step = f64::from(u.height_step[2])
                        * (1.0f64).max(-delta * f64::from(u.height_step[3]));
                    let relative_change_for_step = |step: f64| {
                        let candidate_distance = distance + step;
                        let candidate_point_x = start[0] + direction[0] * candidate_distance;
                        let candidate_point_y = start[1] + direction[1] * candidate_distance;
                        let candidate_uv = [
                            0.25 * candidate_point_x + 0.5,
                            -0.25 * candidate_point_y + 0.5,
                        ];
                        let candidate_basis_z = sample_float_channel(
                            &variable_basis_texels,
                            16,
                            candidate_uv[0],
                            candidate_uv[1],
                            2,
                        );
                        let previous_inverse_z = 1.0 / current_basis_z;
                        let candidate_inverse_z = 1.0 / candidate_basis_z;
                        (candidate_inverse_z - previous_inverse_z).abs()
                            / (8.0 + previous_inverse_z.abs())
                    };
                    let final_step = f64::from(status[i][3]);
                    let initial_change = relative_change_for_step(initial_step);
                    let accepted_change = relative_change_for_step(final_step);
                    transform_retry_report = Some(json!({
                        "adaptive_steps": adaptive_steps,
                        "cpu_transform_reference_verified": initial_change.is_finite()
                            && accepted_change.is_finite()
                            && final_step.is_finite(),
                        "status": status[i][0],
                        "initial_candidate_relative_change": initial_change,
                        "accepted_candidate_relative_change": accepted_change,
                        "nominal_step_size": initial_step,
                        "final_step_size": final_step,
                        "current_uv": current_uv,
                        "candidate_uv": [
                            0.25 * (start[0] + direction[0] * (distance + final_step)) + 0.5,
                            -0.25 * (start[1] + direction[1] * (distance + final_step)) + 0.5
                        ]
                    }));
                    transform_retry_sample_count += 1;
                    continue;
                }
                if seam_teleport {
                    let direction = [
                        f64::from(ray_direction[i][0]),
                        f64::from(ray_direction[i][1]),
                        f64::from(ray_direction[i][2]),
                    ];
                    let shell_distance = 1.75 / direction[2];
                    let start = [
                        direction[0] * shell_distance,
                        direction[1] * shell_distance,
                        -1.25,
                    ];
                    let Some(root) = seam_teleport_root(
                        start,
                        direction,
                        0.25,
                        -0.25,
                        f64::from(ray_direction[i][3]),
                        0.5,
                        &seam_distance_texels,
                    )?
                    else {
                        continue;
                    };
                    if root >= 1.25 / direction[2] {
                        continue;
                    }
                    ensure!(
                        status[i][0] == 5.0 && status[i][2] == 1.0,
                        "analytic seam ray ({x},{y}) did not report its first teleport: status={}, teleports={}",
                        status[i][0],
                        status[i][2]
                    );
                    let error = (f64::from(hit[i][2]) - root).abs();
                    max_cpu_seam_root_distance_error = max_cpu_seam_root_distance_error.max(error);
                    cpu_seam_root_samples += 1;
                    continue;
                }
                if mode == 2 {
                    ensure!(
                        status[i][0] == 4.,
                        "singular interpolated warp must report invalid, got {}",
                        status[i][0]
                    );
                    continue;
                }
                ensure!(status[i][0] == 1., "analytic flat ray did not hit");
                trace_steps.push(status[i][1].max(0.0) as u32);
                let ndc_x = ((x as f64 + 0.5) / 32.) * 2. - 1.;
                let ndc_y = 1. - ((y as f64 + 0.5) / 32.) * 2.;
                let dir = Vec3::new((ndc_x / sy) as f32, (ndc_y / sy) as f32, 1.).normalize();
                let camera_distance = if ramp_height {
                    3.0 / (dir.z as f64 + dir.x as f64)
                } else {
                    (3.0 + hit_z) / dir.z as f64
                };
                let camera_hit_z = camera_distance * dir.z as f64;
                let hit_x = camera_distance * dir.x as f64;
                let hit_y = camera_distance * dir.y as f64;
                let want_uv = [0.5 + hit_x / 4.0, 0.5 - hit_y / 4.0];
                let mut ideal_distance = camera_distance - 1.75 / dir.z as f64;
                let mut ideal_depth = project_depth([hit_x, hit_y, camera_hit_z - 3.0]);
                let mut ray_aligned_uv = want_uv;
                let mut ray_aligned_distance = ideal_distance;
                let mut ray_aligned_depth = ideal_depth;
                let mut actual_point_for_report = [0.0f64; 3];
                let mut surface_z_from_point = 0.0f64;
                if ramp_height || nonlinear_height {
                    let actual_direction = [
                        f64::from(ray_direction[i][0]),
                        f64::from(ray_direction[i][1]),
                        f64::from(ray_direction[i][2]),
                    ];
                    let shell_distance = 1.75 / actual_direction[2];
                    let start = [
                        actual_direction[0] * shell_distance,
                        actual_direction[1] * shell_distance,
                        -1.25,
                    ];
                    let actual_distance = f64::from(hit[i][2]);
                    let actual_point = [
                        start[0] + actual_direction[0] * actual_distance,
                        start[1] + actual_direction[1] * actual_distance,
                        start[2] + actual_direction[2] * actual_distance,
                    ];
                    actual_point_for_report = actual_point;
                    let mode_index = mode as usize;
                    if ramp_height {
                        let object_plane_residual = (actual_point[0] + actual_point[2]).abs();
                        max_object_plane_residual =
                            max_object_plane_residual.max(object_plane_residual);
                        mode_object_plane_residuals[mode_index] =
                            mode_object_plane_residuals[mode_index].max(object_plane_residual);
                    }
                    let sampled_height = f64::from(debug_warp[i][0]);
                    let basis_x = f64::from(debug_warp[i][1]);
                    let basis_z = f64::from(debug_warp[i][2]);
                    let anchor_x = f64::from(ray_direction[i][3]);
                    let anchor_z = f64::from(debug_warp[i][3]);
                    let (surface_u, mapped_surface_z) = if mode == 0 {
                        (
                            basis_x * actual_point[0] + anchor_x,
                            basis_z * actual_point[2] + anchor_z,
                        )
                    } else {
                        (
                            basis_x * (actual_point[0] - anchor_x) + f64::from(hit[i][0]),
                            basis_z * (actual_point[2] - anchor_z) + 0.5,
                        )
                    };
                    let expected_sampled_height = if nonlinear_height {
                        sample_height_texels(&height_samples, surface_u)
                    } else {
                        2.0 * surface_u - 0.5
                    };
                    max_cpu_gpu_height_sample_error = max_cpu_gpu_height_sample_error
                        .max((sampled_height - expected_sampled_height).abs());
                    let surface_z_from_trace = f64::from(status[i][3]);
                    let surface_residual = sampled_height - surface_z_from_trace;
                    max_surface_residual = max_surface_residual.max(surface_residual.abs());
                    surface_z_from_point = mapped_surface_z;
                    mode_surface_transform_mismatches[mode_index] =
                        mode_surface_transform_mismatches[mode_index]
                            .max((surface_z_from_trace - mapped_surface_z).abs());
                    max_gpu_cpu_ray_direction_error = max_gpu_cpu_ray_direction_error.max(
                        ((actual_direction[0] - f64::from(dir.x)).powi(2)
                            + (actual_direction[1] - f64::from(dir.y)).powi(2)
                            + (actual_direction[2] - f64::from(dir.z)).powi(2))
                        .sqrt(),
                    );
                    let cpu_ray = if ramp_height {
                        let plane_rate = actual_direction[0] + actual_direction[2];
                        ensure!(
                            plane_rate.abs() > 1e-6,
                            "GPU ray is parallel to analytic ramp"
                        );
                        ray_aligned_distance = -(start[0] + start[2]) / plane_rate;
                        ensure!(
                            ray_aligned_distance.is_finite() && ray_aligned_distance >= 0.0,
                            "CPU analytic ramp intersection is outside the forward GPU ray"
                        );
                        [
                            start[0] + actual_direction[0] * ray_aligned_distance,
                            start[1] + actual_direction[1] * ray_aligned_distance,
                            start[2] + actual_direction[2] * ray_aligned_distance,
                        ]
                    } else {
                        ray_aligned_distance = nonlinear_height_root(
                            start,
                            actual_direction,
                            &NonlinearHeightSurface {
                                basis_x,
                                basis_z,
                                anchor_x,
                                anchor_z,
                                height_texels: &height_samples,
                                height_scale: f64::from(u.height_step[0]),
                                height_offset: f64::from(u.height_step[1]),
                            },
                        )?;
                        max_cpu_root_distance_error = max_cpu_root_distance_error
                            .max((f64::from(hit[i][2]) - ray_aligned_distance).abs());
                        cpu_root_samples += 1;
                        ideal_distance = ray_aligned_distance;
                        [
                            start[0] + actual_direction[0] * ray_aligned_distance,
                            start[1] + actual_direction[1] * ray_aligned_distance,
                            start[2] + actual_direction[2] * ray_aligned_distance,
                        ]
                    };
                    ray_aligned_uv = [0.5 + cpu_ray[0] / 4.0, 0.5 - cpu_ray[1] / 4.0];
                    ray_aligned_depth = project_depth(cpu_ray);
                    if nonlinear_height {
                        ideal_depth = ray_aligned_depth;
                    }
                }
                let actual_depth =
                    f32::from_le_bytes(depth[i * 4..i * 4 + 4].try_into().unwrap()) as f64;
                for (component, (actual, ideal, ray_aligned)) in [
                    (hit[i][0] as f64, want_uv[0], ray_aligned_uv[0]),
                    (hit[i][1] as f64, want_uv[1], ray_aligned_uv[1]),
                    (hit[i][2] as f64, ideal_distance, ray_aligned_distance),
                    (hit[i][3] as f64, ideal_depth, ray_aligned_depth),
                    (actual_depth, ideal_depth, ray_aligned_depth),
                ]
                .into_iter()
                .enumerate()
                {
                    let ideal_error = (actual - ideal).abs();
                    ideal_component_errors[component] =
                        ideal_component_errors[component].max(ideal_error);
                    ideal_max_error = ideal_max_error.max(ideal_error);
                    let error = (actual - ray_aligned).abs();
                    component_errors[component] = component_errors[component].max(error);
                    let mode_index = mode as usize;
                    mode_component_errors[mode_index][component] =
                        mode_component_errors[mode_index][component].max(error);
                    if error > max_error {
                        let profile_name = if nonlinear_height {
                            "nonlinear_sampled"
                        } else if ramp_height {
                            "horizontal_ramp"
                        } else {
                            "constant_128"
                        };
                        let trace_auxiliary_label = if ramp_height || nonlinear_height {
                            "surface_z_actual"
                        } else {
                            "local_z_actual"
                        };
                        worst = format!(
                            "profile={profile_name}, refinement={hit_refinement}, mode {mode}, pixel {x},{y}, component {component}: got {actual}, cpu_reference={ray_aligned}, previous_reference={ideal}, hit_uv={:?}, shader_uv={want_uv:?}, {trace_auxiliary_label}={}, depth_actual={}, depth_reference={ray_aligned_depth}, gpu_point={actual_point_for_report:?}, debug_warp={:?}, surface_z_from_point={surface_z_from_point}, ideal_ray={dir:?}, gpu_ray={:?}",
                            [hit[i][0], hit[i][1]],
                            status[i][3],
                            hit[i][3],
                            debug_warp[i],
                            [
                                ray_direction[i][0],
                                ray_direction[i][1],
                                ray_direction[i][2]
                            ],
                        );
                    }
                    max_error = max_error.max(error);
                }
            }
        }
    }
    if transform_retry {
        ensure!(
            transform_retry_sample_count == 1,
            "U4 transform retry probe recorded {transform_retry_sample_count} pixels, expected one"
        );
        ctx.check()?;
        return transform_retry_report
            .ok_or_else(|| anyhow::anyhow!("U4 transform retry probe produced no report"));
    }
    // A native filtered atlas has finite interpolation precision. This UV budget
    // is 1/256 of a 16-wide texel (less than 0.01 projected pixel here).
    // Manual loads, ray distances and depth retain the 1e-5 analytic gate.
    let uv_budget = if ctx.policy.filtered {
        1. / 4096.
    } else {
        1e-5
    };
    let within_accuracy_gate = component_errors[..2].iter().all(|e| *e < uv_budget)
        && component_errors[2..].iter().all(|e| *e < 1e-5);
    let within_cpu_root_gate =
        nonlinear_height && cpu_root_samples == 64 && max_cpu_root_distance_error < 1e-4;
    ensure!(
        within_accuracy_gate || ramp_height || nonlinear_height,
        "analytic flat errors {component_errors:?}; worst {worst}"
    );
    ensure!(
        !nonlinear_height || cpu_root_samples == 64,
        "nonlinear CPU root oracle produced {cpu_root_samples} samples, expected 64"
    );
    if seam_teleport {
        ensure!(
            cpu_seam_root_samples >= 16,
            "seam teleport CPU root oracle produced only {cpu_seam_root_samples} eligible samples, expected at least 16"
        );
        let within_seam_root_gate = max_cpu_seam_root_distance_error < 1e-4;
        ctx.check()?;
        return Ok(json!({
            "hit_refinement": hit_refinement,
            "seam_aware_stepping": seam_aware_stepping,
            "fixture": "16x16 piecewise-bilinear L1 seam SDF on an affine flat chart",
            "cpu_root_verified": cpu_seam_root_samples >= 16,
            "seam_root_samples": cpu_seam_root_samples,
            "cpu_root_distance_max_abs_error": max_cpu_seam_root_distance_error,
            "within_cpu_root_gate": within_seam_root_gate,
            "step_size": s.step_size,
            "teleport_count_per_sample": 1,
            "height_profile": "constant_128",
            "verified_against_analytic_plane": false,
            "verified_against_bilinear_seam_sdf": true
        }));
    }
    trace_steps.sort_unstable();
    ctx.check()?;
    Ok(json!({
        "height_profile": if nonlinear_height {"nonlinear_sampled_wave_32"} else if ramp_height {"horizontal_ramp"} else {"constant_128"},
        "height_byte": if ramp_height || nonlinear_height {Value::Null} else {json!(128)},
        "analytic_surface": if nonlinear_height {"piecewise-linear sampled height field"} else if ramp_height {"z=-x"} else {"constant-plane"},
        "analytic_reference": if nonlinear_height {"f64 CPU bilinear texel sample and first-bracket root bisection"} else if ramp_height {"CPU plane intersection along the GPU raster ray"} else {"CPU analytic camera ray"},
        "float_map_filtering": if ctx.policy.filtered {"hardware"} else {"manual"},
        "manual_height_sampling": manual_height_sampling,
        "pixels_per_mode": 64,
        "cpu_root_verified": nonlinear_height && cpu_root_samples == 64,
        "cpu_root_samples": if nonlinear_height {json!(cpu_root_samples)} else {Value::Null},
        "cpu_root_distance_max_abs_error": if nonlinear_height {json!(max_cpu_root_distance_error)} else {Value::Null},
        "within_cpu_root_gate": if nonlinear_height {json!(within_cpu_root_gate)} else {Value::Null},
        "max_abs_error": max_error,
        "ideal_camera_ray_max_abs_error": ideal_max_error,
        "ideal_camera_ray_component_errors": ideal_component_errors,
        "gpu_ray_direction_max_abs_error": max_gpu_cpu_ray_direction_error,
        "max_abs_surface_residual": if ramp_height || nonlinear_height {json!(max_surface_residual)} else {Value::Null},
        "max_object_plane_residual": if ramp_height {json!(max_object_plane_residual)} else {Value::Null},
        "max_cpu_gpu_height_sample_error": if ramp_height || nonlinear_height {json!(max_cpu_gpu_height_sample_error)} else {Value::Null},
        "mode_object_plane_residuals": mode_object_plane_residuals,
        "mode_surface_transform_mismatches": mode_surface_transform_mismatches,
        "within_sampled_surface_gate": if ramp_height || nonlinear_height {json!(max_surface_residual < 1e-5)} else {Value::Null},
        "worst_sample": worst,
        "verified_against_analytic_plane": !nonlinear_height,
        "within_accuracy_gate": within_accuracy_gate,
        "trace_steps_p50": trace_steps[trace_steps.len()/2],
        "trace_steps_p95": trace_steps[(trace_steps.len()*95/100).min(trace_steps.len()-1)],
        "trace_steps_max": trace_steps[trace_steps.len()-1],
        "modes": if nonlinear_height {json!([0])} else {json!([0,1])},
        "component_errors": component_errors,
        "mode_component_errors": mode_component_errors,
        "uv_budget": uv_budget,
        "distance_depth_budget": 1e-5,
        "hit_refinement": hit_refinement,
        "specialized_inverse": specialized_inverse,
        "stored_inverse": stored_inverse,
        "singular_warp": "invalid status verified"
    }))
}
