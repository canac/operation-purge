//! Procedural building geometry: wall extrusion, floor/ceiling, ported
//! verbatim from `cBuilding3D::CreateVerticesAndIndices`
//! (`cBuilding3D.cpp:100-289`, source-map §3.3).
//!
//! PARITY: the first (outer boundary) polygon gets **reversed winding**
//! (`cBuilding3D.cpp:187-193` — "For the first polygon, the vertices order
//! must be reversed to prevent them from being culled"); both wall faces
//! share one axis normal; the ceiling normal points *up* so it renders dark
//! — an intentional look, not a bug to fix. UV = span length / texture
//! size, so `GL_REPEAT` is mandatory.
//!
//! Buffer layout (`cBuilding3D.cpp`): `wallSegments*4 + 8` vertices —
//! wall quads first, then 4 floor + 4 ceiling verts; `wallSegments*6 + 12`
//! indices — wall tris, then 6 floor + 6 ceiling. The three draw ranges in
//! [`BuildingGeometry`] mirror the original's three `DrawIndexedPrimitive`
//! calls (`cBuilding3D.cpp:82-99`).

use crate::error::GlError;
use crate::maze::building::{Building, WallDir};
use crate::render::mesh::{Mesh, VertexLitTextured};

/// The extruded building geometry plus the three draw ranges (walls, floor,
/// ceiling) the renderer issues as separate texture-bound draws.
pub struct BuildingGeometry {
    pub vertices: Vec<VertexLitTextured>,
    pub indices: Vec<u16>,
    /// Index range `[0, wall_index_count)` — wall tris (wall texture).
    pub wall_index_count: usize,
    /// Index range `[floor_index_start, floor_index_start+6)` — 2 floor tris.
    pub floor_index_start: usize,
    /// Index range `[ceiling_index_start, ceiling_index_start+6)` — 2 ceiling tris.
    pub ceiling_index_start: usize,
}

/// Extrudes wall polylines + floor/ceiling into CPU-side vertex/index
/// buffers. Pure (no GL) — the headless correctness proxy for the M4 gate.
///
/// `*_texture_size` are the `[building]` `fWallTextureSize` etc.; UV =
/// span / texture_size (`cBuilding3D.cpp:126-127,215-217,257-259`).
pub fn build_geometry(
    building: &Building,
    wall_texture_size: f32,
    floor_texture_size: f32,
    ceiling_texture_size: f32,
) -> Result<BuildingGeometry, GlError> {
    let (polygons, wall_segments) = building.wall_polygons();

    let n_vertices = wall_segments * 4 + 8;
    // GUARD (design §1.3): u16 indices are kept; a building whose vertex
    // count would overflow u16 is a clean error, not silent corruption.
    if n_vertices > u16::MAX as usize + 1 {
        return Err(GlError::TooManyVertices { count: n_vertices });
    }

    let mut vertices: Vec<VertexLitTextured> = Vec::with_capacity(n_vertices);
    let mut indices: Vec<u16> = Vec::with_capacity(wall_segments * 6 + 12);

    let ceiling = building.ceiling_height;
    let wall_scale_x = 1.0 / wall_texture_size;
    let wall_scale_y = 1.0 / wall_texture_size;

    const WALL_X_NORMAL: [f32; 3] = [0.0, 1.0, 0.0];
    const WALL_Y_NORMAL: [f32; 3] = [1.0, 0.0, 0.0];
    const Z_NORMAL: [f32; 3] = [0.0, 0.0, 1.0];

    let mut first_polygon = true;
    for polygon in &polygons {
        // Iterate over segments: while itVertex != end()-1.
        for pair in polygon.windows(2) {
            let (v1, v2) = (pair[0], pair[1]);
            let p1 = building.wall_vertex_to_point(v1);
            let p2 = building.wall_vertex_to_point(v2);

            let (normal, wall_length) = match Building::wall_direction(v1, v2) {
                WallDir::X => (WALL_X_NORMAL, (p2.0 - p1.0).abs()),
                WallDir::Y => (WALL_Y_NORMAL, (p2.1 - p1.1).abs()),
                // Invalid direction: original logged and left length
                // uninitialized; use 0.0 so the (degenerate) quad is inert.
                WallDir::Invalid => (WALL_X_NORMAL, 0.0),
            };

            let base = vertices.len() as u16;
            // iVertex1..4 (cBuilding3D.cpp:164-181).
            vertices.push(VertexLitTextured {
                position: [p1.0, p1.1, 0.0],
                normal,
                uv: [0.0, 0.0],
            });
            vertices.push(VertexLitTextured {
                position: [p1.0, p1.1, ceiling],
                normal,
                uv: [0.0, ceiling * wall_scale_y],
            });
            vertices.push(VertexLitTextured {
                position: [p2.0, p2.1, 0.0],
                normal,
                uv: [wall_scale_x * wall_length, 0.0],
            });
            vertices.push(VertexLitTextured {
                position: [p2.0, p2.1, ceiling],
                normal,
                uv: [wall_scale_x * wall_length, ceiling * wall_scale_y],
            });

            let (i1, i2, i3, i4) = (base, base + 1, base + 2, base + 3);
            // PARITY (cBuilding3D.cpp:185-193): first polygon reversed.
            if first_polygon {
                indices.extend_from_slice(&[i1, i3, i2, i2, i3, i4]);
            } else {
                indices.extend_from_slice(&[i1, i2, i3, i2, i4, i3]);
            }
        }
        first_polygon = false;
    }

    let wall_index_count = indices.len();
    debug_assert_eq!(wall_index_count, wall_segments * 6);

    let size = building.size();

    // Floor (z = 0), normal +Z; UV repeat = size / floor_texture_size.
    let floor_index_start = indices.len();
    let floor_base = vertices.len() as u16; // == n_vertices - 8
    let frx = size.x / floor_texture_size;
    let fry = size.y / floor_texture_size;
    push_quad(
        &mut vertices,
        0.0,
        Z_NORMAL,
        (frx, fry),
        (size.x, size.y),
    );
    indices.extend_from_slice(&[
        floor_base,
        floor_base + 1,
        floor_base + 2,
        floor_base + 1,
        floor_base + 3,
        floor_base + 2,
    ]);

    // Ceiling (z = ceiling), normal +Z (points UP → dark, PARITY).
    let ceiling_index_start = indices.len();
    let ceil_base = vertices.len() as u16; // == n_vertices - 4
    let crx = size.x / ceiling_texture_size;
    let cry = size.y / ceiling_texture_size;
    push_quad(
        &mut vertices,
        ceiling,
        Z_NORMAL,
        (crx, cry),
        (size.x, size.y),
    );
    indices.extend_from_slice(&[
        ceil_base,
        ceil_base + 1,
        ceil_base + 2,
        ceil_base + 1,
        ceil_base + 3,
        ceil_base + 2,
    ]);

    debug_assert_eq!(vertices.len(), n_vertices);

    Ok(BuildingGeometry {
        vertices,
        indices,
        wall_index_count,
        floor_index_start,
        ceiling_index_start,
    })
}

/// Pushes the 4 corner verts of a floor/ceiling quad at height `z`
/// (`cBuilding3D.cpp:207-224` / `247-264`): corners (0,0),(sx,0),(0,sy),(sx,sy).
fn push_quad(
    vertices: &mut Vec<VertexLitTextured>,
    z: f32,
    normal: [f32; 3],
    repeat: (f32, f32),
    size: (f32, f32),
) {
    vertices.push(VertexLitTextured {
        position: [0.0, 0.0, z],
        normal,
        uv: [0.0, 0.0],
    });
    vertices.push(VertexLitTextured {
        position: [size.0, 0.0, z],
        normal,
        uv: [repeat.0, 0.0],
    });
    vertices.push(VertexLitTextured {
        position: [0.0, size.1, z],
        normal,
        uv: [0.0, repeat.1],
    });
    vertices.push(VertexLitTextured {
        position: [size.0, size.1, z],
        normal,
        uv: [repeat.0, repeat.1],
    });
}

/// Extrudes into one static `Mesh<VertexLitTextured>` for GL upload. The
/// draw ranges are recomputed by the caller via [`build_geometry`] when it
/// needs them for the three separate texture-bound draws.
pub fn build_mesh(
    building: &Building,
    wall_texture_size: f32,
    floor_texture_size: f32,
    ceiling_texture_size: f32,
) -> Result<Mesh<VertexLitTextured>, GlError> {
    let geo = build_geometry(
        building,
        wall_texture_size,
        floor_texture_size,
        ceiling_texture_size,
    )?;
    Mesh::new(geo.vertices, geo.indices)
}

// ---------------------------------------------------------------------------
// Runtime renderer (M4 winding gate) — compiled but exercised only at a
// real display; the headless suite covers the geometry above.
// ---------------------------------------------------------------------------

use std::rc::Rc;

use glow::HasContext;

use crate::render::camera::{DrawCategory, FreeFlyCamera, WindingGate};
use crate::render::gl::GlState;
use crate::render::shaders::{self, Programs};

/// Owns the uploaded building mesh + its three draw ranges + one texture per
/// draw category, and draws them with `prog_lit_tex` under the M4 debug
/// camera and winding gate. Runtime-only.
pub struct BuildingRenderer {
    mesh: Mesh<VertexLitTextured>,
    wall_index_count: usize,
    floor_index_start: usize,
    ceiling_index_start: usize,
    wall_tex: glow::Texture,
    floor_tex: glow::Texture,
    ceiling_tex: glow::Texture,
}

impl BuildingRenderer {
    /// Builds + uploads the mesh and takes the three textures (already
    /// uploaded; `glow::Texture` is a copyable handle). Runtime-only.
    pub fn new(
        gl: Rc<glow::Context>,
        building: &Building,
        wall_texture_size: f32,
        floor_texture_size: f32,
        ceiling_texture_size: f32,
        wall_tex: glow::Texture,
        floor_tex: glow::Texture,
        ceiling_tex: glow::Texture,
    ) -> Result<Self, GlError> {
        let geo = build_geometry(
            building,
            wall_texture_size,
            floor_texture_size,
            ceiling_texture_size,
        )?;
        let mut mesh = Mesh::new(geo.vertices, geo.indices)?;
        mesh.upload(gl)?;
        Ok(Self {
            mesh,
            wall_index_count: geo.wall_index_count,
            floor_index_start: geo.floor_index_start,
            ceiling_index_start: geo.ceiling_index_start,
            wall_tex,
            floor_tex,
            ceiling_tex,
        })
    }

    /// Draws the building with `prog_lit_tex`, applying the debug winding
    /// gate + category isolation. Runtime-only.
    pub fn render(
        &self,
        gl_state: &mut GlState,
        programs: &Programs,
        camera: &FreeFlyCamera,
        aspect: f32,
        gate: &WindingGate,
    ) {
        gl_state.set_winding(gate.decision());
        let gl = Rc::clone(gl_state.context());
        let prog = &programs.lit_textured;

        let view = camera.view();
        let mvp = camera.view_projection(aspect); // world = identity

        unsafe {
            gl.use_program(Some(prog.program));
            if let Some(loc) = prog.uniform("u_mvp") {
                gl.uniform_matrix_4_f32_slice(Some(&loc), false, &mvp.to_row_major_array());
            }
            if let Some(loc) = prog.uniform("u_world_view") {
                gl.uniform_matrix_4_f32_slice(Some(&loc), false, &view.to_row_major_array());
            }
            if let Some(loc) = prog.uniform("u_material") {
                // Building material is white (cBuilding3D.cpp:78 SetColor 0xFFFFFFFF).
                gl.uniform_3_f32(Some(&loc), 1.0, 1.0, 1.0);
            }
            if let Some(loc) = prog.uniform("u_light_atten") {
                let a = shaders::light_atten_uniform();
                gl.uniform_3_f32(Some(&loc), a[0], a[1], a[2]);
            }
            if let Some(loc) = prog.uniform("u_tex") {
                gl.uniform_1_i32(Some(&loc), 0);
            }
            gl.active_texture(glow::TEXTURE0);
        }

        if gate.should_draw(DrawCategory::Walls) {
            unsafe {
                gl.bind_texture(glow::TEXTURE_2D, Some(self.wall_tex));
            }
            self.mesh.draw_range(0, self.wall_index_count);
        }
        if gate.should_draw(DrawCategory::Floor) {
            unsafe {
                gl.bind_texture(glow::TEXTURE_2D, Some(self.floor_tex));
            }
            self.mesh.draw_range(self.floor_index_start, 6);
        }
        if gate.should_draw(DrawCategory::Ceiling) {
            unsafe {
                gl.bind_texture(glow::TEXTURE_2D, Some(self.ceiling_tex));
            }
            self.mesh.draw_range(self.ceiling_index_start, 6);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::maze::Maze;

    fn one_room_bordered() -> Building {
        Building {
            maze: Maze::new(1, 1, vec![true, true], vec![true, true]),
            wall_width: 1.0,
            room_size: 10.0,
            ceiling_height: 5.0,
        }
    }

    #[test]
    fn one_room_buffer_sizes_match_wall_segments_formula() {
        // texture sizes 1.0 => UV scale 1.0 (easy hand-computation).
        let geo = build_geometry(&one_room_bordered(), 1.0, 1.0, 1.0).unwrap();
        // 4 wall segments => 4*4+8 = 24 verts, 4*6+12 = 36 indices.
        assert_eq!(geo.vertices.len(), 24);
        assert_eq!(geo.indices.len(), 36);
        assert_eq!(geo.wall_index_count, 24);
        assert_eq!(geo.floor_index_start, 24);
        assert_eq!(geo.ceiling_index_start, 30);
    }

    #[test]
    fn first_wall_segment_positions_normal_and_uv_are_hand_computed() {
        let geo = build_geometry(&one_room_bordered(), 1.0, 1.0, 1.0).unwrap();
        // Segment 0: (0,0,BR)->(0,1,TR): world (0.5,0.5)->(0.5,9.5),
        // direction wallY => normal (1,0,0), length 9.0, ceiling 5.0.
        assert_eq!(geo.vertices[0].position, [0.5, 0.5, 0.0]);
        assert_eq!(geo.vertices[0].normal, [1.0, 0.0, 0.0]);
        assert_eq!(geo.vertices[0].uv, [0.0, 0.0]);
        assert_eq!(geo.vertices[1].position, [0.5, 0.5, 5.0]);
        assert_eq!(geo.vertices[1].uv, [0.0, 5.0]);
        assert_eq!(geo.vertices[2].position, [0.5, 9.5, 0.0]);
        assert_eq!(geo.vertices[2].uv, [9.0, 0.0]);
        assert_eq!(geo.vertices[3].position, [0.5, 9.5, 5.0]);
        assert_eq!(geo.vertices[3].uv, [9.0, 5.0]);
    }

    #[test]
    fn first_polygon_gets_reversed_winding() {
        let geo = build_geometry(&one_room_bordered(), 1.0, 1.0, 1.0).unwrap();
        // Reversed pattern for base 0: [i1,i3,i2, i2,i3,i4] = [0,2,1, 1,2,3].
        assert_eq!(&geo.indices[0..6], &[0, 2, 1, 1, 2, 3]);
    }

    #[test]
    fn floor_and_ceiling_quads_are_hand_computed() {
        let geo = build_geometry(&one_room_bordered(), 1.0, 1.0, 1.0).unwrap();
        // Floor: verts 16..20, z=0, normal +Z; building size = (10,10,5).
        assert_eq!(geo.vertices[16].position, [0.0, 0.0, 0.0]);
        assert_eq!(geo.vertices[16].normal, [0.0, 0.0, 1.0]);
        assert_eq!(geo.vertices[17].position, [10.0, 0.0, 0.0]);
        assert_eq!(geo.vertices[18].position, [0.0, 10.0, 0.0]);
        assert_eq!(geo.vertices[19].position, [10.0, 10.0, 0.0]);
        assert_eq!(&geo.indices[24..30], &[16, 17, 18, 17, 19, 18]);

        // Ceiling: verts 20..24, z=5, normal +Z (points up -> dark, PARITY).
        assert_eq!(geo.vertices[20].position, [0.0, 0.0, 5.0]);
        assert_eq!(geo.vertices[20].normal, [0.0, 0.0, 1.0]);
        assert_eq!(geo.vertices[23].position, [10.0, 10.0, 5.0]);
        assert_eq!(&geo.indices[30..36], &[20, 21, 22, 21, 23, 22]);
    }

    #[test]
    fn uv_uses_texture_size_divisor() {
        // wall texture size 4.0 => scale 0.25; ceiling height 5 => v = 1.25.
        let geo = build_geometry(&one_room_bordered(), 4.0, 2.0, 8.0).unwrap();
        assert_eq!(geo.vertices[1].uv, [0.0, 1.25]);
        // wall length 9.0 * 0.25 = 2.25.
        assert_eq!(geo.vertices[2].uv, [2.25, 0.0]);
        // Floor repeat = size 10 / 2.0 = 5.0.
        assert_eq!(geo.vertices[19].uv, [5.0, 5.0]);
        // Ceiling repeat = size 10 / 8.0 = 1.25.
        assert_eq!(geo.vertices[23].uv, [1.25, 1.25]);
    }

    #[test]
    fn build_mesh_wraps_into_a_u16_mesh() {
        let mesh = build_mesh(&one_room_bordered(), 1.0, 1.0, 1.0).unwrap();
        assert_eq!(mesh.vertices.len(), 24);
        assert_eq!(mesh.indices.len(), 36);
    }
}
