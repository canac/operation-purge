//! winit glue: owns the window, GL renderers and audio, feeds OS events to
//! the window-free [`Shell`], and applies the [`AppEffect`]s it returns.
//! The frame pump mirrors `cMainWindow::FrameChange` (`cMainWindow.cpp:923-945`).

pub mod config;
pub mod e2e;
pub mod embedded;
pub mod shell;
pub mod window;

use std::time::{Duration, Instant};

use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, ElementState, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::KeyCode;
use winit::window::WindowId;

use crate::audio::game_audio::GameAudio;
use crate::data::assets::AssetDirs;
use crate::input::keys::Key;
use crate::math::Rgba;
use crate::render::scene::{frame_view, SceneRenderer};
use crate::render::text::{GlyphAtlas, TextRenderer};
use config::Config;
use shell::{AppEffect, Shell};
use window::RenderWindow;

/// Redraw pace while unfocused: the sim is paused, so only input polling
/// and the frozen frame remain.
const UNFOCUSED_FRAME_INTERVAL: Duration = Duration::from_millis(100);

/// `dt = min(now - last, 1.0 s)`, `0` on the first frame
/// (PARITY: `cMainWindow.cpp:933`).
pub fn frame_dt(last: Option<Instant>, now: Instant) -> f32 {
    last.map(|l| now.duration_since(l).as_secs_f32().min(1.0)).unwrap_or(0.0)
}

/// The winit `ApplicationHandler` driving the game.
pub struct AppRunner {
    shell: Shell,
    focused: bool,
    last_frame: Option<Instant>,
    /// When to draw the next frame under a frame cap; `None` = right away.
    next_frame_at: Option<Instant>,
    /// Either Shift key is down (New Game team cycling).
    shift: bool,
    window: Option<RenderWindow>,
    atlas: Option<GlyphAtlas>,
    text: Option<TextRenderer>,
    hud_scale_is_auto: bool,
    scene: Option<SceneRenderer>,
    /// Why `scene` is missing, re-shown on every new game.
    scene_error: Option<String>,
    audio: Option<GameAudio>,
    /// Last `CursorMoved` position, physical px.
    cursor: Option<(f64, f64)>,
    /// Scripted E2E driver (`--e2e`); `None` in normal play.
    e2e: Option<e2e::Driver>,
    /// Fixed per-frame dt (`--fixed-dt`), replacing wall-clock time.
    fixed_dt: Option<f32>,
    /// Window size override (`--window WxH`); `None` = borderless fullscreen.
    windowed: Option<(u32, u32)>,
    /// Where to save the next rendered frame (E2E `shot`).
    pending_capture: Option<std::path::PathBuf>,
}

/// Runtime options that are not user preferences (CLI-only).
#[derive(Default)]
pub struct RunOptions {
    pub e2e: Option<e2e::Driver>,
    pub fixed_dt: Option<f32>,
    pub windowed: Option<(u32, u32)>,
}

impl AppRunner {
    pub fn new(config: Config, opts: RunOptions) -> Self {
        let hud_scale_is_auto = config.hud_scale.is_none();
        let shell = Shell::new(config, Config::path());
        Self {
            shell,
            focused: true,
            last_frame: None,
            next_frame_at: None,
            shift: false,
            window: None,
            atlas: None,
            text: None,
            hud_scale_is_auto,
            scene: None,
            scene_error: None,
            audio: None,
            cursor: None,
            e2e: opts.e2e,
            fixed_dt: opts.fixed_dt,
            windowed: opts.windowed,
            pending_capture: None,
        }
    }

    fn apply(&mut self, effect: AppEffect, event_loop: &ActiveEventLoop) {
        match effect {
            AppEffect::None => {}
            AppEffect::Exit => event_loop.exit(),
            AppEffect::ModalOpened => {
                if let Some(audio) = self.audio.as_mut() {
                    audio.stop_all();
                }
            }
            AppEffect::GameStarted => {
                if let Some(audio) = self.audio.as_mut() {
                    audio.stop_all();
                }
                let Some(game) = self.shell.game.as_ref() else {
                    return;
                };
                let error = match self.scene.as_mut() {
                    Some(scene) => scene
                        .load_building(&game.world.building, &game.world.game_data.building)
                        .err()
                        .map(|e| format!("scene build failed: {e}")),
                    None => self.scene_error.clone(),
                };
                if let Some(e) = error {
                    crate::log_error!("{e}");
                    self.shell.init_error = Some(e);
                }
            }
        }
    }

    fn on_key(&mut self, key: Option<Key>, text: Option<&str>, pressed: bool, event_loop: &ActiveEventLoop) {
        let effect = self.shell.on_key(key, text, pressed, self.shift);
        self.apply(effect, event_loop);
    }

    fn hud_scale(&self, monitor_scale: f64) -> f32 {
        self.shell.config.hud_scale.unwrap_or(monitor_scale as f32)
    }

    fn status_color(&self) -> [f32; 4] {
        let c = self
            .shell
            .game_data
            .as_ref()
            .map(|d| d.colors.status_text_2d)
            .unwrap_or(Rgba::new(1.0, 1.0, 1.0, 1.0));
        [c.r, c.g, c.b, c.a]
    }

    /// One rendered frame: the New Game screen, or the posed scene + HUD +
    /// any confirm modal + the profiler pane.
    fn render(&mut self) {
        crate::profile_scope!("Render");
        let color = self.status_color();
        let shell = &self.shell;
        let new_game_text = shell
            .new_game
            .as_ref()
            .map(|ng| crate::game::newgame::screen_text(ng, shell.game_data.as_ref()));
        let active_game = shell.game.as_ref().filter(|_| new_game_text.is_none());
        let frame = active_game.map(frame_view);
        let hud = active_game.map(|game| (game.status_text(), game.confirm_kind().map(|k| k.prompt())));
        let profiler_lines = (shell.profiler_visible && new_game_text.is_none()).then(profiler_pane_lines);
        let error = shell.init_error.as_deref();

        let Some(window) = self.window.as_mut() else {
            return;
        };
        window.gl.clear();

        let screen = window.physical_size;
        let aspect = screen.0.max(1) as f32 / screen.1.max(1) as f32;
        if let (Some(scene), Some(frame)) = (self.scene.as_mut(), frame.as_ref()) {
            scene.render(&mut window.gl, frame, aspect);
        }

        if let (Some(atlas), Some(text)) = (self.atlas.as_ref(), self.text.as_ref()) {
            let line_h = atlas.baked.line_height.max(1.0);
            let draw_block = |lines: &str, x: f32, y0: f32, c: [f32; 4]| -> f32 {
                let mut y = y0;
                for line in lines.split('\n') {
                    text.draw(atlas, line, x, y, c, screen);
                    y += line_h;
                }
                y
            };

            let mut y = 4.0;
            if let Some(lines) = new_game_text.as_ref() {
                y = draw_block(lines, 4.0, 4.0, color) + line_h;
            } else if let Some((hud, prompt)) = hud.as_ref() {
                y = draw_block(hud, 4.0, 4.0, color);
                if let Some(pl) = profiler_lines.as_ref() {
                    let pane_x = (screen.0 as f32) * (2.0 / 3.0);
                    draw_block(pl, pane_x, 4.0, [0.7, 0.9, 0.7, 1.0]);
                }
                if let Some(p) = prompt {
                    let cx = (screen.0 as f32) * 0.35;
                    let cy = (screen.1 as f32) * 0.4;
                    draw_block(p, cx, cy, [1.0, 1.0, 0.4, 1.0]);
                    draw_block(CONFIRM_HINT, cx, cy + line_h * 5.0, [0.9, 0.9, 0.9, 1.0]);
                }
            }
            if let Some(err) = error {
                draw_block(err, 4.0, y, [1.0, 0.4, 0.4, 1.0]);
            }
        }

        if let Some(path) = self.pending_capture.take() {
            if let Err(e) = window.capture_png(&path) {
                crate::log_error!("e2e: capture {} failed: {e}", path.display());
            }
        }
        window.swap_buffers();
    }

    /// The frame pump: `ExecuteCommands` runs regardless of focus, the sim
    /// only advances while focused.
    fn pump(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        let dt = self.fixed_dt.unwrap_or(frame_dt(self.last_frame, now));
        self.last_frame = Some(now);

        if self.run_e2e_steps(event_loop) {
            return;
        }

        let cursor = self.cursor_fraction();
        let effect = self.shell.frame(dt, cursor, self.focused || self.e2e.is_some());
        self.apply(effect, event_loop);
        if let Some(game) = self.shell.playing_mut() {
            match self.audio.as_mut() {
                Some(audio) => audio.frame(game),
                None => game.world.sound_events.clear(),
            }
        }

        self.sync_cursor_mode();
        self.render();

        self.next_frame_at = self.frame_interval().map(|i| now + i);
        if self.next_frame_at.is_none() {
            if let Some(win) = self.window.as_ref() {
                win.window.request_redraw();
            }
        }
    }

    /// The minimum time between frames: throttled while unfocused, else
    /// `fps_cap`. Scripted runs never wait.
    fn frame_interval(&self) -> Option<Duration> {
        if self.e2e.is_some() {
            return None;
        }
        if !self.focused {
            return Some(UNFOCUSED_FRAME_INTERVAL);
        }
        self.shell
            .config
            .fps_cap
            .filter(|&cap| cap > 0)
            .map(|cap| Duration::from_secs_f64(1.0 / cap as f64))
    }

    /// The cursor as a window fraction (`GetMousePosition`), if known.
    fn cursor_fraction(&self) -> Option<glam::Vec2> {
        let (x, y) = self.cursor?;
        let (w, h) = self.window.as_ref()?.physical_size;
        Some(glam::Vec2::new(x as f32 / w.max(1) as f32, y as f32 / h.max(1) as f32))
    }

    /// PARITY: the cursor shows only on the 2D map / menus (`ShowCursor`,
    /// `cOperationPurge.cpp:298, 1016`); in 3D it is hidden and grabbed so
    /// mouse-look deltas never push it out of the window.
    fn sync_cursor_mode(&mut self) {
        let play_3d = self.shell.in_3d_play();
        let Some(win) = self.window.as_mut() else {
            return;
        };
        if win.cursor_captured == play_3d || !self.focused || self.e2e.is_some() {
            return;
        }
        use winit::window::CursorGrabMode;
        win.window.set_cursor_visible(!play_3d);
        let grab = if play_3d {
            win.window
                .set_cursor_grab(CursorGrabMode::Locked)
                .or_else(|_| win.window.set_cursor_grab(CursorGrabMode::Confined))
        } else {
            win.window.set_cursor_grab(CursorGrabMode::None)
        };
        if let Err(e) = grab {
            crate::log_error!("cursor grab failed: {e}");
        }
        win.cursor_captured = play_3d;
    }

    /// Apply this frame's E2E script steps. Returns `true` if the app quit.
    fn run_e2e_steps(&mut self, event_loop: &ActiveEventLoop) -> bool {
        let Some(mut driver) = self.e2e.take() else {
            return false;
        };
        let mut quit = false;
        for step in driver.next_frame_steps() {
            match step {
                e2e::Step::Frames(_) => {}
                e2e::Step::Down(t) | e2e::Step::Up(t) => {
                    let pressed = matches!(step, e2e::Step::Down(_));
                    match t {
                        e2e::Target::Key(code) => {
                            if matches!(code, KeyCode::ShiftLeft | KeyCode::ShiftRight) {
                                self.shift = pressed;
                            }
                            self.on_key(Key::from_key_code(code), None, pressed, event_loop);
                        }
                        e2e::Target::Mouse(b) => {
                            if let Some(k) = Key::from_mouse_button(b) {
                                self.shell.input.on_key_event(k, pressed);
                            }
                        }
                    }
                }
                e2e::Step::Type(text) => {
                    for ch in text.chars() {
                        self.on_key(None, Some(&ch.to_string()), true, event_loop);
                    }
                }
                e2e::Step::Cursor(fx, fy) => self.set_cursor_fraction(glam::Vec2::new(fx, fy)),
                e2e::Step::CursorWorld(x, y) => {
                    if let Some(f) = self.shell.game.as_ref().map(|g| g.log_to_win(glam::Vec2::new(x, y))) {
                        self.set_cursor_fraction(f);
                    }
                }
                e2e::Step::CursorRobot(n) => {
                    let target = self.shell.game.as_ref().and_then(|g| {
                        g.world
                            .robots
                            .values()
                            .filter(|r| !r.is_dead() && r.team == g.world.active_team)
                            .nth(n)
                            .map(|r| g.log_to_win(r.position))
                    });
                    match target {
                        Some(f) => self.set_cursor_fraction(f),
                        None => crate::log_error!("e2e: no live friendly robot #{n}"),
                    }
                }
                e2e::Step::Mouse(dx, dy) => self.shell.input.on_mouse_motion(dx, dy),
                e2e::Step::Wheel(n) => self.shell.input.on_wheel(n),
                e2e::Step::Shot(name) => {
                    self.pending_capture = Some(driver.out_dir.join(format!("{name}.png")));
                }
                e2e::Step::State(name) => {
                    let path = driver.out_dir.join(format!("{name}.txt"));
                    let dump = e2e::state_dump(&self.shell, self.audio.as_ref().map(|a| a.debug_line()));
                    if let Err(e) = std::fs::write(&path, dump) {
                        crate::log_error!("e2e: cannot write {}: {e}", path.display());
                    }
                }
                e2e::Step::Quit => quit = true,
            }
        }
        self.e2e = Some(driver);
        if quit {
            event_loop.exit();
        }
        quit
    }

    fn set_cursor_fraction(&mut self, f: glam::Vec2) {
        if let Some(win) = self.window.as_ref() {
            let (w, h) = win.physical_size;
            self.cursor = Some((f.x as f64 * w as f64, f.y as f64 * h as f64));
        }
    }
}

impl ApplicationHandler for AppRunner {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let win = match RenderWindow::create(event_loop, self.shell.config.vsync, self.windowed) {
            Ok(win) => win,
            Err(e) => {
                crate::log_error!("fatal: window/GL init failed: {e}");
                event_loop.exit();
                return;
            }
        };
        let ctx = win.context();
        match GlyphAtlas::build(ctx.clone(), self.hud_scale(win.scale_factor)) {
            Ok(atlas) => self.atlas = Some(atlas),
            Err(e) => crate::log_error!("glyph atlas build failed: {e}"),
        }
        match TextRenderer::new(ctx.clone()) {
            Ok(text) => self.text = Some(text),
            Err(e) => crate::log_error!("text renderer build failed: {e}"),
        }
        match SceneRenderer::new(ctx, &AssetDirs::new(&self.shell.config.assets_dir).textures) {
            Ok(scene) => self.scene = Some(scene),
            Err(e) => {
                let msg = format!("renderer init failed: {e}");
                crate::log_error!("{msg}");
                self.shell.init_error = Some(msg.clone());
                self.scene_error = Some(msg);
            }
        }
        let sounds = AssetDirs::new(&self.shell.config.assets_dir).sounds;
        self.audio = self.shell.game_data.as_ref().and_then(|d| GameAudio::open(&sounds, d));
        win.window.request_redraw();
        self.window = Some(win);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        // Scripted runs take input only from the script, not the real desk.
        if self.e2e.is_some()
            && matches!(
                event,
                WindowEvent::KeyboardInput { .. }
                    | WindowEvent::ModifiersChanged(_)
                    | WindowEvent::CursorMoved { .. }
                    | WindowEvent::MouseInput { .. }
                    | WindowEvent::MouseWheel { .. }
            )
        {
            return;
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::ModifiersChanged(mods) => self.shift = mods.state().shift_key(),
            WindowEvent::KeyboardInput { event, .. } => {
                let pressed = event.state == ElementState::Pressed;
                let key = Key::from_physical_key(event.physical_key);
                self.on_key(key, event.text.as_deref(), pressed, event_loop);
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = Some((position.x, position.y));
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if let Some(key) = Key::from_mouse_button(button) {
                    self.shell.input.on_key_event(key, state == ElementState::Pressed);
                }
            }
            WindowEvent::MouseWheel { delta, .. } => match delta {
                MouseScrollDelta::LineDelta(_, y) => self.shell.input.on_wheel(y),
                MouseScrollDelta::PixelDelta(pos) => self.shell.input.on_wheel((pos.y / 120.0) as f32),
            },
            // Scripted runs must not depend on which app the OS focuses.
            WindowEvent::Focused(_) if self.e2e.is_some() => {}
            WindowEvent::Focused(focused) => {
                self.focused = focused;
                if focused {
                    self.next_frame_at = None;
                    if let Some(win) = self.window.as_ref() {
                        win.window.request_redraw();
                    }
                } else {
                    self.shift = false;
                    self.shell.on_focus_lost();
                    // The OS drops the grab on focus loss; re-grab on return.
                    if let Some(win) = self.window.as_mut() {
                        win.cursor_captured = false;
                    }
                }
            }
            WindowEvent::Resized(size) => {
                if let Some(win) = self.window.as_mut() {
                    win.resize(size.width, size.height);
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                if let Some(win) = self.window.as_mut() {
                    win.set_scale_factor(scale_factor);
                }
                if self.hud_scale_is_auto {
                    if let Some(atlas) = self.atlas.as_mut() {
                        if let Err(e) = atlas.rebake(scale_factor as f32) {
                            crate::log_error!("atlas rebake failed: {e}");
                        }
                    }
                }
            }
            WindowEvent::RedrawRequested => self.pump(event_loop),
            _ => {}
        }
    }

    fn device_event(&mut self, _event_loop: &ActiveEventLoop, _device_id: DeviceId, event: DeviceEvent) {
        if self.e2e.is_some() {
            return;
        }
        if let DeviceEvent::MouseMotion { delta } = event {
            let scale = self.shell.config.mouse_scale as f64;
            self.shell.input.on_mouse_motion(delta.0 * scale, delta.1 * scale);
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        match self.next_frame_at {
            Some(at) if Instant::now() < at => event_loop.set_control_flow(ControlFlow::WaitUntil(at)),
            Some(_) => {
                self.next_frame_at = None;
                event_loop.set_control_flow(ControlFlow::Poll);
                if let Some(win) = self.window.as_ref() {
                    win.window.request_redraw();
                }
            }
            None => event_loop.set_control_flow(ControlFlow::Poll),
        }
    }

    /// Dump the profiler tree to `Profile output.txt` on shutdown. No-op if
    /// nothing was recorded.
    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        if let Err(e) = crate::profiler::write_exit_dump() {
            crate::log_error!("could not write Profile output.txt: {e}");
        }
    }
}

/// Run the winit event loop with the game shell.
pub fn run(config: Config, opts: RunOptions) -> Result<(), winit::error::EventLoopError> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = AppRunner::new(config, opts);
    event_loop.run_app(&mut app)
}

/// Profiler pane text: the top-level scopes with their total time + call
/// count, refreshed each frame.
fn profiler_pane_lines() -> String {
    crate::profiler::with_profiler(|p| {
        let mut out = String::from("PROFILER\n");
        for (_, n) in p.children(crate::profiler::ROOT) {
            let ms = n.total_time.as_secs_f64() * 1000.0;
            out.push_str(&format!("{}: {ms:.3}ms x{}\n", n.name, n.total_calls));
        }
        out
    })
}

/// The Y/N(/C) key hint shown under a confirm modal prompt.
const CONFIRM_HINT: &str = "[Y] Yes   [N] No   [C] Cancel";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_frame_dt_is_zero_and_later_frames_clamp_to_a_second() {
        let now = Instant::now();
        assert_eq!(frame_dt(None, now), 0.0);
        assert_eq!(frame_dt(Some(now), now + Duration::from_secs(5)), 1.0);
        let dt = frame_dt(Some(now), now + Duration::from_millis(16));
        assert!((dt - 0.016).abs() < 1e-6);
    }
}
