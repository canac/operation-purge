//! Procedural robot geometry: dome (body) + barrel + treads, ported
//! verbatim from `cRobot3D` (`cRobot3D.cpp:117-516`, source-map §3.3).
//!
//! PARITY: dome/barrel/tread normals are the **raw (un-normalized)
//! scaled positions** — `D3DRS_NORMALIZENORMALS` is never set, so a dome
//! normal of length ~0.5 and a barrel normal of length ~0.08 intentionally
//! dim the FFP-Gouraud lighting (the 2011 look; `prog_lit` never
//! normalizes either, see `render::shaders`). The barrel cap is a
//! `D3DPT_TRIANGLEFAN` (`cRobot3D.cpp:485`) converted to an indexed
//! triangle list at build time (design §1.5.13). The second tread is a
//! byte-copy of the first, shifted ±Y (`cRobot3D.cpp:379-391`,
//! `447-450`).
//!
//! Each part is emitted as its own [`Mesh`] so the renderer can set the
//! per-part material color (`RenderBody`/`RenderBarrel`/`RenderTreads`
//! each call `SetColor`) with one `u_material` uniform per draw. Vertex
//! positions/normals/winding are byte-for-byte the original's.

use std::rc::Rc;

use glam::Vec3;
use glow::HasContext;

use crate::data::builtin::Robot3DDimensions;
use crate::error::GlError;
use crate::math::{sphere_coord, Rgba};
use crate::render::camera::FreeFlyCamera;
use crate::render::d3dx::{Mat4, D3DX_PI};
use crate::render::gl::{GlState, WindingDecision};
use crate::render::mesh::{triangle_fan_to_list, Mesh, VertexLit};
use crate::render::shaders::{self, Programs};

fn vert(position: Vec3, normal: Vec3) -> VertexLit {
    VertexLit {
        position: [position.x, position.y, position.z],
        normal: [normal.x, normal.y, normal.z],
    }
}

/// The three procedural robot part meshes, CPU-side (not yet uploaded).
/// Draw order and per-part material colors mirror `cRobot3D::Render`.
pub struct RobotGeometry {
    pub body: (Vec<VertexLit>, Vec<u16>),
    pub barrel: (Vec<VertexLit>, Vec<u16>),
    pub treads: (Vec<VertexLit>, Vec<u16>),
}

/// Body (dome) vertices + indices (`CalcBodyVertices`/`CalcBodyIndices`,
/// `cRobot3D.cpp:166-225`). `nBody = res*res/2` verts, `res*res*3` indices.
pub fn build_body(dims: &Robot3DDimensions) -> (Vec<VertexLit>, Vec<u16>) {
    let res = dims.body_resolution;
    let radius = dims.body_radius; // m_fRadius = fBodyRadius
    let offset = Vec3::new(0.0, 0.0, dims.tread_radius * 2.0);

    let ang_theta_size = D3DX_PI * 2.0 / res as f32;
    let ang_phi_size = D3DX_PI * 0.5 / (res / 2 - 1) as f32;

    let mut verts = Vec::with_capacity((res * res / 2) as usize);
    for ul_phi in 0..res / 2 {
        for ul_theta in 0..res {
            let ang_theta = ul_theta as f32 * ang_theta_size;
            let ang_phi = ul_phi as f32 * ang_phi_size;
            let mut coord = sphere_coord(ang_theta, ang_phi, radius);
            // PARITY: z compressed by fBodyHeight/fRadius (cRobot3D.cpp:188).
            coord.z = coord.z / radius * dims.body_height;
            // PARITY: normal = the (un-normalized) scaled position.
            verts.push(vert(coord + offset, coord));
        }
    }

    let body_index = |theta: u32, phi: u32| -> u16 { (theta + phi * res) as u16 };
    let mut indices = Vec::with_capacity((res * res * 3) as usize);
    for ul_phi in 0..res / 2 {
        for ul_theta in 0..res {
            let next_theta = (ul_theta + 1) % res;
            let next_phi = (ul_phi + 1) % (res / 2);
            let i1 = body_index(ul_theta, ul_phi);
            let i2 = body_index(ul_theta, next_phi);
            let i3 = body_index(next_theta, ul_phi);
            let i4 = body_index(next_theta, next_phi);
            indices.extend_from_slice(&[i1, i2, i3, i2, i4, i3]);
        }
    }
    (verts, indices)
}

/// Barrel tube + cap vertices/indices (`CalcBarrelVertices`/
/// `CalcBarrelIndices`, `cRobot3D.cpp:227-301`, cap fan
/// `cRobot3D.cpp:485`). Local layout: `2*res` tube verts, 1 cap-center
/// vert, then `res+1` cap-rim verts (`3*res+2` total). The cap
/// `TRIANGLEFAN` is converted to an indexed triangle list here.
pub fn build_barrel(dims: &Robot3DDimensions) -> (Vec<VertexLit>, Vec<u16>) {
    let res = dims.barrel_resolution;
    let offset = Vec3::new(0.0, 0.0, dims.tread_radius * 2.0 + dims.barrel_radius);
    let ang_theta_size = D3DX_PI * 2.0 / res as f32;

    let mut verts: Vec<VertexLit> = Vec::with_capacity((3 * res + 2) as usize);
    // Tube: for each theta a near (x=0) and far (x=barrelLength) vertex.
    for theta in 0..res {
        let ang = theta as f32 * ang_theta_size;
        let cy = ang.cos() * dims.barrel_radius;
        let cz = ang.sin() * dims.barrel_radius;
        let normal = Vec3::new(0.0, cy, cz); // PARITY: un-normalized.
        verts.push(vert(Vec3::new(0.0, cy, cz) + offset, normal));
        verts.push(vert(Vec3::new(dims.barrel_length, cy, cz) + offset, normal));
    }

    // Cap center + rim (rim copies the far-ring positions, cap normal +X).
    let cap_normal = Vec3::new(1.0, 0.0, 0.0);
    verts.push(vert(Vec3::new(dims.barrel_length, 0.0, 0.0) + offset, cap_normal));
    // GetBarrelVertexIndex(theta, false) = theta*2 + 1 (the far/front vert).
    for theta in 0..res + 1 {
        let far = ((theta % res) * 2 + 1) as usize;
        let pos = Vec3::from(verts[far].position);
        verts.push(vert(pos, cap_normal));
    }

    // Tube indices: GetBarrelVertexIndex(theta, back=true)=theta*2 (near),
    // (…, false)=theta*2+1 (far). Winding matches cRobot3D.cpp:293-298.
    let mut indices = Vec::with_capacity((res * 6) as usize + (res * 3) as usize);
    for theta in 0..res {
        let next = (theta + 1) % res;
        let i1 = (theta * 2) as u16;
        let i2 = (next * 2) as u16;
        let i3 = (theta * 2 + 1) as u16;
        let i4 = (next * 2 + 1) as u16;
        indices.extend_from_slice(&[i1, i2, i3, i2, i4, i3]);
    }

    // Cap fan: center = local index 2*res; rim = 2*res+1 .. 2*res+1+res.
    let cap_center = (res * 2) as u16;
    let fan: Vec<u16> = std::iter::once(cap_center)
        .chain((0..=res).map(|k| cap_center + 1 + k as u16))
        .collect();
    indices.extend(triangle_fan_to_list(&fan));

    (verts, indices)
}

/// Tread vertices + indices (`CalcTreadVertices`/`CalcTreadIndices`,
/// `cRobot3D.cpp:303-451`). `nTread = res*16+4` verts, `res*48` indices;
/// the second tread is the first copied and shifted −Y (its indices are
/// the first offset by `nTread/2`).
pub fn build_treads(dims: &Robot3DDimensions) -> (Vec<VertexLit>, Vec<u16>) {
    let res = dims.tread_resolution as usize;
    let half_verts = 2 + 8 * res; // one tread: 2 centers + 4 blocks of 2*res
    let offset = Vec3::new(0.0, 0.0, dims.tread_radius);

    let default = VertexLit {
        position: [0.0; 3],
        normal: [0.0; 3],
    };
    let mut half = vec![default; half_verts];

    // The two cover-center vertices.
    let center_y = dims.tread_width * 0.5;
    half[0] = vert(Vec3::new(0.0, -center_y, 0.0) + offset, Vec3::new(0.0, 1.0, 0.0));
    half[1] = vert(Vec3::new(0.0, center_y, 0.0) + offset, Vec3::new(0.0, -1.0, 0.0));

    let ang_theta_size = D3DX_PI / (res as f32 - 1.0);
    for theta in 0..res {
        let i_vertex = 2 + 2 * theta;
        let ang = theta as f32 * ang_theta_size - D3DX_PI * 0.5;
        let cx = ang.cos() * dims.tread_radius;
        let cy = dims.tread_width * 0.5;
        let cz = ang.sin() * dims.tread_radius;

        // Right tread side (block 0).
        let right_front = Vec3::new(cx + dims.tread_length * 0.5, -cy, cz);
        let right_back = Vec3::new(cx + dims.tread_length * 0.5, cy, cz);
        let right_normal = Vec3::new(cx, 0.0, cz);
        half[i_vertex] = vert(right_front + offset, right_normal);
        half[i_vertex + 1] = vert(right_back + offset, right_normal);

        // Left tread side (block 2).
        let left_front = Vec3::new(-cx - dims.tread_length * 0.5, -cy, -cz);
        let left_back = Vec3::new(-cx - dims.tread_length * 0.5, cy, -cz);
        let left_normal = Vec3::new(-cx, 0.0, -cz);
        half[i_vertex + res * 2] = vert(left_front + offset, left_normal);
        half[i_vertex + res * 2 + 1] = vert(left_back + offset, left_normal);

        // Right cover (block 4): copies right side positions, +Y normal.
        let front_cover_normal = Vec3::new(0.0, 1.0, 0.0);
        half[i_vertex + res * 4] = vert(right_front + offset, front_cover_normal);
        half[i_vertex + res * 4 + 1] = vert(right_back + offset, front_cover_normal);

        // Left cover (block 6): copies left side positions, −Y normal.
        let back_cover_normal = Vec3::new(0.0, -1.0, 0.0);
        half[i_vertex + res * 6] = vert(left_front + offset, back_cover_normal);
        half[i_vertex + res * 6 + 1] = vert(left_back + offset, back_cover_normal);
    }

    // Second tread = copy of the first (CopyMemory, cRobot3D.cpp:380).
    let mut verts = half.clone();
    verts.extend_from_slice(&half);

    // Shift both treads ±Y (cRobot3D.cpp:383-391).
    let shift = dims.body_radius - dims.tread_width * 0.5;
    let n_tread = verts.len();
    for (i, v) in verts.iter_mut().enumerate() {
        v.position[1] += if i >= n_tread / 2 { -shift } else { shift };
    }

    // --- indices for the first tread (local vertex offset 2) ---
    let n_side = res * 2;
    let base = 2u16;
    let mut half_idx: Vec<u16> = Vec::with_capacity(res * 24);
    // Actual tread sides (wraps right+left blocks into one loop).
    for s in 0..n_side {
        let next = (s + 1) % n_side;
        let i1 = (s * 2) as u16 + base;
        let i2 = (s * 2 + 1) as u16 + base;
        let i3 = (next * 2) as u16 + base;
        let i4 = (next * 2 + 1) as u16 + base;
        half_idx.extend_from_slice(&[i1, i2, i3, i2, i4, i3]);
    }
    // Front cover (center = local vertex 0).
    let front_center = 0u16;
    for s in 0..n_side {
        let next = (s + 1) % n_side;
        let i1 = (s * 2) as u16 + base + (res * 4) as u16;
        let i2 = (next * 2) as u16 + base + (res * 4) as u16;
        half_idx.extend_from_slice(&[front_center, i1, i2]);
    }
    // Back cover (center = local vertex 1).
    let back_center = 1u16;
    for s in 0..n_side {
        let next = (s + 1) % n_side;
        let i1 = (s * 2 + 1) as u16 + base + (res * 4) as u16;
        let i2 = (next * 2 + 1) as u16 + base + (res * 4) as u16;
        half_idx.extend_from_slice(&[back_center, i2, i1]);
    }

    // Second tread indices = first offset by nTreadVertices/2.
    let vshift = (n_tread / 2) as u16;
    let mut indices = half_idx.clone();
    indices.extend(half_idx.iter().map(|&i| i + vshift));

    (verts, indices)
}

/// Builds all three procedural part meshes (CPU-side) from the compiled-in
/// `OBJECTS` dimensions (`data::builtin::OBJECTS`).
pub fn build_geometry(dims: &Robot3DDimensions) -> RobotGeometry {
    RobotGeometry {
        body: build_body(dims),
        barrel: build_barrel(dims),
        treads: build_treads(dims),
    }
}

// ---------------------------------------------------------------------------
// Runtime renderer — compiled everywhere, exercised only at a display.
// ---------------------------------------------------------------------------

/// Per-instance robot render inputs (`cRobot3D::tRenderData`,
/// `cRobot3D.h`): pose + per-part material colors.
#[derive(Copy, Clone, Debug)]
pub struct RobotRenderData {
    pub position: Vec3,
    pub direction: f32,
    pub barrel_elevation: f32,
    pub body_color: Rgba,
    pub barrel_color: Rgba,
    pub tread_color: Rgba,
}

/// Owns the three uploaded part meshes and draws them with `prog_lit`
/// (per-part material color, point-light-at-eye Gouraud). Runtime-only.
pub struct RobotRenderer {
    body: Mesh<VertexLit>,
    barrel: Mesh<VertexLit>,
    treads: Mesh<VertexLit>,
}

impl RobotRenderer {
    pub fn new(gl: Rc<glow::Context>, dims: &Robot3DDimensions) -> Result<Self, GlError> {
        let geo = build_geometry(dims);
        let mut body = Mesh::new(geo.body.0, geo.body.1)?;
        let mut barrel = Mesh::new(geo.barrel.0, geo.barrel.1)?;
        let mut treads = Mesh::new(geo.treads.0, geo.treads.1)?;
        body.upload(gl.clone())?;
        barrel.upload(gl.clone())?;
        treads.upload(gl)?;
        Ok(Self {
            body,
            barrel,
            treads,
        })
    }

    /// Draws one posed robot. `decision` is the committed per-category
    /// winding decision (original: `D3DCULL_CCW`). Runtime-only.
    pub fn render(
        &self,
        gl_state: &mut GlState,
        programs: &Programs,
        camera: &FreeFlyCamera,
        aspect: f32,
        decision: WindingDecision,
        data: &RobotRenderData,
    ) {
        gl_state.set_winding(decision);
        let gl = Rc::clone(gl_state.context());
        let prog = &programs.lit;

        // world = RotZ(direction) * Translate(position) (cRobot3D.cpp:105-108).
        let world = Mat4::rotation_z(data.direction).mul(&Mat4::translation(data.position));
        // Barrel adds RotY(-elevation) in front of the world (cRobot3D.cpp:476-478).
        let barrel_world = Mat4::rotation_y(-data.barrel_elevation).mul(&world);

        unsafe {
            gl.use_program(Some(prog.program));
            if let Some(loc) = prog.uniform("u_light_atten") {
                let a = shaders::light_atten_uniform();
                gl.uniform_3_f32(Some(&loc), a[0], a[1], a[2]);
            }
        }

        self.draw_part(&gl, prog, camera, aspect, &world, data.body_color, &self.body);
        self.draw_part(
            &gl,
            prog,
            camera,
            aspect,
            &barrel_world,
            data.barrel_color,
            &self.barrel,
        );
        self.draw_part(
            &gl,
            prog,
            camera,
            aspect,
            &world,
            data.tread_color,
            &self.treads,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_part(
        &self,
        gl: &glow::Context,
        prog: &shaders::Program,
        camera: &FreeFlyCamera,
        aspect: f32,
        world: &Mat4,
        color: Rgba,
        mesh: &Mesh<VertexLit>,
    ) {
        let mvp = camera.mvp(world, aspect);
        let world_view = camera.world_view(world);
        unsafe {
            if let Some(loc) = prog.uniform("u_mvp") {
                gl.uniform_matrix_4_f32_slice(Some(&loc), true, &mvp.to_row_major_array());
            }
            if let Some(loc) = prog.uniform("u_world_view") {
                gl.uniform_matrix_4_f32_slice(
                    Some(&loc),
                    true,
                    &world_view.to_row_major_array(),
                );
            }
            if let Some(loc) = prog.uniform("u_material") {
                let m = shaders::material_uniform(color);
                gl.uniform_3_f32(Some(&loc), m[0], m[1], m[2]);
            }
        }
        mesh.draw();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::builtin::OBJECTS;

    #[test]
    fn part_counts_match_the_cpp_formulas() {
        let d = OBJECTS;
        let (bv, bi) = build_body(&d);
        // nBody = res*res/2; body indices = res*res*3.
        assert_eq!(bv.len(), (d.body_resolution * d.body_resolution / 2) as usize);
        assert_eq!(bi.len(), (d.body_resolution * d.body_resolution * 3) as usize);
        // res=12 -> 72 verts, 432 indices.
        assert_eq!(bv.len(), 72);
        assert_eq!(bi.len(), 432);

        let (rv, ri) = build_barrel(&d);
        // nBarrel = res*3+2 verts; res*6 tube + res*3 cap indices.
        assert_eq!(rv.len(), (d.barrel_resolution * 3 + 2) as usize);
        assert_eq!(rv.len(), 26);
        assert_eq!(ri.len(), (d.barrel_resolution * 6 + d.barrel_resolution * 3) as usize);
        assert_eq!(ri.len(), 72);

        let (tv, ti) = build_treads(&d);
        // nTread = res*16+4 verts; res*48 indices.
        assert_eq!(tv.len(), (d.tread_resolution * 16 + 4) as usize);
        assert_eq!(tv.len(), 132);
        assert_eq!(ti.len(), (d.tread_resolution * 48) as usize);
        assert_eq!(ti.len(), 384);
    }

    #[test]
    fn all_indices_are_in_range() {
        let d = OBJECTS;
        for (v, i) in [build_body(&d), build_barrel(&d), build_treads(&d)] {
            let n = v.len() as u16;
            assert!(i.iter().all(|&idx| idx < n), "index out of range");
        }
    }

    #[test]
    fn body_normals_are_the_unnormalized_scaled_position() {
        let d = OBJECTS;
        let (bv, _) = build_body(&d);
        // First vertex: theta=0, phi=0 -> SphereCoord(0,0,0.5) = (0.5,0,0);
        // z-scale leaves it unchanged (bodyHeight==bodyRadius). Normal is the
        // raw position (length 0.5, NOT 1.0) — the dimming PARITY.
        let n0 = Vec3::from(bv[0].normal);
        assert!((n0 - Vec3::new(0.5, 0.0, 0.0)).length() < 1e-6);
        let len = n0.length();
        assert!((len - d.body_radius).abs() < 1e-6);
        assert!((len - 1.0).abs() > 0.1, "normal must NOT be unit length");
        // Position lifts the body by tread_radius*2 on Z.
        assert!((bv[0].position[2] - d.tread_radius * 2.0).abs() < 1e-6);
    }

    #[test]
    fn barrel_normals_are_short_and_cap_is_plus_x() {
        let d = OBJECTS;
        let (rv, _) = build_barrel(&d);
        // Tube vertex 0 (theta=0): normal (0, radius, 0), length = barrel_radius.
        let n0 = Vec3::from(rv[0].normal);
        assert!((n0 - Vec3::new(0.0, d.barrel_radius, 0.0)).length() < 1e-6);
        assert!((n0.length() - d.barrel_radius).abs() < 1e-6);
        assert!(n0.length() < 0.5, "barrel normal is short (dim lighting)");
        // Cap center (local index 2*res) normal is +X.
        let cap = Vec3::from(rv[(d.barrel_resolution * 2) as usize].normal);
        assert!((cap - Vec3::new(1.0, 0.0, 0.0)).length() < 1e-6);
    }

    #[test]
    fn tread_cover_center_normals_are_plus_minus_y() {
        let d = OBJECTS;
        let (tv, _) = build_treads(&d);
        assert_eq!(tv[0].normal, [0.0, 1.0, 0.0]);
        assert_eq!(tv[1].normal, [0.0, -1.0, 0.0]);
        // Second tread's mirrored copy of vertex 0 sits at −Y of the first.
        let half = tv.len() / 2;
        let first_y = tv[0].position[1];
        let second_y = tv[half].position[1];
        // First shifted +shift, second −shift => second_y = first_y − 2*shift.
        let shift = d.body_radius - d.tread_width * 0.5;
        assert!((second_y - (first_y - 2.0 * shift)).abs() < 1e-6);
    }
}
