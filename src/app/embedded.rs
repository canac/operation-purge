//! The shipped `assets/` tree, compiled into the binary so a lone
//! executable runs with no installer. The maze picker scans real files on
//! disk (`maze::search`), so the tree is unpacked rather than read from
//! memory.

use include_dir::{include_dir, Dir, DirEntry};
use std::path::{Path, PathBuf};

static ASSETS: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/assets");

/// Records which asset content last unpacked into a directory.
const STAMP: &str = ".version";

/// Where unpacked assets live when none sit beside the exe or in the
/// working dir: `dirs::data_dir()/operation-purge/assets`.
pub fn default_unpack_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("operation-purge").join("assets"))
}

/// Unpacks the embedded assets into `dest`. Missing files are always
/// written; existing files are overwritten only when `dest` was unpacked
/// from different asset content (the `build.rs` hash of `assets/`), so
/// user-added mazes survive every run and user edits survive until the
/// shipped assets change.
pub fn ensure_unpacked(dest: &Path) -> std::io::Result<()> {
    unpack(&ASSETS, dest, env!("ASSETS_HASH"))
}

fn unpack(src: &Dir, dest: &Path, version: &str) -> std::io::Result<()> {
    let stamp = dest.join(STAMP);
    let stale = std::fs::read_to_string(&stamp).ok().as_deref() != Some(version);
    write_tree(src, dest, stale)?;
    std::fs::write(stamp, version)
}

fn write_tree(dir: &Dir, dest: &Path, overwrite: bool) -> std::io::Result<()> {
    for entry in dir.entries() {
        match entry {
            DirEntry::Dir(d) => write_tree(d, dest, overwrite)?,
            DirEntry::File(f) => {
                let path = dest.join(f.path());
                if overwrite || !path.exists() {
                    if let Some(parent) = path.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::write(&path, f.contents())?;
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dest(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("op-unpack-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn unpacks_every_shipped_file() {
        let dest = temp_dest("all");
        unpack(&ASSETS, &dest, "1").unwrap();
        assert!(dest.join("GameData.toml").is_file());
        assert!(dest.join("MazeFiles").join("Small1.maz").is_file());
        assert_eq!(std::fs::read_to_string(dest.join(STAMP)).unwrap(), "1");
        std::fs::remove_dir_all(dest).unwrap();
    }

    #[test]
    fn same_version_keeps_user_edits_and_extra_files() {
        let dest = temp_dest("keep");
        unpack(&ASSETS, &dest, "1").unwrap();
        std::fs::write(dest.join("GameData.toml"), "edited").unwrap();
        std::fs::write(dest.join("MazeFiles").join("Mine.maz"), "x").unwrap();
        unpack(&ASSETS, &dest, "1").unwrap();
        assert_eq!(std::fs::read_to_string(dest.join("GameData.toml")).unwrap(), "edited");
        assert!(dest.join("MazeFiles").join("Mine.maz").is_file());
        std::fs::remove_dir_all(dest).unwrap();
    }

    #[test]
    fn new_version_refreshes_shipped_files_only() {
        let dest = temp_dest("bump");
        unpack(&ASSETS, &dest, "1").unwrap();
        std::fs::write(dest.join("GameData.toml"), "edited").unwrap();
        std::fs::write(dest.join("MazeFiles").join("Mine.maz"), "x").unwrap();
        unpack(&ASSETS, &dest, "2").unwrap();
        assert_ne!(std::fs::read_to_string(dest.join("GameData.toml")).unwrap(), "edited");
        assert!(dest.join("MazeFiles").join("Mine.maz").is_file());
        assert_eq!(std::fs::read_to_string(dest.join(STAMP)).unwrap(), "2");
        std::fs::remove_dir_all(dest).unwrap();
    }
}
