# Draft social posts

Drafts announcing the fork and its goal of becoming a standalone DASHR library for Unreal Engine, Unity and Blender. Nothing here has been posted. Status wording matches the [validation record](../port/validation.md): no library code exists yet, and Windows Rust rendering, animation review and D3D11 parity beyond one captured scene remain open. Make no speedup claims. Credit Tom Forsyth for the technique; Tom has said he is not looking for pull requests, so keep asks about the fork.

Links: [paper](https://tomforsyth1000.github.io/DASHR/paper/DASHR_Paper.html), [fork](https://github.com/bgyss/DASHR), [plan](../plans/standalone-library-and-integrations.md).

## Screenshot suggestions

Attach one image per post. The comparison PNGs are not committed (they sit under ignored `out/`); regenerate them by following the reproduction steps in the [comparison report](../port/comparisons/2026-09-30-tube.md), or use a fresh `scripts/in-nix mise run view` capture. Paper figures in `paper/` are Tom's; credit him in the caption if reused.

| Post | Suggested image | Why |
| --- | --- | --- |
| LinkedIn | Side-by-side of the Windows D3D11 frame and the Mac/Metal replay of the same tube pose (2534×1640, both unscaled), captioned with the 99.98% foreground overlap and "single pose, not full parity" | Shows the port is real and candid about its limits |
| X | Short screen recording or GIF of the viewer bending the tube with the heightfield detail staying attached; fall back to one close crop of the Mac frame | Motion sells "detail follows the skin" |
| Reddit | Comparison image plus one diagnostic view from the viewer (step-count or status overlay) to show the tracer internals | The subreddit audience wants technique detail |

Alt text for any attachment: "Skinned tube with a roof-tile heightfield rendered by DASHR; left Windows D3D11, right Mac Metal, same camera and pose."

## LinkedIn

Tom Forsyth just published DASHR (Dynamically Animated Skinned Heightfield Rendering), a technique for rendering fine, animated surface detail on skinned characters. It ray-marches a heightfield in the surface space of the mesh, so the detail stays put as the character bends and deforms. It avoids huge triangle counts.

Tom released it as a research paper plus a Windows D3D11 demo, under a very permissive license, and invited others to turn it into a real library.

I'm working on that in my fork:

- A native Rust/wgpu port of the demo. It renders on Metal locally, and a matched Windows D3D11 vs Mac capture of the same scene agrees closely.
- A written plan to go from viewer to a standalone library with a stable C API.
- Target integrations: Blender, Unity and Unreal Engine.
- Every point from Tom's future-work and limitations sections is mapped to a workstream. That covers cloth/lone edges, better ray traversal, compact storage and self-intersection handling.

Still early. The library doesn't exist yet, and D3D11 parity, animation review and engine integrations are all open. The roadmap marks what's verified and what isn't.

Paper: https://tomforsyth1000.github.io/DASHR/paper/DASHR_Paper.html
Fork and plans: https://github.com/bgyss/DASHR

All credit for the technique goes to Tom Forsyth.

#ComputerGraphics #GameDev #Rendering #Rust #Blender #Unity #UnrealEngine

## X (thread)

1/ Tom Forsyth published DASHR, a way to get ray-marched heightfield detail on skinned, animating meshes without piles of triangles. The detail follows the skin as it bends. Paper: https://tomforsyth1000.github.io/DASHR/paper/DASHR_Paper.html *(attach: bending-tube GIF)*

2/ His release is a paper and a D3D11 demo. He says it isn't meant as a drop-in library, but that anyone who wants to build one should go for it. So I'm doing that.

3/ My fork so far: a Rust/wgpu port that runs on Metal, plus a Windows D3D11 vs Mac capture comparison on the same scene. Still experimental and single-asset. *(attach: side-by-side comparison image)*

4/ The goal is a standalone library with a C API for Unreal, Unity and Blender. I've written a phased plan and traced every item from Tom's future-work section into it. No library code yet, so this is the roadmap. https://github.com/bgyss/DASHR

5/ Open questions include whether engines should run translated shaders on their own device, how to handle cloth and open edges, and how to keep seams clean. Thoughts welcome. All credit to Tom Forsyth for the technique. *(Tag his account; confirm the handle first.)*

## Reddit

Suggested for r/GraphicsProgramming; check each subreddit's self-promotion rules first. r/gamedev or r/rust also fit with the Rust angle up front. *(Attach: comparison image and a step-count/status diagnostic view.)*

**Title:** Turning Tom Forsyth's DASHR (animated skinned heightfield rendering) into a library for Unreal, Unity and Blender

Tom Forsyth published DASHR this month: [paper](https://tomforsyth1000.github.io/DASHR/paper/DASHR_Paper.html). It ray-marches a heightfield in a skinned mesh's surface space, with a distortion texture tracking how the surface stretches under animation and "teleports" across UV seams. The detail then deforms with the character instead of swimming.

Tom says it's a proof of concept, not a library, and invites others to build one. I'm doing that in a fork: https://github.com/bgyss/DASHR

Where it stands:

- Rust/wgpu port with the four RGBA32F render targets, both distortion encodings, seam/gutter bake and tracing. It renders on Metal.
- A matched Windows D3D11 vs Mac replay of one scene: about 99.98% foreground overlap, and most pixels within one RGB code. Windows Rust rendering, animation review and other scenes are still unverified.
- Docs mapping each of Tom's future-work and limitation points (binary search at hits and teleports, split teleport textures, tornado poles, cliff edges for cloth, metric tangents vs MikkTSpace) to a workstream.
- A plan to extract a headless core with a C ABI, then Blender, Unity and Unreal integrations. None of that code exists yet.

Questions I'd like opinions on:

1. For Unity/Unreal, is it better to translate the shaders into the engine's render graph, or run wgpu on a separate device and copy results?
2. Has anyone handled open edges or thin cloth in a surface-space ray-marcher like this?
3. Any experience with conservative min/max bounds under heavy bending?

Credit for the technique is all Tom's. The port and plan are mine.
