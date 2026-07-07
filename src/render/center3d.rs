//! Center marker geometry, ported from `cCenter3D` (source-map §3.3).
//! Body lands at M5.
//!
//! 8-vertex unit marker (floor square at z=0.001*fSize + one -X wall
//! square), instanced by `RotZ * Scale * Translate` per the design §3.2
//! row-vector composition order, per-team material color.

use crate::render::mesh::{Mesh, VertexLit};

pub fn build_center_mesh() -> Result<Mesh<VertexLit>, crate::error::GlError> {
    todo!("ported at M5 — 8-vertex center marker (source-map §3.3)")
}
