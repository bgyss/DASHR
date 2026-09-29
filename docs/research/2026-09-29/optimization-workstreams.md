# Optimization workstreams

## Priority and method

Profile the reference before optimizing. Separate steady-state rendering, topology bake latency and image/import quality. Many ideas below are already suggested by [Tom's future-work discussion](../../../paper/DASHR_Paper.html#future-work); the contribution of this plan is their ordering, experiment design and evidence gates. All performance effects remain hypotheses.

| ID | Workstream | Priority | Experiment and promotion evidence |
| --- | --- | --- | --- |
| O1 | Reproducible baseline and instrumentation | First | Fixed scenes, GPU pass timing, trace counters and images; no optimization until reference outputs exist |
| O2 | Static bake structure and caching | High for real assets | Topological seam adjacency plus cache; measure bake latency and map equivalence |
| O3 | Transform/teleport storage | High | Format/layout variants, memory totals, filtered map error and seam tests |
| O4 | Shader math and specialization | High | Affine inverses and production variants; examine compiler output and GPU pass time |
| O5 | Height traversal and convergence | Medium after parity | Bracket refinement, seam-aware stepping, conservative bounds; compare misses and tail steps |
| O6 | LOD, visibility and update frequency | Medium | Screen-error budgets, dirty-pose updates and asset visibility; test temporal transitions |
| O7 | Compute and pass fusion | Conditional | Retain separate outputs initially; account for deformation gradients and global dependencies |
| O8 | Broader volumetric representations | Exploratory | Isolated scalar-field example; prove mapping assumptions before generalizing |

## O1: profiling and reproducibility

The current ImGui frame time and step visualization are useful diagnostic views, but are not isolated GPU pass timings. Vsync defaults on. Capture deformation, edgefill/distortion, primary surface shading, optional shadow contribution, UI and presentation separately. For D3D11 use timestamp queries inside a timestamp-disjoint interval and reject unreliable intervals; the [Microsoft query documentation](https://learn.microsoft.com/en-us/windows/win32/api/d3d11/ns-d3d11-d3d11_query_data_timestamp_disjoint) defines frequency and reliability. Read results asynchronously so profiling does not introduce a frame-by-frame wait.

A/B lighting modes on identical camera/pose is an initial way to estimate shadow cost; specialized primary-only and primary+shadow variants give a cleaner comparison. Record warm-up, power/thermal state, resolution and exact parameters. Use [benchmark protocol](benchmark-protocol.md).

## O2: preprocess once, rebuild precisely

`CreateTeleportEdgefill` compares each vertex against preceding vertices and scans triangles when locating seam partners (`main.cpp:2967` onward). Its full-map iterative flood fill (`main.cpp:3194` onward) can take multiple passes. These are good candidates for larger imported assets, but not evidence of a current runtime bottleneck.

Use source-topology identifiers and an edge adjacency table from the importer. When those are unavailable, a spatial hash can find coincident candidates, followed by distance, normal and skinning-compatibility checks; quantization alone must not join nearby independent surfaces. Define deterministic tie-breaking for multiple candidates. Compare seams against the old algorithm on procedural fixtures before enabling the new importer path.

Separate geometry/UV changes from material, camera and animation changes. Cache static occupancy/teleport/edgefill maps keyed by topology, unique UVs, resolution, bake algorithm version and every input affecting the bake. Store checksums and verify cache invalidation. Baking remains distinct from the dynamic deformation/gutter pass.

A queue-based propagation or exact distance transform can replace repeated global sweeps. Jump flooding is a later GPU experiment: approximate nearest-seed maps can change seam destinations, so do not assume they are equivalent. CPU/GPU occupancy rasterization may disagree at triangle edges; either preserve the reference raster bake or quantify coverage differences. GPU-bake readback removal only matters if asset preparation is actually expensive.

## O3: storage and bandwidth

Source allocations (`main.cpp:2842–2849`) are eight RGBA32F transform images: four temporary and four final. Teleport and edgefill (`3322`, `3370`) each use RGB32F. With no mips, the logical payload is `152 * N * N` bytes, excluding driver padding, CPU maps, material images, shell buffers and depth.

| Map dimension | Transform sets, MiB | Two static maps, MiB | Total, MiB |
| --- | --- | --- | --- |
| 256 | 8 | 1.5 | 9.5 |
| 512 | 32 | 6 | 38 |
| 1024 | 128 | 24 | 152 |
| 2048 | 512 | 96 | 608 |
| 4096 | 2048 | 384 | 2432 |

These are calculated storage amounts, not measured bandwidth or physical resident memory. The default map is 256; the UI allows 16 through 4096. Per-asset scaling can be material even though the reference default is small.

Start with independent toggles:

- Split teleport distance into R32F and destination into RG32F. `TraceRay` reads distance every iteration and destination only on teleport. Total uncompressed static payload need not shrink; reduced frequently sampled working set is the hypothesis. Maintain linear distance and nearest destination sampling.
- Edgefill uses only `.xy`; test RG32F instead of RGB32F. Exact same values can preserve semantics.
- Test RGBA16F transform maps with full-float arithmetic. Compare determinant conditioning, anchored positions, distortion sign, gutters and large coordinate ranges. Half storage halves these texture payloads, but not necessarily elapsed time. Allow hybrid layouts: keep anchored position or problematic bases in 32-bit.
- On portable APIs RGB32F may require RGBA padding; choose supported RG/RGBA layouts explicitly rather than relying on source byte counts.
- Pack four weights and vertex basis directions only after measuring geometry cost. Direction compression must preserve separately stored length and nonorthogonality; conventional normalized TBN encoding is insufficient.

`PipelineEdgefill` mode 1 reads five transform neighborhoods but neighbors only contribute position. Explicitly load four center planes and the position plane of four neighbors instead of constructing five full transforms. The HLSL compiler may already remove unused loads; verify shader assembly or capture counters before claiming fewer fetches.

## O4: inverse math and shader variants

`GetInverse4x4` is used at deformation vertices, during gutter correction and for lighting. A 3x3 basis inverse plus affine offset calculation may remove unnecessary general 4x4 work. Do not replace an inverse with transpose: the basis is scaled and skewed. Validate with random well-conditioned affine bases and deliberately singular inputs, then compare actual emitted instructions.

For lighting, benchmark a locally computed 3x3 inverse against a precomputed forward-basis texture. Extra bandwidth, storage and filtering inconsistencies can outweigh ALU savings. Defer extra textures until timing identifies this work as relevant.

Compile mode-specific production shaders for distortion and lighting, with separate debug variants. Inspect register pressure and total GPU time; uniform branches are not necessarily expensive, so specialization does not guarantee a win. Generate variants deliberately rather than an unbounded combination cache.

## O5: traversal improvements without invalid shortcuts

Separate hit/miss, escape, budget exhaustion, invalid basis and repeated teleport termination. Budget exhaustion should emit diagnostic status, not manufacture a hit. Keep reference semantics as a comparison mode.

Implement bracketed refinement only after preserving the same chart across the bracket or explicitly resolving seam transitions. Smaller hit error is a quality objective; it may increase runtime. Use signed seam distance to shorten approaches to a seam, and compare consecutive sampled transforms to adapt steps in high distortion. Measure camera and shadow rays independently.

Conservative min/max height pyramids are a promising later experiment. Ordinary averaged mips are not conservative traversal bounds. A skipped object-space interval must remain safe under changing surface-space mapping, deformation, gutters and teleports; bounds valid in one planar chart do not prove safety across a bent shell.

[Relaxed cone stepping](https://developer.nvidia.com/gpugems/gpugems3/part-iii-rendering/chapter-18-relaxed-cone-stepping-relief-mapping) is a useful algorithmic baseline for precomputed height traversal and refinement. Its guarantees do not automatically transfer to DASHR's evolving mapping. Compare it first on a static planar chart, then bend and cross seams; reject a speedup if thin-feature misses increase.

## O6: LOD and scheduling

Reuse dynamic transform maps for unchanged poses, even when the camera changes. Do not reuse them across changed bones or shape deformation. Cull wholly invisible assets before dynamic updates only if shadow/reflection consumers do not require them. Distinct instances can share a baked topology atlas; they can share dynamic transforms only with identical deformation in the atlas's object-space convention.

Choose atlas resolution by chart coverage, deformation error and gutter width, not just texture detail. Height images and transform maps serve different purposes and need not have matching resolution. A lower-resolution atlas can lose teleport/gutter precision even when the height texture remains detailed.

Investigate distance-based height fade toward 0.5 and transition to conventional mesh shading, as suggested in the paper. Test silhouettes, temporal stability and depth continuity. Add material mips and explicit gradients separately from intersection bounds; current albedo uses implicit derivatives inside divergent control flow, which needs deliberate treatment in WGSL.

## O7: compute is an experiment, not the default rewrite

Keep UV rasterization for deformation initially: it already supplies triangle coverage and interpolation efficiently. Compute is most attractive for a controlled gutter/gradient pass, instrumentation, baking or a later tiled tracer.

Do not port neighbor reads into an in-place parallel kernel. Threads reading values another thread is rewriting can race, and a workgroup barrier cannot synchronize an entire atlas. Use immutable input and distinct output, or a proved multi-dispatch schedule. A compute conversion still has to produce interior distortion ratios; copying only gutter pixels is not a complete replacement.

Removing clears requires a coverage proof for all texels that can be sampled. Removing the gutter pass by adding an indirection inside every trace iteration may trade one pass for far more samples. Keep both as optional benchmarks until valid and faster.

## O8: research beyond heightfields

Try one bounded SDF or voxel field within an existing chart with the same reference shell and seam machinery. Test whether scalar-field continuity, chart metric and deformation support a usable step bound. Treat cloth/open surfaces, multi-layer fields, Gaussian splats and arbitrary self-intersecting geometry as separate research questions. Rust, Metal and wgpu do not solve these representation limits.
