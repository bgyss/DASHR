use crate::asset::MeshKind;
use anyhow::{Result, ensure};
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3, Vec4};
use serde::{Deserialize, Serialize};

#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Uniforms {
    pub projection: [[f32; 4]; 4],
    pub camera_from_object: [[f32; 4]; 4],
    pub object_from_camera: [[f32; 4]; 4],
    pub height_step: [f32; 4],
    pub bones: [[[f32; 4]; 4]; 4],
    pub sun_atlas: [f32; 4],
    pub modes: [i32; 4], // debug, lighting, distortion, optional forced-hit step
    pub damping_extrusion: [f32; 4],
    pub lighting: [f32; 4],
    pub control: [i32; 4], // teleport iterations, hit-depth toggle, step budget, diagnostics bit + trace-option bits
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EpsilonPolicy {
    #[default]
    Reference,
    ScaleDerived,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResolvedTolerances {
    /// UV-space finite-difference offset, in normalized texture coordinates.
    pub normal_delta_uv: f32,
    /// Local self-shadow origin offset, in object-space units.
    pub local_shadow_bias_object_units: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub mesh: MeshKind,
    pub around: u32,
    pub long: u32,
    pub length: f32,
    pub radius: f32,
    pub thickness: f32,
    pub atlas: u32,
    pub width: u32,
    pub height: u32,
    pub texture_set: u32,
    pub time: f32,
    pub animation_amount: f32,
    pub camera: [f32; 3],
    pub target: [f32; 3],
    pub near: f32,
    pub fov_y: f32,
    pub height_scale: f32,
    pub height_offset: f32,
    pub step_size: f32,
    pub step_scale: f32,
    pub debug: i32,
    pub lighting_mode: i32,
    pub distortion: i32,
    pub debug_max_steps: i32,
    pub damping: [f32; 3],
    pub extra_extrusion: f32,
    pub delta_uv: f32,
    pub shadow_acne: f32,
    pub indirect: f32,
    pub normal_scale: f32,
    pub teleport_iterations: i32,
    pub step_budget: i32,
    pub hit_depth: bool,
    /// Use bounded same-chart and teleport-boundary hit refinement (U2).
    #[serde(default)]
    pub hit_refinement: bool,
    /// Shorten predicted steps that cross the signed teleport SDF (U3).
    #[serde(default)]
    pub seam_aware_stepping: bool,
    /// Retry predicted steps when consecutive inverse surface transforms change sharply (U4).
    #[serde(default)]
    pub adaptive_steps: bool,
    /// Chooses preserved reference tolerances or the opt-in asset-scale policy.
    #[serde(default)]
    pub epsilon_policy: EpsilonPolicy,
    /// Use the specialized 3x3 cofactor inverse in the tracing shader (U15).
    #[serde(default)]
    pub specialized_inverse: bool,
    /// Read a precomputed inverse surface basis from a dedicated atlas (U7).
    #[serde(default)]
    pub stored_inverse: bool,
    /// Generate gutter-filled dynamic maps with a full-atlas compute pass (U8).
    #[serde(default)]
    pub compute_edgefill: bool,
    /// Reconstruct guttered transforms from raw maps in the trace shader (U10).
    #[serde(default)]
    pub indirect_edgefill: bool,
    /// Store seam distance separately from nearest-sampled teleport destinations (U12).
    #[serde(default)]
    pub split_teleport: bool,
    /// Store dynamic transform atlas planes in RGBA16F rather than RGBA32F (U14).
    #[serde(default)]
    pub compact_warp: bool,
    pub background: [f32; 3],
    pub sun_time: f32,
    pub sun_period: f32,
    pub sun_elevation: f32,
    pub uniform_override: Option<Uniforms>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            mesh: MeshKind::Tube,
            around: 8,
            long: 8,
            length: 4.,
            radius: 1.,
            thickness: 1.,
            atlas: 256,
            width: 960,
            height: 720,
            texture_set: 0,
            time: 10.,
            animation_amount: 1.,
            camera: [3., 2., -8.],
            target: [1.5, 0., 0.],
            near: 0.1,
            fov_y: 60.,
            height_scale: 1.,
            height_offset: 0.,
            step_size: 0.02,
            step_scale: 10.,
            debug: 0,
            lighting_mode: 4,
            distortion: 1,
            debug_max_steps: -1,
            damping: [1., 0., 1.5],
            extra_extrusion: 0.25,
            delta_uv: 0.1,
            shadow_acne: 0.01,
            indirect: 0.2,
            normal_scale: 1.5,
            teleport_iterations: 0,
            step_budget: 10000,
            hit_depth: false,
            hit_refinement: false,
            seam_aware_stepping: false,
            adaptive_steps: false,
            epsilon_policy: EpsilonPolicy::Reference,
            specialized_inverse: false,
            stored_inverse: false,
            compute_edgefill: false,
            indirect_edgefill: false,
            split_teleport: false,
            compact_warp: false,
            background: [0.02, 0.025, 0.035],
            sun_time: 8.,
            sun_period: 19.,
            sun_elevation: 0.5,
            uniform_override: None,
        }
    }
}
impl Settings {
    pub fn resolved_tolerances(&self) -> ResolvedTolerances {
        match self.epsilon_policy {
            EpsilonPolicy::Reference => ResolvedTolerances {
                normal_delta_uv: self.delta_uv,
                local_shadow_bias_object_units: self.shadow_acne,
            },
            EpsilonPolicy::ScaleDerived => {
                let object_scale = self.length.max(2.0 * self.radius).max(self.thickness);
                ResolvedTolerances {
                    normal_delta_uv: 1.0 / self.atlas as f32,
                    local_shadow_bias_object_units: object_scale * 1e-3,
                }
            }
        }
    }

    pub fn load(path: &std::path::Path) -> Result<Self> {
        let value: serde_json::Value = serde_json::from_slice(&std::fs::read(path)?)?;
        if value.get("schema").and_then(|v| v.as_str()) == Some("dashr.reference.snapshot.v1") {
            ensure!(
                value["status"] == "captured",
                "reference snapshot failed; use a captured manifest"
            );
            ensure!(
                value["source"]["alpha_mode"].as_i64().unwrap_or(0) == 0,
                "reference alpha blending is not supported by the portable comparison path"
            );
            ensure!(
                value["source"]["flood_fill"].as_bool().unwrap_or(true),
                "reference snapshot without topology flood fill is unsupported"
            );
        }
        let settings: Self =
            serde_json::from_value(value.get("settings").cloned().unwrap_or(value))?;
        settings.validate()?;
        Ok(settings)
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            (4..=128).contains(&self.around) && (1..=128).contains(&self.long),
            "segments out of supported range"
        );
        ensure!(
            (16..=1024).contains(&self.atlas) && self.atlas.is_power_of_two(),
            "atlas must be a power of two in 16..1024"
        );
        ensure!(
            (1..=4096).contains(&self.width)
                && (1..=4096).contains(&self.height)
                && u64::from(self.width) * u64::from(self.height) <= 8_388_608,
            "render dimensions must be 1..4096 with at most 8,388,608 pixels"
        );
        ensure!(
            self.texture_set <= 3,
            "texture set must be 0 (roof), 1 (rocks), 2 (demo) or 3 (analytic)"
        );
        let numbers = [
            self.length,
            self.radius,
            self.thickness,
            self.time,
            self.animation_amount,
            self.near,
            self.fov_y,
            self.height_scale,
            self.height_offset,
            self.step_size,
            self.step_scale,
            self.extra_extrusion,
            self.delta_uv,
            self.shadow_acne,
            self.indirect,
            self.normal_scale,
        ];
        ensure!(
            numbers
                .iter()
                .chain(&self.camera)
                .chain(&self.target)
                .chain(&self.damping)
                .all(|x| x.is_finite()),
            "settings contain NaN/Inf"
        );
        ensure!(
            self.length > 0.
                && self.radius > 0.
                && self.thickness > 0.
                && self.near > 0.
                && (1.0..179.0).contains(&self.fov_y),
            "invalid mesh/camera dimensions"
        );
        ensure!(
            Vec3::from(self.camera).distance(Vec3::from(self.target)) > 0.01,
            "camera and target coincide"
        );
        ensure!(
            self.uniform_override.is_some()
                || (Vec3::from(self.target).distance(Vec3::from(self.camera)) > self.near
                    && (Vec3::from(self.target) - Vec3::from(self.camera))
                        .normalize()
                        .cross(Vec3::Y)
                        .length()
                        > 0.01),
            "unsupported camera up vector"
        );
        ensure!(
            self.height_scale >= 0.
                && self.step_size > 0.
                && self.step_scale >= 0.
                && self.extra_extrusion >= 0.
                && self.delta_uv > 0.
                && self.damping[0] > 0.
                && self.damping[2] > 0.,
            "invalid marching/damping values"
        );
        ensure!(
            (0..=5).contains(&self.debug)
                && (0..=4).contains(&self.lighting_mode)
                && (0..=1).contains(&self.distortion),
            "invalid shader mode"
        );
        ensure!(
            (-1..=10000).contains(&self.debug_max_steps)
                && (1..=10000).contains(&self.step_budget)
                && (0..=10).contains(&self.teleport_iterations),
            "invalid trace budget"
        );
        ensure!(
            !self.indirect_edgefill || (!self.compute_edgefill && !self.stored_inverse),
            "trace-time edgefill indirection cannot be combined with compute edgefill or stored inverse"
        );
        ensure!(
            !self.compact_warp || (!self.compute_edgefill && !self.stored_inverse),
            "compact warp storage cannot be combined with compute edgefill or stored inverse"
        );
        ensure!(
            self.background
                .iter()
                .all(|v| v.is_finite() && (0.0..=1.0).contains(v)),
            "background must contain finite normalized RGB"
        );
        ensure!(
            self.sun_time.is_finite()
                && self.sun_period.is_finite()
                && self.sun_period > 0.
                && self.sun_elevation.is_finite()
                && (0.0..=1.0).contains(&self.sun_elevation),
            "invalid sun settings"
        );
        if let Some(u) = &self.uniform_override {
            // Integer slots are regenerated from validated settings; interpreting their bits as floats
            // may produce NaN for the valid reference debug sentinel -1.
            ensure!(
                u.projection
                    .iter()
                    .flatten()
                    .chain(u.camera_from_object.iter().flatten())
                    .chain(u.object_from_camera.iter().flatten())
                    .chain(u.bones.iter().flatten().flatten())
                    .chain(u.height_step.iter())
                    .chain(u.sun_atlas.iter())
                    .chain(u.damping_extrusion.iter())
                    .chain(u.lighting.iter())
                    .all(|v| v.is_finite()),
                "non-finite reference uniform"
            );
            for matrix in [&u.projection, &u.camera_from_object, &u.object_from_camera] {
                ensure!(
                    Mat4::from_cols_array_2d(matrix).determinant().abs() > 1e-12,
                    "singular reference camera/projection"
                );
            }
            ensure!(
                (u.sun_atlas[3] - self.atlas as f32).abs() < 0.1,
                "reference atlas size differs from resource settings"
            );
            let sun = Vec3::new(u.sun_atlas[0], u.sun_atlas[1], u.sun_atlas[2]);
            ensure!(
                (sun.length() - 1.).abs() < 1e-4,
                "reference sun direction must be normalized"
            );
        }
        Ok(())
    }
    pub fn uniforms(&self, diagnostics: bool) -> Uniforms {
        let tolerances = self.resolved_tolerances();
        let view = Mat4::look_at_lh(Vec3::from(self.camera), Vec3::from(self.target), Vec3::Y);
        let inv = view.inverse();
        let sun_angle = std::f32::consts::TAU * (self.sun_time % self.sun_period) / self.sun_period;
        let horizontal = 1. - self.sun_elevation;
        let sun = Vec3::new(
            horizontal * sun_angle.sin(),
            self.sun_elevation,
            horizontal * sun_angle.cos(),
        )
        .normalize();
        let mut uniforms = Uniforms {
            projection: reverse_z(
                self.width as f32 / self.height as f32,
                self.fov_y,
                self.near,
            )
            .to_cols_array_2d(),
            camera_from_object: view.to_cols_array_2d(),
            object_from_camera: inv.to_cols_array_2d(),
            height_step: [
                self.height_scale,
                self.height_offset,
                self.step_size,
                self.step_scale,
            ],
            bones: crate::asset::bones(self.mesh, self.time, self.animation_amount, self.length)
                .map(|m| m.to_cols_array_2d()),
            sun_atlas: [sun.x, sun.y, sun.z, self.atlas as f32],
            modes: [
                self.debug,
                self.lighting_mode,
                self.distortion,
                self.debug_max_steps,
            ],
            damping_extrusion: [
                self.damping[0],
                self.damping[1],
                self.damping[2],
                self.extra_extrusion,
            ],
            lighting: [
                tolerances.normal_delta_uv,
                tolerances.local_shadow_bias_object_units,
                self.indirect,
                self.normal_scale,
            ],
            control: [
                self.teleport_iterations,
                i32::from(self.hit_depth),
                self.step_budget,
                i32::from(diagnostics)
                    | ((i32::from(self.hit_refinement)
                        | (i32::from(self.seam_aware_stepping) << 1)
                        | (i32::from(self.adaptive_steps) << 2)
                        | (i32::from(self.specialized_inverse) << 3)
                        | (i32::from(self.stored_inverse) << 4)
                        | (i32::from(self.indirect_edgefill) << 5)
                        | (i32::from(self.split_teleport) << 6))
                        << 1),
            ],
        };
        if let Some(reference) = self.uniform_override {
            uniforms.projection = reference.projection;
            uniforms.camera_from_object = reference.camera_from_object;
            uniforms.object_from_camera = reference.object_from_camera;
            uniforms.bones = reference.bones;
            uniforms.sun_atlas = reference.sun_atlas;
        }
        uniforms
    }
}
pub fn reverse_z(aspect: f32, fov: f32, near: f32) -> Mat4 {
    let sy = 1. / (0.5 * fov.to_radians()).tan();
    Mat4::from_cols(
        Vec4::new(sy / aspect, 0., 0., 0.),
        Vec4::new(0., sy, 0., 0.),
        Vec4::new(0., 0., 0., 1.),
        Vec4::new(0., 0., near, 0.),
    )
}
