# DASHR Library and U1-U20 Implementation Plan

> Inline execution plan for the user's request to complete the 20 upstream future-work items and extract a standalone Rust library.

**Goal:** Deliver the standalone DASHR Rust library and close out U1-U20 one item at a time, with each item's own evidence and acceptance decision.

**Architecture:** Keep the existing package and library entry point, but make viewer and CLI dependencies optional and keep them out of default library builds. Publish a documented Rust API around validated assets, GPU context/resources, pose updates, rendering outputs and explicit termination diagnostics; migrate the viewer and a separate headless consumer to that API. Implement robustness and topology changes separately from optimization experiments, preserving the full-float reference path and accepting an optimization only when the benchmark protocol shows a benefit without exceeding its quality gates.

**Tech Stack:** Rust 2024, wgpu/WGSL, Naga validation, serde asset/settings formats, existing native GPU and analytic fixtures.

**Spec:** `docs/plans/standalone-library-and-integrations.md`, `docs/plans/standalone-library-goal.md`, `docs/plans/upstream-future-work.md`, `docs/plans/dashr-porting-roadmap.md`, and `docs/port/validation.md`.

## Global Constraints

- Preserve Tom Forsyth's attribution, metric tangent/bitangent lengths, full-float baseline, host/shader layout, filtered seam-distance sampling and nearest teleport-destination loads.
- No algorithm change may be bundled with P8 extraction; capture and compare the pre-extraction reference first.
- Keep UI/window dependencies out of the default library dependency graph; keep build, UI, file I/O and viewer state out of the core API.
- Return termination states and resource errors to consumers; never convert budget exhaustion or invalid bases into hits.
- Treat U7-U15 as separately measured experiments; retain baseline behavior when an alternative fails the benchmark or quality gate.
- Keep U19-U20 experiments isolated from the production heightfield library.
- A native pass does not close Windows/D3D11 parity, engine integration, visual review or other roadmap gates.

## Serial U-item closeout

Tasks 4-8 below record grouped implementation and research already completed. They are historical evidence, not permission to batch the remaining work. Future U-item work proceeds through the register in `docs/plans/upstream-future-work.md`, one active item at a time, in U1-to-U20 order. U1 is the first closeout item. P8's locally accepted library extraction remains a separate cross-cutting deliverable; integration requirements mapped to U18 stay with U18.

For each U item:

1. Read its roadmap row and linked validation record; restate the remaining acceptance criteria and capture a same-settings baseline.
2. Make one isolated implementation or research change for that item. Do not combine neighboring U items or promote an opt-in experiment by implication.
3. For code changes, add the focused regression first, verify the expected failure, then implement and verify the passing result. Run the repository's required checks for the change.
4. Record the exact fixture, adapter/backend, settings, output or metric, and any failure or external review gate in the validation record and this progress ledger.
5. Mark the item closed only when its item-specific criteria pass, or when a documented evidence-backed scope decision resolves the item. Keep unresolved platform, engine, and human-review criteria open; do not describe them as implemented.
6. Only after recording that decision, advance the single active item to the next U ID. If a required external gate prevents closure, leave the item open, record the blocker, and proceed only with a later item that has no dependency on it.

The upstream register remains the source of truth for each item's status. Prior prototype results count as evidence, but each item must be rechecked against its own acceptance criteria before it can be closed.

## Review Focus

- Malformed, non-manifold, open, T-junction and self-intersecting input must return an error with element identifiers rather than panic or silently bake.
- Singular or ill-conditioned metric bases, seam discontinuities and distortion poles must remain visible in diagnostics.
- Teleport channel changes must preserve discontinuity-safe destination loads and measured seam accuracy.
- Compact formats and traversal changes must not hide budget exits or increase missed detail beyond the existing quality budget.
- Library teardown, failed device creation and caller-owned device/resource lifetimes must be explicit and repeatable.

---

## Execution Tasks

### Task 1: Freeze the current reference — complete

Recorded source revision `fe215d350e4f604298a09e268bb53c575fc9ad59`, Rust 1.95.0, and deterministic tube/cube captures before extraction. Both captures used Apple M1 Max / Metal with 64 bytes/sample, four full-float MRTs, 320x240 output, atlas 256, material set 0 and time 10. Artifacts and manifests are under ignored `out/pre-p8/{tube,cube}`. The regular Nix wrapper could not reach the sandboxed daemon; the same pinned compiler and locked dependencies built successfully offline with Xcode clang, then the captures ran against the host Metal adapter. These are extraction baselines, not D3D11 parity or benchmark evidence.

- [x] Build current source and capture tube/cube manifests, image planes and primary/shadow status data.
- [x] Compare post-extraction outputs against these captures on the same adapter/settings: all 18 image/map/uniform/depth payloads were byte-identical for tube and cube; trace counts and uniform hashes matched.

### Task 2: Extract the P8 library API — complete

Audit dependencies and split library-facing behavior from viewer, CLI, provenance shell commands and capture tooling. Make default features library-only; gate viewer and CLI dependencies behind explicit features and update repository commands. Define versioned asset/cache types, validation errors, GPU context creation or injection, renderer lifecycle, pose update, output planes and status enums. Preserve existing public low-level helpers where that avoids needless breaking changes.

- [x] Default feature set is empty; `winit`, `egui`, `clap`, `pollster`, capture and probe are opt-in.
- [x] Add the `DashrSession` API, `AssetDocument` schema v1, explicit trace status records and cache-keyed `BakedMapCache` schema v1.
- [x] Keep old viewer/capture behavior and all full-float layouts; full check, native GPU suite and release build passed.
- [x] Document lifecycle, error behavior, supported input limits and the pre-1.0 semantic-versioning policy.

### Task 3: Prove independent consumption — complete

Add a separate manifest with its own lockfile and a small consumer that imports the crate through its public API. It must validate/bake an asset, update animation time, render a deterministic fixture, and inspect primary and shadow statuses without a window or copied implementation code. Document creation, reuse, teardown, limits and errors.

- [x] Separate consumer manifest and lockfile build with `default-features = false`.
- [x] Consumer rendered on Apple M1 Max / Metal, reporting 666 primary hits, 146 shadow hits and 80 seam edges.
- [x] Cache creation then cache reuse produced identical color checksum `2902f326d220aecb`.

### Task 4: Implement U1-U6 reliability and authoring diagnostics

- U1: detect and report unstable/high-distortion poles with face/chart identifiers.
- U2: refine same-chart surface hits and seam-resolved teleport hits with bounded binary search.
- U3: use teleport distance to shrink steps near seam edges.
- U4: detect large changes in successive surface mappings and retry with a smaller step.
- U5: diagnose folds, overlapping charts and self-intersection tunnels; record the isolated mapping research result.
- U6: replace eye-tuned tolerances with named, unit-bearing, scale/thickness-derived settings and include resolved values in manifests.

Keep primary and shadow budgets and status data separate. Record accuracy and traversal work for each change.

- [x] U1 initial diagnostic: caller-configured metric condition threshold returns zero-based face ID, deterministic edge-connected chart component, condition number and maximum metric scale. It preserves tangent values and does not hide poles with a step cap.
- [x] U1 640×480 CubePinched seam-cut capture generated from the checked-in public-library probe; primary status/hit maps match and no budget/invalid exits are added.
- [x] U1 TubePinched face-277 two-ring seam-cut probe validates with zero atlas texel-center overlap, but is a no-op: the hotspot remains at 67 steps/67 teleports and the frame is unchanged.
- [x] U1 checked-in face-261 alternative reduces the 640×480 TubePinched hotspot from 30 steps/23 teleports to 22/5 with zero atlas texel-center overlap and no new budget/invalid exits; it changes 15 primary and 83 shadow status pixels and has maximum RGB error `0.417415`.
- [x] U1 tested a one-face face-261 patch and the nearest free shift for the two-face patch; neither improved the candidate (30/23 steps/teleports and 24/24 respectively, with 12/40 and 64/407 primary/shadow status changes).
- [x] U1 three-face face-261 expansion also reduces the hotspot to 22/5 with zero atlas overlap and no new budget/invalid exits; it changes 33 primary and 64 shadow status pixels and has maximum RGB error `0.395823`.
- [x] U1 four-face face-261 expansion also returns 22/5, but changes 33 primary and 72 shadow status pixels and has maximum RGB error `0.492667`; it does not improve on the three-face candidate.
- [ ] Human review the 640×480 CubePinched pair and both two-face/three-face TubePinched face-261 pairs for height/albedo continuity, silhouette changes and visible pole reduction. If neither TubePinched edit is acceptable, a different remedy remains open.
- [x] U2-U4 initial opt-in paths: same-chart/teleport boundary refinement, SDF-guided seam stepping and bounded transform-change retries; analytic GPU and cube-fixture gates pass on Apple M1 Max / Metal.
- [x] U2-U4 repeated combined bent-tube probe across four poses: no new budget/invalid exits, but 5-13 primary and 14-83 shadow status differences by pose; the serialized time-8 trace median was 1.844334 ms reference vs 8.100208 ms combined.
- [x] Isolate U2/U3/U4 on the four-pose bent tube; record per-option hit/status/color deltas and warmed time-8 medians (reference/U2/U3/U4: 1.400125/3.67375/2.215125/3.275291 ms).
- [ ] Compare U2/U3/U4 against independent nonlinear height truth and review images before any promotion.
- [x] U5 initial CPU diagnostic reports posed non-coplanar triangle crossings with face/chart IDs.
- [x] U5 now also reports face orientation flips/collapses with face/chart IDs.
- [x] Detect positive-area coplanar overlap when triangle pairs share a geometric vertex or edge; ordinary point/edge contact remains accepted and non-coplanar crossings are still checked.
- [x] Align chart-split vertices already within positional epsilon before non-coplanar tests; the TubePinched rest seam regression passes while genuine overlap and animated-tunnel cases remain detectable.
- [ ] Detect global surface-map folds, evaluate a better-behaved mapping on bent fixtures and check continuous pose intervals.
- [x] U6 initial opt-in policy derives UV delta and object-space shadow bias, with resolved values serialized into capture manifests; one matched cube capture recorded 30 shadow-mask changes and no invalid/budget exits.
- [x] U6 five-bucket M1 Max / Metal GPU sweep kept primary statuses but changed 1-50 shadow pixels and up to `0.5650` RGB; no invalid/budget exits were added.
- [ ] Independent visual review and broader bent/seam fixture coverage remain before promotion.

### Task 5: Evaluate U7-U10 and U15 shader/maths alternatives

Add individually selectable variants for stored inverse bases (U7), race-free compute edgefill (U8), mesh fins (U9), edgefill indirection in the trace loop (U10), and specialized affine inverses (U15). Compare compiler output and benchmark each variant against the same fixtures. Keep only variants that meet the documented quality gate and show a repeatable benefit; otherwise document the reproducer and no-go result.

- [x] U15 initial 3x3 cofactor inverse matches the double-precision CPU oracle and analytic flat GPU oracle; a one-shot cube capture preserved status counts/masks within float precision.
- [x] Repeat U15's paired 12-sample cube trace on M1 Max / Metal: status/RGB gates passed and the serialized run measured 0.864 ms general / 0.860667 ms cofactor; this single-adapter delta is too small to establish a benefit.
- [x] U15 five-run cube result is below the 10% target-cost threshold (trace median 0.82% lower; total GPU median 0.21% slower); the user approved no-go for default promotion on 2026-10-03, so the cofactor option remains opt-in.
- [ ] Windows/FXC compilation and another adapter remain unverified; revisit these only if default promotion is reconsidered.
- [x] U7 stored-inverse atlas variant was measured once on M1 Max / Metal: cube statuses matched, maximum RGB error was `0.0036595762`, and the additional three full-float planes plus pass make it a no-go for promotion without contrary repeated evidence. The reference path stays default.
- [x] U8 full-atlas compute edgefill matches raster output on cube and pinched bent-tube poses; the serialized cube comparison measured 0.079875 ms raster / 0.014625 ms compute. Its region-only schedule is a no-go until a reachable-UV bound exists; bent-fixture timing/another adapter remain open.
- [x] U9 no-go decision: the pinned portable wgpu graphics path has no geometry stage; CPU-expanded fins need schema-level seam adjacency and a matched benefit over U8 before implementation.
- [x] U10 trace indirection probe preserves cube hit/status values but changes RGB by `0.0037953258`; the serialized run is slower in trace, so it is not promoted.

### Task 6: Evaluate U11-U14 teleport and compact-storage alternatives

Prototype two-channel SDFs (U11), separate distance/destination resources (U12), multiple discontinuity channels with filtered destinations (U13), and compact/hybrid formats (U14). Preserve nearest sampling for any discontinuous destination that cannot be proven safe to filter. Record seam, conditioning, storage and timing results; retain full-float baseline fallback.

- [x] U12 split R32F distance / RG32F nearest destination path reproduces the packed map and cube output exactly, and lowers seam-map storage from 16 to 12 bytes per texel; the serialized trace was slightly slower.
- [x] U14 RGBA16F raw/warp atlas halves dynamic atlas storage but changes the cube map/RGB by `0.015960693` / `0.0025811195` and a pinched bent tube by `0.03227496` / `0.3348779`, with 4 primary and 10 shadow status differences.
- [x] U11 no-go: `Maps::teleport` stores one signed distance/destination per texel, with no defined two-distance corner combination or deterministic destination choice.
- [x] U13 no-go: filtered destinations require seam-group IDs to avoid mixing discontinuous UVs; those IDs and a versioned asset contract are absent, and nearest destination loads remain a library invariant.

### Task 7: Complete U16-U18 library/topology contracts

- U16: validate supported topology, reject T-junctions, interpenetration, self-intersection and arbitrary triangle soup with stable element identifiers. Implement the scoped lone-edge/thin-sheet topology extension only where its surface-space contract is well-defined; isolate open-edge cases that need a separate model.
- U17: add conservative acceleration structures and host-facing local-lighting/depth outputs with bending, gutter and teleport safety; leave AO/SSS and long-range scene visibility to the host.
- U18: document the supported input and device contract, version policy, thread/device ownership and stable Rust lifecycle API.

- [x] U17 host output boundary: actual hit depth and separate primary/local-shadow statuses are in `FrameOutput`; min/max/horizon acceleration is a no-go until its bounds are proved safe across deformation, gutters and teleports, with AO/SSS/long-range visibility owned by the host.

- [x] U16 initial rejection path reports T-junction vertex/edge face IDs and posed non-coplanar self-intersection face/chart IDs before an imported asset renders.
- [x] U18 P8 headless Rust API and independent consumer are accepted locally; the opt-in P9 C ABI preview compiles and passes a native C create/update/render/status-read/destroy smoke run.
- [x] U16 detects non-adjacent coplanar triangle overlap with face/chart IDs.
- [x] Shared-vertex and shared-edge coplanar overlap diagnostics report face/chart IDs without rejecting ordinary contact.
- [x] Public topology preflight rejects an unpaired triangle-soup edge with stable face/edge IDs before map baking.
- [x] Public topology preflight rejects an indexed edge shared by three faces, reporting the edge and incident-face count.
- [ ] Lone-edge/thin-sheet support remains a P13 scope proposal pending approval; schema v1 has no boundary/cliff-edge surface contract.

### Task 8: Complete U19-U20 as isolated research

Build small feasibility probes for non-heightfield representations (SDF, fractal, splat, voxel) and GPU space-warped animation. Record inputs, method, hardware/shader support, measured result, limitations and a go/no-go recommendation. Do not add experimental representations to the production library API without separate evidence and a revised contract.

- [x] Research matrix written with isolated go/no-go recommendations, correctness/performance metrics and promotion gates: [U19-U20 research](../../research/2026-10-03/non-heightfield-warps-u19-u20.md).
- [x] U19 bounded research closeout: SDF, finite Menger, Gaussian adapter and dense voxel probes have individual decisions; no general representation is promoted to the production surface API.
- [x] Analytic SDF plus bounded GPU warp probes run on M1 Max / Metal: sinusoidal shear and smooth identity/one-bone-inverse blend each match 64 CPU root statuses with zero sampled step-bound violations. The blend has positive minimum sampled Jacobian determinant `0.109991`.
- [x] Spatially varying two-bone inverse-LBS probe runs a 120-pose sequence and a 1,488-triangle forward-skinned mesh baseline. All 3,840 statuses matched, mesh status disagreement was zero, maximum analytic-to-mesh depth difference was `0.0237153`, and conservative step violations were zero. A near-contact stress pose returned explicit invalid for all 32 rays when its inverse bound became negative.
- [x] A separate `π`-radian U20 stress pose produces `341` intersecting triangle pairs; endpoint clearance is `-0.5`, the inverse lower bound is `-2.65`, and the GPU rejects all 32 rays before inversion.
- [x] Dense voxel SDF CPU probe measures 16³/32³/64³ interpolation error; a 32³ two-bone GPU voxel warp matches CPU trilinear roots with zero sampled step violations.
- [x] Matched 64³ inverse-LBS voxel GPU follow-up compares with the 32³ field on 3,840 rays; both resolutions match CPU voxel/analytic statuses with zero step violations, while 64³ lowers max analytic hit error from `0.016367` to `0.003553` at 8× payload. GPU timing is variable and no speed claim is made.
- [x] U20 bounded native GPU-space-warp closeout: inverse-LBS SDF and 32³/64³ voxel probes pass CPU-root/status and conservative-step gates; the π-radian actual-intersection case returns `Invalid` for all rays. Production exposure remains a no-go pending full-frame/normal/temporal review and an external integration contract.
- [x] The U20 `π`-radian stress pose now creates `341` intersecting triangle pairs in the 1,488-triangle mesh; its certified inverse lower bound is `-2.65` and all 32 rays return `Invalid` before inversion.
- [x] Finite depth-2 Menger GPU probe evaluates 400 box SDFs against exact CPU ray-box roots; all 256 statuses match with zero conservative-step violations. Its p95 is `7.750834 ms` for 256 rays, so the flat scan is a correctness probe rather than a viable runtime path.
- [x] Finite Menger median-split BVH uses exact leaf box SDFs and node AABB lower bounds; CPU distances and all 1,024 M1 Max / Metal ray statuses match their flat/exact references with zero step violations or stack overflows. On the matched 32×32 workload, the current stored aggregate reports median dispatch `3.875417` ms flat and `1.314709` ms hierarchical, while an earlier run measured `3.923125/1.320459 ms`; box SDF evaluations fell from 400 to `6.009` per distance query. These single-run timings vary and do not promote a speed claim. Ten CPU builds had median/p95 `0.030041/0.032042` ms; other fractal families remain open.
- [x] Gaussian adapter prototype skins anisotropic covariance/centers on GPU and composites 256 primitives over two poses. CPU/GPU output error stays below `1e-5`; compare against a skinned mesh yields silhouette IoU `0.541` and remains a visible quality gap.
- [ ] U19 promotion gates remain open: broader fractal families, authored splat assets, full-frame visual review and production adapters.
- [ ] U20 promotion gates remain open: full-frame silhouette/normal/temporal review and external GPU-warp integration. Its isolated probes remain outside the production library API.

### Task 9: Reconcile roadmap and validation records

For every U ID, update the upstream register with implemented capability or the exact no-go/research outcome and link supporting reports. Update P8 status only to the evidence actually reached. Preserve open platform, engine, benchmark and human-review gates.

- [x] The upstream register now has a capability, measured experiment or explicit fork decision for every U1-U20 item; validation links retain open quality/platform/human gates.
- [x] P8 remains locally accepted and P9 is recorded only as a C ABI preview; no engine-owned resource or translated shader claim is made.
- [x] Optional C ABI v1 builds as a headless cdylib; layout/error tests and a native C create/update/render/read/destroy smoke client pass on Metal.
- [x] Per-thread C log callback reports ABI errors and session lifecycle events; Rust callback regression and native C consumer compile/run pass.
- [x] Naga 30.0.1 translates the production WGSL passes to HLSL and MSL in the shader test.
- [x] P9 checked-in C header is generated from the Rust FFI declarations with cbindgen 0.29.4 and checked by `scripts/ffi-header --check`.
- [ ] P9 target compiler validation, engine-owned GPU resources, and Unity/Unreal scene parity remain open.

## Validation Contract

Use named regression cases and the repo's ordinary formatting/test/clippy/build commands for code changes. Run real-GPU comparisons and the benchmark protocol for rendering/storage/traversal claims. If a device, Windows/D3D11 executor, engine installation or independent visual reviewer is unavailable, leave that gate explicitly open and record the blocker; do not infer acceptance from compilation or CPU fixtures.
