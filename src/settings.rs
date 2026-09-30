use crate::asset::MeshKind;
use anyhow::{Result, ensure};
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3, Vec4};
use serde::{Deserialize, Serialize};

#[repr(C, align(16))]
#[derive(Clone, Copy, Pod, Zeroable, Serialize)]
pub struct Uniforms {
    pub projection: [[f32; 4]; 4],
    pub camera_from_object: [[f32; 4]; 4],
    pub object_from_camera: [[f32; 4]; 4],
    pub height_step: [f32; 4],
    pub bones: [[[f32; 4]; 4]; 4],
    pub sun_atlas: [f32; 4],
    pub modes: [i32; 4],
    pub damping_extrusion: [f32; 4],
    pub lighting: [f32; 4],
    pub control: [i32; 4],
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
        }
    }
}
impl Settings {
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
            (1..=2048).contains(&self.width) && (1..=2048).contains(&self.height),
            "render dimensions must be 1..2048"
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
            Vec3::from(self.target).distance(Vec3::from(self.camera)) > self.near
                && (Vec3::from(self.target) - Vec3::from(self.camera))
                    .normalize()
                    .cross(Vec3::Y)
                    .length()
                    > 0.01,
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
        Ok(())
    }
    pub fn uniforms(&self, diagnostics: bool) -> Uniforms {
        let view = Mat4::look_at_lh(Vec3::from(self.camera), Vec3::from(self.target), Vec3::Y);
        let inv = view.inverse();
        let sun_angle = std::f32::consts::TAU * 8. / 19.;
        let sun = Vec3::new(sun_angle.sin(), 0.5, sun_angle.cos()).normalize();
        Uniforms {
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
                self.delta_uv,
                self.shadow_acne,
                self.indirect,
                self.normal_scale,
            ],
            control: [
                self.teleport_iterations,
                i32::from(self.hit_depth),
                self.step_budget,
                i32::from(diagnostics),
            ],
        }
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
