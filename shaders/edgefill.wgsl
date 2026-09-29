@vertex fn vs_main(@builtin(vertex_index) i:u32) -> Fullscreen {return screen_vertex(i);}
fn fragment_planes(v:Fullscreen) -> Planes {
    let src=nearest(edge_map,v.uv).xy;
    let w=read_warp(src);
    var anchor=w.anchor;
    var distortion=vec2<f32>(1.0);
    if(u.modes.z==1) {
        let step=1.0/u.sun_atlas.w;
        let pu=read_warp(nearest(edge_map,v.uv+vec2<f32>(step,0.0)).xy).anchor;
        let nu=read_warp(nearest(edge_map,v.uv-vec2<f32>(step,0.0)).xy).anchor;
        let pv=read_warp(nearest(edge_map,v.uv+vec2<f32>(0.0,step)).xy).anchor;
        let nv=read_warp(nearest(edge_map,v.uv-vec2<f32>(0.0,step)).xy).anchor;
        distortion=vec2<f32>((w.basis*(pu-nu)).x,(w.basis*(pv-nv)).y)/step;
        let offset=v.uv-src;
        if(any(offset!=vec2<f32>(0.0))){anchor+=inverse_basis(w.basis)*vec3<f32>(offset,0.0);}
    }
    return Planes(vec4<f32>(w.basis[0],distortion.x),vec4<f32>(w.basis[1],distortion.y),vec4<f32>(w.basis[2],select(0.0,1.0,w.valid)),vec4<f32>(anchor,1.0));
}
