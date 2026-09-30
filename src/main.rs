use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use dashr::{
    asset::MeshKind, capture, gpu_resources::GpuContext, probe::contracts, settings::Settings,
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
    #[arg(long)]
    manual_filter: bool,
    /// 0 selects the adapter policy; 1/2/4 forces an MRT width.
    #[arg(long, default_value_t = 0)]
    split: u32,
}
impl Options {
    fn resolve(&self) -> Result<Settings> {
        let mut s = if let Some(file) = &self.settings {
            let v: serde_json::Value =
                serde_json::from_slice(&std::fs::read(file).context("cannot read settings file")?)?;
            serde_json::from_value(v.get("settings").cloned().unwrap_or(v))
                .context("invalid settings JSON")?
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
