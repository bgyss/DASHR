# Upstream future work and limitations: fork traceability

Source: the "Limitations and subtleties" and "Future work" sections of Tom Forsyth's [published paper](../../paper/DASHR_Paper.html#future-work) (v1.0, 2026-09-27; README/publication update 2026-09-29, merged 2026-09-30). The paper remains the algorithm authority. This page restates each of Tom's points, says how this fork treats it and links the workstream that owns it. Status values: **Open** (no work), **Planned** (owned by a workstream), **Partial** (some implementation exists, acceptance incomplete), **Fork decision** (this fork deliberately differs from upstream scope).

All expected benefits are hypotheses until measured under the [benchmark protocol](../research/2026-09-29/benchmark-protocol.md). Tom's paper describes the project as a research paper and proof of concept, says he is not seeking pull requests and explicitly invites others to turn it into a real library. This fork takes up that invitation; see [standalone library and integrations](standalone-library-and-integrations.md). Preserve attribution (see [provenance](../port/provenance.md)) and keep fork-specific claims separate from upstream's.

## Tom's "In scope - I may look at these myself"

| ID | Tom's point | Fork status and owner |
| --- | --- | --- |
| U1 | Mitigate "tornados" (poles) in high-distortion areas; current mitigation is to instruct artists | Planned: O5 traversal and authoring diagnostics (L3). Exporter reports the affected faces/charts; never hide poles with a step cap |
| U2 | Binary search instead of linear interpolation at the surface hit, and again at teleports so heightfield mismatch across a seam resolves precisely | Planned: O5 bracket refinement. Needs same-chart or seam-resolved bracket; compare hit error separately from runtime |
| U3 | Use the teleport SDF channel to warn rays approaching a teleport edge and shrink steps | Planned: O5 seam-aware stepping |
| U4 | Compare new and previous `surfaceFromObject`; if the difference exceeds a threshold take a smaller step and retry | Planned: O5 adaptive stepping; measure camera and shadow rays separately |
| U5 | Self-intersection "tunnels" when surfaces bend into themselves | Open research: R1 in [library plan](standalone-library-and-integrations.md#research-tracks). Diagnose first (detect and report), then explore better-behaved mappings |
| U6 | Many eye-tuned epsilons, especially shadows; automate more | Planned: L9 parameter derivation. Move epsilons to named, unit-bearing settings derived from asset scale and thickness; record them in capture manifests |

## Tom's "Reasonable, but low-priority" items

| ID | Tom's point | Fork status and owner |
| --- | --- | --- |
| U7 | Store inverse `surfaceFromObject` from the distortion pass in a separate texture instead of inverting at shading | Planned: O4 (benchmark 3x3 inverse against stored forward basis; extra bandwidth may lose) |
| U8 | Edgefill copy as a compute shader with read/write target, only copying needed regions (Animation Distortion still needed for the whole mesh) | Planned: O7; separate source/destination maps first, conditional on race-free schedule |
| U9 | Replace the edgefill copy with mesh "fins" written in the first pass | Planned: O7 alternative; unresolved source of Animation Distortion values |
| U10 | Have `TraceRay` always indirect through the Edgefill Texture (filtered read, finite-difference distortion) | Planned: O7 benchmark-only alternative; adds latency/reads to the hot loop, profile first |
| U11 | Two-channel teleport SDF to recreate sharp corners between two edges | Planned: O3 |
| U12 | Split the teleport SDF (single channel, read every step) from the 2-channel destination (read only on teleport) | Planned: O3 (already the first listed storage toggle) |
| U13 | Multiple teleport textures so discontinuities are separated and each can be bilinearly filtered, fewer iterations after teleporting | Planned: O3 extension; validate seam destination error |
| U14 | Replace float32 texture/vertex storage with compact formats | Planned: O3 (RGBA16F transforms, RG edgefill, hybrid layouts). Promotion requires conditioning and seam tests |
| U15 | More efficient maths, e.g. 4x3/3x3 inverse instead of a general 4x4; confirm compiler behavior | Planned: O4; inspect emitted instructions rather than assume |

## Tom's "Out of scope for my work" (fork position)

| ID | Tom's point | Fork position |
| --- | --- | --- |
| U16 | T-junctions, interpenetrating triangles, self-intersecting meshes, arbitrary triangle soup | Remain unsupported. The importer must detect and reject them with element identifiers. Lone edges and thin sheets are a supported goal via L2 |
| U17 | Acceleration structures (min-max mips, horizon maps), better lighting, AO, SSS | Fork decision: acceleration structures and lighting integration are in scope for the library where the host engine needs them. Conservative bounds must remain safe under bending, gutters and teleports (O5). Fancy shading is left to host materials |
| U18 | A general robust drop-in library | Fork decision: in scope. This is the fork's primary forward goal, see the [library plan](standalone-library-and-integrations.md) |
| U19 | Non-heightfield representations (SDF, fractal, splats, voxels) | Out of scope for the library; O8 stays an isolated exploratory experiment |
| U20 | Animating characters in other representations by warping spaces on GPU | Research note only (R4) |

## Tom's "Limitations and subtleties"

| ID | Limitation | Fork handling |
| --- | --- | --- |
| L1 | Requirements: unique UV map, sufficient gutter, no T-junctions, consistent UV winding (could be relaxed), every edge shared by exactly two triangles (possibly across a seam) | Enforced as a validated input contract in the [asset contract](blender-authoring-plan.md#proposed-versioned-asset-contract) and the library's `validate` API. Winding relaxation and open edges are L2 |
| L2 | Sharp/lone edges need extra "cliff edge" geometry, needed for clothing; Tom plans to work on this | Planned: P13 topology extensions (cliff-edge geometry, open boundaries, thin sheets). Design after the closed-mesh library milestone; do not combine with initial port parity |
| L3 | Smooth displacement normals required across seams; a smarter mesh preprocessor could fix this. Sharp surface normals can live in heightfield/normal maps | Planned: exporter/preprocessor (P5 and P13). Report discontinuities per vertex; a forced seam plus teleport is the recommended remedy |
| L4 | Tangent/bitangent lengths matter; MikkTSpace normalises and is unsuitable without modification | Already a fork contract (metric tangent lengths). Planned: a DASHR-specific metric tangent generator in the library, with a documented MikkTSpace-compatible export option for engine shading only |
| L5 | Tangent discontinuities cause extra iterations and tornado poles; prefer forced seams plus teleports | Diagnostics in the importer (see U1) |
| L6 | Self-intersecting bent surfaces produce tunnels | See U5 |
| L7 | The demo is not a benchmark; compact formats, shader variants and context-specific tuning needed | Fork's [benchmark protocol](../research/2026-09-29/benchmark-protocol.md); no speedup claims from GUI FPS. Production shader variants per engine target |
| L8 | The raymarcher is unsophisticated; no min/max mips or horizon maps | See U17 |
| L9 | Self-shadowing stops outside the local skin volume; combine with shadow buffers for long range. Local raymarched shadows let shadow-buffer epsilons relax | Library plan: the host engine owns long-range shadows. Document the hand-off (DASHR supplies local shadow term and actual hit depth). Inter-object shadows are a named gap |
| L10 | One UV set currently drives albedo, normal, height and tangent space; separate, non-unique material UVs are possible but error-prone | Planned: schema revision after the unique-atlas milestone. Name each UV/tangent space in code and schema so mixing is visible |
| L11 | Seams need artists to match albedo/heightfield on both sides; robust solutions should be investigated | Planned: seam-continuity checks in the exporter plus an optional seam-blend/bake assist (P13). Height mismatch measured at teleports, per U2 |

## Priority and scope update (2026-10-02)

The user-supplied future-work passage was checked against the preserved paper: U1-U20 cover all of its list items, including the nested animation research avenue. U18 is the highest development priority; execute P8 before optional integrations and optimizations. U16 remains an explicit support contract and research question, U17 includes host lighting hooks with AO/SSS left to host materials, and U19-U20 retain SDF, fractal, Gaussian-splat and voxel research without becoming initial-library requirements. No item is dropped merely because upstream excludes it from its own work. See the [research audit](../research/2026-10-02-library-first.md) and [P8 goal](standalone-library-goal.md).

## Coverage summary

Every upstream item maps to a workstream or an explicit position above. U1-U15 and L2-L11 are open gates; none is promoted. The current Rust/wgpu viewer implements the reference behavior and related diagnostics, not these improvements. See [validation](../port/validation.md#roadmap-gate-status) for what has actually been executed.
