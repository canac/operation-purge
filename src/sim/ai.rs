//! `cAIRobot`: the 3-state FSM (idle / targeting-center / targeting-robot),
//! scan/resupply/hunt, brake-to-stop, wall/robot-bounce quirks, ported from
//! `cAIRobot.cpp`.
//!
//! Every spawned robot owns an [`AiRuntime`] (`Robot::ai`). `World::frame`
//! drives it exactly where `MoveObjects` does (`cOperationPurge.cpp:609-672`):
//! [`AiCtx::calc_new_position_ai`] in phase 1 and [`AiCtx::move_ai`] in
//! phase 3, for every robot except the player-possessed one.
//!
//! PARITY (preserved verbatim):
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

use crate::data::{AiRobotType, GameData};
use crate::ids::{CenterId, RobotId};
use crate::math::intersect::{point_rect, MovingRect, PlaneAxis, Rect};
use crate::math::{between, equals, f_max, f_min, get_angle};
use crate::path::astar::AStar;
use crate::path::{find_high_level_path, find_low_level_path, RoomGraph, LOOKAHEAD_CAP};
use crate::rng::GameRng;
use crate::sim::center::{Center, CenterKind};
use crate::sim::projectile::solve_aim;
use crate::sim::robot::{fire_gate_aligned, Collision2D, CollisionKind, FireRequest, Robot, Throttle};
use glam::{Vec2, Vec3};
use std::collections::{BTreeMap, VecDeque};
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

/// `eCenterType` — which resource a center transfers.
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
/// `m_shOptions`).
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

impl From<&AiRobotType> for AiOptions {
    fn from(t: &AiRobotType) -> Self {
        Self {
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
    }
}

/// Per-robot AI state — the non-`cRobot` members of `cAIRobot`
/// (`cAIRobot.h:52-66`).
pub struct AiRuntime {
    pub options: AiOptions,
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
}

impl AiRuntime {
    /// `cAIRobot::cAIRobot` (`cAIRobot.cpp:15-41`): `Reset ()`, then
    /// randomize the freshly spawned `robot`'s heading.
    ///
    /// PARITY: two `fRand ()` draws in this order — the scan delay inside
    /// `Reset`, then the heading (`cAIRobot.cpp:39,60`).
    pub fn new(robot: &mut Robot, options: AiOptions, rng: &mut GameRng) -> Self {
        let mut ai = Self {
            options,
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
        };
        ai.reset(robot, rng);
        robot.direction_desired = rng.frand() * PI * 2.0;
        robot.direction = robot.direction_desired;
        ai
    }

    /// `cAIRobot::Reset` (`cAIRobot.cpp:56-61`): re-enter idle and draw a
    /// fresh scan delay. Call whenever computer control is handed back to
    /// the robot.
    pub fn reset(&mut self, robot: &mut Robot, rng: &mut GameRng) {
        self.enter_idle_mode(robot);
        self.scan_delay = rng.frand() * self.options.scan_delay;
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
        robot.direction_desired = get_angle(robot.position, self.desired);
    }
}

/// The borrows one robot's AI step needs, with methods mirroring
/// `cAIRobot`'s members. `robots` holds every robot except `robot`.
pub(crate) struct AiCtx<'a> {
    pub robot: &'a mut Robot,
    pub ai: &'a mut AiRuntime,
    pub graph: RoomGraph<'a>,
    pub astar: &'a mut AStar,
    pub game_data: &'a GameData,
    pub robots: &'a BTreeMap<RobotId, Robot>,
    pub centers: &'a [Center],
    pub rng: &'a mut GameRng,
}

impl AiCtx<'_> {
    fn center(&self, id: CenterId) -> Option<&Center> {
        self.centers.iter().find(|c| c.id == id)
    }

    // ---- resource percentages (cRobot.cpp:739-776) ----

    fn min_ammo_percentage(&self) -> f32 {
        let mut min = 1.0f32;
        for s in &self.robot.proj_states {
            let pct = if s.start_count == 0.0 { 1.0 } else { s.projectiles_left / s.start_count };
            min = f_min(pct, min);
        }
        min
    }

    fn max_ammo_percentage(&self) -> f32 {
        let mut max = 0.0f32;
        for s in &self.robot.proj_states {
            let pct = if s.start_count == 0.0 { 0.0 } else { s.projectiles_left / s.start_count };
            max = f_max(pct, max);
        }
        max
    }

    fn energy_percentage(&self) -> f32 {
        self.robot.energy / self.robot.energy_start
    }

    fn health_percentage(&self) -> f32 {
        self.robot.health / self.robot.health_start
    }

    /// PARITY: `IsRobotVisible`/`IsCenterVisible` (`cOperationPurge.cpp:1691-1728`):
    /// within `fMaxVisibleDist` and the swept footprint clears the building.
    fn is_visible(&self, point: Vec2) -> bool {
        let mvd = self.game_data.robot_types[self.robot.robot_type.get() as usize].max_visible_dist;
        if (self.robot.position - point).length_squared() > mvd * mvd {
            return false;
        }
        let mrc = MovingRect {
            center1: self.robot.position,
            center2: point,
            size: self.robot.size(),
            stop_time: 1.0,
        };
        !self.graph.building.intersect_moving_rect(mrc).hit
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

    // ---- facing (cAIRobot.cpp:698-707) ----

    fn face_point(&mut self, point: Vec2) {
        self.robot.direction_desired = get_angle(self.robot.position, point);
    }

    fn face_desired_position(&mut self) {
        let desired = self.ai.desired;
        self.face_point(desired);
    }

    fn enter_idle_mode(&mut self) {
        self.ai.enter_idle_mode(self.robot);
    }

    /// PARITY: `cAIRobot::CalcNewPositionAI` (`cAIRobot.cpp:64-98`). Returns
    /// the shot fired at the targeted robot, if any; the caller spawns it.
    pub fn calc_new_position_ai(&mut self, dt: f32) -> Option<FireRequest> {
        if self.ai.scan_delay <= 0.0 {
            self.choose_targeted_center();
            self.choose_targeted_robot();
            self.ai.scan_delay = self.ai.options.scan_delay;
        }
        self.move_to_targeted_center();
        let fire = self.attack_targeted_robot();
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

        self.robot.desired_speed = if self.ai.transfer_center.is_none() {
            self.ai.options.desired_speed
        } else {
            0.0
        };
        self.robot.calc_new_position(dt);
        fire
    }

    // ---- ChooseTargetedCenter (cAIRobot.cpp:185-246) ----

    fn choose_targeted_center(&mut self) {
        if self.ai.state == AiState::TargetingCenter {
            if let Some(pos) = self.ai.targeted_center.and_then(|id| self.center(id)).map(|c| c.position) {
                // Already targeting a valid center; just refresh visibility.
                self.ai.target_visible = self.is_visible(pos);
                return;
            }
        }

        // GetReadyCenters (cOperationPurge.cpp:1637-1654).
        let mut choice_dist = 0.0f32;
        let mut chosen: Option<(CenterId, CenterKindTag, Vec2)> = None;
        for c in self.centers {
            if !c.is_transfer_ready(self.robot.id) {
                continue;
            }
            let ty = CenterKindTag::of(&c.kind);
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

        let Some((cid, ty, pos)) = chosen else {
            return;
        };
        if self.find_path(pos, false) {
            self.ai.state = AiState::TargetingCenter;
            self.ai.targeted_center_type = Some(ty);
            self.ai.targeted_center = Some(cid);
        }
    }

    // ---- ChooseTargetedRobot (cAIRobot.cpp:250-326) ----

    fn choose_targeted_robot(&mut self) {
        if self.ai.state == AiState::TargetingCenter
            && self.ai.targeted_center_type.is_some_and(|ty| self.is_transfer_critical(ty))
        {
            // On the way to a critically-needed center; cannot target.
            return;
        }

        if self.ai.state == AiState::TargetingRobot {
            let target = self.ai.targeted_robot.and_then(|id| self.robots.get(&id));
            if let Some(pos) = target.filter(|r| !r.is_dead()).map(|r| r.position) {
                self.ai.target_visible = self.is_visible(pos);
                if self.ai.target_visible {
                    return;
                }
            }
        }

        // GetVisibleRobots vs GetEnemyRobots (cOperationPurge.cpp:1594-1634).
        let restrict_visible = self.ai.state == AiState::TargetingRobot;
        let team = self.robot.team;
        let mut choice_dist = 0.0f32;
        let mut chosen: Option<(RobotId, Vec2)> = None;
        for r in self.robots.values() {
            if r.is_dead() || r.team == team {
                continue;
            }
            if restrict_visible && !self.is_visible(r.position) {
                continue;
            }
            let dist = (self.robot.position - r.position).length_squared();
            if chosen.is_none() || dist < choice_dist {
                chosen = Some((r.id, r.position));
                choice_dist = dist;
            }
        }

        let Some((rid, pos)) = chosen else {
            return;
        };
        if self.find_path(pos, false) {
            self.ai.target_visible = restrict_visible || self.is_visible(pos);
            self.ai.state = AiState::TargetingRobot;
            self.ai.targeted_robot = Some(rid);
        }
    }

    // ---- MoveToTargetedCenter (cAIRobot.cpp:330-436) ----

    fn move_to_targeted_center(&mut self) {
        if let Some(cid) = self.ai.transfer_center {
            let Some(center) = self.center(cid) else {
                return; // logged in C++
            };
            if !center.is_transfer_ready(self.robot.id) {
                // The center is in use by another robot.
                self.enter_idle_mode();
                return;
            }
            let done = match CenterKindTag::of(&center.kind) {
                CenterKindTag::Ammo => equals(self.min_ammo_percentage(), 1.0),
                CenterKindTag::Energy => equals(self.energy_percentage(), 1.0),
                CenterKindTag::Health => equals(self.health_percentage(), 1.0),
            };
            if done {
                self.enter_idle_mode();
            }
        } else if self.ai.state == AiState::TargetingCenter {
            let Some(center) = self.ai.targeted_center.and_then(|id| self.center(id)) else {
                return; // logged in C++
            };
            let (center_id, center_pos) = (center.id, center.position);

            if self.ai.target_visible {
                self.ai.target = center_pos;
                self.face_point(center_pos);
                self.ai.high_level_path.clear();
                self.ai.path.clear();

                // PARITY: brake law v²/(2d) vs friction/brake decel (cAIRobot.cpp:410-426).
                let dist = (self.robot.position - center_pos).length();
                let desired_decel = self.robot.speed * self.robot.speed / (2.0 * dist);
                if desired_decel < self.robot.friction_decel {
                    self.robot.move_state = Throttle::Accelerate;
                } else if desired_decel < self.robot.brake_decel {
                    // The original's `moveCruise` here is commented out.
                    self.robot.move_state = Throttle::Accelerate;
                } else if desired_decel >= self.robot.brake_decel {
                    self.robot.move_state = Throttle::Brake;
                }
            }

            let center = self.center(center_id).expect("looked up above");
            if center.is_transfer_possible(self.robot) {
                self.ai.transfer_center = Some(center_id);
                self.ai.targeted_center = None;
                self.ai.state = AiState::Idle;
            }
        }
    }

    // ---- AttackTargetedRobot (cAIRobot.cpp:439-475) ----

    fn attack_targeted_robot(&mut self) -> Option<FireRequest> {
        if self.ai.state != AiState::TargetingRobot {
            return None;
        }
        let target = self.ai.targeted_robot.and_then(|id| self.robots.get(&id));
        match target.filter(|r| !r.is_dead()).map(|r| r.position) {
            Some(pos) if self.ai.target_visible => {
                self.ai.high_level_path.clear();
                self.ai.path.clear();
                self.fire_at(pos)
            }
            Some(pos) => {
                // Keep extending the path to the (unseen) target.
                self.find_path(pos, true);
                None
            }
            None => {
                // Target gone (destroyed): re-scan next tick.
                self.enter_idle_mode();
                self.ai.scan_delay = 0.0;
                None
            }
        }
    }

    /// PARITY: `cRobot::Fire (const cRobot&)` (`cRobot.cpp:336-370`): solve
    /// the aim trajectory, set the desired heading/elevation, then fire only
    /// if [`fire_gate_aligned`].
    fn fire_at(&mut self, target_pos: Vec2) -> Option<FireRequest> {
        let ty = self.robot.projectile_fire_type()?;
        let pt = &self.game_data.projectile_types[ty.get() as usize];
        let center_z = crate::sim::robot::ROBOT_CENTER_Z;
        let position = Vec3::new(self.robot.position.x, self.robot.position.y, center_z);
        let target = Vec3::new(target_pos.x, target_pos.y, center_z);
        let speed = self.robot.speed + pt.initial_speed;
        let (theta, phi) = solve_aim(position, target, speed, pt.gravity_accel)?;
        self.robot.direction_desired = theta;
        self.robot.barrel_elevation = phi;
        if fire_gate_aligned(self.robot.direction, theta) {
            self.robot.fire(ty, self.rng)
        } else {
            None
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
            let new_high = match find_high_level_path(&self.graph, self.astar, back, goal) {
                Some(p) if !p.is_empty() => p,
                _ => return false,
            };
            // Append skipping the shared first point.
            self.ai.high_level_path.extend_from_slice(&new_high[1..]);
            true
        } else {
            let high = match find_high_level_path(&self.graph, self.astar, self.robot.position, goal) {
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
        let goal = self.ai.high_level_path[LOOKAHEAD_CAP.min(self.ai.high_level_path.len() - 1)];
        let low = find_low_level_path(
            &self.graph,
            self.astar,
            self.robot.position,
            goal,
            self.ai.options.min_wall_dist,
            self.robot.size(),
        );
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
        self.ai.desired = low[1];
        self.face_desired_position();
        true
    }

    /// PARITY: `cAIRobot::MoveAI` (`cAIRobot.cpp:102-175`): bounce off
    /// whatever `collision` hit, commit the move, and advance along the path
    /// if the robot moved freely.
    pub fn move_ai(&mut self, collision: Collision2D) {
        let collided_wall = collision.kind == CollisionKind::Wall;
        let collided_robot = collision.kind == CollisionKind::Robot;
        let collided_target = collided_robot
            && self.ai.state == AiState::TargetingRobot
            && collision.robot_id == self.ai.targeted_robot;

        if collided_wall || (collided_robot && !collided_target) {
            if collided_robot {
                self.robot.direction_desired =
                    bounce_turn_toward_robot(self.robot.next_position(), collision.position);
            } else {
                if !matches!(collision.wall_axis, PlaneAxis::X | PlaneAxis::Y) {
                    // PARITY: logged and bailed before `cRobot::Move` (`cAIRobot.cpp:150-155`).
                    return;
                }
                self.robot.direction_desired =
                    wall_bounce_heading(collision.wall_axis, self.robot.direction);
                self.robot.move_state = Throttle::Cruise;
            }
            self.enter_idle_mode();
        }

        let position_old = self.robot.position;
        self.robot.commit_move(collision);

        if !collided_wall && !collided_robot {
            self.change_desired_direction(position_old);
        }
    }

    /// PARITY: `cAIRobot::ChangeDesiredDirection` (`cAIRobot.cpp:545-620`).
    fn change_desired_direction(&mut self, position_old: Vec2) {
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
            let pos = self.robot.position;
            let changed_idx = (1..self.ai.high_level_path.len())
                .find(|&i| self.graph.is_same_room(pos, self.ai.high_level_path[i]));
            if changed_idx.is_some() || low_empty {
                if let Some(i) = changed_idx {
                    self.ai.high_level_path.drain(0..=i);
                }
                self.find_low_level_path();
            }
        }

        // Did the robot pass its desired position this frame?
        let pos = self.robot.position;
        let movement = Rect {
            center: (position_old + pos) * 0.5,
            size: (position_old - pos).abs(),
        };
        if !point_rect(self.ai.desired, movement) {
            return;
        }

        if self.ai.path.pop_front().is_none() {
            return; // logged in C++
        }
        if let Some(&front) = self.ai.path.front() {
            self.ai.desired = front;
            self.face_desired_position();
        }
    }
}

/// PARITY (`cAIRobot.cpp:130`): the collision-bounce turn for a robot
/// collider is `GetAngle (nextPosition, colliderPosition)` — the angle
/// **from** the robot **to** the collider, i.e. it faces TOWARD the thing it
/// hit (a bug; "turn away" would swap the arguments). Exposed for direct
/// testing because in the full `MoveAI` path the subsequent `EnterIdleMode`
/// overwrites the result.
pub fn bounce_turn_toward_robot(next_position: Vec2, collider_position: Vec2) -> f32 {
    get_angle(next_position, collider_position)
}

/// PARITY: the wall-bounce heading snap (`cAIRobot.cpp:135-148`). A `planeX`
/// wall reverses an X-ward heading to `{0, π}`; a `planeY` wall reverses a
/// Y-ward heading to `{π/2, 3π/2}`. Other planes leave `direction` as is.
pub fn wall_bounce_heading(wall_axis: PlaneAxis, direction: f32) -> f32 {
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
