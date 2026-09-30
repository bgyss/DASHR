# Source and dependency provenance

This port translates Tom Forsyth's DASHR v1.0 source at `9cf55d4c989a4ff7974dc1360e58027e739099f2`, preserved in `demo/`. The paper and its illustrations remain in `paper/`. The fork research snapshot is `d815782`. The software license in the [paper](../../paper/DASHR_Paper.html#license-and-misc) permits MIT-0 or Unlicense; the port selects MIT-0 and retains the author's credit.

The Rust dependency versions are pinned in `Cargo.toml`; `Cargo.lock` fixes transitive dependencies. Runtime shaders are checked-in WGSL derived from `demo/PipelineDeform.hlsl`, `PipelineEdgefill.hlsl`, `PipelineMain.hlsl`, `Utils.hlsl` and `ConstantBuffer.hlsl`. There is no compiled-executable substitution for a source build.

The included roof and aerial-rock material files are credited to Poly Haven by `demo/assets/Assets thanks to Polyhaven.com.txt`. The two `Textures_*` images have no separate asset license recorded in this snapshot. Captures record source and decoded texture hashes. Packaging/distribution of external assets needs its own provenance review.

Windows D3D11 source reproduction and reference captures have not been executed in this porting session. The existing Visual Studio project still depends on external ImGui/stb directories. P0's clean Windows launch gate remains open. This does not prevent analytic fixtures or the native capability probe, as specified by the roadmap.

API references used for the translation:

- [wgpu 30.0.1](https://docs.rs/wgpu/30.0.1/wgpu/): explicit device limits, pipeline layouts and readback.
- [winit 0.30.13](https://docs.rs/winit/0.30.13/winit/): native window lifecycle.
- [image 0.25.10](https://docs.rs/image/0.25.10/image/): PNG/JPEG decoding.
- [Microsoft default sampler descriptor](https://learn.microsoft.com/en-us/windows/win32/api/d3d11/ns-d3d11-cd3d11_sampler_desc): the reference uses clamp addressing for both linear and point samplers.
