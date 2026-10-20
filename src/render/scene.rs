//! The whole-frame scene renderer: `Render3D` + `Render2D`
//! (`cOperationPurge.cpp:2690-3030`) over a pure per-frame snapshot.
//!
//! [`frame_view`] extracts everything a frame draws from a [`Game`] into a
//! [`FrameView`] (pure, unit-testable); [`SceneRenderer`] owns every GL
//! resource and draws that snapshot in the original's pass order.

use std::path::Path;
use std::rc::Rc;

use glam::{Vec2, Vec3};

use crate::data::assets::names;
use crate::data::builtin::OBJECTS;
use crate::data::BuildingOptions;
use crate::error::GlError;
use crate::game::{Game, ObserverType};
use crate::math::camera::Camera;
use crate::math::d3dx::Mat4;
use crate::math::Rgba;
use crate::maze::building::Building;
use crate::render::building3d::{BuildingRenderer, BuildingTextures, TextureSizes};
use crate::render::center3d::{CenterRenderData, CenterRenderer};
use crate::render::gl::{CullMode, GlState};
use crate::render::overlay::{
    health_bar_segments, MapCenterData, MapColors, MapFrame, MapMode, MapRobotData, MapView,
    Overlay2DRenderer, HEALTH_BAR_HEIGHT,
};
use crate::render::robot3d::{RobotRenderData, RobotRenderer};
use crate::render::shaders::Programs;
use crate::render::sprites::{Axis, SpriteBatch, SpriteData, SpriteRenderer};
use crate::render::textures::{ProcessMode, Texture};

/// The camera's matrices, computed once per frame and shared by every pass.
#[derive(Copy, Clone, Debug)]
pub struct FrameMatrices {
    pub view: Mat4,
    pub proj: Mat4,
    /// `view * proj` (row-vector order): the building's MVP (world = identity).
    pub view_proj: Mat4,
}

impl FrameMatrices {
    pub fn new(camera: &Camera, aspect: f32) -> Self {
        let view = camera.view();
        let proj = camera.projection(aspect);
        Self {
            view,
            proj,
            view_proj: view.mul(&proj),
        }
    }

    /// `(mvp, world_view)` for geometry placed by `world`: `world * view *
    /// proj` and `world * view` (the lit shaders' view-space lighting input).
    pub fn lit(&self, world: &Mat4) -> (Mat4, Mat4) {
        let world_view = world.mul(&self.view);
        (world_view.mul(&self.proj), world_view)
    }
}

/// Which sprite texture a sprite samples; resolved to GL by [`SceneRenderer`].
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SpriteTex {
    Hole,
    Projectile,
    Smoke,
    /// Untextured (health bars).
    White,
}

/// One frame's `Render3D` inputs.
#[derive(Clone, Debug)]
pub struct Scene3D {
    pub camera: Camera,
    pub robots: Vec<RobotRenderData>,
    pub centers: Vec<CenterRenderData>,
    pub holes: Vec<SpriteData<SpriteTex>>,
    pub health_bars: Vec<SpriteData<SpriteTex>>,
    /// Projectiles then particles, depth-sorted at draw time.
    pub effects: Vec<SpriteData<SpriteTex>>,
}

/// Everything one frame draws before the text overlays.
#[derive(Clone, Debug, Default)]
pub struct FrameView {
    /// `None` on the full-screen 2D map.
    pub scene: Option<Scene3D>,
    /// The full-screen map or the minimap, if shown.
    pub map: Option<MapFrame>,
    /// The 3D crosshair color; `None` on the 2D map.
    pub crosshair: Option<Rgba>,
}

/// Snapshots `game` into the frame's draw inputs (`Render3D` / `Render2D`).
pub fn frame_view(game: &Game) -> FrameView {
    let colors = &game.world.game_data.colors;
    let in_2d = game.shows_2d();
    let mode = if in_2d {
        Some(MapMode::Full2D)
    } else if game.show_map {
        Some(MapMode::Minimap)
    } else {
        None
    };
    FrameView {
        scene: (!in_2d).then(|| scene_3d(game)),
        map: mode.map(|mode| MapFrame {
            mode,
            view: MapView {
                offset: game.map_visible_offset(),
                size: game.map_size,
            },
            centers: map_centers(game),
            robots: map_robots(game),
            colors: MapColors {
                background: colors.background_2d,
                wall: colors.wall_2d,
                barrel: colors.barrel_2d,
            },
        }),
        crosshair: (!in_2d).then_some(colors.crosshair_2d),
    }
}

fn team_color(game: &Game, team: crate::ids::TeamId) -> Rgba {
    let def = &game.world.game_data.game_definitions[game.world.game_def.get() as usize];
    def.teams
        .get(team.get() as usize)
        .map(|t| t.color)
        .unwrap_or(Rgba::new(0.3, 0.5, 1.0, 1.0))
}

fn scene_3d(game: &Game) -> Scene3D {
    let world = &game.world;
    let data = &world.game_data;
    // PARITY: dead robots and the active robot (the eye sits inside it) are
    // not drawn (`cOperationPurge.cpp:2919-2924`, `:2958-2964`).
    let shown = || {
        world
            .robots
            .iter()
            .filter(|(id, r)| !r.is_dead() && game.active_robot != Some(**id))
            .map(|(_, r)| r)
    };

    let robots = shown()
        .map(|r| RobotRenderData {
            position: Vec3::new(r.position.x, r.position.y, 0.0),
            direction: r.direction,
            barrel_elevation: r.barrel_elevation,
            body_color: team_color(game, r.team),
            barrel_color: data.colors.barrel_3d,
            tread_color: data.colors.tread_3d,
        })
        .collect();

    let centers = world
        .centers
        .iter()
        .map(|c| CenterRenderData {
            position_x: c.position.x,
            position_y: c.position.y,
            orientation: c.orientation_angle,
            size: c.size,
            color: c.color,
        })
        .collect();

    // Wall-oriented, unsorted (`cOperationPurge.cpp:2886-2905`).
    let holes = world
        .holes
        .entries
        .iter()
        .map(|h| SpriteData {
            center: h.position,
            offset: Vec3::ZERO,
            size: Vec2::splat(h.size),
            rotation: h.rotation,
            axis: axis_from_normal(h.normal),
            color: Rgba::new(1.0, 1.0, 1.0, 1.0),
            texture: SpriteTex::Hole,
        })
        .collect();

    // `cOperationPurge.cpp:2951-3000`.
    let mut health_bars = Vec::new();
    for r in shown() {
        let fraction = if r.health_start > 0.0 { r.health / r.health_start } else { 0.0 };
        let z = crate::sim::robot::ROBOT_SIZE_Z + HEALTH_BAR_HEIGHT * 0.5;
        for seg in health_bar_segments(fraction) {
            health_bars.push(SpriteData {
                center: Vec3::new(r.position.x, r.position.y, z),
                offset: Vec3::new(seg.offset_x, 0.0, 0.0),
                size: Vec2::new(seg.width, HEALTH_BAR_HEIGHT),
                rotation: 0.0,
                axis: Axis::PositiveZ,
                color: seg.color,
                texture: SpriteTex::White,
            });
        }
    }

    // Projectiles (black, `cProjectile.cpp:196-214`) then particles.
    let mut effects = Vec::new();
    for p in &world.projectiles {
        let i = p.projectile_type.get() as usize;
        if names::projectile_texture(i).is_none() {
            continue;
        }
        effects.push(SpriteData {
            center: p.position,
            offset: Vec3::ZERO,
            size: Vec2::splat(data.projectile_types[i].size),
            rotation: 0.0,
            axis: Axis::PositiveZ,
            color: Rgba::new(0.0, 0.0, 0.0, 1.0),
            texture: SpriteTex::Projectile,
        });
    }
    for p in world.particles.slots.iter().flatten() {
        effects.push(SpriteData {
            center: p.position,
            offset: Vec3::ZERO,
            size: Vec2::splat(p.size(data)),
            rotation: p.rotation(),
            axis: Axis::PositiveZ,
            color: p.color(data),
            texture: SpriteTex::Smoke,
        });
    }

    Scene3D {
        camera: game.camera(),
        robots,
        centers,
        holes,
        health_bars,
        effects,
    }
}

/// Per-robot 2D map markers (`Render2D`, `cOperationPurge.cpp:2781-2834`):
/// the active robot is tinted `observer_2d`, others use their team color;
/// an observer point is drawn as one more observer-tinted marker.
fn map_robots(game: &Game) -> Vec<MapRobotData> {
    let observer = game.world.game_data.colors.observer_2d;
    let point = game.observer_type == ObserverType::Point;
    let mut out: Vec<MapRobotData> = game
        .world
        .robots
        .iter()
        .filter(|(_, r)| !r.is_dead())
        .map(|(id, r)| MapRobotData {
            position: r.position,
            direction: r.direction,
            marker_color: if !point && game.active_robot == Some(*id) {
                observer
            } else {
                team_color(game, r.team)
            },
        })
        .collect();
    if point {
        out.push(MapRobotData {
            position: Vec2::new(game.observer_point.x, game.observer_point.y),
            direction: game.observer_theta,
            marker_color: observer,
        });
    }
    out
}

fn map_centers(game: &Game) -> Vec<MapCenterData> {
    game.world
        .centers
        .iter()
        .map(|c| MapCenterData {
            position: c.position,
            size: c.size,
            color: c.color,
        })
        .collect()
}

/// A hole's outward wall normal as the sprite orientation axis.
fn axis_from_normal(n: Vec3) -> Axis {
    if n.x > 0.5 {
        Axis::PositiveX
    } else if n.x < -0.5 {
        Axis::NegativeX
    } else if n.y > 0.5 {
        Axis::PositiveY
    } else if n.y < -0.5 {
        Axis::NegativeY
    } else if n.z > 0.5 {
        Axis::PositiveZ
    } else {
        Axis::NegativeZ
    }
}

struct SpriteTextures {
    hole: Texture,
    projectile: Texture,
    smoke: Texture,
    white: Texture,
}

impl SpriteTextures {
    fn get(&self, t: SpriteTex) -> &Texture {
        match t {
            SpriteTex::Hole => &self.hole,
            SpriteTex::Projectile => &self.projectile,
            SpriteTex::Smoke => &self.smoke,
            SpriteTex::White => &self.white,
        }
    }
}

/// The per-building renderers, replaced as a unit by
/// [`SceneRenderer::load_building`].
struct BuildingPass {
    building: BuildingRenderer,
    overlay: Overlay2DRenderer,
}

/// Owns every scene GL resource: the programs, textures, and
/// game-independent renderers (built once), plus the per-building renderers
/// (rebuilt per maze). Runtime-only (needs a current GL context).
pub struct SceneRenderer {
    gl: Rc<glow::Context>,
    programs: Programs,
    robot: RobotRenderer,
    center: CenterRenderer,
    sprites: SpriteRenderer,
    sprite_textures: SpriteTextures,
    building_textures: BuildingTextures,
    per_building: Option<BuildingPass>,
    batch: SpriteBatch<SpriteTex>,
}

impl SceneRenderer {
    /// Compiles the programs, uploads the textures under `textures_dir`
    /// (missing images fall back to white), and builds the
    /// game-independent renderers.
    pub fn new(gl: Rc<glow::Context>, textures_dir: &Path) -> Result<Self, GlError> {
        let load = |name: &str, mode| Texture::load_or_white(&gl, &textures_dir.join(name), mode);
        Ok(Self {
            programs: Programs::compile(&gl)?,
            robot: RobotRenderer::new(gl.clone(), &OBJECTS)?,
            center: CenterRenderer::new(gl.clone())?,
            sprites: SpriteRenderer::new(gl.clone())?,
            sprite_textures: SpriteTextures {
                hole: load(names::HOLE_TEXTURE, ProcessMode::UseAlpha)?,
                projectile: load(names::PROJECTILE_TEXTURE, ProcessMode::UseAlpha)?,
                smoke: load(names::SMOKE_TEXTURE, ProcessMode::UseAlpha)?,
                white: Texture::white(&gl)?,
            },
            building_textures: BuildingTextures {
                wall: load(names::WALL_TEXTURE, ProcessMode::NoAlpha)?,
                floor: load(names::FLOOR_TEXTURE, ProcessMode::NoAlpha)?,
                ceiling: load(names::CEILING_TEXTURE, ProcessMode::NoAlpha)?,
            },
            per_building: None,
            batch: SpriteBatch::new(),
            gl,
        })
    }

    /// Builds the 3D building mesh and the 2D map for a new maze. The
    /// previous maze's renderers are released first, so on error nothing
    /// building-dependent draws until the next successful load.
    pub fn load_building(&mut self, building: &Building, options: &BuildingOptions) -> Result<(), GlError> {
        self.per_building = None;
        let sizes = TextureSizes {
            wall: options.wall_texture_size,
            floor: options.floor_texture_size,
            ceiling: options.ceiling_texture_size,
        };
        self.per_building = Some(BuildingPass {
            building: BuildingRenderer::new(self.gl.clone(), building, sizes)?,
            overlay: Overlay2DRenderer::new(self.gl.clone(), building, &OBJECTS)?,
        });
        Ok(())
    }

    /// Draws `frame`, leaving culling off and depth writes on for the text
    /// overlays that follow.
    pub fn render(&mut self, gl: &mut GlState, frame: &FrameView, aspect: f32) {
        let Some(pass) = self.per_building.as_ref() else {
            return;
        };
        if let Some(scene) = frame.scene.as_ref() {
            crate::profile_scope!("Render3D");
            let m = FrameMatrices::new(&scene.camera, aspect);
            let lit = &self.programs.lit;
            let textures = &self.sprite_textures;
            let bind = |t: SpriteTex| textures.get(t).bind();

            // PARITY: `Render3D` order (`cOperationPurge.cpp:2866-3030`); every
            // 3D pass culls D3DCULL_CCW (`cBuilding3D.cpp:80`, `cCenter3D.cpp:41`,
            // `:2885`, `cRobot3D.cpp:86`).
            gl.set_cull(CullMode::Ccw);
            pass.building.render(&self.programs.lit_textured, &m, &self.building_textures);

            // Centers and holes skip depth writes to avoid z-fighting (`:2872`).
            gl.set_depth_write(false);
            lit.bind_lit();
            for c in &scene.centers {
                self.center.render(lit, &m, c);
            }
            self.batch.start(m.view, false, false);
            scene.holes.iter().for_each(|s| self.batch.add(s));
            self.sprites.render(&self.programs.sprite, &m.proj, &self.batch, bind);
            gl.set_depth_write(true);

            lit.bind_lit();
            for r in &scene.robots {
                self.robot.render(lit, &m, r);
            }

            self.batch.start(m.view, true, false);
            scene.health_bars.iter().for_each(|s| self.batch.add(s));
            self.sprites.render(&self.programs.sprite, &m.proj, &self.batch, bind);

            self.batch.start(m.view, true, true);
            scene.effects.iter().for_each(|s| self.batch.add(s));
            self.sprites.render(&self.programs.sprite, &m.proj, &self.batch, bind);
        }

        // `cOverlay2D.cpp:96`: the 2D overlay never culls.
        gl.set_cull(CullMode::None);
        if let Some(map) = frame.map.as_ref() {
            pass.overlay.render_map(&self.programs.flat, map);
        }
        if let Some(color) = frame.crosshair {
            pass.overlay.render_crosshair(&self.programs.flat, color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lit_matrices_compose_world_then_view_then_projection() {
        let mut cam = Camera::new(Vec3::new(1.0, 2.0, 0.5), 100.0);
        cam.yaw = 0.3;
        let m = FrameMatrices::new(&cam, 1.5);
        let world = Mat4::translation(Vec3::new(5.0, 6.0, 0.0));
        let (mvp, world_view) = m.lit(&world);
        assert_eq!(world_view, world.mul(&cam.view()));
        assert_eq!(mvp, world.mul(&cam.view()).mul(&cam.projection(1.5)));
        assert_eq!(m.view_proj, cam.view().mul(&cam.projection(1.5)));
    }

    fn game() -> Game {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
        let data = crate::data::gamedata::load(&root.join("GameData.toml")).unwrap();
        let maze = crate::maze::Maze::load(&root.join("MazeFiles/Small1.maz")).unwrap();
        let world = crate::sim::world::World::new(
            data,
            maze,
            crate::ids::GameDefId(0),
            crate::ids::TeamId(0),
            987654,
        );
        Game::new(world)
    }

    #[test]
    fn the_2d_map_frame_has_no_3d_scene_or_crosshair() {
        let g = game();
        assert!(g.shows_2d());
        let f = frame_view(&g);
        assert!(f.scene.is_none());
        assert!(f.crosshair.is_none());
        let map = f.map.expect("full-screen map");
        assert_eq!(map.mode, MapMode::Full2D);
        let alive = g.world.robots.values().filter(|r| !r.is_dead()).count();
        assert_eq!(map.robots.len(), alive);
        assert_eq!(map.centers.len(), g.world.centers.len());
    }

    #[test]
    fn the_3d_frame_skips_the_active_robot_and_bars_every_other() {
        let mut g = game();
        let id = *g.world.robots.keys().next().unwrap();
        g.active_robot = Some(id);
        g.screen = crate::game::screens::Screen::Play3D;
        let f = frame_view(&g);
        let scene = f.scene.expect("3D scene");
        let others = g.world.robots.iter().filter(|(i, r)| !r.is_dead() && **i != id).count();
        assert_eq!(scene.robots.len(), others);
        // Two segments (green + red) per health bar.
        assert_eq!(scene.health_bars.len(), others * 2);
        assert!(f.crosshair.is_some());
    }

    #[test]
    fn hole_axis_follows_the_dominant_wall_normal() {
        assert_eq!(axis_from_normal(Vec3::X), Axis::PositiveX);
        assert_eq!(axis_from_normal(-Vec3::X), Axis::NegativeX);
        assert_eq!(axis_from_normal(Vec3::Y), Axis::PositiveY);
        assert_eq!(axis_from_normal(-Vec3::Y), Axis::NegativeY);
        assert_eq!(axis_from_normal(Vec3::Z), Axis::PositiveZ);
        assert_eq!(axis_from_normal(-Vec3::Z), Axis::NegativeZ);
    }
}
