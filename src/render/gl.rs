//! GL context owner + global state (design §3.1, §3.5). Kept as a type
//! stub through M0/M3 — real `glow` calls land at M3, with the
//! winding/culling decision gated empirically at M4 (design §3.3;
//! **no entity rendering before that gate passes**).

/// Per-draw-category front-face/cull decision, filled in once the M4
/// checklist (design §3.3, 7 items) signs off. Culling starts globally
/// disabled.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum WindingDecision {
    /// Not yet empirically verified — culling disabled for this category.
    Undetermined,
    CullBackCcwFront,
    CullBackCwFront,
    CullNone,
}

/// Owns the `glow::Context` (once M3 creates one) and the small pile of
/// global GL state the original set once in `cMainWindow.cpp:878-894`
/// (depth test, blend mode, texture filtering, wrap mode).
pub struct GlState {
    pub depth_write_enabled: bool,
    pub winding: WindingDecision,
}

impl GlState {
    pub fn new() -> Self {
        Self {
            depth_write_enabled: true,
            winding: WindingDecision::Undetermined,
        }
    }

    /// `glEnable(GL_BLEND)` + `GL_SRC_ALPHA`/`GL_ONE_MINUS_SRC_ALPHA`
    /// (always on), `glDepthFunc(GL_LEQUAL)`, `GL_LINEAR` /
    /// `GL_LINEAR_MIPMAP_LINEAR`, `GL_REPEAT` (design §3.5). Body lands at M3.
    pub fn apply_global_state(&self) {
        todo!("ported at M3 — global GL state (design §3.5)")
    }
}

impl Default for GlState {
    fn default() -> Self {
        Self::new()
    }
}
