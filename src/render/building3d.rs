//! Procedural building geometry: wall extrusion, floor/ceiling, ported
//! from `cBuilding3D` (source-map §3.3). Body lands at M4 (the
//! winding/culling gate milestone).
//!
//! PARITY: the first (outer boundary) polygon gets **reversed winding**;
//! both wall faces share one axis normal; the ceiling normal points *up*
//! so it renders dark — an intentional look, not a bug to fix. UV = span
//! length / texture size, so `GL_REPEAT` is mandatory.

use crate::maze::building::Building;
use crate::render::mesh::{Mesh, VertexLitTextured};

/// Extrudes wall polylines + floor/ceiling into one static mesh. Layout:
/// wall quads first, then the last 8 verts / 12 indices = floor+ceiling
/// (source-map §3.3). Body lands at M4.
pub fn build_mesh(_building: &Building) -> Result<Mesh<VertexLitTextured>, crate::error::GlError> {
    todo!("ported at M4 — wall/floor/ceiling extrusion (source-map §3.3)")
}
