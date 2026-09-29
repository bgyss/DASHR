use anyhow::{Result, anyhow};
use dashr::{
    asset::MeshKind, capture, gpu_resources::GpuContext, passes::Renderer, settings::Settings,
};
use glam::{Quat, Vec3};
use std::{path::PathBuf, sync::Arc, time::Instant};
use winit::{
    application::ApplicationHandler,
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

struct Presentation {
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
}
impl Presentation {
    fn new(ctx: &GpuContext, format: wgpu::TextureFormat) -> Self {
        let layout = ctx
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("present float color"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                }],
            });
        let pipeline_layout = ctx
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("presentation"),
                bind_group_layouts: &[Some(&layout)],
                immediate_size: 0,
            });
        let source = format!(
            r#"
            @group(0) @binding(0) var image:texture_2d<f32>;
            struct Output{{@builtin(position) clip:vec4<f32>,@location(0) uv:vec2<f32>}}
            @vertex fn vs(@builtin(vertex_index) i:u32)->Output{{
                let uv=vec2<f32>(f32((i<<1u)&2u),f32(i&2u));
                return Output(vec4<f32>(uv.x*2.0-1.0,1.0-uv.y*2.0,0.0,1.0),uv);
            }}
            @fragment fn fs(v:Output)->@location(0) vec4<f32>{{
                let dims=vec2<i32>(textureDimensions(image));
                var color=clamp(textureLoad(image,clamp(vec2<i32>(v.uv*vec2<f32>(dims)),vec2<i32>(0),dims-vec2<i32>(1)),0).rgb,vec3<f32>(0.0),vec3<f32>(1.0));
                if({}){{color=select(pow((color+vec3<f32>(0.055))/1.055,vec3<f32>(2.4)),color/12.92,color<=vec3<f32>(0.04045));}}
                return vec4<f32>(color,1.0);
            }}
        "#,
            format.is_srgb()
        );
        let module = ctx
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("unorm-compatible display"),
                source: wgpu::ShaderSource::Wgsl(source.into()),
            });
        let pipeline = ctx
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("presentation pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("fs"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            });
        Self { layout, pipeline }
    }
    fn draw(&self, renderer: &Renderer, output: &wgpu::TextureView, view: usize) -> Result<()> {
        let texture = match view {
            0 => &renderer.frame.planes[0],
            1 => &renderer.teleport,
            2 => &renderer.edgefill,
            _ => &renderer.warp[view - 3],
        };
        let group = renderer
            .ctx
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("selected display image"),
                layout: &self.layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture.view),
                }],
            });
        let mut encoder = renderer
            .ctx
            .device
            .create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("present selected image"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: output,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.draw(0..3, 0..1);
        }
        renderer.ctx.queue.submit([encoder.finish()]);
        renderer.ctx.check()
    }
}
struct State {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    renderer: Renderer,
    presentation: Presentation,
    last_tick: Instant,
    dragging: bool,
    cursor: Option<(f64, f64)>,
    view: usize,
    presented: u32,
}
struct App {
    settings: Settings,
    assets: PathBuf,
    manual: bool,
    split: u32,
    state: Option<State>,
    error: Option<anyhow::Error>,
    paused: bool,
    exit_after: Option<u32>,
}
impl App {
    fn fail(&mut self, event_loop: &ActiveEventLoop, error: anyhow::Error) {
        eprintln!("{error:#}");
        self.error = Some(error);
        event_loop.exit();
    }
    fn title(&self) {
        if let Some(state) = &self.state {
            let views = [
                "Color",
                "Teleport UV / sign",
                "Gutter UV",
                "Warp 0",
                "Warp 1",
                "Warp 2",
                "Warp 3",
            ];
            state.window.set_title(&format!("DASHR | {:?} | {} | light {} | distortion {} | {} | Space pause · drag orbit · 1–4 mesh · T material · D debug · L light · M distortion · Tab atlas · C capture",
                self.settings.mesh,views[state.view],self.settings.lighting_mode,self.settings.distortion,if self.paused{"paused"}else{"animated"}));
        }
    }
    fn resize(&mut self, width: u32, height: u32) -> Result<()> {
        if let Some(state) = self.state.as_mut() {
            if width == 0 || height == 0 {
                return Ok(());
            }
            state.config.width = width;
            state.config.height = height;
            state
                .surface
                .configure(&state.renderer.ctx.device, &state.config);
            let scale = (width.max(height) as f64 / 2048.).max(1.);
            self.settings.width = (width as f64 / scale).round() as u32;
            self.settings.height = (height as f64 / scale).round() as u32;
            state
                .renderer
                .resize(self.settings.width, self.settings.height)?;
        }
        Ok(())
    }
    fn rebuild(&mut self) -> Result<()> {
        if let Some(state) = self.state.as_mut() {
            state.renderer =
                Renderer::new(state.renderer.ctx.clone(), &self.settings, &self.assets)?;
        }
        Ok(())
    }
    fn draw(&mut self, event_loop: &ActiveEventLoop) -> Result<()> {
        let Some(state) = self.state.as_mut() else {
            return Ok(());
        };
        let size = state.window.inner_size();
        if size.width == 0 || size.height == 0 {
            return Ok(());
        }
        let now = Instant::now();
        let delta = now.duration_since(state.last_tick).as_secs_f32().min(0.1);
        state.last_tick = now;
        if !self.paused {
            self.settings.time += delta;
        }
        let texture = match state.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) => t,
            wgpu::CurrentSurfaceTexture::Suboptimal(t) => {
                drop(t);
                state
                    .surface
                    .configure(&state.renderer.ctx.device, &state.config);
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                state
                    .surface
                    .configure(&state.renderer.ctx.device, &state.config);
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                state.surface = state
                    .renderer
                    .ctx
                    .instance
                    .create_surface(state.window.clone())?;
                state
                    .surface
                    .configure(&state.renderer.ctx.device, &state.config);
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err(anyhow!("surface validation failed"));
            }
        };
        let _ = state.renderer.submit(&self.settings, false)?;
        self.settings.validate()?;
        self.presentation_draw(&texture.texture.create_view(&Default::default()))?;
        let state = self.state.as_mut().unwrap();
        state.window.pre_present_notify();
        state.renderer.ctx.queue.present(texture);
        state.presented += 1;
        if self
            .exit_after
            .is_some_and(|limit| state.presented >= limit)
        {
            event_loop.exit();
        }
        Ok(())
    }
    fn presentation_draw(&self, output: &wgpu::TextureView) -> Result<()> {
        let state = self.state.as_ref().unwrap();
        state.presentation.draw(&state.renderer, output, state.view)
    }
    fn key(&mut self, key: KeyCode, event_loop: &ActiveEventLoop) -> Result<()> {
        let mut rebuild = false;
        match key {
            KeyCode::Escape => event_loop.exit(),
            KeyCode::Space => self.paused = !self.paused,
            KeyCode::Digit1 => {
                self.settings.mesh = MeshKind::Tube;
                rebuild = true;
            }
            KeyCode::Digit2 => {
                self.settings.mesh = MeshKind::Cube;
                rebuild = true;
            }
            KeyCode::Digit3 => {
                self.settings.mesh = MeshKind::TubePinched;
                rebuild = true;
            }
            KeyCode::Digit4 => {
                self.settings.mesh = MeshKind::CubePinched;
                rebuild = true;
            }
            KeyCode::KeyT => {
                self.settings.texture_set = (self.settings.texture_set + 1) % 4;
                rebuild = true;
            }
            KeyCode::KeyD => self.settings.debug = (self.settings.debug + 1) % 6,
            KeyCode::KeyL => self.settings.lighting_mode = (self.settings.lighting_mode + 1) % 5,
            KeyCode::KeyM => self.settings.distortion = 1 - self.settings.distortion,
            KeyCode::KeyH => self.settings.hit_depth = !self.settings.hit_depth,
            KeyCode::Tab => {
                if let Some(state) = self.state.as_mut() {
                    state.view = (state.view + 1) % 7;
                }
            }
            KeyCode::BracketLeft => {
                self.settings.height_scale = (self.settings.height_scale - 0.1).max(0.)
            }
            KeyCode::BracketRight => {
                self.settings.height_scale = (self.settings.height_scale + 0.1).min(2.5)
            }
            KeyCode::KeyC => capture::run(
                std::path::Path::new("out/viewer-capture"),
                &self.settings,
                &self.assets,
                &capture::CaptureOptions {
                    manual_filter: self.manual,
                    split: self.split,
                    frames: 1,
                    time_step: 0.,
                    raw_frames: true,
                },
            )?,
            KeyCode::KeyS => {
                std::fs::create_dir_all("out")?;
                std::fs::write(
                    "out/viewer-settings.json",
                    serde_json::to_vec_pretty(&self.settings)?,
                )?;
                eprintln!("Saved out/viewer-settings.json");
            }
            _ => (),
        }
        if rebuild {
            self.rebuild()?;
        }
        self.title();
        Ok(())
    }
}
impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }
        let init = || -> Result<State> {
            let window = Arc::new(
                event_loop.create_window(
                    Window::default_attributes()
                        .with_title("DASHR — preparing native renderer")
                        .with_inner_size(winit::dpi::PhysicalSize::new(
                            self.settings.width,
                            self.settings.height,
                        )),
                )?,
            );
            let instance = GpuContext::instance();
            let surface = instance.create_surface(window.clone())?;
            let ctx = pollster::block_on(GpuContext::new(
                instance,
                Some(&surface),
                self.manual,
                self.split,
            ))?;
            let size = window.inner_size();
            let config = surface
                .get_default_config(&ctx.adapter, size.width.max(1), size.height.max(1))
                .ok_or_else(|| anyhow!("no compatible surface format"))?;
            let renderer = Renderer::new(ctx, &self.settings, &self.assets)?;
            let presentation = Presentation::new(&renderer.ctx, config.format);
            surface.configure(&renderer.ctx.device, &config);
            Ok(State {
                window,
                surface,
                config,
                renderer,
                presentation,
                last_tick: Instant::now(),
                dragging: false,
                cursor: None,
                view: 0,
                presented: 0,
            })
        };
        match init() {
            Ok(state) => {
                let size = state.window.inner_size();
                self.state = Some(state);
                if let Err(error) = self.resize(size.width, size.height) {
                    self.fail(event_loop, error);
                    return;
                }
                self.title();
            }
            Err(error) => self.fail(event_loop, error),
        }
    }
    fn suspended(&mut self, _: &ActiveEventLoop) {
        self.state = None;
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if self.state.as_ref().is_none_or(|s| s.window.id() != id) {
            return;
        }
        let result = match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
                Ok(())
            }
            WindowEvent::Resized(size) => self.resize(size.width, size.height),
            WindowEvent::RedrawRequested => self.draw(event_loop),
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed && !event.repeat =>
            {
                if let PhysicalKey::Code(code) = event.physical_key {
                    self.key(code, event_loop)
                } else {
                    Ok(())
                }
            }
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state,
                ..
            } => {
                self.state.as_mut().unwrap().dragging = state == ElementState::Pressed;
                Ok(())
            }
            WindowEvent::CursorMoved { position, .. } => {
                let state = self.state.as_mut().unwrap();
                if state.dragging
                    && let Some((x, y)) = state.cursor
                {
                    let target = Vec3::from(self.settings.target);
                    let offset = Vec3::from(self.settings.camera) - target;
                    let yaw = Quat::from_rotation_y((x - position.x) as f32 * 0.006);
                    let right = Vec3::Y.cross(offset).normalize();
                    let pitch = Quat::from_axis_angle(right, (position.y - y) as f32 * 0.006);
                    let moved = yaw * pitch * offset;
                    if moved.normalize().y.abs() < 0.98 {
                        self.settings.camera = (target + moved).to_array();
                    }
                }
                state.cursor = Some((position.x, position.y));
                Ok(())
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let amount = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 * 0.02,
                };
                let target = Vec3::from(self.settings.target);
                let offset = Vec3::from(self.settings.camera) - target;
                let length = (offset.length() * (-amount * 0.1).exp()).clamp(2.5, 100.);
                self.settings.camera = (target + offset.normalize() * length).to_array();
                Ok(())
            }
            _ => Ok(()),
        };
        if let Err(error) = result {
            self.fail(event_loop, error);
        }
    }
    fn about_to_wait(&mut self, _: &ActiveEventLoop) {
        if let Some(state) = &self.state {
            state.window.request_redraw();
        }
    }
}
pub fn run(
    settings: Settings,
    assets: PathBuf,
    manual: bool,
    split: u32,
    exit_after: Option<u32>,
) -> Result<()> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App {
        settings,
        assets,
        manual,
        split,
        state: None,
        error: None,
        paused: false,
        exit_after,
    };
    event_loop.run_app(&mut app)?;
    if let Some(error) = app.error {
        Err(error)
    } else {
        Ok(())
    }
}
