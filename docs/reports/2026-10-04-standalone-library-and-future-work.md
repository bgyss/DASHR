# DASHR standalone library and upstream future-work report

Report date: **2026-10-04**. Branch: `codex/dashr-standalone-library`. This report covers the work following baseline commit `fe215d3`, including library extraction, authoring diagnostics, traversal/storage experiments, native feasibility research, and the latest seam-cut captures. It records the current state of the work; the branch commit history identifies the exact published source snapshot.

## Outcome and completion boundary

The Rust implementation can now be consumed as a standalone headless library. The default dependency graph excludes the viewer, UI and CLI dependencies. An independent Rust application validates assets, updates poses, renders frames, reuses baked maps and reads primary/shadow diagnostics through the public API. An optional C ABI preview has a generated header and a native C consumer.

All 20 upstream future-work items have been investigated or implemented to a documented scope. The current register records **13 scoped closures and seven open items**. Closures include numerical capabilities, approved or accepted-plan no-go decisions, and bounded research results. They do not mean that all 20 requested outcomes are finished or that the renderer is ready for general engine use. U1 visual review and the U5/U7/U8/U10/U14/U16 disposition decisions remain open. The overall execution goal remains active.

Local library extraction is accepted on Apple M1 Max / Metal. Engine integration, Windows Rust rendering, broader authoring support and several visual-quality gates remain unaccepted. No package release or social announcement has been published by this work.

## Attribution and source authority

DASHR—Dynamically Animated Skinned Heightfield Rendering—is Tom Forsyth's technique. His [paper](../../paper/DASHR_Paper.html) and [original repository](https://github.com/tomforsyth1000/DASHR) are the algorithm authority. The preserved Windows/D3D11 demonstration and paper remain in `demo/` and `paper/`. This fork's contributions are the Rust/wgpu port, library interface, diagnostics, experimental variants, research probes and evidence records. See [provenance](../port/provenance.md).

The implementation preserves the core pipeline: UV-space deformation, gutter/edgefill and distortion correction, discontinuous seam transport, and heightfield tracing. Metric tangent lengths, full-float reference storage, host/shader layouts, filtered seam distance and nearest destination loads remain explicit contracts.

## Standalone library architecture

The `dashr` package has an empty default feature set. `viewer` and `tooling` enable application-facing dependencies; `ffi` enables the C interface. CPU asset preparation and validation remain callable without constructing a GPU device. GPU resources and passes, capture tooling and window/UI state are separate modules.

The public API provides:

- `DashrSession` creation from procedural settings or an `AssetDocument`, with a transferred wgpu context supported through `with_context`.
- Pose updates, target resizing, rendering and explicit session teardown.
- `FrameOutput` color and actual hit UV/distance/reverse-Z depth, plus separate primary and local-shadow diagnostics.
- Named `NotLaunched`, `Hit`, `Escaped`, `BudgetExhausted`, `InvalidBasis` and `DebugForcedHit` states. Budget exhaustion and invalid bases are preserved as unknown/failure states.
- `AssetDocument` schema v1 and `BakedMapCache` schema v1. Cache keys cover the mesh document, atlas dimensions and exact raster occupancy; corrupt or incompatible caches return errors.
- Documented pre-1.0 Rust versioning, resource ownership, device/thread constraints and lifecycle errors.

The current mesh representation retains the reference four-bone layout. It is not a general rig/material importer. Material roots and settings remain separate from the mesh document. Ownership of a transferred wgpu context does not establish interoperability with engine-owned render targets or an engine render graph.

The optional C ABI v1 uses opaque handles, explicit structure sizes, borrowed frame views, per-thread errors and synchronous logging callbacks. Its session owns a wgpu device and exposes CPU frame planes. Borrowed-frame lifetime and FFI thread serialization are documented. The [API guide](../port/library-api.md), [Rust consumer](../../examples/standalone-consumer/src/main.rs), [C consumer](../../examples/c-consumer/main.c) and [header](../../include/dashr.h) provide the concrete entry points.

## Library acceptance evidence

The [validation record](../port/validation.md#p8-standalone-library-extraction) records the extraction gates:

| Check | Recorded result | Limit of the evidence |
| --- | --- | --- |
| Default library dependency graph | No `winit`, `egui`, `clap` or `pollster` in normal dependencies | Optional application features still use them |
| Independent locked Rust consumer | 666 primary hits, 146 shadow hits, 80 seam edges on M1 Max / Metal | One bounded reference fixture |
| Cache creation and reuse | Identical color checksum `2902f326d220aecb` | Does not establish cache portability across bake contracts |
| Pre/post extraction tube and cube | All 18 image/map/uniform/depth payloads per fixture were byte-identical | Same adapter and fixed settings; timing excluded |
| External torus document | Two fresh processes returned checksum `68f61da32d984f15` | Independently authored JSON, not a complete material package |
| C ABI smoke client | ABI v1, 64×64 frame, 530 primary hits and lifecycle callbacks | Native C consumption, not engine integration |
| Naga shader translation | Production WGSL stages accepted for HLSL/MSL generation | Target engine compiler and render-graph execution remain open |

Extraction comparison settings were 320×240 output, atlas 256, material set 0, time 10, on the same Apple M1 Max / Metal adapter. These comparisons preserve behavior of the tested baseline; they are separate from geometric truth and D3D11 parity.

## Item-by-item roadmap status

The [upstream register](../plans/upstream-future-work.md) remains authoritative as work continues. The table below is this report's dated snapshot.

| Item | Work and finding | Current disposition and remaining gate |
| --- | --- | --- |
| U1: tornado/pole mitigation | Runtime UV hotspots map through edgefill to face/chart cut suggestions. CubePinched seam reauthoring reduces a 640×480 hotspot from 360 steps/302 teleports to 65/5. TubePinched face-261 candidates reach 22/5 from 30/23 | **Open:** human review of CubePinched and both TubePinched candidates; numerical improvement does not accept seam appearance |
| U2: hit/teleport binary search | Bounded 12-step refinements pass independent f64 nonlinear-height and bilinear-seam root oracles; sampled seam error falls from `0.225399` to `0.000036631` | **Closed for native numerical scope; opt-in.** Bent-fixture image review and Windows execution remain open; time-8 primary-only median rises from `0.869542` to `3.278125 ms` |
| U3: seam-distance step shrinking | Four-texel predicted-step policy improves CPU seam-root error 13.7×/15.1× under manual/hardware sampling | **Closed for native numerical scope; opt-in.** Image/status changes and cost keep default promotion open |
| U4: transform-change retry | CPU-checked steep affine fixture triggers change above 0.25; four retries shrink the step about 4× and bring the accepted change below threshold | **Closed for native retry scope; opt-in.** Broader visual/Windows evidence remains open |
| U5: self-intersection tunnels | Posed crossings, orientation flips/collapses and coplanar overlaps are diagnosed. Shared seam contacts are accepted without concealing real overlaps | **Open:** proposed no-go for a general mapping/look-ahead rewrite awaits approval; global map folds and continuous collision intervals remain research |
| U6: derived epsilons | Unit-bearing UV normal offsets and object-space shadow bias are derived and serialized. Five scale/thickness buckets preserve primary statuses | **Closed for native derivation scope; opt-in.** Up to 50 shadow changes and max RGB `0.565049` require visual review |
| U7: stored inverse atlas | Five-run trace median improves only 0.8%; total GPU median/p95 regress 4.35%/8.60%; atlas payload rises 75% | **Open:** no-go for default promotion proposed, approval pending; reproducer retained |
| U8: compute edgefill | Full-atlas race-free compute matches raster within `4.76837e-7`; bent fixture has identical statuses/RGB. Five-run edgefill median falls 76.4%, total GPU median 8.9% | **Open:** approve or reject deferral of selective writes; no conservative reachable-UV write mask exists. Other adapters remain open |
| U9: mesh fins | Portable classic wgpu rendering lacks a geometry stage; CPU-expanded fins need authoring adjacency and a matched benefit over U8 | **Closed no-go for current portable library**; schema and benefit evidence are prerequisites to revisit |
| U10: trace-time edgefill indirection | Cube statuses match, max RGB `0.003795326`; five-run trace median is 14.1% slower and total GPU median 4.9% slower | **Open:** no-go for default promotion proposed, approval pending; curved/seam fixtures unmeasured |
| U11: two-channel corner SDF | Current map has one signed distance/destination owner and no defined corner combination or tie rule | **Closed no-go for initial library**; requires analytic corner fixture and representation contract |
| U12: split distance/destination | R32F distance plus RG32F destination exactly reproduce packed maps/frame and reduce seam-map payload from 16 to 12 bytes/texel | **Closed for native storage scope; opt-in memory tradeoff.** Five-run total GPU median is neutral; curved/other-adapter review remains open |
| U13: multiple filtered seam textures | Stable seam-group IDs are absent; destination filtering could mix discontinuous UV coordinates | **Closed no-go for initial library**; versioned seam schema required |
| U14: compact storage | RGBA16F halves raw/warp payload but bent TubePinched changes four primary and ten shadow statuses, max RGB `0.3348779` | **Open:** no-go for default promotion proposed, approval pending; vertices/seam maps retain full precision |
| U15: specialized inverse | Cofactor inverse passes CPU/flat-GPU oracles; five-run trace gain 0.82%, total GPU median 0.21% slower; max RGB `5.96046e-8` | **Closed no-go for default promotion, user approved 2026-10-03.** Opt-in reproducer retained; Windows/FXC and another adapter unverified |
| U16: arbitrary topology | Validation rejects T-junctions, posed crossings, open edges and edges incident to more than two faces with element IDs | **Open:** native rejection verified; lone-edge/thin-sheet deferral to P13 awaits approval and a surface-space/cliff-edge schema |
| U17: acceleration/lighting | Public outputs expose actual depth and separate local-shadow states. Conservative bounds across deformation/gutters/teleports are unproved | **Closed no-go for production acceleration under accepted plan.** Host AO/SSS/scene lighting and engine integration remain open |
| U18: robust drop-in library | Headless default, independent Rust consumer, versioned formats, lifecycle docs and C ABI smoke | **Closed for native standalone Rust contract.** Broad imports, target compilers and engine-owned resources remain open |
| U19: non-heightfield representations | Isolated SDF, finite Menger, Gaussian and voxel probes establish bounded correctness or documented failure modes | **Closed for bounded feasibility/recommendations.** No production representation promoted |
| U20: GPU space warps | Inverse-LBS analytic/voxel probes use CPU roots, 120 poses and an actual self-intersection stress case | **Closed for bounded native feasibility.** Full-frame normal/silhouette/temporal review and integrations remain open |

## U1 seam-cut findings and human review

Rest metric condition alone misses the CubePinched runtime hotspot, so the diagnostic now uses the runtime UV and captured edgefill map. The remedy duplicates a patch into a separate atlas location and rebakes its material samples. It validates topology and reports overlap rather than masking excessive work with a step cap.

The CubePinched 640×480 pair preserves all primary hit/status pixels, adds no budget/invalid exits and lowers primary maximum steps from 419 to 134. Total primary teleports fall from 36,605 to 25,851. It changes 19 shadow statuses; max/mean RGB error is `0.161114/4.46e-6`. Human seam/material/silhouette review is still pending.

TubePinched's face-277 two-ring cut is a no-op at 67 steps/67 teleports. Follow-up face-261 candidates provide the following tradeoffs on M1 Max / Metal at 640×480, atlas 128, time 8:

| Patch | Hotspot steps/teleports | Primary/shadow status changes | Maximum RGB error |
| --- | ---: | ---: | ---: |
| Baseline | 30/23 | — | — |
| Two faces `[260, 261]` | 22/5 | 15/83 | 0.417415 |
| Three faces `[259, 260, 261]` | 22/5 | 33/64 | 0.395823 |
| Four-face expansion | 22/5 | 33/72 | 0.492667 |
| One-face cut | 30/23 | 12/40 | 0.446441 |
| Nearest free two-face placement | 24/24 | 64/407 | 0.620989 |

All these captures have zero atlas texel-center overlap and no new budget/invalid exits. The one-face, nearest-placement and four-face variants do not improve the measured compromise. Both the two-face and three-face cases remain candidates pending a human judgment about texture continuity, silhouette and visible pole reduction. Pending review is not acceptance of either edit, nor proof of a universal automatic seam remedy.

The checked-in [CubePinched](../../examples/u1_seam_cut_probe.rs) and [TubePinched](../../examples/u1_tube_seam_cut_probe.rs) examples reproduce the candidates. Raw PNG/frame/map/manifests remain under ignored `out/u1-reauthored/` and are not distributed with this report. A fresh clone must generate the prerequisite baseline captures before running these examples; commands and settings are in [validation](../port/validation.md#u1-forced-seam-cubepinched-proof-of-concept-2026-10-03).

## Topology and asset authoring

Validation now distinguishes structural document validity from pose/topology validity. `validate_pose_topology` reports relevant vertex, face and chart identifiers for unsupported cases before map baking. It detects geometric crossings, coplanar positive-area overlap and unstable orientation, while accepting ordinary point/edge contacts.

A regression exposed six false contacts at the TubePinched rest cap/body seams. Matching chart-split vertices already within the positional epsilon are aligned during the non-coplanar intersection check. The regression failed before the correction and passed afterward; genuine overlap and animated-tunnel cases remain detectable. This removes a false rejection, not the renderer's actual folded-pose limitations.

The static Blender exporter writes schema-v1 mesh documents and metric tangent bases and rejects unsupported topology/rig input. A Blender 5.2.2 icosphere export round-trips through the independent Rust consumer. Constant/ramp material captures exist, but visible seam gaps and a missing exported-sphere displaced-mesh oracle keep B1 acceptance open. Hardware-filtered ramp sampling misses the ideal plane gate; a manually filtered probe meets it, without being promoted as the default. There is no Blender render-engine integration yet.

Open boundaries and thin sheets need cliff-edge roles and opposite-side mapping definitions absent from schema v1. The proposed U16/P13 deferral retains explicit rejection until that representation is designed; approval is pending.

## Optimization decisions and evidence limits

Repeated optimization comparisons use five alternating A/B runs, 120 warm-up frames and 300 measured frames per variant per run. The project rule is at least 10% median improvement in target cost, with no more than 5% total GPU median/p95 regression, unless an explicit memory/clarity/portability benefit justifies a smaller gain. These are decision rules, not statistical-significance claims. See the [benchmark protocol](../research/2026-09-29/benchmark-protocol.md).

U8 full-atlas compute is the strongest measured cost result in the tested bent fixture. U12 has a clear 25% seam-map memory reduction with neutral timing. U7, U10, U14 and U15 do not support default promotion on their recorded evidence. All experimental switches preserve access to the reference default. A positive result on one fixture/adapter does not establish portable speedups or full-scene performance.

## Bounded representation and warp research

The [U19/U20 research note](../research/2026-10-03/non-heightfield-warps-u19-u20.md) records inputs, methods, CPU truth, resource costs and promotion gates:

- Analytic SDF/warp probes use independently solved roots and sampled conservative-step checks. Their bounded success is not proof for arbitrary SDF deformation.
- Finite Menger depth 2/3 exactly match CPU ray-box truth. The depth-2 BVH reduces distance-query box evaluations from 400 to about six; timings remain run-specific. Depth 4 has 400 exact hits and four exact misses retained as budget exits, rather than manufactured hits or escapes.
- A 256-Gaussian adapter has exact CPU/GPU depth-sort order and close compositing agreement, but silhouette IoU is only `0.541` against the matched mesh. Opacity-weighted depth is not a unique surface intersection; larger authored splats and production sorting remain open.
- Inverse-LBS analytic and dense-voxel tests cover 120 poses/3,840 rays with zero CPU status disagreements, inverse failures or sampled conservative-step violations. At eight times the payload, 64³ voxels reduce maximum analytic hit error from `0.016367` to `0.003553`.
- At a π-radian actual self-intersection pose, the forward mesh has 341 intersecting triangle pairs and the certified inverse lower bound is negative. All 32 stress rays return `Invalid` before inversion, demonstrating fail-closed behavior rather than acceptable folded geometry.

These probes stay under tooling/research boundaries. General fields, authored assets, full-frame normals/silhouettes, temporal coherence and production adapters require separate evidence before exposure through the surface library API.

## Platform and integration gates

One supplied Windows D3D11 snapshot and exact Mac replay have about 99.98% foreground overlap. That is bounded comparison evidence. Windows Rust DX12 execution remains affected by recorded device loss in the VM; cross-build and pipeline compilation are insufficient to close rendering acceptance. Linux and browser execution are unaccepted.

Unity/Unreal engine-owned resource sharing, target shader compilers, render-graph integration and scene comparison remain P9 gates. Blender integration, broad rig/material import, inter-object occlusion/shadows and open topology remain later work. The Rust output contract supplies actual depth and local-shadow status, while host engines still own scene-level lighting and effects.

## Verification record and reproduction

Historical checks are recorded in the canonical validation document with their scope. The library extraction had ordinary checks, native GPU tests, release build and independent consumer runs. Later topology fixes had meaningful regressions; the latest seam work used native Metal capture probes and formatting/diff checks. This report does not label historical commands as fresh runs.

Repository commands for a configured native environment are:

```sh
scripts/in-nix mise run setup
scripts/in-nix mise run check
scripts/in-nix mise run gpu-test
scripts/in-nix mise run build
scripts/in-nix mise run ffi-smoke
scripts/in-nix cargo run --locked --manifest-path examples/standalone-consumer/Cargo.toml
```

The checks for publishing this branch are recorded in the publication note below. Native GPU and UI checks require an environment that exposes the adapter. Restricted-host attempts without Metal access did not count as renderer success. Cargo dependencies/toolchain remain pinned; generated build outputs, captures and checkout-local caches are excluded from Git, including nested consumer `target/` directories.

## Remaining closeout work

1. Record human accept/reject for the CubePinched seam and a TubePinched candidate; if rejected, investigate another U1 remedy.
2. Resolve the pending U5/U7/U8/U10/U14 no-go dispositions and U16/P13 topology deferral without treating proposed decisions as approved.
3. Preserve U2/U3/U4/U6/U12 numerical/storage closures while keeping visual/default-promotion and platform gates open.
4. Complete authored-asset truth and seam review before advancing the Blender static exporter milestone.
5. Design and validate target engine ownership, compiler and scene contracts before claiming engine integration.
6. Keep U19/U20 production-promotion requirements separate from their completed bounded research scopes.

The original goal remains to close each upstream item individually against its actual acceptance criteria or an approved no-go. This report and branch publication are a checkpoint in that ongoing goal.

## Publication validation

Fresh checks on 2026-10-04 passed before committing this checkpoint:

- `cargo fmt --all -- --check`.
- `cargo check --locked --offline --lib --no-default-features`.
- `cargo test --locked --offline --features viewer,ffi,tooling`: **61 passed, zero failed, 19 native GPU tests intentionally ignored**.
- `scripts/ffi-header --check`: generated header matches the Rust declarations.
- `cargo clippy --locked --offline --all-targets --features viewer,ffi,tooling -- -D warnings`.
- `git diff --check`, repository-local links in this report/announcement bundle, and personal-path scans of the authored report/announcement/research text.

These runs used pinned Rust 1.95.0, checkout-local Cargo dependencies and the native Xcode linker. The restricted environment could not access the Nix daemon, so the already-configured toolchain was used directly, with the wrapper bypassed for the header check. The full native GPU suite, release build, C smoke and visual review were not rerun for this publication checkpoint; their earlier evidence and outstanding gates remain as described above.

The user requested a remote branch push. The report, implementation, consumers and evidence documents are published together so the branch contains the capability described by the report. Generated build/capture artifacts remain excluded. Announcement text remains a draft for the user to post.
