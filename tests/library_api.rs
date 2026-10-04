use dashr::{
    api::{TerminationStatus, TraceDiagnostic},
    asset::{Mesh, MeshKind, Vertex, procedural},
    asset_diagnostics::{
        find_high_distortion_faces, find_orientation_flips, find_self_intersections,
        find_t_junctions, suggest_seam_cut_for_hotspot,
    },
    asset_format::AssetDocument,
    map_cache::BakedMapCache,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

fn assert_cut_isolates_region(
    mesh: &Mesh,
    hotspot_face_id: u32,
    region_faces: &[u32],
    cut_edges: &[[u32; 2]],
) {
    let mut edge_faces = BTreeMap::<(u32, u32), Vec<usize>>::new();
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
    let cuts: BTreeSet<_> = cut_edges
        .iter()
        .map(|edge| (edge[0].min(edge[1]), edge[0].max(edge[1])))
        .collect();
    let mut adjacency = vec![Vec::new(); mesh.indices.len() / 3];
    for (edge, faces) in edge_faces {
        if faces.len() == 2 && !cuts.contains(&edge) {
            adjacency[faces[0]].push(faces[1]);
            adjacency[faces[1]].push(faces[0]);
        }
    }
    let mut component = BTreeSet::new();
    let mut pending = VecDeque::from([hotspot_face_id as usize]);
    while let Some(face) = pending.pop_front() {
        if component.insert(face as u32) {
            pending.extend(adjacency[face].iter().copied());
        }
    }
    assert_eq!(
        component,
        region_faces.iter().copied().collect(),
        "suggested cuts must isolate exactly the reported patch"
    );
}

#[test]
fn asset_document_round_trips_and_rejects_unsupported_schema() {
    let document =
        AssetDocument::new(procedural(MeshKind::Tube, 8, 8, 4.0, 1.0, 1.0).unwrap()).unwrap();
    let json = serde_json::to_string(&document).unwrap();
    let decoded: AssetDocument = serde_json::from_str(&json).unwrap();

    decoded.validate().unwrap();
    assert_eq!(decoded.schema_version, AssetDocument::SCHEMA_VERSION);
    assert_eq!(decoded.mesh.vertices.len(), document.mesh.vertices.len());
    assert_eq!(decoded.mesh.indices, document.mesh.indices);

    let mut unsupported = decoded;
    unsupported.schema_version += 1;
    assert!(
        unsupported
            .validate()
            .unwrap_err()
            .to_string()
            .contains("schema version")
    );
}

#[test]
fn asset_document_reports_bad_triangle_indices() {
    let mut document =
        AssetDocument::new(procedural(MeshKind::Cube, 8, 8, 4.0, 1.0, 1.0).unwrap()).unwrap();
    document.mesh.indices[0] = u32::MAX;

    let error = document.validate().unwrap_err().to_string();
    assert!(error.contains("triangle 0"));
    assert!(error.contains("vertex index"));
}

#[test]
fn pose_topology_preflight_rejects_open_triangle_soup_with_element_ids() {
    let vertex = |position, uv| Vertex {
        position,
        uv,
        weights: [1.0, 0.0, 0.0, 0.0],
        normal: [0.0, 0.0, 1.0],
        tangent: [1.0, 0.0, 0.0],
        bitangent: [0.0, 1.0, 0.0],
    };
    let mesh = Mesh {
        vertices: vec![
            vertex([0.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
            vertex([1.0, 0.0, 0.0], [1.0, 0.0, 1.0]),
            vertex([0.0, 1.0, 0.0], [0.0, 1.0, 1.0]),
        ],
        indices: vec![0, 1, 2],
    };

    let error = AssetDocument::new(mesh)
        .unwrap()
        .validate_pose_topology(&[glam::Mat4::IDENTITY; 4])
        .unwrap_err()
        .to_string();

    assert!(error.contains("face 0"), "missing face identifier: {error}");
    assert!(
        error.contains("edge vertices 2, 0"),
        "missing edge vertex identifiers: {error}"
    );
}

#[test]
fn pose_topology_preflight_rejects_three_faces_on_one_indexed_edge() {
    let vertex = |position| Vertex {
        position,
        uv: [position[0], position[1], 1.0],
        weights: [1.0, 0.0, 0.0, 0.0],
        normal: [0.0, 0.0, 1.0],
        tangent: [1.0, 0.0, 0.0],
        bitangent: [0.0, 1.0, 0.0],
    };
    let mesh = Mesh {
        vertices: vec![
            vertex([0.0, 0.0, 0.0]),
            vertex([1.0, 0.0, 0.0]),
            vertex([0.5, 1.0, 0.0]),
            vertex([0.5, -1.0, 0.0]),
            vertex([0.5, 0.0, 1.0]),
        ],
        indices: vec![0, 1, 2, 1, 0, 3, 0, 1, 4],
    };

    let error = AssetDocument::new(mesh)
        .unwrap()
        .validate_pose_topology(&[glam::Mat4::IDENTITY; 4])
        .unwrap_err()
        .to_string();

    assert!(error.contains("face 0"), "missing face identifier: {error}");
    assert!(
        error.contains("edge vertices 0, 1"),
        "missing edge vertex identifiers: {error}"
    );
    assert!(
        error.contains("3 incident triangles"),
        "missing incident-face count: {error}"
    );
}

#[test]
fn render_status_codes_are_named_and_unknown_codes_fail() {
    assert_eq!(
        TerminationStatus::try_from(0).unwrap(),
        TerminationStatus::NotLaunched
    );
    assert_eq!(
        TerminationStatus::try_from(1).unwrap(),
        TerminationStatus::Hit
    );
    assert_eq!(
        TerminationStatus::try_from(2).unwrap(),
        TerminationStatus::Escaped
    );
    assert_eq!(
        TerminationStatus::try_from(3).unwrap(),
        TerminationStatus::BudgetExhausted
    );
    assert_eq!(
        TerminationStatus::try_from(4).unwrap(),
        TerminationStatus::InvalidBasis
    );
    assert_eq!(
        TerminationStatus::try_from(5).unwrap(),
        TerminationStatus::DebugForcedHit
    );
    assert!(TerminationStatus::try_from(6).is_err());
}

#[test]
fn trace_diagnostics_preserve_steps_teleports_and_auxiliary_value() {
    let trace = TraceDiagnostic::from_channels([1.0, 27.0, 3.0, 0.625]).unwrap();

    assert_eq!(trace.status, TerminationStatus::Hit);
    assert_eq!(trace.steps, 27);
    assert_eq!(trace.teleports, 3);
    assert_eq!(trace.auxiliary, 0.625);
    assert!(TraceDiagnostic::from_channels([9.0, 1.0, 0.0, 0.0]).is_err());
}

#[test]
fn baked_map_cache_round_trips_and_checks_its_inputs() {
    let mesh = procedural(MeshKind::Tube, 8, 8, 4.0, 1.0, 1.0).unwrap();
    let occupancy = vec![true; 16 * 16];
    let cache = BakedMapCache::bake(&mesh, 16, &occupancy).unwrap();
    let json = cache.to_json().unwrap();
    let decoded = BakedMapCache::from_json(&json).unwrap();

    decoded.validate_for(&mesh, 16, &occupancy).unwrap();
    assert_eq!(decoded.maps.seam_edges, cache.maps.seam_edges);
    assert_eq!(decoded.maps.teleport, cache.maps.teleport);
    assert_eq!(decoded.maps.edgefill, cache.maps.edgefill);

    let mut different_mesh = mesh.clone();
    different_mesh.vertices[0].position[0] += 0.25;
    assert!(
        decoded
            .validate_for(&different_mesh, 16, &occupancy)
            .is_err()
    );

    let mut different_occupancy = occupancy;
    different_occupancy[0] = false;
    assert!(
        decoded
            .validate_for(&mesh, 16, &different_occupancy)
            .is_err()
    );
}

#[test]
fn high_distortion_report_names_the_face_and_connected_chart() {
    let vertex = |position, uv| Vertex {
        position,
        uv,
        weights: [1.0, 0.0, 0.0, 0.0],
        normal: [0.0, 0.0, 1.0],
        tangent: [1.0, 0.0, 0.0],
        bitangent: [0.0, 1.0, 0.0],
    };
    let mesh = Mesh {
        vertices: vec![
            vertex([0.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
            vertex([1.0, 0.0, 0.0], [1.0, 0.0, 1.0]),
            vertex([0.0, 1.0, 0.0], [0.0, 0.001, 1.0]),
        ],
        indices: vec![0, 1, 2],
    };

    let warnings = find_high_distortion_faces(&mesh, 10.0).unwrap();

    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].face_id, 0);
    assert_eq!(warnings[0].chart_id, 0);
    assert!(warnings[0].condition_number > 900.0);
    assert!(warnings[0].max_metric_scale > 900.0);
    assert!(find_high_distortion_faces(&mesh, 1.0).is_err());
    assert!(
        find_high_distortion_faces(&mesh, 2_000.0)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn runtime_hotspot_uv_maps_through_edgefill_to_a_seam_cut_suggestion() {
    let cube = procedural(MeshKind::CubePinched, 8, 16, 4.0, 1.0, 1.0).unwrap();
    let cube_source_uv = [0.542_968_75, 0.597_656_25];
    let cube_edgefill = vec![[cube_source_uv[0], cube_source_uv[1], 0.0, 1.0]; 16 * 16];
    let cube_suggestion = suggest_seam_cut_for_hotspot(&cube, &cube_edgefill, 16, [0.5849, 0.6359])
        .unwrap()
        .unwrap();
    assert_eq!(cube_suggestion.hotspot_uv, [0.5849, 0.6359]);
    assert_eq!(cube_suggestion.hotspot_face_id, 16);
    assert_eq!(cube_suggestion.chart_id, 0);
    assert_eq!(cube_suggestion.source_uv, cube_source_uv);
    assert_eq!(cube_suggestion.region_face_ids, vec![16]);
    assert_eq!(cube_suggestion.cut_edges, vec![[12, 13], [13, 15]]);
    assert_eq!(cube_suggestion.anchor_seams.len(), 1);
    assert_eq!(cube_suggestion.anchor_seams[0].face_id, 16);
    assert_eq!(cube_suggestion.anchor_seams[0].edge, [15, 12]);
    assert_cut_isolates_region(
        &cube,
        cube_suggestion.hotspot_face_id,
        &cube_suggestion.region_face_ids,
        &cube_suggestion.cut_edges,
    );

    let tube = procedural(MeshKind::TubePinched, 8, 16, 4.0, 1.0, 1.0).unwrap();
    let tube_source_uv = [0.800_781_25, 0.176_118_78];
    let tube_edgefill = vec![[tube_source_uv[0], tube_source_uv[1], 0.0, 1.0]; 16 * 16];
    let tube_suggestion = suggest_seam_cut_for_hotspot(&tube, &tube_edgefill, 16, [0.7940, 0.1829])
        .unwrap()
        .unwrap();
    assert_eq!(tube_suggestion.hotspot_uv, [0.7940, 0.1829]);
    assert_eq!(tube_suggestion.hotspot_face_id, 277);
    assert_eq!(tube_suggestion.chart_id, 0);
    assert_eq!(tube_suggestion.source_uv, tube_source_uv);
    assert_eq!(tube_suggestion.region_face_ids, vec![277]);
    assert_eq!(tube_suggestion.cut_edges, vec![[7, 8], [8, 167]]);
    assert_eq!(tube_suggestion.anchor_seams.len(), 1);
    assert_eq!(tube_suggestion.anchor_seams[0].face_id, 277);
    assert_eq!(tube_suggestion.anchor_seams[0].partner_face_id, 275);
    assert_cut_isolates_region(
        &tube,
        tube_suggestion.hotspot_face_id,
        &tube_suggestion.region_face_ids,
        &tube_suggestion.cut_edges,
    );
}

#[test]
fn self_intersection_report_names_both_faces_and_chart_components() {
    let vertex = |position, uv| Vertex {
        position,
        uv,
        weights: [1.0, 0.0, 0.0, 0.0],
        normal: [0.0, 0.0, 1.0],
        tangent: [1.0, 0.0, 0.0],
        bitangent: [0.0, 1.0, 0.0],
    };
    let mesh = Mesh {
        vertices: vec![
            vertex([0.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
            vertex([1.0, 0.0, 0.0], [1.0, 0.0, 1.0]),
            vertex([0.0, 1.0, 0.0], [0.0, 1.0, 1.0]),
            vertex([0.2, -0.2, -1.0], [0.0, 0.0, 1.0]),
            vertex([0.2, 0.7, 1.0], [1.0, 0.0, 1.0]),
            vertex([0.2, 0.9, -1.0], [0.0, 1.0, 1.0]),
        ],
        indices: vec![0, 1, 2, 3, 4, 5],
    };

    let intersections = find_self_intersections(&mesh, &[glam::Mat4::IDENTITY; 4]).unwrap();

    assert_eq!(intersections.len(), 1);
    assert_eq!(intersections[0].face_a, 0);
    assert_eq!(intersections[0].face_b, 1);
    assert_eq!(intersections[0].chart_a, 0);
    assert_eq!(intersections[0].chart_b, 1);
    let error = AssetDocument::new(mesh)
        .unwrap()
        .validate_pose_topology(&[glam::Mat4::IDENTITY; 4])
        .unwrap_err()
        .to_string();
    assert!(error.contains("faces 0"));
    assert!(error.contains("and 1"));
}

#[test]
fn coplanar_overlap_diagnostic_names_both_faces() {
    let vertex = |position| Vertex {
        position,
        uv: [position[0], position[1], 1.0],
        weights: [1.0, 0.0, 0.0, 0.0],
        normal: [0.0, 0.0, 1.0],
        tangent: [1.0, 0.0, 0.0],
        bitangent: [0.0, 1.0, 0.0],
    };
    let mesh = Mesh {
        vertices: vec![
            vertex([0.0, 0.0, 0.0]),
            vertex([2.0, 0.0, 0.0]),
            vertex([0.0, 2.0, 0.0]),
            vertex([0.5, 0.5, 0.0]),
            vertex([2.0, 2.0, 0.0]),
            vertex([2.0, 0.5, 0.0]),
        ],
        indices: vec![0, 1, 2, 3, 4, 5],
    };

    let overlap = find_self_intersections(&mesh, &[glam::Mat4::IDENTITY; 4]).unwrap();
    assert_eq!(overlap.len(), 1);
    assert_eq!(overlap[0].face_a, 0);
    assert_eq!(overlap[0].face_b, 1);
}

#[test]
fn coplanar_overlap_with_a_shared_geometric_vertex_is_reported() {
    let vertex = |position| Vertex {
        position,
        uv: [position[0], position[1], 1.0],
        weights: [1.0, 0.0, 0.0, 0.0],
        normal: [0.0, 0.0, 1.0],
        tangent: [1.0, 0.0, 0.0],
        bitangent: [0.0, 1.0, 0.0],
    };
    let mesh = Mesh {
        vertices: vec![
            vertex([0.0, 0.0, 0.0]),
            vertex([2.0, 0.0, 0.0]),
            vertex([0.0, 2.0, 0.0]),
            vertex([0.0, 0.0, 0.0]),
            vertex([1.0, 0.2, 0.0]),
            vertex([0.2, 1.0, 0.0]),
        ],
        indices: vec![0, 1, 2, 3, 4, 5],
    };

    let overlap = find_self_intersections(&mesh, &[glam::Mat4::IDENTITY; 4]).unwrap();

    assert_eq!(overlap.len(), 1);
    assert_eq!(overlap[0].face_a, 0);
    assert_eq!(overlap[0].face_b, 1);
}

#[test]
fn coplanar_overlap_at_a_large_world_offset_uses_local_area_tolerance() {
    let vertex = |position: [f32; 3]| Vertex {
        position,
        uv: [position[0], position[1], 1.0],
        weights: [1.0, 0.0, 0.0, 0.0],
        normal: [0.0, 0.0, 1.0],
        tangent: [1.0, 0.0, 0.0],
        bitangent: [0.0, 1.0, 0.0],
    };
    let origin = 100_000_000.0f32;
    let mesh = Mesh {
        vertices: vec![
            vertex([origin, origin, 0.0]),
            vertex([origin + 64.0, origin, 0.0]),
            vertex([origin, origin + 64.0, 0.0]),
            vertex([origin, origin, 0.0]),
            vertex([origin + 32.0, origin + 8.0, 0.0]),
            vertex([origin + 8.0, origin + 32.0, 0.0]),
        ],
        indices: vec![0, 1, 2, 3, 4, 5],
    };

    let overlap = find_self_intersections(&mesh, &[glam::Mat4::IDENTITY; 4]).unwrap();

    assert_eq!(overlap.len(), 1);
    assert_eq!(overlap[0].face_a, 0);
    assert_eq!(overlap[0].face_b, 1);
}

#[test]
fn non_coplanar_intersection_beyond_a_shared_vertex_is_reported() {
    let vertex = |position| Vertex {
        position,
        uv: [position[0], position[1], 1.0],
        weights: [1.0, 0.0, 0.0, 0.0],
        normal: [0.0, 0.0, 1.0],
        tangent: [1.0, 0.0, 0.0],
        bitangent: [0.0, 1.0, 0.0],
    };
    let mesh = Mesh {
        vertices: vec![
            vertex([0.0, 0.0, 0.0]),
            vertex([2.0, 0.0, 0.0]),
            vertex([0.0, 2.0, 0.0]),
            vertex([0.0, 0.0, 0.0]),
            vertex([1.0, 1.0, -1.0]),
            vertex([1.0, 1.0, 1.0]),
        ],
        indices: vec![0, 1, 2, 3, 4, 5],
    };

    let intersections = find_self_intersections(&mesh, &[glam::Mat4::IDENTITY; 4]).unwrap();

    assert_eq!(intersections.len(), 1);
    assert_eq!(intersections[0].face_a, 0);
    assert_eq!(intersections[0].face_b, 1);
}

#[test]
fn coplanar_overlap_with_a_shared_edge_is_reported() {
    let vertex = |position| Vertex {
        position,
        uv: [position[0], position[1], 1.0],
        weights: [1.0, 0.0, 0.0, 0.0],
        normal: [0.0, 0.0, 1.0],
        tangent: [1.0, 0.0, 0.0],
        bitangent: [0.0, 1.0, 0.0],
    };
    let mesh = Mesh {
        vertices: vec![
            vertex([0.0, 0.0, 0.0]),
            vertex([2.0, 0.0, 0.0]),
            vertex([0.0, 2.0, 0.0]),
            vertex([0.0, 0.0, 0.0]),
            vertex([2.0, 0.0, 0.0]),
            vertex([1.0, 1.0, 0.0]),
        ],
        indices: vec![0, 1, 2, 3, 4, 5],
    };

    let overlap = find_self_intersections(&mesh, &[glam::Mat4::IDENTITY; 4]).unwrap();

    assert_eq!(overlap.len(), 1);
    assert_eq!(overlap[0].face_a, 0);
    assert_eq!(overlap[0].face_b, 1);
}

#[test]
fn adjacent_coplanar_triangles_sharing_an_edge_are_not_overlaps() {
    let vertex = |position| Vertex {
        position,
        uv: [position[0], position[1], 1.0],
        weights: [1.0, 0.0, 0.0, 0.0],
        normal: [0.0, 0.0, 1.0],
        tangent: [1.0, 0.0, 0.0],
        bitangent: [0.0, 1.0, 0.0],
    };
    let mesh = Mesh {
        vertices: vec![
            vertex([0.0, 0.0, 0.0]),
            vertex([1.0, 0.0, 0.0]),
            vertex([0.0, 1.0, 0.0]),
            vertex([1.0, 0.0, 0.0]),
            vertex([1.0, 1.0, 0.0]),
            vertex([0.0, 1.0, 0.0]),
        ],
        indices: vec![0, 1, 2, 3, 4, 5],
    };

    let intersections = find_self_intersections(&mesh, &[glam::Mat4::IDENTITY; 4]).unwrap();

    assert!(intersections.is_empty());
}

#[test]
fn coplanar_triangles_touching_only_at_a_shared_vertex_are_not_overlaps() {
    let vertex = |position| Vertex {
        position,
        uv: [position[0], position[1], 1.0],
        weights: [1.0, 0.0, 0.0, 0.0],
        normal: [0.0, 0.0, 1.0],
        tangent: [1.0, 0.0, 0.0],
        bitangent: [0.0, 1.0, 0.0],
    };
    let mesh = Mesh {
        vertices: vec![
            vertex([0.0, 0.0, 0.0]),
            vertex([1.0, 0.0, 0.0]),
            vertex([0.0, 1.0, 0.0]),
            vertex([1.0, 0.0, 0.0]),
            vertex([2.0, 0.0, 0.0]),
            vertex([1.0, -1.0, 0.0]),
        ],
        indices: vec![0, 1, 2, 3, 4, 5],
    };

    let intersections = find_self_intersections(&mesh, &[glam::Mat4::IDENTITY; 4]).unwrap();

    assert!(intersections.is_empty());
}

#[test]
fn adjacent_non_coplanar_triangles_sharing_an_edge_are_not_overlaps() {
    let vertex = |position| Vertex {
        position,
        uv: [position[0], position[1], 1.0],
        weights: [1.0, 0.0, 0.0, 0.0],
        normal: [0.0, 0.0, 1.0],
        tangent: [1.0, 0.0, 0.0],
        bitangent: [0.0, 1.0, 0.0],
    };
    let mesh = Mesh {
        vertices: vec![
            vertex([0.0, 0.0, 0.0]),
            vertex([1.0, 0.0, 0.0]),
            vertex([0.0, 1.0, 0.0]),
            vertex([1.0, 0.0, 0.0]),
            vertex([0.0, 0.0, 0.0]),
            vertex([0.0, 0.0, 1.0]),
        ],
        indices: vec![0, 1, 2, 3, 4, 5],
    };

    let intersections = find_self_intersections(&mesh, &[glam::Mat4::IDENTITY; 4]).unwrap();

    assert!(intersections.is_empty());
}

#[test]
fn animated_reference_tunnel_is_reported_without_rejecting_rest_geometry() {
    let mesh = procedural(MeshKind::Tube, 8, 8, 4.0, 1.0, 1.0).unwrap();
    let document = AssetDocument::new(mesh).unwrap();

    document
        .validate_pose_topology(&[glam::Mat4::IDENTITY; 4])
        .unwrap();
    let animated_bones = dashr::asset::bones(MeshKind::Tube, 10.0, 1.0, 4.0);
    let warnings = find_self_intersections(&document.mesh, &animated_bones).unwrap();
    let orientation_flips = find_orientation_flips(&document.mesh, &animated_bones).unwrap();

    assert!(
        !warnings.is_empty(),
        "the known folded tube pose should stay visible to U5 diagnostics"
    );
    assert!(
        !orientation_flips.is_empty(),
        "the folded tube pose should also report face-local orientation changes"
    );
}

#[test]
fn tube_pinched_rest_seam_contact_is_not_a_self_intersection() {
    let mesh = procedural(MeshKind::TubePinched, 8, 16, 4.0, 1.0, 1.0).unwrap();
    let asset = AssetDocument::new(mesh).unwrap();

    asset
        .validate_pose_topology(&[glam::Mat4::IDENTITY; 4])
        .expect("the reference TubePinched seam only has point/edge contact at rest");
}

#[test]
fn orientation_flip_diagnostic_names_the_face_and_chart() {
    let vertex = |position| Vertex {
        position,
        uv: [position[0], position[1], 1.0],
        weights: [1.0, 0.0, 0.0, 0.0],
        normal: [0.0, 0.0, 1.0],
        tangent: [1.0, 0.0, 0.0],
        bitangent: [0.0, 1.0, 0.0],
    };
    let mesh = Mesh {
        vertices: vec![
            vertex([0.0, 0.0, 0.0]),
            vertex([1.0, 0.0, 0.0]),
            vertex([0.0, 1.0, 0.0]),
        ],
        indices: vec![0, 1, 2],
    };
    assert!(
        find_orientation_flips(&mesh, &[glam::Mat4::IDENTITY; 4])
            .unwrap()
            .is_empty()
    );

    let reflected = glam::Mat4::from_scale(glam::Vec3::new(-1.0, 1.0, 1.0));
    let warnings = find_orientation_flips(&mesh, &[reflected; 4]).unwrap();
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].face_id, 0);
    assert_eq!(warnings[0].chart_id, 0);
    assert!(warnings[0].oriented_area_ratio < 0.0);
}

#[test]
fn t_junction_report_names_the_vertex_and_edge_face() {
    let vertex = |position, uv| Vertex {
        position,
        uv,
        weights: [1.0, 0.0, 0.0, 0.0],
        normal: [0.0, 0.0, 1.0],
        tangent: [1.0, 0.0, 0.0],
        bitangent: [0.0, 1.0, 0.0],
    };
    let mesh = Mesh {
        vertices: vec![
            vertex([0.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
            vertex([1.0, 0.0, 0.0], [1.0, 0.0, 1.0]),
            vertex([0.0, 1.0, 0.0], [0.0, 1.0, 1.0]),
            vertex([0.5, 0.0, 0.0], [0.25, 0.0, 1.0]),
            vertex([0.5, -1.0, 0.0], [0.25, 0.5, 1.0]),
            vertex([1.0, -1.0, 0.0], [0.5, 0.5, 1.0]),
        ],
        indices: vec![0, 1, 2, 3, 4, 5],
    };

    let warnings = find_t_junctions(&mesh).unwrap();

    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].vertex_id, 3);
    assert_eq!(warnings[0].edge_face_id, 0);
    assert_eq!(warnings[0].edge_vertices, [0, 1]);
    let error = AssetDocument::new(mesh)
        .unwrap()
        .validate_pose_topology(&[glam::Mat4::IDENTITY; 4])
        .unwrap_err()
        .to_string();
    assert!(error.contains("vertex 3"));
    assert!(error.contains("face 0"));
}
