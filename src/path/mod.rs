//! Room-level + corner-waypoint pathfinding strategies (source-map §3.7),
//! ported from `cPathFinder.cpp`'s `cHighLevelPathFinder` /
//! `cLowLevelPathFinder`. These supply the room-graph-specific
//! `GetNeighbors`/`IsGoalPoint` that the M2 [`astar::AStar`] core
//! (`cPathFinder::FindPath`) is generic over.
//!
//! `cObstaclePathFinder` is DEAD code and is intentionally NOT ported
//! (design §2 mapping table; it was only ever reached from
//! `cAIRobot::PreventCollision`, itself commented out).
//!
//! PARITY:
//! - Inadmissible weighted A* (`h = 3.0*sqr-dist`, `g = sqr-dist`) lives in
//!   [`astar`] (`cPathFinder.cpp:404-413`) — verified, unchanged.
//! - `cHighLevelPathFinder::FindPath` computes centered start/goal points
//!   (`cPathFinder.cpp:439-442`) but then searches with the ORIGINAL
//!   uncentered points and never appends the goal (`//rPath.push_back` is
//!   commented, `cPathFinder.cpp:449`); the low level DOES append the goal
//!   (`cPathFinder.cpp:516`).
//! - Low-level lookahead cap `min(2, len-1)` rooms is applied by the caller
//!   (`cAIRobot::FindLowLevelPath`, `cAIRobot.cpp:765`); see
//!   [`PathFinder::LOOKAHEAD_CAP`].
//!
//! The building-collision helpers (`IntersectRect`/`IntersectMovingRect`,
//! `cBuilding.cpp`) are re-ported here as [`RoomGraph`] methods: `sim::world`
//! keeps its own private copies (M6) and this milestone may only touch
//! `path`/`ai`/`tests`, so the small amount of duplication is deliberate and
//! each copy carries the same `cBuilding` citation.

pub mod astar;
pub mod nodes;

use crate::maze::Maze;
use crate::math::floor_i32;
use crate::math::intersect::{
    moving_rect_rect, no_intersection_2d, rect_rect, Intersection2D, MovingRect, Rect,
};
use astar::AStar;
use glam::Vec2;

/// A resolved path: a sequence of waypoints from start to goal.
pub struct Path {
    pub waypoints: Vec<Vec2>,
}

/// Namespace holder mirroring `cPathFinder` — the lookahead cap constant
/// plus the free-function entry points below.
pub struct PathFinder;

impl PathFinder {
    /// Lookahead cap for the low-level corner-waypoint strategy —
    /// PARITY, `cAIRobot.cpp:765` (`std::min<int> (2, size - 1)`).
    pub const LOOKAHEAD_CAP: usize = 2;
}

/// Which of the two wall directions a wall runs along (`eWallDirection`,
/// `cMaze.h`): `X` = horizontal (top edge of a room), `Y` = vertical (left
/// edge). Matches `sim::world`'s private `WallDir`.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum WallDir {
    X,
    Y,
}

/// The building's room graph + world scale, exposing exactly the queries
/// `cHighLevel`/`cLowLevel`/`cPathFinder` needed: room membership, wall
/// adjacency, and rect/moving-rect-vs-building intersection.
///
/// PARITY note: `room_size`/`wall_width` are the *building* constants
/// (`cBuilding::m_fRoomSize`/`m_fWallWidth`); the AI's per-robot
/// `fMinWallDist` (`wall_dist`) and footprint (`size`) are passed per call,
/// exactly as `cPathFinder::m_fWallDist`/`m_size2Size` were set per
/// `FindPath` (`cPathFinder.cpp:166-167,182-183`).
pub struct RoomGraph<'a> {
    pub maze: &'a Maze,
    pub room_size: f32,
    pub wall_width: f32,
}

impl<'a> RoomGraph<'a> {
    pub fn new(maze: &'a Maze, room_size: f32, wall_width: f32) -> Self {
        Self {
            maze,
            room_size,
            wall_width,
        }
    }

    /// `cBuilding::IsSameRoom` (`cBuilding.cpp:89-93`): `(int) (x/rs)`
    /// truncation-toward-zero per axis (C `(int)` cast == Rust `as i32`).
    pub fn is_same_room(&self, a: Vec2, b: Vec2) -> bool {
        (a.x / self.room_size) as i32 == (b.x / self.room_size) as i32
            && (a.y / self.room_size) as i32 == (b.y / self.room_size) as i32
    }

    /// `GetWall(x, y, dir)` via the maze accessor (`cMaze::GetSurroundingWalls`
    /// semantics): `WallDir::X` = top edge = `surrounding_walls[0]`;
    /// `WallDir::Y` = left edge = `surrounding_walls[2]`.
    fn get_wall(&self, x: i32, y: i32, dir: WallDir) -> bool {
        let sw = self.maze.surrounding_walls(x, y);
        match dir {
            WallDir::X => sw[0],
            WallDir::Y => sw[2],
        }
    }

    /// `cBuilding::GetSurroundingWalls`→`cMaze::GetSurroundingWalls`
    /// (`cMaze.cpp:307-313`), returned in `eDirection` order
    /// `[Up, Left, Down, Right]` (NOT the maze accessor's
    /// `[Up, Down, Left, Right]`).
    fn surrounding_walls_dir(&self, p: Vec2) -> [bool; 4] {
        let x = (p.x / self.room_size) as i32;
        let y = (p.y / self.room_size) as i32;
        [
            self.get_wall(x, y, WallDir::X),     // Up      = h(x,y)
            self.get_wall(x, y, WallDir::Y),     // Left    = v(x,y)
            self.get_wall(x, y + 1, WallDir::X), // Down    = h(x,y+1)
            self.get_wall(x + 1, y, WallDir::Y), // Right   = v(x+1,y)
        ]
    }

    /// `cMaze::GetWallCorners` (`cMaze.cpp:316-322`), returned in `eCorner`
    /// order `[TopLeft, TopRight, BottomLeft, BottomRight]`.
    fn wall_corners(&self, p: Vec2) -> [bool; 4] {
        let x = (p.x / self.room_size) as i32;
        let y = (p.y / self.room_size) as i32;
        [
            // TL
            self.get_wall(x - 1, y, WallDir::X) || self.get_wall(x, y - 1, WallDir::Y),
            // TR
            self.get_wall(x + 1, y, WallDir::X) || self.get_wall(x + 1, y - 1, WallDir::Y),
            // BL
            self.get_wall(x - 1, y + 1, WallDir::X) || self.get_wall(x, y + 1, WallDir::Y),
            // BR
            self.get_wall(x + 1, y + 1, WallDir::X) || self.get_wall(x + 1, y + 1, WallDir::Y),
        ]
    }

    /// `cBuilding::WallToRect` (`cBuilding.cpp:96-119`).
    fn wall_to_rect(&self, dir: WallDir, x: i32, y: i32) -> Rect {
        let rs = self.room_size;
        let ww = self.wall_width;
        match dir {
            WallDir::X => Rect {
                center: Vec2::new((x as f32 + 0.5) * rs, y as f32 * rs),
                size: Vec2::new(rs + ww, ww),
            },
            WallDir::Y => Rect {
                center: Vec2::new(x as f32 * rs, (y as f32 + 0.5) * rs),
                size: Vec2::new(ww, rs + ww),
            },
        }
    }

    /// PARITY: `cBuilding::IntersectRect` (`cBuilding.cpp:164-208`) —
    /// static rect-vs-wall test. Preserves the quirk of using `size.y` for
    /// BOTH axis bounds (`cBuilding.cpp:170-173`; harmless for the square
    /// robot footprints these paths use).
    pub fn intersect_rect(&self, rc: Rect) -> bool {
        let recip = 1.0 / self.room_size;
        let size_y = (rc.size.y + self.wall_width) * 0.5;
        let iwx1 = floor_i32((rc.center.x - size_y) * recip);
        let iwy1 = floor_i32((rc.center.y - size_y) * recip);
        let iwx2 = floor_i32((rc.center.x + size_y) * recip);
        let iwy2 = floor_i32((rc.center.y + size_y) * recip);

        for y in (iwy1 + 1).max(0)..=iwy2 {
            for x in iwx1.max(0)..=iwx2 {
                if self.get_wall(x, y, WallDir::X) && rect_rect(rc, self.wall_to_rect(WallDir::X, x, y))
                {
                    return true;
                }
            }
        }
        for y in iwy1.max(0)..=iwy2 {
            for x in (iwx1 + 1).max(0)..=iwx2 {
                if self.get_wall(x, y, WallDir::Y) && rect_rect(rc, self.wall_to_rect(WallDir::Y, x, y))
                {
                    return true;
                }
            }
        }
        false
    }

    /// PARITY: `cBuilding::IntersectMovingRect` → `GetIntersectedWalls`
    /// (`cBuilding.cpp:211-216`) with `pntr` set / `pvWalls` null
    /// (`bFindAll = true`): earliest-time wall intersection.
    pub fn intersect_moving_rect(&self, mrc: MovingRect) -> Intersection2D {
        let mut ntr = no_intersection_2d();
        self.get_intersected_walls_portion(mrc, &mut ntr, 0);
        ntr
    }

    /// PARITY: `cBuilding::GetIntersectedWallsPortion`
    /// (`cBuilding.cpp:467-602`) with `bFindAll = true`, ntr-only (the
    /// pathfinder never asks for the wall list): recurse-quarter until the
    /// swept span fits one room row/column or depth 3, then test each
    /// candidate wall and keep the earliest-time hit.
    fn get_intersected_walls_portion(
        &self,
        mrc: MovingRect,
        ntr: &mut Intersection2D,
        level: u32,
    ) {
        let offset_x = (mrc.size.x + self.wall_width) * 0.5;
        let offset_y = (mrc.size.y + self.wall_width) * 0.5;
        let recip = 1.0 / self.room_size;
        let fwx1 = (mrc.center1.x.min(mrc.center2.x) - offset_x) * recip;
        let fwy1 = (mrc.center1.y.min(mrc.center2.y) - offset_y) * recip;
        let fwx2 = (mrc.center1.x.max(mrc.center2.x) + offset_x) * recip;
        let fwy2 = (mrc.center1.y.max(mrc.center2.y) + offset_y) * recip;
        let iwx1 = floor_i32(fwx1);
        let iwy1 = floor_i32(fwy1);
        let iwx2 = floor_i32(fwx2);
        let iwy2 = floor_i32(fwy2);

        if iwx1 == iwx2 || iwy1 == iwy2 || level > 3 {
            let cx = self.maze.cx;
            let cy = self.maze.cy;
            if iwx1 >= cx || iwx2 < 0 || iwy1 >= cy || iwy2 < 0 {
                return;
            }

            for y in (iwy1 + 1)..=iwy2 {
                for x in iwx1..=iwx2 {
                    if self.get_wall(x, y, WallDir::X) {
                        let n = moving_rect_rect(mrc, self.wall_to_rect(WallDir::X, x, y));
                        if n.hit && n.time < ntr.time {
                            *ntr = n;
                        }
                    }
                }
            }
            for y in iwy1..=iwy2 {
                for x in (iwx1 + 1)..=iwx2 {
                    if self.get_wall(x, y, WallDir::Y) {
                        let n = moving_rect_rect(mrc, self.wall_to_rect(WallDir::Y, x, y));
                        if n.hit && n.time < ntr.time {
                            *ntr = n;
                        }
                    }
                }
            }
        } else {
            let movement = mrc.center2 - mrc.center1;
            for (a, b) in [(0.0, 0.25), (0.25, 0.5), (0.5, 0.75), (0.75, 1.0)] {
                let sub = MovingRect {
                    center1: mrc.center1 + movement * a,
                    center2: mrc.center1 + movement * b,
                    size: mrc.size,
                    stop_time: mrc.stop_time,
                };
                self.get_intersected_walls_portion(sub, ntr, level + 1);
            }
        }
    }

    /// PARITY: `cPathFinder::IsPointVisible` (`cPathFinder.cpp:373-401`).
    /// Same-room fast path = neither endpoint's footprint touches a wall;
    /// cross-room = the swept footprint clears the building.
    pub fn is_point_visible(&self, size: Vec2, p1: Vec2, p2: Vec2) -> bool {
        if self.is_same_room(p1, p2) {
            if self.intersect_rect(Rect {
                center: p1,
                size,
            }) {
                return false;
            }
            if self.intersect_rect(Rect {
                center: p2,
                size,
            }) {
                return false;
            }
            return true;
        }
        let mrc = MovingRect {
            center1: p1,
            center2: p2,
            size,
            stop_time: 1.0,
        };
        !self.intersect_moving_rect(mrc).hit
    }

    // ---------------- high-level (room graph) ----------------

    /// PARITY: `cHighLevelPathFinder::GetNeighbors` (`cPathFinder.cpp:454-480`).
    /// Offsets in `eDirection` order; a neighbor across a wall is dropped.
    fn high_level_neighbors(&self, p: Vec2) -> Vec<Vec2> {
        let rs = self.room_size;
        let offsets = [
            Vec2::new(0.0, -rs), // Up
            Vec2::new(-rs, 0.0), // Left
            Vec2::new(0.0, rs),  // Down
            Vec2::new(rs, 0.0),  // Right
        ];
        let walls = self.surrounding_walls_dir(p);
        let mut out = Vec::with_capacity(4);
        for dir in 0..4 {
            let neighbor = p + offsets[dir];
            let crossed_wall = !self.is_same_room(p, neighbor);
            if crossed_wall && walls[dir] {
                continue;
            }
            out.push(neighbor);
        }
        out
    }

    // ---------------- low-level (corner waypoints) ----------------

    /// PARITY: `cLowLevelPathFinder::GetNeighbors` (`cPathFinder.cpp:521-595`).
    /// Emits the four room-corner waypoints (`wall_dist` inset) of every
    /// unwalled neighbor room plus the current room, gated by the margin/
    /// wall-corner visibility test.
    fn low_level_neighbors(&self, p: Vec2, wall_dist: f32, size: Vec2) -> Vec<Vec2> {
        let rs = self.room_size;
        let wd = wall_dist;
        // eCorner order: TL, TR, BL, BR.
        let room_offsets = [
            Vec2::new(wd, wd),
            Vec2::new(rs - wd, wd),
            Vec2::new(wd, rs - wd),
            Vec2::new(rs - wd, rs - wd),
        ];
        // Up, Left, Down, Right, Self.
        let neighbor_offsets = [
            Vec2::new(0.0, -rs),
            Vec2::new(-rs, 0.0),
            Vec2::new(0.0, rs),
            Vec2::new(rs, 0.0),
            Vec2::new(0.0, 0.0),
        ];

        let mut surrounding = [false; 5];
        let sw = self.surrounding_walls_dir(p);
        surrounding[..4].copy_from_slice(&sw);
        surrounding[4] = false;

        let mut corner = [false; 5];
        let cw = self.wall_corners(p);
        corner[..4].copy_from_slice(&cw);
        corner[4] = false;

        let offset = Vec2::new(rem(p.x, rs), rem(p.y, rs));
        let margin_l = offset.x < room_offsets[0].x; // TL.x = wd
        let margin_r = offset.x > room_offsets[1].x; // TR.x = rs-wd
        let margin_u = offset.y < room_offsets[0].y; // TL.y = wd
        let margin_d = offset.y > room_offsets[2].y; // BL.y = rs-wd
        let check_visible = [
            (margin_l && corner[0]) || (margin_r && corner[1]), // Up
            (margin_u && corner[0]) || (margin_d && corner[2]), // Left
            (margin_l && corner[2]) || (margin_r && corner[3]), // Down
            (margin_u && corner[1]) || (margin_d && corner[3]), // Right
            false,
        ];

        let mut out = Vec::new();
        for i in 0..5 {
            if surrounding[i] {
                continue;
            }
            let want_check = check_visible[i];
            let neighbor = p + neighbor_offsets[i];
            let neighbor_room =
                Vec2::new(neighbor.x - rem(neighbor.x, rs), neighbor.y - rem(neighbor.y, rs));
            for ro in &room_offsets {
                let new_neighbor = neighbor_room + *ro;
                if !want_check || self.is_point_visible(size, p, new_neighbor) {
                    out.push(new_neighbor);
                }
            }
        }
        out
    }
}

/// `fmodf` for the non-negative coordinates the pathfinder deals with
/// (Rust `%` on `f32` is truncated remainder == C `fmodf`).
#[inline]
fn rem(a: f32, b: f32) -> f32 {
    a % b
}

/// PARITY: `cHighLevelPathFinder::FindPath` (`cPathFinder.cpp:428-451`).
/// A* over the room graph from `start`, goal = any point in `goal`'s room.
/// Returns the node sequence (start..goal-room inclusive); the goal point is
/// deliberately NOT appended (`//rPath.push_back` commented). `None` if no
/// path exists.
pub fn find_high_level_path(graph: &RoomGraph, start: Vec2, goal: Vec2) -> Option<Vec<Vec2>> {
    let mut astar = AStar::new();
    astar.search(
        start,
        goal,
        |p| graph.is_same_room(p, goal),
        |p| graph.high_level_neighbors(p),
    )
}

/// PARITY: `cLowLevelPathFinder::FindPath` (`cPathFinder.cpp:501-518`).
/// A* over corner waypoints; goal reached when `goal` is visible; the goal
/// point IS appended (`cPathFinder.cpp:516`). `None` if no path exists.
pub fn find_low_level_path(
    graph: &RoomGraph,
    start: Vec2,
    goal: Vec2,
    wall_dist: f32,
    size: Vec2,
) -> Option<Vec<Vec2>> {
    let mut astar = AStar::new();
    let mut path = astar.search(
        start,
        goal,
        |p| graph.is_point_visible(size, p, goal),
        |p| graph.low_level_neighbors(p, wall_dist, size),
    )?;
    path.push(goal);
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fully-connected `cx x cy` maze with only the outer border walled
    /// (a "perfect open field"): every interior room is reachable from every
    /// other, so the high-level A* explores freely.
    fn open_bordered(cx: i32, cy: i32) -> Maze {
        let mut h = vec![false; (cx * (cy + 1)) as usize];
        let mut v = vec![false; ((cx + 1) * cy) as usize];
        for x in 0..cx {
            h[x as usize] = true; // top border
            h[(cy * cx + x) as usize] = true; // bottom border
        }
        for y in 0..cy {
            v[(y * (cx + 1)) as usize] = true; // left border
            v[(y * (cx + 1) + cx) as usize] = true; // right border
        }
        Maze::new(cx, cy, h, v)
    }

    fn room_center(x: i32, y: i32, rs: f32) -> Vec2 {
        Vec2::new((x as f32 + 0.5) * rs, (y as f32 + 0.5) * rs)
    }

    fn rooms(graph: &RoomGraph, path: &[Vec2]) -> Vec<(i32, i32)> {
        path.iter()
            .map(|p| {
                (
                    (p.x / graph.room_size) as i32,
                    (p.y / graph.room_size) as i32,
                )
            })
            .collect()
    }

    /// Hand trace on a 4x4 open field, room size 10. Start room (0,0), goal
    /// room (3,0). Neighbors are enumerated in eDirection order
    /// Up,Left,Down,Right; the inadmissible `h=3*sqr` heuristic makes the
    /// greedy search drive straight along +X (Right) with no detours, so the
    /// room sequence is (0,0)->(1,0)->(2,0)->(3,0).
    #[test]
    fn high_level_room_sequence_matches_hand_trace_4x4() {
        let maze = open_bordered(4, 4);
        let graph = RoomGraph::new(&maze, 10.0, 1.0);
        let start = room_center(0, 0, 10.0);
        let goal = room_center(3, 0, 10.0);
        let path = find_high_level_path(&graph, start, goal).expect("path");
        assert_eq!(rooms(&graph, &path), vec![(0, 0), (1, 0), (2, 0), (3, 0)]);
    }

    /// High-level nodes are `start + k*roomSize` in each axis (offsets are
    /// integer multiples of the room size), i.e. the search preserves the
    /// start's in-room fractional position (`cHighLevelPathFinder` searches
    /// the uncentered start).
    #[test]
    fn high_level_preserves_in_room_offset() {
        let maze = open_bordered(4, 4);
        let graph = RoomGraph::new(&maze, 10.0, 1.0);
        // Start off-center within room (0,0).
        let start = Vec2::new(2.0, 3.0);
        let goal = room_center(2, 2, 10.0);
        let path = find_high_level_path(&graph, start, goal).expect("path");
        assert_eq!(path[0], start);
        for p in &path {
            // Same fractional offset as the start, everywhere.
            assert!((p.x - 2.0).rem_euclid(10.0).abs() < 1e-4 || ((p.x - 2.0) % 10.0).abs() < 1e-4);
        }
        // Reaches the goal room.
        let last = *path.last().unwrap();
        assert!(graph.is_same_room(last, goal));
    }

    /// A wall between room (0,0) and (1,0) forces the high-level path to
    /// detour (down then across then up), never crossing the closed edge.
    #[test]
    // erasing_op: index is written as the literal Y*(cx+1)+X formula with Y=0
    // to make the wall-indexing scheme obvious; the leading `0 *` is intentional.
    #[allow(clippy::erasing_op)]
    fn high_level_routes_around_a_wall() {
        let mut maze = open_bordered(3, 3);
        // Close the vertical wall on the left edge of room (1,0) = v(1,0).
        maze.v_walls[0 * (3 + 1) + 1] = true;
        let graph = RoomGraph::new(&maze, 10.0, 1.0);
        let start = room_center(0, 0, 10.0);
        let goal = room_center(1, 0, 10.0);
        let path = find_high_level_path(&graph, start, goal).expect("path");
        let seq = rooms(&graph, &path);
        assert_eq!(seq.first(), Some(&(0, 0)));
        assert_eq!(seq.last(), Some(&(1, 0)));
        // The direct step (0,0)->(1,0) is blocked, so the path is longer.
        assert!(seq.len() > 2, "expected a detour, got {seq:?}");
    }

    #[test]
    fn low_level_same_room_start_reaches_visible_goal() {
        let maze = open_bordered(2, 2);
        let graph = RoomGraph::new(&maze, 10.0, 1.0);
        let start = room_center(0, 0, 10.0);
        let goal = room_center(0, 0, 10.0);
        let size = Vec2::new(1.6, 1.6);
        let path = find_low_level_path(&graph, start, goal, 3.0, size).expect("path");
        // start visible to goal (same point) => [start, goal].
        assert_eq!(path.first().copied(), Some(start));
        assert_eq!(path.last().copied(), Some(goal));
    }

    #[test]
    // erasing_op: literal Y*(cx+1)+X formula with Y=0 (see note above).
    #[allow(clippy::erasing_op)]
    fn is_point_visible_blocked_by_wall_between_rooms() {
        let mut maze = open_bordered(2, 2);
        // Wall on the left edge of room (1,0).
        maze.v_walls[0 * (2 + 1) + 1] = true;
        let graph = RoomGraph::new(&maze, 10.0, 1.0);
        let size = Vec2::new(1.6, 1.6);
        let a = room_center(0, 0, 10.0);
        let b = room_center(1, 0, 10.0);
        assert!(!graph.is_point_visible(size, a, b), "wall should block LOS");
    }

    #[test]
    fn is_point_visible_clear_across_open_rooms() {
        let maze = open_bordered(2, 2);
        let graph = RoomGraph::new(&maze, 10.0, 1.0);
        let size = Vec2::new(1.6, 1.6);
        let a = room_center(0, 0, 10.0);
        let b = room_center(1, 0, 10.0);
        assert!(graph.is_point_visible(size, a, b), "no wall => visible");
    }
}
