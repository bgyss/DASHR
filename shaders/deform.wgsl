struct DeformVertex {
    @builtin(position) clip:vec4<f32>,
    @location(0) a:vec3<f32>, @location(1) b:vec3<f32>,
    @location(2) c:vec3<f32>, @location(3) d:vec3<f32>,
    @location(4) valid:f32,
}
@vertex fn vs_main(v:VertexInput) -> DeformVertex {
    let bone=bone_matrix(v.weights);
    let p=(bone*vec4<f32>(v.position,1.0)).xyz;
    let t=(bone*vec4<f32>(v.tangent,0.0)).xyz;
    let b=(bone*vec4<f32>(v.bitangent,0.0)).xyz;
    let n=(bone*vec4<f32>(v.normal,0.0)).xyz;
    let basis=mat3x3<f32>(t,b,n);
    var offset=vec3<f32>(0.0);
    if(u.modes.z==0){offset=p-basis*vec3<f32>(v.uv.xy,0.5);}
    let inv=inverse4(affine(basis,offset));
    var anchor=inv[3].xyz;
    if(u.modes.z==1){anchor=p;}
    return DeformVertex(vec4<f32>(v.uv.x*2.0-1.0,1.0-v.uv.y*2.0,1.0,1.0),inv[0].xyz,inv[1].xyz,inv[2].xyz,anchor,select(0.0,1.0,valid_basis(basis)));
}
fn fragment_planes(v:DeformVertex) -> Planes {
    return Planes(vec4<f32>(v.a,1.0),vec4<f32>(v.b,1.0),vec4<f32>(v.c,v.valid),vec4<f32>(v.d,1.0));
}
