//! M6 simulation-core tests (design §6, §7 M6): kinematics/collision/
//! crash-damage/fire-gate/scatter/last-match/aim-NaN/inf-ammo parity, the
//! win/lose/draw conditions, and the headless determinism harness (the
//! regression anchor: Small2.maz + GameData.toml, two teams, fixed scripted
//! input + fixed seed + fixed dt -> deterministic terminal status + tick).

use operation_purge::data::gamedata;
use operation_purge::data::{ProjectileData, RobotType};
use operation_purge::ids::{ProjectileTypeId, RobotId, RobotTypeId, TeamId, GameDefId};
use operation_purge::math::equals;
use operation_purge::math::intersect::{Intersection2D, Plane2D, PlaneAxis};
use operation_purge::maze::Maze;
use operation_purge::rng::GameRng;
use operation_purge::sim::projectile::solve_aim;
use operation_purge::sim::robot::{
    fire_gate_aligned, Collision2D, CollisionKind, Robot, Throttle,
};
use operation_purge::sim::world::{RobotInput, Status, World};
use glam::{Vec2, Vec3};
use std::f32::consts::PI;
use std::path::Path;

// --------------------------------------------------------------------
// synthetic fixtures
// --------------------------------------------------------------------

fn robot_type(
    accel: f32,
    brake_decel: f32,
    friction_decel: f32,
    max_speed: f32,
    energy_start: f32,
    health_start: f32,
    crash_armor: f32,
    projectile_data: Vec<ProjectileData>,
) -> RobotType {
    RobotType {
        name: "Test".into(),
        accel,
        brake_decel,
        friction_decel,
        max_speed,
        rotate_rate: 0.0,
        max_visible_dist: 50.0,
        energy_start,
        health_start,
        projectile_armor: 1.0,
        crash_armor,
        destroy_sound: String::new(),
        destroy_effect: None,
        projectile_data,
    }
}

fn make_robot(rt: &RobotType) -> Robot {
    Robot::new(RobotTypeId(0), TeamId(0), RobotId(1), Vec2::ZERO, rt)
}

// --------------------------------------------------------------------
// 1. GetMoveAccel / two-phase movement hand-trace
// --------------------------------------------------------------------

#[test]
fn two_phase_movement_matches_hand_trace() {
    // accel=10, no friction, desired=100, dt=0.1, heading +X.
    // GetMoveAccel(Accelerate): robot_accel clamps to accel*dt = 1.0 each
    // frame; distance = speed*dt + 0.5*accel*dt^2.
    let rt = robot_type(10.0, 25.0, 0.0, 100.0, 1000.0, 5.0, 25.0, vec![]);
    let mut r = make_robot(&rt);
    r.accelerate(100.0); // desired_speed -> 100

    // Frame 0: speed 0 -> 1, pos 0.005
    r.calc_new_position(0.1);
    r.commit_move(Collision2D::none());
    assert!(equals(r.speed, 1.0), "speed {}", r.speed);
    assert!((r.position.x - 0.005).abs() < 1e-6, "pos {}", r.position.x);

    // Frame 1: speed 1 -> 2, dist 0.105 -> pos 0.11
    r.calc_new_position(0.1);
    r.commit_move(Collision2D::none());
    assert!(equals(r.speed, 2.0), "speed {}", r.speed);
    assert!((r.position.x - 0.11).abs() < 1e-5, "pos {}", r.position.x);

    // Frame 2: speed 2 -> 3, dist 0.205 -> pos 0.315
    r.calc_new_position(0.1);
    r.commit_move(Collision2D::none());
    assert!(equals(r.speed, 3.0), "speed {}", r.speed);
    assert!((r.position.x - 0.315).abs() < 1e-5, "pos {}", r.position.x);
}

#[test]
fn cruise_friction_decays_speed() {
    // friction_decel=1, cruise, speed 10, dt 0.1: friction = -0.1,
    // distance = 10*0.1 + 0.5*(-0.1)*0.01 = 0.9995, speed -> 9.9.
    let rt = robot_type(10.0, 25.0, 1.0, 100.0, 1000.0, 5.0, 25.0, vec![]);
    let mut r = make_robot(&rt);
    r.set_move_state(Throttle::Cruise);
    r.speed = 10.0;
    r.calc_new_position(0.1);
    r.commit_move(Collision2D::none());
    assert!(equals(r.speed, 9.9), "speed {}", r.speed);
    assert!((r.position.x - 0.9995).abs() < 1e-5, "pos {}", r.position.x);
}

// --------------------------------------------------------------------
// 2. collision truncation 0.005
// --------------------------------------------------------------------

#[test]
fn add_collision_truncates_by_offset_over_move_component() {
    let rt = robot_type(10.0, 25.0, 0.0, 100.0, 1000.0, 5.0, 25.0, vec![]);
    let mut r = make_robot(&rt);
    r.move_vec = Vec2::new(1.0, 0.0);
    r.collision_time = 1.0;
    r.collided = false;

    let ntr = Intersection2D {
        hit: true,
        time: 0.5,
        plane: Plane2D::default(),
    };
    let c = Collision2D {
        ntr,
        kind: CollisionKind::Wall,
        robot_id: None,
        wall_axis: PlaneAxis::X,
        vec2_direction: Vec2::ZERO,
    };
    r.add_collision(c);
    // 0.005 back-off along the dominant (x) axis: 0.5 - 0.005/1.0.
    assert!(equals(r.collision_time, 0.495), "{}", r.collision_time);
    assert!(r.collided);
}

// --------------------------------------------------------------------
// 3. crash-damage impulse
// --------------------------------------------------------------------

#[test]
fn crash_damage_impulse_against_wall() {
    // Wall crash (other velocity 0): force = |v.v / speed| = speed.
    // health 5 - 10/25 = 4.6; speed zeroed.
    let rt = robot_type(10.0, 25.0, 0.0, 100.0, 1000.0, 5.0, 25.0, vec![]);
    let mut r = make_robot(&rt);
    r.speed = 10.0;
    r.direction = 0.0;
    r.collided = true;
    r.collision_time = 0.5;
    r.move_vec = Vec2::new(1.0, 0.0);

    r.commit_move(Collision2D::none());
    assert!((r.health - 4.6).abs() < 1e-5, "health {}", r.health);
    assert!(equals(r.speed, 0.0), "speed {}", r.speed);
}

// --------------------------------------------------------------------
// 4. fire-gate no-wrap boundary case
// --------------------------------------------------------------------

#[test]
fn fire_gate_has_no_2pi_wrap() {
    // Geometrically aligned across the 2pi boundary but NOT gate-aligned:
    // |2pi - 0| = 2pi >> pi*0.025 -> false (PARITY: no wrap).
    assert!(!fire_gate_aligned(2.0 * PI, 0.0));
    // Genuinely close headings pass.
    assert!(fire_gate_aligned(0.01, 0.0));
    // Just outside the gate fails.
    assert!(!fire_gate_aligned(PI * 0.03, 0.0));
}

// --------------------------------------------------------------------
// 5. scatter clamp (phi stays in [0, pi/4])
// --------------------------------------------------------------------

#[test]
fn fire_scatter_phi_stays_clamped() {
    let pd = vec![ProjectileData {
        projectile_type: Some(ProjectileTypeId(0)),
        count: 1000,
        fire_rate: 0.0, // always ready
        accuracy: 10.0, // huge scatter (radians) to force the clamp
    }];
    let rt = robot_type(10.0, 25.0, 0.0, 100.0, 1000.0, 5.0, 25.0, pd);
    let mut rng = GameRng::from_seed(12345);

    // Repeatedly fire from the clamp boundary; barrel elevation must never
    // leave [0, pi/4] regardless of the (large) random scatter.
    for _ in 0..200 {
        let mut r = make_robot(&rt);
        r.barrel_elevation = PI * 0.25;
        let req = r.fire(ProjectileTypeId(0), &mut rng).expect("should fire");
        assert!(
            (0.0..=PI * 0.25 + 1e-6).contains(&req.phi),
            "phi out of range: {}",
            req.phi
        );
        assert!((0.0..=PI * 0.25 + 1e-6).contains(&r.barrel_elevation));
    }
}

// --------------------------------------------------------------------
// 6. last-matching-entry selection
// --------------------------------------------------------------------

#[test]
fn fire_and_restock_type_pick_last_matching_entry() {
    let pd = vec![
        ProjectileData {
            projectile_type: Some(ProjectileTypeId(0)),
            count: 10,
            fire_rate: 0.0,
            accuracy: 0.0,
        },
        ProjectileData {
            projectile_type: Some(ProjectileTypeId(1)),
            count: 10,
            fire_rate: 0.0,
            accuracy: 0.0,
        },
    ];
    let rt = robot_type(10.0, 25.0, 0.0, 100.0, 1000.0, 5.0, 25.0, pd);
    let r = make_robot(&rt);

    // Both ready -> the LAST ready entry wins.
    assert_eq!(r.projectile_fire_type(), Some(ProjectileTypeId(1)));

    // Both not full (spend one of each) -> the LAST not-full entry wins.
    let mut r2 = make_robot(&rt);
    r2.increase_ammo(ProjectileTypeId(0), -1.0); // 10 -> 9 via f_min(9,10)
    r2.increase_ammo(ProjectileTypeId(1), -1.0);
    assert_eq!(r2.projectile_restock_type(), Some(ProjectileTypeId(1)));
}

// --------------------------------------------------------------------
// 7. aim solver near-pure-Y -> NaN -> miss, no panic
// --------------------------------------------------------------------

#[test]
fn aim_solver_pure_y_target_misses_without_panic() {
    // Target directly along +Y (dx == 0): cosTheta == 0 -> X-only
    // time-of-flight is 0/0 = NaN -> Newton never converges -> None.
    let result = solve_aim(
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(0.0, 10.0, 0.0),
        100.0,
        10.0,
    );
    assert_eq!(result, None);

    // -Y as well.
    let result = solve_aim(
        Vec3::new(5.0, 5.0, 0.0),
        Vec3::new(5.0, -20.0, 1.0),
        100.0,
        10.0,
    );
    assert_eq!(result, None);
}

#[test]
fn aim_solver_reachable_target_converges() {
    // A target straight ahead along +X at the same height converges to a
    // low first-quadrant elevation.
    let result = solve_aim(
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(20.0, 0.0, 0.0),
        100.0,
        10.0,
    );
    let (theta, phi) = result.expect("reachable target should converge");
    assert!(equals(theta, 0.0), "theta {theta}");
    assert!((0.0..=PI * 0.5).contains(&phi), "phi {phi}");
}

// --------------------------------------------------------------------
// 8. unlisted-type ammo -> +inf (design §1.3, no guard)
// --------------------------------------------------------------------

#[test]
fn unlisted_ammo_rate_yields_infinite_fill() {
    // The transfer site computes `(dt / rate).floor()`; an unlisted type
    // has rate 0.0, so dt/0.0 = +inf (defined IEEE division, no guard).
    let dt = 0.1_f32;
    let rate = 0.0_f32;
    let count = (dt / rate).floor();
    assert!(count.is_infinite() && count.is_sign_positive());

    // increase_ammo(+inf) fills to the starting count via f_min(left+inf,
    // start) == start — no panic, no guard.
    let pd = vec![ProjectileData {
        projectile_type: Some(ProjectileTypeId(0)),
        count: 500,
        fire_rate: 0.2,
        accuracy: 0.0,
    }];
    let rt = robot_type(10.0, 25.0, 0.0, 100.0, 1000.0, 5.0, 25.0, pd);
    let mut r = make_robot(&rt);
    // Spend some ammo, then flood it.
    r.increase_ammo(ProjectileTypeId(0), -100.0); // 500 -> 400
    r.increase_ammo(ProjectileTypeId(0), count); // +inf -> clamps to 500
    assert_eq!(r.proj_states[0].projectiles_left, 500.0);
}

// --------------------------------------------------------------------
// 9. win / lose / draw
// --------------------------------------------------------------------

fn assets_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("assets")
}

fn load_world(seed: u64) -> World {
    let data = gamedata::load(&assets_dir().join("GameData.toml")).expect("GameData.toml");
    let maze = Maze::load(&assets_dir().join("MazeFiles/Small2.maz")).expect("Small2.maz");
    // game definition 0 = "2 Teams Caleb"; active team 0.
    World::new(data, maze, GameDefId(0), TeamId(0), seed)
}

#[test]
fn status_ongoing_at_start() {
    let world = load_world(1);
    assert_eq!(world.status(), Status::None);
}

#[test]
fn status_won_when_all_enemies_dead() {
    let mut world = load_world(1);
    let active = world.active_team;
    for r in world.robots.values_mut() {
        if r.team != active {
            r.health = 0.0;
        }
    }
    world.frame(0.0); // removes the destroyed enemies
    assert_eq!(world.status(), Status::Won);
}

#[test]
fn status_lost_when_active_team_wiped() {
    let mut world = load_world(1);
    let active = world.active_team;
    for r in world.robots.values_mut() {
        if r.team == active {
            r.health = 0.0;
        }
    }
    world.frame(0.0);
    assert_eq!(world.status(), Status::Lost);
}

#[test]
fn status_draw_when_everyone_dead() {
    let mut world = load_world(1);
    for r in world.robots.values_mut() {
        r.health = 0.0;
    }
    world.frame(0.0);
    assert_eq!(world.status(), Status::Draw);
}

// --------------------------------------------------------------------
// 10. headless determinism harness (regression anchor)
// --------------------------------------------------------------------

/// Drives every live robot forward at full throttle while slowly rotating
/// and firing continuously, with a fixed dt, until a terminal status is
/// reached. The rotating spray of projectiles resolves the battle
/// deterministically (fixed seed drives the scatter RNG).
fn run_scripted_battle(seed: u64, cap: u32) -> (u32, Status) {
    let mut world = load_world(seed);
    let dt = 1.0 / 60.0;
    let input = RobotInput {
        throttle: Throttle::Accelerate,
        turn: 0.02,
        elevate: 0.0,
        accel: 1000.0, // saturate desired speed to max_speed
        fire: true,
    };
    let mut tick = 0u32;
    loop {
        let status = world.status();
        if status != Status::None || tick >= cap {
            return (tick, status);
        }
        let ids: Vec<RobotId> = world.robots.keys().copied().collect();
        for id in ids {
            world.set_input(id, input);
        }
        world.frame(dt);
        tick += 1;
    }
}

#[test]
fn scripted_battle_is_deterministic_and_terminates() {
    let cap = 200_000;
    let (tick_a, status_a) = run_scripted_battle(4242, cap);
    let (tick_b, status_b) = run_scripted_battle(4242, cap);

    // Bit-for-bit determinism across identical runs.
    assert_eq!((tick_a, status_a), (tick_b, status_b), "runs diverged");
    // The regression anchor: the battle actually terminates.
    assert_ne!(status_a, Status::None, "battle did not terminate by {cap}");

    // Pinned terminal (discovered from a reference run — update only with a
    // deliberate, understood behavior change).
    assert_eq!(
        (tick_a, status_a),
        (SCRIPTED_TERMINAL_TICK, SCRIPTED_TERMINAL_STATUS),
        "scripted battle terminal changed"
    );
}

// Pinned from a reference run (Small2.maz, game def 0, seed 4242, dt=1/60):
// the enemy team is wiped by the projectile spray while the active team's
// Machine Gunner survives -> Won at tick 4426.
const SCRIPTED_TERMINAL_TICK: u32 = 4426;
const SCRIPTED_TERMINAL_STATUS: Status = Status::Won;
