//! Asset pipeline sanity checks.
//!
//! Every texture and sound the game uses was recovered from
//! `OperationPurge.msi` and normalized offline to one standard format
//! (textures: PNG RGBA8; sounds: 16-bit signed PCM mono WAV — see the README).
//! The asset filenames are compiled in (`data::assets::names`), not read from
//! `GameData.toml`, so these tests pin the compiled-in registry against what
//! is actually on disk and in the expected format.

use operation_purge::data::assets::{names, AssetDirs};
use operation_purge::render::textures::{decode_image, ProcessMode};
use std::path::Path;

fn assets_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("assets")
}

fn dirs() -> AssetDirs {
    AssetDirs::new(assets_dir())
}

/// Every sound and texture in the registry must exist on disk,
/// or the game logs "could not preload ..." / falls back to a white texture.
#[test]
fn every_registered_asset_exists_on_disk() {
    let sounds = dirs().sounds;
    for name in names::ALL_SOUNDS {
        assert!(sounds.join(name).is_file(), "missing Sounds/{name}");
    }
    let textures = dirs().textures;
    for name in names::ALL_TEXTURES {
        assert!(textures.join(name).is_file(), "missing Textures/{name}");
    }
}

/// The offline conversion must leave every sound in the standard format:
/// 16-bit signed PCM, mono. `decode_wav` relies on this (it reads `i16`
/// directly with no per-format branch).
#[test]
fn every_sound_is_16bit_mono_pcm() {
    let sounds = dirs().sounds;
    for name in names::ALL_SOUNDS {
        let path = sounds.join(name);
        let reader =
            hound::WavReader::open(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let spec = reader.spec();
        assert_eq!(spec.channels, 1, "{name} must be mono");
        assert_eq!(spec.bits_per_sample, 16, "{name} must be 16-bit");
        assert_eq!(
            spec.sample_format,
            hound::SampleFormat::Int,
            "{name} must be integer PCM"
        );
    }
}

/// Every registered texture must decode as a non-empty RGBA8 image (PNG).
#[test]
fn every_texture_decodes_as_rgba8() {
    let textures = dirs().textures;
    for name in names::ALL_TEXTURES {
        let path = textures.join(name);
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let img = decode_image(&bytes, ProcessMode::UseAlpha)
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert!(img.width > 0 && img.height > 0, "{name} decoded empty");
        assert_eq!(
            img.rgba.len(),
            (img.width * img.height * 4) as usize,
            "{name} not tightly-packed RGBA8"
        );
    }
}

/// Every shipped `.maz` (the full original set recovered from the MSI) must
/// parse through the codec without error.
#[test]
fn every_shipped_maze_loads() {
    let dir = assets_dir().join("MazeFiles");
    let mut count = 0;
    for entry in std::fs::read_dir(&dir).expect("MazeFiles/ readable") {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) == Some("maz") {
            operation_purge::maze::Maze::load(&path)
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            count += 1;
        }
    }
    assert_eq!(count, 21, "expected the full shipped maze set");
}

#[test]
fn shipped_maze_files_reload_and_are_bordered() {
    for name in ["Small1.maz", "Small2.maz", "Small3.maz", "Small4.maz"] {
        let path = assets_dir().join("MazeFiles").join(name);
        let maze = operation_purge::maze::Maze::load(&path)
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        for x in 0..maze.cx {
            assert!(maze.surrounding_walls(x, 0)[0], "{name}: top border missing at x={x}");
            assert!(
                maze.surrounding_walls(x, maze.cy - 1)[1],
                "{name}: bottom border missing at x={x}"
            );
        }
        for y in 0..maze.cy {
            assert!(
                maze.surrounding_walls(0, y)[2],
                "{name}: left border missing at y={y}"
            );
            assert!(
                maze.surrounding_walls(maze.cx - 1, y)[3],
                "{name}: right border missing at y={y}"
            );
        }
    }
}

#[test]
fn liberation_sans_bold_font_is_present_and_loadable() {
    let path = assets_dir().join("Fonts/LiberationSans-Bold.ttf");
    let data = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    ab_glyph::FontRef::try_from_slice(&data).expect("must parse as a valid font");
}
