//! `GameData` struct tree — the field inventory of the original `tGameData`,
//! expressed as named `serde` tables in `GameData.toml` instead of the
//! original's positional XML.
//!
//! Every value here is in **final form**: colors are `Rgba` f32 (never a
//! packed `COLORREF`/`D3DCOLOR`), angles are radians (never degrees), and
//! `Ref` fields are `Option<IdType>` (never a `(T)-1` sentinel) — the
//! `deserialize_with` helpers in `values.rs` perform these conversions
//! exactly once, at parse time, folding in what the original did as a
//! separate `TransformGameData` pass.

pub mod assets;
pub mod builtin;
pub mod gamedata;
pub mod values;

use crate::ids::{
    AiRobotTypeId, EffectDefId, EmitterTypeId, ParticleTypeId, ProjectileTypeId, RobotTypeId,
};
use crate::math::Rgba;
use serde::Deserialize;

/// `[colors]` — `tColorsOptions`, 8 colors total.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColorsOptions {
    /// Alpha kept.
    #[serde(deserialize_with = "values::deserialize_alpha_color")]
    pub background_2d: Rgba,
    #[serde(deserialize_with = "values::deserialize_color")]
    pub wall_2d: Rgba,
    #[serde(deserialize_with = "values::deserialize_color")]
    pub barrel_2d: Rgba,
    #[serde(deserialize_with = "values::deserialize_color")]
    pub observer_2d: Rgba,
    #[serde(deserialize_with = "values::deserialize_color")]
    pub crosshair_2d: Rgba,
    #[serde(deserialize_with = "values::deserialize_color")]
    pub status_text_2d: Rgba,
    #[serde(deserialize_with = "values::deserialize_color")]
    pub barrel_3d: Rgba,
    #[serde(deserialize_with = "values::deserialize_color")]
    pub tread_3d: Rgba,
}

/// `[building]` — `tBuildingOptions` (shipped defaults: wall width 1.0,
/// room size 10.0, ceiling height 5.0).
///
/// The wall/floor/ceiling texture *filenames* are not stored here — they are
/// compiled in ([`assets::names`], user decision). Only the per-surface UV
/// texture *sizes* remain data.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildingOptions {
    pub wall_width: f32,
    pub room_size: f32,
    pub ceiling_height: f32,
    pub wall_texture_size: f32,
    pub floor_texture_size: f32,
    pub ceiling_texture_size: f32,
}

/// `[sounds]` — `tSoundsOptions`. The two alarm filenames are compiled in
/// ([`assets::names`]); only the shared voice-pool size remains data.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SoundsOptions {
    pub simultaneous_instances: u32,
}

/// `[ui]` — `tUIOptions`: the 10 movement/rate floats. The 36-entry key
/// binding table is compiled in (`input::commands::COMMAND_BINDINGS`), not
/// data (user decision).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiOptions {
    pub rotate_rate: f32,
    pub elevate_rate: f32,
    pub accelerate_rate: f32,
    pub game_speed_rate: f32,
    pub resize_building_rate: f32,
    pub move_building_rate: f32,
    pub move_observer_default_rate: f32,
    pub move_observer_speed1_rate: f32,
    pub move_observer_speed2_rate: f32,
    pub move_observer_speed3_rate: f32,
}

/// `tRobotGroup`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RobotGroup {
    #[serde(deserialize_with = "values::deserialize_ref")]
    pub robot_type: Option<RobotTypeId>,
    #[serde(deserialize_with = "values::deserialize_ref")]
    pub ai_robot_type: Option<AiRobotTypeId>,
    pub count: u32,
    pub initial_delay: f32,
    pub initial_delay_variance: f32,
    pub spawn_delay: f32,
    pub spawn_delay_variance: f32,
    pub spawn_from_point: bool,
}

/// `tCenterGroup { iCenterType, nCenters }`. `center_type`
/// indexes whichever of `ammo`/`energy`/`health` the surrounding
/// `GameDefinition` list refers to (the original's `iCenterType` is a
/// plain `UINT`, not one of the port's `ids::*` newtypes).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CenterGroup {
    #[serde(deserialize_with = "values::deserialize_ref")]
    pub center_type: Option<u32>,
    pub count: u32,
}

/// `tTeamDefinition`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeamDefinition {
    pub name: String,
    #[serde(deserialize_with = "values::deserialize_color")]
    pub color: Rgba,
    #[serde(default)]
    pub robot_groups: Vec<RobotGroup>,
}

/// `tGameDefinition`. One `[[game_definitions]]` table per
/// selectable "game" in the New Game screen's combo box.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameDefinition {
    pub name: String,
    #[serde(default)]
    pub teams: Vec<TeamDefinition>,
    #[serde(default)]
    pub ammo_center_groups: Vec<CenterGroup>,
    #[serde(default)]
    pub energy_center_groups: Vec<CenterGroup>,
    #[serde(default)]
    pub health_center_groups: Vec<CenterGroup>,
}

/// `tProjectileData { iProjectileType, nProjectiles, fFireRate,
/// angAccuracy }` — one magazine entry in a robot type's
/// fire/restock priority list (last-matching-entry selection).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectileData {
    #[serde(deserialize_with = "values::deserialize_ref")]
    pub projectile_type: Option<ProjectileTypeId>,
    pub count: u32,
    pub fire_rate: f32,
    #[serde(deserialize_with = "values::deserialize_degrees_as_radians")]
    pub accuracy: f32,
}

/// `[[robot_types]]` — `tRobotType`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RobotType {
    pub name: String,
    pub accel: f32,
    pub brake_decel: f32,
    pub friction_decel: f32,
    pub max_speed: f32,
    /// PARITY: unused for turning (rate-limit code commented out,
    /// `cRobot.cpp:117-120`) but still loaded/stored.
    #[serde(deserialize_with = "values::deserialize_degrees_as_radians")]
    pub rotate_rate: f32,
    pub max_visible_dist: f32,
    pub energy_start: f32,
    pub health_start: f32,
    pub projectile_armor: f32,
    pub crash_armor: f32,
    #[serde(deserialize_with = "values::deserialize_ref")]
    pub destroy_effect: Option<EffectDefId>,
    #[serde(default)]
    pub projectile_data: Vec<ProjectileData>,
}

/// `[[ai_robot_types]]` — `tAIRobotType`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AiRobotType {
    pub name: String,
    pub desired_speed: f32,
    pub scan_delay: f32,
    pub min_wall_dist: f32,
    /// PARITY: only consumed by dead code (`cObstaclePathFinder`, not
    /// ported) — loaded/stored anyway for schema fidelity.
    pub obstacle_impulse: f32,
    pub desired_ammo: f32,
    pub desired_energy: f32,
    pub desired_health: f32,
    pub critical_ammo: f32,
    pub critical_energy: f32,
    pub critical_health: f32,
}

/// `[[projectile_types]]` — `tProjectileType`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectileType {
    pub name: String,
    pub initial_speed: f32,
    pub damage: f32,
    pub damage_variance: f32,
    pub gravity_accel: f32,
    pub size: f32,
    pub hole_size: f32,
    #[serde(deserialize_with = "values::deserialize_ref")]
    pub fire_effect: Option<EffectDefId>,
    #[serde(deserialize_with = "values::deserialize_ref")]
    pub travel_effect: Option<EffectDefId>,
    #[serde(deserialize_with = "values::deserialize_ref")]
    pub collision_effect: Option<EffectDefId>,
}

/// `tTransferRate { iProjectileType, fTransferRate }`.
/// PARITY: an ammo center with no entry for a fired projectile type has
/// rate `0.0` ⇒ `dt / 0.0 = +inf` ammo — defined IEEE division, preserved
/// as-is with no guard.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransferRate {
    #[serde(deserialize_with = "values::deserialize_ref")]
    pub projectile_type: Option<ProjectileTypeId>,
    pub transfer_rate: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AmmoCenterType {
    pub name: String,
    pub size: f32,
    #[serde(deserialize_with = "values::deserialize_alpha_color")]
    pub color: Rgba,
    #[serde(default)]
    pub transfer_rates: Vec<TransferRate>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnergyCenterType {
    pub name: String,
    pub size: f32,
    #[serde(deserialize_with = "values::deserialize_alpha_color")]
    pub color: Rgba,
    pub transfer_rate: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HealthCenterType {
    pub name: String,
    pub size: f32,
    #[serde(deserialize_with = "values::deserialize_alpha_color")]
    pub color: Rgba,
    pub transfer_rate: f32,
}

/// `[center_types]` — the `CenterTypes { vAmmo/vEnergy/vHealthCenterTypes }`
/// grouping.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CenterTypes {
    #[serde(default)]
    pub ammo: Vec<AmmoCenterType>,
    #[serde(default)]
    pub energy: Vec<EnergyCenterType>,
    #[serde(default)]
    pub health: Vec<HealthCenterType>,
}

/// `tParticleStage { Color, fSize, fTime }` — a keyframe;
/// `fTime` is a 0..1 fraction of the particle's lifetime.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParticleStage {
    #[serde(deserialize_with = "values::deserialize_alpha_color")]
    pub color: Rgba,
    pub size: f32,
    pub time: f32,
}

/// `[[particle_types]]` — `tParticleType`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParticleType {
    pub name: String,
    pub lifetime: f32,
    pub lifetime_variance: f32,
    #[serde(default)]
    pub stages: Vec<ParticleStage>,
    pub inherited_velocity_factor: f32,
    pub rise_speed: f32,
    pub rise_acceleration: f32,
    #[serde(deserialize_with = "values::deserialize_degrees_as_radians")]
    pub initial_rotation: f32,
    #[serde(deserialize_with = "values::deserialize_degrees_as_radians")]
    pub initial_rotation_variance: f32,
    #[serde(deserialize_with = "values::deserialize_degrees_as_radians")]
    pub rotation_rate: f32,
    #[serde(deserialize_with = "values::deserialize_degrees_as_radians")]
    pub rotation_rate_variance: f32,
}

/// `[[particle_emitter_types]]` — `tParticleEmitterType`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParticleEmitterType {
    pub name: String,
    pub lifetime: f32,
    pub lifetime_variance: f32,
    pub emit_by_distance: bool,
    /// PARITY: a *delay* between emits (seconds, or distance units when
    /// `emit_by_distance`), not a frequency.
    pub emit_rate: f32,
    pub emit_rate_variance: f32,
    pub emit_relative: bool,
    #[serde(deserialize_with = "values::deserialize_degrees_as_radians")]
    pub emit_theta_min: f32,
    #[serde(deserialize_with = "values::deserialize_degrees_as_radians")]
    pub emit_theta_max: f32,
    /// May be negative.
    #[serde(deserialize_with = "values::deserialize_degrees_as_radians")]
    pub emit_phi_min: f32,
    #[serde(deserialize_with = "values::deserialize_degrees_as_radians")]
    pub emit_phi_max: f32,
    pub emit_velocity: f32,
    pub emit_velocity_variance: f32,
}

/// `tParticleEffectComponent { iParticleType, iParticleEmitterType }`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParticleEffectComponent {
    #[serde(deserialize_with = "values::deserialize_ref")]
    pub particle_type: Option<ParticleTypeId>,
    #[serde(deserialize_with = "values::deserialize_ref")]
    pub particle_emitter_type: Option<EmitterTypeId>,
}

/// `[[particle_effect_definitions]]` — `tParticleEffectDefinition`.
/// Referenced by index from robot/projectile types via
/// `EffectDefId`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParticleEffectDefinition {
    pub name: String,
    #[serde(default)]
    pub components: Vec<ParticleEffectComponent>,
}

/// The full `GameData.toml` document — same field inventory as the
/// original `tGameData`, expressed as named tables.
///
/// NOT carried over: `tGameData::vParticleEffectComponents`
/// (dead member — components live inside each effect definition, not in a
/// separate top-level list).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameData {
    pub colors: ColorsOptions,
    pub building: BuildingOptions,
    pub sounds: SoundsOptions,
    pub ui: UiOptions,
    #[serde(default)]
    pub game_definitions: Vec<GameDefinition>,
    #[serde(default)]
    pub robot_types: Vec<RobotType>,
    #[serde(default)]
    pub ai_robot_types: Vec<AiRobotType>,
    #[serde(default)]
    pub projectile_types: Vec<ProjectileType>,
    pub center_types: CenterTypes,
    #[serde(default)]
    pub particle_types: Vec<ParticleType>,
    #[serde(default)]
    pub particle_emitter_types: Vec<ParticleEmitterType>,
    #[serde(default)]
    pub particle_effect_definitions: Vec<ParticleEffectDefinition>,
}
