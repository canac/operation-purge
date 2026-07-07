//! Window + GL context creation (design §3.1).
//!
//! Borderless fullscreen on the primary monitor, physical-resolution
//! framebuffer, DPI scale tracking, cursor grab/release for 3D vs
//! 2D/menu modes. Deliberately kept a type stub through M0: exercising
//! `winit`/`glutin` APIs is M3/M4 work (task instructions, §6 "Do NOT
//! exercise winit/glutin/glow/kira/ab_glyph APIs yet").

/// Owns the winit window + glutin GL context once created (M3).
pub struct Window {
    /// Physical framebuffer size in pixels (Retina-aware; distinct from
    /// logical size — design §1.4.13, the top HUD bug source on macOS).
    pub physical_size: (u32, u32),
    /// DPI scale factor reported by the platform; HUD/text metrics are
    /// logical px × this factor.
    pub scale_factor: f64,
}

impl Window {
    /// Creates the borderless-fullscreen window + OpenGL 3.3 core context
    /// (macOS negotiates 4.1 core; design §3.1). Body lands at M3.
    pub fn create() -> Result<Self, crate::error::GlError> {
        todo!("ported at M3 — winit/glutin window + context creation (design §3.1)")
    }

    /// Swaps buffers; render-then-swap only, no present-previous-frame
    /// latency (design §1.3 FIX ledger — the original's 1-frame latency
    /// is deliberately dropped).
    pub fn swap_buffers(&self) {
        todo!("ported at M3 — glutin swap_buffers (design §3.1)")
    }
}
