//! Isolated U19 finite-fractal SDF and GPU sphere-tracing feasibility probe.
use crate::gpu_resources::GpuContext;
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::time::Instant;

const MENGER_DEPTH: usize = 2;
const CELLS_PER_AXIS: usize = 32;
const RAY_COUNT: usize = CELLS_PER_AXIS * CELLS_PER_AXIS;
const BOX_COUNT: usize = 400;
const GPU_WARMUP_REPEATS: usize = 2;
const GPU_MEASURED_REPEATS: usize = 10;
const GPU_REPEATS: usize = GPU_WARMUP_REPEATS + GPU_MEASURED_REPEATS;
const MAX_TRACE_STEPS: usize = 256;
const BVH_LEAF_BOX_COUNT: usize = 4;
const CPU_BVH_BUILD_WARMUP_REPEATS: usize = 2;
const CPU_BVH_BUILD_MEASURED_REPEATS: usize = 10;
const CPU_BVH_BUILD_REPEATS: usize = CPU_BVH_BUILD_WARMUP_REPEATS + CPU_BVH_BUILD_MEASURED_REPEATS;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuBvhNode {
    lower: [f32; 4],
    upper: [f32; 4],
    first: u32,
    count: u32,
    right: u32,
    padding: u32,
}

struct MengerBvh {
    nodes: Vec<GpuBvhNode>,
    boxes: Vec<[f32; 4]>,
}

struct MengerDistance {
    distance: f32,
    nodes_visited: usize,
    boxes_evaluated: usize,
}

impl MengerBvh {
    fn build(boxes: &[[f32; 4]]) -> Self {
        let mut bvh = Self {
            nodes: Vec::new(),
            boxes: Vec::with_capacity(boxes.len()),
        };
        bvh.build_node(boxes.to_vec());
        bvh
    }

    fn build_node(&mut self, mut boxes: Vec<[f32; 4]>) -> usize {
        let node_index = self.nodes.len();
        self.nodes
            .push(<GpuBvhNode as bytemuck::Zeroable>::zeroed());
        let (lower, upper) = box_bounds(&boxes);

        if boxes.len() <= BVH_LEAF_BOX_COUNT {
            let first = self.boxes.len();
            let count = boxes.len();
            self.boxes.append(&mut boxes);
            self.nodes[node_index] = GpuBvhNode {
                lower,
                upper,
                first: first as u32,
                count: count as u32,
                right: 0,
                padding: 0,
            };
            return node_index;
        }

        let axis = longest_centroid_axis(&boxes);
        boxes.sort_by(|a, b| a[axis].total_cmp(&b[axis]));
        let right_boxes = boxes.split_off(boxes.len() / 2);
        let left = self.build_node(boxes);
        let right = self.build_node(right_boxes);
        self.nodes[node_index] = GpuBvhNode {
            lower,
            upper,
            first: left as u32,
            count: 0,
            right: right as u32,
            padding: 0,
        };
        node_index
    }

    fn distance(&self, point: [f32; 3]) -> MengerDistance {
        let mut stack = vec![0usize];
        let mut distance = f32::INFINITY;
        let mut nodes_visited = 0;
        let mut boxes_evaluated = 0;

        while let Some(node_index) = stack.pop() {
            let node = self.nodes[node_index];
            nodes_visited += 1;
            if aabb_distance(point, node.lower, node.upper) >= distance {
                continue;
            }

            if node.count > 0 {
                let first = node.first as usize;
                let end = first + node.count as usize;
                for cube in &self.boxes[first..end] {
                    distance = distance.min(box_sdf(point, *cube));
                    boxes_evaluated += 1;
                }
                continue;
            }

            let left = node.first as usize;
            let right = node.right as usize;
            let left_bound = aabb_distance(point, self.nodes[left].lower, self.nodes[left].upper);
            let right_bound =
                aabb_distance(point, self.nodes[right].lower, self.nodes[right].upper);
            if left_bound <= right_bound {
                if right_bound < distance {
                    stack.push(right);
                }
                if left_bound < distance {
                    stack.push(left);
                }
            } else {
                if left_bound < distance {
                    stack.push(left);
                }
                if right_bound < distance {
                    stack.push(right);
                }
            }
        }

        MengerDistance {
            distance,
            nodes_visited,
            boxes_evaluated,
        }
    }
}

fn box_bounds(boxes: &[[f32; 4]]) -> ([f32; 4], [f32; 4]) {
    let mut lower = [f32::INFINITY; 4];
    let mut upper = [f32::NEG_INFINITY; 4];
    for cube in boxes {
        for axis in 0..3 {
            lower[axis] = lower[axis].min(cube[axis] - cube[3]);
            upper[axis] = upper[axis].max(cube[axis] + cube[3]);
        }
    }
    lower[3] = 0.0;
    upper[3] = 0.0;
    (lower, upper)
}

fn longest_centroid_axis(boxes: &[[f32; 4]]) -> usize {
    let mut lower = [f32::INFINITY; 3];
    let mut upper = [f32::NEG_INFINITY; 3];
    for cube in boxes {
        for axis in 0..3 {
            lower[axis] = lower[axis].min(cube[axis]);
            upper[axis] = upper[axis].max(cube[axis]);
        }
    }
    (0..3)
        .max_by(|a, b| (upper[*a] - lower[*a]).total_cmp(&(upper[*b] - lower[*b])))
        .unwrap()
}

fn aabb_distance(point: [f32; 3], lower: [f32; 4], upper: [f32; 4]) -> f32 {
    let delta = std::array::from_fn::<_, 3, _>(|axis| {
        (lower[axis] - point[axis])
            .max(point[axis] - upper[axis])
            .max(0.0)
    });
    delta
        .into_iter()
        .map(|value| value * value)
        .sum::<f32>()
        .sqrt()
}

fn box_sdf(point: [f32; 3], cube: [f32; 4]) -> f32 {
    let q = std::array::from_fn::<_, 3, _>(|axis| (point[axis] - cube[axis]).abs() - cube[3]);
    let outside = q
        .iter()
        .map(|value| value.max(0.0).powi(2))
        .sum::<f32>()
        .sqrt();
    outside + q[0].max(q[1]).max(q[2]).min(0.0)
}

fn cpu_bvh_grid_check(boxes: &[[f32; 4]], hierarchy: &MengerBvh) -> Result<Value> {
    const GRID_SIZE: usize = 9;
    let coordinate = |index: usize| -1.25 + index as f32 * 2.5 / (GRID_SIZE - 1) as f32;
    let mut maximum_distance_error = 0.0f32;
    let mut nodes_visited = 0usize;
    let mut boxes_evaluated = 0usize;
    let query_count = GRID_SIZE.pow(3);

    for z in 0..GRID_SIZE {
        for y in 0..GRID_SIZE {
            for x in 0..GRID_SIZE {
                let point = [coordinate(x), coordinate(y), coordinate(z)];
                let expected = boxes
                    .iter()
                    .map(|cube| box_sdf(point, *cube))
                    .fold(f32::INFINITY, f32::min);
                let actual = hierarchy.distance(point);
                let error = (actual.distance - expected).abs();
                maximum_distance_error = maximum_distance_error.max(error);
                nodes_visited += actual.nodes_visited;
                boxes_evaluated += actual.boxes_evaluated;
            }
        }
    }

    ensure!(
        maximum_distance_error <= 1e-6,
        "CPU Menger BVH differs from flat union by {maximum_distance_error}"
    );
    Ok(json!({
        "query_count": query_count,
        "maximum_distance_error": maximum_distance_error,
        "average_nodes_visited": nodes_visited as f64 / query_count as f64,
        "average_box_sdf_evaluations": boxes_evaluated as f64 / query_count as f64,
    }))
}

pub(super) fn run(ctx: &GpuContext) -> Result<Value> {
    let boxes = finite_menger_boxes();
    ensure!(
        boxes.len() == BOX_COUNT,
        "depth-{MENGER_DEPTH} Menger sponge produced {} boxes, expected {BOX_COUNT}",
        boxes.len()
    );
    let mut hierarchy = None;
    let mut cpu_bvh_build_samples_ms = Vec::with_capacity(CPU_BVH_BUILD_REPEATS);
    for _ in 0..CPU_BVH_BUILD_REPEATS {
        let start = Instant::now();
        let built = MengerBvh::build(&boxes);
        cpu_bvh_build_samples_ms.push(start.elapsed().as_secs_f64() * 1e3);
        ensure!(
            built.boxes.len() == boxes.len(),
            "Menger BVH retained {} of {} boxes",
            built.boxes.len(),
            boxes.len()
        );
        hierarchy = Some(built);
    }
    let hierarchy = hierarchy.ok_or_else(|| anyhow::anyhow!("no Menger BVH build sample"))?;
    let cpu_bvh_build_warmup_ms = cpu_bvh_build_samples_ms[..CPU_BVH_BUILD_WARMUP_REPEATS].to_vec();
    let cpu_bvh_build_samples_ms =
        cpu_bvh_build_samples_ms[CPU_BVH_BUILD_WARMUP_REPEATS..].to_vec();
    let mut cpu_bvh_build_samples_sorted_ms = cpu_bvh_build_samples_ms.clone();
    cpu_bvh_build_samples_sorted_ms.sort_by(f64::total_cmp);
    let cpu_bvh_check = cpu_bvh_grid_check(&boxes, &hierarchy)?;
    let references = cpu_ray_box_references(&boxes);
    let reference_bytes = bytemuck::cast_slice(&references);
    let nodes_bytes = bytemuck::cast_slice(&hierarchy.nodes);
    let boxes_bytes = bytemuck::cast_slice(&hierarchy.boxes);
    let nodes_buffer = ctx.buffer(
        "U19 Menger BVH nodes",
        nodes_bytes,
        wgpu::BufferUsages::STORAGE,
    );
    let boxes_buffer = ctx.buffer(
        "U19 depth-2 Menger boxes",
        boxes_bytes,
        wgpu::BufferUsages::STORAGE,
    );
    let reference_buffer = ctx.buffer(
        "U19 exact Menger ray-box roots",
        reference_bytes,
        wgpu::BufferUsages::STORAGE,
    );
    let output_size = (RAY_COUNT * 4 * 16) as u64;
    let output = ctx.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("U19 Menger SDF GPU results"),
        size: output_size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = ctx.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("U19 Menger SDF readback"),
        size: output_size,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let layout = ctx
        .device
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("U19 Menger SDF bindings"),
            entries: &[
                storage_binding(0, nodes_bytes.len() as u64, true),
                storage_binding(1, boxes_bytes.len() as u64, true),
                storage_binding(2, reference_bytes.len() as u64, true),
                storage_binding(3, output_size, false),
            ],
        });
    let bind_group = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("U19 Menger SDF bind group"),
        layout: &layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: nodes_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: boxes_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: reference_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: output.as_entire_binding(),
            },
        ],
    });
    let pipeline_layout = ctx
        .device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("U19 Menger SDF pipeline layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
    let shader_source = crate::shaders::menger_sdf_probe();
    let module = ctx
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("U19 finite Menger SDF shader"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });
    let flat_pipeline = ctx
        .device
        .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("U19 flat Menger SDF baseline"),
            layout: Some(&pipeline_layout),
            module: &module,
            entry_point: Some("cs_flat"),
            compilation_options: Default::default(),
            cache: None,
        });
    let hierarchy_pipeline = ctx
        .device
        .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("U19 hierarchical Menger SDF"),
            layout: Some(&pipeline_layout),
            module: &module,
            entry_point: Some("cs_hierarchical"),
            compilation_options: Default::default(),
            cache: None,
        });
    let query = if ctx
        .device
        .features()
        .contains(wgpu::Features::TIMESTAMP_QUERY)
    {
        Some(ctx.device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("U19 Menger SDF repeated timestamps"),
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
                label: Some("U19 flat Menger SDF baseline"),
                timestamp_writes: query.as_ref().map(|query_set| {
                    wgpu::ComputePassTimestampWrites {
                        query_set,
                        beginning_of_pass_write_index: Some((repeat * 4) as u32),
                        end_of_pass_write_index: Some((repeat * 4 + 1) as u32),
                    }
                }),
            });
            pass.set_pipeline(&flat_pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(RAY_COUNT.div_ceil(8) as u32, 1, 1);
        }
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("U19 hierarchical Menger SDF"),
                timestamp_writes: query.as_ref().map(|query_set| {
                    wgpu::ComputePassTimestampWrites {
                        query_set,
                        beginning_of_pass_write_index: Some((repeat * 4 + 2) as u32),
                        end_of_pass_write_index: Some((repeat * 4 + 3) as u32),
                    }
                }),
            });
            pass.set_pipeline(&hierarchy_pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(RAY_COUNT.div_ceil(8) as u32, 1, 1);
        }
    }
    let timestamp_readback = query.as_ref().map(|query_set| {
        let byte_size = (GPU_REPEATS * 4 * 8) as u64;
        let resolve = ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("U19 timestamp resolve"),
            size: byte_size,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback = ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("U19 timestamp readback"),
            size: byte_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.resolve_query_set(query_set, 0..(GPU_REPEATS * 4) as u32, &resolve, 0);
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
    let gpu_timestamps = if let Some(readback) = timestamp_readback {
        let bytes = ctx.map_buffer(&readback)?;
        Some(
            bytes
                .chunks_exact(8)
                .map(|value| u64::from_le_bytes(value.try_into().unwrap()))
                .collect::<Vec<_>>(),
        )
    } else {
        None
    };
    let period_ns = ctx.queue.get_timestamp_period() as f64;
    let (flat_samples_all_ms, hierarchy_samples_all_ms) = match gpu_timestamps {
        Some(timestamps) => {
            let samples = |offset: usize| {
                (0..GPU_REPEATS)
                    .map(|repeat| {
                        let first = repeat * 4 + offset;
                        timestamps[first + 1].saturating_sub(timestamps[first]) as f64 * period_ns
                            / 1e6
                    })
                    .collect::<Vec<_>>()
            };
            (Some(samples(0)), Some(samples(2)))
        }
        None => (None, None),
    };
    let flat_warmup_ms = flat_samples_all_ms
        .as_ref()
        .map(|samples| samples[..GPU_WARMUP_REPEATS].to_vec());
    let flat_samples_ms = flat_samples_all_ms
        .as_ref()
        .map(|samples| samples[GPU_WARMUP_REPEATS..].to_vec());
    let hierarchy_warmup_ms = hierarchy_samples_all_ms
        .as_ref()
        .map(|samples| samples[..GPU_WARMUP_REPEATS].to_vec());
    let hierarchy_samples_ms = hierarchy_samples_all_ms
        .as_ref()
        .map(|samples| samples[GPU_WARMUP_REPEATS..].to_vec());

    let mut flat_status_disagreements = 0usize;
    let mut hierarchy_status_disagreements = 0usize;
    let mut flat_step_violations = 0usize;
    let mut hierarchy_step_violations = 0usize;
    let mut hierarchy_stack_overflows = 0usize;
    let mut hit_count = 0usize;
    let mut flat_hit_errors = Vec::new();
    let mut hierarchy_hit_errors = Vec::new();
    let mut flat_steps = Vec::with_capacity(RAY_COUNT);
    let mut hierarchy_steps = Vec::with_capacity(RAY_COUNT);
    let mut flat_box_evaluations = 0u64;
    let mut hierarchy_box_evaluations = 0u64;
    let mut hierarchy_nodes_visited = 0u64;
    for ray in 0..RAY_COUNT {
        let flat_actual = values[ray];
        let flat_diagnostics = values[RAY_COUNT + ray];
        let hierarchy_actual = values[RAY_COUNT * 2 + ray];
        let hierarchy_diagnostics = values[RAY_COUNT * 3 + ray];
        let expected = references[ray];
        let expected_status = expected[0] as u32;
        if flat_actual[0] as u32 != expected_status {
            flat_status_disagreements += 1;
        }
        if hierarchy_actual[0] as u32 != expected_status {
            hierarchy_status_disagreements += 1;
        }
        if flat_diagnostics[0] > 0.5 {
            flat_step_violations += 1;
        }
        if hierarchy_diagnostics[0] > 0.5 {
            hierarchy_step_violations += 1;
        }
        if hierarchy_diagnostics[3] > 0.5 {
            hierarchy_stack_overflows += 1;
        }
        flat_box_evaluations += flat_diagnostics[2] as u64;
        hierarchy_nodes_visited += hierarchy_diagnostics[1] as u64;
        hierarchy_box_evaluations += hierarchy_diagnostics[2] as u64;
        flat_steps.push(flat_actual[1].max(0.0) as u32);
        hierarchy_steps.push(hierarchy_actual[1].max(0.0) as u32);
        if expected_status == 1 {
            hit_count += 1;
            flat_hit_errors.push((f64::from(flat_actual[2]) - f64::from(expected[1])).abs());
            hierarchy_hit_errors
                .push((f64::from(hierarchy_actual[2]) - f64::from(expected[1])).abs());
        }
    }
    flat_steps.sort_unstable();
    hierarchy_steps.sort_unstable();
    flat_hit_errors.sort_by(f64::total_cmp);
    hierarchy_hit_errors.sort_by(f64::total_cmp);
    let mut flat_samples_sorted = flat_samples_ms.clone().unwrap_or_default();
    flat_samples_sorted.sort_by(f64::total_cmp);
    let mut hierarchy_samples_sorted = hierarchy_samples_ms.clone().unwrap_or_default();
    hierarchy_samples_sorted.sort_by(f64::total_cmp);
    let flat_trace_steps = flat_steps
        .iter()
        .map(|steps| u64::from(*steps))
        .sum::<u64>();
    let hierarchy_trace_steps = hierarchy_steps
        .iter()
        .map(|steps| u64::from(*steps))
        .sum::<u64>();
    ensure!(
        flat_status_disagreements == 0,
        "flat Menger SDF status disagreement"
    );
    ensure!(
        hierarchy_status_disagreements == 0,
        "hierarchical Menger SDF status disagreement"
    );
    ensure!(
        flat_step_violations == 0,
        "flat Menger SDF conservative-step violation"
    );
    ensure!(
        hierarchy_step_violations == 0,
        "hierarchical Menger SDF conservative-step violation"
    );
    ensure!(
        hierarchy_stack_overflows == 0,
        "Menger BVH query stack overflow"
    );
    ctx.check()?;
    let depth3_followup = run_finite_menger_depth_gpu(ctx, 3)?;
    let depth4_followup = run_finite_menger_depth_gpu(ctx, 4)?;

    let mut report = json!({
        "representation": "finite depth-2 Menger sponge queried through a median-split BVH over exact box SDFs",
        "depth": MENGER_DEPTH,
        "box_count": boxes.len(),
        "box_storage_bytes": hierarchy.boxes.len() * std::mem::size_of::<[f32; 4]>(),
        "bvh_node_count": hierarchy.nodes.len(),
        "bvh_storage_bytes": hierarchy.nodes.len() * std::mem::size_of::<GpuBvhNode>(),
        "bvh_leaf_box_count": BVH_LEAF_BOX_COUNT,
        "cpu_bvh_build_warmup_samples_ms": cpu_bvh_build_warmup_ms,
        "cpu_bvh_build_samples_ms": cpu_bvh_build_samples_ms,
        "cpu_bvh_build_median_ms": percentile(&cpu_bvh_build_samples_sorted_ms, 0.5),
        "cpu_bvh_build_p95_ms": percentile(&cpu_bvh_build_samples_sorted_ms, 0.95),
        "cpu_bvh_distance_check": cpu_bvh_check,
        "ray_count": RAY_COUNT,
        "max_trace_steps": MAX_TRACE_STEPS,
        "rays_grid": [CELLS_PER_AXIS, CELLS_PER_AXIS],
        "reference": "exact front-facing ray entry into any retained box",
        "distance_bound": "node AABB distance is a lower bound; exact box SDFs at leaves define the finite-union field",
        "hit_rays": hit_count,
        "flat_status_disagreements": flat_status_disagreements,
        "hierarchical_status_disagreements": hierarchy_status_disagreements,
        "flat_conservative_step_violations": flat_step_violations,
        "hierarchical_conservative_step_violations": hierarchy_step_violations,
        "hierarchical_stack_overflows": hierarchy_stack_overflows,
        "flat_maximum_hit_distance_error": flat_hit_errors.last().copied().unwrap_or(0.0),
        "hierarchical_maximum_hit_distance_error": hierarchy_hit_errors.last().copied().unwrap_or(0.0),
        "hierarchical_median_hit_distance_error": percentile(&hierarchy_hit_errors, 0.5),
        "trace_steps_p50": hierarchy_steps[hierarchy_steps.len() / 2],
        "trace_steps_p95": hierarchy_steps[(hierarchy_steps.len() * 95 / 100).min(hierarchy_steps.len() - 1)],
        "trace_steps_max": hierarchy_steps[hierarchy_steps.len() - 1],
        "flat_average_box_sdf_evaluations_per_distance_query": flat_box_evaluations as f64 / flat_trace_steps as f64,
        "hierarchical_average_box_sdf_evaluations_per_distance_query": hierarchy_box_evaluations as f64 / hierarchy_trace_steps as f64,
        "hierarchical_average_nodes_visited_per_distance_query": hierarchy_nodes_visited as f64 / hierarchy_trace_steps as f64,
        "flat_gpu_warmup_samples_ms": flat_warmup_ms,
        "flat_gpu_compute_samples_ms": flat_samples_ms,
        "flat_gpu_compute_median_ms": percentile(&flat_samples_sorted, 0.5),
        "flat_gpu_compute_p95_ms": percentile(&flat_samples_sorted, 0.95),
        "hierarchical_gpu_warmup_samples_ms": hierarchy_warmup_ms,
        "hierarchical_gpu_compute_samples_ms": hierarchy_samples_ms,
        "gpu_compute_median_ms": percentile(&hierarchy_samples_sorted, 0.5),
        "gpu_compute_p95_ms": percentile(&hierarchy_samples_sorted, 0.95)
    });
    report["depth3_followup"] = depth3_followup;
    report["depth4_followup"] = depth4_followup;
    Ok(report)
}

fn run_finite_menger_depth_gpu(ctx: &GpuContext, depth: usize) -> Result<Value> {
    let expected_box_count = 20usize.pow(depth as u32);
    let max_trace_steps = if depth >= 4 {
        1024usize
    } else {
        MAX_TRACE_STEPS
    };
    let boxes = finite_menger_boxes_at_depth(depth);
    ensure!(
        boxes.len() == expected_box_count,
        "depth-{depth} Menger sponge produced {} boxes, expected {expected_box_count}",
        boxes.len()
    );
    let build_start = Instant::now();
    let hierarchy = MengerBvh::build(&boxes);
    let cpu_bvh_build_ms = build_start.elapsed().as_secs_f64() * 1e3;
    let cpu_bvh_check = cpu_bvh_grid_check(&boxes, &hierarchy)?;
    let references = cpu_ray_box_references(&boxes);
    let nodes_bytes = bytemuck::cast_slice(&hierarchy.nodes);
    let boxes_bytes = bytemuck::cast_slice(&hierarchy.boxes);
    let reference_bytes = bytemuck::cast_slice(&references);
    let output_size = (RAY_COUNT * 4 * 16) as u64;

    let nodes_buffer = ctx.buffer(
        "U19 finite Menger BVH nodes",
        nodes_bytes,
        wgpu::BufferUsages::STORAGE,
    );
    let boxes_buffer = ctx.buffer(
        "U19 finite Menger boxes",
        boxes_bytes,
        wgpu::BufferUsages::STORAGE,
    );
    let reference_buffer = ctx.buffer(
        "U19 finite Menger exact ray-box roots",
        reference_bytes,
        wgpu::BufferUsages::STORAGE,
    );
    let output = ctx.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("U19 finite Menger GPU results"),
        size: output_size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = ctx.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("U19 finite Menger readback"),
        size: output_size,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let layout = ctx
        .device
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("U19 finite Menger bindings"),
            entries: &[
                storage_binding(0, nodes_bytes.len() as u64, true),
                storage_binding(1, boxes_bytes.len() as u64, true),
                storage_binding(2, reference_bytes.len() as u64, true),
                storage_binding(3, output_size, false),
            ],
        });
    let bind_group = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("U19 finite Menger bind group"),
        layout: &layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: nodes_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: boxes_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: reference_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: output.as_entire_binding(),
            },
        ],
    });
    let pipeline_layout = ctx
        .device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("U19 finite Menger pipeline layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
    let shader_source = crate::shaders::menger_sdf_probe();
    let module = ctx
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("U19 finite Menger shader"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });
    let entry_point = if depth >= 4 {
        "cs_hierarchical_depth4"
    } else {
        "cs_hierarchical"
    };
    let pipeline = ctx
        .device
        .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("U19 hierarchical finite Menger SDF"),
            layout: Some(&pipeline_layout),
            module: &module,
            entry_point: Some(entry_point),
            compilation_options: Default::default(),
            cache: None,
        });
    let mut encoder = ctx.device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("U19 hierarchical finite Menger SDF"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(RAY_COUNT.div_ceil(8) as u32, 1, 1);
    }
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

    let mut status_disagreements = 0usize;
    let mut budget_exits = 0usize;
    let mut budget_exit_expected_misses = 0usize;
    let mut budget_exit_expected_hits = 0usize;
    let mut unexpected_status_disagreements = 0usize;
    let mut status_mismatch_samples = Vec::new();
    let mut conservative_step_violations = 0usize;
    let mut stack_overflows = 0usize;
    let mut hit_rays = 0usize;
    let mut hit_distance_errors = Vec::new();
    let mut trace_steps = Vec::with_capacity(RAY_COUNT);
    let mut boxes_evaluated = 0u64;
    let mut nodes_visited = 0u64;
    let mut total_steps = 0u64;
    for ray in 0..RAY_COUNT {
        let actual = values[RAY_COUNT * 2 + ray];
        let diagnostics = values[RAY_COUNT * 3 + ray];
        let expected = references[ray];
        if actual[0] as u32 != expected[0] as u32 {
            status_disagreements += 1;
            let explicit_budget_miss = actual[0] == 3.0 && expected[0] == 2.0;
            if actual[0] == 3.0 {
                budget_exits += 1;
                if expected[0] == 1.0 {
                    budget_exit_expected_hits += 1;
                } else if expected[0] == 2.0 {
                    budget_exit_expected_misses += 1;
                }
            }
            if !explicit_budget_miss || depth < 4 {
                unexpected_status_disagreements += 1;
            }
            if status_mismatch_samples.len() < 8 {
                let (x, y) = ray_xy(ray);
                status_mismatch_samples.push(json!({
                    "ray": ray,
                    "ray_xy": [x, y],
                    "expected_status": expected[0],
                    "expected_entry_distance": expected[1],
                    "actual_status": actual[0],
                    "actual_distance": actual[2],
                    "actual_final_sdf": actual[3],
                    "trace_steps": actual[1],
                    "diagnostics": diagnostics
                }));
            }
        }
        if diagnostics[0] > 0.5 {
            conservative_step_violations += 1;
        }
        if diagnostics[3] > 0.5 {
            stack_overflows += 1;
        }
        nodes_visited += diagnostics[1].max(0.0) as u64;
        boxes_evaluated += diagnostics[2].max(0.0) as u64;
        let steps = actual[1].max(0.0) as u32;
        trace_steps.push(steps);
        total_steps += u64::from(steps);
        if expected[0] == 1.0 {
            hit_rays += 1;
            if actual[0] == 1.0 {
                hit_distance_errors.push((f64::from(actual[2]) - f64::from(expected[1])).abs());
            }
        }
    }
    trace_steps.sort_unstable();
    hit_distance_errors.sort_by(f64::total_cmp);
    ensure!(
        unexpected_status_disagreements == 0,
        "depth-{depth} Menger SDF had {unexpected_status_disagreements} non-budget status disagreements; examples: {status_mismatch_samples:?}"
    );
    ensure!(
        conservative_step_violations == 0,
        "depth-{depth} Menger SDF conservative-step violation"
    );
    ensure!(
        stack_overflows == 0,
        "depth-{depth} Menger BVH query stack overflow"
    );
    ctx.check()?;

    Ok(json!({
        "depth": depth,
        "box_count": boxes.len(),
        "bvh_node_count": hierarchy.nodes.len(),
        "bvh_node_storage_bytes": hierarchy.nodes.len() * std::mem::size_of::<GpuBvhNode>(),
        "box_storage_bytes": hierarchy.boxes.len() * std::mem::size_of::<[f32; 4]>(),
        "cpu_bvh_build_ms": cpu_bvh_build_ms,
        "cpu_bvh_distance_check": cpu_bvh_check,
        "ray_count": RAY_COUNT,
        "max_trace_steps": max_trace_steps,
        "hit_rays": hit_rays,
        "status_disagreements": status_disagreements,
        "budget_exits": budget_exits,
        "budget_exit_expected_misses": budget_exit_expected_misses,
        "budget_exit_expected_hits": budget_exit_expected_hits,
        "unexpected_status_disagreements": unexpected_status_disagreements,
        "status_mismatch_samples": status_mismatch_samples,
        "conservative_step_violations": conservative_step_violations,
        "stack_overflows": stack_overflows,
        "maximum_hit_distance_error": hit_distance_errors.last().copied().unwrap_or(0.0),
        "average_box_sdf_evaluations_per_distance_query": boxes_evaluated as f64 / total_steps as f64,
        "average_nodes_visited_per_distance_query": nodes_visited as f64 / total_steps as f64,
        "trace_steps_p50": trace_steps[trace_steps.len() / 2],
        "trace_steps_p95": trace_steps[(trace_steps.len() * 95 / 100).min(trace_steps.len() - 1)],
        "trace_steps_max": trace_steps[trace_steps.len() - 1]
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

fn retained_children() -> Vec<[i32; 3]> {
    let mut children = Vec::with_capacity(20);
    for x in 0..3 {
        for y in 0..3 {
            for z in 0..3 {
                let center_coordinates = [x == 1, y == 1, z == 1]
                    .into_iter()
                    .filter(|center| *center)
                    .count();
                if center_coordinates <= 1 {
                    children.push([x - 1, y - 1, z - 1]);
                }
            }
        }
    }
    children
}

fn finite_menger_boxes() -> Vec<[f32; 4]> {
    finite_menger_boxes_at_depth(MENGER_DEPTH)
}

fn finite_menger_boxes_at_depth(depth: usize) -> Vec<[f32; 4]> {
    let children = retained_children();
    let mut boxes = vec![[0.0f64, 0.0, 0.0, 1.0]];
    for _ in 0..depth {
        let mut next = Vec::with_capacity(boxes.len() * children.len());
        for parent in boxes {
            let half_size = parent[3] / 3.0;
            for child in &children {
                next.push([
                    parent[0] + f64::from(child[0]) * 2.0 * half_size,
                    parent[1] + f64::from(child[1]) * 2.0 * half_size,
                    parent[2] + f64::from(child[2]) * 2.0 * half_size,
                    half_size,
                ]);
            }
        }
        boxes = next;
    }
    boxes
        .into_iter()
        .map(|cube| {
            [
                cube[0] as f32,
                cube[1] as f32,
                cube[2] as f32,
                cube[3] as f32,
            ]
        })
        .collect()
}

fn ray_xy(ray: usize) -> (f64, f64) {
    let x = ray % CELLS_PER_AXIS;
    let y = ray / CELLS_PER_AXIS;
    (
        (x as f64 + 0.5) / CELLS_PER_AXIS as f64 * 2.5 - 1.25,
        (y as f64 + 0.5) / CELLS_PER_AXIS as f64 * 2.5 - 1.25,
    )
}

fn cpu_ray_box_references(boxes: &[[f32; 4]]) -> Vec<[f32; 4]> {
    (0..RAY_COUNT)
        .map(|ray| {
            let (x, y) = ray_xy(ray);
            let entry = boxes
                .iter()
                .filter_map(|cube| {
                    let center_x = f64::from(cube[0]);
                    let center_y = f64::from(cube[1]);
                    let center_z = f64::from(cube[2]);
                    let half = f64::from(cube[3]);
                    (x >= center_x - half
                        && x <= center_x + half
                        && y >= center_y - half
                        && y <= center_y + half)
                        .then_some(center_z - half + 3.0)
                })
                .min_by(f64::total_cmp);
            match entry {
                Some(distance) if distance >= 0.0 => [1.0, distance as f32, 0.0, 0.0],
                _ => [2.0, 0.0, 0.0, 0.0],
            }
        })
        .collect()
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
    fn hierarchical_menger_distance_matches_flat_union_and_prunes_boxes() {
        let boxes = finite_menger_boxes();
        let hierarchy = MengerBvh::build(&boxes);
        let samples = [
            [boxes[0][0], boxes[0][1], boxes[0][2]],
            [0.0, 0.0, 0.0],
            [-0.8, 0.1, 0.9],
            [0.25, -1.1, 0.33],
            [1.24, 1.24, 1.24],
        ];

        for point in samples {
            let expected = boxes
                .iter()
                .map(|cube| box_sdf(point, *cube))
                .fold(f32::INFINITY, f32::min);
            let actual = hierarchy.distance(point);
            assert!(
                (actual.distance - expected).abs() <= 1e-6,
                "point {point:?}: got {}, expected {expected}",
                actual.distance
            );
        }

        let inside_first_box = hierarchy.distance([boxes[0][0], boxes[0][1], boxes[0][2]]);
        assert!(inside_first_box.boxes_evaluated < boxes.len());
        assert!(inside_first_box.nodes_visited < hierarchy.nodes.len());
    }
}
