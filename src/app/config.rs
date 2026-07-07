//! User preferences (design §4.5), replacing `cRegistry`'s HKCU values.
//!
//! Stored as TOML at `dirs::config_dir()/operation-purge/config.toml`.
//! Every key is optional; a missing key silently falls back to its
//! default — this reproduces the original's "registry read failure ⇒
//! dialog default" semantics (libs-map §4, "Prefs are read at dialog
//! open... missing values fail silently"). The file is written only on
//! New Game accept (design §5.5), never eagerly.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

fn default_assets_dir() -> String {
    "./assets".to_string()
}

fn default_maze_file() -> String {
    "MazeFiles/*.maz".to_string()
}

fn default_vsync() -> bool {
    true
}

fn default_mouse_scale() -> f32 {
    1.0
}

/// TOML-backed config; see design §4.5 for the annotated schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Relative → resolved against the executable's directory; overridable
    /// by `--assets` / `OP_ASSETS`.
    pub assets_dir: String,

    /// May hold a glob pattern (libs-map §4, "Wildcard maze selection");
    /// the *pattern* is persisted, never the resolved file.
    pub maze_file: String,

    pub game_index: u32,
    pub team_index: u32,

    /// Default `true` (design §1.4.5 / §1.5.7): the 2011 unthrottled loop
    /// is opt-in via `vsync = false`, not the default.
    pub vsync: bool,

    /// `None` = uncapped; only meaningful with `vsync = false`.
    pub fps_cap: Option<u32>,

    /// Multiplies raw mouse deltas; default 1.0. Needed because raw
    /// delta units differ per platform (design §1.4.13: macOS deltas are
    /// NSEvent counts, not physical pixels).
    pub mouse_scale: f32,

    /// `None` = use the monitor's DPI scale factor.
    pub hud_scale: Option<f32>,

    /// `None` = time-seeded (see `rng::GameRng::from_time`).
    pub seed: Option<u64>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            assets_dir: default_assets_dir(),
            maze_file: default_maze_file(),
            game_index: 0,
            team_index: 0,
            vsync: default_vsync(),
            fps_cap: None,
            mouse_scale: default_mouse_scale(),
            hud_scale: None,
            seed: None,
        }
    }
}

impl Config {
    /// `dirs::config_dir()/operation-purge/config.toml`.
    pub fn path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("operation-purge").join("config.toml"))
    }

    /// Missing file or parse failure both fall back to `Config::default()`
    /// (registry-semantics parity — libs-map §4).
    pub fn load() -> Self {
        let Some(path) = Self::path() else {
            return Self::default();
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        toml::from_str(&text).unwrap_or_default()
    }

    /// Fold an accepted New Game selection into the config (the registry
    /// `SetData` block, `cMainWindow.cpp:767-776`): the maze **pattern** (not
    /// the resolved file), game index, and team index are persisted.
    pub fn apply_new_game(&mut self, params: &crate::game::newgame::NewGameParams) {
        self.maze_file = params.pattern.clone();
        self.game_index = params.game_idx;
        self.team_index = params.team_idx;
    }

    /// Written only on New Game accept (design §5.5), never eagerly.
    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = Self::path() else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(self).unwrap_or_default();
        std::fs::write(path, text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_matches_documented_schema() {
        let c = Config::default();
        assert_eq!(c.assets_dir, "./assets");
        assert_eq!(c.maze_file, "MazeFiles/*.maz");
        assert_eq!(c.game_index, 0);
        assert_eq!(c.team_index, 0);
        assert!(c.vsync);
        assert_eq!(c.fps_cap, None);
        assert_eq!(c.mouse_scale, 1.0);
        assert_eq!(c.hud_scale, None);
        assert_eq!(c.seed, None);
    }

    #[test]
    fn missing_keys_fall_back_to_defaults() {
        let c: Config = toml::from_str("vsync = false\n").unwrap();
        assert!(!c.vsync);
        assert_eq!(c.assets_dir, "./assets"); // untouched key ⇒ default
    }

    #[test]
    fn round_trips_through_toml() {
        let c = Config {
            seed: Some(12345),
            hud_scale: Some(2.0),
            ..Config::default()
        };
        let text = toml::to_string_pretty(&c).unwrap();
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(back.seed, Some(12345));
        assert_eq!(back.hud_scale, Some(2.0));
    }

    #[test]
    fn apply_new_game_persists_pattern_and_indices() {
        use crate::game::newgame::NewGameParams;
        let mut c = Config::default();
        let params = NewGameParams {
            maze_path: std::path::PathBuf::from("/tmp/MazeFiles/Small3.maz"),
            pattern: "MazeFiles/Small*.maz".to_string(),
            game_idx: 2,
            team_idx: 1,
            cheats_enabled: false,
        };
        c.apply_new_game(&params);
        assert_eq!(c.maze_file, "MazeFiles/Small*.maz"); // pattern, not resolved file
        assert_eq!(c.game_index, 2);
        assert_eq!(c.team_index, 1);

        // Round-trip: write → read yields the same New Game selection.
        let text = toml::to_string_pretty(&c).unwrap();
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(back.maze_file, "MazeFiles/Small*.maz");
        assert_eq!(back.game_index, 2);
        assert_eq!(back.team_index, 1);
    }
}
