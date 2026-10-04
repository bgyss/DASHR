use dashr::{asset::*, math_reference::*};
use glam::{DMat3, DVec2, DVec3, Vec3};
#[test]
fn inverse_preserves_skew_and_metric_scale() {
    let b = DMat3::from_cols(
        DVec3::new(7., 0., 0.),
        DVec3::new(2., 3., 0.),
        DVec3::new(0., 0., 0.4),
    );
    let inv = inverse_basis(b).unwrap();
    for p in [DVec3::new(0.23, 0.78, 0.5), DVec3::new(-15., 1e3, 0.001)] {
        assert!((inv * (b * p) - p).length() < 1e-10);
    }
    assert!(inverse_basis(DMat3::ZERO).is_err());
    assert!(inverse_basis(DMat3::from_cols(DVec3::X, DVec3::X, DVec3::Z)).is_err());
    assert!(inverse_basis(DMat3::from_diagonal(DVec3::new(f64::NAN, 1., 1.))).is_err());
}

#[test]
fn cofactor_inverse_matches_the_general_metric_basis_inverse() {
    for basis in [
        DMat3::from_cols(
            DVec3::new(7.0, 0.0, 0.0),
            DVec3::new(2.0, 3.0, 0.0),
            DVec3::new(0.0, 0.0, 0.4),
        ),
        DMat3::from_cols(
            DVec3::new(2.0, 1.0, 0.5),
            DVec3::new(-0.5, 3.0, 0.25),
            DVec3::new(0.2, -0.4, 0.8),
        ),
    ] {
        let expected = inverse_basis(basis).unwrap();
        let specialized = inverse_basis_cofactor(basis).unwrap();
        for (a, b) in specialized
            .to_cols_array()
            .into_iter()
            .zip(expected.to_cols_array())
        {
            assert!((a - b).abs() < 1e-10);
        }
    }
    assert!(inverse_basis_cofactor(DMat3::ZERO).is_err());
}
#[test]
fn anchor_and_gutter_correction_preserve_object_point() {
    let b = DMat3::from_diagonal(DVec3::new(6., 4., 0.8));
    let uvh = DVec3::new(0.25, 0.7, 0.5);
    let anchor = DVec3::new(10., -2., 3.);
    let p = anchor + b * DVec3::new(0.01, -0.02, 0.2);
    let inv = inverse_basis(b).unwrap();
    assert!(
        (surface_position(inv, anchor, p, uvh, 1) - (uvh + DVec3::new(0.01, -0.02, 0.2))).length()
            < 1e-10
    );
    let offset = uvh - inv * anchor;
    assert!(
        (surface_position(inv, offset, p, uvh, 0) - (uvh + DVec3::new(0.01, -0.02, 0.2))).length()
            < 1e-10
    );
    let delta = DVec2::new(0.02, -0.01);
    let moved = gutter_anchor(inv, anchor, delta).unwrap();
    assert!(
        (surface_position(inv, moved, p, uvh + delta.extend(0.), 1)
            - surface_position(inv, anchor, p, uvh, 1))
        .length()
            < 1e-10
    );
}
#[test]
fn procedural_reference_counts_weights_and_metric_basis() {
    for (kind, nv, nt) in [
        (MeshKind::Tube, 131, 176),
        (MeshKind::TubePinched, 129, 176),
        (MeshKind::Cube, 45, 48),
        (MeshKind::CubePinched, 45, 48),
    ] {
        let mesh = procedural(kind, 8, 8, 4., 1., 1.).unwrap();
        assert_eq!(mesh.vertices.len(), nv, "{kind:?}");
        assert_eq!(mesh.indices.len() / 3, nt, "{kind:?}");
        for v in &mesh.vertices {
            assert!((v.weights.iter().sum::<f32>() - 1.).abs() < 1e-5);
            assert!((Vec3::from(v.normal).length() - 1.).abs() < 1e-5);
        }
        assert!(
            mesh.vertices
                .iter()
                .any(|v| Vec3::from(v.tangent).length() > 2.)
        );
    }
    assert!(procedural(MeshKind::Tube, 0, 8, 4., 1., 1.).is_err());
    assert!(procedural(MeshKind::Tube, 8, 8, 4., f32::NAN, 1.).is_err());
}
#[test]
fn tangent_generation_keeps_uv_lengths() {
    let mut mesh = Mesh {
        vertices: vec![
            Vertex {
                position: [0., 0., 0.],
                uv: [0., 0., 1.],
                ..Vertex::zeroed()
            },
            Vertex {
                position: [2., 0., 0.],
                uv: [0.5, 0., 1.],
                ..Vertex::zeroed()
            },
            Vertex {
                position: [1., 3., 0.],
                uv: [0., 0.5, 1.],
                ..Vertex::zeroed()
            },
        ],
        indices: vec![0, 1, 2],
    };
    generate_basis(&mut mesh).unwrap();
    for v in &mesh.vertices {
        assert!((Vec3::from(v.tangent) - Vec3::new(4., 0., 0.)).length() < 1e-5);
        assert!((Vec3::from(v.bitangent) - Vec3::new(2., 6., 0.)).length() < 1e-5);
    }
    mesh.vertices[2].uv = [0.5, 0., 1.];
    assert!(generate_basis(&mut mesh).is_err());
}
use bytemuck::Zeroable;
