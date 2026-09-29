# DASHR fork development roadmap

## Recommended direction

Build a native Rust/wgpu experimental viewer with Blender as the initial authoring/export front end. Preserve the D3D11 demo and paper as the reference. First test full-float resource support and correctness; then optimize storage and traversal. Keep a native C++/Metal route available if a measured wgpu limitation blocks the selected Mac target. Do not maintain two complete renderer ports before that evidence exists.

This is a proposal dated 2026-09-29; no phase is implemented. It follows the [source assessment](../research/2026-09-29/repository-assessment.md), [optimization workstreams](../research/2026-09-29/optimization-workstreams.md), [platform research](../research/2026-09-29/platform-feasibility.md) and [benchmark gates](../research/2026-09-29/benchmark-protocol.md).

## Decision matrix

| Route | Best use | Main cost | Recommendation |
| --- | --- | --- | --- |
| Optimize D3D11 in place | Lowest-risk algorithm experiments and Windows baseline | Still platform-specific; build needs repair | Retain reference, add capture/profiling first |
| C++/Metal | A macOS-only viewer with direct Apple profiling/control | Platform shell rewrite, MSL translation, second backend | Conditional alternative |
| Rust/wgpu/WGSL | Shared Mac/Windows/Linux experimental renderer | Explicit format/limit handling and shader semantics | Preferred prototype after capability probe |
| Blender exporter + external viewer | Real authored meshes without engine surgery | Unique atlas, basis/rig/topology contract | First Blender milestone |
| Blender custom RenderEngine | DASHR final renders/viewport in Blender | Native renderer lifetime, copies, synchronization, materials | Optional after external viewer passes |
| Cycles/EEVEE source integration | Native engine-wide visibility and shading | Renderer internals, scene traversal, maintenance | Separate long-term workstream |
| Browser WebGPU | Shareable demo | Browser limits, readback, packaging and feature variance | Defer until native portable path is proved |

Effort classes: P0–P2 are bounded setup/probe work; P3–P4 are substantial renderer translation; P5 is an importer/exporter integration; P6 is experimental optimization; P7 is a separate product/engine effort. Calendar estimates would be speculative before baseline and capability probes.

## P0 — freeze provenance and restore reproducibility

Prerequisites: the source snapshot and a Windows D3D11 machine for reference capture. Work: record upstream/source revision, embedded software license and dependency/asset provenance; pin ImGui/stb versions; repair project include paths and launch working directory; document an exact clean Windows build. Preserve original shaders and parameters in a reference mode.

Deliverables: build instructions, dependency manifest, fixed procedural scenes and deterministic parameter dumps. Acceptance: clean build and launch, all four procedural mesh modes captured, explicit shader errors and missing assets reported. Stop: unresolved dependencies or no Windows executor. Analytic work may continue, but do not call the baseline reproduced. Do not copy the compiled executable as evidence that source builds.

## P1 — establish math, image and timing contracts

Depends on P0 for D3D parity images, but analytic fixtures can start independently. Work: export exact camera/pose/settings, intermediate atlas planes, hit UV/object position/depth and step/teleport status. Add asynchronous GPU timing and separate bake timing. Specify reverse-Z projection, UV orientation, winding, sampler addressing, skinning convention and explicit host/shader offsets.

Deliverables: fixture manifest, CPU math oracle, reference captures and benchmark harness. Acceptance: the [protocol](../research/2026-09-29/benchmark-protocol.md) is reproducible and the reference's known failure cases are labeled. No speed claims from GUI FPS alone.

## P2 — decide the portable resource path on real hardware

Depends on P1's representation contract. Work: enumerate wgpu adapter features, limits and format usages on an Apple Silicon Mac and a Windows target. Pin a released dependency set. Exercise four full-float MRTs, filtered transform reads, nearest teleport reads, uniform round trips and offscreen readback.

Branch choices:

- If at least 64 color-attachment bytes/sample and FLOAT32_FILTERABLE are available, request them explicitly and preserve RGBA32F parity initially.
- If float filtering is absent, use tested manual bilinear loads with the source sampler address semantics.
- If the MRT budget is inadequate, split UV deformation output across passes while preserving coverage/interpolation and compare overhead. Do not switch to half precision just to hide a parity blocker.
- If target limits or translation constraints remain unacceptable, document the failing reproducer and evaluate native Metal against the same fixtures.

Record the adapter's maximum attachment budget as well as the requested value. Exactly 64 is sufficient for the current four RGBA32F outputs; investigate larger budgets only for extensions needing more simultaneous output. See the [attachment-budget note](../research/2026-09-29/wgpu-attachment-budget.md).

Deliverables: capability report, minimal shader/readback reproducer, selected fallback policy. Acceptance: actual device creation, pipelines and numerical round-trip results on the selected adapters. Native success does not establish browser support.

## P3 — port deformation and topology maps faithfully

Depends on P1/P2. Translate procedural assets, custom metric basis generation, four-bone animation, UV rasterization, both distortion encodings and static map construction. Use cached reference-baked maps initially if this isolates GPU translation; explicitly label this as incomplete bake portability. Port baking separately with deterministic seam partners and coverage tests.

Suggested Rust modules are `asset`, `topology`, `math_reference`, `gpu_resources`, `passes`, `capture` and `viewer`. These are proposed boundaries, not existing directories. Keep data preparation independent of the UI and device so an exporter or headless harness can share it. GPU resources own their lifetimes and publish deliberate read/write usages.

Acceptance: atlas-plane comparisons, seam sign/destination tests and bending sequence pass. Missing position correction, normalized tangents or stale pose maps block this phase. Do not combine compression or a better raymarcher with initial translation.

## P4 — port tracing, lighting and scene depth

Depends on P3. Translate object-space stepping, surface-position iteration, distortion damping, teleport logic, escape bounds, interpolation and existing lighting. Keep primary and shadow status distinct. Define WGSL sampling explicitly, especially material samples reached by divergent loops. Compile a diagnostic mode and retain reference parameters.

First gate: reference-compatible single-asset color/UV images with full-float data. Second gate: production termination states and actual-hit depth, plus two-asset occlusion. Actual hit-depth output is an intentional feature change and must be compared against an independent reference, not only the demo's shell depth. Near-plane and camera-inside-shell cases need a policy before claiming scene readiness.

Deliverables: Mac and Windows captures, validation logs, shader/layout fixtures and reviewed animation comparisons. Acceptance: protocol gates pass on actual devices; local analytic tests alone cannot promote the port. No unsupported claims about long-range shadows or Cycles integration.

## P5 — Blender authoring and export

Can define the schema after P1; full round-trip depends on P4. Build an exporter for a triangulated smooth closed mesh with a unique atlas, displacement/normal/albedo inputs, thickness, pose and seam topology. Separate material UVs only in a later schema revision. Start with static/dependency-graph-evaluated frames; a general indexed joint palette is a distinct extension over the fixed four-bone demo.

Deliverables: versioned manifest and binary payload schema, validator, an authored test scene and paired Blender/viewer capture. Acceptance: repeatable export/import, diagnostics on unsupported topology, physically meaningful tangent lengths, matching animation and a reviewed seam/silhouette sequence. Keep material displacement settings, units and color handling explicit. See [Blender plan](blender-authoring-plan.md).

## P6 — optimize using measured bottlenecks

Depends on P4 baseline. Run O2–O7 as individually switchable experiments. Begin with static map caching, RG edgefill, split teleport channels, mode specialization and inverse simplification; then test half transform storage, dynamic update skipping, traversal refinement and conservative bounds. Compute edgefill is optional and uses separate source/destination maps initially.

Deliverables: one experiment report per change with timing distributions, allocations and error gates. Acceptance: measured benefit and unchanged agreed quality budget. Stop or revert when a faster variant creates seam holes, unstable poles, hidden budget exits or increased missed detail. Do not merge performance and visual claims into a single FPS number.

## P7 — optional Blender renderer and deeper integration

Depends on P5 and a reliable native library. First prototype a custom RenderEngine using an external renderer process and CPU image transfer. This makes lifetime and cancellation behavior inspectable before attempting GPU texture sharing. Measure interactive latency and dirty-state handling. A library binding or GPU bridge is justified only after the process prototype shows where the copies matter.

Define supported Blender objects, materials, lights and output passes explicitly. A DASHR custom engine is not automatically Cycles/EEVEE compatibility. Direct engine integration additionally needs scene intersections, shadow/reflection visibility, BSDF/material support, acceleration structure updates and platform kernel integration. Reassess that effort only after a useful authored example exists.

## Copy-ready first implementation task

> Restore a reproducible reference build for the DASHR fork at the recorded v1.0 snapshot. Preserve Tom Forsyth's paper, attribution and reference shader behavior. Pin and document missing dependencies; reconcile Visual Studio paths without a renderer rewrite. Add deterministic settings and captures for the procedural tube and cube, with all parameters and source/asset hashes. Record Windows build and render evidence when hardware is available, and explicitly mark unavailable execution as a blocked evidence gate. Do not claim performance improvements, change height decoding, compress transforms, implement Blender integration or publish packages during this phase. Deliver build docs, fixture manifests and a concise report identifying exactly what was executed and what remains unverified.

Future phase tasks should copy their prerequisites, deliverables, acceptance and stop conditions from this plan, and attach the protocol. No port implementation, commits, pushes, PRs or publication are included in the present research request.
