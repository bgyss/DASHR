use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use dashr::{
    asset::MeshKind,
    capture,
    gpu_resources::GpuContext,
    probe::contracts,
    settings::{EpsilonPolicy, Settings},
};
use std::path::PathBuf;

mod viewer;
#[derive(Parser)]
#[command(
    name = "dashr",
    version,
    about = "Tom Forsyth's DASHR — native Rust/wgpu experimental viewer"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}
#[derive(Subcommand)]
enum Command {
    View {
        #[command(flatten)]
        options: Options,
        /// Close after this many presented frames (native window smoke test).
        #[arg(long)]
        exit_after: Option<u32>,
    },
    Capture {
        #[arg(long)]
        out: PathBuf,
        #[arg(long, default_value_t = 1)]
        frames: u32,
        #[arg(long, default_value_t = 0.1)]
        time_step: f32,
        /// Save raw float images for every frame; otherwise only the first.
        #[arg(long)]
        raw_frames: bool,
        #[command(flatten)]
        options: Options,
    },
    Probe {
        #[arg(long, default_value = "out/probe")]
        out: PathBuf,
        #[arg(long)]
        manual_filter: bool,
        #[arg(long, default_value_t = 0)]
        split: u32,
    },
}
#[derive(Args, Default)]
struct Options {
    /// Read a Settings JSON or a capture manifest's settings object.
    #[arg(long)]
    settings: Option<PathBuf>,
    #[arg(long, default_value = "demo/assets")]
    assets: PathBuf,
    #[arg(long)]
    mesh: Option<MeshKind>,
    #[arg(long)]
    around: Option<u32>,
    #[arg(long)]
    long: Option<u32>,
    #[arg(long)]
    atlas: Option<u32>,
    #[arg(long)]
    width: Option<u32>,
    #[arg(long)]
    height: Option<u32>,
    #[arg(long)]
    texture_set: Option<u32>,
    #[arg(long)]
    time: Option<f32>,
    #[arg(long)]
    animation_amount: Option<f32>,
    #[arg(long)]
    debug: Option<i32>,
    #[arg(long)]
    lighting: Option<i32>,
    #[arg(long)]
    distortion: Option<i32>,
    #[arg(long)]
    step_budget: Option<i32>,
    #[arg(long)]
    hit_depth: bool,
    /// Refine same-chart heightfield hits with a bounded binary search.
    #[arg(long)]
    refine_hits: bool,
    /// Shorten steps that would cross a teleport-map edge.
    #[arg(long)]
    seam_aware_stepping: bool,
    /// Shorten steps when consecutive surface transforms change rapidly.
    #[arg(long)]
    adaptive_steps: bool,
    /// Derive UV and local-shadow tolerances from atlas and asset dimensions.
    #[arg(long)]
    scale_derived_epsilons: bool,
    /// Use the specialized 3x3 cofactor inverse in the tracing shader.
    #[arg(long)]
    specialized_inverse: bool,
    /// Read the precomputed inverse surface basis atlas instead of inverting per sample.
    #[arg(long)]
    stored_inverse: bool,
    /// Use the opt-in full-atlas compute shader for dynamic edgefill.
    #[arg(long)]
    compute_edgefill: bool,
    /// Reconstruct guttered transforms from the raw deformation maps while tracing.
    #[arg(long)]
    indirect_edgefill: bool,
    /// Split seam distances and discontinuous destination coordinates into separate textures.
    #[arg(long)]
    split_teleport: bool,
    /// Store dynamic transform atlas planes in RGBA16F instead of RGBA32F.
    #[arg(long)]
    compact_warp: bool,
    #[arg(long)]
    manual_filter: bool,
    /// 0 selects the adapter policy; 1/2/4 forces an MRT width.
    #[arg(long, default_value_t = 0)]
    split: u32,
}
impl Options {
    fn resolve(&self) -> Result<Settings> {
        let mut s = if let Some(file) = &self.settings {
            Settings::load(file).context("invalid settings or reference snapshot")?
        } else {
            Settings::default()
        };
        macro_rules! apply {($($name:ident),*)=>{$(if let Some(value)=self.$name{s.$name=value;})*};}
        apply!(
            mesh,
            around,
            long,
            atlas,
            width,
            height,
            texture_set,
            time,
            animation_amount,
            debug,
            distortion,
            step_budget
        );
        if let Some(lighting) = self.lighting {
            s.lighting_mode = lighting;
        }
        if self.hit_depth {
            s.hit_depth = true;
        }
        if self.refine_hits {
            s.hit_refinement = true;
        }
        if self.seam_aware_stepping {
            s.seam_aware_stepping = true;
        }
        if self.adaptive_steps {
            s.adaptive_steps = true;
        }
        if self.scale_derived_epsilons {
            s.epsilon_policy = EpsilonPolicy::ScaleDerived;
        }
        if self.specialized_inverse {
            s.specialized_inverse = true;
        }
        if self.stored_inverse {
            s.stored_inverse = true;
        }
        if self.compute_edgefill {
            s.compute_edgefill = true;
        }
        if self.indirect_edgefill {
            s.indirect_edgefill = true;
        }
        if self.split_teleport {
            s.split_teleport = true;
        }
        if self.compact_warp {
            s.compact_warp = true;
        }
        if let Some(reference) = s
            .uniform_override
            .as_mut()
            .filter(|_| self.width.is_some() || self.height.is_some())
        {
            let old_aspect = reference.projection[1][1] / reference.projection[0][0];
            let new_aspect = s.width as f32 / s.height as f32;
            reference.projection[0][0] *= old_aspect / new_aspect;
        }
        s.validate()?;
        Ok(s)
    }
}
fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command.unwrap_or(Command::View {
        options: Options {
            assets: "demo/assets".into(),
            ..Default::default()
        },
        exit_after: None,
    }) {
        Command::View {
            options,
            exit_after,
        } => viewer::run(
            options.resolve()?,
            options.assets,
            options.manual_filter,
            options.split,
            exit_after,
        ),
        Command::Capture {
            out,
            frames,
            time_step,
            raw_frames,
            options,
        } => {
            capture::run(
                &out,
                &options.resolve()?,
                &options.assets,
                &capture::CaptureOptions {
                    manual_filter: options.manual_filter,
                    split: options.split,
                    frames,
                    time_step,
                    raw_frames,
                },
            )?;
            println!("Capture saved to {}", out.display());
            Ok(())
        }
        Command::Probe {
            out,
            manual_filter,
            split,
        } => {
            std::fs::create_dir_all(&out)?;
            let ctx = pollster::block_on(GpuContext::new(
                GpuContext::instance(),
                None,
                manual_filter,
                split,
            ))?;
            let report = match contracts(&ctx) {
                Ok(results) => {
                    serde_json::json!({"status":"passed","adapter":ctx.report,"results":results})
                }
                Err(error) => {
                    let report = serde_json::json!({"status":"failed","adapter":ctx.report,"failure":format!("{error:#}")});
                    std::fs::write(
                        out.join("capabilities.json"),
                        serde_json::to_vec_pretty(&report)?,
                    )?;
                    return Err(error);
                }
            };
            std::fs::write(
                out.join("capabilities.json"),
                serde_json::to_vec_pretty(&report)?,
            )?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            Ok(())
        }
    }
}
