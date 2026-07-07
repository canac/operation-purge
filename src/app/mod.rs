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
use crate::render::robot3d::{RobotRenderData, RobotRenderer};
use crate::render::shaders::{compile_all, Programs};
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
    /// P/[/] profiler pane toggles (design §5.3): running (thread-local),
    /// visible (draws the pane), interaction (keyboard tree nav).
    profiler_visible: bool,
    profiler_interaction: bool,
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
            gate: WindingGate::default(),
            game: None,
            audio: None,
            init_error: None,
            new_game: None,
            rng: GameRng::from_seed(seed),
            profiler_visible: false,
            profiler_interaction: false,
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

        // Build the world + game.
        let seed = self.resolve_seed();
        let world = crate::sim::world::World::new(
            data.clone(),
            maze,
            GameDefId(params.game_idx),
            TeamId(params.team_idx),
            seed,
        );
        let mut game = match Game::new(world) {
            Ok(g) => g,
            Err(e) => {
                self.init_error = Some(format!("game init failed: {e}"));
                return;
            }
        };
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
        match CenterRenderer::new(ctx) {
            Ok(c) => self.center = Some(c),
            Err(e) => self.init_error = Some(format!("center mesh failed: {e}")),
        }

        // Audio (device open + preload). Best-effort: a headless/no-device
        // machine simply runs muted.
        self.audio = GameAudio::new(&assets, &data);

        // Persist the accepted selection (`cMainWindow.cpp:767-776`).
        self.config.apply_new_game(params);
        if let Err(e) = self.config.save() {
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
    fn handle_new_game_key(&mut self, event: &winit::event::KeyEvent, event_loop: &ActiveEventLoop) {
        let assets = std::path::Path::new(&self.config.assets_dir).to_path_buf();
        let data = self.game_data.clone();
        let key = event.physical_key;

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
                    if let Some(txt) = event.text.as_ref() {
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
    fn handle_confirm_key(&mut self, event: &winit::event::KeyEvent, event_loop: &ActiveEventLoop) {
        let choice = match event.physical_key {
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

        let Some(window) = self.window.as_mut() else {
            return;
        };
        window.gl.clear();

        let screen = window.physical_size;
        let aspect = screen.0.max(1) as f32 / screen.1.max(1) as f32;
        let decision = self.gate.decision();

        if let Some((camera, robot_data, center_data, _hud, _prompt)) = scene.as_ref() {
            if let (Some(building), Some(programs)) =
                (self.building.as_ref(), self.programs.as_ref())
            {
                building.render(&mut window.gl, programs, camera, aspect, &self.gate);
            }
            if let (Some(robot), Some(programs)) = (self.robot.as_ref(), self.programs.as_ref()) {
                for data in robot_data {
                    robot.render(&mut window.gl, programs, camera, aspect, decision, data);
                }
            }
            if let (Some(center), Some(programs)) = (self.center.as_ref(), self.programs.as_ref())
            {
                for data in center_data {
                    center.render(&mut window.gl, programs, camera, aspect, decision, data);
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
            } else if let Some((_, _, _, hud, prompt)) = scene.as_ref() {
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
        game.world
            .robots
            .values()
            .filter(|r| !r.is_dead())
            .map(|r| {
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

    /// The frame pump (design §5.1). `execute_commands` runs regardless of
    /// focus; the sim only advances while focused.
    fn pump(&mut self) {
        let now = Instant::now();
        let dt = self
            .last_frame
            .map(|last| now.duration_since(last).as_secs_f32().min(1.0))
            .unwrap_or(0.0);
        self.last_frame = Some(now);

        self.input.begin_frame();

        // While the New Game screen is up, the game (if any) is frozen — the
        // loop only pumps the screen (input handled in `window_event`).
        if self.new_game.is_none() {
            if let Some(game) = self.game.as_mut() {
                // PARITY: ExecuteCommands runs every frame (`cMainWindow.cpp:923`,
                // design §5.1).
                let was_confirm = game.in_confirm();
                game.execute_commands(&self.input, dt);
                if self.focused {
                    game.advance(dt);
                }
                // Profiler running toggle (P): thread-local enable.
                if game.command_just_pressed(Command::ToggleProfilerRunning, &self.input) {
                    let now_on = crate::profiler::with_profiler(|p| {
                        p.set_enabled(!p.enabled);
                        p.enabled
                    });
                    eprintln!("profiler running: {now_on}");
                }
                if game.command_just_pressed(Command::ToggleProfilerVisible, &self.input) {
                    self.profiler_visible = !self.profiler_visible;
                }
                if game.command_just_pressed(Command::ToggleProfilerInteraction, &self.input) {
                    self.profiler_interaction = !self.profiler_interaction;
                }

                // PARITY (design §5.4): stop all sounds the instant a modal
                // opens, *before* the frozen frame renders.
                if !was_confirm && game.in_confirm() {
                    if let Some(audio) = self.audio.as_mut() {
                        audio.stop_all();
                    }
                }
            }

            // Audio: listener cadence + alarm loops (design §5.4).
            if let (Some(audio), Some(game)) = (self.audio.as_mut(), self.game.as_mut()) {
                audio.frame(game);
            }
        }

        self.render();

        if let Some(win) = self.window.as_ref() {
            win.window.request_redraw();
        }
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
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::KeyboardInput { event, .. } => {
                let pressed = event.state == ElementState::Pressed;

                // 1) New Game screen: text fields + Tab/arrows/Enter/Esc.
                if self.new_game.is_some() {
                    if pressed {
                        self.handle_new_game_key(&event, event_loop);
                    }
                    return;
                }

                // 2) Confirm modal: Y/N/C keys (non-blocking, design §1.3).
                if self.game.as_ref().is_some_and(|g| g.in_confirm()) {
                    if pressed {
                        self.handle_confirm_key(&event, event_loop);
                    }
                    return;
                }

                // 3) Normal gameplay: feed the command input snapshot. Esc is
                // NOT intercepted — it flows through as `commandEndGame`
                // (design §5.5). A hard fallback exits only when nothing is
                // running (no game to end).
                if self.game.is_none() {
                    if let PhysicalKey::Code(KeyCode::Escape) = event.physical_key {
                        if pressed {
                            event_loop.exit();
                            return;
                        }
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
                MouseScrollDelta::PixelDelta(pos) => self.input.on_wheel((pos.y / 120.0) as f32),
            },
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
            WindowEvent::RedrawRequested => self.pump(),
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

    /// Dump the profiler tree to `Profile output.txt` on shutdown (design
    /// §5.3). No-op if nothing was recorded.
    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        if let Err(e) = crate::profiler::write_exit_dump() {
            eprintln!("could not write Profile output.txt: {e}");
        }
    }
}

/// Run the winit event loop with the M8 game shell (real runtime only).
pub fn run(config: Config) -> Result<(), GameError> {
    let event_loop = EventLoop::new()
        .map_err(|e| GameError::Other(format!("event loop creation failed: {e}")))?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = AppRunner::new(config);
    event_loop
        .run_app(&mut app)
        .map_err(|e| GameError::Other(format!("event loop error: {e}")))
}

// ============================ audio (runtime) ============================
//
// The audio manager owns `SpatialSounds<KiraBackend>` (design §5.4). It is
// gated `#[cfg(not(test))]` because `KiraBackend` opens a device and is not
// compiled under `cfg(test)`; a stub with the same surface keeps `AppRunner`
// compiling in the (window-free) test build.

#[cfg(not(test))]
struct GameAudio {
    sounds: crate::audio::SpatialSounds<crate::audio::KiraBackend>,
    listener_frame: u32,
}

#[cfg(not(test))]
impl GameAudio {
    /// Open the device, preload every alarm + per-event clip, and register a
    /// voice pool (`nSimultaneousInstances`) per clip. Returns `None` (muted)
    /// on any device/decoding failure so the game still runs.
    fn new(assets: &std::path::Path, data: &GameData) -> Option<Self> {
        let mut backend = match crate::audio::KiraBackend::new() {
            Ok(b) => b,
            Err(e) => {
                eprintln!("audio disabled (no device): {e}");
                return None;
            }
        };
        let voices = data.sounds.simultaneous_instances.max(1) as usize;
        let sounds_dir = assets.join("Sounds");

        // Preload each named clip on the backend (deduped by path). ClipIds
        // are assigned densely from 0, so registering that many pools in the
        // SpatialSounds keeps its pool ClipIds aligned with the backend's.
        let mut ids: Vec<crate::audio::ClipId> = Vec::new();
        let enemy = preload_clip(&mut backend, &sounds_dir, &data.sounds.enemy_visible_sound);
        ids.extend(enemy);
        let targeted = preload_clip(&mut backend, &sounds_dir, &data.sounds.targeted_sound);
        ids.extend(targeted);

        // Per-event clips (fire/collision/destroy) — preloaded so playback is
        // click-free; per-event 3D emission is wired once the World surfaces a
        // frame event log (follow-up).
        let (robot_sounds, projectile_sounds) = crate::audio::per_event_sound_names(data);
        for name in robot_sounds.values() {
            ids.extend(preload_clip(&mut backend, &sounds_dir, name));
        }
        for (fire, collision) in projectile_sounds.values() {
            ids.extend(preload_clip(&mut backend, &sounds_dir, fire));
            ids.extend(preload_clip(&mut backend, &sounds_dir, collision));
        }

        let n_clips = ids.iter().map(|c| c.0 + 1).max().unwrap_or(0);
        let mut sounds = crate::audio::SpatialSounds::new(backend);
        for _ in 0..n_clips {
            sounds.register_clip(voices);
        }
        sounds.set_alarm_clips(enemy, targeted);

        Some(Self {
            sounds,
            listener_frame: 0,
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

        // Alarms only sound in 3D; the 2D map / modals stop them (design §5.4).
        if game.sim_frozen() {
            self.sounds.update_alarms(false, false);
        } else {
            self.sounds.update_alarms(game.enemy_visible, game.targeted);
        }
    }

    /// Stop every voice + alarm (`StopAllSounds` — used before a modal opens,
    /// design §5.4 ordering).
    fn stop_all(&mut self) {
        self.sounds.update_alarms(false, false);
        self.sounds.stop_all();
    }
}

#[cfg(not(test))]
fn preload_clip(
    backend: &mut crate::audio::KiraBackend,
    dir: &std::path::Path,
    name: &str,
) -> Option<crate::audio::ClipId> {
    if name.is_empty() {
        return None;
    }
    let path = dir.join(name);
    match backend.preload(&path) {
        Ok(c) => Some(c),
        Err(e) => {
            eprintln!("could not preload {}: {e}", path.display());
            None
        }
    }
}

#[cfg(not(test))]
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

#[cfg(test)]
struct GameAudio;

#[cfg(test)]
impl GameAudio {
    fn new(_assets: &std::path::Path, _data: &GameData) -> Option<Self> {
        None
    }
    fn frame(&mut self, _game: &mut Game) {}
    fn stop_all(&mut self) {}
}

/// Profiler tree pane text (design §5.3): a flat readout of the top-level
/// scopes with their total time + call count, refreshed each frame.
fn profiler_pane_lines() -> String {
    crate::profiler::with_profiler(|p| {
        let mut out = String::from("PROFILER\n");
        let mut it = p.iter();
        it.first();
        while !it.is_done() {
            if let (Some(name), Some(t), Some(calls)) = (
                it.current_name(),
                it.current_total_time(),
                it.current_total_calls(),
            ) {
                out.push_str(&format!("{name}: {:.3}ms x{calls}\n", t.as_secs_f64() * 1000.0));
            }
            it.next();
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
