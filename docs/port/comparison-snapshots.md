# Windows reference snapshots and Mac controls

The modified C++ demo exports a clean D3D11 frame and exact replay settings. The Rust viewer adds an egui control panel. Rendering parity remains a comparison task; the panel does not reproduce Dear ImGui's visual styling or wireframe overlays.

## Export from Windows

Use the newly built reference executable. The local VM package is `out/windows-comparison/vm-drop/`; it includes ARM64 and x64 builds, HLSL and material assets. Launch from its shader directory:

```powershell
Push-Location .\reference
.\skinnedheightfield-arm64.exe
Pop-Location
```

Choose the desired camera, mesh, texture and lighting, then press **F9** or **Comparison snapshot**. This freezes both bone clocks and the sun, and saves the next rendered frame in a timestamped directory under `reference/out/comparison/`. The panel's output-folder field or `DASHR_COMPARISON_OUTPUT` environment variable selects another root.

Each directory contains:

- `frame.png`: main-pass image before wireframe or GUI drawing.
- `overlay.png`: optional image including wireframe and GUI, for visual context.
- `manifest.json`: versioned `dashr.reference.snapshot.v1` settings, adapter/feature level, background, both animation clocks, exact camera/projection/bone/sun uniforms and sampler/color conventions.

Zip and attach that whole directory for comparison. The PNG plus manifest are necessary for a reproducible replay. Readback honors D3D row pitch; PNG encoding uses Windows WIC. Errors appear in the panel and stderr; failed manifests are rejected by Rust. Alpha blending and disabling topology flood fill are unsupported replay modes.

## Replay on Mac

From the repository root:

```sh
scripts/in-nix cargo run --locked --release -- capture \
  --settings path/to/manifest.json --out out/reference-replay
scripts/in-nix cargo run --locked --release -- view \
  --settings path/to/manifest.json
```

The capture uses the reference dimensions and matrix columns, including the exact second bone clock's pose. Explicit width/height overrides adjust horizontal projection for the new aspect ratio. The interactive window also adjusts aspect on resize; use offscreen capture for an exact-size comparison.

The panel's **Load snapshot / settings** field accepts the manifest. **Release reference pose** switches to editable camera/animation/sun controls. Applying mesh/atlas changes releases the pose. GUI input cannot orbit the camera or activate shortcuts. Sliders preserve imported values outside their displayed ranges until edited. **Capture clean frame** freezes animation and sun and writes a timestamped Rust capture under `out/`. **Save default settings** writes `out/viewer-settings.json`.

The sun-elevation formula now matches the C++ demo: normalize `((1-h)*sin(angle), h, (1-h)*cos(angle))`. At `h=0.5`, elevation is 45 degrees. Imported exact uniforms override derived defaults.

## Verification and remaining limits

The isolated environment pins Rust 1.95.0, egui 0.36.2, wgpu 30.0.1 and winit 0.30.13. Portable C++ serialization/Rust replay tests cover matrix transposition, escaped metadata, failed manifests and nonfinite values. Native Metal tests cover frozen uniform replay, background compositing and the existing full-float/affine/trace contracts. Native window creation and three overlay frames pass; interactive visual review remains open.

The revised inverse/damping helpers use static scalar accesses so FXC no longer forces ray-loop unrolling. Both Rust and C++ compile/link for Windows ARM64 and x64 on this Mac. In the running Windows 11 VM, the rebuilt Rust probe progresses beyond the reported FXC errors but loses the DX12 device during texture readback. The adapter reports **Microsoft Basic Render Driver**, `Cpu`, driver `10.0.22621.5415`. ARM64, x64, manual-filter/single-target and a 32×32 analytic capture all reproduce device loss. Its cause remains unresolved; no Windows rendering success or acceleration is claimed. Microsoft documents WARP as a software renderer and describes historical device-removal bugs; those facts alone do not diagnose this failure ([WARP guide](https://learn.microsoft.com/en-us/windows/win32/direct3darticles/directx-warp), [WARP release discussion](https://devblogs.microsoft.com/directx/announcing-warp-preview-with-shader-model-6-7-support/)).

A separate compile-only ARM64 diagnostic created deformation, gutter and trace pipelines in Windows without submitting GPU work. All three compiled successfully, directly verifying that FXC accepts the revised trace shader. This isolates shader compilation success from the unresolved device loss.

The new C++ WIC exporter still requires execution in the VM. External captures, human seam/animation review, material-decoder differences and D3D11/Metal parity remain open gates. Local builds and packages are ignored artifacts, not checked-in reference truth.
