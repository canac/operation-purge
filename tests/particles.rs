//! M9 particle-effect wiring tests (design §6, §7 M9): the three-tier
//! system's integration into `World::frame` — effects must spawn on the
//! projectile fire/travel/collision and robot-destroy events, feeding the
//! shared particle arena. (The per-class unit tests — degenerate-case
//! quaternion, replace-oldest arena, fEmitRate-as-delay, sub-frame
//! interpolation, stage lerp — live in `src/sim/particles.rs`.)

use operation_purge::data::{GameData, GameDefinition, RobotGroup, TeamDefinition};
use operation_purge::data::gamedata;
use operation_purge::ids::{AiRobotTypeId, EffectDefId, GameDefId, ProjectileTypeId, RobotTypeId, TeamId};
use operation_purge::maze::Maze;
use operation_purge::rng::GameRng;
use operation_purge::sim::particles::ParticleEffect;
use operation_purge::sim::projectile::Projectile;
use operation_purge::sim::world::{RobotInput, World};
use glam::Vec3;
use std::path::{Path, PathBuf};

fn assets_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("assets")
}

fn load_data() -> GameData {
    gamedata::load(&assets_dir().join("GameData.toml")).expect("GameData.toml")
}

/// A fully-connected maze with only the outer border walled.
fn open_bordered(cx: i32, cy: i32) -> Maze {
    let mut h = vec![false; (cx * (cy + 1)) as usize];
    let mut v = vec![false; ((cx + 1) * cy) as usize];
    for x in 0..cx {
        h[x as usize] = true;
        h[(cy * cx + x) as usize] = true;
    }
    for y in 0..cy {
        v[(y * (cx + 1)) as usize] = true;
        v[(y * (cx + 1) + cx) as usize] = true;
    }
    Maze::new(cx, cy, h, v)
}

fn data_with_def(def: GameDefinition) -> (GameData, u32) {
    let mut data = load_data();
    data.game_definitions.push(def);
    let idx = (data.game_definitions.len() - 1) as u32;
    (data, idx)
}

fn group(robot_type: u32, ai_type: u32, count: u32) -> RobotGroup {
    RobotGroup {
        robot_type: Some(RobotTypeId(robot_type)),
        ai_robot_type: Some(AiRobotTypeId(ai_type)),
        count,
        initial_delay: 0.0,
        initial_delay_variance: 0.0,
        spawn_delay: 0.0,
        spawn_delay_variance: 0.0,
        spawn_from_point: false,
    }
}

/// One team of `count` robots of `robot_type`, no centers.
fn single_team_world(robot_type: u32, count: u32, cx: i32, cy: i32, seed: u64) -> World {
    let color = load_data().game_definitions[0].teams[0].color;
    let def = GameDefinition {
        name: "M9".into(),
        teams: vec![TeamDefinition {
            name: "A".into(),
            color,
            robot_groups: vec![group(robot_type, 0, count)],
        }],
        ammo_center_groups: vec![],
        energy_center_groups: vec![],
        health_center_groups: vec![],
    };
    let (data, def_idx) = data_with_def(def);
    let maze = open_bordered(cx, cy);
    World::new(data, maze, GameDefId(def_idx), TeamId(0), seed)
}

#[test]
fn firing_spawns_effects_and_particles() {
    // Robot type 0 ("Robot") fires the bullet (fire_effect + collision) or
    // the rocket (travel_effect + collision). Possessing it to fire must,
    // within a few seconds, (a) create at least one projectile, (b) produce
    // live particles (the fire and/or travel emitters ran through the arena),
    // and (c) leave a wall hole (a wall collision resolved) with a burst
    // effect present.
    let mut world = single_team_world(0, 1, 1, 1, 3);
    let id = *world.robots.keys().next().expect("one robot");

    let dt = 1.0 / 60.0;
    let mut saw_projectile = false;
    let mut saw_particles = false;
    let mut saw_hole = false;

    for _ in 0..1200 {
        world.set_input(
            id,
            RobotInput {
                fire: true,
                ..Default::default()
            },
        );
        let holes_before = world.holes.entries.len();
        world.frame(dt);

        if !world.projectiles.is_empty() {
            saw_projectile = true;
        }
        if world.particles.live_count() > 0 {
            saw_particles = true;
        }
        if world.holes.entries.len() > holes_before {
            // A projectile hit a wall this frame: the collision effect was
            // spawned in the same step (both go through step_projectiles).
            saw_hole = true;
        }
        if saw_projectile && saw_particles && saw_hole {
            break;
        }
    }

    assert!(saw_projectile, "possession never fired a projectile");
    assert!(
        saw_particles,
        "fire/travel effect never produced any particles"
    );
    assert!(saw_hole, "no projectile ever collided with a wall");
}

#[test]
fn projectile_travel_effect_is_dynamic_and_drives_a_trail() {
    // The rocket (projectile type 1) has a DYNAMIC, distance-based travel
    // trail (effect def 2). Inject one mid-room, aimed along +X, and step
    // the world: the trail must (a) stay dynamic and never self-prune (its
    // "Rocket travel flame" emitter has lifetime 0 yet survives), and
    // (b) emit trail particles as the projectile travels (gap-free trail).
    let mut world = single_team_world(3, 0, 3, 3, 11); // Machine Gunner: harmless bystander
    // Remove the bystander robot so only our injected rocket is in play.
    world.robots.clear();

    let room_size = world.game_data.building.room_size;
    let start = Vec3::new(room_size * 0.5, room_size * 1.5, 0.5);
    let initial_speed =
        world.game_data.projectile_types[1].initial_speed;
    let mut rng = GameRng::from_seed(11);
    let mut proj = Projectile::new(
        ProjectileTypeId(1),
        None,
        start,
        0.0,
        0.0, // theta: along +X
        0.0, // phi: level
        initial_speed,
    );
    proj.travel_effect = Some(ParticleEffect::new(
        &world.game_data,
        EffectDefId(2),
        true, // dynamic
        proj.position,
        proj.velocity,
        &mut rng,
    ));
    world.projectiles.push(proj);

    let dt = 1.0 / 60.0;
    let mut trail_particles = false;
    let mut checked_dynamic = false;
    for _ in 0..120 {
        world.frame(dt);
        for p in &world.projectiles {
            if let Some(eff) = &p.travel_effect {
                assert!(eff.is_dynamic, "trail must be a dynamic effect");
                assert!(
                    !eff.emitters.is_empty(),
                    "dynamic trail must never self-prune (lifetime-0 emitter survives)"
                );
                checked_dynamic = true;
            }
        }
        if world.particles.live_count() > 0 {
            trail_particles = true;
        }
        if world.projectiles.is_empty() {
            break; // the rocket hit a wall
        }
    }
    assert!(checked_dynamic, "the injected rocket trail was never observed alive");
    assert!(trail_particles, "the travel trail never emitted particles");
}

#[test]
fn destroying_a_robot_spawns_its_destroy_effect() {
    // Robot type 0 carries destroy_effect = 4 ("Robot destroy", 2 emitters).
    // A robot that dies this frame must have its destroy effect pushed to
    // the world (a static burst), with no other effects in play.
    let mut world = single_team_world(0, 1, 1, 1, 5);
    assert!(world.effects.is_empty(), "no effects before any event");
    assert_eq!(world.particles.live_count(), 0);

    let id = *world.robots.keys().next().expect("one robot");
    // Kill it outright (no firing, so the only effect can be the destroy).
    world.robots.get_mut(&id).unwrap().health = 0.0;

    let dt = 1.0 / 60.0;
    world.frame(dt); // no fire input -> the sole event is the destruction

    assert!(world.robots.is_empty(), "the dead robot must be removed");
    assert_eq!(
        world.effects.len(),
        1,
        "exactly the destroy effect must have spawned"
    );
    let destroy = &world.effects[0];
    assert!(!destroy.is_dynamic, "a destroy burst is a static effect");
    assert_eq!(
        destroy.emitters.len(),
        2,
        "the 'Robot destroy' definition has two emitter components"
    );
}
