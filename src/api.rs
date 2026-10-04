//! Stable, headless Rust entry points for validating assets and rendering DASHR frames.
use crate::{
    asset, asset_format::AssetDocument, gpu_resources::GpuContext, passes::Renderer,
    settings::Settings,
};
use anyhow::{Result, ensure};
use serde_json::Value;
use std::path::{Path, PathBuf};

pub const API_VERSION: u32 = 1;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GpuOptions {
    /// Force manual bilinear loads when true.
    pub manual_filter: bool,
    /// Force a 1, 2 or 4 attachment split; zero selects the adapter policy.
    pub split_planes: u32,
    /// Optional exact-input cache for the static topology and seam maps.
    pub map_cache_path: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminationStatus {
    NotLaunched,
    Hit,
    Escaped,
    BudgetExhausted,
    InvalidBasis,
    DebugForcedHit,
}

impl TryFrom<u32> for TerminationStatus {
    type Error = u32;

    fn try_from(code: u32) -> std::result::Result<Self, Self::Error> {
        match code {
            0 => Ok(Self::NotLaunched),
            1 => Ok(Self::Hit),
            2 => Ok(Self::Escaped),
            3 => Ok(Self::BudgetExhausted),
            4 => Ok(Self::InvalidBasis),
            5 => Ok(Self::DebugForcedHit),
            unknown => Err(unknown),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TraceDiagnostic {
    pub status: TerminationStatus,
    pub steps: u32,
    pub teleports: u32,
    /// Hit height for primary rays or local-shadow distance for shadow rays.
    pub auxiliary: f32,
}

impl TraceDiagnostic {
    pub fn from_channels(channels: [f32; 4]) -> Result<Self> {
        ensure!(
            channels.iter().all(|channel| channel.is_finite()),
            "trace diagnostics contain a non-finite channel"
        );
        let integer = |value: f32, name: &str| -> Result<u32> {
            ensure!(
                value >= 0.0 && value.fract() == 0.0 && value <= u32::MAX as f32,
                "trace {name} channel is not a nonnegative integer: {value}"
            );
            Ok(value as u32)
        };
        let code = integer(channels[0], "status")?;
        let status = TerminationStatus::try_from(code)
            .map_err(|unknown| anyhow::anyhow!("unknown trace termination status {unknown}"))?;
        Ok(Self {
            status,
            steps: integer(channels[1], "step count")?,
            teleports: integer(channels[2], "teleport count")?,
            auxiliary: channels[3],
        })
    }
}

#[derive(Clone, Debug)]
pub struct FrameOutput {
    pub width: u32,
    pub height: u32,
    pub color: Vec<[f32; 4]>,
    pub hit_uv_distance_depth: Vec<[f32; 4]>,
    pub primary: Vec<TraceDiagnostic>,
    pub shadow: Vec<TraceDiagnostic>,
}

impl FrameOutput {
    fn from_planes(width: u32, height: u32, planes: Vec<Vec<[f32; 4]>>) -> Result<Self> {
        ensure!(
            planes.len() == 4,
            "renderer returned {} planes; expected four",
            planes.len()
        );
        let expected = (width as usize)
            .checked_mul(height as usize)
            .ok_or_else(|| anyhow::anyhow!("frame dimensions overflow addressable memory"))?;
        ensure!(
            planes.iter().all(|plane| plane.len() == expected),
            "renderer returned a plane with dimensions that do not match {width}x{height}"
        );
        let mut planes = planes.into_iter();
        let color = planes.next().unwrap();
        let hit_uv_distance_depth = planes.next().unwrap();
        let primary = planes
            .next()
            .unwrap()
            .into_iter()
            .map(TraceDiagnostic::from_channels)
            .collect::<Result<Vec<_>>>()?;
        let shadow = planes
            .next()
            .unwrap()
            .into_iter()
            .map(TraceDiagnostic::from_channels)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            width,
            height,
            color,
            hit_uv_distance_depth,
            primary,
            shadow,
        })
    }
}

/// Owns a headless renderer and its settings. Dropping the session releases its GPU resources.
pub struct DashrSession {
    renderer: Renderer,
    settings: Settings,
}

impl DashrSession {
    pub async fn headless(
        settings: Settings,
        asset_root: impl AsRef<Path>,
        options: GpuOptions,
    ) -> Result<Self> {
        settings.validate()?;
        let mesh = asset::procedural(
            settings.mesh,
            settings.around,
            settings.long,
            settings.length,
            settings.radius,
            settings.thickness,
        )?;
        let asset = AssetDocument::new(mesh)?;
        let renderer = Renderer::headless_with_asset_and_cache(
            &settings,
            asset_root.as_ref(),
            asset,
            options.manual_filter,
            options.split_planes,
            options.map_cache_path.as_deref(),
        )
        .await?;
        Ok(Self { renderer, settings })
    }

    pub async fn from_asset(
        settings: Settings,
        asset: AssetDocument,
        asset_root: impl AsRef<Path>,
        options: GpuOptions,
    ) -> Result<Self> {
        settings.validate()?;
        asset.validate()?;
        let renderer = Renderer::headless_with_asset_and_cache(
            &settings,
            asset_root.as_ref(),
            asset,
            options.manual_filter,
            options.split_planes,
            options.map_cache_path.as_deref(),
        )
        .await?;
        Ok(Self { renderer, settings })
    }

    /// Reuse a context created by the host. The session then owns its clone of the wgpu handles.
    pub fn with_context(
        context: GpuContext,
        settings: Settings,
        asset: AssetDocument,
        asset_root: impl AsRef<Path>,
    ) -> Result<Self> {
        Self::with_context_and_cache(context, settings, asset, asset_root, None)
    }

    pub fn with_context_and_cache(
        context: GpuContext,
        settings: Settings,
        asset: AssetDocument,
        asset_root: impl AsRef<Path>,
        cache_path: Option<&Path>,
    ) -> Result<Self> {
        settings.validate()?;
        asset.validate()?;
        let renderer = Renderer::with_asset_and_cache(
            context,
            &settings,
            asset_root.as_ref(),
            asset,
            cache_path,
        )?;
        Ok(Self { renderer, settings })
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn adapter_report(&self) -> &Value {
        &self.renderer.ctx.report
    }

    pub fn bake_ms(&self) -> f64 {
        self.renderer.bake_ms
    }

    pub fn seam_edges(&self) -> usize {
        self.renderer.seam_edges
    }

    pub fn last_timings(&self) -> Option<&Value> {
        self.renderer.last_timings.as_ref()
    }

    /// Update animation time and the reference four-bone animation amount.
    pub fn update_pose(&mut self, time: f32, animation_amount: f32) -> Result<()> {
        ensure!(time.is_finite(), "pose time must be finite");
        ensure!(
            animation_amount.is_finite(),
            "animation amount must be finite"
        );
        let mut settings = self.settings.clone();
        settings.time = time;
        settings.animation_amount = animation_amount;
        settings.validate()?;
        self.settings = settings;
        Ok(())
    }

    pub fn resize(&mut self, width: u32, height: u32) -> Result<()> {
        let mut settings = self.settings.clone();
        settings.width = width;
        settings.height = height;
        settings.validate()?;
        self.renderer.resize(width, height)?;
        self.settings = settings;
        Ok(())
    }

    /// Render synchronously and read the four documented output/diagnostic planes to the CPU.
    pub fn render(&mut self) -> Result<FrameOutput> {
        let planes = self.renderer.render_capture(&self.settings)?;
        FrameOutput::from_planes(self.settings.width, self.settings.height, planes)
    }
}
