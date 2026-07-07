//! Verbatim left-handed, Z-up, **row-vector** D3DX math (design §3.2) —
//! the single most important parity mechanism in the port.
//!
//! A row-major `Mat4` newtype over `glam` storage with `v * M`
//! multiplication (`out[j] = sum_i v[i]*M[i][j]`), so C++ composition
//! sites (`world = rot_z * scale * translate`) read identically in Rust —
//! the entire class of per-site transposition mistakes is eliminated.
//!
//! Conversion to OpenGL happens at exactly one boundary (design §3.2):
//! `proj_final = proj_d3d * CLIP_FIX` (row-vector order), giving clip-space
//! `z' = 2z - w` — **never** the post-divide constant `2z - 1` — and every
//! uniform upload transposes so GLSL sees standard column-vector `P*V*W*pos`.
//!
//! ## Storage convention
//!
//! `glam::Mat4` is a column-major container of 4 `Vec4` "columns"
//! (`x_axis`/`y_axis`/`z_axis`/`w_axis`); this module treats those slots
//! purely as flat f32 storage and reuses glam's "column `i`" to mean
//! **our row `i`** (a deliberate relabeling, documented once, here). Under
//! that mapping `to_cols_array()`/`to_cols_array_2d()` hand back exactly
//! this matrix's 16 floats in row-major order — which is precisely the
//! layout `glUniformMatrix4fv(..., transpose = TRUE, ...)` wants. None of
//! `glam::Mat4`'s own camera/rotation helper *methods* are used (they are
//! right-handed/column-vector and would silently produce the wrong
//! matrices); only its storage is borrowed.
//!
//! ## `D3DXMATRIX` source note
//!
//! `D3DXMatrixLookAtLH`/`D3DXMatrixPerspectiveFovLH`/rotation/translation/
//! scaling are Microsoft D3DX9 library functions — their call sites live
//! in this codebase (`cOperationPurge.cpp:3056,3060`, `cSprite3D.cpp:111-163`,
//! `cRobot3D.cpp:105-476`, `cCenter3D.cpp:62-69`, `cOverlay2D.cpp:161-192`)
//! but their *bodies* are closed-source and not present in this repo. The
//! formulas below are the well-documented, widely reproduced (e.g. Wine's
//! `d3dx9_36`) reference algorithms for each function; every call site's
//! **arguments** (fov `pi/4`, znear `0.5`, up `+Z`, etc.) are verified
//! against this repo's actual call sites, cited per-function below.

use glam::{Vec3, Vec4};

/// `D3DX_PI = 3.141592654f` (PARITY — not `std::f32::consts::PI`'s more
/// precise value; the original's literal is used everywhere angle
/// constants matter for bit-for-bit-adjacent behavior). Verified against
/// call sites e.g. `cOperationPurge.cpp:3060` (`D3DX_PI * 0.25f` fovY).
#[allow(clippy::excessive_precision, clippy::approx_constant)]
pub const D3DX_PI: f32 = 3.141592654;

/// Row-major 4x4 matrix, D3DX storage convention (`v * M`, not `M * v`).
/// See the module doc for the glam-storage relabeling.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Mat4(pub glam::Mat4);

impl Mat4 {
    pub const IDENTITY: Mat4 = Mat4(glam::Mat4::IDENTITY);

    /// Build directly from row-major values (row 0 first).
    pub const fn from_rows(r0: [f32; 4], r1: [f32; 4], r2: [f32; 4], r3: [f32; 4]) -> Mat4 {
        Mat4(glam::Mat4::from_cols_array_2d(&[r0, r1, r2, r3]))
    }

    /// Row `i` as `[col0, col1, col2, col3]`.
    pub fn row(&self, i: usize) -> [f32; 4] {
        self.0.to_cols_array_2d()[i]
    }

    /// Element at (row, col) — matches `D3DXMATRIX::operator()(row, col)`.
    pub fn get(&self, row: usize, col: usize) -> f32 {
        self.row(row)[col]
    }

    /// The 16 elements in row-major order — exactly the layout
    /// `glUniformMatrix4fv(..., transpose = TRUE, ...)` expects.
    pub fn to_row_major_array(&self) -> [f32; 16] {
        self.0.to_cols_array()
    }

    /// Verbatim port of `D3DXMatrixLookAtLH` (standard D3DX9 reference
    /// algorithm; see module doc). Call site: `cOperationPurge.cpp:3053-3056`
    /// — `up = (0,0,1)` (+Z), confirming the Z-up convention.
    ///
    /// ```text
    /// zaxis = normalize(at - eye)
    /// xaxis = normalize(cross(up, zaxis))
    /// yaxis = cross(zaxis, xaxis)
    /// [ xaxis.x           yaxis.x           zaxis.x          0 ]
    /// [ xaxis.y           yaxis.y           zaxis.y          0 ]
    /// [ xaxis.z           yaxis.z           zaxis.z          0 ]
    /// [ -dot(xaxis,eye)   -dot(yaxis,eye)   -dot(zaxis,eye)   1 ]
    /// ```
    pub fn look_at_lh(eye: Vec3, at: Vec3, up: Vec3) -> Mat4 {
        let zaxis = (at - eye).normalize();
        let xaxis = up.cross(zaxis).normalize();
        let yaxis = zaxis.cross(xaxis);

        Mat4::from_rows(
            [xaxis.x, yaxis.x, zaxis.x, 0.0],
            [xaxis.y, yaxis.y, zaxis.y, 0.0],
            [xaxis.z, yaxis.z, zaxis.z, 0.0],
            [-xaxis.dot(eye), -yaxis.dot(eye), -zaxis.dot(eye), 1.0],
        )
    }

    /// Verbatim port of `D3DXMatrixPerspectiveFovLH` (standard D3DX9
    /// reference algorithm; see module doc). Call site:
    /// `cOperationPurge.cpp:3060` — `fov_y = D3DX_PI * 0.25`,
    /// `znear = 0.5`, `zfar = fMaxVisibleDist`. Clip z lands in `[0, w]`
    /// (D3D convention), remapped to GL's `[-w, w]` by [`clip_fix`].
    ///
    /// ```text
    /// yScale = cot(fovY/2)
    /// xScale = yScale / aspect
    /// [ xScale   0        0                0 ]
    /// [ 0        yScale   0                0 ]
    /// [ 0        0        zf/(zf-zn)       1 ]
    /// [ 0        0        -zn*zf/(zf-zn)   0 ]
    /// ```
    pub fn perspective_fov_lh(fov_y: f32, aspect: f32, znear: f32, zfar: f32) -> Mat4 {
        let y_scale = 1.0 / (fov_y * 0.5).tan();
        let x_scale = y_scale / aspect;

        Mat4::from_rows(
            [x_scale, 0.0, 0.0, 0.0],
            [0.0, y_scale, 0.0, 0.0],
            [0.0, 0.0, zfar / (zfar - znear), 1.0],
            [0.0, 0.0, -znear * zfar / (zfar - znear), 0.0],
        )
    }

    /// Verbatim port of `D3DXMatrixRotationX`. Call site:
    /// `cSprite3D.cpp:111` (sprite orientation-axis rotation).
    pub fn rotation_x(angle: f32) -> Mat4 {
        let (s, c) = angle.sin_cos();
        Mat4::from_rows(
            [1.0, 0.0, 0.0, 0.0],
            [0.0, c, s, 0.0],
            [0.0, -s, c, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        )
    }

    /// Verbatim port of `D3DXMatrixRotationY`. Call site:
    /// `cSprite3D.cpp:115`, `cRobot3D.cpp:476` (barrel elevation).
    pub fn rotation_y(angle: f32) -> Mat4 {
        let (s, c) = angle.sin_cos();
        Mat4::from_rows(
            [c, 0.0, -s, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [s, 0.0, c, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        )
    }

    /// Verbatim port of `D3DXMatrixRotationZ`. Call site:
    /// `cSprite3D.cpp:119`, `cCenter3D.cpp:62`, `cRobot3D.cpp:105`,
    /// `cOverlay2D.cpp:190` (heading/orientation about the Z-up axis).
    pub fn rotation_z(angle: f32) -> Mat4 {
        let (s, c) = angle.sin_cos();
        Mat4::from_rows(
            [c, s, 0.0, 0.0],
            [-s, c, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        )
    }

    /// Verbatim port of `D3DXMatrixTranslation`. Call sites:
    /// `cSprite3D.cpp:163`, `cCenter3D.cpp:69`, `cRobot3D.cpp:107`,
    /// `cOverlay2D.cpp:164,192`, `cOperationPurge.cpp:2743,2749`.
    pub fn translation(v: Vec3) -> Mat4 {
        Mat4::from_rows(
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [v.x, v.y, v.z, 1.0],
        )
    }

    /// Verbatim port of `D3DXMatrixScaling`. Call sites: `cCenter3D.cpp:66`,
    /// `cOverlay2D.cpp:161`, `cOperationPurge.cpp:2746`.
    pub fn scaling(v: Vec3) -> Mat4 {
        Mat4::from_rows(
            [v.x, 0.0, 0.0, 0.0],
            [0.0, v.y, 0.0, 0.0],
            [0.0, 0.0, v.z, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        )
    }

    /// Row-vector composition: `self` applied first, then `rhs`
    /// (`matA * matB` in every C++ call site, e.g. `cSprite3D.cpp:168`
    /// `matRotation * m_matBillboard * matTranslate * m_matView`, reads
    /// identically in Rust as `rot.mul(&billboard).mul(&translate).mul(&view)`).
    /// Standard matrix-product formula `out[i][j] = sum_k self[i][k]*rhs[k][j]`
    /// (row/column-vector convention only changes which side you multiply
    /// a *vector* on, not how two matrices multiply).
    pub fn mul(&self, rhs: &Mat4) -> Mat4 {
        let a = self.0.to_cols_array_2d();
        let b = rhs.0.to_cols_array_2d();
        let mut out = [[0.0f32; 4]; 4];
        for (i, out_row) in out.iter_mut().enumerate() {
            for (j, out_elem) in out_row.iter_mut().enumerate() {
                *out_elem = a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j] + a[i][3] * b[3][j];
            }
        }
        Mat4(glam::Mat4::from_cols_array_2d(&out))
    }

    /// Transform a row-vector `[x,y,z,w]` by this matrix:
    /// `out[j] = sum_i v[i] * self[i][j]` (design §3.2). Used by tests and
    /// by the eventual CLIP_FIX/upload pipeline (M4) to sanity-check a
    /// homogeneous point end to end.
    pub fn mul_row_vec4(&self, v: [f32; 4]) -> [f32; 4] {
        let m = self.0.to_cols_array_2d();
        let mut out = [0.0f32; 4];
        for (j, out_elem) in out.iter_mut().enumerate() {
            *out_elem = v[0] * m[0][j] + v[1] * m[1][j] + v[2] * m[2][j] + v[3] * m[3][j];
        }
        out
    }
}

/// Composed with the projection (row-vector order:
/// `proj_final = proj_d3d.mul(&clip_fix())`) to remap D3D clip z in `[0,w]`
/// to GL clip z in `[-w,w]`: `z' = 2z - w`, matrix elements `[2][2]=2.0`,
/// `[3][2]=-1.0` — **never** the additive `2z-1` (design §1.5.3, §3.2).
///
/// Proof this is `z' = 2z - w` and not `2z - 1`: for a row-vector
/// `[x,y,z,w] * CLIP_FIX`, the z' output column (`j=2`) picks up
/// `v[2]*M[2][2] + v[3]*M[3][2] = z*2.0 + w*(-1.0) = 2z - w`; every other
/// column passes its input through unchanged (identity elsewhere).
pub fn clip_fix() -> Mat4 {
    Mat4::from_rows(
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 2.0, 0.0],
        [0.0, 0.0, -1.0, 1.0],
    )
}

/// Billboard basis extraction — PARITY, `cSprite3D.cpp:80-85`: reads the
/// view matrix's upper-left 3x3 **columns** (fixing the column index,
/// varying row 0..2), each independently re-normalized exactly as
/// `D3DXVec3Normalize` does. Column 0 = right, column 1 = up, column 2 =
/// forward — `D3DXMATRIX::operator()(row, col)` indexes `m[row][col]`, so
/// `m_matView(0,0), m_matView(1,0), m_matView(2,0)` is literally column 0.
/// Returns `(right, up, forward)`.
pub fn billboard_basis(view: &Mat4) -> (Vec3, Vec3, Vec3) {
    let m = view.0.to_cols_array_2d();
    let right = Vec3::new(m[0][0], m[1][0], m[2][0]).normalize();
    let up = Vec3::new(m[0][1], m[1][1], m[2][1]).normalize();
    let forward = Vec3::new(m[0][2], m[1][2], m[2][2]).normalize();
    (right, up, forward)
}

/// Homogeneous row-vector helper for tests: `Vec3` point (`w=1`) through
/// `mat`, returning the raw `[x,y,z,w]` (no perspective divide) so callers
/// can inspect clip-space `z`/`w` directly.
pub fn transform_point_homogeneous(mat: &Mat4, p: Vec3) -> Vec4 {
    let out = mat.mul_row_vec4([p.x, p.y, p.z, 1.0]);
    Vec4::from(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::equals;

    fn approx_eq_mat(a: &Mat4, b: &Mat4, eps: f32) {
        for r in 0..4 {
            for c in 0..4 {
                let (av, bv) = (a.get(r, c), b.get(r, c));
                assert!(
                    (av - bv).abs() < eps,
                    "mismatch at [{r}][{c}]: {av} vs {bv}"
                );
            }
        }
    }

    #[test]
    fn identity_row_major_roundtrip() {
        let ident = Mat4::IDENTITY;
        assert_eq!(ident.get(0, 0), 1.0);
        assert_eq!(ident.get(1, 1), 1.0);
        assert_eq!(ident.get(0, 1), 0.0);
        assert_eq!(ident.to_row_major_array()[0], 1.0);
        assert_eq!(ident.to_row_major_array()[4], 0.0);
        assert_eq!(ident.to_row_major_array()[5], 1.0);
    }

    #[test]
    fn look_at_lh_hand_computed_reference() {
        // Camera at origin looking down +Y, up = +Z (the game's convention).
        let view = Mat4::look_at_lh(Vec3::ZERO, Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.0, 0.0, 1.0));
        // zaxis = (0,1,0); xaxis = normalize(cross(up,zaxis)) = cross((0,0,1),(0,1,0)) = (-1,0,0);
        // yaxis = cross(zaxis,xaxis) = cross((0,1,0),(-1,0,0)) = (0,0,1).
        let expected = Mat4::from_rows(
            [-1.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        );
        approx_eq_mat(&view, &expected, 1e-6);
    }

    #[test]
    fn look_at_lh_translates_eye_to_origin() {
        let eye = Vec3::new(3.0, 4.0, 5.0);
        let view = Mat4::look_at_lh(eye, eye + Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.0, 0.0, 1.0));
        // Transforming the eye point itself must land at the view-space origin.
        let out = transform_point_homogeneous(&view, eye);
        assert!(out.x.abs() < 1e-4);
        assert!(out.y.abs() < 1e-4);
        assert!(out.z.abs() < 1e-4);
    }

    #[test]
    fn perspective_fov_lh_hand_computed_reference() {
        // fovY = pi/2 -> yScale = cot(pi/4) = 1.0; aspect = 2.0 -> xScale = 0.5.
        let zn = 0.5;
        let zf = 100.0;
        let proj = Mat4::perspective_fov_lh(std::f32::consts::FRAC_PI_2, 2.0, zn, zf);
        assert!(equals(proj.get(0, 0), 0.5));
        assert!(equals(proj.get(1, 1), 1.0));
        assert!(equals(proj.get(2, 2), zf / (zf - zn)));
        assert!(equals(proj.get(2, 3), 1.0));
        assert!(equals(proj.get(3, 2), -zn * zf / (zf - zn)));
        assert!(equals(proj.get(3, 3), 0.0));
        // Off-diagonal zeros untouched.
        assert!(equals(proj.get(0, 1), 0.0));
        assert!(equals(proj.get(1, 0), 0.0));
    }

    #[test]
    fn perspective_fov_lh_matches_the_games_actual_call_site_args() {
        // cOperationPurge.cpp:3060: D3DX_PI * 0.25 fov, znear 0.5.
        let proj = Mat4::perspective_fov_lh(D3DX_PI * 0.25, 16.0 / 9.0, 0.5, 200.0);
        assert!(proj.get(1, 1) > 0.0);
        assert!(proj.get(0, 0) > 0.0);
        // Near plane maps to clip z=0 (D3D convention: z in [0,w]).
        let near_point = Vec3::new(0.0, 0.0, 0.5);
        let clip = transform_point_homogeneous(&proj, near_point);
        assert!(equals(clip.z, 0.0));
        assert!(equals(clip.w, 0.5));
    }

    #[test]
    fn rotation_x_hand_computed() {
        let m = Mat4::rotation_x(D3DX_PI * 0.5);
        // 90 deg about X: Y axis -> Z axis (row-vector convention: e_y * M).
        let out = m.mul_row_vec4([0.0, 1.0, 0.0, 0.0]);
        assert!(equals(out[0], 0.0));
        assert!((out[1]).abs() < 1e-6);
        assert!((out[2] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn rotation_y_hand_computed() {
        let m = Mat4::rotation_y(D3DX_PI * 0.5);
        // 90 deg about Y: Z axis -> X axis.
        let out = m.mul_row_vec4([0.0, 0.0, 1.0, 0.0]);
        assert!((out[0] - 1.0).abs() < 1e-6);
        assert!(out[1].abs() < 1e-6);
        assert!(out[2].abs() < 1e-6);
    }

    #[test]
    fn rotation_z_hand_computed() {
        let m = Mat4::rotation_z(D3DX_PI * 0.5);
        // 90 deg about Z: X axis -> Y axis.
        let out = m.mul_row_vec4([1.0, 0.0, 0.0, 0.0]);
        assert!(out[0].abs() < 1e-6);
        assert!((out[1] - 1.0).abs() < 1e-6);
        assert!(out[2].abs() < 1e-6);
    }

    #[test]
    fn translation_and_scaling_hand_computed() {
        let t = Mat4::translation(Vec3::new(1.0, 2.0, 3.0));
        let out = t.mul_row_vec4([0.0, 0.0, 0.0, 1.0]);
        assert_eq!(out, [1.0, 2.0, 3.0, 1.0]);

        let s = Mat4::scaling(Vec3::new(2.0, 3.0, 4.0));
        let out = s.mul_row_vec4([1.0, 1.0, 1.0, 1.0]);
        assert_eq!(out, [2.0, 3.0, 4.0, 1.0]);
    }

    #[test]
    fn mul_composition_order_matches_cpp_left_to_right_reading() {
        // rot(90 about Z) * translate(1,0,0): C++ reads "rotate then
        // translate" left to right; a point at the local +X axis should
        // rotate to +Y, THEN be offset by the translation.
        let rot = Mat4::rotation_z(D3DX_PI * 0.5);
        let trans = Mat4::translation(Vec3::new(1.0, 0.0, 0.0));
        let combined = rot.mul(&trans);
        let out = combined.mul_row_vec4([1.0, 0.0, 0.0, 1.0]);
        assert!((out[0] - 1.0).abs() < 1e-6);
        assert!((out[1] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn clip_fix_maps_z_to_2z_minus_w() {
        let fix = clip_fix();
        for &(z, w) in &[(0.0f32, 10.0f32), (5.0, 10.0), (10.0, 10.0), (3.5, 7.0)] {
            let out = fix.mul_row_vec4([1.0, 2.0, z, w]);
            assert!(
                equals(out[2], 2.0 * z - w),
                "z={z} w={w}: got {} expected {}",
                out[2],
                2.0 * z - w
            );
            // x/y pass through unchanged; w passes through unchanged too.
            assert_eq!(out[0], 1.0);
            assert_eq!(out[1], 2.0);
            assert_eq!(out[3], w);
        }
    }

    #[test]
    fn clip_fix_composed_with_projection_maps_near_and_far_planes() {
        let zn = 0.5;
        let zf = 200.0;
        let proj = Mat4::perspective_fov_lh(D3DX_PI * 0.25, 1.0, zn, zf);
        let final_proj = proj.mul(&clip_fix());

        let near_clip = transform_point_homogeneous(&final_proj, Vec3::new(0.0, 0.0, zn));
        // D3D clip z at near plane is 0 (before this fixup) => GL clip z = 2*0 - w = -w.
        assert!(equals(near_clip.z, -near_clip.w));

        let far_clip = transform_point_homogeneous(&final_proj, Vec3::new(0.0, 0.0, zf));
        // D3D clip z at far plane is w (before this fixup) => GL clip z = 2*w - w = w.
        assert!(equals(far_clip.z, far_clip.w));
    }

    #[test]
    fn billboard_basis_matches_known_camera() {
        // Same camera as `look_at_lh_hand_computed_reference`: eye at
        // origin looking down +Y, up=+Z. That view matrix's rows are
        // row0=(-1,0,0,0) row1=(0,0,1,0) row2=(0,1,0,0) row3=(0,0,0,1).
        // Columns 0/1/2 (fixed col, varying row 0..2):
        //   col0 = (row0[0],row1[0],row2[0]) = (-1,0,0) -> right
        //   col1 = (row0[1],row1[1],row2[1]) = (0,0,1)  -> up (matches +Z world up)
        //   col2 = (row0[2],row1[2],row2[2]) = (0,1,0)  -> forward (matches +Y look direction)
        let view = Mat4::look_at_lh(Vec3::ZERO, Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.0, 0.0, 1.0));
        let (right, up, forward) = billboard_basis(&view);
        assert!((right - Vec3::new(-1.0, 0.0, 0.0)).length() < 1e-6);
        assert!((up - Vec3::new(0.0, 0.0, 1.0)).length() < 1e-6);
        assert!((forward - Vec3::new(0.0, 1.0, 0.0)).length() < 1e-6);
    }

    #[test]
    fn billboard_basis_vectors_are_normalized() {
        let view = Mat4::look_at_lh(
            Vec3::new(5.0, -3.0, 2.0),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
        );
        let (right, up, forward) = billboard_basis(&view);
        assert!((right.length() - 1.0).abs() < 1e-5);
        assert!((up.length() - 1.0).abs() < 1e-5);
        assert!((forward.length() - 1.0).abs() < 1e-5);
    }
}
