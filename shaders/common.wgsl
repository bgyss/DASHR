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
// Original general cofactor inverse with statically addressable components.
// FXC cannot address dynamic matrix/array l-values inside the caller's ray loop.
fn inverse4(m:mat4x4<f32>) -> mat4x4<f32> {
    let a=m[0][0]; let b=m[1][0]; let c=m[2][0]; let d=m[3][0];
    let e=m[0][1]; let f=m[1][1]; let g=m[2][1]; let h=m[3][1];
    let i=m[0][2]; let j=m[1][2]; let k=m[2][2]; let l=m[3][2];
    let p=m[0][3]; let q=m[1][3]; let r=m[2][3]; let s=m[3][3];
    let s0=a*f-e*b; let s1=a*g-e*c; let s2=a*h-e*d;
    let s3=b*g-f*c; let s4=b*h-f*d; let s5=c*h-g*d;
    let c0=i*q-p*j; let c1=i*r-p*k; let c2=i*s-p*l;
    let c3=j*r-q*k; let c4=j*s-q*l; let c5=k*s-r*l;
    let det=s0*c5-s1*c4+s2*c3+s3*c2-s4*c1+s5*c0;
    if(abs(det)<1e-30){return mat4x4<f32>();}
    return mat4x4<f32>(
        vec4<f32>(f*c5-g*c4+h*c3,-e*c5+g*c2-h*c1,e*c4-f*c2+h*c0,-e*c3+f*c1-g*c0),
        vec4<f32>(-b*c5+c*c4-d*c3,a*c5-c*c2+d*c1,-a*c4+b*c2-d*c0,a*c3-b*c1+c*c0),
        vec4<f32>(q*s5-r*s4+s*s3,-p*s5+r*s2-s*s1,p*s4-q*s2+s*s0,-p*s3+q*s1-r*s0),
        vec4<f32>(-j*s5+k*s4-l*s3,i*s5-k*s2+l*s1,-i*s4+j*s2-l*s0,i*s3-j*s1+k*s0)
    )*(1.0/det);
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
fn damping_axis(distortion:f32)->vec2<f32> {
    if(distortion<u.damping_extrusion.y){return vec2<f32>(1.0,0.01);}
    if(distortion>u.damping_extrusion.z){
        let scale=1.0/(u.damping_extrusion.x*distortion);
        return vec2<f32>(scale,scale);
    }
    return vec2<f32>(1.0);
}
fn surface_position(point:vec3<f32>,uv:vec2<f32>) -> Surface {
    let w=read_warp(uv);
    let du=damping_axis(w.distortion.x); let dv=damping_axis(w.distortion.y);
    let scale=vec2<f32>(du.x,dv.x); let factor=vec2<f32>(du.y,dv.y);
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
