use dashr::{asset::*, topology::*};
use glam::{Vec2, Vec3};
fn occupancy(mesh: &Mesh, n: u32) -> Vec<bool> {
    let mut occ = vec![false; (n * n) as usize];
    for y in 0..n {
        for x in 0..n {
            let p = Vec2::new(x as f32 + 0.5, y as f32 + 0.5) / n as f32;
            for tri in mesh.indices.chunks_exact(3) {
                let v: Vec<_> = tri
                    .iter()
                    .map(|i| Vec3::from(mesh.vertices[*i as usize].uv).truncate())
                    .collect();
                let cross = |a: Vec2, b: Vec2| a.x * b.y - a.y * b.x;
                let e = [
                    cross(v[1] - v[0], p - v[0]),
                    cross(v[2] - v[1], p - v[1]),
                    cross(v[0] - v[2], p - v[2]),
                ];
                if e.iter().all(|d| *d >= 0.) || e.iter().all(|d| *d <= 0.) {
                    occ[(y * n + x) as usize] = true;
                    break;
                }
            }
        }
    }
    occ
}
#[test]
fn closed_reference_meshes_bake_valid_seam_and_gutter_maps() {
    for kind in [
        MeshKind::Tube,
        MeshKind::Cube,
        MeshKind::TubePinched,
        MeshKind::CubePinched,
    ] {
        let mesh = procedural(kind, 8, 8, 4., 1., 1.).unwrap();
        let n = 64;
        let occ = occupancy(&mesh, n);
        let maps = bake(&mesh, n, &occ).unwrap();
        assert!(maps.seam_edges > 0);
        assert!(maps.teleport.iter().flatten().all(|x| x.is_finite()));
        for (i, uv) in maps.edgefill.iter().enumerate() {
            let x = (uv[0] * n as f32).floor() as u32;
            let y = (uv[1] * n as f32).floor() as u32;
            assert!(
                x < n && y < n && occ[(y * n + x) as usize],
                "{kind:?}: gutter {i} -> {uv:?}"
            );
        }
        for (i, t) in maps.teleport.iter().enumerate() {
            if occ[i] {
                assert!(
                    t[2] <= 0.,
                    "{kind:?}: interior {i} has positive seam distance"
                );
            }
        }
        assert!(maps.teleport.iter().any(|p| p[2] > 0.));
    }
}
#[test]
fn open_mesh_and_empty_or_wrong_coverage_are_rejected() {
    let mut mesh = procedural(MeshKind::Cube, 8, 8, 4., 1., 1.).unwrap();
    assert!(bake(&mesh, 64, &[]).is_err());
    assert!(bake(&mesh, 64, &vec![false; 4096]).is_err());
    mesh.indices.truncate(mesh.indices.len() - 3);
    assert!(bake(&mesh, 64, &occupancy(&mesh, 64)).is_err());
}
