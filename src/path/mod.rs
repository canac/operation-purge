//! Room-level + corner-waypoint pathfinding strategies,
//! ported from `cPathFinder.cpp`'s `cHighLevelPathFinder` /
//! `cLowLevelPathFinder`. These supply the room-graph-specific
//! `GetNeighbors`/`IsGoalPoint` that the [`astar::AStar`] core
//! (`cPathFinder::FindPath`) is generic over.
//!
//! `cObstaclePathFinder` is DEAD code and is intentionally NOT ported
//! (it was only ever reached from
//! `cAIRobot::PreventCollision`, itself commented out).
//!
//! PARITY:
//! - `cHighLevelPathFinder::FindPath` never appends the goal
//!   (`//rPath.push_back` is commented, `cPathFinder.cpp:449`); the low level
//!   DOES append the goal (`cPathFinder.cpp:516`).
//! - Low-level lookahead cap `min(2, len-1)` rooms is applied by the caller
//!   (`cAIRobot::FindLowLevelPath`, `cAIRobot.cpp:765`); see
//!   [`LOOKAHEAD_CAP`].
//!
//! The building-collision helpers (`IntersectRect`/`IntersectMovingRect`,
//! `cBuilding.cpp`) live once on [`crate::maze::building::Building`];
//! [`RoomGraph`] borrows a `&Building` and delegates to them (the room graph
//! only adds the pathfinding-specific room/corner adjacency on top).

pub mod astar;
pub mod nodes;

use crate::maze::building::{Building, WallDir};
use crate::math::intersect::{MovingRect, Rect};
use astar::AStar;
use glam::Vec2;

/// How many high-level rooms ahead the low-level path aims — PARITY,
/// `cAIRobot.cpp:765` (`std::min<int> (2, size - 1)`).
pub const LOOKAHEAD_CAP: usize = 2;

/// The building's room graph: borrows a [`Building`] (for its maze, world
/// scale, and wall-collision queries) and adds the room/corner adjacency the
/// `cHighLevel`/`cLowLevel` pathfinders needed.
///
/// PARITY note: the AI's per-robot `fMinWallDist` (`wall_dist`) and footprint
/// (`size`) are passed per call, exactly as `cPathFinder::m_fWallDist` /
/// `m_size2Size` were set per `FindPath` (`cPathFinder.cpp:166-167,182-183`).
pub struct RoomGraph<'a> {
    pub building: &'a Building,
}

impl<'a> RoomGraph<'a> {
    pub fn new(building: &'a Building) -> Self {
        Self { building }
    }

    /// `cBuilding::IsSameRoom` (`cBuilding.cpp:89-93`): `(int) (x/rs)`
    /// truncation-toward-zero per axis (C `(int)` cast == Rust `as i32`).
    pub fn is_same_room(&self, a: Vec2, b: Vec2) -> bool {
        let rs = self.building.room_size;
        (a.x / rs) as i32 == (b.x / rs) as i32 && (a.y / rs) as i32 == (b.y / rs) as i32
    }

    /// The center of the room containing `p`.
    pub fn room_center(&self, p: Vec2) -> Vec2 {
        let rs = self.building.room_size;
        Vec2::new(p.x - rem(p.x, rs) + rs * 0.5, p.y - rem(p.y, rs) + rs * 0.5)
    }

    /// `cBuilding::GetSurroundingWalls`→`cMaze::GetSurroundingWalls`
    /// (`cMaze.cpp:307-313`), returned in `eDirection` order
    /// `[Up, Left, Down, Right]` (NOT the maze accessor's
    /// `[Up, Down, Left, Right]`).
    fn surrounding_walls_dir(&self, p: Vec2) -> [bool; 4] {
        let rs = self.building.room_size;
        self.building.surrounding_walls_dir((p.x / rs) as i32, (p.y / rs) as i32)
    }

    /// `cMaze::GetWallCorners` (`cMaze.cpp:316-322`), returned in `eCorner`
    /// order `[TopLeft, TopRight, BottomLeft, BottomRight]`.
    fn wall_corners(&self, p: Vec2) -> [bool; 4] {
        let rs = self.building.room_size;
        let x = (p.x / rs) as i32;
        let y = (p.y / rs) as i32;
        [
            // TL
            self.building.wall_at(WallDir::X, x - 1, y) || self.building.wall_at(WallDir::Y, x, y - 1),
            // TR
            self.building.wall_at(WallDir::X, x + 1, y) || self.building.wall_at(WallDir::Y, x + 1, y - 1),
            // BL
            self.building.wall_at(WallDir::X, x - 1, y + 1) || self.building.wall_at(WallDir::Y, x, y + 1),
            // BR
            self.building.wall_at(WallDir::X, x + 1, y + 1) || self.building.wall_at(WallDir::Y, x + 1, y + 1),
        ]
    }

    /// PARITY: `cPathFinder::IsPointVisible` (`cPathFinder.cpp:373-401`).
    /// Same-room fast path = neither endpoint's footprint touches a wall;
    /// cross-room = the swept footprint clears the building.
    pub fn is_point_visible(&self, size: Vec2, p1: Vec2, p2: Vec2) -> bool {
        if self.is_same_room(p1, p2) {
            if self.building.intersect_rect(Rect { center: p1, size }) {
                return false;
            }
            if self.building.intersect_rect(Rect { center: p2, size }) {
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
        !self.building.intersect_moving_rect(mrc).hit
    }

    // ---------------- high-level (room graph) ----------------

    /// PARITY: `cHighLevelPathFinder::GetNeighbors` (`cPathFinder.cpp:454-480`).
    /// Offsets in `eDirection` order; a neighbor across a wall is dropped.
    fn high_level_neighbors(&self, p: Vec2) -> Vec<Vec2> {
        let rs = self.building.room_size;
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
        let rs = self.building.room_size;
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

/// `cHighLevelPathFinder::FindPath` (`cPathFinder.cpp:428-451`).
/// A* over the room graph from `start`'s room center, goal = any point in
/// `goal`'s room. Returns room centers (start room..goal room inclusive); the
/// goal point is NOT appended (`//rPath.push_back` commented). `None` if no
/// path exists.
///
/// FIX: the C++ computed the centered start/goal (`cPathFinder.cpp:439-442`)
/// but searched the raw points, so waypoints kept the robot's in-room offset
/// and a wall-hugging robot got low-level goals inside walls two rooms ahead.
pub fn find_high_level_path(
    graph: &RoomGraph,
    astar: &mut AStar,
    start: Vec2,
    goal: Vec2,
) -> Option<Vec<Vec2>> {
    let start = graph.room_center(start);
    let goal = graph.room_center(goal);
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
    astar: &mut AStar,
    start: Vec2,
    goal: Vec2,
    wall_dist: f32,
    size: Vec2,
) -> Option<Vec<Vec2>> {
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
    use crate::maze::Maze;

    /// Wrap a test maze in a `Building` with the standard test scale
    /// (`room_size = 10`, `wall_width = 1`) so a `RoomGraph` can borrow it.
    fn test_building(maze: Maze) -> Building {
        Building {
            maze,
            wall_width: 1.0,
            room_size: 10.0,
            ceiling_height: 5.0,
        }
    }

    /// Index of the wall on the left edge of room `(x, y)` in `v_walls`.
    fn v_wall_index(x: usize, y: usize, cx: usize) -> usize {
        y * (cx + 1) + x
    }

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
                    (p.x / graph.building.room_size) as i32,
                    (p.y / graph.building.room_size) as i32,
                )
            })
            .collect()
    }

    #[test]
    fn high_level_room_sequence_matches_hand_trace_4x4() {
        let maze = open_bordered(4, 4);
        let building = test_building(maze);
        let graph = RoomGraph::new(&building);
        let start = room_center(0, 0, 10.0);
        let goal = room_center(3, 0, 10.0);
        let path = find_high_level_path(&graph, &mut AStar::new(), start, goal).expect("path");
        assert_eq!(rooms(&graph, &path), vec![(0, 0), (1, 0), (2, 0), (3, 0)]);
    }

    #[test]
    fn high_level_path_runs_through_room_centers() {
        let maze = open_bordered(4, 4);
        let building = test_building(maze);
        let graph = RoomGraph::new(&building);
        let start = Vec2::new(2.0, 3.0);
        let goal = Vec2::new(27.0, 21.0);
        let path = find_high_level_path(&graph, &mut AStar::new(), start, goal).expect("path");
        assert_eq!(path[0], room_center(0, 0, 10.0));
        for p in &path {
            assert_eq!(*p, room_center((p.x / 10.0) as i32, (p.y / 10.0) as i32, 10.0));
        }
        assert!(graph.is_same_room(*path.last().unwrap(), goal));
    }

    fn bfs_room_steps(maze: &Maze, from: (i32, i32), to: (i32, i32)) -> Option<usize> {
        let (cx, cy) = (maze.cx, maze.cy);
        let h = |x: i32, y: i32| maze.h_walls[(y * cx + x) as usize];
        let v = |x: i32, y: i32| maze.v_walls[(y * (cx + 1) + x) as usize];
        let mut dist = vec![usize::MAX; (cx * cy) as usize];
        let mut queue = std::collections::VecDeque::from([from]);
        dist[(from.1 * cx + from.0) as usize] = 0;
        while let Some((x, y)) = queue.pop_front() {
            let d = dist[(y * cx + x) as usize];
            if (x, y) == to {
                return Some(d);
            }
            let moves = [
                (x, y - 1, !h(x, y)),
                (x, y + 1, !h(x, y + 1)),
                (x - 1, y, !v(x, y)),
                (x + 1, y, !v(x + 1, y)),
            ];
            for (nx, ny, open) in moves {
                if open && (0..cx).contains(&nx) && (0..cy).contains(&ny) && dist[(ny * cx + nx) as usize] == usize::MAX {
                    dist[(ny * cx + nx) as usize] = d + 1;
                    queue.push_back((nx, ny));
                }
            }
        }
        None
    }

    proptest::proptest! {
        #[test]
        fn high_level_path_takes_fewest_rooms(
            walls in proptest::collection::vec(proptest::bool::weighted(0.35), 2 * 8 * 9),
            from in (0..8i32, 0..8i32),
            to in (0..8i32, 0..8i32),
        ) {
            let mut maze = open_bordered(8, 8);
            for (i, &w) in walls.iter().enumerate() {
                let (h, idx) = if i < 72 { (true, i) } else { (false, i - 72) };
                let walls = if h { &mut maze.h_walls } else { &mut maze.v_walls };
                walls[idx] |= w;
            }
            let expected = bfs_room_steps(&maze, from, to);
            let building = test_building(maze);
            let graph = RoomGraph::new(&building);
            let path = find_high_level_path(&graph, &mut AStar::new(), room_center(from.0, from.1, 10.0), room_center(to.0, to.1, 10.0));
            proptest::prop_assert_eq!(path.map(|p| p.len() - 1), expected);
        }
    }

    #[test]
    fn wall_hugging_start_reaches_lookahead_room() {
        let mut maze = open_bordered(3, 2);
        // h(2,1), under room (2,0) only: a start hugging room (0,0)'s open
        // bottom edge would sit inside it if carried over uncentered.
        maze.h_walls[5] = true;
        let building = test_building(maze);
        let graph = RoomGraph::new(&building);
        let size = Vec2::new(1.6, 1.6);
        let start = Vec2::new(5.0, 9.0);
        let high = find_high_level_path(&graph, &mut AStar::new(), start, room_center(2, 0, 10.0)).expect("high");
        let goal = high[LOOKAHEAD_CAP.min(high.len() - 1)];
        assert!(find_low_level_path(&graph, &mut AStar::new(), start, goal, 3.0, size).is_some());
    }

    /// A wall between room (0,0) and (1,0) forces the high-level path to
    /// detour (down then across then up), never crossing the closed edge.
    #[test]
    fn high_level_routes_around_a_wall() {
        let mut maze = open_bordered(3, 3);
        // Close the vertical wall on the left edge of room (1,0) = v(1,0).
        maze.v_walls[v_wall_index(1, 0, 3)] = true;
        let building = test_building(maze);
        let graph = RoomGraph::new(&building);
        let start = room_center(0, 0, 10.0);
        let goal = room_center(1, 0, 10.0);
        let path = find_high_level_path(&graph, &mut AStar::new(), start, goal).expect("path");
        let seq = rooms(&graph, &path);
        assert_eq!(seq.first(), Some(&(0, 0)));
        assert_eq!(seq.last(), Some(&(1, 0)));
        // The direct step (0,0)->(1,0) is blocked, so the path is longer.
        assert!(seq.len() > 2, "expected a detour, got {seq:?}");
    }

    #[test]
    fn low_level_same_room_start_reaches_visible_goal() {
        let maze = open_bordered(2, 2);
        let building = test_building(maze);
        let graph = RoomGraph::new(&building);
        let start = room_center(0, 0, 10.0);
        let goal = room_center(0, 0, 10.0);
        let size = Vec2::new(1.6, 1.6);
        let path = find_low_level_path(&graph, &mut AStar::new(), start, goal, 3.0, size).expect("path");
        // start visible to goal (same point) => [start, goal].
        assert_eq!(path.first().copied(), Some(start));
        assert_eq!(path.last().copied(), Some(goal));
    }

    #[test]
    fn is_point_visible_blocked_by_wall_between_rooms() {
        let mut maze = open_bordered(2, 2);
        // Wall on the left edge of room (1,0).
        maze.v_walls[v_wall_index(1, 0, 2)] = true;
        let building = test_building(maze);
        let graph = RoomGraph::new(&building);
        let size = Vec2::new(1.6, 1.6);
        let a = room_center(0, 0, 10.0);
        let b = room_center(1, 0, 10.0);
        assert!(!graph.is_point_visible(size, a, b), "wall should block LOS");
    }

    #[test]
    fn is_point_visible_clear_across_open_rooms() {
        let maze = open_bordered(2, 2);
        let building = test_building(maze);
        let graph = RoomGraph::new(&building);
        let size = Vec2::new(1.6, 1.6);
        let a = room_center(0, 0, 10.0);
        let b = room_center(1, 0, 10.0);
        assert!(graph.is_point_visible(size, a, b), "no wall => visible");
    }
}
