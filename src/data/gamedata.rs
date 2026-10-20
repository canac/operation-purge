//! `GameData.toml` loader.
//!
//! GUARD: the original's `cGameData::ReadFileXML` ignored
//! `TiXmlDocument::LoadFile`'s boolean result and then unconditionally
//! dereferenced `FirstChild("GameData")->ToElement()` — a missing or
//! malformed file crashed outright (`cGameData.cpp:405-413`). Here every
//! failure mode is a caught, typed `DataError`.

use super::GameData;
use crate::data::assets::names;
use crate::error::DataError;
use crate::input::commands::FIRE_COMMANDS;
use std::path::Path;

/// Reads and [`parse`]s `path`.
pub fn load(path: &Path) -> Result<GameData, DataError> {
    let text = std::fs::read_to_string(path).map_err(|source| DataError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    parse(&text, path)
}

/// Parses `text` as `GameData.toml` and [`validate`]s it. `path` only
/// labels errors.
pub fn parse(text: &str, path: &Path) -> Result<GameData, DataError> {
    let data: GameData = toml::from_str(text).map_err(|e| DataError::Toml {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;
    validate(&data, path)?;
    Ok(data)
}

/// Everything the sim and app assume about loaded data: every `Ref` in
/// bounds, refs the sim dereferences unconditionally present, projectile
/// types matching the compiled-in fire bindings and assets, and speeds and
/// sizes non-negative.
pub fn validate(data: &GameData, path: &Path) -> Result<(), DataError> {
    validate_refs(data, path)?;
    validate_required(data, path)?;
    validate_projectile_types(data, path)?;
    validate_non_negative(data, path)
}

fn invalid(path: &Path, message: String) -> DataError {
    DataError::Invalid {
        path: path.to_path_buf(),
        message,
    }
}

/// GUARD: the original never validated `*Ref` indices and would index out
/// of bounds at spawn time.
fn validate_refs(data: &GameData, path: &Path) -> Result<(), DataError> {
    let n_robot_types = data.robot_types.len();
    let n_ai_robot_types = data.ai_robot_types.len();
    let n_projectile_types = data.projectile_types.len();
    let n_particle_types = data.particle_types.len();
    let n_emitter_types = data.particle_emitter_types.len();
    let n_effect_defs = data.particle_effect_definitions.len();
    let n_ammo_centers = data.center_types.ammo.len();
    let n_energy_centers = data.center_types.energy.len();
    let n_health_centers = data.center_types.health.len();

    let check = |field: &'static str, value: Option<u32>, len: usize| -> Result<(), DataError> {
        if let Some(index) = value {
            if index as usize >= len {
                return Err(DataError::RefOutOfRange {
                    path: path.to_path_buf(),
                    field,
                    index,
                    len,
                });
            }
        }
        Ok(())
    };

    for game_def in &data.game_definitions {
        for team in &game_def.teams {
            for group in &team.robot_groups {
                check(
                    "robot_groups.robot_type",
                    group.robot_type.map(|id| id.get()),
                    n_robot_types,
                )?;
                check(
                    "robot_groups.ai_robot_type",
                    group.ai_robot_type.map(|id| id.get()),
                    n_ai_robot_types,
                )?;
            }
        }
        for group in &game_def.ammo_center_groups {
            check("ammo_center_groups.center_type", group.center_type, n_ammo_centers)?;
        }
        for group in &game_def.energy_center_groups {
            check(
                "energy_center_groups.center_type",
                group.center_type,
                n_energy_centers,
            )?;
        }
        for group in &game_def.health_center_groups {
            check(
                "health_center_groups.center_type",
                group.center_type,
                n_health_centers,
            )?;
        }
    }

    for robot_type in &data.robot_types {
        check(
            "robot_types.destroy_effect",
            robot_type.destroy_effect.map(|id| id.get()),
            n_effect_defs,
        )?;
        for pd in &robot_type.projectile_data {
            check(
                "robot_types.projectile_data.projectile_type",
                pd.projectile_type.map(|id| id.get()),
                n_projectile_types,
            )?;
        }
    }

    for projectile_type in &data.projectile_types {
        check(
            "projectile_types.fire_effect",
            projectile_type.fire_effect.map(|id| id.get()),
            n_effect_defs,
        )?;
        check(
            "projectile_types.travel_effect",
            projectile_type.travel_effect.map(|id| id.get()),
            n_effect_defs,
        )?;
        check(
            "projectile_types.collision_effect",
            projectile_type.collision_effect.map(|id| id.get()),
            n_effect_defs,
        )?;
    }

    for ammo_center in &data.center_types.ammo {
        for rate in &ammo_center.transfer_rates {
            check(
                "center_types.ammo.transfer_rates.projectile_type",
                rate.projectile_type.map(|id| id.get()),
                n_projectile_types,
            )?;
        }
    }

    for effect_def in &data.particle_effect_definitions {
        for component in &effect_def.components {
            check(
                "particle_effect_definitions.components.particle_type",
                component.particle_type.map(|id| id.get()),
                n_particle_types,
            )?;
            check(
                "particle_effect_definitions.components.particle_emitter_type",
                component.particle_emitter_type.map(|id| id.get()),
                n_emitter_types,
            )?;
        }
    }

    Ok(())
}

/// Refs the spawners dereference without a `None` branch (`sim/world.rs`
/// robot and center spawning).
fn validate_required(data: &GameData, path: &Path) -> Result<(), DataError> {
    for game_def in &data.game_definitions {
        let missing = |field: &str| {
            invalid(path, format!("game `{}`: `{field}` must not be `None`", game_def.name))
        };
        for group in game_def.teams.iter().flat_map(|t| &t.robot_groups) {
            if group.robot_type.is_none() {
                return Err(missing("robot_groups.robot_type"));
            }
            if group.ai_robot_type.is_none() {
                return Err(missing("robot_groups.ai_robot_type"));
            }
        }
        let centers = [
            ("ammo_center_groups", &game_def.ammo_center_groups),
            ("energy_center_groups", &game_def.energy_center_groups),
            ("health_center_groups", &game_def.health_center_groups),
        ];
        for (list, groups) in centers {
            if groups.iter().any(|g| g.center_type.is_none()) {
                return Err(missing(&format!("{list}.center_type")));
            }
        }
    }
    Ok(())
}

/// Fire bindings and projectile assets are compiled in per projectile-type
/// index, so the data must list exactly that many types with the Rocket
/// where [`names::ROCKET_INDEX`] expects it.
fn validate_projectile_types(data: &GameData, path: &Path) -> Result<(), DataError> {
    let types = &data.projectile_types;
    if types.len() != FIRE_COMMANDS.len() {
        return Err(invalid(
            path,
            format!(
                "expected {} projectile_types (one per compiled-in fire binding), found {}",
                FIRE_COMMANDS.len(),
                types.len()
            ),
        ));
    }
    for (i, t) in types.iter().enumerate() {
        if (t.name == names::ROCKET_NAME) != (i == names::ROCKET_INDEX) {
            return Err(invalid(
                path,
                format!(
                    "projectile_types[{}] must be `{}` (its sounds and sprite are compiled in by index)",
                    names::ROCKET_INDEX,
                    names::ROCKET_NAME
                ),
            ));
        }
    }
    Ok(())
}

/// Negative speeds, accelerations, distances, and sizes break the sim's
/// clamps and comparisons, so they are rejected (NaN too).
fn validate_non_negative(data: &GameData, path: &Path) -> Result<(), DataError> {
    let check = |owner: &str, field: &str, v: f32| {
        if v >= 0.0 {
            Ok(())
        } else {
            Err(invalid(path, format!("`{owner}`: `{field}` must be non-negative, got {v}")))
        }
    };
    let b = &data.building;
    for (field, v) in [
        ("wall_width", b.wall_width),
        ("room_size", b.room_size),
        ("ceiling_height", b.ceiling_height),
    ] {
        check("building", field, v)?;
    }
    for t in &data.robot_types {
        for (field, v) in [
            ("accel", t.accel),
            ("brake_decel", t.brake_decel),
            ("friction_decel", t.friction_decel),
            ("max_speed", t.max_speed),
            ("max_visible_dist", t.max_visible_dist),
        ] {
            check(&t.name, field, v)?;
        }
    }
    for t in &data.ai_robot_types {
        for (field, v) in [
            ("desired_speed", t.desired_speed),
            ("scan_delay", t.scan_delay),
            ("min_wall_dist", t.min_wall_dist),
        ] {
            check(&t.name, field, v)?;
        }
    }
    for t in &data.projectile_types {
        for (field, v) in [
            ("initial_speed", t.initial_speed),
            ("size", t.size),
            ("hole_size", t.hole_size),
        ] {
            check(&t.name, field, v)?;
        }
    }
    Ok(())
}
