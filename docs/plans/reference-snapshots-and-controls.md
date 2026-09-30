# Reference snapshots and native controls

Implement the requested Windows comparison export and an equivalent native Rust control panel. Preserve shader algorithms and keep GUI rendering out of comparison captures and measured renderer passes.

## Snapshot contract

The Windows exporter freezes bone/sun animation and records the exact captured frame. A clean `frame.png`, optional `overlay.png`, and versioned `manifest.json` share a timestamped directory. The manifest contains full mesh/material/marching settings, clear color, both bone clocks, and exact projection, camera, bone and sun uniforms. Convert the reference's row-major matrices to explicit columns for Rust. Check readback row pitch and report capture errors visibly.

The Mac importer accepts the manifest through `--settings`, preserves exact reference matrices/pose/sun, and exposes a deliberate control to release the frozen pose. Renderer scalar settings remain editable. Fix the known sun-elevation formula and make background/sun settings explicit. A saved image alone does not establish pixel parity.

## Control panel

Use released egui integrations compatible with wgpu 30/winit 0.30.13; pin Rust 1.95 in the isolated rustup environment. Provide debug/lighting/distortion, animation, mesh/material/atlas, marching/damping and lighting controls, atlas selection, settings save and capture. Apply expensive asset edits transactionally through an explicit rebuild button. GUI input must not move the camera or trigger application shortcuts.

## Verification

Test portable C++ serialization and Rust replay against nontrivial literal matrices; reject invalid/nonfinite inputs. Test sun direction and GUI resource/input state transitions. Run native CPU/shader checks, GPU replay checks, release build and window/interaction smoke tests. Cross-link Windows x64 and ARM64 reference executables and provide a VM bundle with launch/export/replay instructions. Real Windows readback remains an explicit external gate until exercised in the VM.
