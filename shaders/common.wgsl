// Translated from Tom Forsyth's DASHR v1.0 (MIT-0). Column-major host/GPU matrices.
struct Uniforms {
    projection: mat4x4<f32>,
    camera_from_object: mat4x4<f32>,
    object_from_camera: mat4x4<f32>,
    height_step: vec4<f32>, // height scale, height offset, step size, step scale
    bones: array<mat4x4<f32>,4>,
    sun_atlas: vec4<f32>, // normalized object-space sun.xyz, atlas size
    modes: vec4<i32>, // debug, lighting, distortion, reference debug forced-hit step
    damping_extrusion: vec4<f32>, // damping factors 1/2/3, extra shell extrusion
    lighting: vec4<f32>, // deltaUV, shadow acne, indirect light, height-normal scale
    control: vec4<i32>, // teleport iterations, hit-depth toggle, production budget, diagnostics
}
@group(0) @binding(0) var<uniform> u: Uniforms;
@group(1) @binding(0) var warp0: texture_2d<f32>;
@group(1) @binding(1) var warp1: texture_2d<f32>;
@group(1) @binding(2) var warp2: texture_2d<f32>;
@group(1) @binding(3) var warp3: texture_2d<f32>;
@group(1) @binding(4) var teleport_map: texture_2d<f32>;
@group(1) @binding(5) var edge_map: texture_2d<f32>;
@group(1) @binding(6) var height_map: texture_2d<f32>;
@group(1) @binding(7) var albedo_map: texture_2d<f32>;
@group(1) @binding(8) var normal_map: texture_2d<f32>;
@group(1) @binding(9) var linear_sampler: sampler;
struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec3<f32>,
    @location(2) weights: vec4<f32>,
    @location(3) normal: vec3<f32>,
    @location(4) tangent: vec3<f32>,
    @location(5) bitangent: vec3<f32>,
}
fn bone_matrix(w:vec4<f32>) -> mat4x4<f32> {
    return u.bones[0]*w.x+u.bones[1]*w.y+u.bones[2]*w.z+u.bones[3]*w.w;
}
fn finite3(v:vec3<f32>) -> bool { return all(abs(v)<vec3<f32>(1e30)); }
fn valid_basis(b:mat3x3<f32>) -> bool {
    let scale=length(b[0])*length(b[1])*length(b[2]);
    return scale>0.0 && abs(determinant(b))>scale*1e-8 && finite3(b[0]) && finite3(b[1]) && finite3(b[2]);
}
// General cofactor inverse; preserve nonorthogonal metric bases.
fn inverse4(m:mat4x4<f32>) -> mat4x4<f32> {
    var cof:mat4x4<f32>;
    for(var c=0u;c<4u;c++) {
        for(var r=0u;r<4u;r++) {
            var cols:array<u32,3>; var rows:array<u32,3>; var ci=0u;var ri=0u;
            for(var k=0u;k<4u;k++) {
                if(k!=c){cols[ci]=k;ci++;}
                if(k!=r){rows[ri]=k;ri++;}
            }
            let minor=mat3x3<f32>(
                vec3<f32>(m[cols[0]][rows[0]],m[cols[0]][rows[1]],m[cols[0]][rows[2]]),
                vec3<f32>(m[cols[1]][rows[0]],m[cols[1]][rows[1]],m[cols[1]][rows[2]]),
                vec3<f32>(m[cols[2]][rows[0]],m[cols[2]][rows[1]],m[cols[2]][rows[2]]));
            cof[c][r]=determinant(minor)*select(1.0,-1.0,((c+r)%2u)==1u);
        }
    }
    let det=dot(m[0],cof[0]);
    if(abs(det)<1e-30){return mat4x4<f32>();}
    return transpose(cof)*(1.0/det);
}
fn affine(b:mat3x3<f32>,p:vec3<f32>) -> mat4x4<f32> {
    return mat4x4<f32>(vec4<f32>(b[0],0.0),vec4<f32>(b[1],0.0),vec4<f32>(b[2],0.0),vec4<f32>(p,1.0));
}
fn inverse_basis(b:mat3x3<f32>) -> mat3x3<f32> {
    let m=inverse4(affine(b,vec3<f32>(0.0)));
    return mat3x3<f32>(m[0].xyz,m[1].xyz,m[2].xyz);
}
fn nearest(t:texture_2d<f32>,uv:vec2<f32>) -> vec4<f32> {
    let dims=vec2<i32>(textureDimensions(t));
    return textureLoad(t,clamp(vec2<i32>(floor(uv*vec2<f32>(dims))),vec2<i32>(0),dims-vec2<i32>(1)),0);
}
struct Warp { basis:mat3x3<f32>, anchor:vec3<f32>, distortion:vec2<f32>, valid:bool }
fn read_warp(uv:vec2<f32>) -> Warp {
    let a=sample_float(warp0,uv); let b=sample_float(warp1,uv);
    let c=sample_float(warp2,uv); let d=sample_float(warp3,uv);
    let basis=mat3x3<f32>(a.xyz,b.xyz,c.xyz);
    return Warp(basis,d.xyz,vec2<f32>(a.w,b.w),c.w>0.5 && valid_basis(basis));
}
struct Surface { position:vec3<f32>, factor:f32, valid:bool }
fn surface_position(point:vec3<f32>,uv:vec2<f32>) -> Surface {
    let w=read_warp(uv);
    var scale=vec2<f32>(1.0);var factor=vec2<f32>(1.0);
    for(var i=0u;i<2u;i++) {
        if(w.distortion[i]<u.damping_extrusion.y){factor[i]=0.01;}
        else if(w.distortion[i]>u.damping_extrusion.z){scale[i]=1.0/(u.damping_extrusion.x*w.distortion[i]);factor[i]=scale[i];}
    }
    var pos:vec3<f32>;
    if(u.modes.z==0){pos=w.basis*point+w.anchor; pos=vec3<f32>(uv+(pos.xy-uv)*scale,pos.z);}
    else {pos=(w.basis*(point-w.anchor))*vec3<f32>(scale,1.0)+vec3<f32>(uv,0.5);}
    return Surface(pos,min(factor.x,factor.y),w.valid && finite3(pos));
}
struct Fullscreen { @builtin(position) clip:vec4<f32>, @location(0) uv:vec2<f32> }
fn screen_vertex(i:u32) -> Fullscreen {
    let uv=vec2<f32>(f32((i<<1u)&2u),f32(i&2u));
    return Fullscreen(vec4<f32>(uv.x*2.0-1.0,1.0-uv.y*2.0,0.0,1.0),uv);
}
struct Planes { a:vec4<f32>, b:vec4<f32>, c:vec4<f32>, d:vec4<f32> }
