//! Game screens, replacing the original's modal `MessageBox` call sites
//! with in-engine screens.
//!
//! FIX: the original blocked inside `MessageBox` mid-frame; the port
//! replaces every such site with a non-blocking `Confirm` screen (the sim is
//! frozen and sounds are stopped **first**, ordering preserved).

/// Which field has focus in the New Game screen's text entry.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Field {
    Pattern,
    Password,
}

/// The three terminal game states offered by the GameOver modal.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum GameOverKind {
    Won,
    Lost,
    Draw,
}

impl GameOverKind {
    /// PARITY: `cOperationPurge::ExecuteCommands` game-over message text
    /// (`cOperationPurge.cpp:1362-1376`).
    pub fn message(self) -> &'static str {
        match self {
            GameOverKind::Won => "Your team won!",
            GameOverKind::Lost => "Your team lost!",
            GameOverKind::Draw => "All robots were destroyed!",
        }
    }
}

/// The confirmation modals, one per gameplay `MessageBox` site.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ConfirmKind {
    /// F4 (`commandRestart`): "quit the current game?" → restart the same
    /// game (`cOperationPurge.cpp:897-903`).
    Restart,
    /// Esc (`commandEndGame`): "quit the current game?" → New Game
    /// (`cOperationPurge.cpp:889-896`).
    EndGame,
    /// A team won/lost/all-destroyed: replay / new game / keep watching
    /// (`cOperationPurge.cpp:1391-1414`).
    GameOver(GameOverKind),
}

/// A choice made on a `Confirm` modal (Y/N/C keys or click).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ConfirmChoice {
    Yes,
    No,
    Cancel,
}

/// The action a confirmed modal resolves to; the app/game dispatches on it.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ConfirmAction {
    /// Restart the same game (`StartGame`).
    Restart,
    /// Pop the New Game screen (`cMainWindow::NewGame`).
    NewGame,
    /// Dismiss the modal and resume the (frozen) game.
    Resume,
    /// Keep watching after game over (`m_bIgnoreGameEnd = true`).
    KeepWatching,
    /// The choice is not offered by this modal (ignored).
    None,
}

impl ConfirmKind {
    /// The prompt body rendered in the modal (verbatim from the C++ sites).
    pub fn prompt(self) -> String {
        match self {
            ConfirmKind::Restart | ConfirmKind::EndGame => {
                "Are you sure you want to quit the current game?".to_string()
            }
            ConfirmKind::GameOver(kind) => {
                format!("{}\nWould you like to play the last game again?", kind.message())
            }
        }
    }

    /// Whether the modal offers a third "Cancel" choice (only GameOver does,
    /// `MB_YESNOCANCEL`; the rest are `MB_YESNO`).
    pub fn offers_cancel(self) -> bool {
        matches!(self, ConfirmKind::GameOver(_))
    }

    /// Map a Y/N/C choice to its action, per the C++ `switch` at each site.
    pub fn resolve(self, choice: ConfirmChoice) -> ConfirmAction {
        match (self, choice) {
            // Restart (`cOperationPurge.cpp:897-903`): Yes restarts, No dismisses.
            (ConfirmKind::Restart, ConfirmChoice::Yes) => ConfirmAction::Restart,
            (ConfirmKind::Restart, _) => ConfirmAction::Resume,

            // End game (`cOperationPurge.cpp:889-896`): Yes → New Game, No dismisses.
            (ConfirmKind::EndGame, ConfirmChoice::Yes) => ConfirmAction::NewGame,
            (ConfirmKind::EndGame, _) => ConfirmAction::Resume,

            // Game over (`cOperationPurge.cpp:1392-1414`): Yes replays,
            // No → New Game, Cancel keeps watching.
            (ConfirmKind::GameOver(_), ConfirmChoice::Yes) => ConfirmAction::Restart,
            (ConfirmKind::GameOver(_), ConfirmChoice::No) => ConfirmAction::NewGame,
            (ConfirmKind::GameOver(_), ConfirmChoice::Cancel) => ConfirmAction::KeepWatching,
        }
    }
}

/// The running game's screen. The New Game screen is the app's
/// `Option<NewGame>`, not a variant here, because it can sit over a frozen
/// game.
#[derive(Clone, Debug)]
pub enum Screen {
    Map2D { forced_by_death: bool },
    Play3D,
    /// The free observer-point camera (`observerPoint`).
    Observer,
    Confirm {
        kind: ConfirmKind,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restart_modal_yes_restarts_no_resumes() {
        let k = ConfirmKind::Restart;
        assert_eq!(k.resolve(ConfirmChoice::Yes), ConfirmAction::Restart);
        assert_eq!(k.resolve(ConfirmChoice::No), ConfirmAction::Resume);
        assert!(!k.offers_cancel());
    }

    #[test]
    fn end_game_modal_yes_new_game_no_resumes() {
        let k = ConfirmKind::EndGame;
        assert_eq!(k.resolve(ConfirmChoice::Yes), ConfirmAction::NewGame);
        assert_eq!(k.resolve(ConfirmChoice::No), ConfirmAction::Resume);
    }

    #[test]
    fn game_over_modal_offers_replay_new_game_keep_watching() {
        let k = ConfirmKind::GameOver(GameOverKind::Won);
        assert!(k.offers_cancel());
        assert_eq!(k.resolve(ConfirmChoice::Yes), ConfirmAction::Restart);
        assert_eq!(k.resolve(ConfirmChoice::No), ConfirmAction::NewGame);
        assert_eq!(k.resolve(ConfirmChoice::Cancel), ConfirmAction::KeepWatching);
        assert!(k.prompt().contains("Your team won!"));
    }

    #[test]
    fn game_over_kind_messages_match_source() {
        assert_eq!(GameOverKind::Won.message(), "Your team won!");
        assert_eq!(GameOverKind::Lost.message(), "Your team lost!");
        assert_eq!(GameOverKind::Draw.message(), "All robots were destroyed!");
    }
}
