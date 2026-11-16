# Hanabi Studio

An original, MIT-licensed 3D dance-animation studio written in Rust, with a workflow
familiar to people who animate characters with PMX/PMD models and VMD motions.
It is **not** MMD and contains no code, data or artwork from MMD or from any
third-party effect pack. The file formats are implemented from their published layouts.

## Workspaces

| Workspace | What it does |
|-----------|--------------|
| **Animation** | Separate Load Model / Motion / Audio buttons (plus accessory and pose), a render-area guide or render-area-only view matching the output size, pose bones with the rotate/move gizmo, register keys, dope-sheet timeline with a scroll bar and playhead follow, curve editor, morph sliders, IK, physics, camera & light keys, quick tools (mirror pose, copy/paste pose, paste mirrored, key all, smooth keys, generate idle/wave/nod/beat-bounce motion). |
| **Effects** | Four sections. **Lights**: lighting style (classic / polished / bright anime / realistic Ray Lighting), sky, the main sun, and placed point lights, spotlights and directional lights (drag their icons in the view), with light rigs you can save. **Materials**: each model part gets a type (cloth, skin, hair, metal, glass…), your own normal, height, metallic and roughness maps, special kinds (glass, water with ripples, wet + rain, fur, grass, glow), and a *luminous* switch that turns a part into a light source; open any material in a node editor (textures, colours, noise, fresnel, maths) to build your own; save materials as presets for every project. **Stage**: particles, light beams, water surface, mirror floor. **Camera & Screen**: an ordered list of screen effects including lens depth of field (f-stop, lens, hexagon/octagon bokeh), bloom, reflections, auto exposure, motion blur and more, with presets you can save. A full node graph is still available under “Node graph (advanced)”. MME effect files are intentionally not supported. |
| **Color** | Lift / gamma / gain wheels, exposure, contrast, highlights, shadows, temperature, tint, saturation, vibrance, film looks, presets, before/after split and a live RGB histogram. |
| **Render** | Offline GPU rendering to MP4 (H.264), WebM (VP9), MOV (ProRes) via ffmpeg, or PNG sequences; any size up to 8K; physics stepped once per frame so renders match playback; song muxed in. |

## Files

* Opens: `.pmx` (2.0/2.1), `.pmd`, `.x` (text DirectX accessories), `.vmd` (bone, morph, camera, light, shadow, visibility/IK keys), `.vpd` poses, `.wav` audio, `.hproj` projects, `.hmo` motions.
* Real-world downloads: textures are found even when letter case differs, paths are Windows-style or absolute, files were moved into sub-folders, or an unzip tool garbled Japanese/Chinese file names. Motions saved on Chinese PCs (GBK names) and bone/morph names cut to 15 bytes by VMD are matched to the model. Broken numbers in models are repaired on load.
* Projects (`.hproj`, version 2) store keyframes in a compact binary block: a 191,000-key captured dance went from 180 MB to 12 MB and opens in 0.02 s. Older projects still open. Saving keeps the previous file as `.bak`; projects with unsaved changes are backed up every three minutes (File → Open last autosave).
* Help → Credits of loaded files shows the readme / terms-of-use files found next to every model, stage, motion and song, and copies a credits list for a video description.
* Saves: motions as **`.vmd`** (MMD-compatible) or **`.hmo`** (this app's readable JSON), camera/light motion as `.vmd`, poses as `.vpd`, whole projects as `.hproj`.
* Project-save formats of other programs (`.pmm` etc.) are not read.

## AI control (MCP)

While running, the studio serves the Model Context Protocol on `http://127.0.0.1:7860/mcp`.
To let an AI client launch and drive it, add:

```json
{ "mcpServers": { "hanabi-studio": { "command": "/path/to/hanabi-studio", "args": ["--mcp-stdio"] } } }
```

Tools include loading files, listing bones/morphs, posing and keying, camera/light, effect presets
and node parameters, colour grading, rendering images/videos, generating motion and saving.

## Build

```
cargo build --release
./target/release/hanabi-studio [files…]
```

Options: `--mcp-stdio`, `--mcp-port N` (0 = off), `--workspace animation|effects|color|render`,
`--preset "Water Stage"`, `--empty`, `--screenshot out.png`, `--frame N`, `--show credits|video-help|physics|mcp`,
`--selftest <folder>`, `--resave old.hproj new.hproj`.

Builds for Linux and Windows are made by `.github/workflows/build.yml` when the project is pushed to GitHub (tags like `v0.2.0` become releases).

Requirements: OpenGL 3.3, Linux or Windows 10/11. Songs play through the app's own player (ALSA on Linux, the Windows sound service on Windows).
Optional: `ffmpeg` for MP4/WebM/MOV export. It is not bundled (it has its own licence); Help → Set up video export walks through installing it (winget on Windows, the package manager on Linux) and finds it on the PATH, next to the app, or wherever the person points.

Testing against community files: `hanabi-studio --selftest <folder>` loads every model, stage, motion and song under a folder, checks textures, saves every VMD and reads it back, plays real dances on every dancer with physics (looking for broken numbers and torn chains), and times five dancers on the largest stage together. `--frame N` plays to frame N before a `--screenshot`, and `--show credits|video-help` opens a window for pictures.

## Shortcuts

Space play/pause · ←/→ frame · ↑/↓ next/previous key · Enter register key · R/T rotate/move ·
K auto-key · Ctrl+C/V copy/paste · Ctrl+Shift+V paste mirrored · Del delete keys ·
Ctrl+Z/Y undo/redo · F1–F4 workspaces · right-drag orbit · middle-drag pan · wheel zoom.

## Third-party libraries

Built with open-source crates under MIT, Apache-2.0, Zlib, ISC and BSD licences, including egui/eframe,
glow, glam, rapier3d (physics), image, encoding_rs, serde/serde_json, rfd, rayon and libloading.
The full list with versions and licences is in `THIRD-PARTY.md`.
See `cargo tree` / each crate for its licence text. Toon ramps and the demo mannequin are
generated in code.
