//! Hierarchical scope profiler, replacing `Profile.h`/`cProfileManager`
//! (libs-map §1.8, design §1.4.8, §5.3).
//!
//! Shipped, user-visible feature (P/[/] keybindings toggle
//! running/visible/interaction; on-screen tree pane; 250 ms refresh; exit
//! dump to `Profile output.txt`) — not dev-only tooling. The original
//! measured raw `rdtsc` ticks over `QueryPerformanceFrequency` (a unit
//! mismatch that "worked" only because the overlay shows ratios); the
//! port uses `std::time::Instant`, a correct monotonic clock, everywhere
//! rdtsc appeared.

use std::time::{Duration, Instant};

/// One node in the scope tree. Node identity in the original was
/// static-string **pointer** comparison; here `&'static str` combined
/// with `Eq`/interning-by-value is a correct substitute (source-map §3.7
/// "Node identity = static-string pointer comparison in the game's mode").
pub struct ProfileNode {
    pub name: &'static str,
    pub total_time: Duration,
    pub total_calls: u32,
    pub children: Vec<ProfileNode>,
}

impl ProfileNode {
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            total_time: Duration::ZERO,
            total_calls: 0,
            children: Vec::new(),
        }
    }
}

/// Global profiler state; the app owns one instance (replacing the
/// original's `g_ProfileManager` singleton, libs-map §1.8).
pub struct ProfileManager {
    pub enabled: bool,
    pub root: ProfileNode,
}

impl ProfileManager {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            root: ProfileNode::new("root"),
        }
    }
}

/// RAII scope guard — the Rust analog of the `PROFILE("name")` macro used
/// at ~25 sites in `cOperationPurge.cpp` (libs-map §1.8). Body (actually
/// threading timing into the manager's tree) lands at M3.
pub struct ProfileScope {
    _name: &'static str,
    _start: Instant,
}

impl ProfileScope {
    pub fn enter(name: &'static str) -> Self {
        Self {
            _name: name,
            _start: Instant::now(),
        }
    }
}

impl Drop for ProfileScope {
    fn drop(&mut self) {
        // Recording into the shared tree requires access to the active
        // `ProfileManager`, wired up at M3 alongside `profile_scope!()`.
    }
}

/// Enters a named profile scope for the remainder of the current block.
/// Placeholder macro; gains real tree-recording semantics at M3.
#[macro_export]
macro_rules! profile_scope {
    ($name:expr) => {
        let _profile_scope = $crate::profiler::ProfileScope::enter($name);
    };
}
