//! Asset directory layout (design §4.6), replacing the original's
//! `SetCurrentDirectory(FilesDir)` + CWD-relative opens (libs-map §4).
//!
//! Every path is an explicit `PathBuf` rooted at `assets_dir` — the port
//! never chdirs (design §4.1, §5.1 "Config storage").

use std::path::{Path, PathBuf};

/// Resolved asset subdirectories, joined once at startup.
pub struct AssetDirs {
    pub root: PathBuf,
    pub sounds: PathBuf,
    pub textures: PathBuf,
    pub maze_files: PathBuf,
    pub fonts: PathBuf,
}

impl AssetDirs {
    /// `Textures/`, `Sounds/`, `MazeFiles/`, `Fonts/` under `root`
    /// (`cGameData::GetSoundsPath()` = `"Sounds"`,
    /// `GetTexturesPath()` = `"Textures"`, libs-map §2.1).
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self {
            sounds: root.join("Sounds"),
            textures: root.join("Textures"),
            maze_files: root.join("MazeFiles"),
            fonts: root.join("Fonts"),
            root,
        }
    }

    pub fn game_data_toml(&self) -> PathBuf {
        self.root.join("GameData.toml")
    }

    pub fn texture(&self, filename: impl AsRef<Path>) -> PathBuf {
        self.textures.join(filename)
    }

    pub fn sound(&self, filename: impl AsRef<Path>) -> PathBuf {
        self.sounds.join(filename)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subdirs_join_the_conventional_names() {
        let dirs = AssetDirs::new("/tmp/assets");
        assert_eq!(dirs.sounds, Path::new("/tmp/assets/Sounds"));
        assert_eq!(dirs.textures, Path::new("/tmp/assets/Textures"));
        assert_eq!(dirs.maze_files, Path::new("/tmp/assets/MazeFiles"));
        assert_eq!(dirs.fonts, Path::new("/tmp/assets/Fonts"));
        assert_eq!(
            dirs.game_data_toml(),
            Path::new("/tmp/assets/GameData.toml")
        );
    }
}
