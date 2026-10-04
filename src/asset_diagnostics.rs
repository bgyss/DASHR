//! Face-level metric distortion diagnostics for authoring tools and importers.
use crate::{asset::Mesh, asset_format::AssetDocument, topology::seam_pairs};
use anyhow::{Result, ensure};
use glam::{DVec2, DVec3, Mat4, Vec4};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DistortionWarning {
    /// Zero-based triangle index in the asset's index buffer.
    pub face_id: u32,
    /// Deterministic connected component id derived from shared indexed edges.
    pub chart_id: u32,
    /// Ratio between the largest and smallest singular values of the face's metric basis.
    pub condition_number: f32,
    /// Largest singular value, retained separately because metric scale matters to DASHR.
    pub max_metric_scale: f32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeamAnchor {
    /// Face in the suggested region that owns this already-separated seam.
    pub face_id: u32,
    /// Edge on the suggested-region side, in face winding order.
    pub edge: [u32; 2],
    /// Face across the existing seam.
    pub partner_face_id: u32,
    /// Partner edge ordered to match the physical endpoints of `edge`.
    pub partner_edge: [u32; 2],
}

#[derive(Clone, Debug, PartialEq)]
pub struct SeamCutSuggestion {
    /// Escaped or high-work UV reported by the renderer.
    pub hotspot_uv: [f32; 2],
    /// Source UV after bilinear sampling the renderer's edgefill map.
    pub source_uv: [f32; 2],
    /// Barycentric location of `source_uv` in the reported source face.
    pub hotspot_barycentric: [f32; 3],
    /// Face containing the runtime hotspot after resolving its edgefill source UV.
    pub hotspot_face_id: u32,
    /// Connected indexed-edge chart containing the hotspot.
    pub chart_id: u32,
    /// Connected face region to isolate, ordered from the hotspot toward an existing seam.
    pub region_face_ids: Vec<u32>,
    /// Shared edges between the region and surrounding mesh that must be split.
    /// Each pair is sorted by vertex ID for deterministic reporting.
    pub cut_edges: Vec<[u32; 2]>,
    /// Existing seams along the region boundary that anchor the new cut.
    pub anchor_seams: Vec<SeamAnchor>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelfIntersectionWarning {
    pub face_a: u32,
    pub face_b: u32,
    pub chart_a: u32,
    pub chart_b: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TJunctionWarning {
    pub vertex_id: u32,
    pub edge_face_id: u32,
    pub edge_vertices: [u32; 2],
    pub chart_id: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrientationFlipWarning {
    /// Zero-based triangle index in the asset's index buffer.
    pub face_id: u32,
    /// Deterministic connected component id derived from shared indexed edges.
    pub chart_id: u32,
    /// Signed posed-to-rest oriented-area ratio; nonpositive values are flips/degeneracies.
    pub oriented_area_ratio: f32,
}

struct PosedTriangle {
    face_id: usize,
    chart_id: u32,
    points: [DVec3; 3],
    min: DVec3,
    max: DVec3,
}

/// Reports faces whose tangent basis exceeds the caller's anisotropy threshold.
///
/// The threshold is explicit because valid ranges depend on asset scale and authoring policy.
/// This diagnostic never changes step budgets or alters metric tangents to hide a pole.
pub fn find_high_distortion_faces(
    mesh: &Mesh,
    max_condition_number: f32,
) -> Result<Vec<DistortionWarning>> {
    ensure!(
        max_condition_number.is_finite() && max_condition_number > 1.0,
        "maximum metric condition number must be finite and greater than one"
    );
    AssetDocument::new(mesh.clone())?.validate()?;
    let charts = chart_ids(mesh);
    let mut warnings = Vec::new();
    for (face_index, triangle) in mesh.indices.chunks_exact(3).enumerate() {
        let (condition_number, max_metric_scale) = face_condition(mesh, triangle);
        if condition_number >= max_condition_number {
            warnings.push(DistortionWarning {
                face_id: face_index as u32,
                chart_id: charts[face_index],
                condition_number,
                max_metric_scale,
            });
        }
    }
    Ok(warnings)
}

/// Maps a runtime hotspot through edgefill and suggests a face region to isolate with new seams.
///
/// The edgefill map must be the row-major RGBA32F map produced by `topology::bake`, at the same
/// atlas resolution used for the capture. `region_face_ids` is the shortest face path from the
/// hotspot to an existing seam; `cut_edges` is the full shared-edge boundary of that region, so
/// splitting those edges isolates it in the face-adjacency graph. Existing seam pairs in
/// `anchor_seams` remain as anchors. The result is an authoring suggestion only; callers still
/// need to duplicate vertices, assign a non-overlapping UV chart, and rebake material maps before
/// rendering it. Hotspot UVs use clamp-to-edge sampling. An unmapped UV or a patch with no isolating
/// cut returns `None`. No geometry or renderer settings change.
pub fn suggest_seam_cut_for_hotspot(
    mesh: &Mesh,
    edgefill: &[[f32; 4]],
    atlas_size: u32,
    hotspot_uv: [f32; 2],
) -> Result<Option<SeamCutSuggestion>> {
    AssetDocument::new(mesh.clone())?.validate()?;
    ensure!(
        (16..=1024).contains(&atlas_size),
        "invalid edgefill atlas resolution {atlas_size}"
    );
    let expected_len = (atlas_size as usize)
        .checked_mul(atlas_size as usize)
        .ok_or_else(|| anyhow::anyhow!("edgefill atlas size overflows address space"))?;
    ensure!(
        edgefill.len() == expected_len,
        "edgefill map has {} texels, expected {expected_len}",
        edgefill.len()
    );
    ensure!(
        hotspot_uv.iter().all(|value| value.is_finite()),
        "hotspot UV must be finite"
    );
    ensure!(
        edgefill.iter().all(|texel| {
            texel.iter().all(|value| value.is_finite())
                && (0.0..=1.0).contains(&texel[0])
                && (0.0..=1.0).contains(&texel[1])
        }),
        "edgefill map contains invalid source UVs or non-finite values"
    );

    let source_uv = sample_edgefill_source(edgefill, atlas_size, hotspot_uv);
    let Some((hotspot_face_id, hotspot_barycentric)) = locate_uv_face(mesh, source_uv) else {
        return Ok(None);
    };
    let Some(path) = seam_cut_path_from_face(mesh, hotspot_face_id)? else {
        return Ok(None);
    };
    Ok(Some(SeamCutSuggestion {
        hotspot_uv,
        source_uv,
        hotspot_barycentric,
        hotspot_face_id: path.hotspot_face_id,
        chart_id: path.chart_id,
        region_face_ids: path.region_face_ids,
        cut_edges: path.cut_edges,
        anchor_seams: path.anchor_seams,
    }))
}

fn sample_edgefill_source(edgefill: &[[f32; 4]], atlas_size: u32, uv: [f32; 2]) -> [f32; 2] {
    let axis = |coordinate: f32| {
        let texel = coordinate.clamp(0.0, 1.0) * atlas_size as f32 - 0.5;
        let lower_raw = texel.floor() as i32;
        let fraction = texel - lower_raw as f32;
        let lower = lower_raw.clamp(0, atlas_size as i32 - 1) as u32;
        let upper = (lower_raw + 1).clamp(0, atlas_size as i32 - 1) as u32;
        (lower, upper, fraction)
    };
    let (x0, x1, tx) = axis(uv[0]);
    let (y0, y1, ty) = axis(uv[1]);
    let sample = |x: u32, y: u32| {
        let texel = edgefill[(y * atlas_size + x) as usize];
        [texel[0], texel[1]]
    };
    let top_left = sample(x0, y0);
    let top_right = sample(x1, y0);
    let bottom_left = sample(x0, y1);
    let bottom_right = sample(x1, y1);
    std::array::from_fn(|channel| {
        let top = top_left[channel] * (1.0 - tx) + top_right[channel] * tx;
        let bottom = bottom_left[channel] * (1.0 - tx) + bottom_right[channel] * tx;
        top * (1.0 - ty) + bottom * ty
    })
}

fn locate_uv_face(mesh: &Mesh, source_uv: [f32; 2]) -> Option<(u32, [f32; 3])> {
    let point = DVec2::new(f64::from(source_uv[0]), f64::from(source_uv[1]));
    for (face_id, triangle) in mesh.indices.chunks_exact(3).enumerate() {
        let uv = [triangle[0], triangle[1], triangle[2]].map(|index| {
            let value = mesh.vertices[index as usize].uv;
            DVec2::new(f64::from(value[0]), f64::from(value[1]))
        });
        let edge_a = uv[1] - uv[0];
        let edge_b = uv[2] - uv[0];
        let relative = point - uv[0];
        let determinant = edge_a.perp_dot(edge_b);
        if determinant.abs() <= f64::EPSILON {
            continue;
        }
        let weight_b = relative.perp_dot(edge_b) / determinant;
        let weight_c = edge_a.perp_dot(relative) / determinant;
        let weights = [1.0 - weight_b - weight_c, weight_b, weight_c];
        if weights
            .iter()
            .all(|weight| *weight >= -1e-5 && *weight <= 1.00001)
        {
            return Some((face_id as u32, weights.map(|weight| weight as f32)));
        }
    }
    None
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SeamCutPath {
    hotspot_face_id: u32,
    chart_id: u32,
    region_face_ids: Vec<u32>,
    cut_edges: Vec<[u32; 2]>,
    anchor_seams: Vec<SeamAnchor>,
}

fn seam_cut_path_from_face(mesh: &Mesh, hotspot_face_id: u32) -> Result<Option<SeamCutPath>> {
    let face_count = mesh.indices.len() / 3;
    ensure!(
        (hotspot_face_id as usize) < face_count,
        "hotspot face {hotspot_face_id} is outside the mesh"
    );

    let seams = seam_pairs(mesh)?;
    let mut seams_by_face = vec![Vec::new(); face_count];
    for seam in seams {
        seams_by_face[seam.face].push(seam);
    }
    let source_face = hotspot_face_id as usize;

    let mut edge_faces: BTreeMap<(u32, u32), Vec<usize>> = BTreeMap::new();
    for (face, triangle) in mesh.indices.chunks_exact(3).enumerate() {
        for (a, b) in [
            (triangle[0], triangle[1]),
            (triangle[1], triangle[2]),
            (triangle[2], triangle[0]),
        ] {
            edge_faces
                .entry((a.min(b), a.max(b)))
                .or_default()
                .push(face);
        }
    }

    let mut adjacency = vec![Vec::<(usize, [u32; 2])>::new(); face_count];
    for (&(a, b), faces) in &edge_faces {
        ensure!(
            faces.len() <= 2,
            "non-manifold edge {a}-{b} touches {} faces",
            faces.len()
        );
        if faces.len() == 2 {
            adjacency[faces[0]].push((faces[1], [a, b]));
            adjacency[faces[1]].push((faces[0], [a, b]));
        }
    }
    for neighbours in &mut adjacency {
        neighbours.sort_by_key(|(face, edge)| (*face, *edge));
    }

    let mut visited = vec![false; face_count];
    let mut previous = vec![None::<usize>; face_count];
    let mut pending = VecDeque::new();
    visited[source_face] = true;
    pending.push_back(source_face);

    let mut terminal_face = None;
    while let Some(face) = pending.pop_front() {
        if !seams_by_face[face].is_empty() {
            terminal_face = Some(face);
            break;
        }
        for &(next, _) in &adjacency[face] {
            if !visited[next] {
                visited[next] = true;
                previous[next] = Some(face);
                pending.push_back(next);
            }
        }
    }

    let Some(terminal_face) = terminal_face else {
        return Ok(None);
    };
    let mut region_face_ids = vec![terminal_face as u32];
    let mut face = terminal_face;
    while face != source_face {
        face = previous[face].expect("every reached non-source face has a predecessor");
        region_face_ids.push(face as u32);
    }
    region_face_ids.reverse();

    let region: BTreeSet<_> = region_face_ids.iter().map(|face| *face as usize).collect();
    let mut cut_edges = Vec::new();
    for ((a, b), faces) in &edge_faces {
        if faces.len() == 2 && (region.contains(&faces[0]) != region.contains(&faces[1])) {
            cut_edges.push([*a, *b]);
        }
    }
    cut_edges.sort_unstable();
    cut_edges.dedup();
    if cut_edges.is_empty() {
        return Ok(None);
    }

    let mut anchors_by_position = BTreeMap::<[usize; 2], SeamAnchor>::new();
    for &face in &region {
        for seam in &seams_by_face[face] {
            anchors_by_position
                .entry(seam.position_key)
                .or_insert_with(|| SeamAnchor {
                    face_id: seam.face as u32,
                    edge: seam.edge.map(|vertex| vertex as u32),
                    partner_face_id: seam.partner_face as u32,
                    partner_edge: seam.partner_edge.map(|vertex| vertex as u32),
                });
        }
    }

    let charts = chart_ids(mesh);
    Ok(Some(SeamCutPath {
        hotspot_face_id,
        chart_id: charts[source_face],
        region_face_ids,
        cut_edges,
        anchor_seams: anchors_by_position.into_values().collect(),
    }))
}

/// Finds triangle intersections, including coplanar interior overlap, after a four-bone pose.
///
/// This is an authoring diagnostic, not a continuous collision detector. It returns an error if
/// the broadphase exceeds its pair budget. Shared-point or shared-edge contact alone is not an
/// overlap, but pairs that share geometric vertices are still checked for interior intersection.
pub fn find_self_intersections(
    mesh: &Mesh,
    bones: &[Mat4; 4],
) -> Result<Vec<SelfIntersectionWarning>> {
    AssetDocument::new(mesh.clone())?;
    ensure!(
        bones
            .iter()
            .flat_map(Mat4::to_cols_array)
            .all(f32::is_finite),
        "pose contains a non-finite bone matrix"
    );
    let charts = chart_ids(mesh);
    let mut posed = Vec::with_capacity(mesh.indices.len() / 3);
    let mut global_min = DVec3::splat(f64::INFINITY);
    let mut global_max = DVec3::splat(f64::NEG_INFINITY);
    for (face_id, triangle) in mesh.indices.chunks_exact(3).enumerate() {
        let points = std::array::from_fn(|corner| {
            let index = triangle[corner];
            let vertex = &mesh.vertices[index as usize];
            let position = Vec4::new(
                vertex.position[0],
                vertex.position[1],
                vertex.position[2],
                1.0,
            );
            let skinned = (0..4).fold(Vec4::ZERO, |sum, bone| {
                sum + bones[bone] * position * vertex.weights[bone]
            });
            DVec3::new(
                f64::from(skinned.x),
                f64::from(skinned.y),
                f64::from(skinned.z),
            )
        });
        ensure!(
            points.iter().all(|point| point.is_finite()),
            "pose produced a non-finite position on face {face_id}"
        );
        let min = points[0].min(points[1]).min(points[2]);
        let max = points[0].max(points[1]).max(points[2]);
        global_min = global_min.min(min);
        global_max = global_max.max(max);
        posed.push(PosedTriangle {
            face_id,
            chart_id: charts[face_id],
            points,
            min,
            max,
        });
    }
    let epsilon = (global_max - global_min).length().max(1.0) * 1e-7;
    let mut order: Vec<_> = (0..posed.len()).collect();
    order.sort_by(|&left, &right| posed[left].min.x.total_cmp(&posed[right].min.x));
    let mut warnings = Vec::new();
    let mut candidate_pairs = 0usize;
    for order_index in 0..order.len() {
        let a = &posed[order[order_index]];
        for &right_index in &order[order_index + 1..] {
            let b = &posed[right_index];
            if b.min.x > a.max.x + epsilon {
                break;
            }
            candidate_pairs += 1;
            ensure!(
                candidate_pairs <= 5_000_000,
                "self-intersection broadphase exceeded 5,000,000 candidate pairs"
            );
            if !bounds_overlap(a, b, epsilon) {
                continue;
            }
            let intersects = if shares_vertex(a, b, epsilon) {
                let (a_points, b_points) = align_shared_vertices(a.points, b.points, epsilon);
                triangles_intersect_non_coplanar(a_points, b_points, epsilon)
                    || coplanar_triangles_overlap_area(a_points, b_points, epsilon)
            } else {
                triangles_intersect(a.points, b.points, epsilon)
            };
            if intersects {
                let (left, right) = if a.face_id < b.face_id {
                    (a, b)
                } else {
                    (b, a)
                };
                warnings.push(SelfIntersectionWarning {
                    face_a: left.face_id as u32,
                    face_b: right.face_id as u32,
                    chart_a: left.chart_id,
                    chart_b: right.chart_id,
                });
            }
        }
    }
    warnings.sort_by_key(|warning| (warning.face_a, warning.face_b));
    Ok(warnings)
}

/// Finds vertices that lie in the interior of a non-incident triangle edge.
///
/// Tolerance is `1e-6` times the mesh's object-space bounding-box diagonal. The pair budget
/// keeps importer diagnostics bounded; oversized requests return an error instead of truncating.
pub fn find_t_junctions(mesh: &Mesh) -> Result<Vec<TJunctionWarning>> {
    AssetDocument::new(mesh.clone())?;
    let positions: Vec<_> = mesh
        .vertices
        .iter()
        .map(|vertex| {
            DVec3::new(
                f64::from(vertex.position[0]),
                f64::from(vertex.position[1]),
                f64::from(vertex.position[2]),
            )
        })
        .collect();
    let mut minimum = DVec3::splat(f64::INFINITY);
    let mut maximum = DVec3::splat(f64::NEG_INFINITY);
    for &position in &positions {
        minimum = minimum.min(position);
        maximum = maximum.max(position);
    }
    let epsilon = (maximum - minimum).length().max(1e-9) * 1e-6;
    let edge_count = (mesh.indices.len() / 3).saturating_mul(3);
    let candidate_pairs = positions.len().checked_mul(edge_count).ok_or_else(|| {
        anyhow::anyhow!("T-junction candidate count overflows addressable memory")
    })?;
    ensure!(
        candidate_pairs <= 50_000_000,
        "T-junction preflight exceeds 50,000,000 vertex-edge pairs"
    );
    let charts = chart_ids(mesh);
    let mut warnings = BTreeSet::new();
    for (vertex_id, point) in positions.iter().enumerate() {
        for (face_id, triangle) in mesh.indices.chunks_exact(3).enumerate() {
            if triangle.contains(&(vertex_id as u32)) {
                continue;
            }
            for (a, b) in [
                (triangle[0], triangle[1]),
                (triangle[1], triangle[2]),
                (triangle[2], triangle[0]),
            ] {
                let start = positions[a as usize];
                let end = positions[b as usize];
                let edge = end - start;
                let length_squared = edge.length_squared();
                if length_squared <= epsilon * epsilon {
                    continue;
                }
                let parameter = (*point - start).dot(edge) / length_squared;
                let end_margin = epsilon / length_squared.sqrt();
                if parameter <= end_margin || parameter >= 1.0 - end_margin {
                    continue;
                }
                let distance = (*point - (start + edge * parameter)).length();
                if distance <= epsilon {
                    warnings.insert((vertex_id as u32, face_id as u32, a.min(b), a.max(b)));
                }
            }
        }
    }
    Ok(warnings
        .into_iter()
        .map(
            |(vertex_id, edge_face_id, edge_a, edge_b)| TJunctionWarning {
                vertex_id,
                edge_face_id,
                edge_vertices: [edge_a, edge_b],
                chart_id: charts[edge_face_id as usize],
            },
        )
        .collect())
}

/// Reports posed triangles whose oriented area has flipped or collapsed relative to rest.
///
/// This is a face-local warning, not a proof that the surface-space map is globally folded.
pub fn find_orientation_flips(
    mesh: &Mesh,
    bones: &[Mat4; 4],
) -> Result<Vec<OrientationFlipWarning>> {
    AssetDocument::new(mesh.clone())?.validate()?;
    ensure!(
        bones
            .iter()
            .flat_map(Mat4::to_cols_array)
            .all(f32::is_finite),
        "pose contains a non-finite bone matrix"
    );
    let charts = chart_ids(mesh);
    let mut warnings = Vec::new();
    for (face_id, triangle) in mesh.indices.chunks_exact(3).enumerate() {
        let rest: [DVec3; 3] = std::array::from_fn(|corner| {
            DVec3::from_array(
                mesh.vertices[triangle[corner] as usize]
                    .position
                    .map(f64::from),
            )
        });
        let posed: [DVec3; 3] = std::array::from_fn(|corner| {
            let vertex = &mesh.vertices[triangle[corner] as usize];
            let position = Vec4::new(
                vertex.position[0],
                vertex.position[1],
                vertex.position[2],
                1.0,
            );
            let skinned = (0..4).fold(Vec4::ZERO, |sum, bone| {
                sum + bones[bone] * position * vertex.weights[bone]
            });
            DVec3::new(
                f64::from(skinned.x),
                f64::from(skinned.y),
                f64::from(skinned.z),
            )
        });
        ensure!(
            posed.iter().all(|point| point.is_finite()),
            "pose produced a non-finite position on face {face_id}"
        );
        let rest_normal = (rest[1] - rest[0]).cross(rest[2] - rest[0]);
        let rest_area2 = rest_normal.length_squared();
        ensure!(
            rest_area2 > 1e-24,
            "rest face {face_id} has degenerate geometric area"
        );
        let posed_normal = (posed[1] - posed[0]).cross(posed[2] - posed[0]);
        let ratio = posed_normal.dot(rest_normal) / rest_area2;
        if ratio <= 0.0 {
            warnings.push(OrientationFlipWarning {
                face_id: face_id as u32,
                chart_id: charts[face_id],
                oriented_area_ratio: ratio.clamp(-(f32::MAX as f64), f32::MAX as f64) as f32,
            });
        }
    }
    Ok(warnings)
}

fn bounds_overlap(a: &PosedTriangle, b: &PosedTriangle, epsilon: f64) -> bool {
    (0..3).all(|axis| a.min[axis] <= b.max[axis] + epsilon && b.min[axis] <= a.max[axis] + epsilon)
}

fn shares_vertex(a: &PosedTriangle, b: &PosedTriangle, epsilon: f64) -> bool {
    a.points.iter().any(|left| {
        b.points
            .iter()
            .any(|right| (*left - *right).length_squared() <= epsilon * epsilon)
    })
}

fn align_shared_vertices(
    mut a: [DVec3; 3],
    mut b: [DVec3; 3],
    epsilon: f64,
) -> ([DVec3; 3], [DVec3; 3]) {
    // UV-split copies can differ by a few float ULPs. Use the same geometric tolerance as
    // `shares_vertex` when testing their contact so roundoff does not turn a shared point/edge
    // into a tiny apparent crossing.
    let mut matched_b = [false; 3];
    for a_point in &mut a {
        let nearest = (0..3)
            .filter(|&b_index| !matched_b[b_index])
            .map(|b_index| ((*a_point - b[b_index]).length_squared(), b_index))
            .filter(|(distance_squared, _)| *distance_squared <= epsilon * epsilon)
            .min_by(
                |(left_distance, left_index), (right_distance, right_index)| {
                    left_distance
                        .total_cmp(right_distance)
                        .then_with(|| left_index.cmp(right_index))
                },
            );
        if let Some((_, b_index)) = nearest {
            let shared_position = (*a_point + b[b_index]) * 0.5;
            *a_point = shared_position;
            b[b_index] = shared_position;
            matched_b[b_index] = true;
        }
    }
    (a, b)
}

fn triangles_intersect(a: [DVec3; 3], b: [DVec3; 3], epsilon: f64) -> bool {
    triangles_intersect_non_coplanar(a, b, epsilon) || coplanar_triangles_intersect(a, b, epsilon)
}

fn triangles_intersect_non_coplanar(a: [DVec3; 3], b: [DVec3; 3], epsilon: f64) -> bool {
    (0..3).any(|edge| segment_intersects_triangle(a[edge], a[(edge + 1) % 3], b, epsilon))
        || (0..3).any(|edge| segment_intersects_triangle(b[edge], b[(edge + 1) % 3], a, epsilon))
}

fn coplanar_triangles_intersect(a: [DVec3; 3], b: [DVec3; 3], epsilon: f64) -> bool {
    let Some((a2, b2, area_epsilon)) = project_coplanar_triangles(a, b, epsilon) else {
        return false;
    };
    if a2
        .iter()
        .any(|point| point_in_triangle(*point, b2, area_epsilon))
        || b2
            .iter()
            .any(|point| point_in_triangle(*point, a2, area_epsilon))
    {
        return true;
    }
    (0..3).any(|i| {
        (0..3).any(|j| {
            segments_intersect_2d(a2[i], a2[(i + 1) % 3], b2[j], b2[(j + 1) % 3], area_epsilon)
        })
    })
}

fn coplanar_triangles_overlap_area(a: [DVec3; 3], b: [DVec3; 3], epsilon: f64) -> bool {
    let Some((a2, b2, area_epsilon)) = project_coplanar_triangles(a, b, epsilon) else {
        return false;
    };
    let intersection = clip_triangle(a2, b2);
    polygon_area(&intersection) > area_epsilon
}

fn project_coplanar_triangles(
    a: [DVec3; 3],
    b: [DVec3; 3],
    epsilon: f64,
) -> Option<([DVec2; 3], [DVec2; 3], f64)> {
    let normal_a = (a[1] - a[0]).cross(a[2] - a[0]);
    let normal_b = (b[1] - b[0]).cross(b[2] - b[0]);
    let scale = normal_a.length() * normal_b.length();
    if normal_a.length_squared() <= epsilon * epsilon
        || normal_b.length_squared() <= epsilon * epsilon
        || normal_a.cross(normal_b).length_squared() > scale * scale * 1e-16
    {
        return None;
    }
    let unit_normal = normal_a.normalize();
    if b.iter()
        .any(|point| (*point - a[0]).dot(unit_normal).abs() > epsilon)
    {
        return None;
    }
    let drop_axis = if normal_a.x.abs() >= normal_a.y.abs() && normal_a.x.abs() >= normal_a.z.abs()
    {
        0
    } else if normal_a.y.abs() >= normal_a.z.abs() {
        1
    } else {
        2
    };
    let project = |point: DVec3| match drop_axis {
        0 => DVec2::new(point.y, point.z),
        1 => DVec2::new(point.x, point.z),
        _ => DVec2::new(point.x, point.y),
    };
    let a2 = a.map(project);
    let b2 = b.map(project);
    let edge_scale = [a2, b2]
        .into_iter()
        .flat_map(|triangle| (0..3).map(move |i| (triangle[(i + 1) % 3] - triangle[i]).length()))
        .fold(0.0f64, f64::max);
    let area_epsilon = epsilon * edge_scale;
    Some((a2, b2, area_epsilon.max(epsilon * epsilon)))
}

fn clip_triangle(subject: [DVec2; 3], clip: [DVec2; 3]) -> Vec<DVec2> {
    let clip_orientation = orient2(clip[0], clip[1], clip[2]).signum();
    if clip_orientation == 0.0 {
        return Vec::new();
    }
    let mut polygon = subject.to_vec();
    for edge in 0..3 {
        if polygon.is_empty() {
            return polygon;
        }
        let start = clip[edge];
        let end = clip[(edge + 1) % 3];
        let input = std::mem::take(&mut polygon);
        let Some(mut previous) = input.last().copied() else {
            return Vec::new();
        };
        let mut previous_side = clip_orientation * orient2(start, end, previous);
        for current in input {
            let current_side = clip_orientation * orient2(start, end, current);
            let previous_inside = previous_side >= 0.0;
            let current_inside = current_side >= 0.0;
            if previous_inside != current_inside {
                let denominator = previous_side - current_side;
                if denominator != 0.0 {
                    let t = previous_side / denominator;
                    polygon.push(previous + (current - previous) * t);
                }
            }
            if current_inside {
                polygon.push(current);
            }
            previous = current;
            previous_side = current_side;
        }
    }
    polygon
}

fn polygon_area(polygon: &[DVec2]) -> f64 {
    if polygon.len() < 3 {
        return 0.0;
    }
    let origin = polygon[0];
    (1..polygon.len() - 1)
        .map(|i| (polygon[i] - origin).perp_dot(polygon[i + 1] - origin))
        .sum::<f64>()
        .abs()
        * 0.5
}

fn orient2(a: DVec2, b: DVec2, point: DVec2) -> f64 {
    let ab = b - a;
    let ap = point - a;
    ab.x * ap.y - ab.y * ap.x
}

fn point_in_triangle(point: DVec2, triangle: [DVec2; 3], epsilon: f64) -> bool {
    let signs = [
        orient2(triangle[0], triangle[1], point),
        orient2(triangle[1], triangle[2], point),
        orient2(triangle[2], triangle[0], point),
    ];
    let has_negative = signs.iter().any(|value| *value < -epsilon);
    let has_positive = signs.iter().any(|value| *value > epsilon);
    !(has_negative && has_positive)
}

fn segments_intersect_2d(a0: DVec2, a1: DVec2, b0: DVec2, b1: DVec2, epsilon: f64) -> bool {
    let o1 = orient2(a0, a1, b0);
    let o2 = orient2(a0, a1, b1);
    let o3 = orient2(b0, b1, a0);
    let o4 = orient2(b0, b1, a1);
    if (o1 > epsilon && o2 < -epsilon || o1 < -epsilon && o2 > epsilon)
        && (o3 > epsilon && o4 < -epsilon || o3 < -epsilon && o4 > epsilon)
    {
        return true;
    }
    let on_segment = |a: DVec2, b: DVec2, point: DVec2, orientation: f64| {
        orientation.abs() <= epsilon
            && point.x >= a.x.min(b.x) - epsilon
            && point.x <= a.x.max(b.x) + epsilon
            && point.y >= a.y.min(b.y) - epsilon
            && point.y <= a.y.max(b.y) + epsilon
    };
    on_segment(a0, a1, b0, o1)
        || on_segment(a0, a1, b1, o2)
        || on_segment(b0, b1, a0, o3)
        || on_segment(b0, b1, a1, o4)
}

fn segment_intersects_triangle(
    start: DVec3,
    end: DVec3,
    triangle: [DVec3; 3],
    epsilon: f64,
) -> bool {
    let direction = end - start;
    let edge1 = triangle[1] - triangle[0];
    let edge2 = triangle[2] - triangle[0];
    let cross = direction.cross(edge2);
    let determinant = edge1.dot(cross);
    if determinant.abs() <= epsilon {
        return false;
    }
    let inverse = 1.0 / determinant;
    let from_origin = start - triangle[0];
    let u = from_origin.dot(cross) * inverse;
    if u < -epsilon || u > 1.0 + epsilon {
        return false;
    }
    let cross = from_origin.cross(edge1);
    let v = direction.dot(cross) * inverse;
    if v < -epsilon || u + v > 1.0 + epsilon {
        return false;
    }
    let t = edge2.dot(cross) * inverse;
    t > epsilon && t < 1.0 - epsilon
}

fn face_condition(mesh: &Mesh, triangle: &[u32]) -> (f32, f32) {
    let vertex = |index: u32| &mesh.vertices[index as usize];
    let p0 = DVec3::from_array(vertex(triangle[0]).position.map(f64::from));
    let p1 = DVec3::from_array(vertex(triangle[1]).position.map(f64::from));
    let p2 = DVec3::from_array(vertex(triangle[2]).position.map(f64::from));
    let uv = |index| {
        let uv = vertex(index).uv;
        DVec2::new(f64::from(uv[0]), f64::from(uv[1]))
    };
    let uv0 = uv(triangle[0]);
    let uv1 = uv(triangle[1]);
    let uv2 = uv(triangle[2]);
    let edge1 = p1 - p0;
    let edge2 = p2 - p0;
    let delta1 = uv1 - uv0;
    let delta2 = uv2 - uv0;
    let uv_determinant = delta1.x * delta2.y - delta1.y * delta2.x;
    if uv_determinant == 0.0 {
        return (f32::MAX, f32::MAX);
    }

    let tangent = (edge1 * delta2.y - edge2 * delta1.y) / uv_determinant;
    let bitangent = (edge2 * delta1.x - edge1 * delta2.x) / uv_determinant;
    let a = tangent.length_squared();
    let b = tangent.dot(bitangent);
    let c = bitangent.length_squared();
    let trace = a + c;
    let determinant = a * c - b * b;
    let discriminant = (trace * trace - 4.0 * determinant).max(0.0);
    let lambda_max = 0.5 * (trace + discriminant.sqrt());
    let lambda_min = 0.5 * (trace - discriminant.sqrt());
    let condition = if lambda_min <= f64::EPSILON * lambda_max {
        f32::MAX
    } else {
        finite_f32((lambda_max / lambda_min).sqrt())
    };
    (condition, finite_f32(lambda_max.sqrt()))
}

fn finite_f32(value: f64) -> f32 {
    if !value.is_finite() || value >= f32::MAX as f64 {
        f32::MAX
    } else {
        value as f32
    }
}

fn chart_ids(mesh: &Mesh) -> Vec<u32> {
    let face_count = mesh.indices.len() / 3;
    let mut edge_faces: BTreeMap<(u32, u32), Vec<usize>> = BTreeMap::new();
    for (face, triangle) in mesh.indices.chunks_exact(3).enumerate() {
        for (a, b) in [
            (triangle[0], triangle[1]),
            (triangle[1], triangle[2]),
            (triangle[2], triangle[0]),
        ] {
            edge_faces
                .entry((a.min(b), a.max(b)))
                .or_default()
                .push(face);
        }
    }
    let mut neighbours = vec![Vec::new(); face_count];
    for faces in edge_faces.values() {
        for (slot, &left) in faces.iter().enumerate() {
            for &right in &faces[slot + 1..] {
                neighbours[left].push(right);
                neighbours[right].push(left);
            }
        }
    }
    let mut ids = vec![u32::MAX; face_count];
    for seed in 0..face_count {
        if ids[seed] != u32::MAX {
            continue;
        }
        let id = seed as u32;
        ids[seed] = id;
        let mut pending = vec![seed];
        while let Some(face) = pending.pop() {
            for &neighbour in &neighbours[face] {
                if ids[neighbour] == u32::MAX {
                    ids[neighbour] = id;
                    pending.push(neighbour);
                }
            }
        }
    }
    ids
}

#[cfg(test)]
mod tests {
    use super::sample_edgefill_source;

    #[test]
    fn edgefill_source_sampling_is_bilinear_and_clamps_to_edges() {
        let atlas_size = 16;
        let edgefill: Vec<_> = (0..atlas_size)
            .flat_map(|y| {
                (0..atlas_size).map(move |x| {
                    [
                        (x as f32 + 0.5) / atlas_size as f32,
                        (y as f32 + 0.5) / atlas_size as f32,
                        0.0,
                        1.0,
                    ]
                })
            })
            .collect();

        let between_centers =
            sample_edgefill_source(&edgefill, atlas_size, [5.0 / 16.0, 7.0 / 16.0]);
        assert_eq!(between_centers, [5.0 / 16.0, 7.0 / 16.0]);

        let clamped = sample_edgefill_source(&edgefill, atlas_size, [-0.5, 1.5]);
        assert_eq!(clamped, [0.5 / 16.0, 15.5 / 16.0]);
    }
}
