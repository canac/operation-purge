//! World-scale building geometry + wall collision queries, ported from
//! `cBuilding` (source-map §3.4). `cBuilding : public cMaze` adds room
//! scale (`fRoomSize`, `fWallWidth`, `fCeilingHeight` from
//! `[building]`) and all wall-collision math on top of the bit grid.
//!
//! GUARD (design §1.3): out-of-grid wall queries return an explicit
//! "no wall" rather than the original's unchecked array read — this
//! matches what the non-crashing C++ reads effectively did, so open-edge
//! mazes stay legal and their emergent behavior is preserved.
//!
//! Full geometry/collision bodies land at M1 (world scale) and M6 (sweep
//! tests used by `sim::robot`/`sim::projectile`).

use super::Maze;
use glam::Vec3;

/// A maze plus the world-scale constants needed to turn room coordinates
/// into world-space geometry and to run wall collision queries.
pub struct Building {
    pub maze: Maze,
    pub wall_width: f32,
    pub room_size: f32,
    pub ceiling_height: f32,
}

impl Building {
    /// Room `(x,y)` occupies `[x*room_size,(x+1)*room_size] x
    /// [y*room_size,(y+1)*room_size]` (libs-map §3, "Maze -> world"), from
    /// the floor (z=0) up to the ceiling (z=ceiling_height).
    pub fn room_bounds(&self, x: i32, y: i32) -> (Vec3, Vec3) {
        let min = Vec3::new(x as f32 * self.room_size, y as f32 * self.room_size, 0.0);
        let max = Vec3::new(
            (x + 1) as f32 * self.room_size,
            (y + 1) as f32 * self.room_size,
            self.ceiling_height,
        );
        (min, max)
    }

    /// Static rectangle-vs-wall collision test.
    pub fn test_static_rect(&self) {
        todo!("ported at M6 — static rect collision (source-map §3.4)")
    }

    /// Recursive-quartered swept rectangle test, depth <= 3.
    pub fn test_swept_rect(&self) {
        todo!("ported at M6 — swept rect collision, depth<=3 (source-map §3.4)")
    }

    /// Parabolic trajectory vs. wall boxes + floor/ceiling planes.
    pub fn test_trajectory(&self) {
        todo!("ported at M6 — trajectory vs building (source-map §3.4)")
    }

    /// Wall-outline polygon tracing, consumed by `render::building3d`.
    pub fn trace_outline(&self) {
        todo!("ported at M4 — wall-outline tracing (source-map §3.3)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::maze::Maze;

    fn tiny_building() -> Building {
        Building {
            maze: Maze::new(2, 2, vec![false; 6], vec![false; 6]),
            wall_width: 1.0,
            room_size: 10.0,
            ceiling_height: 5.0,
        }
    }

    #[test]
    fn room_bounds_uses_shipped_scale_constants() {
        let building = tiny_building();
        let (min, max) = building.room_bounds(1, 0);
        assert_eq!(min, Vec3::new(10.0, 0.0, 0.0));
        assert_eq!(max, Vec3::new(20.0, 10.0, 5.0));
    }

    #[test]
    fn room_bounds_room_zero_starts_at_origin() {
        let building = tiny_building();
        let (min, _max) = building.room_bounds(0, 0);
        assert_eq!(min, Vec3::new(0.0, 0.0, 0.0));
    }
}
