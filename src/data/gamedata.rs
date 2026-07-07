//! `GameData.toml` loader (design §4.1).
//!
//! GUARD: the original's `cGameData::ReadFileXML` ignored
//! `TiXmlDocument::LoadFile`'s boolean result and then unconditionally
//! dereferenced `FirstChild("GameData")->ToElement()` — a missing or
//! malformed file crashed outright (`cGameData.cpp:405-413`, libs-map
//! §2.1). Here every failure mode is a caught, typed `DataError`.

use super::GameData;
use crate::error::DataError;
use std::path::Path;

/// Parses `path` as `GameData.toml`, then validates it (GUARD: the
/// original ignored `TiXmlDocument::LoadFile`'s result and unconditionally
/// dereferenced the root element — libs-map §2.1).
pub fn load(path: &Path) -> Result<GameData, DataError> {
    let text = std::fs::read_to_string(path).map_err(|source| DataError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let data: GameData = toml::from_str(&text).map_err(|e| DataError::Toml {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;
    validate(&data, path)?;
    Ok(data)
}

/// Full post-load validation: every `*Ref` bounds check (GUARD, design §4.1).
/// (Key bindings are compiled in, not data, so there is no command-table
/// shape check anymore.)
pub fn validate(data: &GameData, path: &Path) -> Result<(), DataError> {
    validate_refs(data, path)
}

/// Validates every `Ref` field against the length of the vector it
/// indexes into (GUARD, design §4.1): the original never validated `*Ref`
/// indices at all and would index out of bounds at spawn time
/// (libs-map §2.2).
pub fn validate_refs(data: &GameData, path: &Path) -> Result<(), DataError> {
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
