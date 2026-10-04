const RAYS_PER_AXIS: u32 = 32u;
const RAY_COUNT: u32 = RAYS_PER_AXIS * RAYS_PER_AXIS;
const MAX_TRACE_STEPS: u32 = 256u;
const BVH_STACK_CAPACITY: u32 = 32u;

struct BvhNode {
    lower: vec4<f32>,
    upper: vec4<f32>,
    first: u32,
    count: u32,
    right: u32,
    padding: u32,
}

struct DistanceQuery {
    distance: f32,
    nodes_visited: u32,
    boxes_evaluated: u32,
    stack_overflow: u32,
}

@group(0) @binding(0) var<storage, read> bvh_nodes: array<BvhNode>;
@group(0) @binding(1) var<storage, read> boxes: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read> root_truth: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read_write> output: array<vec4<f32>>;

fn box_sdf(point: vec3<f32>, center_half: vec4<f32>) -> f32 {
    let q = abs(point - center_half.xyz) - vec3<f32>(center_half.w);
    return length(max(q, vec3<f32>(0.0))) + min(max(q.x, max(q.y, q.z)), 0.0);
}

fn aabb_distance(point: vec3<f32>, lower: vec3<f32>, upper: vec3<f32>) -> f32 {
    let delta = max(max(lower - point, point - upper), vec3<f32>(0.0));
    return length(delta);
}

fn flat_menger_sdf(point: vec3<f32>) -> DistanceQuery {
    var result = DistanceQuery(1e30, 0u, 0u, 0u);
    for (var index = 0u; index < arrayLength(&boxes); index++) {
        result.distance = min(result.distance, box_sdf(point, boxes[index]));
        result.boxes_evaluated += 1u;
    }
    return result;
}

fn hierarchical_menger_sdf(point: vec3<f32>) -> DistanceQuery {
    var result = DistanceQuery(1e30, 0u, 0u, 0u);
    var stack: array<u32, 32>;
    var stack_size = 1u;
    stack[0] = 0u;

    loop {
        if (stack_size == 0u) {
            break;
        }
        stack_size -= 1u;
        let node = bvh_nodes[stack[stack_size]];
        result.nodes_visited += 1u;
        if (aabb_distance(point, node.lower.xyz, node.upper.xyz) >= result.distance) {
            continue;
        }

        if (node.count > 0u) {
            for (var offset = 0u; offset < node.count; offset++) {
                result.distance = min(result.distance, box_sdf(point, boxes[node.first + offset]));
                result.boxes_evaluated += 1u;
            }
            continue;
        }

        let left = node.first;
        let right = node.right;
        let left_bound = aabb_distance(point, bvh_nodes[left].lower.xyz, bvh_nodes[left].upper.xyz);
        let right_bound = aabb_distance(point, bvh_nodes[right].lower.xyz, bvh_nodes[right].upper.xyz);
        if (left_bound <= right_bound) {
            if (right_bound < result.distance) {
                if (stack_size < BVH_STACK_CAPACITY) {
                    stack[stack_size] = right;
                    stack_size += 1u;
                } else {
                    result.stack_overflow = 1u;
                }
            }
            if (left_bound < result.distance) {
                if (stack_size < BVH_STACK_CAPACITY) {
                    stack[stack_size] = left;
                    stack_size += 1u;
                } else {
                    result.stack_overflow = 1u;
                }
            }
        } else {
            if (left_bound < result.distance) {
                if (stack_size < BVH_STACK_CAPACITY) {
                    stack[stack_size] = left;
                    stack_size += 1u;
                } else {
                    result.stack_overflow = 1u;
                }
            }
            if (right_bound < result.distance) {
                if (stack_size < BVH_STACK_CAPACITY) {
                    stack[stack_size] = right;
                    stack_size += 1u;
                } else {
                    result.stack_overflow = 1u;
                }
            }
        }
    }
    return result;
}

fn trace_ray(ray: u32, hierarchical: bool, max_trace_steps: u32) {
    if (ray >= RAY_COUNT) {
        return;
    }
    let column = ray % RAYS_PER_AXIS;
    let row = ray / RAYS_PER_AXIS;
    let ray_x = (f32(column) + 0.5) / f32(RAYS_PER_AXIS) * 2.5 - 1.25;
    let ray_y = (f32(row) + 0.5) / f32(RAYS_PER_AXIS) * 2.5 - 1.25;
    let origin = vec3<f32>(ray_x, ray_y, -3.0);
    let truth = root_truth[ray];
    var distance = 0.0;
    var final_sdf = 0.0;
    var trace_steps = 0u;
    var status = 2u;
    var bound_violation = 0u;
    var nodes_visited = 0u;
    var boxes_evaluated = 0u;
    var stack_overflow = 0u;
    for (var iteration = 0u; iteration < max_trace_steps; iteration++) {
        var query: DistanceQuery;
        if (hierarchical) {
            query = hierarchical_menger_sdf(origin + vec3<f32>(0.0, 0.0, distance));
        } else {
            query = flat_menger_sdf(origin + vec3<f32>(0.0, 0.0, distance));
        }
        final_sdf = query.distance;
        nodes_visited += query.nodes_visited;
        boxes_evaluated += query.boxes_evaluated;
        stack_overflow += query.stack_overflow;
        trace_steps = iteration + 1u;
        if (query.stack_overflow != 0u) {
            status = 4u;
            break;
        }
        if (final_sdf <= 1e-4) {
            status = 1u;
            break;
        }
        let step_size = max(final_sdf, 1e-5);
        if (truth.x == 1.0 && step_size > truth.y - distance + 2e-4) {
            bound_violation = 1u;
        }
        distance += step_size;
        if (distance > 6.0) {
            status = 2u;
            break;
        }
        if (iteration + 1u == max_trace_steps) {
            status = 3u;
        }
    }
    let result_index = select(ray, RAY_COUNT * 2u + ray, hierarchical);
    let diagnostics_index = select(RAY_COUNT + ray, RAY_COUNT * 3u + ray, hierarchical);
    output[result_index] = vec4<f32>(f32(status), f32(trace_steps), distance, final_sdf);
    output[diagnostics_index] = vec4<f32>(
        f32(bound_violation),
        f32(nodes_visited),
        f32(boxes_evaluated),
        f32(stack_overflow),
    );
}

@compute @workgroup_size(8, 1, 1)
fn cs_flat(@builtin(global_invocation_id) id: vec3<u32>) {
    trace_ray(id.x, false, MAX_TRACE_STEPS);
}

@compute @workgroup_size(8, 1, 1)
fn cs_hierarchical(@builtin(global_invocation_id) id: vec3<u32>) {
    trace_ray(id.x, true, MAX_TRACE_STEPS);
}

@compute @workgroup_size(8, 1, 1)
fn cs_hierarchical_depth4(@builtin(global_invocation_id) id: vec3<u32>) {
    trace_ray(id.x, true, 1024u);
}
