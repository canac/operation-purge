//! Typed error enums.
//!
//! The original had exactly two `catch (cException&)` sites and otherwise
//! funneled failures through `g_pLogFile`. `cException` subclasses carried
//! `$(FileName)` and `$(ErrorCode)` (= `GetLastError`, formatted `0x%X`) via
//! macro substitution (`cFile.cpp:437`). These enums carry the same
//! information through `path` fields and `#[source]` chains.

use std::path::PathBuf;
use thiserror::Error;

/// Errors from `data/` (`GameData.toml` load).
///
/// GUARD: replaces `cGameData::ReadFileXML`'s ignored `TiXmlDocument::LoadFile`
/// return value followed by an unconditional `FirstChild("GameData")->ToElement()`
/// deref — a missing/malformed file crashed the original outright
/// (cGameData.cpp:405-413). Here every failure is a typed, caught `Result`.
#[derive(Debug, Error)]
pub enum DataError {
    #[error("cannot read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("{path}: {message}")]
    Toml { path: PathBuf, message: String },

    #[error("{path}: field `{field}` references index {index}, but only {len} entries exist")]
    RefOutOfRange {
        path: PathBuf,
        field: &'static str,
        index: u32,
        len: usize,
    },

    /// A value the game can't run with (a required ref left `None`, a
    /// negative speed, a projectile list the compiled-in bindings and assets
    /// don't match).
    #[error("{path}: {message}")]
    Invalid { path: PathBuf, message: String },
}

/// Errors from `maze/mod.rs` (`.maz` binary codec).
///
/// GUARD: the C++ performs zero header validation — a signed `cx`/`cy` pair
/// is fed straight into an allocation size (`cMaze.cpp:179-229`). The port
/// rejects `cx < 1 || cy < 1 || cx > 4096 || cy > 4096` and any file whose
/// length doesn't match the derived wall-bit-array size.
#[derive(Debug, Error)]
pub enum MazeError {
    #[error("cannot read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("{path}: hostile header cx={cx} cy={cy}")]
    HostileHeader { path: PathBuf, cx: i32, cy: i32 },

    #[error("{path}: expected {expected} bytes for cx={cx} cy={cy}, found {found}")]
    WrongLength {
        path: PathBuf,
        cx: i32,
        cy: i32,
        expected: u64,
        found: u64,
    },
}

/// Errors from `render/gl.rs` context/shader/mesh setup.
#[derive(Debug, Error)]
pub enum GlError {
    #[error("failed to create GL context: {0}")]
    Context(String),

    #[error("shader compile failed ({stage}): {log}")]
    ShaderCompile { stage: &'static str, log: String },

    #[error("program link failed: {0}")]
    ProgramLink(String),

    /// GUARD: building meshes keep 16-bit indices; a mesh that would
    /// overflow `u16` is a clean load error instead of the silent
    /// corruption the original's fixed-size index buffers would produce.
    #[error("mesh exceeds u16 index range: {count} vertices")]
    TooManyVertices { count: usize },
}

/// Errors from `audio/` clip loading / kira backend setup.
#[derive(Debug, Error)]
pub enum AudioError {
    #[error("cannot read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("{path}: {message}")]
    Decode { path: PathBuf, message: String },

    #[error("audio backend error: {0}")]
    Backend(String),
}
