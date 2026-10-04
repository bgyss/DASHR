use anyhow::{Context, Result, bail, ensure};
use dashr::{
    api::{DashrSession, GpuOptions, TerminationStatus},
    asset::{MeshKind, procedural},
    asset_format::AssetDocument,
    settings::Settings,
};
use std::path::PathBuf;

#[derive(Debug, Default, PartialEq)]
struct ConsumerOptions {
    asset: Option<PathBuf>,
    assets: Option<PathBuf>,
    texture_set: Option<u32>,
    capture: Option<PathBuf>,
    size: Option<u32>,
}

impl ConsumerOptions {
    fn parse(arguments: impl IntoIterator<Item = String>) -> Result<Self> {
        let mut options = Self::default();
        let mut arguments = arguments.into_iter();
        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "--asset" => {
                    ensure!(
                        options.asset.is_none(),
                        "asset path was provided more than once"
                    );
                    options.asset = Some(PathBuf::from(
                        arguments.next().context("--asset requires a path")?,
                    ));
                }
                "--assets" => {
                    ensure!(
                        options.assets.is_none(),
                        "asset root was provided more than once"
                    );
                    options.assets = Some(PathBuf::from(
                        arguments.next().context("--assets requires a directory")?,
                    ));
                }
                "--texture-set" => {
                    ensure!(
                        options.texture_set.is_none(),
                        "texture set was provided more than once"
                    );
                    let value = arguments
                        .next()
                        .context("--texture-set requires an integer")?;
                    let texture_set = value
                        .parse::<u32>()
                        .with_context(|| format!("invalid texture set {value:?}"))?;
                    ensure!(texture_set <= 3, "texture set must be in 0..=3");
                    options.texture_set = Some(texture_set);
                }
                "--capture" => {
                    ensure!(
                        options.capture.is_none(),
                        "capture path was provided more than once"
                    );
                    options.capture = Some(PathBuf::from(
                        arguments.next().context("--capture requires a path")?,
                    ));
                }
                "--size" => {
                    ensure!(
                        options.size.is_none(),
                        "image size was provided more than once"
                    );
                    let value = arguments.next().context("--size requires an integer")?;
                    let size = value
                        .parse::<u32>()
                        .with_context(|| format!("invalid image size {value:?}"))?;
                    ensure!(
                        (64..=1024).contains(&size),
                        "image size must be in 64..=1024"
                    );
                    options.size = Some(size);
                }
                value if value.starts_with('-') => bail!("unknown option {value:?}"),
                value if options.asset.is_none() => {
                    options.asset = Some(PathBuf::from(value));
                }
                value => bail!("unexpected positional argument {value:?}"),
            }
        }
        Ok(options)
    }
}

fn main() -> Result<()> {
    let options = ConsumerOptions::parse(std::env::args().skip(1))?;
    let default_asset_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("demo/assets");
    let asset_root = options.assets.unwrap_or(default_asset_root);
    let imported_asset = options.asset;
    let mut settings = Settings::default();
    settings.mesh = MeshKind::Tube;
    settings.around = 8;
    settings.long = 8;
    settings.atlas = 64;
    let image_size = options.size.unwrap_or(64);
    settings.width = image_size;
    settings.height = image_size;
    if let Some(texture_set) = options.texture_set {
        settings.texture_set = texture_set;
    }
    if imported_asset.is_some() {
        settings.camera = [0.0, 2.0, -7.0];
        settings.target = [0.0, 0.0, 0.0];
        settings.animation_amount = 0.0;
    }
    let animation_amount = if imported_asset.is_some() { 0.0 } else { 1.0 };

    let asset = if let Some(path) = &imported_asset {
        AssetDocument::load(path)?
    } else {
        let mesh = procedural(
            settings.mesh,
            settings.around,
            settings.long,
            settings.length,
            settings.radius,
            settings.thickness,
        )?;
        AssetDocument::new(mesh)?
    };
    asset.validate()?;

    let mut session = pollster::block_on(DashrSession::from_asset(
        settings,
        asset,
        asset_root,
        GpuOptions {
            map_cache_path: imported_asset
                .is_none()
                .then(|| PathBuf::from("out/standalone-consumer/maps-v1.json")),
            ..GpuOptions::default()
        },
    ))?;
    session.update_pose(0.4, animation_amount)?;
    let frame = session.render()?;

    if let Some(path) = &options.capture {
        write_bmp(path, frame.width, frame.height, &frame.color)?;
        println!("saved color capture to {}", path.display());
    }

    let primary_hits = frame
        .primary
        .iter()
        .filter(|trace| trace.status == TerminationStatus::Hit)
        .count();
    let shadow_hits = frame
        .shadow
        .iter()
        .filter(|trace| trace.status == TerminationStatus::Hit)
        .count();
    let color_checksum = frame
        .color
        .iter()
        .flatten()
        .fold(0xcbf29ce484222325u64, |hash, channel| {
            (hash ^ u64::from(channel.to_bits())).wrapping_mul(0x100000001b3)
        });
    let adapter = session.adapter_report();
    println!(
        "{} / {}: {}x{}, texture set {}, {} primary hits, {} shadow hits, {} seam edges, color {:016x}",
        adapter["name"].as_str().unwrap_or("unknown adapter"),
        adapter["backend"].as_str().unwrap_or("unknown backend"),
        frame.width,
        frame.height,
        session.settings().texture_set,
        primary_hits,
        shadow_hits,
        session.seam_edges(),
        color_checksum
    );
    Ok(())
}

fn write_bmp(path: &std::path::Path, width: u32, height: u32, pixels: &[[f32; 4]]) -> Result<()> {
    ensure!(width > 0 && height > 0, "BMP dimensions must be nonzero");
    ensure!(
        width <= i32::MAX as u32 && height <= i32::MAX as u32,
        "BMP dimensions exceed the format limit"
    );
    let pixel_count = (width as usize)
        .checked_mul(height as usize)
        .context("BMP dimensions overflow address space")?;
    ensure!(
        pixels.len() == pixel_count,
        "BMP pixel count does not match dimensions"
    );
    ensure!(
        pixels.iter().flatten().all(|channel| channel.is_finite()),
        "BMP capture contains a non-finite channel"
    );

    let row_bytes = (width as usize)
        .checked_mul(3)
        .context("BMP row size overflow")?;
    let padded_row_bytes = row_bytes
        .checked_add(3)
        .context("BMP row padding overflow")?
        & !3;
    let image_bytes = padded_row_bytes
        .checked_mul(height as usize)
        .context("BMP image size overflow")?;
    let file_bytes = 54usize
        .checked_add(image_bytes)
        .context("BMP file size overflow")?;
    ensure!(
        file_bytes <= u32::MAX as usize,
        "BMP file exceeds the format limit"
    );

    let mut bytes = Vec::with_capacity(file_bytes);
    bytes.extend_from_slice(b"BM");
    bytes.extend_from_slice(&(file_bytes as u32).to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.extend_from_slice(&54u32.to_le_bytes());
    bytes.extend_from_slice(&40u32.to_le_bytes());
    bytes.extend_from_slice(&(width as i32).to_le_bytes());
    bytes.extend_from_slice(&(height as i32).to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&24u16.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&(image_bytes as u32).to_le_bytes());
    bytes.extend_from_slice(&2835u32.to_le_bytes());
    bytes.extend_from_slice(&2835u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());

    for y in (0..height as usize).rev() {
        let row_start = bytes.len();
        for x in 0..width as usize {
            let pixel = pixels[y * width as usize + x];
            for channel in [pixel[2], pixel[1], pixel[0]] {
                bytes.push((channel.clamp(0.0, 1.0) * 255.0 + 0.5).floor() as u8);
            }
        }
        bytes.resize(row_start + padded_row_bytes, 0);
    }

    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, bytes).with_context(|| format!("write BMP capture {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ConsumerOptions, write_bmp};
    use std::{fs, path::PathBuf};

    #[test]
    fn parses_positional_asset_and_optional_material_root_and_texture_set() {
        let options = ConsumerOptions::parse(
            ["mesh.json", "--assets", "materials", "--texture-set", "2"]
                .into_iter()
                .map(str::to_owned),
        )
        .unwrap();

        assert_eq!(options.asset, Some(PathBuf::from("mesh.json")));
        assert_eq!(options.assets, Some(PathBuf::from("materials")));
        assert_eq!(options.texture_set, Some(2));
    }

    #[test]
    fn parses_flagged_asset_and_default_material_root() {
        let options =
            ConsumerOptions::parse(["--asset", "mesh.json"].into_iter().map(str::to_owned))
                .unwrap();

        assert_eq!(options.asset, Some(PathBuf::from("mesh.json")));
        assert_eq!(options.assets, None);
        assert_eq!(options.texture_set, None);
    }

    #[test]
    fn rejects_texture_sets_outside_the_public_range() {
        let error = ConsumerOptions::parse(["--texture-set", "4"].into_iter().map(str::to_owned))
            .unwrap_err();
        assert!(error.to_string().contains("0..=3"));
    }

    #[test]
    fn parses_capture_path_and_square_output_size() {
        let options = ConsumerOptions::parse(
            ["--capture", "frame.bmp", "--size", "256"]
                .into_iter()
                .map(str::to_owned),
        )
        .unwrap();
        assert_eq!(options.capture, Some(PathBuf::from("frame.bmp")));
        assert_eq!(options.size, Some(256));
    }

    #[test]
    fn rejects_output_sizes_outside_the_supported_range() {
        let error =
            ConsumerOptions::parse(["--size", "2048"].into_iter().map(str::to_owned)).unwrap_err();
        assert!(error.to_string().contains("64..=1024"));
    }

    #[test]
    fn writes_a_bottom_up_bmp_color_capture() {
        let path =
            std::env::temp_dir().join(format!("dashr-consumer-capture-{}.bmp", std::process::id()));
        let pixels = [
            [1.0, 0.0, 0.0, 1.0],
            [0.0, 1.0, 0.0, 1.0],
            [0.0, 0.0, 1.0, 1.0],
            [1.0, 1.0, 1.0, 1.0],
        ];
        write_bmp(&path, 2, 2, &pixels).unwrap();
        let bytes = fs::read(&path).unwrap();
        fs::remove_file(path).unwrap();

        assert_eq!(&bytes[..2], b"BM");
        assert_eq!(u32::from_le_bytes(bytes[18..22].try_into().unwrap()), 2);
        assert_eq!(u32::from_le_bytes(bytes[22..26].try_into().unwrap()), 2);
        assert_eq!(&bytes[54..60], &[255, 0, 0, 255, 255, 255]);
        assert_eq!(&bytes[62..68], &[0, 0, 255, 0, 255, 0]);
    }
}
