//! Center marker geometry, ported verbatim from `cCenter3D`
//! (`cCenter3D.cpp:54-122`, source-map §3.3).
//!
//! An 8-vertex unit marker — a floor square lifted to `z = fWallDist`
//! (0.001) plus one −X wall square inset by `fWallDist` — instanced per
//! center by `RotZ(orientation) * Scale(size) * Translate(position)`
//! (design §3.2 row-vector order, `cCenter3D.cpp:61-71`). The 0.001 local
//! lift becomes `0.001 * fSize` after the scale, so markers sit just above
//! the floor/wall and never z-fight. Per-team material color.
//!
//! The original draws two `D3DPT_TRIANGLESTRIP`s (verts 0-3, 4-7); those
//! strips are converted to indexed triangle lists here (design §3.5).

use std::rc::Rc;

use glam::Vec3;
use glow::HasContext;

use crate::error::GlError;
use crate::math::Rgba;
use crate::render::camera::FreeFlyCamera;
use crate::render::d3dx::Mat4;
use crate::render::gl::{GlState, WindingDecision};
use crate::render::mesh::{Mesh, VertexLit};
use crate::render::shaders::{self, Programs};

/// `fWallDist` (`cCenter3D.cpp:97`): the local z/x lift keeping the marker
/// off the floor/wall; scaled by `fSize` at instance time.
pub const WALL_DIST: f32 = 0.001;

fn vert(x: f32, y: f32, z: f32, n: [f32; 3]) -> VertexLit {
    VertexLit {
        position: [x, y, z],
        normal: n,
    }
}

/// The 8 marker vertices (`cCenter3D::CreateVertices`).
pub fn build_vertices() -> [VertexLit; 8] {
    let floor_n = [0.0, 0.0, 1.0];
    let wall_n = [-1.0, 0.0, 0.0];
    [
        // Floor square (z = fWallDist).
        vert(-0.5, -0.5, WALL_DIST, floor_n),
        vert(0.5, -0.5, WALL_DIST, floor_n),
        vert(-0.5, 0.5, WALL_DIST, floor_n),
        vert(0.5, 0.5, WALL_DIST, floor_n),
        // −X wall square (x = -0.5 + fWallDist).
        vert(-0.5 + WALL_DIST, -0.5, 0.0, wall_n),
        vert(-0.5 + WALL_DIST, 0.5, 0.0, wall_n),
        vert(-0.5 + WALL_DIST, -0.5, 1.0, wall_n),
        vert(-0.5 + WALL_DIST, 0.5, 1.0, wall_n),
    ]
}

/// Triangle-list indices for the two strips (0,1,2,3) and (4,5,6,7).
/// A strip `v0 v1 v2 v3` unwraps to `(v0,v1,v2)(v2,v1,v3)`.
pub fn build_indices() -> [u16; 12] {
    [0, 1, 2, 2, 1, 3, 4, 5, 6, 6, 5, 7]
}

/// The per-center world transform, `RotZ(orientation) * Scale(size) *
/// Translate(x, y, 0)` (row-vector order, `cCenter3D.cpp:61-71`). `size`
/// scales uniformly (`fSize` on all three axes).
pub fn world_matrix(orientation: f32, size: f32, position_x: f32, position_y: f32) -> Mat4 {
    Mat4::rotation_z(orientation)
        .mul(&Mat4::scaling(Vec3::new(size, size, size)))
        .mul(&Mat4::translation(Vec3::new(position_x, position_y, 0.0)))
}

/// Per-instance center inputs (`cCenter3D::tRenderData`).
#[derive(Copy, Clone, Debug)]
pub struct CenterRenderData {
    pub position_x: f32,
    pub position_y: f32,
    pub orientation: f32,
    pub size: f32,
    pub color: Rgba,
}

/// Owns the uploaded 8-vertex marker mesh and instances it with `prog_lit`.
/// Runtime-only.
pub struct CenterRenderer {
    mesh: Mesh<VertexLit>,
}

impl CenterRenderer {
    pub fn new(gl: Rc<glow::Context>) -> Result<Self, GlError> {
        let mut mesh = Mesh::new(build_vertices().to_vec(), build_indices().to_vec())?;
        mesh.upload(gl)?;
        Ok(Self { mesh })
    }

    /// Draws one center marker. Runtime-only.
    pub fn render(
        &self,
        gl_state: &mut GlState,
        programs: &Programs,
        camera: &FreeFlyCamera,
        aspect: f32,
        decision: WindingDecision,
        data: &CenterRenderData,
    ) {
        gl_state.set_winding(decision);
        let gl = Rc::clone(gl_state.context());
        let prog = &programs.lit;

        let world = world_matrix(data.orientation, data.size, data.position_x, data.position_y);
        let mvp = camera.mvp(&world, aspect);
        let world_view = camera.world_view(&world);

        unsafe {
            gl.use_program(Some(prog.program));
            if let Some(loc) = prog.uniform("u_mvp") {
                gl.uniform_matrix_4_f32_slice(Some(&loc), true, &mvp.to_row_major_array());
            }
            if let Some(loc) = prog.uniform("u_world_view") {
                gl.uniform_matrix_4_f32_slice(Some(&loc), true, &world_view.to_row_major_array());
            }
            if let Some(loc) = prog.uniform("u_material") {
                let m = shaders::material_uniform(data.color);
                gl.uniform_3_f32(Some(&loc), m[0], m[1], m[2]);
            }
            if let Some(loc) = prog.uniform("u_light_atten") {
                let a = shaders::light_atten_uniform();
                gl.uniform_3_f32(Some(&loc), a[0], a[1], a[2]);
            }
        }
        self.mesh.draw();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::equals;
    use crate::render::d3dx::D3DX_PI;

    #[test]
    fn eight_vertices_twelve_indices() {
        assert_eq!(build_vertices().len(), 8);
        assert_eq!(build_indices().len(), 12);
        assert!(build_indices().iter().all(|&i| i < 8));
    }

    #[test]
    fn floor_is_lifted_and_wall_is_inset_by_walldist() {
        let v = build_vertices();
        // Floor square lifted to +Z by fWallDist.
        for f in &v[0..4] {
            assert_eq!(f.position[2], WALL_DIST);
            assert_eq!(f.normal, [0.0, 0.0, 1.0]);
        }
        // −X wall inset by fWallDist, normal −X.
        for w in &v[4..8] {
            assert_eq!(w.position[0], -0.5 + WALL_DIST);
            assert_eq!(w.normal, [-1.0, 0.0, 0.0]);
        }
    }

    #[test]
    fn world_matrix_composes_rotz_scale_translate() {
        // orientation 0 => RotZ = identity; size 2 scales, then translate.
        let m = world_matrix(0.0, 2.0, 3.0, 4.0);
        // Local floor corner (0.5, 0.5, 0.001) -> scaled (1,1,0.002) -> +(3,4,0).
        let out = m.mul_row_vec4([0.5, 0.5, WALL_DIST, 1.0]);
        assert!(equals(out[0], 4.0));
        assert!(equals(out[1], 5.0));
        assert!(equals(out[2], WALL_DIST * 2.0)); // z-lift = 0.001 * fSize
    }

    #[test]
    fn world_matrix_rotation_then_scale_then_translate_order() {
        // 90° about Z: local +X maps toward +Y before the translation.
        let m = world_matrix(D3DX_PI * 0.5, 1.0, 10.0, 0.0);
        let out = m.mul_row_vec4([1.0, 0.0, 0.0, 1.0]);
        // RotZ(90): (1,0)->(0,1); +translate (10,0) => (10,1).
        assert!((out[0] - 10.0).abs() < 1e-5);
        assert!((out[1] - 1.0).abs() < 1e-5);
    }
}
