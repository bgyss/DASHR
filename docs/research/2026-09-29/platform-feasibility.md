# DASHR platform feasibility: Blender, macOS/Metal, Rust/wgpu

Research date: 2026-09-29. This is a source-based feasibility assessment and proposed plan, not evidence of a running port. No Blender, Metal or wgpu implementation was built or benchmarked for this assessment. API documentation was checked live; pin exact dependencies and recheck capabilities before implementation.

## Recommendation

Build one reproducible standalone renderer first, using Rust/wgpu if cross-platform maintenance is the priority. Target native Metal on macOS and D3D12 on Windows, retaining Tom Forsyth's D3D11 demo as the reference. Begin with the existing raster passes and fragment ray marcher. A native Metal-only C++ port is a reasonable alternative when the immediate goal is a small Mac demonstration and reuse of the current CPU code matters more than broader portability.

Develop Blender support in two stages: an authoring/export bridge, then a custom render engine only after the standalone renderer passes parity checks. Treat integration inside Cycles or EEVEE as a separate research project. Rust, wgpu and Metal support do not themselves imply faster rendering.

## What the current implementation actually needs

The following requirements come from the checked-in source, rather than platform marketing:

- `demo/PipelineDeform.hlsl` rasterizes the skinned mesh in UV space and writes four inverse-transform vectors to four render targets. `demo/main.cpp:2842` onward allocates eight RGBA32F textures for original/padded transforms. Both deformation and padding require a format plan.
- `demo/PipelineEdgefill.hlsl` samples the intermediate transforms and copies them using a precomputed edgefill map. Seam teleport destinations must remain point-sampled; the distance component and deformation data use different sampling semantics.
- `demo/PipelineMain.hlsl` performs iterative inverse mapping, heightfield intersection and optional shadow tracing in a fragment shader. It reads eight sampled textures and two samplers. Most traversal reads explicitly use mip level zero, while albedo and normal reads include implicit sampling.
- `demo/ConstantBuffer.hlsl` uses explicit row-major matrices. `demo/Utils.hlsl` applies four weighted bone matrices without bone indices, and mutates several arguments through HLSL `inout` parameters.
- `demo/main.cpp:1407` onward and `:1013` use reverse Z: greater depth comparison and a zero clear. The main shader returns color, without an actual-hit depth output. A faithful port should preserve that behavior initially; correct scene depth is a later behavior change.
- `GenerateTangentSpace()` in `demo/main.cpp:2441` calculates UV-scaled, angle-weighted tangent/bitangent vectors. The deformation shader explicitly relies on a basis that can be scaled and nonorthogonal. Ordinary normalized normal-map tangents are insufficient as a drop-in replacement.

These are conventional programmable rendering operations. The unusual work is numerical and geometric correctness, rather than a requirement for hardware ray tracing, mesh shaders or CUDA.

## Rust/wgpu: preferred portable renderer candidate

### Verified platform capability

wgpu documents native Metal, D3D12 and Vulkan backends, with WGSL shader support and optional SPIR-V/GLSL input. It uses D3D/Metal coordinates and a zero-to-one depth range. The retrieved documentation identified wgpu **30.0.1**; this is a source snapshot, not a dependency selection or claim that that version has been tested here. [wgpu crate documentation](https://docs.rs/wgpu/30.0.1/wgpu/)

The likely architecture is a Rust host with explicit mesh/scene data, WGSL shader modules, a portable asset format and a headless image/capture path. Keep topology processing, serialization and GPU rendering separate so Blender need not link the renderer into its process at the first milestone. This architecture is a recommendation based on the repository's coupling, not a verified implementation.

### Hard compatibility checks before translating everything

| Concern | Repository requirement | Port decision |
| --- | --- | --- |
| MRT output budget | Four RGBA32F outputs need 64 bytes per sample | Query/request a sufficient limit, or split deformation/padding into two two-target passes; retain full precision initially |
| Float filtering | Inverse-transform textures are linearly sampled | Request `FLOAT32_FILTERABLE` when available; otherwise implement tested bilinear reads using `textureLoad` |
| RGB32F maps | Teleport/edgefill use three float channels | Use an explicit supported replacement, initially RGBA32F; do not silently convert signed distances/UVs to normalized color formats |
| Uniform layout | HLSL row-major constants | Specify CPU packing and shader layout; verify known transforms before animation |
| Ray loop behavior | Hit-dependent branching and texture samples | Make sampling LOD/control flow deliberate, with recorded image comparisons |
| Scene depth | Proxy-shell depth, alpha on misses | Preserve reference first; later test discard/hit depth against other geometry |

wgpu's default color-attachment byte limit is 32; higher limits must be supported by the adapter and explicitly requested on the device. Its documented defaults permit 16 sampled textures per stage and 256 compute invocations per workgroup. The main shader's eight sampled textures fit that texture count; the four full-float outputs do not fit the default byte budget. Four RGBA16F outputs fit 32 bytes, but that is a precision experiment rather than the baseline. [wgpu limits](https://docs.rs/wgpu/30.0.1/wgpu/struct.Limits.html)

`FLOAT32_FILTERABLE` enables filtering for R32F, RG32F and RGBA32F. `TIMESTAMP_QUERY` enables pass timestamp writes; timestamps inside passes or command encoders have additional feature requirements. Query support and request only the needed features. Keep timing optional, and label CPU submission/wall timing separately from GPU timing. [wgpu features](https://docs.rs/wgpu/30.0.1/wgpu/struct.Features.html)

`Adapter::get_texture_format_features` reports format support; adapter-specific usages can require a native extension. Record adapter info, requested device limits, feature flags and selected formats in every capture. This makes a native-only capability tier distinguishable from a portable tier. [wgpu adapter API](https://docs.rs/wgpu/30.0.1/wgpu/struct.Adapter.html)

A budget above 64 can be requested if the adapter exposes it; 64 is not a universal ceiling. The present four-target passes need exactly 64. See the [attachment-budget follow-up](wgpu-attachment-budget.md) for negotiation, backend caveats and fallbacks.

### Shader translation and compute constraints

WGSL matrices are column-oriented. Implicit `textureSample` has derivative/uniformity constraints; `textureSampleLevel` provides explicit LOD. Storage textures have a defined format/access model, including RGBA16F and RGBA32F. These facts motivate explicit packing, LOD policy and ping-pong resources; do not treat HLSL source conversion as numerical validation. [WGSL specification](https://www.w3.org/TR/WGSL/), [storage texel formats](https://gpuweb.github.io/gpuweb/wgsl/#texel-formats)

Port `inout` functions into explicit result structs or controlled pointers, retain floating-point arithmetic and loop bounds, and preserve the existing inverse function until equivalence is established. Validate both distortion modes and the point/linear sampler split. Do not normalize the basis in the name of conventional tangent-space cleanup.

A compute version of the ray marcher can sample textures; the real change is losing the proxy rasterizer's coverage/interpolation and implicit derivatives. It needs camera-ray generation, object bounding/culling, multiple-object hit arbitration, depth output and an explicit LOD choice. A naïve full-screen dispatch can do more work than the shell rasterizer. Implement it only as a measured alternative after the fragment path works.

Padding/edgefill is a narrower compute candidate: independent destination texels reading the same source maps can write four destination textures. Preserve separate input/output resources. GPU seam-map construction is a larger topology/synchronization project; keep the CPU generator first. No general performance win is established by either proposal.

### Native first, browser later

The WebGPU-style API offers a possible browser path, but this project has no browser build. Asset loading, shader validation, format features, attachment limits, readback, interaction and deployment must all be demonstrated separately. Avoid adding WebGL compatibility, hardware ray tracing or experimental mesh-shader dependencies to the initial renderer.

## macOS / Metal

### Viable routes

1. **Rust/wgpu with Metal backend:** shares the host and shader contract with other platforms; evaluate native Metal capture/profiling when needed.
2. **C++ plus Metal-cpp and MSL:** retain CPU mesh/seam code and replace Win32/D3D11 presentation/resources. Apple provides Metal-cpp as a low-overhead C++ interface to Metal, reducing the need to rewrite the CPU algorithm in Swift. It does not translate D3D11 or HLSL automatically. [Metal-cpp](https://developer.apple.com/metal/cpp/)
3. **Blender `gpu` backend:** useful for an in-process Blender prototype, with a separate shader translation path and Blender version coupling.

Apple's texture example demonstrates fragment sampling and render-populated textures, which directly match major operations in the demo. This establishes the availability of primitives, not DASHR parity or performance. [Creating and sampling textures](https://developer.apple.com/documentation/metal/creating-and-sampling-textures)

### Format, memory and synchronization policy

Metal format capabilities vary by GPU family. Apple's feature tables distinguish RGBA16F from RGBA32F filtering/rendering capabilities. Recheck the actual Mac's feature families and formats rather than extrapolating from a recent Apple GPU. [Metal feature tables](https://developer.apple.com/metal/Metal-Feature-Set-Tables.pdf)

On Apple GPUs, shared storage is CPU/GPU accessible, private storage is GPU-only and memoryless textures are temporary tile resources. Use private storage for warp/padding textures consumed by later passes; they must retain their contents and should not be treated as memoryless attachments. Unified memory does not eliminate CPU/GPU synchronization. [Apple GPU storage modes](https://developer.apple.com/documentation/metal/choosing-a-resource-storage-mode-for-apple-gpus), [shared-resource synchronization](https://developer.apple.com/documentation/metal/mtlstoragemode/shared)

The existing warp allocation is `128 × N²` bytes before other textures: at 4096² it is 2 GiB. A Mac port should expose an allocation estimate and recover gracefully from unsupported resolutions. This is a source-derived allocation calculation, not a measured resident-memory result. Reduce texture precision only after image and convergence checks; avoid rewriting all passes for tile-specific techniques before establishing a working portable baseline.

Acceptance requires an actual Apple Silicon capture with fixed animation time, comparable resolution/settings, intermediate texture checks and per-pass timing. macOS Intel/AMD, browser Metal and other Apple platforms remain additional targets with separate evidence gates.

## Blender: authoring bridge first

### Export and preparation

Blender exposes evaluated dependency-graph objects and temporary evaluated meshes for exporters, including animation/modifier evaluation; temporary meshes must be released. It also exposes loop-triangle tessellation and normal-map tangent generation. Those APIs support an exporter, but their standard tangent output does not establish compatibility with DASHR's UV metric. [Blender dependency graph](https://docs.blender.org/api/5.0/bpy.types.Depsgraph.html), [Blender mesh API](https://docs.blender.org/api/5.0/bpy.types.Mesh.html)

Proposed asset contract:

- Triangulated mesh with UVs, separate face-corner data where UV/normal discontinuities require it, stable chart/seam adjacency and material assignments.
- Original mesh plus indexed joint/weight data for skeleton playback, **or** an evaluated per-frame geometry stream for broad modifier support. Declare the choice and never apply skinning twice.
- UV-scaled tangent/bitangent construction matching the renderer; validate mirrored islands, degenerate UV triangles, nonuniform object scale and skinning-induced near-singular bases.
- Height units, midpoint/offset, texture channel, precision and color-space semantics. Height/maps are data; albedo color conversion is a separate operation.
- Explicit policy for overlaps, UDIMs, nonmanifold edges, topology-changing modifiers and missing UVs. Initially reject unsupported cases with actionable diagnostics.

The four hard-coded demo bones should remain a small reference fixture. General Blender rigs need indexed joints and a declared influence policy, or the evaluated-geometry route; neither is already implemented.

### Custom render engine

`bpy.types.RenderEngine` exposes `render`, `view_update`, `view_draw` and render-result/pass handling. This is the appropriate public extension point for a standalone DASHR renderer inside Blender. It does not mean adding a custom intersection routine to Cycles. [Blender RenderEngine API](https://docs.blender.org/api/5.0/bpy.types.RenderEngine.html)

Two useful prototypes are (a) launching the standalone renderer and presenting returned frames, and (b) using Blender's `gpu` module for all rendering. Begin with file/image exchange for correctness; continuous viewport updates need incremental invalidation and must avoid per-frame blocking readback. Rust/wgpu textures cannot simply be assumed to be importable into Blender's GPU context; shared device/texture ownership is a separate native integration spike.

Blender's GPU API translates GLSL to MSL on Apple platforms with compatibility limitations, including matrix-constructor restrictions and reserved keywords. Use supported shader-creation APIs and test the chosen Blender release/backend. The older `bgl` module is deprecated in favor of the API-independent `gpu` module. [Blender GPU API](https://docs.blender.org/api/5.2/gpu.html), [bgl deprecation](https://docs.blender.org/api/4.5/bgl.html)

The cited Blender APIs span 4.5, 5.0 and 5.2 documentation snapshots. Choose one supported release for the first add-on, record its exact version/backend, and verify the examples there. Do not copy legacy OpenGL-oriented render-engine example code as evidence of a working Metal integration.

### Cycles and EEVEE are different scope

Cycles displacement changes a subdivided mesh; bump changes shading normals and lacks displaced silhouettes/self-shadowing. These are useful quality/performance baselines, but neither reproduces DASHR's inverse-mapped seam-teleport traversal. [Cycles material displacement](https://docs.blender.org/manual/en/4.5/render/cycles/material_settings.html)

A Cycles integration would need an investigation of intersection representation, surface evaluation and deformed bounds across all ray types and GPU kernels. An EEVEE integration would need ownership of draw passes, actual-hit depth, shadows, material evaluation and temporal behavior. No documented Python hook was established here for inserting DASHR's intersection algorithm directly into either built-in renderer. Scope these as Blender-source experiments after the exporter/custom-engine work demonstrates value, rather than promising a material node port.

## Integration gates

The canonical phase numbering and task contracts live in the [fork roadmap](../../plans/dashr-porting-roadmap.md). Platform gates below complement that plan.

| Gate | Required evidence |
| --- | --- |
| Reference | Fixed Windows mesh/animation/settings and intermediate/final captures; shell-depth and known limitations documented |
| Portable resources | Shader validation, Metal/D3D12 device creation, MRT and sampler capability probes; fallback paths tested |
| Mac renderer | Apple Silicon release captures, pass timing where supported, seam stress and both distortion modes |
| Blender export | Pinned release, positions/UV metric/seam round trip and declared animation policy |
| Blender custom engine | Viewport and final render, color, lifecycle, cancellation, depth and invalidation tested |
| Optimization | Controlled before/after quality, convergence, memory and timing; rollback path retained |
| Built-in engine research | Working Cycles/EEVEE insertion point and measured benefit; maintenance cost assessed |

These are proposed gates, not evidence that a port exists. The shared asset and numerical reference work supports a portable viewer, a Mac demo or a Blender tool; the roadmap recommends the portable viewer first.
