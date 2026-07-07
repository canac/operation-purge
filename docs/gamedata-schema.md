# `GameData.toml` schema

`assets/GameData.toml` is the single data-driven definition of the game:
robot / AI / weapon tuning, teams, supply centers, particle effects, colors,
and key bindings. It is a **one-time XML→TOML conversion** of the original
`GameData.xml` (design §4.2) — same *field inventory* as the 2011 build, but a
serde-friendly, human-editable shape. The binary only ever parses TOML; it is
**not** interchangeable with the original XML.

The Rust reader is `serde`-derived (`src/data/mod.rs`) with
`deserialize_with` conversion helpers in `src/data/values.rs`. Conversions
fold in at parse time, so the parsed values are already in final form — no
second mutate pass.

## Value conventions

| Authoring form | In `.toml` | In memory |
|---|---|---|
| Boolean / integer / float | native TOML scalar | `bool` / `u32` / `f32` (non-negative validated on load) |
| **Color** (opaque) | `"0xRRGGBB"` hex string | `Rgba { r, g, b, a }` f32, `a = 1.0` |
| **AlphaColor** (with alpha) | `"0xAARRGGBB"` hex string — **alpha first** | `Rgba` f32 |
| **Key** | display-name string (e.g. `"F4"`, `"Left mouse button"`) | ordinal in the 100-entry `src/input/keys.rs` table |
| **Ref** | `"None"` or a 0-based integer index | `Option<Index>`; bounds-validated **after** the whole file loads (GUARD) |

Notes:

- **Colors are `0xRRGGBB`, alpha-colors are `0xAARRGGBB` (alpha first)** —
  matching the original's lexeme. The COLORREF `0xAABBGGRR` byte order never
  exists in Rust memory; the conversion lands straight in `Rgba` f32.
- **Alpha is kept** for `background_2d`, all three center-type `color` lists,
  and all particle-stage colors. Alpha is **forced opaque** for the other
  seven `[colors]` fields and for team colors (`src/data/values.rs`).
- **Angles are authored in degrees for readability and converted to radians**
  on load, on exactly these fields (design §4.2, `cOperationPurge.cpp:2600-2691`
  — nothing else is transformed):
  `robot_type.rotate_rate`, `projectile.accuracy`,
  `particle_type.initial_rotation(+_variance)`,
  `particle_type.rotation_rate(+_variance)`,
  `emitter.emit_theta_min/max`, `emitter.emit_phi_min/max`.
- A `Ref` string `"None"` becomes `Option::None`; an out-of-range index is a
  **load error** (GUARD) — refs are validated once the whole file is parsed so
  forward references resolve.
- Unknown keys are a load error; the `[ui].commands` list must contain exactly
  **36** entries (the fixed command ABI).

## Tables

### `[colors]`

2D/3D palette. `background_2d` is an `AlphaColor`; the rest are opaque
`Color`.

| Key | Kind |
|---|---|
| `background_2d` | AlphaColor |
| `wall_2d`, `barrel_2d`, `observer_2d`, `crosshair_2d`, `status_text_2d`, `barrel_3d`, `tread_3d` | Color |

### `[building]`

Maze→world scaling and UV tiling. `wall_width`, `room_size`,
`ceiling_height` are the extrusion constants (see
[maze-format.md](maze-format.md)); the `*_texture_size` keys are UV tiling
scales. The wall/floor/ceiling texture *filenames* are **not** data — they
are compiled in (`src/data/assets.rs`); see the
[README asset section](../README.md#assets).

### `[sounds]`

The DS3D voice budget: `simultaneous_instances` (concurrent-voice cap). The
two alarm-loop clip *filenames* are compiled in (`src/data/assets.rs`), not
data.

### `[ui]`

Input rates plus the command→key binding table.

- **Rates:** `rotate_rate`, `elevate_rate`, `accelerate_rate`,
  `game_speed_rate`, `resize_building_rate`, `move_building_rate`,
  `move_observer_default_rate`, `move_observer_speed1_rate`,
  `move_observer_speed2_rate`, `move_observer_speed3_rate`.
- **`[[ui.commands]]`** — exactly **36** entries, **positional**: entry `N`
  binds command `N` of the fixed `Command` enum ABI
  (`src/input/commands.rs`; order matches the original
  `cGameData::s_aszCommandsNames`). Each entry is
  `{ key = "<display name>", control = <bool>, alt = <bool>, shift = <bool> }`.
  Modifier matching is **not exact** (`bExactModifiers = false`): a command
  with no required modifiers still fires when extra modifiers are held. See
  the [README controls table](../README.md#controls) for the resolved
  key → action mapping.

### `[[game_definitions]]`

An array of selectable matches. Each has a `name`, an array of `teams`, and
`ammo_center_groups` / `energy_center_groups` / `health_center_groups`.

- **`[[game_definitions.teams]]`** — `name`, `color` (opaque `Color`,
  team-color alpha is forced opaque), and `robot_groups`.
- **`[[game_definitions.teams.robot_groups]]`** — a spawn cohort:
  `robot_type` and `ai_robot_type` are `Ref` indices into `[[robot_types]]` /
  `[[ai_robot_types]]`; `count`; `initial_delay(+_variance)`,
  `spawn_delay(+_variance)` (seconds); `spawn_from_point` (spawn at the team's
  point vs a fresh random position).
- **`*_center_groups`** — `{ center_type = <Ref>, count = <int> }` into the
  matching `[center_types.*]` list.

### `[[robot_types]]`

Chassis stats: `name`, `accel`, `brake_decel`, `friction_decel`, `max_speed`,
`rotate_rate` (**degrees→radians**), `max_visible_dist`, `energy_start`,
`health_start`, `projectile_armor`, `crash_armor`, `destroy_effect` (`Ref`
into particle effects). (The destroy *sound* is compiled in, not data.)

- **`[[robot_types.projectile_data]]`** (optional, repeatable) — the weapons
  this chassis carries: `projectile_type` (`Ref`), `count` (ammo),
  `fire_rate` (seconds between shots), `accuracy` (**degrees→radians**;
  angular scatter). A robot type with no `projectile_data` is unarmed.

### `[[ai_robot_types]]`

AI behaviour tuning: `name`, `desired_speed`, `scan_delay`, `min_wall_dist`,
`obstacle_impulse`, and the resupply thresholds
`desired_ammo / _energy / _health` and `critical_ammo / _energy / _health`
(0–1 fractions).

### `[[projectile_types]]`

Weapon projectiles: `name`, `initial_speed`, `damage(+_variance)`,
`gravity_accel`, `size`, `hole_size`, `fire_effect` / `travel_effect` /
`collision_effect` (each a `Ref` — `"None"` allowed), and `fire_command`
(`{ key, control, alt, shift }` inline table). (The projectile/hole
*textures* and fire/collision *sounds* are compiled in, not data.)

### `[center_types.ammo]` / `[center_types.energy]` / `[center_types.health]`

Supply-center kinds (arrays). Each has `name`, `size`, `color`
(**AlphaColor** — alpha kept), and a transfer spec:

- **ammo** carries `[[center_types.ammo.transfer_rates]]` =
  `{ projectile_type = <Ref>, transfer_rate = <f32> }` (per weapon). An ammo
  center with **no entry** for a fired projectile type transfers nothing
  (PARITY).
- **energy** / **health** carry a single `transfer_rate` scalar.

### `[[particle_types]]`

Per-particle appearance/motion: `name` (the particle *texture* is compiled
in, not data), `lifetime(+_variance)`, `inherited_velocity_factor`, `rise_speed`,
`rise_acceleration`, `initial_rotation(+_variance)` (**degrees→radians**),
`rotation_rate(+_variance)` (**degrees→radians**), and an ordered list of
color/size stages.

- **`[[particle_types.stages]]`** — `color` (**AlphaColor**, alpha kept),
  `size`, `time` (0..1 normalized lifetime position; the particle lerps
  through stages).

### `[[particle_emitter_types]]`

How particles are spawned: `name`, `lifetime(+_variance)`, `emit_by_distance`
(distance-driven vs time-driven), `emit_rate(+_variance)` (a *delay* between
emits — seconds, or distance units when `emit_by_distance`),
`emit_relative`, `emit_theta_min/max` and `emit_phi_min/max` (spherical
emission cone, **degrees→radians**), `emit_velocity(+_variance)`.

### `[[particle_effect_definitions]]`

Named composite effects referenced by `*_effect` fields elsewhere. Each has a
`name` and a list of `components`, each pairing one particle type with one
emitter type:

- **`[[particle_effect_definitions.components]]`** —
  `{ particle_type = <Ref>, particle_emitter_type = <Ref> }`.

## Golden test

The shipped `assets/GameData.toml` is pinned by an `insta` snapshot of the
fully-parsed `GameData` (post-conversion colors, radians, 36 commands, `None`
refs). Negative tests assert that an unknown key and an out-of-range `Ref`
each error, and that fewer than 36 commands is a shape error
(`tests/gamedata.rs`).
