//! New Game screen logic: pattern field + live match count, game/team
//! combos, password/cheat polarity (design §5.5). Full body lands at M11.
//!
//! PARITY: empty password field **enables** cheats — any non-empty text
//! disables them (`cMainWindow.cpp:760-761`, design §1.3).

/// PARITY: an empty password string enables cheats.
pub fn cheats_enabled(password: &str) -> bool {
    password.is_empty()
}

/// Re-runs the legacy wildcard matcher (`maze::search`) against the
/// pattern field on every edit, for the live match count shown in the
/// screen (`UpdatePatternInfo` twin, design §5.5). Body lands at M11.
pub fn update_pattern_info(_pattern: &str, _maze_dir: &std::path::Path) -> usize {
    todo!("ported at M11 — live match count via maze::search (design §5.5)")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_password_enables_cheats() {
        assert!(cheats_enabled(""));
        assert!(!cheats_enabled("anything"));
    }
}
