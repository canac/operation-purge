//! Headless AI-vs-AI benchmark: times the AI pass separately from the sim
//! and runs until one team is left (or the frame cap).
//!
//! `cargo run --release --example ai_bench -- [maze] [game_def] [seed] [frames]`

use operation_purge::data::gamedata;
use operation_purge::ids::{GameDefId, TeamId};
use operation_purge::maze::Maze;
use operation_purge::sim::ai::run_ai;
use operation_purge::sim::world::World;
use std::path::Path;
use std::time::{Duration, Instant};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let maze_name = args.first().map_or("Big1", String::as_str);
    let game_def: u32 = args.get(1).map_or(1, |s| s.parse().expect("game_def"));
    let seed: u64 = args.get(2).map_or(4242, |s| s.parse().expect("seed"));
    let frames: u32 = args.get(3).map_or(30_000, |s| s.parse().expect("frames"));

    let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
    let data = gamedata::load(&assets.join("GameData.toml")).expect("GameData.toml");
    let maze = Maze::load(&assets.join(format!("MazeFiles/{maze_name}.maz"))).expect("maze");
    let mut world = World::new(data, maze, GameDefId(game_def), TeamId(0), seed);

    let dt = 1.0 / 60.0;
    let (mut ai_time, mut sim_time, mut worst) = (Duration::ZERO, Duration::ZERO, Duration::ZERO);
    let (mut ran, mut stopped, mut robot_frames) = (0u32, 0u32, 0u32);
    while ran < frames {
        let mut teams: Vec<_> = world.robots.values().filter(|r| !r.is_dead()).map(|r| r.team).collect();
        teams.sort();
        teams.dedup();
        if teams.len() < 2 {
            break;
        }
        let t = Instant::now();
        run_ai(&mut world, dt);
        let a = t.elapsed();
        world.frame(dt);
        sim_time += t.elapsed() - a;
        ai_time += a;
        worst = worst.max(a);
        ran += 1;
        for r in world.robots.values().filter(|r| !r.is_dead()) {
            stopped += u32::from(r.speed.abs() < 0.1);
            robot_frames += 1;
        }
    }

    let per = |d: Duration| d.as_secs_f64() * 1000.0 / f64::from(ran.max(1));
    println!(
        "{maze_name} def={game_def} seed={seed} frames={ran} alive={} stopped={:.0}%",
        world.robots.values().filter(|r| !r.is_dead()).count(),
        stopped as f32 * 100.0 / robot_frames.max(1) as f32
    );
    println!(
        "ai {:.3} ms/frame (worst {:.2} ms)  sim {:.3} ms/frame",
        per(ai_time),
        worst.as_secs_f64() * 1000.0,
        per(sim_time)
    );
}
