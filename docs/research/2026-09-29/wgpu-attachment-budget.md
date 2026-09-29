# wgpu attachment budget: defaults, larger limits and DASHR

Research date: 2026-09-29. These conclusions extend the [platform feasibility assessment](platform-feasibility.md). API and backend source documentation support the limit model; no device capability probe or renderer experiment was executed here.

## Conclusion

wgpu's default color-attachment budget is **32 bytes per sample**. **64 is not a universal wgpu ceiling.** An application can request a larger budget, including more than 64, when the selected adapter exposes that capability. The budget is `max_color_attachment_bytes_per_sample`, requested through `DeviceDescriptor::required_limits`. Requesting a value greater than the adapter's exposed limit is not a supported override. [wgpu 30.0.1 limits](https://docs.rs/wgpu/30.0.1/wgpu/struct.Limits.html)

DASHR's existing deformation and edgefill passes each write four RGBA32F targets: `4 * 16 = 64` bytes per sample. Therefore, a successfully requested budget of **64 is enough for the current outputs**. A larger budget would matter for added simultaneous outputs, such as extra forward-basis or diagnostic textures. Eight allocated transform images do not constitute eight simultaneous outputs: the temporary and final sets belong to different passes. [Source allocations](../../../demo/main.cpp), [deformation outputs](../../../demo/PipelineDeform.hlsl), [edgefill outputs](../../../demo/PipelineEdgefill.hlsl)

## Three limits to distinguish

| Value | Meaning | Can the application change it? |
| --- | --- | --- |
| Default device budget | Conservative starting value of 32 | Yes, request a higher adapter-supported value during device creation |
| Adapter-exposed budget | What this backend/adapter offers to wgpu | Ordinary device configuration cannot exceed it |
| Requested device budget | What the application actually enabled | Recreate the device with a supported higher request if necessary |

An adapter can support a better limit than a created device enables. Query the adapter first and request only the budget needed; enabling all maximum limits is unnecessary and can have performance consequences. Attachment count, renderable formats and float filtering remain separate requirements. [wgpu limit negotiation](https://docs.rs/wgpu/30.0.1/wgpu/struct.Limits.html)

## More than 64: backend-dependent

The inspected Vulkan backend derives its byte budget from maximum color-attachment count multiplied by maximum target pixel byte cost. With eight attachments at 16 bytes each, that calculation permits 128 bytes per sample. This establishes that the wgpu model is not capped at 64; it is not a measurement of any particular device. The source also notes that the derived budget is not a complete hardware guarantee on every tile-based GPU. [wgpu Vulkan adapter source](https://wgpu.rs/doc/src/wgpu_hal/vulkan/adapter.rs.html)

For macOS, query the actual **Metal** adapter and record its reported limits. The API correspondence reference ties this limit to Metal render-target constraints and notes that the mapping is more complicated than a simple byte sum. Do not infer a Mac's limit from Vulkan, a different GPU family or a browser implementation. **More than 64 on the intended Mac remains unverified.** [WebGPU backend correspondence](https://gpuweb.github.io/gpuweb/correspondence/)

Native Metal APIs or a modified wgpu backend are separate experiments, not configuration switches that safely unlock an unsupported limit. Establish a supported format/output combination and an actual render capture before claiming they solve a restriction.

## Capability check and request

This illustrative Rust fragment prepares the relevant limits; it is not a complete compiled program. Use the result as `DeviceDescriptor::required_limits` when requesting the device, and retain the project's other required limits/features.

```rust
let supported = adapter.limits();
let needed_bytes = 64; // Set to 128 only if the pipeline needs it.

if supported.max_color_attachment_bytes_per_sample < needed_bytes {
    // Select a split-pass or other validated fallback.
    return Err("adapter attachment budget is insufficient");
}

let mut required = wgpu::Limits::default();
required.max_color_attachment_bytes_per_sample = needed_bytes;

// Pass `required` as DeviceDescriptor::required_limits.
// Check the attachment count, renderable formats and sampling features too.
```

Production code should report the adapter name/backend, available and requested budgets, selected formats and fallback choice. Float32 linear filtering still needs `FLOAT32_FILTERABLE` support/request or a tested manual filtering path; raising the attachment budget does not enable filtering. [wgpu features](https://docs.rs/wgpu/30.0.1/wgpu/struct.Features.html)

## Alternatives when the reported maximum is insufficient

1. **Split outputs into multiple render passes.** This preserves full-float storage but may duplicate vertex/raster work. Compare pass overhead and identical coverage/interpolation against the baseline.
2. **Use smaller output formats.** Four RGBA16F targets total 32 bytes per sample. This is a precision change and needs seam, conditioning and hit-error gates; it is not automatically reference-equivalent.
3. **Move suitable work to compute/storage resources.** Color-attachment budgets do not govern compute outputs, but storage resource counts, format usages, synchronization and other limits do. The edgefill/distortion pass is a narrower experiment than replacing UV rasterization and its interpolation.
4. **Reduce simultaneous outputs through a redesigned representation.** Fewer channels can reduce the requirement, but must preserve the scaled/skewed basis, mode-1 anchor and distortion data used by the tracer.

For this fork, first probe and request **64**, preserve the original float32 representation, and validate it. Only investigate greater budgets when a concrete extension needs extra simultaneous outputs. Record a larger reported limit as capability evidence; device creation and a numerical render/readback fixture are still required to establish usability. See the [benchmark protocol](benchmark-protocol.md) and [roadmap P2](../../plans/dashr-porting-roadmap.md).
