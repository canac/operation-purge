//! Rendering (design §2, §3). Kept as type stubs through M0 — the task
//! instructions explicitly defer exercising `glow`/`glutin` APIs to avoid
//! fighting their API surface before the winding/culling gate (M4) is
//! reached (design §7 M0/M4).

pub mod building3d;
pub mod camera;
pub mod center3d;
pub mod d3dx;
pub mod gl;
pub mod mesh;
pub mod overlay;
pub mod robot3d;
pub mod shaders;
pub mod sprites;
pub mod text;
pub mod textures;
