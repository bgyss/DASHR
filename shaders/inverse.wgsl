@vertex fn vs_main(@builtin(vertex_index) i:u32) -> Fullscreen {return screen_vertex(i);}
fn fragment_planes(v:Fullscreen) -> Planes {
    let src=nearest(edge_map,v.uv).xy;
    let w=read_warp(src);
    let inverse= inverse_basis(w.basis);
    return Planes(vec4<f32>(inverse[0],0.0),vec4<f32>(inverse[1],0.0),vec4<f32>(inverse[2],0.0),vec4<f32>(0.0));
}
