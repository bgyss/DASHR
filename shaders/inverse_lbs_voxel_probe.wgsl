const SEQUENCE_POSE_COUNT: u32 = 120u;
const RAYS_PER_POSE: u32 = 32u;
const TOTAL_RAYS: u32 = SEQUENCE_POSE_COUNT * RAYS_PER_POSE;
const MAX_INVERSE_ITERATIONS: u32 = 16u;
const MAX_TRACE_STEPS: u32 = 256u;

@group(0) @binding(0) var<uniform> params: vec4<f32>;
@group(0) @binding(1) var<uniform> grid_info: vec4<u32>;
@group(0) @binding(2) var<storage, read> root_truth: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read> voxel_grid: array<f32>;
@group(0) @binding(4) var<storage, read_write> output: array<vec4<f32>>;

struct InverseSolution {
    canonical: vec3<f32>,
    iterations: u32,
    residual: f32,
    minimum_determinant: f32,
    valid: u32,
}

fn blend_weight(x: f32) -> vec2<f32> {
    let t = clamp((x + 1.0) * 0.5, 0.0, 1.0);
    let weight = t * t * (3.0 - 2.0 * t);
    let derivative = select(0.0, 3.0 * t * (1.0 - t), t > 0.0 && t < 1.0);
    return vec2<f32>(weight, derivative);
}

fn rotate_y(point: vec3<f32>, angle: f32) -> vec3<f32> {
    let sine = sin(angle);
    let cosine = cos(angle);
    return vec3<f32>(
        cosine * point.x + sine * point.z,
        point.y,
        -sine * point.x + cosine * point.z,
    );
}

fn forward_lbs(point: vec3<f32>, angle: f32) -> vec3<f32> {
    return mix(point, rotate_y(point, angle), blend_weight(point.x).x);
}

fn inverse_lbs(world: vec3<f32>, angle: f32) -> InverseSolution {
    var canonical = world;
    var minimum_determinant = 1e30;
    var residual_length = 1e30;
    for (var iteration = 0u; iteration < MAX_INVERSE_ITERATIONS; iteration++) {
        let weight_data = blend_weight(canonical.x);
        let weight = weight_data.x;
        let weight_derivative = weight_data.y;
        let sine = sin(angle);
        let cosine = cos(angle);
        let rotated_delta = rotate_y(canonical, angle) - canonical;
        let column_x = vec3<f32>(
            1.0 + weight * (cosine - 1.0),
            0.0,
            -weight * sine,
        ) + rotated_delta * weight_derivative;
        let column_y = vec3<f32>(0.0, 1.0, 0.0);
        let column_z = vec3<f32>(weight * sine, 0.0, 1.0 + weight * (cosine - 1.0));
        let determinant = dot(column_x, cross(column_y, column_z));
        if (determinant <= 1e-8 || determinant > 1e10 || determinant != determinant) {
            return InverseSolution(canonical, iteration + 1u, residual_length, determinant, 0u);
        }
        minimum_determinant = min(minimum_determinant, determinant);
        let residual = forward_lbs(canonical, angle) - world;
        residual_length = length(residual);
        if (residual_length <= 1e-5) {
            let valid = select(0u, 1u, length(canonical) <= params.z);
            return InverseSolution(canonical, iteration + 1u, residual_length, minimum_determinant, valid);
        }
        let delta = vec3<f32>(
            dot(residual, cross(column_y, column_z)),
            dot(residual, cross(column_z, column_x)),
            dot(residual, cross(column_x, column_y)),
        ) / determinant;
        let delta_length = length(delta);
        if (delta_length > 1e4 || delta_length != delta_length) {
            return InverseSolution(canonical, iteration + 1u, residual_length, minimum_determinant, 0u);
        }
        canonical -= delta * min(1.0, 0.5 / max(delta_length, 1e-30));
    }
    residual_length = length(forward_lbs(canonical, angle) - world);
    let weight_data = blend_weight(canonical.x);
    let rotated_delta = rotate_y(canonical, angle) - canonical;
    let column_x = vec3<f32>(
        1.0 + weight_data.x * (cos(angle) - 1.0),
        0.0,
        -weight_data.x * sin(angle),
    ) + rotated_delta * weight_data.y;
    let column_y = vec3<f32>(0.0, 1.0, 0.0);
    let column_z = vec3<f32>(weight_data.x * sin(angle), 0.0, 1.0 + weight_data.x * (cos(angle) - 1.0));
    let final_determinant = dot(column_x, cross(column_y, column_z));
    minimum_determinant = min(minimum_determinant, final_determinant);
    let valid = select(0u, 1u, residual_length <= 1e-5 && length(canonical) <= params.z && final_determinant > 0.0);
    return InverseSolution(canonical, MAX_INVERSE_ITERATIONS, residual_length, minimum_determinant, valid);
}

fn voxel_load(x: i32, y: i32, z: i32) -> f32 {
    let resolution = i32(grid_info.x);
    let index = u32((z * resolution + y) * resolution + x);
    return voxel_grid[index];
}

fn voxel_sample(point: vec3<f32>) -> f32 {
    let resolution = f32(grid_info.x);
    let spacing = 3.0 / (resolution - 1.0);
    let grid_position = clamp((point + vec3<f32>(1.5)) / spacing,
                              vec3<f32>(0.0), vec3<f32>(resolution - 1.0));
    let maximum_base = i32(grid_info.x) - 2;
    let base = min(vec3<i32>(floor(grid_position)), vec3<i32>(maximum_base));
    let fraction = grid_position - vec3<f32>(base);
    let x00 = mix(voxel_load(base.x, base.y, base.z), voxel_load(base.x + 1, base.y, base.z), fraction.x);
    let x10 = mix(voxel_load(base.x, base.y + 1, base.z), voxel_load(base.x + 1, base.y + 1, base.z), fraction.x);
    let x01 = mix(voxel_load(base.x, base.y, base.z + 1), voxel_load(base.x + 1, base.y, base.z + 1), fraction.x);
    let x11 = mix(voxel_load(base.x, base.y + 1, base.z + 1), voxel_load(base.x + 1, base.y + 1, base.z + 1), fraction.x);
    return mix(mix(x00, x10, fraction.y), mix(x01, x11, fraction.y), fraction.z);
}

fn store_result(index: u32, status: u32, steps: u32, distance: f32, field: f32,
                inverse_iterations: u32, maximum_residual: f32, minimum_determinant: f32,
                bound_violation: u32) {
    output[index * 2u] = vec4<f32>(f32(status), f32(steps), distance, field);
    output[index * 2u + 1u] = vec4<f32>(
        f32(inverse_iterations), maximum_residual, minimum_determinant, f32(bound_violation),
    );
}

@compute @workgroup_size(8, 1, 1)
fn cs_main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= TOTAL_RAYS) {
        return;
    }
    let pose = id.x / RAYS_PER_POSE;
    let ray = id.x % RAYS_PER_POSE;
    let ray_x = ((f32(ray) + 0.5) / f32(RAYS_PER_POSE) * 2.0 - 1.0) * 0.9;
    let angle = params.x * sin(6.283185307179586 * f32(pose) / f32(SEQUENCE_POSE_COUNT - 1u));
    let lower_bound = 1.0 - 2.0 * abs(sin(angle * 0.5)) * (1.0 + 0.75 * params.z);
    if (lower_bound <= 0.0 || abs(ray_x) >= params.y) {
        store_result(id.x, 4u, 0u, 0.0, 0.0, 0u, 0.0, 0.0, 0u);
        return;
    }
    let half_chord = sqrt(max(params.y * params.y - ray_x * ray_x, 0.0));
    let entry = 2.0 - half_chord;
    let exit = 2.0 + half_chord;
    let truth = root_truth[id.x];
    var distance = entry;
    var final_field = 1e30;
    var status = 2u;
    var trace_steps = 0u;
    var total_inverse_iterations = 0u;
    var maximum_residual = 0.0;
    var minimum_determinant = 1e30;
    var bound_violation = 0u;
    for (var iteration = 0u; iteration < MAX_TRACE_STEPS; iteration++) {
        let world = vec3<f32>(ray_x, 0.0, -2.0 + distance);
        let inverse = inverse_lbs(world, angle);
        total_inverse_iterations += inverse.iterations;
        maximum_residual = max(maximum_residual, inverse.residual);
        minimum_determinant = min(minimum_determinant, inverse.minimum_determinant);
        trace_steps = iteration + 1u;
        if (inverse.valid == 0u) {
            status = 4u;
            break;
        }
        final_field = voxel_sample(inverse.canonical);
        if (final_field <= 1e-4) {
            status = 1u;
            break;
        }
        let step_size = max(final_field / params.w * lower_bound, 1e-5);
        if (truth.x == 1.0 && step_size > truth.y - distance + 2e-4) {
            bound_violation = 1u;
        }
        distance += step_size;
        if (distance > exit) {
            status = 2u;
            break;
        }
        if (iteration + 1u == MAX_TRACE_STEPS) {
            status = 3u;
        }
    }
    store_result(
        id.x, status, trace_steps, distance, final_field, total_inverse_iterations,
        maximum_residual, minimum_determinant, bound_violation,
    );
}
