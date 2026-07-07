//! Application shell: winit `ApplicationHandler`, frame pump, focus gate,
//! redraw (design §2, §5.1 mirrors `cMainWindow::FrameChange`).

pub mod config;
pub mod window;

use crate::error::GameError;

/// Top-level app state: window/GL context, timing, and (once M8 lands) the
/// `Game`. Kept as a type stub through M0 — see `window::Window` for why
/// winit/glutin APIs aren't exercised yet.
pub struct App {
    pub config: config::Config,
    /// `true` while the window has input focus; gates the sim step exactly
    /// where the original gated on `DISCL_FOREGROUND` (design §5.1).
    pub focused: bool,
    /// Wall-clock timestamp of the previous frame, for the `dt = min(now -
    /// last, 1.0s)` clamp (design §5.1, `cMainWindow.cpp:923-946`).
    pub last_frame: Option<std::time::Instant>,
}

impl App {
    pub fn new(config: config::Config) -> Self {
        Self {
            config,
            focused: true,
            last_frame: None,
        }
    }

    /// One iteration of the frame pump: `dt` clamp → input snapshot →
    /// `execute_commands` (always) → sim step (only if focused) → render →
    /// swap (design §5.1). Body lands at M3+ as the render/sim/input
    /// subsystems come online.
    pub fn pump_frame(&mut self) -> Result<(), GameError> {
        todo!("ported at M3+ — frame pump per design §5.1")
    }

    /// `dt = min(now - last, 1.0 s)` (PARITY: `cMainWindow.cpp:933`).
    pub fn next_dt(&mut self) -> f32 {
        let now = std::time::Instant::now();
        let dt = match self.last_frame {
            Some(last) => now.duration_since(last).as_secs_f32().min(1.0),
            None => 0.0,
        };
        self.last_frame = Some(now);
        dt
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_frame_dt_is_zero_not_a_giant_spike() {
        let mut app = App::new(config::Config::default());
        assert_eq!(app.next_dt(), 0.0);
    }
}
