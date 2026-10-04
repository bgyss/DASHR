// Translated from Tom Forsyth's DASHR v1.0 (MIT-0). Column-major host/GPU matrices.
struct Uniforms {
    projection: mat4x4<f32>,
    camera_from_object: mat4x4<f32>,
    object_from_camera: mat4x4<f32>,
    height_step: vec4<f32>, // height scale, height offset, step size, step scale
    bones: array<mat4x4<f32>,4>,
    sun_atlas: vec4<f32>, // normalized object-space sun.xyz, atlas size
    modes: vec4<i32>, // debug, lighting, distortion, optional forced-hit step
    damping_extrusion: vec4<f32>, // damping factors 1/2/3, extra shell extrusion
    lighting: vec4<f32>, // deltaUV, shadow acne, indirect light, height-normal scale
    control: vec4<i32>, // teleport iterations, hit-depth toggle, production budget, diagnostics/trace-option bits
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
@group(1) @binding(10) var inverse0: texture_2d<f32>;
@group(1) @binding(11) var inverse1: texture_2d<f32>;
@group(1) @binding(12) var inverse2: texture_2d<f32>;
@group(1) @binding(13) var destination_map: texture_2d<f32>;
@group(1) @binding(14) var distance_map: texture_2d<f32>;
fn trace_feature(flag:i32) -> bool {
    return ((u.control.w>>1)&flag)!=0;
}
fn diagnostics_enabled() -> bool {
    return (u.control.w&1)!=0;
}
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
    if(trace_feature(8)){return inverse_basis_fast(b);}
    let m=inverse4(affine(b,vec3<f32>(0.0)));
    return mat3x3<f32>(m[0].xyz,m[1].xyz,m[2].xyz);
}
fn inverse_basis_fast(b:mat3x3<f32>) -> mat3x3<f32> {
    let determinant=dot(b[0],cross(b[1],b[2]));
    if(abs(determinant)<1e-30){return mat3x3<f32>();}
    let row0=cross(b[1],b[2])/determinant;
    let row1=cross(b[2],b[0])/determinant;
    let row2=cross(b[0],b[1])/determinant;
    return mat3x3<f32>(
        vec3<f32>(row0.x,row1.x,row2.x),
        vec3<f32>(row0.y,row1.y,row2.y),
        vec3<f32>(row0.z,row1.z,row2.z)
    );
}
fn object_basis(uv:vec2<f32>,surface_from_object:mat3x3<f32>) -> mat3x3<f32> {
    if(trace_feature(16)) {
        let a=sample_float(inverse0,uv).xyz;
        let b=sample_float(inverse1,uv).xyz;
        let c=sample_float(inverse2,uv).xyz;
        return mat3x3<f32>(a,b,c);
    }
    return inverse_basis(surface_from_object);
}
fn nearest(t:texture_2d<f32>,uv:vec2<f32>) -> vec4<f32> {
    let dims=vec2<i32>(textureDimensions(t));
    return textureLoad(t,clamp(vec2<i32>(floor(uv*vec2<f32>(dims))),vec2<i32>(0),dims-vec2<i32>(1)),0);
}
fn sample_float_bilinear(t:texture_2d<f32>,uv:vec2<f32>) -> vec4<f32> {
    let dims=vec2<i32>(textureDimensions(t));
    let texel=uv*vec2<f32>(dims)-vec2<f32>(0.5);
    let base=vec2<i32>(floor(texel));
    let fraction=fract(texel);
    let maximum=dims-vec2<i32>(1);
    let a=textureLoad(t,clamp(base,vec2<i32>(0),maximum),0);
    let b=textureLoad(t,clamp(base+vec2<i32>(1,0),vec2<i32>(0),maximum),0);
    let c=textureLoad(t,clamp(base+vec2<i32>(0,1),vec2<i32>(0),maximum),0);
    let d=textureLoad(t,clamp(base+vec2<i32>(1,1),vec2<i32>(0),maximum),0);
    return mix(mix(a,b,fraction.x),mix(c,d,fraction.x),fraction.y);
}
fn sample_height_bilinear(uv:vec2<f32>) -> f32 {
    // U2 hit refinement uses explicit texel interpolation so the root target does not inherit
    // adapter-specific hardware-filter precision from ordinary ray steps.
    let dims=vec2<i32>(textureDimensions(height_map));
    let texel=uv*vec2<f32>(dims)-vec2<f32>(0.5);
    let base=vec2<i32>(floor(texel));
    let fraction=fract(texel);
    let maximum=dims-vec2<i32>(1);
    let a=textureLoad(height_map,clamp(base,vec2<i32>(0),maximum),0).r;
    let b=textureLoad(height_map,clamp(base+vec2<i32>(1,0),vec2<i32>(0),maximum),0).r;
    let c=textureLoad(height_map,clamp(base+vec2<i32>(0,1),vec2<i32>(0),maximum),0).r;
    let d=textureLoad(height_map,clamp(base+vec2<i32>(1,1),vec2<i32>(0),maximum),0).r;
    return mix(mix(a,b,fraction.x),mix(c,d,fraction.x),fraction.y);
}
fn sample_height(uv:vec2<f32>) -> f32 {
    if(trace_feature(128)){return sample_height_bilinear(uv);}
    return textureSampleLevel(height_map,linear_sampler,uv,0.0).r;
}
fn seam_distance(uv:vec2<f32>) -> f32 {
    if(trace_feature(64)){return sample_float(distance_map,uv).r;}
    return sample_float(teleport_map,uv).z;
}
fn seam_distance_bilinear(uv:vec2<f32>) -> f32 {
    if(trace_feature(64)){return sample_float_bilinear(distance_map,uv).r;}
    return sample_float_bilinear(teleport_map,uv).z;
}
fn nearest_seam_destination(uv:vec2<f32>) -> vec2<f32> {
    if(trace_feature(64)){return nearest(destination_map,uv).xy;}
    return nearest(teleport_map,uv).xy;
}
struct Warp { basis:mat3x3<f32>, anchor:vec3<f32>, distortion:vec2<f32>, valid:bool }
fn read_warp_raw(uv:vec2<f32>) -> Warp {
    let a=sample_float(warp0,uv); let b=sample_float(warp1,uv);
    let c=sample_float(warp2,uv); let d=sample_float(warp3,uv);
    let basis=mat3x3<f32>(a.xyz,b.xyz,c.xyz);
    return Warp(basis,d.xyz,vec2<f32>(a.w,b.w),c.w>0.5 && valid_basis(basis));
}
fn read_warp(uv:vec2<f32>) -> Warp {
    if(!trace_feature(32)){return read_warp_raw(uv);}
    let source=sample_float(edge_map,uv).xy;
    var w=read_warp_raw(source);
    if(u.modes.z==1) {
        let step=1.0/u.sun_atlas.w;
        let pu=read_warp_raw(sample_float(edge_map,uv+vec2<f32>(step,0.0)).xy).anchor;
        let nu=read_warp_raw(sample_float(edge_map,uv-vec2<f32>(step,0.0)).xy).anchor;
        let pv=read_warp_raw(sample_float(edge_map,uv+vec2<f32>(0.0,step)).xy).anchor;
        let nv=read_warp_raw(sample_float(edge_map,uv-vec2<f32>(0.0,step)).xy).anchor;
        w.distortion=vec2<f32>((w.basis*(pu-nu)).x,(w.basis*(pv-nv)).y)/step;
        let offset=uv-source;
        if(any(offset!=vec2<f32>(0.0))){w.anchor+=inverse_basis(w.basis)*vec3<f32>(offset,0.0);}
    }
    return w;
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
