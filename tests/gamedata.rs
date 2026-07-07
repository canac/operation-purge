//! Golden-file tests for `GameData.toml` (design §4.1, §6; libs-map §2.4).
//!
//! `assets/GameData.toml` is both the shipped default AND the golden
//! fixture: it was produced by a one-time conversion of
//! `GameDataTest/GameData.xml` (design §4.6 item 3) and must parse to a
//! stable, snapshot-pinned `GameData` (post-conversion colors as `Rgba`,
//! angles in radians, all 36 commands, `Ref`s resolved to `Option<Id>`).

use operation_purge::data::gamedata;
use std::path::Path;

fn assets_gamedata_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/GameData.toml")
}

#[test]
fn shipped_gamedata_toml_parses_and_matches_the_golden_snapshot() {
    let data = gamedata::load(&assets_gamedata_path())
        .expect("assets/GameData.toml must load cleanly");
    insta::assert_debug_snapshot!(data);
}

#[test]
fn shipped_gamedata_toml_has_exactly_36_commands() {
    let data = gamedata::load(&assets_gamedata_path()).unwrap();
    assert_eq!(data.ui.commands.len(), 36);
}

#[test]
fn shipped_gamedata_toml_keeps_alpha_on_background_and_forces_it_opaque_elsewhere() {
    let data = gamedata::load(&assets_gamedata_path()).unwrap();
    // Background2DColor keeps alpha (design §4.1 keep-alpha list); its
    // source XML token was "0x60 0x00 0x40 0x00" (alpha=0x60).
    assert!((data.colors.background_2d.a - (0x60 as f32 / 255.0)).abs() < 1e-6);
    // Wall2DColor is a plain `Color`, forced opaque.
    assert_eq!(data.colors.wall_2d.a, 1.0);
}

#[test]
fn shipped_gamedata_toml_converts_degrees_to_radians() {
    let data = gamedata::load(&assets_gamedata_path()).unwrap();
    // RobotType "Robot" has Rotation rate = 720 (deg/s) in the source XML.
    let robot = &data.robot_types[0];
    assert!((robot.rotate_rate - 720f32.to_radians()).abs() < 1e-4);
}

#[test]
fn shipped_gamedata_toml_resolves_none_refs() {
    let data = gamedata::load(&assets_gamedata_path()).unwrap();
    // "Bullet" projectile type has Travel effect = None in the source XML.
    let bullet = data
        .projectile_types
        .iter()
        .find(|p| p.name == "Bullet")
        .expect("Bullet projectile type must exist");
    assert_eq!(bullet.travel_effect, None);
    assert!(bullet.fire_effect.is_some());
}

/// GUARD (design §4.1): `deny_unknown_fields` on every table — an unknown
/// key in `[building]` (a typo, e.g.) must be a load error, not a silent
/// drop.
#[test]
fn unknown_field_is_a_load_error() {
    let dir = std::env::temp_dir().join(format!(
        "op-gamedata-unknown-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("bad.toml");
    std::fs::write(
        &path,
        minimal_valid_toml().replace(
            "[building]",
            "[building]\nthis_field_does_not_exist = true",
        ),
    )
    .unwrap();

    let result = gamedata::load(&path);
    std::fs::remove_dir_all(&dir).ok();

    assert!(
        matches!(result, Err(operation_purge::error::DataError::Toml { .. })),
        "expected a Toml error for an unknown field, got {result:?}"
    );
}

/// GUARD (design §4.1, §1.3): the original never validated `*Ref` indices
/// and would index out of bounds at spawn time — the port must reject an
/// out-of-range `Ref` at load time instead.
#[test]
fn out_of_range_ref_is_a_load_error() {
    let dir = std::env::temp_dir().join(format!(
        "op-gamedata-badref-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("bad_ref.toml");
    // robot_types is empty, but a robot_group references robot_type = 0.
    let toml = format!(
        "{}\n\n[[game_definitions]]\nname = \"G\"\n\n\
         [[game_definitions.teams]]\nname = \"T\"\ncolor = \"0xFF0000\"\n\n\
         [[game_definitions.teams.robot_groups]]\nrobot_type = 0\nai_robot_type = \"None\"\n\
         count = 1\ninitial_delay = 0\ninitial_delay_variance = 0\nspawn_delay = 0\n\
         spawn_delay_variance = 0\nspawn_from_point = false\n",
        minimal_valid_toml()
    );
    std::fs::write(&path, toml).unwrap();

    let result = gamedata::load(&path);
    std::fs::remove_dir_all(&dir).ok();

    assert!(
        matches!(
            result,
            Err(operation_purge::error::DataError::RefOutOfRange { .. })
        ),
        "expected a RefOutOfRange error, got {result:?}"
    );
}

#[test]
fn fewer_than_36_commands_is_a_shape_error() {
    let dir = std::env::temp_dir().join(format!(
        "op-gamedata-shortcommands-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("short.toml");
    std::fs::write(&path, minimal_valid_toml_with_n_commands(35)).unwrap();

    let result = gamedata::load(&path);
    std::fs::remove_dir_all(&dir).ok();

    assert!(
        matches!(result, Err(operation_purge::error::DataError::Shape { .. })),
        "expected a Shape error for a 35-entry commands table, got {result:?}"
    );
}

/// A minimal-but-complete `GameData.toml` (all required tables present,
/// zero-length arrays elsewhere) used as a base for the negative tests
/// above.
fn minimal_valid_toml() -> String {
    minimal_valid_toml_with_n_commands(36)
}

fn minimal_valid_toml_with_n_commands(n: usize) -> String {
    let mut out = String::new();
    out.push_str(
        r#"
[colors]
background_2d = "0x60000000"
wall_2d = "0xFF0000"
barrel_2d = "0x808080"
observer_2d = "0xFFFFFF"
crosshair_2d = "0x808000"
status_text_2d = "0xFFFFFF"
barrel_3d = "0x808080"
tread_3d = "0x404040"

[building]
wall_width = 1
room_size = 10
ceiling_height = 5
wall_texture_size = 1
floor_texture_size = 1
ceiling_texture_size = 1

[sounds]
simultaneous_instances = 10

[ui]
rotate_rate = 1
elevate_rate = 1
accelerate_rate = 1
game_speed_rate = 1
resize_building_rate = 1
move_building_rate = 1
move_observer_default_rate = 1
move_observer_speed1_rate = 1
move_observer_speed2_rate = 1
move_observer_speed3_rate = 1

[center_types]
"#,
    );
    for _ in 0..n {
        out.push_str("\n[[ui.commands]]\nkey = \"A\"\ncontrol = false\nalt = false\nshift = false\n");
    }
    out
}
