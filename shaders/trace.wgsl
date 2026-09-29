struct ShellVertex {
    @builtin(position) clip:vec4<f32>,
    @location(0) uvh:vec3<f32>,
    @location(1) start:vec3<f32>,
    @location(2) direction:vec3<f32>,
}
@vertex fn vs_main(v:VertexInput) -> ShellVertex {
    let bone=bone_matrix(v.weights);
    let normal=(bone*vec4<f32>(v.normal,0.0)).xyz;
    let p=(bone*vec4<f32>(v.position,1.0)).xyz+normal*v.uv.z*(u.height_step.x+u.damping_extrusion.w)*0.5;
    let camera=u.camera_from_object*vec4<f32>(p,1.0);
    let direction=(u.object_from_camera*vec4<f32>(camera.xyz,0.0)).xyz;
    // Normalize after interpolation, matching the original perspective ray.
    return ShellVertex(u.projection*camera,v.uv,p,direction);
}
// Status 0=not launched, 1=hit, 2=envelope escape, 3=budget, 4=invalid,
// 5=explicit reference debug forced hit. Primary and shadow retain independent results.
struct Trace { uvh:vec3<f32>, distance:f32, steps:u32, teleports:u32, status:u32 }
fn trace_ray(initial:vec3<f32>,start:vec3<f32>,direction:vec3<f32>) -> Trace {
    var pos=initial;var distance=0.0;var point=start;
    var step_size=0.0;var delta=-1.0;var teleports=0u;var steps=0u;
    let envelope_min=-0.5*u.height_step.x+u.height_step.y;
    let envelope_max=0.5*u.height_step.x+1.0+u.height_step.y;
    loop {
        if(steps>=u32(u.control.z)){break;}
        let previous_distance=distance;let previous_pos=pos;let previous_point=point;let previous_delta=delta;
        if(u.modes.x==2 || u.modes.w==i32(steps)){return Trace(pos,distance,steps,teleports,5u);}
        let sdf=sample_float(teleport_map,pos.xy).z;
        var teleported=false;
        if(sdf>0.0) {
            pos=vec3<f32>(nearest(teleport_map,pos.xy).xy,pos.z);
            teleports++;teleported=true;
            for(var i=0;i<u.control.x;i++){
                let surface=surface_position(point,pos.xy);
                if(!surface.valid){return Trace(pos,distance,steps,teleports,4u);}
                pos=surface.position;
            }
        }
        distance=previous_distance+step_size;
        point=start+direction*distance;
        let surface=surface_position(point,pos.xy);
        if(!surface.valid){return Trace(pos,distance,steps,teleports,4u);}
        pos=surface.position;
        if(surface.factor<1.0) {
            distance=previous_distance+step_size*max(0.0001,surface.factor);
            point=start+direction*distance;
        }
        if(any(pos.xy<vec2<f32>(0.0)) || any(pos.xy>vec2<f32>(1.0)) || pos.z<envelope_min || pos.z>envelope_max) {
            return Trace(pos,distance,steps,teleports,2u);
        }
        let h=textureSampleLevel(height_map,linear_sampler,pos.xy,0.0).r;
        let height=(h-0.5)*u.height_step.x+0.5+u.height_step.y;
        delta=height-pos.z;
        if(delta>0.0) {
            if(!teleported) {
                let change=delta-previous_delta;
                if(abs(change)>0.00001) {
                    let lambda=clamp(delta/change,0.0,1.0);
                    pos=mix(pos,previous_pos,lambda);
                    distance=mix(distance,previous_distance,lambda);
                }
            }
            return Trace(pos,distance,steps,teleports,1u);
        }
        step_size=u.height_step.z*max(1.0,-delta*u.height_step.w);
        steps++;
    }
    return Trace(pos,distance,steps,teleports,3u);
}
struct TracePlanes {
    color:vec4<f32>, hit:vec4<f32>, primary:vec4<f32>, shadow:vec4<f32>, depth:f32,
}
fn fragment_trace(v:ShellVertex) -> TracePlanes {
    if(u.modes.x==1) {
        return TracePlanes(textureSampleLevel(height_map,linear_sampler,v.uvh.xy,0.0),
            vec4<f32>(0.0),vec4<f32>(0.0),vec4<f32>(0.0),v.clip.z);
    }
    let direction=normalize(v.direction);
    let primary=trace_ray(v.uvh,v.start,direction);
    var shadow=Trace(vec3<f32>(0.0),0.0,0u,0u,0u);
    var color=vec3<f32>(0.02,0.025,0.035);
    var alpha=0.0; var depth=0.0;
    let point=v.start+direction*primary.distance;
    let hit_clip=u.projection*u.camera_from_object*vec4<f32>(point,1.0);
    let actual_depth=hit_clip.z/hit_clip.w;
    if(primary.status==1u || primary.status==5u) {
        let uv=primary.uvh.xy;
        let w=read_warp(uv);
        let object_basis=inverse_basis(w.basis);
        var normal=normalize(object_basis[2]);
        let albedo=textureSampleLevel(albedo_map,linear_sampler,uv,0.0).rgb;
        if(u.modes.y==0){color=textureSampleLevel(height_map,linear_sampler,uv,0.0).rgb;}
        else if(u.modes.x==2){color=albedo;}
        else if(u.modes.x==5){color=vec3<f32>(0.5+(w.distortion-vec2<f32>(1.0))*0.25,0.0);}
        else {
            if(u.modes.y==2){
                let step=u.lighting.x;
                let hu=textureSampleLevel(height_map,linear_sampler,uv-vec2<f32>(step,0.0),0.0).r-textureSampleLevel(height_map,linear_sampler,uv+vec2<f32>(step,0.0),0.0).r;
                let hv=textureSampleLevel(height_map,linear_sampler,uv-vec2<f32>(0.0,step),0.0).r-textureSampleLevel(height_map,linear_sampler,uv+vec2<f32>(0.0,step),0.0).r;
                let scale=0.25*u.lighting.w/step;
                normal=normalize(cross(object_basis[0]-hu*scale*object_basis[2],object_basis[1]-hv*scale*object_basis[2]));
            } else if(u.modes.y>=3){
                let normal_surface=textureSampleLevel(normal_map,linear_sampler,uv,0.0).xyz*vec3<f32>(u.lighting.w,u.lighting.w,1.0);
                normal=normalize(object_basis*normal_surface);
            }
            let ndl=dot(normal,u.sun_atlas.xyz);
            let indirect=u.lighting.z*clamp(ndl*0.5+0.5,0.0,1.0);
            var direct=clamp(clamp(ndl,0.0,1.0)-indirect,0.0,1.0);
            if(u.modes.y>=4 && direct>0.0){
                let origin=point+u.lighting.y*normal;
                let s=surface_position(origin,uv);
                if(s.valid){shadow=trace_ray(s.position,origin,u.sun_atlas.xyz);}
                else {shadow.status=4u;}
                if(shadow.status==1u || shadow.status==5u){direct=0.0;}
            }
            color=albedo*(direct+indirect);
        }
        if(u.modes.x==4){
            let sdf=sample_float(teleport_map,uv).z;
            color=vec3<f32>(uv,select(0.0,1.0,sdf> -0.01));
            if(fract(uv.x*16.0)<0.01){color.x+=0.2;}
            if(fract(uv.y*16.0)<0.01){color.y+=0.2;}
            if(fract(uv.x*256.0)<0.08){color.x+=0.2;}
            if(fract(uv.y*256.0)<0.08){color.y+=0.2;}
        }
        alpha=1.0;
        depth=select(v.clip.z,actual_depth,u.control.y==1);
        if(u.control.y==1 && (hit_clip.w<=0.0 || actual_depth<0.0 || actual_depth>1.0)){alpha=0.0;depth=0.0;}
    }
    if(u.modes.x==3){color=vec3<f32>(f32(shadow.steps)*0.01,f32(primary.steps)*0.01,f32(primary.teleports)*0.1);alpha=1.0;depth=v.clip.z;}
    // Diagnostics deliberately retain failed shell traces. The normal display discards
    // them, preserving alpha test and scene depth semantics.
    if(alpha<=0.0 && u.control.w==0){discard;}
    return TracePlanes(vec4<f32>(color,alpha),
        vec4<f32>(primary.uvh.xy,primary.distance,actual_depth),
        vec4<f32>(f32(primary.status),f32(primary.steps),f32(primary.teleports),primary.uvh.z),
        vec4<f32>(f32(shadow.status),f32(shadow.steps),f32(shadow.teleports),shadow.distance),depth);
}
