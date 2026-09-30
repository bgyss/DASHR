use dashr::{asset::MeshKind, settings::Settings};
use winit::event::WindowEvent;

fn slider<'a, N: egui::emath::Numeric>(
    value: &'a mut N,
    range: std::ops::RangeInclusive<N>,
) -> egui::Slider<'a> {
    egui::Slider::new(value, range).clamping(egui::SliderClamping::Edits)
}

fn owns_input(event: &WindowEvent, consumed: bool, pointer: bool, keyboard: bool) -> bool {
    match event {
        WindowEvent::KeyboardInput { .. } | WindowEvent::Ime(_) => consumed || keyboard,
        WindowEvent::MouseInput { .. }
        | WindowEvent::MouseWheel { .. }
        | WindowEvent::CursorMoved { .. } => consumed || pointer,
        _ => false,
    }
}

#[derive(Clone, Debug, PartialEq)]
struct PendingResources {
    mesh: MeshKind,
    around: u32,
    long: u32,
    length: f32,
    radius: f32,
    thickness: f32,
    atlas: u32,
    texture_set: u32,
}
impl From<&Settings> for PendingResources {
    fn from(s: &Settings) -> Self {
        Self {
            mesh: s.mesh,
            around: s.around,
            long: s.long,
            length: s.length,
            radius: s.radius,
            thickness: s.thickness,
            atlas: s.atlas,
            texture_set: s.texture_set,
        }
    }
}
impl PendingResources {
    fn candidate(&self, settings: &Settings) -> anyhow::Result<Settings> {
        let mut candidate = settings.clone();
        candidate.mesh = self.mesh;
        candidate.around = self.around;
        candidate.long = self.long;
        candidate.length = self.length;
        candidate.radius = self.radius;
        candidate.thickness = self.thickness;
        candidate.atlas = self.atlas;
        candidate.texture_set = self.texture_set;
        if self != &Self::from(settings) {
            candidate.uniform_override = None;
        }
        candidate.validate()?;
        Ok(candidate)
    }
}

use anyhow::Result;
use dashr::{gpu_resources::GpuContext, passes::Renderer};
use winit::window::Window;

pub(super) struct Actions {
    pub apply: Option<Settings>,
    pub capture: bool,
    pub save: bool,
    pub load: Option<String>,
}
pub(super) struct Overlay {
    context: egui::Context,
    input: egui_winit::State,
    painter: egui_wgpu::Renderer,
    pending: PendingResources,
    pub message: String,
    pub visible: bool,
    sun_paused: bool,
    settings_path: String,
}
impl Overlay {
    pub fn new(
        ctx: &GpuContext,
        window: &Window,
        format: wgpu::TextureFormat,
        settings: &Settings,
    ) -> Self {
        let context = egui::Context::default();
        let input = egui_winit::State::new(
            context.clone(),
            egui::ViewportId::ROOT,
            window,
            Some(window.scale_factor() as f32),
            window.theme(),
            Some(ctx.device.limits().max_texture_dimension_2d as usize),
        );
        let painter =
            egui_wgpu::Renderer::new(&ctx.device, format, egui_wgpu::RendererOptions::default());
        Self {
            context,
            input,
            painter,
            pending: settings.into(),
            message: String::new(),
            visible: true,
            sun_paused: true,
            settings_path: "out/viewer-settings.json".into(),
        }
    }
    pub fn event(&mut self, window: &Window, event: &WindowEvent) -> bool {
        let response = self.input.on_window_event(window, event);
        owns_input(
            event,
            response.consumed,
            self.context.egui_wants_pointer_input(),
            self.context.egui_wants_keyboard_input(),
        )
    }
    pub fn sync_resources(&mut self, s: &Settings) {
        self.pending = s.into();
    }
    pub fn pause_sun(&mut self) {
        self.sun_paused = true;
    }
    pub fn advance_sun(&self, s: &mut Settings, delta: f32) {
        if !self.sun_paused {
            s.sun_time += delta;
        }
    }
    pub fn frame(
        &mut self,
        window: &Window,
        s: &mut Settings,
        paused: &mut bool,
        view: &mut usize,
    ) -> (egui::FullOutput, Actions) {
        let input = self.input.take_egui_input(window);
        let mut actions = Actions {
            apply: None,
            capture: false,
            save: false,
            load: None,
        };
        let mut release = false;
        let mut candidate = s.clone();
        let output = self.context.run_ui(input, |root_ui| {
            let ctx = root_ui.ctx();
            if !self.visible {
                return;
            }
            egui::Window::new("Skinned Heightfield")
                .default_pos([12., 12.])
                .default_width(340.)
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(650.)
                        .show(ui, |ui| {
                            ui.label("DASHR Rust / wgpu");
                            if candidate.uniform_override.is_some() {
                                ui.label("Reference snapshot: exact camera, pose and sun frozen");
                                if ui.button("Release reference pose").clicked() {
                                    release = true;
                                }
                            }
                            egui::ComboBox::from_label("Debug mode")
                                .selected_text(
                                    [
                                        "Off",
                                        "Vertex height",
                                        "Vertex albedo",
                                        "Step counts",
                                        "UV grid",
                                        "Anim distortion",
                                    ][candidate.debug as usize],
                                )
                                .show_ui(ui, |ui| {
                                    for (i, label) in [
                                        "Off",
                                        "Vertex height",
                                        "Vertex albedo",
                                        "Step counts",
                                        "UV grid",
                                        "Anim distortion",
                                    ]
                                    .iter()
                                    .enumerate()
                                    {
                                        ui.selectable_value(&mut candidate.debug, i as i32, *label);
                                    }
                                });
                            egui::ComboBox::from_label("Lighting mode")
                                .selected_text(
                                    [
                                        "Unlit height",
                                        "Surface normal",
                                        "Height deltas",
                                        "Normal map",
                                        "Normal map + shadows",
                                    ][candidate.lighting_mode as usize],
                                )
                                .show_ui(ui, |ui| {
                                    for (i, label) in [
                                        "Unlit height",
                                        "Surface normal",
                                        "Height deltas",
                                        "Normal map",
                                        "Normal map + shadows",
                                    ]
                                    .iter()
                                    .enumerate()
                                    {
                                        ui.selectable_value(
                                            &mut candidate.lighting_mode,
                                            i as i32,
                                            *label,
                                        );
                                    }
                                });
                            egui::ComboBox::from_label("Distortion mode")
                                .selected_text(
                                    ["Affine mat4x3", "ObjectPos + mat3x3"]
                                        [candidate.distortion as usize],
                                )
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut candidate.distortion,
                                        0,
                                        "Affine mat4x3",
                                    );
                                    ui.selectable_value(
                                        &mut candidate.distortion,
                                        1,
                                        "ObjectPos + mat3x3",
                                    );
                                });
                            ui.separator();
                            ui.label("ANIMATION");
                            if ui.checkbox(paused, "Animation paused").changed() && !*paused {
                                release = true;
                            }
                            release |= ui
                                .add(slider(&mut candidate.time, 0.0..=120.0).text("Pose time"))
                                .changed();
                            release |= ui
                                .add(
                                    slider(&mut candidate.animation_amount, 0.0..=5.0)
                                        .text("Anim amount"),
                                )
                                .changed();
                            if ui
                                .checkbox(&mut self.sun_paused, "Sun animation paused")
                                .changed()
                                && !self.sun_paused
                            {
                                release = true;
                            }
                            release |= ui
                                .add(slider(&mut candidate.sun_time, 0.0..=40.0).text("Sun time"))
                                .changed();
                            release |= ui
                                .add(
                                    slider(&mut candidate.sun_period, 0.1..=40.0)
                                        .text("Sun period"),
                                )
                                .changed();
                            release |= ui
                                .add(
                                    slider(&mut candidate.sun_elevation, 0.0..=1.0)
                                        .text("Sun elevation"),
                                )
                                .changed();
                            ui.separator();
                            ui.label("MESH / TEXTURE — apply changes");
                            egui::ComboBox::from_label("Mesh")
                                .selected_text(format!("{:?}", self.pending.mesh))
                                .show_ui(ui, |ui| {
                                    for mesh in [
                                        MeshKind::Tube,
                                        MeshKind::Cube,
                                        MeshKind::TubePinched,
                                        MeshKind::CubePinched,
                                    ] {
                                        ui.selectable_value(
                                            &mut self.pending.mesh,
                                            mesh,
                                            format!("{mesh:?}"),
                                        );
                                    }
                                });
                            egui::ComboBox::from_label("Texture set")
                                .selected_text(
                                    ["Roof", "Rocks", "Demo", "Analytic"]
                                        [self.pending.texture_set as usize],
                                )
                                .show_ui(ui, |ui| {
                                    for (i, label) in
                                        ["Roof", "Rocks", "Demo", "Analytic"].iter().enumerate()
                                    {
                                        ui.selectable_value(
                                            &mut self.pending.texture_set,
                                            i as u32,
                                            *label,
                                        );
                                    }
                                });
                            ui.add(slider(&mut self.pending.long, 4..=64).text("Segments long"));
                            ui.add(
                                slider(&mut self.pending.around, 4..=32).text("Segments around"),
                            );
                            ui.add(
                                slider(&mut self.pending.length, 0.1..=10.0).text("Mesh length"),
                            );
                            ui.add(
                                slider(&mut self.pending.radius, 0.1..=10.0).text("Mesh radius"),
                            );
                            ui.add(
                                slider(&mut self.pending.thickness, 0.1..=10.0)
                                    .text("Surface thickness"),
                            );
                            let mut exponent = self.pending.atlas.ilog2();
                            if ui
                                .add(slider(&mut exponent, 4..=10).text("Atlas size pow2"))
                                .changed()
                            {
                                self.pending.atlas = 1 << exponent;
                            }
                            if ui
                                .button("Apply / rebuild (releases reference pose)")
                                .clicked()
                            {
                                match self.pending.candidate(&candidate) {
                                    Ok(next) => actions.apply = Some(next),
                                    Err(e) => self.message = format!("{e:#}"),
                                }
                            }
                            ui.label("Wireframe is not implemented in this renderer.");
                            ui.separator();
                            ui.label("RAY MARCHER");
                            ui.add(
                                slider(&mut candidate.step_size, 0.001..=0.05).text("Step size"),
                            );
                            ui.add(
                                slider(&mut candidate.step_scale, 0.0..=100.0).text("Step scale"),
                            );
                            ui.add(
                                slider(&mut candidate.height_scale, 0.0..=2.5).text("Height scale"),
                            );
                            ui.add(
                                slider(&mut candidate.height_offset, -1.0..=1.0)
                                    .text("Height offset"),
                            );
                            ui.add(
                                slider(&mut candidate.extra_extrusion, 0.0..=1.0)
                                    .text("Extra extrusion"),
                            );
                            ui.add(
                                slider(&mut candidate.teleport_iterations, 0..=10)
                                    .text("Iterations after teleport"),
                            );
                            ui.add(
                                slider(&mut candidate.debug_max_steps, -1..=100)
                                    .text("Debug forced-hit step"),
                            );
                            ui.add(
                                slider(&mut candidate.step_budget, 1..=10000)
                                    .text("Production budget"),
                            );
                            ui.add(
                                slider(&mut candidate.damping[0], 0.01..=5.0)
                                    .text("Damping factor 1"),
                            );
                            ui.add(
                                slider(&mut candidate.damping[1], -10.0..=1.0)
                                    .text("Damping factor 2"),
                            );
                            ui.add(
                                slider(&mut candidate.damping[2], 0.01..=5.0)
                                    .text("Damping factor 3"),
                            );
                            ui.separator();
                            ui.label("LIGHTING / DISPLAY");
                            ui.add(
                                slider(&mut candidate.indirect, 0.0..=1.0)
                                    .text("Indirect lighting"),
                            );
                            ui.add(
                                slider(&mut candidate.shadow_acne, 0.0..=0.1)
                                    .text("Shadow acne offset"),
                            );
                            ui.add(
                                slider(&mut candidate.normal_scale, 0.0..=2.0)
                                    .text("Height-normal scale"),
                            );
                            ui.add(
                                slider(&mut candidate.delta_uv, 0.0001..=0.1)
                                    .text("Normal delta UV"),
                            );
                            ui.checkbox(&mut candidate.hit_depth, "Experimental hit depth");
                            ui.color_edit_button_rgb(&mut candidate.background);
                            egui::ComboBox::from_label("Inspect map")
                                .selected_text(
                                    [
                                        "Color", "Teleport", "Gutter", "Warp 0", "Warp 1",
                                        "Warp 2", "Warp 3",
                                    ][*view],
                                )
                                .show_ui(ui, |ui| {
                                    for (i, label) in [
                                        "Color", "Teleport", "Gutter", "Warp 0", "Warp 1",
                                        "Warp 2", "Warp 3",
                                    ]
                                    .iter()
                                    .enumerate()
                                    {
                                        ui.selectable_value(view, i, *label);
                                    }
                                });
                            ui.collapsing("Camera", |ui| {
                                for (name, vec) in [
                                    ("Camera", &mut candidate.camera),
                                    ("Target", &mut candidate.target),
                                ] {
                                    ui.label(name);
                                    ui.horizontal(|ui| {
                                        for value in vec.iter_mut() {
                                            release |= ui
                                                .add(egui::DragValue::new(value).speed(0.05))
                                                .changed();
                                        }
                                    });
                                }
                                release |= ui
                                    .add(
                                        slider(&mut candidate.fov_y, 10.0..=120.0)
                                            .text("Vertical FOV"),
                                    )
                                    .changed();
                            });
                            ui.separator();
                            ui.label("COMPARISON");
                            ui.text_edit_singleline(&mut self.settings_path);
                            ui.horizontal(|ui| {
                                actions.save |= ui.button("Save default settings").clicked();
                                if ui.button("Load snapshot / settings").clicked() {
                                    actions.load = Some(self.settings_path.clone());
                                }
                                actions.capture |= ui.button("Capture clean frame").clicked();
                            });
                            if !self.message.is_empty() {
                                ui.label(&self.message);
                            }
                            ui.label("F1: show/hide controls · Space: pause · drag: orbit");
                        });
                });
        });
        if release {
            candidate.uniform_override = None;
            self.message = "Reference pose released; using editable camera/animation/sun".into();
        }
        match candidate.validate() {
            Ok(()) => *s = candidate,
            Err(e) => self.message = format!("{e:#}"),
        }
        self.input
            .handle_platform_output(window, output.platform_output.clone());
        (output, actions)
    }
    pub fn paint(
        &mut self,
        renderer: &Renderer,
        window: &Window,
        view: &wgpu::TextureView,
        mut output: egui::FullOutput,
    ) -> Result<()> {
        let ctx = &renderer.ctx;
        for (id, deltas) in std::mem::take(&mut output.textures_delta.set) {
            for delta in deltas {
                self.painter
                    .update_texture(&ctx.device, &ctx.queue, id, &delta);
            }
        }
        let jobs = self
            .context
            .tessellate(std::mem::take(&mut output.shapes), output.pixels_per_point);
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [window.inner_size().width, window.inner_size().height],
            pixels_per_point: output.pixels_per_point,
        };
        let mut encoder = ctx.device.create_command_encoder(&Default::default());
        let mut commands =
            self.painter
                .update_buffers(&ctx.device, &ctx.queue, &mut encoder, &jobs, &screen);
        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("control overlay"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                })
                .forget_lifetime();
            self.painter.render(&mut pass, &jobs, &screen);
        }
        commands.push(encoder.finish());
        ctx.queue.submit(commands);
        for id in std::mem::take(&mut output.textures_delta.free) {
            self.painter.free_texture(&id);
        }
        ctx.check()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn displaying_slider_preserves_unedited_reference_values() {
        let mut step_size = 0.1_f32;
        let mut output = egui::Context::default().run_ui(Default::default(), |ui| {
            ui.add(slider(&mut step_size, 0.001..=0.05));
        });
        output.textures_delta.set.clear();
        output.textures_delta.free.clear();
        assert_eq!(step_size, 0.1);
    }

    #[test]
    fn gui_keyboard_focus_blocks_shortcuts_but_does_not_block_resize() {
        assert!(owns_input(
            &WindowEvent::Ime(winit::event::Ime::Enabled),
            false,
            false,
            true
        ));
        assert!(!owns_input(
            &WindowEvent::Resized(winit::dpi::PhysicalSize::new(100, 100)),
            true,
            true,
            true
        ));
    }

    #[test]
    fn gui_pointer_ownership_blocks_camera_wheel() {
        let wheel = WindowEvent::MouseWheel {
            device_id: winit::event::DeviceId::dummy(),
            delta: winit::event::MouseScrollDelta::LineDelta(0., 1.),
            phase: winit::event::TouchPhase::Moved,
        };
        assert!(owns_input(&wheel, false, true, false));
        assert!(!owns_input(&wheel, false, false, true));
    }

    #[test]
    fn staged_geometry_requires_apply_and_preserves_render_edits() {
        let mut settings = Settings::default();
        let mut pending = PendingResources::from(&settings);
        pending.around = 16;
        pending.atlas = 512;
        assert_eq!(settings.around, 8);
        assert_eq!(settings.atlas, 256);
        settings.height_scale = 1.7;
        let applied = pending.candidate(&settings).unwrap();
        assert_eq!(applied.around, 16);
        assert_eq!(applied.atlas, 512);
        assert_eq!(applied.height_scale, 1.7);
    }

    #[test]
    fn invalid_pending_resources_leave_active_settings_intact() {
        let settings = Settings::default();
        let mut pending = PendingResources::from(&settings);
        pending.atlas = 123;
        assert!(pending.candidate(&settings).is_err());
        assert_eq!(settings.atlas, 256);
    }
}
