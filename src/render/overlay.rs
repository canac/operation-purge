//! 2D map + robot/center markers + crosshair + health-bar layout, replacing
//! `cOverlay2D`'s geometry half (`cOverlay2D.cpp`, source-map §3.2) plus the
//! `Render2D` viewport/world-matrix setup (`cOperationPurge.cpp:2690-2760`).
//! Reused by the New Game screen, modals, and profiler pane at M8/M11
//! (design §3.6 "one render path, no second UI stack").
//!
//! All geometry is **NDC-direct**: the flat program's MVP is the 2D world
//! matrix (or identity for the crosshair), exactly like the original's
//! identity projection path. PARITY: viewport offset 0.025 / size 0.95 (2D
//! mode), 0.05 / 0.9 (minimap); the map world matrix's **negative Y scale**
//! is the inverted-Y arrow pan (down = Up); crosshair = 4 segments
//! ±0.2 → ±0.05 NDC; health bars 1.0×0.1, green (0,1,0,1) on red (1,0,0,1);
//! 2D barrel radius = 3D×2, length = 3D + bodyRadius
//! (`cOperationPurge.cpp:2200-2203`).

use std::rc::Rc;

use glam::{Vec2, Vec3};
use glow::HasContext;

use crate::data::builtin::Robot3DDimensions;
use crate::error::GlError;
use crate::maze::building::Building;
use crate::math::Rgba;
use crate::render::d3dx::Mat4;
use crate::render::gl::{GlState, WindingDecision};
use crate::render::mesh::{Mesh, VertexPos};
use crate::render::shaders::{Program, Programs};

/// 2D map viewport (`m_fBuilding2DOffset`/`m_fBuilding2DSize`,
/// `cOperationPurge.cpp:74-75`).
pub const MAP_VIEWPORT_OFFSET: f32 = 0.025;
pub const MAP_VIEWPORT_SIZE: f32 = 0.95;
/// Minimap viewport (`cOperationPurge.cpp:2709-2710`).
pub const MINIMAP_VIEWPORT_OFFSET: f32 = 0.05;
pub const MINIMAP_VIEWPORT_SIZE: f32 = 0.9;

/// Which 2D presentation is active (`m_b2DMode`): the full-screen map or
/// the in-game minimap.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum MapMode {
    Full2D,
    Minimap,
}

/// Screen-space (NDC) offset + size of the map region for a mode
/// (`cOperationPurge.cpp:2700-2712`). Full-2D maps `offset`/`size` through
/// `2*x - 1` / `2*x`; the minimap uses raw `0.05` / `0.9`.
pub fn viewport(mode: MapMode) -> (Vec2, Vec2) {
    match mode {
        MapMode::Full2D => (
            Vec2::splat(MAP_VIEWPORT_OFFSET * 2.0 - 1.0),
            Vec2::splat(MAP_VIEWPORT_SIZE * 2.0),
        ),
        MapMode::Minimap => (
            Vec2::splat(MINIMAP_VIEWPORT_OFFSET),
            Vec2::splat(MINIMAP_VIEWPORT_SIZE),
        ),
    }
}

/// The 2D map world matrix (`cOperationPurge.cpp:2742-2750`):
/// `Translate(-visOffset*room) * Scale(screen/(visBuilding*room), with a
/// NEGATIVE Y) * Translate(screenOffset.x, -screenOffset.y)`. The negative
/// Y scale + negated screen-Y offset flip the map vertically — the PARITY
/// inverted-Y arrow-pan behavior (down = Up).
///
/// `vis_offset`/`vis_size` are in **room units**; `screen_offset`/
/// `screen_size` are NDC (from [`viewport`]).
pub fn map_world_matrix(
    room_size: f32,
    vis_offset: Vec2,
    vis_size: Vec2,
    screen_offset: Vec2,
    screen_size: Vec2,
) -> Mat4 {
    let t1 = Mat4::translation(Vec3::new(
        -vis_offset.x * room_size,
        -vis_offset.y * room_size,
        0.0,
    ));
    let scale = Mat4::scaling(Vec3::new(
        screen_size.x / (vis_size.x * room_size),
        -screen_size.y / (vis_size.y * room_size),
        1.0,
    ));
    let t3 = Mat4::translation(Vec3::new(screen_offset.x, -screen_offset.y, 0.0));
    t1.mul(&scale).mul(&t3)
}

/// The 2D robot-marker dimensions derived from the 3D ones
/// (`cOperationPurge.cpp:2200-2203`): body radius unchanged, barrel radius
/// doubled, barrel length extended by the body radius.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Robot2DDimensions {
    pub body_radius: f32,
    pub barrel_radius: f32,
    pub barrel_length: f32,
}

impl Robot2DDimensions {
    pub fn from_3d(d: &Robot3DDimensions) -> Self {
        Self {
            body_radius: d.body_radius,
            barrel_radius: d.barrel_radius * 2.0,
            barrel_length: d.barrel_length + d.body_radius,
        }
    }
}

/// The 8 robot-marker vertices (`CalcRobotVertices`, `cOverlay2D.cpp:364-383`):
/// 4 body-square verts then 4 barrel verts (each set a `TRIANGLESTRIP`).
pub fn robot_marker_vertices(d: &Robot2DDimensions) -> [VertexPos; 8] {
    let r = d.body_radius;
    let br = d.barrel_radius;
    let bl = d.barrel_length;
    let p = |x: f32, y: f32| VertexPos {
        position: [x, y, 0.0],
    };
    [
        // Body square.
        p(r, r),
        p(-r, r),
        p(r, -r),
        p(-r, -r),
        // Barrel (points +X, out the front).
        p(r, br),
        p(bl, br),
        p(r, -br),
        p(bl, -br),
    ]
}

/// The 8 crosshair vertices — 4 line segments ±0.2 → ±0.05 NDC
/// (`CalcCrosshairVertices`, `cOverlay2D.cpp:387-402`). Drawn as a
/// `LINELIST` (pairs), MVP = identity.
pub fn crosshair_vertices() -> [VertexPos; 8] {
    let p = |x: f32, y: f32| VertexPos {
        position: [x, y, 0.0],
    };
    [
        p(0.2, 0.0),
        p(0.05, 0.0),
        p(-0.2, 0.0),
        p(-0.05, 0.0),
        p(0.0, 0.2),
        p(0.0, 0.05),
        p(0.0, -0.2),
        p(0.0, -0.05),
    ]
}

/// A health-bar segment: horizontal offset from the robot center, width,
/// and color (all in the sprite's local 1.0×0.1 space).
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct HealthSegment {
    pub offset_x: f32,
    pub width: f32,
    pub color: Rgba,
}

/// Full-width / full-height of a health bar (`size2Size(1.0, 0.1)`,
/// `cOperationPurge.cpp:2975`).
pub const HEALTH_BAR_WIDTH: f32 = 1.0;
pub const HEALTH_BAR_HEIGHT: f32 = 0.1;

/// The green (remaining) + red (lost) health-bar segments for a health
/// fraction (`cOperationPurge.cpp:2987-2995`). Green is left-anchored, red
/// fills the rest; both centered so the pair spans the full 1.0 width.
pub fn health_bar_segments(fraction: f32) -> [HealthSegment; 2] {
    let f = fraction;
    [
        HealthSegment {
            offset_x: (f - 1.0) * HEALTH_BAR_WIDTH * 0.5,
            width: HEALTH_BAR_WIDTH * f,
            color: Rgba::new(0.0, 1.0, 0.0, 1.0),
        },
        HealthSegment {
            offset_x: f * HEALTH_BAR_WIDTH * 0.5,
            width: HEALTH_BAR_WIDTH * (1.0 - f),
            color: Rgba::new(1.0, 0.0, 0.0, 1.0),
        },
    ]
}

/// 2D building geometry: wall vertices + a `LINELIST` index buffer tracing
/// each wall polygon, plus a 4-vertex background quad
/// (`CalcBuildingVerticesAndIndices`, `cOverlay2D.cpp:281-345`).
pub struct MapGeometry {
    pub vertices: Vec<VertexPos>,
    /// Line-list indices (pairs) for the wall outline.
    pub wall_line_indices: Vec<u16>,
    /// Index of the first of the 4 background-quad vertices.
    pub background_first_vertex: usize,
    /// Triangle-list indices (2 tris) for the background quad.
    pub background_indices: [u16; 6],
}

/// Builds the 2D map geometry from the traced wall polygons. Each polygon
/// contributes one vertex per segment; the last segment's second index
/// wraps back to the polygon's first vertex (closing the loop), matching
/// the original exactly.
pub fn build_map_geometry(building: &Building) -> MapGeometry {
    let (polygons, _wall_segments) = building.wall_polygons();
    let mut vertices: Vec<VertexPos> = Vec::new();
    let mut wall_line_indices: Vec<u16> = Vec::new();

    for polygon in &polygons {
        let first_vertex = vertices.len() as u16;
        // The polygon repeats its start vertex at the end; iterate segments.
        for pair in polygon.windows(2) {
            let (x, y) = building.wall_vertex_to_point(pair[0]);
            let i = vertices.len() as u16;
            vertices.push(VertexPos {
                position: [x, y, 0.0],
            });
            wall_line_indices.push(i);
            wall_line_indices.push(i + 1);
        }
        // The last line's second endpoint references the polygon's start.
        if let Some(last) = wall_line_indices.last_mut() {
            *last = first_vertex;
        }
    }

    // Background quad spanning the whole building.
    let size = building.size();
    let background_first_vertex = vertices.len();
    let b0 = background_first_vertex as u16;
    vertices.push(VertexPos { position: [0.0, 0.0, 0.0] });
    vertices.push(VertexPos { position: [size.x, 0.0, 0.0] });
    vertices.push(VertexPos { position: [0.0, size.y, 0.0] });
    vertices.push(VertexPos { position: [size.x, size.y, 0.0] });
    let background_indices = [b0, b0 + 1, b0 + 2, b0 + 1, b0 + 3, b0 + 2];

    MapGeometry {
        vertices,
        wall_line_indices,
        background_first_vertex,
        background_indices,
    }
}

/// The 2D map's single index buffer + draw ranges: the wall outline (a
/// `LINES` list, `[0, wall_line_count)`) followed by the background quad
/// (`TRIANGLES`, `[background_start, background_start + 6)`). Flattens
/// [`build_map_geometry`] so one [`Mesh`] draws both primitives.
pub struct MapMesh {
    pub vertices: Vec<VertexPos>,
    pub indices: Vec<u16>,
    pub wall_line_count: usize,
    pub background_start: usize,
}

/// Builds the combined 2D-map mesh (walls + background quad) for a building.
pub fn build_map_mesh(building: &Building) -> MapMesh {
    let geo = build_map_geometry(building);
    let wall_line_count = geo.wall_line_indices.len();
    let mut indices = geo.wall_line_indices;
    let background_start = indices.len();
    indices.extend_from_slice(&geo.background_indices);
    MapMesh {
        vertices: geo.vertices,
        indices,
        wall_line_count,
        background_start,
    }
}

/// The 12 triangle-list indices for the robot marker: the body square (verts
/// 0-3) and the barrel (verts 4-7), each a `TRIANGLESTRIP` unwrapped to two
/// triangles. Body = `[0, 6)`, barrel = `[6, 12)` — drawn in separate colors.
pub fn robot_marker_indices() -> [u16; 12] {
    [0, 1, 2, 2, 1, 3, 4, 5, 6, 6, 5, 7]
}

/// The 8 `LINES` indices (4 segments) for the crosshair.
pub fn crosshair_line_indices() -> [u16; 8] {
    [0, 1, 2, 3, 4, 5, 6, 7]
}

/// A centered unit quad (`x, y ∈ [-0.5, 0.5]`) reused for every health-bar
/// segment; scaled + placed per segment by [`health_bar_segment_matrix`].
pub fn health_bar_quad_vertices() -> [VertexPos; 4] {
    let p = |x: f32, y: f32| VertexPos {
        position: [x, y, 0.0],
    };
    [p(-0.5, -0.5), p(0.5, -0.5), p(-0.5, 0.5), p(0.5, 0.5)]
}

/// The two-triangle indices for [`health_bar_quad_vertices`].
pub fn health_bar_quad_indices() -> [u16; 6] {
    [0, 1, 2, 2, 1, 3]
}

/// World matrix placing the robot marker (which points +X in local space)
/// at the robot's position + heading: `RotZ(direction) * Translate(pos)`
/// (row-vector order, matching [`super::center3d::world_matrix`]).
pub fn robot_marker_matrix(direction: f32, pos: Vec2) -> Mat4 {
    Mat4::rotation_z(direction).mul(&Mat4::translation(Vec3::new(pos.x, pos.y, 0.0)))
}

/// PARITY: `cOverlay2D::StartRender` visibility bounds — whole rooms fully
/// inside the visible window (`ceil(offset)` .. `floor(offset + size)`,
/// `cOverlay2D.cpp:103-106`); robots/centers outside are skipped.
pub fn marker_visible(pos: Vec2, room_size: f32, vis_offset: Vec2, vis_size: Vec2) -> bool {
    let lo = Vec2::new(vis_offset.x.ceil(), vis_offset.y.ceil());
    let hi = Vec2::new((vis_offset.x + vis_size.x).floor(), (vis_offset.y + vis_size.y).floor());
    let p = pos / room_size;
    // `Between` accepts either bound order.
    let between = |v: f32, a: f32, b: f32| (v >= a && v <= b) || (v >= b && v <= a);
    between(p.x, lo.x, hi.x) && between(p.y, lo.y, hi.y)
}

/// The map colors pulled from `[colors]` (`background_2d`, `wall_2d`,
/// `barrel_2d`). The per-robot body color (team, or `observer_2d` for the
/// active robot) travels on [`MapRobotData`].
#[derive(Copy, Clone, Debug)]
pub struct MapColors {
    pub background: Rgba,
    pub wall: Rgba,
    pub barrel: Rgba,
}

/// Per-robot inputs for the 2D map: pose and the resolved body-marker color
/// (team color, or the observer tint for the active robot / observer point).
#[derive(Copy, Clone, Debug)]
pub struct MapRobotData {
    pub position: Vec2,
    pub direction: f32,
    pub marker_color: Rgba,
}

/// Per-center inputs for the 2D map (`cCenter::Render2D`).
#[derive(Copy, Clone, Debug)]
pub struct MapCenterData {
    pub position: Vec2,
    pub size: f32,
    pub color: Rgba,
}

/// The visible map window in room units (`pt2VisibleBuildingOffset` /
/// `size2VisibleBuilding`, `cOperationPurge.cpp:2733-2737`).
#[derive(Copy, Clone, Debug)]
pub struct MapView {
    pub offset: Vec2,
    pub size: Vec2,
}

/// Owns the uploaded 2D-map meshes and draws the full-screen map, the
/// in-game minimap, and the 3D crosshair with `prog_flat`. The map mesh is
/// per-building (rebuilt when a new game starts); the marker / crosshair /
/// center meshes are static. Runtime-only (needs a current GL context).
pub struct Overlay2DRenderer {
    map: Mesh<VertexPos>,
    map_wall_line_count: usize,
    map_background_start: usize,
    marker: Mesh<VertexPos>,
    crosshair: Mesh<VertexPos>,
    unit_quad: Mesh<VertexPos>,
    room_size: f32,
}

impl Overlay2DRenderer {
    pub fn new(
        gl: Rc<glow::Context>,
        building: &Building,
        dims: &Robot3DDimensions,
    ) -> Result<Self, GlError> {
        let mm = build_map_mesh(building);
        let mut map = Mesh::new(mm.vertices, mm.indices)?;
        map.upload(gl.clone())?;

        let marker_dims = Robot2DDimensions::from_3d(dims);
        let mut marker = Mesh::new(
            robot_marker_vertices(&marker_dims).to_vec(),
            robot_marker_indices().to_vec(),
        )?;
        marker.upload(gl.clone())?;

        let mut crosshair = Mesh::new(
            crosshair_vertices().to_vec(),
            crosshair_line_indices().to_vec(),
        )?;
        crosshair.upload(gl.clone())?;

        let mut unit_quad = Mesh::new(
            health_bar_quad_vertices().to_vec(),
            health_bar_quad_indices().to_vec(),
        )?;
        unit_quad.upload(gl)?;

        Ok(Self {
            map,
            map_wall_line_count: mm.wall_line_count,
            map_background_start: mm.background_start,
            marker,
            crosshair,
            unit_quad,
            room_size: building.room_size,
        })
    }

    /// Sets `prog_flat`'s `u_mvp` + `u_color` uniforms for one draw.
    fn set_flat(gl: &glow::Context, prog: &Program, mvp: &Mat4, color: Rgba) {
        unsafe {
            gl.use_program(Some(prog.program));
            if let Some(loc) = prog.uniform("u_mvp") {
                gl.uniform_matrix_4_f32_slice(Some(&loc), false, &mvp.to_row_major_array());
            }
            if let Some(loc) = prog.uniform("u_color") {
                gl.uniform_4_f32(Some(&loc), color.r, color.g, color.b, color.a);
            }
        }
    }

    /// PARITY: the building half of `Render2D` (`cOperationPurge.cpp:2762-2835`):
    /// background (opaque in [`MapMode::Full2D`]), wall outline, centers,
    /// then robot markers, each culled to the visible window.
    #[allow(clippy::too_many_arguments)]
    pub fn render_map(
        &self,
        gl_state: &mut GlState,
        programs: &Programs,
        mode: MapMode,
        view: MapView,
        centers: &[MapCenterData],
        robots: &[MapRobotData],
        colors: MapColors,
    ) {
        gl_state.set_winding(WindingDecision::CullNone);
        let gl = Rc::clone(gl_state.context());
        let prog = &programs.flat;

        let (screen_off, screen_size) = viewport(mode);
        let world = map_world_matrix(self.room_size, view.offset, view.size, screen_off, screen_size);
        let visible = |p: Vec2| marker_visible(p, self.room_size, view.offset, view.size);

        // D3D's identity-projection z=0 is the near plane; GL's is mid-depth,
        // so drop scene depth to keep the map on top of the 3D view.
        unsafe { gl.clear(glow::DEPTH_BUFFER_BIT) };

        let mut background = colors.background;
        if mode == MapMode::Full2D {
            background.a = 1.0;
        }
        Self::set_flat(&gl, prog, &world, background);
        self.map.draw_range(self.map_background_start, 6);
        Self::set_flat(&gl, prog, &world, colors.wall);
        self.map.draw_lines_range(0, self.map_wall_line_count);

        for c in centers.iter().filter(|c| visible(c.position)) {
            let mvp = Mat4::scaling(Vec3::splat(c.size))
                .mul(&Mat4::translation(Vec3::new(c.position.x, c.position.y, 0.0)))
                .mul(&world);
            Self::set_flat(&gl, prog, &mvp, c.color);
            self.unit_quad.draw();
        }

        for r in robots.iter().filter(|r| visible(r.position)) {
            let marker_mvp = robot_marker_matrix(r.direction, r.position).mul(&world);
            Self::set_flat(&gl, prog, &marker_mvp, r.marker_color);
            self.marker.draw_range(0, 6); // body square
            Self::set_flat(&gl, prog, &marker_mvp, colors.barrel);
            self.marker.draw_range(6, 6); // barrel
        }
    }

    /// PARITY: `cOverlay2D::RenderCrosshair` — screen-centered, 3D mode only.
    pub fn render_crosshair(&self, gl_state: &mut GlState, programs: &Programs, color: Rgba) {
        gl_state.set_winding(WindingDecision::CullNone);
        let gl = Rc::clone(gl_state.context());
        let identity = Mat4::scaling(Vec3::ONE);
        unsafe { gl.clear(glow::DEPTH_BUFFER_BIT) };
        Self::set_flat(&gl, &programs.flat, &identity, color);
        self.crosshair.draw_lines_range(0, 8);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::builtin::OBJECTS;
    use crate::maze::Maze;
    use crate::math::equals;

    fn one_room() -> Building {
        Building {
            maze: Maze::new(1, 1, vec![true, true], vec![true, true]),
            wall_width: 1.0,
            room_size: 10.0,
            ceiling_height: 5.0,
        }
    }

    #[test]
    fn viewport_rects_match_the_source_constants() {
        let (o, s) = viewport(MapMode::Full2D);
        // 2*0.025 - 1 = -0.95 ; 2*0.95 = 1.9.
        assert!(equals(o.x, -0.95) && equals(o.y, -0.95));
        assert!(equals(s.x, 1.9) && equals(s.y, 1.9));

        let (o, s) = viewport(MapMode::Minimap);
        assert!(equals(o.x, 0.05) && equals(o.y, 0.05));
        assert!(equals(s.x, 0.9) && equals(s.y, 0.9));
    }

    #[test]
    fn map_world_matrix_flips_y_inverted_pan() {
        let room = 10.0;
        let (screen_off, screen_size) = viewport(MapMode::Full2D);
        // One visible room, offset origin.
        let m = map_world_matrix(
            room,
            Vec2::ZERO,
            Vec2::ONE,
            screen_off,
            screen_size,
        );
        // World (0,0) -> NDC top: y ≈ +0.95.
        let top = m.mul_row_vec4([0.0, 0.0, 0.0, 1.0]);
        assert!((top[1] - 0.95).abs() < 1e-5, "top y {}", top[1]);
        // World (0, room) -> NDC bottom: y ≈ -0.95 (increasing world y goes
        // DOWN on screen — the inverted-Y PARITY).
        let bottom = m.mul_row_vec4([0.0, room, 0.0, 1.0]);
        assert!((bottom[1] + 0.95).abs() < 1e-5, "bottom y {}", bottom[1]);
        assert!(bottom[1] < top[1], "world +Y must map DOWN");
    }

    #[test]
    fn robot_2d_dimensions_transform_from_3d() {
        let d = Robot2DDimensions::from_3d(&OBJECTS);
        assert!(equals(d.body_radius, OBJECTS.body_radius)); // 0.5
        assert!(equals(d.barrel_radius, OBJECTS.barrel_radius * 2.0)); // 0.16
        assert!(equals(
            d.barrel_length,
            OBJECTS.barrel_length + OBJECTS.body_radius
        )); // 1.3
    }

    #[test]
    fn robot_marker_vertices_use_the_2d_dimensions() {
        let d = Robot2DDimensions::from_3d(&OBJECTS);
        let v = robot_marker_vertices(&d);
        // Body square corner at (+r,+r).
        assert_eq!(v[0].position, [d.body_radius, d.body_radius, 0.0]);
        // Barrel tip at (barrel_length, barrel_radius).
        assert_eq!(v[5].position, [d.barrel_length, d.barrel_radius, 0.0]);
        assert_eq!(v[7].position, [d.barrel_length, -d.barrel_radius, 0.0]);
    }

    #[test]
    fn crosshair_segments_span_0_2_to_0_05() {
        let v = crosshair_vertices();
        assert_eq!(v[0].position, [0.2, 0.0, 0.0]);
        assert_eq!(v[1].position, [0.05, 0.0, 0.0]);
        assert_eq!(v[6].position, [0.0, -0.2, 0.0]);
        assert_eq!(v[7].position, [0.0, -0.05, 0.0]);
    }

    #[test]
    fn health_bar_segments_split_green_and_red() {
        let [green, red] = health_bar_segments(0.25);
        // Green = 25% width, left-anchored; red = 75%, to its right.
        assert!(equals(green.width, 0.25));
        assert!(equals(green.offset_x, (0.25 - 1.0) * 0.5)); // -0.375
        assert_eq!(green.color, Rgba::new(0.0, 1.0, 0.0, 1.0));
        assert!(equals(red.width, 0.75));
        assert!(equals(red.offset_x, 0.25 * 0.5)); // 0.125
        assert_eq!(red.color, Rgba::new(1.0, 0.0, 0.0, 1.0));
    }

    #[test]
    fn health_bar_full_and_empty_bounds() {
        let [g, r] = health_bar_segments(1.0);
        assert!(equals(g.width, 1.0) && equals(r.width, 0.0));
        let [g, r] = health_bar_segments(0.0);
        assert!(equals(g.width, 0.0) && equals(r.width, 1.0));
    }

    #[test]
    fn one_room_map_has_four_wall_verts_and_a_closed_line_loop() {
        let geo = build_map_geometry(&one_room());
        // 4 wall segments (square) + 4 background verts.
        assert_eq!(geo.vertices.len(), 8);
        assert_eq!(geo.background_first_vertex, 4);
        // 4 line segments => 8 line indices; last wraps to the first vertex.
        assert_eq!(geo.wall_line_indices.len(), 8);
        assert_eq!(*geo.wall_line_indices.last().unwrap(), 0);
        // Background quad indices reference verts 4..8.
        assert_eq!(geo.background_indices, [4, 5, 6, 5, 7, 6]);
    }

    #[test]
    fn build_map_mesh_concatenates_walls_then_background() {
        let mm = build_map_mesh(&one_room());
        // 8 wall-line indices, then the 6 background-quad indices.
        assert_eq!(mm.wall_line_count, 8);
        assert_eq!(mm.background_start, 8);
        assert_eq!(mm.indices.len(), 14);
        assert_eq!(&mm.indices[8..14], &[4, 5, 6, 5, 7, 6]);
        assert_eq!(mm.vertices.len(), 8);
    }

    #[test]
    fn robot_marker_indices_are_two_strips_within_eight_verts() {
        let idx = robot_marker_indices();
        assert_eq!(idx.len(), 12);
        assert!(idx.iter().all(|&i| i < 8));
        // Body strip [0,6), barrel strip [6,12).
        assert_eq!(&idx[0..6], &[0, 1, 2, 2, 1, 3]);
        assert_eq!(&idx[6..12], &[4, 5, 6, 6, 5, 7]);
    }

    #[test]
    fn crosshair_line_indices_are_four_segments() {
        assert_eq!(crosshair_line_indices(), [0, 1, 2, 3, 4, 5, 6, 7]);
    }

    #[test]
    fn robot_marker_matrix_rotates_then_translates() {
        // Heading 0: local +X point lands at pos + (len, 0).
        let m = robot_marker_matrix(0.0, Vec2::new(2.0, 3.0));
        let out = m.mul_row_vec4([1.0, 0.0, 0.0, 1.0]);
        assert!(equals(out[0], 3.0) && equals(out[1], 3.0));
        // Heading 90°: local +X point swings toward +Y.
        let m = robot_marker_matrix(crate::render::d3dx::D3DX_PI * 0.5, Vec2::new(2.0, 3.0));
        let out = m.mul_row_vec4([1.0, 0.0, 0.0, 1.0]);
        assert!((out[0] - 2.0).abs() < 1e-5 && (out[1] - 4.0).abs() < 1e-5);
    }

    #[test]
    fn marker_visible_uses_whole_rooms_inside_the_window() {
        let (off, size) = (Vec2::new(0.5, 0.0), Vec2::new(2.0, 2.0));
        assert!(marker_visible(Vec2::new(10.0, 10.0), 10.0, off, size));
        assert!(!marker_visible(Vec2::new(5.0, 10.0), 10.0, off, size));
        assert!(!marker_visible(Vec2::new(25.0, 10.0), 10.0, off, size));
    }
}
