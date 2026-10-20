//! `.maz` binary maze format (the authoritative
//! spec, verified against `cMaze.cpp:179-229`).
//!
//! Byte layout: 8-byte header = Win32 `SIZE` (i32 LE `cx`, i32 LE `cy`),
//! then `ceil(N/8)` bytes of LSB-first wall bits, `N = 2*cx*cy + cx + cy`.
//! Region 1 (`[0, cx*(cy+1))`) = X-direction (horizontal) walls, `i = Y*cx
//! + X` = top edge of room `(X,Y)`. Region 2 (base `cx*(cy+1)`) =
//! Y-direction (vertical) walls, `i = base + Y*(cx+1) + X` = left edge of
//! room `(X,Y)`.

pub mod building;
pub mod search;

use crate::error::MazeError;
use std::io::{Read, Write};
use std::path::Path;

/// GUARD: the original does zero header validation — a
/// signed `cx`/`cy` pair goes straight into an allocation size. The port
/// rejects hostile headers outside this range.
pub const MAX_DIM: i32 = 4096;

/// A loaded maze: dimensions in rooms + the two wall-bit regions.
///
/// Kept as separate `h_walls`/`v_walls` `Vec<bool>` (rather than the
/// original's single packed bitset) — the packing/unpacking is purely a
/// serialization detail of the `.maz` codec, not something the rest of
/// the sim should have to unpack bit-by-bit.
pub struct Maze {
    pub cx: i32,
    pub cy: i32,
    /// `(cy+1)` rows × `cx` columns; `h_walls[y*cx + x]` = wall on the top
    /// edge of room `(x, y)`.
    pub h_walls: Vec<bool>,
    /// `cy` rows × `(cx+1)` columns; `v_walls[y*(cx+1) + x]` = wall on the
    /// left edge of room `(x, y)`.
    pub v_walls: Vec<bool>,
}

impl Maze {
    /// Total wall-bit count `N = 2*cx*cy + cx + cy` (`cMaze.cpp:374-410`
    /// `SetArraySize`).
    pub fn wall_bit_count(cx: i32, cy: i32) -> i64 {
        2 * cx as i64 * cy as i64 + cx as i64 + cy as i64
    }

    /// Constructs a maze from explicit wall arrays (used by `mazegen` and
    /// tests). `h_walls.len() == cx*(cy+1)`, `v_walls.len() == cy*(cx+1)`.
    pub fn new(cx: i32, cy: i32, h_walls: Vec<bool>, v_walls: Vec<bool>) -> Self {
        debug_assert_eq!(h_walls.len(), (cx as usize) * (cy as usize + 1));
        debug_assert_eq!(v_walls.len(), (cy as usize) * (cx as usize + 1));
        Self {
            cx,
            cy,
            h_walls,
            v_walls,
        }
    }

    /// Reads and validates a `.maz` file. GUARD: `1 <= cx, cy <= 4096` and
    /// exact file length; short reads are a clean `MazeError`, never a
    /// crash (mirrors `cFile::Read`'s exact-byte-count-or-throw semantics).
    ///
    /// PARITY: `cMaze::OpenMaze` (`cMaze.cpp:179-204`) reads the 8-byte
    /// `SIZE` header, then reads the wall bitset one BYTE at a time (not
    /// in one `ceil(N/8)`-sized read) — behaviorally identical to a single
    /// bulk read since `cFile::Read` is exact-byte-count-or-throw either
    /// way, so the port reads the whole tail in one call.
    pub fn load(path: &Path) -> Result<Maze, MazeError> {
        let mut file = std::fs::File::open(path).map_err(|source| MazeError::Io {
            path: path.to_path_buf(),
            source,
        })?;

        let mut header = [0u8; 8];
        file.read_exact(&mut header).map_err(|source| MazeError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let cx = i32::from_le_bytes([header[0], header[1], header[2], header[3]]);
        let cy = i32::from_le_bytes([header[4], header[5], header[6], header[7]]);

        // GUARD: the
        // original feeds a signed cx/cy straight into an allocation size
        // with zero validation. Reject hostile headers before allocating.
        if cx < 1 || cy < 1 || cx > MAX_DIM || cy > MAX_DIM {
            return Err(MazeError::HostileHeader {
                path: path.to_path_buf(),
                cx,
                cy,
            });
        }

        let n = Self::wall_bit_count(cx, cy);
        let n_bytes = (n as u64).div_ceil(8);
        let expected_total = 8 + n_bytes;

        let mut rest = Vec::new();
        file.read_to_end(&mut rest).map_err(|source| MazeError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let found_total = 8 + rest.len() as u64;
        if rest.len() as u64 != n_bytes {
            return Err(MazeError::WrongLength {
                path: path.to_path_buf(),
                cx,
                cy,
                expected: expected_total,
                found: found_total,
            });
        }

        // Unpack LSB-first, byte by byte (cMaze.cpp:189-203). The final
        // byte's padding bits (beyond N) are masked/ignored — pre-2011
        // files may carry garbage there.
        let mut all_walls = vec![false; n as usize];
        for (wall_idx, wall) in all_walls.iter_mut().enumerate() {
            let byte = rest[wall_idx / 8];
            let bit = wall_idx % 8;
            *wall = (byte & (1 << bit)) != 0;
        }

        let h_count = (cx as usize) * (cy as usize + 1);
        let h_walls = all_walls[..h_count].to_vec();
        let v_walls = all_walls[h_count..].to_vec();

        Ok(Maze {
            cx,
            cy,
            h_walls,
            v_walls,
        })
    }

    /// Writes a `.maz` file, zero-padding the final byte's unused bits
    /// (`cMaze::SaveMaze`, `cMaze.cpp:207-229`).
    pub fn save(&self, path: &Path) -> Result<(), MazeError> {
        let n = Self::wall_bit_count(self.cx, self.cy) as usize;
        debug_assert_eq!(self.h_walls.len() + self.v_walls.len(), n);

        let mut file = std::fs::File::create(path).map_err(|source| MazeError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        file.write_all(&self.cx.to_le_bytes())
            .and_then(|_| file.write_all(&self.cy.to_le_bytes()))
            .map_err(|source| MazeError::Io {
                path: path.to_path_buf(),
                source,
            })?;

        let n_bytes = n.div_ceil(8);
        let mut bytes = vec![0u8; n_bytes];
        for wall_idx in 0..n {
            let wall = if wall_idx < self.h_walls.len() {
                self.h_walls[wall_idx]
            } else {
                self.v_walls[wall_idx - self.h_walls.len()]
            };
            if wall {
                bytes[wall_idx / 8] |= 1 << (wall_idx % 8);
            }
        }
        file.write_all(&bytes).map_err(|source| MazeError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(())
    }

    /// `GetWall(X, Y, wallX)` (`cMaze.cpp:246`) — the X-direction
    /// (horizontal) wall = top edge of room `(x, y)`. Out-of-grid = no
    /// wall (GUARD), matching the non-crashing in-range reads.
    pub fn wall_x(&self, x: i32, y: i32) -> bool {
        self.h_wall(x, y)
    }

    /// `GetWall(X, Y, wallY)` (`cMaze.cpp:246`) — the Y-direction
    /// (vertical) wall = left edge of room `(x, y)`. Out-of-grid = no wall.
    pub fn wall_y(&self, x: i32, y: i32) -> bool {
        self.v_wall(x, y)
    }

    /// `h_walls[y*cx + x]` accessor with GUARD out-of-grid = no wall
    /// (open-edge mazes stay legal).
    fn h_wall(&self, x: i32, y: i32) -> bool {
        if x < 0 || y < 0 || x >= self.cx || y > self.cy {
            return false;
        }
        self.h_walls[(y * self.cx + x) as usize]
    }

    /// `v_walls[y*(cx+1) + x]` accessor with GUARD out-of-grid = no wall.
    fn v_wall(&self, x: i32, y: i32) -> bool {
        if x < 0 || y < 0 || x > self.cx || y >= self.cy {
            return false;
        }
        self.v_walls[(y * (self.cx + 1) + x) as usize]
    }

    /// `GetSurroundingWalls` (`cMaze.cpp:307-313`): room `(x,y)` → top =
    /// `h(x,y)`, bottom = `h(x,y+1)`, left = `v(x,y)`, right =
    /// `v(x+1,y)`. Returns `[top, bottom, left, right]`.
    ///
    /// GUARD: out-of-grid queries return "no wall" rather
    /// than the original's unchecked array read/log-and-return-`m_aWalls[0]`
    /// (`cMaze::GetWallReference`, `cMaze.cpp:286-301`) — this matches what
    /// the non-crashing in-range C++ reads effectively did, so open-edge
    /// mazes stay legal and their emergent behavior is preserved.
    pub fn surrounding_walls(&self, x: i32, y: i32) -> [bool; 4] {
        [
            self.h_wall(x, y),
            self.h_wall(x, y + 1),
            self.v_wall(x, y),
            self.v_wall(x + 1, y),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// The synthesized 10-byte 2x2 golden: `02 00 00 00 02 00 00 00 73 0B`.
    const GOLDEN_2X2: [u8; 10] = [0x02, 0, 0, 0, 0x02, 0, 0, 0, 0x73, 0x0B];

    fn write_temp(bytes: &[u8]) -> std::path::PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "op-maze-test-{}-{}.maz",
            std::process::id(),
            fastrand_like()
        ));
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(bytes).unwrap();
        path
    }

    // Tiny non-crypto counter so parallel tests don't collide on filenames
    // without pulling in a new dependency.
    fn fastrand_like() -> u64 {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        COUNTER.fetch_add(1, Ordering::Relaxed)
    }

    #[test]
    fn golden_2x2_loads_with_expected_walls() {
        let path = write_temp(&GOLDEN_2X2);
        let maze = Maze::load(&path).unwrap();
        std::fs::remove_file(&path).ok();

        assert_eq!(maze.cx, 2);
        assert_eq!(maze.cy, 2);
        // X-walls set: 0,1 (top row), 4,5 (bottom row).
        assert_eq!(maze.h_walls, vec![true, true, false, false, true, true]);
        // Y-walls (offset 6) set: 6,8,9,11 -> local indices 0,2,3,5 within
        // the 6-entry v_walls array (base subtracted).
        assert_eq!(
            maze.v_walls,
            vec![true, false, true, true, false, true]
        );
    }

    #[test]
    fn golden_2x2_round_trips_through_save() {
        let path = write_temp(&GOLDEN_2X2);
        let maze = Maze::load(&path).unwrap();
        std::fs::remove_file(&path).ok();

        let out_path = write_temp(&[]);
        maze.save(&out_path).unwrap();
        let bytes = std::fs::read(&out_path).unwrap();
        std::fs::remove_file(&out_path).ok();
        assert_eq!(bytes, GOLDEN_2X2);
    }

    #[test]
    fn hostile_header_zero_or_negative_is_rejected() {
        // cx = 0
        let path = write_temp(&[0, 0, 0, 0, 2, 0, 0, 0]);
        assert!(matches!(
            Maze::load(&path),
            Err(MazeError::HostileHeader { .. })
        ));
        std::fs::remove_file(&path).ok();

        // cx = -1 (0xFFFFFFFF)
        let path = write_temp(&[0xFF, 0xFF, 0xFF, 0xFF, 2, 0, 0, 0]);
        assert!(matches!(
            Maze::load(&path),
            Err(MazeError::HostileHeader { .. })
        ));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn hostile_header_over_max_dim_is_rejected() {
        let over = (MAX_DIM + 1).to_le_bytes();
        let mut bytes = over.to_vec();
        bytes.extend_from_slice(&2i32.to_le_bytes());
        let path = write_temp(&bytes);
        assert!(matches!(
            Maze::load(&path),
            Err(MazeError::HostileHeader { .. })
        ));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn truncated_file_is_a_clean_error_not_a_panic() {
        // Valid 2x2 header but missing wall bytes.
        let path = write_temp(&[2, 0, 0, 0, 2, 0, 0, 0]);
        assert!(matches!(
            Maze::load(&path),
            Err(MazeError::WrongLength { .. })
        ));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn padding_bits_in_final_byte_are_masked_and_ignored() {
        // Same golden maze, but with garbage set in the final byte's
        // unused high bits (N=12 -> only bits 0-3 of byte 1 are real;
        // bits 4-7 are padding), which readers must ignore.
        let mut bytes = GOLDEN_2X2;
        bytes[9] |= 0b1111_0000;
        let path = write_temp(&bytes);
        let maze = Maze::load(&path).unwrap();
        std::fs::remove_file(&path).ok();
        assert_eq!(maze.h_walls, vec![true, true, false, false, true, true]);
        assert_eq!(
            maze.v_walls,
            vec![true, false, true, true, false, true]
        );
    }

    #[test]
    fn surrounding_walls_matches_get_surrounding_walls_semantics() {
        let path = write_temp(&GOLDEN_2X2);
        let maze = Maze::load(&path).unwrap();
        std::fs::remove_file(&path).ok();

        // Room (0,0): top=h(0,0)=true, bottom=h(0,1)=false (h_walls index
        // 1*2+0=2 -> false), left=v(0,0)=true, right=v(1,0)=false.
        let walls = maze.surrounding_walls(0, 0);
        assert_eq!(walls, [true, false, true, false]);
    }

    #[test]
    fn out_of_grid_queries_return_no_wall_guard() {
        let path = write_temp(&GOLDEN_2X2);
        let maze = Maze::load(&path).unwrap();
        std::fs::remove_file(&path).ok();

        // Way outside the 2x2 grid.
        let walls = maze.surrounding_walls(50, 50);
        assert_eq!(walls, [false, false, false, false]);
        let walls = maze.surrounding_walls(-5, -5);
        assert_eq!(walls, [false, false, false, false]);
    }

    fn arb_maze() -> impl Strategy<Value = (i32, i32, Vec<bool>, Vec<bool>)> {
        (1i32..8, 1i32..8).prop_flat_map(|(cx, cy)| {
            let h_count = (cx as usize) * (cy as usize + 1);
            let v_count = (cy as usize) * (cx as usize + 1);
            (
                Just(cx),
                Just(cy),
                proptest::collection::vec(any::<bool>(), h_count),
                proptest::collection::vec(any::<bool>(), v_count),
            )
        })
    }

    proptest! {
        #[test]
        fn write_then_read_is_identity((cx, cy, h_walls, v_walls) in arb_maze()) {
            let maze = Maze::new(cx, cy, h_walls.clone(), v_walls.clone());
            let path = write_temp(&[]);
            maze.save(&path).unwrap();
            let loaded = Maze::load(&path).unwrap();
            std::fs::remove_file(&path).ok();

            prop_assert_eq!(loaded.cx, cx);
            prop_assert_eq!(loaded.cy, cy);
            prop_assert_eq!(loaded.h_walls, h_walls);
            prop_assert_eq!(loaded.v_walls, v_walls);
        }

        #[test]
        fn write_then_read_is_identity_including_borderless(
            (cx, cy, mut h_walls, mut v_walls) in arb_maze()
        ) {
            // Force every border wall bit false: a maze with no outer
            // border is legal.
            for x in 0..cx {
                h_walls[x as usize] = false; // top row
                h_walls[(cy as usize) * cx as usize + x as usize] = false; // bottom row
            }
            for y in 0..cy {
                v_walls[(y as usize) * (cx as usize + 1)] = false; // left col
                v_walls[(y as usize) * (cx as usize + 1) + cx as usize] = false; // right col
            }

            let maze = Maze::new(cx, cy, h_walls.clone(), v_walls.clone());
            let path = write_temp(&[]);
            maze.save(&path).unwrap();
            let loaded = Maze::load(&path).unwrap();
            std::fs::remove_file(&path).ok();

            prop_assert_eq!(loaded.h_walls, h_walls);
            prop_assert_eq!(loaded.v_walls, v_walls);
        }
    }
}
