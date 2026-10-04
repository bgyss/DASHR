# Blender authoring and integration plan

## Initial scope

Deliver one Blender exporter and one authored example that the standalone DASHR renderer can consume. Use a smooth closed triangulated mesh, one unique UV atlas and deterministic evaluated poses. This exposes authoring limitations without first taking on Cycles, EEVEE or native texture sharing. No add-on exists yet.

Use the [platform feasibility sources](../research/2026-09-29/platform-feasibility.md) and [roadmap P5/P7](dashr-porting-roadmap.md) for API evidence and phase dependencies. Pin one Blender release and test against its API; the research links span multiple releases and are not a compatibility guarantee.

## Proposed versioned asset contract

| Field group | Required contents | Validation |
| --- | --- | --- |
| Identity | Schema version, source mesh ID, fixture hashes, exporter/bake version | Reject unsupported versions, stale hashes and mismatched payload counts |
| Geometry | Object-space positions, triangle indices, source topology IDs, face-corner attributes | Reject zero-area triangles and ambiguous seam partners |
| Mapping | Unique atlas UVs, chart IDs and orientation, resolution and gutter budget | Reject overlaps/stacking, inconsistent winding, degenerate UV area |
| Basis | Metric tangent/bitangent, normal direction and physical thickness | Preserve skew and lengths; diagnose bad conditioning |
| Animation | Explicit evaluated-frame or indexed-skeleton mode | Never apply armature skinning twice |
| Materials | Height/albedo/normal image references, channels, decode modes and precision | Height is data; color conversion is explicit; reject missing resources |
| Height definition | Object units, midpoint, scale/offset and shell padding | Round-trip a constant height and ramp analytically |
| Seams | Stable paired edges, orientation, compatible normals and animation | Preserve geometry identity across UV splits |
| Camera | World/object/camera transforms, projection and time | Match renderer conventions and capture manifest |

Use a human-readable manifest plus explicit-width binary arrays, with endianness and byte strides documented. Keep exported identifiers/path references project-relative and distributable. Do not assume a plain glTF export preserves unique-atlas requirements, metric tangents, physical thickness or seam topology. A custom sidecar could add them, but it is a design choice to validate.

## Authoring workflow

1. Duplicate the artist's working mesh into a render/export representation. Triangulate consistently and preserve original source topology IDs before splitting face corners for UVs or normals.
2. Validate closed manifold connectivity and smooth displacement normals across seams. Report unsupported open edges, hard creases, nonmanifold edges and T-junctions with mesh element identifiers. Reject ambiguous geometry rather than welding arbitrary nearby vertices.
3. Create a dedicated unique UV map with consistent orientation and enough gutter space for the chosen atlas. Initially use it for material maps too. Mirrored/stacked material UVs, UDIMs and multiple mappings are later extensions.
4. Compute the scaled surface basis from position and UV derivatives, matching the reference generator's averaging convention for parity fixtures. Blender's normalized tangent output may help shading but cannot replace the DASHR metric basis. Normals retain displacement thickness separately from shading normals.
5. Bake or assign height/normal/albedo with explicit units and midpoint. Preserve 16-bit/float height sources in an optional quality path; keep an 8-bit reference path for parity with the demo. Label the difference.
6. Export frozen evaluated frames for a static or animated fixture, retaining required UV/color attributes and releasing temporary Blender meshes. Recompute the basis from evaluated geometry if that is the selected deformation definition. Preserve stable topology between frames; reject topology-changing modifiers initially.
7. Import into the external viewer, bake static maps and compare seam coverage, hit UV, normals and silhouettes. Save paired captures and human review notes.

Skeleton playback is a separate exporter mode: export rest geometry, joint indices/weights, inverse bind transforms and a documented object-space bone palette. General rigs require changes beyond the demo's four fixed matrices. Freeze an influence policy, test weight normalization and nonuniform scale, and distinguish skinning an existing metric basis from recomputing it on evaluated geometry; these need not produce identical fields.

## Incremental experiments

### B1: static round trip

Export one authored inflated sphere-like closed mesh with deliberately visible seams and a constant/ramp height. Compare vertex/UV payloads and analytic hits; then test a detailed height image. Acceptance: importer diagnostics work and the benchmark geometry gates pass. Export success alone is insufficient.

**Status (2026-10-03):** Blender 5.2.2 exported a checked-in radius-1.5 icosphere fixture with 80 per-face UV islands and 120 seam edges, and the independent Rust consumer baked and rendered it. Constant and ramp material roots both render. The native filtered ramp path exceeds the `1e-5` hit/depth budget. A probe-only manually bilinear height path still exceeds the gate with hardware-filtered float maps; the fully manual height and float-map path passes, but is not the production path. A CPU displaced-mesh oracle for this sphere and visual seam/silhouette review remain open.

### B2: evaluated deformation

Export a small fixed-topology bending sequence. Compare renderer outputs with the frozen evaluated geometry and establish temporal seam stability. Cache static maps by topology/UV schema; rebuild dynamic transforms per changed pose. Acceptance: the full cycle passes the protocol and no double animation occurs.

### B3: skeleton extension

Generalize joint palette and influence data using a small rig before a production character. Test rest-pose equivalence and frame-by-frame pose parity against evaluated Blender vertices. Reject unsupported modifier combinations or bake them into evaluated frames. Acceptance: indexed skinning agrees with its declared deformation contract.

### B4: external-process custom RenderEngine

Register a custom engine for a pinned Blender release, send a manifest and scene state to the standalone renderer, and return the completed image into a render result. Add render cancellation, process failure handling, resize and frame changes. Begin with one camera/asset/light and explicitly supported outputs. Acceptance: repeatable final renders, no orphan process/resource leaks, correct color handling and useful diagnostics.

### B5: interactive viewport

Implement dependency-graph dirty-state updates and asynchronous frame delivery. Separate pose, topology, material and camera invalidation. Measure latency and readback cost before attempting native GPU texture exchange. Device/context sharing with Rust/wgpu is an unresolved integration question, not an existing Blender Python capability.

### B6: built-in engine spike

Only after an authored demonstration establishes value, inspect a pinned Blender source revision for either Cycles intersection integration or EEVEE pass ownership. Choose one. Specify visibility for primary, shadow and reflection rays, hit depth, materials and deformation bounds. Require a small reproducible prototype and compare maintenance/quality/cost with native displacement. A shader node, Geometry Nodes mesh approximation or Python custom engine does not prove a new Cycles primitive.

## Additions from upstream future work and the library goal

- **Lone edges and thin sheets** (clothing): Tom plans cliff-edge geometry; the exporter should detect open boundaries and report them now, and support them in P13 of the [library plan](standalone-library-and-integrations.md). See [upstream future work](upstream-future-work.md) L2.
- **Metric tangents:** export scaled tangent/bitangent lengths, never MikkTSpace-normalised vectors; an optional normalised tangent channel for Blender/engine shading is separate.
- **Seam continuity checks:** compare albedo and height across paired seam edges and name offending charts (L11).
- **Tornado/tunnel diagnostics:** report high-distortion and self-folding regions by face/chart (U1, U5).
- **Shared schema:** Unity and Unreal importers (P11/P12) reuse this contract, so version it independently of Blender.
- **Packaging:** ship as a Blender 4.2+ extension with package contents at the ZIP root, relative imports only and the archive layout verified before release.

## Review checklist

A successful authoring tool must answer what the artist needs to change: atlas overlap, inadequate gutters, unsupported edge topology, discontinuous displacement, excessive bend/thickness or an ill-conditioned basis. Numeric diagnostics should name the affected vertices/faces/charts. Do not hide tornado poles with a step cap or silently fall back to a plausible image and label it supported.

Keep a high-resolution displaced mesh in Blender as a quality reference, with explicitly matched settings. It is a comparison asset, not the DASHR implementation. GPU and human seam/silhouette review remain required before promoting a fixture.
