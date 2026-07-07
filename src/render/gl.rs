//! GL context owner + global state (design §3.1, §3.5).
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
//!
//! Winding/culling stays **globally disabled** (`WindingDecision::Undetermined`)
//! until the M4 gate signs off its 7-item checklist (design §3.3); no
//! entity rendering may start before that gate.

use std::rc::Rc;

use glow::HasContext;

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

/// Pure, context-free mirror of the global GL state, so the "did this
/// actually change?" gate (which avoids redundant `glDepthMask`/cull calls)
/// is unit-testable without a live GL context. [`GlState`] holds one of
/// these and only emits a real `glow` call when a setter reports a change.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct StateCache {
    pub depth_write: bool,
    pub depth_test: bool,
    pub blend: bool,
    pub winding: WindingDecision,
}

impl StateCache {
    /// The post-`apply_global_state` baseline (design §3.5): depth test on +
    /// writing, blend on, culling undetermined/disabled.
    pub fn initial() -> Self {
        Self {
            depth_write: true,
            depth_test: true,
            blend: true,
            winding: WindingDecision::Undetermined,
        }
    }

    /// Records a requested depth-write state; returns `true` iff it differs
    /// from the cached value (i.e. a real `glDepthMask` call is warranted).
    pub fn set_depth_write(&mut self, enable: bool) -> bool {
        let changed = self.depth_write != enable;
        self.depth_write = enable;
        changed
    }

    /// Records a requested winding decision; returns `true` on a change.
    pub fn set_winding(&mut self, winding: WindingDecision) -> bool {
        let changed = self.winding != winding;
        self.winding = winding;
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

    pub fn cache(&self) -> &StateCache {
        &self.cache
    }

    /// Sets the once-per-run global pipeline state (design §3.5,
    /// `cMainWindow.cpp:878-894`): blend always on with
    /// `SRC_ALPHA`/`ONE_MINUS_SRC_ALPHA`, depth test `LEQUAL` with writing
    /// enabled, culling disabled (M3/§3.3 gate), clear color black. Only
    /// invoked at real runtime with a current context.
    pub fn apply_global_state(&mut self) {
        unsafe {
            self.gl.enable(glow::BLEND);
            self.gl
                .blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
            self.gl.enable(glow::DEPTH_TEST);
            self.gl.depth_func(glow::LEQUAL);
            self.gl.depth_mask(true);
            // GUARD/§3.3: no face culling until the M4 winding gate signs off.
            self.gl.disable(glow::CULL_FACE);
            self.gl.clear_color(0.0, 0.0, 0.0, 1.0);
            self.gl.clear_depth_f32(1.0);
        }
        self.cache = StateCache::initial();
    }

    /// Toggles `glDepthMask` exactly where the original toggled
    /// `D3DRS_ZWRITEENABLE` (design §3.5). Redundant calls are suppressed by
    /// the cache.
    pub fn set_depth_write(&mut self, enable: bool) {
        if self.cache.set_depth_write(enable) {
            unsafe {
                self.gl.depth_mask(enable);
            }
        }
    }

    /// Applies a per-category winding/culling decision once M4 commits one
    /// (design §3.3). `Undetermined`/`CullNone` disable culling; the two
    /// concrete decisions enable back-face culling with the stated front
    /// face. Redundant calls suppressed by the cache.
    pub fn set_winding(&mut self, winding: WindingDecision) {
        if self.cache.set_winding(winding) {
            unsafe {
                match winding {
                    WindingDecision::Undetermined | WindingDecision::CullNone => {
                        self.gl.disable(glow::CULL_FACE);
                    }
                    WindingDecision::CullBackCcwFront => {
                        self.gl.enable(glow::CULL_FACE);
                        self.gl.cull_face(glow::BACK);
                        self.gl.front_face(glow::CCW);
                    }
                    WindingDecision::CullBackCwFront => {
                        self.gl.enable(glow::CULL_FACE);
                        self.gl.cull_face(glow::BACK);
                        self.gl.front_face(glow::CW);
                    }
                }
            }
        }
    }

    /// Physical-pixel viewport (design §3.1: `glViewport` uses physical
    /// `inner_size`, never logical).
    pub fn set_viewport(&self, width: i32, height: i32) {
        unsafe {
            self.gl.viewport(0, 0, width, height);
        }
    }

    /// Clears color (black) + depth (1.0) — start of every frame (§3.5).
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
        assert!(c.depth_test);
        assert!(c.blend);
        assert_eq!(c.winding, WindingDecision::Undetermined);
    }

    #[test]
    fn depth_write_setter_reports_changes_and_suppresses_redundant() {
        let mut c = StateCache::initial();
        // Already true ⇒ no change ⇒ no GL call.
        assert!(!c.set_depth_write(true));
        // Flip to false ⇒ change.
        assert!(c.set_depth_write(false));
        assert!(!c.depth_write);
        // Redundant false ⇒ no change.
        assert!(!c.set_depth_write(false));
        // Back to true ⇒ change.
        assert!(c.set_depth_write(true));
    }

    #[test]
    fn winding_setter_reports_changes() {
        let mut c = StateCache::initial();
        assert!(!c.set_winding(WindingDecision::Undetermined));
        assert!(c.set_winding(WindingDecision::CullBackCcwFront));
        assert!(!c.set_winding(WindingDecision::CullBackCcwFront));
        assert!(c.set_winding(WindingDecision::CullNone));
    }
}
