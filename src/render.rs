//! OpenGL 3.3 core rendering. [`scene::SceneRenderer`] is the entry point
//! for the 3D scene and 2D map; [`text`] draws the HUD text over it.

pub mod building3d;
pub mod center3d;
pub mod gl;
pub mod mesh;
pub mod overlay;
pub mod robot3d;
pub mod scene;
pub mod shaders;
pub mod sprites;
pub mod text;
pub mod textures;
