# DASHR
DASHR - Dynamically Animated Skinned Heightfield Rendering

Short video: <https://youtu.be/-Su9YrcazRk>
Longer video soon.

Tom Forsyth, v1.0 2026 September 27th

Download everything!

Paper: paper/DASHR_Paper.html

Executable Windows demo: demo/skinnedheightfield.exe

Windows/DirectX code in demo directory.

## Native Rust/wgpu viewer

An experimental native port is available alongside the preserved D3D11 reference. It includes procedural assets, animation, seam/gutter baking, both distortion encodings, tracing/lighting and deterministic capture diagnostics.

Run from the repository root:

```sh
scripts/in-nix mise run setup
scripts/in-nix mise run view
```

Nix supplies rustup/mise; Rust 1.95.0 and Cargo dependencies are pinned, with mutable state isolated under the checkout. See the [viewer guide](docs/port/README.md), [validation and remaining gates](docs/port/validation.md), and [provenance](docs/port/provenance.md). Native Metal rendering is verified locally; D3D11/Windows parity and reviewed scene readiness remain open.

## Fork research and plans

[Optimization and porting research](docs/research/2026-09-29/README.md) assesses the source, macOS/Metal and Rust/wgpu feasibility, Blender integration, and phased development with benchmark gates. The dated research predates the native implementation. No speedup or cross-platform parity is claimed.
