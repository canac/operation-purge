//! Typed error enums (design §2, §4.1, §4.2).
//!
//! The original had exactly two `catch (cException&)` sites and otherwise
//! funneled failures through `g_pLogFile` (libs-map §1.6, §5.3). `cException`
//! subclasses carried `$(FileName)` and `$(ErrorCode)` (= `GetLastError`,
//! formatted `0x%X`) via macro substitution (`cFile.cpp:437`). These enums
//! reproduce that information through `path` fields and `#[source]` chains
//! instead of string substitution.

use std::path::PathBuf;
use thiserror::Error;

/// Errors from `data/` (`GameData.toml` load).
///
/// GUARD: replaces `cGameData::ReadFileXML`'s ignored `TiXmlDocument::LoadFile`
/// return value followed by an unconditional `FirstChild("GameData")->ToElement()`
/// deref — a missing/malformed file crashed the original outright
/// (libs-map §2.1, cGameData.cpp:405-413). Here every failure is a typed,
/// caught `Result`.
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

    #[error("{path}: {message}")]
    Shape { path: PathBuf, message: String },
}

/// Errors from `maze/mod.rs` (`.maz` binary codec).
///
/// GUARD: the C++ performs zero header validation — a signed `cx`/`cy` pair
/// is fed straight into an allocation size (libs-map §3 "Validation
/// gotchas", `cMaze.cpp:179-229`). The port rejects `cx < 1 || cy < 1 ||
/// cx > 4096 || cy > 4096` and any file whose length doesn't match the
/// derived wall-bit-array size (design §1.3 GUARD ledger).
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

    /// GUARD: building meshes keep 16-bit indices (design §1.3); a mesh
    /// that would overflow `u16` is a clean load error instead of the
    /// silent corruption the original's fixed-size index buffers would
    /// produce.
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

/// Top-level error carried out of subsystem entry points.
///
/// `anyhow` is used only for plumbing inside `main.rs` (design §1.2); every
/// subsystem below `main` returns one of the typed enums above, aggregated
/// here so callers that span subsystems (e.g. `game/screens.rs` reacting to
/// a failed data reload) can match on a single type.
#[derive(Debug, Error)]
pub enum GameError {
    #[error(transparent)]
    Data(#[from] DataError),

    #[error(transparent)]
    Maze(#[from] MazeError),

    #[error(transparent)]
    Gl(#[from] GlError),

    #[error(transparent)]
    Audio(#[from] AudioError),

    #[error("{0}")]
    Other(String),
}
