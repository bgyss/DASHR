//! Versioned serialization for deterministic topology and seam bake results.
use crate::{asset::Mesh, asset_format::AssetDocument, topology};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;

pub const CACHE_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BakedMapCache {
    pub schema_version: u32,
    pub atlas_size: u32,
    pub input_key: String,
    pub maps: topology::Maps,
}

impl BakedMapCache {
    pub fn bake(mesh: &Mesh, atlas_size: u32, occupancy: &[bool]) -> Result<Self> {
        Self::new(
            mesh,
            atlas_size,
            occupancy,
            topology::bake(mesh, atlas_size, occupancy)?,
        )
    }

    pub fn new(
        mesh: &Mesh,
        atlas_size: u32,
        occupancy: &[bool],
        maps: topology::Maps,
    ) -> Result<Self> {
        let cache = Self {
            schema_version: CACHE_SCHEMA_VERSION,
            atlas_size,
            input_key: input_key(mesh, atlas_size, occupancy)?,
            maps,
        };
        cache.validate_for(mesh, atlas_size, occupancy)?;
        Ok(cache)
    }

    pub fn validate_for(&self, mesh: &Mesh, atlas_size: u32, occupancy: &[bool]) -> Result<()> {
        self.validate_payload()?;
        ensure!(
            self.atlas_size == atlas_size,
            "baked map cache atlas size {} does not match requested size {atlas_size}",
            self.atlas_size
        );
        let expected = input_key(mesh, atlas_size, occupancy)?;
        ensure!(
            self.input_key == expected,
            "baked map cache input key does not match mesh and GPU occupancy"
        );
        Ok(())
    }

    pub fn from_json(json: &str) -> Result<Self> {
        let cache: Self = serde_json::from_str(json).context("decode DASHR baked map cache")?;
        cache.validate_payload()?;
        Ok(cache)
    }

    pub fn to_json(&self) -> Result<String> {
        self.validate_payload()?;
        serde_json::to_string(self).context("encode DASHR baked map cache")
    }

    pub fn load(path: &Path) -> Result<Self> {
        let json = std::fs::read_to_string(path)
            .with_context(|| format!("read DASHR map cache {}", path.display()))?;
        Self::from_json(&json)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        self.validate_payload()?;
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create map cache directory {}", parent.display()))?;
        }
        std::fs::write(path, self.to_json()?)
            .with_context(|| format!("write DASHR map cache {}", path.display()))?;
        Ok(())
    }

    fn validate_payload(&self) -> Result<()> {
        ensure!(
            self.schema_version == CACHE_SCHEMA_VERSION,
            "unsupported DASHR map cache schema version {}; supported version is {}",
            self.schema_version,
            CACHE_SCHEMA_VERSION
        );
        ensure!(
            (16..=1024).contains(&self.atlas_size),
            "invalid cached atlas size {}",
            self.atlas_size
        );
        let expected = (self.atlas_size * self.atlas_size) as usize;
        ensure!(
            self.maps.teleport.len() == expected && self.maps.edgefill.len() == expected,
            "cached maps do not match atlas size {}",
            self.atlas_size
        );
        ensure!(self.maps.seam_edges > 0, "cached map has no seam edges");
        ensure!(
            self.maps
                .teleport
                .iter()
                .chain(&self.maps.edgefill)
                .flatten()
                .all(|value| value.is_finite()),
            "cached map contains non-finite values"
        );
        Ok(())
    }
}

fn input_key(mesh: &Mesh, atlas_size: u32, occupancy: &[bool]) -> Result<String> {
    ensure!(
        (16..=1024).contains(&atlas_size),
        "invalid map cache atlas size {atlas_size}"
    );
    ensure!(
        occupancy.len() == (atlas_size * atlas_size) as usize,
        "map cache occupancy dimensions do not match atlas size {atlas_size}"
    );
    ensure!(
        occupancy.iter().any(|occupied| *occupied),
        "map cache occupancy has no covered texels"
    );
    let asset_bytes = serde_json::to_vec(mesh).context("encode mesh for map cache key")?;
    let mut hasher = Sha256::new();
    hasher.update(b"DASHR topology bake cache\0");
    hasher.update(CACHE_SCHEMA_VERSION.to_le_bytes());
    hasher.update(AssetDocument::SCHEMA_VERSION.to_le_bytes());
    hasher.update(atlas_size.to_le_bytes());
    hasher.update((asset_bytes.len() as u64).to_le_bytes());
    hasher.update(asset_bytes);
    for occupied in occupancy {
        hasher.update([u8::from(*occupied)]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}
