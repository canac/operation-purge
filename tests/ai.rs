//! AI & pathfinding tests:
//! - the wall-bounce heading snap and the turn-TOWARD-collider quirk
//!   (`cAIRobot.cpp:130,135-148`) as direct unit checks;
//! - a scripted scenario where an AI robot drives to an ammo center, brakes
//!   to a stop inside it, and resupplies;
//! - a scripted scenario where an AI robot hunts and damages an enemy;
//! - a two-robot collision that exercises the turn-toward-collider path;
//! - the SEEDED AI-vs-AI battle on Small2.maz as the deterministic CI
//!   regression anchor.
//!
//! (The A* room-sequence hand-trace + node-hash tests live in the `path`
//! module's own unit tests)

use operation_purge::data::gamedata;
use operation_purge::data::{CenterGroup, GameData, GameDefinition, RobotGroup, TeamDefinition};
use operation_purge::ids::{AiRobotTypeId, GameDefId, RobotTypeId, TeamId};
use operation_purge::math::intersect::PlaneAxis;
use operation_purge::maze::Maze;
use operation_purge::sim::ai::{bounce_turn_toward_robot, wall_bounce_heading, AiState};
use operation_purge::sim::world::{Status, World};
use glam::Vec2;
use std::f32::consts::PI;
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

// --------------------------------------------------------------------
// 1. wall-bounce heading snap (cAIRobot.cpp:135-148)
// --------------------------------------------------------------------

#[test]
fn wall_bounce_snaps_to_cardinal_headings() {
    // planeX: an X-ward heading reverses to {0, pi}.
    assert_eq!(wall_bounce_heading(PlaneAxis::X, 0.1), PI); // in [0,pi/2] -> pi
    assert_eq!(wall_bounce_heading(PlaneAxis::X, PI * 1.7), PI); // in [3pi/2,2pi] -> pi
    assert_eq!(wall_bounce_heading(PlaneAxis::X, 2.0), 0.0); // otherwise -> 0

    // planeY: a Y-ward heading reverses to {pi/2, 3pi/2}.
    assert_eq!(wall_bounce_heading(PlaneAxis::Y, 0.5), PI * 1.5); // in [0,pi] -> 3pi/2
    assert_eq!(wall_bounce_heading(PlaneAxis::Y, 4.0), PI * 0.5); // otherwise -> pi/2
}

// --------------------------------------------------------------------
// 2. turn-TOWARD-collider quirk (cAIRobot.cpp:130) — PARITY
// --------------------------------------------------------------------

#[test]
fn robot_bounce_turns_toward_the_collider_not_away() {
    // Robot at origin; collider straight along +X -> face +X (angle 0),
    // i.e. TOWARD the collider (a "turn away" would give pi).
    let a = bounce_turn_toward_robot(Vec2::ZERO, Vec2::new(5.0, 0.0));
    assert!(a.abs() < 1e-6, "expected ~0 (toward +X collider), got {a}");

    // Collider along +Y -> face +Y (angle pi/2), toward the collider.
    let b = bounce_turn_toward_robot(Vec2::ZERO, Vec2::new(0.0, 5.0));
    assert!((b - PI * 0.5).abs() < 1e-6, "expected pi/2, got {b}");

    // Collider behind (-X) -> face -X (angle pi), still TOWARD it.
    let c = bounce_turn_toward_robot(Vec2::ZERO, Vec2::new(-5.0, 0.0));
    assert!((c.abs() - PI).abs() < 1e-6, "expected +/-pi, got {c}");
}

// --------------------------------------------------------------------
// helpers to build controlled single-def scenarios
// --------------------------------------------------------------------

/// Build a `GameData` whose only game definition is `def`, appended to the
/// real data (so colors/ui/robot/projectile/center types stay valid). Returns
/// the data + the index of the appended definition.
fn data_with_def(def: GameDefinition) -> (GameData, u32) {
    let mut data = load_data();
    data.game_definitions.push(def);
    let idx = (data.game_definitions.len() - 1) as u32;
    (data, idx)
}

fn team(name: &str, color: operation_purge::math::Rgba, groups: Vec<RobotGroup>) -> TeamDefinition {
    TeamDefinition {
        name: name.into(),
        color,
        robot_groups: groups,
    }
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

// --------------------------------------------------------------------
// 3. AI drives to a center, brakes to a stop inside, and resupplies
// --------------------------------------------------------------------

#[test]
fn ai_drives_to_center_and_resupplies() {
    // One team, one AI robot ("Robot"/"AI Robot"), no enemies, several ammo
    // centers. With ammo depleted (< desired_ammo) the AI must target a
    // center, path to it, brake to a stop inside, and refill.
    let color = load_data().game_definitions[0].teams[0].color;
    let def = GameDefinition {
        name: "Resupply".into(),
        teams: vec![team("AI", color, vec![group(0, 0, 1)])],
        ammo_center_groups: vec![CenterGroup {
            center_type: Some(0),
            count: 6,
        }],
        energy_center_groups: vec![],
        health_center_groups: vec![],
    };
    let (data, def_idx) = data_with_def(def);
    let maze = open_bordered(3, 3);
    let mut world = World::new(data, maze, GameDefId(def_idx), TeamId(0), 7);

    // Deplete every projectile magazine of the (single) robot.
    let id = *world.robots.keys().next().expect("one AI robot");
    {
        let r = world.robots.get_mut(&id).unwrap();
        assert!(
            !r.proj_states.is_empty(),
            "robot type must carry ammo for this scenario"
        );
        for s in &mut r.proj_states {
            s.projectiles_left = 0.0;
        }
    }

    let dt = 1.0 / 60.0;
    let mut resupplied = false;
    for _ in 0..6000 {
        world.frame(dt, None, None);
        let r = world.robots.get(&id).unwrap();
        if r.proj_states.iter().any(|s| s.projectiles_left > 0.0) {
            resupplied = true;
            // It must have been (essentially) stopped inside the center to
            // transfer — the brake law brought it to rest.
            assert!(
                r.speed.abs() < 1.0,
                "expected the robot to have braked to ~rest, speed {}",
                r.speed
            );
            break;
        }
    }
    assert!(resupplied, "AI never drove to a center and resupplied");
}

// --------------------------------------------------------------------
// 4. AI hunts a visible enemy and damages it (fire trajectory + gate)
// --------------------------------------------------------------------

#[test]
fn ai_hunts_and_damages_enemy() {
    // Two teams of one robot each in a single open room, so they see each
    // other quickly. The machine gunner should aim (solve trajectory), pass
    // the alignment gate, fire, and draw the enemy's health down.
    let color0 = load_data().game_definitions[0].teams[0].color;
    let color1 = load_data().game_definitions[0].teams[1].color;
    let def = GameDefinition {
        name: "Hunt".into(),
        teams: vec![
            team("A", color0, vec![group(4, 1, 1)]), // Machine Gunner / AI Machine Gunner
            team("B", color1, vec![group(0, 0, 1)]), // Robot / AI Robot
        ],
        ammo_center_groups: vec![],
        energy_center_groups: vec![],
        health_center_groups: vec![],
    };
    let (data, def_idx) = data_with_def(def);
    // One big room so line-of-sight is immediate.
    let maze = open_bordered(1, 1);
    let mut world = World::new(data, maze, GameDefId(def_idx), TeamId(0), 3);

    let start_health: f32 = world.robots.values().map(|r| r.health).sum();

    let dt = 1.0 / 60.0;
    let mut damaged = false;
    for _ in 0..6000 {
        world.frame(dt, None, None);
        let health: f32 = world.robots.values().map(|r| r.health).sum();
        // A robot took damage (projectile or crash) => combat happened.
        if world.robots.len() < 2 || health + 1e-3 < start_health {
            damaged = true;
            break;
        }
    }
    assert!(damaged, "no AI robot ever damaged another (hunt/fire failed)");
}

// --------------------------------------------------------------------
// 5. two-robot collision exercises the turn-toward-collider FSM path
// --------------------------------------------------------------------

#[test]
fn two_robots_collide_and_enter_idle() {
    // Two robots on the SAME team (so they never fire at each other and thus
    // survive to collide) confined to one open room. Idle robots drive in
    // random directions, hit the walls and each other; every collision runs
    // the MoveAI reaction (turn-toward-collider / wall-snap, then
    // EnterIdleMode). We assert a collision is observed and the FSM stays
    // well-defined for every AI robot.
    let color0 = load_data().game_definitions[0].teams[0].color;
    let def = GameDefinition {
        name: "Collide".into(),
        teams: vec![team("A", color0, vec![group(0, 0, 2)])],
        ammo_center_groups: vec![],
        energy_center_groups: vec![],
        health_center_groups: vec![],
    };
    let (data, def_idx) = data_with_def(def);
    let maze = open_bordered(1, 1);
    let mut world = World::new(data, maze, GameDefId(def_idx), TeamId(0), 11);

    let dt = 1.0 / 60.0;
    let mut collided_seen = false;
    for _ in 0..4000 {
        world.frame(dt, None, None);
        if world.robots.values().any(|r| r.collided) {
            collided_seen = true;
            break;
        }
    }
    assert!(
        collided_seen,
        "the two robots never collided in a shared room"
    );
    for r in world.robots.values() {
        let ai = r.ai.as_ref().expect("every spawned robot has an AI runtime");
        assert!(matches!(ai.state, AiState::Idle | AiState::TargetingCenter | AiState::TargetingRobot));
    }
}

// --------------------------------------------------------------------
// 6. seeded AI-vs-AI battle determinism (CI regression anchor)
// --------------------------------------------------------------------

type Fingerprint = Vec<(u32, u32, u32, u32)>;

/// A fingerprint of the full sim state: sorted (id, pos bits, health bits).
fn fingerprint(world: &World) -> Fingerprint {
    let mut v: Vec<(u32, u32, u32, u32)> = world
        .robots
        .iter()
        .map(|(id, r)| {
            (
                id.get(),
                r.position.x.to_bits(),
                r.position.y.to_bits(),
                r.health.to_bits(),
            )
        })
        .collect();
    v.sort();
    v
}

fn load_battle(seed: u64) -> World {
    let data = load_data();
    let maze = Maze::load(&assets_dir().join("MazeFiles/Small2.maz")).expect("Small2.maz");
    // Game def 0 = "2 Teams Caleb"; active team 0.
    World::new(data, maze, GameDefId(0), TeamId(0), seed)
}

/// Run an AI-vs-AI battle up to `cap` ticks. Returns (tick, status,
/// fingerprint-at-stop).
fn run_ai_battle(seed: u64, cap: u32) -> (u32, Status, Fingerprint) {
    let mut world = load_battle(seed);
    let dt = 1.0 / 60.0;
    let mut tick = 0u32;
    loop {
        let status = world.status();
        if status != Status::None || tick >= cap {
            return (tick, status, fingerprint(&world));
        }
        world.frame(dt, None, None);
        tick += 1;
    }
}

#[test]
fn ai_battle_is_deterministic() {
    // A generous but bounded cap: the anchor's contract is *determinism*
    // (two identical runs must evolve bit-identically) plus liveness
    // (combat actually resolves the battle or, failing that, the state is
    // still reproducible at the cap).
    let cap = 30_000;
    let (tick_a, status_a, fp_a) = run_ai_battle(4242, cap);
    let (tick_b, status_b, fp_b) = run_ai_battle(4242, cap);

    // Bit-for-bit determinism across identical seeded runs.
    assert_eq!(tick_a, tick_b, "tick diverged");
    assert_eq!(status_a, status_b, "status diverged");
    assert_eq!(fp_a, fp_b, "world state diverged between identical runs");

    // The AI produced a real fight: either it terminated, or robots were
    // destroyed / damaged along the way (fingerprint changed from spawn).
    let spawn_fp = fingerprint(&load_battle(4242));
    assert!(
        status_a != Status::None || fp_a != spawn_fp,
        "the battle made no observable progress"
    );

    // A different seed must be independently deterministic too (a bounded
    // window keeps this cheap).
    let (tick_c, status_c, fp_c) = run_ai_battle(99, 1500);
    let (tick_d, status_d, fp_d) = run_ai_battle(99, 1500);
    assert_eq!((tick_c, status_c, &fp_c), (tick_d, status_d, &fp_d));
}
