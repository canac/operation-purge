# Operation Purge

A faithful Rust + OpenGL port of the 2011 C++/Direct3D9 game **Operation
Purge** by CaNaC Games. The port reproduces the original's simulation, AI,
rendering, and audio **bug-for-bug** where the behaviour is observable, while
replacing the Win32/Direct3D9/DirectSound/registry/TinyXML stack with a
portable, pure-Rust one (winit + glutin + glow, kira, serde/TOML).

## What the game is

You command robots inside a top-down maze that is extruded into a first-person
3D arena. From the **2D map** you select and *possess* one of your team's
robots; from its cockpit you drive, aim, and fire bullets and rockets at rival
teams while the AI drives everyone else. Robots restock ammo/energy/health at
scattered supply centers. A match ends when one team is the last standing
(win / lose / draw), and a game-over modal offers replay, a new game, or
"keep watching".

The maze geometry comes from `.maz` files; robot/AI/weapon tuning, teams,
particle effects, and colors are data-driven from a single `GameData.toml`.
Asset paths and key bindings are compiled in (`src/data/assets.rs`,
`src/input/commands.rs`) — the game ships one fixed set of each.

## Build & run

Requires **Rust 1.96+** (edition 2021).

```sh
cargo build --release        # build everything
cargo test                   # full headless test suite (no display/audio needed)
cargo run --release          # launch the game
```

Optional flags:

```sh
cargo run --release -- --assets ./assets   # override the assets directory
cargo run --release -- --seed 12345        # fixed RNG seed (reproducible spawns)
```

`--assets <dir>` (or the `OP_ASSETS` env var) points at the asset tree;
`--seed <u64>` overrides the config's seed. A hidden `--fixed-dt <secs>` flag
exists only for deterministic testing and does not drive the interactive path.

The game opens **borderless fullscreen** on the in-engine **New Game** screen
(maze pattern + live match count, Game/Team selectors, password), spawns the
world on OK, and drops onto the paused 2D map. See [`RUN.md`](RUN.md) for the
full launch walkthrough and the manual sign-off checklists.

### Assets

The `assets/` tree keeps the original subdirectory conventions. Every
texture and sound is the **original recovered from `OperationPurge.msi`**,
normalized offline to one standard format (see
[Assets: provenance & format](#assets-provenance--format)):

```
assets/
  GameData.toml          game-data (see docs/gamedata-schema.md)
  MazeFiles/*.maz         maze geometry (see docs/maze-format.md)
  Textures/*.png          wall/floor/ceiling + projectile/hole/smoke (PNG RGBA8)
  Sounds/*.wav            alarms + weapon/destroy sounds (16-bit PCM mono)
  Fonts/*.ttf             HUD/UI font (LiberationSans-Bold, SIL OFL)
```

Asset **filenames are compiled in** (`src/data/assets.rs`), not read from
`GameData.toml` — the game ships a fixed asset set, so the paths are baked
into the binary (same rationale as the hardcoded `Objects`/`LightData`).

## Controls

Key bindings are **compiled in** (`COMMAND_BINDINGS` / `FIRE_COMMANDS` in
`src/input/commands.rs`), transcribed from the original `GameData.xml`. They
are positional against the fixed command ABI — the same enum order the 2011
build bound against (design §5, help-doc §1):

| Context | Action | Default key |
|---|---|---|
| Global | End game (confirm → New Game) | `Esc` |
| Global | Restart same game (confirm) | `F4` |
| Global | Toggle minimap | `F1` |
| Global | Info / HUD | `F2` |
| Global | Profiler: run / show tree / interact | `P` / `[` / `]` |
| 2D map | Select friendly robot under cursor | Left mouse |
| 2D map | Place observer point under cursor | `Ctrl`+Left mouse |
| 2D map | Possess the selection → enter 3D | `F` (`SetObserver`) |
| 2D map | Pan map right/down/left/up | `Right` / `Up` / `Left` / `Down` \* |
| 2D map | Resize building (w/h ±) | `Ctrl`+arrows |
| 3D | Back to the 2D map | `A` (`Enter2DMode`) |
| 3D | Cancel back to 3D (if allowed) | `S` (`Exit2DMode`) |
| 3D | Accelerate / decelerate (no key = cruise) | `E` / `D` |
| 3D | Turn / elevate barrel | mouse X / mouse Y |
| 3D | Change desired speed | mouse wheel |
| 3D | Fire all weapons | `Space` |
| 3D | Fire bullet / rocket | Left mouse / Right mouse |
| 3D | Game speed up / down / pause | `T` / `G` / `Q` |
| 3D | Toggle AI mode (cheat: take an enemy) | `V` |
| Observer | Fly fwd/back/right/left/up/down | `E` / `D` / `F` / `S` / `R` / `V` |
| Observer | Fly speed 1 / 2 / 3 | `1` / `2` / `3` |
| Modal | Yes / No / Cancel | `Y` / `N` / `C` |

\* **Arrow-pan inversion is intentional (PARITY):** the down-arrow pans the map
"up" and vice-versa, matching `cOperationPurge`'s 2D-map handler. Several keys
are deliberately reused across contexts (e.g. `E`/`D`/`F`/`S`/`V` mean one
thing to a possessed robot and another to the free observer) exactly as in the
2011 build.

An **empty New Game password enables cheats** (`ToggleAIMode` to seize an
enemy robot, and free-observer cheat-fire); a non-empty password disables both.
This is faithful to the original (`cMainWindow.cpp:760-761`).

## Configuration

User preferences replace the original's `HKCU` registry values with a TOML
file:

- **Location:** `dirs::config_dir()/operation-purge/config.toml`
  - macOS: `~/Library/Application Support/operation-purge/config.toml`
  - Linux: `~/.config/operation-purge/config.toml`
  - Windows: `%APPDATA%\operation-purge\config.toml`
- Every key is optional; a missing key or an unparseable file silently falls
  back to the default (reproducing the "registry read failure ⇒ dialog
  default" semantics). The file is written **only** when you accept the New
  Game dialog, never eagerly.

| Key | Type | Default | Meaning |
|---|---|---|---|
| `assets_dir` | string | `"./assets"` | Asset tree. Relative: tried beside the exe, then the working dir, else the embedded copy is unpacked to the data dir. |
| `maze_file` | string | `"MazeFiles/*.maz"` | Maze path **or glob pattern**; the pattern is persisted, not the resolved file. |
| `game_index` | int | `0` | Selected game definition. |
| `team_index` | int | `0` | Selected team. |
| `vsync` | bool | `true` | `false` = the original uncapped loop. |
| `fps_cap` | int? | none | Frame cap (only with `vsync = false`). |
| `mouse_scale` | float | `1.0` | Multiplies raw mouse deltas (platform-dependent units). |
| `hud_scale` | float? | none | HUD scale; none = monitor DPI factor. |
| `seed` | int? | none | RNG seed; none = time-seeded. |

### Registry / GameData migration

The original stored data in two places the port deliberately does **not** read:

1. **Registry (`HKCU`) prefs** → replaced by the TOML `config.toml` above.
   There is no automatic import of old registry values; re-select your maze /
   game / team once in the New Game dialog and it persists. When hand-porting
   an old `MazeFile` pattern, normalize Windows `\` separators to `/`.

2. **`GameData.xml`** → **`GameData.toml`**. The game-data file is
   **not format-compatible** with the original XML (or the old
   OptionsEditor). It carries the identical *field inventory* but in a
   serde-friendly TOML shape with human-readable conventions (colors as hex
   strings, angles in degrees). **The migration path is a one-time XML→TOML
   conversion** — the shipped `assets/GameData.toml` already is that
   conversion of `GameDataTest/GameData.xml`; the binary only ever parses
   TOML. See [`docs/gamedata-schema.md`](docs/gamedata-schema.md).

### Assets: provenance & format

Every shipped asset is the **verified original**, recovered from the game's
own installer `OperationPurge.msi` (the `AppData` payload cabinet). They are
normalized offline to a single standard format so the runtime loaders stay
simple (one decode path, no per-format branching):

- **Textures → PNG RGBA8.** The originals were a mix of 24-bit and 4-bit BMP;
  all are converted to PNG (RGB→RGBA is lossless). Set:
  `Brick` (wall), `Steel` (floor), `Ceiling`, `Projectile`, `Hole`, `Smoke`.
- **Sounds → 16-bit signed PCM, mono WAV** (native sample rate kept; kira
  resamples to the device rate). The originals mixed 8-bit and 16-bit PCM at
  11025/22050/11127 Hz. Set: `FastAlarm`, `Alarm`, `BulletFire`,
  `BulletCollision`, `RocketFire`, `RocketCollision`, `Destroy`.
- **`MazeFiles/*.maz`** — the full original set (Big1/2, BigBlank, BigRandom,
  Small1–15, SmallBlank, SmallRandom), byte-for-byte from the MSI.
- **`Objects.txt` / `LightData.txt`** — recovered and transcribed into the
  compiled-in constants in `src/data/builtin.rs`; these are now **verified
  originals**, not reconstructions. `LIGHT_DATA` is `att = 1/(0.5 + 0.25·d)`;
  `OBJECTS` uses barrel resolution 12 and projectile resolution 16 (which the
  shipped `Objects.txt` set, overriding the divergent hardcoded C++ fallbacks).

`tests/assets.rs` pins the compiled-in asset registry against what is on
disk (every registered texture/sound must exist) and asserts the standard
format (16-bit mono PCM; RGBA8 PNG), so a missing or mis-encoded asset can't
silently ship. The offline conversion itself is a throwaway step (no dev tool
is kept in-tree); only `mazegen` remains, for authoring new mazes.

## Platform notes

- **macOS:** the framebuffer is physical (Retina) resolution — `glViewport`
  uses physical pixels while HUD metrics use logical px × scale factor. Raw
  mouse deltas are NSEvent counts, not pixels; tune look sensitivity with
  `mouse_scale`. GL is core-profile **4.1** (Apple's ceiling); the shaders
  target that.
- **Windows:** DXGI/DWM compositor; standard winit/glutin path.
- **Linux/X11:** works with raw `DeviceEvent::MouseMotion`; no locked cursor
  grab.
- **Linux/Wayland:** `ScaleFactorChanged` re-bakes the glyph atlas on DPI
  change.

If `GameData.toml` or a maze file fails to load, the app does not crash — it
keeps running and surfaces the reason (GUARD).

## Packaging

The build itself has **no packaging dependency** — `cargo build --release`
produces the binary. Distributable bundles are an optional extra step:

- **macOS `.app`** via [`cargo-bundle`](https://crates.io/crates/cargo-bundle):
  ```sh
  cargo install cargo-bundle
  cargo bundle --release        # produces target/release/bundle/osx/*.app
  ```
  Then copy the `assets/` tree next to the bundle (or into its
  `Resources/`) and point `assets_dir` at it.
- **Windows (single `.exe`, cross-built from macOS):** the `assets/` tree is
  compiled into the binary. On first run, if no `assets/` sits beside the exe
  or in the working dir, it is unpacked to `%APPDATA%\operation-purge\assets`
  (refreshed when the version changes; user-added mazes are kept).
  ```sh
  brew install mingw-w64
  rustup target add x86_64-pc-windows-gnu
  scripts/build-windows.sh      # -> target/x86_64-pc-windows-gnu/release/operation-purge.exe
  ```
  Ship just the `.exe`. It has no console window and needs no extra DLLs on
  Windows 10+.
- **Linux:** same embedding applies, so the release binary runs alone; it
  unpacks to `~/.local/share/operation-purge/assets`.

## Standing visual / audio gates (need desktop sign-off)

The port is developed and verified **headless**: all CPU-side logic is covered
by `cargo test`, but the window, GL context, shaders, audio device, and the
play/feel are only exercisable on a real desktop. The following still need a
human at a display (all three OSes) — see [`RUN.md`](RUN.md) for the detailed
per-item procedure:

- **M4 winding / culling checklist** — floor visible only from above; ceiling
  visible-and-dark only from below; inner wall faces; reversed outer-boundary
  polygon. Culling ships **disabled** (`CullNone`) until each item is signed
  off and a concrete `WindingDecision` is baked in.
- **M5 lighting look** vs help-doc screenshots — dome dimming from raw
  (un-normalized) normals, dark up-normal ceiling, ambient `0x202020`, UV
  repeat tiling on walls/floor, map colors after the COLORREF→RGBA
  conversion, viewport offsets (0.025/0.95 full-2D, 0.05/0.9 minimap), and the
  depth-fixup (`z' = 2z − w` + `GL_LEQUAL`) occlusion check. Retune
  `LIGHT_DATA` here if the scene reads too dark/bright.
- **Text crispness** — HUD/UI glyph atlas at each DPI scale factor.
- **Particle trails** — gap-free rocket trails and projectile black
  silhouettes at 25% and 400% game speed.
- **Audio spatialization** — pan/attenuation listening at <10 / 50 / >100
  units; the DS3D listener update (every 10th frame); enemy-visible /
  targeted alarm loops; **all sounds stop the instant a modal opens**.
- **Full play loop feel** — New Game screen live match count + config
  persistence; possess → drive/turn/fire against the AI at 60 Hz vsync;
  forced-back-to-2D on death; win/lose/draw modal; EndGame/Restart confirms;
  observer free-fly at speeds 1/2/3; profiler pane + `Profile output.txt` on
  exit.

## Documentation

- [`docs/quirks.md`](docs/quirks.md) — the parity ledger: every `PARITY` /
  `GUARD` / `FIX` comment paired with its C++ and Rust `file:line`
  (auto-generated by `scripts/gen_quirks.sh`).
- [`docs/maze-format.md`](docs/maze-format.md) — the `.maz` binary format.
- [`docs/gamedata-schema.md`](docs/gamedata-schema.md) — the `GameData.toml`
  schema and value conventions.
- [`RUN.md`](RUN.md) — launch instructions and the manual sign-off checklists.

## License

MIT — see [`LICENSE`](LICENSE). The bundled font `LiberationSans-Bold.ttf` is
under the SIL Open Font License (see `assets/Fonts/LICENSE.txt`).
