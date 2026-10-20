//! Window-free app state: the New Game screen, the running game, and the
//! key routing between them and the confirm modals. The winit glue in
//! `app` feeds it events and applies the [`AppEffect`]s it returns.

use std::path::{Path, PathBuf};

use glam::Vec2;

use super::config::Config;
use crate::data::assets::AssetDirs;
use crate::data::GameData;
use crate::game::newgame::{NewGame, NewGameOutcome, NewGameParams};
use crate::game::screens::{ConfirmAction, ConfirmChoice};
use crate::game::Game;
use crate::ids::{GameDefId, TeamId};
use crate::input::commands::{binding, Command};
use crate::input::keys::Key;
use crate::input::Input;
use crate::maze::Maze;
use crate::rng::GameRng;
use crate::sim::world::World;

/// What the glue must do after a [`Shell`] call.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AppEffect {
    None,
    /// Quit the app.
    Exit,
    /// A new game was built: load its building into the renderer and
    /// silence the previous game's sounds.
    GameStarted,
    /// A confirm modal just opened: stop all sounds before it renders.
    ModalOpened,
}

pub struct Shell {
    pub config: Config,
    /// Where an accepted New Game selection is persisted; `None` skips it.
    config_path: Option<PathBuf>,
    pub game_data: Option<GameData>,
    pub input: Input,
    /// Seeds the New Game random maze pick (`rand () % vszMazeFiles.size ()`).
    rng: GameRng,
    /// The New Game screen; while open it is drawn instead of the game,
    /// which stays frozen underneath.
    pub new_game: Option<NewGame>,
    pub game: Option<Game>,
    /// Shown in red under the HUD or the New Game screen.
    pub init_error: Option<String>,
    /// `[` toggles the profiler pane (P toggles recording).
    pub profiler_visible: bool,
}

impl Shell {
    /// Load `GameData.toml` from the configured assets and open the New Game
    /// screen; a missing or broken data file leaves the screen up with the
    /// error shown.
    pub fn new(config: Config, config_path: Option<PathBuf>) -> Self {
        let path = AssetDirs::new(&config.assets_dir).game_data_toml();
        let (game_data, init_error) = match crate::data::gamedata::load(&path) {
            Ok(data) => (Some(data), None),
            Err(e) => {
                let msg = format!("could not load {}: {e}", path.display());
                crate::log_error!("{msg}");
                (None, Some(msg))
            }
        };
        let mut shell = Self::with_data(config, config_path, game_data);
        shell.init_error = init_error;
        shell
    }

    pub fn with_data(config: Config, config_path: Option<PathBuf>, game_data: Option<GameData>) -> Self {
        let rng = GameRng::from_seed(resolve_seed(&config));
        let mut shell = Self {
            config,
            config_path,
            game_data,
            input: Input::new(),
            rng,
            new_game: None,
            game: None,
            init_error: None,
            profiler_visible: false,
        };
        shell.open_new_game_screen(false);
        shell
    }

    fn assets_dir(&self) -> &Path {
        Path::new(&self.config.assets_dir)
    }

    /// The running game while the New Game screen is closed.
    pub fn playing_mut(&mut self) -> Option<&mut Game> {
        self.game.as_mut().filter(|_| self.new_game.is_none())
    }

    /// Whether the player is in 3D with no screen or modal over it.
    pub fn in_3d_play(&self) -> bool {
        self.new_game.is_none() && self.game.as_ref().is_some_and(|g| !g.in_2d() && !g.in_confirm())
    }

    /// Open the New Game screen seeded from the persisted config
    /// (`WM_INITDIALOG`, `cMainWindow.cpp:647`). `invoked_mid_game`: Cancel
    /// resumes the running game instead of quitting.
    pub fn open_new_game_screen(&mut self, invoked_mid_game: bool) {
        let mut ng = NewGame::new(
            self.config.maze_file.clone(),
            self.config.game_index,
            self.config.team_index,
            invoked_mid_game,
        );
        if let Some(data) = self.game_data.as_ref() {
            ng.clamp_selection(data);
        }
        ng.refresh_matches(self.assets_dir());
        self.new_game = Some(ng);
    }

    /// Build the world and game for accepted New Game params (`SetGameParams`
    /// then `StartGame`, `cOperationPurge.cpp:104-306`) and persist the
    /// selection (`cMainWindow.cpp:767-776`).
    fn start_new_game(&mut self, params: &NewGameParams) -> AppEffect {
        self.init_error = None;
        let Some(data) = self.game_data.clone() else {
            self.init_error = Some("no GameData.toml — game not started".into());
            return AppEffect::None;
        };
        let maze = match Maze::load(&params.maze_path) {
            Ok(m) => m,
            Err(e) => {
                self.init_error = Some(format!("maze load failed: {e}"));
                return AppEffect::None;
            }
        };

        let world = World::new(
            data,
            maze,
            GameDefId(params.game_idx),
            TeamId(params.team_idx),
            resolve_seed(&self.config),
        );
        let mut game = Game::new(world);
        game.cheats_enabled = params.cheats_enabled;
        self.game = Some(game);

        self.config.apply_new_game(params);
        if let Some(path) = self.config_path.as_deref() {
            // Re-read so CLI-only overrides (`--seed`, `--assets`) never persist.
            let mut saved = Config::load_from(path);
            saved.apply_new_game(params);
            if let Err(e) = saved.save_to(path) {
                crate::log_error!("could not write {}: {e}", path.display());
            }
        }

        self.new_game = None;
        AppEffect::GameStarted
    }

    /// Route one key edge: the New Game screen, else an open modal, else the
    /// gameplay input. `key` is `None` for text-only events.
    pub fn on_key(&mut self, key: Option<Key>, text: Option<&str>, pressed: bool, shift: bool) -> AppEffect {
        // Releases always reach the game input: a key held when a screen or
        // modal opened would otherwise read as down forever after it closes.
        if !pressed {
            if let Some(k) = key {
                self.input.on_key_event(k, false);
            }
            return AppEffect::None;
        }
        if self.new_game.is_some() {
            return self.new_game_key(key, text, shift);
        }
        if self.game.as_ref().is_some_and(|g| g.in_confirm()) {
            return self.confirm_key(key);
        }
        // Esc is the EndGame command; it only quits outright with no game.
        if self.game.is_none() && key == Some(Key::Escape) {
            return AppEffect::Exit;
        }
        if let Some(k) = key {
            self.input.on_key_event(k, true);
        }
        AppEffect::None
    }

    /// The New Game screen's keys: Tab cycles the focused field, Left/Right
    /// cycle the Game combo (the Team combo with Shift), Enter = OK,
    /// Esc = Cancel, Backspace edits, and typed text feeds the focused field.
    fn new_game_key(&mut self, key: Option<Key>, text: Option<&str>, shift: bool) -> AppEffect {
        let assets = self.assets_dir().to_path_buf();
        let Some(ng) = self.new_game.as_mut() else {
            return AppEffect::None;
        };
        let outcome = match key {
            Some(Key::Enter | Key::NumpadEnter) => ng.ok(&mut self.rng),
            Some(Key::Escape) => ng.cancel(),
            Some(Key::Tab) => {
                ng.cycle_focus();
                return AppEffect::None;
            }
            Some(k @ (Key::ArrowLeft | Key::ArrowRight)) => {
                let delta = if k == Key::ArrowRight { 1 } else { -1 };
                if let Some(d) = self.game_data.as_ref() {
                    if shift {
                        ng.cycle_team(delta, d);
                    } else {
                        ng.cycle_game(delta, d);
                    }
                }
                return AppEffect::None;
            }
            Some(Key::Backspace) => {
                if ng.backspace() {
                    ng.refresh_matches(&assets);
                }
                return AppEffect::None;
            }
            _ => {
                let mut changed = false;
                for c in text.unwrap_or_default().chars() {
                    changed |= ng.type_char(c);
                }
                if changed {
                    ng.refresh_matches(&assets);
                }
                return AppEffect::None;
            }
        };
        match outcome {
            NewGameOutcome::Accept(params) => self.start_new_game(&params),
            // The original's "invalid maze file" MessageBox kept the dialog
            // open (`cMainWindow.cpp:763`).
            NewGameOutcome::Invalid => {
                self.init_error = Some("Pattern matches no maze files.".into());
                AppEffect::None
            }
            NewGameOutcome::CancelExit => AppEffect::Exit,
            NewGameOutcome::CancelResume => {
                self.new_game = None;
                AppEffect::None
            }
        }
    }

    /// An open confirm modal's keys: Y / N, plus C where it offers Cancel
    /// (`MB_YESNOCANCEL`).
    fn confirm_key(&mut self, key: Option<Key>) -> AppEffect {
        let Some(game) = self.game.as_mut() else {
            return AppEffect::None;
        };
        let offers_cancel = game.confirm_kind().is_some_and(|k| k.offers_cancel());
        let choice = match key {
            Some(Key::KeyY) => ConfirmChoice::Yes,
            Some(Key::KeyN) => ConfirmChoice::No,
            Some(Key::KeyC) if offers_cancel => ConfirmChoice::Cancel,
            _ => return AppEffect::None,
        };
        if game.resolve_confirm(choice) == ConfirmAction::NewGame {
            self.open_new_game_screen(true);
        }
        AppEffect::None
    }

    /// One frame (`cMainWindow::FrameChange`): snapshot input, then run
    /// `ExecuteCommands` always and `FrameChange` only when `sim_live`.
    /// `cursor` is the mouse as a window fraction, if known.
    pub fn frame(&mut self, dt: f32, cursor: Option<Vec2>, sim_live: bool) -> AppEffect {
        self.input.begin_frame();
        if let (Some(game), Some(c)) = (self.game.as_mut(), cursor) {
            game.cursor_win = c;
        }
        if self.new_game.is_some() {
            return AppEffect::None;
        }
        let Some(game) = self.game.as_mut() else {
            return AppEffect::None;
        };

        let was_confirm = game.in_confirm();
        {
            crate::profile_scope!("ExecuteCommands");
            game.execute_commands(&self.input, dt);
        }
        if sim_live {
            crate::profile_scope!("FrameChange");
            game.advance(dt);
        }
        if binding(Command::ToggleProfilerRunning).just_pressed(&self.input) {
            let on = crate::profiler::with_profiler(|p| {
                p.set_enabled(!p.enabled());
                p.enabled()
            });
            println!("profiler running: {on}");
        }
        if binding(Command::ToggleProfilerVisible).just_pressed(&self.input) {
            self.profiler_visible = !self.profiler_visible;
        }
        let opened = !was_confirm && game.in_confirm();
        game.update_map_view();
        if opened {
            AppEffect::ModalOpened
        } else {
            AppEffect::None
        }
    }

    /// Focus loss clears all input and any in-flight possession control
    /// (`DISCL_FOREGROUND`).
    pub fn on_focus_lost(&mut self) {
        self.input.clear_on_focus_loss();
        if let Some(game) = self.game.as_mut() {
            game.world.clear_inputs();
        }
    }
}

/// The configured seed, else the wall clock.
fn resolve_seed(config: &Config) -> u64 {
    config.seed.unwrap_or_else(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x0522_1101)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::screens::ConfirmKind;

    fn assets() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("assets")
    }

    fn config() -> Config {
        Config {
            assets_dir: assets().display().to_string(),
            maze_file: "MazeFiles/Small1.maz".into(),
            seed: Some(7),
            ..Config::default()
        }
    }

    fn shell_with(config: Config) -> Shell {
        let data = crate::data::gamedata::load(&assets().join("GameData.toml")).unwrap();
        Shell::with_data(config, None, Some(data))
    }

    fn press(shell: &mut Shell, key: Key, shift: bool) -> AppEffect {
        let effect = shell.on_key(Some(key), None, true, shift);
        shell.on_key(Some(key), None, false, shift);
        effect
    }

    fn started() -> Shell {
        let mut shell = shell_with(config());
        assert_eq!(press(&mut shell, Key::Enter, false), AppEffect::GameStarted);
        shell
    }

    /// Hold `key` across a frame boundary so the game sees its press edge.
    fn frame_with(shell: &mut Shell, key: Key) -> AppEffect {
        shell.on_key(Some(key), None, true, false);
        shell.frame(0.016, None, true)
    }

    #[test]
    fn shift_arrows_cycle_the_team_and_plain_arrows_the_game() {
        let mut shell = shell_with(config());
        press(&mut shell, Key::ArrowRight, true);
        let ng = shell.new_game.as_ref().unwrap();
        assert_eq!((ng.game_idx, ng.team_idx), (0, 1));
        press(&mut shell, Key::ArrowRight, false);
        let ng = shell.new_game.as_ref().unwrap();
        assert_eq!((ng.game_idx, ng.team_idx), (1, 0));
    }

    #[test]
    fn out_of_range_config_indices_clamp() {
        let shell = shell_with(Config {
            game_index: 999,
            team_index: 999,
            ..config()
        });
        let ng = shell.new_game.as_ref().unwrap();
        let data = shell.game_data.as_ref().unwrap();
        assert_eq!(ng.game_idx as usize, data.game_definitions.len() - 1);
        assert_eq!(ng.team_idx as usize, ng.team_count(data) - 1);
    }

    #[test]
    fn a_pattern_with_no_matches_keeps_the_screen_up_with_the_error() {
        let mut shell = shell_with(Config {
            maze_file: "MazeFiles/None*.maz".into(),
            ..config()
        });
        assert_eq!(press(&mut shell, Key::Enter, false), AppEffect::None);
        assert!(shell.new_game.is_some());
        assert_eq!(shell.init_error.as_deref(), Some("Pattern matches no maze files."));
    }

    #[test]
    fn a_missing_data_file_is_reported_on_ok() {
        let mut shell = Shell::with_data(config(), None, None);
        press(&mut shell, Key::Enter, false);
        assert!(shell.new_game.is_some());
        assert!(shell.init_error.as_deref().unwrap().contains("GameData.toml"));
    }

    #[test]
    fn starting_a_game_clears_the_previous_error() {
        let mut shell = shell_with(config());
        shell.init_error = Some("stale".into());
        press(&mut shell, Key::Enter, false);
        assert_eq!(shell.init_error, None);
    }

    #[test]
    fn cancel_at_startup_exits_and_mid_game_resumes() {
        let mut shell = shell_with(config());
        assert_eq!(press(&mut shell, Key::Escape, false), AppEffect::Exit);

        let mut shell = started();
        shell.open_new_game_screen(true);
        assert_eq!(press(&mut shell, Key::Escape, false), AppEffect::None);
        assert!(shell.new_game.is_none() && shell.game.is_some());
    }

    #[test]
    fn esc_held_through_the_end_game_modal_does_not_reopen_it() {
        let mut shell = started();
        shell.frame(0.016, None, true);
        assert_eq!(frame_with(&mut shell, Key::Escape), AppEffect::ModalOpened);
        assert_eq!(shell.game.as_ref().unwrap().confirm_kind(), Some(ConfirmKind::EndGame));

        shell.on_key(Some(Key::KeyN), None, true, false);
        shell.frame(0.016, None, true);
        shell.frame(0.016, None, true);
        assert!(!shell.game.as_ref().unwrap().in_confirm());
    }

    #[test]
    fn end_game_yes_opens_the_new_game_screen() {
        let mut shell = started();
        shell.frame(0.016, None, true);
        frame_with(&mut shell, Key::Escape);
        press(&mut shell, Key::KeyY, false);
        assert!(shell.new_game.as_ref().is_some_and(|ng| ng.invoked_mid_game));
    }

    #[test]
    fn cancelling_2d_restores_the_previous_robot() {
        let mut shell = started();
        let game = shell.game.as_mut().unwrap();
        let mut ids = game.world.robots.keys().copied();
        let (first, second) = (ids.next().unwrap(), ids.next().unwrap());
        game.select_robot(first);
        game.set_observer();
        shell.frame(0.016, None, true);

        frame_with(&mut shell, binding(Command::Enter2DMode).key);
        shell.on_key(Some(binding(Command::Enter2DMode).key), None, false, false);
        shell.game.as_mut().unwrap().select_robot(second);
        shell.frame(0.016, None, true);
        frame_with(&mut shell, binding(Command::Exit2DMode).key);

        let game = shell.game.as_ref().unwrap();
        assert!(!game.in_2d());
        assert_eq!(game.active_robot, Some(first));
    }
}
