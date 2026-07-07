//! `Mesh<V: Pod>`: VBO/EBO wrapper with `u16` indices, replacing
//! `cObject3D<V>` (design §2, §3.5).
//!
//! GUARD (design §1.3): building meshes keep 16-bit indices
//! (`wallSegments*4+8` verts); a mesh that would overflow `u16` is a
//! clean `GlError::TooManyVertices` at load time instead of the silent
//! corruption a fixed-size index buffer would otherwise produce.
//!
//! Triangle fans (the barrel cap, `cRobot3D`) are converted to indexed
//! triangle lists at geometry-build time (design §3.5/§1.5.13) — GL core
//! has `GL_TRIANGLE_FAN`, but converting keeps one batching path and stays
//! backend-portable. [`triangle_fan_to_list`] is the pure, unit-tested
//! conversion.

use std::rc::Rc;

use bytemuck::{Pod, Zeroable};
use glow::HasContext;

use crate::error::GlError;

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

/// Describes a vertex type's GL attribute layout so [`Mesh`]/[`DynamicMesh`]
/// can configure the VAO generically. Implemented for each of the four
/// vertex structs above (matching the four original FVFs, design §3.4).
pub trait VertexLayout: Pod {
    /// Configures `glVertexAttribPointer` calls for a currently-bound VAO +
    /// VBO. Runtime-only (needs a current GL context).
    ///
    /// # Safety
    /// A VAO and the vertex VBO must be bound; `gl` must be current.
    unsafe fn configure_attribs(gl: &glow::Context);
}

impl VertexLayout for VertexPos {
    unsafe fn configure_attribs(gl: &glow::Context) {
        let stride = std::mem::size_of::<Self>() as i32;
        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, stride, 0);
    }
}

impl VertexLayout for VertexLit {
    unsafe fn configure_attribs(gl: &glow::Context) {
        let stride = std::mem::size_of::<Self>() as i32;
        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, stride, 0);
        gl.enable_vertex_attrib_array(1);
        gl.vertex_attrib_pointer_f32(1, 3, glow::FLOAT, false, stride, 12);
    }
}

impl VertexLayout for VertexLitTextured {
    unsafe fn configure_attribs(gl: &glow::Context) {
        let stride = std::mem::size_of::<Self>() as i32;
        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, stride, 0);
        gl.enable_vertex_attrib_array(1);
        gl.vertex_attrib_pointer_f32(1, 3, glow::FLOAT, false, stride, 12);
        gl.enable_vertex_attrib_array(2);
        gl.vertex_attrib_pointer_f32(2, 2, glow::FLOAT, false, stride, 24);
    }
}

impl VertexLayout for VertexSprite {
    unsafe fn configure_attribs(gl: &glow::Context) {
        let stride = std::mem::size_of::<Self>() as i32;
        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, stride, 0);
        // rgba8, normalized to [0,1] — mirrors the D3DCOLOR diffuse.
        gl.enable_vertex_attrib_array(1);
        gl.vertex_attrib_pointer_f32(1, 4, glow::UNSIGNED_BYTE, true, stride, 12);
        gl.enable_vertex_attrib_array(2);
        gl.vertex_attrib_pointer_f32(2, 2, glow::FLOAT, false, stride, 16);
    }
}

/// Converts a triangle-fan index run to an indexed triangle list
/// (`cRobot3D` barrel cap, design §3.5). A fan `[c, v0, v1, v2, ...]`
/// becomes triangles `(c, v0, v1), (c, v1, v2), ...` preserving winding.
/// Fewer than 3 vertices yields no triangles.
pub fn triangle_fan_to_list(fan: &[u16]) -> Vec<u16> {
    if fan.len() < 3 {
        return Vec::new();
    }
    let center = fan[0];
    let mut out = Vec::with_capacity((fan.len() - 2) * 3);
    for w in fan[1..].windows(2) {
        out.push(center);
        out.push(w[0]);
        out.push(w[1]);
    }
    out
}

/// A uploaded static mesh: VAO + VBO + `u16` EBO. Constructed CPU-side
/// (validating the u16 GUARD), then [`upload`](Mesh::upload)ed at runtime.
pub struct Mesh<V: Pod> {
    pub vertices: Vec<V>,
    pub indices: Vec<u16>,
    gl_handles: Option<GlBuffers>,
}

struct GlBuffers {
    gl: Rc<glow::Context>,
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    ebo: glow::Buffer,
    index_count: i32,
}

impl<V: Pod> Mesh<V> {
    pub fn new(vertices: Vec<V>, indices: Vec<u16>) -> Result<Self, GlError> {
        if vertices.len() > MAX_VERTICES {
            return Err(GlError::TooManyVertices {
                count: vertices.len(),
            });
        }
        Ok(Self {
            vertices,
            indices,
            gl_handles: None,
        })
    }
}

impl<V: VertexLayout> Mesh<V> {
    /// Uploads to a VAO/VBO/EBO. Runtime-only (needs a current GL context).
    pub fn upload(&mut self, gl: Rc<glow::Context>) -> Result<(), GlError> {
        unsafe {
            let vao = gl
                .create_vertex_array()
                .map_err(|e| GlError::Context(format!("mesh vao: {e}")))?;
            let vbo = gl
                .create_buffer()
                .map_err(|e| GlError::Context(format!("mesh vbo: {e}")))?;
            let ebo = gl
                .create_buffer()
                .map_err(|e| GlError::Context(format!("mesh ebo: {e}")))?;
            gl.bind_vertex_array(Some(vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck::cast_slice(&self.vertices),
                glow::STATIC_DRAW,
            );
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(ebo));
            gl.buffer_data_u8_slice(
                glow::ELEMENT_ARRAY_BUFFER,
                bytemuck::cast_slice(&self.indices),
                glow::STATIC_DRAW,
            );
            V::configure_attribs(&gl);
            gl.bind_vertex_array(None);
            self.gl_handles = Some(GlBuffers {
                gl,
                vao,
                vbo,
                ebo,
                index_count: self.indices.len() as i32,
            });
        }
        Ok(())
    }

    /// Draws the index range `[start, start + count)` as `GL_TRIANGLES`.
    /// Runtime-only; no-op if not yet uploaded. `start`/`count` are index
    /// (not triangle) counts — matches the building's three draw ranges.
    pub fn draw_range(&self, start: usize, count: usize) {
        let Some(h) = self.gl_handles.as_ref() else {
            return;
        };
        unsafe {
            h.gl.bind_vertex_array(Some(h.vao));
            h.gl.draw_elements(
                glow::TRIANGLES,
                count as i32,
                glow::UNSIGNED_SHORT,
                (start * std::mem::size_of::<u16>()) as i32,
            );
            h.gl.bind_vertex_array(None);
        }
    }

    /// Draws all indices as `GL_TRIANGLES`. Runtime-only.
    pub fn draw(&self) {
        if let Some(h) = self.gl_handles.as_ref() {
            self.draw_range(0, h.index_count as usize);
        }
    }
}

impl<V: Pod> Drop for Mesh<V> {
    fn drop(&mut self) {
        if let Some(h) = self.gl_handles.as_ref() {
            unsafe {
                h.gl.delete_vertex_array(h.vao);
                h.gl.delete_buffer(h.vbo);
                h.gl.delete_buffer(h.ebo);
            }
        }
    }
}

/// Dynamic (per-frame-refilled) mesh using buffer orphaning
/// (`glBufferData(NULL)` then fill — the GL twin of D3D's `Lock DISCARD`,
/// design §3.5). Used by the sprite batcher (M5). Runtime-only.
pub struct DynamicMesh<V: Pod> {
    pub capacity: usize,
    gl: Option<Rc<glow::Context>>,
    vao: Option<glow::VertexArray>,
    vbo: Option<glow::Buffer>,
    _marker: std::marker::PhantomData<V>,
}

impl<V: VertexLayout> DynamicMesh<V> {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            gl: None,
            vao: None,
            vbo: None,
            _marker: std::marker::PhantomData,
        }
    }

    /// Allocates the VAO/VBO sized for `capacity` verts. Runtime-only.
    pub fn upload(&mut self, gl: Rc<glow::Context>) -> Result<(), GlError> {
        unsafe {
            let vao = gl
                .create_vertex_array()
                .map_err(|e| GlError::Context(format!("dyn vao: {e}")))?;
            let vbo = gl
                .create_buffer()
                .map_err(|e| GlError::Context(format!("dyn vbo: {e}")))?;
            gl.bind_vertex_array(Some(vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            gl.buffer_data_size(
                glow::ARRAY_BUFFER,
                (self.capacity * std::mem::size_of::<V>()) as i32,
                glow::STREAM_DRAW,
            );
            V::configure_attribs(&gl);
            gl.bind_vertex_array(None);
            self.gl = Some(gl);
            self.vao = Some(vao);
            self.vbo = Some(vbo);
        }
        Ok(())
    }

    /// Orphans (discards) the buffer then refills it with `vertices`
    /// (design §3.5). Runtime-only; no-op if not uploaded.
    pub fn orphan_and_fill(&mut self, vertices: &[V]) {
        let (Some(gl), Some(vbo)) = (self.gl.as_ref(), self.vbo) else {
            return;
        };
        unsafe {
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            // Orphan: reallocate with NULL, then upload — Lock DISCARD twin.
            gl.buffer_data_size(
                glow::ARRAY_BUFFER,
                (self.capacity * std::mem::size_of::<V>()) as i32,
                glow::STREAM_DRAW,
            );
            gl.buffer_sub_data_u8_slice(glow::ARRAY_BUFFER, 0, bytemuck::cast_slice(vertices));
        }
    }
}

impl<V: Pod> Drop for DynamicMesh<V> {
    fn drop(&mut self) {
        if let (Some(gl), Some(vao), Some(vbo)) = (self.gl.as_ref(), self.vao, self.vbo) {
            unsafe {
                gl.delete_vertex_array(vao);
                gl.delete_buffer(vbo);
            }
        }
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
        assert!(matches!(result, Err(GlError::TooManyVertices { .. })));
    }

    #[test]
    fn exactly_max_vertices_is_allowed() {
        let verts = vec![VertexPos { position: [0.0; 3] }; MAX_VERTICES];
        assert!(Mesh::new(verts, vec![]).is_ok());
    }

    #[test]
    fn fan_of_four_becomes_two_triangles() {
        // Fan [c, a, b, d] -> (c,a,b),(c,b,d).
        assert_eq!(
            triangle_fan_to_list(&[0, 1, 2, 3]),
            vec![0, 1, 2, 0, 2, 3]
        );
    }

    #[test]
    fn fan_of_n_becomes_n_minus_2_triangles_preserving_winding() {
        // A 6-vertex fan (center + 5 rim) => 4 triangles => 12 indices.
        let fan = [10u16, 11, 12, 13, 14, 15];
        let tris = triangle_fan_to_list(&fan);
        assert_eq!(tris.len(), 12);
        assert_eq!(
            tris,
            vec![10, 11, 12, 10, 12, 13, 10, 13, 14, 10, 14, 15]
        );
    }

    #[test]
    fn degenerate_fans_yield_no_triangles() {
        assert!(triangle_fan_to_list(&[]).is_empty());
        assert!(triangle_fan_to_list(&[0]).is_empty());
        assert!(triangle_fan_to_list(&[0, 1]).is_empty());
    }
}
