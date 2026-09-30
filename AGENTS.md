# Repository Guidelines

## Project Structure & Module Organization

- `src/` contains the Rust library and CLI. Keep asset/topology preparation independent of device resources, render passes, capture and viewer code.
- `shaders/` contains shared WGSL plus deformation, gutter/distortion and tracing passes.
- `tests/` contains Rust integration tests; `fixtures/` holds deterministic tube/cube settings.
- `demo/` preserves the original Windows/D3D11 reference and material assets; `paper/` preserves Tom Forsyth's paper and illustrations.
- `docs/plans/` holds roadmaps; `docs/port/` documents operation and evidence gates. Generated `out/`, `target/` and `.dev/` directories are ignored.

## Build, Test, and Development Commands

Run from the repository root using Nix/mise and the pinned rustup toolchain:

- `scripts/in-nix mise run setup`: install Rust 1.95.0 and fetch locked dependencies into checkout-local state.
- `scripts/in-nix mise run check`: check formatting, run ordinary tests and enforce clippy without warnings.
- `scripts/in-nix mise run gpu-test`: run explicitly ignored tests on a real native GPU.
- `scripts/in-nix mise run build`: build the release executable.
- `scripts/in-nix mise run view`: launch the interactive viewer.
- `scripts/in-nix mise run probe`: save capability and numerical reports under `out/probe`.

Use `mise run fmt` inside the Nix shell to apply rustfmt. Keep toolchain/dependency changes explicit and update corresponding lockfiles.

## Coding Style & Naming Conventions

Use Rust 2024, rustfmt's four-space indentation, `snake_case` functions/modules, `PascalCase` types and `SCREAMING_SNAKE_CASE` constants. Keep modules focused. Document WGSL binding locations, matrix conventions and packed uniform fields; validate shader variants through Naga tests.

## Testing Guidelines

Use descriptive `snake_case` test names and Rust's built-in test framework. For focused runs: `scripts/in-nix cargo test --locked --test topology`. Add regression cases for changed math, seam transport, resource fallbacks or termination states. No numeric coverage threshold is established. Record GPU adapter/backend and fixed settings; CPU tests cannot establish rendering parity.

## Commit & Pull Request Guidelines

History mixes release labels and descriptive subjects, including `docs: assess DASHR optimization and platform porting plans`. Use concise imperative subjects; type prefixes are optional. Keep commits focused. PRs should explain behavior, link relevant issues, list executed checks and identify unverified platforms. For rendering changes, include comparable captures/settings and diagnostic results.

## Renderer Contracts & Evidence

Preserve reference attribution, metric tangent lengths, full-float baseline and host/shader layouts. Filter seam distance; load destination UV nearest. Follow [validation gates](docs/port/validation.md); native success does not prove D3D11 parity, scene readiness or speedups. Keep shareable documentation machine-neutral.
