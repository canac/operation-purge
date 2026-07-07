//! HUD text via an `ab_glyph` atlas, replacing `ID3DXFont`/`ID3DXSprite`
//! (design §3.6). Body lands at M3.
//!
//! Liberation Sans Bold (SIL OFL, Arial-metric-compatible) at 15 logical
//! px x `hud_scale`; ortho pixel-space projection anchored at (0,0).
//! Wayland `ScaleFactorChanged` must re-bake the atlas (design §3.1).

pub const HUD_FONT_LOGICAL_PX: f32 = 15.0;

pub struct GlyphAtlas {
    pub scale: f32,
}

impl GlyphAtlas {
    /// Rasterizes the bundled `LiberationSans-Bold.ttf` into a GL texture
    /// atlas at `logical_px * scale`. Body lands at M3.
    pub fn build(_scale: f32) -> Result<Self, crate::error::GlError> {
        todo!("ported at M3 — ab_glyph atlas bake (design §3.6)")
    }

    /// Re-bakes at a new scale factor (Wayland `ScaleFactorChanged`).
    pub fn rebake(&mut self, _scale: f32) {
        todo!("ported at M3 — atlas re-bake (design §3.1)")
    }
}
