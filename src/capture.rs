//! Reproducible offscreen artifacts; no performance/parity promotion from captures alone.
use crate::{material::hash, passes::Renderer, settings::Settings};
use anyhow::{Context, Result, ensure};
use glam::{Mat4, Vec4};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path, process::Command};

fn write_json(path: &Path, value: &Value) -> Result<()> {
    std::fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}
pub fn source_hashes() -> Result<BTreeMap<String, String>> {
    Ok(crate::fingerprints::source_hashes(Path::new(env!(
        "CARGO_MANIFEST_DIR"
    )))?)
}
pub fn save_float(path: &Path, pixels: &[[f32; 4]]) -> Result<()> {
    ensure!(
        pixels.iter().flatten().all(|x| x.is_finite()),
        "non-finite data in {}",
        path.file_name().unwrap_or_default().to_string_lossy()
    );
    let mut bytes = Vec::with_capacity(pixels.len() * 16);
    for pixel in pixels {
        for value in pixel {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
    }
    std::fs::write(path, bytes)?;
    Ok(())
}
pub fn statistics(primary: &[[f32; 4]], shadow: &[[f32; 4]]) -> Value {
    let summarize = |pixels: &[[f32; 4]]| {
        let mut counts = [0usize; 6];
        let mut steps = Vec::new();
        let mut teleports = 0usize;
        for p in pixels {
            let code = p[0] as usize;
            if code < 6 {
                counts[code] += 1;
            }
            if code != 0 {
                steps.push(p[1] as u32);
                teleports += p[2] as usize;
            }
        }
        steps.sort_unstable();
        let percentile = |q: f64| {
            if steps.is_empty() {
                0
            } else {
                steps[((steps.len() - 1) as f64 * q).ceil() as usize]
            }
        };
        json!({"not_launched":counts[0],"hit":counts[1],"escaped":counts[2],"budget":counts[3],"invalid":counts[4],"debug_forced_hit":counts[5],"teleports":teleports,"steps_p50":percentile(0.5),"steps_p95":percentile(0.95),"steps_p99":percentile(0.99),"steps_max":steps.last().copied().unwrap_or(0)})
    };
    json!({"primary":summarize(primary),"shadow":summarize(shadow)})
}
fn positions(s: &Settings, hit: &[[f32; 4]], status: &[[f32; 4]]) -> Vec<[f32; 4]> {
    let u = s.uniforms(true);
    let inverse = (Mat4::from_cols_array_2d(&u.projection)
        * Mat4::from_cols_array_2d(&u.camera_from_object))
    .inverse();
    hit.iter()
        .zip(status)
        .enumerate()
        .map(|(i, (h, st))| {
            if (st[0] == 1. || st[0] == 5.) && h[3] > 0. {
                let x = (i as u32 % s.width) as f32 + 0.5;
                let y = (i as u32 / s.width) as f32 + 0.5;
                let p = inverse
                    * Vec4::new(
                        x / s.width as f32 * 2. - 1.,
                        1. - y / s.height as f32 * 2.,
                        h[3],
                        1.,
                    );
                (p / p.w).to_array()
            } else {
                [0.; 4]
            }
        })
        .collect()
}
fn command_output(program: &str, args: &[&str]) -> String {
    Command::new(program)
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_else(|| "unavailable".to_owned())
}
pub struct CaptureOptions {
    pub manual_filter: bool,
    pub split: u32,
    pub frames: u32,
    pub time_step: f32,
    pub raw_frames: bool,
}
pub fn run(out: &Path, s: &Settings, assets: &Path, options: &CaptureOptions) -> Result<()> {
    let CaptureOptions {
        manual_filter: manual,
        split: planes,
        frames,
        time_step,
        raw_frames,
    } = *options;
    ensure!(
        (1..=10000).contains(&frames) && time_step.is_finite() && time_step >= 0.,
        "invalid capture sequence"
    );
    std::fs::create_dir_all(out)?;
    let mut manifest = json!({
        "schema":"dashr.capture.v1","status":"running","runtime_git_revision":command_output("git",&["rev-parse","HEAD"]),
        "source_snapshot":"9cf55d4c989a4ff7974dc1360e58027e739099f2",
        "runtime_working_tree_dirty":match command_output("git",&["status","--porcelain"]).as_str(){"unavailable"=>Value::Null,status=>json!(!status.is_empty())},
        "build_provenance":build_provenance(),
        "executable_sha256":hash(&std::fs::read(std::env::current_exe()?)?),
        "runtime_source_hashes":source_hashes().ok(),"fixture_sha256":hash(&serde_json::to_vec(s)?),"settings":s,
        "derived_uniforms":s.uniforms(true),"uniform_sha256":hash(bytemuck::bytes_of(&s.uniforms(true))),
        "runtime_rustc":command_output("rustc",&["-vV"]),"os":std::env::consts::OS,"architecture":std::env::consts::ARCH,
        "os_version":if cfg!(target_os="macos"){command_output("sw_vers",&["-productVersion"])}else{command_output("uname",&["-r"])},
        "frame_count":frames,"time_step":time_step,"color_mode":"reference UNORM data, raw shader RGB, PNG clamps to byte without sRGB conversion",
        "raw_format":"row-major top-left RGBA32 little-endian float; no header; dimensions recorded in manifest",
        "depth_mode":if s.hit_depth{"actual hit reverse-Z (experimental)"}else{"reference shell reverse-Z"},
        "near_plane_policy":"raster shell clipping; camera inside shell unsupported; scene occlusion not promoted",
        "display":"offscreen, no GUI/wireframes/vsync","power_state":"unrecorded; unsuitable for performance promotion",
        "evidence":{"d3d11_parity":"unverified","windows":"unverified","human_review":"pending","benchmark_protocol":"not executed"}
    });
    write_json(&out.join("manifest.json"), &manifest)?;
    std::fs::write(
        out.join("uniforms.bin"),
        bytemuck::bytes_of(&s.uniforms(true)),
    )?;
    let work = || -> Result<Value> {
        s.validate()?;
        let mut renderer = pollster::block_on(Renderer::headless(s, assets, manual, planes))?;
        let mut records = Vec::new();
        for index in 0..frames {
            let mut settings = s.clone();
            settings.time = s.time + index as f32 * time_step;
            let images = renderer.render_capture(&settings)?;
            ensure!(
                images.iter().flatten().flatten().all(|v| v.is_finite()),
                "NaN/Inf in frame {index}"
            );
            let png: Vec<u8> = images[0]
                .iter()
                .flat_map(|p| {
                    [p[0], p[1], p[2], 1.].map(|v| (v.clamp(0., 1.) * 255. + 0.5).floor() as u8)
                })
                .collect();
            image::save_buffer(
                out.join(format!("frame-{index:04}.png")),
                &png,
                s.width,
                s.height,
                image::ColorType::Rgba8,
            )?;
            let stats = statistics(&images[2], &images[3]);
            if index == 0 || raw_frames {
                for (name, pixels) in [
                    "color",
                    "hit-uv-distance-depth",
                    "primary-status-steps-teleports-height",
                    "shadow-status-steps-teleports-distance",
                ]
                .into_iter()
                .zip(&images)
                {
                    save_float(
                        &out.join(format!("frame-{index:04}-{name}.rgba32f")),
                        pixels,
                    )?;
                }
                save_float(
                    &out.join(format!("frame-{index:04}-object-position.rgba32f")),
                    &positions(&settings, &images[1], &images[2]),
                )?;
                let depth = renderer
                    .ctx
                    .read_texture(&renderer.frame.depth.texture, 4)?;
                std::fs::write(out.join(format!("frame-{index:04}-depth.f32")), depth)?;
            }
            if index == 0 {
                for (i, p) in renderer.atlas_planes()?.iter().enumerate() {
                    save_float(&out.join(format!("warp-{i}.rgba32f")), p)?;
                }
                for (i, p) in renderer.raw.iter().enumerate() {
                    save_float(
                        &out.join(format!("raw-warp-{i}.rgba32f")),
                        &renderer.ctx.read_float(&p.texture)?,
                    )?;
                }
                save_float(
                    &out.join("teleport.rgba32f"),
                    &renderer.ctx.read_float(&renderer.teleport.texture)?,
                )?;
                save_float(
                    &out.join("edgefill.rgba32f"),
                    &renderer.ctx.read_float(&renderer.edgefill.texture)?,
                )?;
            }
            records.push(json!({"frame":index,"time":settings.time,"stats":stats,"gpu_pass_ms":renderer.last_timings,
                "uniform_sha256":hash(bytemuck::bytes_of(&settings.uniforms(true))),
                "bone_matrices":settings.uniforms(true).bones}));
            if index % 30 == 0 {
                eprintln!("captured {}/{} frames", index + 1, frames);
            }
        }
        write_json(&out.join("trace-stats.json"), &json!(records))?;
        Ok(
            json!({"adapter":renderer.ctx.report,"material":renderer.provenance,"prepare_and_bake_ms":renderer.bake_ms,"seam_edges":renderer.seam_edges,"logical_warp_bytes":128u64*s.atlas as u64*s.atlas as u64,"validation":"no wgpu errors observed"}),
        )
    };
    match work() {
        Ok(result) => {
            manifest["status"] = json!("captured");
            manifest["result"] = result;
            write_json(&out.join("manifest.json"), &manifest)?;
            Ok(())
        }
        Err(error) => {
            manifest["status"] = json!("failed");
            manifest["failure"] = json!(format!("{error:#}"));
            write_json(&out.join("manifest.json"), &manifest)?;
            Err(error).context("capture failed; failure manifest saved")
        }
    }
}

pub fn build_provenance() -> Value {
    serde_json::from_str(include_str!(concat!(
        env!("OUT_DIR"),
        "/build-provenance.json"
    )))
    .expect("valid compiler-generated provenance")
}
