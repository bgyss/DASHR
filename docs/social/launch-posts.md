# Draft social posts

Drafts announcing the fork and its goal of becoming a standalone DASHR library for Unreal Engine, Unity and Blender. Nothing here has been posted. Status wording matches the [validation record](../port/validation.md): a headless Rust library and C ABI preview now exist; engine integrations, Windows Rust rendering and reviewed animation remain open. Avoid general speedup claims; identify the fixture and adapter for measured results. Credit Tom Forsyth for the technique; Tom has said he is not looking for pull requests, so keep asks about the fork.

Links: [paper](https://tomforsyth1000.github.io/DASHR/paper/DASHR_Paper.html), [fork](https://github.com/bgyss/DASHR), [plan](../plans/standalone-library-and-integrations.md).

## Screenshots

Attach one image per post. Images live in [`images/`](images/) and were generated from this repository's native viewer/capture path; the Windows frame comes from the preserved D3D11 comparison snapshot. Paper figures in `paper/` are Tom's; credit him if reused.

| Post | Image | Why and caption |
| --- | --- | --- |
| LinkedIn | [`windows-vs-mac-tube.png`](images/windows-vs-mac-tube.png) | Windows D3D11 reference (left) and Mac/Metal Rust/wgpu replay (right) of the same tube pose, cropped identically. Caption: "One skinned-tube pose. About 99.98% foreground overlap; a single-frame check, not full parity." The pose is a tight, folded stress case |
| X | [`milder-tube-bend.png`](images/milder-tube-bend.png) | Gentle bend at 1600×900 with the heightfield detail clearly attached to the surface. Caption: "DASHR port on Metal: a bent tube with ray-marched detail." Consider a short viewer GIF of the bend if you can record one |
| Reddit | Both images, plus a diagnostic view from the viewer (step-count or status overlay) | Parity evidence and a bend that shows the technique; the diagnostic view suits the technical audience |

Alt text, comparison: "Skinned tube with a roof-tile heightfield rendered by DASHR; left Windows D3D11, right Mac Metal, same camera and pose." Alt text, bend: "A gently bent tube with ray-marched roof-tile relief detail on a dark background."

Provenance:

- `windows-vs-mac-tube.png` is composed from the Windows frame `47876180…` and the Mac replay `51a473a3…` recorded in the [comparison metrics](../port/comparisons/2026-09-30-tube-metrics.json); reproduce it with the steps in the [comparison report](../port/comparisons/2026-09-30-tube.md). Both frames were cropped identically and not otherwise altered.
- `milder-tube-bend.png` was rendered on Apple M1 Max / Metal at repository revision `1e57a43` with [`milder-tube-settings.json`](images/milder-tube-settings.json) (`fixtures/tube.json` with animation amount 0.35, atlas 512 and a closer camera): `scripts/in-nix cargo run --locked --release --features viewer -- capture --settings docs/social/images/milder-tube-settings.json --out out/social-milder`. The trace reported zero invalid and zero budget terminations; some ragged fringing remains at the left silhouette. It is a demonstration frame, not a parity or quality acceptance result.

## LinkedIn

Tom Forsyth just published DASHR (Dynamically Animated Skinned Heightfield Rendering), a technique for rendering fine, animated surface detail on skinned characters. It ray-marches a heightfield in the surface space of the mesh, so the detail stays put as the character bends and deforms. It avoids huge triangle counts.

Tom released it as a research paper plus a Windows D3D11 demo, under a very permissive license, and invited others to turn it into a real library.

I'm working on that in my fork:

- A native Rust/wgpu port of the demo. It renders on Metal locally, and a matched Windows D3D11 vs Mac capture of the same scene agrees closely.
- A headless Rust library, an opt-in C ABI preview and a roadmap for engine integrations.
- Target integrations: Blender, Unity and Unreal Engine.
- Every point from Tom's future-work and limitations sections is mapped to a workstream. That covers cloth/lone edges, better ray traversal, compact storage and self-intersection handling.

Still early. The Rust API and C ABI are local previews; D3D11 parity, animation review and engine integrations remain open. The roadmap marks what's verified and what isn't.

Paper: https://tomforsyth1000.github.io/DASHR/paper/DASHR_Paper.html
Fork and plans: https://github.com/bgyss/DASHR

All credit for the technique goes to Tom Forsyth.

#ComputerGraphics #GameDev #Rendering #Rust #Blender #Unity #UnrealEngine

### Addendum to the first announcement (2026-10-04)

Since drafting this, I've moved the fork beyond the standalone-library plan. DASHR now has a headless Rust API, versioned asset and map-cache formats, an independent Rust consumer, and an optional C ABI preview with a generated header and native C client. The local extraction checks preserved the fixed tube/cube renders byte for byte on Metal.

I've also worked through all 20 of Tom's future-work items individually: traversal refinements, seam/pole diagnostics, topology validation, storage and shader experiments, and isolated SDF/fractal/splat/voxel animation research. Some produced useful bounded results; others made the case for keeping the reference path. For example, full-atlas compute edgefill reduced that pass's median time by 76.4% in five paired runs of one bent-tube fixture on M1 Max/Metal, while a specialized inverse improved trace time by only 0.82%, so it stays opt-in.

The seam work is especially interesting: one pinched-cube hotspot went from 360 steps and 302 teleports to 65 steps and 5 teleports after an authored cut. That is a diagnostic result; the visual review is still open.

This is still an experimental library. Seven roadmap items remain open, several closures have a deliberately bounded scope, and Blender/Unity/Unreal integrations and broader platform/scene validation are unfinished. I've put the results, decisions and remaining gates in a [comprehensive work report](../reports/2026-10-04-standalone-library-and-future-work.md).

Fork: https://github.com/bgyss/DASHR

All credit for the DASHR technique remains Tom Forsyth's; the Rust library, experiments and integration work are this fork's contribution.

## X (thread)

1/ Tom Forsyth published DASHR, a way to get ray-marched heightfield detail on skinned, animating meshes without piles of triangles. The detail follows the skin as it bends. Paper: https://tomforsyth1000.github.io/DASHR/paper/DASHR_Paper.html *(attach: `milder-tube-bend.png` or a bend GIF)*

2/ His release is a paper and a D3D11 demo. He says it isn't meant as a drop-in library, but that anyone who wants to build one should go for it. So I'm doing that.

3/ My fork so far: a Rust/wgpu port that runs on Metal, plus a Windows D3D11 vs Mac capture comparison on the same scene. Still experimental and single-asset. *(attach: `windows-vs-mac-tube.png`)*

4/ The fork now has a headless Rust library and an optional C ABI preview. Unreal, Unity and Blender integrations remain planned. Every item from Tom's future-work section has a documented result or open gate. https://github.com/bgyss/DASHR

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
- A headless Rust API and opt-in C ABI preview with an independent native C smoke client. Blender, Unity and Unreal integrations remain planned and unverified.

Questions I'd like opinions on:

1. For Unity/Unreal, is it better to translate the shaders into the engine's render graph, or run wgpu on a separate device and copy results?
2. Has anyone handled open edges or thin cloth in a surface-space ray-marcher like this?
3. Any experience with conservative min/max bounds under heavy bending?

Credit for the technique is all Tom's. The port and plan are mine.
