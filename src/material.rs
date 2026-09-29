//! Byte input semantics match the reference, including high-byte PNG16 decoding.
use anyhow::{Context, Result, ensure};
use glam::Vec3;
use image::DynamicImage;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::Path;

pub struct Pixels {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}
impl Pixels {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.width > 0
                && self.height > 0
                && self.width <= 8192
                && self.height <= 8192
                && self.rgba.len() == self.width as usize * self.height as usize * 4,
            "invalid image dimensions/payload"
        );
        Ok(())
    }
}
#[derive(Serialize)]
pub struct MaterialProvenance {
    pub files: Vec<(String, String)>,
    pub decoded_height_sha256: String,
    pub decoded_albedo_sha256: String,
    pub decoding: &'static str,
}
pub struct Material {
    pub height: Pixels,
    pub albedo: Pixels,
    pub normal: Vec<u8>,
    pub provenance: MaterialProvenance,
}

pub fn hash(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}
pub fn decode_reference(img: DynamicImage) -> Pixels {
    let (width, height) = (img.width(), img.height());
    let rgba = if img.color().bits_per_pixel() / img.color().channel_count() as u16 == 16 {
        img.into_rgba16()
            .into_raw()
            .into_iter()
            .map(|v| (v >> 8) as u8)
            .collect()
    } else {
        img.into_rgba8().into_raw()
    };
    Pixels {
        width,
        height,
        rgba,
    }
}
pub fn repeat(p: &mut Pixels, count: u32) -> Result<()> {
    p.validate()?;
    ensure!(count > 0, "repeat count must be positive");
    if count == 1 {
        return Ok(());
    }
    let src = p.rgba.clone();
    for y in 0..p.height {
        for x in 0..p.width {
            let from = (((y as u64 * count as u64) % p.height as u64) * p.width as u64
                + (x as u64 * count as u64) % p.width as u64) as usize
                * 4;
            let to = (y as usize * p.width as usize + x as usize) * 4;
            p.rgba[to..to + 4].copy_from_slice(&src[from..from + 4]);
        }
    }
    Ok(())
}
pub fn normal_map(p: &Pixels, scale: f32) -> Result<Vec<u8>> {
    p.validate()?;
    ensure!(
        scale.is_finite() && scale > 0.,
        "normal bake scale must be finite and positive"
    );
    let kernel = (p.width / 1024).max(1) as i32;
    let mut normals = vec![0; p.rgba.len()];
    let sample = |x: i32, y: i32| {
        p.rgba[(y.rem_euclid(p.height as i32) as usize * p.width as usize
            + x.rem_euclid(p.width as i32) as usize)
            * 4] as f32
    };
    for y in 0..p.height as i32 {
        for x in 0..p.width as i32 {
            let normal = Vec3::new(
                sample(x - kernel, y) - sample(x + kernel, y),
                sample(x, y - kernel) - sample(x, y + kernel),
                127. * kernel as f32 / scale,
            )
            .normalize();
            let offset = (y as usize * p.width as usize + x as usize) * 4;
            for (i, v) in normal.to_array().into_iter().enumerate() {
                normals[offset + i] = (0.5 + (v * 127.).clamp(-127., 127.)).floor() as i8 as u8;
            }
        }
    }
    Ok(normals)
}
pub fn load(root: &Path, set: u32) -> Result<Material> {
    let (mut height, mut albedo, files, scale, repeats) = if set == 3 {
        let height = Pixels {
            width: 32,
            height: 32,
            rgba: vec![[128, 128, 128, 255]; 1024]
                .into_iter()
                .flatten()
                .collect(),
        };
        let rgba = (0..1024)
            .flat_map(|i| {
                if ((i % 32) / 4 + (i / 32) / 4) % 2 == 0 {
                    [210, 145, 60, 255]
                } else {
                    [50, 100, 170, 255]
                }
            })
            .collect();
        (
            height,
            Pixels {
                width: 32,
                height: 32,
                rgba,
            },
            Vec::new(),
            1.,
            1,
        )
    } else {
        let (h, a, scale, repeats) = match set {
            0 => (
                "roof_3_4k.blend/textures/roof_3_disp_4k.png",
                "roof_3_4k.blend/textures/roof_3_diff_4k.jpg",
                8.,
                1,
            ),
            1 => (
                "aerial_rocks_02_4k.blend/textures/aerial_rocks_02_disp_4k.png",
                "aerial_rocks_02_4k.blend/textures/aerial_rocks_02_diff_4k.jpg",
                8.,
                4,
            ),
            2 => ("Textures_h.png", "Textures_a.png", 0.125, 1),
            _ => anyhow::bail!("unknown texture set {set}"),
        };
        let read = |name: &str| -> Result<(Pixels, (String, String))> {
            let bytes=std::fs::read(root.join(name)).with_context(||format!("missing/unreadable material asset {name}; set --assets to the demo/assets directory"))?;
            let img =
                image::load_from_memory(&bytes).with_context(|| format!("cannot decode {name}"))?;
            Ok((decode_reference(img), (name.to_owned(), hash(&bytes))))
        };
        let (height, hfile) = read(h)?;
        let (albedo, afile) = read(a)?;
        (height, albedo, vec![hfile, afile], scale, repeats)
    };
    repeat(&mut height, repeats)?;
    repeat(&mut albedo, repeats)?;
    let normal = normal_map(&height, scale)?;
    let provenance = MaterialProvenance {
        files,
        decoded_height_sha256: hash(&height.rgba),
        decoded_albedo_sha256: hash(&albedo.rgba),
        decoding: "RGBA8 UNORM data; PNG16 high byte, JPEG Rust decoder; no sRGB conversion; one mip",
    };
    Ok(Material {
        height,
        albedo,
        normal,
        provenance,
    })
}
