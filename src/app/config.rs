//! User preferences, replacing `cRegistry`'s HKCU values.
//!
//! Stored as TOML at `dirs::config_dir()/operation-purge/config.toml`.
//! Every key is optional; a missing key silently falls back to its
//! default, as a failed registry read fell back to the dialog default
//! (`cMainWindow.cpp:647-673`). The file is written only on New Game
//! accept, never eagerly.

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

/// TOML-backed config.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Relative → tried beside the executable, then the working dir, else
    /// the embedded copy is unpacked to the data dir (`app::embedded`);
    /// overridable by `--assets` / `OP_ASSETS`.
    pub assets_dir: String,

    /// May hold a wildcard pattern; the *pattern* is persisted, never the
    /// resolved file (`cMainWindow.cpp:770`).
    pub maze_file: String,

    pub game_index: u32,
    pub team_index: u32,

    /// Default `true`: the 2011 unthrottled loop is opt-in via
    /// `vsync = false`.
    pub vsync: bool,

    /// Frames per second ceiling while focused; `None` or `0` = uncapped
    /// (vsync, when on, still paces the loop).
    pub fps_cap: Option<u32>,

    /// Multiplies raw mouse deltas; default 1.0. Needed because raw
    /// delta units differ per platform (macOS reports NSEvent counts, not
    /// physical pixels).
    pub mouse_scale: f32,

    /// `None` = use the monitor's DPI scale factor.
    pub hud_scale: Option<f32>,

    /// `None` = seeded from the wall clock at launch and at each new game.
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
    /// `$OP_CONFIG` if set, else `dirs::config_dir()/operation-purge/config.toml`.
    pub fn path() -> Option<PathBuf> {
        if let Some(p) = std::env::var_os("OP_CONFIG") {
            return Some(PathBuf::from(p));
        }
        dirs::config_dir().map(|d| d.join("operation-purge").join("config.toml"))
    }

    /// Missing file or parse failure both fall back to `Config::default()`,
    /// like a failed registry read.
    pub fn load() -> Self {
        Self::path().map(|p| Self::load_from(&p)).unwrap_or_default()
    }

    /// [`load`](Self::load) from an explicit path.
    pub fn load_from(path: &std::path::Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| toml::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Fold an accepted New Game selection into the config (the registry
    /// `SetData` block, `cMainWindow.cpp:767-776`): the maze **pattern** (not
    /// the resolved file), game index, and team index are persisted.
    pub fn apply_new_game(&mut self, params: &crate::game::newgame::NewGameParams) {
        self.maze_file = params.pattern.clone();
        self.game_index = params.game_idx;
        self.team_index = params.team_idx;
    }

    /// Write the config to `path`, creating its directory.
    pub fn save_to(&self, path: &std::path::Path) -> std::io::Result<()> {
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
