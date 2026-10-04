@group(2) @binding(0) var output0: texture_storage_2d<rgba32float, write>;
@group(2) @binding(1) var output1: texture_storage_2d<rgba32float, write>;
@group(2) @binding(2) var output2: texture_storage_2d<rgba32float, write>;
@group(2) @binding(3) var output3: texture_storage_2d<rgba32float, write>;

@compute @workgroup_size(8, 8, 1)
fn cs_main(@builtin(global_invocation_id) gid:vec3<u32>) {
    let dims=textureDimensions(output0);
    if(gid.x>=dims.x || gid.y>=dims.y){return;}
    let uv=(vec2<f32>(gid.xy)+vec2<f32>(0.5))/vec2<f32>(dims);
    let p=fragment_planes(Fullscreen(vec4<f32>(0.0),uv));
    let coord=vec2<i32>(gid.xy);
    textureStore(output0,coord,p.a);
    textureStore(output1,coord,p.b);
    textureStore(output2,coord,p.c);
    textureStore(output3,coord,p.d);
}
