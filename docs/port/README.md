# Native Rust/wgpu viewer

The experimental viewer ports the reference's four procedural mesh modes, four-bone skinning, metric tangent generation, GPU deformation atlas, CPU seam/gutter bake, both distortion encodings, ray stepping/teleports, lighting and local shadows. The original D3D11 source and paper remain intact. [Validation status](validation.md) separates native-device evidence from the roadmap's remaining parity gates.

## Isolated environment

Nix pins the shell and supplies rustup, mise, Git and native build dependencies. Rustup selects Rust **1.94.1**, including rustfmt/clippy, from `rust-toolchain.toml`. Cargo dependencies and their transitive versions are locked. Cargo, rustup, mise and XDG cache/state directories live under ignored `.dev/`; build outputs live in ignored `target/`. The shell does not change the user's default Rust toolchain.

Run from the repository root:

```sh
scripts/in-nix mise run setup
scripts/in-nix mise run check
scripts/in-nix mise run gpu-test
scripts/in-nix mise run view
```

The first setup downloads the locked Nix dependencies and pinned Rust distribution. GPU tests require a real native adapter and are explicitly ignored by the ordinary CPU test command. `scripts/in-nix` uses a two-file flake snapshot so untracked worktrees work and Nix does not copy Cargo/toolchain caches into its source store. After checking in the flake, `nix develop` is also available. `scripts/in-nix bash` opens the same environment in a terminal.

The Darwin shell clears inherited Xcode library/include search paths that can override the Nix SDK. Linux includes Vulkan and window-system libraries. Linux and Windows execution remain unverified. On Windows, the pinned rustup toolchain and Cargo project can be built with the platform's normal native SDK; the Nix shell is not a Windows SDK replacement.

## Viewer controls

| Control | Action |
| --- | --- |
| Left drag / scroll | Orbit / zoom |
| Space | Pause animation |
| 1 / 2 / 3 / 4 | Tube / cube / pinched tube / pinched cube |
| T | Cycle roof / rocks / demo / analytic materials |
| D | Cycle the six original debug modes |
| L | Cycle the five lighting modes |
| M | Switch affine-offset / anchored-position encoding |
| H | Toggle experimental actual-hit depth |
| Tab | Inspect color, teleport, gutter and four warp planes |
| [ / ] | Decrease / increase displacement scale |
| S | Save complete settings to `out/viewer-settings.json` |
| C | Capture current settings to `out/viewer-capture` |
| Escape | Close |

The UI uses keyboard controls and JSON settings in place of the original ImGui sliders and wireframe overlays. The cube retains the reference's fixed bone-0 weights; this port does not invent a new cube rig. Atlas display clamps raw signed values for inspection; exported float maps preserve the values.

## Deterministic captures and settings

```sh
scripts/in-nix cargo run --locked --release -- probe --out out/probe
scripts/in-nix cargo run --locked --release -- capture \
  --out out/tube --mesh tube --texture-set 0 --time 10 --width 480 --height 360
scripts/in-nix cargo run --locked --release -- capture \
  --out out/cube --mesh cube --texture-set 2 --distortion 0 --hit-depth
scripts/in-nix cargo run --locked --release -- capture \
  --out out/bend --mesh tube --time 0 --frames 120 --time-step 0.1 \
  --width 320 --height 240
scripts/in-nix cargo run --locked --release -- capture \
  --out out/manual --manual-filter --split 2 --texture-set 3
```

Use a fresh output folder for each comparison. Captures include full resolved settings, fixture hash, material source/decoded hashes, adapter features and requested/maximum limits. The executable embeds its **build-time** source hashes and compiler identity; manifests also record executable SHA256 and separately labeled runtime workspace hashes/toolchain information. Runtime Git queries are anchored to the source checkout, even when the executable is launched from another directory. Source edits after a build cannot relabel compiled shaders.

Load either a settings JSON or a capture manifest with `--settings path.json`; explicit CLI options override loaded settings. CLI flags expose common choices; saved JSON exposes all camera, mesh, damping, lighting and marching parameters. Assets default to `demo/assets` relative to the launch directory; use `--assets` when launching elsewhere. See `dashr capture --help`.

The default camera is a reproducible port camera, not a claim that the original interactive camera has been reproduced. Time is the original bone animation clock, with periods 12 and `12 * 0.763` seconds. Captures advance only by the explicit time step.

## Artifact contract

Each capture writes:

- `manifest.json`: inputs, build/runtime provenance, device policy, validation and evidence status. A failed run records its failure.
- `frame-NNNN.png`: shader RGB clamped to bytes, without an additional sRGB conversion.
- `trace-stats.json`: independent primary/shadow hit, escape, budget, invalid and explicit debug-forced-hit counts; steps/teleports; per-pass GPU timestamp samples when supported.
- First-frame raw color, UV/distance/hit-depth, primary status and shadow status images; reconstructed object positions and selected depth buffer. The exact first-frame 544-byte uniform payload is saved as `uniforms.bin`, with matrix values/hash in the manifest and pose matrices/hash per frame.
- First-frame raw/padded warp planes plus static teleport and gutter maps.

`--raw-frames` writes float/depth data for every frame. Raw `.rgba32f` files are top-left, row-major, four little-endian float32 components per pixel, without a header. Frame dimensions and atlas size are in the manifest. `.f32` depth files contain one little-endian float per pixel. Raw color alpha denotes a primary hit except when an explicit debug mode paints the shell.

Diagnostic statuses are `0` not launched, `1` hit, `2` escaped, `3` budget exhausted, `4` invalid basis/value, `5` explicit reference debug forced hit. A production budget exit never becomes a hit. Diagnostic captures retain failed shell traces; normal display discards them. Object positions are reconstructed from actual-hit depth and the recorded inverse projection, and are zero for misses.

The default depth buffer matches the demo's shell depth. `--hit-depth` writes actual-hit reverse-Z depth; an analytic flat-chart fixture tests it independently. This depth uses the source tracer’s returned object-space distance and inherits its traversal approximations; the neutral flat-chart test does not prove deformed-scene accuracy. Multi-asset occlusion, near-clipped shells and cameras inside shells remain unaccepted scene gates.

## Resource and translation decisions

The adapter's maximum attachment budget is recorded separately from the requested budget. Exactly 64 bytes/sample permits four RGBA32F outputs. `FLOAT32_FILTERABLE` is explicitly requested when available. Without filtering, tested manual bilinear loads preserve clamp addressing. Without the full MRT budget, the same full-float output is split into one- or two-plane passes. `--manual-filter` and `--split 1/2/4` exercise these paths; no half-float substitute is used.

Texture decoding preserves RGBA8 UNORM height/albedo semantics, including taking the high byte of 16-bit PNG input and byte-based wrapped normal baking into SNORM. JPEG uses the pinned Rust decoder, so byte equality with stb is an open comparison. All materials retain one mip. Texture samples in divergent tracing use explicit level zero.

Metric tangents/bitangents are never normalized. Mode 1 corrects object-position anchors in gutters. Seam distance is filtered; discontinuous destination UV is loaded nearest. Static maps are baked from actual GPU occupancy, not cached reference files. Topology preparation is intended for these procedural fixtures, not a general importer.

Two deliberate robustness changes are labeled: overflowing reference seam-distance sentinels become finite signed sentinels, and singular/nonfinite interpolated bases become an invalid termination state. Pinched fixtures retain their known pathological behavior; these diagnostics are not a universal geometric repair.

Atlas size is limited to powers of two from 16 through 1024, and offscreen dimensions to 2048 per axis. Eight warp planes cost `128 * atlas²` bytes, before static maps, frame targets, materials and staging. Larger inputs, compression, generalized joint palettes, Blender export and optimization experiments require separate roadmap work.

## Timing limits

GPU timestamps are resolved to staging buffers after submission and kept separate from inclusive preparation/bake time. Capture timing samples include diagnostics and any split passes; they are observations, not a benchmark result. No warm-up/repeated-run protocol, power-state recording, cross-platform comparison or speedup claim is supplied here. Use the [benchmark protocol](../research/2026-09-29/benchmark-protocol.md) before promoting performance or visual equivalence.
