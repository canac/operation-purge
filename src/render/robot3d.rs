//! Procedural robot geometry: dome + barrel + treads, ported from
//! `cRobot3D` (source-map §3.3). Body lands at M5.
//!
//! PARITY: dome normals are the **raw scaled positions, never
//! normalized** (`D3DRS_NORMALIZENORMALS` never set) — lighting
//! intensity scales with the (0.08-0.5 length) normal, part of the 2011
//! look. Barrel cap is a triangle fan, converted to an indexed triangle
//! list at geometry build (design §1.5.13). The second tread is a
//! byte-copy of the first, shifted +-y.

use crate::data::builtin::Robot3DDimensions;
use crate::render::mesh::{Mesh, VertexLit};

/// Builds the dome + barrel + tread meshes from the compiled-in `OBJECTS`
/// dimensions (`data::builtin::OBJECTS`). Body lands at M5.
pub fn build_robot_meshes(
    _dims: &Robot3DDimensions,
) -> Result<Vec<Mesh<VertexLit>>, crate::error::GlError> {
    todo!("ported at M5 — dome/barrel/tread geometry (source-map §3.3)")
}
