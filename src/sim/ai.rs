//! `cAIRobot`: the 3-state FSM (idle / targeting-center / targeting-robot),
//! scan/resupply/hunt, brake-to-stop, wall/robot-bounce quirks
//! (source-map §3.5, §1), ported from `cAIRobot.cpp`.
//!
//! # Integration with the frozen M6 loop
//!
//! `sim::world::World::frame` (M6) is a fixed pipeline: `apply_inputs`
//! (fire/throttle/rotate/elevate/accelerate from a per-robot [`RobotInput`])
//! → two-phase movement → projectile hits → remove-dead → transfer. It has
//! no AI hook in the movement phase and does not surface collision details.
//!
//! So the AI runs as a **pre-frame pass** ([`run_ai`]) that, for every
//! computer-controlled robot (one [`AiRuntime`] per robot), reproduces
//! `cAIRobot::CalcNewPositionAI`'s *decision* half by writing the robot's
//! `direction_desired` / `barrel_elevation` / `move_state` / `desired_speed`
//! fields directly (exactly as the C++ does) and emitting a `RobotInput`
//! that is a **no-op for movement** (zero turn/elevate/accel deltas,
//! `throttle == move_state`) carrying only the fire decision — so
//! `apply_inputs` reproduces the AI's intent without clobbering it, and the
//! subsequent `calc_new_position` performs `cRobot::CalcNewPosition` (which
//! sets `direction = direction_desired`) in the same relative order as the
//! C++ (fire uses the pre-update heading in both).
//!
//! `cAIRobot::MoveAI` (post-move bounce + `ChangeDesiredDirection` path
//! advancement) is folded into the *start* of the next frame's pass, reading
//! the robot's committed post-move state (`collided`, `position`, `move_vec`)
//! and the stored pre-move `position_old`. Because the frozen loop discards
//! the exact `tCollision2D`, the collider is **reconstructed** (nearest
//! overlapping robot ⇒ robot collision, else wall collision keyed on the
//! dominant move axis) — a documented deviation; the reconstruction is
//! deterministic and the isolated bounce/turn logic is unit-tested directly.
//!
//! PARITY (design §1.3, preserved verbatim):
//! - AI turns **toward** robot colliders (`cAIRobot.cpp:130`,
//!   [`bounce_turn_toward_robot`]) — a bug (should turn away), kept as
//!   behavior-defining. NB the subsequent `EnterIdleMode` overwrites
//!   `direction_desired` with the heading toward `m_pt2Desired`
//!   (`cAIRobot.cpp:161,541`); both are ported verbatim.
//! - Wall-bounce heading snap to `{0, π/2, π, 3π/2}` (`cAIRobot.cpp:135-148`).
//! - Brake law `v²/(2d)` to stop inside a targeted center
//!   (`cAIRobot.cpp:413-426`).
//! - Low-level lookahead `min(2, len-1)` rooms (`cAIRobot.cpp:765`).
//! - `PreventCollision` is DEAD code (commented out at its only call site,
//!   `cAIRobot.cpp:87-95`) — intentionally NOT ported.

use crate::data::{ProjectileType, RobotType};
use crate::ids::{CenterId, RobotId, TeamId};
use crate::math::intersect::{point_rect, MovingRect, Rect};
use crate::math::{between, equals, get_angle};
use crate::path::{find_high_level_path, find_low_level_path, RoomGraph};
use crate::sim::center::CenterKind;
use crate::sim::projectile::solve_aim;
use crate::sim::robot::{Collision2D, CollisionKind, Robot, Throttle, ROBOT_SIZE_XY};
use crate::sim::world::{RobotInput, World};
use glam::{Vec2, Vec3};
use std::collections::VecDeque;
use std::f32::consts::PI;

/// The AI's 3-state FSM (`cAIRobot::eAIState`, `cAIRobot.h:31-36`).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum AiState {
    /// `stateIdle` — not pursuing anything; drive straight / re-scan.
    Idle,
    /// `stateTargetingCenter` — heading toward a resupply center.
    TargetingCenter,
    /// `stateTargetingRobot` — hunting a specific enemy robot.
    TargetingRobot,
}

/// `eCenterType` (source-map §1) — which resource a center transfers.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum CenterKindTag {
    Ammo,
    Energy,
    Health,
}

impl CenterKindTag {
    fn of(kind: &CenterKind) -> Self {
        match kind {
            CenterKind::Ammo { .. } => CenterKindTag::Ammo,
            CenterKind::Energy { .. } => CenterKindTag::Energy,
            CenterKind::Health { .. } => CenterKindTag::Health,
        }
    }
}

/// Snapshot of a `tAIRobotType` (the AI's tuning knobs; `cAIRobot.h:52`
/// `m_shOptions`), copied in at runtime creation.
#[derive(Clone, Debug)]
pub struct AiOptions {
    pub desired_speed: f32,
    pub scan_delay: f32,
    pub min_wall_dist: f32,
    pub desired_ammo: f32,
    pub desired_energy: f32,
    pub desired_health: f32,
    pub critical_ammo: f32,
    pub critical_energy: f32,
    pub critical_health: f32,
}

/// Per-robot AI runtime state — the non-`cRobot` members of `cAIRobot`
/// (`cAIRobot.h:52-66`), held alongside its `sim::robot::Robot` in
/// `World::ai`.
pub struct AiRuntime {
    pub options: AiOptions,
    pub room_size: f32,
    /// `m_AIState`.
    pub state: AiState,
    /// `m_HighLevelPath` (front = index 0).
    pub high_level_path: Vec<Vec2>,
    /// `m_Path` (a `std::list`).
    pub path: VecDeque<Vec2>,
    /// `m_pt2Desired`.
    pub desired: Vec2,
    /// `m_pt2Target`.
    pub target: Vec2,
    /// `m_idTargetedCenter`.
    pub targeted_center: Option<CenterId>,
    /// `m_idTransferCenter`.
    pub transfer_center: Option<CenterId>,
    /// `m_idTargetedRobot`.
    pub targeted_robot: Option<RobotId>,
    /// `m_TargetedCenterType`.
    pub targeted_center_type: Option<CenterKindTag>,
    /// `m_bTargetVisible`.
    pub target_visible: bool,
    /// `m_fScanDelay`.
    pub scan_delay: f32,
    /// The robot's position at the start of the previous frame's move,
    /// fed to `ChangeDesiredDirection` (`pt2PositionOld`, `cAIRobot.cpp:164`).
    pub position_old: Vec2,
}

impl AiRuntime {
    /// `cAIRobot::cAIRobot` (`cAIRobot.cpp:15-41`): `Reset ()` then randomize
    /// the initial heading. Mutates the freshly-spawned `robot`'s heading in
    /// place (the C++ ctor sets `m_angDirectionDesired`/`m_angDirection`).
    ///
    /// PARITY: two `fRand ()` draws in this exact order — first
    /// `m_fScanDelay = fRand () * scanDelay` (inside `Reset`), then
    /// `m_angDirectionDesired = fRand () * 2π` — so the shared RNG stream
    /// matches (`cAIRobot.cpp:39,60`).
    pub fn new(robot: &mut Robot, options: AiOptions, room_size: f32, rng: &mut crate::rng::GameRng) -> Self {
        let mut ai = Self {
            options,
            room_size,
            state: AiState::Idle,
            high_level_path: Vec::new(),
            path: VecDeque::new(),
            desired: robot.position,
            target: robot.position,
            targeted_center: None,
            transfer_center: None,
            targeted_robot: None,
            targeted_center_type: None,
            target_visible: false,
            scan_delay: 0.0,
            position_old: robot.position,
        };
        // Reset (): EnterIdleMode + m_fScanDelay = fRand () * fScanDelay.
        ai.enter_idle_mode(robot);
        ai.scan_delay = rng.frand() * ai.options.scan_delay;
        // Randomize the robot's direction (ctor, cAIRobot.cpp:39-40).
        robot.direction_desired = rng.frand() * PI * 2.0;
        robot.direction = robot.direction_desired;
        ai
    }

    /// `cAIRobot::EnterIdleMode` (`cAIRobot.cpp:530-542`).
    fn enter_idle_mode(&mut self, robot: &mut Robot) {
        self.state = AiState::Idle;
        robot.move_state = Throttle::Accelerate;
        self.targeted_center = None;
        self.transfer_center = None;
        self.targeted_robot = None;
        self.targeted_center_type = None;
        self.target_visible = false;
        self.path.clear();
        self.high_level_path.clear();
        // FaceDesiredPosition ().
        robot.direction_desired = get_angle(robot.position, self.desired);
    }
}

// -------------------- robot / center views (snapshots) --------------------

/// Read-only view of another robot for targeting/visibility (positions are
/// fixed during the AI pass; `is_dead` never flips mid-pass).
#[derive(Copy, Clone)]
struct RobotView {
    id: RobotId,
    team: TeamId,
    position: Vec2,
    is_dead: bool,
}

/// Read-only view of a center for `GetReadyCenters`/targeting.
#[derive(Copy, Clone)]
struct CenterView {
    id: CenterId,
    kind: CenterKindTag,
    position: Vec2,
    size: f32,
    transferring: bool,
    transfer_robot: Option<RobotId>,
}

impl CenterView {
    /// `cCenter::IsTransferReady` (`cCenter.cpp:94-97`).
    fn is_transfer_ready(&self, id: RobotId) -> bool {
        !self.transferring || self.transfer_robot == Some(id)
    }
}

// ------------------------------ the pass -----------------------------------

/// Run one AI decision pass over every computer-controlled robot, in
/// ascending `RobotId` order (matching `std::map` iteration). Call this
/// **before** [`World::frame`]; it fills `World::inputs` for each AI robot.
///
/// PARITY: the whole-map ordering + per-robot `CalcNewPositionAI` decision
/// then `MoveAI` (folded to next frame's start) mirrors `MoveObjects`
/// (`cOperationPurge.cpp` sim half): AI decisions run before projectile/
/// transfer resolution, which the frozen `frame()` also does.
pub fn run_ai(world: &mut World, dt: f32) {
    run_ai_except(world, dt, None);
}

/// Like [`run_ai`], but skips the AI *decision* for `skip` — the
/// player-possessed robot (design §5.1/§5.5, M8). PARITY:
/// `cOperationPurge::MoveObjects` drives the active robot with
/// `CalcNewPosition` (player input) instead of `CalcNewPositionAI` when
/// `itRobot->first == m_idActiveRobot && m_ObserverType != observerAIRobot`
/// (`cOperationPurge.cpp:631-637,660-666`). The skipped robot still gets its
/// `AiRuntime` created (its ctor RNG draws happen at spawn regardless) but no
/// per-frame AI decision runs and no `RobotInput` is written for it, so the
/// player's input set by `game::Game::execute_commands` survives into
/// `World::frame`.
pub fn run_ai_except(world: &mut World, dt: f32, skip: Option<RobotId>) {
    ensure_ai(world);

    // Snapshot the world (immutable) before mutating individual robots.
    let robot_views: Vec<RobotView> = world
        .robots
        .values()
        .map(|r| RobotView {
            id: r.id,
            team: r.team,
            position: r.position,
            is_dead: r.is_dead(),
        })
        .collect();
    let center_views: Vec<CenterView> = world
        .centers
        .iter()
        .map(|c| CenterView {
            id: c.id,
            kind: CenterKindTag::of(&c.kind),
            position: c.position,
            size: c.size,
            transferring: c.transferring,
            transfer_robot: c.transfer_robot,
        })
        .collect();

    let ids: Vec<RobotId> = world.robots.keys().copied().collect();

    // Disjoint-field borrows: graph reads world.building.maze; game_data
    // reads world.game_data; the per-robot mutation touches world.robots /
    // world.ai / world.inputs. All distinct fields.
    let graph = RoomGraph::new(
        &world.building.maze,
        world.building.room_size,
        world.building.wall_width,
    );
    let robot_types = &world.game_data.robot_types;
    let projectile_types = &world.game_data.projectile_types;

    for id in ids {
        // The player-possessed robot is driven by `RobotInput` from
        // `execute_commands`, not by the AI (design §5.5, M8).
        if Some(id) == skip {
            continue;
        }
        // Skip (defensive) if the entities vanished.
        if !world.robots.contains_key(&id) || !world.ai.contains_key(&id) {
            continue;
        }

        let fire = {
            let robot = world.robots.get_mut(&id).unwrap();
            if robot.is_dead() {
                continue;
            }
            let ai = world.ai.get_mut(&id).unwrap();
            let mut ctx = AiCtx {
                robot,
                ai,
                graph: &graph,
                robot_types,
                projectile_types,
                robots: &robot_views,
                centers: &center_views,
                id,
                fire: false,
            };

            // MoveAI half (last frame's outcome), then CalcNewPositionAI half.
            ctx.move_ai_reaction();
            ctx.calc_new_position_ai_decide(dt);
            ctx.ai.position_old = ctx.robot.position;
            ctx.fire
        };

        // Emit a movement-neutral RobotInput carrying only the fire decision.
        let throttle = world.robots.get(&id).unwrap().move_state;
        world.inputs.insert(
            id,
            RobotInput {
                throttle,
                turn: 0.0,
                elevate: 0.0,
                accel: 0.0,
                fire,
            },
        );
    }
}

/// Create an [`AiRuntime`] for every robot that lacks one (newly spawned),
/// and drop runtimes whose robot has died (`RobotId` reuse safety).
fn ensure_ai(world: &mut World) {
    // Prune dead robots' runtimes.
    world.ai.retain(|id, _| world.robots.contains_key(id));

    // robot_type -> ai_robot_type from the active game definition's groups.
    let def = &world.game_data.game_definitions[world.game_def.get() as usize];
    let mut rt_to_ai: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
    for team in &def.teams {
        for group in &team.robot_groups {
            if let (Some(rt), Some(ai)) = (group.robot_type, group.ai_robot_type) {
                rt_to_ai.entry(rt.get()).or_insert(ai.get());
            }
        }
    }

    let missing: Vec<RobotId> = world
        .robots
        .keys()
        .filter(|id| !world.ai.contains_key(id))
        .copied()
        .collect();
    let room_size = world.building.room_size;

    for id in missing {
        let ai_type_idx = {
            let robot = world.robots.get(&id).unwrap();
            rt_to_ai.get(&robot.robot_type.get()).copied().unwrap_or(0) as usize
        };
        let options = {
            let t = &world.game_data.ai_robot_types[ai_type_idx];
            AiOptions {
                desired_speed: t.desired_speed,
                scan_delay: t.scan_delay,
                min_wall_dist: t.min_wall_dist,
                desired_ammo: t.desired_ammo,
                desired_energy: t.desired_energy,
                desired_health: t.desired_health,
                critical_ammo: t.critical_ammo,
                critical_energy: t.critical_energy,
                critical_health: t.critical_health,
            }
        };
        // Disjoint: world.robots (mut) + world.rng (mut).
        let robot = world.robots.get_mut(&id).unwrap();
        let runtime = AiRuntime::new(robot, options, room_size, &mut world.rng);
        world.ai.insert(id, runtime);
    }
}

// ------------------------------ per-robot ----------------------------------

/// Transient bundle of the borrows needed to run one robot's AI, with
/// methods mirroring `cAIRobot`'s members.
struct AiCtx<'a> {
    robot: &'a mut Robot,
    ai: &'a mut AiRuntime,
    graph: &'a RoomGraph<'a>,
    robot_types: &'a [RobotType],
    projectile_types: &'a [ProjectileType],
    robots: &'a [RobotView],
    centers: &'a [CenterView],
    id: RobotId,
    fire: bool,
}

impl AiCtx<'_> {
    fn size(&self) -> Vec2 {
        Vec2::new(ROBOT_SIZE_XY, ROBOT_SIZE_XY)
    }

    fn max_visible_dist(&self) -> f32 {
        self.robot_types[self.robot.robot_type.get() as usize].max_visible_dist
    }

    fn team(&self) -> TeamId {
        self.robot.team
    }

    // ---- resource percentages (cRobot.cpp:739-776) ----

    fn min_ammo_percentage(&self) -> f32 {
        let mut min = 1.0f32;
        for s in &self.robot.proj_states {
            let pct = if s.start_count == 0.0 { 1.0 } else { s.projectiles_left / s.start_count };
            min = crate::sim::robot::f_min(pct, min);
        }
        min
    }

    fn max_ammo_percentage(&self) -> f32 {
        let mut max = 0.0f32;
        for s in &self.robot.proj_states {
            let pct = if s.start_count == 0.0 { 0.0 } else { s.projectiles_left / s.start_count };
            max = crate::sim::robot::f_max(pct, max);
        }
        max
    }

    fn energy_percentage(&self) -> f32 {
        self.robot.energy / self.robot.energy_start
    }

    fn health_percentage(&self) -> f32 {
        self.robot.health / self.robot.health_start
    }

    // ---- visibility (cOperationPurge.cpp:1691-1728) ----

    fn is_within_range(&self, point: Vec2) -> f32 {
        (self.robot.position - point).length_squared()
    }

    fn point_visible_swept(&self, point: Vec2) -> bool {
        let mvd = self.max_visible_dist();
        if self.is_within_range(point) > mvd * mvd {
            return false;
        }
        let mrc = MovingRect {
            center1: self.robot.position,
            center2: point,
            size: self.size(),
            stop_time: 1.0,
        };
        !self.graph.intersect_moving_rect(mrc).hit
    }

    // ---- transfer desire/critical (cAIRobot.cpp:478-527) ----

    fn is_transfer_desired(&self, ty: CenterKindTag) -> bool {
        match ty {
            CenterKindTag::Ammo => self.min_ammo_percentage() <= self.ai.options.desired_ammo,
            CenterKindTag::Energy => self.energy_percentage() <= self.ai.options.desired_energy,
            CenterKindTag::Health => self.health_percentage() <= self.ai.options.desired_health,
        }
    }

    fn is_transfer_critical(&self, ty: CenterKindTag) -> bool {
        match ty {
            CenterKindTag::Ammo => self.max_ammo_percentage() <= self.ai.options.critical_ammo,
            CenterKindTag::Energy => self.energy_percentage() <= self.ai.options.critical_energy,
            CenterKindTag::Health => self.health_percentage() <= self.ai.options.critical_health,
        }
    }

    // ---- center transfer possibility (cCenter.cpp:99-104) ----

    fn is_transfer_possible_with(&self, center: &CenterView) -> bool {
        if !self.robot.is_stationary() {
            return false;
        }
        // cCenter::IsWithinBoundries (cCenter.cpp:78-91): robot bounding rect
        // fully inside the center square shrunk by the robot's size.
        let robot_rect = self.robot.bounding_rect();
        let small = Rect {
            center: center.position,
            size: Vec2::new(center.size, center.size) - robot_rect.size,
        };
        point_rect(robot_rect.center, small)
    }

    // ---- facing (cAIRobot.cpp:698-707) ----

    fn face_point(&mut self, point: Vec2) {
        self.robot.direction_desired = get_angle(self.robot.position, point);
    }

    fn face_desired_position(&mut self) {
        let desired = self.ai.desired;
        self.face_point(desired);
    }

    fn enter_idle_mode(&mut self) {
        // Delegated to the runtime helper so the ctor and here agree.
        let robot = &mut *self.robot;
        self.ai.enter_idle_mode(robot);
    }

    // ---- CalcNewPositionAI decision half (cAIRobot.cpp:64-98) ----

    fn calc_new_position_ai_decide(&mut self, dt: f32) {
        if self.ai.scan_delay <= 0.0 {
            self.choose_targeted_center();
            self.choose_targeted_robot();
            self.ai.scan_delay = self.ai.options.scan_delay;
        }
        self.move_to_targeted_center();
        self.attack_targeted_robot();
        self.ai.scan_delay -= dt;

        if self.ai.state != AiState::TargetingRobot {
            // Keep the barrel parallel to the ground when not targeting.
            self.robot.barrel_elevation = 0.0;
        }
        if self.ai.state == AiState::Idle
            && equals(self.robot.direction, self.robot.direction_desired)
        {
            // Finished turning while idle: start moving again.
            self.robot.move_state = Throttle::Accelerate;
        }

        // PARITY: PreventCollision block (cAIRobot.cpp:87-95) is commented
        // out in the original — omitted here.

        // m_fDesiredSpeed = transferCenter valid ? 0 : option.desired_speed.
        self.robot.desired_speed = if self.ai.transfer_center.is_none() {
            self.ai.options.desired_speed
        } else {
            0.0
        };
        // cRobot::CalcNewPosition is performed by the frozen frame().
    }

    // ---- ChooseTargetedCenter (cAIRobot.cpp:185-246) ----

    fn choose_targeted_center(&mut self) {
        if self.ai.state == AiState::TargetingCenter {
            if let Some(cid) = self.ai.targeted_center {
                if let Some(c) = self.centers.iter().find(|c| c.id == cid) {
                    // Already targeting a valid center; just refresh visibility.
                    self.ai.target_visible = self.point_visible_swept(c.position);
                    return;
                }
            }
        }

        // GetReadyCenters (cOperationPurge.cpp:1637-1654).
        let mut choice_dist = 0.0f32;
        let mut chosen: Option<(CenterId, CenterKindTag, Vec2)> = None;
        for c in self.centers.iter() {
            if !c.is_transfer_ready(self.id) {
                continue;
            }
            let ty = c.kind;
            let desired = self.is_transfer_desired(ty);
            let critical = self.is_transfer_critical(ty);
            let want = (self.ai.state == AiState::Idle && (desired || critical))
                || (self.ai.state == AiState::TargetingRobot && critical);
            let dist = (self.robot.position - c.position).length_squared();
            if want && (chosen.is_none() || dist < choice_dist) {
                chosen = Some((c.id, ty, c.position));
                choice_dist = dist;
            }
        }

        let (cid, ty, pos) = match chosen {
            Some(v) => v,
            None => return,
        };

        if self.find_path(pos, false) {
            self.ai.state = AiState::TargetingCenter;
            self.ai.targeted_center_type = Some(ty);
            self.ai.targeted_center = Some(cid);
        }
    }

    // ---- ChooseTargetedRobot (cAIRobot.cpp:250-326) ----

    fn choose_targeted_robot(&mut self) {
        if self.ai.state == AiState::TargetingCenter {
            if let Some(ty) = self.ai.targeted_center_type {
                if self.is_transfer_critical(ty) {
                    // On the way to a critically-needed center; cannot target.
                    return;
                }
            }
        }

        if self.ai.state == AiState::TargetingRobot {
            if let Some(rid) = self.ai.targeted_robot {
                if let Some(rv) = self.robots.iter().find(|r| r.id == rid) {
                    if !rv.is_dead {
                        self.ai.target_visible = self.point_visible_swept(rv.position);
                        if self.ai.target_visible {
                            return;
                        }
                    }
                }
            }
        }

        // GetVisibleRobots vs GetEnemyRobots (cOperationPurge.cpp:1596-1634).
        let restrict_visible = self.ai.state == AiState::TargetingRobot;
        let team = self.team();
        let mut choice_dist = 0.0f32;
        let mut chosen: Option<(RobotId, Vec2)> = None;
        for rv in self.robots.iter() {
            if rv.is_dead || rv.team == team {
                continue;
            }
            if restrict_visible && !self.point_visible_swept(rv.position) {
                continue;
            }
            let dist = (self.robot.position - rv.position).length_squared();
            if chosen.is_none() || dist < choice_dist {
                chosen = Some((rv.id, rv.position));
                choice_dist = dist;
            }
        }

        let (rid, pos) = match chosen {
            Some(v) => v,
            None => return,
        };

        if self.find_path(pos, false) {
            self.ai.target_visible = restrict_visible || self.point_visible_swept(pos);
            self.ai.state = AiState::TargetingRobot;
            self.ai.targeted_robot = Some(rid);
        }
    }

    // ---- MoveToTargetedCenter (cAIRobot.cpp:330-436) ----

    fn move_to_targeted_center(&mut self) {
        if let Some(cid) = self.ai.transfer_center {
            let center = match self.centers.iter().find(|c| c.id == cid) {
                Some(c) => *c,
                None => return, // logged in C++, then return
            };
            if !center.is_transfer_ready(self.id) {
                // The center is in use by another robot.
                self.enter_idle_mode();
                return;
            }
            let done = match center.kind {
                CenterKindTag::Ammo => equals(self.min_ammo_percentage(), 1.0),
                CenterKindTag::Energy => equals(self.energy_percentage(), 1.0),
                CenterKindTag::Health => equals(self.health_percentage(), 1.0),
            };
            if done {
                self.enter_idle_mode();
            }
        } else if self.ai.state == AiState::TargetingCenter {
            let center = self.ai.targeted_center.and_then(|cid| {
                self.centers.iter().find(|c| c.id == cid).copied()
            });
            let center = match center {
                Some(c) => c,
                None => return, // logged in C++, then return
            };

            if self.ai.target_visible {
                // Aim toward the targeted center and drop the (now moot) path.
                self.ai.target = center.position;
                self.face_point(center.position);
                self.ai.high_level_path.clear();
                self.ai.path.clear();

                // Brake law: slow to stop in the middle of the center.
                // PARITY: v²/(2d) vs friction/brake decel (cAIRobot.cpp:410-426).
                let friction = self.robot.friction_decel;
                let brake = self.robot.brake_decel;
                let dist = (self.robot.position - center.position).length();
                let desired_decel = self.robot.speed * self.robot.speed / (2.0 * dist);
                if desired_decel < friction {
                    self.robot.move_state = Throttle::Accelerate;
                } else if desired_decel < brake {
                    // NB the original also uses accelerate here (the cruise
                    // branch is commented out, cAIRobot.cpp:420-421).
                    self.robot.move_state = Throttle::Accelerate;
                } else {
                    self.robot.move_state = Throttle::Brake;
                }
            }

            if self.is_transfer_possible_with(&center) {
                self.ai.transfer_center = Some(center.id);
                self.ai.targeted_center = None;
                self.ai.state = AiState::Idle;
            }
        }
    }

    // ---- AttackTargetedRobot (cAIRobot.cpp:439-475) ----

    fn attack_targeted_robot(&mut self) {
        if self.ai.state != AiState::TargetingRobot {
            return;
        }
        let target = self
            .ai
            .targeted_robot
            .and_then(|rid| self.robots.iter().find(|r| r.id == rid).copied());
        match target {
            Some(rv) if !rv.is_dead => {
                if self.ai.target_visible {
                    self.ai.high_level_path.clear();
                    self.ai.path.clear();
                    self.fire_at(rv.position);
                } else {
                    // Keep extending the path to the (unseen) target.
                    self.find_path(rv.position, true);
                }
            }
            _ => {
                // Target gone (destroyed): re-scan next tick.
                self.enter_idle_mode();
                self.ai.scan_delay = 0.0;
            }
        }
    }

    /// PARITY: `cRobot::Fire (const cRobot&)` (`cRobot.cpp:336-370`): solve
    /// the aim trajectory, set the desired heading/elevation, then fire ONLY
    /// if aligned within the no-2π-wrap gate (`fabs(dir - dirDesired) <
    /// π·0.025`). The actual muzzle spawn happens in `frame()`'s
    /// `apply_inputs` (which uses the pre-`CalcNewPosition` heading, exactly
    /// like the C++). Sets `self.fire` on success.
    fn fire_at(&mut self, target_pos: Vec2) {
        // GetProjectileFireType (last ready type).
        let ty = match self.robot.projectile_fire_type() {
            Some(t) => t,
            None => return,
        };
        let pt = &self.projectile_types[ty.get() as usize];
        let center_z = crate::sim::robot::ROBOT_CENTER_Z;
        let position = Vec3::new(self.robot.position.x, self.robot.position.y, center_z);
        let target = Vec3::new(target_pos.x, target_pos.y, center_z);
        let speed = self.robot.speed + pt.initial_speed;
        let (theta, phi) = match solve_aim(position, target, speed, pt.gravity_accel) {
            Some(v) => v,
            None => return, // GetTrajectory failed; heading unchanged.
        };
        self.robot.direction_desired = theta;
        self.robot.barrel_elevation = phi;
        if (self.robot.direction - theta).abs() < PI * 0.025 {
            self.fire = true;
        }
    }

    // ---- FindPath / FindLowLevelPath (cAIRobot.cpp:712-780) ----

    fn find_path(&mut self, goal: Vec2, extend: bool) -> bool {
        let goal_old = self.ai.target;
        self.ai.target = goal;
        if extend && self.graph.is_same_room(goal_old, goal) {
            // New goal shares the old path's room.
            return true;
        }

        if extend && !self.ai.high_level_path.is_empty() {
            let back = *self.ai.high_level_path.last().unwrap();
            let new_high = match find_high_level_path(self.graph, back, self.ai.target) {
                Some(p) if !p.is_empty() => p,
                _ => return false,
            };
            // Append skipping the shared first point.
            self.ai.high_level_path.extend_from_slice(&new_high[1..]);
            true
        } else {
            let high = match find_high_level_path(self.graph, self.robot.position, self.ai.target) {
                Some(p) if !p.is_empty() => p,
                _ => return false,
            };
            self.ai.high_level_path = high;
            self.find_low_level_path()
        }
    }

    fn find_low_level_path(&mut self) -> bool {
        if self.ai.high_level_path.is_empty() {
            return false;
        }
        // PARITY: lookahead min(2, len-1) rooms (cAIRobot.cpp:765).
        let idx = crate::path::PathFinder::LOOKAHEAD_CAP.min(self.ai.high_level_path.len() - 1);
        let goal = self.ai.high_level_path[idx];
        let size = self.size();
        let wall_dist = self.ai.options.min_wall_dist;
        let low = find_low_level_path(self.graph, self.robot.position, goal, wall_dist, size);
        let low = match low {
            Some(p) if p.len() >= 2 => p,
            _ => {
                self.ai.high_level_path.clear();
                self.ai.path.clear();
                return false;
            }
        };
        // Skip the first point (== start).
        self.ai.path = low[1..].iter().copied().collect();
        self.ai.desired = *self.ai.path.front().unwrap();
        self.face_desired_position();
        true
    }

    // ---- MoveAI reaction + ChangeDesiredDirection (cAIRobot.cpp:102-175) ----

    /// The `cAIRobot::MoveAI` logic that in the C++ ran *around*
    /// `cRobot::Move`, folded to the start of the next frame: react to the
    /// collision the frozen `frame()` already committed (bounce/idle), or —
    /// if it moved freely — advance along the path.
    fn move_ai_reaction(&mut self) {
        if self.robot.collided {
            let collision = self.reconstruct_collision();
            self.react_to_collision(collision);
        } else {
            self.change_desired_direction();
        }
    }

    /// Reconstruct the discarded `tCollision2D` from committed state: nearest
    /// overlapping live robot ⇒ robot collision, else a wall collision keyed
    /// on the dominant move axis. Deterministic; a documented deviation.
    fn reconstruct_collision(&self) -> Collision2D {
        // Nearest other robot whose (slightly expanded) footprint overlaps.
        let margin = 0.05f32;
        let my = Rect {
            center: self.robot.position,
            size: self.size() + Vec2::new(margin, margin),
        };
        let mut best: Option<(f32, RobotId)> = None;
        for rv in self.robots.iter() {
            if rv.id == self.id || rv.is_dead {
                continue;
            }
            let other = Rect {
                center: rv.position,
                size: self.size() + Vec2::new(margin, margin),
            };
            if crate::math::intersect::rect_rect(my, other) {
                let d = (self.robot.position - rv.position).length_squared();
                if best.is_none() || d < best.unwrap().0 {
                    best = Some((d, rv.id));
                }
            }
        }
        if let Some((_, rid)) = best {
            // react_to_collision re-looks-up the collider position by id.
            return Collision2D {
                ntr: crate::math::intersect::no_intersection_2d(),
                kind: CollisionKind::Robot,
                robot_id: Some(rid),
                wall_axis: crate::math::intersect::PlaneAxis::Invalid,
                vec2_direction: Vec2::ZERO,
            };
        }
        // Wall: dominant move axis => the plane whose normal it drove into.
        let axis = if self.robot.move_vec.x.abs() >= self.robot.move_vec.y.abs() {
            crate::math::intersect::PlaneAxis::X
        } else {
            crate::math::intersect::PlaneAxis::Y
        };
        Collision2D {
            ntr: crate::math::intersect::no_intersection_2d(),
            kind: CollisionKind::Wall,
            robot_id: None,
            wall_axis: axis,
            vec2_direction: Vec2::ZERO,
        }
    }

    /// PARITY: `cAIRobot::MoveAI` turning block (`cAIRobot.cpp:105-162`).
    fn react_to_collision(&mut self, collision: Collision2D) {
        let collided_target = collision.kind == CollisionKind::Robot
            && self.ai.state == AiState::TargetingRobot
            && collision.robot_id == self.ai.targeted_robot;

        let collided_wall = collision.kind == CollisionKind::Wall;
        let collided_robot = collision.kind == CollisionKind::Robot;

        if collided_wall || (collided_robot && !collided_target) {
            if collided_robot {
                // PARITY (bug): turn TOWARD the collider (cAIRobot.cpp:130).
                let collider_pos = collision
                    .robot_id
                    .and_then(|rid| self.robots.iter().find(|r| r.id == rid))
                    .map(|r| r.position)
                    .unwrap_or(self.robot.position);
                self.robot.direction_desired =
                    bounce_turn_toward_robot(self.robot.next_position(), collider_pos);
            } else {
                // Wall snap to {0, π/2, π, 3π/2} (cAIRobot.cpp:135-148).
                self.robot.direction_desired =
                    wall_bounce_heading(collision.wall_axis, self.robot.direction);
                self.robot.move_state = Throttle::Cruise;
            }
            // PARITY: EnterIdleMode overwrites direction_desired with the
            // heading toward m_pt2Desired and resets move_state to Accelerate
            // (cAIRobot.cpp:161,533,541) — the bounce assignment above is
            // thus latent, but both are ported verbatim.
            self.enter_idle_mode();
        }
    }

    /// PARITY: `cAIRobot::ChangeDesiredDirection` (`cAIRobot.cpp:545-620`).
    fn change_desired_direction(&mut self) {
        if self.ai.target_visible {
            return;
        }
        let high_empty = self.ai.high_level_path.is_empty();
        let low_empty = self.ai.path.is_empty();
        if high_empty && low_empty {
            return;
        }

        if !high_empty {
            // Did the robot enter a room a later high-level point is in?
            let mut changed_idx: Option<usize> = None;
            for i in 1..self.ai.high_level_path.len() {
                if self.graph.is_same_room(self.robot.position, self.ai.high_level_path[i]) {
                    changed_idx = Some(i);
                    break;
                }
            }
            if changed_idx.is_some() || low_empty {
                if let Some(i) = changed_idx {
                    // erase [begin, itPathPoint+1) == indices 0..=i.
                    self.ai.high_level_path.drain(0..=i);
                }
                self.find_low_level_path();
            }
        }

        // Did the robot pass its desired position last frame?
        let pos = self.robot.position;
        let old = self.ai.position_old;
        let movement = Rect {
            center: (old + pos) * 0.5,
            size: Vec2::new((old.x - pos.x).abs(), (old.y - pos.y).abs()),
        };
        if !point_rect(self.ai.desired, movement) {
            return;
        }

        if self.ai.path.is_empty() {
            return; // logged in C++
        }
        self.ai.path.pop_front();
        if let Some(&front) = self.ai.path.front() {
            self.ai.desired = front;
            self.face_desired_position();
        }
    }
}

/// PARITY (design §1.3, `cAIRobot.cpp:130`): the collision-bounce turn for a
/// robot collider is `GetAngle (nextPosition, colliderPosition)` — the angle
/// **from** the robot **to** the collider, i.e. it faces TOWARD the thing it
/// hit (a bug; "turn away" would swap the arguments). Exposed for direct
/// testing because in the full `MoveAI` path the subsequent `EnterIdleMode`
/// overwrites the result.
pub fn bounce_turn_toward_robot(next_position: Vec2, collider_position: Vec2) -> f32 {
    get_angle(next_position, collider_position)
}

/// PARITY: the wall-bounce heading snap (`cAIRobot.cpp:135-148`). A `planeX`
/// wall reverses an X-ward heading to `{0, π}`; a `planeY` wall reverses a
/// Y-ward heading to `{π/2, 3π/2}`.
pub fn wall_bounce_heading(wall_axis: crate::math::intersect::PlaneAxis, direction: f32) -> f32 {
    use crate::math::intersect::PlaneAxis;
    match wall_axis {
        PlaneAxis::X => {
            if between(direction, 0.0, PI * 0.5) || between(direction, PI * 1.5, PI * 2.0) {
                PI
            } else {
                0.0
            }
        }
        PlaneAxis::Y => {
            if between(direction, 0.0, PI) {
                PI * 1.5
            } else {
                PI * 0.5
            }
        }
        PlaneAxis::Z | PlaneAxis::Invalid => direction,
    }
}
