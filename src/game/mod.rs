//! `Game`: owns the `World` + render objects + camera + speed + timers
//! (design §2), replacing `cOperationPurge`'s app half (source-map §3.4).
//!
//! M8 wires the **playable integration** (design §7 M8, §5.1, §5.5): the
//! possession loop (2D select → possess → drive/turn/elevate/fire), the
//! game-speed ramp/pause, live HUD polling, and forced-2D on the active
//! robot's death. The frame pump lives in `app/mod.rs`; this module holds
//! the CPU-side game logic it drives (all unit-testable without a window).
//!
//! PARITY notes are attached at each ported site with a `cOperationPurge.cpp`
//! line citation. The pieces still deferred to M11 (the New Game screen,
//! Confirm/GameOver modals, observer-point cheat fire, building 2D pan) are
//! marked with `// M11:`.

pub mod hud;
pub mod newgame;
pub mod screens;

use glam::{Vec2, Vec3};

use crate::data::builtin::OBJECTS;
use crate::error::GameError;
use crate::ids::RobotId;
use crate::input::commands::{Command, CommandBindings};
use crate::input::Input;
use crate::math::intersect::MovingRect;
use crate::path::RoomGraph;
use crate::render::camera::FreeFlyCamera;
use crate::render::d3dx::D3DX_PI;
use crate::sim::robot::{Throttle, ROBOT_SIZE_XY, ROBOT_SIZE_Z};
use crate::sim::world::{RobotInput, Status, World};

use hud::{HudSnapshot, MoveState, ProjectileTypeStatus, RobotHudSnapshot};
use screens::{ConfirmChoice, ConfirmKind, GameOverKind, Screen};

/// PARITY: game speed starts paused at `0.0` after robot selection
/// (`cOperationPurge.cpp:53` `m_fGameSpeed (INITIAL_GAME_SPEED)`); `T`/`G`
/// ramp by `fGameSpeedRate * dt`, `Q` sets it to `0`, no upper bound
/// (`cOperationPurge.cpp:1132-1145`, design §5.1).
pub const INITIAL_GAME_SPEED: f32 = 0.0;

/// PARITY: the enemy-visible HUD/alarm poll runs every 0.25 s of *game* time
/// (`cOperationPurge.cpp:586`, design §5.3).
pub const ENEMY_VISIBLE_POLL_SECONDS: f32 = 0.25;

/// PARITY: `cOperationPurge::eObserverType` — which vantage the 3D camera +
/// HUD use. `Robot` = the player controls a friendly robot; `AiRobot` = the
/// player *observes* an AI robot (no control unless cheating); `Point` = the
/// free observer camera (`cOperationPurge.h`, `cOperationPurge.cpp:524`).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ObserverType {
    Robot,
    AiRobot,
    Point,
}

/// The whole live game: the simulation plus every app-half field
/// (`cOperationPurge`'s non-sim members). Rendering/audio are owned by the
/// app (`app/mod.rs`); this struct is deliberately window-free so the loop
/// logic is unit-tested headless.
pub struct Game {
    pub world: World,
    pub screen: Screen,

    /// `m_fGameSpeed`.
    pub game_speed: f32,
    /// Resolved key bindings for the 36 commands + per-projectile fire.
    pub bindings: CommandBindings,

    // --- observer / active robot (`m_ObserverType`, `m_idActiveRobot`) ---
    pub observer_type: ObserverType,
    /// `m_idActiveRobot` — the possessed/observed robot (`None` = observer
    /// point or nothing selected).
    pub active_robot: Option<RobotId>,
    /// `m_bActiveIsEnemy`.
    pub active_is_enemy: bool,
    /// `m_pt3Observer` / `m_angObserverTheta` / `m_angObserverPhi` (the free
    /// observer camera pose; the M8 possession loop leaves it at defaults).
    pub observer_point: Vec3,
    pub observer_theta: f32,
    pub observer_phi: f32,

    /// `m_bEnableCheating` (empty New Game password enables cheats, design
    /// §1.3). Default off until the New Game screen sets it (M11).
    pub cheats_enabled: bool,

    // --- HUD / display toggles + polled flags (design §5.3) ---
    /// `m_bShowMap` (F1 minimap in 3D mode).
    pub show_map: bool,
    /// `m_bEnemyVisible` (0.25 s game-time poll).
    pub enemy_visible: bool,
    /// `m_bTargeted`.
    pub targeted: bool,
    /// Whether the active robot sits above a center.
    pub above_center: bool,
    /// `fLastTime` for the 0.25 s enemy-visible poll (ex-static → field,
    /// design §5.1; `cOperationPurge.cpp:584`).
    pub enemy_visible_last_time: f32,

    /// The 2D-map cursor position in world coordinates (`WinToLog` of the
    /// mouse), pushed by the app before [`execute_commands`](Self::execute_commands)
    /// so robot selection can pick under it. Neutral (origin) headless.
    pub cursor_world: Vec2,

    /// The player's control for the possessed robot this frame, built in
    /// `execute_commands` and consumed by `advance` (so it survives the
    /// AI pass that skips the possessed robot).
    player_input: Option<(RobotId, RobotInput)>,

    /// `m_bIgnoreGameEnd` — set by the GameOver "keep watching" choice; while
    /// true the win/lose/draw check is skipped so the user can keep watching
    /// after the game ended (`cOperationPurge.cpp:1351-1354`).
    pub ignore_game_end: bool,

    /// The screen to return to when a `Confirm` modal is dismissed
    /// (No / Cancel-resume). Captured when the modal is opened.
    confirm_return: Option<Screen>,
}

impl Game {
    /// Build a fresh game around an already-constructed [`World`], entering
    /// the 2D map at speed 0 (design §5.5: New Game OK → Map2D at 0.0).
    /// Fails only if the game data's command table is malformed (GUARD).
    pub fn new(world: World) -> Result<Self, GameError> {
        let bindings = CommandBindings::from_game_data(&world.game_data)?;
        Ok(Self {
            world,
            screen: Screen::Map2D {
                forced_by_death: false,
            },
            game_speed: INITIAL_GAME_SPEED,
            bindings,
            observer_type: ObserverType::Robot,
            active_robot: None,
            active_is_enemy: false,
            observer_point: Vec3::ZERO,
            observer_theta: 0.0,
            observer_phi: 0.0,
            cheats_enabled: false,
            show_map: false,
            enemy_visible: false,
            targeted: false,
            above_center: false,
            enemy_visible_last_time: 0.0,
            cursor_world: Vec2::ZERO,
            player_input: None,
            ignore_game_end: false,
            confirm_return: None,
        })
    }

    // ============================ queries ============================

    pub fn in_2d(&self) -> bool {
        matches!(self.screen, Screen::Map2D { .. })
    }

    /// In 3D play or the free observer camera (both step the sim).
    pub fn in_3d(&self) -> bool {
        matches!(self.screen, Screen::Play3D | Screen::Observer)
    }

    /// A confirmation modal is up.
    pub fn in_confirm(&self) -> bool {
        matches!(self.screen, Screen::Confirm { .. })
    }

    /// PARITY: the sim is frozen in the 2D map and behind any modal
    /// (`cOperationPurge.cpp:1229-1233`; modal freeze is the FIX-ledger
    /// non-blocking-modal replacement, design §1.3).
    pub fn sim_frozen(&self) -> bool {
        self.in_2d() || self.in_confirm()
    }

    /// The active confirm modal's kind, if any (the app renders its prompt).
    pub fn confirm_kind(&self) -> Option<ConfirmKind> {
        match self.screen {
            Screen::Confirm { kind } => Some(kind),
            _ => None,
        }
    }

    /// Public edge/level command queries so the app (which owns the profiler
    /// UI + window) can test bound keys without duplicating the binding
    /// table.
    pub fn command_just_pressed(&self, cmd: Command, input: &Input) -> bool {
        self.cmd_pressed(cmd, input)
    }

    pub fn command_down(&self, cmd: Command, input: &Input) -> bool {
        self.cmd_down(cmd, input)
    }

    /// PARITY: `m_bForceChange` — set by `Enter2DMode(true)` when the active
    /// robot died; while true the user may not cancel back into 3D
    /// (`cOperationPurge.cpp:1022-1035`).
    pub fn forced_by_death(&self) -> bool {
        matches!(
            self.screen,
            Screen::Map2D {
                forced_by_death: true
            }
        )
    }

    /// The current win/lose/draw status (`GetGameStatus`); the app pops the
    /// GameOver modal off this (M11).
    pub fn status(&self) -> Status {
        self.world.status()
    }

    // ==================== screen / observer transitions ====================

    /// PARITY: `cOperationPurge::Enter2DMode` (`cOperationPurge.cpp:2037-2061`).
    /// Resets game speed to `INITIAL_GAME_SPEED`, switches to the 2D map, and
    /// (when forced by death) clears the active robot so the user must pick a
    /// new one.
    pub fn enter_2d_mode(&mut self, forced_by_death: bool) {
        if self.in_2d() {
            return;
        }
        self.game_speed = INITIAL_GAME_SPEED;
        self.screen = Screen::Map2D { forced_by_death };
        if forced_by_death {
            self.active_robot = None;
        }
    }

    /// PARITY: `cOperationPurge::GetRobotUnderPoint` (`cOperationPurge.cpp:2015-2034`)
    /// — first *alive* robot (ascending id) whose bounding rect contains
    /// `point`.
    pub fn robot_under_point(&self, point: Vec2) -> Option<RobotId> {
        for (id, r) in self.world.robots.iter() {
            if !r.is_dead() && crate::math::intersect::point_rect(point, r.bounding_rect()) {
                return Some(*id);
            }
        }
        None
    }

    /// PARITY: `commandSelectObserverRobot` handler (`cOperationPurge.cpp:957-966`)
    /// — select a robot as the observer target; friendly ⇒ `Robot`,
    /// enemy ⇒ `AiRobot` (AI-only observation).
    pub fn select_robot(&mut self, id: RobotId) {
        let Some(r) = self.world.robots.get(&id) else {
            return;
        };
        if r.is_dead() {
            return;
        }
        self.active_is_enemy = r.team != self.world.active_team;
        self.active_robot = Some(id);
        self.observer_type = if self.active_is_enemy {
            ObserverType::AiRobot
        } else {
            ObserverType::Robot
        };
    }

    /// PARITY: `commandSelectObserverPosition` handler (`cOperationPurge.cpp:968-977`).
    pub fn select_observer_point(&mut self, point: Vec2) {
        self.observer_point = Vec3::new(point.x, point.y, ROBOT_SIZE_Z * 0.5);
        self.observer_theta = 0.0;
        self.observer_phi = 0.0;
        self.observer_type = ObserverType::Point;
        self.active_robot = None;
    }

    /// PARITY: `commandSetObserver` handler (`cOperationPurge.cpp:979-1018`).
    /// Commits the selected observer and drops into 3D. Returns `false` (no
    /// transition) when nothing is selected. M11: the previous AI robot's
    /// `Reset()` on hand-back is deferred.
    pub fn set_observer(&mut self) -> bool {
        let valid = self.active_robot.is_some() || self.observer_type == ObserverType::Point;
        if !valid {
            return false;
        }
        if self.observer_type == ObserverType::Point {
            self.active_robot = None;
            self.screen = Screen::Observer;
        } else {
            self.screen = Screen::Play3D;
        }
        true
    }

    /// PARITY: `commandExit2DMode` handler (`cOperationPurge.cpp:1020-1035`).
    /// Cancels the 2D selection and resumes 3D — but only when the change was
    /// **not** forced by the active robot's death.
    pub fn cancel_2d(&mut self) -> bool {
        if !self.in_2d() || self.forced_by_death() {
            return false;
        }
        // Only resume if there is a vantage to resume to.
        if self.active_robot.is_some() || self.observer_type == ObserverType::Point {
            self.screen = if self.observer_type == ObserverType::Point {
                Screen::Observer
            } else {
                Screen::Play3D
            };
            true
        } else {
            false
        }
    }

    // ===================== confirm modals / restart =====================

    /// Open a non-blocking `Confirm` modal (FIX: replaces the C++ mid-frame
    /// `MessageBox`, design §1.3). The current screen is captured so No /
    /// Cancel-resume can restore it. The caller (app) stops all sounds
    /// **first** (design §5.4 ordering).
    pub fn request_confirm(&mut self, kind: ConfirmKind) {
        if self.in_confirm() {
            return;
        }
        let current = std::mem::replace(&mut self.screen, Screen::Confirm { kind });
        self.confirm_return = Some(current);
    }

    /// Resolve a `Confirm` modal choice. Resume / KeepWatching / Restart are
    /// applied here; NewGame / Exit are returned for the app to handle
    /// (they need the window + renderers). Returns `ConfirmAction::None` when
    /// no modal is up.
    pub fn resolve_confirm(&mut self, choice: ConfirmChoice) -> screens::ConfirmAction {
        use screens::ConfirmAction;
        let Some(kind) = self.confirm_kind() else {
            return ConfirmAction::None;
        };
        let action = kind.resolve(choice);
        match action {
            ConfirmAction::Resume => {
                self.screen = self.confirm_return.take().unwrap_or(Screen::Play3D);
            }
            ConfirmAction::KeepWatching => {
                self.ignore_game_end = true;
                self.screen = self.confirm_return.take().unwrap_or(Screen::Play3D);
            }
            ConfirmAction::Restart => {
                self.restart();
            }
            // NewGame / Exit / None: the app rebuilds the world / quits.
            _ => {}
        }
        action
    }

    /// PARITY: `cOperationPurge::StartGame` restart path
    /// (`cOperationPurge.cpp:153-306`) reusing the same maze/game/team: reset
    /// the world and every app-half field, then force the 2D map. The
    /// building geometry (and its renderer) is unchanged, so only the world
    /// is rebuilt.
    pub fn restart(&mut self) {
        self.world.start_game();
        self.game_speed = INITIAL_GAME_SPEED;
        self.observer_type = ObserverType::Robot;
        self.active_robot = None;
        self.active_is_enemy = false;
        self.ignore_game_end = false;
        self.confirm_return = None;
        self.enemy_visible = false;
        self.targeted = false;
        self.above_center = false;
        self.enemy_visible_last_time = 0.0;
        // Enter2DMode(true) — forced (no active robot to return to).
        self.screen = Screen::Map2D {
            forced_by_death: true,
        };
    }

    // ========================= command execution =========================

    fn cmd_down(&self, cmd: Command, input: &Input) -> bool {
        self.bindings.binding(cmd).is_down(input)
    }

    fn cmd_pressed(&self, cmd: Command, input: &Input) -> bool {
        self.bindings.binding(cmd).just_pressed(input)
    }

    /// PARITY: `cOperationPurge::ExecuteCommands` (`cOperationPurge.cpp:880-1225`).
    /// Runs **every** frame regardless of focus (design §5.1); `real_dt` is
    /// the wall-clock delta (the game-speed ramp and mouse feel are in real
    /// time, exactly as the C++ passes `fTimeElapsed`). The M8 subset covers:
    /// 2D-map selection/possession, the game-speed ramp/pause, AI-mode toggle,
    /// and the possession control (fire / throttle / turn / elevate / accel).
    /// Profiler toggles, building resize/pan and observer-point cheat fire are
    /// M11.
    pub fn execute_commands(&mut self, input: &Input, real_dt: f32) {
        self.player_input = None;

        // Behind a modal, no game commands are processed (the sim is frozen
        // and Y/N/C are routed through `resolve_confirm`).
        if self.in_confirm() {
            return;
        }

        let ui = &self.world.game_data.ui;
        let game_speed_rate = ui.game_speed_rate;
        let rotate_rate = ui.rotate_rate;
        let elevate_rate = ui.elevate_rate;
        let accelerate_rate = ui.accelerate_rate;

        // PARITY: EndGame (Esc) / Restart (F4) are tested at the very top of
        // `ExecuteCommands`, before the 2D-mode gate, so they fire in both
        // modes (`cOperationPurge.cpp:889-903`). Each opens a non-blocking
        // Confirm modal (FIX: no mid-frame MessageBox, design §1.3).
        if self.cmd_down(Command::EndGame, input) {
            self.request_confirm(ConfirmKind::EndGame);
            return;
        }
        if self.cmd_down(Command::Restart, input) {
            self.request_confirm(ConfirmKind::Restart);
            return;
        }

        if self.in_2d() {
            // These commands can only run in 2D mode (`cOperationPurge.cpp:956-1058`).
            if self.cmd_pressed(Command::SelectObserverRobot, input) {
                if let Some(id) = self.robot_under_point(self.cursor_world) {
                    self.select_robot(id);
                }
            }
            if self.cmd_pressed(Command::SelectObserverPosition, input) {
                self.select_observer_point(self.cursor_world);
            }
            if self.cmd_pressed(Command::SetObserver, input) {
                self.set_observer();
            }
            if self.cmd_pressed(Command::Exit2DMode, input) {
                self.cancel_2d();
            }
            // 2D commands processed; do not fall through to 3D (`:1058`).
            return;
        }

        // ---- 3D mode ----
        if self.cmd_pressed(Command::ToggleMap, input) {
            self.show_map = !self.show_map;
        }

        // Enter 2D mode (`cOperationPurge.cpp:1122-1129`): if we just entered,
        // skip the remaining 3D commands this frame.
        if self.cmd_down(Command::Enter2DMode, input) {
            self.enter_2d_mode(false);
            return;
        }

        // Game speed (`cOperationPurge.cpp:1132-1145`).
        if self.cmd_down(Command::IncreaseGameSpeed, input) {
            self.game_speed += real_dt * game_speed_rate;
        }
        if self.cmd_down(Command::DecreaseGameSpeed, input) {
            self.game_speed -= real_dt * game_speed_rate;
        }
        if self.cmd_down(Command::Pause, input) {
            self.game_speed = 0.0;
        }
        // PARITY: no upper bound; only clamped below at 0 (`:1145`).
        self.game_speed = self.game_speed.max(0.0);

        // Toggle AI mode (`cOperationPurge.cpp:1147-1162`): enemies can only be
        // AI-observed unless cheating.
        if self.cmd_pressed(Command::ToggleAIMode, input)
            && (!self.active_is_enemy || self.cheats_enabled)
        {
            match self.observer_type {
                ObserverType::Robot => self.observer_type = ObserverType::AiRobot,
                ObserverType::AiRobot => self.observer_type = ObserverType::Robot,
                ObserverType::Point => {}
            }
        }

        // Possession control (`cOperationPurge.cpp:1164-1197`, observerRobot).
        if self.observer_type == ObserverType::Robot {
            if let Some(id) = self.active_robot {
                let alive = self
                    .world
                    .robots
                    .get(&id)
                    .is_some_and(|r| !r.is_dead());
                if alive {
                    // Fire: the general fire command OR any per-projectile
                    // fire command (`:1169-1183`).
                    let fire = self.cmd_down(Command::FireProjectiles, input)
                        || self
                            .bindings
                            .fire_commands
                            .iter()
                            .any(|b| b.is_down(input));

                    // No move keys down ⇒ cruise (`:1186-1192`).
                    let throttle = if self.cmd_down(Command::Decelerate, input) {
                        Throttle::Brake
                    } else if self.cmd_down(Command::Accelerate, input) {
                        Throttle::Accelerate
                    } else {
                        Throttle::Cruise
                    };

                    // Mouse feel (`:1194-1196`).
                    let (dx, dy) = input.mouse_delta();
                    let wheel = input.wheel_delta();
                    let turn = dx as f32 * D3DX_PI * 2.0 / rotate_rate;
                    let elevate = -(dy as f32) * D3DX_PI * 0.5 / elevate_rate;
                    let accel = wheel / accelerate_rate;

                    self.player_input = Some((
                        id,
                        RobotInput {
                            throttle,
                            turn,
                            elevate,
                            accel,
                            fire,
                        },
                    ));
                }
            }
        } else if self.observer_type == ObserverType::Point {
            self.apply_observer_movement(input, real_dt);
        }

        // PARITY: game-over check at the END of `ExecuteCommands`
        // (`cOperationPurge.cpp:1351-1414`). Reached only in 3D — the 2D-mode
        // branch returns at `:1058`, so a team win/lose is only surfaced from
        // 3D. Uses the *current* (pre-`advance`) world status, exactly as the
        // C++ runs ExecuteCommands before FrameChange steps the sim. Skipped
        // while "keep watching" is set. The app stops all sounds first.
        if !self.ignore_game_end && !self.in_confirm() {
            let kind = match self.world.status() {
                Status::Won => Some(GameOverKind::Won),
                Status::Lost => Some(GameOverKind::Lost),
                Status::Draw => Some(GameOverKind::Draw),
                Status::None => None,
            };
            if let Some(kind) = kind {
                self.request_confirm(ConfirmKind::GameOver(kind));
            }
        }
    }

    /// PARITY: the free-observer command block (`cOperationPurge.cpp:1064-1120`
    /// movement + `:1199-1224` cheat fire). Mouse turns/elevates the observer
    /// (phi clamped ±π/2), E/D/S/F/R/V move along the observer's local axes at
    /// one of three speed rates (default when none held), the position is
    /// clamped inside the building with a 0.0001 margin, and — when cheats are
    /// enabled — firing spawns ownerless projectiles on a 0.03 s cooldown.
    fn apply_observer_movement(&mut self, input: &Input, real_dt: f32) {
        let ui = &self.world.game_data.ui;
        let rotate_rate = ui.rotate_rate;
        let elevate_rate = ui.elevate_rate;

        // Mouse look (`:1199-1203`).
        let (dx, dy) = input.mouse_delta();
        self.observer_theta += dx as f32 * D3DX_PI * 2.0 / rotate_rate;
        self.observer_phi += -(dy as f32) * D3DX_PI * 0.5 / elevate_rate;
        // fClamp(m_angObserverPhi, -pi/2, pi/2) (`:1202`).
        self.observer_phi = self.observer_phi.clamp(-D3DX_PI * 0.5, D3DX_PI * 0.5);

        // Movement speed rate (`:1064-1080`): 1/2/3 override the default.
        let speed = if self.cmd_down(Command::MoveObserverSpeed1, input) {
            ui.move_observer_speed1_rate
        } else if self.cmd_down(Command::MoveObserverSpeed2, input) {
            ui.move_observer_speed2_rate
        } else if self.cmd_down(Command::MoveObserverSpeed3, input) {
            ui.move_observer_speed3_rate
        } else {
            ui.move_observer_default_rate
        };

        let step = real_dt * speed;
        let (c, s) = (self.observer_theta.cos(), self.observer_theta.sin());
        // Forwards/backwards along heading (`:1082-1089`).
        if self.cmd_down(Command::MoveObserverForwards, input) {
            self.observer_point += Vec3::new(c * step, s * step, 0.0);
        }
        if self.cmd_down(Command::MoveObserverBackwards, input) {
            self.observer_point -= Vec3::new(c * step, s * step, 0.0);
        }
        // Strafe right/left (perpendicular) (`:1090-1097`).
        if self.cmd_down(Command::MoveObserverRight, input) {
            self.observer_point += Vec3::new(-s * step, c * step, 0.0);
        }
        if self.cmd_down(Command::MoveObserverLeft, input) {
            self.observer_point -= Vec3::new(-s * step, c * step, 0.0);
        }
        // Up/down along world Z (`:1098-1105`).
        if self.cmd_down(Command::MoveObserverUp, input) {
            self.observer_point.z += step;
        }
        if self.cmd_down(Command::MoveObserverDown, input) {
            self.observer_point.z -= step;
        }

        // Keep the observer inside the building (`:1107-1112`), 0.0001 margin.
        let b = &self.world.building;
        let size = Vec3::new(
            b.maze.cx as f32 * b.room_size,
            b.maze.cy as f32 * b.room_size,
            b.ceiling_height,
        );
        const MARGIN: f32 = 0.0001;
        self.observer_point.x = self.observer_point.x.clamp(MARGIN, size.x - MARGIN);
        self.observer_point.y = self.observer_point.y.clamp(MARGIN, size.y - MARGIN);
        self.observer_point.z = self.observer_point.z.clamp(MARGIN, size.z - MARGIN);

        // Toggle map (`:1114-1117`).
        if self.cmd_pressed(Command::ToggleMap, input) {
            self.show_map = !self.show_map;
        }

        // Cheat fire (`:1204-1223`): only when cheats are enabled; spawns an
        // ownerless projectile per type on a 0.03 s cooldown. The general
        // fire OR a per-type fire command triggers it; a `just_pressed`
        // bypasses the cooldown, matching the `bPressed` OR clauses.
        if self.cheats_enabled {
            let n = self.world.game_data.projectile_types.len();
            let cooldown_ready = self.world.cheat_fire_cooldown <= 0.0;
            let general_down = self.cmd_down(Command::FireProjectiles, input);
            let general_pressed = self.cmd_pressed(Command::FireProjectiles, input);
            let observer_pos = self.observer_point;
            let (theta, phi) = (self.observer_theta, self.observer_phi);
            for i in 0..n {
                let ty = crate::ids::ProjectileTypeId(i as u32);
                let type_binding = self.bindings.fire_commands.get(i);
                let type_down = type_binding.is_some_and(|b| b.is_down(input));
                let type_pressed = type_binding.is_some_and(|b| b.just_pressed(input));
                if (cooldown_ready && (general_down || type_down))
                    || general_pressed
                    || type_pressed
                {
                    self.world
                        .add_projectile(ty, None, observer_pos, 0.0, theta, phi);
                    self.world.cheat_fire_cooldown += 0.03;
                }
            }
            // fNextFire = fMax(fNextFire - fGameTimeElapsed, 0) (`:1223`). The
            // cheat clock ticks in *game* time.
            let game_dt = real_dt * self.game_speed;
            self.world.cheat_fire_cooldown =
                (self.world.cheat_fire_cooldown - game_dt).max(0.0);
        }
    }

    // ============================= advance =============================

    /// PARITY: `cOperationPurge::FrameChange` (`cOperationPurge.cpp:1227-1420`).
    /// Called only when focused (design §5.1). Drives the AI (skipping the
    /// possessed robot), commits the player's input, steps the world by
    /// `real_dt * game_speed`, forces the 2D map if the active robot died, and
    /// refreshes the HUD poll flags. Objects are **not** moved in 2D mode
    /// (`:1229-1233`).
    pub fn advance(&mut self, real_dt: f32) {
        // PARITY: no objects move in the 2D map (`:1229-1233`); the sim is
        // also frozen behind any modal (FIX non-blocking modal, design §1.3).
        if self.sim_frozen() {
            return;
        }
        let dt = real_dt * self.game_speed;

        // Fresh inputs each frame (stale possession/AI input must not linger).
        self.world.clear_inputs();

        // AI drives every robot except the player-possessed one
        // (`:631-637` — active robot uses CalcNewPosition, not …AI).
        let skip = if self.observer_type != ObserverType::AiRobot {
            self.active_robot
        } else {
            None
        };
        crate::sim::ai::run_ai_except(&mut self.world, dt, skip);

        if let Some((id, inp)) = self.player_input.take() {
            self.world.set_input(id, inp);
        }

        self.world.frame(dt);

        // Active robot destroyed ⇒ force the 2D map (`:1242-1245`).
        if self.observer_type != ObserverType::Point {
            let dead = match self.active_robot {
                Some(id) => self.world.robots.get(&id).is_none_or(|r| r.is_dead()),
                None => true,
            };
            if dead {
                self.enter_2d_mode(true);
            }
        }

        self.update_hud_flags();
    }

    /// PARITY: the targeted / above-center / enemy-visible flags computed in
    /// `GetInfoString` (`cOperationPurge.cpp:552-592`). The enemy-visible flag
    /// only refreshes every 0.25 s of game time.
    fn update_hud_flags(&mut self) {
        self.targeted = false;
        self.above_center = false;

        let observing_robot = matches!(
            self.observer_type,
            ObserverType::Robot | ObserverType::AiRobot
        );
        let Some(active) = self.active_robot.filter(|_| observing_robot) else {
            self.enemy_visible = false;
            return;
        };

        // Targeted: any AI robot whose FSM is hunting the active robot
        // (`cRobot::IsTargetingRobot`).
        for ai in self.world.ai.values() {
            if ai.state == crate::sim::ai::AiState::TargetingRobot
                && ai.targeted_robot == Some(active)
            {
                self.targeted = true;
                break;
            }
        }

        // Above a center (`IsWithinBoundries`).
        if let Some(r) = self.world.robots.get(&active) {
            self.above_center = self.world.centers.iter().any(|c| c.within_boundaries(r));
        }

        // Enemy visible — 0.25 s game-time poll (`:584-591`).
        if self.world.game_time - self.enemy_visible_last_time > ENEMY_VISIBLE_POLL_SECONDS {
            self.enemy_visible = self.active_sees_enemy();
            self.enemy_visible_last_time = self.world.game_time;
        }
    }

    /// PARITY: `GetVisibleRobots` (`cOperationPurge.cpp:1596-1634`) restricted
    /// to whether *any* enemy is visible: within `fMaxVisibleDist` and with no
    /// wall blocking the swept sight line.
    fn active_sees_enemy(&self) -> bool {
        let Some(id) = self.active_robot else {
            return false;
        };
        let Some(active) = self.world.robots.get(&id) else {
            return false;
        };
        let mvd =
            self.world.game_data.robot_types[active.robot_type.get() as usize].max_visible_dist;
        let graph = RoomGraph::new(
            &self.world.building.maze,
            self.world.building.room_size,
            self.world.building.wall_width,
        );
        let size = Vec2::new(ROBOT_SIZE_XY, ROBOT_SIZE_XY);
        for r in self.world.robots.values() {
            if r.is_dead() || r.team == active.team {
                continue;
            }
            if (active.position - r.position).length_squared() > mvd * mvd {
                continue;
            }
            let mrc = MovingRect {
                center1: active.position,
                center2: r.position,
                size,
                stop_time: 1.0,
            };
            if !graph.intersect_moving_rect(mrc).hit {
                return true;
            }
        }
        false
    }

    // ============================== HUD ==============================

    /// The active robot's HUD block, or `None` for the observer point (or when
    /// nothing is being observed) — mirrors `GetInfoString`'s `szRobotInfo`
    /// gate (`cOperationPurge.cpp:523-531`).
    fn active_robot_hud(&self) -> Option<RobotHudSnapshot> {
        if self.observer_type == ObserverType::Point {
            return None;
        }
        let id = self.active_robot?;
        let r = self.world.robots.get(&id)?;
        let def = &self.world.game_data.game_definitions[self.world.game_def.get() as usize];
        let rt = &self.world.game_data.robot_types[r.robot_type.get() as usize];
        let team_name = def
            .teams
            .get(r.team.get() as usize)
            .map(|t| t.name.clone())
            .unwrap_or_default();

        let projectile_types = r
            .proj_states
            .iter()
            .map(|s| ProjectileTypeStatus {
                fire_delay: s.fire_delay,
                fire_rate: s.fire_rate,
                ammo_left: s.projectiles_left.max(0.0) as u32,
                ammo_total: s.start_count.max(0.0) as u32,
            })
            .collect();

        let move_state = match r.move_state {
            Throttle::Accelerate => MoveState::Accelerate,
            Throttle::Cruise => MoveState::Cruise,
            Throttle::Brake => MoveState::Decelerate,
        };

        Some(RobotHudSnapshot {
            speed: r.speed,
            max_speed: r.max_speed,
            desired_speed: r.desired_speed,
            energy: r.energy,
            energy_start: r.energy_start,
            health: r.health,
            health_start: r.health_start,
            projectile_types,
            move_state,
            type_name: rt.name.clone(),
            team_name,
        })
    }

    /// Assemble the live HUD snapshot the status block is rendered from
    /// (design §5.3), polling the current world/observer state.
    pub fn hud_snapshot(&self) -> HudSnapshot {
        let def = &self.world.game_data.game_definitions[self.world.game_def.get() as usize];
        let n_teams = def.teams.len();
        let mut spawned = vec![0u32; n_teams];
        for r in self.world.robots.values() {
            if !r.is_dead() {
                let t = r.team.get() as usize;
                if t < n_teams {
                    spawned[t] += 1;
                }
            }
        }

        HudSnapshot {
            robot: self.active_robot_hud(),
            spawned_per_team: spawned,
            game_speed: self.game_speed,
            game_time: self.world.game_time,
            ai_mode: self.observer_type == ObserverType::AiRobot,
            enemy_visible: self.enemy_visible,
            targeted: self.targeted,
            above_center: self.above_center,
            projectiles: self.world.projectiles.len(),
            particles: self.world.particles.live_count(),
            effects: self.world.effects.len(),
            holes: self.world.holes.entries.len(),
        }
    }

    /// The full status text (`GetInfoString`), assembled from the live poll.
    pub fn status_text(&self) -> String {
        hud::status_text(&self.hud_snapshot())
    }

    // ============================= camera =============================

    /// PARITY: `cOperationPurge::SetupScene` view pose (`cOperationPurge.cpp:3034-3056`)
    /// expressed as a [`FreeFlyCamera`] (which builds the same `LookAtLH` +
    /// `PerspectiveFovLH · CLIP_FIX`, design §3.2). Observer-point uses
    /// `SphereCoord(theta, phi, 1)` (== the camera's yaw/pitch forward);
    /// observer-robot sits at the robot's viewpoint offset looking down its
    /// barrel. `z_far = min(fMaxVisibleDist, |building|)` (`:3044-3046`).
    pub fn camera(&self) -> FreeFlyCamera {
        let b = &self.world.building;
        let building_size = Vec3::new(
            b.maze.cx as f32 * b.room_size,
            b.maze.cy as f32 * b.room_size,
            b.ceiling_height,
        );
        let diagonal = building_size.length();

        if self.observer_type == ObserverType::Point {
            let mut cam = FreeFlyCamera::new(self.observer_point, diagonal);
            cam.yaw = self.observer_theta;
            cam.pitch = self.observer_phi;
            return cam;
        }

        if let Some(id) = self.active_robot {
            if let Some(r) = self.world.robots.get(&id) {
                // ViewpointOffset = (0,0, treadRadius*2) (`cRobot3D.cpp:52`).
                let eye = Vec3::new(r.position.x, r.position.y, OBJECTS.tread_radius * 2.0);
                let mvd =
                    self.world.game_data.robot_types[r.robot_type.get() as usize].max_visible_dist;
                let mut cam = FreeFlyCamera::new(eye, mvd.min(diagonal));
                cam.yaw = r.direction;
                cam.pitch = r.barrel_elevation;
                return cam;
            }
        }

        // Fallback overview (nothing selected / in 2D): center of the building.
        FreeFlyCamera::new(building_size * 0.5, diagonal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::gamedata;
    use crate::ids::{GameDefId, TeamId};
    use crate::maze::Maze;
    use std::path::Path;

    fn shipped_world() -> World {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
        let data = gamedata::load(&root.join("GameData.toml")).expect("GameData.toml parses");
        let maze = Maze::load(&root.join("MazeFiles/Small1.maz")).expect("Small1.maz loads");
        World::new(data, maze, GameDefId(0), TeamId(0), 12345)
    }

    fn game() -> Game {
        Game::new(shipped_world()).expect("bindings resolve")
    }

    #[test]
    fn starts_in_map2d_at_zero_speed() {
        let g = game();
        assert!(g.in_2d());
        assert!(!g.forced_by_death());
        assert_eq!(g.game_speed, INITIAL_GAME_SPEED);
        assert_eq!(g.game_speed, 0.0);
    }

    #[test]
    fn select_friendly_robot_then_possess_enters_play3d() {
        let mut g = game();
        // Team 0 is the active team; find one of its robots.
        let id = *g
            .world
            .robots
            .iter()
            .find(|(_, r)| r.team == TeamId(0))
            .expect("a friendly robot spawned")
            .0;
        g.select_robot(id);
        assert_eq!(g.active_robot, Some(id));
        assert_eq!(g.observer_type, ObserverType::Robot);
        assert!(!g.active_is_enemy);

        assert!(g.set_observer());
        assert!(matches!(g.screen, Screen::Play3D));
    }

    #[test]
    fn selecting_enemy_robot_is_ai_observation_only() {
        let mut g = game();
        let enemy = g
            .world
            .robots
            .iter()
            .find(|(_, r)| r.team != TeamId(0))
            .map(|(id, _)| *id);
        if let Some(id) = enemy {
            g.select_robot(id);
            assert_eq!(g.observer_type, ObserverType::AiRobot);
            assert!(g.active_is_enemy);
        }
    }

    #[test]
    fn set_observer_without_selection_does_not_transition() {
        let mut g = game();
        assert!(!g.set_observer());
        assert!(g.in_2d());
    }

    #[test]
    fn play3d_a_returns_to_map2d_via_enter_2d_mode() {
        let mut g = game();
        g.screen = Screen::Play3D;
        g.game_speed = 3.0;
        g.enter_2d_mode(false);
        assert!(g.in_2d());
        assert!(!g.forced_by_death());
        // Entering 2D resets the game speed (`Enter2DMode`).
        assert_eq!(g.game_speed, 0.0);
    }

    #[test]
    fn cancel_2d_resumes_but_forced_by_death_cannot_be_cancelled() {
        let mut g = game();
        // Possess, go 3D, then forced back by death.
        let id = *g.world.robots.keys().next().unwrap();
        g.select_robot(id);
        g.set_observer();
        g.enter_2d_mode(true);
        assert!(g.forced_by_death());
        assert!(!g.cancel_2d(), "forced-by-death 2D may not be cancelled");
        assert!(g.in_2d());
    }

    #[test]
    fn cancel_2d_resumes_when_not_forced() {
        let mut g = game();
        let id = *g.world.robots.keys().next().unwrap();
        g.select_robot(id);
        g.set_observer(); // now Play3D with an active robot
        g.enter_2d_mode(false);
        assert!(g.in_2d());
        assert!(g.cancel_2d());
        assert!(matches!(g.screen, Screen::Play3D));
    }

    #[test]
    fn death_of_possessed_robot_forces_map2d() {
        let mut g = game();
        let id = *g
            .world
            .robots
            .iter()
            .find(|(_, r)| r.team == TeamId(0))
            .map(|(id, _)| id)
            .unwrap();
        g.select_robot(id);
        g.set_observer();
        assert!(matches!(g.screen, Screen::Play3D));
        // Kill the possessed robot, then advance one frame with speed > 0.
        g.world.robots.get_mut(&id).unwrap().health = 0.0;
        g.game_speed = 1.0;
        g.advance(0.016);
        assert!(g.in_2d());
        assert!(g.forced_by_death());
        assert_eq!(g.active_robot, None);
    }

    #[test]
    fn advance_does_not_step_the_world_in_2d_mode() {
        let mut g = game();
        g.game_speed = 1.0; // even with speed, 2D mode freezes the sim
        let t0 = g.world.game_time;
        g.advance(0.5);
        assert_eq!(g.world.game_time, t0);
    }
}
