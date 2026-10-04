@group(0) @binding(0) var<uniform> params:vec4<f32>;
@group(0) @binding(1) var<storage,read_write> output:array<vec4<f32>>;

fn canonical_sdf(position:vec3<f32>) -> f32 {
    return length(position)-1.0;
}

fn to_canonical(position:vec3<f32>,mode:u32) -> vec3<f32> {
    if(mode==0u){return vec3<f32>(position.x,position.y,position.z+params.x*sin(params.y*position.x));}
    let weight=smoothstep(-0.5,0.5,position.x);
    let cosine=cos(0.2);
    let sine=sin(0.2);
    let rotated=vec3<f32>(cosine*position.x-sine*position.z,position.y,sine*position.x+cosine*position.z);
    return mix(position,rotated,weight);
}

@compute @workgroup_size(8, 1, 1)
fn cs_main(@builtin(global_invocation_id) id:vec3<u32>) {
    if(id.x>=128u){return;}
    let mode=id.x/64u;
    let ray=id.x%64u;
    let coordinate=(f32(ray)+0.5)/64.0*2.0-1.0;
    let origin=vec3<f32>(coordinate*1.5,0.0,-3.0);
    let direction=normalize(vec3<f32>(coordinate*0.4,0.0,1.0));
    var distance=0.0;
    var sdf=0.0;
    var steps=0u;
    var status=2u;
    for(var iteration=0u;iteration<256u;iteration++) {
        let position=origin+direction*distance;
        sdf=canonical_sdf(to_canonical(position,mode));
        if(sdf<=1e-4){status=1u;steps=iteration+1u;break;}
        let bound=select(params.z,params.w,mode==1u);
        distance+=max(sdf/bound,1e-5);
        steps=iteration+1u;
        if(distance>6.0){status=2u;break;}
        if(iteration==255u){status=3u;}
    }
    output[id.x]=vec4<f32>(f32(status),f32(steps),distance,sdf);
}
