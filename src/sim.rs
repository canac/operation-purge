//! Simulation core, mirroring `cOperationPurge`'s sim
//! half plus the entity classes it owns.
//!
//! Ownership/iteration-order contract (PARITY-critical):
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
