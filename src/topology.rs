//! Deterministic reference seam raster/flood bake, using GPU raster occupancy.
use crate::asset::Mesh;
use anyhow::{Result, ensure};
use glam::{Vec2, Vec3};

pub struct Maps {
    pub teleport: Vec<[f32; 4]>,
    pub edgefill: Vec<[f32; 4]>,
    pub seam_edges: usize,
}
#[derive(Clone, Copy)]
struct Cell {
    present: bool,
    edge_distance: f32,
    edge_uv: Vec2,
    teleport_distance: f32,
    teleport_uv: Vec2,
}

pub fn bake(mesh: &Mesh, n: u32, occupancy: &[bool]) -> Result<Maps> {
    ensure!(
        (16..=1024).contains(&n) && occupancy.len() == (n * n) as usize,
        "invalid atlas size or occupancy dimensions"
    );
    ensure!(
        occupancy.iter().any(|x| *x),
        "GPU UV raster produced no covered texels"
    );
    // Same 1mm proximity rule as the original; do not silently use it as an importer.
    let mut prox: Vec<usize> = (0..mesh.vertices.len()).collect();
    for i in 0..mesh.vertices.len() {
        for j in 0..i {
            if Vec3::from(mesh.vertices[i].position)
                .distance_squared(Vec3::from(mesh.vertices[j].position))
                < 1e-6
            {
                prox[i] = prox[j];
                break;
            }
        }
    }
    let mut edges = Vec::new();
    for tri in mesh.indices.chunks_exact(3) {
        for (a, b) in [(tri[2], tri[0]), (tri[0], tri[1]), (tri[1], tri[2])] {
            edges.push((a as usize, b as usize));
        }
    }
    let mut seams = Vec::new();
    for &(a, b) in &edges {
        if edges.contains(&(b, a)) {
            continue;
        }
        let candidates: Vec<_> = edges
            .iter()
            .filter(|&&(c, d)| prox[c] == prox[b] && prox[d] == prox[a])
            .copied()
            .collect();
        ensure!(
            candidates.len() == 1,
            "open/nonmanifold or ambiguous seam at vertices {a}, {b}: {} partners",
            candidates.len()
        );
        let (c, d) = candidates[0];
        ensure!(
            Vec3::from(mesh.vertices[a].normal).distance(Vec3::from(mesh.vertices[d].normal))
                < 1e-3
                && Vec3::from(mesh.vertices[b].normal)
                    .distance(Vec3::from(mesh.vertices[c].normal))
                    < 1e-3,
            "hard-normal seam is unsupported"
        );
        seams.push((a, b, d, c));
    }
    let mut cells: Vec<_> = occupancy
        .iter()
        .map(|present| Cell {
            present: *present,
            edge_distance: f32::MAX,
            edge_uv: Vec2::ZERO,
            teleport_distance: if *present { -f32::MAX } else { f32::MAX },
            teleport_uv: Vec2::ZERO,
        })
        .collect();
    let uv = |v: usize| Vec3::from(mesh.vertices[v].uv).truncate();
    for &(a, b, c, d) in &seams {
        // Each directed seam writes its partner; the reverse appears in seams too.
        let src0 = uv(a);
        let src1 = uv(b);
        let dst0 = uv(c);
        let dst1 = uv(d);
        let delta = dst1 - dst0;
        ensure!(delta.length_squared() > 1e-12, "degenerate UV seam");
        let min = ((dst0.min(dst1) * n as f32).floor() - Vec2::splat(5.))
            .max(Vec2::ZERO)
            .as_uvec2();
        let max = ((dst0.max(dst1) * n as f32).floor() + Vec2::splat(5.))
            .min(Vec2::splat(n as f32 - 1.))
            .as_uvec2();
        for x in min.x..=max.x {
            for y in min.y..=max.y {
                let p = Vec2::new(x as f32 + 0.5, y as f32 + 0.5) / n as f32;
                let lambda = (delta.dot(p - dst0) / delta.length_squared()).clamp(0., 1.);
                let vector = dst0 + delta * lambda - p;
                let mut distance = vector.length();
                if vector.x * delta.y < vector.y * delta.x {
                    distance = -distance;
                }
                let cell = &mut cells[(y * n + x) as usize];
                let replace = if distance < 0. {
                    distance > cell.teleport_distance
                } else {
                    distance < cell.teleport_distance
                };
                if replace {
                    cell.teleport_distance = distance;
                    cell.teleport_uv = if distance < -2. / n as f32 {
                        p
                    } else {
                        src0 + (src1 - src0) * lambda
                    };
                }
            }
        }
    }
    ensure!(!seams.is_empty(), "asset has no chart seams");
    let d10 = 1. / n as f32;
    let d11 = 2.0f32.sqrt() / n as f32;
    let neighbours = [
        (0, -1, d10),
        (1, -1, d11),
        (1, 0, d10),
        (1, 1, d11),
        (0, 1, d10),
        (-1, 1, d11),
        (-1, 0, d10),
        (-1, -1, d11),
    ];
    let mut changed = true;
    let mut rounds = 0;
    while changed {
        changed = false;
        rounds += 1;
        ensure!(rounds <= 4 * n, "topology flood fill did not converge");
        for y in 0..n {
            for x in 0..n {
                let center = cells[(y * n + x) as usize];
                if !center.present
                    && center.teleport_distance == f32::MAX
                    && center.edge_distance == f32::MAX
                {
                    continue;
                }
                let uvcenter = Vec2::new(x as f32 + 0.5, y as f32 + 0.5) / n as f32;
                let teleport_test = if center.teleport_distance < 0. {
                    center.teleport_distance - d11
                } else {
                    center.teleport_distance + d11
                };
                let edge_test = if center.present {
                    d11
                } else {
                    center.edge_distance + d11
                };
                for &(dx, dy, distance) in &neighbours {
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    if nx < 0 || ny < 0 || nx >= n as i32 || ny >= n as i32 {
                        continue;
                    }
                    let nei = &mut cells[(ny as u32 * n + nx as u32) as usize];
                    if !nei.present && nei.edge_distance > edge_test {
                        if center.present {
                            nei.edge_distance = distance;
                            nei.edge_uv = uvcenter;
                            changed = true;
                        } else if center.edge_distance < f32::MAX {
                            nei.edge_distance = center.edge_distance + distance;
                            nei.edge_uv = center.edge_uv;
                            changed = true;
                        }
                    }
                    if teleport_test > 0. {
                        if nei.teleport_distance > teleport_test
                            && center.teleport_distance < f32::MAX
                        {
                            nei.teleport_uv = center.teleport_uv;
                            nei.teleport_distance = center.teleport_distance + distance;
                            changed = true;
                        }
                    } else if nei.teleport_distance == f32::MAX
                        || nei.teleport_distance < teleport_test
                    {
                        nei.teleport_uv = center.teleport_uv;
                        nei.teleport_distance = center.teleport_distance - distance;
                        changed = true;
                    }
                }
            }
        }
    }
    let mut teleport = Vec::with_capacity(cells.len());
    let mut edgefill = Vec::with_capacity(cells.len());
    for (i, cell) in cells.into_iter().enumerate() {
        let p = Vec2::new((i as u32 % n) as f32 + 0.5, (i as u32 / n) as f32 + 0.5) / n as f32;
        // Reference FLT_MAX*50 sentinels overflow. Saturation preserves the SDF sign
        // without introducing Inf/NaN into filtered maps; unseeded destinations use self UV.
        let dest = if cell.teleport_distance.abs() == f32::MAX {
            p
        } else {
            cell.teleport_uv
        };
        teleport.push([
            dest.x,
            dest.y,
            cell.teleport_distance.clamp(-20., 20.) * 50.,
            1.,
        ]);
        ensure!(
            cell.present || cell.edge_distance < f32::MAX,
            "unfilled atlas texel {i}"
        );
        let src = if cell.present { p } else { cell.edge_uv };
        edgefill.push([src.x, src.y, 0., 1.]);
    }
    Ok(Maps {
        teleport,
        edgefill,
        seam_edges: seams.len(),
    })
}
