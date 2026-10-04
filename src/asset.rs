//! Procedural assets translated from Tom Forsyth's CreateModel/GenerateTangentSpace.
use anyhow::{Result, ensure};
use bytemuck::{Pod, Zeroable};
use glam::{Mat3, Mat4, Vec2, Vec3};
use serde::{Deserialize, Serialize};
use std::f32::consts::{FRAC_PI_2, TAU};

#[repr(C)]
#[derive(Clone, Copy, Debug, Serialize, Deserialize, Pod, Zeroable)]
#[serde(deny_unknown_fields)]
pub struct Vertex {
    pub position: [f32; 3],
    pub uv: [f32; 3],
    pub weights: [f32; 4],
    pub normal: [f32; 3],
    pub tangent: [f32; 3],
    pub bitangent: [f32; 3],
}

impl Vertex {
    fn new(position: Vec3, uv: Vec2, normal: Vec3, weights: [f32; 4]) -> Self {
        Self {
            position: position.to_array(),
            uv: uv.extend(1.0).to_array(),
            normal: normal.to_array(),
            weights,
            ..Self::zeroed()
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "tooling", derive(clap::ValueEnum))]
#[serde(rename_all = "kebab-case")]
pub enum MeshKind {
    Tube,
    Cube,
    TubePinched,
    CubePinched,
}
impl MeshKind {
    pub fn is_tube(self) -> bool {
        matches!(self, Self::Tube | Self::TubePinched)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}
impl Mesh {
    fn tri(&mut self, a: u32, b: u32, c: u32) {
        self.indices.extend([a, b, c]);
    }
}

pub fn procedural(
    kind: MeshKind,
    around: u32,
    long: u32,
    length: f32,
    radius: f32,
    thickness: f32,
) -> Result<Mesh> {
    ensure!(
        (4..=128).contains(&around) && (1..=128).contains(&long),
        "segments around must be 4..128 and long 1..128"
    );
    ensure!(
        [length, radius, thickness]
            .iter()
            .all(|x| x.is_finite() && *x > 0.0),
        "mesh dimensions must be finite and positive"
    );
    let mut mesh = if kind.is_tube() {
        tube(
            kind == MeshKind::TubePinched,
            around,
            long,
            length,
            radius,
            thickness,
        )
    } else {
        cube(kind == MeshKind::CubePinched, around / 4, radius, thickness)
    };
    generate_basis(&mut mesh)?;
    Ok(mesh)
}

fn weights(t: f32) -> [f32; 4] {
    if t < 0.1 {
        [1., 0., 0., 0.]
    } else if t < 0.2 {
        let f = (0.2 - t) / 0.1;
        [f, 1. - f, 0., 0.]
    } else if t < 0.4 {
        [0., 1., 0., 0.]
    } else if t < 0.6 {
        let f = (0.6 - t) / 0.2;
        [0., f, 1. - f, 0.]
    } else if t < 0.8 {
        [0., 0., 1., 0.]
    } else if t < 0.9 {
        let f = (0.9 - t) / 0.1;
        [0., 0., f, 1. - f]
    } else {
        [0., 0., 0., 1.]
    }
}

fn tube(pinched: bool, around: u32, long: u32, length: f32, radius: f32, thickness: f32) -> Mesh {
    let mut m = Mesh {
        vertices: Vec::new(),
        indices: Vec::new(),
    };
    let q = ((around + 2) / 4).max(1);
    let end_length = FRAC_PI_2 * radius;
    let vlength = length + 2. * end_length;
    let vstart = 0.1 + 0.6 * end_length / vlength;
    for l in 0..=long {
        for a in 0..=around {
            let t = l as f32 / long as f32;
            let u = a as f32 / around as f32;
            let normal = Vec3::new((TAU * u).sin(), -(TAU * u).cos(), 0.);
            m.vertices.push(Vertex::new(
                normal * radius + Vec3::Z * (length * t),
                Vec2::new(0.1 + 0.8 * u, vstart + 0.6 * length / vlength * t),
                normal * thickness,
                weights(t),
            ));
        }
    }
    for cap in 0..2 {
        let sign = if cap == 0 { 1. } else { -1. };
        let voffset = if cap == 0 {
            vstart
        } else {
            0.1 + 0.6 * (end_length + length) / vlength
        };
        let vs = sign * 0.6 * end_length / (vlength * q as f32);
        let mut w = [0.; 4];
        w[if cap == 0 { 0 } else { 3 }] = 1.;
        for l in 1..=q {
            for a in 0..around {
                let u = a as f32 / around as f32;
                let e = sign * FRAC_PI_2 * l as f32 / q as f32;
                if pinched || l < q {
                    let n = Vec3::new(
                        (TAU * u).sin() * e.cos(),
                        -(TAU * u).cos() * e.cos(),
                        -e.sin(),
                    );
                    m.vertices.push(Vertex::new(
                        n * radius + Vec3::Z * (cap as f32 * length),
                        Vec2::new(0.1 + 0.8 * u, voffset - vs * l as f32),
                        n * thickness,
                        w,
                    ));
                }
                if l < q {
                    let angle = TAU * (u + 1. / around as f32);
                    let n = Vec3::new(angle.sin() * e.cos(), -angle.cos() * e.cos(), -e.sin());
                    m.vertices.push(Vertex::new(
                        n * radius + Vec3::Z * (cap as f32 * length),
                        Vec2::new(
                            0.1 + 0.8 * (u + e.cos() / around as f32),
                            voffset - vs * l as f32,
                        ),
                        n * thickness,
                        w,
                    ));
                }
            }
        }
    }
    if !pinched {
        for cap in 0..2 {
            let sign = if cap == 0 { 1. } else { -1. };
            let e = sign * FRAC_PI_2 * (q - 1) as f32 / q as f32;
            let mut w = [0.; 4];
            w[if cap == 0 { 0 } else { 3 }] = 1.;
            for a in 0..around {
                let angle = TAU * a as f32 / around as f32;
                let n = Vec3::new(angle.sin() * e.cos(), -angle.cos() * e.cos(), -e.sin());
                m.vertices.push(Vertex::new(
                    n * radius + Vec3::Z * (cap as f32 * length),
                    Vec2::new(
                        if cap == 0 {
                            0.25 + 0.1 * angle.sin()
                        } else {
                            0.75 - 0.1 * angle.sin()
                        },
                        0.85 + 0.1 * angle.cos(),
                    ),
                    n * thickness,
                    w,
                ));
            }
            m.vertices.push(Vertex::new(
                Vec3::Z * (if cap == 0 { -radius } else { length + radius }),
                Vec2::new(if cap == 0 { 0.25 } else { 0.75 }, 0.85),
                Vec3::Z * (-sign * thickness),
                w,
            ));
        }
    }
    for l in 0..long {
        for a in 0..around {
            let v = l * (around + 1) + a;
            m.tri(v, v + 1, v + around + 1);
            m.tri(v + around + 1, v + 1, v + around + 2);
        }
    }
    let body = (long + 1) * (around + 1);
    let strip = ((q - 1) * 2 + u32::from(pinched)) * around;
    for cap in 0..2 {
        let base = body + cap * strip;
        for a in 0..around {
            let mut v0 = a + cap * (around + 1) * long;
            let mut v1 = v0 + 1;
            for l in 0..q - 1 {
                let v2 = base + (l * around + a) * 2;
                let v3 = v2 + 1;
                if cap == 0 {
                    m.tri(v0, v2, v1);
                    m.tri(v1, v2, v3);
                } else {
                    m.tri(v0, v1, v2);
                    m.tri(v2, v1, v3);
                }
                v0 = v2;
                v1 = v3;
            }
            if pinched {
                let tip = base + (q - 1) * around * 2 + a;
                if cap == 0 {
                    m.tri(v0, tip, v1);
                } else {
                    m.tri(v1, tip, v0);
                }
            }
        }
    }
    if !pinched {
        let base = body + 2 * strip;
        for cap in 0..2 {
            for a in 0..around {
                let v0 = base + cap * (around + 1) + a;
                let v1 = base + cap * (around + 1) + around;
                let v2 = base + cap * (around + 1) + (a + around - 1) % around;
                if cap == 0 {
                    m.tri(v0, v2, v1);
                } else {
                    m.tri(v0, v1, v2);
                }
            }
        }
    }
    m
}

fn cube(pinched: bool, n: u32, radius: f32, thickness: f32) -> Mesh {
    let mut m = Mesh {
        vertices: Vec::new(),
        indices: Vec::new(),
    };
    let mut add = |p: Vec3, uv: Vec2| {
        let normal = p.normalize();
        m.vertices.push(Vertex::new(
            normal * radius,
            uv,
            normal * thickness,
            [1., 0., 0., 0.],
        ));
    };
    for face in 0..4 {
        for w in 0..n {
            for h in 0..=n {
                let hf = w as f32 / n as f32;
                let vf = h as f32 / n as f32;
                let hp = -1. + 2. * hf;
                let yp = -1. + 2. * vf;
                let p = match face {
                    0 => Vec3::new(hp, yp, -1.),
                    1 => Vec3::new(1., yp, hp),
                    2 => Vec3::new(-hp, yp, 1.),
                    _ => Vec3::new(-1., yp, -hp),
                };
                add(p, Vec2::new((hf + face as f32) * 0.2 + 0.1, 0.6 - vf * 0.2));
            }
        }
    }
    for h in 0..=n {
        let v = h as f32 / n as f32;
        add(
            Vec3::new(-1., -1. + 2. * v, -1.),
            Vec2::new(0.9, 0.6 - v * 0.2),
        );
    }
    for face in 4..=5 {
        for w in 0..=n {
            for h in 0..=n {
                let hf = w as f32 / n as f32;
                let vf = h as f32 / n as f32;
                let hp = -1. + 2. * hf;
                let vp = -1. + 2. * vf;
                let p = if face == 4 {
                    Vec3::new(-vp, 1., hp)
                } else {
                    Vec3::new(vp, -1., hp)
                };
                let top = if face == 4 {
                    if pinched { 0.4 } else { 0.3 }
                } else if pinched {
                    0.8
                } else {
                    0.9
                };
                add(p, Vec2::new(0.3 + 0.2 * hf, top - 0.2 * vf));
            }
        }
    }
    for f in 0..4 {
        for w in 0..n {
            for h in 0..n {
                let v = (f * n + w) * (n + 1) + h;
                m.tri(v, v + 1, v + n + 1);
                m.tri(v + n + 1, v + 1, v + n + 2);
            }
        }
    }
    let mut base = (1 + 4 * n) * (n + 1);
    for f in 4..=5 {
        for w in 0..n {
            for h in 0..n {
                let mut v0 = base + h + w * (n + 1);
                let mut v1 = v0 + 1;
                let mut v2 = v0 + n + 1;
                let mut v3 = v2 + 1;
                if pinched && f == 4 && h == 0 {
                    v0 = (n + w) * (n + 1) + n;
                    v2 = v0 + n + 1;
                }
                if pinched && f == 5 && h == n - 1 {
                    v1 = (n + w) * (n + 1);
                    v3 = v1 + n + 1;
                }
                m.tri(v0, v1, v2);
                m.tri(v2, v1, v3);
            }
        }
        base += (n + 1) * (n + 1);
    }
    m
}

pub fn generate_basis(mesh: &mut Mesh) -> Result<()> {
    let mut sums = vec![(Vec3::ZERO, Vec3::ZERO, 0.0f32); mesh.vertices.len()];
    ensure!(
        mesh.indices.len().is_multiple_of(3),
        "indices must contain triangles"
    );
    for tri in mesh.indices.chunks_exact(3) {
        ensure!(
            tri.iter().all(|i| (*i as usize) < mesh.vertices.len()),
            "triangle index out of range"
        );
        let a = mesh.vertices[tri[0] as usize];
        let b = mesh.vertices[tri[1] as usize];
        let c = mesh.vertices[tri[2] as usize];
        let p = Vec3::from(b.position) - Vec3::from(a.position);
        let q = Vec3::from(c.position) - Vec3::from(a.position);
        let r = q - p;
        let u = Vec3::from(b.uv) - Vec3::from(a.uv);
        let v = Vec3::from(c.uv) - Vec3::from(a.uv);
        let area = u.x * v.y - u.y * v.x;
        ensure!(
            p.is_finite()
                && q.is_finite()
                && area.is_finite()
                && p.cross(q).length() > 1e-10
                && area.abs() > 1e-10,
            "degenerate/non-finite object or UV triangle"
        );
        // Reference uses absolute UV area. Mirrored UVs are not an importer feature.
        let tangent = (p * v.y - q * u.y) / area.abs();
        let bitangent = (-p * v.x + q * u.x) / area.abs();
        let angle = |x: Vec3, y: Vec3| (x.dot(y) / (x.length() * y.length())).clamp(-1., 1.).acos();
        for (&i, w) in tri.iter().zip([angle(p, q), angle(-p, r), angle(-r, -q)]) {
            sums[i as usize].0 += tangent * w;
            sums[i as usize].1 += bitangent * w;
            sums[i as usize].2 += w;
        }
    }
    for (vertex, (t, b, w)) in mesh.vertices.iter_mut().zip(sums) {
        vertex.tangent = (t / w.max(1e-30)).to_array();
        vertex.bitangent = (b / w.max(1e-30)).to_array();
    }
    Ok(())
}

/// Original four-bone pose. Time is the explicit original animation clock.
pub fn bones(kind: MeshKind, time: f32, amount: f32, length: f32) -> [Mat4; 4] {
    let a = amount * (TAU * time / 12.).sin();
    let b = amount * (TAU * time / (12. * 0.763)).sin();
    let mut result = [Mat4::IDENTITY; 4];
    if kind.is_tube() {
        let forwards = [
            Vec3::X,
            Vec3::new(a, b, 1.),
            Vec3::new(0., a, 1.),
            Vec3::new(b, a, 1.),
        ];
        let origins = [0., 0.15 * length, 0.5 * length, 0.85 * length];
        for i in 0..4 {
            let forward = forwards[i].normalize();
            let right = Vec3::Y.cross(forward).normalize();
            let up = forward.cross(right).normalize();
            let rotation = Mat4::from_mat3(Mat3::from_cols(right, up, forward));
            let origin = Vec3::Z * origins[i];
            result[i] = if i == 0 {
                rotation
            } else {
                result[i - 1]
                    * Mat4::from_translation(origin)
                    * rotation
                    * Mat4::from_translation(-origin)
            };
        }
    } else {
        for (i, u) in [
            Vec3::Y,
            Vec3::new(a, 1., b),
            Vec3::new(-a, 1., 0.),
            Vec3::new(-b, 1., -a),
        ]
        .into_iter()
        .enumerate()
        {
            let up = u.normalize();
            let right = up.cross(Vec3::Z).normalize();
            let forward = right.cross(up).normalize();
            result[i] = Mat4::from_mat3(Mat3::from_cols(right, up, forward));
        }
    }
    result
}
