# DASHR future-work audit and library-first conclusions

Decision date: 2026-10-02. The standalone drop-in library is this fork's highest development priority. This is a planning decision, not a claim that the current port is ready for production.

## Sources and verification

The user supplied the future-work passage from Tom Forsyth's [DASHR paper](https://tomforsyth1000.github.io/DASHR/paper/DASHR_Paper.html#future-work). Live retrieval through the browser tool failed during this pass; remote freshness is unverified. The passage was checked against the preserved [paper's Future work section](../../paper/DASHR_Paper.html#future-work). The existing [upstream register](../plans/upstream-future-work.md) contains all 20 corresponding items and 11 additional limitation entries. Those IDs remain canonical to avoid a competing backlog.

The paper establishes upstream's research/demo intent, invitation to build a library, and preference against pull requests. It does not establish that any proposed improvement is implemented or faster. Maintain attribution and discuss findings only when separately authorized; no upstream contact is part of this task.

## Complete future-work coverage

| IDs | Preserved work | Priority and evidence |
| --- | --- | --- |
| U18 | Robust standalone drop-in library | First: P8 extraction, external consumer, then P9 ABI/resource feasibility |
| U1-U6 | Tornado mitigation; binary refinement at hits and teleports; SDF seam warnings; transform-change adaptive retries; self-intersection tunnels; derived epsilons, especially shadows | Correctness/robustness backlog; adversarial fixtures and explicit failure diagnostics before promotion |
| U7-U10 | Stored inverse; compute edgefill; mesh fins; filtered edgefill indirection with finite-difference distortion | Separate experiments; retain whole-mesh Animation Distortion and measure hot-loop latency/read costs |
| U11-U15 | Multi-channel corner SDF (two channels is a hypothesis); split SDF/destination; multiple destination fields; compact texture/vertex storage; smaller inverses and compiler inspection | Profile and compare against full-float baseline; destination filtering allowed only within proved continuity regions |
| U16 | T-junctions, interpenetration, self-intersections and unusual topology; lone edges and thin sheets/clothing | Reject unsupported inputs; P13 defines extensions after closed-mesh acceptance. Arbitrary triangle soup remains outside the supported contract |
| U17 | Efficient traversal, min/max mipmaps, horizon maps, lighting, ambient occlusion, subsurface scattering and other effects | Conservative acceleration research plus host shading hooks; advanced lighting belongs to host materials initially |
| U19-U20 | SDF, fractal, Gaussian splats, voxels and GPU warp animation for other representations | Retained exploratory research; authoring and representation semantics unresolved; not initial library acceptance |

## What drop-in must mean here

A Rust library target alone is insufficient. `Cargo.toml` currently declares winit, egui, CLI and capture-related dependencies unconditionally. The planned library must be consumable by a separate application without copying source, launching a viewer or pulling window/UI dependencies into its core dependency graph. Device-independent validation and topology preparation remain reusable without a GPU.

The first supported runtime is a documented headless Rust/wgpu consumer. Drop-in Unity/Unreal support requires a separate P9 decision: host-owned device, render graph, texture formats, synchronization, shader translation and depth/shadow hand-off. A C ABI alone cannot solve GPU interoperability. Own-device rendering with copies is a possible restricted mode, with transfer costs disclosed. API names and crate boundaries in the plan remain proposals until extraction validates them.

## Execution and promotion

Start P8 now from the existing native implementation while resolving reference evidence. Preserve metric tangent lengths, full-float storage, matrix/uniform layouts, filtered seam distance and nearest destination UV. Do not bundle traversal or compression changes into extraction. P5-P7 and engine plugins follow the library rather than delaying it.

P8 passes only with independent consumption, unchanged fixture behavior, lifecycle/error documentation and a UI-free dependency graph. Windows/D3D11 parity, reviewed animation, scene readiness and speedups retain their own [validation gates](../port/validation.md). All upstream improvements remain planned or exploratory until their individual measurements exist. The [copy-ready goal](../plans/standalone-library-goal.md) defines the next implementation contract.
