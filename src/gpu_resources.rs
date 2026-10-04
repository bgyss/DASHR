//! Deliberate native resource policy. Never silently lower warp precision.
use anyhow::{Result, ensure};
use serde::Serialize;
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Policy {
    pub planes: u32,
    pub filtered: bool,
    pub requested_bytes: u32,
}
pub fn select_policy(
    max_bytes: u32,
    filterable: bool,
    manual: bool,
    force_planes: u32,
) -> Result<Policy> {
    ensure!(
        [0, 1, 2, 4].contains(&force_planes),
        "split width must be 0, 1, 2 or 4"
    );
    let planes = if force_planes != 0 {
        force_planes
    } else if max_bytes >= 64 {
        4
    } else if max_bytes >= 32 {
        2
    } else {
        1
    };
    ensure!(
        max_bytes >= planes * 16,
        "RGBA32F render attachments require {} bytes/sample; adapter exposes {max_bytes}",
        planes * 16
    );
    Ok(Policy {
        planes,
        filtered: filterable && !manual,
        requested_bytes: planes * 16,
    })
}

use serde_json::{Value, json};
use std::{
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};
use wgpu::util::DeviceExt;
#[derive(Clone)]
pub struct GpuContext {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub policy: Policy,
    pub report: Value,
    errors: Arc<Mutex<Vec<String>>>,
}
impl GpuContext {
    pub fn instance() -> wgpu::Instance {
        wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        })
    }
    pub async fn new(
        instance: wgpu::Instance,
        surface: Option<&wgpu::Surface<'_>>,
        manual: bool,
        force_planes: u32,
    ) -> Result<Self> {
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: surface,
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            })
            .await?;
        let limits = adapter.limits();
        let features = adapter.features();
        let float_format = adapter.get_texture_format_features(wgpu::TextureFormat::Rgba32Float);
        let required_usages = wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST;
        ensure!(
            float_format.allowed_usages.contains(required_usages),
            "adapter cannot render/sample/read back full-float warp planes: {:?}",
            float_format.allowed_usages
        );
        let policy = select_policy(
            limits
                .max_color_attachment_bytes_per_sample
                .min(limits.max_color_attachments * 16),
            features.contains(wgpu::Features::FLOAT32_FILTERABLE),
            manual,
            force_planes,
        )?;
        let mut requested = wgpu::Features::empty();
        if policy.filtered {
            requested |= wgpu::Features::FLOAT32_FILTERABLE;
        }
        if features.contains(wgpu::Features::TIMESTAMP_QUERY) {
            requested |= wgpu::Features::TIMESTAMP_QUERY;
        }
        let required = wgpu::Limits {
            max_color_attachment_bytes_per_sample: policy.requested_bytes,
            max_color_attachments: policy.planes,
            max_texture_dimension_2d: limits.max_texture_dimension_2d.min(8192),
            ..Default::default()
        };
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("DASHR full-float device"),
                required_features: requested,
                required_limits: required,
                ..Default::default()
            })
            .await?;
        let errors = Arc::new(Mutex::new(Vec::new()));
        let sink = errors.clone();
        device.on_uncaptured_error(Arc::new(move |error: wgpu::Error| {
            sink.lock().unwrap().push(error.to_string());
        }));
        let lost = errors.clone();
        device.set_device_lost_callback(move |reason, message| {
            lost.lock()
                .unwrap()
                .push(format!("device lost: {reason:?}: {message}"));
        });
        let info = adapter.get_info();
        let report = json!({
            "name":info.name,"vendor":info.vendor,"device":info.device,"device_type":format!("{:?}",info.device_type),
            "backend":format!("{:?}",info.backend),"driver":info.driver,"driver_info":info.driver_info,
            "adapter_features":format!("{features:?}"),"enabled_features":format!("{requested:?}"),
            "adapter_max_color_attachment_bytes_per_sample":limits.max_color_attachment_bytes_per_sample,
            "adapter_max_color_attachments":limits.max_color_attachments,
            "requested_color_attachment_bytes_per_sample":policy.requested_bytes,
            "requested_max_color_attachments":policy.planes,
            "adapter_max_texture_dimension_2d":limits.max_texture_dimension_2d,
            "requested_max_texture_dimension_2d":device.limits().max_texture_dimension_2d,
            "rgba32float_usages":format!("{:?}",float_format.allowed_usages),"rgba32float_flags":format!("{:?}",float_format.flags),
            "policy":policy,"timestamp_period_ns":queue.get_timestamp_period()
        });
        Ok(Self {
            instance,
            adapter,
            device,
            queue,
            policy,
            report,
            errors,
        })
    }
    pub fn check(&self) -> Result<()> {
        let errors = self.errors.lock().unwrap();
        ensure!(
            errors.is_empty(),
            "wgpu validation/device failure: {}",
            errors.join("\n")
        );
        Ok(())
    }
    pub fn wait(&self) -> Result<()> {
        self.device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(30)),
        })?;
        self.check()
    }
    pub fn buffer(&self, label: &str, data: &[u8], usage: wgpu::BufferUsages) -> wgpu::Buffer {
        self.device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents: data,
                usage,
            })
    }
    pub fn map_buffer(&self, buffer: &wgpu::Buffer) -> Result<Vec<u8>> {
        let (tx, rx) = mpsc::channel();
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = tx.send(result);
            });
        self.wait()?;
        rx.recv_timeout(Duration::from_secs(2))??;
        let data = buffer.slice(..).get_mapped_range()?.to_vec();
        buffer.unmap();
        Ok(data)
    }
    pub fn read_texture(&self, texture: &wgpu::Texture, pixel_bytes: u32) -> Result<Vec<u8>> {
        let width = texture.width();
        let height = texture.height();
        let pitch = (width * pixel_bytes).div_ceil(256) * 256;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("padded image readback"),
            size: pitch as u64 * height as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        let aspect = if texture.format() == wgpu::TextureFormat::Depth32Float {
            wgpu::TextureAspect::DepthOnly
        } else {
            wgpu::TextureAspect::All
        };
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(pitch),
                    rows_per_image: Some(height),
                },
            },
            texture.size(),
        );
        self.queue.submit([encoder.finish()]);
        let padded = self.map_buffer(&buffer)?;
        let mut packed = Vec::with_capacity((width * height * pixel_bytes) as usize);
        for row in padded.chunks_exact(pitch as usize) {
            packed.extend_from_slice(&row[..(width * pixel_bytes) as usize]);
        }
        Ok(packed)
    }
    pub fn read_float(&self, texture: &wgpu::Texture) -> Result<Vec<[f32; 4]>> {
        let bytes = self.read_texture(texture, 16)?;
        Ok(bytes
            .chunks_exact(16)
            .map(|p| {
                std::array::from_fn(|i| f32::from_le_bytes(p[i * 4..i * 4 + 4].try_into().unwrap()))
            })
            .collect())
    }
    pub fn read_half_float(&self, texture: &wgpu::Texture) -> Result<Vec<[f32; 4]>> {
        let bytes = self.read_texture(texture, 8)?;
        Ok(bytes
            .chunks_exact(8)
            .map(|pixel| {
                std::array::from_fn(|channel| {
                    let offset = channel * 2;
                    half::f16::from_bits(u16::from_le_bytes([pixel[offset], pixel[offset + 1]]))
                        .to_f32()
                })
            })
            .collect())
    }
}
pub struct Texture {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
}
impl Texture {
    pub fn new(
        ctx: &GpuContext,
        label: &str,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
        usage: wgpu::TextureUsages,
    ) -> Result<Self> {
        ensure!(
            width > 0
                && height > 0
                && width <= ctx.device.limits().max_texture_dimension_2d
                && height <= ctx.device.limits().max_texture_dimension_2d,
            "{label}: unsupported texture size {width}x{height}"
        );
        let texture = ctx.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Ok(Self { texture, view })
    }
    pub fn float(ctx: &GpuContext, label: &str, width: u32, height: u32) -> Result<Self> {
        Self::new(
            ctx,
            label,
            width,
            height,
            wgpu::TextureFormat::Rgba32Float,
            wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
        )
    }
    pub fn float_storage(ctx: &GpuContext, label: &str, width: u32, height: u32) -> Result<Self> {
        Self::new(
            ctx,
            label,
            width,
            height,
            wgpu::TextureFormat::Rgba32Float,
            wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::STORAGE_BINDING,
        )
    }
    pub fn float16(ctx: &GpuContext, label: &str, width: u32, height: u32) -> Result<Self> {
        Self::new(
            ctx,
            label,
            width,
            height,
            wgpu::TextureFormat::Rgba16Float,
            wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
        )
    }
    pub fn upload(&self, ctx: &GpuContext, pixels: &[u8], bytes_per_pixel: u32) {
        ctx.queue.write_texture(
            self.texture.as_image_copy(),
            pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(self.texture.width() * bytes_per_pixel),
                rows_per_image: Some(self.texture.height()),
            },
            self.texture.size(),
        );
    }
}
