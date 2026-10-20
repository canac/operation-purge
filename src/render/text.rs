//! HUD text via an `ab_glyph` atlas, replacing `ID3DXFont`/`ID3DXSprite`.
//!
//! Liberation Sans Bold (SIL OFL, Arial-metric-compatible) at 15 logical px
//! × `hud_scale`, baked into a single-channel (R8 coverage) shelf-packed
//! atlas. The bake — glyph metrics, coverage rasterization, and shelf
//! packing — is pure CPU and unit-tested without a GL context; the atlas
//! upload and the small text-draw shader are compiled but exercised only at
//! runtime. Wayland `ScaleFactorChanged` re-bakes at the new scale.
//!
//! The text program samples the R8 coverage as alpha under a uniform color,
//! which `prog_sprite`'s RGBA modulate cannot express, so it stays separate.

use std::collections::HashMap;
use std::rc::Rc;

use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use glow::HasContext;

use crate::error::GlError;
use crate::render::shaders::Program;

/// HUD font size in logical pixels before the DPI/`hud_scale` multiplier.
pub const HUD_FONT_LOGICAL_PX: f32 = 15.0;

/// Printable ASCII range baked into the atlas (space..=tilde).
const FIRST_CHAR: char = ' ';
const LAST_CHAR: char = '~';

/// 1px gutter between packed glyphs so trilinear/edge sampling never bleeds
/// a neighbor.
const PADDING: u32 = 1;

/// Bundled font (SIL OFL). Embedded so there is no runtime file dependency
/// for text.
const FONT_BYTES: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/Fonts/LiberationSans-Bold.ttf"
));

/// Per-glyph metrics + atlas placement (all in device pixels at the baked
/// scale). A glyph with no outline (e.g. space) has zero `size` and an
/// empty UV rect but a real `advance`.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct GlyphInfo {
    /// Horizontal pen advance after drawing this glyph.
    pub advance: f32,
    /// Left/top bearing of the coverage bitmap relative to the pen origin on
    /// the baseline (x right, y down; `top` is negative for ascenders).
    pub bearing: (f32, f32),
    /// Coverage bitmap size in pixels.
    pub size: (u32, u32),
    /// UV rect (normalized), `uv_min` = top-left, `uv_max` = bottom-right.
    pub uv_min: (f32, f32),
    pub uv_max: (f32, f32),
}

/// A fully baked, CPU-side glyph atlas: R8 coverage bitmap + glyph table +
/// vertical metrics. Pure output of [`BakedAtlas::bake`].
pub struct BakedAtlas {
    pub scale: f32,
    /// Device-pixel font size (`HUD_FONT_LOGICAL_PX * scale`).
    pub px: f32,
    pub ascent: f32,
    pub descent: f32,
    pub line_height: f32,
    pub width: u32,
    pub height: u32,
    /// Single-channel coverage, row-major, top row first (`width*height`).
    pub coverage: Vec<u8>,
    pub glyphs: HashMap<char, GlyphInfo>,
}

impl BakedAtlas {
    /// Rasterize + shelf-pack the printable-ASCII glyphs of Liberation Sans
    /// Bold at `HUD_FONT_LOGICAL_PX * scale`. Pure CPU (no GL context).
    pub fn bake(scale: f32) -> Result<BakedAtlas, GlError> {
        let font = FontRef::try_from_slice(FONT_BYTES)
            .map_err(|e| GlError::Context(format!("font load failed: {e}")))?;
        let px = HUD_FONT_LOGICAL_PX * scale;
        let scaled = font.as_scaled(PxScale::from(px));

        // Fixed atlas width; height grows by shelves to fit. 256px comfortably
        // holds the ~95 printable ASCII glyphs at 15–30px in a handful of rows.
        let atlas_width: u32 = 256;

        // First pass: rasterize each glyph's coverage + metrics, and pack.
        let mut glyphs: HashMap<char, GlyphInfo> = HashMap::new();
        let mut coverages: Vec<(char, u32, u32, u32, u32, Vec<u8>)> = Vec::new(); // ch,x,y,w,h,pixels

        let mut pen_x: u32 = PADDING;
        let mut pen_y: u32 = PADDING;
        let mut shelf_height: u32 = 0;

        for code in (FIRST_CHAR as u32)..=(LAST_CHAR as u32) {
            let ch = char::from_u32(code).unwrap();
            let glyph_id = font.glyph_id(ch);
            let advance = scaled.h_advance(glyph_id);

            let outlined = font.outline_glyph(glyph_id.with_scale(px));
            let (w, h, bearing, pixels) = match &outlined {
                Some(o) => {
                    let bounds = o.px_bounds();
                    let w = bounds.width().ceil() as u32;
                    let h = bounds.height().ceil() as u32;
                    let mut buf = vec![0u8; (w * h) as usize];
                    o.draw(|gx, gy, c| {
                        if gx < w && gy < h {
                            buf[(gy * w + gx) as usize] = (c * 255.0 + 0.5) as u8;
                        }
                    });
                    (w, h, (bounds.min.x, bounds.min.y), buf)
                }
                None => (0, 0, (0.0, 0.0), Vec::new()),
            };

            // A glyph with no bitmap (space etc.) is advance-only.
            if w > 0 && h > 0 {
                if pen_x + w + PADDING > atlas_width {
                    // Wrap to the next shelf.
                    pen_x = PADDING;
                    pen_y += shelf_height + PADDING;
                    shelf_height = 0;
                }
                coverages.push((ch, pen_x, pen_y, w, h, pixels));
                pen_x += w + PADDING;
                shelf_height = shelf_height.max(h);
            }

            glyphs.insert(
                ch,
                GlyphInfo {
                    advance,
                    bearing,
                    size: (w, h),
                    // UVs filled after we know the final atlas height.
                    uv_min: (0.0, 0.0),
                    uv_max: (0.0, 0.0),
                },
            );
        }

        let atlas_height = (pen_y + shelf_height + PADDING).max(1);
        let mut coverage = vec![0u8; (atlas_width * atlas_height) as usize];

        for (ch, ax, ay, w, h, pixels) in &coverages {
            for row in 0..*h {
                for col in 0..*w {
                    let src = (row * w + col) as usize;
                    let dst = ((ay + row) * atlas_width + (ax + col)) as usize;
                    coverage[dst] = pixels[src];
                }
            }
            if let Some(g) = glyphs.get_mut(ch) {
                g.uv_min = (*ax as f32 / atlas_width as f32, *ay as f32 / atlas_height as f32);
                g.uv_max = (
                    (*ax + *w) as f32 / atlas_width as f32,
                    (*ay + *h) as f32 / atlas_height as f32,
                );
            }
        }

        Ok(BakedAtlas {
            scale,
            px,
            ascent: scaled.ascent(),
            descent: scaled.descent(),
            line_height: scaled.height() + scaled.line_gap(),
            width: atlas_width,
            height: atlas_height,
            coverage,
            glyphs,
        })
    }
}

/// Runtime glyph atlas: a baked atlas uploaded as an R8 GL texture (deleted
/// on drop). Constructed at runtime once a GL context exists.
pub struct GlyphAtlas {
    gl: Rc<glow::Context>,
    pub baked: BakedAtlas,
    texture: glow::Texture,
}

impl GlyphAtlas {
    /// Bakes at `scale` and uploads the coverage atlas as an R8 texture
    /// (`GL_CLAMP_TO_EDGE`, linear). Runtime-only.
    pub fn build(gl: Rc<glow::Context>, scale: f32) -> Result<GlyphAtlas, GlError> {
        let baked = BakedAtlas::bake(scale)?;
        let texture = Self::upload(&gl, &baked)?;
        Ok(GlyphAtlas { gl, baked, texture })
    }

    fn upload(gl: &glow::Context, baked: &BakedAtlas) -> Result<glow::Texture, GlError> {
        unsafe {
            let tex = gl
                .create_texture()
                .map_err(|e| GlError::Context(format!("glyph atlas texture: {e}")))?;
            gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::R8 as i32,
                baked.width as i32,
                baked.height as i32,
                0,
                glow::RED,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(&baked.coverage)),
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::LINEAR as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                glow::LINEAR as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_S,
                glow::CLAMP_TO_EDGE as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_T,
                glow::CLAMP_TO_EDGE as i32,
            );
            gl.bind_texture(glow::TEXTURE_2D, None);
            Ok(tex)
        }
    }

    /// The GL texture handle (for the text-draw program).
    pub fn texture(&self) -> glow::Texture {
        self.texture
    }

    /// Re-bakes at a new scale (Wayland `ScaleFactorChanged`) and re-uploads,
    /// releasing the old texture. Runtime-only.
    pub fn rebake(&mut self, scale: f32) -> Result<(), GlError> {
        let baked = BakedAtlas::bake(scale)?;
        let texture = Self::upload(&self.gl, &baked)?;
        unsafe {
            self.gl.delete_texture(self.texture);
        }
        self.baked = baked;
        self.texture = texture;
        Ok(())
    }
}

impl Drop for GlyphAtlas {
    fn drop(&mut self) {
        unsafe { self.gl.delete_texture(self.texture) };
    }
}

/// GLSL 330 for the text program: pixel-space ortho, R8 coverage sampled as
/// alpha, uniform color.
const TEXT_VS: &str = r#"#version 330 core
layout(location = 0) in vec2 a_pos;
layout(location = 1) in vec2 a_uv;
uniform vec2 u_screen;
out vec2 v_uv;
void main() {
    // (0,0) = top-left pixel; y grows downward.
    float ndc_x = a_pos.x / u_screen.x * 2.0 - 1.0;
    float ndc_y = 1.0 - a_pos.y / u_screen.y * 2.0;
    v_uv = a_uv;
    gl_Position = vec4(ndc_x, ndc_y, 0.0, 1.0);
}
"#;

const TEXT_FS: &str = r#"#version 330 core
in vec2 v_uv;
uniform sampler2D u_tex;
uniform vec4 u_color;
out vec4 frag;
void main() {
    float coverage = texture(u_tex, v_uv).r;
    frag = vec4(u_color.rgb, u_color.a * coverage);
}
"#;

/// Minimal runtime text drawer: owns the text program and streams glyph
/// quads through a dynamic VBO. Runtime-only. Draws single lines at a
/// pixel origin; y is the text *baseline* offset by ascent inside [`draw`].
pub struct TextRenderer {
    gl: Rc<glow::Context>,
    program: Program,
    vao: glow::VertexArray,
    vbo: glow::Buffer,
}

impl TextRenderer {
    pub fn new(gl: Rc<glow::Context>) -> Result<Self, GlError> {
        let program = Program::link(&gl, TEXT_VS, TEXT_FS)?;
        unsafe {
            let vao = gl
                .create_vertex_array()
                .map_err(|e| GlError::Context(format!("text vao: {e}")))?;
            let vbo = gl
                .create_buffer()
                .map_err(|e| GlError::Context(format!("text vbo: {e}")))?;
            gl.bind_vertex_array(Some(vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            let stride = 4 * std::mem::size_of::<f32>() as i32; // x,y,u,v
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, stride, 0);
            gl.enable_vertex_attrib_array(1);
            gl.vertex_attrib_pointer_f32(1, 2, glow::FLOAT, false, stride, 2 * 4);
            gl.bind_vertex_array(None);

            Ok(Self {
                gl,
                program,
                vao,
                vbo,
            })
        }
    }

    /// Draws `text` with its top-left at pixel `(x, y)` in `color` (RGBA
    /// 0..1). `screen` is the physical framebuffer size. Runtime-only.
    pub fn draw(
        &self,
        atlas: &GlyphAtlas,
        text: &str,
        x: f32,
        y: f32,
        color: [f32; 4],
        screen: (u32, u32),
    ) {
        let mut verts: Vec<f32> = Vec::with_capacity(text.len() * 24);
        let mut pen_x = x;
        let baseline = y + atlas.baked.ascent; // top of line → baseline
        for ch in text.chars() {
            if let Some(g) = atlas.baked.glyphs.get(&ch) {
                let (gw, gh) = g.size;
                if gw > 0 && gh > 0 {
                    let x0 = pen_x + g.bearing.0;
                    let y0 = baseline + g.bearing.1;
                    let x1 = x0 + gw as f32;
                    let y1 = y0 + gh as f32;
                    let (u0, v0) = g.uv_min;
                    let (u1, v1) = g.uv_max;
                    // Two triangles.
                    verts.extend_from_slice(&[
                        x0, y0, u0, v0, x1, y0, u1, v0, x1, y1, u1, v1, x0, y0, u0, v0, x1, y1, u1,
                        v1, x0, y1, u0, v1,
                    ]);
                }
                pen_x += g.advance;
            }
        }
        if verts.is_empty() {
            return;
        }
        let prog = &self.program;
        prog.bind();
        prog.set_vec2(&prog.u.screen, screen.0 as f32, screen.1 as f32);
        prog.set_color(&prog.u.color, crate::math::Rgba::new(color[0], color[1], color[2], color[3]));
        unsafe {
            let gl = &self.gl;
            gl.bind_texture(glow::TEXTURE_2D, Some(atlas.texture()));
            gl.bind_vertex_array(Some(self.vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
            // Buffer orphaning: discard then refill.
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck::cast_slice(&verts),
                glow::STREAM_DRAW,
            );
            gl.draw_arrays(glow::TRIANGLES, 0, (verts.len() / 4) as i32);
            gl.bind_vertex_array(None);
        }
    }
}

impl Drop for TextRenderer {
    fn drop(&mut self) {
        unsafe {
            self.gl.delete_vertex_array(self.vao);
            self.gl.delete_buffer(self.vbo);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bake_covers_all_printable_ascii() {
        let atlas = BakedAtlas::bake(1.0).unwrap();
        for code in (FIRST_CHAR as u32)..=(LAST_CHAR as u32) {
            let ch = char::from_u32(code).unwrap();
            assert!(atlas.glyphs.contains_key(&ch), "missing glyph {ch:?}");
        }
        // 15 logical px at scale 1.0.
        assert_eq!(atlas.px, 15.0);
        assert!(atlas.ascent > 0.0 && atlas.descent < 0.0);
    }

    #[test]
    fn glyph_bitmaps_stay_inside_the_atlas_and_do_not_overlap() {
        let atlas = BakedAtlas::bake(2.0).unwrap();
        // Collect placed (non-empty) glyph rects.
        let mut rects: Vec<(u32, u32, u32, u32)> = Vec::new();
        for g in atlas.glyphs.values() {
            let (w, h) = g.size;
            if w == 0 || h == 0 {
                continue;
            }
            let x = (g.uv_min.0 * atlas.width as f32).round() as u32;
            let y = (g.uv_min.1 * atlas.height as f32).round() as u32;
            assert!(x + w <= atlas.width, "glyph overflows atlas width");
            assert!(y + h <= atlas.height, "glyph overflows atlas height");
            // UVs are normalized and ordered.
            assert!(g.uv_min.0 >= 0.0 && g.uv_max.0 <= 1.0);
            assert!(g.uv_min.1 >= 0.0 && g.uv_max.1 <= 1.0);
            assert!(g.uv_max.0 > g.uv_min.0 && g.uv_max.1 > g.uv_min.1);
            rects.push((x, y, w, h));
        }
        assert!(!rects.is_empty());
        for (i, a) in rects.iter().enumerate() {
            for b in rects.iter().skip(i + 1) {
                let disjoint = a.0 + a.2 <= b.0
                    || b.0 + b.2 <= a.0
                    || a.1 + a.3 <= b.1
                    || b.1 + b.3 <= a.1;
                assert!(disjoint, "glyph rects overlap: {a:?} vs {b:?}");
            }
        }
        assert_eq!(atlas.coverage.len(), (atlas.width * atlas.height) as usize);
    }

    #[test]
    fn advance_scales_linearly_with_scale_factor() {
        let a1 = BakedAtlas::bake(1.0).unwrap();
        let a2 = BakedAtlas::bake(2.0).unwrap();
        let adv1 = a1.glyphs[&'M'].advance;
        let adv2 = a2.glyphs[&'M'].advance;
        assert!(adv1 > 0.0);
        assert!((adv2 - adv1 * 2.0).abs() < 1e-3, "{adv2} vs {adv1}*2");
    }

    #[test]
    fn space_has_width_but_no_bitmap() {
        let atlas = BakedAtlas::bake(1.0).unwrap();
        let space = atlas.glyphs[&' '];
        assert_eq!(space.size, (0, 0));
        assert!(space.advance > 0.0);
    }
}
