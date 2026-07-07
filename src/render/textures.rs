//! Filename-keyed texture cache, replacing `cTextureManager`
//! (source-map §3.2, `cTextureManager.cpp`).
//!
//! Every shipped texture is normalized offline to **PNG RGBA8** (see the
//! README), so decode is a single `image::load_from_memory` call — the crate
//! auto-detects the format and the port needs no per-format branch.
//! `processNoAlpha` forces the texture fully opaque (the original passed
//! `D3DFMT_X8R8G8B8`, `cTextureManager.cpp:68`); `processUseAlpha` keeps the
//! source alpha. The building textures are opaque, the projectile/particle
//! sprites carry real alpha. The luminance-to-alpha mode was dead code in the
//! original (commented out) and is not ported.
//!
//! `GL_REPEAT` wrap is mandatory (building UV = length/textureSize) and
//! `GL_LINEAR`/`GL_LINEAR_MIPMAP_LINEAR` filtering matches the D3D sampler
//! state (design §3.5). Decode is a pure CPU function (unit-tested against
//! known pixel colors of the shipped PNGs); GL upload is compiled but
//! exercised only at runtime.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use glow::HasContext;

use crate::error::GlError;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ProcessMode {
    /// Force fully opaque, even if the source has an alpha channel
    /// (`D3DFMT_X8R8G8B8`).
    NoAlpha,
    /// Keep the source alpha channel (`D3DFMT_A8R8G8B8`).
    UseAlpha,
}

/// A decoded, CPU-side image: tightly packed RGBA8, top row first (the
/// `image` crate's orientation, matching GL's expectation once we treat row
/// 0 as the top). NPOT sizes are legal (design §3.5, `GL_REPEAT`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl DecodedImage {
    /// Pixel at `(x, y)` as `(r, g, b, a)` — test helper.
    pub fn pixel(&self, x: u32, y: u32) -> (u8, u8, u8, u8) {
        let i = ((y * self.width + x) * 4) as usize;
        (
            self.rgba[i],
            self.rgba[i + 1],
            self.rgba[i + 2],
            self.rgba[i + 3],
        )
    }
}

/// Decode image bytes to RGBA8, applying `mode`'s alpha policy. The format is
/// auto-detected (all shipped textures are PNG). Pure — no GL.
pub fn decode_image(bytes: &[u8], mode: ProcessMode) -> Result<DecodedImage, String> {
    let img = image::load_from_memory(bytes).map_err(|e| format!("image decode failed: {e}"))?;
    let mut rgba = img.to_rgba8();
    if mode == ProcessMode::NoAlpha {
        // processNoAlpha: force every texel opaque (X8R8G8B8).
        for px in rgba.chunks_exact_mut(4) {
            px[3] = 0xFF;
        }
    }
    Ok(DecodedImage {
        width: rgba.width(),
        height: rgba.height(),
        rgba: rgba.into_raw(),
    })
}

/// A GL texture handle plus its dimensions.
pub struct Texture {
    pub width: u32,
    pub height: u32,
    pub handle: glow::Texture,
}

impl Texture {
    /// Uploads a decoded image as a mipmapped, `GL_REPEAT`, trilinear-filtered
    /// 2D texture (design §3.5). Runtime-only (needs a current context).
    pub fn upload(gl: &glow::Context, image: &DecodedImage) -> Result<Texture, GlError> {
        unsafe {
            let handle = gl
                .create_texture()
                .map_err(|e| GlError::Context(format!("glGenTextures failed: {e}")))?;
            gl.bind_texture(glow::TEXTURE_2D, Some(handle));
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA8 as i32,
                image.width as i32,
                image.height as i32,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(&image.rgba)),
            );
            gl.generate_mipmap(glow::TEXTURE_2D);
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::LINEAR_MIPMAP_LINEAR as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                glow::LINEAR as i32,
            );
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::REPEAT as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::REPEAT as i32);
            gl.bind_texture(glow::TEXTURE_2D, None);
            Ok(Texture {
                width: image.width,
                height: image.height,
                handle,
            })
        }
    }
}

/// Global, filename-keyed cache (replacing the original's shared
/// non-refcounted global pointer cache, source-map §3.2). Owns the shared
/// context so `load` can upload on cache misses.
pub struct TextureManager {
    gl: Rc<glow::Context>,
    cache: HashMap<PathBuf, Texture>,
}

impl TextureManager {
    pub fn new(gl: Rc<glow::Context>) -> Self {
        Self {
            gl,
            cache: HashMap::new(),
        }
    }

    /// Loads (or returns the cached) texture at `path`. On a miss, reads the
    /// file, decodes per `mode`, and uploads a full mip chain. Runtime-only.
    pub fn load(&mut self, path: &Path, mode: ProcessMode) -> Result<&Texture, GlError> {
        if !self.cache.contains_key(path) {
            let bytes = std::fs::read(path)
                .map_err(|e| GlError::Context(format!("cannot read {}: {e}", path.display())))?;
            let image = decode_image(&bytes, mode)
                .map_err(|e| GlError::Context(format!("{}: {e}", path.display())))?;
            let texture = Texture::upload(&self.gl, &image)?;
            self.cache.insert(path.to_path_buf(), texture);
        }
        Ok(&self.cache[path])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texture_path(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("assets/Textures")
            .join(name)
    }

    fn decode_file(name: &str, mode: ProcessMode) -> DecodedImage {
        let bytes = std::fs::read(texture_path(name)).expect("texture file readable");
        decode_image(&bytes, mode).expect("image decodes")
    }

    #[test]
    fn steel_png_decodes_to_known_pixels() {
        // Steel.png is the offline PNG conversion of the original 32x32 BMP
        // recovered from the MSI; RGB→RGBA is lossless. Pixel values pinned
        // against the real asset (a grey screw-plate on white).
        let img = decode_file("Steel.png", ProcessMode::NoAlpha);
        assert_eq!((img.width, img.height), (32, 32));
        assert_eq!(img.rgba.len(), 32 * 32 * 4);
        assert_eq!(img.pixel(0, 0), (255, 255, 255, 255));
        assert_eq!(img.pixel(16, 16), (192, 192, 192, 255));
        assert_eq!(img.pixel(8, 8), (128, 128, 128, 255));
    }

    #[test]
    fn building_textures_decode_opaque_rgba8() {
        for name in ["Brick.png", "Steel.png", "Ceiling.png"] {
            let img = decode_file(name, ProcessMode::NoAlpha);
            assert_eq!((img.width, img.height), (32, 32), "{name}");
            assert!(img.rgba.chunks_exact(4).all(|p| p[3] == 0xFF), "{name} must be opaque");
        }
    }

    #[test]
    fn sprite_texture_carries_alpha_with_use_alpha() {
        // The projectile/particle sprites carry a real alpha channel; UseAlpha
        // must preserve it (at least one non-opaque texel) while NoAlpha forces
        // every texel opaque.
        let kept = decode_file("Smoke.png", ProcessMode::UseAlpha);
        assert!(
            kept.rgba.chunks_exact(4).any(|p| p[3] != 0xFF),
            "UseAlpha must keep the sprite's transparency"
        );
        let forced = decode_file("Smoke.png", ProcessMode::NoAlpha);
        assert!(forced.rgba.chunks_exact(4).all(|p| p[3] == 0xFF));
    }

    #[test]
    fn garbage_bytes_are_a_clean_error_not_a_panic() {
        assert!(decode_image(b"not an image at all", ProcessMode::NoAlpha).is_err());
    }
}
