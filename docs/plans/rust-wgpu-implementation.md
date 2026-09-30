# Rust/wgpu implementation plan

**Goal:** Port the native demonstration alongside the preserved D3D11 source, following P1–P4 of the [roadmap](dashr-porting-roadmap.md). P0 Windows execution, cross-platform parity, reviewed animation and P5–P7 remain explicit evidence gates or later integrations.

**Architecture:** Device-independent procedural assets, topology preparation and a double-precision math oracle feed explicit wgpu resources. WGSL preserves the deformation → gutter/distortion → shell/trace pass order. A winit viewer and deterministic offscreen capture share the renderer.

**Constraints:** Full-float warp planes; metric tangents and shell-thickness normals; fixed four-bone skinning; both distortion encodings; filtered seam distance and nearest seam destination; clamp sampling; original height decoding and lighting. No storage compression or traversal optimization. Reject unsupported resources/topology and record failures. Native GPU evidence never promotes Windows parity or browser support.

## Tasks

- [x] Asset and math contracts: port all four procedural modes, angle-weighted metric basis, original poses/material preprocessing, conditioned double-precision oracle and explicit uniform offsets. Test known vertex/triangle counts, basis scale, singular inputs, points/directions and anchor correction.
- [x] Portable resources and shaders: request the actual attachment budget and filterability, retain manual bilinear and split-pass fallbacks. Port deformation, gutter correction, stepping/damping/teleport/interpolation, lighting and independent primary/shadow termination. Validate every shader variant and perform real device round trips.
- [x] Topology bake: use actual GPU occupancy; deterministic coincident edge partners, reference signed seam raster and flood propagation. Test closed seams, unsupported boundaries, edge sign and coverage. Upload full-float static maps.
- [x] Viewer and capture: interactive camera/animation/settings, reverse-Z shell compatibility plus selectable actual-hit depth, headless images/maps/status and manifest. Exercise all meshes, both encodings and fallback paths on available hardware. Record unavailable validation honestly.
- [x] Documentation and review: exact build/run/capture commands, provenance, dependency lock, phase status and remaining gates. Run format, full tests, lint, release capture and independent code review; resolve substantive findings.

## Review focus

- Atlas texels on triangle boundaries and gutters must match GPU occupancy and sample addresses.
- Compressed/pinched bases must terminate visibly, never silently become a hit.
- Resource fallback pipelines must preserve all four planes and coverage.
- Missing assets, invalid sizes and device failures must give actionable errors.
- Near clipping and camera-inside-shell captures must be labeled unsupported until separately proved.

The current user request authorizes implementation of the supplied roadmap. It supersedes the research-only scope note at its end. Work proceeds in the existing isolated checkout; no commit, push, publication or optimization claim is implied.

Implementation checklist verified on the available Mac. This does not promote the unresolved platform/parity/scene gates; see [the validation record](../port/validation.md).
