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

The `gpu-test` task runs native tests serially because its timing probes share one physical adapter; concurrent test renderers contaminated pass-time comparisons.

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

## P5/B1 Blender static exporter

A static exporter is available at `scripts/blender_export_dashr.py`. It reads an evaluated closed Blender mesh or generates torus and icosphere fixtures, writes an `AssetDocument` v1 JSON file plus a provenance manifest, and computes each triangle's metric tangent basis from world-space positions and UVs. It rejects armature modifiers, vertex groups, degenerate or inconsistently wound UV triangles, and open/nonmanifold geometric edges.

The Blender 5.2.2 LTS application is reproducible on this host outside the restricted sandbox. Inside the sandbox, a minimal Metal probe returns no default Metal device, and the Blender process exits 139 in `supports_barycentric_whitelist` before Python. The installed 5.2 release source calls Metal capability checks without a null-device guard, while [current upstream source](https://github.com/blender/blender/blob/main/source/blender/gpu/metal/mtl_backend.mm) now checks for the missing device. Running the same background command with host GPU access reaches Python and completes the exporter; see the [5.2 release source](https://github.com/blender/blender/blob/blender-v5.2-release/source/blender/gpu/metal/mtl_backend.mm) for the released path.

The Blender-hosted regression `scripts/test_blender_export_dashr.py` passed under 5.2.2. The generated fixture at [`blender-sphere-v1.json`](../../examples/standalone-consumer/assets/blender-sphere-v1.json) is a radius-1.5 icosphere with 80 triangles, 240 loop vertices, 80 per-face UV islands and 120 geometric seam edges. Its manifest records Blender version and asset hash. The independent Rust consumer loaded and rendered the checked-in export on Apple M1 Max / Metal at 64×64, reporting 506 primary hits, 103 local-shadow hits, 240 seam edges and color checksum `25f3f712f870d392`.

The `scripts/generate_b1_materials.py` helper creates constant and horizontal-ramp height-map roots. The same Blender asset rendered with texture set 2 through the standalone consumer at 64×64: constant map produced 379 primary/13 shadow hits and checksum `2a5b88b3706dcd2e`; ramp map produced 487 primary/125 shadow hits and checksum `5dd274d5a756e581`. At 256×256, the constant map produced 6,206 primary/230 shadow hits and checksum `2935aad446ebe00f`; the ramp map produced 7,885 primary/2,044 shadow hits and checksum `834bfcafef160da9`. BMP captures are generated under ignored `out/` for visual review. The larger captures show visible gaps and distortion near seams, so that appearance remains unaccepted. A separate analytic constant-height chart probe passes its current UV/distance/depth budgets.

On M1 Max / Metal, the production hardware-filtered ramp path misses the ideal continuous-plane distance/depth gate: maximum error was `0.003646` baseline and `0.003806` with 12-step hit refinement. The sampled height differed from the CPU ideal 2×2 linear function by up to `0.001639`; refined hits were within `0.000613` of the sampled field. Manual bilinear height sampling still missed the gate when float warp maps used hardware filtering (maximum error `0.000455`). Manually filtering both height and float warp maps passed the analytic ramp gate with maximum error `4.768e-6` and maximum sampled-surface residual `2.623e-6`. These probe variants show that both sampling paths affect the result; they do not validate either manual option as a production default or close the hardware-filtered gate.

B1 still needs a CPU displaced-mesh oracle for the exported sphere and reviewed seam/silhouette output; `AssetDocument` v1 continues to load height images separately via `Settings::texture_set`.

## P8 standalone library extraction

P8's local extraction gates passed on 2026-10-03. The `dashr` package now builds a headless library by default; `winit`, `egui`, `clap` and `pollster` are opt-in viewer/tooling dependencies. The public `dashr::api` surface provides session creation, asset validation, pose updates, render outputs and named primary/shadow termination statuses. `AssetDocument` uses schema v1; `BakedMapCache` uses cache schema v1 and keys maps by the serialized mesh, atlas size and exact GPU occupancy.

The independent consumer at `examples/standalone-consumer/` built from its own locked manifest and rendered without a window. On Apple M1 Max / Metal it reported 666 primary hits, 146 shadow hits and 80 seam edges. Running once to create the map cache and again to load it produced the same color checksum (`2902f326d220aecb`). The consumer validates a procedural asset, advances pose and reads diagnostics through the public API.

The consumer's optional asset-file path also loaded the checked-in external schema-v1 torus at `examples/standalone-consumer/assets/torus-v1.json` (SHA-256 `5462261f6078dde06893ef0c54c8cf4869cfa063c310f72e9381f5c1fe377ad2`). Two fresh-process runs of the locked command in [library API](library-api.md#independent-consumer) rendered 64×64 on Apple M1 Max / Metal with 502 primary hits, 51 local-shadow hits, 576 seam edges and the same color checksum, `68f61da32d984f15`. The fixture is an independently produced mesh document, not a Blender export or a self-contained material package.

Before and after extraction, tube and cube captures used the same settings (320×240, atlas 256, material set 0, time 10) and adapter. All 18 image, map, uniform and depth payloads were byte-identical for each fixture; trace counts and uniform hashes also matched. GPU timing samples were excluded because elapsed timings vary between runs. Captures are under ignored `out/pre-p8/` and `out/post-p8/`.

Validation passed:

- `scripts/in-nix mise run check` — formatting, default-feature library check, full viewer/tooling tests and strict Clippy.
- `scripts/in-nix mise run gpu-test` — all three ignored native GPU test functions.
- `scripts/in-nix mise run build` — release viewer and CLI.
- `scripts/in-nix cargo run --locked --manifest-path examples/standalone-consumer/Cargo.toml` — locked standalone consumer, run twice on Metal.

This accepts extraction behavior on the tested Mac adapter. It does not promote D3D11 parity, Windows Rust execution, independent visual review or P9-P13 integrations.

## P9 C ABI preview

The optional `ffi` feature builds a `cdylib` without enabling viewer/tooling features. ABI version 1 has a checked-in C header generated from `src/ffi.rs` with cbindgen 0.29.4; `scripts/ffi-header --check` checks that the header is current. It defines opaque session handles, explicit struct sizes, trace/status PODs, a borrowed frame view, per-thread error reporting and a per-thread synchronous logger. Rust tests check the C struct sizes and verify that an ABI error reaches the registered callback. Run `scripts/in-nix mise run ffi-smoke` to build the release library and compile/run the C consumer with `-Wall -Wextra -Werror`. It ran on Apple M1 Max / Metal; its callback receives session-created, pose-updated and session-destroyed events, and the client reads 530 primary hits from a 64×64 frame.

The handle owns its wgpu device and copies frame planes to host memory. This does not prove Unity/Unreal use of engine-owned GPU resources, target shader compilation, or cross-engine parity; those P9 gates remain open.

Naga 30.0.1 also translates the current deformation, edgefill, inverse and trace WGSL passes to HLSL and MSL in the ordinary shader test. This verifies translator acceptance only. The generated stages have not been compiled with target engine compilers, matched in Unity/Unreal render graphs, or compared on engine-owned textures.

## U18 standalone Rust library closeout

The native Rust-library acceptance was rerun after U16's topology changes. `cargo check --locked --offline --lib --no-default-features` passes, and the normal dependency graph contains no `winit`, `egui`, `clap` or `pollster`; viewer/tooling dependencies remain opt-in. The separate locked consumer uses `dashr` with `default-features = false`, loads an asset, updates pose and renders on Apple M1 Max / Metal. It reported 666 primary hits, 146 shadow hits, 80 seam edges and the established checksum `2902f326d220aecb`.

The canonical `scripts/in-nix mise run ffi-smoke` also passes on this checkout. It builds the release `cdylib`, compiles `examples/c-consumer/main.c` with warnings as errors and runs it on Metal; the client reports ABI version 1, a 64×64 frame and 530 primary hits, while receiving session-created, pose-updated and session-destroyed callbacks. Asset schema v1, cache schema v1 and C ABI v1 are documented separately from the crate's pre-1.0 Rust semver policy; session ownership, context reuse, borrowed-frame lifetime and FFI thread serialization are documented.

This closes U18 for the native standalone Rust library contract. It does not close P9-P13 integration gates: Unity/Unreal engine-owned resources, target compiler validation and scene comparison, Blender integration, broad rig/material import, and P13 open-topology extensions remain unverified.

## U1-U4 traversal experiments

These options are disabled by default; `--refine-hits`, `--seam-aware-stepping` and `--adaptive-steps` select them. The P8 full-float path remains the default.

- **U1:** `asset_diagnostics::find_high_distortion_faces` reports zero-based triangle IDs, deterministic shared-edge chart-component IDs, metric condition number and maximum metric scale at a caller-specified threshold. A synthetic 1000:1 metric triangle identifies its face/chart. The focused `high_distortion_report_names_the_face_and_connected_chart` regression passed on 2026-10-03 (1/1). A separate call through the public API on generated 8×16 fixtures (length/radius/thickness 4/1/1) found maximum rest-metric conditions of 1.55665 for `Tube`, 1.75610 for `TubePinched`, and 1.46172 for both cube variants. At thresholds 1.5/1.7 it flagged 288/0 `Tube` faces, 304/16 `TubePinched` faces, and zero faces for either cube variant. This is rest-mesh conditioning, not proof of runtime pole locations; it misses the `CubePinched` case. The new `suggest_seam_cut_for_hotspot` maps runtime UVs through the capture edgefill map and returns an isolating face patch: the actual captured `CubePinched` hotspot maps to face 16 with two shared edges to split, and the `TubePinched` hotspot maps to face 277 with two shared edges to split. Its regression verifies those cuts separate the reported patch in the face-adjacency graph. The full Rust test suite passes with viewer/ffi features, and Clippy passes. Global damping multipliers were measured as a no-go. Applying the seam guidance, rebaking materials, and comparing the rendered hotspot, termination data, and seam appearance remain open before U1 can close.

  The checked-in `examples/u1_tube_seam_cut_probe.rs` now provides reproducible TubePinched cases at 640×480 on Apple M1 Max / Metal. The face-277 two-ring patch has zero atlas texel-center overlap but is a no-op: its hotspot stays at 67 steps/67 teleports and the rendered frame is identical. Two face-261 candidates reduce the source hotspot `[49, 63]` from 30 steps/23 teleports to 22/5 with zero atlas texel-center overlap and no new budget/invalid exits. The two-face region `[260, 261]` uses shift `[-24, 97]`, changes 15 primary and 83 shadow status pixels, and has maximum RGB error `0.417415`. The three-face region `[259, 260, 261]` uses shift `[-24, 97]`, changes 33 primary and 64 shadow status pixels, and has maximum RGB error `0.395823`. Expanding to four faces `[2, 259, 260, 261]` keeps the 22/5 hotspot trace but changes 33 primary and 72 shadow status pixels with max RGB error `0.492667`, so it does not improve the three-face result. The one-face patch leaves the hotspot at 30/23, changing 12 primary and 40 shadow status pixels with max RGB error `0.446441`. The nearest free two-face shift `[-1, -3]` changes the hotspot to 24/24, changes 64 primary and 407 shadow status pixels, and has max RGB error `0.620989`; neither alternative is an improvement. Reproduce the cases with `cargo run --example u1_tube_seam_cut_probe --features tooling -- face261`, `face261-wide`, `face261-wider`, `face261-minimal`, or `face261-near`; outputs are written under `out/u1-reauthored/`. Human review of the two-face and three-face candidates remains required before accepting a seam edit; if both are rejected, U1 needs another TubePinched remedy.
- **U2 (native numerical scope verified; opt-in):** same-chart hit brackets and signed-distance teleport crossings use at most 12 bisection samples. The independent Metal regression traces 64 rays through a 256×2, 16-cycle sampled height profile and checks first-hit distance against an f64 CPU bilinear-texture root. Hardware-filtered height steps improved maximum root error from `6.97845e-4` to `9.29780e-6`; manual-bilinear height steps improved `1.08745e-4` to `9.43360e-6`. A separate 16×16 piecewise-bilinear L1 seam SDF yielded 24 eligible crossings per float-map policy; CPU-root error improved from `0.225398904` to `0.000036631`. Both regressions pass with manual and hardware float-map sampling.

  The earlier 80×64 cube comparison (material set 3, step size `0.02`, atlas 64) still records common-hit distance p50/p95/max changing from `0.01011/0.01711/0.11307` to `0.00574/0.01336/0.10642`; both paths differed from the local fine-step hit mask by two pixels, and that fine trace had seven primary and one shadow budget exit. The independent CPU roots now provide the direct accuracy evidence for hit and seam refinement.

  On the 120×90 `TubePinched` fixture (`around=8`, `long=16`, texture set 0), the U2-only path produced no new budget or invalid-basis exits at times 0, 2.5, 5 and 8. Primary/shadow status deltas were `3/18`, `9/55`, `4/33` and `3/16`; hit deltas were `3/13`, `9/28`, `4/15` and `3/10`. At time 8, primary-only median trace time increased from `0.869542` to `3.278125 ms`. Keep refinement opt-in: measured cost and image/status changes need broader visual review before default promotion. Windows/D3D11 target execution remains unverified.
- **U3 (native numerical scope verified; opt-in):** when a candidate step predicts an SDF sign change inside the four-texel seam band, the shader shortens that step before loading the nearest-sampled destination. The independent 16×16 bilinear seam-SDF fixture produced 24 eligible crossings under each float-map policy. Maximum CPU-root distance error improved from `0.225398904` to `0.016467009` with manual float-map sampling and to `0.014919150` with hardware float-map sampling: reductions of `13.7×` and `15.1×`, both below the nominal `0.04` ray step. The GPU root-contract test passes.

  On the 120×90 `TubePinched` fixture (`around=8`, `long=16`, texture set 0), the U3-only path produced no new budget or invalid-basis exits at times 0, 2.5, 5 and 8. Primary/shadow status deltas were `1/2`, `1/7`, `2/5` and `2/3`; hit deltas were `1/2`, `1/4`, `2/3` and `2/3`. At time 8, primary-only median trace time increased from `0.869542` to `1.383250 ms`; the largest captured max RGB error was `0.554070` at time 5. Keep the path opt-in pending broader visual review. Windows/D3D11 execution remains unverified.
- **U4 (native transform-retry gate verified; opt-in):** the four-retry path compares inverse transforms and shortens a predicted step when relative change exceeds `0.25`. A probe-only affine fixture gives the inverse z basis a steep, piecewise-linear change across one atlas cell; the CPU independently samples the basis texture and recomputes the inverse-transform delta for the same ray. Under manual float-map sampling, the baseline candidate step/change were `0.074193239`/`0.330824286`; retries reduced them to `0.018548310`/`0.182093463`. Under hardware float-map sampling they were `0.074467346`/`0.331156142`, reduced to `0.018616837`/`0.182496154`. In both cases the CPU-checked accepted transform change is below `0.25` and the step shrinks about 4×.

  On the 120×90 `TubePinched` fixture (`around=8`, `long=16`, texture set 0), the U4-only path produced no new budget or invalid-basis exits at times 0, 2.5, 5 and 8. It changed one primary and two shadow statuses at time 2.5, with no status changes at the other poses; maximum RGB error was `0.164022`. At time 8, primary-only median trace time increased from `0.869542` to `3.213500 ms`. Keep U4 opt-in pending broader visual review. Windows/D3D11 execution remains unverified.

### U1 pinched-fixture runtime baseline

On 2026-10-03, reference-path captures on Apple M1 Max / Metal used 160×120 output, atlas 128, `around=8`, `long=16`, texture set 0 and a 10,000-step budget. No U1 treatment was active.

| Fixture/time | Primary hits/escapes | Primary p95/max steps | Primary teleports | Primary budget/invalid |
| --- | ---: | ---: | ---: | ---: |
| `CubePinched`, 8.0 | 617/478 | 73/418 | 2,551 | 0/0 |
| `TubePinched`, 0.0 | 2,108/1,219 | 71/111 | 3,537 | 0/0 |
| `TubePinched`, 2.5 | 2,406/1,208 | 114/259 | 4,436 | 0/0 |
| `TubePinched`, 5.0 | 2,292/1,404 | 88/182 | 4,460 | 0/0 |
| `TubePinched`, 8.0 | 1,552/836 | 104/245 | 2,103 | 0/0 |

Shadow rays also had zero budget and invalid exits in all five captures. The highest-work `CubePinched` primary ray escaped at screen pixel `(74, 66)` after 418 steps and 357 teleports, at UV `(0.5849, 0.6359)`. `TubePinched` repeatedly had an escaping ray at `(46, 63)` with 66 steps and 66 teleports at UV `(0.7940, 0.1829)`. These are candidate U1 stress regions, not independently confirmed visual tornadoes; one-shot capture timings are not benchmark evidence and human review remains open. Raw captures and manifests are under ignored `out/u1-baseline/`.

The escaped UVs lie outside source triangles, but their edgefill samples map them back to specific faces. The `CubePinched` hotspot maps to face 16, chart 0, with rest-metric condition `1.46172`. The `TubePinched` hotspot maps to faces 275/277, chart 0, with condition `1.55665`. Thus the 1.7 face-condition threshold misses both runtime stress locations; seam guidance must use the runtime UV/edgefill mapping as well as rest-mesh conditioning.

### U1 existing damping multiplier sweep

The existing global `damping[0]` value was raised from its default 1.0 while holding the `CubePinched` fixture and all other settings fixed. It was also cross-checked on `TubePinched` at time 2.5. No value introduced a budget or invalid-basis exit, but every candidate still escaped at the baseline high-work rays.

| Damping multiplier | Cube primary p95/max steps; teleports | Cube shadow p95/max steps; teleports | Primary status/hit delta; shadow status delta | RGB max/p99 error | Cube hotspot steps/teleports |
| ---: | --- | --- | --- | --- | --- |
| 1 (reference) | 73/418; 2,551 | 57/77; 411 | 0/0; 0 | 0/0 | 418/357 |
| 2 | 143/365; 2,710 | 112/154; 705 | 21/21; 39 | 0.6814/0.0118 | 365/244 |
| 4 | 272/428; 3,521 | 218/303; 1,624 | 23/23; 47 | 0.7469/0.0167 | 274/23 |
| 8 | 542/852; 6,045 | 428/603; 2,929 | 26/26; 53 | 0.5659/0.0199 | 531/33 |

On `TubePinched` at time 2.5, the reference primary p95/max was 114/259 steps with 4,436 teleports. At 2× these rose to 222/516 steps and 6,566 teleports; the repeated high-teleport pixel changed from 66 steps/66 teleports to 147/147. At 4× they rose to 441/1,029 steps and 9,768 teleports; that pixel had 169 steps/26 teleports while another ray reached 1,029 steps. Primary hit/status deltas were 19/19 and 25/25 pixels at 2×/4×; shadow status deltas were 120/163 pixels. Maximum/p99 RGB errors were `0.5719/0.0271` and `0.6129/0.0381` respectively. Decision: do not promote a global damping multiplier as the U1 remedy. A local forced-seam/teleport remedy remains open; this sweep does not close U1.

### U1 forced-seam CubePinched proof of concept (2026-10-03)

The checked-in [`u1_seam_cut_probe` example](../../examples/u1_seam_cut_probe.rs) consumes the `CubePinched` baseline capture, obtains the edgefill-guided suggestion for face 16, duplicates that face, moves its UV patch by exactly 205/4096 and 1434/4096 into the unused upper atlas band, and copies the decoded height/albedo texels with 32-pixel padding. It saves the reauthored `AssetDocument`, material root, paired PNG/raw planes, and a JSON manifest under ignored `out/u1-reauthored/cube-pinched/`.

On Apple M1 Max / Metal at the baseline 160×120 settings, the modified mesh passed asset/topology validation and rendered. A CPU atlas check found zero texel-center overlaps between the moved patch and surrounding faces. Primary status counts and hit mask were identical to the reference; the hotspot ray changed from `Escaped, 418 steps, 357 teleports` to `Escaped, 68 steps, 5 teleports`. Global primary p95/max steps changed from `73/418` to `71/116`, and total primary teleports fell from `2,551` to `1,654`. No new budget/invalid exits appeared. Two shadow status pixels changed; maximum/mean RGB error was `0.01104/4.73e-7`. This is a single-fixture proof of concept, not a performance result or human visual approval. The paired frames and raw diagnostics remain available for review.

The checked-in example now also writes a 640×480 baseline/candidate pair under `out/u1-reauthored/cube-pinched-640/`, using the same settings and asset rewrite. On Apple M1 Max / Metal, the high-resolution pair preserved all primary hit/status pixels and introduced no budget/invalid exits. The pinned hotspot changed from `Escaped, 360 steps, 302 teleports` to `Escaped, 65 steps, 5 teleports`; global primary p95/max steps were `73/419` vs `71/134`, and teleports fell from `36,605` to `25,851`. Shadow status changed on 19 pixels, with shadow hit/escape counts `805/8,206` vs `810/8,201`; max/mean RGB error was `0.161114/4.46e-6`. The captures make the seam appearance reviewable at higher resolution, but visual acceptance remains pending.

**CubePinched human review gate:** compare the 640×480 baseline and candidate for height/albedo continuity across the new face-16 seam, silhouette changes, and whether the original pole/tornado artifact visibly improves without introducing a stronger artifact. Record accept/reject and any observed issue; numeric status and color summaries support this review but do not replace it.

The checked-in [`u1_tube_seam_cut_probe` example](../../examples/u1_tube_seam_cut_probe.rs) now exercises the TubePinched hotspot after the U5 false-positive correction. The edgefill suggestion starts at face 277; a two-ring authoring patch `[13, 14, 15, 277, 278, 279]` moves into an unused atlas region with zero overlapping texel centers and passes `validate_pose_topology`. On Apple M1 Max / Metal at 640×480, however, baseline and candidate primary/shadow status maps and RGB output are identical; the hotspot remains `Escaped, 67 steps, 67 teleports`. This patch does not mitigate the TubePinched hotspot. Its pair and manifest are under ignored `out/u1-reauthored/tube-pinched-640/`. Follow-up face-261 cases now provide two measurable alternatives; see the U1 evidence above for their full results and reproduction commands. Both the two-face and three-face cuts reduce the pinned ray from 30 steps/23 teleports to 22/5 with no new budget/invalid exits. Their primary/shadow status changes and RGB error differ, so neither is accepted on metrics alone.

**TubePinched human review gate:** compare the 640×480 baseline against the two-face `face261` candidate and three-face `face261-wide` candidate for texture continuity, silhouette changes and hotspot reduction. Record which candidate, if either, is acceptable; if both are rejected, U1 still needs another TubePinched remedy.

Reproduce the CubePinched capture and seam-cut example on a native GPU with:

```sh
scripts/in-nix cargo run --locked --features viewer -- capture --out out/u1-baseline/cube-pinched --mesh cube-pinched --around 8 --long 16 --atlas 128 --width 160 --height 120 --texture-set 0 --time 8 --animation-amount 1
scripts/in-nix cargo run --locked --example u1_seam_cut_probe --features tooling
scripts/in-nix cargo run --locked --features viewer -- capture --out out/u1-baseline/tube-pinched --mesh tube-pinched --around 8 --long 16 --atlas 128 --width 160 --height 120 --texture-set 0 --time 8 --animation-amount 1
scripts/in-nix cargo run --locked --example u1_tube_seam_cut_probe --features tooling
```

With all three flags enabled, the corrected one-shot cube capture had primary/shadow hit-mask differences of 6/36 pixels (0.008%/0.047%) and no budget or invalid-basis exits. Its primary maximum step count was 537, showing that the combined path can increase tail work. The single trace-time sample was 4.73 ms versus 2.39 ms reference; no speedup is claimed. A one-step GPU budget test still returned explicit exhaustion. Separate per-option bent-fixture runs, seam crops and independent nonlinear heightfield truth remain required before promoting any U1-U4 option.

A repeated probe on M1 Max / Metal used a 160x120 `TubePinched`, atlas 128, material set 0, and pose times `0, 2.5, 5, 8`. With U2-U4 all enabled, primary/shadow status differences by pose were `6/32`, `13/83`, `5/55`, and `6/29` pixels; hit-mask differences were `6/19`, `13/49`, `5/34`, and `6/14`. No new budget/invalid exits appeared, but maximum RGB differences ranged from `0.332` to `0.681`. In the serialized native suite, the 12-sample pose-8 median was `1.844334 ms` reference versus `8.100208 ms` combined U2-U4. Decision: do not combine/promote the three options. This does not isolate the value of each option; individual bent-fixture and independent nonlinear-height truth gates remain open.

A second bent-fixture probe isolated U2, U3 and U4 on the same four poses, then collected 12 paired trace samples at pose 8. U2 primary/shadow hit differences were `3/13`, `9/28`, `4/15`, and `3/10` pixels by pose, with max RGB deltas `0.381/0.515/0.575/0.251`; U3 hit differences were `1/2`, `1/4`, `2/3`, and `2/3`, with RGB deltas `0.050/0.269/0.554/0.067`; U4 had no status/hit delta at poses 0, 5 and 8, a `1/2` primary/shadow status/hit delta at 2.5, and RGB deltas `0/0.164/0/0`. A separate warmed run measured time-8 trace medians `1.400125 ms` reference, `3.67375 ms` U2, `2.215125 ms` U3, and `3.275291 ms` U4. None added budget/invalid exits. These local probes do not use independent nonlinear heightfield truth; leave all three options disabled by default pending that oracle and visual review.

Lighting-mode 0 provides a primary-only comparison; mode 4 includes normal/albedo shading and local shadows. In one paired 12-sample time-8 run, primary-only medians were `0.773792 ms` reference, `2.294458 ms` U2, `1.232541 ms` U3 and `2.491125 ms` U4. Shadow-enabled medians were `1.396375`, `3.642`, `2.214458` and `3.266042 ms`; their differences include shading work as well as local-shadow rays. This shows the traversal options add work on both primary and shadow-enabled paths for this fixture.

## U5 self-intersection diagnostics

U1's TubePinched reauthor attempt exposed a false positive: chart-split vertices at cap/body contacts differed by a few float ULPs, producing six rest-pose warnings. `find_self_intersections` now aligns vertices already matched by its positional epsilon before testing triangle interiors. The `tube_pinched_rest_seam_contact_is_not_a_self_intersection` regression was observed RED with six reports, then GREEN after the fix; the public topology preflight now accepts the reference TubePinched rest mesh. Existing true-overlap and animated-tunnel regressions continue to pass.

`AssetDocument::validate_pose_topology` rejects T-junctions and posed triangle crossings in the identity/rest pose before asset rendering, with vertex/face/chart-component identifiers. The self-intersection pass uses an x-axis AABB sweep; it checks non-coplanar crossings for all pairs and positive-area coplanar overlap even when pairs share geometric vertices or edges. Point-only and edge-only contact remains valid. `asset_diagnostics::find_orientation_flips` reports posed triangles whose oriented area flips or collapses, with face/chart IDs and signed area ratio. Synthetic T-junction, shared-vertex/edge coplanar overlap, adjacent-face contact, non-coplanar intersection and reflected-face CPU regressions pass. The reference tube's animated time 10 produces crossing and orientation warnings; these runtime U5 findings remain diagnostic so a valid rest asset can render. The face-local flip test does not prove the surface-space map is globally folded, and continuous collisions between sampled poses remain open. Better-behaved mapping research remains open under R1.

The focused `animated_reference_tunnel_is_reported_without_rejecting_rest_geometry` test now checks both diagnostics at the reference tube's time-10 pose while confirming that identity/rest topology remains valid. The preserved paper says the current negative-distortion `0.01` step often detects compression late, leaving visible tunnels; it describes look-ahead as potentially costly with uncertain visual benefit and points to anti-squash bones or finer tessellation as content-side fixes ([self-intersection discussion](../../paper/DASHR_Paper.html#animation-distortion-0.0)). A proposed U5 no-go is to retain fail-closed rest validation and animated-pose diagnostics, and defer a general look-ahead or mapping redesign until a representative authored bent asset has agreed visual criteria. U5 remains open pending approval of that disposition; global map folds and continuous collisions remain open under R1.

## U16 supported topology contract

`AssetDocument::validate` checks schema and triangle-record structure. `AssetDocument::validate_pose_topology` additionally rejects T-junctions and posed/rest self-intersections with stable vertex, face and chart IDs, rejects indexed edges incident to more than two triangles, then applies the same seam-pair rules used by the map bake. Unpaired, ambiguous and nonmanifold edges fail with face and edge-vertex IDs before asset baking. The validator checks these specific geometry failures first, so their diagnostic IDs are preserved instead of being replaced by a generic open-edge error.

The new `pose_topology_preflight_rejects_open_triangle_soup_with_element_ids` regression was observed failing before the change because the public preflight accepted a single unpaired triangle. A second RED regression showed that three faces sharing one indexed edge fell through to a generic open-edge error; preflight now reports the nonmanifold edge and its three incident triangles. `cargo test --locked --offline --test library_api --test topology` passes all 23 focused tests, including existing T-junction, crossing, coplanar-overlap, valid-contact and closed-reference-mesh cases.

Lone-edge and thin-sheet support is a separate P13 proposal. Schema v1 contains triangles, UVs, metric basis data and skinning weights, but has no explicit cliff-edge geometry, per-boundary surface role or opposite-side UV mapping. A support implementation would therefore have to invent how the open boundary continues through the displaced surface and how it maps across the edge. This matches P13's explicit stop condition: when open edges require a different surface-space definition, record a separate research question rather than ship partial mapping. The source paper also identifies surface-space ambiguity for unusual topology ([out-of-scope discussion](../../paper/DASHR_Paper.html#out-of-scope-for-my-work.)). Keep open sheets rejected in the current library; propose deferring the extension until P13 defines that contract and supplies authored fixtures. U16's native rejection path is verified; this P13 no-go disposition remains pending approval.

## U6 scale-derived tolerances

`Settings::epsilon_policy` now selects `Reference` or opt-in `ScaleDerived`. The default preserves the existing tolerances. The derived profile sets the normal-map finite-difference delta to `1 / atlas_size` in normalized UV units and the local-shadow origin bias to `0.001 * max(length, 2 * radius, thickness)` object-space units. `Capture` manifests include `resolved_tolerances`; unit tests check the formula and the uniform values. The native derivation contract is verified; the profile remains opt-in until its shadow and appearance changes receive independent review.

The first matched 320×240 cube capture on Apple M1 Max / Metal kept primary statuses and hit mask identical. Shadow hits changed from 251 to 277, with 30 hit-mask differences, no budget/invalid exits, and maximum shadow steps increasing from 83 to 103. The one GPU timing sample was 1.90 ms for `Reference` and 2.09 ms for `ScaleDerived`; this is not a benchmark. Captures and resolved values are under ignored `out/u6/`.

The focused five-bucket GPU regression was refreshed on Apple M1 Max / Metal at 80×64 output and atlas 64. It compared small `2/0.5/0.25`, reference `4/1/1`, large `8/2/2`, thin `4/1/0.15`, and thickness-dominant `4/1/6` length/radius/thickness settings. Primary statuses and hit masks matched in every bucket, and the derived profile added no budget or invalid-basis exits. Shadow status/hit disagreements were `1, 7, 2, 5, 50` pixels; maximum RGB errors were `0.037427, 0.459591, 0.145422, 0.305865, 0.565049`. Keep `ScaleDerived` opt-in. Independent visual review, Windows/D3D11 execution and more varied fixtures remain open.

## U7 stored inverse atlas experiment

An opt-in `stored_inverse` path writes the inverse surface basis to three additional full-float atlas planes and samples those planes at hits instead of inverting the filtered surface basis in the trace shader. The native Metal cube probe on Apple M1 Max reported zero primary-status disagreements but a maximum RGB difference of `0.0036595762` at 80x64 output / atlas 64. The accuracy issue is structural: interpolating precomputed inverse bases is not equivalent to filtering the forward basis first and then inverting it, which is the reference operation.

The three `RGBA32F` planes add 48 bytes per atlas texel against the existing 64 bytes for four warp planes (+75% atlas storage), add one full-atlas pass (split across passes on narrow-MRT devices), and require three extra sampled planes at a hit. The updated U7 GPU regression passed on Apple M1 Max / Metal: zero primary-status disagreements and maximum RGB error `0.0036595762`.

The repeated comparison followed the repository benchmark protocol with five runs, 120 warm-up frames and 300 measured frames per variant per run, alternating A/B order. Across-run medians for the main trace pass were `0.710791 ms` reference and `0.705000 ms` stored inverse (0.8% lower). Summed GPU-pass medians were `0.809125 ms` and `0.844291 ms` (+4.35%); summed GPU p95 medians were `0.944208 ms` and `1.025458 ms` (+8.60%). The trace improvement is below the proposed 10% adoption threshold and the total p95 regression exceeds 5%; this supports a no-go for default promotion. Keep U7 open pending approval of that disposition; preserve the experiment opt-in for reproductions. The five-run report is generated at `out/u7/paired-timing-80x64-atlas64-2026-10-03.json` by `cargo test --locked --offline --features tooling --test gpu stored_inverse_probe_reports_cube_quality_delta -- --ignored --nocapture`.

## U8 compute edgefill prototype

An opt-in `compute_edgefill` pipeline reads the raw deformation planes and static source map, then writes all four gutter/distortion planes to distinct `RGBA32F` storage textures. Each invocation owns one destination texel and reads only the raw textures and immutable maps, so the schedule has no read/write race. The Metal probe on Apple M1 Max at 80x64 output / atlas 64 / cube material set 3 had maximum atlas error `4.77e-7`, no primary-status disagreements and zero RGB output error against the raster path.

The serialized native GPU suite's 12-sample cube comparison measured edgefill medians `0.079875 ms` raster and `0.014625 ms` compute. A separate three-pose pinched-tube check matched raster maps within `4.77e-7`, with identical statuses, no new budget/invalid exits and zero RGB error. The requested region-only copy schedule is a no-go under the current trace contract: topology baking assigns every atlas texel a nearest surface source, and a deformed ray can query any UV in its envelope, so skipped cells could retain stale transforms. Revisit only with a proven reachable-UV bound. Timing on the bent fixture and another adapter remain open.

A separate U8 quality probe used `TubePinched` at animation times 0, 4, and 8 on Apple M1 Max / Metal. Each pose matched raster atlas maps within `4.76837e-7`, with identical statuses, no new budget/invalid exits, and zero RGB error.

The curved-fixture performance run followed the benchmark protocol with five runs, 120 warm-up frames and 300 measured frames per variant per run, alternating A/B order. For `TubePinched` at 96×72 output / atlas 128 / `around=8` / `long=16`, across-run median edgefill time fell from `0.059125` to `0.013959 ms` (76.4%); total GPU median fell from `2.155500` to `1.964541 ms` (8.9%), and total GPU p95 median fell from `2.362792` to `2.146958 ms` (9.1%). The full-atlas compute path meets the edgefill target-cost gate without a total-frame regression. The five-run aggregate is at `out/u8/tube-pinched-paired-timing-96x72-atlas128-2026-10-03.json`.

Selective writes remain unimplemented. The current ray contract can query any UV in the normalized atlas envelope, so the proposed region-only dispatch has no proven conservative write mask. A no-go for selective writes while keeping the measured full-atlas compute path is proposed, pending approval; another adapter and Windows/D3D11 execution remain open.

## U9 mesh-fin alternative decision

No fin renderer was added to the library. The pinned wgpu 30.0.1 classic `RenderPipelineDescriptor` exposes vertex and fragment stages; the separate mesh pipeline API is capability-dependent and is not a portable path for this library ([wgpu 30 render pipeline](https://docs.rs/wgpu/30.0.1/wgpu/struct.RenderPipelineDescriptor.html), [wgpu 30 mesh pipeline](https://docs.rs/wgpu/30.0.1/wgpu/struct.MeshPipelineDescriptor.html)). CPU-expanded fins are technically possible, but would need deterministic seam adjacency, matching skinning/metric attributes and a new atlas-coverage path. The full-atlas U8 compute path now has a five-run curved-fixture performance result: edgefill median fell `76.4%` and total GPU median `8.9%` while matching raster output, so fins have no measured performance case yet. No-go for a mesh-fin implementation in the current portable library; revisit after the authoring schema preserves explicit seam/fin relationships and a matched prototype demonstrates a benefit over U8.

## U10 trace-time edgefill indirection experiment

An opt-in `indirect_edgefill` path skips the dynamic gutter/distortion pass and reconstructs transforms by filtering the static edge-source map in the trace shader, reading the raw deformation atlas, and calculating distortion from neighboring samples. The cube probe on M1 Max / Metal at 80x64 output / atlas 64 preserved all primary hit/status values and added no budget/invalid exits. Maximum RGB difference was `0.0037953258`.

The refreshed cube comparison used five alternating A/B runs with 120 warm-up frames and 300 measured frames per variant per run. It preserved all `5,120` hit/status samples and added no budget/invalid exits, with maximum RGB error `0.003795326`. Across-run trace medians were `0.711625 ms` raster and `0.811667 ms` indirect (+14.1%); total GPU medians were `0.811000` and `0.850791 ms` (+4.9%). Trace p95 median rose from `0.759125` to `0.904542 ms`; total p95 medians were `1.021375` and `1.017084 ms`. Run 1 had much higher timestamps for both variants and remains included in the five-run spread. This does not meet the project's optimization adoption threshold; a no-go for default promotion is proposed, pending approval. Keep the path opt-in as a reproducer. Curved/seam-specific error and other adapters remain unmeasured. The paired aggregate is at `out/u10/indirect-edgefill-cube-80x64-atlas64-2026-10-03.json`.

## U12 split teleport distance and destination

An opt-in `split_teleport` path keeps the signed seam distance in an `R32Float` texture and the discontinuous destination in `Rg32Float`. The trace samples the scalar distance on each step and loads the destination only when teleporting. The native cube probe reproduced both channels exactly, with zero primary/shadow status disagreements and zero RGB difference. Static seam-map storage fell from 16 to 12 bytes per atlas texel.

The refreshed cube comparison followed the benchmark protocol with five alternating A/B runs, 120 warm-up frames and 300 measured frames per variant per run. Across-run trace medians were `0.710458 ms` packed and `0.705500 ms` split; total GPU medians were `0.811500` and `0.811001 ms`; total GPU p95 medians were `1.030082` and `0.928166 ms`. Exact map/frame equality remains intact, and packed/split seam-map storage is `16/12` bytes per texel. Treat timing as neutral given run spread; retain split storage as an opt-in memory tradeoff pending curved-seam review on another adapter. The paired report is at `out/u12/split-teleport-cube-80x64-atlas64-2026-10-03.json`.

## U14 compact transform atlas

An opt-in `compact_warp` path stores the four raw and four guttered transform planes as `RGBA16F`; trace outputs, materials, seam maps and metric vertex attributes remain unchanged. On the M1 Max / Metal cube fixture at 80×64 output and atlas 64, raw-plus-warp storage fell from 128 to 64 bytes per atlas texel. Primary and shadow status codes matched the full-float reference, while maximum atlas component error was `0.015960693` and maximum RGB error was `0.0025811195`. The refreshed 12-sample trace medians were `0.738709 ms` full-float and `0.696000 ms` for RGBA16F; this single cube result is not a benchmark acceptance.

The pinched bent-tube probe at pose 8 / atlas 128 measured maximum map error `0.03227496`, four primary and ten shadow status differences, and maximum RGB error `0.3348779`; no new budget/invalid exits appeared. The bent case fails the geometry/status quality gate. A no-go for default promotion of this RGBA16F variant is proposed, pending approval. Keep it opt-in for reproductions; the experiment does not compact vertices or teleport maps.

A pinched bent-tube probe at pose 8 / atlas 128 measured maximum map error `0.03227496`, 4 primary and 10 shadow status differences, and maximum RGB error `0.3348779`; no new budget/invalid exits appeared. The format is not acceptable for this tested bent asset.

## U11/U13 multi-seam representation decision

**U11 no-go (closed for the initial library).** `Maps::teleport` is one `Vec4` per texel: one signed distance in `z` and one destination in `xy`. The bake keeps one winning seam distance/destination at a texel and stores no stable edge or seam-group ID. A corner field therefore has no defined two-distance combination or deterministic destination choice. No analytic corner fixture or combination rule is established; keep the two-channel corner SDF outside the initial library. This no-go was recorded in the accepted execution plan.

**U13 no-go (closed for the initial library).** Multiple filtered destinations require stable seam-group IDs so filtering cannot mix coordinates across distinct discontinuities. `Maps` and `AssetDocument` lack those IDs, and the renderer contract keeps teleport destinations point-sampled. A versioned exporter/schema contract and a per-group destination fixture are prerequisites to revisit the proposal. The U12 split preserves the existing single-owner data and does not solve U13.

## U17 conservative bounds and host lighting boundary

The library API returns per-pixel hit UV, ray distance and actual reverse-Z hit depth, plus separate primary and local-shadow termination diagnostics. The existing flat-chart Metal oracle compares the hit plane, including projected depth, against CPU camera-ray intersections at a `1e-5` distance/depth budget; `gpu_uniform_sampler_and_affine_contracts` passed with both manual and hardware float-map sampling on this checkout.

The public-library `u1_seam_cut_probe` was rerun on Apple M1 Max / Metal at 160×120, `CubePinched`, time 8, atlas 128, lighting mode 4. It reads `FrameOutput` from both the reference asset and a remapped seam-cut asset. The reference hotspot returned `Escaped` with 418 steps and 357 teleports and hit UV/distance/depth `[0.584923, 0.635862, 2.439656, 0.009692]`; the candidate returned `Escaped` with 68 steps and 5 teleports and `[0.607095, 0.928471, 2.316052, 0.009809]`. Across the full frame, primary hit/escape counts were unchanged at `617/478`, shadow hit/escape counts were `55/514`, there were zero budget/invalid exits, and the seam remap changed two shadow statuses. This exercises host-visible depth and separate status planes after deformation, edgefill/gutter remapping and teleports. It does not approve the visual seam edit; U1 human review remains open.

Hosts can interpret shadow `Hit` as locally occluded, `Escaped` as no local occluder, and `BudgetExhausted` / `InvalidBasis` as unknown while applying their own scene shadows. This contract is documented in [library API outputs](library-api.md#outputs-and-diagnostics).

No production min/max height pyramid or horizon map was added; the Menger BVH under `src/probe/` is isolated U19 research and does not accelerate the height-atlas renderer. A bound built over one atlas interval is not yet proven conservative after animation deformation, gutter remapping and seam teleport, so skipping samples could manufacture misses. The accepted execution plan records a no-go for production acceleration until an adversarial bent/seam proof and a matched trace benchmark exist. AO, SSS, and long-range/inter-object visibility remain host material or scene responsibilities. U17 is closed for the native library scope under this plan decision; target-engine lighting/depth integration remains open.

## U15 specialized inverse experiment

`Settings::specialized_inverse` selects a 3x3 cofactor inverse instead of the existing general 4x4 affine inverse; it is off by default. CPU tests compare both paths on skewed metric bases and reject singular input. The GPU contract probe ran the specialized path against the analytic flat chart for every existing distortion mode; Naga translates the production variants to HLSL/MSL, and the runtime probe executes on Metal.

The refreshed cube comparison followed the benchmark protocol with five alternating A/B runs, 120 warm-up frames and 300 measured frames per variant per run. Primary/shadow status maps matched and maximum RGB error was `5.96046e-8`. Across-run trace medians were `0.713042 ms` general and `0.707209 ms` cofactor (0.82% faster); total GPU medians were `0.811249` and `0.812916 ms` (+0.21%). This is below the project's 10% target-cost adoption threshold. The user approved a no-go for default promotion on 2026-10-03; keep the option opt-in for reproductions. Actual Windows/FXC compilation and another adapter remain open. The paired report is at `out/u15/specialized-inverse-cube-80x64-atlas64-2026-10-03.json`.

## U19 non-heightfield research closeout

Task 8's U19 acceptance is bounded feasibility evidence with one decision per representation; it does not require adding a new production renderer. In the current checkout, `gpu_uniform_sampler_and_affine_contracts` passed its analytic SDF/warp and dense-voxel checks on both manual and hardware float-map policies. The SDF probes match 64 CPU root statuses with zero sampled conservative-step violations; the blended warp's minimum sampled Jacobian determinant is `0.109991`. The depth-2/3/4 finite-Menger test and 256-splat Gaussian adapter test also passed on M1 Max / Metal. The Menger depth-4 probe keeps four CPU-confirmed misses as `Budget` exits, rather than false escapes or hits. Gaussian CPU/GPU sorting and compositing agree, but the 256-splat mask over-covers the matched mesh (silhouette IoU `0.541`) and its opacity-weighted center depth is not a unique surface hit. Dense voxel accuracy improves at 64³, but payload grows 8× and no speed advantage is established.

This closes U19 for the bounded research/recommendation scope under the accepted Task 8 plan. Keep general SDF deformation, arbitrary fractal estimators, Gaussian surface-hit semantics and sparse voxel streaming out of the production API. Authored assets, broader families, full-frame human review, larger workloads and production adapters remain promotion gates; see the [U19-U20 research note](../research/2026-10-03/non-heightfield-warps-u19-u20.md).

## U19 finite Menger BVH follow-up

The tooling-only Menger probe compares the original flat scan with a median-split BVH over 400 depth-2 boxes. Point-to-node-AABB distance is the conservative pruning bound; leaves evaluate exact box SDFs. A CPU 9³ query grid matched the flat finite-union field with zero maximum error. A matched 32×32 GPU run on Apple M1 Max / Metal traced 1,024 deterministic rays against exact CPU ray-box roots. Both paths had zero status disagreements and conservative-step violations; hierarchical hit-distance error was zero and its fixed 32-entry stack did not overflow. The BVH evaluated `6.009` box SDFs per distance query on average versus 400 for the flat scan, with `17.498` node visits. The current stored aggregate reports flat median/p95 `3.875417/3.890000 ms` and BVH `1.314709/1.320208 ms`; an earlier matched run recorded `3.923125/4.793625` and `1.320459/1.323542 ms`. Treat the timing spread as run-specific, not a general speed claim. Ten measured CPU builds after two warm-ups took median/p95 `0.030041/0.032042 ms`. The tree uses `12,240` bytes plus `6,400` bytes for boxes.

A depth-3 follow-up exercises the runtime-sized WGSL box array with 8,000 exact boxes and a 4,095-node BVH. Its 9³ CPU distance check has zero maximum error. On the same M1 Max / Metal adapter, all 1,024 GPU ray statuses and hit distances matched CPU ray-box roots, with 496 hits, zero conservative-step violations, and no stack overflow. The BVH averaged `8.622` box evaluations and `27.858` node visits per distance query; p50/p95/max trace steps were `14/69/131`. One CPU build took `1.116 ms`; no depth-3 GPU timing was collected. The raw result is under ignored `out/probe/capabilities.json`. Depth 4 has the explicit budget exits below; repeated depth-3/depth-4 timings, other fractal families/adapters, and larger workloads remain open. The experiment stays outside the production library API; see the [U19-U20 research note](../research/2026-10-03/non-heightfield-warps-u19-u20.md).

The depth-4 follow-up scales to 160,000 boxes and a 123,391-node BVH. The 9³ CPU query grid again matched the flat union exactly. Of 1,024 M1 Max / Metal rays, 400 exact hits matched CPU distance with zero error; there were no conservative-step violations or stack overflows. Four exact-miss rays near `(±0.5078, ±0.5078)` remain `Budget` after the 1,024-step limit because the SDF stays near zero in empty channels. No expected hit exhausted the budget, and no miss was reported as a hit. The BVH averaged `7.381` box evaluations and `42.240` node visits per query, with p50/p95/max steps `15/467/1024`. One CPU build took `33.475 ms`; depth-4 GPU timing was not collected. Deeper stages and general fractal fields remain unverified.

## U19 Gaussian GPU depth-sort follow-up

The isolated Gaussian adapter now sorts each pose's 256 canonical splats on the GPU with a bitonic workgroup kernel before compositing. On Apple M1 Max / Metal, the GPU order exactly matched the independent CPU order for both poses with zero inversions; the order-index buffer uses `2,048` bytes. CPU/GPU output had zero pixels outside the `1e-3` tolerance; maximum color, coverage and depth errors were `3.15e-7`, `4.05e-7` and `6.62e-6`. The current stored aggregate reports depth-sort median/p95 `0.028292/0.028625 ms` and compositing `0.229625/0.231042 ms`; an earlier run recorded `0.051834/0.052375` and `0.428291/0.430834 ms`. Timings vary; no speed or promotion claim is made. The 0.5-coverage silhouette IoU remains `0.541` against the skinned mesh, so sort correctness does not close the shape-quality gap. The sort kernel is fixed to 256 primitives; larger authored assets and visual review remain open. See the [U19-U20 research note](../research/2026-10-03/non-heightfield-warps-u19-u20.md).

## U20 dense voxel resolution comparison

The tooling-only inverse-LBS voxel probe now runs both `32³` and `64³` float32 SDF grids through the same 120-pose, 32-ray-per-pose sequence and the same analytic/voxel CPU root references. On Apple M1 Max / Metal, both resolutions matched all voxel CPU and analytic statuses, with zero inverse failures and conservative-step violations. Increasing the payload from `128 KiB` to `1 MiB` reduced maximum analytic hit-distance error from `0.016367` to `0.003553` object units and maximum sampled interpolation error from `0.068430` to `0.033672`; the 64³ trace used `33/80/118` p50/p95/max steps versus `34/81/131` at 32³. Repeated GPU timings varied; median dispatches were about `0.35 ms` at 32³ and `0.32 ms` at 64³, without a claimed speedup.

## U20 GPU-space-warp research closeout

The two-bone inverse-LBS SDF and dense-voxel tests were rerun on Apple M1 Max / Metal on this checkout. The 120-pose SDF probe passed all 3,840 CPU-root comparisons with zero inverse failures or conservative-step violations; maximum hit-distance error was `0.000161648`. Its conventional 1,488-triangle skinned-mesh baseline had zero status disagreements and maximum analytic-depth difference `0.0237153`. The actual `π`-radian stress pose still reports 341 self-intersecting triangle pairs, a negative certified inverse bound (`-2.65`), and `Invalid` for all 32 rays before inversion.

The dense-voxel test also passed for both 32³ and 64³ fields across 120 poses/3,840 rays, with zero CPU status disagreements, inverse failures or conservative-step violations. At eight times the payload, 64³ reduced maximum analytic hit error from `0.016367` to `0.003553`. These results close U20's bounded native feasibility scope under Task 8. Full-frame silhouette/normal/temporal review and external GPU-warp integration remain open promotion gates; neither warp is exposed by the production library.

The inverse-LBS stress pose now also reaches an actual self-intersecting mesh. At `π` radians, the estimated endpoint-surface clearance is `-0.5` object units, the 1,488-triangle mesh reports `341` intersecting triangle pairs, and the certified inverse lower bound is `-2.65`. All 32 stress rays return `Invalid` before inversion. This verifies fail-closed behavior at a pose with actual mesh overlap; it does not verify surface quality through the fold. Full-frame silhouette/normal, temporal, and visual review remain open. See the [U19-U20 research note](../research/2026-10-03/non-heightfield-warps-u19-u20.md).

## Roadmap gate status

| Phase | Implemented capability | Acceptance still required |
| --- | --- | --- |
| P0 | Source preserved; software attribution/license and Rust dependencies documented/pinned | Complete reference-build/asset provenance and additional reference fixtures |
| P1 | CPU affine/metric/anchor contracts, explicit GPU layout, analytic trace/depth oracle, deterministic captures and GPU timestamps | Intermediate D3D11 map comparison and additional camera/pose fixtures; complete benchmark protocol |
| P2 | Real Mac device, four full-float MRTs, filtered/nearest/manual reads, uniform and offscreen readback; forced split paths | Real Windows adapter/pipelines/results; no browser claim |
| P3 | All four procedural assets, fixed-bone tube poses, both encodings and GPU-occupancy seam/gutter bake | Cross-backend atlas comparisons and reviewed seam/bend sequence |
| P4 | Shell tracing, damping, teleports, interpolation, five lighting modes/local shadows, status diagnostics and optional hit depth | D3D11 single-asset parity; reviewed Mac/Windows animation; multi-asset occlusion; near/camera-inside policy validation |
| P5 | Partial: static Blender exporter, versioned mesh document writer, Blender 5.2.2 sphere-like fixture, independent consumer round trip and constant/ramp texture-root smoke runs | CPU displaced-mesh analytic comparison for the exported sphere, ramp hit accuracy, and reviewed seam/silhouette output |
| P6–P7 | No implementation claimed | Measured optimizations and optional engine integration remain separate work |
| P8 | Headless default Rust library, versioned asset/cache formats, independent consumer, matching pre/post captures | Local P8 gates passed; D3D11 parity remains under P0-P4 |
| P9–P13 | P9 C ABI preview, generated header, per-thread logging callback, C smoke client and Naga HLSL/MSL translation smoke pass; P10–P13 have no implementation claim | Target engine shader compilation/render graphs, engine-owned resources, Blender/Unity/Unreal integrations and topology extensions |

## Known limits

The library is consumable but the renderer still targets the reference's single-asset, four-bone fixtures. It has no general rig importer, inter-object shadows, reflections, translucent displacement, wireframe overlays, Blender integration or browser build. Pinched/strongly compressed fixtures can still fold or exhaust traversal. A successfully saved diagnostic capture does not mean its geometry passed quality gates.

Windows C++ snapshot export and one external D3D11/Metal image comparison are verified. Windows Rust rendering, Linux execution, independent displaced-mesh truth and human animation review remain unaccepted. P8 extraction is accepted locally; no rendering parity or performance improvement is claimed.
