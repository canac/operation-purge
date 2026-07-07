//! The four GLSL 330 programs (design §3.4), one per FVF vertex layout,
//! embedded via `include_str!`. Bodies (and the actual `.glsl` sources)
//! land at M4.

/// Vertex layout byte sizes, matching the original's four FVFs
/// (design §3.4 table).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ProgramKind {
    /// pos vec3 (12 B) — 2D map, crosshair, health bars.
    Flat,
    /// pos+normal (24 B) — robot dome/barrel/treads, center markers.
    Lit,
    /// pos+normal+uv (32 B) — building walls/floor/ceiling.
    LitTextured,
    /// pos + rgba8 + uv (24 B) — particles, projectiles, holes, HUD text.
    Sprite,
}

/// A compiled + linked GL program (opaque handle once M4 wires up `glow`).
pub struct Program {
    pub kind: ProgramKind,
}

/// Compiles and links all four programs. Body lands at M4.
pub fn compile_all() -> Result<Vec<Program>, crate::error::GlError> {
    todo!("ported at M4 — 4 GLSL 330 programs (design §3.4)")
}
