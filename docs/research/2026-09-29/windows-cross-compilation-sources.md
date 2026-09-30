# Windows cross-compilation and Parallels feasibility

Research date: 2026-09-29. Scope: Apple Silicon macOS host, Windows x64/Arm target candidates, the preserved C++/D3D11 reference and the Rust/wgpu 30.0.1 port. This note records source review and live primary-source research. Build probes and VM execution are separate evidence; no Windows execution, GPU capture or performance result is claimed here.

## Executed Mac build probes

All four executables were compiled and linked on the Mac using the repository's Rust 1.94.1/Nix environment, temporary tools from its locked Nixpkgs input, cargo-xwin 0.23.1 and LLVM clang/lld 21.1.8. PE headers were independently inspected with `llvm-readobj`.

| Implementation | x64 MSVC | ARM64 MSVC | Scope |
| --- | --- | --- | --- |
| Rust/wgpu | Linked, AMD64 PE | Linked, ARM64 PE | Current Rust source unchanged |
| Original C++/D3D11 | Linked, AMD64 PE | Linked, ARM64 PE | Temporary syntax-only compatibility patch plus pinned external dependencies |
| VM rendering | Not executed | Not executed | Parallels guest process/adapter/render gates remain open |

The SDK cache selected Windows SDK `10.0.26100` and MSVC `14.44.17.14` packages from the Visual Studio 17 manifest. This exploratory download used the manifest default; persistent build tasks should pin these inputs explicitly.

The original source did **not** compile untouched under clang-cl: `float [[nodiscard]] SecondsSince(...)` is rejected as an attribute applied to a type. In an ignored temporary copy, moving the attribute to `[[nodiscard]] float SecondsSince(...)` allowed compilation. This changes a compile-time diagnostic annotation, not renderer math or shaders. The tracked `demo/` tree remains unchanged. A Microsoft-compatible `goto` extension emitted a warning and was accepted.

An ImGui 1.91.9b trial lacked `ImGuiStyle::FontScaleDpi`. The successful dependency is **ImGui v1.92.5**, commit `6d910d5487d11ca567b61c7824b0c78c569d62f0`, plus stb `f0569113c93ad095470c54bf34a17b36646bbbb5`. All dependencies/tools/caches are local under ignored `.dev/` or immutable Nix store paths.

Rust outputs are `target/{x86_64,aarch64}-pc-windows-msvc/release/dashr.exe`. C++ outputs are `out/windows-probe/original-{x86_64,aarch64}/skinnedheightfield.exe`. The checked-in [build evidence](windows-cross-build-report.json) records their sizes and SHA256 values; the local copy is `out/windows-probe/build-report.json`.

The VM-ready folder is `out/windows-probe/vm-drop/`, also packaged as `out/windows-probe/dashr-windows-probe.zip`. It includes both architectures, unmodified HLSL/assets, full tube/cube settings and `RUN-IN-WINDOWS.txt`. The original shipped x64 executable is included under a distinct name; it is not source-build evidence. DLL import inspection found `VCRUNTIME140.dll` and Windows UCRT/API dependencies; install the matching Microsoft Visual C++ Redistributable if absent.

### Reproduce the Rust cross-build

The committed flake currently does not include cargo-xwin/lld or Windows mise tasks. The tested temporary environment uses the same locked Nixpkgs input:

```sh
scripts/in-nix rustup target add x86_64-pc-windows-msvc aarch64-pc-windows-msvc
scripts/in-nix nix shell --impure --expr \
  'let f = builtins.getFlake (toString ./.dev/nix-shell); p = f.inputs.nixpkgs.legacyPackages.aarch64-darwin; in [ p.cargo-xwin p.llvmPackages.lld ]' \
  --command bash -c 'export XWIN_CACHE_DIR="$PWD/.dev/xwin"; unset CC CXX; cargo xwin build --locked --release --target aarch64-pc-windows-msvc'
```

Use `x86_64-pc-windows-msvc` for the other target. For C++, use the temporary compatibility copy, compile all ImGui core and Win32/DX11 backend units with clang-cl and the matching CRT/SDK include roots, and link with lld-link and the Windows import libraries. clang-cl requires `--` before macOS absolute input filenames to prevent an absolute filename being parsed as its `/U` option; keep include/output options before that separator.

## Assessment

Cross-compiling Windows executables on this Mac is a credible build workflow. `cargo-xwin` documents macOS use and both Windows MSVC architectures; standalone `xwin` provides the Microsoft headers/libraries for LLVM C++ builds. The original D3D11 demo is the more direct Parallels hardware-rendering experiment. The Rust port selects wgpu `PRIMARY`, which supplies DX12/Vulkan on native Windows and excludes GL. Parallels documents DirectX 11 support, so accelerated execution of that current Rust path remains unestablished. [cargo-xwin maintainer documentation](https://docs.rs/crate/cargo-xwin/0.23.1), [xwin maintainer documentation](https://docs.rs/xwin/0.10.0/xwin/), [wgpu backend definitions](https://docs.rs/wgpu/30.0.1/wgpu/struct.Backends.html), [Parallels graphics support](https://www.parallels.com/games/)

A successful `.exe` link establishes build feasibility only. Guest process startup, adapter creation, shader execution, rendering correctness and hardware acceleration each require their own evidence. Follow the repository's [validation gates](../../port/validation.md).

## Toolchain and target architecture

Rust distributes standard libraries for `x86_64-pc-windows-msvc` and `aarch64-pc-windows-msvc`; current documentation classifies both as Tier 1 with host tools. That status applies to the target platform, while the same documentation explicitly says non-Windows-to-MSVC cross compilation may be possible but is unsupported by Rust itself. Installing a rustup target supplies the standard library, not every linker or native dependency. [Rust MSVC target support](https://doc.rust-lang.org/rustc/platform-support/windows-msvc.html), [rustup cross-compilation requirements](https://rust-lang.github.io/rustup/cross-compilation.html)

The retrieved cargo-xwin release is **0.23.1**. Its default compiler is `clang-cl`; it recommends a full LLVM installation. Defaults include `x86_64,aarch64`, desktop libraries and Visual Studio manifest version 17. Use `XWIN_CACHE_DIR=.dev/xwin` to keep the cache local, and `XWIN_ARCH=aarch64` when only Arm64 packages are needed. `XWIN_SDK_VERSION` and `XWIN_CRT_VERSION` pin versions; otherwise the latest versions in the manifest are selected. `cargo xwin cache xwin` supports prefetching. The tested command is in [the reproduction section](#reproduce-the-rust-cross-build); the Microsoft SDK license terms apply. [cargo-xwin requirements, usage and configuration](https://docs.rs/crate/cargo-xwin/0.23.1)

For C++, xwin supplies CRT/SDK include and library roots, including a `--use-winsysroot-style` layout for clang-cl. The pinned Nix tool is xwin **0.9.0**; the retrieved cargo-xwin 0.23.1 metadata uses xwin 0.10.0 internally. Select the architecture explicitly: standalone xwin defaults to x86_64. Its documentation cautions that Arm targets are less tested. `splat` automatically downloads/unpacks missing packages; `--accept-license` or `XWIN_ACCEPT_LICENSE=1` suppresses the SDK-license prompt. Use clang-cl with the Windows target triple and matching SDK libraries, then `lld-link`. The executed C++ probes establish compilation after the documented syntax-only compatibility fix; guest execution is still unverified. [xwin 0.9.0 usage](https://docs.rs/xwin/0.9.0/xwin/), [Clang MSVC compatibility](https://clang.llvm.org/docs/MSVCCompatibility.html), [LLD Windows support](https://lld.llvm.org/windows_support.html)

Prefer an Arm64 build if the guest is Windows on Arm to avoid x64 CPU emulation. Keep x64 as a useful second target and as the closest target to the existing reference project. Windows 11 on Arm can run both x86 and x64 applications through emulation; this does not translate a D3D12 renderer into D3D11. [Microsoft Windows-on-Arm emulation](https://learn.microsoft.com/en-us/windows/arm/apps-on-arm-x86-emulation)

## Original reference build and launch blockers

Source review of [the project](../../../demo/skinnedheightfield.vcxproj), [main.cpp](../../../demo/main.cpp), [miscellaneous helpers](../../../demo/yak_shiv_misc.h) and [matrix helpers](../../../demo/yak_shiv_matrix.h) found:

- The project defines Win32/x64 configurations only, with v141/v143 toolsets and external sibling include paths. It has no Arm64 configuration or portable build manifest.
- Dear ImGui core sources plus Win32/DX11 backends and `stb_image.h` are external dependencies absent from the tracked tree. The successful probe used ImGui **v1.92.5**, commit `6d910d5487d11ca567b61c7824b0c78c569d62f0`, and stb `f0569113c93ad095470c54bf34a17b36646bbbb5`, with explicit include/source paths. `tinyexr` appears in include paths but is not included by the main translation unit. [ImGui revision](https://github.com/ocornut/imgui/tree/6d910d5487d11ca567b61c7824b0c78c569d62f0), [stb revision](https://github.com/nothings/stb/tree/f0569113c93ad095470c54bf34a17b36646bbbb5)
- MSVC spellings such as `__int64`, `<intrin.h>` and `__debugbreak` compiled under clang-cl targeting the Windows ABI. The misplaced `[[nodiscard]]` declaration required the temporary syntax-only fix above. This does not establish portability of the headers when compiled for native macOS.
- The project links `d3d11.lib`, `d3dcompiler.lib` and `dxgi.lib`. clang-cl must compile ImGui for the same architecture, and the link must resolve CRT and Windows import libraries for that architecture.
- Runtime shader compilation uses `D3DCompileFromFile`, `vs_5_0` and `ps_5_0`. Keep HLSL files/includes and the material asset tree beside the expected working directory; copying only the executable is insufficient. Windows provides the runtime API through `D3DCompiler_47.dll`. [Microsoft D3DCompileFromFile requirements](https://learn.microsoft.com/en-us/windows/win32/api/d3dcompiler/nf-d3dcompiler-d3dcompilefromfile)

The reference asks for feature levels 11_0/10_0, creates a hardware D3D11 device first and retries WARP on `DXGI_ERROR_UNSUPPORTED`. Its shader-model-5 pipelines still need successful shader creation; the lower feature-level fallback alone is not proof that the complete workload runs.

## Parallels graphics constraints and Rust backend selection

| API/path | Primary-source finding | Implication for DASHR |
| --- | --- | --- |
| Direct3D 11 | Parallels currently advertises DirectX support up to 11. | Reference runtime is plausible; formats, shaders, captures and actual adapter remain unverified. |
| Direct3D 12 | Current product FAQ still states DirectX 11; an older Parallels staff statement explicitly says DX12 is unsupported. | Do not assume accelerated wgpu DX12 from Windows having a DX12 runtime installed. |
| OpenGL | KB reviewed 2026-08-25 says Desktop 27 with Windows 11 Arm on Apple Silicon supports **4.3**, requiring updated Parallels Tools. Older pages still say 4.1. | Record installed Desktop/Tools versions; 4.1 is not a universal current ceiling. |
| Vulkan | No current vendor support guarantee was found. A 2023 staff reply says Vulkan is unsupported. | Treat accelerated Vulkan as unavailable for planning until actual guest adapter evidence or newer vendor documentation establishes it. |

Sources: [current Parallels graphics FAQ](https://www.parallels.com/games/), [DX12 staff statement, December 2023](https://forum.parallels.com/threads/black-screen-on-aoe2de.356376/), [current OpenGL KB](https://kb.parallels.com/en/115487), [Vulkan staff statement, March 2023](https://forum.parallels.com/threads/graphics-card-drivers.359935/). The older forum statements are historical primary evidence, not a fresh Desktop 27 capability measurement.

The local host inspection reported Parallels Desktop **26.4.0** installed. VM inventory was unavailable because Parallels CLI initialization failed. Desktop 27's OpenGL 4.3 improvement therefore cannot be attributed to the inspected installation; guest version, architecture and adapter capabilities remain unmeasured.

The inspected [Rust GPU context](../../../src/gpu_resources.rs) hardcodes `Backends::PRIMARY` and does not call the documented environment override helpers. Thus `WGPU_BACKEND=gl` alone does not enable GL here. wgpu 30.0.1 has no D3D11 backend; GL is secondary. [wgpu backend and environment definitions](https://docs.rs/wgpu/30.0.1/wgpu/struct.Backends.html)

**Windows GL source qualification:** the backend documentation describes Windows GL through ANGLE, but the locked wgpu-hal 30.0.1 implementation explicitly selects `mod wgl` under `cfg(windows)` and reexports its native Windows `Instance`/`Surface`. `src/gles/wgl.rs` calls native WGL context and pixel-format APIs. Therefore ANGLE/EGL is not the only possible Windows route in this source, and those older backend comments should not rule out a native WGL experiment. Its adapter source accepts desktop GL 3.3 or newer. Deliberate `Backends::GL` selection, successful WGL context creation, format support, resource limits and shader execution remain necessary; no such experiment was run. [locked wgpu-hal package](https://docs.rs/crate/wgpu-hal/30.0.1)

The same `src/gles/adapter.rs` enables compute only for desktop GL 4.3, GLES 3.1 or `GL_ARB_compute_shader`, and exposes zero compute limits otherwise. The current context inherits nonzero compute requirements from `Limits::default`; [the uniform probe](../../../src/probe.rs) actually dispatches a compute shader. **Inference:** a guest exposing only GL 4.1 without the compute extension would need explicit downlevel device-limit negotiation for raster-only operation and a supported alternate uniform-validation path; changing the backend flag alone is insufficient. Desktop 27's GL 4.3 claim addresses the API-version prerequisite, but actual compute limits, storage-buffer support and full-float rendering still need probing. [wgpu default/downlevel limits](https://docs.rs/wgpu/30.0.1/wgpu/struct.Limits.html)

**Software-execution caveat:** Microsoft documents WARP D3D12 support and native Arm64 code generation in both Arm64 and emulated x64 processes. The locked wgpu-hal 30.0.1 source (`src/auxil/dxgi/factory.rs`, `src/dx12/adapter.rs`) retains software adapters and labels them `Cpu`; it tries D3D12 device creation even at feature level 11_0. Therefore lack of accelerated Parallels DX12 does not prove the Rust executable cannot render through WARP. That is a separate untested software path. A D3D12 API device at feature level 11_0 remains a D3D12 device; the level number does not make it use D3D11. [Microsoft WARP guide](https://learn.microsoft.com/en-us/windows/win32/direct3darticles/directx-warp), [wgpu-hal 30.0.1 package](https://docs.rs/crate/wgpu-hal/30.0.1)

## Evidence required after linking

Record binary architecture and DLL imports first. In the guest, record Windows build, Parallels Desktop/Tools versions, adapter name/backend/device type, exposed/requested attachment limits, float-format support and enabled features. Run the numerical probe and a bounded deterministic capture before a viewer smoke test. Preserve failures as failures, and label WARP results as software execution. Only actual comparable captures can support reference parity; only measured runs with recorded hardware/settings can support a performance claim.

This research used the locked local source and public vendor/maintainer documentation. Authenticated GitHub queries verified the cited ImGui/stb revisions. No claim of latest dependency selection is implied. No VM was accessed or changed.

## Reproduce the original C++ probe

First fetch ImGui v1.92.5 and the pinned stb revision into `.dev/reference-deps/`, and populate `.dev/xwin` with the Rust cross-build. Copy `demo/main.cpp` and both yak headers into `out/windows-probe/reference-source/`; in that copy move `[[nodiscard]]` before `float` on `SecondsSince`. Run the following script from the repository root inside the locked Nix shell with LLVM lld added as above:

```bash
#!/usr/bin/env bash
set -euo pipefail
sdk_root="$PWD/.dev/xwin/xwin"
imgui_root="$PWD/.dev/reference-deps/imgui-1.92.5"
for arch in x86_64 aarch64; do
  build_root="$PWD/out/windows-probe/original-$arch"
  mkdir -p "$build_root"
  flags=(--target="$arch-pc-windows-msvc" /c /std:c++17 /MD /EHsc /O2 /DUNICODE /D_UNICODE /DNDEBUG)
  includes=(-imsvc "$sdk_root/crt/include" -imsvc "$sdk_root/sdk/include/ucrt" -imsvc "$sdk_root/sdk/include/shared" -imsvc "$sdk_root/sdk/include/um" -I"$imgui_root" -I"$imgui_root/backends" -I"$PWD/.dev/reference-deps/stb")
  clang-cl "${flags[@]}" "${includes[@]}" /Fo"$build_root/main.obj" -- out/windows-probe/reference-source/main.cpp
  for unit in imgui imgui_draw imgui_tables imgui_widgets imgui_demo; do
    clang-cl "${flags[@]}" "${includes[@]}" /Fo"$build_root/$unit.obj" -- "$imgui_root/$unit.cpp"
  done
  for unit in imgui_impl_win32 imgui_impl_dx11; do
    clang-cl "${flags[@]}" "${includes[@]}" /Fo"$build_root/$unit.obj" -- "$imgui_root/backends/$unit.cpp"
  done
  lld-link /OUT:"$build_root/skinnedheightfield.exe" /SUBSYSTEM:CONSOLE /LIBPATH:"$sdk_root/crt/lib/$arch" /LIBPATH:"$sdk_root/sdk/lib/ucrt/$arch" /LIBPATH:"$sdk_root/sdk/lib/um/$arch" out/windows-probe/original-"$arch"/*.obj d3d11.lib d3dcompiler.lib dxgi.lib user32.lib gdi32.lib dwmapi.lib imm32.lib
  cp demo/*.hlsl "$build_root/"
done
```

The script compiles all reference/ImGui translation units for each architecture and links both executables. It does not start a VM, run shaders or copy the material assets; copy `demo/assets/` into each launch directory, or use the assembled local VM bundle. The research and JSON evidence are committed; generated executables, SDK caches and VM archives remain ignored.
