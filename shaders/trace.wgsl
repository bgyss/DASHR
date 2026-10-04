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
struct RefinedHit { position:vec3<f32>, distance:f32, iterations:u32, status:u32 }
struct RefinedTeleport { source_uv:vec2<f32>, distance:f32, iterations:u32, status:u32 }
// Status 0=refined, 1=seam bracket refused, 2=invalid interpolated basis.
fn refine_teleport(
    lower_position:vec3<f32>,lower_distance:f32,
    upper_position:vec3<f32>,upper_distance:f32,
    start:vec3<f32>,direction:vec3<f32>,used_steps:u32,
) -> RefinedTeleport {
    var low_position=lower_position;var low_distance=lower_distance;
    var high_position=upper_position;var high_distance=upper_distance;
    var iterations=0u;
    // Do not spend the refinement budget when adapter-filtered samples made a false bracket.
    if(seam_distance_bilinear(low_position.xy)>0.0 || seam_distance_bilinear(high_position.xy)<=0.0) {
        return RefinedTeleport(high_position.xy,high_distance,iterations,1u);
    }
    for(var i=0u;i<12u;i++) {
        if(used_steps+iterations>=u32(u.control.z)){break;}
        let middle_distance=(low_distance+high_distance)*0.5;
        let ray_point=start+direction*middle_distance;
        var uv=(low_position.xy+high_position.xy)*0.5;
        var surface=surface_position(ray_point,uv);
        if(!surface.valid){return RefinedTeleport(high_position.xy,high_distance,iterations,2u);}
        for(var resolve=0;resolve<max(0,u.control.x);resolve++) {
            uv=surface.position.xy;
            surface=surface_position(ray_point,uv);
            if(!surface.valid){return RefinedTeleport(high_position.xy,high_distance,iterations,2u);}
        }
        iterations++;
        if(seam_distance_bilinear(surface.position.xy)>0.0) {
            high_position=surface.position;
            high_distance=middle_distance;
        } else {
            low_position=surface.position;
            low_distance=middle_distance;
        }
    }
    return RefinedTeleport(high_position.xy,high_distance,iterations,0u);
}
fn refine_hit(
    lower_position:vec3<f32>,lower_distance:f32,
    upper_position:vec3<f32>,upper_distance:f32,
    start:vec3<f32>,direction:vec3<f32>,used_steps:u32,
) -> RefinedHit {
    var low_position=lower_position;var low_distance=lower_distance;
    var high_position=upper_position;var high_distance=upper_distance;
    var iterations=0u;
    for(var i=0u;i<12u;i++) {
        if(used_steps+iterations>=u32(u.control.z)){break;}
        let middle_distance=(low_distance+high_distance)*0.5;
        let ray_point=start+direction*middle_distance;
        var uv=(low_position.xy+high_position.xy)*0.5;
        var surface=surface_position(ray_point,uv);
        if(!surface.valid){return RefinedHit(high_position,high_distance,iterations,2u);}
        for(var resolve=0;resolve<max(0,u.control.x);resolve++) {
            uv=surface.position.xy;
            surface=surface_position(ray_point,uv);
            if(!surface.valid){return RefinedHit(high_position,high_distance,iterations,2u);}
        }
        iterations++;
        // Do not refine through a discontinuous teleport bracket.
        if(seam_distance(surface.position.xy)>0.0) {
            return RefinedHit(upper_position,upper_distance,iterations,1u);
        }
        // Refine against the normalized texel-defined height surface; ordinary steps keep the
        // adapter sampler selected by the reference path.
        let h=sample_height_bilinear(surface.position.xy);
        let height=(h-0.5)*u.height_step.x+0.5+u.height_step.y;
        if(height-surface.position.z>0.0) {
            high_position=surface.position;
            high_distance=middle_distance;
        } else {
            low_position=surface.position;
            low_distance=middle_distance;
        }
    }
    return RefinedHit(high_position,high_distance,iterations,0u);
}
fn trace_ray(initial:vec3<f32>,start:vec3<f32>,direction:vec3<f32>) -> Trace {
    var pos=initial;var distance=0.0;var point=start;
    var step_size=0.0;var delta=-1.0;var teleports=0u;var steps=0u;var has_bracket=false;
    var seam_bracket_valid=false;var seam_safe_position=initial;var seam_safe_distance=0.0;
    let envelope_min=-0.5*u.height_step.x+u.height_step.y;
    let envelope_max=0.5*u.height_step.x+1.0+u.height_step.y;
    loop {
        if(steps>=u32(u.control.z)){break;}
        var previous_distance=distance;var previous_pos=pos;let previous_point=point;var previous_delta=delta;
        if(u.modes.x==2 || (u.modes.w>=0 && u.modes.w==i32(steps))){return Trace(pos,distance,steps,teleports,5u);}
        let sdf=seam_distance(pos.xy);
        var teleported=false;
        if(sdf>0.0) {
            if(trace_feature(1) && seam_bracket_valid) {
                let refined=refine_teleport(seam_safe_position,seam_safe_distance,pos,distance,start,direction,steps);
                steps+=refined.iterations;
                if(refined.status==2u){return Trace(pos,distance,steps,teleports,4u);}
                if(steps>=u32(u.control.z)){return Trace(pos,distance,steps,teleports,3u);}
                distance=refined.distance;
                point=start+direction*distance;
                pos=vec3<f32>(nearest_seam_destination(refined.source_uv),pos.z);
                previous_distance=distance;previous_pos=pos;previous_delta=-1.0;
                has_bracket=false;
            } else {
                pos=vec3<f32>(nearest_seam_destination(pos.xy),pos.z);
            }
            if(trace_feature(1)) {
                seam_bracket_valid=false;
                has_bracket=false;
            }
            teleports++;teleported=true;
            for(var i=0;i<u.control.x;i++){
                let surface=surface_position(point,pos.xy);
                if(!surface.valid){return Trace(pos,distance,steps,teleports,4u);}
                pos=surface.position;
            }
            // The analytic probe captures this first crossing before the next trace step.
            if(u.modes.w==-3){return Trace(pos,distance,steps,teleports,5u);}
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
        let h=sample_height(pos.xy);
        let height=(h-0.5)*u.height_step.x+0.5+u.height_step.y;
        delta=height-pos.z;
        if(delta>0.0) {
            if(trace_feature(1) && !teleported && has_bracket) {
                let refined=refine_hit(previous_pos,previous_distance,pos,distance,start,direction,steps);
                steps+=refined.iterations;
                if(refined.status==2u){return Trace(refined.position,refined.distance,steps,teleports,4u);}
                if(refined.status==0u){pos=refined.position;distance=refined.distance;}
            } else if(!trace_feature(1) && !teleported) {
                let change=delta-previous_delta;
                if(abs(change)>0.00001) {
                    let lambda=clamp(delta/change,0.0,1.0);
                    pos=mix(pos,previous_pos,lambda);
                    distance=mix(distance,previous_distance,lambda);
                }
            }
            return Trace(pos,distance,steps,teleports,1u);
        }
        if(trace_feature(1)) {
            if(!teleported && seam_distance(pos.xy)<=0.0) {
                seam_safe_position=pos;
                seam_safe_distance=distance;
                seam_bracket_valid=true;
            }
            has_bracket=true;
        }
        step_size=u.height_step.z*max(1.0,-delta*u.height_step.w);
        if(trace_feature(2) || trace_feature(4)) {
            for(var retry=0u;retry<4u;retry++) {
                let candidate_point=start+direction*(distance+step_size);
                let candidate=surface_position(candidate_point,pos.xy);
                if(!candidate.valid){step_size*=0.5;continue;}
                var shrink=1.0;
                if(trace_feature(2)) {
                    let sdf0=seam_distance(pos.xy);
                    let seam_band=4.0/u.sun_atlas.w;
                    if(sdf0<=0.0 && -sdf0<=seam_band) {
                        let sdf1=seam_distance(candidate.position.xy);
                        if(sdf1>0.0) {
                            let fraction=clamp(-sdf0/(sdf1-sdf0),0.0,1.0);
                            let one_texel=1.0/u.sun_atlas.w;
                            if(-sdf0<=one_texel) {
                                shrink=min(shrink,clamp(fraction*1.1,0.01,1.0));
                            } else {
                                shrink=min(shrink,clamp(fraction*0.75,0.05,0.9));
                            }
                        }
                    }
                }
                if(trace_feature(4)) {
                    let previous_transform=object_basis(pos.xy,read_warp(pos.xy).basis);
                    let candidate_transform=object_basis(candidate.position.xy,read_warp(candidate.position.xy).basis);
                    let magnitude=max(length(previous_transform[0])+length(previous_transform[1])+length(previous_transform[2]),1e-6);
                    let change=max(
                        max(length(candidate_transform[0]-previous_transform[0]),length(candidate_transform[1]-previous_transform[1])),
                        length(candidate_transform[2]-previous_transform[2])
                    )/magnitude;
                    if(change>0.25){shrink=min(shrink,clamp(0.25/change,0.1,0.5));}
                }
                if(shrink>=0.999){break;}
                step_size*=shrink;
            }
        }
        if(u.modes.w==-4) {
            // The U4 probe returns the step after bounded transform-change retries.
            return Trace(vec3<f32>(pos.xy,step_size),distance,steps,teleports,5u);
        }
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
        let hit_object_basis=object_basis(uv,w.basis);
        var normal=normalize(hit_object_basis[2]);
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
                normal=normalize(cross(hit_object_basis[0]-hu*scale*hit_object_basis[2],hit_object_basis[1]-hv*scale*hit_object_basis[2]));
            } else if(u.modes.y>=3){
                let normal_surface=textureSampleLevel(normal_map,linear_sampler,uv,0.0).xyz*vec3<f32>(u.lighting.w,u.lighting.w,1.0);
                normal=normalize(hit_object_basis*normal_surface);
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
            let sdf=seam_distance(uv);
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
    var color_output=vec4<f32>(color,alpha);
    // Negative trace modes are probe-only diagnostics rejected by Settings validation.
    if(u.modes.w == -2 || u.modes.w == -3 || u.modes.w == -4){
        let debug_warp=read_warp(primary.uvh.xy);
        let debug_height=sample_height(primary.uvh.xy);
        color_output=vec4<f32>(debug_height,debug_warp.basis[0].x,debug_warp.basis[2].z,debug_warp.anchor.z);
    }
    // Diagnostics deliberately retain failed shell traces. The normal display discards
    // them, preserving alpha test and scene depth semantics.
    if(alpha<=0.0 && !diagnostics_enabled()){discard;}
    var shadow_output=vec4<f32>(f32(shadow.status),f32(shadow.steps),f32(shadow.teleports),shadow.distance);
    if(u.modes.w == -2 || u.modes.w == -3 || u.modes.w == -4){shadow_output=vec4<f32>(direction,read_warp(primary.uvh.xy).anchor.x);}
    return TracePlanes(color_output,
        vec4<f32>(primary.uvh.xy,primary.distance,actual_depth),
        vec4<f32>(f32(primary.status),f32(primary.steps),f32(primary.teleports),primary.uvh.z),
        shadow_output,depth);
}
