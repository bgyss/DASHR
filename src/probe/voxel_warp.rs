//! U20 follow-up: warp a dense trilinear SDF through the measured inverse-LBS map.
use super::inverse_lbs;
use crate::gpu_resources::GpuContext;
use anyhow::{Result, ensure};
use glam::DVec3;
use serde_json::{Value, json};
use std::time::Instant;

const BASE_GRID_RESOLUTION: usize = 32;
const HIGHER_GRID_RESOLUTION: usize = 64;
const GRID_HALF_EXTENT: f64 = 1.5;
const GPU_WARMUP_REPEATS: usize = 10;
const GPU_MEASURED_REPEATS: usize = 10;
const GPU_REPEATS: usize = GPU_WARMUP_REPEATS + GPU_MEASURED_REPEATS;
const FIELD_SAMPLES: usize = 512;
const BISECTION_STEPS: usize = 40;

#[derive(Default)]
struct CpuVoxelStats {
    inverse_failures: usize,
    inverse_iterations: u64,
    maximum_inverse_residual: f64,
    minimum_jacobian_determinant: f64,
}

pub(super) fn run(ctx: &GpuContext) -> Result<Value> {
    let mut baseline = run_resolution(ctx, BASE_GRID_RESOLUTION)?;
    let higher_resolution = run_resolution(ctx, HIGHER_GRID_RESOLUTION)?;
    baseline["higher_resolution"] = higher_resolution;
    Ok(baseline)
}

fn run_resolution(ctx: &GpuContext, grid_resolution: usize) -> Result<Value> {
    let (analytic_references, analytic_inverse_failures) = inverse_lbs::sequence_ray_references();
    let grid = build_capsule_grid(grid_resolution);
    let grid_spacing = grid_spacing(grid_resolution);
    let lipschitz_bound = trilinear_lipschitz_bound(&grid, grid_resolution);
    ensure!(
        lipschitz_bound.is_finite() && lipschitz_bound > 0.0,
        "dense capsule voxel field has invalid trilinear Lipschitz bound"
    );
    let grid_error = interpolation_error(&grid, grid_resolution);
    let cpu_started = Instant::now();
    let mut cpu_stats = CpuVoxelStats {
        minimum_jacobian_determinant: f64::INFINITY,
        ..Default::default()
    };
    let voxel_references: Vec<_> = (0..inverse_lbs::SEQUENCE_POSE_COUNT)
        .flat_map(|pose| {
            let angle = inverse_lbs::sequence_angle(pose);
            (0..inverse_lbs::RAYS_PER_POSE).map(move |ray| (angle, ray))
        })
        .map(|(angle, ray)| voxel_root_for_ray(ray, angle, &grid, grid_resolution, &mut cpu_stats))
        .collect();
    let cpu_oracle_ms = cpu_started.elapsed().as_secs_f64() * 1000.0;

    let params = [
        inverse_lbs::SAFE_ANGLE_AMPLITUDE as f32,
        inverse_lbs::BOUND_RADIUS as f32,
        inverse_lbs::DOMAIN_RADIUS as f32,
        lipschitz_bound as f32,
    ];
    let params_buffer = ctx.buffer(
        "U20 voxel inverse-LBS parameters",
        bytemuck::cast_slice(&params),
        wgpu::BufferUsages::UNIFORM,
    );
    let grid_info = [grid_resolution as u32, 0, 0, 0];
    let grid_info_buffer = ctx.buffer(
        "U20 voxel grid dimensions",
        bytemuck::cast_slice(&grid_info),
        wgpu::BufferUsages::UNIFORM,
    );
    let voxel_buffer = ctx.buffer(
        "U20 dense capsule SDF grid",
        bytemuck::cast_slice(&grid),
        wgpu::BufferUsages::STORAGE,
    );
    let reference_bytes = bytemuck::cast_slice(&voxel_references);
    let reference_size = reference_bytes.len() as u64;
    let reference_buffer = ctx.buffer(
        "U20 CPU trilinear voxel ray roots",
        reference_bytes,
        wgpu::BufferUsages::STORAGE,
    );
    let output_size = (voxel_references.len() * 2 * 16) as u64;
    let output = ctx.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("U20 voxel inverse-LBS results"),
        size: output_size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = ctx.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("U20 voxel inverse-LBS readback"),
        size: output_size,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let layout = ctx
        .device
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("U20 inverse-LBS voxel bindings"),
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
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(16),
                    },
                    count: None,
                },
                storage_binding(2, reference_size, true),
                storage_binding(3, grid.len() as u64 * 4, true),
                storage_binding(4, output_size, false),
            ],
        });
    let bind_group = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("U20 inverse-LBS voxel bind group"),
        layout: &layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: params_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: grid_info_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: reference_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: voxel_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: output.as_entire_binding(),
            },
        ],
    });
    let pipeline_layout = ctx
        .device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("U20 inverse-LBS voxel layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
    let source = crate::shaders::inverse_lbs_voxel_probe();
    let module = ctx
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("U20 inverse-LBS voxel shader"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
    let pipeline = ctx
        .device
        .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("U20 inverse-LBS voxel ray marcher"),
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
            label: Some("U20 inverse-LBS voxel timestamps"),
            ty: wgpu::QueryType::Timestamp,
            count: (GPU_REPEATS * 2) as u32,
        }))
    } else {
        None
    };
    let mut encoder = ctx.device.create_command_encoder(&Default::default());
    for repeat in 0..GPU_REPEATS {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("U20 inverse-LBS dense voxel trace"),
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
        pass.dispatch_workgroups(voxel_references.len().div_ceil(8) as u32, 1, 1);
    }
    let timestamp_readback = query.as_ref().map(|query_set| {
        let byte_size = (GPU_REPEATS * 2 * 8) as u64;
        let resolve = ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("U20 voxel timestamp resolve"),
            size: byte_size,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback = ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("U20 voxel timestamp readback"),
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

    let bytes = ctx.map_buffer(&readback)?;
    let values: Vec<[f32; 4]> = bytes
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
    let (gpu_warmup_ms, gpu_samples_ms) = match gpu_times_all_ms {
        Some(samples) => (
            Some(samples[..GPU_WARMUP_REPEATS].to_vec()),
            Some(samples[GPU_WARMUP_REPEATS..].to_vec()),
        ),
        None => (None, None),
    };

    let mut status_disagreements_vs_voxel_cpu = 0usize;
    let mut status_disagreements_vs_analytic = 0usize;
    let mut conservative_step_violations = 0usize;
    let mut inverse_solve_failures = analytic_inverse_failures + cpu_stats.inverse_failures;
    let mut voxel_hit_errors = Vec::new();
    let mut analytic_hit_errors = Vec::new();
    let mut steps = Vec::with_capacity(voxel_references.len());
    for ray in 0..voxel_references.len() {
        let trace = values[ray * 2];
        let diagnostics = values[ray * 2 + 1];
        let status = trace[0] as u32;
        let voxel_status = voxel_references[ray][0] as u32;
        let analytic_status = analytic_references[ray][0] as u32;
        if status != voxel_status {
            status_disagreements_vs_voxel_cpu += 1;
        }
        if status != analytic_status {
            status_disagreements_vs_analytic += 1;
        }
        if status == 4 {
            inverse_solve_failures += 1;
        }
        if diagnostics[3] > 0.5 {
            conservative_step_violations += 1;
        }
        steps.push(trace[1].max(0.0) as u32);
        if status == 1 && voxel_status == 1 {
            voxel_hit_errors
                .push((f64::from(trace[2]) - f64::from(voxel_references[ray][1])).abs());
        }
        if status == 1 && analytic_status == 1 {
            analytic_hit_errors
                .push((f64::from(trace[2]) - f64::from(analytic_references[ray][1])).abs());
        }
    }
    steps.sort_unstable();
    voxel_hit_errors.sort_by(f64::total_cmp);
    analytic_hit_errors.sort_by(f64::total_cmp);
    let mut gpu_samples_sorted = gpu_samples_ms.clone().unwrap_or_default();
    gpu_samples_sorted.sort_by(f64::total_cmp);
    ctx.check()?;

    Ok(json!({
        "representation": format!("{grid_resolution}-cubed trilinear capsule SDF sampled in inverse-LBS canonical space"),
        "sequence_pose_count": inverse_lbs::SEQUENCE_POSE_COUNT,
        "rays_per_pose": inverse_lbs::RAYS_PER_POSE,
        "pose_angle_amplitude_radians": inverse_lbs::SAFE_ANGLE_AMPLITUDE,
        "world_bound_radius": inverse_lbs::BOUND_RADIUS,
        "canonical_inverse_domain_radius": inverse_lbs::DOMAIN_RADIUS,
        "cpu_root_samples_per_ray": FIELD_SAMPLES,
        "cpu_root_bisection_steps": BISECTION_STEPS,
        "resolution": [grid_resolution, grid_resolution, grid_resolution],
        "grid_bounds": [-GRID_HALF_EXTENT, GRID_HALF_EXTENT],
        "sample_spacing": grid_spacing,
        "voxel_payload_bytes": grid.len() * 4,
        "trilinear_lipschitz_bound": lipschitz_bound,
        "sampled_interpolation_error": grid_error,
        "inverse_solve_failures": inverse_solve_failures,
        "analytic_cpu_root_failures": analytic_inverse_failures,
        "status_disagreements_vs_voxel_cpu": status_disagreements_vs_voxel_cpu,
        "status_disagreements_vs_analytic": status_disagreements_vs_analytic,
        "conservative_step_violations": conservative_step_violations,
        "step_violation_tolerance_object_units": 2e-4,
        "maximum_hit_distance_error_vs_voxel_cpu": voxel_hit_errors.last().copied().unwrap_or(0.0),
        "maximum_hit_distance_error_vs_analytic": analytic_hit_errors.last().copied().unwrap_or(0.0),
        "median_hit_distance_error_vs_analytic": percentile(&analytic_hit_errors, 0.5),
        "trace_steps_p50": steps[steps.len() / 2],
        "trace_steps_p95": steps[(steps.len() * 95 / 100).min(steps.len() - 1)],
        "trace_steps_max": steps[steps.len() - 1],
        "cpu_voxel_root_failures": cpu_stats.inverse_failures,
        "cpu_inverse_iterations": cpu_stats.inverse_iterations,
        "cpu_minimum_sampled_jacobian_determinant": cpu_stats.minimum_jacobian_determinant,
        "cpu_maximum_inverse_residual": cpu_stats.maximum_inverse_residual,
        "cpu_oracle_ms": cpu_oracle_ms,
        "gpu_warmup_repeats": GPU_WARMUP_REPEATS,
        "gpu_measured_repeats": GPU_MEASURED_REPEATS,
        "gpu_warmup_samples_ms": gpu_warmup_ms,
        "gpu_compute_samples_ms": gpu_samples_ms,
        "gpu_compute_median_ms": percentile(&gpu_samples_sorted, 0.5),
        "gpu_compute_p95_ms": percentile(&gpu_samples_sorted, 0.95)
    }))
}

fn storage_binding(binding: u32, byte_size: u64, read_only: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: wgpu::BufferSize::new(byte_size),
        },
        count: None,
    }
}

fn grid_spacing(resolution: usize) -> f64 {
    (2.0 * GRID_HALF_EXTENT) / (resolution - 1) as f64
}

fn build_capsule_grid(resolution: usize) -> Vec<f32> {
    let spacing = grid_spacing(resolution);
    let mut grid = Vec::with_capacity(resolution.pow(3));
    for z in 0..resolution {
        for y in 0..resolution {
            for x in 0..resolution {
                let point = DVec3::new(
                    -GRID_HALF_EXTENT + x as f64 * spacing,
                    -GRID_HALF_EXTENT + y as f64 * spacing,
                    -GRID_HALF_EXTENT + z as f64 * spacing,
                );
                grid.push(inverse_lbs::capsule_sdf(point) as f32);
            }
        }
    }
    grid
}

fn trilinear_sample(grid: &[f32], point: DVec3, resolution: usize) -> f64 {
    let spacing = grid_spacing(resolution);
    let grid_position = (point + DVec3::splat(GRID_HALF_EXTENT)) / spacing;
    let maximum_base = (resolution - 2) as i32;
    let base = [
        grid_position.x.floor().clamp(0.0, f64::from(maximum_base)) as usize,
        grid_position.y.floor().clamp(0.0, f64::from(maximum_base)) as usize,
        grid_position.z.floor().clamp(0.0, f64::from(maximum_base)) as usize,
    ];
    let fraction = [
        (grid_position.x - base[0] as f64).clamp(0.0, 1.0),
        (grid_position.y - base[1] as f64).clamp(0.0, 1.0),
        (grid_position.z - base[2] as f64).clamp(0.0, 1.0),
    ];
    let sample = |dx: usize, dy: usize, dz: usize| {
        f64::from(grid[((base[2] + dz) * resolution + base[1] + dy) * resolution + base[0] + dx])
    };
    let x00 = sample(0, 0, 0) * (1.0 - fraction[0]) + sample(1, 0, 0) * fraction[0];
    let x10 = sample(0, 1, 0) * (1.0 - fraction[0]) + sample(1, 1, 0) * fraction[0];
    let x01 = sample(0, 0, 1) * (1.0 - fraction[0]) + sample(1, 0, 1) * fraction[0];
    let x11 = sample(0, 1, 1) * (1.0 - fraction[0]) + sample(1, 1, 1) * fraction[0];
    let y0 = x00 * (1.0 - fraction[1]) + x10 * fraction[1];
    let y1 = x01 * (1.0 - fraction[1]) + x11 * fraction[1];
    y0 * (1.0 - fraction[2]) + y1 * fraction[2]
}

fn trilinear_lipschitz_bound(grid: &[f32], resolution: usize) -> f64 {
    let spacing = grid_spacing(resolution);
    let mut max_gradient = [0.0f64; 3];
    for z in 0..resolution {
        for y in 0..resolution {
            for x in 0..resolution {
                let index = (z * resolution + y) * resolution + x;
                if x + 1 < resolution {
                    let edge = (f64::from(grid[index + 1]) - f64::from(grid[index])).abs();
                    max_gradient[0] = max_gradient[0].max(edge / spacing);
                }
                if y + 1 < resolution {
                    let edge = (f64::from(grid[index + resolution]) - f64::from(grid[index])).abs();
                    max_gradient[1] = max_gradient[1].max(edge / spacing);
                }
                if z + 1 < resolution {
                    let edge = (f64::from(grid[index + resolution * resolution])
                        - f64::from(grid[index]))
                    .abs();
                    max_gradient[2] = max_gradient[2].max(edge / spacing);
                }
            }
        }
    }
    DVec3::from_array(max_gradient).length() * (1.0 + 1e-6)
}

fn interpolation_error(grid: &[f32], resolution: usize) -> Value {
    let spacing = grid_spacing(resolution);
    let mut maximum = 0.0f64;
    let mut squared = 0.0f64;
    let mut samples = 0usize;
    for z in 0..resolution - 1 {
        for y in 0..resolution - 1 {
            for x in 0..resolution - 1 {
                let point = DVec3::new(
                    -GRID_HALF_EXTENT + (x as f64 + 0.5) * spacing,
                    -GRID_HALF_EXTENT + (y as f64 + 0.5) * spacing,
                    -GRID_HALF_EXTENT + (z as f64 + 0.5) * spacing,
                );
                let error = (trilinear_sample(grid, point, resolution)
                    - inverse_lbs::capsule_sdf(point))
                .abs();
                maximum = maximum.max(error);
                squared += error * error;
                samples += 1;
            }
        }
    }
    json!({
        "sample_count": samples,
        "max_abs_error": maximum,
        "rms_error": (squared / samples as f64).sqrt()
    })
}

fn voxel_root_for_ray(
    ray: usize,
    angle: f64,
    grid: &[f32],
    resolution: usize,
    stats: &mut CpuVoxelStats,
) -> [f32; 4] {
    let ray_x = inverse_lbs::ray_x(ray);
    if ray_x.abs() >= inverse_lbs::BOUND_RADIUS {
        return [2.0, 0.0, 0.0, 0.0];
    }
    let half_chord = (inverse_lbs::BOUND_RADIUS.powi(2) - ray_x.powi(2)).sqrt();
    let entry = 2.0 - half_chord;
    let exit = 2.0 + half_chord;
    let mut previous_distance = entry;
    let Some(mut previous_field) =
        field_on_ray(ray_x, previous_distance, angle, grid, resolution, stats)
    else {
        stats.inverse_failures += 1;
        return [4.0, 0.0, 0.0, 0.0];
    };
    if previous_field <= 0.0 {
        return [1.0, entry as f32, 0.0, 0.0];
    }
    for sample in 1..=FIELD_SAMPLES {
        let distance = entry + (exit - entry) * sample as f64 / FIELD_SAMPLES as f64;
        let Some(field) = field_on_ray(ray_x, distance, angle, grid, resolution, stats) else {
            stats.inverse_failures += 1;
            return [4.0, 0.0, 0.0, 0.0];
        };
        if previous_field > 0.0 && field <= 0.0 {
            let mut low = previous_distance;
            let mut high = distance;
            for _ in 0..BISECTION_STEPS {
                let middle = (low + high) * 0.5;
                let Some(middle_field) =
                    field_on_ray(ray_x, middle, angle, grid, resolution, stats)
                else {
                    stats.inverse_failures += 1;
                    return [4.0, 0.0, 0.0, 0.0];
                };
                if middle_field > 0.0 {
                    low = middle;
                } else {
                    high = middle;
                }
            }
            return [1.0, ((low + high) * 0.5) as f32, 0.0, 0.0];
        }
        previous_distance = distance;
        previous_field = field;
    }
    [2.0, 0.0, 0.0, 0.0]
}

fn field_on_ray(
    ray_x: f64,
    distance: f64,
    angle: f64,
    grid: &[f32],
    resolution: usize,
    stats: &mut CpuVoxelStats,
) -> Option<f64> {
    let world = inverse_lbs::world_ray_point(ray_x, distance);
    let inverse = inverse_lbs::inverse_lbs(world, angle)?;
    stats.inverse_iterations += u64::from(inverse.iterations);
    stats.maximum_inverse_residual = stats.maximum_inverse_residual.max(inverse.residual);
    stats.minimum_jacobian_determinant =
        stats.minimum_jacobian_determinant.min(inverse.determinant);
    Some(trilinear_sample(grid, inverse.canonical, resolution))
}

fn percentile(sorted: &[f64], p: f64) -> Option<f64> {
    if sorted.is_empty() {
        return None;
    }
    let index = ((sorted.len() - 1) as f64 * p).round() as usize;
    sorted.get(index).copied()
}
