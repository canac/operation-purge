//! A* node arena, replacing `cNodeList` + `cElementBank<T>`
//! (libs-map §1.5, source-map §3.7), ported from `cNodeList.h/.cpp` +
//! `cElementBank.h/.cpp` + `Node.h`.
//!
//! Semantics preserved: `reset()` = cursor back to 0, stale contents left
//! as-is (`cElementBank::Reset` just zeroes `m_nElementsUsed`, callers
//! fully overwrite on reuse, libs-map §1.5); initial capacity 256
//! (`cElementBank` ctor call in `cNodeList::cNodeList`, `cNodeList.cpp:8`),
//! 2x growth (`cElementBank::GrowElementBank`, `cElementBank.cpp:56-67`).
//! Node identity = position quantized to 1/256 unit (`s_nNodesPerPoint =
//! 256`, `cPathFinder.cpp:14`), hashed via a 16-bit-per-axis pack that
//! **wraps every 256 world units** because the quantized coordinate is
//! truncated into an `unsigned short` (`cNodeList::HashPoint`,
//! `cNodeList.cpp:79-84`); the float->u16 truncation keeps C truncation-
//! toward-zero semantics via `as i64 as u16`, never a saturating `as u16`
//! (design §1.3 FIX ledger) — `as i64` truncates toward zero exactly like
//! a C `(int)`/`(unsigned short)` cast on an in-range value, and the
//! subsequent `as u16` reproduces the 16-bit truncation the original gets
//! from casting into `unsigned short`.

use glam::Vec2;
use std::collections::HashMap;

/// Initial node-pool capacity — PARITY, `cNodeList.cpp:8` (`cElementBank`
/// ctor argument `256`).
pub const INITIAL_CAPACITY: usize = 256;

/// PARITY: `cPathFinder.cpp:14` `s_nNodesPerPoint = 256` — the hash
/// quantizes world-space coordinates to 1/256-unit steps.
pub const NODES_PER_POINT: f32 = 256.0;

/// `tNode` (`Node.h:8-15`). `parent` replaces the raw `tNode*` with an
/// arena index; `location` is the exact (unquantized) float position —
/// only the *hash key* is quantized, matching `cNodeList::HashPoint`
/// being computed from, not stored alongside, the node.
#[derive(Copy, Clone, Debug, Default)]
pub struct Node {
    pub location: Vec2,
    pub parent: Option<usize>,
    /// `fCostFromStart` (g).
    pub cost_from_start: f32,
    /// `fCostToGoal` (h, the — inadmissible by design — heuristic estimate).
    pub cost_to_goal: f32,
    pub on_open: bool,
    pub on_closed: bool,
}

/// `Vec`-backed arena with a used-cursor, replacing `cElementBank<tNode>`
/// plus `cNodeList`'s hash-map-based dedup (`cNodeList.cpp`).
pub struct NodeArena {
    nodes: Vec<Node>,
    used: usize,
    /// Replaces `cNodeList::m_NodeHashMap` (`stdext::hash_map<DWORD, pNode>`).
    hash_map: HashMap<u32, usize>,
}

impl NodeArena {
    pub fn new() -> Self {
        Self {
            nodes: vec![Node::default(); INITIAL_CAPACITY],
            used: 0,
            hash_map: HashMap::new(),
        }
    }

    /// PARITY: `cNodeList::Reset` (`cNodeList.cpp:19-23`) — clears the
    /// hash map and rewinds the arena cursor; does **not** touch existing
    /// `Node` contents (matching `cElementBank::Reset`'s "stale contents
    /// left as-is" semantics, libs-map §1.5). Safe because [`get_node`]
    /// unconditionally overwrites every field of a freshly (re)used slot.
    pub fn reset(&mut self) {
        self.used = 0;
        self.hash_map.clear();
    }

    /// Quantizes a single world-space coordinate to 1/256-unit steps,
    /// keeping C truncation-toward-zero and 16-bit wraparound semantics.
    ///
    /// PARITY (design §1.3 FIX ledger): `cNodeList::HashPoint`
    /// (`cNodeList.cpp:79-84`) computes `(unsigned short) (coord *
    /// nNodesPerPoint)`. A C++ float-to-`unsigned short` cast of an
    /// in-range value truncates toward zero then keeps the low 16 bits;
    /// `as i64 as u16` reproduces exactly that (Rust's `f32 as i64`
    /// truncates toward zero like a C `(int)`/`(long)` cast, and `as u16`
    /// on the `i64` keeps only the low 16 bits, matching a hardware
    /// integer narrowing) — **not** a saturating `as u16` directly on the
    /// float, which would clamp instead of wrapping and silently break
    /// the "hash wraps every 256 world units" behavior for any coordinate
    /// magnitude beyond 256.
    pub fn hash_coord(v: f32) -> u16 {
        (v * NODES_PER_POINT) as i64 as u16
    }

    /// `cNodeList::HashPoint` (`cNodeList.cpp:79-84`): packs the quantized
    /// y (high 16 bits) and x (low 16 bits) into one `DWORD` key.
    pub fn hash_point(p: Vec2) -> u32 {
        (Self::hash_coord(p.y) as u32) << 16 | Self::hash_coord(p.x) as u32
    }

    /// `cNodeList::GetNode` (`cNodeList.cpp:26-42`): find-or-create by
    /// hash. Returns an arena index (`pNode` replacement).
    ///
    /// On a hash miss the freshly (re)used slot's fields are fully
    /// overwritten (`location` set, `parent`/costs zeroed, `on_open`/
    /// `on_closed` cleared) — the C++ only explicitly resets
    /// `pt2Location`/`bOnOpen`/`bOnClosed` on that path (`cNodeList.cpp:34-37`),
    /// leaving `fCostFromStart`/`fCostToGoal` as whatever garbage the
    /// freshly-`new`'d or previously-reused C++ node object held; that
    /// garbage is provably never read before being overwritten by the
    /// caller (`cPathFinder::FindPath`'s `bOnOpen || bOnClosed` gate is
    /// false for a brand-new node, so it always takes the branch that
    /// sets both costs before anything reads them) — zeroing them here
    /// is a behavior-preserving safety improvement, not a divergence.
    pub fn get_node(&mut self, location: Vec2) -> usize {
        let key = Self::hash_point(location);
        if let Some(&idx) = self.hash_map.get(&key) {
            return idx;
        }

        let idx = self.alloc();
        self.nodes[idx] = Node {
            location,
            parent: None,
            cost_from_start: 0.0,
            cost_to_goal: 0.0,
            on_open: false,
            on_closed: false,
        };
        self.hash_map.insert(key, idx);
        idx
    }

    /// `cElementBank::GetFreeElement` + `GrowElementBank`
    /// (`cElementBank.cpp:44-67`): 2x growth when the pool is exhausted.
    fn alloc(&mut self) -> usize {
        if self.used >= self.nodes.len() {
            let new_len = self.nodes.len() * 2;
            self.nodes.resize(new_len, Node::default());
        }
        let idx = self.used;
        self.used += 1;
        idx
    }

    pub fn get(&self, idx: usize) -> &Node {
        &self.nodes[idx]
    }

    pub fn get_mut(&mut self, idx: usize) -> &mut Node {
        &mut self.nodes[idx]
    }

    /// `cNodeList::GetNodesUsed` (`cNodeList.cpp:45-48`).
    pub fn used(&self) -> usize {
        self.used
    }
}

impl Default for NodeArena {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_capacity_is_256_and_reset_keeps_it() {
        let mut arena = NodeArena::new();
        assert_eq!(arena.used(), 0);
        for i in 0..256 {
            arena.get_node(Vec2::new(i as f32, 0.0));
        }
        assert_eq!(arena.used(), 256);
        arena.reset();
        assert_eq!(arena.used(), 0);
    }

    #[test]
    fn growth_doubles_capacity_past_256() {
        let mut arena = NodeArena::new();
        // Small fractional step so all 300 positions stay well inside one
        // 256-world-unit hash period (see `hash_coord_wraps_every_256...`
        // below) and therefore hash to distinct keys.
        for i in 0..300 {
            arena.get_node(Vec2::new(i as f32 * 0.1, 0.0));
        }
        assert_eq!(arena.used(), 300);
    }

    #[test]
    fn get_node_deduplicates_by_hash() {
        let mut arena = NodeArena::new();
        let a = arena.get_node(Vec2::new(1.5, 2.5));
        let b = arena.get_node(Vec2::new(1.5, 2.5));
        assert_eq!(a, b);
        assert_eq!(arena.used(), 1);
    }

    #[test]
    fn reset_lets_positions_be_reallocated_fresh() {
        let mut arena = NodeArena::new();
        let idx = arena.get_node(Vec2::new(3.0, 4.0));
        arena.get_mut(idx).on_open = true;
        arena.get_mut(idx).cost_from_start = 42.0;

        arena.reset();
        let idx2 = arena.get_node(Vec2::new(3.0, 4.0));
        // Same slot is reused (arena isn't cleared) but GetNode
        // unconditionally resets the logical fields.
        assert_eq!(idx2, 0);
        assert!(!arena.get(idx2).on_open);
        assert_eq!(arena.get(idx2).cost_from_start, 0.0);
    }

    #[test]
    fn hash_coord_quantizes_to_1_over_256_unit() {
        assert_eq!(NodeArena::hash_coord(0.0), 0);
        assert_eq!(NodeArena::hash_coord(1.0 / 256.0), 1);
        assert_eq!(NodeArena::hash_coord(2.0 / 256.0), 2);
        // Truncation toward zero within a quantization bucket.
        assert_eq!(NodeArena::hash_coord(1.999 / 256.0), 1);
    }

    #[test]
    fn hash_coord_wraps_every_256_world_units_via_u16_truncation() {
        // 256.0 world units * 256 (NODES_PER_POINT) = 65536 = 2^16 -> wraps to 0.
        assert_eq!(NodeArena::hash_coord(256.0), 0);
        // One quantization step past that wraps to 1, matching the coord
        // just past 0 (i.e. the hash is periodic with period 256 world
        // units, not just a monotonic quantization).
        assert_eq!(
            NodeArena::hash_coord(256.0 + 1.0 / 256.0),
            NodeArena::hash_coord(1.0 / 256.0)
        );
    }

    #[test]
    fn hash_coord_truncation_matches_c_semantics_not_saturation() {
        // A value large enough that a naive `as u16` on the float itself
        // would saturate to u16::MAX, but the C-truncation path
        // (`as i64 as u16`) instead wraps via the low 16 bits.
        let v = 70000.0 / NODES_PER_POINT; // *256 = 70000, doesn't fit u16
        let got = NodeArena::hash_coord(v);
        let expected = 70000i64 as u16; // low 16 bits of 70000 = 4464
        assert_eq!(got, expected);
        assert_ne!(got, u16::MAX);
    }

    #[test]
    fn hash_point_packs_y_high_x_low() {
        let key = NodeArena::hash_point(Vec2::new(1.0 / 256.0, 2.0 / 256.0));
        assert_eq!(key, (2u32 << 16) | 1u32);
    }
}
