# Running Operation Purge (milestones M4 winding gate + M5 static scene)

The port is developed and tested **headless** (no display server), so all
CPU-side logic is covered by `cargo test`. The window, GL context, shaders,
and 3D building can only be exercised at a real desktop. This note explains
how to launch the M4 debug view and how to sign off the winding/culling
checklist.

## Build & test (any machine, no display needed)

```sh
cargo build --all-targets     # compiles all winit/glutin/glow/ab_glyph usage
cargo test                    # all tests green (CPU-side logic only)
```

The headless suite covers the M4 correctness proxies: the wall-outline
tracer and building extrusion (positions / winding order / UVs against a
hand-computed 1×1 room), the triangle-fan→list conversion, the u16 mesh
GUARD, the four shader sources' PARITY constants, and the camera MVP
(including the `z' = 2z - w` clip fixup at the near/far planes).

## Run the M4 debug view (needs a real display)

```sh
cargo run                     # or: cargo run --release
cargo run -- --assets ./assets
```

The app opens **borderless fullscreen** with an OpenGL 3.3 core context
(macOS negotiates 4.1 core — fine, GLSL stays `#version 330`), compiles the
four shader programs, loads `assets/MazeFiles/Small1.maz`, extrudes the
building, and drops a **free-fly debug camera** inside room (0,0). As of M5
the scene also contains one **procedural robot** (dome + barrel + treads) at
the room center and one **center marker** one room to its +X, both lit by the
point-light-at-eye Gouraud shader.

### Controls

| Key | Action |
|---|---|
| `W` / `S` | move forward / back |
| `A` / `D` | strafe left / right |
| `Space` / `Left Ctrl` | move up / down (world +Z / −Z) |
| mouse | look (yaw / pitch) |
| **`C`** | **toggle back-face culling** (starts OFF) |
| **`F`** | **flip the front face** (`GL_CCW` ↔ `GL_CW`) |
| **`V`** | **cycle the isolated draw category**: All → Walls → Floor → Ceiling |
| `Esc` | quit |

The on-screen HUD (top-left) shows the current cull state, front-face
winding, isolated category, and camera pose.

## The §3.3 winding / culling checklist

Culling is **globally disabled by default** (`WindingDecision::Undetermined`
→ `CullNone`), exactly as the design mandates — nothing is culled until you
turn it on and sign each item off. The **expected** mapping is
`D3DCULL_CCW ≡ glFrontFace(GL_CCW) + glCullFace(GL_BACK)` (design §3.3 /
§1.5.2): D3D winds in y-down screen space, GL in y-up window space, so the
same clip-space triangle flips winding class. Press `C` (front stays CCW,
the default) and verify each face **stays** visible from the side it should
face and **disappears** from the back side; if a class is inverted, `F`
flips it — record which way each category needed.

Exercisable now (the building exists at M4):

1. **Floor visible only from above.** Isolate Floor (`V`), enable cull
   (`C`). Looking down → floor solid; move the camera below z=0 and look up
   → it culls.
2. **Ceiling visible (and dark) only from below.** Isolate Ceiling. From
   inside, looking up → ceiling visible and **dark** (its normal points UP,
   away from the eye light — the intentional 2011 quirk, PARITY). From above
   the roof looking down → it culls.
3. **Inner wall faces from inside rooms.** Isolate Walls. Standing in a room,
   the wall faces that bound the room are visible.
4. **Reversed-winding outer-boundary polygon from inside the building.** The
   first traced polygon (the outer boundary) is emitted with reversed
   winding (`cBuilding3D.cpp:187-193`, PARITY) so its inward faces show from
   inside the building even after the interior walls' winding is fixed.

Now exercisable at **M5** (the demo robot + center exist):

5. **Robot dome / barrel / treads from outside.** Fly around the robot at the
   room center. Confirm the dome renders **noticeably dim** — its normals are
   the raw (un-normalized) scaled positions of length ~0.5, and the barrel's
   are ~0.08, so the FFP-Gouraud term is scaled down on purpose (PARITY, the
   2011 look). The two treads sit ±Y of the body.
6. **Center markers.** The green floor+wall marker one room over sits just
   above the floor/wall (z-lift `0.001 · fSize`, no z-fighting).

Sprite quads with index pattern (0,2,1)(1,2,3) are unit-tested
(`render::sprites`); the sprite/particle/health-bar visuals get wired into
the live scene at M8/M9.

## M5 visual gate (needs a real display, side-by-side vs help-doc shots)

Compare against the help-doc screenshots: **dome dimming** from raw normals,
**dark ceiling** (up-normal), **UV repeat tiling** on walls/floor, **map
colors** after the COLORREF→RGBA conversion, and the **viewport offsets**
(0.025 / 0.95 full-2D, 0.05 / 0.9 minimap — the 2D map path is wired at M8).
If the dome/scene reads too dark or too bright, tune `LIGHT_DATA`
(`a0=1.0, a1=0.02, a2=0.0`) in `src/data/builtin.rs` — the file was lost and
these are reconstructed values; keep the `light_data_values_are_pinned` test
in sync with any change.

**Depth-fixup visual check** (design §7 M4): with cull off, fly so a near
wall overlaps a far wall / the ceiling — nearer geometry must occlude
farther geometry (`GL_LEQUAL` + the `proj·CLIP_FIX` `z' = 2z − w` remap
working together). The remap itself is pinned by the `render::d3dx` and
`render::camera` unit tests.

### Signing off

Once each of items 1–4 behaves correctly with culling **on**, record the
per-category decision (`glFrontFace(GL_CCW/CW)` + `glCullFace(GL_BACK)`) as a
doc comment in `render/gl.rs`, commit reference screenshots, and only then
bake a concrete `WindingDecision` into each draw category (it stays
`Undetermined`/`CullNone` until then).

## Platform notes

- **macOS**: framebuffer is physical (Retina) resolution; `glViewport` uses
  physical pixels while HUD metrics use logical px × scale. Raw mouse deltas
  are NSEvent counts — tune look sensitivity via `mouse_scale` in config.
- **X11**: no locked cursor grab (relevant from M8; the M4 debug camera reads
  raw `DeviceEvent::MouseMotion`, which works regardless).
- **Wayland**: `ScaleFactorChanged` re-bakes the glyph atlas.

If `assets/GameData.toml` or `Small1.maz` fails to load, the view still runs
and prints the reason in the HUD (GUARD: no crash).
