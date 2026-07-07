//! Simulation core (design §2, §5.1), mirroring `cOperationPurge`'s sim
//! half (source-map §2, §3.4) plus the entity classes it owns.
//!
//! Ownership/iteration-order contract (PARITY-critical, design §5.1):
//! robots live in a `BTreeMap<RobotId, Robot>` — ascending-ID iteration
//! provably matches `std::map<tRobotID, cAIRobot*>` over the
//! `cIDGenerator` sequence; projectiles live in a `Vec` with in-order
//! (never `swap_remove`) retain-style removal.

pub mod ai;
pub mod center;
pub mod idgen;
pub mod particles;
pub mod projectile;
pub mod robot;
pub mod spawn;
pub mod world;
