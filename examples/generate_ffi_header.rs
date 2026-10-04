use anyhow::{Context, Result, bail};
use cbindgen::{Builder, Config};
use std::{env, fs, path::PathBuf};

fn main() -> Result<()> {
    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let config = Config::from_file(crate_root.join("cbindgen.toml"))
        .map_err(anyhow::Error::msg)
        .context("load cbindgen.toml")?;
    let bindings = Builder::new()
        .with_src(crate_root.join("src/ffi.rs"))
        .with_config(config)
        .generate()
        .map_err(|error| anyhow::anyhow!("generate C ABI header: {error:?}"))?;

    let check = env::args().skip(1).any(|argument| argument == "--check");
    let mut generated = Vec::new();
    bindings.write(&mut generated);
    let output = crate_root.join("include/dashr.h");
    if check {
        let checked_in = fs::read(&output)?;
        if generated != checked_in {
            bail!("include/dashr.h is stale; run scripts/ffi-header to regenerate it");
        }
        println!("include/dashr.h matches src/ffi.rs and cbindgen.toml");
    } else {
        fs::write(&output, generated)
            .with_context(|| format!("write generated header {}", output.display()))?;
        println!("generated {}", output.display());
    }
    Ok(())
}
