//! The 3D eye pose and its view/projection matrices (`SetupScene`,
//! `cOperationPurge.cpp:3034-3060`), built entirely from [`crate::math::d3dx`]:
//! `view = LookAtLH`, `proj = PerspectiveFovLH * CLIP_FIX` (row-vector order).
//!
//! Every matrix uniform is uploaded as its `to_row_major_array()` bytes with
//! `transpose = FALSE`: GL reads them column-major, so the shader's `mat4` is
//! the transpose of our row-vector matrix — exactly what the column-vector
//! `P * V * W * pos` needs.

use glam::Vec3;

use crate::math::d3dx::{self, Mat4};

/// Field of view (Y) and near plane — verbatim `cOperationPurge.cpp:3060`
/// (`fov = D3DX_PI * 0.25`, `znear = 0.5`; far = `fMaxVisibleDist`).
pub const FOV_Y: f32 = d3dx::D3DX_PI * 0.25;
pub const Z_NEAR: f32 = 0.5;

/// An eye in the game's LH Z-up frame. `yaw` rotates about +Z (heading),
/// `pitch` tilts up/down; both in radians.
#[derive(Copy, Clone, Debug)]
pub struct Camera {
    pub position: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub z_far: f32,
}

impl Camera {
    pub fn new(position: Vec3, z_far: f32) -> Self {
        Self {
            position,
            yaw: 0.0,
            pitch: 0.0,
            z_far,
        }
    }

    /// Forward (look) direction in world space — `SphereCoord(yaw, pitch, 1)`.
    pub fn forward(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        Vec3::new(cp * cy, cp * sy, sp)
    }

    /// View matrix (`D3DXMatrixLookAtLH`, up = +Z).
    pub fn view(&self) -> Mat4 {
        Mat4::look_at_lh(self.position, self.position + self.forward(), Vec3::Z)
    }

    /// Projection composed with `CLIP_FIX` (`proj_d3d * CLIP_FIX`), giving GL
    /// clip-space `z' = 2z - w`.
    pub fn projection(&self, aspect: f32) -> Mat4 {
        let proj = Mat4::perspective_fov_lh(FOV_Y, aspect, Z_NEAR, self.z_far);
        proj.mul(&d3dx::clip_fix())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::equals;

    fn looking_down_y() -> Camera {
        let mut cam = Camera::new(Vec3::ZERO, 200.0);
        cam.yaw = d3dx::D3DX_PI * 0.5;
        cam
    }

    #[test]
    fn view_projection_maps_near_and_far_plane_z_to_2z_minus_w() {
        let cam = looking_down_y();
        let vp = cam.view().mul(&cam.projection(1.0));

        // Near plane: D3D clip z = 0 => GL clip z = 2*0 - w = -w.
        let near_world = cam.forward() * Z_NEAR;
        let clip = vp.mul_row_vec4([near_world.x, near_world.y, near_world.z, 1.0]);
        assert!(equals(clip[2], -clip[3]), "near: z {} w {}", clip[2], clip[3]);

        // Far plane: D3D clip z = w => GL clip z = 2*w - w = w.
        let far_world = cam.forward() * 200.0;
        let clip = vp.mul_row_vec4([far_world.x, far_world.y, far_world.z, 1.0]);
        assert!(equals(clip[2], clip[3]), "far: z {} w {}", clip[2], clip[3]);
    }

    #[test]
    fn view_projection_places_forward_point_at_screen_center() {
        let cam = looking_down_y();
        let vp = cam.view().mul(&cam.projection(1.0));
        let p = cam.forward() * 10.0;
        let clip = vp.mul_row_vec4([p.x, p.y, p.z, 1.0]);
        assert!(clip[3] > 0.0, "point must be in front (w>0)");
        assert!((clip[0] / clip[3]).abs() < 1e-5);
        assert!((clip[1] / clip[3]).abs() < 1e-5);
    }

    #[test]
    fn view_translates_camera_to_view_space_origin() {
        let cam = Camera::new(Vec3::new(3.0, -4.0, 5.0), 200.0);
        let out = cam.view().mul_row_vec4([3.0, -4.0, 5.0, 1.0]);
        assert!(out[0].abs() < 1e-4 && out[1].abs() < 1e-4 && out[2].abs() < 1e-4);
    }
}
