//! `Mesh<V>`: VAO + VBO + `u16` EBO, replacing `cObject3D<V>`.
//!
//! GUARD: building meshes keep 16-bit indices (`wallSegments*4+8` verts); a
//! mesh that would overflow `u16` is a clean `GlError::TooManyVertices` at
//! load time instead of the silent corruption a fixed-size index buffer
//! would otherwise produce.
//!
//! Triangle fans (the barrel cap, `cRobot3D`) are converted to indexed
//! triangle lists at geometry-build time — GL core has `GL_TRIANGLE_FAN`, but
//! converting keeps one batching path. [`triangle_fan_to_list`] is the pure,
//! unit-tested conversion.

use std::marker::PhantomData;
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
/// normalized in the shader either (dome normals of length 0.08-0.5
/// intentionally dim the lighting).
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

/// pos + rgba8 (4x u8 normalized) + uv (24 B) — sprites.
///
/// Color is plain RGBA bytes, so the D3DCOLOR B,G,R,A memory-order hazard
/// never exists here.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct VertexSprite {
    pub position: [f32; 3],
    pub color: [u8; 4],
    pub uv: [f32; 2],
}

/// Maximum vertex count a `u16`-indexed mesh can hold.
pub const MAX_VERTICES: usize = u16::MAX as usize + 1;

/// The u16 GUARD: `Ok` iff `count` vertices are addressable by `u16` indices.
pub fn check_vertex_count(count: usize) -> Result<(), GlError> {
    if count > MAX_VERTICES {
        return Err(GlError::TooManyVertices { count });
    }
    Ok(())
}

/// Describes a vertex type's GL attribute layout so [`Mesh`] can configure
/// the VAO generically. Implemented for each of the four vertex structs
/// above (matching the four original FVFs).
pub trait VertexLayout: Pod {
    /// Configures `glVertexAttribPointer` calls for a currently-bound VAO +
    /// VBO.
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

/// Converts a triangle-fan index run to an indexed triangle list (`cRobot3D`
/// barrel cap). A fan `[c, v0, v1, v2, ...]` becomes triangles
/// `(c, v0, v1), (c, v1, v2), ...` preserving winding. Fewer than 3 vertices
/// yields no triangles.
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

/// An uploaded mesh: VAO + VBO + `u16` EBO, deleted on drop. Runtime-only
/// (needs a current GL context).
pub struct Mesh<V> {
    gl: Rc<glow::Context>,
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    ebo: glow::Buffer,
    index_count: usize,
    vertex_capacity: usize,
    _vertex: PhantomData<V>,
}

impl<V: VertexLayout> Mesh<V> {
    /// Uploads static vertices + indices.
    pub fn new(gl: Rc<glow::Context>, vertices: &[V], indices: &[u16]) -> Result<Self, GlError> {
        check_vertex_count(vertices.len())?;
        let mesh = Self::alloc(gl, vertices.len(), indices)?;
        unsafe {
            mesh.gl.bind_buffer(glow::ARRAY_BUFFER, Some(mesh.vbo));
            mesh.gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck::cast_slice(vertices),
                glow::STATIC_DRAW,
            );
        }
        Ok(mesh)
    }

    /// Allocates a stream vertex buffer of `vertex_capacity` vertices behind
    /// static `indices`; fill it each frame with [`Mesh::refill`].
    pub fn dynamic(
        gl: Rc<glow::Context>,
        vertex_capacity: usize,
        indices: &[u16],
    ) -> Result<Self, GlError> {
        check_vertex_count(vertex_capacity)?;
        let mesh = Self::alloc(gl, vertex_capacity, indices)?;
        mesh.orphan();
        Ok(mesh)
    }

    fn alloc(gl: Rc<glow::Context>, vertex_capacity: usize, indices: &[u16]) -> Result<Self, GlError> {
        unsafe {
            let vao = gl
                .create_vertex_array()
                .map_err(|e| GlError::Context(format!("mesh vao: {e}")))?;
            let mesh = Self {
                vbo: gl.create_buffer().map_err(|e| GlError::Context(format!("mesh vbo: {e}")))?,
                ebo: gl.create_buffer().map_err(|e| GlError::Context(format!("mesh ebo: {e}")))?,
                gl,
                vao,
                index_count: indices.len(),
                vertex_capacity,
                _vertex: PhantomData,
            };
            let gl = &mesh.gl;
            gl.bind_vertex_array(Some(mesh.vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(mesh.vbo));
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(mesh.ebo));
            gl.buffer_data_u8_slice(
                glow::ELEMENT_ARRAY_BUFFER,
                bytemuck::cast_slice(indices),
                glow::STATIC_DRAW,
            );
            V::configure_attribs(gl);
            gl.bind_vertex_array(None);
            Ok(mesh)
        }
    }

    /// Discards the vertex storage (`glBufferData(NULL)`), the GL twin of
    /// D3D's `Lock DISCARD`.
    fn orphan(&self) {
        unsafe {
            self.gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
            self.gl.buffer_data_size(
                glow::ARRAY_BUFFER,
                (self.vertex_capacity * std::mem::size_of::<V>()) as i32,
                glow::STREAM_DRAW,
            );
        }
    }

    /// Orphans the vertex buffer and refills its front with `vertices`
    /// (at most the capacity given to [`Mesh::dynamic`]).
    pub fn refill(&self, vertices: &[V]) {
        debug_assert!(vertices.len() <= self.vertex_capacity);
        self.orphan();
        unsafe {
            self.gl
                .buffer_sub_data_u8_slice(glow::ARRAY_BUFFER, 0, bytemuck::cast_slice(vertices));
        }
    }

    fn draw_primitive(&self, primitive: u32, start: usize, count: usize) {
        unsafe {
            self.gl.bind_vertex_array(Some(self.vao));
            self.gl.draw_elements(
                primitive,
                count as i32,
                glow::UNSIGNED_SHORT,
                (start * std::mem::size_of::<u16>()) as i32,
            );
            self.gl.bind_vertex_array(None);
        }
    }

    /// Draws the index range `[start, start + count)` as `GL_TRIANGLES`.
    pub fn draw_range(&self, start: usize, count: usize) {
        self.draw_primitive(glow::TRIANGLES, start, count);
    }

    /// Draws the index range `[start, start + count)` as `GL_LINES` (pairs).
    pub fn draw_lines_range(&self, start: usize, count: usize) {
        self.draw_primitive(glow::LINES, start, count);
    }

    /// Draws all indices as `GL_TRIANGLES`.
    pub fn draw(&self) {
        self.draw_range(0, self.index_count);
    }
}

impl<V> Drop for Mesh<V> {
    fn drop(&mut self) {
        unsafe {
            self.gl.delete_vertex_array(self.vao);
            self.gl.delete_buffer(self.vbo);
            self.gl.delete_buffer(self.ebo);
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
        assert!(matches!(
            check_vertex_count(MAX_VERTICES + 1),
            Err(GlError::TooManyVertices { .. })
        ));
    }

    #[test]
    fn exactly_max_vertices_is_allowed() {
        assert!(check_vertex_count(MAX_VERTICES).is_ok());
    }

    #[test]
    fn fan_of_four_becomes_two_triangles() {
        assert_eq!(
            triangle_fan_to_list(&[0, 1, 2, 3]),
            vec![0, 1, 2, 0, 2, 3]
        );
    }

    #[test]
    fn fan_of_n_becomes_n_minus_2_triangles_preserving_winding() {
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
