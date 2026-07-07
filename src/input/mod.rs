//! Event-accumulated per-frame input snapshot, replacing `cDirectInput`'s
//! per-frame polling (design §2, §5.2).
//!
//! PARITY: `m_bFirstQuery` phantom-edge bug — the first frame's "previous"
//! snapshot equals its "current" snapshot, so no spurious press/release
//! edges fire on frame one (design §1.3 FIX ledger; this is filed as a FIX
//! because it has no feel impact, only observable-on-frame-1 correctness).
//! Focus loss clears all pending/current state (DISCL_FOREGROUND parity).

pub mod commands;
pub mod keys;

use keys::{Key, KEY_COUNT};

/// A frame's worth of key/button state, indexed by `Key::ordinal()`.
#[derive(Clone)]
pub struct InputState {
    pub down: [bool; KEY_COUNT],
    /// Accumulated raw mouse deltas since the last frame (design §5.2:
    /// `DeviceEvent::MouseMotion` dx/dy accumulate across the frame).
    pub mouse_delta: (f64, f64),
    /// Accumulated wheel detents (`LineDelta * 120` emulation, design §5.2).
    pub wheel_delta: f32,
}

impl Default for InputState {
    fn default() -> Self {
        Self {
            down: [false; KEY_COUNT],
            mouse_delta: (0.0, 0.0),
            wheel_delta: 0.0,
        }
    }
}

impl InputState {
    pub fn is_down(&self, key: Key) -> bool {
        self.down[key.ordinal() as usize]
    }
}

/// Owns the pending (being accumulated from OS events) and current/previous
/// (frame-boundary snapshots, for edge detection) input states.
pub struct Input {
    pending: InputState,
    current: InputState,
    previous: InputState,
    first_frame: bool,
}

impl Default for Input {
    fn default() -> Self {
        Self::new()
    }
}

impl Input {
    pub fn new() -> Self {
        Self {
            pending: InputState::default(),
            current: InputState::default(),
            previous: InputState::default(),
            first_frame: true,
        }
    }

    /// Called from the winit event handler as OS events arrive; body
    /// (setting/clearing `pending.down`, accumulating deltas) lands at M3
    /// alongside the winit event loop.
    pub fn on_key_event(&mut self, _key: Key, _is_down: bool) {
        todo!("wired to winit KeyEvent at M3 (design §5.2)")
    }

    /// Snapshots `pending` into `current` (rotating the old `current` into
    /// `previous`) at frame start. FIX: on the very first call, `previous`
    /// is set equal to `current` rather than kept at its all-false default,
    /// suppressing phantom edges (design §1.3).
    pub fn begin_frame(&mut self) {
        self.previous = self.current.clone();
        self.current = self.pending.clone();
        if self.first_frame {
            self.previous = self.current.clone();
            self.first_frame = false;
        }
        self.pending.mouse_delta = (0.0, 0.0);
        self.pending.wheel_delta = 0.0;
    }

    /// Focus loss clears all state — DISCL_FOREGROUND parity (design §5.2).
    pub fn clear_on_focus_loss(&mut self) {
        self.pending = InputState::default();
        self.current = InputState::default();
        self.previous = InputState::default();
    }

    pub fn just_pressed(&self, key: Key) -> bool {
        self.current.is_down(key) && !self.previous.is_down(key)
    }

    pub fn just_released(&self, key: Key) -> bool {
        !self.current.is_down(key) && self.previous.is_down(key)
    }

    pub fn is_down(&self, key: Key) -> bool {
        self.current.is_down(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_frame_has_no_phantom_edges() {
        let mut input = Input::new();
        input.pending.down[Key::KeyA.ordinal() as usize] = true;
        input.begin_frame();
        // Held down on frame one ⇒ current == previous ⇒ not a "just pressed" edge.
        assert!(input.is_down(Key::KeyA));
        assert!(!input.just_pressed(Key::KeyA));
    }

    #[test]
    fn focus_loss_clears_everything() {
        let mut input = Input::new();
        input.pending.down[Key::KeyA.ordinal() as usize] = true;
        input.begin_frame();
        input.clear_on_focus_loss();
        assert!(!input.is_down(Key::KeyA));
    }
}
