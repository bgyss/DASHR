//! Reproduce the TubePinched U1 seam-cut candidate and compare host-visible frames.
use anyhow::{Context, Result, bail, ensure};
use dashr::{
    api::{DashrSession, FrameOutput, GpuOptions, TerminationStatus},
    asset::{Mesh, MeshKind, procedural},
    asset_diagnostics::suggest_seam_cut_for_hotspot,
    asset_format::AssetDocument,
    material::{self, Pixels},
    settings::Settings,
};
use glam::{DVec2, Mat4};
use image::{ColorType, ImageFormat};
use serde_json::{Value, json};
use std::{collections::BTreeSet, fs};

#[derive(Clone, Copy)]
enum HotspotCase {
    Face277Escape,
    Face261HighWork,
    Face261Minimal,
    Face261Near,
    Face261Wide,
    Face261Wider,
}

impl HotspotCase {
    fn parse() -> Result<Self> {
        match std::env::args().nth(1).as_deref() {
            None | Some("face277") => Ok(Self::Face277Escape),
            Some("face261") => Ok(Self::Face261HighWork),
            Some("face261-minimal") => Ok(Self::Face261Minimal),
            Some("face261-near") => Ok(Self::Face261Near),
            Some("face261-wide") => Ok(Self::Face261Wide),
            Some("face261-wider") => Ok(Self::Face261Wider),
            Some(value) => bail!(
                "unknown hotspot case {value:?}; choose face277, face261, face261-minimal, face261-near, face261-wide or face261-wider"
            ),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Face277Escape => "face277-escape",
            Self::Face261HighWork => "face261-high-work",
            Self::Face261Minimal => "face261-minimal",
            Self::Face261Near => "face261-near",
            Self::Face261Wide => "face261-wide",
            Self::Face261Wider => "face261-wider",
        }
    }

    fn hotspot_uv(self) -> [f32; 2] {
        match self {
            Self::Face277Escape => [0.7940, 0.1829],
            Self::Face261HighWork
            | Self::Face261Minimal
            | Self::Face261Near
            | Self::Face261Wide
            | Self::Face261Wider => [0.2492534, 0.1192122],
        }
    }

    fn source_pixel(self) -> (u32, u32) {
        match self {
            Self::Face277Escape => (46, 63),
            Self::Face261HighWork
            | Self::Face261Minimal
            | Self::Face261Near
            | Self::Face261Wide
            | Self::Face261Wider => (49, 63),
        }
    }

    fn region_rings(self) -> usize {
        match self {
            Self::Face277Escape => 2,
            Self::Face261HighWork => 1,
            Self::Face261Minimal => 0,
            Self::Face261Near => 1,
            Self::Face261Wide => 2,
            Self::Face261Wider => 3,
        }
    }

    fn prefer_near_shift(self) -> bool {
        matches!(self, Self::Face261Near)
    }
}

fn scale_pixel_center(
    source_pixel: (u32, u32),
    source_size: (u32, u32),
    output_size: (u32, u32),
) -> (u32, u32) {
    let normalized_x = (f64::from(source_pixel.0) + 0.5) / f64::from(source_size.0);
    let normalized_y = (f64::from(source_pixel.1) + 0.5) / f64::from(source_size.1);
    (
        (normalized_x * f64::from(output_size.0)).floor() as u32,
        (normalized_y * f64::from(output_size.1)).floor() as u32,
    )
}

fn uv_triangle_contains(mesh: &Mesh, face: usize, point: DVec2) -> bool {
    let triangle = &mesh.indices[face * 3..face * 3 + 3];
    let uv: [DVec2; 3] = std::array::from_fn(|corner| {
        let index = triangle[corner];
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

fn bounds_for_faces(mesh: &Mesh, faces: &[u32]) -> [f32; 4] {
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
    let mut remap = std::collections::BTreeMap::<u32, u32>::new();
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

fn grow_region(mesh: &Mesh, seed_face: u32, rings: usize) -> Vec<u32> {
    let mut edge_faces = std::collections::BTreeMap::<[u32; 2], Vec<u32>>::new();
    for (face, triangle) in mesh.indices.chunks_exact(3).enumerate() {
        for (a, b) in [
            (triangle[0], triangle[1]),
            (triangle[1], triangle[2]),
            (triangle[2], triangle[0]),
        ] {
            edge_faces
                .entry([a.min(b), a.max(b)])
                .or_default()
                .push(face as u32);
        }
    }
    let mut region = BTreeSet::from([seed_face]);
    let mut frontier = vec![seed_face];
    for _ in 0..rings {
        let mut next = Vec::new();
        for face in frontier {
            let triangle = &mesh.indices[face as usize * 3..face as usize * 3 + 3];
            for (a, b) in [
                (triangle[0], triangle[1]),
                (triangle[1], triangle[2]),
                (triangle[2], triangle[0]),
            ] {
                for neighbour in edge_faces.get(&[a.min(b), a.max(b)]).into_iter().flatten() {
                    if region.insert(*neighbour) {
                        next.push(*neighbour);
                    }
                }
            }
        }
        frontier = next;
    }
    region.into_iter().collect()
}

fn find_free_shift(
    mesh: &Mesh,
    region_faces: &[u32],
    atlas: u32,
    prefer_near: bool,
) -> Option<(i32, i32)> {
    let region: BTreeSet<_> = region_faces.iter().map(|face| *face as usize).collect();
    let region_cells: Vec<_> = (0..atlas)
        .flat_map(|y| (0..atlas).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            let point = DVec2::new(
                (f64::from(x) + 0.5) / f64::from(atlas),
                (f64::from(y) + 0.5) / f64::from(atlas),
            );
            region
                .iter()
                .any(|&face| uv_triangle_contains(mesh, face, point))
        })
        .collect();
    if region_cells.is_empty() {
        return None;
    }
    let mut other_occupied = vec![false; (atlas * atlas) as usize];
    for face in (0..mesh.indices.len() / 3).filter(|face| !region.contains(face)) {
        for y in 0..atlas {
            for x in 0..atlas {
                let point = DVec2::new(
                    (f64::from(x) + 0.5) / f64::from(atlas),
                    (f64::from(y) + 0.5) / f64::from(atlas),
                );
                if uv_triangle_contains(mesh, face, point) {
                    other_occupied[(y * atlas + x) as usize] = true;
                }
            }
        }
    }
    let bounds = bounds_for_faces(mesh, region_faces);
    let margin = 1.0 / atlas as f32;
    let mut free_shifts = Vec::new();
    for dy in (-(atlas as i32)..=(atlas as i32)).rev() {
        for dx in -(atlas as i32)..=(atlas as i32) {
            if dx.abs() + dy.abs() < 2 {
                continue;
            }
            let du = dx as f32 / atlas as f32;
            let dv = dy as f32 / atlas as f32;
            let moved = [
                bounds[0] + du,
                bounds[1] + dv,
                bounds[2] + du,
                bounds[3] + dv,
            ];
            if moved[0] < margin
                || moved[1] < margin
                || moved[2] > 1.0 - margin
                || moved[3] > 1.0 - margin
            {
                continue;
            }
            let clear = region_cells.iter().all(|&(x, y)| {
                let tx = x as i32 + dx;
                let ty = y as i32 + dy;
                (-1..=1).all(|oy| {
                    (-1..=1).all(|ox| {
                        let nx = tx + ox;
                        let ny = ty + oy;
                        nx >= 0
                            && ny >= 0
                            && nx < atlas as i32
                            && ny < atlas as i32
                            && !other_occupied[(ny as u32 * atlas + nx as u32) as usize]
                    })
                })
            });
            if clear {
                if !prefer_near {
                    return Some((dx, dy));
                }
                free_shifts.push((dx, dy));
            }
        }
    }
    free_shifts
        .into_iter()
        .min_by_key(|(dx, dy)| (dx.abs() + dy.abs(), dy.abs(), dx.abs(), *dy, *dx))
}

fn authored_cut_edges(mesh: &Mesh, region_faces: &[u32]) -> Vec<[u32; 2]> {
    let region: BTreeSet<_> = region_faces.iter().map(|face| *face as usize).collect();
    let mut edge_faces = std::collections::BTreeMap::<[u32; 2], Vec<usize>>::new();
    for (face, triangle) in mesh.indices.chunks_exact(3).enumerate() {
        for (a, b) in [
            (triangle[0], triangle[1]),
            (triangle[1], triangle[2]),
            (triangle[2], triangle[0]),
        ] {
            edge_faces
                .entry([a.min(b), a.max(b)])
                .or_default()
                .push(face);
        }
    }
    edge_faces
        .into_iter()
        .filter_map(|(edge, faces)| {
            (faces.len() == 2 && (region.contains(&faces[0]) != region.contains(&faces[1])))
                .then_some(edge)
        })
        .collect()
}

fn count_region_overlaps(mesh: &Mesh, region_faces: &[u32], atlas: u32) -> usize {
    let region: BTreeSet<_> = region_faces.iter().map(|face| *face as usize).collect();
    let mut overlaps = 0;
    for y in 0..atlas {
        for x in 0..atlas {
            let point = DVec2::new(
                (f64::from(x) + 0.5) / f64::from(atlas),
                (f64::from(y) + 0.5) / f64::from(atlas),
            );
            let patch_covered = region
                .iter()
                .any(|&face| uv_triangle_contains(mesh, face, point));
            if patch_covered
                && (0..mesh.indices.len() / 3)
                    .filter(|face| !region.contains(face))
                    .any(|face| uv_triangle_contains(mesh, face, point))
            {
                overlaps += 1;
            }
        }
    }
    overlaps
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
            if (0..width).contains(&target_x) && (0..height).contains(&target_y) {
                let from = (y as usize * width as usize + x as usize) * 4;
                let to = (target_y as usize * width as usize + target_x as usize) * 4;
                pixels.rgba[to..to + 4].copy_from_slice(&source[from..from + 4]);
            }
        }
    }
}

fn save_frame(path: &std::path::Path, frame: &FrameOutput) -> Result<()> {
    let rgba: Vec<u8> = frame
        .color
        .iter()
        .flat_map(|pixel| {
            [pixel[0], pixel[1], pixel[2], 1.0]
                .map(|value| (value.clamp(0.0, 1.0) * 255.0 + 0.5).floor() as u8)
        })
        .collect();
    image::save_buffer(path, &rgba, frame.width, frame.height, ColorType::Rgba8)?;
    Ok(())
}

fn counts(frame: &FrameOutput, primary: bool) -> Value {
    let traces = if primary {
        &frame.primary
    } else {
        &frame.shadow
    };
    let mut result = [0usize; 6];
    for trace in traces {
        let code = match trace.status {
            TerminationStatus::NotLaunched => 0,
            TerminationStatus::Hit => 1,
            TerminationStatus::Escaped => 2,
            TerminationStatus::BudgetExhausted => 3,
            TerminationStatus::InvalidBasis => 4,
            TerminationStatus::DebugForcedHit => 5,
        };
        result[code] += 1;
    }
    json!({
        "not_launched": result[0], "hit": result[1], "escaped": result[2],
        "budget": result[3], "invalid": result[4], "debug_forced_hit": result[5]
    })
}

fn main() -> Result<()> {
    let case = HotspotCase::parse()?;
    let repo = std::env::current_dir()?;
    let baseline = repo.join("out/u1-baseline/tube-pinched");
    let manifest: Value = serde_json::from_slice(&fs::read(baseline.join("manifest.json"))?)?;
    let mut settings: Settings = serde_json::from_value(manifest["settings"].clone())?;
    let baseline_size = (settings.width, settings.height);
    let mesh = procedural(
        MeshKind::TubePinched,
        settings.around,
        settings.long,
        settings.length,
        settings.radius,
        settings.thickness,
    )?;
    let bytes = fs::read(baseline.join("edgefill.rgba32f"))?;
    ensure!(bytes.len() == (settings.atlas * settings.atlas * 16) as usize);
    let edgefill: Vec<[f32; 4]> = bytes
        .chunks_exact(16)
        .map(|pixel| {
            std::array::from_fn(|channel| {
                f32::from_le_bytes(pixel[channel * 4..channel * 4 + 4].try_into().unwrap())
            })
        })
        .collect();
    let hotspot_uv = case.hotspot_uv();
    let suggestion = suggest_seam_cut_for_hotspot(&mesh, &edgefill, settings.atlas, hotspot_uv)?
        .context("TubePinched hotspot did not produce a seam-cut suggestion")?;
    let region_faces = grow_region(&mesh, suggestion.hotspot_face_id, case.region_rings());
    ensure!(
        region_faces.contains(&suggestion.hotspot_face_id),
        "expanded U1 patch no longer contains the suggested hotspot face"
    );
    let bounds = bounds_for_faces(&mesh, &region_faces);
    let shift = find_free_shift(
        &mesh,
        &region_faces,
        settings.atlas,
        case.prefer_near_shift(),
    )
    .context("could not place TubePinched seam cut in an unused atlas region")?;
    let delta_uv = [
        shift.0 as f32 / settings.atlas as f32,
        shift.1 as f32 / settings.atlas as f32,
    ];
    let mut material = material::load(std::path::Path::new("demo/assets"), settings.texture_set)?;
    ensure!(material.height.width == material.albedo.width);
    ensure!(material.height.height == material.albedo.height);
    let texels_per_atlas = material.height.width / settings.atlas;
    ensure!(material.height.width.is_multiple_of(settings.atlas));
    let shift_pixels = (
        shift.0 * texels_per_atlas as i32,
        shift.1 * texels_per_atlas as i32,
    );
    move_texture_region(&mut material.height, bounds, shift_pixels.0, shift_pixels.1);
    move_texture_region(&mut material.albedo, bounds, shift_pixels.0, shift_pixels.1);
    let mut reauthored = mesh.clone();
    split_region(&mut reauthored, &region_faces, delta_uv);
    let uv_overlap_texel_centers =
        count_region_overlaps(&reauthored, &region_faces, settings.atlas);
    ensure!(
        uv_overlap_texel_centers == 0,
        "reauthored TubePinched patch overlaps at {uv_overlap_texel_centers} atlas texel centers"
    );
    let authored_cut_edges = authored_cut_edges(&mesh, &region_faces);
    let document = AssetDocument::new(reauthored)?;
    document.validate_pose_topology(&[Mat4::IDENTITY; 4])?;
    let output = repo.join(format!(
        "out/u1-reauthored/tube-pinched-{}-640",
        case.label()
    ));
    let texture_dir = output.join("materials/roof_3_4k.blend/textures");
    fs::create_dir_all(&texture_dir)?;
    document.save(&output.join("reauthored.asset.json"))?;
    image::save_buffer_with_format(
        texture_dir.join("roof_3_disp_4k.png"),
        &material.height.rgba,
        material.height.width,
        material.height.height,
        ColorType::Rgba8,
        ImageFormat::Png,
    )?;
    image::save_buffer_with_format(
        texture_dir.join("roof_3_diff_4k.jpg"),
        &material.albedo.rgba,
        material.albedo.width,
        material.albedo.height,
        ColorType::Rgba8,
        ImageFormat::Png,
    )?;

    settings.width = 640;
    settings.height = 480;
    let baseline_session = pollster::block_on(DashrSession::headless(
        settings.clone(),
        "demo/assets",
        GpuOptions::default(),
    ))?;
    let adapter = baseline_session.adapter_report().clone();
    let mut baseline_session = baseline_session;
    let baseline_frame = baseline_session.render()?;
    drop(baseline_session);
    let mut candidate_session = pollster::block_on(DashrSession::from_asset(
        settings.clone(),
        document,
        output.join("materials"),
        GpuOptions::default(),
    ))?;
    ensure!(candidate_session.adapter_report() == &adapter);
    let candidate_bake_ms = candidate_session.bake_ms();
    let candidate_frame = candidate_session.render()?;
    save_frame(&output.join("baseline.png"), &baseline_frame)?;
    save_frame(&output.join("candidate.png"), &candidate_frame)?;

    let primary_status_pixels = baseline_frame
        .primary
        .iter()
        .zip(&candidate_frame.primary)
        .filter(|(a, b)| a.status != b.status)
        .count();
    let shadow_status_pixels = baseline_frame
        .shadow
        .iter()
        .zip(&candidate_frame.shadow)
        .filter(|(a, b)| a.status != b.status)
        .count();
    let new_budget_or_invalid_exits = baseline_frame
        .primary
        .iter()
        .zip(&candidate_frame.primary)
        .chain(baseline_frame.shadow.iter().zip(&candidate_frame.shadow))
        .filter(|(a, b)| {
            matches!(
                b.status,
                TerminationStatus::BudgetExhausted | TerminationStatus::InvalidBasis
            ) && a.status != b.status
        })
        .count();
    let max_rgb_error = baseline_frame
        .color
        .iter()
        .zip(&candidate_frame.color)
        .flat_map(|(a, b)| (0..3).map(move |channel| (a[channel] - b[channel]).abs()))
        .fold(0.0_f32, f32::max);
    let output_pixel = scale_pixel_center(
        case.source_pixel(),
        baseline_size,
        (settings.width, settings.height),
    );
    let hotspot_index = (output_pixel.1 * settings.width + output_pixel.0) as usize;
    let report = json!({
        "status": "captured",
        "case": case.label(),
        "adapter": adapter,
        "settings": settings,
        "hotspot_uv": suggestion.hotspot_uv,
        "source_hotspot_pixel": case.source_pixel(),
        "output_hotspot_pixel": output_pixel,
        "hotspot_face_id": suggestion.hotspot_face_id,
        "suggested_region_face_ids": suggestion.region_face_ids,
        "authored_region_face_ids": region_faces,
        "suggested_cut_edges": suggestion.cut_edges,
        "authored_cut_edges": authored_cut_edges,
        "region_growth_steps": case.region_rings(),
        "region_uv_bounds": bounds,
        "uv_overlap_texel_centers": uv_overlap_texel_centers,
        "shift_atlas_cells": shift,
        "shift_pixels": shift_pixels,
        "delta_uv": delta_uv,
        "baseline": {
            "primary": counts(&baseline_frame, true),
            "shadow": counts(&baseline_frame, false),
            "hotspot_trace": format!("{:?}", baseline_frame.primary[hotspot_index]),
            "hotspot_uv_distance_depth": baseline_frame.hit_uv_distance_depth[hotspot_index]
        },
        "candidate": {
            "primary": counts(&candidate_frame, true),
            "shadow": counts(&candidate_frame, false),
            "hotspot_trace": format!("{:?}", candidate_frame.primary[hotspot_index]),
            "hotspot_uv_distance_depth": candidate_frame.hit_uv_distance_depth[hotspot_index]
        },
        "deltas": {
            "primary_status_pixels": primary_status_pixels,
            "shadow_status_pixels": shadow_status_pixels,
            "new_budget_or_invalid_exits": new_budget_or_invalid_exits,
            "max_rgb_error": max_rgb_error,
            "candidate_bake_ms": candidate_bake_ms
        },
        "human_visual": "pending"
    });
    fs::write(
        output.join("manifest.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
