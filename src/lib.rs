//! Operation Purge — Rust + OpenGL port.
//!
//! Crate root. See the design document (implementation contract) §2
//! "Module Layout" for the module tree this file declares.
//!
//! Milestone M0 establishes the module skeleton and the shared foundational
//! types (`ids`, `error`, `math::{equals, Rgba}`, `rng`, `app::config`) that
//! every later milestone depends on. Everything else is a type stub with
//! `todo!()`/`unimplemented!()` bodies until its owning milestone lands.

pub mod app;
pub mod errorlog;
pub mod error;
pub mod profiler;
pub mod rng;
pub mod ids;
pub mod math;
pub mod data;
pub mod maze;
pub mod sim;
pub mod path;
pub mod render;
pub mod input;
pub mod audio;
pub mod game;
