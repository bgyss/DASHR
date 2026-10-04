//! Isolated U20 probe for query-space inversion of a two-bone LBS field.
use crate::{
    asset::{Mesh, Vertex},
    asset_diagnostics::find_self_intersections,
    gpu_resources::GpuContext,
};
use anyhow::{Result, ensure};
use glam::{DMat3, DVec3, Mat4};
use serde_json::{Value, json};
use std::f64::consts::{PI, TAU};
use std::time::Instant;

pub(super) const SEQUENCE_POSE_COUNT: usize = 120;
const DISPATCH_POSE_COUNT: usize = SEQUENCE_POSE_COUNT + 1;
pub(super) const RAYS_PER_POSE: usize = 32;
const DISPATCH_RAY_COUNT: usize = DISPATCH_POSE_COUNT * RAYS_PER_POSE;
const FIELD_SAMPLES: usize = 512;
const BISECTION_STEPS: usize = 40;
const GPU_WARMUP_REPEATS: usize = 2;
const GPU_MEASURED_REPEATS: usize = 10;
const GPU_REPEATS: usize = GPU_WARMUP_REPEATS + GPU_MEASURED_REPEATS;
pub(super) const SAFE_ANGLE_AMPLITUDE: f64 = 0.45;
const SELF_CONTACT_ANGLE: f64 = PI;
pub(super) const BOUND_RADIUS: f64 = 1.02;
pub(super) const DOMAIN_RADIUS: f64 = 1.1;
pub(super) const CAPSULE_HALF_SEGMENT: f64 = 0.75;
pub(super) const CAPSULE_RADIUS: f64 = 0.25;
const MESH_AXIAL_STEPS: usize = 32;
const MESH_RADIAL_STEPS: usize = 24;

#[derive(Clone, Copy)]
pub(super) struct InverseSolution {
    pub(super) canonical: DVec3,
    pub(super) iterations: u32,
    pub(super) residual: f64,
    pub(super) determinant: f64,
}

#[derive(Clone, Copy)]
struct RayReference {
    status: u32,
    distance: f64,
}

#[derive(Default)]
struct CpuReferenceStats {
    inverse_failures: usize,
    inverse_iterations: u64,
    maximum_residual: f64,
    minimum_jacobian_determinant: f64,
}

pub(super) fn run(ctx: &GpuContext) -> Result<Value> {
    let mut references = Vec::with_capacity(DISPATCH_RAY_COUNT);
    let mut cpu_stats = CpuReferenceStats {
        minimum_jacobian_determinant: f64::INFINITY,
        ..Default::default()
    };
    let cpu_started = Instant::now();
    for pose in 0..SEQUENCE_POSE_COUNT {
        let angle = sequence_angle(pose);
        for ray in 0..RAYS_PER_POSE {
            let reference = root_for_ray(ray_x(ray), angle, &mut cpu_stats);
            references.push([reference.status as f32, reference.distance as f32, 0.0, 0.0]);
        }
    }
    for _ in 0..RAYS_PER_POSE {
        references.push([4.0, 0.0, 0.0, 0.0]);
    }
    let cpu_oracle_ms = cpu_started.elapsed().as_secs_f64() * 1000.0;

    let mesh = capsule_mesh();
    let cpu_mesh_started = Instant::now();
    let mut mesh_status_disagreements = 0usize;
    let mut mesh_depth_errors = Vec::new();
    let mut mesh_hit_count = 0usize;
    let mut mesh_mismatch_examples = Vec::new();
    let mut maximum_mesh_depth_sample = Value::Null;
    for pose in 0..SEQUENCE_POSE_COUNT {
        let angle = sequence_angle(pose);
        let posed_vertices = skin_mesh_vertices(&mesh, angle);
        for ray in 0..RAYS_PER_POSE {
            let ray_index = pose * RAYS_PER_POSE + ray;
            let mesh_hit = ray_mesh_distance(&mesh, &posed_vertices, ray_x(ray));
            let analytic = &references[ray_index];
            let analytic_hit = analytic[0] == 1.0;
            if analytic_hit != mesh_hit.is_some() {
                mesh_status_disagreements += 1;
                if mesh_mismatch_examples.len() < 12 {
                    mesh_mismatch_examples.push(json!({
                        "pose": pose,
                        "ray": ray,
                        "ray_x": ray_x(ray),
                        "analytic_status": analytic[0],
                        "analytic_distance": analytic[1],
                        "mesh_distance": mesh_hit
                    }));
                }
            }
            if let (true, Some(distance)) = (analytic_hit, mesh_hit) {
                mesh_hit_count += 1;
                let error = (distance - f64::from(analytic[1])).abs();
                mesh_depth_errors.push(error);
                if maximum_mesh_depth_sample.is_null()
                    || error
                        > maximum_mesh_depth_sample["absolute_error"]
                            .as_f64()
                            .unwrap_or(0.0)
                {
                    maximum_mesh_depth_sample = json!({
                        "absolute_error": error,
                        "pose": pose,
                        "ray": ray,
                        "ray_x": ray_x(ray),
                        "analytic_distance": analytic[1],
                        "mesh_distance": distance
                    });
                }
            }
        }
    }
    mesh_depth_errors.sort_by(f64::total_cmp);
    let stress_bones = [
        Mat4::IDENTITY,
        Mat4::from_rotation_y(SELF_CONTACT_ANGLE as f32),
        Mat4::IDENTITY,
        Mat4::IDENTITY,
    ];
    let stress_overlap_pairs = find_self_intersections(&mesh, &stress_bones)?.len();
    let cpu_mesh_ms = cpu_mesh_started.elapsed().as_secs_f64() * 1000.0;

    let stress_clearance_estimate = endpoint_surface_clearance(SELF_CONTACT_ANGLE);
    let sequence_bound = (0..SEQUENCE_POSE_COUNT)
        .map(|pose| inverse_lipschitz_lower_bound(sequence_angle(pose)))
        .fold(f64::INFINITY, f64::min);
    let stress_bound = inverse_lipschitz_lower_bound(SELF_CONTACT_ANGLE);
    let params = [
        SAFE_ANGLE_AMPLITUDE as f32,
        SELF_CONTACT_ANGLE as f32,
        BOUND_RADIUS as f32,
        DOMAIN_RADIUS as f32,
    ];
    let params_buffer = ctx.buffer(
        "U20 inverse-LBS probe parameters",
        bytemuck::cast_slice(&params),
        wgpu::BufferUsages::UNIFORM,
    );
    let reference_bytes = bytemuck::cast_slice(&references);
    let reference_size = reference_bytes.len() as u64;
    let reference_buffer = ctx.buffer(
        "U20 CPU inverse-LBS ray truth",
        reference_bytes,
        wgpu::BufferUsages::STORAGE,
    );
    let output_size = (DISPATCH_RAY_COUNT * 2 * 16) as u64;
    let output = ctx.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("U20 inverse-LBS probe results"),
        size: output_size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = ctx.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("U20 inverse-LBS probe readback"),
        size: output_size,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let layout = ctx
        .device
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("U20 inverse-LBS probe bindings"),
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
                        min_binding_size: wgpu::BufferSize::new(reference_size),
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
            ],
        });
    let bind_group = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("U20 inverse-LBS probe bindings"),
        layout: &layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: params_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: reference_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: output.as_entire_binding(),
            },
        ],
    });
    let pipeline_layout = ctx
        .device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("U20 inverse-LBS probe pipeline layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
    let shader_source = crate::shaders::inverse_lbs_sdf_probe();
    let module = ctx
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("U20 inverse-LBS SDF shader"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });
    let pipeline = ctx
        .device
        .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("U20 inverse-LBS SDF probe"),
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
            label: Some("U20 inverse-LBS repeated timestamps"),
            ty: wgpu::QueryType::Timestamp,
            count: (GPU_REPEATS * 2) as u32,
        }))
    } else {
        None
    };
    let mut encoder = ctx.device.create_command_encoder(&Default::default());
    for repeat in 0..GPU_REPEATS {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("U20 inverse-LBS 120-pose trace sequence"),
            timestamp_writes: query
                .as_ref()
                .map(|query_set| wgpu::ComputePassTimestampWrites {
                    query_set,
                    beginning_of_pass_write_index: Some((repeat * 2) as u32),
                    end_of_pass_write_index: Some((repeat * 2 + 1) as u32),
                }),
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(DISPATCH_RAY_COUNT.div_ceil(8) as u32, 1, 1);
    }
    let timestamp_readback = query.as_ref().map(|query_set| {
        let byte_size = (GPU_REPEATS * 2 * 8) as u64;
        let resolve = ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("U20 timestamp resolve"),
            size: byte_size,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback = ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("U20 timestamp readback"),
            size: byte_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.resolve_query_set(query_set, 0..(GPU_REPEATS * 2) as u32, &resolve, 0);
        encoder.copy_buffer_to_buffer(&resolve, 0, &readback, 0, byte_size);
        readback
    });
    encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, output_size);
    ctx.queue.submit([encoder.finish()]);

    let result_bytes = ctx.map_buffer(&readback)?;
    let values: Vec<[f32; 4]> = result_bytes
        .chunks_exact(16)
        .map(|record| {
            std::array::from_fn(|channel| {
                f32::from_le_bytes(record[channel * 4..channel * 4 + 4].try_into().unwrap())
            })
        })
        .collect();
    let gpu_times_all_ms = if let Some(readback) = timestamp_readback {
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
    let (gpu_times_warmup_ms, gpu_times_ms) = match gpu_times_all_ms {
        Some(samples) => (
            Some(samples[..GPU_WARMUP_REPEATS].to_vec()),
            Some(samples[GPU_WARMUP_REPEATS..].to_vec()),
        ),
        None => (None, None),
    };

    let mut status_disagreements = 0usize;
    let mut conservative_step_violations = 0usize;
    let mut gpu_inverse_failures = 0usize;
    let mut gpu_inverse_iterations = 0u64;
    let mut max_inverse_residual = 0.0f64;
    let mut minimum_sampled_jacobian_determinant = f64::INFINITY;
    let mut maximum_hit_distance_error = 0.0f64;
    let mut hit_count = 0usize;
    let mut steps = Vec::with_capacity(SEQUENCE_POSE_COUNT * RAYS_PER_POSE);
    for pose in 0..SEQUENCE_POSE_COUNT {
        for ray in 0..RAYS_PER_POSE {
            let ray_index = pose * RAYS_PER_POSE + ray;
            let trace = values[ray_index * 2];
            let diagnostics = values[ray_index * 2 + 1];
            let actual_status = trace[0] as u32;
            let expected_status = references[ray_index][0] as u32;
            let trace_steps = trace[1].max(0.0) as u32;
            steps.push(trace_steps);
            if actual_status != expected_status {
                status_disagreements += 1;
            }
            if actual_status == 4 {
                gpu_inverse_failures += 1;
            }
            if diagnostics[3] > 0.5 {
                conservative_step_violations += 1;
            }
            gpu_inverse_iterations += diagnostics[0].max(0.0) as u64;
            max_inverse_residual = max_inverse_residual.max(f64::from(diagnostics[1]));
            if diagnostics[2].is_finite() && diagnostics[2] > 0.0 {
                minimum_sampled_jacobian_determinant =
                    minimum_sampled_jacobian_determinant.min(f64::from(diagnostics[2]));
            }
            if actual_status == 1 && expected_status == 1 {
                hit_count += 1;
                maximum_hit_distance_error = maximum_hit_distance_error
                    .max((f64::from(trace[2]) - references[ray_index][1] as f64).abs());
            }
        }
    }
    steps.sort_unstable();
    ensure!(
        steps.len() == SEQUENCE_POSE_COUNT * RAYS_PER_POSE,
        "inverse-LBS GPU probe returned an incomplete sequence"
    );
    let stress_start = SEQUENCE_POSE_COUNT * RAYS_PER_POSE;
    let stress_invalid_ray_statuses = (0..RAYS_PER_POSE)
        .filter(|ray| values[(stress_start + ray) * 2][0] as u32 == 4)
        .count();
    let cpu_failures = cpu_stats.inverse_failures;
    let mut gpu_times_sorted = gpu_times_ms.clone().unwrap_or_default();
    gpu_times_sorted.sort_by(f64::total_cmp);
    ctx.check()?;

    Ok(json!({
        "representation": "analytic capsule SDF under spatially varying two-bone linear blend skinning",
        "sequence_pose_count": SEQUENCE_POSE_COUNT,
        "rays_per_pose": RAYS_PER_POSE,
        "safe_sequence_angle_amplitude_radians": SAFE_ANGLE_AMPLITUDE,
        "world_bound_radius": BOUND_RADIUS,
        "canonical_inverse_domain_radius": DOMAIN_RADIUS,
        "cpu_field_samples_per_ray": FIELD_SAMPLES,
        "cpu_root_bisection_steps": BISECTION_STEPS,
        "gpu_warmup_dispatches": GPU_WARMUP_REPEATS,
        "gpu_measured_dispatches": GPU_MEASURED_REPEATS,
        "minimum_certified_inverse_lipschitz_bound": sequence_bound,
        "inverse_solve_failures": cpu_failures + gpu_inverse_failures,
        "cpu_reference_inverse_failures": cpu_failures,
        "gpu_inverse_failures": gpu_inverse_failures,
        "status_disagreements": status_disagreements,
        "conservative_step_violations": conservative_step_violations,
        "hit_rays": hit_count,
        "maximum_hit_distance_error": maximum_hit_distance_error,
        "minimum_sampled_jacobian_determinant": minimum_sampled_jacobian_determinant,
        "maximum_inverse_residual": max_inverse_residual,
        "cpu_inverse_iterations": cpu_stats.inverse_iterations,
        "gpu_inverse_iterations": gpu_inverse_iterations,
        "trace_steps_p50": steps[steps.len() / 2],
        "trace_steps_p95": steps[(steps.len() * 95 / 100).min(steps.len() - 1)],
        "trace_steps_max": steps[steps.len() - 1],
        "cpu_oracle_ms": cpu_oracle_ms,
        "conventional_skinned_mesh_baseline": {
            "pose_count": SEQUENCE_POSE_COUNT,
            "rays_per_pose": RAYS_PER_POSE,
            "triangle_count": mesh.indices.len() / 3,
            "axial_steps": MESH_AXIAL_STEPS,
            "radial_segments": MESH_RADIAL_STEPS,
            "mesh_status_disagreements": mesh_status_disagreements,
            "mesh_hit_count": mesh_hit_count,
            "common_hit_count": mesh_depth_errors.len(),
            "maximum_analytic_depth_difference": mesh_depth_errors.last().copied().unwrap_or(0.0),
            "median_analytic_depth_difference": percentile(&mesh_depth_errors, 0.5),
            "maximum_depth_difference_sample": maximum_mesh_depth_sample,
            "status_mismatch_examples": mesh_mismatch_examples,
            "cpu_mesh_trace_ms": cpu_mesh_ms,
            "stress_pose_self_intersection_pairs": stress_overlap_pairs
        },
        "gpu_compute_warmup_samples_ms": gpu_times_warmup_ms,
        "gpu_compute_samples_ms": gpu_times_ms,
        "gpu_compute_median_ms": percentile(&gpu_times_sorted, 0.5),
        "gpu_compute_p95_ms": percentile(&gpu_times_sorted, 0.95),
        "self_contact_stress_pose": {
            "angle_radians": SELF_CONTACT_ANGLE,
            "end_center_clearance_estimate": stress_clearance_estimate,
            "certified_min_singular_value": stress_bound,
            "invalid_ray_statuses": stress_invalid_ray_statuses,
            "status_code": 4,
            "skinned_mesh_self_intersection_pairs": stress_overlap_pairs
        }
    }))
}

pub(super) fn sequence_angle(pose: usize) -> f64 {
    SAFE_ANGLE_AMPLITUDE * (TAU * pose as f64 / (SEQUENCE_POSE_COUNT - 1) as f64).sin()
}

pub(super) fn inverse_lipschitz_lower_bound(angle: f64) -> f64 {
    let rotation_delta_norm = 2.0 * (0.5 * angle.abs()).sin();
    1.0 - rotation_delta_norm * (1.0 + 0.75 * DOMAIN_RADIUS)
}

pub(super) fn weight_and_derivative(x: f64) -> (f64, f64) {
    let t = ((x + 1.0) * 0.5).clamp(0.0, 1.0);
    let weight = t * t * (3.0 - 2.0 * t);
    let derivative = if t > 0.0 && t < 1.0 {
        3.0 * t * (1.0 - t)
    } else {
        0.0
    };
    (weight, derivative)
}

pub(super) fn rotate_y(point: DVec3, angle: f64) -> DVec3 {
    let (sine, cosine) = angle.sin_cos();
    DVec3::new(
        cosine * point.x + sine * point.z,
        point.y,
        -sine * point.x + cosine * point.z,
    )
}

pub(super) fn forward_lbs(point: DVec3, angle: f64) -> DVec3 {
    let (weight, _) = weight_and_derivative(point.x);
    point.lerp(rotate_y(point, angle), weight)
}

pub(super) fn lbs_jacobian(point: DVec3, angle: f64) -> DMat3 {
    let (weight, weight_derivative) = weight_and_derivative(point.x);
    let (sine, cosine) = angle.sin_cos();
    let rotated_delta = rotate_y(point, angle) - point;
    let column_x = DVec3::new(1.0 + weight * (cosine - 1.0), 0.0, -weight * sine)
        + rotated_delta * weight_derivative;
    let column_y = DVec3::Y;
    let column_z = DVec3::new(weight * sine, 0.0, 1.0 + weight * (cosine - 1.0));
    DMat3::from_cols(column_x, column_y, column_z)
}

pub(super) fn inverse_lbs(world: DVec3, angle: f64) -> Option<InverseSolution> {
    let mut canonical = world;
    let mut minimum_determinant = f64::INFINITY;
    let mut residual_length: f64;
    for iteration in 0..16 {
        let residual = forward_lbs(canonical, angle) - world;
        residual_length = residual.length();
        let jacobian = lbs_jacobian(canonical, angle);
        let determinant = jacobian.determinant();
        if !determinant.is_finite() || determinant <= 1e-8 {
            return None;
        }
        minimum_determinant = minimum_determinant.min(determinant);
        if residual_length <= 1e-10 {
            if canonical.length() <= DOMAIN_RADIUS + 1e-8 {
                return Some(InverseSolution {
                    canonical,
                    iterations: iteration + 1,
                    residual: residual_length,
                    determinant: minimum_determinant,
                });
            }
            return None;
        }
        let delta = jacobian.inverse() * residual;
        if !delta.is_finite() {
            return None;
        }
        let length = delta.length();
        let scale = (0.5 / length.max(1e-30)).min(1.0);
        canonical -= delta * scale;
    }
    residual_length = (forward_lbs(canonical, angle) - world).length();
    if residual_length <= 1e-8 && canonical.length() <= DOMAIN_RADIUS + 1e-8 {
        let determinant = lbs_jacobian(canonical, angle).determinant();
        if determinant > 0.0 && determinant.is_finite() {
            return Some(InverseSolution {
                canonical,
                iterations: 16,
                residual: residual_length,
                determinant: minimum_determinant.min(determinant),
            });
        }
    }
    None
}

pub(super) fn capsule_sdf(point: DVec3) -> f64 {
    let closest = point.x.clamp(-CAPSULE_HALF_SEGMENT, CAPSULE_HALF_SEGMENT);
    (point - DVec3::new(closest, 0.0, 0.0)).length() - CAPSULE_RADIUS
}

pub(super) fn ray_x(ray: usize) -> f64 {
    ((ray as f64 + 0.5) / RAYS_PER_POSE as f64 * 2.0 - 1.0) * 0.9
}

pub(super) fn world_ray_point(x: f64, distance: f64) -> DVec3 {
    DVec3::new(x, 0.0, -2.0 + distance)
}

fn root_for_ray(ray_x: f64, angle: f64, stats: &mut CpuReferenceStats) -> RayReference {
    if ray_x.abs() >= BOUND_RADIUS {
        return RayReference {
            status: 2,
            distance: 0.0,
        };
    }
    let half_chord = (BOUND_RADIUS * BOUND_RADIUS - ray_x * ray_x).sqrt();
    let entry = 2.0 - half_chord;
    let exit = 2.0 + half_chord;
    let mut previous_distance = entry;
    let Some(mut previous_sdf) = sample_field(ray_x, previous_distance, angle, stats) else {
        stats.inverse_failures += 1;
        return RayReference {
            status: 4,
            distance: 0.0,
        };
    };
    if previous_sdf <= 0.0 {
        return RayReference {
            status: 1,
            distance: entry,
        };
    }
    for sample in 1..=FIELD_SAMPLES {
        let distance = entry + (exit - entry) * sample as f64 / FIELD_SAMPLES as f64;
        let Some(sdf) = sample_field(ray_x, distance, angle, stats) else {
            stats.inverse_failures += 1;
            return RayReference {
                status: 4,
                distance: 0.0,
            };
        };
        if previous_sdf > 0.0 && sdf <= 0.0 {
            let mut low = previous_distance;
            let mut high = distance;
            for _ in 0..BISECTION_STEPS {
                let middle = (low + high) * 0.5;
                let Some(middle_sdf) = sample_field(ray_x, middle, angle, stats) else {
                    stats.inverse_failures += 1;
                    return RayReference {
                        status: 4,
                        distance: 0.0,
                    };
                };
                if middle_sdf > 0.0 {
                    low = middle;
                } else {
                    high = middle;
                }
            }
            return RayReference {
                status: 1,
                distance: (low + high) * 0.5,
            };
        }
        previous_distance = distance;
        previous_sdf = sdf;
    }
    RayReference {
        status: 2,
        distance: 0.0,
    }
}

pub(super) fn sequence_ray_references() -> (Vec<[f32; 4]>, usize) {
    let mut references = Vec::with_capacity(SEQUENCE_POSE_COUNT * RAYS_PER_POSE);
    let mut stats = CpuReferenceStats {
        minimum_jacobian_determinant: f64::INFINITY,
        ..Default::default()
    };
    for pose in 0..SEQUENCE_POSE_COUNT {
        let angle = sequence_angle(pose);
        for ray in 0..RAYS_PER_POSE {
            let reference = root_for_ray(ray_x(ray), angle, &mut stats);
            references.push([reference.status as f32, reference.distance as f32, 0.0, 0.0]);
        }
    }
    (references, stats.inverse_failures)
}

fn sample_field(
    ray_x: f64,
    distance: f64,
    angle: f64,
    stats: &mut CpuReferenceStats,
) -> Option<f64> {
    let inverse = inverse_lbs(world_ray_point(ray_x, distance), angle)?;
    stats.inverse_iterations += u64::from(inverse.iterations);
    stats.maximum_residual = stats.maximum_residual.max(inverse.residual);
    stats.minimum_jacobian_determinant =
        stats.minimum_jacobian_determinant.min(inverse.determinant);
    Some(capsule_sdf(inverse.canonical))
}

fn endpoint_surface_clearance(angle: f64) -> f64 {
    let left = forward_lbs(DVec3::new(-CAPSULE_HALF_SEGMENT, 0.0, 0.0), angle);
    let right = forward_lbs(DVec3::new(CAPSULE_HALF_SEGMENT, 0.0, 0.0), angle);
    left.distance(right) - 2.0 * CAPSULE_RADIUS
}

pub(super) fn capsule_mesh() -> Mesh {
    let mut vertices = Vec::new();
    let mut rows: Vec<Vec<u32>> = Vec::with_capacity(MESH_AXIAL_STEPS + 1);
    for axial in 0..=MESH_AXIAL_STEPS {
        let x = -1.0 + 2.0 * axial as f64 / MESH_AXIAL_STEPS as f64;
        let radius = if x < -CAPSULE_HALF_SEGMENT {
            (CAPSULE_RADIUS * CAPSULE_RADIUS - (x + CAPSULE_HALF_SEGMENT).powi(2)).sqrt()
        } else if x > CAPSULE_HALF_SEGMENT {
            (CAPSULE_RADIUS * CAPSULE_RADIUS - (x - CAPSULE_HALF_SEGMENT).powi(2)).sqrt()
        } else {
            CAPSULE_RADIUS
        };
        let (weight, _) = weight_and_derivative(x);
        let mut row = Vec::new();
        if radius <= 1e-12 {
            let pole_normal = if x < 0.0 { -1.0 } else { 1.0 };
            row.push(vertices.len() as u32);
            vertices.push(Vertex {
                position: [x as f32, 0.0, 0.0],
                uv: [0.5, axial as f32 / MESH_AXIAL_STEPS as f32, 1.0],
                weights: [1.0 - weight as f32, weight as f32, 0.0, 0.0],
                normal: [pole_normal, 0.0, 0.0],
                tangent: [0.0, 1.0, 0.0],
                bitangent: [0.0, 0.0, pole_normal],
            });
        } else {
            for radial in 0..MESH_RADIAL_STEPS {
                let angle = TAU * radial as f64 / MESH_RADIAL_STEPS as f64;
                let (sine, cosine) = angle.sin_cos();
                row.push(vertices.len() as u32);
                vertices.push(Vertex {
                    position: [x as f32, (radius * cosine) as f32, (radius * sine) as f32],
                    uv: [
                        radial as f32 / MESH_RADIAL_STEPS as f32,
                        axial as f32 / MESH_AXIAL_STEPS as f32,
                        1.0,
                    ],
                    weights: [1.0 - weight as f32, weight as f32, 0.0, 0.0],
                    normal: [0.0, cosine as f32, sine as f32],
                    tangent: [1.0, 0.0, 0.0],
                    bitangent: [0.0, -sine as f32, cosine as f32],
                });
            }
        }
        rows.push(row);
    }

    let mut indices = Vec::new();
    for rows_pair in rows.windows(2) {
        let lower = &rows_pair[0];
        let upper = &rows_pair[1];
        if lower.len() == 1 {
            for radial in 0..MESH_RADIAL_STEPS {
                let next = (radial + 1) % MESH_RADIAL_STEPS;
                indices.extend([lower[0], upper[radial], upper[next]]);
            }
        } else if upper.len() == 1 {
            for radial in 0..MESH_RADIAL_STEPS {
                let next = (radial + 1) % MESH_RADIAL_STEPS;
                indices.extend([lower[radial], upper[0], lower[next]]);
            }
        } else {
            for radial in 0..MESH_RADIAL_STEPS {
                let next = (radial + 1) % MESH_RADIAL_STEPS;
                indices.extend([
                    lower[radial],
                    upper[radial],
                    upper[next],
                    lower[radial],
                    upper[next],
                    lower[next],
                ]);
            }
        }
    }
    Mesh { vertices, indices }
}

pub(super) fn skin_mesh_vertices(mesh: &Mesh, angle: f64) -> Vec<DVec3> {
    mesh.vertices
        .iter()
        .map(|vertex| {
            let point = DVec3::from_array(vertex.position.map(f64::from));
            let weight = f64::from(vertex.weights[1]);
            point.lerp(rotate_y(point, angle), weight)
        })
        .collect()
}

fn ray_mesh_distance(mesh: &Mesh, posed_vertices: &[DVec3], ray_x: f64) -> Option<f64> {
    ray_mesh_distance_at(mesh, posed_vertices, ray_x, 0.0)
}

pub(super) fn ray_mesh_distance_at(
    mesh: &Mesh,
    posed_vertices: &[DVec3],
    ray_x: f64,
    ray_y: f64,
) -> Option<f64> {
    let origin = DVec3::new(ray_x, ray_y, -2.0);
    let direction = DVec3::Z;
    mesh.indices
        .chunks_exact(3)
        .filter_map(|triangle| {
            ray_triangle_distance(
                origin,
                direction,
                [
                    posed_vertices[triangle[0] as usize],
                    posed_vertices[triangle[1] as usize],
                    posed_vertices[triangle[2] as usize],
                ],
            )
        })
        .min_by(f64::total_cmp)
}

fn ray_triangle_distance(origin: DVec3, direction: DVec3, triangle: [DVec3; 3]) -> Option<f64> {
    const BARYCENTRIC_EPSILON: f64 = 1e-10;
    let edge1 = triangle[1] - triangle[0];
    let edge2 = triangle[2] - triangle[0];
    let p = direction.cross(edge2);
    let determinant = edge1.dot(p);
    if determinant.abs() <= 1e-12 {
        return None;
    }
    let inverse = 1.0 / determinant;
    let from_vertex = origin - triangle[0];
    let u = from_vertex.dot(p) * inverse;
    if !(-BARYCENTRIC_EPSILON..=1.0 + BARYCENTRIC_EPSILON).contains(&u) {
        return None;
    }
    let q = from_vertex.cross(edge1);
    let v = direction.dot(q) * inverse;
    if v < -BARYCENTRIC_EPSILON || u + v > 1.0 + BARYCENTRIC_EPSILON {
        return None;
    }
    let distance = edge2.dot(q) * inverse;
    (distance > 0.0).then_some(distance)
}

fn percentile(sorted: &[f64], p: f64) -> Option<f64> {
    if sorted.is_empty() {
        return None;
    }
    let index = ((sorted.len() - 1) as f64 * p).round() as usize;
    sorted.get(index).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ray_triangle_accepts_roundoff_near_a_shared_edge() {
        let triangle = [
            DVec3::new(0.0, 0.0, 0.0),
            DVec3::new(1.0, 0.0, 0.0),
            DVec3::new(0.0, 1.0, 0.0),
        ];
        let distance = ray_triangle_distance(DVec3::new(0.5, -1e-12, -1.0), DVec3::Z, triangle);

        assert_eq!(distance, Some(1.0));
    }
}
