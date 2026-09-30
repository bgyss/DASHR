# DASHR research and development packet

Dated 2026-09-29. Source snapshot: `9cf55d4c989a4ff7974dc1360e58027e739099f2`, Tom Forsyth's v1.0 demo. This packet assesses the fork and proposes future work; it does not implement or benchmark a port.

## Recommendation

Preserve the DirectX 11 reference, establish capture/profiling fixtures, then build a native Rust/wgpu renderer with a Metal backend for macOS. Add Blender asset export before a custom Blender renderer. A C++/Metal viewer is a credible alternative if Mac-only delivery or a measured wgpu constraint outweighs shared-backend maintenance.

The strongest optimization experiments are transform/teleport bandwidth, static topology caching, shader specialization and smaller inverses. Better traversal and LOD follow once seams and deformation are reproducible. Rust alone provides no GPU speedup guarantee, and compute conversion should remain a measured alternative.

## Read in this order

1. [Repository assessment](repository-assessment.md): architecture, build gaps, embedded license and numerical constraints.
2. [Optimization workstreams](optimization-workstreams.md): ranked hypotheses, calculated memory footprint and experiment gates.
3. [Platform feasibility](platform-feasibility.md): official Metal/wgpu/WGSL/Blender sources and concrete API restrictions.
4. [Benchmark protocol](benchmark-protocol.md): fixtures, capture schema, suggested thresholds and completion evidence.
5. [Fork roadmap](../../plans/dashr-porting-roadmap.md): phases, prerequisites, deliverables, stop conditions and first implementation prompt.
6. [Blender authoring plan](../../plans/blender-authoring-plan.md): asset contract and progression from exporter to optional engine work.

The follow-up [Windows cross-compilation review](windows-cross-compilation-sources.md) records executed x64/ARM64 build probes for both implementations and separate Parallels runtime gates. Its [build evidence](windows-cross-build-report.json) fingerprints the linked executables. These later probes do not change the original research-only evidence scope below.

The follow-up [wgpu attachment-budget note](wgpu-attachment-budget.md) explains requesting 64 or more, adapter ceilings, backend differences and fallback designs.

## Decisions with the largest impact

- Four RGBA32F targets need 64 attachment bytes/sample. wgpu defaults to 32; request 64 when supported. More than 64 is possible on suitable adapters, but the target Mac must be probed. Split passes before considering precision changes.
- Float32 filtering requires capability handling. Preserve manual bilinear fallback semantics where needed; do not silently select nearest sampling.
- Eight RGBA32F transform images plus two RGB32F static maps have a calculated logical payload of 9.5 MiB at 256², 152 MiB at 1024² and 2432 MiB at 4096². These are allocation calculations, not measured bandwidth.
- DASHR's basis encodes scale and skew. Standard normalized Blender/MikkTSpace tangents lose necessary information.
- The reference writes shell depth, not refined hit depth, and traces local shadows only. Broader scene integration requires new correctness work.
- The software license is embedded in the paper; dependencies/assets have separate provenance. Upstream describes the project as a research demo and does not seek PRs. Maintain the port in this fork and preserve attribution.

## Evidence and limits

Source inspection and live official API research support the assessment. No build, shader execution, port, GPU timing or Blender integration was validated. GitHub CLI authentication/API access failed, so upstream freshness was not verified. Future implementation must pin versions and prove behavior on real target hardware. Detailed sources are linked beside the claims in the individual documents; the checked-in paper remains the algorithm authority.
