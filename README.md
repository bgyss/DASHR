# DASHR
DASHR - Dynamically Animated Skinned Heightfield Rendering

Tom Forsyth, v1.0 2026 September 27th

Short video: <https://youtu.be/-Su9YrcazRk>

Longer video soon - details in the paper.


Online paper: <https://tomforsyth1000.github.io/DASHR/paper/DASHR_Paper.html>


Download everything! Then:

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

The forward goal is a standalone library for Unreal Engine, Unity and Blender. See [Tom's future work traced into this fork](docs/plans/upstream-future-work.md) and the [standalone library and integration plan](docs/plans/standalone-library-and-integrations.md).

The default `dashr` dependency is headless and does not enable `winit`, `egui`, or CLI dependencies. Applications can consume it with `default-features = false`; enable the `viewer` feature to build the bundled CLI and interactive viewer. An optional `ffi` feature builds a headless cdylib with a checked-in C header. Run `scripts/in-nix mise run ffi-smoke` to build the library and compile/run the native C consumer. See the [library API guide](docs/port/library-api.md), the independent Rust consumer, and the [C ABI smoke client](examples/c-consumer/main.c).

The [2026-10-04 work report](docs/reports/2026-10-04-standalone-library-and-future-work.md) covers library acceptance, all 20 future-work items, measured experiments and remaining gates. The [announcement drafts](docs/social/launch-posts.md) include an addendum to the first post.
