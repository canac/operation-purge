//! `GameData.toml` loading: the shipped file's converted values, and the
//! load-time rejections (unknown keys, bad refs, data the compiled-in
//! bindings and assets can't serve).

use operation_purge::data::gamedata;
use operation_purge::error::DataError;
use std::path::Path;

fn shipped() -> operation_purge::data::GameData {
    gamedata::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/GameData.toml"))
        .expect("assets/GameData.toml must load cleanly")
}

fn parse(text: &str) -> Result<operation_purge::data::GameData, DataError> {
    gamedata::parse(text, Path::new("test.toml"))
}

#[test]
fn shipped_gamedata_keeps_alpha_only_where_authored() {
    let data = shipped();
    // Authored as "0x60004000": alpha byte first.
    assert!((data.colors.background_2d.a - 0x60 as f32 / 255.0).abs() < 1e-6);
    assert!((data.colors.background_2d.g - 0x40 as f32 / 255.0).abs() < 1e-6);
    assert_eq!(data.colors.wall_2d.a, 1.0);
    assert_eq!((data.colors.wall_2d.r, data.colors.wall_2d.g), (1.0, 0.0));
}

#[test]
fn shipped_gamedata_converts_degrees_to_radians() {
    let data = shipped();
    assert!((data.robot_types[0].rotate_rate - 720f32.to_radians()).abs() < 1e-4);
    let phi_min = data.particle_emitter_types.iter().map(|e| e.emit_phi_min).fold(0.0, f32::min);
    assert!((phi_min - (-90f32).to_radians()).abs() < 1e-6);
}

#[test]
fn shipped_gamedata_has_the_expected_shape_and_refs() {
    let data = shipped();
    let names: Vec<_> = data.projectile_types.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["Bullet", "Rocket", "Better Bullet"]);
    assert_eq!(data.projectile_types[0].travel_effect, None);
    assert!(data.projectile_types[0].fire_effect.is_some());
    assert_eq!(data.game_definitions[0].name, "2 Teams Caleb");
    let group = &data.game_definitions[0].teams[1].robot_groups[0];
    assert_eq!((group.robot_type.map(|r| r.get()), group.count), (Some(0), 25));
    assert_eq!(data.sounds.simultaneous_instances, 10);
    assert_eq!(data.building.room_size, 10.0);
}

#[test]
fn minimal_toml_is_valid() {
    parse(&minimal_valid_toml()).unwrap();
}

#[test]
fn unknown_field_is_a_load_error() {
    let text = minimal_valid_toml().replace("[building]", "[building]\nnot_a_field = true");
    assert!(matches!(parse(&text), Err(DataError::Toml { .. })));
}

#[test]
fn out_of_range_ref_is_a_load_error() {
    let text = minimal_valid_toml().replace("robot_type = 0", "robot_type = 1");
    assert!(matches!(parse(&text), Err(DataError::RefOutOfRange { .. })));
}

#[test]
fn none_ref_the_spawner_needs_is_a_load_error() {
    for (from, to) in [
        ("robot_type = 0", "robot_type = \"None\""),
        ("ai_robot_type = 0", "ai_robot_type = \"None\""),
        ("center_type = 0", "center_type = \"None\""),
    ] {
        let text = minimal_valid_toml().replace(from, to);
        assert!(matches!(parse(&text), Err(DataError::Invalid { .. })), "{to}");
    }
}

#[test]
fn projectile_types_must_match_the_compiled_in_bindings() {
    let base = minimal_valid_toml();
    let (start, end) = (base.rfind("[[projectile_types]]").unwrap(), base.find("[center_types]").unwrap());
    let two_types = format!("{}{}", &base[..start], &base[end..]);
    assert!(matches!(parse(&two_types), Err(DataError::Invalid { .. })));

    let swapped = base.replace("name = \"Rocket\"", "name = \"Missile\"");
    assert!(matches!(parse(&swapped), Err(DataError::Invalid { .. })));
}

#[test]
fn negative_speed_is_a_load_error() {
    let text = minimal_valid_toml().replacen("initial_speed = 1", "initial_speed = -1", 1);
    assert!(matches!(parse(&text), Err(DataError::Invalid { .. })));
    let text = minimal_valid_toml().replace("max_speed = 1", "max_speed = nan");
    assert!(matches!(parse(&text), Err(DataError::Invalid { .. })));
}

fn minimal_valid_toml() -> String {
    let projectile = |name: &str| {
        format!(
            "[[projectile_types]]\nname = \"{name}\"\ninitial_speed = 1\ndamage = 1\n\
             damage_variance = 0\ngravity_accel = 0\nsize = 1\nhole_size = 1\n\
             fire_effect = \"None\"\ntravel_effect = \"None\"\ncollision_effect = \"None\"\n\n"
        )
    };
    format!(
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

[[game_definitions]]
name = "G"

[[game_definitions.teams]]
name = "T"
color = "0xFF0000"

[[game_definitions.teams.robot_groups]]
robot_type = 0
ai_robot_type = 0
count = 1
initial_delay = 0
initial_delay_variance = 0
spawn_delay = 0
spawn_delay_variance = 0
spawn_from_point = false

[[game_definitions.health_center_groups]]
center_type = 0
count = 1

[[robot_types]]
name = "R"
accel = 1
brake_decel = 1
friction_decel = 1
max_speed = 1
rotate_rate = 1
max_visible_dist = 1
energy_start = 1
health_start = 1
projectile_armor = 1
crash_armor = 1
destroy_effect = "None"

[[ai_robot_types]]
name = "A"
desired_speed = 1
scan_delay = 1
min_wall_dist = 1
obstacle_impulse = 1
desired_ammo = 1
desired_energy = 1
desired_health = 1
critical_ammo = 1
critical_energy = 1
critical_health = 1

{}{}{}[center_types]

[[center_types.health]]
name = "H"
size = 1
color = "0xFF00FF00"
transfer_rate = 1
"#,
        projectile("Bullet"),
        projectile("Rocket"),
        projectile("Better Bullet"),
    )
}
