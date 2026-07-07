//! Camera + free-fly debug controls + the M4 winding-gate debug state
//! (design §3.2, §3.3, §7 M4).
//!
//! The view/projection path is built entirely from `render::d3dx` (the
//! verbatim LH Z-up row-vector D3DX module) so no glam camera helper is
//! ever involved: `view = LookAtLH`, `proj = PerspectiveFovLH * CLIP_FIX`
//! (row-vector order). Every matrix uniform is uploaded as its
//! `to_row_major_array()` bytes with `transpose = FALSE`: GL reads them
//! column-major, so the shader's `mat4` is the transpose of our row-vector
//! matrix — exactly what the column-vector `P * V * W * pos` needs. (The
//! design §3.2 prescription of `transpose = TRUE` was wrong and sheared the
//! scene; corrected in commit "Fix transposed MVP upload".)

use glam::Vec3;

use crate::render::d3dx::{self, Mat4};
use crate::render::gl::WindingDecision;

/// Field of view (Y), near, far — verbatim `cOperationPurge.cpp:3060`
/// (`fov = D3DX_PI * 0.25`, `znear = 0.5`; far = `fMaxVisibleDist`).
pub const FOV_Y: f32 = d3dx::D3DX_PI * 0.25;
pub const Z_NEAR: f32 = 0.5;

/// A free-fly debug camera in the game's LH Z-up frame. `yaw` rotates about
/// +Z (heading), `pitch` tilts up/down; both in radians.
#[derive(Copy, Clone, Debug)]
pub struct FreeFlyCamera {
    pub position: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub z_far: f32,
}

impl FreeFlyCamera {
    pub fn new(position: Vec3, z_far: f32) -> Self {
        Self {
            position,
            yaw: 0.0,
            pitch: 0.0,
            z_far,
        }
    }

    /// Forward (look) direction in world space (LH Z-up).
    pub fn forward(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        Vec3::new(cp * cy, cp * sy, sp)
    }

    /// Right vector (perpendicular to forward, in the XY plane).
    pub fn right(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        Vec3::new(sy, -cy, 0.0)
    }

    /// Applies mouse-look deltas (radians), clamping pitch to avoid the
    /// look-straight-up gimbal singularity.
    pub fn rotate(&mut self, d_yaw: f32, d_pitch: f32) {
        self.yaw += d_yaw;
        self.pitch = (self.pitch + d_pitch).clamp(-1.5, 1.5);
    }

    /// Moves along the camera basis: `f` = forward, `r` = right, `u` = world
    /// up (+Z), each already scaled by speed * dt by the caller.
    pub fn translate(&mut self, f: f32, r: f32, u: f32) {
        self.position += self.forward() * f + self.right() * r + Vec3::Z * u;
    }

    /// View matrix (`D3DXMatrixLookAtLH`, up = +Z).
    pub fn view(&self) -> Mat4 {
        Mat4::look_at_lh(self.position, self.position + self.forward(), Vec3::Z)
    }

    /// Projection composed with `CLIP_FIX` (row-vector order,
    /// `proj_d3d * CLIP_FIX`), giving GL clip-space `z' = 2z - w` (design
    /// §3.2). Upload as `to_row_major_array()` with `transpose = FALSE`.
    pub fn projection(&self, aspect: f32) -> Mat4 {
        let proj = Mat4::perspective_fov_lh(FOV_Y, aspect, Z_NEAR, self.z_far);
        proj.mul(&d3dx::clip_fix())
    }

    /// `view * projection_fixed` — the MVP for world-space geometry whose
    /// world matrix is identity (the building). Row-vector order.
    pub fn view_projection(&self, aspect: f32) -> Mat4 {
        self.view().mul(&self.projection(aspect))
    }

    /// Full MVP for geometry with world matrix `world`: `world * view *
    /// proj_fixed` (row-vector; C++ left-to-right composition reads the
    /// same). Upload as `to_row_major_array()` with `transpose = FALSE`.
    pub fn mvp(&self, world: &Mat4, aspect: f32) -> Mat4 {
        world.mul(&self.view()).mul(&self.projection(aspect))
    }

    /// `world * view` — feeds the lit shaders' `u_world_view` (view-space
    /// lighting + `N_view = mat3(world_view) * N`, design §3.4).
    pub fn world_view(&self, world: &Mat4) -> Mat4 {
        world.mul(&self.view())
    }
}

/// Which building sub-mesh the M4 debug camera isolates while signing off
/// the winding checklist (design §3.3). `All` draws everything.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DrawCategory {
    All,
    Walls,
    Floor,
    Ceiling,
}

impl DrawCategory {
    const CYCLE: [DrawCategory; 4] = [
        DrawCategory::All,
        DrawCategory::Walls,
        DrawCategory::Floor,
        DrawCategory::Ceiling,
    ];

    fn next(self) -> DrawCategory {
        let i = Self::CYCLE.iter().position(|&c| c == self).unwrap_or(0);
        Self::CYCLE[(i + 1) % Self::CYCLE.len()]
    }
}

/// The M4 winding-gate debug state (design §3.3). Culling starts **off**
/// (`WindingDecision::Undetermined`) as the design mandates; the debug keys
/// let the user toggle culling, flip the front-face winding, and cycle the
/// isolated draw category to sign off each checklist item.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct WindingGate {
    /// Whether back-face culling is enabled.
    pub cull_enabled: bool,
    /// `true` = `GL_CCW` is the front face (the expected D3DCULL_CCW mapping,
    /// design §3.3/§1.5.2); `false` = `GL_CW`.
    pub front_ccw: bool,
    pub category: DrawCategory,
}

impl Default for WindingGate {
    fn default() -> Self {
        // Culling off until the checklist signs off (design §3.3).
        Self {
            cull_enabled: false,
            front_ccw: true,
            category: DrawCategory::All,
        }
    }
}

impl WindingGate {
    /// Debug key: toggle back-face culling on/off.
    pub fn toggle_cull(&mut self) {
        self.cull_enabled = !self.cull_enabled;
    }

    /// Debug key: flip which winding is the front face (`GL_CCW`↔`GL_CW`).
    pub fn flip_front_face(&mut self) {
        self.front_ccw = !self.front_ccw;
    }

    /// Debug key: cycle the isolated draw category.
    pub fn cycle_category(&mut self) {
        self.category = self.category.next();
    }

    /// The [`WindingDecision`] to hand `GlState::set_winding` given the
    /// current toggles.
    pub fn decision(&self) -> WindingDecision {
        if !self.cull_enabled {
            WindingDecision::CullNone
        } else if self.front_ccw {
            WindingDecision::CullBackCcwFront
        } else {
            WindingDecision::CullBackCwFront
        }
    }

    /// Whether the given category should be drawn under the current isolation.
    pub fn should_draw(&self, category: DrawCategory) -> bool {
        self.category == DrawCategory::All || self.category == category
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::equals;

    #[test]
    fn mvp_maps_near_and_far_plane_z_to_2z_minus_w() {
        // Camera at origin looking down +Y (yaw = pi/2), up = +Z.
        let mut cam = FreeFlyCamera::new(Vec3::ZERO, 200.0);
        cam.yaw = d3dx::D3DX_PI * 0.5;
        let vp = cam.view_projection(1.0);

        // A point Z_NEAR ahead along the look direction lands on the near
        // plane: D3D clip z = 0 => GL clip z = 2*0 - w = -w.
        let near_world = cam.forward() * Z_NEAR;
        let clip = vp.mul_row_vec4([near_world.x, near_world.y, near_world.z, 1.0]);
        assert!(equals(clip[2], -clip[3]), "near: z {} w {}", clip[2], clip[3]);

        // A point z_far ahead lands on the far plane: D3D clip z = w =>
        // GL clip z = 2*w - w = w.
        let far_world = cam.forward() * 200.0;
        let clip = vp.mul_row_vec4([far_world.x, far_world.y, far_world.z, 1.0]);
        assert!(equals(clip[2], clip[3]), "far: z {} w {}", clip[2], clip[3]);
    }

    #[test]
    fn mvp_places_forward_point_at_screen_center() {
        let mut cam = FreeFlyCamera::new(Vec3::ZERO, 200.0);
        cam.yaw = d3dx::D3DX_PI * 0.5; // look +Y
        let vp = cam.view_projection(1.0);
        // A point straight ahead projects to NDC (0,0) after divide.
        let p = cam.forward() * 10.0;
        let clip = vp.mul_row_vec4([p.x, p.y, p.z, 1.0]);
        assert!(clip[3] > 0.0, "point must be in front (w>0)");
        assert!((clip[0] / clip[3]).abs() < 1e-5);
        assert!((clip[1] / clip[3]).abs() < 1e-5);
    }

    #[test]
    fn view_translates_camera_to_view_space_origin() {
        let cam = FreeFlyCamera::new(Vec3::new(3.0, -4.0, 5.0), 200.0);
        let view = cam.view();
        let out = view.mul_row_vec4([3.0, -4.0, 5.0, 1.0]);
        assert!(out[0].abs() < 1e-4 && out[1].abs() < 1e-4 && out[2].abs() < 1e-4);
    }

    #[test]
    fn winding_gate_starts_with_culling_off() {
        let g = WindingGate::default();
        assert!(!g.cull_enabled);
        assert_eq!(g.decision(), WindingDecision::CullNone);
    }

    #[test]
    fn toggle_cull_enables_the_expected_ccw_front_mapping() {
        let mut g = WindingGate::default();
        g.toggle_cull();
        // Expected D3DCULL_CCW ≡ glFrontFace(GL_CCW)+cull back (design §3.3).
        assert_eq!(g.decision(), WindingDecision::CullBackCcwFront);
        g.flip_front_face();
        assert_eq!(g.decision(), WindingDecision::CullBackCwFront);
        g.toggle_cull();
        assert_eq!(g.decision(), WindingDecision::CullNone);
    }

    #[test]
    fn cycle_category_walks_all_walls_floor_ceiling() {
        let mut g = WindingGate::default();
        assert_eq!(g.category, DrawCategory::All);
        g.cycle_category();
        assert_eq!(g.category, DrawCategory::Walls);
        g.cycle_category();
        assert_eq!(g.category, DrawCategory::Floor);
        g.cycle_category();
        assert_eq!(g.category, DrawCategory::Ceiling);
        g.cycle_category();
        assert_eq!(g.category, DrawCategory::All);
    }

    #[test]
    fn should_draw_respects_isolation() {
        let mut g = WindingGate::default();
        assert!(g.should_draw(DrawCategory::Floor)); // All => everything
        g.category = DrawCategory::Floor;
        assert!(g.should_draw(DrawCategory::Floor));
        assert!(!g.should_draw(DrawCategory::Walls));
    }
}
