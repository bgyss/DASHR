# Standalone library and engine/DCC integration plan

Goal: evolve the native Rust/wgpu port from an experimental single-asset viewer into an embeddable DASHR library that Unreal Engine, Unity and Blender integrations can use. This fork takes up Tom Forsyth's invitation to make a real library; upstream remains a research paper and demo and does not plan this itself. Upstream's points are tracked in [upstream future work](upstream-future-work.md); this page covers architecture, integration routes and phases. The earlier [roadmap](dashr-porting-roadmap.md) retains P0-P7 evidence gates; P8-P13 extend it. P8 extraction is the highest development priority and does not depend on completing P5-P7.

The existing Rust crate was a starting point, not proof of a standalone library. As of 2026-10-03, P8 is implemented locally: the `dashr` package exposes a headless default library and its viewer/tooling dependencies are opt-in. The P8 extraction gates passed on Apple M1 Max / Metal. An opt-in P9 C ABI preview now builds and runs a native C client, while engine-owned resources, shader translation and engine scenes remain unverified. D3D11 parity and P10-P13 remain open. Effort and benefit statements are hypotheses; each phase lists evidence required to call it done.

## Principles

- **Core is host-agnostic.** The default library feature set excludes windowing, UI, file-dialog, CLI and capture dependencies. The existing viewer, capture and probe clients are feature-gated and use the same library renderer. `asset` and `topology` remain device-independent until GPU occupancy is required for the reference bake.
- **The host owns the frame.** Engines have their own cameras, depth, lighting, materials, shadows and resource lifetimes. DASHR produces deformation maps, a traced local surface result (UV, object position, normal/tangent frame, local shadow term, hit depth, status) and lets the host shade it. A built-in viewer shader is a reference client, not the product.
- **Explicit contracts over convenience.** Matrix convention, reverse-Z, UV orientation, height decode, units and sampler semantics are part of a versioned ABI and asset schema, not engine defaults.
- **Reference stays the authority.** Library output must match the D3D11/paper reference on the fixtures before optimizations are promoted.
- **Diagnostics are data.** Termination states (hit, miss, escape, budget, invalid basis, repeated teleport) are returned to the host, never hidden. Importer diagnostics report high-condition faces and posed non-coplanar intersections with face/chart identifiers.

## Target architecture

| Layer | Contents | Notes |
| --- | --- | --- |
| `dashr` Rust library | Asset validation, topology/seam bake and cache, math reference, deformation/trace passes, status codes | Default features are headless; viewer/tooling are opt-in. Deterministic CPU oracle retained for tests |
| Backend abstraction | wgpu implementation first; trait boundary for host-provided GPU resources | Engines usually require rendering on *their* device (D3D11/D3D12/Vulkan/Metal), so wgpu-owned devices alone are not enough; see Integration modes |
| `dashr-capi` | Stable C ABI (`cbindgen`), opaque handles, versioned structs, error codes, no panics across the boundary | Consumed by Unity native plugin, Unreal module and Blender binding |
| Shader package | WGSL source of truth; generated/validated HLSL, GLSL/SPIR-V and MSL variants via Naga or an equivalent translator | Every variant passes the same fixtures; specialization per mode (primary only, primary+shadow) |
| Asset tooling | Exporter schemas, `dashr validate`, `dashr bake` CLI, cache keys | Shared by Blender add-on and engine importers |
| Reference clients | Current viewer, headless capture/compare harness | Used in CI-like evidence gates on real adapters |

Open design question (decide in P9 with a prototype, not in advance): whether the shipped shader path is (a) WGSL translated to each engine's shading language and run inside the engine's render graph using engine-owned textures, or (b) the wgpu renderer run on a shared or separate device with an interop copy. (a) gives best engine integration and avoids cross-device copies but costs a translation/validation pipeline; (b) is faster to build but adds synchronization and memory copies. Measure copy cost before choosing.

## Integration modes

| Mode | Used by | Description | Main risk |
| --- | --- | --- | --- |
| Offline/external process | Blender final renders (B4), CI, baking | Standalone process, manifest in, images out | Latency; good first target |
| In-process, own device | Blender viewport prototype, standalone tools | Library owns wgpu device; results copied to host | Copy cost, sync, device sharing limits |
| In-engine shaders (engine-owned resources) | Unity, Unreal | DASHR passes expressed as engine render passes/compute using engine textures; C API supplies baked maps, pose data and settings | Shader translation fidelity, engine render-graph hooks, per-backend variants |
| Asset-time only | Unity/Unreal importers | Validate, bake static maps, cache; runtime done by engine passes | Needs runtime for real-time animation |

## Phases

Numbering continues the [roadmap](dashr-porting-roadmap.md). Each phase follows the roadmap convention: prerequisites, deliverables, acceptance and stop conditions. Acceptance requires real-device evidence under the [validation gates](../port/validation.md); local CPU tests alone never promote a rendering phase.

### P8 — library extraction and stable API

Prerequisites: P4 acceptance (D3D11 single-asset parity, reviewed animation) for parity claims; extraction can start in parallel on the existing code.
Work: retain the `dashr` package and expose a default-feature headless library; make viewer/CLI/tooling dependencies optional; define the public Rust API (create context, load/validate asset, bake/cache, update pose, render/trace, read status); add asset/cache schema versions, resource lifetime rules and error types; keep viewer state and process-bound provenance outside the default library API.
Deliverables: `dashr` Rust library API, versioned mesh and bake-cache formats, API documentation, independent headless consumer and semantic-versioning policy.
Acceptance: the consumer builds from its own locked manifest without copied implementation files, validates/bakes an asset, updates pose, renders and reads primary/shadow statuses; default dependencies exclude UI/CLI; full-float tube/cube outputs and trace counts are byte-identical before/after extraction on the same adapter; ordinary and native GPU gates pass.
Stop: if extraction changes outputs, isolate and fix the change before continuing; do not bundle algorithm improvements or optimizations with extraction.

### P9 — C ABI and engine-shader feasibility

Prerequisites: P8.
Status (2026-10-03): a `ffi`-feature ABI v1 preview builds as a `cdylib`; Rust layout/error/callback tests and a native C client pass on Apple M1 Max / Metal. The per-thread synchronous log callback reports errors and session lifecycle events. cbindgen 0.29.4 generates the checked-in C header from `src/ffi.rs`, with a check mode wired into `mise run check`. Naga 30.0.1 translates the production WGSL stages to HLSL and MSL. The preview uses a library-owned wgpu device and copies frame output to host memory; target compiler validation, engine-owned textures and Unity/Unreal scene comparisons remain open.
Work: `dashr-capi` with opaque handles, POD structs with explicit sizes and a version field, thread-safety contract, and host logging callbacks. Prototype the in-engine shader route: translate WGSL to HLSL and MSL, run the deformation and trace passes in a minimal Unity and Unreal scene against engine-owned textures, and compare to the wgpu reference. Measure the interop alternative (copy cost) on the same scene.
Deliverables: generated C header, ABI tests (layout and round-trip), translation report listing unsupported constructs, decision record selecting the shader path per engine.
Acceptance: translated shaders pass the fixture comparisons within the agreed error budget; the decision record cites measured timings. Naga/translator gaps are listed as reproducers, not hand-waved.
Stop: if translated shaders diverge in seam behavior or float precision, keep the wgpu-device route for that engine and record the blocker.

### P10 — Blender integration

Prerequisites: P5 (exporter and schema) and P8; P9 for in-process binding.
Status (2026-10-03): the P5 exporter now runs under Blender 5.2.2 with host GPU access; the checked-in icosphere asset imports in the independent consumer. B1 constant/ramp analytic acceptance and human visual review remain open. Blender custom RenderEngine/viewport work is not implemented.
Work: ship a Blender 4.2+ extension (package contents at the ZIP root with `blender_manifest.toml`, relative imports only, archive layout verified before release) providing the exporter, validator and diagnostics; B4 external-process custom RenderEngine first; B5 viewport; consider an in-process binding via the C API only after B4 shows where copies matter. See the [Blender plan](blender-authoring-plan.md) for experiments B1-B6 and the new items added there for lone edges and seam-continuity checks.
Deliverables: installable extension for a pinned Blender release, authored example scene, paired Blender/viewer capture, cancellation and failure-handling tests.
Acceptance: B1-B4 acceptance criteria; clear artist-facing diagnostics naming vertices/faces/charts.
Stop: if GPU texture sharing is not feasible, remain on process + image transfer and document latency.

### P11 — Unity integration

Prerequisites: P9 decision record for Unity; P5 asset schema.
Work: native plugin wrapping the C API for baking/validation (editor import pipeline, cached static maps as assets); runtime deformation and trace as SRP passes or a `CommandBuffer`/compute integration, depending on render pipeline (URP/HDRP/built-in are separate targets; choose one first); skinned mesh pose export into the DASHR transform convention; material hook so the host shader samples DASHR output; hand-off of actual hit depth.
Deliverables: Unity package with a sample scene, importer for the DASHR asset, documented supported Unity version/pipeline/graphics API matrix.
Acceptance: sample character deforms correctly with Unity skinning; seam and silhouette comparison against the reference; frame timing recorded with the benchmark protocol; unsupported graphics APIs fail with explicit errors.
Stop: unresolved engine hook for depth or shadows limits the integration to a documented subset.

### P12 — Unreal Engine integration

Prerequisites: P9 decision record for Unreal; P5 asset schema.
Work: Unreal plugin/module with asset importer and cooked data; static bake at cook time; runtime passes through the Render Dependency Graph (RDG) and global/compute shaders, or a scene-view-extension hook; map Unreal skeletal mesh/Nanite/virtual-shadow-map interactions explicitly (Nanite and virtual shadow maps are known hard cases; decide supported versus unsupported on evidence); pose conventions and coordinate handedness (Unreal is left-handed, Z-up) in the API.
Deliverables: plugin, sample map, supported Unreal version/RHI matrix, cooked-asset format.
Acceptance: parity with the reference on fixtures after accounting for coordinate conversion; frame timing; behavior with the engine's shadow and depth systems documented.
Stop: if required RDG hooks are not available without engine source changes, document the limitation and ship a restricted feature set.

### P13 — topology extensions

Prerequisites: P8 and a passing closed-mesh library milestone; can start design earlier.
Work: cliff-edge geometry for sharp/lone edges, open boundaries and thin sheets (clothing), relaxing the uniform UV-winding requirement, separate non-unique material UVs, smooth-normal preprocessing across seams, seam-continuity checks and blend assist.
Deliverables: schema revision, validator rules, cloth-like fixture and paired captures.
Acceptance: new fixtures meet the benchmark gates without regressing closed-mesh results; unsupported topology (T-junctions, interpenetration, triangle soup) still rejected with element identifiers.
Stop: if open edges require a different surface-space definition, document it as a separate research question instead of shipping a partial mapping.

## Cross-cutting workstreams

| Track | Content | Upstream items |
| --- | --- | --- |
| Topology extensions (P13) | Cliff-edge geometry, open boundaries and thin sheets for cloth, relaxing UV winding, separate material UVs, smooth-normal preprocessing | L1, L2, L3, L10 |
| Traversal quality | Bracket/binary refinement at hits and teleports, seam-aware and adaptive stepping, tornado detection | U1-U4 |
| Storage | Split/compact teleport and transform storage, multi-teleport textures, 2-channel SDF, half/hybrid formats | U11-U14 |
| Shader/maths | Affine inverses, stored inverse, production variants, edgefill compute/fins experiments | U7-U10, U15 |
| Parameter derivation | Replace eye-tuned epsilons with units derived from asset scale/thickness; record in manifests | U6 |
| Metric tangent tooling | DASHR metric tangent generator; MikkTSpace-compatible export for engine shading only | L4 |
| Shadows and depth hand-off | Local self-shadow term, hit depth, long-range shadows by host; inter-object occlusion | L9, U17 |
| Seam quality tools | Albedo/height continuity check and blend assist | L11 |

## Research tracks

- **R1 Self-intersection tunnels.** CPU diagnostics now report non-coplanar crossings, non-adjacent coplanar overlaps and face orientation flips/collapses with face/chart IDs. Detecting global surface-space map folds and testing better-behaved mappings on bent fixtures remains open (U5).
- **R2 Conservative bounds.** Min/max pyramids valid under bending, gutters and teleports (O5).
- **R3 Inter-object visibility.** Shadow and reflection rays leaving the local skin volume; needs engine scene access.
- **R4 Other representations.** [U19-U20 research](../research/2026-10-03/non-heightfield-warps-u19-u20.md) records analytic SDF/warp probes, a two-bone inverse-LBS pose sequence, CPU voxel sampling, matched 32³/64³ GPU voxel warps, finite Menger depth-2 timing, depth-3 exact ray agreement and depth-4 explicit budget exits, and a two-pose Gaussian adapter with GPU sorting. Visual/normal review, authored splat assets, larger sort workloads, repeated depth-3/4 timing, general fractal fields and production integration remain open; these experiments stay outside the library path.

## Packaging, licensing and governance

- Upstream's permissive license (MIT-0 or the public-domain-style alternative, see the paper) permits this; the repo already uses MIT-0 for the Rust crate. Retain Tom's attribution and keep third-party assets, demo executables and dependencies separate in a provenance manifest.
- Publishing crates, plugins, Unity/Unreal marketplace listings or a Blender extensions-platform upload are outward-facing acts: do them only on explicit request, after the matching phase's evidence exists.
- Do not ship Tom's demo art assets inside engine packages without checking their provenance.
- Keep shareable docs machine-neutral.
- Offer upstream issues, not PRs, for findings about the paper (Tom said he is not seeking PRs).

## Prioritized next steps

1. Execute [P8 standalone library goal](standalone-library-goal.md): prove consumption from an independent headless application without viewer dependencies or copied implementation files.
2. Resolve remaining P0-P4 evidence alongside extraction; keep Windows/D3D11 parity and reviewed animation explicit gates.
3. Establish P9 C ABI and host-owned resource feasibility before engine-specific integration work. A separate-device copy path must disclose synchronization and measured transfer cost.
4. Resume P5 authoring and select one P10-P12 integration only after the relevant library/API prerequisites pass.
5. Run traversal, storage and topology experiments as separately measured changes. Keep all U1-U20 items in the [upstream register](upstream-future-work.md), including deferred research.

The [2026-10-02 research conclusions](../research/2026-10-02-library-first.md) record the source audit and priority decision. Library extraction preserves current behavior; it does not certify robustness, scene readiness or every engine/backend.
