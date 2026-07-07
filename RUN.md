# Running Operation Purge (M11 — full game flow)

The port is developed and tested **headless** (no display server), so all
CPU-side logic is covered by `cargo test`. The window, GL context, shaders,
audio device, and the play/feel/visual behaviour can only be exercised at a
real desktop. As of **M11** `cargo run` launches the **full flow**: the app
opens on the in-engine **New Game screen** (maze pattern + live match count,
Game/Team combos, password), and OK spawns the world and drops onto the 2D
map → possess a robot → fight the AI in 3D → win/lose/draw pops a
**GameOver modal** (replay / new game / keep watching) → quit via the
EndGame confirm. This note explains how to launch it, what to eyeball, and
(further down) the retained M4/M5 winding + lighting sign-off notes.

## Build & test (any machine, no display needed)

```sh
cargo build --all-targets     # compiles all winit/glutin/glow/kira/ab_glyph usage
cargo test                    # all tests green (CPU-side logic only)
```

## M11 — the full flow (needs a real display + audio device)

```sh
cargo run                     # or: cargo run --release
cargo run -- --assets ./assets --seed 12345
```

The app opens borderless fullscreen on the **New Game screen** (design §5.5):
the maze **pattern** field (default `MazeFiles/*.maz`, seeded from
`config.toml`) shows a live "Pattern matches N files" count recomputed by the
legacy wildcard matcher on every edit; `Tab` switches between the pattern and
password fields; `Left`/`Right` cycle the **Game** combo (`Shift`+`Left`/
`Right` cycle **Team**); `Enter` = OK, `Esc` = Cancel. **An empty password
enables cheats** (PARITY, design §1.3). OK random-picks a matching maze,
persists the selection to `config.toml`, spawns the world, and drops onto the
**2D map** with the game **paused** (speed 0.0). Cancel **at startup quits**;
Cancel **mid-game resumes** the paused game.

The frame pump is `app::AppRunner::pump` (design §5.1): `dt = min(now−last,
1 s)`, `Game::execute_commands` runs **every** frame, `Game::advance` runs
only while focused (focus loss clears input + freezes the sim). The New Game
screen and all modals freeze the game.

### Controls (the bound keys come from `[ui].commands` in `GameData.toml`)

| Where | Action | Default command |
|---|---|---|
| 2D map | select a friendly (white) robot under the cursor | `SelectObserverRobot` |
| 2D map | **possess** the selected robot → enter 3D | `SetObserver` |
| 2D map | cancel back to 3D (disabled when forced by death) | `Exit2DMode` |
| 3D | accelerate / decelerate; no key = cruise | `Accelerate` / `Decelerate` |
| 3D | turn / elevate barrel | mouse X / mouse Y |
| 3D | change desired speed | mouse wheel |
| 3D | fire | `FireProjectiles` (+ per-projectile fire keys) |
| 3D | speed up / slow down / pause the sim | `IncreaseGameSpeed` / `DecreaseGameSpeed` / `Pause` |
| 3D | back to the 2D map | `Enter2DMode` |
| 3D | toggle the minimap / AI mode | `ToggleMap` / `ToggleAIMode` |
| 3D | **end game** confirm (Yes → New Game) | `EndGame` (`Esc`) |
| 3D | **restart** confirm (Yes → same game) | `Restart` (`F4`) |
| observer | fly (fwd/back/strafe/up/down) at 3 speeds | `MoveObserver*` / `MoveObserverSpeed1/2/3` |
| observer | **cheat fire** (empty-password games only, 0.03 s cooldown) | `FireProjectiles` |
| modal | Yes / No / Cancel | `Y` / `N` / `C` |
| profiler | running / visible / interaction | `ToggleProfiler{Running,Visible,Interaction}` (`P`/`[`/`]`) |

### Visual / feel gates to sign off (M8)

1. **Full loop**: start on the 2D map → select + possess a friendly robot →
   drive/turn/elevate/fire against the AI → get destroyed → the view is
   **forced back to the 2D map** and cannot be cancelled until you pick a new
   robot (`forced_by_death`).
2. **Feel at 60 Hz vsync**: movement/turn/fire feel is frame-rate coupled
   (variable dt, PARITY). Confirm the game-speed ramp (`T`/`G`) and pause
   (`Q`) behave; game speed has **no upper bound** and never goes below 0.
3. **Camera**: possessing poses the eye at the robot's viewpoint offset
   `(0,0, treadRadius·2)` looking down its barrel (yaw = heading, pitch =
   barrel elevation); `z_far = min(fMaxVisibleDist, |building|)`.
4. **HUD** (top-left): the live status block updates every frame — speed,
   energy/health, per-type reload + ammo, spawned-per-team counts, game
   speed %, game time, and the conditional lines (AI mode / enemy visible
   [0.25 s poll] / targeted / above center).

### M11 visual / feel gates to sign off (needs a real display, all 3 OSes)

1. **New Game screen**: launch → the pattern field shows the correct live
   match count (`MazeFiles/*.maz` ⇒ "matches 4 files"); editing the pattern
   updates it; `Left`/`Right` cycle Game, `Shift`+arrows cycle Team; typing a
   password masks with `*`; OK spawns the game, Cancel-at-startup quits.
2. **Config persistence**: after OK, `config.toml` (in the OS config dir)
   holds the maze **pattern**, `game_index`, `team_index`; relaunch defaults
   the New Game screen to those values.
3. **Cheat polarity**: start a game with an **empty** password → in 3D, `V`
   (`ToggleAIMode`) lets you take control of an enemy robot, and the free
   observer can cheat-fire. A **non-empty** password disables both.
4. **Win / lose / draw modal**: play (or fast-forward with `T`) until a team
   wins → the GameOver modal appears with the correct text; **all sounds stop
   the instant it opens**; `Y` replays the same game, `N` pops New Game, `C`
   keeps watching (modal does not re-open).
5. **EndGame / Restart confirms**: `Esc` → "quit the current game?" (Yes →
   New Game, No → resume); `F4` → same prompt (Yes → restart same maze).
6. **Observer fly**: on the 2D map `Ctrl+click`-place an observer point and
   possess it → fly with the `MoveObserver*` keys at speeds 1/2/3; pitch
   clamps at ±π/2; the camera cannot leave the building (0.0001 margin).
7. **Profiler**: `P` toggles profiling on, `[` shows the tree pane on the
   right third (game viewport shrinks to ⅔ conceptually), `]` toggles
   interaction; on **exit** `Profile output.txt` is written to the OS
   data-local dir (`dirs::data_local_dir()/operation-purge/`).

### Still pending / deferred (not M11)

- **Wall holes at 0.005 offset**, projectile/particle **sprite rendering**,
  and the **2D top-down map overlay** (incl. arrow-pan / `Ctrl`+arrow resize)
  are computed by the sim every frame but are not yet drawn — sprite/overlay
  draw wiring rides on the M5 sprite batcher. The New Game screen, modals, and
  profiler pane are drawn as **plain overlay text** (top-left / right-third);
  a boxed dialog layout is cosmetic follow-up.
- **Per-event 3D sounds** (fire/collision/destroy): clips are preloaded and
  voice pools registered; the DS3D **listener** (every 10th frame) + **alarm
  loops** are driven and **stopped before any modal** (design §5.4). Emitting
  the per-event one-shots needs a World frame-event log (follow-up). Audio is
  muted automatically if no output device is available.
- **Internal-error fatal modal**: `ConfirmKind::InternalError` + its resolve
  path (Yes → exit) exist; wiring the errorlog channel to raise it at runtime
  is a follow-up (the modal machinery and prompt are in place).

## (Retained) M4/M5 — winding gate + static scene notes

The headless suite covers the M4 correctness proxies: the wall-outline
tracer and building extrusion (positions / winding order / UVs against a
hand-computed 1×1 room), the triangle-fan→list conversion, the u16 mesh
GUARD, the four shader sources' PARITY constants, and the camera MVP
(including the `z' = 2z - w` clip fixup at the near/far planes).

> **Note (M8):** the interactive **free-fly debug camera** and its winding
> keys (`C`/`F`/`V`) have been **removed** — `cargo run` now launches the M8
> game loop (above). Culling still defaults to `WindingDecision::CullNone`
> (nothing is culled), so the winding checklist below can still be eyeballed
> from the possessed-robot camera; the lighting/dimming and UV-tiling checks
> apply to the live robots + building rendered in that loop. The depth-fixup
> and winding correctness are pinned by the `render::d3dx` / `render::camera`
> unit tests regardless.

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
