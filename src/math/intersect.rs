//! Swept AABB/plane/trajectory intersection tests, ported verbatim from
//! `Intersect.h/.cpp` (source-map §3.7, §7; design §1.3).
//!
//! PARITY: face tests run in a **fixed order** — `+x, -x, +y, -y, +z, -z`
//! — and the first hit *in that order* wins, not the earliest-time hit
//! (source-map §7, design §1.3). Bounce-face selection depends on this
//! order in corner cases where first-hit-order != earliest-time.
//! The no-intersection sentinel time is `1.0` (`Intersect.h:107,115`).
//!
//! Every function here mirrors one `Intersect::*` free function from the
//! C++ (there was no `class`, only a `namespace`). Structs mirror the
//! `t*` value types from `Intersect.h`; field names are the Rust-cased
//! equivalent of the original member names to keep side-by-side diffing
//! easy.

use super::{accelerate_point, between, equals};
use glam::{Vec2, Vec3};

/// Fixed face-test order — PARITY, never reorder.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Face {
    PlusX,
    MinusX,
    PlusY,
    MinusY,
    PlusZ,
    MinusZ,
}

/// The exact iteration order `Intersect.h/.cpp` tests faces in.
pub const FACE_ORDER: [Face; 6] = [
    Face::PlusX,
    Face::MinusX,
    Face::PlusY,
    Face::MinusY,
    Face::PlusZ,
    Face::MinusZ,
];

/// Time value meaning "no intersection found in [0, 1]".
///
/// PARITY: `Intersect.h:107,115` — literally `1.0`, the same value as a
/// hit exactly at the end of the swept interval; callers distinguish via
/// the returned `hit` flag, not by comparing to this sentinel.
pub const NO_HIT_TIME: f32 = 1.0;

/// `ePlaneType` (`Intersect.h:33`) — which axis a plane's normal points
/// along. `Invalid` is the sentinel value baked into the "no intersection"
/// constants (`ntr2NoIntersection`/`ntr3NoIntersection`, `Intersect.h:106,115`).
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub enum PlaneAxis {
    X,
    Y,
    #[default]
    Invalid,
    Z,
}

/// `tQuadratic` (`Intersect.h:10-14`).
#[derive(Copy, Clone, Debug)]
pub struct Quadratic {
    pub a: f32,
    pub b: f32,
    pub c: f32,
}

/// `tTrajectory` (`Intersect.h:17-23`). `direction` is `vec3Direction`
/// (an initial velocity, not a unit vector) and `gravity` is the scalar
/// `fGravityAccel` applied on -Z via `accelerate_point`.
#[derive(Copy, Clone, Debug)]
pub struct Trajectory {
    pub start: Vec3,
    pub direction: Vec3,
    pub gravity: f32,
    pub travel_time: f32,
}

/// `tLine2D` (`Intersect.h:25-29`).
#[derive(Copy, Clone, Debug)]
pub struct Line2D {
    pub p1: Vec2,
    pub p2: Vec2,
    pub stop_time: f32,
}

/// `tPlane2D` (`Intersect.h:35-39`).
#[derive(Copy, Clone, Debug, Default)]
pub struct Plane2D {
    pub axis: PlaneAxis,
    pub center: Vec2,
    pub size: Vec2,
}

/// `tPlane3D` (`Intersect.h:55-59`).
#[derive(Copy, Clone, Debug, Default)]
pub struct Plane3D {
    pub axis: PlaneAxis,
    pub center: Vec3,
    pub size: Vec3,
}

/// `tMovingPlane3D` (`Intersect.h:61-67`).
#[derive(Copy, Clone, Debug)]
pub struct MovingPlane3D {
    pub axis: PlaneAxis,
    pub center1: Vec3,
    pub center2: Vec3,
    pub size: Vec3,
    pub stop_time: f32,
}

/// `tRect` (`Intersect.h:69-72`).
#[derive(Copy, Clone, Debug)]
pub struct Rect {
    pub center: Vec2,
    pub size: Vec2,
}

/// `tMovingRect` (`Intersect.h:74-79`).
#[derive(Copy, Clone, Debug)]
pub struct MovingRect {
    pub center1: Vec2,
    pub center2: Vec2,
    pub size: Vec2,
    pub stop_time: f32,
}

/// `tBox` (`Intersect.h:81-84`). Named `Box3` — `Box` is a Rust prelude type.
#[derive(Copy, Clone, Debug)]
pub struct Box3 {
    pub center: Vec3,
    pub size: Vec3,
}

/// `tMovingBox` (`Intersect.h:86-91`).
#[derive(Copy, Clone, Debug)]
pub struct MovingBox {
    pub center1: Vec3,
    pub center2: Vec3,
    pub size: Vec3,
    pub stop_time: f32,
}

/// `tIntersection2D` (`Intersect.h:93-97`).
#[derive(Copy, Clone, Debug)]
pub struct Intersection2D {
    pub hit: bool,
    pub time: f32,
    pub plane: Plane2D,
}

/// `tIntersection3D` (`Intersect.h:102-106`).
#[derive(Copy, Clone, Debug)]
pub struct Intersection3D {
    pub hit: bool,
    pub time: f32,
    pub plane: Plane3D,
}

/// `ntr2NoIntersection` (`Intersect.h:106`): `{false, 1.0f, {planeInvalid, (0,0), (0,0)}}`.
pub fn no_intersection_2d() -> Intersection2D {
    Intersection2D {
        hit: false,
        time: NO_HIT_TIME,
        plane: Plane2D::default(),
    }
}

/// `ntr3NoIntersection` (`Intersect.h:115`): `{false, 1.0f, {planeInvalid, (0,0,0), (0,0,0)}}`.
pub fn no_intersection_3d() -> Intersection3D {
    Intersection3D {
        hit: false,
        time: NO_HIT_TIME,
        plane: Plane3D::default(),
    }
}

// ---------------------------------------------------------------------
// 2D
// ---------------------------------------------------------------------

/// `Intersect::PointRect` (`Intersect.cpp:662-669`).
pub fn point_rect(point: Vec2, rect: Rect) -> bool {
    let inside_x = between(
        point.x,
        rect.center.x - rect.size.x * 0.5,
        rect.center.x + rect.size.x * 0.5,
    );
    let inside_y = between(
        point.y,
        rect.center.y - rect.size.y * 0.5,
        rect.center.y + rect.size.y * 0.5,
    );
    inside_x && inside_y
}

/// `Intersect::PlaneLine` (`Intersect.cpp:574-660`).
pub fn plane_line(plane: Plane2D, line: Line2D) -> Intersection2D {
    let slope_x = (line.p2.x - line.p1.x) / line.stop_time;
    let slope_y = (line.p2.y - line.p1.y) / line.stop_time;

    let point: Vec2;
    let inside_x: bool;
    let inside_y: bool;
    let time: f32;

    match plane.axis {
        PlaneAxis::X => {
            if equals(slope_x, 0.0) {
                if !equals(plane.center.x, line.p1.x) {
                    return no_intersection_2d();
                }
                time = 0.0;
                point = line.p1;
            } else {
                time = (plane.center.x - line.p1.x) / slope_x;
                point = Vec2::new(plane.center.x, line.p1.y + time * slope_y);
            }
            inside_x = true;
            inside_y = between(
                point.y,
                plane.center.y - plane.size.y * 0.5,
                plane.center.y + plane.size.y * 0.5,
            );
        }
        PlaneAxis::Y => {
            if equals(slope_y, 0.0) {
                if !equals(plane.center.y, line.p1.y) {
                    return no_intersection_2d();
                }
                time = 0.0;
                point = line.p1;
            } else {
                time = (plane.center.y - line.p1.y) / slope_y;
                point = Vec2::new(line.p1.x + time * slope_x, plane.center.y);
            }
            inside_x = between(
                point.x,
                plane.center.x - plane.size.x * 0.5,
                plane.center.x + plane.size.x * 0.5,
            );
            inside_y = true;
        }
        _ => return no_intersection_2d(),
    }

    let hit = inside_x && inside_y && between(time, 0.0, line.stop_time);
    if hit {
        Intersection2D { hit, time, plane }
    } else {
        no_intersection_2d()
    }
}

/// `Intersect::RectLine` (`Intersect.cpp:515-572`) — PARITY: tests faces in
/// the fixed order `+x, -x, +y, -y` (the 2D subset of `FACE_ORDER`).
pub fn rect_line(rect: Rect, line: Line2D) -> Intersection2D {
    if point_rect(line.p1, rect) {
        return Intersection2D {
            hit: true,
            time: 0.0,
            plane: Plane2D {
                axis: PlaneAxis::X,
                center: rect.center,
                size: rect.size,
            },
        };
    }

    // +x
    let plane = Plane2D {
        axis: PlaneAxis::X,
        center: Vec2::new(rect.center.x + rect.size.x * 0.5, rect.center.y),
        size: Vec2::new(0.0, rect.size.y),
    };
    let r = plane_line(plane, line);
    if r.hit {
        return r;
    }

    // -x
    let plane = Plane2D {
        axis: PlaneAxis::X,
        center: Vec2::new(rect.center.x - rect.size.x * 0.5, rect.center.y),
        size: Vec2::new(0.0, rect.size.y),
    };
    let r = plane_line(plane, line);
    if r.hit {
        return r;
    }

    // +y
    let plane = Plane2D {
        axis: PlaneAxis::Y,
        center: Vec2::new(rect.center.x, rect.center.y + rect.size.y * 0.5),
        size: Vec2::new(rect.size.x, 0.0),
    };
    let r = plane_line(plane, line);
    if r.hit {
        return r;
    }

    // -y
    let plane = Plane2D {
        axis: PlaneAxis::Y,
        center: Vec2::new(rect.center.x, rect.center.y - rect.size.y * 0.5),
        size: Vec2::new(rect.size.x, 0.0),
    };
    let r = plane_line(plane, line);
    if r.hit {
        return r;
    }

    no_intersection_2d()
}

/// `Intersect::MovingRectLine` (`Intersect.cpp:459-484`).
pub fn moving_rect_line(mrect: MovingRect, line: Line2D) -> Intersection2D {
    let rect = Rect {
        center: mrect.center1,
        size: mrect.size,
    };
    let mut new_line = line;
    new_line.p2 -= mrect.center2 - mrect.center1;

    let r = rect_line(rect, new_line);
    if !r.hit {
        return r;
    }

    if r.time <= mrect.stop_time {
        r
    } else {
        let rect2 = Rect {
            center: mrect.center2,
            size: mrect.size,
        };
        rect_line(rect2, line)
    }
}

/// `Intersect::RectRect` (`Intersect.cpp:154-160`).
pub fn rect_rect(rect1: Rect, rect2: Rect) -> bool {
    let big = Rect {
        center: rect1.center,
        size: rect1.size + rect2.size,
    };
    point_rect(rect2.center, big)
}

/// `Intersect::MovingRectRect` (`Intersect.cpp:84-117`).
pub fn moving_rect_rect(mrect: MovingRect, rect: Rect) -> Intersection2D {
    let big = Rect {
        center: rect.center,
        size: mrect.size + rect.size,
    };
    let line = Line2D {
        p1: mrect.center1,
        p2: mrect.center2,
        stop_time: mrect.stop_time,
    };

    let r = rect_line(big, line);
    if !r.hit {
        return r;
    }

    if r.time <= mrect.stop_time {
        r
    } else if point_rect(mrect.center2, big) {
        Intersection2D {
            hit: true,
            time: mrect.stop_time,
            ..no_intersection_2d()
        }
    } else {
        no_intersection_2d()
    }
}

/// `Intersect::MovingRectMovingRect` (`Intersect.cpp:11-82`).
pub fn moving_rect_moving_rect(rect1: MovingRect, rect2: MovingRect) -> Intersection2D {
    let big = MovingRect {
        center1: rect1.center1,
        center2: rect1.center2,
        size: rect1.size + rect2.size,
        stop_time: rect1.stop_time,
    };
    let line = Line2D {
        p1: rect2.center1,
        p2: rect2.center2,
        stop_time: rect2.stop_time,
    };

    let result = moving_rect_line(big, line);
    if !result.hit {
        return result;
    }

    let valid1 = result.time <= rect1.stop_time;
    let valid2 = result.time <= rect2.stop_time;

    if !valid1 && !valid2 {
        let r1 = Rect {
            center: rect1.center2,
            size: rect1.size,
        };
        let r2 = Rect {
            center: rect2.center2,
            size: rect2.size,
        };
        if rect_rect(r1, r2) {
            let stop = if rect1.stop_time > rect2.stop_time {
                rect1.stop_time
            } else {
                rect2.stop_time
            };
            Intersection2D {
                hit: true,
                time: stop,
                ..no_intersection_2d()
            }
        } else {
            no_intersection_2d()
        }
    } else if !valid1 {
        let big2 = MovingRect {
            center1: rect2.center1,
            center2: rect2.center2,
            size: rect2.size + rect1.size,
            stop_time: rect2.stop_time,
        };
        let r = Rect {
            center: rect1.center2,
            size: rect1.size,
        };
        moving_rect_rect(big2, r)
    } else if !valid2 {
        let big2 = MovingRect {
            center1: rect1.center1,
            center2: rect1.center2,
            size: rect1.size + rect2.size,
            stop_time: rect1.stop_time,
        };
        let r = Rect {
            center: rect2.center2,
            size: rect2.size,
        };
        moving_rect_rect(big2, r)
    } else {
        result
    }
}

// ---------------------------------------------------------------------
// 3D
// ---------------------------------------------------------------------

/// `Intersect::PointBox` (`Intersect.cpp:671-680`).
pub fn point_box(point: Vec3, b: Box3) -> bool {
    let inside_x = between(
        point.x,
        b.center.x - b.size.x * 0.5,
        b.center.x + b.size.x * 0.5,
    );
    let inside_y = between(
        point.y,
        b.center.y - b.size.y * 0.5,
        b.center.y + b.size.y * 0.5,
    );
    let inside_z = between(
        point.z,
        b.center.z - b.size.z * 0.5,
        b.center.z + b.size.z * 0.5,
    );
    inside_x && inside_y && inside_z
}

/// `Intersect::SolveQuadratic` (`Intersect.cpp:719-761`) — returns the
/// smallest non-negative real solution, or `None` if there isn't one.
pub fn solve_quadratic(q: Quadratic) -> Option<f32> {
    let Quadratic { a, b, c } = q;
    let discriminant = b * b - 4.0 * a * c;
    if discriminant < 0.0 {
        return None;
    }
    if equals(a, 0.0) {
        // Special case to prevent division by zero (results in infinity).
        return None;
    }
    let sqrt_d = discriminant.sqrt();
    let s1 = (-b + sqrt_d) / (2.0 * a);
    let s2 = (-b - sqrt_d) / (2.0 * a);

    if s1 < 0.0 && s2 < 0.0 {
        return None;
    }
    if s1 < 0.0 {
        return Some(s2);
    }
    if s2 < 0.0 {
        return Some(s1);
    }
    Some(if s1 < s2 { s1 } else { s2 })
}

/// `Intersect::TrajectoryPlane` (`Intersect.cpp:346-457`).
pub fn trajectory_plane(traj: Trajectory, plane: Plane3D) -> Intersection3D {
    let point: Vec3;
    let inside_x: bool;
    let inside_y: bool;
    let inside_z: bool;
    let time: f32;

    match plane.axis {
        PlaneAxis::X => {
            if equals(traj.direction.x, 0.0) {
                if !equals(plane.center.x, traj.start.x) {
                    return no_intersection_3d();
                }
                return Intersection3D {
                    hit: true,
                    time: 0.0,
                    plane,
                };
            }
            time = (plane.center.x - traj.start.x) / traj.direction.x;
            let mut p = accelerate_point(time, traj.start, traj.direction, traj.gravity);
            p.x = plane.center.x;
            point = p;

            inside_x = true;
            inside_y = between(
                point.y,
                plane.center.y - plane.size.y * 0.5,
                plane.center.y + plane.size.y * 0.5,
            );
            inside_z = between(
                point.z,
                plane.center.z - plane.size.z * 0.5,
                plane.center.z + plane.size.z * 0.5,
            );
        }
        PlaneAxis::Y => {
            if equals(traj.direction.y, 0.0) {
                if !equals(plane.center.y, traj.start.y) {
                    return no_intersection_3d();
                }
                return Intersection3D {
                    hit: true,
                    time: 0.0,
                    plane,
                };
            }
            time = (plane.center.y - traj.start.y) / traj.direction.y;
            let mut p = accelerate_point(time, traj.start, traj.direction, traj.gravity);
            p.y = plane.center.y;
            point = p;

            inside_x = between(
                point.x,
                plane.center.x - plane.size.x * 0.5,
                plane.center.x + plane.size.x * 0.5,
            );
            inside_y = true;
            inside_z = between(
                point.z,
                plane.center.z - plane.size.z * 0.5,
                plane.center.z + plane.size.z * 0.5,
            );
        }
        PlaneAxis::Z => {
            if equals(traj.gravity, 0.0) {
                // Special case: the "a" coefficient of the quadratic would be zero.
                if equals(traj.direction.z, 0.0) {
                    if !equals(plane.center.z, traj.start.z) {
                        return no_intersection_3d();
                    }
                    return Intersection3D {
                        hit: true,
                        time: 0.0,
                        plane,
                    };
                }
                time = (plane.center.z - traj.start.z) / traj.direction.z;
            } else {
                let quadratic = Quadratic {
                    a: -traj.gravity * 0.5,
                    b: traj.direction.z,
                    c: traj.start.z - plane.center.z,
                };
                match solve_quadratic(quadratic) {
                    Some(t) => time = t,
                    None => return no_intersection_3d(),
                }
            }

            point = Vec3::new(
                traj.start.x + traj.direction.x * time,
                traj.start.y + traj.direction.y * time,
                plane.center.z,
            );

            inside_x = between(
                point.x,
                plane.center.x - plane.size.x * 0.5,
                plane.center.x + plane.size.x * 0.5,
            );
            inside_y = between(
                point.y,
                plane.center.y - plane.size.y * 0.5,
                plane.center.y + plane.size.y * 0.5,
            );
            inside_z = true;
        }
        PlaneAxis::Invalid => return no_intersection_3d(),
    }

    let hit = inside_x && inside_y && inside_z && between(time, 0.0, traj.travel_time);
    if hit {
        Intersection3D { hit, time, plane }
    } else {
        no_intersection_3d()
    }
}

/// `Intersect::TrajectoryMovingPlane` (`Intersect.cpp:682-714`).
pub fn trajectory_moving_plane(traj: Trajectory, mplane: MovingPlane3D) -> Intersection3D {
    let mut new_traj = traj;
    new_traj.direction.x -= mplane.center2.x - mplane.center1.x;
    new_traj.direction.y -= mplane.center2.y - mplane.center1.y;
    new_traj.direction.z -= mplane.center2.z - mplane.center1.z;

    let plane = Plane3D {
        axis: mplane.axis,
        center: mplane.center1,
        size: mplane.size,
    };
    let result = trajectory_plane(new_traj, plane);
    if !result.hit {
        return result;
    }

    if result.time <= mplane.stop_time {
        result
    } else {
        let plane2 = Plane3D {
            axis: mplane.axis,
            center: mplane.center2,
            size: mplane.size,
        };
        trajectory_plane(new_traj, plane2)
    }
}

/// `Intersect::TrajectoryBox` (`Intersect.cpp:267-344`) — PARITY: tests
/// faces in the fixed `FACE_ORDER` (+x, -x, +y, -y, +z, -z); the first hit
/// in that order wins, not necessarily the earliest-time hit.
pub fn trajectory_box(traj: Trajectory, b: Box3) -> Intersection3D {
    if point_box(traj.start, b) {
        return Intersection3D {
            hit: true,
            time: 0.0,
            plane: Plane3D {
                axis: PlaneAxis::Z,
                center: b.center,
                size: b.size,
            },
        };
    }

    // +x
    let plane = Plane3D {
        axis: PlaneAxis::X,
        center: Vec3::new(b.center.x + b.size.x * 0.5, b.center.y, b.center.z),
        size: Vec3::new(0.0, b.size.y, b.size.z),
    };
    let r = trajectory_plane(traj, plane);
    if r.hit {
        return r;
    }

    // -x
    let plane = Plane3D {
        axis: PlaneAxis::X,
        center: Vec3::new(b.center.x - b.size.x * 0.5, b.center.y, b.center.z),
        size: Vec3::new(0.0, b.size.y, b.size.z),
    };
    let r = trajectory_plane(traj, plane);
    if r.hit {
        return r;
    }

    // +y
    let plane = Plane3D {
        axis: PlaneAxis::Y,
        center: Vec3::new(b.center.x, b.center.y + b.size.y * 0.5, b.center.z),
        size: Vec3::new(b.size.x, 0.0, b.size.z),
    };
    let r = trajectory_plane(traj, plane);
    if r.hit {
        return r;
    }

    // -y
    let plane = Plane3D {
        axis: PlaneAxis::Y,
        center: Vec3::new(b.center.x, b.center.y - b.size.y * 0.5, b.center.z),
        size: Vec3::new(b.size.x, 0.0, b.size.z),
    };
    let r = trajectory_plane(traj, plane);
    if r.hit {
        return r;
    }

    // +z
    let plane = Plane3D {
        axis: PlaneAxis::Z,
        center: Vec3::new(b.center.x, b.center.y, b.center.z + b.size.z * 0.5),
        size: Vec3::new(b.size.x, b.size.y, 0.0),
    };
    let r = trajectory_plane(traj, plane);
    if r.hit {
        return r;
    }

    // -z
    let plane = Plane3D {
        axis: PlaneAxis::Z,
        center: Vec3::new(b.center.x, b.center.y, b.center.z - b.size.z * 0.5),
        size: Vec3::new(b.size.x, b.size.y, 0.0),
    };
    let r = trajectory_plane(traj, plane);
    if r.hit {
        return r;
    }

    no_intersection_3d()
}

/// `Intersect::TrajectoryMovingBox` (`Intersect.cpp:170-265`) — PARITY:
/// same fixed `FACE_ORDER` as `trajectory_box`.
pub fn trajectory_moving_box(traj: Trajectory, mb: MovingBox) -> Intersection3D {
    let start_box = Box3 {
        center: mb.center1,
        size: mb.size,
    };
    if point_box(traj.start, start_box) {
        return Intersection3D {
            hit: true,
            time: 0.0,
            plane: Plane3D {
                axis: PlaneAxis::Z,
                center: start_box.center,
                size: start_box.size,
            },
        };
    }

    // +x
    let mplane = MovingPlane3D {
        axis: PlaneAxis::X,
        center1: Vec3::new(mb.center1.x + mb.size.x * 0.5, mb.center1.y, mb.center1.z),
        center2: Vec3::new(mb.center2.x + mb.size.x * 0.5, mb.center2.y, mb.center2.z),
        size: Vec3::new(0.0, mb.size.y, mb.size.z),
        stop_time: mb.stop_time,
    };
    let r = trajectory_moving_plane(traj, mplane);
    if r.hit {
        return r;
    }

    // -x
    let mplane = MovingPlane3D {
        axis: PlaneAxis::X,
        center1: Vec3::new(mb.center1.x - mb.size.x * 0.5, mb.center1.y, mb.center1.z),
        center2: Vec3::new(mb.center2.x - mb.size.x * 0.5, mb.center2.y, mb.center2.z),
        size: Vec3::new(0.0, mb.size.y, mb.size.z),
        stop_time: mb.stop_time,
    };
    let r = trajectory_moving_plane(traj, mplane);
    if r.hit {
        return r;
    }

    // +y
    let mplane = MovingPlane3D {
        axis: PlaneAxis::Y,
        center1: Vec3::new(mb.center1.x, mb.center1.y + mb.size.y * 0.5, mb.center1.z),
        center2: Vec3::new(mb.center2.x, mb.center2.y + mb.size.y * 0.5, mb.center2.z),
        size: Vec3::new(mb.size.x, 0.0, mb.size.z),
        stop_time: mb.stop_time,
    };
    let r = trajectory_moving_plane(traj, mplane);
    if r.hit {
        return r;
    }

    // -y
    let mplane = MovingPlane3D {
        axis: PlaneAxis::Y,
        center1: Vec3::new(mb.center1.x, mb.center1.y - mb.size.y * 0.5, mb.center1.z),
        center2: Vec3::new(mb.center2.x, mb.center2.y - mb.size.y * 0.5, mb.center2.z),
        size: Vec3::new(mb.size.x, 0.0, mb.size.z),
        stop_time: mb.stop_time,
    };
    let r = trajectory_moving_plane(traj, mplane);
    if r.hit {
        return r;
    }

    // +z
    let mplane = MovingPlane3D {
        axis: PlaneAxis::Z,
        center1: Vec3::new(mb.center1.x, mb.center1.y, mb.center1.z + mb.size.z * 0.5),
        center2: Vec3::new(mb.center2.x, mb.center2.y, mb.center2.z + mb.size.z * 0.5),
        size: Vec3::new(mb.size.x, mb.size.y, 0.0),
        stop_time: mb.stop_time,
    };
    let r = trajectory_moving_plane(traj, mplane);
    if r.hit {
        return r;
    }

    // -z
    let mplane = MovingPlane3D {
        axis: PlaneAxis::Z,
        center1: Vec3::new(mb.center1.x, mb.center1.y, mb.center1.z - mb.size.z * 0.5),
        center2: Vec3::new(mb.center2.x, mb.center2.y, mb.center2.z - mb.size.z * 0.5),
        size: Vec3::new(mb.size.x, mb.size.y, 0.0),
        stop_time: mb.stop_time,
    };
    let r = trajectory_moving_plane(traj, mplane);
    if r.hit {
        return r;
    }

    no_intersection_3d()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn face_order_is_fixed_and_pinned() {
        assert_eq!(
            FACE_ORDER,
            [
                Face::PlusX,
                Face::MinusX,
                Face::PlusY,
                Face::MinusY,
                Face::PlusZ,
                Face::MinusZ,
            ]
        );
    }

    #[test]
    fn no_hit_sentinel_is_one() {
        assert_eq!(NO_HIT_TIME, 1.0);
        assert_eq!(no_intersection_2d().time, 1.0);
        assert_eq!(no_intersection_3d().time, 1.0);
        assert!(!no_intersection_2d().hit);
        assert!(!no_intersection_3d().hit);
    }

    fn unit_box(center: Vec3) -> Box3 {
        Box3 {
            center,
            size: Vec3::new(2.0, 2.0, 2.0),
        }
    }

    #[test]
    fn trajectory_box_hits_nearest_face_along_pure_x_travel() {
        // travel_time is chosen so the ray reaches the near (-x) face but
        // NOT the far (+x) face — this deliberately avoids the
        // first-tested-face-wins quirk (exercised separately below) so
        // this test genuinely pins "only one face is reachable, and it's
        // the correct one".
        let traj = Trajectory {
            start: Vec3::new(-5.0, 0.0, 0.0),
            direction: Vec3::new(1.0, 0.0, 0.0),
            gravity: 0.0,
            travel_time: 5.0,
        };
        let b = unit_box(Vec3::ZERO);
        let result = trajectory_box(traj, b);
        assert!(result.hit);
        assert_eq!(result.plane.axis, PlaneAxis::X);
        assert!(equals(result.plane.center.x, -1.0));
        assert!(equals(result.time, 4.0));
    }

    #[test]
    fn trajectory_box_miss_returns_sentinel() {
        // travel_time is too short to reach any face of the box.
        let traj = Trajectory {
            start: Vec3::new(-5.0, 0.0, 0.0),
            direction: Vec3::new(1.0, 0.0, 0.0),
            gravity: 0.0,
            travel_time: 0.5,
        };
        let result = trajectory_box(traj, unit_box(Vec3::ZERO));
        assert!(!result.hit);
        assert_eq!(result.time, NO_HIT_TIME);
    }

    /// PARITY corner case (design §1.3/§6): `RectLine` (the 2D twin of
    /// `TrajectoryBox`'s face loop) returns the **first face tested that
    /// reports a hit**, not the hit with the smallest `time`. This line
    /// truly enters the box through -y at t=0.2 (the geometrically
    /// earliest crossing) and exits through +x at t=0.8, but because
    /// FACE_ORDER tests +x before -y, `rect_line` returns the *later*,
    /// exit-face hit — never even evaluating -y.
    #[test]
    fn rect_line_first_tested_hit_wins_over_earliest_time() {
        let rect = Rect {
            center: Vec2::ZERO,
            size: Vec2::new(2.0, 2.0), // half-extents 1.0
        };
        // Hand-solved so that point(0.2) = (0,-1) (true entry, -y face)
        // and point(0.8) = (1,0) (exit, +x face), both valid crossings.
        let line = Line2D {
            p1: Vec2::new(-1.0 / 3.0, -4.0 / 3.0),
            p2: Vec2::new(4.0 / 3.0, 1.0 / 3.0),
            stop_time: 1.0,
        };

        // Sanity check the hand-solved parametrization independently of
        // rect_line's internals.
        let at = |t: f32| line.p1 + t * (line.p2 - line.p1);
        assert!((at(0.2) - Vec2::new(0.0, -1.0)).length() < 1e-5);
        assert!((at(0.8) - Vec2::new(1.0, 0.0)).length() < 1e-5);

        let result = rect_line(rect, line);
        assert!(result.hit);
        assert_eq!(result.plane.axis, PlaneAxis::X);
        assert!(equals(result.time, 0.8));
        assert!(
            !equals(result.time, 0.2),
            "must not report the earlier, geometrically-true entry time"
        );
    }

    #[test]
    fn trajectory_box_start_inside_is_immediate_hit_at_time_zero() {
        let traj = Trajectory {
            start: Vec3::ZERO,
            direction: Vec3::new(1.0, 0.0, 0.0),
            gravity: 0.0,
            travel_time: 1.0,
        };
        let result = trajectory_box(traj, unit_box(Vec3::ZERO));
        assert!(result.hit);
        assert_eq!(result.time, 0.0);
    }

    #[test]
    fn trajectory_box_parabolic_arc_hits_floor() {
        // A pure vertical drop under gravity, falling from above the box:
        // the first (and, per PARITY face order, authoritative) crossing
        // is the box's **top** face (+z) — falling onto something hits
        // its top surface, not its bottom.
        let traj = Trajectory {
            start: Vec3::new(0.0, 0.0, 5.0),
            direction: Vec3::ZERO,
            gravity: 10.0,
            travel_time: 10.0,
        };
        let b = unit_box(Vec3::ZERO);
        let result = trajectory_box(traj, b);
        assert!(result.hit);
        assert_eq!(result.plane.axis, PlaneAxis::Z);
        assert!(equals(result.plane.center.z, 1.0));
        // Quadratic -5t^2 + 4 = 0 -> t = sqrt(4/5).
        assert!((result.time - (0.8f32).sqrt()).abs() < 1e-4);
    }

    #[test]
    fn rect_line_fixed_order_2d() {
        // PARITY: a line that passes fully through a symmetric box (both
        // entry through -x and exit through +x are, individually, valid
        // finite-segment crossings within [0, stop_time]) is answered by
        // whichever face FACE_ORDER tests first — +x — even though -x is
        // the geometrically-entered face. This is the same "first tested,
        // not earliest time" PARITY quirk as
        // `rect_line_first_tested_hit_wins_over_earliest_time`, arising
        // here from the simplest possible symmetric setup.
        let rect = Rect {
            center: Vec2::ZERO,
            size: Vec2::new(2.0, 2.0),
        };
        let line = Line2D {
            p1: Vec2::new(-5.0, 0.0),
            p2: Vec2::new(5.0, 0.0),
            stop_time: 1.0,
        };
        let result = rect_line(rect, line);
        assert!(result.hit);
        assert_eq!(result.plane.axis, PlaneAxis::X);
        assert!(equals(result.plane.center.x, 1.0));
        assert!(equals(result.time, 0.6));
    }

    #[test]
    fn point_rect_and_point_box() {
        let rect = Rect {
            center: Vec2::ZERO,
            size: Vec2::new(2.0, 2.0),
        };
        assert!(point_rect(Vec2::new(0.5, 0.5), rect));
        assert!(!point_rect(Vec2::new(2.0, 0.0), rect));

        let b = unit_box(Vec3::ZERO);
        assert!(point_box(Vec3::new(0.5, -0.5, 0.9), b));
        assert!(!point_box(Vec3::new(2.0, 0.0, 0.0), b));
    }

    #[test]
    fn solve_quadratic_picks_smallest_nonnegative_root() {
        // t^2 - 3t + 2 = 0 -> roots 1, 2
        let q = Quadratic {
            a: 1.0,
            b: -3.0,
            c: 2.0,
        };
        assert!(equals(solve_quadratic(q).unwrap(), 1.0));
    }

    #[test]
    fn solve_quadratic_negative_discriminant_is_none() {
        let q = Quadratic {
            a: 1.0,
            b: 0.0,
            c: 1.0,
        };
        assert!(solve_quadratic(q).is_none());
    }

    #[test]
    fn solve_quadratic_both_roots_negative_is_none() {
        // t^2 + 3t + 2 = 0 -> roots -1, -2
        let q = Quadratic {
            a: 1.0,
            b: 3.0,
            c: 2.0,
        };
        assert!(solve_quadratic(q).is_none());
    }

    #[test]
    fn moving_rect_moving_rect_head_on_collision() {
        let a = MovingRect {
            center1: Vec2::new(-5.0, 0.0),
            center2: Vec2::new(5.0, 0.0),
            size: Vec2::new(1.0, 1.0),
            stop_time: 1.0,
        };
        let b = MovingRect {
            center1: Vec2::new(5.0, 0.0),
            center2: Vec2::new(-5.0, 0.0),
            size: Vec2::new(1.0, 1.0),
            stop_time: 1.0,
        };
        let result = moving_rect_moving_rect(a, b);
        assert!(result.hit);
        // Combined half-extents = 0.5+0.5 = 1.0; initial separation = 10;
        // closing speed = 10+10 = 20 -> contact at (10-1.0)/20 = 0.45.
        assert!(equals(result.time, 0.45));
    }

    #[test]
    fn moving_rect_moving_rect_no_collision() {
        let a = MovingRect {
            center1: Vec2::new(-5.0, 0.0),
            center2: Vec2::new(-1.0, 0.0),
            size: Vec2::new(0.5, 0.5),
            stop_time: 1.0,
        };
        let b = MovingRect {
            center1: Vec2::new(5.0, 5.0),
            center2: Vec2::new(1.0, 5.0),
            size: Vec2::new(0.5, 0.5),
            stop_time: 1.0,
        };
        let result = moving_rect_moving_rect(a, b);
        assert!(!result.hit);
    }
}
