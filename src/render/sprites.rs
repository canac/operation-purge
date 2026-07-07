//! CPU view-space billboard sprite batcher, ported from `cSprite3D`
//! (`cSprite3D.cpp:13-349`, source-map §3.2).
//!
//! PARITY: the billboard basis reads the view matrix's **columns** 0/1/2
//! (`cSprite3D.cpp:80-85`, via [`crate::render::d3dx::billboard_basis`]);
//! the 6 orientation matrices are copied verbatim (`cSprite3D.cpp:128-159`);
//! sprite quads use the index pattern `(0,2,1)(1,2,3)`
//! (`cSprite3D.cpp:55-60`); each sprite's 4 vertices are pre-transformed
//! into **view space** on the CPU so the sprite shader only applies the
//! projection (design §3.4 prog_sprite).
//!
//! FIX (design §1.3): the original keyed sprites in a `std::map<float,
//! tSprite*>` by depth, silently **dropping** any two with an identical
//! depth key, and its `Flush` had a `std::max` batch-size bug plus a
//! last-sprite-of-batch mis-texture. The port sorts a `Vec` with a
//! **stable** back-to-front sort (no equal-key drop) and computes exact
//! per-texture draw runs — visual-only fixes, no feel impact.

use std::rc::Rc;

use glam::{Vec2, Vec3};
use glow::HasContext;

use crate::error::GlError;
use crate::math::Rgba;
use crate::render::camera::FreeFlyCamera;
use crate::render::d3dx::{self, Mat4};
use crate::render::gl::GlState;
use crate::render::mesh::{VertexLayout, VertexSprite};
use crate::render::shaders::Programs;

/// `s_SpriteBatchSize` (`cOperationPurge.cpp:17`).
pub const BATCH_SIZE: usize = 2048;

/// `eAxis` (`cSprite3D.h:12`) — ordinals are load-bearing (they index the
/// verbatim orientation-matrix table).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Axis {
    PositiveX,
    NegativeX,
    PositiveY,
    NegativeY,
    PositiveZ,
    NegativeZ,
}

impl Axis {
    fn is_x(self) -> bool {
        matches!(self, Axis::PositiveX | Axis::NegativeX)
    }
    fn is_y(self) -> bool {
        matches!(self, Axis::PositiveY | Axis::NegativeY)
    }
}

/// The 6 verbatim orientation matrices (`cSprite3D.cpp:128-159`), given in
/// row-major D3DX literal order exactly as the source constructs them.
pub fn orient_matrix(axis: Axis) -> Mat4 {
    match axis {
        Axis::PositiveX => Mat4::from_rows(
            [0.0, -1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ),
        Axis::NegativeX => Mat4::from_rows(
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ),
        Axis::PositiveY => Mat4::from_rows(
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ),
        Axis::NegativeY => Mat4::from_rows(
            [-1.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ),
        Axis::PositiveZ => Mat4::from_rows(
            [-1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ),
        Axis::NegativeZ => Mat4::from_rows(
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ),
    }
}

/// The billboard matrix (`cSprite3D.cpp:80-91`): rows are the view
/// matrix's right/up/forward columns (each re-normalized).
pub fn billboard_matrix(view: &Mat4) -> Mat4 {
    let (right, up, forward) = d3dx::billboard_basis(view);
    Mat4::from_rows(
        [right.x, right.y, right.z, 0.0],
        [up.x, up.y, up.z, 0.0],
        [forward.x, forward.y, forward.z, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    )
}

/// One sprite to add (`cSprite3D::tSpriteData`). `texture` is the caller's
/// texture handle used only to split draw runs.
#[derive(Copy, Clone, Debug)]
pub struct SpriteData<T> {
    pub center: Vec3,
    pub offset: Vec3,
    pub size: Vec2,
    pub rotation: f32,
    pub axis: Axis,
    pub color: Rgba,
    pub texture: T,
}

/// Local quad corner offsets (`apt2VertexOffsets`, `cSprite3D.cpp:180-183`).
const CORNERS: [Vec2; 4] = [
    Vec2::new(-0.5, -0.5),
    Vec2::new(0.5, -0.5),
    Vec2::new(-0.5, 0.5),
    Vec2::new(0.5, 0.5),
];

/// Float RGBA → normalized u8 RGBA (the vertex diffuse; the D3DCOLOR
/// byte-order hazard never exists here — see `render::mesh`).
pub fn rgba_to_u8(c: Rgba) -> [u8; 4] {
    let f = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    [f(c.r), f(c.g), f(c.b), f(c.a)]
}

/// Computes a sprite's 4 view-space vertices and its depth sort key (the
/// sum of the 4 view-space z's, matching `pt3Center.z`,
/// `cSprite3D.cpp:202,214`). `billboard = Some(m)` uses the billboard path
/// (`matRotation * billboard * translate * view`, offset added to each
/// corner); `None` uses the oriented path (`orient * matRotation *
/// translate * view`).
pub fn sprite_vertices<T>(
    data: &SpriteData<T>,
    view: &Mat4,
    billboard: Option<&Mat4>,
) -> ([VertexSprite; 4], f32) {
    let rotation = if data.axis.is_x() {
        Mat4::rotation_x(data.rotation)
    } else if data.axis.is_y() {
        Mat4::rotation_y(data.rotation)
    } else {
        Mat4::rotation_z(data.rotation)
    };
    let translate = Mat4::translation(data.center);

    let transform = match billboard {
        Some(bb) => rotation.mul(bb).mul(&translate).mul(view),
        None => orient_matrix(data.axis)
            .mul(&rotation)
            .mul(&translate)
            .mul(view),
    };

    let color = rgba_to_u8(data.color);
    let mut verts = [VertexSprite {
        position: [0.0; 3],
        color,
        uv: [0.0; 2],
    }; 4];
    let mut depth = 0.0f32;
    for (i, corner) in CORNERS.iter().enumerate() {
        let mut offset = Vec3::new(corner.x * data.size.x, corner.y * data.size.y, 0.0);
        if billboard.is_some() {
            offset += data.offset;
        }
        let p = transform.mul_row_vec4([offset.x, offset.y, offset.z, 1.0]);
        verts[i].position = [p[0], p[1], p[2]];
        verts[i].uv = [corner.x + 0.5, corner.y + 0.5];
        depth += p[2];
    }
    (verts, depth)
}

/// One accumulated sprite: its 4 view-space vertices, depth key, texture.
#[derive(Clone)]
struct Entry<T> {
    verts: [VertexSprite; 4],
    depth: f32,
    texture: T,
}

/// A contiguous run of same-texture sprites within one batch — one draw
/// call. `first`/`count` are sprite (not vertex/index) counts.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct DrawRun<T> {
    pub texture: T,
    pub first: usize,
    pub count: usize,
}

/// Splits an ordered texture-per-sprite list into per-texture draw runs
/// that never cross a `BATCH_SIZE` boundary (matching `Flush`'s
/// batch-fill + texture-run loop, `cSprite3D.cpp:245-306`). Every sprite
/// appears in exactly one run — the equal-depth-drop / last-sprite
/// mis-texture bugs are designed out (FIX).
pub fn build_draw_runs<T: Copy + PartialEq>(textures: &[T]) -> Vec<DrawRun<T>> {
    let mut runs: Vec<DrawRun<T>> = Vec::new();
    for (i, &tex) in textures.iter().enumerate() {
        let at_batch_boundary = i % BATCH_SIZE == 0;
        match runs.last_mut() {
            Some(run) if !at_batch_boundary && run.texture == tex => run.count += 1,
            _ => runs.push(DrawRun {
                texture: tex,
                first: i,
                count: 1,
            }),
        }
    }
    runs
}

/// The CPU-side sprite accumulator. Generic over the texture handle so the
/// headless tests can use a plain `u32` while the runtime uses
/// `glow::Texture`.
pub struct SpriteBatch<T> {
    view: Mat4,
    billboard: bool,
    sort_depth: bool,
    billboard_mat: Mat4,
    sprites: Vec<Entry<T>>,
}

impl<T: Copy + PartialEq> SpriteBatch<T> {
    pub fn new() -> Self {
        Self {
            view: Mat4::IDENTITY,
            billboard: false,
            sort_depth: false,
            billboard_mat: Mat4::IDENTITY,
            sprites: Vec::with_capacity(BATCH_SIZE),
        }
    }

    /// `StartRender` (`cSprite3D.cpp:73-92`): set the view matrix and
    /// billboard/sort flags; precompute the billboard basis matrix.
    pub fn start(&mut self, view: Mat4, billboard: bool, sort_depth: bool) {
        self.view = view;
        self.billboard = billboard;
        self.sort_depth = sort_depth;
        self.billboard_mat = billboard_matrix(&view);
        self.sprites.clear();
    }

    /// `AddSprite` (`cSprite3D.cpp:97-222`): transform + accumulate.
    pub fn add(&mut self, data: &SpriteData<T>) {
        let bb = if self.billboard {
            Some(&self.billboard_mat)
        } else {
            None
        };
        let (verts, depth) = sprite_vertices(data, &self.view, bb);
        self.sprites.push(Entry {
            verts,
            depth,
            texture: data.texture,
        });
    }

    pub fn len(&self) -> usize {
        self.sprites.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sprites.is_empty()
    }

    /// Orders the accumulated sprites back-to-front when depth sorting is
    /// on: a **stable** descending sort by depth key (farthest first). With
    /// sorting off, insertion order is preserved (the original's arbitrary
    /// map key was just the insertion counter). No sprite is ever dropped
    /// (FIX vs the equal-key `std::map`).
    fn ordered(&self) -> Vec<&Entry<T>> {
        let mut order: Vec<&Entry<T>> = self.sprites.iter().collect();
        if self.sort_depth {
            order.sort_by(|a, b| {
                b.depth
                    .partial_cmp(&a.depth)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }
        order
    }

    /// The flat interleaved vertex buffer + per-texture draw runs, in final
    /// (back-to-front, batched) draw order. Pure — the runtime flush uploads
    /// `vertices` and issues one `draw_range` per run.
    pub fn build(&self) -> (Vec<VertexSprite>, Vec<DrawRun<T>>) {
        let order = self.ordered();
        let mut vertices = Vec::with_capacity(order.len() * 4);
        let mut textures = Vec::with_capacity(order.len());
        for e in &order {
            vertices.extend_from_slice(&e.verts);
            textures.push(e.texture);
        }
        (vertices, build_draw_runs(&textures))
    }
}

impl<T: Copy + PartialEq> Default for SpriteBatch<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// The static index pattern for `n` sprites: `(0,2,1)(1,2,3)` per quad
/// (`cSprite3D.cpp:55-60`), offset by 4 verts per sprite.
pub fn sprite_indices(n_sprites: usize) -> Vec<u32> {
    let mut out = Vec::with_capacity(n_sprites * 6);
    for s in 0..n_sprites {
        let v = (s * 4) as u32;
        out.extend_from_slice(&[v, v + 2, v + 1, v + 1, v + 2, v + 3]);
    }
    out
}

// ---------------------------------------------------------------------------
// Runtime renderer — compiled everywhere, exercised only at a display.
// ---------------------------------------------------------------------------

/// Owns a dynamic (orphaned) vertex buffer sized for one batch plus a
/// static index buffer, and draws a [`SpriteBatch`] with `prog_sprite`
/// (verts already view-space → projection only). Runtime-only.
pub struct SpriteRenderer {
    gl: Rc<glow::Context>,
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    ebo: glow::Buffer,
}

impl SpriteRenderer {
    pub fn new(gl: Rc<glow::Context>) -> Result<Self, GlError> {
        unsafe {
            let vao = gl
                .create_vertex_array()
                .map_err(|e| GlError::Context(format!("sprite vao: {e}")))?;
            let vbo = gl
                .create_buffer()
                .map_err(|e| GlError::Context(format!("sprite vbo: {e}")))?;
            let ebo = gl
                .create_buffer()
                .map_err(|e| GlError::Context(format!("sprite ebo: {e}")))?;
            gl.bind_vertex_array(Some(vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            gl.buffer_data_size(
                glow::ARRAY_BUFFER,
                (BATCH_SIZE * 4 * std::mem::size_of::<VertexSprite>()) as i32,
                glow::STREAM_DRAW,
            );
            // Static index buffer: BATCH_SIZE quads, never changes.
            let indices = sprite_indices(BATCH_SIZE);
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(ebo));
            gl.buffer_data_u8_slice(
                glow::ELEMENT_ARRAY_BUFFER,
                bytemuck::cast_slice(&indices),
                glow::STATIC_DRAW,
            );
            VertexSprite::configure_attribs(&gl);
            gl.bind_vertex_array(None);
            Ok(Self { gl, vao, vbo, ebo })
        }
    }

    /// Uploads the batch and draws it. The batch is expected to hold
    /// ≤ `BATCH_SIZE` sprites per call; larger batches are drawn in
    /// `BATCH_SIZE`-sprite chunks. Runtime-only.
    pub fn render<T: Copy + PartialEq>(
        &self,
        gl_state: &mut GlState,
        programs: &Programs,
        camera: &FreeFlyCamera,
        aspect: f32,
        batch: &SpriteBatch<T>,
        bind_texture: impl Fn(&glow::Context, T),
    ) {
        if batch.is_empty() {
            return;
        }
        let (vertices, runs) = batch.build();
        let prog = &programs.sprite;
        let proj = camera.projection(aspect);
        let _ = gl_state;

        unsafe {
            self.gl.use_program(Some(prog.program));
            if let Some(loc) = prog.uniform("u_proj") {
                self.gl
                    .uniform_matrix_4_f32_slice(Some(&loc), false, &proj.to_row_major_array());
            }
            if let Some(loc) = prog.uniform("u_tex") {
                self.gl.uniform_1_i32(Some(&loc), 0);
            }
            self.gl.active_texture(glow::TEXTURE0);
            self.gl.bind_vertex_array(Some(self.vao));
        }

        for run in &runs {
            // Vertices for this run's batch, relative to a batch boundary.
            let batch_start = (run.first / BATCH_SIZE) * BATCH_SIZE;
            let local_first = run.first - batch_start;
            unsafe {
                bind_texture(&self.gl, run.texture);
                self.gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
                // Orphan + refill only at each batch's first run.
                if local_first == 0 {
                    let end = (batch_start + BATCH_SIZE).min(vertices.len() / 4);
                    let slice = &vertices[batch_start * 4..end * 4];
                    self.gl.buffer_data_size(
                        glow::ARRAY_BUFFER,
                        (BATCH_SIZE * 4 * std::mem::size_of::<VertexSprite>()) as i32,
                        glow::STREAM_DRAW,
                    );
                    self.gl.buffer_sub_data_u8_slice(
                        glow::ARRAY_BUFFER,
                        0,
                        bytemuck::cast_slice(slice),
                    );
                }
                self.gl.draw_elements(
                    glow::TRIANGLES,
                    (run.count * 6) as i32,
                    glow::UNSIGNED_INT,
                    (local_first * 6 * std::mem::size_of::<u32>()) as i32,
                );
            }
        }
        unsafe {
            self.gl.bind_vertex_array(None);
        }
    }
}

impl Drop for SpriteRenderer {
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

    fn look_down_y() -> Mat4 {
        // Eye at origin looking +Y, up +Z (matches d3dx billboard test).
        Mat4::look_at_lh(Vec3::ZERO, Vec3::new(0.0, 1.0, 0.0), Vec3::Z)
    }

    #[test]
    fn billboard_matrix_rows_are_the_view_basis() {
        let view = look_down_y();
        let bb = billboard_matrix(&view);
        // right=(-1,0,0), up=(0,0,1), forward=(0,1,0) for this camera.
        assert!((bb.get(0, 0) - -1.0).abs() < 1e-6);
        assert!((bb.get(1, 2) - 1.0).abs() < 1e-6); // up row -> z
        assert!((bb.get(2, 1) - 1.0).abs() < 1e-6); // forward row -> y
    }

    #[test]
    fn billboard_sprite_faces_the_camera_and_has_correct_uvs() {
        let view = look_down_y();
        let data = SpriteData {
            center: Vec3::new(0.0, 10.0, 0.0), // 10 units ahead (down +Y)
            offset: Vec3::ZERO,
            size: Vec2::new(2.0, 2.0),
            rotation: 0.0,
            axis: Axis::PositiveZ,
            color: Rgba::new(1.0, 1.0, 1.0, 1.0),
            texture: 0u32,
        };
        let mut batch = SpriteBatch::new();
        batch.start(view, true, true);
        batch.add(&data);
        let (verts, _runs) = batch.build();
        assert_eq!(verts.len(), 4);
        // A billboard centered 10 ahead sits ~10 in view-space z (forward).
        let avg_z = verts.iter().map(|v| v.position[2]).sum::<f32>() / 4.0;
        assert!((avg_z - 10.0).abs() < 1e-3, "avg z {avg_z}");
        // The quad spans ±1 in view-space x/y (size 2 => half-extent 1).
        let xs: Vec<f32> = verts.iter().map(|v| v.position[0]).collect();
        assert!(xs.iter().cloned().fold(f32::MIN, f32::max) > 0.9);
        assert!(xs.iter().cloned().fold(f32::MAX, f32::min) < -0.9);
        // UVs are the corner + 0.5 => the unit square corners.
        let uvs: Vec<[f32; 2]> = verts.iter().map(|v| v.uv).collect();
        assert!(uvs.contains(&[0.0, 0.0]));
        assert!(uvs.contains(&[1.0, 1.0]));
    }

    #[test]
    fn depth_sort_is_stable_and_back_to_front() {
        let view = look_down_y();
        let mut batch: SpriteBatch<u32> = SpriteBatch::new();
        batch.start(view, true, true);
        let make = |dist: f32, tex: u32| SpriteData {
            center: Vec3::new(0.0, dist, 0.0),
            offset: Vec3::ZERO,
            size: Vec2::new(1.0, 1.0),
            rotation: 0.0,
            axis: Axis::PositiveZ,
            color: Rgba::new(1.0, 1.0, 1.0, 1.0),
            texture: tex,
        };
        // Two near (tex 1 then 2 — equal depth), one far (tex 3).
        batch.add(&make(5.0, 1));
        batch.add(&make(5.0, 2));
        batch.add(&make(20.0, 3));
        let (_verts, runs) = batch.build();
        // Back-to-front: farthest (tex 3) first, then equal-depth pair in
        // insertion order (tex 1, then tex 2) — no drop despite equal keys.
        let tex_order: Vec<u32> = runs.iter().map(|r| r.texture).collect();
        assert_eq!(tex_order, vec![3, 1, 2]);
        let total: usize = runs.iter().map(|r| r.count).sum();
        assert_eq!(total, 3, "no sprite dropped");
    }

    #[test]
    fn draw_runs_merge_same_texture_and_never_drop() {
        // 5 sprites, textures A A B A A.
        let runs = build_draw_runs(&[1u32, 1, 2, 1, 1]);
        assert_eq!(
            runs,
            vec![
                DrawRun { texture: 1, first: 0, count: 2 },
                DrawRun { texture: 2, first: 2, count: 1 },
                DrawRun { texture: 1, first: 3, count: 2 },
            ]
        );
        let total: usize = runs.iter().map(|r| r.count).sum();
        assert_eq!(total, 5);
    }

    #[test]
    fn draw_runs_split_at_batch_boundaries() {
        // 2*BATCH_SIZE sprites, all one texture => one run per batch.
        let textures = vec![7u32; BATCH_SIZE * 2];
        let runs = build_draw_runs(&textures);
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0], DrawRun { texture: 7, first: 0, count: BATCH_SIZE });
        assert_eq!(
            runs[1],
            DrawRun { texture: 7, first: BATCH_SIZE, count: BATCH_SIZE }
        );
    }

    #[test]
    fn sprite_index_pattern_is_0_2_1_1_2_3() {
        assert_eq!(sprite_indices(1), vec![0, 2, 1, 1, 2, 3]);
        assert_eq!(sprite_indices(2), vec![0, 2, 1, 1, 2, 3, 4, 6, 5, 5, 6, 7]);
    }

    #[test]
    fn projectile_black_silhouette_color_survives_conversion() {
        // PARITY (cProjectile.cpp:211): tint literally (0,0,0,1).
        assert_eq!(rgba_to_u8(Rgba::new(0.0, 0.0, 0.0, 1.0)), [0, 0, 0, 255]);
    }
}
