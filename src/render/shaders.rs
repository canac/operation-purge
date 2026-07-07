//! The four GLSL 330 core programs (design §3.4), one per FVF vertex
//! layout. Sources are inline `const` strings (the design allows
//! `include_str!` from `.glsl` files *or* inline consts); they are compiled
//! at runtime only. All CPU-side setup math (material/light uniform
//! packing) is pure and unit-tested; the GLSL text itself is pinned by
//! tests that assert its load-bearing PARITY constants are present.

use std::rc::Rc;

use glow::HasContext;

use crate::data::builtin::LIGHT_DATA;
use crate::error::GlError;

/// One shader program per original FVF layout (design §3.4 table).
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

/// D3D9 global ambient `0x202020` (`cBuilding3D.cpp:60`
/// `D3DRS_AMBIENT = 0x00202020`): each channel `0x20/255`.
pub const GLOBAL_AMBIENT: f32 = 0x20 as f32 / 255.0;

// ---------------------------------------------------------------------------
// GLSL sources
// ---------------------------------------------------------------------------

/// prog_flat VS/FS: `gl_Position = u_mvp * pos`; solid `u_color`
/// (mathematically identical to the original LIGHTING-on + AMBIENT
/// 0x00FFFFFF flat-color trick, design §3.4).
pub const FLAT_VS: &str = r#"#version 330 core
layout(location = 0) in vec3 a_pos;
uniform mat4 u_mvp;
void main() { gl_Position = u_mvp * vec4(a_pos, 1.0); }
"#;

pub const FLAT_FS: &str = r#"#version 330 core
uniform vec4 u_color;
out vec4 frag;
void main() { frag = u_color; }
"#;

/// The shared FFP-Gouraud vertex-lighting body (design §3.4 prog_lit).
/// Emitted into both the lit and lit-textured vertex shaders so the
/// equation stays byte-identical between them.
///
/// PARITY notes baked into the GLSL:
/// - lighting is computed in view space; the one white point light sits at
///   the eye (view-space origin), so `L = normalize(-P_view)`;
/// - `N_view = mat3(world_view) * N` with **NO inverse-transpose** — D3D FFP
///   uses the matrix directly;
/// - `N` is **NEVER normalized** (dome normals of length 0.08-0.5
///   intentionally dim the lighting — the 2011 look);
/// - `att = 1/(a0 + a1*d + a2*d^2)`, coefficients from the `LIGHT_DATA` const;
/// - `global_ambient = vec3(0x20/255)`; `mat_ambient = mat_diffuse = u_material`.
const LIT_GOURAUD: &str = r#"
// PARITY: FFP Gouraud lighting, one white point light AT THE EYE.
vec3 gouraud(vec3 pos, vec3 normal, vec3 material, vec3 atten) {
    vec4 p_view = u_world_view * vec4(pos, 1.0);
    // PARITY: N_view = mat3(world_view) * N, NO inverse-transpose, N NOT normalized.
    vec3 n_view = mat3(u_world_view) * normal;
    vec3 L = normalize(-p_view.xyz);
    float d = length(p_view.xyz);
    float att = 1.0 / (atten.x + atten.y * d + atten.z * d * d);
    float ndl = max(dot(n_view, L), 0.0);
    vec3 ambient = material * vec3(0.12549020); // global ambient 0x202020
    vec3 diffuse = material * ndl * att;         // white light diffuse = 1
    return clamp(ambient + diffuse, 0.0, 1.0);
}
"#;

fn lit_vs() -> String {
    format!(
        r#"#version 330 core
layout(location = 0) in vec3 a_pos;
layout(location = 1) in vec3 a_normal;
uniform mat4 u_mvp;
uniform mat4 u_world_view;
uniform vec3 u_material;
uniform vec3 u_light_atten;
out vec4 v_color;
{body}
void main() {{
    v_color = vec4(gouraud(a_pos, a_normal, u_material, u_light_atten), 1.0);
    gl_Position = u_mvp * vec4(a_pos, 1.0);
}}
"#,
        body = LIT_GOURAUD
    )
}

pub const LIT_FS: &str = r#"#version 330 core
in vec4 v_color;
out vec4 frag;
void main() { frag = v_color; }
"#;

fn lit_tex_vs() -> String {
    format!(
        r#"#version 330 core
layout(location = 0) in vec3 a_pos;
layout(location = 1) in vec3 a_normal;
layout(location = 2) in vec2 a_uv;
uniform mat4 u_mvp;
uniform mat4 u_world_view;
uniform vec3 u_material;
uniform vec3 u_light_atten;
out vec4 v_color;
out vec2 v_uv;
{body}
void main() {{
    v_color = vec4(gouraud(a_pos, a_normal, u_material, u_light_atten), 1.0);
    v_uv = a_uv;
    gl_Position = u_mvp * vec4(a_pos, 1.0);
}}
"#,
        body = LIT_GOURAUD
    )
}

/// stage-0 MODULATE: `texture(uv) * gouraud_color` (design §3.4).
pub const LIT_TEX_FS: &str = r#"#version 330 core
in vec4 v_color;
in vec2 v_uv;
uniform sampler2D u_tex;
out vec4 frag;
void main() { frag = texture(u_tex, v_uv) * v_color; }
"#;

/// prog_sprite VS: vertices arrive ALREADY view-space (cSprite3D billboard
/// pipeline), so only the projection is applied (design §3.4).
pub const SPRITE_VS: &str = r#"#version 330 core
layout(location = 0) in vec3 a_pos;
layout(location = 1) in vec4 a_color;
layout(location = 2) in vec2 a_uv;
uniform mat4 u_proj;
out vec4 v_color;
out vec2 v_uv;
void main() {
    v_color = a_color;
    v_uv = a_uv;
    gl_Position = u_proj * vec4(a_pos, 1.0);
}
"#;

pub const SPRITE_FS: &str = r#"#version 330 core
in vec4 v_color;
in vec2 v_uv;
uniform sampler2D u_tex;
out vec4 frag;
void main() { frag = texture(u_tex, v_uv) * v_color; }
"#;

// ---------------------------------------------------------------------------
// CPU-side uniform packing (pure, unit-tested)
// ---------------------------------------------------------------------------

/// The `u_light_atten` uniform value from the compiled-in `LIGHT_DATA`
/// const (design §4.3): `[a0, a1, a2]`.
pub fn light_atten_uniform() -> [f32; 3] {
    [LIGHT_DATA.a0, LIGHT_DATA.a1, LIGHT_DATA.a2]
}

/// The `u_material` uniform value — the part/team color's RGB. The D3D
/// material set both ambient and diffuse to this color (design §3.4).
pub fn material_uniform(color: crate::math::Rgba) -> [f32; 3] {
    [color.r, color.g, color.b]
}

// ---------------------------------------------------------------------------
// Runtime GL compilation
// ---------------------------------------------------------------------------

/// A compiled + linked GL program plus the shared context handle. Uniform
/// locations are looked up lazily by name at draw time by the callers.
pub struct Program {
    pub kind: ProgramKind,
    pub program: glow::Program,
    gl: Rc<glow::Context>,
}

impl Program {
    pub fn uniform(&self, name: &str) -> Option<glow::UniformLocation> {
        unsafe { self.gl.get_uniform_location(self.program, name) }
    }
}

/// The four compiled programs (design §3.4). Runtime-only (needs a current
/// GL context); never constructed by the headless test suite.
pub struct Programs {
    pub flat: Program,
    pub lit: Program,
    pub lit_textured: Program,
    pub sprite: Program,
}

/// Compiles and links all four programs (design §3.4). Runtime-only.
pub fn compile_all(gl: Rc<glow::Context>) -> Result<Programs, GlError> {
    let flat = link(&gl, ProgramKind::Flat, FLAT_VS, FLAT_FS)?;
    let lit = link(&gl, ProgramKind::Lit, &lit_vs(), LIT_FS)?;
    let lit_textured = link(&gl, ProgramKind::LitTextured, &lit_tex_vs(), LIT_TEX_FS)?;
    let sprite = link(&gl, ProgramKind::Sprite, SPRITE_VS, SPRITE_FS)?;
    Ok(Programs {
        flat,
        lit,
        lit_textured,
        sprite,
    })
}

fn link(
    gl: &Rc<glow::Context>,
    kind: ProgramKind,
    vs_src: &str,
    fs_src: &str,
) -> Result<Program, GlError> {
    unsafe {
        let compile =
            |stage: u32, src: &str, name: &'static str| -> Result<glow::Shader, GlError> {
                let shader = gl
                    .create_shader(stage)
                    .map_err(|e| GlError::ShaderCompile { stage: name, log: e })?;
                gl.shader_source(shader, src);
                gl.compile_shader(shader);
                if !gl.get_shader_compile_status(shader) {
                    return Err(GlError::ShaderCompile {
                        stage: name,
                        log: gl.get_shader_info_log(shader),
                    });
                }
                Ok(shader)
            };

        let vs = compile(glow::VERTEX_SHADER, vs_src, "vertex")?;
        let fs = compile(glow::FRAGMENT_SHADER, fs_src, "fragment")?;
        let program = gl.create_program().map_err(GlError::ProgramLink)?;
        gl.attach_shader(program, vs);
        gl.attach_shader(program, fs);
        gl.link_program(program);
        if !gl.get_program_link_status(program) {
            return Err(GlError::ProgramLink(gl.get_program_info_log(program)));
        }
        gl.delete_shader(vs);
        gl.delete_shader(fs);
        Ok(Program {
            kind,
            program,
            gl: gl.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::Rgba;

    #[test]
    fn global_ambient_is_0x20_over_255() {
        assert!((GLOBAL_AMBIENT - 0.1254902).abs() < 1e-6);
        // The GLSL literal must match the const.
        assert!(LIT_GOURAUD.contains("0.12549020"));
    }

    #[test]
    fn light_atten_uniform_comes_from_the_builtin_const() {
        // Verified originals from the recovered LightData.txt.
        assert_eq!(light_atten_uniform(), [0.5, 0.25, 0.0]);
    }

    #[test]
    fn material_uniform_is_rgb_only() {
        let c = Rgba {
            r: 0.25,
            g: 0.5,
            b: 0.75,
            a: 1.0,
        };
        assert_eq!(material_uniform(c), [0.25, 0.5, 0.75]);
    }

    #[test]
    fn lit_shader_encodes_parity_lighting_rules() {
        // Light at eye: L = normalize(-P_view).
        assert!(LIT_GOURAUD.contains("normalize(-p_view.xyz)"));
        // NO inverse-transpose: N_view uses mat3(world_view) directly.
        assert!(LIT_GOURAUD.contains("mat3(u_world_view) * normal"));
        // N is NEVER normalized — no normalize() applied to n_view.
        assert!(!LIT_GOURAUD.contains("normalize(n_view"));
        // Attenuation 1/(a0 + a1 d + a2 d^2).
        assert!(LIT_GOURAUD.contains("atten.x + atten.y * d + atten.z * d * d"));
    }

    #[test]
    fn sprite_vs_only_applies_projection() {
        // Verts already view-space => VS multiplies by u_proj only.
        assert!(SPRITE_VS.contains("u_proj * vec4(a_pos, 1.0)"));
        assert!(!SPRITE_VS.contains("u_world_view"));
    }

    #[test]
    fn lit_tex_fs_modulates_texture_by_gouraud_color() {
        assert!(LIT_TEX_FS.contains("texture(u_tex, v_uv) * v_color"));
    }

    #[test]
    fn all_sources_are_glsl_330_core() {
        for src in [
            FLAT_VS,
            FLAT_FS,
            &lit_vs(),
            LIT_FS,
            &lit_tex_vs(),
            LIT_TEX_FS,
            SPRITE_VS,
            SPRITE_FS,
        ] {
            assert!(src.starts_with("#version 330 core"));
        }
    }
}
