# Running Operation Purge

The port is developed and tested **headless** (no display server), so all
CPU-side logic is covered by `cargo test`. The window, GL context, shaders,
audio device, and the play/feel/visual behaviour can only be exercised at a
real desktop. `cargo run` launches the full flow: the app opens on the
in-engine **New Game screen** (maze pattern + live match count, Game/Team
combos, password), and OK spawns the world and drops onto the 2D map →
possess a robot → fight the AI in 3D → win/lose/draw pops a **GameOver
modal** (replay / new game / keep watching) → quit via the EndGame confirm.
This note explains how to launch it and what to eyeball.

## Build & test (any machine, no display needed)

```sh
cargo build --all-targets     # compiles all winit/glutin/glow/kira/ab_glyph usage
cargo test                    # all tests green (CPU-side logic only)
```

## The full flow (needs a real display + audio device)

```sh
cargo run                     # or: cargo run --release
cargo run -- --assets ./assets --seed 12345
```

The app opens borderless fullscreen on the **New Game screen**: the maze
**pattern** field (default `MazeFiles/*.maz`, seeded from `config.toml`)
shows a live "Pattern matches N files" count recomputed by the legacy
wildcard matcher on every edit; `Tab` switches between the pattern and
password fields; `Left`/`Right` cycle the **Game** combo (`Shift`+`Left`/
`Right` cycle **Team**); `Enter` = OK, `Esc` = Cancel. **An empty password
enables cheats** (PARITY, `cMainWindow.cpp:760-761`). OK random-picks a
matching maze, persists the selection to `config.toml`, spawns the world, and
drops onto the **2D map** with the game **paused** (speed 0.0). Cancel **at
startup quits**; Cancel **mid-game resumes** the paused game.

The frame pump is `app::AppRunner::pump`: `dt = min(now−last, 1 s)`,
`Game::execute_commands` runs **every** frame, `Game::advance` runs only
while focused (focus loss clears input + freezes the sim). The New Game
screen and all modals freeze the game.

Errors go to stderr and to `Logfile.txt` beside `config.toml` (the only
channel on the console-less Windows release build).

### Controls (compiled in, `src/input/commands.rs`; full key table in the README)

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
| profiler | record / show the pane | `ToggleProfiler{Running,Visible}` (`P`/`[`) |

### Visual / feel gates to sign off (all 3 OSes)

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
5. **New Game screen**: launch → the pattern field shows the correct live
   match count (`MazeFiles/*.maz` ⇒ "matches 21 files"); editing the pattern
   updates it; `Left`/`Right` cycle Game, `Shift`+arrows cycle Team; typing a
   password masks with `*`; OK spawns the game, Cancel-at-startup quits.
6. **Config persistence**: after OK, `config.toml` (in the OS config dir)
   holds the maze **pattern**, `game_index`, `team_index`; relaunch defaults
   the New Game screen to those values.
7. **Cheat polarity**: start a game with an **empty** password → in 3D, `V`
   (`ToggleAIMode`) lets you take control of an enemy robot, and the free
   observer can cheat-fire. A **non-empty** password disables both.
8. **Win / lose / draw modal**: play (or fast-forward with `T`) until a team
   wins → the GameOver modal appears with the correct text; **all sounds stop
   the instant it opens**; `Y` replays the same game, `N` pops New Game, `C`
   keeps watching (modal does not re-open).
9. **EndGame / Restart confirms**: `Esc` → "quit the current game?" (Yes →
   New Game, No → resume); `F4` → same prompt (Yes → restart same maze).
10. **Observer fly**: on the 2D map `Ctrl+click`-place an observer point and
    possess it → fly with the `MoveObserver*` keys at speeds 1/2/3; pitch
    clamps at ±π/2; the camera cannot leave the building (0.0001 margin).
11. **Profiler**: `P` toggles profiling on, `[` shows the tree pane on the
    right third (game viewport shrinks to ⅔ conceptually); on **exit**
    `Profile output.txt` is written to the OS data-local dir
    (`dirs::data_local_dir()/operation-purge/`).
12. **Lighting look** vs the help-doc screenshots: **dome dimming** from raw
    (un-normalized) normals, **dark ceiling** (up-normal, PARITY), **UV
    repeat tiling** on walls/floor, **map colors** after the COLORREF→RGBA
    conversion, and the **viewport offsets** (0.025 / 0.95 full-2D, 0.05 /
    0.9 minimap). Near geometry must occlude far geometry (`GL_LEQUAL` + the
    `z' = 2z − w` clip remap, pinned by the `math::d3dx` and `render::camera`
    unit tests).

### Still pending / deferred

- **Internal-error fatal modal**: `ConfirmKind::InternalError` + its resolve
  path (Yes → exit) exist, but nothing raises it yet; errors are only logged
  (see above).
- **Face culling** stays off (`WindingDecision::CullNone`). The original
  enabled `D3DCULL_CCW` for holes and later passes; with culling off those
  faces just draw both sides. Turning it on needs a per-category sign-off at
  a display: floor visible only from above, ceiling only from below, inner
  wall faces from inside rooms, and the reversed-winding outer boundary
  polygon (`cBuilding3D.cpp:187-193`) from inside the building. The expected
  mapping is `D3DCULL_CCW ≡ glFrontFace(GL_CCW) + glCullFace(GL_BACK)`;
  record the decision in `render/gl.rs`.
- **AI cost**: robots chasing an unseen target re-run the path search every
  frame, exactly like the original (`cAIRobot.cpp:464`). That is ~5 ms per
  frame with ~20 robots in release builds (see the `Move AI robots`
  profiler scope).
- **2D map zoom spills**: as in the original, a zoomed 2D map (Ctrl+arrows)
  still draws walls past the map's screen rectangle.

## Scripted E2E runs (no human needed)

`scripts/e2e.sh SCRIPT OUT_DIR [--seed N]` launches the real window
(1280×800, fixed 1/60 s steps) against a throwaway config, feeds it the
script's input, and writes PNG captures + state dumps into `OUT_DIR`. Real
keyboard/mouse input is ignored while a script runs. Grammar is documented
at the top of `src/app/e2e.rs`; a minimal session:

```text
frames 3
key Enter            # New Game OK
cursor_robot 0       # hover the first friendly robot on the 2D map
click left           # select it
key KeyF             # possess -> 3D
down KeyT            # speed the game up
frames 120
up KeyT
shot play            # OUT_DIR/play.png
state play           # OUT_DIR/play.txt (screen, robots, sounds played...)
quit
```

## Platform notes

- **macOS**: framebuffer is physical (Retina) resolution; `glViewport` uses
  physical pixels while HUD metrics use logical px × scale. Raw mouse deltas
  are NSEvent counts — tune look sensitivity via `mouse_scale` in config.
- **X11**: no locked cursor grab; raw `DeviceEvent::MouseMotion` still works.
- **Wayland**: `ScaleFactorChanged` re-bakes the glyph atlas.

If `assets/GameData.toml` or `Small1.maz` fails to load, the view still runs
and prints the reason in the HUD (GUARD: no crash).
