//! `Mesh<V: Pod>`: VBO/EBO wrapper with `u16` indices, replacing
//! `cObject3D<V>` (design §2, §3.5). Bodies land at M4.
//!
//! GUARD (design §1.3): building meshes keep 16-bit indices
//! (`wallSegments*4+8` verts); a mesh that would overflow `u16` is a
//! clean `GlError::TooManyVertices` at load time instead of the silent
//! corruption a fixed-size index buffer would otherwise produce.

use bytemuck::{Pod, Zeroable};

/// pos vec3 (12 B) — overlay/2D geometry.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct VertexPos {
    pub position: [f32; 3],
}

/// pos+normal (24 B) — lit, untextured (robot parts, center markers).
///
/// PARITY: normals are stored and uploaded exactly as computed — never
/// normalized in the shader either (design §3.4 `prog_lit`: dome normals
/// of length 0.08-0.5 intentionally dim the lighting).
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct VertexLit {
    pub position: [f32; 3],
    pub normal: [f32; 3],
}

/// pos+normal+uv (32 B) — building walls/floor/ceiling.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct VertexLitTextured {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

/// pos + rgba8 (4x u8 normalized) + uv (24 B) — sprites, HUD text.
///
/// The D3DCOLOR B,G,R,A memory-order hazard (source-map §5.7) never
/// exists here: color is stored as plain RGBA bytes in Rust-native order.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct VertexSprite {
    pub position: [f32; 3],
    pub color: [u8; 4],
    pub uv: [f32; 2],
}

/// Maximum vertex count a `u16`-indexed mesh can hold.
pub const MAX_VERTICES: usize = u16::MAX as usize + 1;

/// Static (upload-once) mesh: one VBO + one `u16` EBO. Body lands at M4.
pub struct Mesh<V: Pod> {
    pub vertices: Vec<V>,
    pub indices: Vec<u16>,
}

impl<V: Pod> Mesh<V> {
    pub fn new(vertices: Vec<V>, indices: Vec<u16>) -> Result<Self, crate::error::GlError> {
        if vertices.len() > MAX_VERTICES {
            return Err(crate::error::GlError::TooManyVertices {
                count: vertices.len(),
            });
        }
        Ok(Self { vertices, indices })
    }

    /// Uploads to a VBO/EBO pair. Body lands at M4.
    pub fn upload(&self) {
        todo!("ported at M4 — VBO/EBO upload (design §3.5)")
    }
}

/// Dynamic (per-frame-refilled) mesh using buffer orphaning
/// (`glBufferData(NULL)` then fill — the GL twin of D3D's `Lock DISCARD`,
/// design §3.5). Body lands at M4/M5 (sprite batcher).
pub struct DynamicMesh<V: Pod> {
    pub capacity: usize,
    _marker: std::marker::PhantomData<V>,
}

impl<V: Pod> DynamicMesh<V> {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            _marker: std::marker::PhantomData,
        }
    }

    pub fn orphan_and_fill(&mut self, _vertices: &[V]) {
        todo!("ported at M4/M5 — buffer orphaning (design §3.5)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertex_sizes_match_the_original_fvfs() {
        assert_eq!(std::mem::size_of::<VertexPos>(), 12);
        assert_eq!(std::mem::size_of::<VertexLit>(), 24);
        assert_eq!(std::mem::size_of::<VertexLitTextured>(), 32);
        assert_eq!(std::mem::size_of::<VertexSprite>(), 24);
    }

    #[test]
    fn oversized_mesh_is_a_clean_error() {
        let verts = vec![VertexPos { position: [0.0; 3] }; MAX_VERTICES + 1];
        let result = Mesh::new(verts, vec![]);
        assert!(matches!(
            result,
            Err(crate::error::GlError::TooManyVertices { .. })
        ));
    }
}
