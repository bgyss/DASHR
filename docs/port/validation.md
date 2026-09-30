# Rust/wgpu port validation

Initial execution date: 2026-09-29; comparison evidence updated 2026-09-30. This record describes the experimental native port, not completed D3D11 parity or scene integration.

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

## Windows cross-compilation follow-up

Both the Rust viewer and the C++ reference were subsequently compiled and linked on this Mac for Windows x64 and ARM64. Rust source was unchanged; C++ used pinned ImGui/stb and a syntax-only `[[nodiscard]]` fix in a temporary copy. That initial cross-build left the tracked reference unchanged; the later snapshot follow-up modifies it. See the [cross-compilation review](../research/2026-09-29/windows-cross-compilation-sources.md) and [binary fingerprints](../research/2026-09-29/windows-cross-build-report.json).

Linking and PE inspection alone establish build feasibility. Later Windows execution and comparison evidence is recorded below; cross-compilation by itself does not establish rendering parity.

## Snapshot and control follow-up

The comparison exporter, exact-uniform replay and native egui controls are documented in [comparison snapshots](comparison-snapshots.md). Rust 1.95.0 is now pinned for egui 0.36.2. Current ordinary tests and strict formatting/clippy checks, three native GPU test functions, release build and a three-frame overlay window smoke test passed. These include frozen-pose pixel replay and background compositing. Interactive overlay review remains open; the later supplied snapshot verifies Windows WIC readback.

The user supplied a running original D3D11 screenshot without exact pose/build metadata. The new Rust ARM64/x64 probes were exercised in the running VM: the Microsoft Basic Render Driver loses the DX12 device during readback. Manual/split fallback and a tiny analytic capture also fail. A separate compile-only ARM64 diagnostic created all three renderer pipelines without submitting GPU work; deformation, gutter and trace compiled successfully, verifying the FXC unrolling fix independently. This is a recorded Windows rendering failure, not a render acceptance. The earlier cross-build report describes the pre-export implementation; the reference now includes snapshot support.

## Matched Windows snapshot and Mac replay

The supplied `comparison-20260930T055747-973Z` snapshot verifies Windows D3D11 rendering and WIC export on Parallels Display Adapter (WDDM). Its exact camera, pose and light replayed at 2534×1640 on Apple M1 Max / Metal. The estimated foreground overlap was 99.9794%; 99.8611% of foreground pixels differed by at most one RGB code per channel. The Mac trace reported zero invalid or budget terminations. See the [comparison report and preserved settings](comparisons/2026-09-30-tube.md) for metrics, hashes, reproduction and limitations.

This closes the snapshot-and-controls workstream. It does not resolve Windows Rust DX12 device loss or establish full animation/material/seam parity. Reference intermediate maps and human animation review remain separate evidence gates.

## Roadmap gate status

| Phase | Implemented capability | Acceptance still required |
| --- | --- | --- |
| P0 | Source preserved; software attribution/license and Rust dependencies documented/pinned | Complete reference-build/asset provenance and additional reference fixtures |
| P1 | CPU affine/metric/anchor contracts, explicit GPU layout, analytic trace/depth oracle, deterministic captures and GPU timestamps | Intermediate D3D11 map comparison and additional camera/pose fixtures; complete benchmark protocol |
| P2 | Real Mac device, four full-float MRTs, filtered/nearest/manual reads, uniform and offscreen readback; forced split paths | Real Windows adapter/pipelines/results; no browser claim |
| P3 | All four procedural assets, fixed-bone tube poses, both encodings and GPU-occupancy seam/gutter bake | Cross-backend atlas comparisons and reviewed seam/bend sequence |
| P4 | Shell tracing, damping, teleports, interpolation, five lighting modes/local shadows, status diagnostics and optional hit depth | D3D11 single-asset parity; reviewed Mac/Windows animation; multi-asset occlusion; near/camera-inside policy validation |
| P5–P7 | No implementation claimed | Blender schema/export round trip, measured optimizations and optional engine integration remain separate work |

## Known limits

This renderer is an experimental single-asset viewer. It has no general mesh/rig importer, inter-object shadows, reflections, translucent displacement, wireframe overlays, Blender integration or browser build. Pinched/strongly compressed fixtures can still fold or exhaust traversal. A successfully saved diagnostic capture does not mean its geometry passed quality gates.

Windows C++ snapshot export and one external D3D11/Metal image comparison are now verified. Windows Rust rendering, Linux execution, independent displaced-mesh truth and human animation review remain unaccepted. No phase is fully promoted and no performance improvement is claimed.
