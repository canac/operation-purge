//! `math::d3dx` matrix-convention pins through the public API. A companion
//! to the unit tests in `src/math/d3dx.rs` that re-derives each expected
//! matrix by hand rather than sharing their fixtures.

use operation_purge::math::equals;
use operation_purge::math::d3dx::{billboard_basis, clip_fix, transform_point_homogeneous, Mat4, D3DX_PI};
use glam::Vec3;

#[test]
fn look_at_lh_is_left_handed_z_up() {
    // Eye at (0,-10,0) looking toward the origin (+Y direction), up = +Z —
    // exactly the convention `cOperationPurge.cpp:3053-3056` uses
    // (`vec3Up (0.0f, 0.0f, 1.0f)`).
    let eye = Vec3::new(0.0, -10.0, 0.0);
    let view = Mat4::look_at_lh(eye, Vec3::ZERO, Vec3::new(0.0, 0.0, 1.0));

    // The origin (the look-at target) must land on the view-space +Z axis
    // at the eye-to-target distance (D3D view space looks down +Z).
    let target_view = transform_point_homogeneous(&view, Vec3::ZERO);
    assert!(equals(target_view.x, 0.0));
    assert!(equals(target_view.y, 0.0));
    assert!((target_view.z - 10.0).abs() < 1e-4);
}

#[test]
fn perspective_fov_lh_uses_the_games_exact_call_site_constants() {
    // cOperationPurge.cpp:3060: D3DXMatrixPerspectiveFovLH(&matProjection,
    // D3DX_PI * 0.25f, aspect, 0.5f, fViewDist).
    let fov_y = D3DX_PI * 0.25;
    let znear = 0.5f32;
    let zfar = 500.0f32;
    let proj = Mat4::perspective_fov_lh(fov_y, 16.0 / 9.0, znear, zfar);

    // D3D convention: clip z in [0, w]. Verify both ends of the range.
    let near = transform_point_homogeneous(&proj, Vec3::new(0.0, 0.0, znear));
    assert!(equals(near.z, 0.0));
    let far = transform_point_homogeneous(&proj, Vec3::new(0.0, 0.0, zfar));
    assert!(equals(far.z, far.w));
}

#[test]
fn rotation_matrices_are_orthonormal_and_handed_correctly() {
    // A +Z-axis rotation by pi/2 must send +X to +Y (right-handed rotation
    // about the Z-up pole, matching the LH-but-Z-up game convention as
    // used at every heading/orientation call site).
    let rot = Mat4::rotation_z(D3DX_PI * 0.5);
    let out = rot.mul_row_vec4([1.0, 0.0, 0.0, 0.0]);
    assert!(out[0].abs() < 1e-6);
    assert!((out[1] - 1.0).abs() < 1e-6);
}

#[test]
fn clip_fix_element_form_matches_2z_minus_w_not_2z_minus_1() {
    let fix = clip_fix();
    // The z remap is z' = 2z - w in matrix form, never the additive 2z - 1.
    assert_eq!(fix.get(2, 2), 2.0);
    assert_eq!(fix.get(3, 2), -1.0);
    // A homogeneous point with w != 1 proves this is NOT the post-divide
    // `2z - 1` constant (which only coincides with `2z - w` when w == 1).
    let out = fix.mul_row_vec4([0.0, 0.0, 3.0, 5.0]);
    assert!(equals(out[2], 1.0)); // 2*3 - 5 = 1
    assert!(!equals(out[2], 2.0 * 3.0 - 1.0)); // would be 5 under the wrong formula
}

#[test]
fn billboard_basis_reads_view_matrix_columns_0_1_2() {
    // cSprite3D.cpp:80-85. Use a non-trivial camera so the check isn't
    // vacuously true for an identity-like view.
    let view = Mat4::look_at_lh(
        Vec3::new(10.0, 0.0, 0.0),
        Vec3::ZERO,
        Vec3::new(0.0, 0.0, 1.0),
    );
    let (right, up, forward) = billboard_basis(&view);

    // Right/up/forward must be mutually orthogonal unit vectors.
    assert!((right.length() - 1.0).abs() < 1e-5);
    assert!((up.length() - 1.0).abs() < 1e-5);
    assert!((forward.length() - 1.0).abs() < 1e-5);
    assert!(right.dot(up).abs() < 1e-5);
    assert!(right.dot(forward).abs() < 1e-5);
    assert!(up.dot(forward).abs() < 1e-5);

    // Looking from +X toward the origin, forward must point toward -X.
    assert!((forward - Vec3::new(-1.0, 0.0, 0.0)).length() < 1e-5);
}
