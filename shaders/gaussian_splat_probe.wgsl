const SPLAT_COUNT: u32 = 256u;
const POSE_COUNT: u32 = 2u;
const IMAGE_SIZE: u32 = 32u;
const PIXELS_PER_POSE: u32 = IMAGE_SIZE * IMAGE_SIZE;
const TOTAL_PIXELS: u32 = POSE_COUNT * PIXELS_PER_POSE;
const SUPPORT_RADIUS_SQUARED: f32 = 9.0;

struct SplatInput {
    center_weight: vec4<f32>,
    color_opacity: vec4<f32>,
    covariance_0: vec4<f32>,
    covariance_1: vec4<f32>,
    covariance_2: vec4<f32>,
}

@group(0) @binding(0) var<uniform> params: vec4<f32>;
@group(0) @binding(1) var<storage, read> splats: array<SplatInput>;
@group(0) @binding(2) var<storage, read_write> output: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read_write> sort_order: array<u32>;

var<workgroup> sort_depths: array<f32, 256>;
var<workgroup> sort_indices: array<u32, 256>;

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

fn skin_jacobian(point: vec3<f32>, angle: f32) -> mat3x3<f32> {
    let weight_data = blend_weight(point.x);
    let weight = weight_data.x;
    let delta = rotate_y(point, angle) - point;
    let sine = sin(angle);
    let cosine = cos(angle);
    let column_x = vec3<f32>(
        1.0 + weight * (cosine - 1.0),
        0.0,
        -weight * sine,
    ) + delta * weight_data.y;
    let column_y = vec3<f32>(0.0, 1.0, 0.0);
    let column_z = vec3<f32>(weight * sine, 0.0, 1.0 + weight * (cosine - 1.0));
    return mat3x3<f32>(column_x, column_y, column_z);
}

@compute @workgroup_size(256, 1, 1)
fn cs_sort(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) lane: u32,
) {
    if (workgroup.x >= POSE_COUNT) {
        return;
    }
    let angle = select(0.0, params.x, workgroup.x == 1u);
    let splat = splats[lane];
    let canonical_center = splat.center_weight.xyz;
    let posed_center = mix(
        canonical_center,
        rotate_y(canonical_center, angle),
        splat.center_weight.w,
    );
    sort_depths[lane] = posed_center.z;
    sort_indices[lane] = lane;
    workgroupBarrier();

    var stage = 2u;
    loop {
        if (stage > SPLAT_COUNT) {
            break;
        }
        var distance = stage >> 1u;
        loop {
            if (distance == 0u) {
                break;
            }
            let partner = lane ^ distance;
            if (partner > lane) {
                let left_depth = sort_depths[lane];
                let right_depth = sort_depths[partner];
                let left_index = sort_indices[lane];
                let right_index = sort_indices[partner];
                let left_is_after = left_depth > right_depth
                    || (left_depth == right_depth && left_index > right_index);
                let left_is_before = left_depth < right_depth
                    || (left_depth == right_depth && left_index < right_index);
                let ascending = (lane & stage) == 0u;
                if ((ascending && left_is_after) || (!ascending && left_is_before)) {
                    sort_depths[lane] = right_depth;
                    sort_depths[partner] = left_depth;
                    sort_indices[lane] = right_index;
                    sort_indices[partner] = left_index;
                }
            }
            workgroupBarrier();
            if (distance == 1u) {
                break;
            }
            distance >>= 1u;
        }
        if (stage == SPLAT_COUNT) {
            break;
        }
        stage <<= 1u;
    }
    sort_order[workgroup.x * SPLAT_COUNT + lane] = sort_indices[lane];
}

@compute @workgroup_size(8, 1, 1)
fn cs_main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= TOTAL_PIXELS) {
        return;
    }
    let pose = id.x / PIXELS_PER_POSE;
    let pixel = id.x % PIXELS_PER_POSE;
    let column = pixel % IMAGE_SIZE;
    let row = pixel / IMAGE_SIZE;
    let pixel_x = ((f32(column) + 0.5) / f32(IMAGE_SIZE) * 2.0 - 1.0) * params.y;
    let pixel_y = ((f32(row) + 0.5) / f32(IMAGE_SIZE) * 2.0 - 1.0) * params.y;
    let angle = select(0.0, params.x, pose == 1u);
    var transmittance = 1.0;
    var accumulated_color = vec3<f32>(0.0);
    var accumulated_depth = 0.0;
    var contributions = 0u;

    for (var index = 0u; index < SPLAT_COUNT; index++) {
        let splat_index = sort_order[pose * SPLAT_COUNT + index];
        let splat = splats[splat_index];
        let canonical_center = splat.center_weight.xyz;
        let weight = splat.center_weight.w;
        let posed_center = mix(canonical_center, rotate_y(canonical_center, angle), weight);
        let jacobian = skin_jacobian(canonical_center, angle);
        let covariance = mat3x3<f32>(
            splat.covariance_0.xyz,
            splat.covariance_1.xyz,
            splat.covariance_2.xyz,
        );
        let posed_covariance = jacobian * covariance * transpose(jacobian);
        let xx = posed_covariance[0][0] + params.z;
        let xy = posed_covariance[1][0];
        let yy = posed_covariance[1][1] + params.z;
        let determinant = xx * yy - xy * xy;
        if (determinant <= 1e-12) {
            continue;
        }
        let delta = vec2<f32>(pixel_x - posed_center.x, pixel_y - posed_center.y);
        let radius_squared = (yy * delta.x * delta.x - 2.0 * xy * delta.x * delta.y + xx * delta.y * delta.y) / determinant;
        if (radius_squared > SUPPORT_RADIUS_SQUARED) {
            continue;
        }
        let alpha = clamp(splat.color_opacity.w * exp(-0.5 * radius_squared), 0.0, 0.99);
        if (alpha <= 1e-6) {
            continue;
        }
        let contribution = transmittance * alpha;
        accumulated_color += contribution * splat.color_opacity.xyz;
        accumulated_depth += contribution * (2.0 + posed_center.z);
        transmittance *= 1.0 - alpha;
        contributions += 1u;
        if (transmittance < 0.001) {
            break;
        }
    }
    let coverage = 1.0 - transmittance;
    var mean_depth = 0.0;
    if (coverage > 1e-8) {
        mean_depth = accumulated_depth / coverage;
    }
    output[id.x * 2u] = vec4<f32>(accumulated_color, coverage);
    output[id.x * 2u + 1u] = vec4<f32>(mean_depth, f32(contributions), transmittance, 0.0);
}
