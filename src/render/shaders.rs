//! The four GLSL 330 core programs, one per original FVF vertex layout.
//! Sources are inline `const` strings compiled at runtime only. All CPU-side
//! uniform packing is pure and unit-tested; the GLSL text itself is pinned by
//! tests that assert its load-bearing PARITY constants are present.

use std::rc::Rc;

use glow::HasContext;

use crate::data::builtin::LIGHT_DATA;
use crate::error::GlError;
use crate::math::d3dx::Mat4;
use crate::math::Rgba;

// ---------------------------------------------------------------------------
// GLSL sources
// ---------------------------------------------------------------------------

/// prog_flat VS/FS: `gl_Position = u_mvp * pos`; solid `u_color`
/// (mathematically identical to the original LIGHTING-on + AMBIENT
/// 0x00FFFFFF flat-color trick).
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

/// The shared FFP-Gouraud vertex-lighting body.
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
/// - `global_ambient = vec3(0x20/255)` (`D3DRS_AMBIENT = 0x00202020`,
///   `cBuilding3D.cpp:59`); `mat_ambient = mat_diffuse = u_material`;
/// - the light's ambient is white too (`cOperationPurge.cpp:3095`), and D3D
///   attenuates per-light ambient like diffuse.
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
    vec3 ambient = material * (vec3(0.12549020) + att); // global 0x202020 + light ambient
    vec3 diffuse = material * ndl * att;                 // white light diffuse = 1
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

/// stage-0 MODULATE: `texture(uv) * gouraud_color`.
pub const LIT_TEX_FS: &str = r#"#version 330 core
in vec4 v_color;
in vec2 v_uv;
uniform sampler2D u_tex;
out vec4 frag;
void main() { frag = texture(u_tex, v_uv) * v_color; }
"#;

/// prog_sprite VS: vertices arrive ALREADY view-space (cSprite3D billboard
/// pipeline), so only the projection is applied.
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
/// const: `[a0, a1, a2]`.
pub fn light_atten_uniform() -> [f32; 3] {
    [LIGHT_DATA.a0, LIGHT_DATA.a1, LIGHT_DATA.a2]
}

/// The `u_material` uniform value — the part/team color's RGB. The D3D
/// material set both ambient and diffuse to this color.
pub fn material_uniform(color: Rgba) -> [f32; 3] {
    [color.r, color.g, color.b]
}

// ---------------------------------------------------------------------------
// Runtime GL compilation
// ---------------------------------------------------------------------------

/// Uniform locations resolved once at link time; `None` for a uniform the
/// program does not declare (or the driver optimized out).
#[derive(Default)]
pub struct Uniforms {
    pub mvp: Option<glow::UniformLocation>,
    pub world_view: Option<glow::UniformLocation>,
    pub material: Option<glow::UniformLocation>,
    pub light_atten: Option<glow::UniformLocation>,
    pub color: Option<glow::UniformLocation>,
    pub proj: Option<glow::UniformLocation>,
    pub tex: Option<glow::UniformLocation>,
    pub screen: Option<glow::UniformLocation>,
}

/// A linked GL program with its uniform locations. Deletes the program on
/// drop. Runtime-only (needs a current GL context).
pub struct Program {
    gl: Rc<glow::Context>,
    program: glow::Program,
    pub u: Uniforms,
}

impl Program {
    /// Compiles and links `vs_src` + `fs_src`, releasing every GL object it
    /// created if any step fails.
    pub fn link(gl: &Rc<glow::Context>, vs_src: &str, fs_src: &str) -> Result<Program, GlError> {
        unsafe {
            let vs = compile(gl, glow::VERTEX_SHADER, vs_src, "vertex")?;
            let fs = match compile(gl, glow::FRAGMENT_SHADER, fs_src, "fragment") {
                Ok(fs) => fs,
                Err(e) => {
                    gl.delete_shader(vs);
                    return Err(e);
                }
            };
            let linked = gl.create_program().map_err(GlError::ProgramLink).and_then(|program| {
                gl.attach_shader(program, vs);
                gl.attach_shader(program, fs);
                gl.link_program(program);
                if gl.get_program_link_status(program) {
                    gl.detach_shader(program, vs);
                    gl.detach_shader(program, fs);
                    Ok(program)
                } else {
                    let log = gl.get_program_info_log(program);
                    gl.delete_program(program);
                    Err(GlError::ProgramLink(log))
                }
            });
            gl.delete_shader(vs);
            gl.delete_shader(fs);
            let program = linked?;
            let loc = |name: &str| gl.get_uniform_location(program, name);
            let u = Uniforms {
                mvp: loc("u_mvp"),
                world_view: loc("u_world_view"),
                material: loc("u_material"),
                light_atten: loc("u_light_atten"),
                color: loc("u_color"),
                proj: loc("u_proj"),
                tex: loc("u_tex"),
                screen: loc("u_screen"),
            };
            Ok(Program {
                gl: gl.clone(),
                program,
                u,
            })
        }
    }

    /// Makes this the current program and points `u_tex` (if declared) at
    /// texture unit 0, which is also made active.
    pub fn bind(&self) {
        unsafe {
            self.gl.use_program(Some(self.program));
            if let Some(loc) = &self.u.tex {
                self.gl.uniform_1_i32(Some(loc), 0);
                self.gl.active_texture(glow::TEXTURE0);
            }
        }
    }

    pub fn set_mat4(&self, loc: &Option<glow::UniformLocation>, m: &Mat4) {
        if let Some(loc) = loc {
            unsafe {
                self.gl
                    .uniform_matrix_4_f32_slice(Some(loc), false, &m.to_row_major_array());
            }
        }
    }

    pub fn set_vec2(&self, loc: &Option<glow::UniformLocation>, x: f32, y: f32) {
        if let Some(loc) = loc {
            unsafe { self.gl.uniform_2_f32(Some(loc), x, y) };
        }
    }

    pub fn set_vec3(&self, loc: &Option<glow::UniformLocation>, v: [f32; 3]) {
        if let Some(loc) = loc {
            unsafe { self.gl.uniform_3_f32(Some(loc), v[0], v[1], v[2]) };
        }
    }

    pub fn set_color(&self, loc: &Option<glow::UniformLocation>, c: Rgba) {
        if let Some(loc) = loc {
            unsafe { self.gl.uniform_4_f32(Some(loc), c.r, c.g, c.b, c.a) };
        }
    }

    /// Binds a lit program for a pass: [`Program::bind`] plus the constant
    /// `u_light_atten`.
    pub fn bind_lit(&self) {
        self.bind();
        self.set_vec3(&self.u.light_atten, light_atten_uniform());
    }

    /// The per-draw lit uniforms: `u_mvp`, `u_world_view`, `u_material`.
    pub fn set_lit(&self, mvp: &Mat4, world_view: &Mat4, material: Rgba) {
        self.set_mat4(&self.u.mvp, mvp);
        self.set_mat4(&self.u.world_view, world_view);
        self.set_vec3(&self.u.material, material_uniform(material));
    }
}

impl Drop for Program {
    fn drop(&mut self) {
        unsafe { self.gl.delete_program(self.program) };
    }
}

unsafe fn compile(
    gl: &glow::Context,
    stage: u32,
    src: &str,
    name: &'static str,
) -> Result<glow::Shader, GlError> {
    let shader = gl
        .create_shader(stage)
        .map_err(|e| GlError::ShaderCompile { stage: name, log: e })?;
    gl.shader_source(shader, src);
    gl.compile_shader(shader);
    if !gl.get_shader_compile_status(shader) {
        let log = gl.get_shader_info_log(shader);
        gl.delete_shader(shader);
        return Err(GlError::ShaderCompile { stage: name, log });
    }
    Ok(shader)
}

/// The four game programs. Runtime-only; never constructed by the headless
/// test suite.
pub struct Programs {
    pub flat: Program,
    pub lit: Program,
    pub lit_textured: Program,
    pub sprite: Program,
}

impl Programs {
    pub fn compile(gl: &Rc<glow::Context>) -> Result<Programs, GlError> {
        Ok(Programs {
            flat: Program::link(gl, FLAT_VS, FLAT_FS)?,
            lit: Program::link(gl, &lit_vs(), LIT_FS)?,
            lit_textured: Program::link(gl, &lit_tex_vs(), LIT_TEX_FS)?,
            sprite: Program::link(gl, SPRITE_VS, SPRITE_FS)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_ambient_literal_is_0x20_over_255() {
        assert!((0x20 as f32 / 255.0 - 0.1254902).abs() < 1e-6);
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
        // The light's white ambient is attenuated and added to the global ambient.
        assert!(LIT_GOURAUD.contains("vec3(0.12549020) + att"));
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
