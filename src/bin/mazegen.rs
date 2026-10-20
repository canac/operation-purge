//! Seeded recursive-backtracker generator for new bordered `.maz` files.
//!
//! Not part of the original game: its only contract is the `.maz` format
//! (`maze/mod.rs`, `cMaze.cpp:179-229`); the algorithm has no C++ ground truth.
//!
//! Usage: `mazegen OUT.maz W H [SEED]`. Writes one perfect (single-solution,
//! fully connected) `W`x`H` maze with a full outer border. Refuses to
//! overwrite an existing file, so the recovered originals in
//! `assets/MazeFiles/` can't be clobbered by accident.

use operation_purge::maze::Maze;
use rand::rngs::SmallRng;
use rand::{RngExt, SeedableRng};
use std::path::PathBuf;

const USAGE: &str = "usage: mazegen OUT.maz W H [SEED]";

/// A reference to one shared wall bit, tagged by which region it lives in.
#[derive(Copy, Clone)]
enum WallRef {
    H(usize),
    V(usize),
}

/// Recursive-backtracker perfect-maze generation, iterative (explicit
/// stack) to avoid recursion-depth limits on larger grids. Produces a
/// maze where every room is reachable from every other room via exactly
/// one path (no loops), with a full outer border added afterward (the
/// format itself legally allows open edges).
fn generate(cx: i32, cy: i32, rng: &mut SmallRng) -> Maze {
    // Start with every possible wall present, then carve passages.
    let mut h_walls = vec![true; (cx as usize) * (cy as usize + 1)];
    let mut v_walls = vec![true; (cy as usize) * (cx as usize + 1)];

    let h_index = |x: i32, y: i32| -> usize { (y * cx + x) as usize };
    let v_index = |x: i32, y: i32| -> usize { (y * (cx + 1) + x) as usize };

    let mut visited = vec![false; (cx as usize) * (cy as usize)];
    let room_index = |x: i32, y: i32| -> usize { (y * cx + x) as usize };

    let start = (rng.random_range(0..cx), rng.random_range(0..cy));
    let mut stack = vec![start];
    visited[room_index(start.0, start.1)] = true;

    while let Some(&(x, y)) = stack.last() {
        // Unvisited neighbors, each paired with the shared-wall array
        // index to clear if chosen.
        let mut neighbors: Vec<(i32, i32, WallRef)> = Vec::with_capacity(4);
        if x > 0 && !visited[room_index(x - 1, y)] {
            neighbors.push((x - 1, y, WallRef::V(v_index(x, y))));
        }
        if x + 1 < cx && !visited[room_index(x + 1, y)] {
            neighbors.push((x + 1, y, WallRef::V(v_index(x + 1, y))));
        }
        if y > 0 && !visited[room_index(x, y - 1)] {
            neighbors.push((x, y - 1, WallRef::H(h_index(x, y))));
        }
        if y + 1 < cy && !visited[room_index(x, y + 1)] {
            neighbors.push((x, y + 1, WallRef::H(h_index(x, y + 1))));
        }

        if neighbors.is_empty() {
            stack.pop();
            continue;
        }

        let (nx, ny, wall) = neighbors[rng.random_range(0..neighbors.len())];
        match wall {
            WallRef::H(idx) => h_walls[idx] = false,
            WallRef::V(idx) => v_walls[idx] = false,
        }

        visited[room_index(nx, ny)] = true;
        stack.push((nx, ny));
    }

    // The carve only clears walls between two in-grid rooms, so this just
    // makes the border guarantee explicit.
    for x in 0..cx {
        h_walls[h_index(x, 0)] = true;
        h_walls[h_index(x, cy)] = true;
    }
    for y in 0..cy {
        v_walls[v_index(0, y)] = true;
        v_walls[v_index(cx, y)] = true;
    }

    Maze::new(cx, cy, h_walls, v_walls)
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if !(3..=4).contains(&args.len()) {
        anyhow::bail!(USAGE);
    }
    let path = PathBuf::from(&args[0]);
    let dim = |s: &str, name: &str| -> anyhow::Result<i32> {
        match s.parse::<i32>() {
            Ok(v) if (1..=4096).contains(&v) => Ok(v),
            _ => anyhow::bail!("{name} must be an integer in 1..=4096, got `{s}`\n{USAGE}"),
        }
    };
    let cx = dim(&args[1], "W")?;
    let cy = dim(&args[2], "H")?;
    let seed = match args.get(3) {
        Some(s) => s
            .parse::<u64>()
            .map_err(|e| anyhow::anyhow!("SEED `{s}` is not a valid u64: {e}"))?,
        None => rand::random(),
    };

    if path.exists() {
        anyhow::bail!("refusing to overwrite existing {}", path.display());
    }
    let maze = generate(cx, cy, &mut SmallRng::seed_from_u64(seed));
    maze.save(&path)
        .map_err(|e| anyhow::anyhow!("failed to write {}: {e}", path.display()))?;
    println!("wrote {} ({cx}x{cy}, seed {seed})", path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A perfect maze on an `cx*cy` grid has exactly `cx*cy - 1` open
    /// (non-wall) internal edges — enough to connect every room with no
    /// loops. This also implicitly checks full connectivity together with
    /// `all_rooms_reachable`.
    #[test]
    fn generated_maze_has_exactly_one_spanning_tree_worth_of_open_walls() {
        let mut rng = SmallRng::seed_from_u64(42);
        let maze = generate(6, 5, &mut rng);
        let open_internal =
            maze.h_walls.iter().filter(|w| !**w).count() + maze.v_walls.iter().filter(|w| !**w).count();
        assert_eq!(open_internal, 6 * 5 - 1);
    }

    #[test]
    fn generated_maze_is_fully_bordered() {
        let mut rng = SmallRng::seed_from_u64(7);
        let cx = 5;
        let cy = 4;
        let maze = generate(cx, cy, &mut rng);
        for x in 0..cx {
            assert!(maze.surrounding_walls(x, 0)[0], "top border missing at x={x}");
            assert!(
                maze.surrounding_walls(x, cy - 1)[1],
                "bottom border missing at x={x}"
            );
        }
        for y in 0..cy {
            assert!(maze.surrounding_walls(0, y)[2], "left border missing at y={y}");
            assert!(
                maze.surrounding_walls(cx - 1, y)[3],
                "right border missing at y={y}"
            );
        }
    }

    #[test]
    fn generated_maze_is_deterministic_for_a_given_seed() {
        let mut rng_a = SmallRng::seed_from_u64(99);
        let mut rng_b = SmallRng::seed_from_u64(99);
        let maze_a = generate(6, 6, &mut rng_a);
        let maze_b = generate(6, 6, &mut rng_b);
        assert_eq!(maze_a.h_walls, maze_b.h_walls);
        assert_eq!(maze_a.v_walls, maze_b.v_walls);
    }

    #[test]
    fn every_room_reachable_from_every_other_room() {
        let mut rng = SmallRng::seed_from_u64(3);
        let cx = 6;
        let cy = 6;
        let maze = generate(cx, cy, &mut rng);

        // BFS over the room graph using surrounding_walls as the
        // adjacency oracle.
        let mut visited = vec![false; (cx * cy) as usize];
        let mut queue = std::collections::VecDeque::new();
        queue.push_back((0i32, 0i32));
        visited[0] = true;
        let mut count = 1;
        while let Some((x, y)) = queue.pop_front() {
            let [top, bottom, left, right] = maze.surrounding_walls(x, y);
            let mut visit = |nx: i32, ny: i32, blocked: bool| {
                if !blocked && nx >= 0 && ny >= 0 && nx < cx && ny < cy {
                    let idx = (ny * cx + nx) as usize;
                    if !visited[idx] {
                        visited[idx] = true;
                        count += 1;
                        queue.push_back((nx, ny));
                    }
                }
            };
            visit(x, y - 1, top);
            visit(x, y + 1, bottom);
            visit(x - 1, y, left);
            visit(x + 1, y, right);
        }
        assert_eq!(count, (cx * cy) as usize, "not every room is reachable");
    }
}
