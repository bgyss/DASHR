//! Shader variants preserve representation while changing resource access/pass width.
pub fn source(pass: &str, filtered: bool, planes: u32, first: u32) -> String {
    assert!([1, 2, 4].contains(&planes) && first + planes <= 4);
    let base = common(filtered);
    let body = match pass {
        "deform" => include_str!("../shaders/deform.wgsl"),
        "edgefill" => include_str!("../shaders/edgefill.wgsl"),
        "trace" => include_str!("../shaders/trace.wgsl"),
        _ => panic!("unknown pass"),
    };
    let mut source = format!("{base}\n{body}\n");
    source.push_str("struct Outputs {\n");
    for i in 0..planes {
        source.push_str(&format!("@location({i}) p{i}:vec4<f32>,\n"));
    }
    if pass == "trace" {
        source.push_str("@builtin(frag_depth) depth:f32,\n");
    }
    let input = match pass {
        "deform" => "DeformVertex",
        "edgefill" => "Fullscreen",
        _ => "ShellVertex",
    };
    let function = if pass == "trace" {
        "fragment_trace"
    } else {
        "fragment_planes"
    };
    source.push_str(&format!(
        "}}\n@fragment fn fs_main(v:{input})->Outputs {{let p={function}(v);return Outputs("
    ));
    for i in 0..planes {
        if i > 0 {
            source.push(',');
        }
        let names = if pass == "trace" {
            ["color", "hit", "primary", "shadow"]
        } else {
            ["a", "b", "c", "d"]
        };
        source.push_str(&format!("p.{}", names[(first + i) as usize]));
    }
    if pass == "trace" {
        source.push_str(",p.depth");
    }
    source.push_str(");}\n");
    source
}

pub fn common(filtered: bool) -> String {
    let sample = if filtered {
        "fn sample_float(t:texture_2d<f32>,uv:vec2<f32>)->vec4<f32>{return textureSampleLevel(t,linear_sampler,uv,0.0);}".to_owned()
    } else {
        r#"fn sample_float(t:texture_2d<f32>,uv:vec2<f32>)->vec4<f32>{
            let dims=vec2<i32>(textureDimensions(t));let p=uv*vec2<f32>(dims)-vec2<f32>(0.5);
            let base=vec2<i32>(floor(p));let f=fract(p);
            let a=textureLoad(t,clamp(base,vec2<i32>(0),dims-vec2<i32>(1)),0);
            let b=textureLoad(t,clamp(base+vec2<i32>(1,0),vec2<i32>(0),dims-vec2<i32>(1)),0);
            let c=textureLoad(t,clamp(base+vec2<i32>(0,1),vec2<i32>(0),dims-vec2<i32>(1)),0);
            let d=textureLoad(t,clamp(base+vec2<i32>(1,1),vec2<i32>(0),dims-vec2<i32>(1)),0);
            return mix(mix(a,b,f.x),mix(c,d,f.x),f.y);
        }"#
        .to_owned()
    };
    format!("{}\n{sample}\n", include_str!("../shaders/common.wgsl"))
}
