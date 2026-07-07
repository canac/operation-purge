//! Filename-keyed texture cache, replacing `cTextureManager`
//! (source-map §3.2). Bodies land at M3.
//!
//! `image` crate BMP decode (4-bit palettized, NPOT) was spiked at M0
//! (task instructions); `GL_REPEAT` wrap is mandatory (building UV =
//! length/textureSize) and `processNoAlpha` force-opaque is preserved
//! for textures the original loaded without an alpha channel.

use std::collections::HashMap;
use std::path::PathBuf;

pub enum ProcessMode {
    /// Force fully opaque, even if the source has an alpha channel.
    NoAlpha,
    /// Keep the source alpha channel.
    UseAlpha,
}

/// Opaque GL texture handle (once M3 wires up `glow`).
pub struct Texture {
    pub width: u32,
    pub height: u32,
}

/// Global, filename-keyed cache (replacing the original's shared
/// non-refcounted global pointer cache, source-map §3.2).
#[derive(Default)]
pub struct TextureManager {
    // Read once `load()` grows a real body at M3 — until then this is
    // write-only, which trips `dead_code`; silenced rather than removed
    // since the field is exactly what M3 needs.
    #[allow(dead_code)]
    cache: HashMap<PathBuf, Texture>,
}

impl TextureManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Loads (or returns the cached) texture at `path`, with a full mip
    /// chain and `GL_REPEAT` wrap. Body lands at M3.
    pub fn load(&mut self, _path: &std::path::Path, _mode: ProcessMode) -> &Texture {
        todo!("ported at M3 — image-crate BMP decode + GL upload (design §3.1, §4.6)")
    }
}
