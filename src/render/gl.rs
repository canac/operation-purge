//! GL context owner + global state.
//!
//! All `glow` calls the renderer makes for *global* pipeline state funnel
//! through [`GlState`], which owns the shared `glow::Context` and a small
//! [`StateCache`] that suppresses redundant driver calls. The context is
//! created in `app/window.rs` (real runtime only — never in unit tests,
//! which exercise [`StateCache`]'s pure decision logic instead).
//!
//! Mirrors the D3D9 state the original set once in `cMainWindow.cpp:878-894`
//! (Z-test on + LESSEQUAL, alpha blend SRCALPHA/INVSRCALPHA, stage-0
//! MODULATE, linear filtering) — texture filtering/wrap are per-texture
//! parameters in GL, set at upload in `render/textures.rs`, not here.

use std::rc::Rc;

use glow::HasContext;

/// `D3DRS_CULLMODE`, restricted to the two modes the original uses.
///
/// D3D's front face is clockwise on screen and GL's is counter-clockwise;
/// the port keeps D3D's screen-space x/y (`CLIP_FIX` only remaps z), so
/// `D3DCULL_CCW` maps to `glFrontFace(GL_CW)` + `glCullFace(GL_BACK)`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CullMode {
    /// `D3DCULL_NONE`.
    None,
    /// `D3DCULL_CCW`: cull triangles that wind counter-clockwise on screen.
    Ccw,
}

/// Pure, context-free mirror of the global GL state, so the "did this
/// actually change?" gate is unit-testable without a live GL context.
/// [`GlState`] only emits a real `glow` call when a setter reports a change.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct StateCache {
    pub depth_write: bool,
    pub cull: CullMode,
}

impl StateCache {
    /// The post-[`GlState::apply_global_state`] baseline: depth writes on,
    /// culling off.
    pub fn initial() -> Self {
        Self {
            depth_write: true,
            cull: CullMode::None,
        }
    }

    /// Records a requested depth-write state; returns `true` iff it differs
    /// from the cached value (i.e. a real `glDepthMask` call is warranted).
    pub fn set_depth_write(&mut self, enable: bool) -> bool {
        let changed = self.depth_write != enable;
        self.depth_write = enable;
        changed
    }

    /// Records a requested cull mode; returns `true` on a change.
    pub fn set_cull(&mut self, cull: CullMode) -> bool {
        let changed = self.cull != cull;
        self.cull = cull;
        changed
    }
}

impl Default for StateCache {
    fn default() -> Self {
        Self::initial()
    }
}

/// Owns the shared `glow::Context` and the [`StateCache`]. Constructed at
/// runtime by `app/window.rs` once the GL context exists.
pub struct GlState {
    gl: Rc<glow::Context>,
    cache: StateCache,
}

impl GlState {
    /// Wraps a live context. Does **not** issue GL calls — call
    /// [`GlState::apply_global_state`] once the context is current.
    pub fn new(gl: Rc<glow::Context>) -> Self {
        Self {
            gl,
            cache: StateCache::initial(),
        }
    }

    /// Shared context handle (for shader/mesh/texture modules).
    pub fn context(&self) -> &Rc<glow::Context> {
        &self.gl
    }

    /// Sets the once-per-run global pipeline state (`cMainWindow.cpp:878-894`):
    /// blend always on with `SRC_ALPHA`/`ONE_MINUS_SRC_ALPHA`, depth test
    /// `LEQUAL` with writing enabled, culling off, clear color black.
    pub fn apply_global_state(&mut self) {
        unsafe {
            self.gl.enable(glow::BLEND);
            self.gl
                .blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
            self.gl.enable(glow::DEPTH_TEST);
            self.gl.depth_func(glow::LEQUAL);
            self.gl.depth_mask(true);
            self.gl.disable(glow::CULL_FACE);
            self.gl.cull_face(glow::BACK);
            self.gl.front_face(glow::CW);
            self.gl.clear_color(0.0, 0.0, 0.0, 1.0);
            self.gl.clear_depth_f32(1.0);
        }
        self.cache = StateCache::initial();
    }

    /// Toggles `glDepthMask` exactly where the original toggled
    /// `D3DRS_ZWRITEENABLE`. Redundant calls are suppressed by the cache.
    pub fn set_depth_write(&mut self, enable: bool) {
        if self.cache.set_depth_write(enable) {
            unsafe {
                self.gl.depth_mask(enable);
            }
        }
    }

    /// Applies a `D3DRS_CULLMODE`. Front face and cull face are fixed in
    /// [`GlState::apply_global_state`], so only `GL_CULL_FACE` toggles.
    pub fn set_cull(&mut self, cull: CullMode) {
        if self.cache.set_cull(cull) {
            unsafe {
                match cull {
                    CullMode::None => self.gl.disable(glow::CULL_FACE),
                    CullMode::Ccw => self.gl.enable(glow::CULL_FACE),
                }
            }
        }
    }

    /// Physical-pixel viewport (`glViewport` uses physical `inner_size`,
    /// never logical).
    pub fn set_viewport(&self, width: i32, height: i32) {
        unsafe {
            self.gl.viewport(0, 0, width, height);
        }
    }

    /// Clears color (black) + depth (1.0) — start of every frame.
    pub fn clear(&self) {
        unsafe {
            self.gl
                .clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_cache_matches_the_global_state_baseline() {
        let c = StateCache::initial();
        assert!(c.depth_write);
        assert_eq!(c.cull, CullMode::None);
    }

    #[test]
    fn depth_write_setter_reports_changes_and_suppresses_redundant() {
        let mut c = StateCache::initial();
        assert!(!c.set_depth_write(true));
        assert!(c.set_depth_write(false));
        assert!(!c.depth_write);
        assert!(!c.set_depth_write(false));
        assert!(c.set_depth_write(true));
    }

    #[test]
    fn cull_setter_reports_changes() {
        let mut c = StateCache::initial();
        assert!(!c.set_cull(CullMode::None));
        assert!(c.set_cull(CullMode::Ccw));
        assert!(!c.set_cull(CullMode::Ccw));
        assert!(c.set_cull(CullMode::None));
    }
}
