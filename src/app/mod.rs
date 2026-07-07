//! Application shell: winit `ApplicationHandler`, frame pump, focus gate,
//! redraw (design §2, §5.1 mirrors `cMainWindow::FrameChange`).

pub mod config;
pub mod window;

use std::time::Instant;

use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, ElementState, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::WindowId;

use glam::Vec3;

use crate::data::GameData;
use crate::error::GameError;
use crate::input::keys::Key;
use crate::input::Input;
use crate::math::Rgba;
use crate::maze::building::Building;
use crate::maze::Maze;
use crate::render::building3d::BuildingRenderer;
use crate::render::camera::{FreeFlyCamera, WindingGate};
use crate::render::center3d::{CenterRenderData, CenterRenderer};
use crate::render::robot3d::{RobotRenderData, RobotRenderer};
use crate::render::shaders::{compile_all, Programs};
use crate::render::text::{GlyphAtlas, TextRenderer};
use crate::render::textures::{DecodedImage, ProcessMode, Texture};
use config::Config;
use window::RenderWindow;

/// Held free-fly movement keys for the M4 debug camera: forward, back, left,
/// right, up, down (W/S/A/D/Space/LeftControl).
#[derive(Default, Copy, Clone)]
struct MoveKeys {
    fwd: bool,
    back: bool,
    left: bool,
    right: bool,
    up: bool,
    down: bool,
}

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

/// Compose a command's binding into a human-readable "Ctrl+Shift+F" style
/// string using the verbatim key display names (design §5.2). Pure — unit
/// tested without a window.
pub fn describe_binding(binding: &crate::input::commands::CommandBinding) -> String {
    let mut s = String::new();
    if binding.control {
        s.push_str("Ctrl+");
    }
    if binding.alt {
        s.push_str("Alt+");
    }
    if binding.shift {
        s.push_str("Shift+");
    }
    s.push_str(binding.key.display_name());
    s
}

/// The winit `ApplicationHandler` that drives the M3 shell: creates the
/// window/GL/atlas on `resumed`, accumulates input, and each redraw echoes
/// every bound command (with modifiers + mouse buttons) as on-screen text —
/// highlighting the ones currently active — plus live mouse/wheel deltas and
/// the focus state. This is the runtime path the visual gate exercises; it
/// is never entered by the headless test suite.
pub struct AppRunner {
    config: Config,
    game_data: Option<GameData>,
    input: Input,
    focused: bool,
    last_frame: Option<Instant>,
    window: Option<RenderWindow>,
    atlas: Option<GlyphAtlas>,
    text: Option<TextRenderer>,
    hud_scale_is_auto: bool,
    // M4 winding-gate scene.
    programs: Option<Programs>,
    building: Option<BuildingRenderer>,
    // M5 static-scene demo entities (dome/treads/center visual gate).
    robot: Option<RobotRenderer>,
    center: Option<CenterRenderer>,
    demo_robot: Option<RobotRenderData>,
    demo_center: Option<CenterRenderData>,
    camera: FreeFlyCamera,
    gate: WindingGate,
    move_keys: MoveKeys,
    scene_error: Option<String>,
}

impl AppRunner {
    pub fn new(config: Config) -> Self {
        // Best-effort data load; the shell still runs (echoing "no data") if
        // GameData.toml is missing/broken (GUARD: no null-deref, design §4.1).
        let path = std::path::Path::new(&config.assets_dir).join("GameData.toml");
        let game_data = match crate::data::gamedata::load(&path) {
            Ok(data) => Some(data),
            Err(e) => {
                eprintln!("warning: could not load {}: {e}", path.display());
                None
            }
        };
        let hud_scale_is_auto = config.hud_scale.is_none();
        Self {
            config,
            game_data,
            input: Input::new(),
            focused: true,
            last_frame: None,
            window: None,
            atlas: None,
            text: None,
            hud_scale_is_auto,
            programs: None,
            building: None,
            robot: None,
            center: None,
            demo_robot: None,
            demo_center: None,
            camera: FreeFlyCamera::new(Vec3::ZERO, 200.0),
            gate: WindingGate::default(),
            move_keys: MoveKeys::default(),
            scene_error: None,
        }
    }

    /// Builds the M4 debug scene: compile the 4 programs, load `Small1.maz`,
    /// extrude the building, upload its textures (white fallback on failure),
    /// and place the free-fly camera inside room (0,0). Runtime-only; records
    /// a message on failure rather than crashing (GUARD, design §4.1).
    fn init_scene(&mut self, win: &RenderWindow) {
        let ctx = win.context();
        let programs = match compile_all(ctx.clone()) {
            Ok(p) => p,
            Err(e) => {
                self.scene_error = Some(format!("shader compile failed: {e}"));
                return;
            }
        };
        self.programs = Some(programs);

        let Some(data) = self.game_data.as_ref() else {
            self.scene_error = Some("no GameData.toml — building not built".into());
            return;
        };
        let assets = std::path::Path::new(&self.config.assets_dir);
        let maze_path = assets.join("MazeFiles/Small1.maz");
        let maze = match Maze::load(&maze_path) {
            Ok(m) => m,
            Err(e) => {
                self.scene_error = Some(format!("maze load failed: {e}"));
                return;
            }
        };
        let b = &data.building;
        let building = Building {
            maze,
            wall_width: b.wall_width,
            room_size: b.room_size,
            ceiling_height: b.ceiling_height,
        };

        // Textures: try the [building] BMPs, fall back to solid white so the
        // geometry is always visible for the winding gate.
        let tex_dir = assets.join("Textures");
        let load_tex = |name: &str| -> glow::Texture {
            let path = tex_dir.join(name);
            std::fs::read(&path)
                .ok()
                .and_then(|bytes| {
                    crate::render::textures::decode_bmp(&bytes, ProcessMode::NoAlpha).ok()
                })
                .and_then(|img| Texture::upload(&ctx, &img).ok())
                .unwrap_or_else(|| white_texture(&ctx))
                .handle
        };
        let wall_tex = load_tex(&b.wall_texture);
        let floor_tex = load_tex(&b.floor_texture);
        let ceiling_tex = load_tex(&b.ceiling_texture);

        // Place the camera inside room (0,0), looking down +Y (yaw = pi/2).
        let mut camera = FreeFlyCamera::new(
            Vec3::new(b.room_size * 0.5, b.room_size * 0.5, b.ceiling_height * 0.5),
            building.size().length() * 2.0 + 10.0,
        );
        camera.yaw = crate::render::d3dx::D3DX_PI * 0.5;
        self.camera = camera;

        match BuildingRenderer::new(
            ctx,
            &building,
            b.wall_texture_size,
            b.floor_texture_size,
            b.ceiling_texture_size,
            wall_tex,
            floor_tex,
            ceiling_tex,
        ) {
            Ok(r) => self.building = Some(r),
            Err(e) => self.scene_error = Some(format!("building mesh failed: {e}")),
        }

        // M5 static-scene demo: one robot + one center marker in room (0,0),
        // so the visual gate can eyeball dome dimming (raw normals), treads,
        // and the floor/wall center marker under the point-light-at-eye
        // lighting. Positions are fixed (live game wiring is M8).
        let ctx = win.context();
        let dims = crate::data::builtin::OBJECTS;
        let body_color = data
            .game_definitions
            .first()
            .and_then(|g| g.teams.first())
            .map(|t| t.color)
            .unwrap_or(Rgba::new(0.3, 0.5, 1.0, 1.0));
        let cx = b.room_size * 0.5;
        let cy = b.room_size * 0.5;
        match RobotRenderer::new(ctx.clone(), &dims) {
            Ok(r) => {
                self.robot = Some(r);
                self.demo_robot = Some(RobotRenderData {
                    position: Vec3::new(cx, cy, 0.0),
                    direction: 0.0,
                    barrel_elevation: 0.3,
                    body_color,
                    barrel_color: data.colors.barrel_3d,
                    tread_color: data.colors.tread_3d,
                });
            }
            Err(e) => self.scene_error = Some(format!("robot mesh failed: {e}")),
        }
        match CenterRenderer::new(ctx) {
            Ok(c) => {
                self.center = Some(c);
                self.demo_center = Some(CenterRenderData {
                    position_x: cx + b.room_size,
                    position_y: cy,
                    orientation: 0.0,
                    size: 2.0,
                    color: Rgba::new(0.2, 0.8, 0.3, 1.0),
                });
            }
            Err(e) => self.scene_error = Some(format!("center mesh failed: {e}")),
        }
    }

    /// Advances the free-fly camera from held keys + accumulated mouse look.
    fn update_camera(&mut self, dt: f32) {
        let speed = 20.0 * dt;
        let m = &self.move_keys;
        let f = (m.fwd as i32 - m.back as i32) as f32 * speed;
        let r = (m.right as i32 - m.left as i32) as f32 * speed;
        let u = (m.up as i32 - m.down as i32) as f32 * speed;
        self.camera.translate(f, r, u);
        // Mouse look: raw deltas → radians (sign matches yaw/pitch axes).
        let (dx, dy) = self.input.mouse_delta();
        self.camera
            .rotate(dx as f32 * 0.0025, -dy as f32 * 0.0025);
    }

    fn hud_scale(&self, monitor_scale: f64) -> f32 {
        self.config.hud_scale.unwrap_or(monitor_scale as f32)
    }

    fn status_color(&self) -> [f32; 4] {
        let c = self
            .game_data
            .as_ref()
            .map(|d| d.colors.status_text_2d)
            .unwrap_or(Rgba {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 1.0,
            });
        [c.r, c.g, c.b, c.a]
    }

    fn render(&mut self) {
        let color = self.status_color();
        let Some(window) = self.window.as_mut() else {
            return;
        };
        window.gl.clear();

        let screen = window.physical_size;
        let aspect = screen.0.max(1) as f32 / screen.1.max(1) as f32;

        // 3D building (M4 winding gate).
        if let (Some(building), Some(programs)) =
            (self.building.as_ref(), self.programs.as_ref())
        {
            building.render(&mut window.gl, programs, &self.camera, aspect, &self.gate);
        }

        // M5 static demo entities (lit robot + center marker). Original uses
        // D3DCULL_CCW for both; we reuse the debug gate's decision so the
        // winding keys still let the user inspect them.
        if let Some(programs) = self.programs.as_ref() {
            let decision = self.gate.decision();
            if let (Some(robot), Some(data)) = (self.robot.as_ref(), self.demo_robot.as_ref()) {
                robot.render(&mut window.gl, programs, &self.camera, aspect, decision, data);
            }
            if let (Some(center), Some(data)) = (self.center.as_ref(), self.demo_center.as_ref()) {
                center.render(&mut window.gl, programs, &self.camera, aspect, decision, data);
            }
        }

        // Debug HUD overlay.
        if let (Some(atlas), Some(text)) = (self.atlas.as_ref(), self.text.as_ref()) {
            let line_h = atlas.baked.line_height.max(1.0);
            let mut y = 4.0f32;
            let cull = if self.gate.cull_enabled {
                if self.gate.front_ccw {
                    "ON front=CCW (cull BACK)"
                } else {
                    "ON front=CW (cull BACK)"
                }
            } else {
                "OFF"
            };
            let lines = [
                "Operation Purge — M4 winding/culling gate (free-fly debug camera)".to_string(),
                format!(
                    "cull [C]: {}   flip-front-face [F]   category [V]: {:?}",
                    cull, self.gate.category
                ),
                "move: W/A/S/D  up/down: Space/LeftCtrl   look: mouse   quit: Esc".to_string(),
                format!(
                    "cam pos ({:.1},{:.1},{:.1}) yaw {:.2} pitch {:.2}",
                    self.camera.position.x,
                    self.camera.position.y,
                    self.camera.position.z,
                    self.camera.yaw,
                    self.camera.pitch
                ),
            ];
            for line in lines {
                text.draw(atlas, &line, 4.0, y, color, screen);
                y += line_h;
            }
            if let Some(err) = self.scene_error.as_ref() {
                text.draw(atlas, err, 4.0, y, [1.0, 0.4, 0.4, 1.0], screen);
            }
        }

        window.swap_buffers();
    }
}

/// A 1x1 opaque-white texture used as a fallback when a building BMP is
/// missing, so the winding-gate geometry is always visible. Runtime-only.
fn white_texture(gl: &std::rc::Rc<glow::Context>) -> Texture {
    let image = DecodedImage {
        width: 1,
        height: 1,
        rgba: vec![255, 255, 255, 255],
    };
    // Upload cannot realistically fail for a 1x1 image; on the off chance it
    // does, fall back to a zero handle is not possible, so we unwrap the
    // (runtime-only) result.
    Texture::upload(gl, &image).expect("1x1 white texture upload")
}

impl ApplicationHandler for AppRunner {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        match RenderWindow::create(event_loop, self.config.vsync) {
            Ok(win) => {
                let scale = self.hud_scale(win.scale_factor);
                let ctx = win.context();
                match (GlyphAtlas::build(ctx.clone(), scale), TextRenderer::new(ctx)) {
                    (Ok(atlas), Ok(text)) => {
                        self.atlas = Some(atlas);
                        self.text = Some(text);
                    }
                    (a, t) => {
                        if let Err(e) = a {
                            eprintln!("glyph atlas build failed: {e}");
                        }
                        if let Err(e) = t {
                            eprintln!("text renderer build failed: {e}");
                        }
                    }
                }
                win.window.request_redraw();
                self.init_scene(&win);
                self.window = Some(win);
            }
            Err(e) => {
                eprintln!("fatal: window/GL init failed: {e}");
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::KeyboardInput { event, .. } => {
                let pressed = event.state == ElementState::Pressed;
                if let PhysicalKey::Code(code) = event.physical_key {
                    match code {
                        // Escape quits (M0/M3/M4 accept: Esc quits).
                        KeyCode::Escape if pressed => {
                            event_loop.exit();
                            return;
                        }
                        // M4 winding-gate debug keys (design §3.3).
                        KeyCode::KeyC if pressed => self.gate.toggle_cull(),
                        KeyCode::KeyF if pressed => self.gate.flip_front_face(),
                        KeyCode::KeyV if pressed => self.gate.cycle_category(),
                        // Free-fly movement (held).
                        KeyCode::KeyW => self.move_keys.fwd = pressed,
                        KeyCode::KeyS => self.move_keys.back = pressed,
                        KeyCode::KeyA => self.move_keys.left = pressed,
                        KeyCode::KeyD => self.move_keys.right = pressed,
                        KeyCode::Space => self.move_keys.up = pressed,
                        KeyCode::ControlLeft => self.move_keys.down = pressed,
                        _ => {}
                    }
                }
                if let Some(key) = Key::from_physical_key(event.physical_key) {
                    self.input.on_key_event(key, pressed);
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if let Some(key) = Key::from_mouse_button(button) {
                    self.input.on_key_event(key, state == ElementState::Pressed);
                }
            }
            WindowEvent::MouseWheel { delta, .. } => match delta {
                MouseScrollDelta::LineDelta(_, y) => self.input.on_wheel(y),
                MouseScrollDelta::PixelDelta(pos) => {
                    // Normalize trackpad pixels to ~detents (design §5.2).
                    self.input.on_wheel((pos.y / 120.0) as f32)
                }
            },
            WindowEvent::Focused(focused) => {
                self.focused = focused;
                if !focused {
                    // DISCL_FOREGROUND parity: focus loss clears input (§5.2).
                    self.input.clear_on_focus_loss();
                    self.move_keys = MoveKeys::default();
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
                // Wayland re-bake at the new scale if HUD scale is auto (§3.1).
                if self.hud_scale_is_auto {
                    if let Some(atlas) = self.atlas.as_mut() {
                        if let Err(e) = atlas.rebake(scale_factor as f32) {
                            eprintln!("atlas rebake failed: {e}");
                        }
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                // Frame boundary: dt clamp, snapshot accumulated input, then
                // advance the debug camera and draw.
                let now = Instant::now();
                let dt = self
                    .last_frame
                    .map(|last| now.duration_since(last).as_secs_f32().min(1.0))
                    .unwrap_or(0.0);
                self.last_frame = Some(now);
                self.input.begin_frame();
                if self.focused {
                    self.update_camera(dt);
                }
                self.render();
                if let Some(win) = self.window.as_ref() {
                    win.window.request_redraw();
                }
            }
            _ => {}
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        if let DeviceEvent::MouseMotion { delta } = event {
            let scale = self.config.mouse_scale as f64;
            self.input.on_mouse_motion(delta.0 * scale, delta.1 * scale);
        }
    }
}

/// Run the winit event loop with the M3 shell (real runtime only).
pub fn run(config: Config) -> Result<(), GameError> {
    let event_loop = EventLoop::new()
        .map_err(|e| GameError::Other(format!("event loop creation failed: {e}")))?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = AppRunner::new(config);
    event_loop
        .run_app(&mut app)
        .map_err(|e| GameError::Other(format!("event loop error: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_frame_dt_is_zero_not_a_giant_spike() {
        let mut app = App::new(config::Config::default());
        assert_eq!(app.next_dt(), 0.0);
    }

    #[test]
    fn describe_binding_prefixes_modifiers_in_ctrl_alt_shift_order() {
        use crate::input::commands::CommandBinding;
        let b = CommandBinding {
            key: Key::KeyF,
            control: true,
            alt: false,
            shift: true,
        };
        assert_eq!(describe_binding(&b), "Ctrl+Shift+F");
        let plain = CommandBinding {
            key: Key::Space,
            control: false,
            alt: false,
            shift: false,
        };
        assert_eq!(describe_binding(&plain), "Space");
    }
}
