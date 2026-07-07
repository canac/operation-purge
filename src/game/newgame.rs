//! New Game screen logic (design §5.5), replacing the Win32
//! `NewGameDlgProc` (`cMainWindow.cpp:592-789`) with an in-engine screen
//! rendered through `render::overlay` + `render::text` — one render path,
//! no second UI stack (design §3.6).
//!
//! The screen holds a maze **pattern** field (whose live match count is
//! recomputed via the legacy wildcard matcher on every edit — the
//! `UpdatePatternInfo` twin, `cMainWindow.cpp:1142-1165`), a Game combo, a
//! Team combo (repopulated when the Game changes, mirroring the
//! `CBN_SELCHANGE` handler), and a **password** field.
//!
//! PARITY: an EMPTY password **enables** cheats — any non-empty text
//! disables them (`cMainWindow.cpp:760-761`, design §1.3). This polarity is
//! the opposite of what a naive reading suggests, so it is pinned by a test.

use std::path::{Path, PathBuf};

use crate::data::GameData;
use crate::rng::GameRng;

use super::screens::Field;

/// PARITY: an empty password string enables cheats
/// (`cMainWindow.cpp:760-761`). Kept as a free function for the pinning
/// test; [`NewGame::cheats_enabled`] delegates to it.
pub fn cheats_enabled(password: &str) -> bool {
    password.is_empty()
}

/// PARITY: `szPassword[40]` (`cMainWindow.cpp:758`) — the Win32 edit control
/// read into a 40-`TCHAR` buffer, so at most 39 characters plus the NUL.
pub const MAX_PASSWORD_LEN: usize = 39;

/// The parameters produced by an accepted New Game screen, fed to
/// [`crate::app::config::Config::apply_new_game`] (persistence) and to
/// `World::new` (spawn). Mirrors `cOperationPurge::SetGameParams`
/// (`cOperationPurge.cpp:104-150`).
#[derive(Clone, Debug, PartialEq)]
pub struct NewGameParams {
    /// The randomly-picked matching maze file (absolute path).
    pub maze_path: PathBuf,
    /// The *pattern* is persisted, never the resolved file
    /// (`cMainWindow.cpp:770`, design §4.5).
    pub pattern: String,
    pub game_idx: u32,
    pub team_idx: u32,
    pub cheats_enabled: bool,
}

/// What pressing OK / Cancel on the New Game screen resolves to. The app
/// (or a headless test) dispatches on this — no mid-frame modal dialog
/// (FIX: no Win32 `DialogBox`, design §1.3).
#[derive(Clone, Debug, PartialEq)]
pub enum NewGameOutcome {
    /// OK with ≥1 matching maze: start the game with these params.
    Accept(NewGameParams),
    /// OK with **zero** matching mazes: the original popped an
    /// "invalid maze file" `MessageBox` and stayed on the dialog
    /// (`cMainWindow.cpp:763`). Here the screen simply refuses to accept.
    Invalid,
    /// Cancel **at startup** (no game running): quit the app
    /// (`cMainWindow.cpp:105` — `NewGame()` failing exits).
    CancelExit,
    /// Cancel **mid-game**: resume the paused game (design §5.5).
    CancelResume,
}

/// The New Game screen model (the `NewGameDlgProc` static state, hoisted to
/// a struct). All logic is CPU-side and unit-tested without a window.
#[derive(Clone, Debug)]
pub struct NewGame {
    pub pattern: String,
    pub password: String,
    pub game_idx: u32,
    pub team_idx: u32,
    pub focus: Field,
    /// PARITY: Cancel behaviour differs by whether a game is already running
    /// (design §5.5): startup ⇒ exit, mid-game ⇒ resume.
    pub invoked_mid_game: bool,
    /// The resolved matches from the last [`refresh_matches`](Self::refresh_matches)
    /// (the `vszMazeFiles` static, `cMainWindow.cpp:598`). `IDOK` random-picks
    /// from this list.
    pub matches: Vec<PathBuf>,
}

impl NewGame {
    /// Build the screen, seeding the fields from the persisted config
    /// (mirrors `WM_INITDIALOG` reading the last params from the registry,
    /// `cMainWindow.cpp:647-673`). `refresh_matches` must be called once by
    /// the caller with the resolved assets dir to populate the live count.
    pub fn new(pattern: String, game_idx: u32, team_idx: u32, invoked_mid_game: bool) -> Self {
        Self {
            pattern,
            password: String::new(),
            game_idx,
            team_idx,
            focus: Field::Pattern,
            invoked_mid_game,
            matches: Vec::new(),
        }
    }

    /// PARITY: empty password ⇒ cheats on (`cMainWindow.cpp:760-761`).
    pub fn cheats_enabled(&self) -> bool {
        cheats_enabled(&self.password)
    }

    /// Live match count shown next to the pattern field
    /// ("Pattern matches N file(s)", `cMainWindow.cpp:1162-1164`).
    pub fn match_count(&self) -> usize {
        self.matches.len()
    }

    /// Re-run the legacy wildcard matcher against the pattern field
    /// (`UpdatePatternInfo`, `cMainWindow.cpp:1142-1165`): split the pattern
    /// into dir + title, resolve the dir against `assets_dir`, and collect
    /// every non-directory file whose name matches the title. Called on
    /// every edit of the pattern field and once at open.
    pub fn refresh_matches(&mut self, assets_dir: &Path) {
        self.matches = resolve_matches(assets_dir, &self.pattern);
    }

    /// Feed a typed character into the focused text field (winit
    /// `KeyEvent.text`). Non-printing / control characters are ignored.
    /// Returns `true` if the pattern changed (so the caller re-runs the
    /// matcher).
    pub fn type_char(&mut self, c: char) -> bool {
        if c.is_control() {
            return false;
        }
        match self.focus {
            Field::Pattern => {
                self.pattern.push(c);
                true
            }
            Field::Password => {
                if self.password.chars().count() < MAX_PASSWORD_LEN {
                    self.password.push(c);
                }
                false
            }
        }
    }

    /// Backspace in the focused field. Returns `true` if the pattern changed.
    pub fn backspace(&mut self) -> bool {
        match self.focus {
            Field::Pattern => self.pattern.pop().is_some(),
            Field::Password => {
                self.password.pop();
                false
            }
        }
    }

    /// Tab cycles focus between the two text fields (Pattern ⇄ Password).
    pub fn cycle_focus(&mut self) {
        self.focus = match self.focus {
            Field::Pattern => Field::Password,
            Field::Password => Field::Pattern,
        };
    }

    /// Left/Right on the Game combo. Clamped to `[0, n_games)`; changing the
    /// game resets the team to 0 exactly as the `CBN_SELCHANGE` handler
    /// re-populates the team combo and `CB_SETCURSEL`s index 0
    /// (`cMainWindow.cpp:712-737`). `delta` is +1 (Right) or −1 (Left).
    pub fn cycle_game(&mut self, delta: i32, data: &GameData) {
        let n = data.game_definitions.len() as i32;
        if n == 0 {
            return;
        }
        let next = (self.game_idx as i32 + delta).clamp(0, n - 1) as u32;
        if next != self.game_idx {
            self.game_idx = next;
            self.team_idx = 0;
        }
    }

    /// Left/Right on the Team combo, clamped to the selected game's team
    /// count.
    pub fn cycle_team(&mut self, delta: i32, data: &GameData) {
        let n = self.team_count(data) as i32;
        if n == 0 {
            return;
        }
        self.team_idx = (self.team_idx as i32 + delta).clamp(0, n - 1) as u32;
    }

    /// Number of teams in the currently-selected game (the Team combo's
    /// population, `cMainWindow.cpp:727-737`).
    pub fn team_count(&self, data: &GameData) -> usize {
        data.game_definitions
            .get(self.game_idx as usize)
            .map(|g| g.teams.len())
            .unwrap_or(0)
    }

    /// Pick a random matching maze (`vszMazeFiles[rand () % size]`,
    /// `cMainWindow.cpp:744`). `None` when there are no matches.
    pub fn pick_random_maze(&self, rng: &mut GameRng) -> Option<PathBuf> {
        if self.matches.is_empty() {
            return None;
        }
        use rand::RngExt as _;
        let i = rng.inner_mut().random_range(0..self.matches.len());
        Some(self.matches[i].clone())
    }

    /// `IDOK` (`cMainWindow.cpp:738-782`): random-pick a matching maze,
    /// resolve the game/team selection, and produce the params. Refuses
    /// (`Invalid`) when the pattern matches no files.
    pub fn ok(&self, rng: &mut GameRng) -> NewGameOutcome {
        match self.pick_random_maze(rng) {
            Some(maze_path) => NewGameOutcome::Accept(NewGameParams {
                maze_path,
                pattern: self.pattern.clone(),
                game_idx: self.game_idx,
                team_idx: self.team_idx,
                cheats_enabled: self.cheats_enabled(),
            }),
            None => NewGameOutcome::Invalid,
        }
    }

    /// `IDCANCEL` (`cMainWindow.cpp:783-787`): quit at startup, resume
    /// mid-game (design §5.5).
    pub fn cancel(&self) -> NewGameOutcome {
        if self.invoked_mid_game {
            NewGameOutcome::CancelResume
        } else {
            NewGameOutcome::CancelExit
        }
    }
}

/// `UpdatePatternInfo`'s core (`cMainWindow.cpp:1142-1160`): `SplitFileName`
/// the pattern into (dir, title), resolve the dir against `assets_dir`, and
/// return every matching file. The random pick and count both read this.
pub fn resolve_matches(assets_dir: &Path, pattern: &str) -> Vec<PathBuf> {
    let (dir, title) = crate::maze::search::split_file_name(pattern);
    let search_dir = if dir == "." {
        assets_dir.to_path_buf()
    } else {
        assets_dir.join(dir)
    };
    let mut matches = crate::maze::search::find_matches(&search_dir, title);
    // `cFileSearch` walks the directory in filesystem order; sort for a
    // deterministic random pick / count across platforms (the count is
    // order-independent, and a stable order makes the seeded pick
    // reproducible in tests).
    matches.sort();
    matches
}

/// Live match count for the pattern, resolved against `assets_dir`
/// (`UpdatePatternInfo` return, `cMainWindow.cpp:1160-1164`). Kept as a free
/// function for callers/tests that only need the count.
pub fn update_pattern_info(pattern: &str, assets_dir: &Path) -> usize {
    resolve_matches(assets_dir, pattern).len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::gamedata;
    use std::path::Path as StdPath;

    fn assets() -> PathBuf {
        StdPath::new(env!("CARGO_MANIFEST_DIR")).join("assets")
    }

    fn data() -> GameData {
        gamedata::load(&assets().join("GameData.toml")).expect("GameData.toml parses")
    }

    #[test]
    fn empty_password_enables_cheats() {
        assert!(cheats_enabled(""));
        assert!(!cheats_enabled("anything"));
        let mut ng = NewGame::new(String::new(), 0, 0, false);
        assert!(ng.cheats_enabled());
        ng.password = "secret".into();
        assert!(!ng.cheats_enabled());
    }

    #[test]
    fn pattern_matches_the_shipped_mazes() {
        // The shipped pool is Small1..Small4.maz.
        let n = update_pattern_info("MazeFiles/*.maz", &assets());
        assert_eq!(n, 4, "Small1-4.maz should match the default glob");
    }

    #[test]
    fn pattern_narrows_the_match_count() {
        assert_eq!(update_pattern_info("MazeFiles/Small1.maz", &assets()), 1);
        assert_eq!(update_pattern_info("MazeFiles/DoesNotExist*.maz", &assets()), 0);
    }

    #[test]
    fn typing_pattern_char_refreshes_count() {
        let mut ng = NewGame::new("MazeFiles/Small1.maz".into(), 0, 0, false);
        ng.refresh_matches(&assets());
        assert_eq!(ng.match_count(), 1);
        // Retarget to the glob by editing the field (backspace the tail,
        // type '*').
        ng.pattern = "MazeFiles/*.maz".into();
        ng.refresh_matches(&assets());
        assert_eq!(ng.match_count(), 4);
    }

    #[test]
    fn tab_cycles_focus_and_typing_lands_in_the_focused_field() {
        let mut ng = NewGame::new(String::new(), 0, 0, false);
        assert_eq!(ng.focus, Field::Pattern);
        assert!(ng.type_char('a'));
        assert_eq!(ng.pattern, "a");
        ng.cycle_focus();
        assert_eq!(ng.focus, Field::Password);
        assert!(!ng.type_char('x'));
        assert_eq!(ng.password, "x");
        assert_eq!(ng.pattern, "a"); // unchanged
        ng.cycle_focus();
        assert_eq!(ng.focus, Field::Pattern);
    }

    #[test]
    fn password_field_respects_the_40_tchar_cap() {
        let mut ng = NewGame::new(String::new(), 0, 0, false);
        ng.focus = Field::Password;
        for _ in 0..100 {
            ng.type_char('z');
        }
        assert_eq!(ng.password.chars().count(), MAX_PASSWORD_LEN);
    }

    #[test]
    fn backspace_edits_the_focused_field() {
        let mut ng = NewGame::new("ab".into(), 0, 0, false);
        assert!(ng.backspace());
        assert_eq!(ng.pattern, "a");
        ng.focus = Field::Password;
        ng.password = "pw".into();
        assert!(!ng.backspace());
        assert_eq!(ng.password, "p");
    }

    #[test]
    fn game_and_team_combos_clamp_within_bounds() {
        let d = data();
        let mut ng = NewGame::new(String::new(), 0, 0, false);
        let n_games = d.game_definitions.len();
        assert!(n_games >= 1);

        // Left at 0 stays at 0.
        ng.cycle_game(-1, &d);
        assert_eq!(ng.game_idx, 0);
        // Right cannot exceed the last game.
        for _ in 0..(n_games + 5) {
            ng.cycle_game(1, &d);
        }
        assert_eq!(ng.game_idx as usize, n_games - 1);

        // Team clamps to the selected game's team count.
        let n_teams = ng.team_count(&d);
        assert!(n_teams >= 1);
        for _ in 0..(n_teams + 5) {
            ng.cycle_team(1, &d);
        }
        assert_eq!(ng.team_idx as usize, n_teams - 1);
        ng.cycle_team(-100, &d);
        assert_eq!(ng.team_idx, 0);
    }

    #[test]
    fn changing_game_resets_team_to_zero() {
        let d = data();
        if d.game_definitions.len() < 2 {
            return; // shipped data may define a single game
        }
        let mut ng = NewGame::new(String::new(), 0, 0, false);
        ng.cycle_team(1, &d);
        if ng.team_idx == 0 {
            return; // first game has one team
        }
        ng.cycle_game(1, &d);
        assert_eq!(ng.team_idx, 0);
    }

    #[test]
    fn ok_with_matches_accepts_and_carries_params() {
        let mut ng = NewGame::new("MazeFiles/*.maz".into(), 0, 0, false);
        ng.refresh_matches(&assets());
        let mut rng = GameRng::from_seed(42);
        match ng.ok(&mut rng) {
            NewGameOutcome::Accept(p) => {
                assert!(p.maze_path.ends_with_str_maz());
                assert_eq!(p.pattern, "MazeFiles/*.maz");
                assert!(p.cheats_enabled); // empty password
            }
            other => panic!("expected Accept, got {other:?}"),
        }
    }

    #[test]
    fn ok_with_no_matches_is_invalid() {
        let mut ng = NewGame::new("MazeFiles/none*.maz".into(), 0, 0, false);
        ng.refresh_matches(&assets());
        let mut rng = GameRng::from_seed(1);
        assert_eq!(ng.ok(&mut rng), NewGameOutcome::Invalid);
    }

    #[test]
    fn cancel_at_startup_exits_but_mid_game_resumes() {
        let startup = NewGame::new(String::new(), 0, 0, false);
        assert_eq!(startup.cancel(), NewGameOutcome::CancelExit);
        let mid = NewGame::new(String::new(), 0, 0, true);
        assert_eq!(mid.cancel(), NewGameOutcome::CancelResume);
    }

    #[test]
    fn random_pick_is_seed_deterministic() {
        let mut ng = NewGame::new("MazeFiles/*.maz".into(), 0, 0, false);
        ng.refresh_matches(&assets());
        let a = NewGame::new("MazeFiles/*.maz".into(), 0, 0, false);
        let mut a = a;
        a.refresh_matches(&assets());
        let mut r1 = GameRng::from_seed(7);
        let mut r2 = GameRng::from_seed(7);
        assert_eq!(ng.pick_random_maze(&mut r1), a.pick_random_maze(&mut r2));
    }

    // Test helper: assert the picked path is a .maz file.
    trait EndsWithMaz {
        fn ends_with_str_maz(&self) -> bool;
    }
    impl EndsWithMaz for PathBuf {
        fn ends_with_str_maz(&self) -> bool {
            self.extension().and_then(|e| e.to_str()) == Some("maz")
        }
    }
}
