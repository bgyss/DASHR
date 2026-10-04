use anyhow::{Context, Result, ensure};
use dashr::{
    api::{DashrSession, FrameOutput, GpuOptions, TerminationStatus},
    asset::{Mesh, procedural},
    asset_diagnostics::suggest_seam_cut_for_hotspot,
    asset_format::AssetDocument,
    material::{self, Pixels},
    settings::Settings,
};
use glam::DVec2;
use image::{ColorType, ImageFormat};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

const BASELINE: &str = "out/u1-baseline/cube-pinched";
const OUTPUT: &str = "out/u1-reauthored/cube-pinched-640";
const OUTPUT_WIDTH: u32 = 640;
const OUTPUT_HEIGHT: u32 = 480;
// Pixel center corresponding to (74, 66) in the pinned 160×120 baseline.
const HOTSPOT_PIXEL: (u32, u32) = (298, 266);
const UV_SHIFT_PIXELS: (i32, i32) = (205, 1434);

fn read_edgefill(path: &Path, atlas_size: u32) -> Result<Vec<[f32; 4]>> {
    let bytes = fs::read(path).with_context(|| format!("read edgefill map {}", path.display()))?;
    let expected = atlas_size as usize * atlas_size as usize * 16;
    ensure!(
        bytes.len() == expected,
        "edgefill map has {} bytes, expected {expected}",
        bytes.len()
    );
    Ok(bytes
        .chunks_exact(16)
        .map(|pixel| {
            std::array::from_fn(|channel| {
                f32::from_le_bytes(pixel[channel * 4..channel * 4 + 4].try_into().unwrap())
            })
        })
        .collect())
}

fn region_uv_bounds(mesh: &Mesh, faces: &[u32]) -> [f32; 4] {
    let mut bounds = [
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    ];
    for &face in faces {
        for &index in &mesh.indices[face as usize * 3..face as usize * 3 + 3] {
            let uv = mesh.vertices[index as usize].uv;
            bounds[0] = bounds[0].min(uv[0]);
            bounds[1] = bounds[1].min(uv[1]);
            bounds[2] = bounds[2].max(uv[0]);
            bounds[3] = bounds[3].max(uv[1]);
        }
    }
    bounds
}

fn split_region(mesh: &mut Mesh, faces: &[u32], delta_uv: [f32; 2]) {
    let mut remap = BTreeMap::<u32, u32>::new();
    for &face in faces {
        for corner in 0..3 {
            let slot = face as usize * 3 + corner;
            let source = mesh.indices[slot];
            let destination = *remap.entry(source).or_insert_with(|| {
                let mut vertex = mesh.vertices[source as usize];
                vertex.uv[0] += delta_uv[0];
                vertex.uv[1] += delta_uv[1];
                let index = mesh.vertices.len() as u32;
                mesh.vertices.push(vertex);
                index
            });
            mesh.indices[slot] = destination;
        }
    }
}

fn uv_triangle_contains(mesh: &Mesh, face: usize, point: DVec2) -> bool {
    let triangle = &mesh.indices[face * 3..face * 3 + 3];
    let uv = [triangle[0], triangle[1], triangle[2]].map(|index| {
        let value = mesh.vertices[index as usize].uv;
        DVec2::new(f64::from(value[0]), f64::from(value[1]))
    });
    let edge_a = uv[1] - uv[0];
    let edge_b = uv[2] - uv[0];
    let relative = point - uv[0];
    let determinant = edge_a.perp_dot(edge_b);
    if determinant.abs() < 1e-12 {
        return false;
    }
    let weight_b = relative.perp_dot(edge_b) / determinant;
    let weight_c = edge_a.perp_dot(relative) / determinant;
    let weight_a = 1.0 - weight_b - weight_c;
    [weight_a, weight_b, weight_c]
        .iter()
        .all(|weight| *weight > 1e-6 && *weight < 1.0 - 1e-6)
}

fn verify_region_uv_separation(
    mesh: &Mesh,
    region_faces: &[u32],
    atlas_size: u32,
) -> Result<usize> {
    let region: BTreeMap<_, _> = region_faces
        .iter()
        .map(|face| (*face as usize, ()))
        .collect();
    let mut overlaps = 0;
    for y in 0..atlas_size {
        for x in 0..atlas_size {
            let point = DVec2::new(
                (x as f64 + 0.5) / atlas_size as f64,
                (y as f64 + 0.5) / atlas_size as f64,
            );
            let region_covered = region
                .keys()
                .any(|face| uv_triangle_contains(mesh, *face, point));
            if region_covered
                && (0..mesh.indices.len() / 3)
                    .filter(|face| !region.contains_key(face))
                    .any(|face| uv_triangle_contains(mesh, face, point))
            {
                overlaps += 1;
            }
        }
    }
    ensure!(
        overlaps == 0,
        "suggested UV patch overlaps surrounding faces at {overlaps} atlas texel centers"
    );
    Ok(overlaps)
}

fn move_texture_region(pixels: &mut Pixels, bounds: [f32; 4], dx: i32, dy: i32) {
    let source = pixels.rgba.clone();
    let width = pixels.width as i32;
    let height = pixels.height as i32;
    let padding = 32;
    let x0 = ((bounds[0] * width as f32).floor() as i32 - padding).clamp(0, width - 1);
    let x1 = ((bounds[2] * width as f32).ceil() as i32 + padding).clamp(0, width);
    let y0 = ((bounds[1] * height as f32).floor() as i32 - padding).clamp(0, height - 1);
    let y1 = ((bounds[3] * height as f32).ceil() as i32 + padding).clamp(0, height);
    for y in y0..y1 {
        for x in x0..x1 {
            let target_x = x + dx;
            let target_y = y + dy;
            if !(0..width).contains(&target_x) || !(0..height).contains(&target_y) {
                continue;
            }
            let from = (y as usize * width as usize + x as usize) * 4;
            let to = (target_y as usize * width as usize + target_x as usize) * 4;
            pixels.rgba[to..to + 4].copy_from_slice(&source[from..from + 4]);
        }
    }
}

fn write_png(path: &Path, pixels: &Pixels) -> Result<()> {
    image::save_buffer_with_format(
        path,
        &pixels.rgba,
        pixels.width,
        pixels.height,
        ColorType::Rgba8,
        ImageFormat::Png,
    )?;
    Ok(())
}

fn write_float_plane(path: &Path, pixels: &[[f32; 4]]) -> Result<()> {
    let mut bytes = Vec::with_capacity(pixels.len() * 16);
    for pixel in pixels {
        for value in pixel {
            ensure!(value.is_finite(), "non-finite output in {}", path.display());
            bytes.extend_from_slice(&value.to_le_bytes());
        }
    }
    fs::write(path, bytes).with_context(|| format!("write {}", path.display()))
}

fn diagnostic_planes(frame: &FrameOutput, primary: bool) -> Vec<[f32; 4]> {
    let trace = if primary {
        &frame.primary
    } else {
        &frame.shadow
    };
    trace
        .iter()
        .map(|item| {
            let status = match item.status {
                TerminationStatus::NotLaunched => 0,
                TerminationStatus::Hit => 1,
                TerminationStatus::Escaped => 2,
                TerminationStatus::BudgetExhausted => 3,
                TerminationStatus::InvalidBasis => 4,
                TerminationStatus::DebugForcedHit => 5,
            };
            [
                status as f32,
                item.steps as f32,
                item.teleports as f32,
                item.auxiliary,
            ]
        })
        .collect()
}

fn frame_summary(frame: &FrameOutput) -> Value {
    let summarize = |trace: &[dashr::api::TraceDiagnostic]| {
        let mut counts = [0usize; 6];
        let mut steps = Vec::new();
        let mut teleports = 0usize;
        for item in trace {
            let code = match item.status {
                TerminationStatus::NotLaunched => 0,
                TerminationStatus::Hit => 1,
                TerminationStatus::Escaped => 2,
                TerminationStatus::BudgetExhausted => 3,
                TerminationStatus::InvalidBasis => 4,
                TerminationStatus::DebugForcedHit => 5,
            };
            counts[code] += 1;
            if item.status != TerminationStatus::NotLaunched {
                steps.push(item.steps);
                teleports += item.teleports as usize;
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
        json!({
            "not_launched": counts[0], "hit": counts[1], "escaped": counts[2],
            "budget": counts[3], "invalid": counts[4], "debug_forced_hit": counts[5],
            "teleports": teleports, "steps_p95": percentile(0.95),
            "steps_max": steps.last().copied().unwrap_or(0)
        })
    };
    json!({
        "width": frame.width,
        "height": frame.height,
        "primary": summarize(&frame.primary),
        "shadow": summarize(&frame.shadow)
    })
}

fn write_frame(directory: &Path, name: &str, frame: &FrameOutput) -> Result<()> {
    let rgba: Vec<u8> = frame
        .color
        .iter()
        .flat_map(|pixel| {
            [pixel[0], pixel[1], pixel[2], 1.0]
                .map(|value| (value.clamp(0.0, 1.0) * 255.0 + 0.5).floor() as u8)
        })
        .collect();
    image::save_buffer(
        directory.join(format!("{name}.png")),
        &rgba,
        frame.width,
        frame.height,
        ColorType::Rgba8,
    )?;
    write_float_plane(
        &directory.join(format!("{name}-color.rgba32f")),
        &frame.color,
    )?;
    write_float_plane(
        &directory.join(format!("{name}-primary-status.rgba32f")),
        &diagnostic_planes(frame, true),
    )?;
    write_float_plane(
        &directory.join(format!("{name}-shadow-status.rgba32f")),
        &diagnostic_planes(frame, false),
    )
}

async fn run() -> Result<()> {
    let baseline_dir = PathBuf::from(BASELINE);
    let baseline_manifest: Value =
        serde_json::from_slice(&fs::read(baseline_dir.join("manifest.json"))?)?;
    let mut settings: Settings = serde_json::from_value(baseline_manifest["settings"].clone())?;
    settings.width = OUTPUT_WIDTH;
    settings.height = OUTPUT_HEIGHT;
    let edgefill = read_edgefill(&baseline_dir.join("edgefill.rgba32f"), settings.atlas)?;
    let source_mesh = procedural(
        settings.mesh,
        settings.around,
        settings.long,
        settings.length,
        settings.radius,
        settings.thickness,
    )?;
    let suggestion =
        suggest_seam_cut_for_hotspot(&source_mesh, &edgefill, settings.atlas, [0.5849, 0.6359])?
            .context("U1 hotspot did not produce a seam-cut suggestion")?;
    ensure!(
        suggestion.region_face_ids == [16] && suggestion.cut_edges == [[12, 13], [13, 15]],
        "the baseline hotspot no longer matches the pinned CubePinched seam suggestion"
    );

    let uv_bounds = region_uv_bounds(&source_mesh, &suggestion.region_face_ids);
    let mut reauthored_mesh = source_mesh.clone();
    let mut material = material::load(Path::new("demo/assets"), settings.texture_set)?;
    ensure!(
        material.height.width == material.albedo.width
            && material.height.height == material.albedo.height,
        "height and albedo dimensions must match for texel-preserving patch copy"
    );
    let (shift_x, shift_y) = UV_SHIFT_PIXELS;
    let delta_uv = [
        shift_x as f32 / material.height.width as f32,
        shift_y as f32 / material.height.height as f32,
    ];
    ensure!(
        uv_bounds[0] + delta_uv[0] >= 0.0
            && uv_bounds[1] + delta_uv[1] >= 0.0
            && uv_bounds[2] + delta_uv[0] <= 1.0
            && uv_bounds[3] + delta_uv[1] <= 1.0,
        "selected UV translation would move the suggested patch outside the atlas"
    );
    split_region(&mut reauthored_mesh, &suggestion.region_face_ids, delta_uv);
    let uv_overlap_texel_centers = verify_region_uv_separation(
        &reauthored_mesh,
        &suggestion.region_face_ids,
        settings.atlas,
    )?;
    move_texture_region(&mut material.height, uv_bounds, shift_x, shift_y);
    move_texture_region(&mut material.albedo, uv_bounds, shift_x, shift_y);

    let output_dir = PathBuf::from(OUTPUT);
    let material_root = output_dir.join("materials");
    let texture_dir = material_root.join("roof_3_4k.blend/textures");
    fs::create_dir_all(&texture_dir)?;
    write_png(&texture_dir.join("roof_3_disp_4k.png"), &material.height)?;
    // The loader detects the image format from the bytes; PNG preserves decoded source texels.
    write_png(&texture_dir.join("roof_3_diff_4k.jpg"), &material.albedo)?;

    let document = AssetDocument::new(reauthored_mesh)?;
    document.validate_pose_topology(&[glam::Mat4::IDENTITY; 4])?;
    document.save(&output_dir.join("reauthored.asset.json"))?;
    let baseline_session =
        DashrSession::headless(settings.clone(), "demo/assets", GpuOptions::default()).await?;
    let baseline_adapter = baseline_session.adapter_report().clone();
    let mut baseline_session = baseline_session;
    let baseline_frame = baseline_session.render()?;
    drop(baseline_session);

    let mut candidate_session = DashrSession::from_asset(
        settings.clone(),
        document,
        &material_root,
        GpuOptions::default(),
    )
    .await?;
    let candidate_adapter = candidate_session.adapter_report().clone();
    let candidate_bake_ms = candidate_session.bake_ms();
    let candidate_frame = candidate_session.render()?;
    drop(candidate_session);
    ensure!(
        baseline_adapter == candidate_adapter,
        "paired frames used different adapters"
    );

    let status_delta = baseline_frame
        .primary
        .iter()
        .zip(&candidate_frame.primary)
        .filter(|(baseline, candidate)| baseline.status != candidate.status)
        .count();
    let hit_delta = baseline_frame
        .primary
        .iter()
        .zip(&candidate_frame.primary)
        .filter(|(baseline, candidate)| {
            (baseline.status == TerminationStatus::Hit)
                != (candidate.status == TerminationStatus::Hit)
        })
        .count();
    let shadow_status_delta = baseline_frame
        .shadow
        .iter()
        .zip(&candidate_frame.shadow)
        .filter(|(baseline, candidate)| baseline.status != candidate.status)
        .count();
    let new_failures = baseline_frame
        .primary
        .iter()
        .zip(&candidate_frame.primary)
        .chain(baseline_frame.shadow.iter().zip(&candidate_frame.shadow))
        .filter(|(baseline, candidate)| {
            matches!(
                candidate.status,
                TerminationStatus::BudgetExhausted | TerminationStatus::InvalidBasis
            ) && baseline.status != candidate.status
        })
        .count();
    let mut color_errors: Vec<_> = baseline_frame
        .color
        .iter()
        .zip(&candidate_frame.color)
        .flat_map(|(baseline, candidate)| {
            (0..3).map(move |channel| (baseline[channel] - candidate[channel]).abs())
        })
        .collect();
    let max_color_error = color_errors.iter().copied().fold(0.0, f32::max);
    color_errors.sort_by(f32::total_cmp);
    let mean_color_error = color_errors.iter().sum::<f32>() / color_errors.len() as f32;
    let hotspot_index = (HOTSPOT_PIXEL.1 * settings.width + HOTSPOT_PIXEL.0) as usize;
    let anchor_seams: Vec<_> = suggestion
        .anchor_seams
        .iter()
        .map(|seam| {
            json!({
                "face_id": seam.face_id,
                "edge": seam.edge,
                "partner_face_id": seam.partner_face_id,
                "partner_edge": seam.partner_edge
            })
        })
        .collect();

    write_frame(&output_dir, "baseline", &baseline_frame)?;
    write_frame(&output_dir, "candidate", &candidate_frame)?;
    let report = json!({
        "status": "captured",
        "adapter": baseline_adapter,
        "settings": settings,
        "suggestion": {
            "hotspot_uv": suggestion.hotspot_uv,
            "source_uv": suggestion.source_uv,
            "hotspot_face_id": suggestion.hotspot_face_id,
            "region_face_ids": suggestion.region_face_ids,
            "cut_edges": suggestion.cut_edges,
            "anchor_seams": anchor_seams,
            "uv_bounds": uv_bounds,
            "uv_overlap_texel_centers": uv_overlap_texel_centers,
            "uv_shift_pixels": [shift_x, shift_y],
            "uv_shift": delta_uv
        },
        "baseline": frame_summary(&baseline_frame),
        "candidate": frame_summary(&candidate_frame),
        "deltas": {
            "primary_status_pixels": status_delta,
            "primary_hit_mask_pixels": hit_delta,
            "shadow_status_pixels": shadow_status_delta,
            "new_budget_or_invalid_exits": new_failures,
            "max_rgb_error": max_color_error,
            "mean_rgb_error": mean_color_error,
            "candidate_bake_ms": candidate_bake_ms,
            "baseline_hotspot": {
                "diagnostic": format!("{:?}", baseline_frame.primary[hotspot_index]),
                "uv_distance_depth": baseline_frame.hit_uv_distance_depth[hotspot_index]
            },
            "candidate_hotspot": {
                "diagnostic": format!("{:?}", candidate_frame.primary[hotspot_index]),
                "uv_distance_depth": candidate_frame.hit_uv_distance_depth[hotspot_index]
            }
        },
        "review": { "human_visual": "pending", "d3d11_parity": "unverified" }
    });
    fs::write(
        output_dir.join("manifest.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn main() -> Result<()> {
    pollster::block_on(run())
}
