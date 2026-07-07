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
//! The wall-outline tracer (`wall_polygons`, ported from
//! `cBuilding::UpdateWallOutline`/`CalculateWallVertices`/
//! `CalculateWallPolygons`/`GetNextDirection`/`GetNextVertex`,
//! `cBuilding.cpp:276-960`) turns the maze bit grid into closed wall
//! polygons, consumed by `render::building3d` (M4). The **first** polygon
//! it emits is the outer boundary (it contains the globally smallest wall
//! vertex) — `render::building3d` gives that polygon reversed winding, a
//! PARITY behavior from `cBuilding3D.cpp:187-193`.

use std::collections::BTreeSet;

use super::Maze;
use crate::math::intersect::{
    moving_rect_rect, no_intersection_2d, no_intersection_3d, rect_rect, trajectory_box,
    trajectory_plane, Box3, Intersection2D, Intersection3D, MovingRect, Plane3D, PlaneAxis, Rect,
    Trajectory,
};
use crate::math::{accelerate_point, floor_i32};
use glam::{Vec2, Vec3};

/// A maze plus the world-scale constants needed to turn room coordinates
/// into world-space geometry and to run wall collision queries.
pub struct Building {
    pub maze: Maze,
    pub wall_width: f32,
    pub room_size: f32,
    pub ceiling_height: f32,
}

/// `eCorner` (`cMaze.h:29`): the four corners of a grid point. The integer
/// order `TopLeft < TopRight < BottomLeft < BottomRight` is load-bearing —
/// it is the `tWallVertex` set-ordering tiebreaker (`operator<`,
/// `cBuilding.cpp`) AND the row/column index into the `GetNextVertex` /
/// `GetWallDirection` tables. Do not reorder.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Corner {
    /// The verbatim enum ordinal (`cornerTopLeft = 0 .. cornerBottomRight = 3`).
    pub fn ord(self) -> u8 {
        match self {
            Corner::TopLeft => 0,
            Corner::TopRight => 1,
            Corner::BottomLeft => 2,
            Corner::BottomRight => 3,
        }
    }
}

/// `eDirection` (`cMaze.h:18`): tracer travel direction. Ordinal order
/// `Up=0, Left=1, Down=2, Right=3` is load-bearing (used by
/// `RotateDirection` and as the first-valid-direction scan order).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Direction {
    Up,
    Left,
    Down,
    Right,
}

impl Direction {
    const ALL: [Direction; 4] = [
        Direction::Up,
        Direction::Left,
        Direction::Down,
        Direction::Right,
    ];

    fn ord(self) -> usize {
        match self {
            Direction::Up => 0,
            Direction::Left => 1,
            Direction::Down => 2,
            Direction::Right => 3,
        }
    }

    fn from_ord(o: usize) -> Direction {
        Direction::ALL[o % 4]
    }
}

/// `RotateDirection` (`cMaze.cpp`): `(d1 + d2) % 4` over the ordinal space.
fn rotate_direction(d1: Direction, d2: Direction) -> Direction {
    Direction::from_ord((d1.ord() + d2.ord()) % 4)
}

/// The direction a wall segment runs (`eWallDirection`, `cMaze.h:9`).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum WallDir {
    /// X-direction (horizontal) wall — normal points +Y.
    X,
    /// Y-direction (vertical) wall — normal points +X.
    Y,
    Invalid,
}

/// `cBuilding::tWallVertex` (`cBuilding.h`): a grid point plus which corner
/// of it this vertex sits on. `position_x/y` are grid (room) coordinates
/// `0..=cx` / `0..=cy` (kept `i32` so the original's `DWORD` underflow at
/// `X-1`/`Y-1` maps to a harmless out-of-grid "no wall").
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct WallVertex {
    pub position_x: i32,
    pub position_y: i32,
    pub corner: Corner,
}

impl WallVertex {
    fn key(&self) -> (i32, i32, u8) {
        // operator< : PositionY, then PositionX, then Corner (cBuilding.cpp).
        (self.position_y, self.position_x, self.corner.ord())
    }
}

impl PartialOrd for WallVertex {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for WallVertex {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.key().cmp(&other.key())
    }
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

    /// World-space size of the whole building (`cBuilding::GetSize`,
    /// `cBuilding.cpp:52`): `(cx*room, cy*room, ceiling)`.
    pub fn size(&self) -> Vec3 {
        Vec3::new(
            self.maze.cx as f32 * self.room_size,
            self.maze.cy as f32 * self.room_size,
            self.ceiling_height,
        )
    }

    /// `cBuilding::WallVertexToPoint` (`cBuilding.cpp:283`): grid vertex →
    /// world XY, inset by half the wall width toward the corner.
    pub fn wall_vertex_to_point(&self, v: WallVertex) -> (f32, f32) {
        let mut x = v.position_x as f32 * self.room_size;
        let mut y = v.position_y as f32 * self.room_size;
        let h = self.wall_width * 0.5;
        match v.corner {
            Corner::TopLeft => {
                x -= h;
                y -= h;
            }
            Corner::TopRight => {
                x += h;
                y -= h;
            }
            Corner::BottomRight => {
                x += h;
                y += h;
            }
            Corner::BottomLeft => {
                x -= h;
                y += h;
            }
        }
        (x, y)
    }

    /// `cBuilding::GetWallDirection` (`cBuilding.cpp:308`): the direction of
    /// the wall between two adjacent polygon vertices, plus (for the
    /// same-position/different-corner case) the corner-pair lookup table.
    pub fn wall_direction(v1: WallVertex, v2: WallVertex) -> WallDir {
        let x_equal = v1.position_x == v2.position_x;
        let y_equal = v1.position_y == v2.position_y;
        if !x_equal && y_equal {
            WallDir::X
        } else if x_equal && !y_equal {
            WallDir::Y
        } else if x_equal && y_equal {
            // aaDirections[Vertex1.Corner][Vertex2.Corner].
            const T: [[WallDir; 4]; 4] = [
                // cornerTopLeft
                [WallDir::Invalid, WallDir::X, WallDir::Y, WallDir::Invalid],
                // cornerTopRight
                [WallDir::X, WallDir::Invalid, WallDir::Invalid, WallDir::Y],
                // cornerBottomLeft
                [WallDir::Y, WallDir::Invalid, WallDir::Invalid, WallDir::X],
                // cornerBottomRight
                [WallDir::Invalid, WallDir::Y, WallDir::X, WallDir::Invalid],
            ];
            T[v1.corner.ord() as usize][v2.corner.ord() as usize]
        } else {
            WallDir::Invalid
        }
    }

    // --- wall-outline tracer (cBuilding.cpp:276-960) -------------------

    /// Direction-indexed surrounding walls of room `(x, y)` (`cMaze::
    /// GetSurroundingWalls`, `cMaze.cpp:307`): `[Up, Left, Down, Right]`.
    fn maze_surrounding_walls(&self, x: i32, y: i32) -> [bool; 4] {
        // abWalls[Up]=GetWall(X,Y,wallX); [Left]=GetWall(X,Y,wallY);
        // [Down]=GetWall(X,Y+1,wallX); [Right]=GetWall(X+1,Y,wallY).
        [
            self.maze.wall_x(x, y),
            self.maze.wall_y(x, y),
            self.maze.wall_x(x, y + 1),
            self.maze.wall_y(x + 1, y),
        ]
    }

    /// `cBuilding::GetSurroundingWalls(tWallVertex)` (`cBuilding.cpp:418`):
    /// walls around the vertex's grid point, with the two far-side walls
    /// zeroed per corner. Returns `[Up, Left, Down, Right]`.
    fn vertex_surrounding_walls(&self, v: WallVertex) -> [bool; 4] {
        let center_off_x = if matches!(v.corner, Corner::TopLeft | Corner::BottomLeft) {
            -1
        } else {
            0
        };
        let center_off_y = if matches!(v.corner, Corner::TopLeft | Corner::TopRight) {
            -1
        } else {
            0
        };
        let px = v.position_x + center_off_x;
        let py = v.position_y + center_off_y;
        let mut w = self.maze_surrounding_walls(px, py);
        // The far-side walls do not matter for the corner.
        match v.corner {
            Corner::TopLeft => {
                w[Direction::Up.ord()] = false;
                w[Direction::Left.ord()] = false;
            }
            Corner::TopRight => {
                w[Direction::Up.ord()] = false;
                w[Direction::Right.ord()] = false;
            }
            Corner::BottomLeft => {
                w[Direction::Down.ord()] = false;
                w[Direction::Left.ord()] = false;
            }
            Corner::BottomRight => {
                w[Direction::Down.ord()] = false;
                w[Direction::Right.ord()] = false;
            }
        }
        w
    }

    /// `cBuilding::CalculateWallVertices` (`cBuilding.cpp:777`): every grid
    /// point contributes 0-4 corner vertices depending on which of its four
    /// incident walls exist.
    fn calculate_wall_vertices(&self) -> Vec<WallVertex> {
        let cx = self.maze.cx;
        let cy = self.maze.cy;
        let mut out = Vec::new();
        for y in 0..=cy {
            for x in 0..=cx {
                let wall_r = self.maze.wall_x(x, y);
                let wall_l = self.maze.wall_x(x - 1, y);
                let wall_u = self.maze.wall_y(x, y - 1);
                let wall_d = self.maze.wall_y(x, y);

                if !wall_r && !wall_l && !wall_u && !wall_d {
                    continue;
                }
                if wall_u == wall_l && x != 0 && y != 0 {
                    out.push(WallVertex {
                        position_x: x,
                        position_y: y,
                        corner: Corner::TopLeft,
                    });
                }
                if wall_u == wall_r && x != cx && y != 0 {
                    out.push(WallVertex {
                        position_x: x,
                        position_y: y,
                        corner: Corner::TopRight,
                    });
                }
                if wall_d == wall_r && x != cx && y != cy {
                    out.push(WallVertex {
                        position_x: x,
                        position_y: y,
                        corner: Corner::BottomRight,
                    });
                }
                if wall_d == wall_l && x != 0 && y != cy {
                    out.push(WallVertex {
                        position_x: x,
                        position_y: y,
                        corner: Corner::BottomLeft,
                    });
                }
            }
        }
        out
    }

    /// `cBuilding::GetNextDirection` (`cBuilding.cpp:607`): pick the tracer's
    /// next travel direction from `vertex`, given its last direction.
    fn get_next_direction(&self, vertex: WallVertex, last: Option<Direction>) -> Option<Direction> {
        let surrounding = self.vertex_surrounding_walls(vertex);
        let mut valid = [false; 4];
        for d in Direction::ALL {
            valid[d.ord()] = !surrounding[d.ord()];
        }

        let x = vertex.position_x;
        let y = vertex.position_y;
        let wall_r = self.maze.wall_x(x, y);
        let wall_l = self.maze.wall_x(x - 1, y);
        let wall_u = self.maze.wall_y(x, y - 1);
        let wall_d = self.maze.wall_y(x, y);
        if !wall_r {
            valid[Direction::Right.ord()] = false;
        }
        if !wall_l {
            valid[Direction::Left.ord()] = false;
        }
        if !wall_u {
            valid[Direction::Up.ord()] = false;
        }
        if !wall_d {
            valid[Direction::Down.ord()] = false;
        }

        // Allow following the tip of a wall.
        if wall_r && !wall_l && !wall_u && !wall_d {
            if vertex.corner == Corner::TopLeft {
                valid[Direction::Down.ord()] = true;
            } else if vertex.corner == Corner::BottomLeft {
                valid[Direction::Up.ord()] = true;
            }
        } else if !wall_r && wall_l && !wall_u && !wall_d {
            if vertex.corner == Corner::TopRight {
                valid[Direction::Down.ord()] = true;
            } else if vertex.corner == Corner::BottomRight {
                valid[Direction::Up.ord()] = true;
            }
        } else if !wall_r && !wall_l && wall_u && !wall_d {
            if vertex.corner == Corner::BottomRight {
                valid[Direction::Left.ord()] = true;
            } else if vertex.corner == Corner::BottomLeft {
                valid[Direction::Right.ord()] = true;
            }
        } else if !wall_r && !wall_l && !wall_u && wall_d {
            if vertex.corner == Corner::TopRight {
                valid[Direction::Left.ord()] = true;
            } else if vertex.corner == Corner::TopLeft {
                valid[Direction::Right.ord()] = true;
            }
        }

        // Cannot continue in, or reverse of, the last direction.
        if let Some(last) = last {
            valid[last.ord()] = false;
            valid[rotate_direction(last, Direction::Down).ord()] = false;
        }

        for d in Direction::ALL {
            if valid[d.ord()] {
                return Some(d);
            }
        }
        None
    }

    /// `cBuilding::GetNextVertex` (`cBuilding.cpp:689`): step from `vertex`
    /// in `direction` until the next existing wall vertex is reached (or the
    /// path leaves the building / hits a blocking wall).
    fn get_next_vertex(
        &self,
        vertex: WallVertex,
        direction: Direction,
        set: &BTreeSet<WallVertex>,
    ) -> Option<WallVertex> {
        // aaNextVertexData[Direction][Corner] = (offset_x, offset_y, corner).
        const NEXT: [[(i32, i32, Corner); 4]; 4] = [
            // directionUp
            [
                (0, -1, Corner::BottomLeft),
                (0, -1, Corner::BottomRight),
                (0, 0, Corner::TopLeft),
                (0, 0, Corner::TopRight),
            ],
            // directionLeft
            [
                (-1, 0, Corner::TopRight),
                (0, 0, Corner::TopLeft),
                (-1, 0, Corner::BottomRight),
                (0, 0, Corner::BottomLeft),
            ],
            // directionDown
            [
                (0, 0, Corner::BottomLeft),
                (0, 0, Corner::BottomRight),
                (0, 1, Corner::TopLeft),
                (0, 1, Corner::TopRight),
            ],
            // directionRight
            [
                (0, 0, Corner::TopRight),
                (1, 0, Corner::TopLeft),
                (0, 0, Corner::BottomRight),
                (1, 0, Corner::BottomLeft),
            ],
        ];

        let cx = self.maze.cx;
        let cy = self.maze.cy;
        let mut new_vertex = vertex;
        loop {
            let surrounding = self.vertex_surrounding_walls(vertex);
            if surrounding[direction.ord()] {
                // A wall blocks access to the next vertex.
                return None;
            }
            let data = NEXT[direction.ord()][new_vertex.corner.ord() as usize];
            new_vertex.position_x += data.0;
            new_vertex.position_y += data.1;
            new_vertex.corner = data.2;

            if new_vertex.position_x < 0
                || new_vertex.position_x > cx
                || new_vertex.position_y < 0
                || new_vertex.position_y > cy
            {
                return None;
            }
            if let Some(found) = set.get(&new_vertex) {
                return Some(*found);
            }
        }
    }

    /// `cBuilding::CalculateWallPolygons` (`cBuilding.cpp:825`): trace the
    /// wall vertices into closed polygons. Each polygon repeats its start
    /// vertex at the end (verbatim). Returns `(polygons, wall_segments)`
    /// where `wall_segments` is the total distinct vertex count driving the
    /// `wallSegments*4+8` buffer sizing in `render::building3d`.
    ///
    /// The **first** polygon in the returned vector is the outer boundary
    /// (it begins at the globally smallest wall vertex) — `build_mesh`
    /// reverses its winding (PARITY, `cBuilding3D.cpp:187-193`).
    pub fn wall_polygons(&self) -> (Vec<Vec<WallVertex>>, usize) {
        let vertices = self.calculate_wall_vertices();
        let mut set: BTreeSet<WallVertex> = vertices.into_iter().collect();
        let mut polygons = Vec::new();
        let mut wall_segments = 0usize;

        while let Some(&start) = set.iter().next() {
            let mut polygon = vec![start];
            wall_segments += 1;
            let mut orientation: Option<Direction> = None;
            let mut vertex = start;
            loop {
                orientation = self.get_next_direction(vertex, orientation);
                let Some(dir) = orientation else {
                    // No valid direction: matches the original's log-and-
                    // stop; close the (degenerate) polygon and move on.
                    break;
                };
                let Some(next) = self.get_next_vertex(vertex, dir, &set) else {
                    break;
                };
                vertex = next;
                set.remove(&next);
                if vertex == start {
                    // Loop closed — the start vertex is repeated at the end.
                    polygon.push(start);
                    break;
                }
                polygon.push(vertex);
                wall_segments += 1;
            }
            // Guard against an unclosed degenerate polygon leaving `start`
            // in the set forever (the original always closes on real mazes).
            set.remove(&start);
            polygons.push(polygon);
        }

        (polygons, wall_segments)
    }

    // ===================== wall collision (cBuilding.cpp) =====================
    // PARITY: ported from `cBuilding.cpp`. Wall existence is read through
    // `Maze::surrounding_walls` (the GUARD out-of-grid=no-wall accessor,
    // design §1.3). These are the single source for wall collision — both
    // `sim::world` (robot/projectile) and `path` (the AI room graph) call them.

    /// `cMaze::GetWall(x, y, dir)` for the two wall directions.
    /// `WallDir::X` = `surrounding_walls[top]` = `h_wall(x,y)`;
    /// `WallDir::Y` = `surrounding_walls[left]` = `v_wall(x,y)`.
    pub fn wall_at(&self, dir: WallDir, x: i32, y: i32) -> bool {
        let sw = self.maze.surrounding_walls(x, y);
        match dir {
            WallDir::X => sw[0], // top edge of room (x,y)
            WallDir::Y => sw[2], // left edge of room (x,y)
            WallDir::Invalid => false, // tracer sentinel; never a collision query
        }
    }

    /// `cBuilding::WallToRect` (`cBuilding.cpp:96-119`).
    pub fn wall_to_rect(&self, dir: WallDir, x: i32, y: i32) -> Rect {
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
            WallDir::Invalid => unreachable!("wall_to_rect: collision uses only X/Y"),
        }
    }

    /// `cBuilding::WallToBox` (`cBuilding.cpp:122-147`).
    pub fn wall_to_box(&self, dir: WallDir, x: i32, y: i32) -> Box3 {
        let rs = self.room_size;
        let ww = self.wall_width;
        let ch = self.ceiling_height;
        match dir {
            WallDir::X => Box3 {
                center: Vec3::new((x as f32 + 0.5) * rs, y as f32 * rs, 0.5 * ch),
                size: Vec3::new(rs + ww, ww, ch),
            },
            WallDir::Y => Box3 {
                center: Vec3::new(x as f32 * rs, (y as f32 + 0.5) * rs, 0.5 * ch),
                size: Vec3::new(ww, rs + ww, ch),
            },
            WallDir::Invalid => unreachable!("wall_to_box: collision uses only X/Y"),
        }
    }

    /// PARITY: `cBuilding::IntersectRect` (`cBuilding.cpp:164-208`) — static
    /// rect-vs-wall test used by spawn placement. Preserves the original's
    /// quirk of using `size.y` for BOTH the x and y bounds
    /// (`cBuilding.cpp:170-173`; harmless because spawn rects are square).
    pub fn intersect_rect(&self, rc: Rect) -> bool {
        let recip = 1.0 / self.room_size;
        let size_y = (rc.size.y + self.wall_width) * 0.5;
        let iwx1 = floor_i32((rc.center.x - size_y) * recip);
        let iwy1 = floor_i32((rc.center.y - size_y) * recip);
        let iwx2 = floor_i32((rc.center.x + size_y) * recip);
        let iwy2 = floor_i32((rc.center.y + size_y) * recip);

        // Walls in the x direction.
        for y in (iwy1 + 1).max(0)..=iwy2 {
            for x in iwx1.max(0)..=iwx2 {
                if self.wall_at(WallDir::X, x, y)
                    && rect_rect(rc, self.wall_to_rect(WallDir::X, x, y))
                {
                    return true;
                }
            }
        }
        // Walls in the y direction.
        for y in iwy1.max(0)..=iwy2 {
            for x in (iwx1 + 1).max(0)..=iwx2 {
                if self.wall_at(WallDir::Y, x, y)
                    && rect_rect(rc, self.wall_to_rect(WallDir::Y, x, y))
                {
                    return true;
                }
            }
        }
        false
    }

    /// PARITY: `cBuilding::IntersectMovingRect` -> `GetIntersectedWalls`
    /// (`cBuilding.cpp:211-216,451-460`) with `pntr` set, `pvWalls` null =>
    /// `bFindAll = true`: returns the earliest-time wall intersection.
    pub fn intersect_moving_rect(&self, mrc: MovingRect) -> Intersection2D {
        let mut ntr = no_intersection_2d();
        let mut walls: Vec<(WallDir, i32, i32)> = Vec::new();
        self.get_intersected_walls_portion(mrc, false, true, &mut walls, &mut ntr, 0);
        ntr
    }

    /// PARITY: `cBuilding::IntersectTrajectory` (`cBuilding.cpp:219-273`).
    pub fn intersect_trajectory(&self, traj: Trajectory) -> Intersection3D {
        // The moving rect spans the trajectory's 2D footprint (size 0).
        let end =
            accelerate_point(traj.travel_time, traj.start, traj.direction, traj.gravity);
        let mrc = MovingRect {
            center1: traj.start.truncate(),
            center2: end.truncate(),
            size: Vec2::ZERO,
            stop_time: 1.0,
        };
        let mut walls: Vec<(WallDir, i32, i32)> = Vec::new();
        let mut ntr2 = no_intersection_2d();
        self.get_intersected_walls_portion(mrc, true, false, &mut walls, &mut ntr2, 0);

        let mut ntr = no_intersection_3d();
        for &(dir, x, y) in &walls {
            let bx = self.wall_to_box(dir, x, y);
            let n = trajectory_box(traj, bx);
            if n.hit && n.time < ntr.time {
                ntr = n;
            }
        }

        let size = self.size();
        // Floor plane.
        let floor = Plane3D {
            axis: PlaneAxis::Z,
            center: Vec3::new(size.x * 0.5, size.y * 0.5, 0.0),
            size,
        };
        let n = trajectory_plane(traj, floor);
        if n.hit && n.time < ntr.time {
            ntr = n;
        }
        // Ceiling plane.
        let ceiling = Plane3D {
            axis: PlaneAxis::Z,
            center: Vec3::new(size.x * 0.5, size.y * 0.5, self.ceiling_height),
            size,
        };
        let n = trajectory_plane(traj, ceiling);
        if n.hit && n.time < ntr.time {
            ntr = n;
        }

        ntr
    }

    /// PARITY: `cBuilding::GetIntersectedWallsPortion`
    /// (`cBuilding.cpp:467-602`). Both call sites pass `bFindAll = true`, so
    /// this always visits every sub-segment and every candidate wall;
    /// `want_walls` collects the intersected walls, `want_ntr` keeps the
    /// earliest-time intersection. Recursion depth is capped at 3.
    #[allow(clippy::too_many_arguments)]
    fn get_intersected_walls_portion(
        &self,
        mrc: MovingRect,
        want_walls: bool,
        want_ntr: bool,
        walls: &mut Vec<(WallDir, i32, i32)>,
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
                // Both points are outside of the maze.
                return;
            }

            // Walls in the x direction.
            for y in (iwy1 + 1)..=iwy2 {
                for x in iwx1..=iwx2 {
                    if self.wall_at(WallDir::X, x, y) {
                        let n = moving_rect_rect(mrc, self.wall_to_rect(WallDir::X, x, y));
                        if !n.hit {
                            continue;
                        }
                        if want_walls {
                            walls.push((WallDir::X, x, y));
                        }
                        if want_ntr && n.time < ntr.time {
                            *ntr = n;
                        }
                    }
                }
            }
            // Walls in the y direction.
            for y in iwy1..=iwy2 {
                for x in (iwx1 + 1)..=iwx2 {
                    if self.wall_at(WallDir::Y, x, y) {
                        let n = moving_rect_rect(mrc, self.wall_to_rect(WallDir::Y, x, y));
                        if !n.hit {
                            continue;
                        }
                        if want_walls {
                            walls.push((WallDir::Y, x, y));
                        }
                        if want_ntr && n.time < ntr.time {
                            *ntr = n;
                        }
                    }
                }
            }
        } else {
            // Split the segment into four and recurse.
            let movement = mrc.center2 - mrc.center1;
            for (a, b) in [(0.0, 0.25), (0.25, 0.5), (0.5, 0.75), (0.75, 1.0)] {
                let sub = MovingRect {
                    center1: mrc.center1 + movement * a,
                    center2: mrc.center1 + movement * b,
                    size: mrc.size,
                    stop_time: mrc.stop_time,
                };
                self.get_intersected_walls_portion(sub, want_walls, want_ntr, walls, ntr, level + 1);
            }
        }
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

    /// A single fully-bordered 1x1 room: walls on all four sides.
    fn one_room_bordered() -> Building {
        Building {
            maze: Maze::new(1, 1, vec![true, true], vec![true, true]),
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

    #[test]
    fn one_room_traces_a_single_four_segment_outer_polygon() {
        let b = one_room_bordered();
        let (polygons, wall_segments) = b.wall_polygons();
        assert_eq!(polygons.len(), 1, "one bordered room => one polygon");
        assert_eq!(wall_segments, 4, "square frame has 4 wall segments");

        // Verbatim traced order (hand-computed, see design §7 M4 test note):
        // start = smallest vertex (0,0,BR); trace Down, Right, Up, Left.
        let p = &polygons[0];
        assert_eq!(p.len(), 5, "closed polygon repeats its start vertex");
        assert_eq!(
            p[0],
            WallVertex {
                position_x: 0,
                position_y: 0,
                corner: Corner::BottomRight
            }
        );
        assert_eq!(
            p[1],
            WallVertex {
                position_x: 0,
                position_y: 1,
                corner: Corner::TopRight
            }
        );
        assert_eq!(
            p[2],
            WallVertex {
                position_x: 1,
                position_y: 1,
                corner: Corner::TopLeft
            }
        );
        assert_eq!(
            p[3],
            WallVertex {
                position_x: 1,
                position_y: 0,
                corner: Corner::BottomLeft
            }
        );
        assert_eq!(p[4], p[0], "start repeated at the end");
    }

    #[test]
    fn wall_vertex_to_point_insets_by_half_wall_width() {
        let b = one_room_bordered();
        // (0,0,BR) => base (0,0) + (+0.5, +0.5).
        assert_eq!(
            b.wall_vertex_to_point(WallVertex {
                position_x: 0,
                position_y: 0,
                corner: Corner::BottomRight
            }),
            (0.5, 0.5)
        );
        // (0,1,TR) => base (0,10) + (+0.5, -0.5).
        assert_eq!(
            b.wall_vertex_to_point(WallVertex {
                position_x: 0,
                position_y: 1,
                corner: Corner::TopRight
            }),
            (0.5, 9.5)
        );
        // (1,1,TL) => base (10,10) + (-0.5, -0.5).
        assert_eq!(
            b.wall_vertex_to_point(WallVertex {
                position_x: 1,
                position_y: 1,
                corner: Corner::TopLeft
            }),
            (9.5, 9.5)
        );
    }

    #[test]
    fn wall_direction_classifies_segments() {
        let v_br = WallVertex {
            position_x: 0,
            position_y: 0,
            corner: Corner::BottomRight,
        };
        let v_tr = WallVertex {
            position_x: 0,
            position_y: 1,
            corner: Corner::TopRight,
        };
        let v_tl = WallVertex {
            position_x: 1,
            position_y: 1,
            corner: Corner::TopLeft,
        };
        // (0,0)->(0,1): x equal, y differ => wallY.
        assert_eq!(Building::wall_direction(v_br, v_tr), WallDir::Y);
        // (0,1)->(1,1): x differ, y equal => wallX.
        assert_eq!(Building::wall_direction(v_tr, v_tl), WallDir::X);
    }

    #[test]
    fn borderless_maze_traces_no_polygons() {
        let b = tiny_building(); // all walls false
        let (polygons, wall_segments) = b.wall_polygons();
        assert!(polygons.is_empty());
        assert_eq!(wall_segments, 0);
    }
}
