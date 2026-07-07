//! Window + GL context creation (design §3.1).
//!
//! Borderless fullscreen on the primary monitor, physical-resolution
//! framebuffer, DPI scale tracking. All winit/glutin/glow calls here run
//! **only at real runtime** — [`RenderWindow::create`] takes an
//! `ActiveEventLoop`, which unit tests never construct, so the headless test
//! suite compiles this module without ever invoking it (task constraint).
//!
//! Config selection excludes sRGB-capable-only configs (§3.1: the D3D9 FFP
//! was linear pass-through; an sRGB default framebuffer would retint
//! everything), requests 24-bit depth, no MSAA. Context is OpenGL 3.3 core
//! (macOS negotiates 4.1 core — fine, GLSL stays `#version 330`).

use std::ffi::CString;
use std::num::NonZeroU32;
use std::rc::Rc;

use glutin::config::{ConfigTemplateBuilder, GlConfig};
use glutin::context::{
    ContextApi, ContextAttributesBuilder, PossiblyCurrentContext, Version,
};
use glutin::display::GetGlDisplay;
use glutin::prelude::*;
use glutin::surface::{Surface, SurfaceAttributesBuilder, SwapInterval, WindowSurface};
use glutin_winit::{DisplayBuilder, GlWindow};
use raw_window_handle::HasWindowHandle;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Fullscreen, Window, WindowAttributes};

use crate::error::GlError;
use crate::render::gl::GlState;

/// Owns the winit window, the glutin GL surface+context, and the renderer's
/// [`GlState`] (which wraps the shared `glow::Context`).
pub struct RenderWindow {
    pub window: Window,
    surface: Surface<WindowSurface>,
    context: PossiblyCurrentContext,
    pub gl: GlState,
    /// Physical framebuffer size in pixels (Retina-aware; distinct from
    /// logical size — design §1.4.13, the top HUD bug source on macOS).
    pub physical_size: (u32, u32),
    /// DPI scale factor reported by the platform; HUD/text metrics are
    /// logical px × this factor.
    pub scale_factor: f64,
}

impl RenderWindow {
    /// Creates the borderless-fullscreen window + OpenGL 3.3 core context on
    /// the primary monitor. Runtime-only (needs a live event loop).
    pub fn create(event_loop: &ActiveEventLoop, vsync: bool) -> Result<Self, GlError> {
        let window_attributes = WindowAttributes::default()
            .with_title("Operation Purge")
            .with_fullscreen(Some(Fullscreen::Borderless(None)));

        // No sRGB request; 24-bit depth; no MSAA.
        let template = ConfigTemplateBuilder::new()
            .with_alpha_size(8)
            .with_depth_size(24)
            .with_stencil_size(0);

        let display_builder =
            DisplayBuilder::new().with_window_attributes(Some(window_attributes));

        let (window, gl_config) = display_builder
            .build(event_loop, template, |configs| {
                // §3.1: prefer a config that is NOT sRGB-capable so the default
                // framebuffer stays linear; fall back to whatever exists.
                configs
                    .reduce(|best, cfg| {
                        let prefer_cfg = !cfg.srgb_capable() && best.srgb_capable();
                        if prefer_cfg {
                            cfg
                        } else {
                            best
                        }
                    })
                    .expect("at least one GL config")
            })
            .map_err(|e| GlError::Context(format!("config/window build failed: {e}")))?;

        let window = window.ok_or_else(|| GlError::Context("no window created".into()))?;

        let raw_window_handle = window
            .window_handle()
            .map_err(|e| GlError::Context(format!("no window handle: {e}")))?
            .as_raw();

        let gl_display = gl_config.display();
        let context_attributes = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::OpenGl(Some(Version::new(3, 3))))
            .build(Some(raw_window_handle));

        let not_current = unsafe {
            gl_display
                .create_context(&gl_config, &context_attributes)
                .map_err(|e| GlError::Context(format!("create_context failed: {e}")))?
        };

        let size = window.inner_size();
        let scale_factor = window.scale_factor();
        let (width, height) = (size.width.max(1), size.height.max(1));

        let surface_attributes = window
            .build_surface_attributes(SurfaceAttributesBuilder::<WindowSurface>::new())
            .map_err(|e| GlError::Context(format!("surface attributes failed: {e}")))?;
        let surface = unsafe {
            gl_display
                .create_window_surface(&gl_config, &surface_attributes)
                .map_err(|e| GlError::Context(format!("create_window_surface failed: {e}")))?
        };

        let context = not_current
            .make_current(&surface)
            .map_err(|e| GlError::Context(format!("make_current failed: {e}")))?;

        let glow_ctx = unsafe {
            glow::Context::from_loader_function_cstr(|s| gl_display.get_proc_address(s))
        };
        let gl_ctx = Rc::new(glow_ctx);

        // Swap interval: 1 by default (vsync); 0 restores the unthrottled loop.
        let interval = if vsync {
            SwapInterval::Wait(NonZeroU32::new(1).unwrap())
        } else {
            SwapInterval::DontWait
        };
        let _ = surface.set_swap_interval(&context, interval);

        let mut gl = GlState::new(gl_ctx);
        gl.apply_global_state();
        gl.set_viewport(width as i32, height as i32);

        Ok(Self {
            window,
            surface,
            context,
            gl,
            physical_size: (width, height),
            scale_factor,
        })
    }

    /// Shared `glow::Context` for building textures/atlas/shaders.
    pub fn context(&self) -> Rc<glow::Context> {
        Rc::clone(self.gl.context())
    }

    /// Handle a physical-size change (resize / DPI): resize the surface and
    /// viewport. Uses physical pixels (design §3.1).
    pub fn resize(&mut self, width: u32, height: u32) {
        let (w, h) = (width.max(1), height.max(1));
        self.physical_size = (w, h);
        self.surface.resize(
            &self.context,
            NonZeroU32::new(w).unwrap(),
            NonZeroU32::new(h).unwrap(),
        );
        self.gl.set_viewport(w as i32, h as i32);
    }

    pub fn set_scale_factor(&mut self, scale_factor: f64) {
        self.scale_factor = scale_factor;
    }

    /// Swaps buffers (render-then-swap; the original's present-previous-frame
    /// latency is dropped — design §1.3 FIX ledger).
    pub fn swap_buffers(&self) {
        let _ = self.surface.swap_buffers(&self.context);
    }
}

/// Load a GL proc address by name (rarely needed outside context creation;
/// kept for completeness / debug tooling).
pub fn proc_address_name(name: &str) -> CString {
    CString::new(name).expect("proc name has no interior NUL")
}
