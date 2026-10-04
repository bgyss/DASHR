//! Isolated U19 Gaussian-splat adapter prototype with explicit compositing/depth semantics.
use super::inverse_lbs;
use crate::gpu_resources::GpuContext;
use anyhow::{Result, ensure};
use bytemuck::{Pod, Zeroable};
use glam::{DMat3, DVec3};
use serde_json::{Value, json};
use std::f64::consts::TAU;
use std::mem::size_of;
use std::time::Instant;

const POSE_ANGLES: [f64; 2] = [0.0, 0.45];
const AXIAL_SAMPLES: usize = 16;
const RADIAL_SAMPLES: usize = 16;
const SPLAT_COUNT: usize = AXIAL_SAMPLES * RADIAL_SAMPLES;
const IMAGE_SIZE: usize = 32;
const PIXELS_PER_POSE: usize = IMAGE_SIZE * IMAGE_SIZE;
const TOTAL_PIXELS: usize = POSE_ANGLES.len() * PIXELS_PER_POSE;
const WORLD_HALF_EXTENT: f64 = 1.15;
const OPACITY: f32 = 0.68;
const SIGMA_AXIAL: f64 = 0.12;
const SIGMA_RING: f64 = 0.08;
const SIGMA_NORMAL: f64 = 0.045;
const SUPPORT_RADIUS_SQUARED: f64 = 9.0;
const GPU_WARMUP_REPEATS: usize = 2;
const GPU_MEASURED_REPEATS: usize = 10;
const GPU_REPEATS: usize = GPU_WARMUP_REPEATS + GPU_MEASURED_REPEATS;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SplatInput {
    /// Canonical center and the second-bone weight.
    center_weight: [f32; 4],
    /// Unpremultiplied RGB and base opacity.
    color_opacity: [f32; 4],
    /// Canonical covariance matrix columns, padded to vec4 alignment.
    covariance_0: [f32; 4],
    covariance_1: [f32; 4],
    covariance_2: [f32; 4],
}

fn capsule_splats() -> Vec<SplatInput> {
    let mut splats = Vec::with_capacity(SPLAT_COUNT);
    for axial in 0..AXIAL_SAMPLES {
        let x = -1.0 + 2.0 * (axial as f64 + 0.5) / AXIAL_SAMPLES as f64;
        let radius = capsule_radius_at_x(x);
        let weight = inverse_lbs::weight_and_derivative(x).0;
        for ring in 0..RADIAL_SAMPLES {
            let angle = TAU * (ring as f64 + 0.5) / RADIAL_SAMPLES as f64;
            let (sine, cosine) = angle.sin_cos();
            let point = DVec3::new(x, radius * cosine, radius * sine);
            let normal = if x < -inverse_lbs::CAPSULE_HALF_SEGMENT {
                DVec3::new(x + inverse_lbs::CAPSULE_HALF_SEGMENT, point.y, point.z).normalize()
            } else if x > inverse_lbs::CAPSULE_HALF_SEGMENT {
                DVec3::new(x - inverse_lbs::CAPSULE_HALF_SEGMENT, point.y, point.z).normalize()
            } else {
                DVec3::new(0.0, cosine, sine)
            };
            let axial_tangent = (DVec3::X - normal * normal.x).normalize();
            let ring_tangent = normal.cross(axial_tangent).normalize();
            let covariance = covariance_from_frame(
                axial_tangent,
                ring_tangent,
                normal,
                SIGMA_AXIAL,
                SIGMA_RING,
                SIGMA_NORMAL,
            );
            let axial_color = ((x + 1.0) * 0.5).clamp(0.0, 1.0);
            let radial_color = ((cosine + 1.0) * 0.5).clamp(0.0, 1.0);
            splats.push(SplatInput {
                center_weight: [
                    point.x as f32,
                    point.y as f32,
                    point.z as f32,
                    weight as f32,
                ],
                color_opacity: [
                    (0.15 + 0.8 * axial_color) as f32,
                    (0.2 + 0.55 * radial_color) as f32,
                    0.72,
                    OPACITY,
                ],
                covariance_0: [
                    covariance.x_axis.x as f32,
                    covariance.x_axis.y as f32,
                    covariance.x_axis.z as f32,
                    0.0,
                ],
                covariance_1: [
                    covariance.y_axis.x as f32,
                    covariance.y_axis.y as f32,
                    covariance.y_axis.z as f32,
                    0.0,
                ],
                covariance_2: [
                    covariance.z_axis.x as f32,
                    covariance.z_axis.y as f32,
                    covariance.z_axis.z as f32,
                    0.0,
                ],
            });
        }
    }
    splats
}

pub(super) fn run(ctx: &GpuContext) -> Result<Value> {
    ensure!(
        size_of::<SplatInput>() == 80,
        "Gaussian storage struct must match five WGSL vec4 values"
    );
    let canonical = capsule_splats();
    let cpu_sort_started = Instant::now();
    let expected_sort_orders: Vec<_> = POSE_ANGLES
        .iter()
        .map(|&angle| sorted_splat_indices(&canonical, angle))
        .collect();
    let cpu_sort_ms = cpu_sort_started.elapsed().as_secs_f64() * 1000.0;
    let ordered_splats: Vec<Vec<_>> = expected_sort_orders
        .iter()
        .map(|order| order.iter().map(|&index| canonical[index]).collect())
        .collect();
    let sort_order_inversions = ordered_splats
        .iter()
        .zip(POSE_ANGLES)
        .map(|(pose, angle)| {
            pose.windows(2)
                .filter(|pair| {
                    deformed_center(&pair[0], angle).z > deformed_center(&pair[1], angle).z
                })
                .count()
        })
        .sum::<usize>();

    let pixel_filter_variance = ((2.0 * WORLD_HALF_EXTENT / IMAGE_SIZE as f64) * 0.35).powi(2);
    let params = [
        POSE_ANGLES[1] as f32,
        WORLD_HALF_EXTENT as f32,
        pixel_filter_variance as f32,
        0.0,
    ];
    let params_buffer = ctx.buffer(
        "U19 Gaussian probe parameters",
        bytemuck::cast_slice(&params),
        wgpu::BufferUsages::UNIFORM,
    );
    let splat_bytes = bytemuck::cast_slice(&canonical);
    let splat_buffer = ctx.buffer(
        "U19 canonical Gaussian primitives",
        splat_bytes,
        wgpu::BufferUsages::STORAGE,
    );
    let sort_order_size = (POSE_ANGLES.len() * SPLAT_COUNT * size_of::<u32>()) as u64;
    let sort_order = ctx.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("U19 GPU-sorted Gaussian primitive indices"),
        size: sort_order_size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let sort_order_readback = ctx.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("U19 Gaussian sort-order readback"),
        size: sort_order_size,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let output_size = (TOTAL_PIXELS * 2 * 16) as u64;
    let output = ctx.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("U19 Gaussian composite output"),
        size: output_size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = ctx.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("U19 Gaussian composite readback"),
        size: output_size,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let layout = ctx
        .device
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("U19 Gaussian probe bindings"),
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
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(splat_bytes.len() as u64),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(output_size),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(sort_order_size),
                    },
                    count: None,
                },
            ],
        });
    let bind_group = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("U19 Gaussian probe bind group"),
        layout: &layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: params_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: splat_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: output.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: sort_order.as_entire_binding(),
            },
        ],
    });
    let pipeline_layout = ctx
        .device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("U19 Gaussian probe pipeline layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
    let shader_source = crate::shaders::gaussian_splat_probe();
    let module = ctx
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("U19 Gaussian raster shader"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });
    let sort_pipeline = ctx
        .device
        .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("U19 Gaussian depth sort"),
            layout: Some(&pipeline_layout),
            module: &module,
            entry_point: Some("cs_sort"),
            compilation_options: Default::default(),
            cache: None,
        });
    let pipeline = ctx
        .device
        .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("U19 Gaussian adapter prototype"),
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
            label: Some("U19 Gaussian repeated timestamps"),
            ty: wgpu::QueryType::Timestamp,
            count: (GPU_REPEATS * 4) as u32,
        }))
    } else {
        None
    };
    let mut encoder = ctx.device.create_command_encoder(&Default::default());
    for repeat in 0..GPU_REPEATS {
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("U19 GPU Gaussian depth sort"),
                timestamp_writes: query.as_ref().map(|query_set| {
                    wgpu::ComputePassTimestampWrites {
                        query_set,
                        beginning_of_pass_write_index: Some((repeat * 4) as u32),
                        end_of_pass_write_index: Some((repeat * 4 + 1) as u32),
                    }
                }),
            });
            pass.set_pipeline(&sort_pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(POSE_ANGLES.len() as u32, 1, 1);
        }
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("U19 front-to-back Gaussian composite"),
                timestamp_writes: query.as_ref().map(|query_set| {
                    wgpu::ComputePassTimestampWrites {
                        query_set,
                        beginning_of_pass_write_index: Some((repeat * 4 + 2) as u32),
                        end_of_pass_write_index: Some((repeat * 4 + 3) as u32),
                    }
                }),
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(TOTAL_PIXELS.div_ceil(8) as u32, 1, 1);
        }
    }
    let timestamp_readback = query.as_ref().map(|query_set| {
        let byte_size = (GPU_REPEATS * 4 * 8) as u64;
        let resolve = ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("U19 Gaussian timestamp resolve"),
            size: byte_size,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback = ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("U19 Gaussian timestamp readback"),
            size: byte_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.resolve_query_set(query_set, 0..(GPU_REPEATS * 4) as u32, &resolve, 0);
        encoder.copy_buffer_to_buffer(&resolve, 0, &readback, 0, byte_size);
        readback
    });
    encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, output_size);
    encoder.copy_buffer_to_buffer(&sort_order, 0, &sort_order_readback, 0, sort_order_size);
    ctx.queue.submit([encoder.finish()]);

    let bytes = ctx.map_buffer(&readback)?;
    let gpu_values: Vec<[f32; 4]> = bytes
        .chunks_exact(16)
        .map(|record| {
            std::array::from_fn(|channel| {
                f32::from_le_bytes(record[channel * 4..channel * 4 + 4].try_into().unwrap())
            })
        })
        .collect();
    let sort_order_bytes = ctx.map_buffer(&sort_order_readback)?;
    let gpu_sort_order: Vec<usize> = sort_order_bytes
        .chunks_exact(size_of::<u32>())
        .map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()) as usize)
        .collect();
    ensure!(
        gpu_sort_order.len() == POSE_ANGLES.len() * SPLAT_COUNT
            && gpu_sort_order.iter().all(|index| *index < canonical.len()),
        "GPU Gaussian sort produced an invalid primitive index"
    );
    let gpu_sorted_order_mismatches = gpu_sort_order
        .iter()
        .zip(expected_sort_orders.iter().flatten())
        .filter(|(actual, expected)| actual != expected)
        .count();
    let gpu_sort_order_inversions = POSE_ANGLES
        .iter()
        .enumerate()
        .map(|(pose, &angle)| {
            gpu_sort_order[pose * SPLAT_COUNT..(pose + 1) * SPLAT_COUNT]
                .windows(2)
                .filter(|pair| {
                    deformed_center(&canonical[pair[0]], angle).z
                        > deformed_center(&canonical[pair[1]], angle).z
                })
                .count()
        })
        .sum::<usize>();
    ensure!(
        gpu_sort_order_inversions == 0,
        "GPU Gaussian sort produced {gpu_sort_order_inversions} depth inversions"
    );

    let gpu_pass_times_all_ms = if let Some(readback) = timestamp_readback {
        let bytes = ctx.map_buffer(&readback)?;
        let period_ns = ctx.queue.get_timestamp_period() as f64;
        Some(
            bytes
                .chunks_exact(16)
                .map(|sample| {
                    let start = u64::from_le_bytes(sample[0..8].try_into().unwrap());
                    let end = u64::from_le_bytes(sample[8..16].try_into().unwrap());
                    end.saturating_sub(start) as f64 * period_ns / 1e6
                })
                .collect::<Vec<_>>(),
        )
    } else {
        None
    };
    let (gpu_sort_warmup_ms, gpu_sort_samples_ms, gpu_warmup_ms, gpu_samples_ms) =
        match gpu_pass_times_all_ms {
            Some(samples) => {
                let sort_samples = (0..GPU_REPEATS)
                    .map(|repeat| samples[repeat * 2])
                    .collect::<Vec<_>>();
                let composite_samples = (0..GPU_REPEATS)
                    .map(|repeat| samples[repeat * 2 + 1])
                    .collect::<Vec<_>>();
                (
                    Some(sort_samples[..GPU_WARMUP_REPEATS].to_vec()),
                    Some(sort_samples[GPU_WARMUP_REPEATS..].to_vec()),
                    Some(composite_samples[..GPU_WARMUP_REPEATS].to_vec()),
                    Some(composite_samples[GPU_WARMUP_REPEATS..].to_vec()),
                )
            }
            None => (None, None, None, None),
        };

    let mesh = inverse_lbs::capsule_mesh();
    let mut cpu_render_ms = 0.0f64;
    let mut cpu_mesh_ms = 0.0f64;
    let mut cpu_outputs = Vec::with_capacity(TOTAL_PIXELS * 2);
    let mut mesh_mask_disagreements = 0usize;
    let mut mesh_intersection = 0usize;
    let mut mesh_union = 0usize;
    let mut mesh_hit_pixels = 0usize;
    let mut mesh_depth_errors = Vec::new();
    let pixel_variance = ((2.0 * WORLD_HALF_EXTENT / IMAGE_SIZE as f64) * 0.35).powi(2);
    for (&angle, ordered) in POSE_ANGLES.iter().zip(&ordered_splats) {
        let render_started = Instant::now();
        let mut pose_cpu_pixels = Vec::with_capacity(PIXELS_PER_POSE);
        for pixel in 0..PIXELS_PER_POSE {
            let (x, y) = pixel_world(pixel);
            pose_cpu_pixels.push(cpu_composite_pixel(ordered, angle, x, y, pixel_variance));
        }
        cpu_render_ms += render_started.elapsed().as_secs_f64() * 1000.0;

        let mesh_started = Instant::now();
        let posed_vertices = inverse_lbs::skin_mesh_vertices(&mesh, angle);
        for (pixel, (splat_color, splat_depth)) in pose_cpu_pixels.iter().enumerate() {
            let (x, y) = pixel_world(pixel);
            let mesh_depth = inverse_lbs::ray_mesh_distance_at(&mesh, &posed_vertices, x, y);
            let splat_coverage = splat_color[3];
            let mesh_hit = mesh_depth.is_some();
            let splat_hit = splat_coverage >= 0.5;
            if mesh_hit {
                mesh_hit_pixels += 1;
            }
            if mesh_hit != splat_hit {
                mesh_mask_disagreements += 1;
            }
            if mesh_hit || splat_hit {
                mesh_union += 1;
            }
            if mesh_hit && splat_hit {
                mesh_intersection += 1;
                mesh_depth_errors.push((splat_depth[0] - mesh_depth.unwrap()).abs());
            }
        }
        cpu_mesh_ms += mesh_started.elapsed().as_secs_f64() * 1000.0;
        cpu_outputs.extend(pose_cpu_pixels);
    }
    let mut maximum_color_error = 0.0f64;
    let mut maximum_coverage_error = 0.0f64;
    let mut maximum_depth_error = 0.0f64;
    let mut output_mismatches = 0usize;
    let mut splat_covered_pixels = 0usize;
    let mut total_contributions = 0usize;
    for pixel in 0..TOTAL_PIXELS {
        let (cpu_color, cpu_depth) = cpu_outputs[pixel];
        let gpu_color = gpu_values[pixel * 2];
        let gpu_depth = gpu_values[pixel * 2 + 1];
        let color_error = (0..3)
            .map(|channel| (f64::from(gpu_color[channel]) - cpu_color[channel]).abs())
            .fold(0.0, f64::max);
        let coverage_error = (f64::from(gpu_color[3]) - cpu_color[3]).abs();
        let depth_error = (f64::from(gpu_depth[0]) - cpu_depth[0]).abs();
        maximum_color_error = maximum_color_error.max(color_error);
        maximum_coverage_error = maximum_coverage_error.max(coverage_error);
        maximum_depth_error = maximum_depth_error.max(depth_error);
        if color_error > 1e-3 || coverage_error > 1e-3 || depth_error > 1e-3 {
            output_mismatches += 1;
        }
        if cpu_color[3] >= 0.5 {
            splat_covered_pixels += 1;
        }
        total_contributions += cpu_depth[1] as usize;
    }
    mesh_depth_errors.sort_by(f64::total_cmp);
    let mut gpu_sort_samples_sorted = gpu_sort_samples_ms.clone().unwrap_or_default();
    gpu_sort_samples_sorted.sort_by(f64::total_cmp);
    let mut gpu_samples_sorted = gpu_samples_ms.clone().unwrap_or_default();
    gpu_samples_sorted.sort_by(f64::total_cmp);
    let silhouette_iou = if mesh_union == 0 {
        1.0
    } else {
        mesh_intersection as f64 / mesh_union as f64
    };
    ctx.check()?;

    Ok(json!({
        "representation": "front-to-back composited anisotropic Gaussian surface samples",
        "pose_count": POSE_ANGLES.len(),
        "pose_angles_radians": POSE_ANGLES,
        "splat_count": SPLAT_COUNT,
        "image_size": [IMAGE_SIZE, IMAGE_SIZE],
        "splat_storage_bytes_per_pose": SPLAT_COUNT * size_of::<SplatInput>(),
        "splat_storage_bytes_all_poses": canonical.len() * size_of::<SplatInput>(),
        "sort_order_storage_bytes": sort_order_size as usize,
        "pose_sort_key": "deformed center z ascending; camera at z=-2 looking +z",
        "sort_order_inversions": sort_order_inversions,
        "gpu_sorted_order_mismatches": gpu_sorted_order_mismatches,
        "gpu_sort_order_inversions": gpu_sort_order_inversions,
        "depth_semantics": "opacity-weighted mean primitive center depth; not a unique surface hit",
        "splat_covered_pixels": splat_covered_pixels,
        "average_contributions_per_pixel": total_contributions as f64 / TOTAL_PIXELS as f64,
        "gpu_cpu_output_mismatches": output_mismatches,
        "cpu_gpu_max_color_error": maximum_color_error,
        "cpu_gpu_max_coverage_error": maximum_coverage_error,
        "cpu_gpu_max_depth_error": maximum_depth_error,
        "mesh_hit_pixels": mesh_hit_pixels,
        "mesh_union_pixels": mesh_union,
        "mesh_splat_mask_disagreements": mesh_mask_disagreements,
        "mesh_splat_silhouette_iou": silhouette_iou,
        "mesh_depth_common_pixels": mesh_depth_errors.len(),
        "mesh_depth_difference_median": percentile(&mesh_depth_errors, 0.5),
        "mesh_depth_difference_max": mesh_depth_errors.last().copied().unwrap_or(0.0),
        "cpu_sort_update_ms": cpu_sort_ms,
        "gpu_sort_warmup_samples_ms": gpu_sort_warmup_ms,
        "gpu_sort_compute_samples_ms": gpu_sort_samples_ms,
        "gpu_sort_compute_median_ms": percentile(&gpu_sort_samples_sorted, 0.5),
        "gpu_sort_compute_p95_ms": percentile(&gpu_sort_samples_sorted, 0.95),
        "cpu_splat_composite_ms": cpu_render_ms,
        "cpu_mesh_update_trace_ms": cpu_mesh_ms,
        "gpu_warmup_samples_ms": gpu_warmup_ms,
        "gpu_compute_samples_ms": gpu_samples_ms,
        "gpu_compute_median_ms": percentile(&gpu_samples_sorted, 0.5),
        "gpu_compute_p95_ms": percentile(&gpu_samples_sorted, 0.95)
    }))
}

fn capsule_radius_at_x(x: f64) -> f64 {
    if x < -inverse_lbs::CAPSULE_HALF_SEGMENT {
        (inverse_lbs::CAPSULE_RADIUS.powi(2) - (x + inverse_lbs::CAPSULE_HALF_SEGMENT).powi(2))
            .sqrt()
    } else if x > inverse_lbs::CAPSULE_HALF_SEGMENT {
        (inverse_lbs::CAPSULE_RADIUS.powi(2) - (x - inverse_lbs::CAPSULE_HALF_SEGMENT).powi(2))
            .sqrt()
    } else {
        inverse_lbs::CAPSULE_RADIUS
    }
}

fn covariance_from_frame(
    axial: DVec3,
    ring: DVec3,
    normal: DVec3,
    sigma_axial: f64,
    sigma_ring: f64,
    sigma_normal: f64,
) -> DMat3 {
    let axial_variance = sigma_axial * sigma_axial;
    let ring_variance = sigma_ring * sigma_ring;
    let normal_variance = sigma_normal * sigma_normal;
    let column = |component: usize| {
        axial * (axial_variance * axial[component])
            + ring * (ring_variance * ring[component])
            + normal * (normal_variance * normal[component])
    };
    DMat3::from_cols(column(0), column(1), column(2))
}

fn covariance_from_splat(splat: &SplatInput) -> DMat3 {
    DMat3::from_cols(
        DVec3::new(
            f64::from(splat.covariance_0[0]),
            f64::from(splat.covariance_0[1]),
            f64::from(splat.covariance_0[2]),
        ),
        DVec3::new(
            f64::from(splat.covariance_1[0]),
            f64::from(splat.covariance_1[1]),
            f64::from(splat.covariance_1[2]),
        ),
        DVec3::new(
            f64::from(splat.covariance_2[0]),
            f64::from(splat.covariance_2[1]),
            f64::from(splat.covariance_2[2]),
        ),
    )
}

fn deformed_center(splat: &SplatInput, angle: f64) -> DVec3 {
    let canonical = DVec3::new(
        f64::from(splat.center_weight[0]),
        f64::from(splat.center_weight[1]),
        f64::from(splat.center_weight[2]),
    );
    canonical.lerp(
        inverse_lbs::rotate_y(canonical, angle),
        f64::from(splat.center_weight[3]),
    )
}

fn sorted_splat_indices(canonical: &[SplatInput], angle: f64) -> Vec<usize> {
    let mut order: Vec<_> = (0..canonical.len()).collect();
    order.sort_by(|left, right| {
        deformed_center(&canonical[*left], angle)
            .z
            .total_cmp(&deformed_center(&canonical[*right], angle).z)
            .then_with(|| left.cmp(right))
    });
    order
}

fn pixel_world(pixel: usize) -> (f64, f64) {
    let x = pixel % IMAGE_SIZE;
    let y = pixel / IMAGE_SIZE;
    (
        ((x as f64 + 0.5) / IMAGE_SIZE as f64 * 2.0 - 1.0) * WORLD_HALF_EXTENT,
        ((y as f64 + 0.5) / IMAGE_SIZE as f64 * 2.0 - 1.0) * WORLD_HALF_EXTENT,
    )
}

fn cpu_composite_pixel(
    splats: &[SplatInput],
    angle: f64,
    pixel_x: f64,
    pixel_y: f64,
    pixel_variance: f64,
) -> ([f64; 4], [f64; 4]) {
    let mut transmittance = 1.0f64;
    let mut color = [0.0f64; 3];
    let mut depth_sum = 0.0f64;
    let mut contributions = 0u32;
    for splat in splats {
        let center = deformed_center(splat, angle);
        let jacobian = inverse_lbs::lbs_jacobian(
            DVec3::new(
                f64::from(splat.center_weight[0]),
                f64::from(splat.center_weight[1]),
                f64::from(splat.center_weight[2]),
            ),
            angle,
        );
        let covariance = jacobian * covariance_from_splat(splat) * jacobian.transpose();
        let xx = covariance.x_axis.x + pixel_variance;
        let xy = covariance.y_axis.x;
        let yy = covariance.y_axis.y + pixel_variance;
        let determinant = xx * yy - xy * xy;
        if determinant <= 1e-16 {
            continue;
        }
        let delta = DVec3::new(pixel_x - center.x, pixel_y - center.y, 0.0);
        let radius_squared = (yy * delta.x * delta.x - 2.0 * xy * delta.x * delta.y
            + xx * delta.y * delta.y)
            / determinant;
        if radius_squared > SUPPORT_RADIUS_SQUARED {
            continue;
        }
        let alpha =
            (f64::from(splat.color_opacity[3]) * (-0.5 * radius_squared).exp()).clamp(0.0, 0.99);
        if alpha <= 1e-6 {
            continue;
        }
        let contribution = transmittance * alpha;
        for (channel, accumulated) in color.iter_mut().enumerate() {
            *accumulated += contribution * f64::from(splat.color_opacity[channel]);
        }
        depth_sum += contribution * (2.0 + center.z);
        transmittance *= 1.0 - alpha;
        contributions += 1;
        if transmittance < 0.001 {
            break;
        }
    }
    let coverage = 1.0 - transmittance;
    let mean_depth = if coverage > 1e-8 {
        depth_sum / coverage
    } else {
        0.0
    };
    (
        [color[0], color[1], color[2], coverage],
        [mean_depth, f64::from(contributions), transmittance, 0.0],
    )
}

fn percentile(sorted: &[f64], p: f64) -> Option<f64> {
    if sorted.is_empty() {
        return None;
    }
    let index = ((sorted.len() - 1) as f64 * p).round() as usize;
    sorted.get(index).copied()
}
