//! `cCenter`/`cAmmoCenter`/`cEnergyCenter`/`cHealthCenter` collapsed into
//! one enum, ported from
//! `cCenter.cpp`.
//!
//! PARITY: a robot must be **completely stationary** with its bounding
//! rect fully inside the center square; one robot docked at a time;
//! transfer is rate-limited (`cCenter.cpp:78-104`).
//! Ammo transfer for a projectile type with no listed rate is `0.0` ⇒
//! `dt / 0.0 = +inf` ammo — defined IEEE division, preserved with **no
//! guard code**. The stateful transfer *selection* (which
//! robot docks, last-transfer bookkeeping) lives in `sim::world` because
//! it needs the robot map; this module holds the per-center data + the
//! two pure predicates.

use crate::ids::{CenterId, ProjectileTypeId, RobotId};
use crate::math::intersect::{Rect, point_rect};
use crate::math::Rgba;
use crate::sim::robot::Robot;
use glam::Vec2;
use std::f32::consts::PI;

/// `eOrientation` (`cCenter.h:15-21`) — verbatim variant order
/// (Right, Up, Down, Left); the ordinal is not otherwise observable but
/// the order is kept for faithfulness.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Orientation {
    Right,
    Up,
    Down,
    Left,
}

impl Orientation {
    /// `cCenter::cCenter` orientation → angle (`cCenter.cpp:22-44`).
    pub fn angle(self) -> f32 {
        match self {
            Orientation::Right => PI, // D3DX_PI * 1.0
            Orientation::Up => PI * 0.5,
            Orientation::Left => 0.0, // D3DX_PI * 0.0
            Orientation::Down => PI * 1.5,
        }
    }
}

/// The three center variants (`cAmmoCenter`/`cEnergyCenter`/`cHealthCenter`).
#[derive(Clone, Debug)]
pub enum CenterKind {
    Ammo {
        /// `(projectile_type, rate)` pairs; a type with no entry
        /// transfers at an implicit rate of `0.0` (PARITY, see above).
        rates: Vec<(ProjectileTypeId, f32)>,
    },
    Energy {
        rate: f32,
    },
    Health {
        rate: f32,
    },
}

pub struct Center {
    pub id: CenterId,
    pub kind: CenterKind,
    pub position: Vec2,
    pub size: f32,
    pub color: Rgba,
    pub orientation: Orientation,
    pub orientation_angle: f32,

    // --- transfer bookkeeping (ex-member state, `cCenter.h:53-58` +
    //     `cAmmoCenter` `cCenter.h:77-79`) ---
    /// `m_bTransferring`.
    pub transferring: bool,
    /// `m_idTransfer` — the docked robot, or `None`.
    pub transfer_robot: Option<RobotId>,
    /// `cAmmoCenter::m_idLastTransfer`.
    pub last_transfer_robot: Option<RobotId>,
    /// `cAmmoCenter::m_iLastTransferType`.
    pub last_transfer_type: Option<ProjectileTypeId>,
    /// `cAmmoCenter::m_fTimeLeft`.
    pub time_left: f32,
}

impl Center {
    pub fn new(
        id: CenterId,
        kind: CenterKind,
        position: Vec2,
        size: f32,
        color: Rgba,
        orientation: Orientation,
    ) -> Self {
        Self {
            id,
            kind,
            position,
            size,
            color,
            orientation,
            orientation_angle: orientation.angle(),
            transferring: false,
            transfer_robot: None,
            last_transfer_robot: None,
            last_transfer_type: None,
            time_left: 0.0,
        }
    }

    /// PARITY: `cCenter::IsWithinBoundries` (`cCenter.cpp:78-91`) — the
    /// robot's bounding rect must be *completely* inside the center square
    /// shrunk by the robot's size.
    pub fn within_boundaries(&self, robot: &Robot) -> bool {
        let robot_rect = robot.bounding_rect();
        let small_center = Rect {
            center: self.position,
            size: Vec2::new(self.size, self.size) - robot_rect.size,
        };
        point_rect(robot_rect.center, small_center)
    }

    /// `cCenter::IsTransferReady` (`cCenter.cpp:94-97`): free, or already
    /// transferring with `robot`.
    pub fn is_transfer_ready(&self, robot: RobotId) -> bool {
        !self.transferring || self.transfer_robot == Some(robot)
    }

    /// PARITY: `cCenter::IsTransferPossible` (`cCenter.cpp:99-104`) — the
    /// robot must be stationary and within the boundaries.
    pub fn is_transfer_possible(&self, robot: &Robot) -> bool {
        robot.is_stationary() && self.within_boundaries(robot)
    }
}
