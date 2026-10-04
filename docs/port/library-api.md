# Standalone Rust library API

The `dashr` package now builds as a headless Rust library by default. Its default feature set does not enable `winit`, `egui`, or CLI dependencies. Enable the `viewer` feature only when building the bundled command line and windowed viewer.

## Cargo dependency

For an application that uses a local checkout:

```toml
[dependencies]
dashr = { path = "../DASHR", default-features = false }
```

The crate exposes `dashr::api::API_VERSION` and a versioned `AssetDocument` JSON schema. Build an `AssetDocument` from a validated mesh or load it from a JSON file. The current schema supports the reference's four-bone vertex layout, triangle indices, and metric tangent basis; material selection remains in `Settings`.

The crate is pre-1.0. Patch releases preserve the public Rust API; an incompatible Rust API change increments the minor version. At 1.0, Rust API changes follow normal semantic-versioning rules. Asset and bake-cache schema versions are independent and must be incremented when their serialized contracts change.

## Headless lifecycle

`DashrSession::headless` creates a standalone wgpu context from a `Settings` value. `DashrSession::from_asset` uses a validated `AssetDocument`. Applications that already created a context can transfer it to `DashrSession::with_context`.

```rust
use dashr::{
    api::{DashrSession, GpuOptions},
    asset::MeshKind,
    settings::Settings,
};

let mut settings = Settings::default();
settings.mesh = MeshKind::Tube;
let mut session = DashrSession::headless(settings, "demo/assets", GpuOptions::default()).await?;
session.update_pose(0.4, 1.0)?;
let frame = session.render()?;
```

The application owns the session. It owns its wgpu context, render targets, and pass resources, and releasing the session releases those resources. `resize` validates the new dimensions before rebuilding frame targets. Device creation, asset, settings, and render failures are returned as `anyhow::Result`; the API does not hide device loss or invalid trace states.

Set `GpuOptions::map_cache_path` to persist static topology/seam maps. The cache key includes the mesh document, atlas size, and GPU raster occupancy; incompatible or corrupted cache files return errors instead of being reused. Increase `CACHE_SCHEMA_VERSION` when the bake algorithm or serialized map meaning changes. Cache files use JSON and can be large; the cache is opt-in.

## Outputs and diagnostics

`FrameOutput` returns the color plane, hit UV/ray-distance/actual reverse-Z depth plane, and separate primary and local-shadow diagnostics. A primary `Hit` is a visible local surface hit. For a launched local shadow ray, `Hit` means a local occluder was found and `Escaped` means the ray left DASHR's local skin volume without one. `NotLaunched`, `BudgetExhausted`, and `InvalidBasis` remain distinct so a host never has to treat an unknown shadow result as lit. Each `TraceDiagnostic` also contains step and teleport counts plus an auxiliary value (hit height for primary rays or local-shadow distance for shadow rays).

The public v1 status mapping preserves the shader codes:

| Code | Status |
| ---: | --- |
| 0 | `NotLaunched` |
| 1 | `Hit` |
| 2 | `Escaped` |
| 3 | `BudgetExhausted` |
| 4 | `InvalidBasis` |
| 5 | `DebugForcedHit` |

Unknown status codes and non-finite/non-integral diagnostic channels return an error. A budget exit remains distinct from a hit.

## Current input and device limits

- The mesh schema stores position, UV-height, four bone weights, normal and metric tangent/bitangent per vertex. Weights must be finite, nonnegative, and sum to one.
- `AssetDocument::validate()` checks schema and triangle-record structure. `validate_pose_topology()` additionally rejects T-junctions, rest-pose self-intersections (including positive-area coplanar overlaps), indexed edges with more than two incident triangles, and unpaired or nonmanifold seam edges with element IDs before map baking. Later animation folds remain trace/authoring diagnostics under U5. Open boundaries and thin sheets remain unsupported pending a P13 boundary/cliff-edge contract.
- Material images are loaded from the supplied material root using `Settings::texture_set`. A self-contained versioned material package is a later asset-schema step.
- The renderer owns or receives a wgpu context and defaults to full-float warp maps. Adapter limits and selected filtering/split policy are available in `adapter_report()`.
- `update_pose` exposes the current four-bone reference animation. A general indexed joint palette and native Unity/Unreal GPU resource sharing are not part of this P8 API.
- `asset_diagnostics::find_high_distortion_faces` reports zero-based triangle face IDs, deterministic edge-connected chart component IDs, condition numbers and maximum metric scale. The caller supplies an anisotropy threshold greater than one; the report does not normalize tangents or cap ray steps.
- `asset_diagnostics::suggest_seam_cut_for_hotspot` accepts a runtime hotspot UV and the matching row-major RGBA32F edgefill map. It bilinearly resolves the hotspot to a source face and returns the face patch to isolate, the complete set of shared edges to split, and existing seam pairs that anchor the patch. The edge list separates the reported patch in the face-adjacency graph. The result is an authoring suggestion: it does not modify the mesh, re-unwrap UVs, or rebake materials; callers must make those edits and verify the new asset before rendering.
- `asset_diagnostics::find_self_intersections` applies a supplied four-bone pose and reports non-coplanar crossings and positive-area coplanar overlap with face and chart-component IDs, including pairs that share geometric vertices or edges. Point-only and edge-only contact is not reported as overlap. Continuous collision over an animation interval remains outside this first diagnostic.
- `asset_diagnostics::find_orientation_flips` reports posed triangle winding reversals and zero-area collapses with face/chart IDs and signed area ratios. It does not establish that the full surface-space mapping is globally injective.
- `Settings::epsilon_policy` defaults to `Reference`, preserving the existing UV delta and shadow bias. `ScaleDerived` uses `1 / atlas_size` for normal finite differences and `0.001 * max(length, 2 * radius, thickness)` object units for local-shadow bias. Captures record both resolved values; the derived profile remains experimental until its shadow and normal quality gates pass.
- `Settings::specialized_inverse` opts into the U15 3x3 cofactor inverse. It is disabled by default; see [U15 validation](validation.md#u15-specialized-inverse-experiment) before choosing it for a workload.
- `Settings::hit_refinement`, `seam_aware_stepping` and `adaptive_steps` opt into the U2-U4 traversal experiments. They are disabled by default and are recorded in settings/capture manifests. U2 uses explicit bilinear height texel loads inside hit-bisection iterations; ordinary trace steps retain the adapter sampler. See the [U1-U4 evidence](validation.md#u1-u4-traversal-experiments) before relying on them for production.
- `Settings::stored_inverse`, `compute_edgefill`, `indirect_edgefill`, `split_teleport` and `compact_warp` are isolated U7-U14 experiments, all disabled by default. Some combinations are rejected because their trace/resource contracts conflict. Their per-fixture results and remaining gates are in the [validation record](validation.md#u7-stored-inverse-atlas-experiment).

## Independent consumer

The repository includes a separate Cargo package that depends on the public library with default features disabled. Run it on a native GPU with:

```sh
scripts/in-nix cargo run --locked --manifest-path examples/standalone-consumer/Cargo.toml
```

It constructs and validates a mesh document, bakes the maps, advances the pose, renders a fixture, and reads primary/shadow statuses without opening a window or copying library implementation files. Pass a versioned asset JSON as the final argument to load that document instead:

```sh
scripts/in-nix cargo run --locked --manifest-path examples/standalone-consumer/Cargo.toml -- \
  examples/standalone-consumer/assets/torus-v1.json
```

For the Blender B1 icosphere, generate the constant/ramp material roots and select one explicitly:

```sh
python3 scripts/generate_b1_materials.py --output-dir out/b1-materials
scripts/in-nix cargo run --locked --manifest-path examples/standalone-consumer/Cargo.toml -- \
  --asset examples/standalone-consumer/assets/blender-sphere-v1.json \
  --assets out/b1-materials/constant --texture-set 2 \
  --size 256 --capture out/b1-sphere-constant.bmp
```

Use `out/b1-materials/ramp` for the ramp image. The consumer also retains the original positional asset argument and defaults to `demo/assets` when `--assets` is omitted.

The sample torus and Blender-exported icosphere exercise external `AssetDocument` v1 ingestion. The consumer accepts an optional `--assets <directory>` material root, `--texture-set <0..3>`, square `--size <64..1024>` (default 64), and `--capture <path.bmp>`. Captures are 24-bit bottom-up BMP files made from clamped shader RGB bytes without sRGB conversion; they are for visual inspection and do not establish parity. `scripts/generate_b1_materials.py` creates constant and ramp roots for the Blender sphere example. The mesh file is not a self-contained material package. These are local integration examples; they do not certify Windows/D3D11 parity or engine integration.

The CLI and interactive viewer are opt-in clients:

```sh
scripts/in-nix cargo run --locked --release --features viewer -- view
```

## Optional C ABI

Build the headless shared library with:

```sh
scripts/in-nix cargo build --locked --no-default-features --features ffi --lib
```

The checked-in [`dashr.h`](../../include/dashr.h) is generated from `src/ffi.rs` with pinned cbindgen 0.29.4. Regenerate it with `scripts/ffi-header`; `scripts/ffi-header --check` verifies the checked-in copy. It defines ABI version 1, opaque serialized session handles, status/trace PODs, borrowed frame views, thread-local error retrieval, and a per-thread log callback. Log messages carry a level and temporary NUL-terminated UTF-8 text; user data remains owned by the host and must stay valid until unregister. Callbacks run synchronously and must not unwind or re-enter DASHR. A null asset JSON uses the procedural mesh selected by settings; a non-null string loads the versioned `AssetDocument`. The library owns its wgpu device and copies frame outputs to CPU-readable arrays; this ABI does not share textures or synchronization handles with an engine device.

Handle calls must be serialized on the thread that owns the session. Frame pointers remain valid only until the next update/render/destroy call for that handle. Functions translate errors and caught Rust panics into null or `-1` results plus `dashr_last_error_message()`; callers must destroy each non-null handle exactly once.

Run `scripts/in-nix mise run ffi-smoke` to build the release cdylib, compile the native client with `-Wall -Wextra -Werror`, and render a frame on a native GPU. The C smoke client at [`examples/c-consumer/main.c`](../../examples/c-consumer/main.c) registers a logger, creates a headless session, updates pose, renders and reads primary statuses. Rust unit coverage verifies error delivery to the callback. This proves the callback and ABI boundary on the tested native host only; Unity/Unreal engine-owned resources and target shader compilation remain open under P9.
