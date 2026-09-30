//! Shared build/runtime hashing; paths in exported metadata are repository-relative.
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, io, path::Path};
pub const CONFIG_FILES: &[&str] = &[
    "build.rs",
    "Cargo.toml",
    "Cargo.lock",
    "flake.nix",
    "flake.lock",
    "rust-toolchain.toml",
    "mise.toml",
    "scripts/in-nix",
    "demo/main.cpp",
    "demo/comparison_snapshot_format.h",
    "demo/comparison_snapshot_win.h",
    "demo/PipelineMain.hlsl",
    "demo/PipelineDeform.hlsl",
    "demo/PipelineEdgefill.hlsl",
    "demo/Utils.hlsl",
    "demo/ConstantBuffer.hlsl",
    "demo/VertexInput.hlsl",
];
pub fn source_hashes(root: &Path) -> io::Result<BTreeMap<String, String>> {
    let mut names = Vec::new();
    for dir in ["src", "shaders"] {
        for entry in std::fs::read_dir(root.join(dir))? {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                names.push(format!("{dir}/{}", entry.file_name().to_string_lossy()));
            }
        }
    }
    names.extend(CONFIG_FILES.iter().map(|s| s.to_string()));
    names.sort();
    names
        .into_iter()
        .map(|name| {
            let bytes = std::fs::read(root.join(&name))?;
            Ok((name, format!("{:x}", Sha256::digest(bytes))))
        })
        .collect()
}
