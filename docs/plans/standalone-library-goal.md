# P8 goal: standalone DASHR library consumption

## Objective and prerequisites

Extract a reusable headless library from the current Rust/wgpu port as the highest development priority. Follow the [library architecture](standalone-library-and-integrations.md), [roadmap](dashr-porting-roadmap.md), [upstream item register](upstream-future-work.md) and [research conclusions](../research/2026-10-02-library-first.md).

Read AGENTS.md and the current [validation record](../port/validation.md) first. Record the source revision and deterministic fixture settings. P8 may start before unresolved P0-P4 evidence is closed; P5-P7 are not prerequisites. Do not promote parity on extraction evidence alone.

## Work and artifacts

- Separate device-independent asset validation, topology/seam preparation and CPU math from GPU resources/passes and from viewer, CLI and capture clients. Choose crate separation or feature boundaries based on a dependency audit; do not rename crates solely to satisfy the plan.
- Define public asset, bake, pose-update, tracing/output and diagnostics contracts, explicit resource ownership, thread/device constraints, error behavior and schema/cache versions. Preserve host/shader layouts and sampler semantics.
- Provide an independent minimal headless consumer with its own manifest, depending only on the public library interface. Document build/run steps, API lifecycle, supported input/resource limits and unsupported cases in docs/port.
- Make the current viewer and capture clients use that interface. Preserve a full-float reference path; retain attribution and provenance. Define semantic versioning and local packaging checks without publishing.

Exclude algorithm improvements, compressed storage, new topology, engine plugins and package publication from extraction. Track them under their existing milestones and U IDs.

## Acceptance and validation

1. The independent consumer builds without copied implementation files, opens no window and can validate/bake an asset, update its pose, render a deterministic fixture and inspect primary/shadow termination diagnostics.
2. The core dependency graph contains no winit/egui or viewer/UI dependencies; CPU preparation remains callable without GPU creation. Optional client features must not activate by default for core consumers.
3. Existing viewer and headless fixture behavior is preserved against captures taken before extraction on the same adapter/settings. Compare images, atlas planes and termination states under the existing benchmark protocol; record adapter/backend and source hashes.
4. Document and exercise asset/context creation, reuse and teardown, malformed input and unsupported resource failures. No global viewer state or hidden device ownership may be required by consumers.
5. Run `scripts/in-nix mise run check`, `scripts/in-nix mise run gpu-test` on a real native GPU, and `scripts/in-nix mise run build`. Validate the consumer from its own manifest using locked dependencies and the pinned toolchain. Record exact commands and results, including consumer packaging/dependency checks. A blocked GPU run leaves rendering acceptance open.

Stop and isolate any changed output; do not repair it by loosening tolerances or hiding termination failures. An unavailable Windows executor leaves Windows parity open. Host-owned engine resources, C ABI and shader translation remain P9 gates; a passing headless consumer does not establish Unity, Unreal or Blender integration.

## Copy-ready execution prompt

> Implement P8 using this goal and the linked canonical plans. Prioritize an independently consumable, UI-free standalone library, preserve the full-float reference and all sampler/basis/layout contracts, and migrate existing clients without algorithm changes. Deliver the external consumer, public lifecycle/error documentation, dependency and local packaging evidence, and paired fixture results. Keep unavailable device/platform/human review gates explicit. Do not publish packages, contact upstream or implement engine integrations during extraction. This goal is a future implementation contract; execution requires an implementation request. The current 2026-10-02 request authorizes planning documents and a local commit only.
