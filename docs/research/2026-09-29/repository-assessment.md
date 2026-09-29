# DASHR repository assessment

Assessed 2026-09-29 at commit `9cf55d4c989a4ff7974dc1360e58027e739099f2` (v1.0). This is a source audit of this fork, not a performance evaluation. No executable, shader compilation, Blender session, or GPU capture was run.

## Conclusion

Preserve Tom Forsyth's demonstration as the reference and build a portable experimental renderer beside it. The highest-value first work is reproducibility, correctness fixtures and profiling. Rust/wgpu is a reasonable shared renderer candidate for macOS and other native platforms; native Metal is a useful alternative or diagnostic backend. Blender should first supply validated assets and animation, then optionally host a preview. Changing Cycles or EEVEE is a separate engine project.

The distinctive capability is the animated mapping between object space and surface space, including seam teleports and deformation damping. Replacing only the heightfield tracer does not recreate DASHR. See the local [paper overview](../../../paper/DASHR_Paper.html#overview-of-the-full-process), [limitations](../../../paper/DASHR_Paper.html#limitations-and-subtleties), and [future work](../../../paper/DASHR_Paper.html#future-work).

## What is here

The clean starting checkout contains 53 tracked files. There are 3,506 lines of application C++, two support headers and seven HLSL files; the substantial algorithm description is an HTML paper with illustrations. There is a Windows executable and Visual Studio solution, but no portable build system, tests, CI, dependency lockfiles or existing research directory. The `.blend` strings in asset paths name directories containing image textures, not included Blender scene files.

| Component | Source entry point | Role and consequence |
| --- | --- | --- |
| Platform/application | [main.cpp](../../../demo/main.cpp), `main`, `CreateDeviceD3D` | Win32, D3D11, Dear ImGui, input, GUI and resources in one file |
| Procedural assets | `CreateModel`, `GenerateTangentSpace` | Tubes and inflated cubes; no general mesh importer |
| Texture preparation | `CreateTextureAndNormalMapFromImage`, `CreateTextures` | Byte image decoding, contrast/repetition transforms, CPU normal baking |
| Topology bake | `CreateTeleportEdgefill` | GPU occupancy raster/readback followed by CPU seam matching, propagation, upload |
| Deformation | [PipelineDeform.hlsl](../../../demo/PipelineDeform.hlsl), `vs_main` | Skins basis and position, inverts transform, UV-space rasterization into four MRTs |
| Gutter/distortion | [PipelineEdgefill.hlsl](../../../demo/PipelineEdgefill.hlsl), `ps_main` | Copies valid transforms outward, computes deformation ratios and position correction |
| Surface rendering | [PipelineMain.hlsl](../../../demo/PipelineMain.hlsl), `TraceRay` | Rasterized extruded shell launches primary and local shadow traces |
| Shared math/layout | [Utils.hlsl](../../../demo/Utils.hlsl), [ConstantBuffer.hlsl](../../../demo/ConstantBuffer.hlsl), [VertexInput.hlsl](../../../demo/VertexInput.hlsl) | Inversion, four-bone skinning, CPU/GPU layout contract |

### Frame and asset lifecycle

Asset/configuration changes rebuild geometry or textures and regenerate static teleport/edgefill maps. Their generation includes a staging copy and blocking CPU readback (`main.cpp:2923` onward), but this is not a steady-state per-frame readback.

Each regular frame updates bones, clears targets, rasterizes deformation in UV space, performs the fullscreen edgefill/distortion pass, rasterizes the shell and runs the heightfield shader, optionally draws wireframes and GUI, then presents. Keep asset-bake time separate from frame time. The similarly named simulation object count is not evidence that the demo renders a scene of independently animated DASHR assets.

## Build and provenance gaps

- `skinnedheightfield.vcxproj` x64 configurations reference sibling ImGui, stb, tinyexr and MyInc directories. ImGui compilation units and stb are absent from the checkout. The two yak headers are included locally by `main.cpp`, but project header entries refer to MyInc. Win32 configurations have different include paths/toolsets. Reconcile these before claiming a reproducible Windows build; do not infer all listed directories are required runtime libraries.
- x64 uses v143, Win32 v141; shader compilation is runtime `D3DCompileFromFile` using `vs_5_0` and `ps_5_0`. Launch paths must find both shaders and assets. Shader filenames are set by `CreateShaders`.
- `yak_shiv_misc.h` uses MSVC intrinsics, `__int64`, Windows performance counters and `long` aliases for 32-bit types. macOS LP64 makes `long` 64-bit: replace these with explicit-width types before retaining C++ layouts. A C++ Metal port requires more than swapping the graphics API.
- License is present in the paper, even though a root LICENSE file is absent: [license section](../../../paper/DASHR_Paper.html#license-and-misc) offers MIT-0 or Unlicense for the software. External libraries and assets need their own provenance. Preserve Tom's credit and the paper; inventory dependency/asset terms before distributing a packaged fork.
- The paper explicitly describes a research demo and says upstream is not seeking pull requests. Treat the roadmap as fork work. Do not promise upstream maintenance or submit contributions on the author's behalf.

GitHub CLI authentication was invalid and its API connection failed in this session. No live upstream synchronization or issue audit was performed; the snapshot above is the local source of truth. Official platform documentation was researched separately.

## Correctness boundaries that govern every port

1. A unique, nonoverlapping UV parameterization with gutters is fundamental to the existing implementation. Mirrored/stacked material UVs cannot substitute for it. UV winding must be consistent.
2. Seam partners are topologically split but geometrically coincident edges. Current support assumes two incident triangles and smooth displacement normals. Open boundaries, hard creases, T-junctions and nonmanifold topology need explicit rejection or new construction rules.
3. Tangent and bitangent encode metric scale and may be nonorthogonal. The normal length encodes shell thickness. Normalizing these into a conventional TBN basis loses the mapping. `GenerateTangentSpace` explains this directly.
4. `Animate` uses four fixed bone matrices and four weights, without per-vertex joint indices. A real asset pipeline must generalize the palette deliberately or initially reject unsupported rigs.
5. `DistortionMode=0` stores affine offset; mode 1 stores inverse basis plus anchored object position and includes gutter position correction. These are distinct representations, not interchangeable packing modes.
6. Teleport signed distance is linearly sampled, destination UV is point sampled. Filtering the destination across seam discontinuities is invalid.
7. `TraceRay` moves in object space, estimates surface location, applies damping and performs optional teleport correction. Its height distance is not a global object-space signed distance field. Sphere-tracing rules are not automatically valid here.
8. The marcher has a hard 10,000-step guard; debug `MaxSteps` forces a hit at the chosen count. That debug behavior is not an acceptable production termination policy.
9. `ps_main` outputs color only. Reverse-Z depth comes from the extruded shell, not the refined hit. Multi-object occlusion, camera-inside-shell behavior, transparency, picking and scene depth integration require additional validation/design.
10. Local shadow traces leave the skin envelope; they do not implement all scene or long-range self-shadowing. The paper recommends combining this with conventional shadows.
11. Generic image loading uses `stbi_load` and RGBA8 UNORM, even for displacement PNGs. All material textures here have one mip. A new high-precision import path changes reference inputs and must be tested separately from translation parity.
12. Singular UV triangles, nearly singular animated bases, severe folds and pinched poles can break the representation. The general inverse divides by the determinant without a conditioning check; the paper describes tornado artifacts. A port should diagnose and abstain on unsupported cases rather than silently claim a fix.

## Evidence status

Verified: file inventory, source paths, pass order, resource formats, defaults, build declarations and paper contents. Calculated: texture storage estimates in the optimization document. Proposed: experiments, backend designs and acceptance thresholds. Unverified: build success, visual equivalence, speed, general production-mesh support and actual platform feature availability. No recommendation here carries a measured speedup claim.

For explicit resource binding, note that the main pass binds the linear sampler while the point sampler remains set from the preceding edgefill pass. A new backend should bind both deliberately; depending on inherited D3D11 state obscures the sampler contract.
