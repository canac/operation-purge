# The `.maz` maze file format

The port reads and writes the **exact same** `.maz` binary as the 2011
original — the format is part of the contract. This document is the
authoritative spec, verified against `OperationPurge/cMaze.cpp:179-229`
(the editor, MazeCreator, compiles that same serializer, so editor and game
share it byte-for-byte). The Rust codec lives in `src/maze/mod.rs`.

> The stale `MazeCreator/cMaze.{h,cpp}` (Feb-2010) copy is **not** the source
> of truth — it is not built and carries an off-by-one padding bug (see
> below). Do not port from it.

## What is in the file

Binary, **little-endian**, with **no magic number, no version, and no
checksum**. The file stores only the maze dimensions and the wall bits —
nothing else.

| Offset | Size | Content |
|---|---|---|
| `0` | 8 | Win32 `SIZE`: `int32 cx` (width in rooms), `int32 cy` (height in rooms), signed LE |
| `8` | `ceil(N/8)` | Bit-packed wall flags, `N = 2·cx·cy + cx + cy` |

Total file size = `8 + ceil((2·cx·cy + cx + cy) / 8)`. Because there is no
header beyond `cx`/`cy`, that size relationship is the **only** integrity check
possible.

## Wall array ordering (index `i`, 0-based)

The `N` wall bits are two concatenated regions. `N = 2·cx·cy + cx + cy`
(`cMaze.cpp:374-410`, `SetArraySize`).

- **Region 1 — X-direction (horizontal) walls.** Row-major, `cx` columns ×
  `(cy+1)` rows. `i = Y·cx + X` is the wall spanning corner `(X,Y) → (X+1,Y)`,
  i.e. the **top edge of room `(X,Y)`**, with `Y ∈ 0..=cy`. (`cMaze.cpp:387`)
- **Region 2 — Y-direction (vertical) walls.** Base offset `cx·(cy+1)`.
  `(cx+1)` columns × `cy` rows. `i = cx·(cy+1) + Y·(cx+1) + X` is the wall
  spanning `(X,Y) → (X,Y+1)`, i.e. the **left edge of room `(X,Y)`**, with
  `X ∈ 0..=cx`. (`cMaze.cpp:395`)

Room adjacency (`GetSurroundingWalls`, `cMaze.cpp:307-313`): room `(x,y)` has
top = `h(x,y)`, bottom = `h(x,y+1)`, left = `v(x,y)`, right = `v(x+1,y)`.

The Rust `Maze` struct keeps the two regions as decoded `Vec<bool>`
(`h_walls` length `cx·(cy+1)`, `v_walls` length `cy·(cx+1)`) rather than the
original single packed bitset — the packing is purely a serialization detail.

## Bit packing

**LSB-first.** Wall index `i` lives at file byte `8 + i/8`, bit `i % 8`. A set
bit means the wall is **present**:

- read: `byte & (1 << bit)` (`cMaze.cpp:201`)
- write: `byte |= wall << bit` (`cMaze.cpp:223`)

The game's writer zeroes the unused padding bits in the final byte. **Readers
must mask/ignore** those trailing bits: the stale MazeCreator copy has an
off-by-one (`>` instead of `>=` at its lines 97/130), so some pre-2011 files
may carry one garbage bit at position `N`. The port tolerates padding bits.

## Worked example (synthesized — no sample `.maz` files exist in the tree)

A 2×2 maze with a full outer border: `N = 2·2·2 + 2 + 2 = 12` walls, so
`8 + ceil(12/8) = 10` bytes.

```
02 00 00 00  02 00 00 00  73 0B
└─ cx = 2 ──┘└─ cy = 2 ──┘└walls┘
```

X-walls set: `0,1` (top row), `4,5` (bottom row). Y-walls (region base = 6)
set: `6,8` (row 0 left/right), `9,11` (row 1 left/right). Packed:

- byte 0 = bits `{0,1,4,5,6}` = `0x73`
- byte 1 = bits `{8,9,11}` → local bits `{0,1,3}` = `0x0B`

## Validation (GUARD — hardening the original's zero validation)

The C++ does **no** header validation: `cx`/`cy` are signed `int32` fed
straight into an allocation size, so hostile files can demand multi-GB
allocations or trigger UB on negatives. The port (`src/maze/mod.rs`) hardens
this:

- rejects `cx <= 0 || cy <= 0` and any dimension over `MAX_DIM = 4096`;
- requires the **exact** file length — a short read is a clean `MazeError`,
  never a panic (mirroring `cFile::Read`'s exact-byte-count-or-throw
  semantics, which is why truncated `.maz` files fail cleanly in the original);
- tolerates set padding bits in the final byte.

A maze with **no outer border is legal** (the editor has Add/Remove Border
commands); open-edge behaviour is emergent, not validated.

## What is NOT in the file (generated at runtime)

Spawn points, pickup centers, and the extruded 3D building are all derived at
load time, not stored:

- **Building:** the maze becomes the single `cBuilding` (`cBuilding : public
  cMaze`). Room `(X,Y)` occupies
  `[X·RoomSize, (X+1)·RoomSize] × [Y·RoomSize, (Y+1)·RoomSize]`. Scaling
  constants come from `GameData.toml` `[building]` (shipped: room size 10,
  wall width 1, ceiling height 5; help-doc constraints RoomSize ≥ 5,
  CeilingHeight ≥ 1). A wall rect is centered at
  `((X+0.5)·RoomSize, Y·RoomSize)`, box z-center `CeilingHeight/2`
  (`cBuilding.cpp:101-136`); floor/ceiling span `mazeRect·RoomSize`
  (`cBuilding.cpp:251,262`).
- **Team spawn points:** random rejection sampling inside the building
  extents, up to 1000 tries (`cOperationPurge.cpp:2347-2369`).
- **Supply centers:** one random wall-adjacent candidate per room, drawn
  without replacement, offset flush against the wall by
  `fSize/2 + WallWidth/2` with a random slide (`cOperationPurge.cpp:2372-2470`).

## Generating mazes

The shipped `assets/MazeFiles/*.maz` are the **original mazes**, recovered
byte-for-byte from `OperationPurge.msi` (Big1/2, BigBlank, BigRandom,
Small1–15, SmallBlank, SmallRandom). To author *new* mazes, the `mazegen`
tool (`src/bin/mazegen.rs`) emits one bordered perfect maze per run. It
refuses to overwrite an existing file, so it can't clobber the originals:

```sh
cargo run --bin mazegen -- assets/MazeFiles/Mine.maz 10 10 [SEED]
```

## Wildcard maze selection

The New Game maze field may be a **glob pattern** (e.g. `MazeFiles/*.maz`)
rather than a concrete file. When it does not name an existing file it is
expanded via the ported legacy wildcard matcher; on OK the game starts on a
**random** matching file, and the *pattern* (not the resolved file) is what is
persisted to `config.toml`. See `src/maze/search.rs`.
