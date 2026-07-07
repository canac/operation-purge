//! `cIDGenerator`: binary-subdivision ID sequence for robots/centers
//! (source-map §3.7, §8.10), ported verbatim from `cIDGenerator.h/.cpp`.
//!
//! PARITY: the constructor's `nBits` parameter is a no-op bug —
//! `cIDGenerator.cpp:5` initializes `m_nBits` from a ternary whose *both*
//! branches evaluate to `sizeof(DWORD)*8` (32), so the passed-in argument
//! is silently ignored and `m_nBits` is unconditionally 32. Kept verbatim
//! (not "fixed" to actually honor the argument) because the resulting
//! 32-level ID sequence is observable: it's the iteration order
//! `BTreeMap<RobotId, Robot>` must provably match `std::map<tRobotID,
//! cAIRobot*>` over (design §5.1).
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
    /// PARITY: the original constructor's `nBits` argument is ignored
    /// (source-map §8.10, `cIDGenerator.cpp:5`) — the port keeps the same
    /// no-op parameter so call sites read identically, but the behavior
    /// does not depend on it (always starts at `level=0, node=0` with an
    /// effective bit width of [`N_BITS`]).
    pub fn new(_n_bits: u32) -> Self {
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
        Self::new(N_BITS)
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
        let mut gen = IdGenerator::new(N_BITS);
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
        let mut gen = IdGenerator::new(N_BITS);
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
    fn ctor_n_bits_argument_is_ignored() {
        // PARITY: any nBits argument must produce the identical sequence.
        let mut a = IdGenerator::new(8);
        let mut b = IdGenerator::new(32);
        let mut c = IdGenerator::new(0);
        for _ in 0..64 {
            let (va, vb, vc) = (a.next(), b.next(), c.next());
            assert_eq!(va, vb);
            assert_eq!(vb, vc);
        }
    }

    #[test]
    fn reset_restarts_the_sequence() {
        let mut gen = IdGenerator::new(N_BITS);
        let first_id = gen.next();
        gen.next();
        gen.next();
        gen.reset();
        assert_eq!(gen.next(), first_id);
    }

    /// design §6 / §7 M2: "BTreeMap<RobotId, _> iteration over them equals
    /// ascending std::map order" — a `BTreeMap`/`std::map` always iterates
    /// in ascending key order regardless of insertion order, so inserting
    /// the generator's (unsorted-looking) sequence and reading it back
    /// must come out numerically sorted, exactly like the original's
    /// `std::map<tRobotID, cAIRobot*>`.
    #[test]
    fn btreemap_over_the_generated_ids_iterates_in_ascending_order() {
        let mut gen = IdGenerator::new(N_BITS);
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
