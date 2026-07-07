//! App-state machine (design §5.5), replacing the original's modal
//! dialogs (`NewGameDlgProc`, `MessageBox` call sites) with in-engine
//! screens rendered through `render::overlay` + `render::text` — one
//! render path, no second UI stack (design §3.6). Full transitions land
//! at M11.

/// Which field has focus in the New Game screen's text entry.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Field {
    Pattern,
    Password,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum GameOverKind {
    Won,
    Lost,
    Draw,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ConfirmKind {
    Restart,
    EndGame,
    GameOver(GameOverKind),
    InternalError,
}

/// The app's top-level screen/mode (design §5.5). PARITY: empty New Game
/// password **enables** cheats (`cMainWindow.cpp:760-761`); 2D-map arrow
/// pan is inverted (down = Up); Cancel at startup exits, Cancel mid-game
/// resumes.
pub enum Screen {
    NewGame {
        pattern: String,
        match_count: usize,
        game_idx: u32,
        team_idx: u32,
        password: String,
        focus: Field,
        invoked_mid_game: bool,
    },
    Map2D {
        forced_by_death: bool,
    },
    Play3D,
    Observer,
    Confirm {
        kind: ConfirmKind,
    },
}

impl Screen {
    pub fn new_game_at_startup() -> Self {
        Screen::NewGame {
            pattern: String::new(),
            match_count: 0,
            game_idx: 0,
            team_idx: 0,
            password: String::new(),
            focus: Field::Pattern,
            invoked_mid_game: false,
        }
    }
}
