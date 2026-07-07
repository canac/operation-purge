//! Status-text assembly (design §5.3), replacing `cOverlay2D`'s HUD-text
//! half. Ported verbatim from `cRobot::GetInfoString` (`cRobot.cpp:595-669`)
//! + `cOperationPurge::GetInfoString` (`cOperationPurge.cpp:518-606`).
//!
//! This module builds the status string from a supplied [`HudSnapshot`]
//! (the layout / assembly half of M5); polling the live game to fill the
//! snapshot is M8. `s_Precision = 2` (`cOperationPurge.cpp:21`).
//!
//! PARITY: the four conditional lines are emitted in the source's order —
//! AI mode, then enemy visible, then targeted, then above-center
//! (`cOperationPurge.cpp:601-604`; EnemyVisible outranks Targeted, code
//! over docs, design §1.3). The enemy-visible flag itself is refreshed
//! every 0.25 s of *game* time (`cOperationPurge.cpp:586`) — that polling
//! lives in the game loop (M8); here it is just a snapshot field.

/// Enemy-visible HUD/alarm poll interval, in game-time seconds.
pub const ENEMY_VISIBLE_POLL_SECONDS: f32 = 0.25;

/// `s_Precision` (`cOperationPurge.cpp:21`): decimal places for the HUD's
/// floating-point fields.
pub const PRECISION: usize = 2;

/// Robot move state (`cRobot::eMoveState`) → the verbatim status label.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum MoveState {
    Accelerate,
    Cruise,
    Decelerate,
}

impl MoveState {
    /// `szMoveStateText` (`cRobot.cpp:630-644`).
    pub fn label(self) -> &'static str {
        match self {
            MoveState::Accelerate => "Accelerating",
            MoveState::Cruise => "Cruising",
            MoveState::Decelerate => "Decelerating",
        }
    }
}

/// Per-projectile-type reload + ammunition status
/// (`cRobot::tProjectileTypeState`, `cRobot.cpp:612-624`).
#[derive(Copy, Clone, Debug)]
pub struct ProjectileTypeStatus {
    pub fire_delay: f32,
    pub fire_rate: f32,
    pub ammo_left: u32,
    pub ammo_total: u32,
}

/// The active robot's portion of the HUD (`cRobot::GetInfoString`). Absent
/// when not observing a robot (free/observer-point camera) — the block is
/// then empty, matching the observer-point case.
#[derive(Clone, Debug)]
pub struct RobotHudSnapshot {
    pub speed: f32,
    pub max_speed: f32,
    pub desired_speed: f32,
    pub energy: f32,
    pub energy_start: f32,
    pub health: f32,
    pub health_start: f32,
    pub projectile_types: Vec<ProjectileTypeStatus>,
    pub move_state: MoveState,
    pub type_name: String,
    pub team_name: String,
}

/// Everything the status block needs (`cOperationPurge::GetInfoString`
/// inputs), decoupled from the live sim so it is unit-testable.
#[derive(Clone, Debug)]
pub struct HudSnapshot {
    pub robot: Option<RobotHudSnapshot>,
    /// Spawned (alive) robot count per team definition, in team order.
    pub spawned_per_team: Vec<u32>,
    /// Fractional game-speed multiplier (displayed as a percentage).
    pub game_speed: f32,
    pub game_time: f32,
    pub ai_mode: bool,
    pub enemy_visible: bool,
    pub targeted: bool,
    pub above_center: bool,
    pub projectiles: usize,
    pub particles: usize,
    pub effects: usize,
    pub holes: usize,
}

/// The active robot's status block (`cRobot::GetInfoString`,
/// `cRobot.cpp:654-668`). Returns `""` when no robot is being observed.
fn robot_info(robot: &Option<RobotHudSnapshot>) -> String {
    let Some(r) = robot else {
        return String::new();
    };
    let p = PRECISION;

    let reload = r
        .projectile_types
        .iter()
        .map(|s| format!("{:.*} / {:.*}", p, s.fire_delay, p, s.fire_rate))
        .collect::<Vec<_>>()
        .join(", ");
    let ammo = r
        .projectile_types
        .iter()
        .map(|s| format!("{} / {}", s.ammo_left, s.ammo_total))
        .collect::<Vec<_>>()
        .join(", ");

    format!(
        "Speed: {:.*} / {:.*}\n\
         Desired speed: {:.*}\n\
         Energy: {:.*} / {:.*}\n\
         Health: {:.*} / {:.*}\n\
         Reload time: ({})\n\
         Ammunition left: ({})\n\
         {}\n\
         Type: {}\n\
         Team: {}\n",
        p, r.speed, p, r.max_speed,
        p, r.desired_speed,
        p, r.energy, p, r.energy_start,
        p, r.health, p, r.health_start,
        reload,
        ammo,
        r.move_state.label(),
        r.type_name,
        r.team_name,
    )
}

/// Assembles the full status text block (`cOperationPurge::GetInfoString`).
pub fn status_text(snapshot: &HudSnapshot) -> String {
    let p = PRECISION;

    let spawned = snapshot
        .spawned_per_team
        .iter()
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join(", ");

    // Conditional lines, in the source's order (AI mode, enemy visible,
    // targeted, above center).
    let mut conditionals = String::new();
    if snapshot.ai_mode {
        conditionals.push_str("You are in AI mode\n");
    }
    if snapshot.enemy_visible {
        conditionals.push_str("An enemy robot is visible\n");
    }
    if snapshot.targeted {
        conditionals.push_str("You are being targeted\n");
    }
    if snapshot.above_center {
        conditionals.push_str("You are above a center\n");
    }

    format!(
        "{}Spawned robots: ({})\n\
         Game speed: {:.*}%\n\
         Game time: {:.*}\n\
         {}Projectiles: {}\n\
         Particles: {}\n\
         Effects: {}\n\
         Holes: {}",
        robot_info(&snapshot.robot),
        spawned,
        p, snapshot.game_speed * 100.0,
        p, snapshot.game_time,
        conditionals,
        snapshot.projectiles,
        snapshot.particles,
        snapshot.effects,
        snapshot.holes,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn robot() -> RobotHudSnapshot {
        RobotHudSnapshot {
            speed: 12.5,
            max_speed: 100.0,
            desired_speed: 50.0,
            energy: 750.0,
            energy_start: 1000.0,
            health: 3.0,
            health_start: 5.0,
            projectile_types: vec![
                ProjectileTypeStatus {
                    fire_delay: 0.25,
                    fire_rate: 1.0,
                    ammo_left: 40,
                    ammo_total: 100,
                },
                ProjectileTypeStatus {
                    fire_delay: 0.0,
                    fire_rate: 5.0,
                    ammo_left: 3,
                    ammo_total: 5,
                },
            ],
            move_state: MoveState::Cruise,
            type_name: "Scout".to_string(),
            team_name: "Red".to_string(),
        }
    }

    fn snapshot() -> HudSnapshot {
        HudSnapshot {
            robot: Some(robot()),
            spawned_per_team: vec![3, 2],
            game_speed: 1.0,
            game_time: 42.0,
            ai_mode: false,
            enemy_visible: false,
            targeted: false,
            above_center: false,
            projectiles: 7,
            particles: 128,
            effects: 4,
            holes: 9,
        }
    }

    #[test]
    fn full_snapshot_assembles_verbatim() {
        let s = status_text(&snapshot());
        let expected = "Speed: 12.50 / 100.00\n\
             Desired speed: 50.00\n\
             Energy: 750.00 / 1000.00\n\
             Health: 3.00 / 5.00\n\
             Reload time: (0.25 / 1.00, 0.00 / 5.00)\n\
             Ammunition left: (40 / 100, 3 / 5)\n\
             Cruising\n\
             Type: Scout\n\
             Team: Red\n\
             Spawned robots: (3, 2)\n\
             Game speed: 100.00%\n\
             Game time: 42.00\n\
             Projectiles: 7\n\
             Particles: 128\n\
             Effects: 4\n\
             Holes: 9";
        assert_eq!(s, expected);
    }

    #[test]
    fn conditional_lines_appear_in_source_order() {
        let mut snap = snapshot();
        snap.ai_mode = true;
        snap.enemy_visible = true;
        snap.targeted = true;
        snap.above_center = true;
        let s = status_text(&snap);
        let ai = s.find("You are in AI mode").unwrap();
        let visible = s.find("An enemy robot is visible").unwrap();
        let targeted = s.find("You are being targeted").unwrap();
        let above = s.find("You are above a center").unwrap();
        // Order: AI mode < enemy visible < targeted < above center.
        assert!(ai < visible && visible < targeted && targeted < above);
        // They sit between the Game time line and the Projectiles line.
        let game_time = s.find("Game time").unwrap();
        let projectiles = s.find("Projectiles:").unwrap();
        assert!(game_time < ai && above < projectiles);
    }

    #[test]
    fn observer_point_has_no_robot_block() {
        let mut snap = snapshot();
        snap.robot = None;
        let s = status_text(&snap);
        assert!(!s.contains("Speed:"));
        assert!(s.starts_with("Spawned robots: (3, 2)"));
    }

    #[test]
    fn game_speed_is_shown_as_a_percentage() {
        let mut snap = snapshot();
        snap.robot = None;
        snap.game_speed = 0.25;
        let s = status_text(&snap);
        assert!(s.contains("Game speed: 25.00%"));
    }
}
