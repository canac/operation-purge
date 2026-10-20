//! Game integration tests: state-machine transitions (2D ↔ 3D ↔ possess ↔
//! death-forces-2D), live HUD assembly, the game-speed ramp/pause math, and
//! `execute_commands` mapping bound keys → actions. No window is opened.

use std::path::Path;

use operation_purge::data::gamedata;
use operation_purge::game::{Game, ObserverType, INITIAL_GAME_SPEED};
use operation_purge::game::screens::{
    ConfirmAction, ConfirmChoice, ConfirmKind, GameOverKind, Screen,
};
use operation_purge::ids::{GameDefId, RobotId, TeamId};
use operation_purge::input::commands::{binding, Command, CommandBinding};
use operation_purge::input::keys::Key;
use operation_purge::input::Input;
use operation_purge::maze::Maze;
use operation_purge::sim::robot::Throttle;
use operation_purge::sim::world::World;

fn assets() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("assets")
}

fn game() -> Game {
    let root = assets();
    let data = gamedata::load(&root.join("GameData.toml")).expect("GameData.toml parses");
    let maze = Maze::load(&root.join("MazeFiles/Small1.maz")).expect("Small1.maz loads");
    let world = World::new(data, maze, GameDefId(0), TeamId(0), 987654);
    Game::new(world)
}

fn friendly_robot(game: &Game) -> RobotId {
    *game
        .world
        .robots
        .iter()
        .find(|(_, r)| r.team == TeamId(0))
        .expect("a friendly robot spawned")
        .0
}

/// Press a bound command (its key + any required modifiers) into `input`,
/// then snapshot the frame.
fn hold_command(input: &mut Input, binding: &CommandBinding) {
    input.on_key_event(binding.key, true);
    if binding.control {
        input.on_key_event(Key::ControlLeft, true);
    }
    if binding.alt {
        input.on_key_event(Key::AltLeft, true);
    }
    if binding.shift {
        input.on_key_event(Key::ShiftLeft, true);
    }
}

// ------------------------------ transitions ------------------------------

#[test]
fn full_possession_loop_2d_to_possess_to_3d() {
    let mut g = game();
    assert!(g.in_2d());
    assert_eq!(g.game_speed, INITIAL_GAME_SPEED);

    let id = friendly_robot(&g);
    g.select_robot(id);
    assert_eq!(g.observer_type, ObserverType::Robot);
    assert!(g.set_observer());
    assert!(matches!(g.screen, Screen::Play3D));
    assert_eq!(g.active_robot, Some(id));
}

#[test]
fn play3d_back_to_2d_and_death_forces_2d() {
    let mut g = game();
    let id = friendly_robot(&g);
    g.select_robot(id);
    g.set_observer();

    // A (Enter2DMode) back to the map — cancellable.
    g.enter_2d_mode(false);
    assert!(g.in_2d() && !g.forced_by_death());
    assert!(g.cancel_2d());
    assert!(matches!(g.screen, Screen::Play3D));

    // Death forces the 2D map and it may not be cancelled.
    g.world.robots.get_mut(&id).unwrap().health = 0.0;
    g.game_speed = 1.0;
    g.ignore_game_end = true;
    g.advance(0.016);
    assert!(g.in_2d() && g.forced_by_death());
    assert_eq!(g.active_robot, None);
    assert!(!g.cancel_2d());
}

// ------------------------------ game speed -------------------------------

#[test]
fn game_speed_ramps_by_rate_times_dt_and_pause_zeroes_it() {
    let mut g = game();
    g.screen = Screen::Play3D;
    let rate = g.world.game_data.ui.game_speed_rate;
    let dt = 0.1;

    // Increase (T).
    let mut input = Input::new();
    input.begin_frame(); // clear first-frame edge suppression
    hold_command(&mut input, binding(Command::IncreaseGameSpeed));
    input.begin_frame();
    g.execute_commands(&input, dt);
    assert!((g.game_speed - rate * dt).abs() < 1e-4, "speed = {}", g.game_speed);

    // Pause (Q) → 0.
    let mut input = Input::new();
    input.begin_frame();
    hold_command(&mut input, binding(Command::Pause));
    input.begin_frame();
    g.execute_commands(&input, dt);
    assert_eq!(g.game_speed, 0.0);
}

#[test]
fn game_speed_never_goes_below_zero() {
    let mut g = game();
    g.screen = Screen::Play3D;
    g.game_speed = 0.01;
    let dt = 1.0; // large dt so Decrease would overshoot below zero

    let mut input = Input::new();
    input.begin_frame();
    hold_command(&mut input, binding(Command::DecreaseGameSpeed));
    input.begin_frame();
    g.execute_commands(&input, dt);
    assert_eq!(g.game_speed, 0.0, "clamped at 0 (no negative game speed)");
}

// ---------------------- execute_commands → actions -----------------------

#[test]
fn accelerate_command_sets_the_possessed_robots_throttle() {
    let mut g = game();
    let id = friendly_robot(&g);
    g.select_robot(id);
    g.set_observer();
    g.game_speed = 1.0;

    let mut input = Input::new();
    input.begin_frame();
    hold_command(&mut input, binding(Command::Accelerate));
    input.begin_frame();

    g.execute_commands(&input, 0.016);
    g.advance(0.016);

    // apply_inputs set the throttle from the player's RobotInput.
    assert_eq!(g.world.robots[&id].move_state, Throttle::Accelerate);
}

#[test]
fn no_move_keys_makes_the_possessed_robot_cruise() {
    let mut g = game();
    let id = friendly_robot(&g);
    g.select_robot(id);
    g.set_observer();
    g.game_speed = 1.0;

    let mut input = Input::new();
    input.begin_frame();
    input.begin_frame(); // nothing held

    g.execute_commands(&input, 0.016);
    g.advance(0.016);
    // PARITY: no move keys down ⇒ cruise (`cOperationPurge.cpp:1190`).
    assert_eq!(g.world.robots[&id].move_state, Throttle::Cruise);
}

#[test]
fn fire_command_fires_the_possessed_robots_weapon() {
    let mut g = game();
    let id = friendly_robot(&g);
    g.select_robot(id);
    g.set_observer();
    g.game_speed = 1.0;

    // A freshly-spawned robot has a ready-to-fire type (fire_delay 0, ammo>0).
    let ty = g.world.robots[&id]
        .projectile_fire_type()
        .expect("spawned robot should have a ready projectile type");
    let ammo_before = g.world.robots[&id]
        .proj_states
        .iter()
        .find(|s| s.projectile_type == ty)
        .unwrap()
        .projectiles_left;

    let mut input = Input::new();
    input.begin_frame();
    hold_command(&mut input, binding(Command::FireProjectiles));
    input.begin_frame();

    g.execute_commands(&input, 0.016);
    g.advance(0.016);

    // Firing decrements the fired type's ammo (the projectile itself may
    // already have collided + been removed within the same frame).
    let ammo_after = g.world.robots[&id]
        .proj_states
        .iter()
        .find(|s| s.projectile_type == ty)
        .unwrap()
        .projectiles_left;
    assert!(
        ammo_after < ammo_before,
        "fire command should consume ammo ({ammo_before} -> {ammo_after})"
    );
}

#[test]
fn enter_2d_command_leaves_play3d() {
    let mut g = game();
    let id = friendly_robot(&g);
    g.select_robot(id);
    g.set_observer();
    assert!(matches!(g.screen, Screen::Play3D));

    let mut input = Input::new();
    input.begin_frame();
    hold_command(&mut input, binding(Command::Enter2DMode));
    input.begin_frame();

    g.execute_commands(&input, 0.016);
    assert!(g.in_2d());
    // Entering 2D resets the game speed to the initial paused value.
    assert_eq!(g.game_speed, INITIAL_GAME_SPEED);
}

// -------------------------------- live HUD -------------------------------

#[test]
fn hud_snapshot_reflects_the_possessed_robot_and_world() {
    let mut g = game();
    let id = friendly_robot(&g);
    g.select_robot(id);
    g.set_observer();

    let text = g.status_text();
    // The active-robot block is present with its type/team.
    assert!(text.contains("Speed:"), "missing robot block:\n{text}");
    assert!(text.contains("Type:"));
    assert!(text.contains("Team:"));
    // Global block always present.
    assert!(text.contains("Spawned robots:"));
    assert!(text.contains("Game speed: 0.00%"));
    assert!(text.contains("Holes: 0"));
}

#[test]
fn hud_snapshot_has_no_robot_block_before_possession() {
    let g = game();
    // Nothing possessed yet, but observer type defaults to Robot with no
    // active robot ⇒ the robot block is empty.
    let snap = g.hud_snapshot();
    assert!(snap.robot.is_none());
    let text = operation_purge::game::hud::status_text(&snap);
    assert!(!text.contains("Speed:"));
    assert!(text.starts_with("Spawned robots:"));
    // On the 2D map the HUD shows only the robot under the mouse.
    assert!(g.status_text().is_empty());
}

#[test]
fn map_hud_shows_the_hovered_robots_stats() {
    let mut g = game();
    let id = friendly_robot(&g);
    let pos = g.world.robots[&id].position;
    g.cursor_win = g.log_to_win(pos);
    let text = g.status_text();
    assert!(text.starts_with("Speed:"));
    assert!(!text.contains("Spawned robots:"));
}

#[test]
fn hud_ai_mode_line_appears_when_observing_an_ai_robot() {
    let mut g = game();
    // Force AI observation (as when observing an enemy).
    g.observer_type = ObserverType::AiRobot;
    g.active_robot = Some(friendly_robot(&g));
    let snap = g.hud_snapshot();
    assert!(snap.ai_mode);
    assert!(operation_purge::game::hud::status_text(&snap).contains("You are in AI mode"));
}

// =============================== full flow ==============================

/// Possess a friendly robot and drop into 3D play.
fn play3d(g: &mut Game) {
    let id = friendly_robot(g);
    g.select_robot(id);
    g.set_observer();
    assert!(matches!(g.screen, Screen::Play3D));
}

/// Snapshot `input` so that `cmd` reads as `just_pressed` this frame.
fn press_edge(cmd: Command) -> Input {
    let mut input = Input::new();
    input.begin_frame(); // frame 0: nothing down (suppresses first-frame edges)
    hold_command(&mut input, binding(cmd));
    input.begin_frame(); // frame 1: down now, up previously ⇒ just_pressed
    input
}

#[test]
fn esc_opens_end_game_confirm_and_no_resumes() {
    let mut g = game();
    play3d(&mut g);
    let input = press_edge(Command::EndGame);
    g.execute_commands(&input, 0.016);
    assert_eq!(g.confirm_kind(), Some(ConfirmKind::EndGame));
    assert!(g.sim_frozen(), "sim frozen behind a modal");

    // No resumes the (frozen) Play3D game.
    assert_eq!(g.resolve_confirm(ConfirmChoice::No), ConfirmAction::Resume);
    assert!(matches!(g.screen, Screen::Play3D));
}

#[test]
fn esc_held_through_the_modal_does_not_reopen_it() {
    let mut g = game();
    play3d(&mut g);
    let mut input = press_edge(Command::EndGame);
    g.execute_commands(&input, 0.016);
    g.resolve_confirm(ConfirmChoice::No);
    input.begin_frame();
    g.execute_commands(&input, 0.016);
    assert!(!g.in_confirm());
}

#[test]
fn per_type_fire_fires_that_type_not_the_last_ready_one() {
    let mut g = game();
    let id = *g
        .world
        .robots
        .iter()
        .find(|(_, r)| r.proj_states.len() >= 2)
        .expect("a robot carrying two projectile types")
        .0;
    g.select_robot(id);
    g.observer_type = ObserverType::Robot;
    g.set_observer();
    g.game_speed = 1.0;
    let ammo = |g: &Game| -> Vec<f32> {
        g.world.robots[&id].proj_states.iter().map(|s| s.projectiles_left).collect()
    };
    let before = ammo(&g);
    let first = g.world.robots[&id].proj_states[0].projectile_type.get() as usize;
    let fire = operation_purge::input::commands::fire_binding(first).unwrap();

    let mut input = Input::new();
    input.begin_frame();
    hold_command(&mut input, fire);
    input.begin_frame();
    g.execute_commands(&input, 0.016);
    g.advance(0.016);

    let after = ammo(&g);
    assert!(after[0] < before[0], "{before:?} -> {after:?}");
    assert_eq!(after[1], before[1]);
}

#[test]
fn esc_end_game_yes_requests_new_game() {
    let mut g = game();
    play3d(&mut g);
    let input = press_edge(Command::EndGame);
    g.execute_commands(&input, 0.016);
    // Yes → the app pops the New Game screen; the Game reports the action.
    assert_eq!(g.resolve_confirm(ConfirmChoice::Yes), ConfirmAction::NewGame);
}

#[test]
fn f4_restart_confirm_yes_resets_the_world() {
    let mut g = game();
    play3d(&mut g);
    g.game_speed = 4.0;
    let t0 = g.world.game_time;
    // Advance a little so game_time moves off zero.
    g.advance(0.05);
    assert!(g.world.game_time > t0);

    let input = press_edge(Command::Restart);
    g.execute_commands(&input, 0.016);
    assert_eq!(g.confirm_kind(), Some(ConfirmKind::Restart));

    assert_eq!(g.resolve_confirm(ConfirmChoice::Yes), ConfirmAction::Restart);
    // StartGame reset: back to the 2D map at speed 0, world time zeroed.
    assert!(g.in_2d());
    assert_eq!(g.game_speed, INITIAL_GAME_SPEED);
    assert_eq!(g.world.game_time, 0.0);
    assert_eq!(g.active_robot, None);
}

#[test]
fn game_over_modal_pops_in_3d_and_keep_watching_resumes() {
    let mut g = game();
    play3d(&mut g);

    // Force a Won status: everyone finished spawning, every enemy dead, a
    // friendly still alive.
    for s in g.world.spawn_info.iter_mut() {
        s.finished_spawning = true;
    }
    let active_team = g.world.active_team;
    for r in g.world.robots.values_mut() {
        if r.team != active_team {
            r.health = 0.0;
        }
    }
    assert_eq!(g.world.status(), operation_purge::sim::world::Status::Won);

    // The game-over check runs at the end of the sim step (`FrameChange`).
    g.advance(0.016);
    assert_eq!(
        g.confirm_kind(),
        Some(ConfirmKind::GameOver(GameOverKind::Won))
    );

    // Keep watching (Cancel) sets ignore_game_end and resumes; the modal
    // does not re-open next frame.
    assert_eq!(
        g.resolve_confirm(ConfirmChoice::Cancel),
        ConfirmAction::KeepWatching
    );
    assert!(g.ignore_game_end);
    assert!(matches!(g.screen, Screen::Play3D));
    g.advance(0.016);
    assert!(!g.in_confirm(), "keep-watching suppresses the modal");
}

#[test]
fn losing_the_last_friendly_pops_game_over_on_the_death_frame() {
    let mut g = game();
    play3d(&mut g);
    g.game_speed = 1.0;
    for s in g.world.spawn_info.iter_mut() {
        s.finished_spawning = true;
    }
    let active_team = g.world.active_team;
    for r in g.world.robots.values_mut() {
        if r.team == active_team {
            r.health = 0.0;
        }
    }
    g.advance(0.016);
    assert_eq!(
        g.confirm_kind(),
        Some(ConfirmKind::GameOver(GameOverKind::Lost))
    );
}

#[test]
fn game_over_check_does_not_fire_in_2d_mode() {
    let mut g = game();
    // Stay on the 2D map. Force a terminal status.
    for s in g.world.spawn_info.iter_mut() {
        s.finished_spawning = true;
    }
    let active_team = g.world.active_team;
    for r in g.world.robots.values_mut() {
        if r.team != active_team {
            r.health = 0.0;
        }
    }
    assert_eq!(g.world.status(), operation_purge::sim::world::Status::Won);
    g.execute_commands(&Input::new(), 0.016);
    g.advance(0.016);
    // PARITY: FrameChange returns before the game-over check in 2D (`:1229`).
    assert!(g.in_2d());
    assert!(!g.in_confirm());
}

#[test]
fn observer_moves_forward_along_heading_and_clamps_phi() {
    let mut g = game();
    // Enter the free observer at the building centre.
    g.select_observer_point(glam::Vec2::new(5.0, 5.0));
    assert!(g.set_observer());
    assert!(matches!(g.screen, Screen::Observer));
    assert_eq!(g.observer_type, ObserverType::Point);
    g.observer_theta = 0.0;

    let x0 = g.observer_point.x;
    let input = press_edge(Command::MoveObserverForwards);
    g.execute_commands(&input, 0.1);
    assert!(g.observer_point.x > x0, "moved +x along theta=0 heading");

    // A big downward mouse delta drives phi past +pi/2 → clamped.
    let mut input = Input::new();
    input.begin_frame();
    input.on_mouse_motion(0.0, -100000.0);
    input.begin_frame();
    g.execute_commands(&input, 0.016);
    let half_pi = std::f32::consts::FRAC_PI_2;
    assert!(g.observer_phi <= half_pi + 1e-4 && g.observer_phi >= -half_pi - 1e-4);
    assert!((g.observer_phi - half_pi).abs() < 1e-3);
}

#[test]
fn observer_cheat_fire_spawns_projectiles_only_when_cheats_on() {
    let mut g = game();
    g.select_observer_point(glam::Vec2::new(5.0, 5.0));
    g.set_observer();

    // Cheats OFF: firing does nothing.
    g.cheats_enabled = false;
    let input = press_edge(Command::FireProjectiles);
    let before = g.world.projectiles.len();
    g.execute_commands(&input, 0.016);
    assert_eq!(g.world.projectiles.len(), before, "no cheat fire without cheats");

    // Cheats ON: a fresh press spawns one projectile per projectile type.
    g.cheats_enabled = true;
    let n_types = g.world.game_data.projectile_types.len();
    let input = press_edge(Command::FireProjectiles);
    g.execute_commands(&input, 0.016);
    assert_eq!(g.world.projectiles.len(), before + n_types);
}

#[test]
fn observer_cheat_fire_respects_the_cooldown_while_held() {
    let mut g = game();
    g.select_observer_point(glam::Vec2::new(5.0, 5.0));
    g.set_observer();
    g.cheats_enabled = true;
    g.game_speed = 1.0;
    let n_types = g.world.game_data.projectile_types.len();

    // Hold fire down (NOT a fresh press): first frame fires, cooldown set.
    let mut input = Input::new();
    input.begin_frame();
    hold_command(&mut input, binding(Command::FireProjectiles));
    input.begin_frame();
    g.execute_commands(&input, 0.016);
    let after_first = g.world.projectiles.len();
    assert_eq!(after_first, n_types);

    // Next frame the key is still down but NOT just_pressed; the 0.03 s
    // cooldown blocks another shot at a 16 ms game-time step.
    input.begin_frame();
    g.execute_commands(&input, 0.016);
    assert_eq!(g.world.projectiles.len(), after_first, "cooldown blocks held-fire");
}

#[test]
fn end_game_yes_then_cancelled_new_game_resumes_play() {
    let mut g = game();
    play3d(&mut g);
    g.request_confirm(ConfirmKind::EndGame);
    assert_eq!(g.resolve_confirm(ConfirmChoice::Yes), ConfirmAction::NewGame);
    // The app shows New Game; on Cancel the game must not still be modal.
    assert!(!g.in_confirm());
    assert!(matches!(g.screen, Screen::Play3D));
}
