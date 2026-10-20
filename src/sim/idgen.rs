//! `cIDGenerator`: binary-subdivision ID sequence for robots/centers,
//! ported verbatim from `cIDGenerator.h/.cpp`.
//!
//! PARITY: `cIDGenerator.cpp:5` initializes `m_nBits` from a ternary whose
//! *both* branches evaluate to 32, so the sequence is always 32 levels
//! deep. It is observable: it's the iteration order `BTreeMap<RobotId,
//! Robot>` must match `std::map<tRobotID, cAIRobot*>` over.
//!
//! The sequence is a bit-reversal (Van der Corput) counter: the n-th
//! (0-indexed) value equals `(n+1)` with its 32 bits reversed. That closed
//! form is used as an independent cross-check in the tests, alongside a
//! direct port of the node/level state machine below.

/// PARITY: `cIDGenerator.cpp:5` — both ternary branches evaluate to this,
/// so the ctor's `nBits` argument never has any effect.
pub const N_BITS: u32 = 32;

pub struct IdGenerator {
    node: u32,
    level: u32,
}

impl IdGenerator {
    /// Starts the sequence at `level=0, node=0`. The original ctor's `nBits`
    /// argument had no effect (`cIDGenerator.cpp:5`), so it is not ported.
    pub fn new() -> Self {
        Self { node: 0, level: 0 }
    }

    /// Returns the next ID in the binary-subdivision sequence, or `0` once
    /// the sequence is exhausted (`level >= N_BITS`, i.e. after
    /// `2^32 - 1` calls).
    ///
    /// PARITY: `cIDGenerator.cpp:18-31` `Next()` verbatim:
    /// `id = (1 << (nBits-1-level)) + node * (1 << (nBits-level))`, then
    /// advance `node`, rolling over to the next `level` once
    /// `node >= 1 << level`.
    ///
    /// PARITY (hardware shift quirk): `1 << (nBits - level)` shifts by 32
    /// when `level == 0`, which is C++ shift-by-width undefined behavior —
    /// on the x86 target the game shipped on this resolves to a hardware
    /// `SHL` with the shift count masked mod 32 (`1 << 32 == 1 << 0 == 1`
    /// there). `u32::wrapping_shl` performs that exact masking, so it
    /// reproduces the platform behavior byte-for-byte. It never actually
    /// changes the result: `node` is provably `0` whenever `level == 0`
    /// (there is only one node at level 0), so the term it multiplies is
    /// always `0 * anything`.
    #[allow(clippy::should_implement_trait)] // named to match `cIDGenerator::Next` verbatim
    pub fn next(&mut self) -> u32 {
        if self.level >= N_BITS {
            return 0;
        }

        let id = 1u32
            .wrapping_shl(N_BITS - 1 - self.level)
            .wrapping_add(self.node.wrapping_mul(1u32.wrapping_shl(N_BITS - self.level)));

        self.node += 1;
        if self.node >= 1u32.wrapping_shl(self.level) {
            self.node = 0;
            self.level += 1;
        }

        id
    }

    /// PARITY: `cIDGenerator.cpp:34-38` `Reset()`.
    pub fn reset(&mut self) {
        self.node = 0;
        self.level = 0;
    }
}

impl Default for IdGenerator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::RobotId;
    use std::collections::BTreeMap;

    /// Independent closed-form cross-check, derived by hand from the
    /// `node`/`level` recurrence in [`IdGenerator::next`] but computed
    /// without any shared mutable state or wraparound bookkeeping:
    /// `id(n) = (2*node + 1) << (31 - level)`, where `level` is the
    /// unique value with `2^level - 1 <= n < 2^(level+1) - 1` and
    /// `node = n - (2^level - 1)` is the 0-based index within that level.
    /// (Equivalently: level `L` holds the `2^L` odd multiples of
    /// `2^(31-L)` in increasing order — the classic dyadic/Van der Corput
    /// enumeration order.)
    fn closed_form_reference(n: u32) -> u32 {
        let mut level = 0u32;
        let mut base = 0u32; // 2^level - 1
        loop {
            let count = 1u32 << level;
            if n < base + count {
                let node = n - base;
                return (2 * node + 1) << (31 - level);
            }
            base += count;
            level += 1;
        }
    }

    #[test]
    fn first_64_ids_are_pinned_and_match_the_closed_form() {
        let mut gen = IdGenerator::new();
        for n in 0..64u32 {
            let id = gen.next();
            assert_eq!(
                id,
                closed_form_reference(n),
                "ID #{n} diverged from the closed-form reference"
            );
        }
    }

    #[test]
    fn first_few_ids_are_pinned_to_explicit_hex_literals() {
        // Hand-derived from the Next() recurrence directly (level 0..2):
        // level 0 (1 node):     0x80000000
        // level 1 (2 nodes):    0x40000000, 0xC0000000
        // level 2 (4 nodes):    0x20000000, 0x60000000, 0xA0000000, 0xE0000000
        let mut gen = IdGenerator::new();
        let expected: [u32; 7] = [
            0x8000_0000,
            0x4000_0000,
            0xC000_0000,
            0x2000_0000,
            0x6000_0000,
            0xA000_0000,
            0xE000_0000,
        ];
        for &e in &expected {
            assert_eq!(gen.next(), e);
        }
    }

    #[test]
    fn reset_restarts_the_sequence() {
        let mut gen = IdGenerator::new();
        let first_id = gen.next();
        gen.next();
        gen.next();
        gen.reset();
        assert_eq!(gen.next(), first_id);
    }

    /// "BTreeMap<RobotId, _> iteration over them equals ascending std::map
    /// order" — a `BTreeMap`/`std::map` always iterates
    /// in ascending key order regardless of insertion order, so inserting
    /// the generator's (unsorted-looking) sequence and reading it back
    /// must come out numerically sorted, exactly like the original's
    /// `std::map<tRobotID, cAIRobot*>`.
    #[test]
    fn btreemap_over_the_generated_ids_iterates_in_ascending_order() {
        let mut gen = IdGenerator::new();
        let mut map: BTreeMap<RobotId, u32> = BTreeMap::new();
        let mut generated = Vec::new();
        for i in 0..64u32 {
            let id = gen.next();
            generated.push(id);
            map.insert(RobotId::new(id), i);
        }

        let iterated: Vec<u32> = map.keys().map(|id| id.get()).collect();
        let mut sorted = generated.clone();
        sorted.sort_unstable();
        assert_eq!(iterated, sorted);

        // Insertion order was NOT already sorted — this is a meaningful
        // check, not a tautology.
        assert_ne!(generated, sorted);
    }
}
