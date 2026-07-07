//! CPU view-space billboard sprite batcher, ported from `cSprite3D`
//! (source-map §3.2). Body lands at M5.
//!
//! PARITY: billboard basis reads view-matrix **columns** 0/1/2 exactly
//! as `cSprite3D.cpp:80-85` does (design §3.2); 6 hardcoded axis
//! orientation matrices, copied exactly (`cSprite3D.cpp:128-159`); index
//! pattern `(0,2,1)(1,2,3)`. FIX (design §1.3): the original's
//! `std::map<float, tSprite*>` depth sort silently drops equal-depth-key
//! sprites and has a `Flush` max/min over-lock bug — the port uses a
//! `Vec` + stable sort with exact counts instead (visual-only fix, no
//! feel impact).

use crate::render::mesh::VertexSprite;

/// `s_SpriteBatchSize` (`cOperationPurge.cpp:17`).
pub const BATCH_SIZE: usize = 2048;

pub struct SpriteBatch {
    pub sprites: Vec<VertexSprite>,
}

impl SpriteBatch {
    pub fn new() -> Self {
        Self {
            sprites: Vec::with_capacity(BATCH_SIZE),
        }
    }

    /// Sorts back-to-front by view-space depth (stable sort, Vec-backed —
    /// FIX ledger above) and flushes in texture-run batches. Body lands at M5.
    pub fn flush(&mut self) {
        todo!("ported at M5 — depth sort + batched draw (source-map §3.2, design §1.3 FIX)")
    }
}

impl Default for SpriteBatch {
    fn default() -> Self {
        Self::new()
    }
}
