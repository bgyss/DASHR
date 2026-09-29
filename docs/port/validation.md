# Rust/wgpu port validation

Execution date: 2026-09-29. This record describes the experimental native port, not completed D3D11 parity or scene integration.

## Available-device evidence

The initial native run used Apple M1 Max through wgpu's Metal backend, with Rust 1.94.1 in the locked Nix/mise/rustup environment. The adapter exposed exactly **64** color-attachment bytes/sample and `FLOAT32_FILTERABLE`; the native path explicitly requested 64 and four attachments.

Numerical tests exercise:

- All 544 uniform bytes, including matrix columns and integer fields, through GPU storage readback.
- Full-float MRT raster/readback for a scaled/skewed affine chart in both distortion modes. Initial maximum absolute error against the double-precision oracle was `4.76837158203125e-7`.
- Filtered midpoint sampling and nearest destination loads, plus the forced manual path.
- Independent flat-chart UV, ray-distance and actual reverse-Z depth checks in both encodings. In the pilot run, manual UV error was about `3.1e-7`; native filtered UV error was about `9.8e-5`; distance error about `3.1e-7`; depth error about `2.4e-9`.
- Invalid termination for a singular interpolated inverse basis.
- Full renderer captures with native filtering/four MRTs and manual filtering with one/two MRTs; atlas agreement within `1e-5`, and hit-mask disagreement at most 0.1% for the bounded cube fixture.
- Explicit production-budget exits, exact integer diagnostic background codes, and raw-height debug bypass of tracing.

The GPU UV gate was revised after the pilot to distinguish hardware filtering from the CPU affine-roundtrip criterion: hardware-filtered UV tolerance is `1/4096` on a 16-wide atlas (less than 0.01 projected pixel for this flat fixture); manual UV, distance and depth use `1e-5`. This does not revise the roadmap's independent D3D11/seam/silhouette parity gates. Numerical results and thresholds are emitted by `probe`.

The eight roof-texture captures in `out/validation/{tube,cube,tube-pinched,cube-pinched}-mode-{0,1}/` exercised all mesh/encoding combinations at 320×240 and time 10. The sequence in `out/validation/bending-120/` exercised 120 tube poses at times 0 through 11.9; primary and shadow diagnostics reported no invalid-basis or budget exits in those runs. No independent visual-equivalence review is implied. Complete frozen settings are checked in as `fixtures/tube.json` and `fixtures/cube.json`.

The native window smoke test created a window and presented three frames before closing. A paused reference roof-texture tube capture produced actual heightfield hits, local-shadow hits, exported atlas planes and no reported invalid/budget terminations at that selected pose. This is evidence of rendering, not a visual-equivalence approval.

## Review and execution record

An independent read-only code review identified diagnostic-channel clear values and build/runtime provenance mixing. Both received regression tests and fixes. The environment review also identified mise state outside the intended local-state policy; the shell now sets `MISE_STATE_DIR` and `XDG_STATE_HOME`.

Run the current checks:

```sh
scripts/in-nix mise run check
scripts/in-nix mise run gpu-test
scripts/in-nix mise run build
scripts/in-nix target/release/dashr probe --out out/probe
scripts/in-nix target/release/dashr view --texture-set 3 --exit-after 3
```

Final `mise run check` passed 13 CPU/shader/provenance tests and strict clippy/format checks; `mise run gpu-test` passed both native-device test functions and their resource/geometry cases. Release compilation, a final fixture capture, exact uniform dump (544 bytes), numerical probe and native window smoke test passed after the review fixes.

Generated captures/reports live under ignored `out/`; each manifest fingerprints its executable, embedded build sources, resolved settings and assets. They are local execution artifacts, not checked-in reference truth.

## Roadmap gate status

| Phase | Implemented capability | Acceptance still required |
| --- | --- | --- |
| P0 | Source preserved; software attribution/license and Rust dependencies documented/pinned | Repair and clean-launch original Windows reference; pin original external dependencies; reference captures |
| P1 | CPU affine/metric/anchor contracts, explicit GPU layout, analytic trace/depth oracle, deterministic captures and GPU timestamps | Original camera/pose captures and intermediate D3D11 comparison; complete benchmark protocol |
| P2 | Real Mac device, four full-float MRTs, filtered/nearest/manual reads, uniform and offscreen readback; forced split paths | Real Windows adapter/pipelines/results; no browser claim |
| P3 | All four procedural assets, fixed-bone tube poses, both encodings and GPU-occupancy seam/gutter bake | Cross-backend atlas comparisons and reviewed seam/bend sequence |
| P4 | Shell tracing, damping, teleports, interpolation, five lighting modes/local shadows, status diagnostics and optional hit depth | D3D11 single-asset parity; reviewed Mac/Windows animation; multi-asset occlusion; near/camera-inside policy validation |
| P5–P7 | No implementation claimed | Blender schema/export round trip, measured optimizations and optional engine integration remain separate work |

## Known limits

This renderer is an experimental single-asset viewer. It has no general mesh/rig importer, inter-object shadows, reflections, translucent displacement, wireframe overlays, Blender integration or browser build. Pinched/strongly compressed fixtures can still fold or exhaust traversal. A successfully saved diagnostic capture does not mean its geometry passed quality gates.

Windows, Linux, original D3D11 reproduction, external reference image comparison, independent displaced-mesh truth and human animation review were not available in this session. No phase is fully promoted and no performance improvement is claimed.
