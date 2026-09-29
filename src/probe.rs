//! Numerical native-device gates, independent of the full viewer.
use crate::{
    asset::Vertex,
    gpu_resources::{GpuContext, Texture},
    passes::{attachments, pipeline},
    settings::Settings,
    shaders,
};
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
        json!({"uniform_bytes_round_tripped":544,"filtered_sample":&values[..4],"nearest_sample":&values[4..],"affine_max_abs_error":max_error,"rgba32float_mrt":"rendered and read back","distortion_modes_tested":[0,1],"flat_trace":flat_trace(ctx)?}),
    )
}

fn flat_trace(ctx: &GpuContext) -> Result<Value> {
    use crate::{
        passes::{Frame, bindings},
        settings::Uniforms,
    };
    use glam::{Mat4, Vec2, Vec3};
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
    let mut u = s.uniforms(true);
    u.bones = [Mat4::IDENTITY.to_cols_array_2d(); 4];
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
    let usage = wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING;
    let height = Texture::new(
        ctx,
        "flat height",
        2,
        2,
        wgpu::TextureFormat::Rgba8Unorm,
        usage,
    )?;
    height.upload(ctx, &[128; 16], 4);
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
    let t_group = bindings(
        ctx,
        &t_layout,
        &warp,
        [&maps[0], &maps[1]],
        [&height, &albedo, &normal],
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
    let mut worst = String::new();
    let hit_z = -2. * (128.0f64 / 255. - 0.5);
    let camera_hit_z = 3. + hit_z;
    let sy = 1. / (30.0f64.to_radians().tan());
    for mode in 0..3 {
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
        let hit = ctx.read_float(&frame.planes[1].texture)?;
        let status = ctx.read_float(&frame.planes[2].texture)?;
        let depth = ctx.read_texture(&frame.depth.texture, 4)?;
        for y in 12..20 {
            for x in 12..20 {
                let i = y * 32 + x;
                if mode == 2 {
                    ensure!(
                        status[i][0] == 4.,
                        "singular interpolated warp must report invalid, got {}",
                        status[i][0]
                    );
                    continue;
                }
                ensure!(status[i][0] == 1., "analytic flat ray did not hit");
                let ndc_x = ((x as f64 + 0.5) / 32.) * 2. - 1.;
                let ndc_y = 1. - ((y as f64 + 0.5) / 32.) * 2.;
                let dir = Vec3::new((ndc_x / sy) as f32, (ndc_y / sy) as f32, 1.).normalize();
                let want_uv = [
                    0.5 + ndc_x * camera_hit_z / sy / 4.,
                    0.5 - ndc_y * camera_hit_z / sy / 4.,
                ];
                let want_distance = (1.25 + hit_z) / dir.z as f64;
                let want_depth = 0.1 / camera_hit_z;
                for (component, (a, b)) in [
                    (hit[i][0] as f64, want_uv[0]),
                    (hit[i][1] as f64, want_uv[1]),
                    (hit[i][2] as f64, want_distance),
                    (hit[i][3] as f64, want_depth),
                    (
                        f32::from_le_bytes(depth[i * 4..i * 4 + 4].try_into().unwrap()) as f64,
                        want_depth,
                    ),
                ]
                .into_iter()
                .enumerate()
                {
                    let error = (a - b).abs();
                    component_errors[component] = component_errors[component].max(error);
                    if error > max_error {
                        worst = format!(
                            "mode {mode}, pixel {x},{y}, component {component}: got {a}, want {b}"
                        );
                    }
                    max_error = max_error.max(error);
                }
            }
        }
    }
    // A native filtered atlas has finite interpolation precision. This UV budget
    // is 1/256 of a 16-wide texel (less than 0.01 projected pixel here).
    // Manual loads, ray distances and depth retain the 1e-5 analytic gate.
    let uv_budget = if ctx.policy.filtered {
        1. / 4096.
    } else {
        1e-5
    };
    ensure!(
        component_errors[..2].iter().all(|e| *e < uv_budget)
            && component_errors[2..].iter().all(|e| *e < 1e-5),
        "analytic flat errors {component_errors:?}; worst {worst}"
    );
    ctx.check()?;
    Ok(
        json!({"height_byte":128,"hit_plane_object_z":hit_z,"pixels_per_mode":64,"max_abs_error":max_error,"actual_depth":"verified against analytic plane","modes":[0,1],"component_errors":component_errors,"uv_budget":uv_budget,"distance_depth_budget":1e-5,"singular_warp":"invalid status verified"}),
    )
}
