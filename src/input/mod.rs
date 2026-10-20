//! Event-accumulated per-frame input snapshot, replacing `cDirectInput`'s
//! per-frame polling.
//!
//! PARITY: `m_bFirstQuery` phantom-edge bug — the first frame's "previous"
//! snapshot equals its "current" snapshot, so no spurious press/release
//! edges fire on frame one (a FIX: no feel impact, only frame-1 correctness).
//! Focus loss clears all pending/current state (DISCL_FOREGROUND parity).

pub mod commands;
pub mod keys;

use keys::{Key, KEY_COUNT};

/// A frame's worth of key/button state, indexed by `Key::ordinal()`.
#[derive(Clone)]
pub struct InputState {
    pub down: [bool; KEY_COUNT],
    /// Accumulated raw mouse deltas since the last frame
    /// (`DeviceEvent::MouseMotion` dx/dy accumulate across the frame).
    pub mouse_delta: (f64, f64),
    /// Accumulated wheel detents (`LineDelta * 120` emulation).
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

    /// Called from the winit event handler as key/button OS events arrive:
    /// sets or clears the pending `down` bit for `key`. PARITY: mirrors
    /// `cDirectInput`'s per-device-poll `abKeyDown[]` array, but driven by
    /// events instead of a poll. Idempotent — winit key
    /// auto-repeat resending `Pressed` while already down is a no-op, so no
    /// phantom edge appears at the next `begin_frame`.
    pub fn on_key_event(&mut self, key: Key, is_down: bool) {
        self.pending.down[key.ordinal() as usize] = is_down;
    }

    /// Accumulate a raw relative mouse motion (`DeviceEvent::MouseMotion`
    /// dx/dy) into the pending frame; deltas sum across all events in a
    /// frame and reset at `begin_frame`.
    pub fn on_mouse_motion(&mut self, dx: f64, dy: f64) {
        self.pending.mouse_delta.0 += dx;
        self.pending.mouse_delta.1 += dy;
    }

    /// Accumulate scroll into the pending frame, expressed in WHEEL_DELTA
    /// detents. PARITY: winit `MouseScrollDelta::LineDelta(_, y)` maps one
    /// line to one detent (the original polled `lMouseMoveZ` in `WHEEL_DELTA`
    /// = 120 units; feel code divides by `fAccelerateRate`, so detents — not
    /// raw 120s — are the natural unit here). Trackpad
    /// `PixelDelta` is normalized by the caller.
    pub fn on_wheel(&mut self, detents: f32) {
        self.pending.wheel_delta += detents;
    }

    /// Snapshots `pending` into `current` (rotating the old `current` into
    /// `previous`) at frame start. FIX: on the very first call, `previous`
    /// is set equal to `current` rather than kept at its all-false default,
    /// suppressing phantom edges.
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

    /// Focus loss clears all state — DISCL_FOREGROUND parity.
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

    /// Accumulated raw mouse delta for the current frame.
    pub fn mouse_delta(&self) -> (f64, f64) {
        self.current.mouse_delta
    }

    /// Accumulated wheel detents for the current frame.
    pub fn wheel_delta(&self) -> f32 {
        self.current.wheel_delta
    }

    /// Whether *any* control-side key is down (either-side,
    /// `cOperationPurge.cpp:3129`).
    pub fn control_down(&self) -> bool {
        self.is_down(Key::ControlLeft) || self.is_down(Key::ControlRight)
    }

    /// Whether *any* alt-side key is down (either-side).
    pub fn alt_down(&self) -> bool {
        self.is_down(Key::AltLeft) || self.is_down(Key::AltRight)
    }

    /// Whether *any* shift-side key is down (either-side).
    pub fn shift_down(&self) -> bool {
        self.is_down(Key::ShiftLeft) || self.is_down(Key::ShiftRight)
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

    #[test]
    fn press_then_release_produces_exactly_one_edge_each() {
        let mut input = Input::new();
        // Frame 0: nothing.
        input.begin_frame();
        assert!(!input.just_pressed(Key::Space));
        // Frame 1: press arrives.
        input.on_key_event(Key::Space, true);
        input.begin_frame();
        assert!(input.just_pressed(Key::Space));
        assert!(input.is_down(Key::Space));
        // Frame 2: still held (no new edge).
        input.begin_frame();
        assert!(!input.just_pressed(Key::Space));
        assert!(input.is_down(Key::Space));
        // Frame 3: release.
        input.on_key_event(Key::Space, false);
        input.begin_frame();
        assert!(input.just_released(Key::Space));
        assert!(!input.is_down(Key::Space));
    }

    #[test]
    fn key_autorepeat_does_not_refire_the_press_edge() {
        let mut input = Input::new();
        input.begin_frame(); // consume the first-frame edge suppression (FIX)
        input.on_key_event(Key::KeyW, true);
        input.begin_frame();
        assert!(input.just_pressed(Key::KeyW));
        // OS auto-repeat resends Pressed mid-frame while already down.
        input.on_key_event(Key::KeyW, true);
        input.begin_frame();
        assert!(!input.just_pressed(Key::KeyW));
        assert!(input.is_down(Key::KeyW));
    }

    #[test]
    fn mouse_and_wheel_deltas_accumulate_then_reset_each_frame() {
        let mut input = Input::new();
        input.on_mouse_motion(3.0, -2.0);
        input.on_mouse_motion(1.5, 0.5);
        input.on_wheel(1.0);
        input.on_wheel(2.0);
        input.begin_frame();
        assert_eq!(input.mouse_delta(), (4.5, -1.5));
        assert_eq!(input.wheel_delta(), 3.0);
        // Next frame with no motion: deltas are back to zero.
        input.begin_frame();
        assert_eq!(input.mouse_delta(), (0.0, 0.0));
        assert_eq!(input.wheel_delta(), 0.0);
    }

    #[test]
    fn either_side_modifier_helpers() {
        let mut input = Input::new();
        input.on_key_event(Key::ControlRight, true);
        input.on_key_event(Key::ShiftLeft, true);
        input.begin_frame();
        assert!(input.control_down());
        assert!(input.shift_down());
        assert!(!input.alt_down());
    }

    #[test]
    fn focus_loss_clears_accumulated_deltas_too() {
        let mut input = Input::new();
        input.on_mouse_motion(10.0, 10.0);
        input.on_wheel(5.0);
        input.begin_frame();
        input.clear_on_focus_loss();
        assert_eq!(input.mouse_delta(), (0.0, 0.0));
        assert_eq!(input.wheel_delta(), 0.0);
    }
}
