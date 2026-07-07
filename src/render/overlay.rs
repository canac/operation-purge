//! 2D map + crosshair + health bars + viewport management, replacing
//! `cOverlay2D`'s geometry half (source-map §3.2). Body lands at M5;
//! reused by the New Game screen, modals, and profiler pane (design
//! §3.6 "one render path, no second UI stack").
//!
//! PARITY: viewport offset 0.025 / size 0.95 (2D mode), 0.05 / 0.9
//! (minimap); inverted-Y arrow pan (down = Up); crosshair = 4 segments
//! +-0.2 -> +-0.05 NDC; health bars 1.0x0.1 above robots, green on red.

/// 2D map viewport (design §3.6, §7 constants table).
pub const MAP_VIEWPORT_OFFSET: f32 = 0.025;
pub const MAP_VIEWPORT_SIZE: f32 = 0.95;
pub const MINIMAP_VIEWPORT_OFFSET: f32 = 0.05;
pub const MINIMAP_VIEWPORT_SIZE: f32 = 0.9;

pub struct Overlay;

impl Overlay {
    /// Draws the 2D map: wall line-list + robot markers. Body lands at M5.
    pub fn draw_map(&mut self) {
        todo!("ported at M5 — 2D map geometry (source-map §3.2, §7)")
    }

    /// Draws the crosshair: 4 segments +-0.2 -> +-0.05 NDC. Body lands at M5.
    pub fn draw_crosshair(&mut self) {
        todo!("ported at M5 — crosshair (design §7 constants table)")
    }

    /// Draws a 1.0x0.1 health bar above a robot, green (0,1,0,1) on red
    /// (1,0,0,1). Body lands at M5.
    pub fn draw_health_bar(&mut self, _fraction: f32) {
        todo!("ported at M5 — health bar (design §7 constants table)")
    }
}
