//! Application shell: winit `ApplicationHandler`, frame pump, focus gate,
//! redraw (design §2, §5.1 mirrors `cMainWindow::FrameChange`).
//!
//! M8 replaces the M4/M5 debug view with the **real game loop** (design §7
//! M8): each redraw clamps `dt`, snapshots input, runs
//! [`Game::execute_commands`] **always**, steps [`Game::advance`] only while
//! focused, and renders the posed scene + HUD. The bulk of the logic lives in
//! [`crate::game::Game`] (unit-tested headless); this file is the runtime-only
//! glue that owns the window, GL renderers, and audio.

pub mod config;
pub mod embedded;
pub mod e2e;
pub mod window;

use std::time::Instant;

use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, ElementState, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::WindowId;

use glam::Vec3;

use crate::data::GameData;
use crate::game::newgame::{NewGame, NewGameOutcome, NewGameParams};
use crate::game::screens::{ConfirmAction, ConfirmChoice};
use crate::game::Game;
use crate::ids::{GameDefId, TeamId};
use crate::input::commands::Command;
use crate::input::keys::Key;
use crate::input::Input;
use crate::rng::GameRng;
use crate::math::Rgba;
use crate::maze::building::Building;
use crate::maze::Maze;
use crate::render::building3d::BuildingRenderer;
use crate::render::camera::WindingGate;
use crate::render::center3d::{CenterRenderData, CenterRenderer};
use crate::render::overlay::{
    MapCenterData, MapColors, MapMode, MapRobotData, MapView, Overlay2DRenderer,
};
use crate::render::robot3d::{RobotRenderData, RobotRenderer};
use crate::render::shaders::{compile_all, Programs};
use crate::render::sprites::{Axis, SpriteBatch, SpriteData, SpriteRenderer};
use crate::render::text::{GlyphAtlas, TextRenderer};
use crate::render::textures::{DecodedImage, ProcessMode, Texture};
use config::Config;
use window::RenderWindow;

/// Top-level app state used by the headless timing unit tests. The live
/// runtime path is [`AppRunner`]; this stub keeps the `dt`-clamp contract
/// (design §5.1) testable without a window.
pub struct App {
    pub config: config::Config,
    pub focused: bool,
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

/// The winit `ApplicationHandler` that drives the M8 playable loop.
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
    // Scene renderers.
    programs: Option<Programs>,
    building: Option<BuildingRenderer>,
    robot: Option<RobotRenderer>,
    center: Option<CenterRenderer>,
    overlay: Option<Overlay2DRenderer>,
    sprites: Option<SpriteRenderer>,
    sprite_textures: Option<SpriteTextures>,
    gate: WindingGate,
    // The live game (world + screens + camera + speed + timers).
    game: Option<Game>,
    audio: Option<GameAudio>,
    init_error: Option<String>,

    /// The active New Game screen (design §5.5). `Some` at launch and when
    /// re-invoked mid-game; while it is `Some` the app renders the New Game
    /// overlay and freezes any running game. `None` once a game is playing.
    new_game: Option<NewGame>,
    /// Shared RNG (seeded from config/time) used for the New Game random maze
    /// pick (`rand () % vszMazeFiles.size ()`, design §4.4).
    rng: GameRng,
    /// `[` toggles the profiler pane (P toggles recording, thread-local).
    profiler_visible: bool,

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
        // Best-effort data load; the shell still runs (echoing "no data") if
        // GameData.toml is missing/broken (GUARD: no null-deref).
        let path = crate::data::assets::AssetDirs::new(&config.assets_dir).game_data_toml();
        let game_data = match crate::data::gamedata::load(&path) {
            Ok(data) => Some(data),
            Err(e) => {
                eprintln!("warning: could not load {}: {e}", path.display());
                None
            }
        };
        let hud_scale_is_auto = config.hud_scale.is_none();
        let seed = config.seed.unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0x0522_1101)
        });
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
            overlay: None,
            sprites: None,
            sprite_textures: None,
            gate: WindingGate::default(),
            game: None,
            audio: None,
            init_error: None,
            new_game: None,
            rng: GameRng::from_seed(seed),
            profiler_visible: false,
            cursor: None,
            e2e: opts.e2e,
            fixed_dt: opts.fixed_dt,
            windowed: opts.windowed,
            pending_capture: None,
        }
    }

    /// Seed the RNG from config, or from wall-clock time when unset
    /// (design §1.5.11).
    fn resolve_seed(&self) -> u64 {
        self.config.seed.unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0x0522_1101)
        })
    }

    /// Compile the four shader programs (once, at window creation). Runtime
    /// only; records a message on failure rather than crashing.
    fn init_gl(&mut self, win: &RenderWindow) {
        let ctx = win.context();
        match compile_all(ctx) {
            Ok(p) => self.programs = Some(p),
            Err(e) => self.init_error = Some(format!("shader compile failed: {e}")),
        }
    }

    /// Open the New Game screen (design §5.5). `invoked_mid_game` distinguishes
    /// the startup entry (Cancel quits) from a mid-game re-entry (Cancel
    /// resumes). Seeds the fields from the persisted config and runs the
    /// matcher once for the live count (`WM_INITDIALOG`, `cMainWindow.cpp:647`).
    fn open_new_game_screen(&mut self, invoked_mid_game: bool) {
        let mut ng = NewGame::new(
            self.config.maze_file.clone(),
            self.config.game_index,
            self.config.team_index,
            invoked_mid_game,
        );
        ng.refresh_matches(std::path::Path::new(&self.config.assets_dir));
        self.new_game = Some(ng);
    }

    /// Build the world + game + all scene renderers for the accepted New Game
    /// params (`SetGameParams` + `StartGame`, `cOperationPurge.cpp:104-306`).
    /// The maze/game/team come from the New Game screen, not a fixed file.
    fn start_new_game(&mut self, params: &NewGameParams) {
        let Some(win) = self.window.as_ref() else {
            return;
        };
        let ctx = win.context();
        let Some(data) = self.game_data.clone() else {
            self.init_error = Some("no GameData.toml — game not started".into());
            return;
        };

        let assets = std::path::Path::new(&self.config.assets_dir).to_path_buf();
        // `Maze` is not `Clone`, so load it twice — once for the world, once
        // for the renderer's `Building`.
        let (maze, maze_render) =
            match (Maze::load(&params.maze_path), Maze::load(&params.maze_path)) {
                (Ok(m), Ok(m2)) => (m, m2),
                (Err(e), _) | (_, Err(e)) => {
                    self.init_error = Some(format!("maze load failed: {e}"));
                    return;
                }
            };

        let b = data.building.clone();
        let building = Building {
            maze: maze_render,
            wall_width: b.wall_width,
            room_size: b.room_size,
            ceiling_height: b.ceiling_height,
        };

        // Textures for the building (white fallback so geometry is visible).
        // Filenames are compiled in (`data::assets::names`), not read from data.
        use crate::data::assets::names;
        let tex_dir = assets.join("Textures");
        let load_tex_mode = |name: &str, mode: ProcessMode| -> glow::Texture {
            let path = tex_dir.join(name);
            std::fs::read(&path)
                .ok()
                .and_then(|bytes| crate::render::textures::decode_image(&bytes, mode).ok())
                .and_then(|img| Texture::upload(&ctx, &img).ok())
                .unwrap_or_else(|| white_texture(&ctx))
                .handle
        };
        let load_tex = |name: &str| load_tex_mode(name, ProcessMode::NoAlpha);
        let wall_tex = load_tex(names::WALL_TEXTURE);
        let floor_tex = load_tex(names::FLOOR_TEXTURE);
        let ceiling_tex = load_tex(names::CEILING_TEXTURE);
        self.sprite_textures = Some(SpriteTextures {
            projectile: load_tex_mode(names::PROJECTILE_TEXTURE, ProcessMode::UseAlpha),
            hole: load_tex_mode(names::HOLE_TEXTURE, ProcessMode::UseAlpha),
            smoke: load_tex_mode(names::SMOKE_TEXTURE, ProcessMode::UseAlpha),
            white: white_texture(&ctx).handle,
        });

        // Build the world + game.
        let seed = self.resolve_seed();
        let world = crate::sim::world::World::new(
            data.clone(),
            maze,
            GameDefId(params.game_idx),
            TeamId(params.team_idx),
            seed,
        );
        let mut game = Game::new(world);
        // PARITY: empty New Game password enables cheats (design §1.3).
        game.cheats_enabled = params.cheats_enabled;
        self.game = Some(game);

        // Scene renderers.
        match BuildingRenderer::new(
            ctx.clone(),
            &building,
            b.wall_texture_size,
            b.floor_texture_size,
            b.ceiling_texture_size,
            wall_tex,
            floor_tex,
            ceiling_tex,
        ) {
            Ok(r) => self.building = Some(r),
            Err(e) => self.init_error = Some(format!("building mesh failed: {e}")),
        }
        match RobotRenderer::new(ctx.clone(), &crate::data::builtin::OBJECTS) {
            Ok(r) => self.robot = Some(r),
            Err(e) => self.init_error = Some(format!("robot mesh failed: {e}")),
        }
        match Overlay2DRenderer::new(ctx.clone(), &building, &crate::data::builtin::OBJECTS) {
            Ok(o) => self.overlay = Some(o),
            Err(e) => self.init_error = Some(format!("2D map mesh failed: {e}")),
        }
        match CenterRenderer::new(ctx.clone()) {
            Ok(c) => self.center = Some(c),
            Err(e) => self.init_error = Some(format!("center mesh failed: {e}")),
        }
        match SpriteRenderer::new(ctx) {
            Ok(r) => self.sprites = Some(r),
            Err(e) => self.init_error = Some(format!("sprite renderer failed: {e}")),
        }

        // Audio (device open + preload). Best-effort: a headless/no-device
        // machine simply runs muted.
        self.audio = GameAudio::new(&assets, &data);

        // Persist the accepted selection (`cMainWindow.cpp:767-776`).
        // Re-read so CLI-only overrides (`--seed`, `--assets`) never persist.
        self.config.apply_new_game(params);
        let mut saved = Config::load();
        saved.apply_new_game(params);
        if let Err(e) = saved.save() {
            eprintln!("warning: could not write config.toml: {e}");
        }

        // Leave the New Game screen; the game begins on the 2D map at speed 0.
        self.new_game = None;
        self.init_error = None;
    }

    /// Dispatch a New Game OK/Cancel outcome. Returns `true` if the app should
    /// exit (Cancel at startup).
    fn handle_new_game_outcome(&mut self, outcome: NewGameOutcome) -> bool {
        match outcome {
            NewGameOutcome::Accept(params) => {
                self.start_new_game(&params);
                false
            }
            NewGameOutcome::Invalid => {
                // The original popped an "invalid maze file" MessageBox and
                // stayed on the dialog (`cMainWindow.cpp:763`); here the screen
                // just stays open.
                self.init_error = Some("Pattern matches no maze files.".into());
                false
            }
            NewGameOutcome::CancelExit => true,
            NewGameOutcome::CancelResume => {
                // Resume the frozen game.
                self.new_game = None;
                false
            }
        }
    }

    /// Route a key press to the New Game screen (design §5.5): Tab cycles the
    /// focused field, Left/Right cycle the Game/Team combos, Enter = OK,
    /// Esc = Cancel, Backspace edits, and printable `KeyEvent.text` feeds the
    /// focused text field (re-running the matcher on a pattern edit).
    fn handle_new_game_key(
        &mut self,
        key: PhysicalKey,
        text: Option<&str>,
        event_loop: &ActiveEventLoop,
    ) {
        let assets = std::path::Path::new(&self.config.assets_dir).to_path_buf();
        let data = self.game_data.clone();

        // Resolve OK / Cancel first (they consume the New Game screen).
        let outcome = {
            let Some(ng) = self.new_game.as_mut() else {
                return;
            };
            match key {
                PhysicalKey::Code(KeyCode::Enter) | PhysicalKey::Code(KeyCode::NumpadEnter) => {
                    Some(ng.ok(&mut self.rng))
                }
                PhysicalKey::Code(KeyCode::Escape) => Some(ng.cancel()),
                PhysicalKey::Code(KeyCode::Tab) => {
                    ng.cycle_focus();
                    None
                }
                PhysicalKey::Code(KeyCode::ArrowLeft)
                | PhysicalKey::Code(KeyCode::ArrowRight) => {
                    let delta = if matches!(key, PhysicalKey::Code(KeyCode::ArrowRight)) {
                        1
                    } else {
                        -1
                    };
                    if let Some(d) = data.as_ref() {
                        // Left/Right cycle Game unless Shift is held (Team).
                        if self.input.shift_down() {
                            ng.cycle_team(delta, d);
                        } else {
                            ng.cycle_game(delta, d);
                        }
                    }
                    None
                }
                PhysicalKey::Code(KeyCode::Backspace) => {
                    if ng.backspace() {
                        ng.refresh_matches(&assets);
                    }
                    None
                }
                _ => {
                    // Printable text into the focused field.
                    if let Some(txt) = text {
                        let mut changed = false;
                        for c in txt.chars() {
                            changed |= ng.type_char(c);
                        }
                        if changed {
                            ng.refresh_matches(&assets);
                        }
                    }
                    None
                }
            }
        };

        if let Some(outcome) = outcome {
            if self.handle_new_game_outcome(outcome) {
                // Cancel at startup quits, dumping the profiler first.
                let _ = crate::profiler::write_exit_dump();
                event_loop.exit();
            }
        }
    }

    /// Route a key press to an open Confirm modal: Y / N / C (design §5.5).
    /// The resolved action either mutates the game (Restart/Resume/KeepWatch,
    /// handled inside `resolve_confirm`) or the app (New Game / Exit).
    fn handle_confirm_key(&mut self, key: PhysicalKey, event_loop: &ActiveEventLoop) {
        let choice = match key {
            PhysicalKey::Code(KeyCode::KeyY) => ConfirmChoice::Yes,
            PhysicalKey::Code(KeyCode::KeyN) => ConfirmChoice::No,
            PhysicalKey::Code(KeyCode::KeyC) => ConfirmChoice::Cancel,
            _ => return,
        };
        // GameOver-style modals are the only ones offering Cancel; ignore C
        // on the others (`MB_YESNO`).
        let offers_cancel = self
            .game
            .as_ref()
            .and_then(|g| g.confirm_kind())
            .is_some_and(|k| k.offers_cancel());
        if choice == ConfirmChoice::Cancel && !offers_cancel {
            return;
        }

        let Some(game) = self.game.as_mut() else {
            return;
        };
        match game.resolve_confirm(choice) {
            ConfirmAction::NewGame => self.open_new_game_screen(true),
            ConfirmAction::Exit => {
                let _ = crate::profiler::write_exit_dump();
                event_loop.exit();
            }
            // Restart / Resume / KeepWatching applied inside resolve_confirm.
            _ => {}
        }
    }

    /// Route one key edge: New Game screen, then an open modal, then the
    /// gameplay input snapshot. Shared by OS events and the E2E driver.
    fn on_key(
        &mut self,
        key: PhysicalKey,
        text: Option<&str>,
        pressed: bool,
        event_loop: &ActiveEventLoop,
    ) {
        // Releases always reach the game input: a key held when a screen or
        // modal opened would otherwise read as down forever after it closes.
        if !pressed {
            if let Some(k) = Key::from_physical_key(key) {
                self.input.on_key_event(k, false);
            }
            return;
        }

        if self.new_game.is_some() {
            self.handle_new_game_key(key, text, event_loop);
            return;
        }

        // Confirm modal: Y/N/C keys (non-blocking, design §1.3).
        if self.game.as_ref().is_some_and(|g| g.in_confirm()) {
            self.handle_confirm_key(key, event_loop);
            return;
        }

        // Esc is NOT intercepted — it flows through as `commandEndGame`
        // (design §5.5); the hard fallback exits only with no game running.
        if self.game.is_none() && key == PhysicalKey::Code(KeyCode::Escape) {
            event_loop.exit();
            return;
        }
        if let Some(k) = Key::from_physical_key(key) {
            self.input.on_key_event(k, true);
        }
    }

    fn hud_scale(&self, monitor_scale: f64) -> f32 {
        self.config.hud_scale.unwrap_or(monitor_scale as f32)
    }

    fn status_color(&self) -> [f32; 4] {
        let c = self
            .game_data
            .as_ref()
            .map(|d| d.colors.status_text_2d)
            .unwrap_or(Rgba::new(1.0, 1.0, 1.0, 1.0));
        [c.r, c.g, c.b, c.a]
    }

    /// One rendered frame. Dispatches on the top-level screen (design §5.5):
    /// the New Game screen (overlay text) at launch / mid-game re-entry, or
    /// the posed 3D scene + HUD + (optional) confirm modal + profiler pane
    /// while a game is running.
    fn render(&mut self) {
        crate::profile_scope!("Render");
        let color = self.status_color();

        // New Game screen text (built while the game is frozen / absent).
        let new_game_lines = self.new_game.as_ref().map(|ng| self.new_game_lines(ng));

        // Scene + HUD snapshot (immutable game borrow before the mutable GL
        // borrow). Skipped while the New Game screen is up.
        let scene = if new_game_lines.is_none() {
            self.game.as_ref().map(|game| {
                (
                    game.camera(),
                    self.collect_robot_render_data(game),
                    self.collect_center_render_data(game),
                    game.status_text(),
                    game.confirm_kind().map(|k| k.prompt()),
                    self.collect_sprite_batches(game),
                )
            })
        } else {
            None
        };
        let profiler_lines = if self.profiler_visible && new_game_lines.is_none() {
            Some(profiler_pane_lines())
        } else {
            None
        };

        // 2D-map snapshot (`Render2D`): full-screen in Map2D, the minimap in
        // 3D while `m_bShowMap`, plus the crosshair in 3D.
        let active_game = self.game.as_ref().filter(|_| new_game_lines.is_none());
        let in_2d = active_game.is_some_and(Game::shows_2d);
        let show_minimap = active_game.is_some_and(|g| g.show_map);
        let map = active_game.map(|game| {
            (
                MapView {
                    offset: game.map_visible_offset(),
                    size: game.map_size,
                },
                self.collect_map_center_data(game),
                self.collect_map_render_data(game),
            )
        });
        let map_colors = self.game_data.as_ref().map(|d| MapColors {
            background: d.colors.background_2d,
            wall: d.colors.wall_2d,
            barrel: d.colors.barrel_2d,
        });
        let crosshair_color = self.game_data.as_ref().map(|d| d.colors.crosshair_2d);

        let Some(window) = self.window.as_mut() else {
            return;
        };
        window.gl.clear();

        let screen = window.physical_size;
        let aspect = screen.0.max(1) as f32 / screen.1.max(1) as f32;
        let decision = self.gate.decision();

        // PARITY: `Render3D` order (`cOperationPurge.cpp:2848-3030`): building;
        // centers + holes without depth writes; robots; health bars; then
        // depth-sorted projectiles + particles.
        if !in_2d {
            crate::profile_scope!("Render3D");
            if let (Some((camera, robot_data, center_data, _hud, _prompt, batches)), Some(programs)) =
                (scene.as_ref(), self.programs.as_ref())
            {
                if let Some(building) = self.building.as_ref() {
                    building.render(&mut window.gl, programs, camera, aspect, &self.gate);
                }
                window.gl.set_depth_write(false);
                if let Some(center) = self.center.as_ref() {
                    for data in center_data {
                        center.render(&mut window.gl, programs, camera, aspect, decision, data);
                    }
                }
                let bind = |gl: &glow::Context, t: glow::Texture| unsafe {
                    use glow::HasContext;
                    gl.bind_texture(glow::TEXTURE_2D, Some(t));
                };
                if let Some(sprites) = self.sprites.as_ref() {
                    sprites.render(&mut window.gl, programs, camera, aspect, &batches.holes, bind);
                }
                window.gl.set_depth_write(true);
                if let Some(robot) = self.robot.as_ref() {
                    for data in robot_data {
                        robot.render(&mut window.gl, programs, camera, aspect, decision, data);
                    }
                }
                if let Some(sprites) = self.sprites.as_ref() {
                    sprites.render(&mut window.gl, programs, camera, aspect, &batches.health, bind);
                    sprites.render(&mut window.gl, programs, camera, aspect, &batches.effects, bind);
                }
            }
        }
        if let (Some(overlay), Some(programs), Some(colors), Some((view, centers, robots))) = (
            self.overlay.as_ref(),
            self.programs.as_ref(),
            map_colors,
            map.as_ref(),
        ) {
            if in_2d || show_minimap {
                let mode = if in_2d { MapMode::Full2D } else { MapMode::Minimap };
                overlay.render_map(&mut window.gl, programs, mode, *view, centers, robots, colors);
            }
            if !in_2d {
                if let Some(c) = crosshair_color {
                    overlay.render_crosshair(&mut window.gl, programs, c);
                }
            }
        }

        // Text overlays (top-left, logical px).
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

            if let Some(lines) = new_game_lines.as_ref() {
                draw_block(lines, 4.0, 4.0, color);
            } else if let Some((_, _, _, hud, prompt, _)) = scene.as_ref() {
                let y = draw_block(hud, 4.0, 4.0, color);
                // Profiler pane on the right third (design §5.3 split).
                if let Some(pl) = profiler_lines.as_ref() {
                    let pane_x = (screen.0 as f32) * (2.0 / 3.0);
                    draw_block(pl, pane_x, 4.0, [0.7, 0.9, 0.7, 1.0]);
                }
                // Confirm modal prompt, centered-ish (non-blocking overlay).
                if let Some(p) = prompt {
                    let cx = (screen.0 as f32) * 0.35;
                    let cy = (screen.1 as f32) * 0.4;
                    draw_block(p, cx, cy, [1.0, 1.0, 0.4, 1.0]);
                    draw_block(confirm_hint(), cx, cy + line_h * 5.0, [0.9, 0.9, 0.9, 1.0]);
                }
                if let Some(err) = self.init_error.as_ref() {
                    draw_block(err, 4.0, y, [1.0, 0.4, 0.4, 1.0]);
                }
            }
        }

        if let Some(path) = self.pending_capture.take() {
            if let Err(e) = window.capture_png(&path) {
                eprintln!("e2e: capture {} failed: {e}", path.display());
            }
        }
        window.swap_buffers();
    }

    /// The New Game screen's rendered lines (pattern + live count, game/team
    /// combos, password, focus marker) — the in-engine `NewGameDlgProc`.
    fn new_game_lines(&self, ng: &NewGame) -> String {
        let games = self.game_data.as_ref().map(|d| d.game_definitions.as_slice());
        let game_name = games
            .and_then(|g| g.get(ng.game_idx as usize))
            .map(|g| g.name.as_str())
            .unwrap_or("<none>");
        let team_name = games
            .and_then(|g| g.get(ng.game_idx as usize))
            .and_then(|g| g.teams.get(ng.team_idx as usize))
            .map(|t| t.name.as_str())
            .unwrap_or("<none>");
        let pat_focus = if ng.focus == crate::game::screens::Field::Pattern {
            ">"
        } else {
            " "
        };
        let pw_focus = if ng.focus == crate::game::screens::Field::Password {
            ">"
        } else {
            " "
        };
        let stars: String = "*".repeat(ng.password.chars().count());
        let count = ng.match_count();
        let plural = if count == 1 { "" } else { "s" };
        format!(
            "OPERATION PURGE — New Game\n\
             \n\
             {pat_focus} Maze pattern: {}\n\
                Pattern matches {count} file{plural}\n\
             \n\
               Game (Left/Right): {game_name}\n\
               Team (Left/Right): {team_name}\n\
             \n\
             {pw_focus} Password: {stars}\n\
             \n\
             [Tab] switch field   [Enter] OK   [Esc] Cancel\n\
             (empty password enables cheats)",
            ng.pattern,
        )
    }

    /// Per-robot render data (team body color + barrel/tread colors) for the
    /// live robots. The active robot's body would be tinted `Observer2DColor`
    /// in the 2D map (M11); the 3D scene uses team colors.
    fn collect_robot_render_data(&self, game: &Game) -> Vec<RobotRenderData> {
        let data = &game.world.game_data;
        let def = &data.game_definitions[game.world.game_def.get() as usize];
        // PARITY: the active robot is not drawn — the eye sits inside it
        // (`cOperationPurge.cpp:2919-2924`).
        game.world
            .robots
            .iter()
            .filter(|(id, r)| !r.is_dead() && game.active_robot != Some(**id))
            .map(|(_, r)| {
                let body_color = def
                    .teams
                    .get(r.team.get() as usize)
                    .map(|t| t.color)
                    .unwrap_or(Rgba::new(0.3, 0.5, 1.0, 1.0));
                RobotRenderData {
                    position: Vec3::new(r.position.x, r.position.y, 0.0),
                    direction: r.direction,
                    barrel_elevation: r.barrel_elevation,
                    body_color,
                    barrel_color: data.colors.barrel_3d,
                    tread_color: data.colors.tread_3d,
                }
            })
            .collect()
    }

    /// The three 3D sprite passes for this frame. Empty if the textures
    /// are not loaded yet.
    fn collect_sprite_batches(&self, game: &Game) -> SpriteBatches {
        let mut out = SpriteBatches::default();
        let Some(tex) = self.sprite_textures.as_ref() else {
            return out;
        };
        let view = game.camera().view();
        let data = &game.world.game_data;

        // Holes: wall-oriented, unsorted (`cOperationPurge.cpp:2886-2905`).
        out.holes.start(view, false, false);
        for h in &game.world.holes.entries {
            out.holes.add(&SpriteData {
                center: h.position,
                offset: Vec3::ZERO,
                size: glam::Vec2::splat(h.size),
                rotation: h.rotation,
                axis: axis_from_normal(h.normal),
                color: Rgba::new(1.0, 1.0, 1.0, 1.0),
                texture: tex.hole,
            });
        }

        // Health bars over every other live robot (`:2951-3000`).
        out.health.start(view, true, false);
        for (id, r) in game.world.robots.iter() {
            if r.is_dead() || game.active_robot == Some(*id) {
                continue;
            }
            let fraction = if r.health_start > 0.0 { r.health / r.health_start } else { 0.0 };
            let z = crate::sim::robot::ROBOT_SIZE_Z + crate::render::overlay::HEALTH_BAR_HEIGHT * 0.5;
            for seg in crate::render::overlay::health_bar_segments(fraction) {
                out.health.add(&SpriteData {
                    center: Vec3::new(r.position.x, r.position.y, z),
                    offset: Vec3::new(seg.offset_x, 0.0, 0.0),
                    size: glam::Vec2::new(seg.width, crate::render::overlay::HEALTH_BAR_HEIGHT),
                    rotation: 0.0,
                    axis: Axis::PositiveZ,
                    color: seg.color,
                    texture: tex.white,
                });
            }
        }

        // Projectiles (black, `cProjectile.cpp:196-214`) then particles.
        out.effects.start(view, true, true);
        for p in &game.world.projectiles {
            let i = p.projectile_type.get() as usize;
            if crate::data::assets::names::projectile_texture(i).is_none() {
                continue;
            }
            let size = data.projectile_types[i].size;
            out.effects.add(&SpriteData {
                center: p.position,
                offset: Vec3::ZERO,
                size: glam::Vec2::splat(size),
                rotation: 0.0,
                axis: Axis::PositiveZ,
                color: Rgba::new(0.0, 0.0, 0.0, 1.0),
                texture: tex.projectile,
            });
        }
        for p in game.world.particles.slots.iter().flatten() {
            out.effects.add(&SpriteData {
                center: p.position,
                offset: Vec3::ZERO,
                size: glam::Vec2::splat(p.size(data)),
                rotation: p.rotation(),
                axis: Axis::PositiveZ,
                color: p.color(data),
                texture: tex.smoke,
            });
        }
        out
    }

    fn collect_center_render_data(&self, game: &Game) -> Vec<CenterRenderData> {
        game.world
            .centers
            .iter()
            .map(|c| CenterRenderData {
                position_x: c.position.x,
                position_y: c.position.y,
                orientation: c.orientation_angle,
                size: c.size,
                color: c.color,
            })
            .collect()
    }

    /// Per-robot inputs for the 2D map (`Render2D`, `cOperationPurge.cpp:2781-2834`):
    /// the active robot is tinted `observer_2d`, others use their team
    /// color; an observer point is drawn as one more observer-tinted marker.
    fn collect_map_render_data(&self, game: &Game) -> Vec<MapRobotData> {
        use crate::game::ObserverType;
        let data = &game.world.game_data;
        let def = &data.game_definitions[game.world.game_def.get() as usize];
        let observer = data.colors.observer_2d;
        let point = game.observer_type == ObserverType::Point;
        let mut out: Vec<MapRobotData> = game
            .world
            .robots
            .iter()
            .filter(|(_, r)| !r.is_dead())
            .map(|(id, r)| {
                let team_color = def
                    .teams
                    .get(r.team.get() as usize)
                    .map(|t| t.color)
                    .unwrap_or(Rgba::new(0.3, 0.5, 1.0, 1.0));
                let marker_color = if !point && game.active_robot == Some(*id) {
                    observer
                } else {
                    team_color
                };
                MapRobotData {
                    position: r.position,
                    direction: r.direction,
                    marker_color,
                }
            })
            .collect();
        if point {
            out.push(MapRobotData {
                position: glam::Vec2::new(game.observer_point.x, game.observer_point.y),
                direction: game.observer_theta,
                marker_color: observer,
            });
        }
        out
    }

    fn collect_map_center_data(&self, game: &Game) -> Vec<MapCenterData> {
        game.world
            .centers
            .iter()
            .map(|c| MapCenterData {
                position: c.position,
                size: c.size,
                color: c.color,
            })
            .collect()
    }

    /// The frame pump (design §5.1). `execute_commands` runs regardless of
    /// focus; the sim only advances while focused.
    fn pump(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        let dt = self
            .last_frame
            .map(|last| now.duration_since(last).as_secs_f32().min(1.0))
            .unwrap_or(0.0);
        self.last_frame = Some(now);
        let dt = self.fixed_dt.unwrap_or(dt);

        if self.run_e2e_steps(event_loop) {
            return;
        }

        self.input.begin_frame();

        let cursor_win = self.cursor_fraction();
        if let (Some(game), Some(c)) = (self.game.as_mut(), cursor_win) {
            game.cursor_win = c;
        }

        // While the New Game screen is up, the game (if any) is frozen — the
        // loop only pumps the screen (input handled in `window_event`).
        if self.new_game.is_none() {
            if let Some(game) = self.game.as_mut() {
                // PARITY: ExecuteCommands runs every frame (`cMainWindow.cpp:923`,
                // design §5.1).
                let was_confirm = game.in_confirm();
                {
                    crate::profile_scope!("ExecuteCommands");
                    game.execute_commands(&self.input, dt);
                }
                if self.focused || self.e2e.is_some() {
                    crate::profile_scope!("FrameChange");
                    game.advance(dt);
                }
                // Profiler running toggle (P): thread-local enable.
                if game.command_just_pressed(Command::ToggleProfilerRunning, &self.input) {
                    let now_on = crate::profiler::with_profiler(|p| {
                        p.set_enabled(!p.enabled());
                        p.enabled()
                    });
                    eprintln!("profiler running: {now_on}");
                }
                if game.command_just_pressed(Command::ToggleProfilerVisible, &self.input) {
                    self.profiler_visible = !self.profiler_visible;
                }

                // PARITY (design §5.4): stop all sounds the instant a modal
                // opens, *before* the frozen frame renders.
                if !was_confirm && game.in_confirm() {
                    if let Some(audio) = self.audio.as_mut() {
                        audio.stop_all();
                    }
                }
            }

            if let Some(game) = self.game.as_mut() {
                game.update_map_view();
            }

            // Audio: listener cadence, per-event one-shots, alarm loops (design §5.4).
            if let Some(game) = self.game.as_mut() {
                match self.audio.as_mut() {
                    Some(audio) => audio.frame(game),
                    None => game.world.sound_events.clear(),
                }
            }
        }

        self.sync_cursor_mode();
        self.render();

        if let Some(win) = self.window.as_ref() {
            win.window.request_redraw();
        }
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
        let Some(win) = self.window.as_ref() else {
            return;
        };
        let play_3d = self.new_game.is_none()
            && self
                .game
                .as_ref()
                .is_some_and(|g| !g.in_2d() && !g.in_confirm());
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
            eprintln!("cursor grab failed: {e}");
        }
        if let Some(win) = self.window.as_mut() {
            win.cursor_captured = play_3d;
        }
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
                        e2e::Target::Key(code) => self.on_key(e2e::physical(code), None, pressed, event_loop),
                        e2e::Target::Mouse(b) => {
                            if let Some(k) = Key::from_mouse_button(b) {
                                self.input.on_key_event(k, pressed);
                            }
                        }
                    }
                }
                e2e::Step::Type(text) => {
                    for ch in text.chars() {
                        let s = ch.to_string();
                        self.on_key(PhysicalKey::Unidentified(winit::keyboard::NativeKeyCode::Unidentified), Some(&s), true, event_loop);
                    }
                }
                e2e::Step::Cursor(fx, fy) => self.set_cursor_fraction(glam::Vec2::new(fx, fy)),
                e2e::Step::CursorWorld(x, y) => {
                    if let Some(f) = self.game.as_ref().map(|g| g.log_to_win(glam::Vec2::new(x, y))) {
                        self.set_cursor_fraction(f);
                    }
                }
                e2e::Step::CursorRobot(n) => {
                    let target = self.game.as_ref().and_then(|g| {
                        g.world
                            .robots
                            .values()
                            .filter(|r| !r.is_dead() && r.team == g.world.active_team)
                            .nth(n)
                            .map(|r| g.log_to_win(r.position))
                    });
                    match target {
                        Some(f) => self.set_cursor_fraction(f),
                        None => eprintln!("e2e: no live friendly robot #{n}"),
                    }
                }
                e2e::Step::Mouse(dx, dy) => self.input.on_mouse_motion(dx, dy),
                e2e::Step::Wheel(n) => self.input.on_wheel(n),
                e2e::Step::Shot(name) => {
                    self.pending_capture = Some(driver.out_dir.join(format!("{name}.png")));
                }
                e2e::Step::State(name) => {
                    let path = driver.out_dir.join(format!("{name}.txt"));
                    if let Err(e) = std::fs::write(&path, self.state_dump()) {
                        eprintln!("e2e: cannot write {}: {e}", path.display());
                    }
                }
                e2e::Step::Quit => quit = true,
            }
        }
        self.e2e = Some(driver);
        if quit {
            let _ = crate::profiler::write_exit_dump();
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

    /// A plain-text snapshot of the app + game for E2E assertions.
    fn state_dump(&self) -> String {
        use std::fmt::Write;
        let mut out = String::new();
        let _ = writeln!(out, "new_game_open={}", self.new_game.is_some());
        let _ = writeln!(out, "init_error={:?}", self.init_error);
        if let Some(ng) = self.new_game.as_ref() {
            let _ = writeln!(
                out,
                "new_game pattern={:?} matches={} game={} team={} password_len={}",
                ng.pattern,
                ng.match_count(),
                ng.game_idx,
                ng.team_idx,
                ng.password.chars().count()
            );
        }
        let Some(g) = self.game.as_ref() else {
            let _ = writeln!(out, "game=none");
            return out;
        };
        let w = &g.world;
        let _ = writeln!(out, "screen={:?}", g.screen);
        let _ = writeln!(out, "confirm={:?}", g.confirm_kind());
        let _ = writeln!(out, "observer_type={:?}", g.observer_type);
        let _ = writeln!(out, "active_robot={:?}", g.active_robot);
        let _ = writeln!(out, "active_is_enemy={}", g.active_is_enemy);
        let _ = writeln!(out, "cheats_enabled={}", g.cheats_enabled);
        let _ = writeln!(out, "game_speed={:.3}", g.game_speed);
        let _ = writeln!(out, "game_time={:.3}", w.game_time);
        let _ = writeln!(out, "show_map={}", g.show_map);
        let _ = writeln!(out, "map_center={:?} map_size={:?}", g.map_center, g.map_size);
        let _ = writeln!(out, "cursor_win={:?} cursor_world={:?}", g.cursor_win, g.win_to_log(g.cursor_win));
        let _ = writeln!(out, "status={:?}", w.status());
        let _ = writeln!(out, "robots_alive={}", w.robots.values().filter(|r| !r.is_dead()).count());
        let _ = writeln!(out, "projectiles={}", w.projectiles.len());
        let _ = writeln!(out, "particles={}", w.particles.live_count());
        let _ = writeln!(out, "holes={}", w.holes.entries.len());
        let _ = writeln!(out, "effects={}", w.effects.len());
        for (id, r) in w.robots.iter() {
            let _ = writeln!(
                out,
                "robot {:?} team={} dead={} pos=({:.2},{:.2}) dir={:.2} health={:.1}/{:.1}",
                id,
                r.team.get(),
                r.is_dead(),
                r.position.x,
                r.position.y,
                r.direction,
                r.health,
                r.health_start
            );
        }
        match self.audio.as_ref() {
            Some(a) => {
                let _ = writeln!(out, "audio=on {}", a.debug_line());
            }
            None => {
                let _ = writeln!(out, "audio=off");
            }
        }
        let _ = writeln!(out, "--- hud ---\n{}", g.status_text());
        out
    }
}

/// GL handles for the sprite textures (white = untextured sprites).
struct SpriteTextures {
    projectile: glow::Texture,
    hole: glow::Texture,
    smoke: glow::Texture,
    white: glow::Texture,
}

#[derive(Default)]
struct SpriteBatches {
    holes: SpriteBatch<glow::Texture>,
    health: SpriteBatch<glow::Texture>,
    effects: SpriteBatch<glow::Texture>,
}

/// A hole's outward wall normal as the sprite orientation axis.
fn axis_from_normal(n: Vec3) -> Axis {
    if n.x > 0.5 {
        Axis::PositiveX
    } else if n.x < -0.5 {
        Axis::NegativeX
    } else if n.y > 0.5 {
        Axis::PositiveY
    } else if n.y < -0.5 {
        Axis::NegativeY
    } else if n.z > 0.5 {
        Axis::PositiveZ
    } else {
        Axis::NegativeZ
    }
}

/// A 1x1 opaque-white texture used as a fallback when a building BMP is
/// missing. Runtime-only.
fn white_texture(gl: &std::rc::Rc<glow::Context>) -> Texture {
    let image = DecodedImage {
        width: 1,
        height: 1,
        rgba: vec![255, 255, 255, 255],
    };
    Texture::upload(gl, &image).expect("1x1 white texture upload")
}

impl ApplicationHandler for AppRunner {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        match RenderWindow::create(event_loop, self.config.vsync, self.windowed) {
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
                self.window = Some(win);
                // Compile shaders, then open the New Game screen at startup
                // (design §5.5) — the world/renderers are built on OK.
                let win_ref = self.window.take().unwrap();
                self.init_gl(&win_ref);
                self.window = Some(win_ref);
                self.open_new_game_screen(false);
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
        // Scripted runs take input only from the script, not the real desk.
        if self.e2e.is_some()
            && matches!(
                event,
                WindowEvent::KeyboardInput { .. }
                    | WindowEvent::CursorMoved { .. }
                    | WindowEvent::MouseInput { .. }
                    | WindowEvent::MouseWheel { .. }
            )
        {
            return;
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::KeyboardInput { event, .. } => {
                let pressed = event.state == ElementState::Pressed;
                self.on_key(event.physical_key, event.text.as_deref(), pressed, event_loop);
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = Some((position.x, position.y));
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if let Some(key) = Key::from_mouse_button(button) {
                    self.input.on_key_event(key, state == ElementState::Pressed);
                }
            }
            WindowEvent::MouseWheel { delta, .. } => match delta {
                MouseScrollDelta::LineDelta(_, y) => self.input.on_wheel(y),
                MouseScrollDelta::PixelDelta(pos) => self.input.on_wheel((pos.y / 120.0) as f32),
            },
            // Scripted runs must not depend on which app the OS focuses.
            WindowEvent::Focused(_) if self.e2e.is_some() => {}
            WindowEvent::Focused(focused) => {
                self.focused = focused;
                if !focused {
                    // DISCL_FOREGROUND parity: focus loss clears input (§5.2)
                    // and any in-flight possession control.
                    self.input.clear_on_focus_loss();
                    if let Some(game) = self.game.as_mut() {
                        game.world.clear_inputs();
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
                            eprintln!("atlas rebake failed: {e}");
                        }
                    }
                }
            }
            WindowEvent::RedrawRequested => self.pump(event_loop),
            _ => {}
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        if self.e2e.is_some() {
            return;
        }
        if let DeviceEvent::MouseMotion { delta } = event {
            let scale = self.config.mouse_scale as f64;
            self.input.on_mouse_motion(delta.0 * scale, delta.1 * scale);
        }
    }

    /// Dump the profiler tree to `Profile output.txt` on shutdown (design
    /// §5.3). No-op if nothing was recorded.
    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        if let Err(e) = crate::profiler::write_exit_dump() {
            eprintln!("could not write Profile output.txt: {e}");
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

// ============================ audio (runtime) ============================

struct GameAudio {
    sounds: crate::audio::SpatialSounds<crate::audio::KiraBackend>,
    listener_frame: u32,
    clips: std::collections::HashMap<&'static str, crate::audio::ClipId>,
    /// One-shots started since launch (E2E state dump).
    played: u64,
}

impl GameAudio {
    /// Open the device, preload every alarm + per-event clip, and register a
    /// voice pool (`nSimultaneousInstances`) per clip. Returns `None` (muted)
    /// on any device/decoding failure so the game still runs.
    fn new(assets: &std::path::Path, data: &GameData) -> Option<Self> {
        use crate::data::assets::names;
        let voices = data.sounds.simultaneous_instances.max(1) as usize;
        let capacity = crate::audio::sound_capacity(names::ALL_SOUNDS.len(), voices);
        let backend = match crate::audio::KiraBackend::new(capacity) {
            Ok(b) => b,
            Err(e) => {
                crate::log_error!("audio disabled (no device): {e}");
                return None;
            }
        };
        let sounds_dir = crate::data::assets::AssetDirs::new(assets).sounds;

        let mut sounds = crate::audio::SpatialSounds::new(backend);
        let mut clips = std::collections::HashMap::new();
        for &name in names::ALL_SOUNDS {
            let path = sounds_dir.join(name);
            match crate::audio::decode_wav(&path) {
                Ok(data) => {
                    clips.insert(name, sounds.register_clip(data, voices));
                }
                Err(e) => crate::log_error!("could not preload {}: {e}", path.display()),
            }
        }
        sounds.set_alarm_clips(
            clips.get(names::ENEMY_VISIBLE_SOUND).copied(),
            clips.get(names::TARGETED_SOUND).copied(),
        );

        Some(Self {
            sounds,
            listener_frame: 0,
            clips,
            played: 0,
        })
    }

    /// Refresh the DS3D listener (every 10th frame) + drive the alarm loops.
    fn frame(&mut self, game: &mut Game) {
        self.listener_frame =
            (self.listener_frame + 1) % crate::audio::spatial::LISTENER_UPDATE_INTERVAL_FRAMES;
        if self.listener_frame == 0 {
            let (pos, rot) = game_listener_pose(game);
            game.world.set_audio_listener(pos, rot);
        }

        let listener = game.world.audio_listener;
        let live = game_live_listener(game);
        for ev in std::mem::take(&mut game.world.sound_events) {
            use crate::data::assets::names;
            use crate::sim::world::SoundKind;
            let name = match ev.kind {
                SoundKind::Fire(t) => names::projectile_sounds(t.get() as usize).0,
                SoundKind::Collision(t) => names::projectile_sounds(t.get() as usize).1,
                SoundKind::Destroy => names::DESTROY_SOUND,
            };
            if let Some(&clip) = self.clips.get(name) {
                self.sounds.play_3d(clip, ev.origin, &listener, live);
                self.played += 1;
            }
        }

        // Alarms only sound in 3D; the 2D map / modals stop them (design §5.4).
        if game.sim_frozen() {
            self.sounds.update_alarms(false, false);
        } else {
            self.sounds.update_alarms(game.enemy_visible, game.targeted);
        }
    }

    fn debug_line(&self) -> String {
        format!("sounds_played={} alarm={:?}", self.played, self.sounds.current_alarm())
    }

    /// Stop every voice + alarm (`StopAllSounds` — used before a modal opens,
    /// design §5.4 ordering).
    fn stop_all(&mut self) {
        self.sounds.update_alarms(false, false);
        self.sounds.stop_all();
    }
}

fn game_listener_pose(game: &Game) -> (glam::Vec2, f32) {
    use crate::game::ObserverType;
    match game.observer_type {
        ObserverType::Point => (
            glam::Vec2::new(game.observer_point.x, game.observer_point.y),
            game.observer_theta,
        ),
        _ => game
            .active_robot
            .and_then(|id| game.world.robots.get(&id))
            .map(|r| (r.position, r.direction))
            .unwrap_or((glam::Vec2::ZERO, 0.0)),
    }
}

/// Where the 3D voice-steal rule measures from this frame
/// (`cOperationPurge.cpp:1911-1920`): the observer point, else the active robot.
fn game_live_listener(game: &Game) -> glam::Vec3 {
    match game.observer_type {
        crate::game::ObserverType::Point => game.observer_point,
        _ => game_listener_pose(game).0.extend(0.0),
    }
}

/// Profiler tree pane text (design §5.3): a flat readout of the top-level
/// scopes with their total time + call count, refreshed each frame.
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
fn confirm_hint() -> &'static str {
    "[Y] Yes   [N] No   [C] Cancel"
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
