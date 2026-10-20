//! Asset directory layout (design §4.6), replacing the original's
//! `SetCurrentDirectory(FilesDir)` + CWD-relative opens (libs-map §4).
//!
//! Every path is an explicit `PathBuf` rooted at `assets_dir` — the port
//! never chdirs (design §4.1, §5.1 "Config storage").
//!
//! [`names`] is the **compiled-in asset registry**: the game ships a fixed
//! set of textures and sounds (recovered from `OperationPurge.msi`), so their
//! filenames are baked in here rather than read from `GameData.toml` (user
//! decision — same rationale as [`super::builtin`]). Everything is stored in
//! one standard format: textures are PNG RGBA8, sounds are 16-bit signed PCM
//! mono WAV (converted offline from the originals; see the README).

use std::path::{Path, PathBuf};

/// The game's fixed asset filenames (design §4.6). Baked in — the running
/// game never reads these names from data. The originals assigned them in
/// the shipped `GameData.xml`; this port hardcodes that assignment.
pub mod names {
    // --- Textures (PNG RGBA8, under `Textures/`) ---
    /// Building wall (`Brick`), floor (`Steel`), ceiling (`Ceiling`).
    pub const WALL_TEXTURE: &str = "Brick.png";
    pub const FLOOR_TEXTURE: &str = "Steel.png";
    pub const CEILING_TEXTURE: &str = "Ceiling.png";
    /// Bullet/Better-Bullet projectile sprite (Rocket had an empty texture).
    pub const PROJECTILE_TEXTURE: &str = "Projectile.png";
    /// Wall-impact scorch mark (all projectiles).
    pub const HOLE_TEXTURE: &str = "Hole.png";
    /// Particle sprite (every particle type used this one texture).
    pub const SMOKE_TEXTURE: &str = "Smoke.png";

    // --- Sounds (16-bit PCM mono WAV, under `Sounds/`) ---
    /// The two alarm loops (`SoundsOptions`).
    pub const ENEMY_VISIBLE_SOUND: &str = "FastAlarm.wav";
    pub const TARGETED_SOUND: &str = "Alarm.wav";
    /// Per-weapon fire/collision + robot destroy (per-type in the original;
    /// the assignment is fixed here: Bullet/Better-Bullet use the Bullet
    /// pair, Rocket the Rocket pair, every robot uses `Destroy`).
    pub const BULLET_FIRE_SOUND: &str = "BulletFire.wav";
    pub const BULLET_COLLISION_SOUND: &str = "BulletCollision.wav";
    pub const ROCKET_FIRE_SOUND: &str = "RocketFire.wav";
    pub const ROCKET_COLLISION_SOUND: &str = "RocketCollision.wav";
    pub const DESTROY_SOUND: &str = "Destroy.wav";

    /// The projectile sprite for a projectile type index (Bullet, Rocket,
    /// Better Bullet); `None` = not drawn (`cProjectile::Render`).
    pub fn projectile_texture(type_index: usize) -> Option<&'static str> {
        (type_index != 1).then_some(PROJECTILE_TEXTURE)
    }

    /// `(fire, collision)` sounds for a projectile type index.
    pub fn projectile_sounds(type_index: usize) -> (&'static str, &'static str) {
        if type_index == 1 {
            (ROCKET_FIRE_SOUND, ROCKET_COLLISION_SOUND)
        } else {
            (BULLET_FIRE_SOUND, BULLET_COLLISION_SOUND)
        }
    }

    /// Every sound the game plays — the app preloads exactly this set once
    /// at startup (alarms first, so their `ClipId`s are 0 and 1).
    pub const ALL_SOUNDS: &[&str] = &[
        ENEMY_VISIBLE_SOUND,
        TARGETED_SOUND,
        BULLET_FIRE_SOUND,
        BULLET_COLLISION_SOUND,
        ROCKET_FIRE_SOUND,
        ROCKET_COLLISION_SOUND,
        DESTROY_SOUND,
    ];

    /// Every texture the game uploads — used by the on-disk asset guard test.
    pub const ALL_TEXTURES: &[&str] = &[
        WALL_TEXTURE,
        FLOOR_TEXTURE,
        CEILING_TEXTURE,
        PROJECTILE_TEXTURE,
        HOLE_TEXTURE,
        SMOKE_TEXTURE,
    ];
}

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
