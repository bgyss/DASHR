#[path = "src/fingerprints.rs"]
mod fingerprints;
use std::{path::Path, process::Command};
fn output(program: &str, args: &[&str], root: &Path) -> Option<String> {
    Command::new(program)
        .args(args)
        .current_dir(root)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
}
fn main() {
    let root_string = std::env::var("CARGO_MANIFEST_DIR").expect("Cargo source root");
    let root = Path::new(&root_string);
    let hashes = fingerprints::source_hashes(root).expect("hash compiled source snapshot");
    for name in hashes.keys() {
        println!("cargo:rerun-if-changed={name}");
    }
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=shaders");
    let rustc = std::env::var("RUSTC").expect("Cargo compiler path");
    let snapshot = serde_json::json!({
        "schema":"dashr.build.v1","source_hashes":hashes,
        "rustc":output(&rustc,&["-vV"],root).unwrap_or_else(||"unavailable".into()),
        "git_revision":output("git",&["rev-parse","HEAD"],root),
        "working_tree_dirty":output("git",&["status","--porcelain"],root).map(|s|!s.is_empty()),
        "target":std::env::var("TARGET").expect("Cargo target")
    });
    let path =
        std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo output directory"))
            .join("build-provenance.json");
    std::fs::write(path, serde_json::to_vec_pretty(&snapshot).unwrap())
        .expect("write immutable build provenance");
}
