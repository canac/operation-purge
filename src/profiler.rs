//! Hierarchical scope profiler, replacing `Profile.h`/`cProfileManager`.
//!
//! Shipped, user-visible feature (P toggles running, `[` toggles the
//! on-screen pane; exit dump to `Profile output.txt`). The original
//! measured raw `rdtsc` ticks over `QueryPerformanceFrequency` (a unit
//! mismatch that "worked" only because the overlay shows ratios); the port
//! uses `std::time::Instant` everywhere rdtsc appeared.
//!
//! Structure mirrors `Common/Profile.{h,cpp}`: a tree of named nodes with a
//! per-node recursion counter (`Call`/`Return`) and a "current node" cursor
//! the `PROFILE(name)` scope walks. The tree is an index arena; node
//! identity by `&'static str` value replaces the original's static-string
//! *pointer* comparison.

use std::cell::RefCell;
use std::fmt::Write as _;
use std::time::{Duration, Instant};

/// Arena index of the root node; its children are the top-level scopes.
pub const ROOT: usize = 0;

/// One node in the profile tree (`cProfileNode`).
pub struct ProfileNode {
    pub name: &'static str,
    pub total_time: Duration,
    pub total_calls: u32,
    /// PARITY (`cProfileNode::m_RecursionCounter`): re-entering the same-named
    /// scope only bumps this; timing starts on 0→1 and accumulates on 1→0,
    /// so recursive scopes are timed once, not N times.
    recursion: u32,
    start: Option<Instant>,
    parent: Option<usize>,
    children: Vec<usize>,
}

impl ProfileNode {
    fn new(name: &'static str, parent: Option<usize>) -> Self {
        Self {
            name,
            total_time: Duration::ZERO,
            total_calls: 0,
            recursion: 0,
            start: None,
            parent,
            children: Vec::new(),
        }
    }
}

/// The profile tree plus its session clock (the original's
/// `g_ProfileManager`).
pub struct ProfileManager {
    enabled: bool,
    nodes: Vec<ProfileNode>,
    current: usize,
    /// Enabled time from earlier on/off stretches; the session clock only
    /// runs while enabled so dump percentages compare like with like.
    enabled_before: Duration,
    enabled_since: Option<Instant>,
}

impl ProfileManager {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            nodes: vec![ProfileNode::new("Root", None)],
            current: ROOT,
            enabled_before: Duration::ZERO,
            enabled_since: enabled.then(Instant::now),
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// `cProfileManager::SetProfilingEnabled` (the profiler-running key).
    pub fn set_enabled(&mut self, enabled: bool) {
        self.set_enabled_at(enabled, Instant::now());
    }

    fn set_enabled_at(&mut self, enabled: bool, now: Instant) {
        if enabled == self.enabled {
            return;
        }
        self.enabled = enabled;
        if enabled {
            self.enabled_since = Some(now);
        } else if let Some(since) = self.enabled_since.take() {
            self.enabled_before += now.saturating_duration_since(since);
        }
    }

    /// Total time spent enabled up to `now` (`GetTimeElapsed`).
    fn time_elapsed_at(&self, now: Instant) -> Duration {
        self.enabled_before
            + self
                .enabled_since
                .map_or(Duration::ZERO, |since| now.saturating_duration_since(since))
    }

    fn find_or_create_child(&mut self, parent: usize, name: &'static str) -> usize {
        // PARITY: `cProfileNode::GetSubNode` linear-scans siblings by name.
        if let Some(&existing) = self.nodes[parent]
            .children
            .iter()
            .find(|&&c| self.nodes[c].name == name)
        {
            return existing;
        }
        let idx = self.nodes.len();
        self.nodes.push(ProfileNode::new(name, Some(parent)));
        self.nodes[parent].children.push(idx);
        idx
    }

    /// `cProfileManager::StartProfile`. PARITY: if `name` matches the current
    /// node's own name (recursive re-entry) we stay put and only bump the
    /// recursion counter; otherwise descend to the named child.
    pub fn enter(&mut self, name: &'static str, now: Instant) {
        if !self.enabled {
            return;
        }
        if self.nodes[self.current].name != name {
            self.current = self.find_or_create_child(self.current, name);
        }
        let node = &mut self.nodes[self.current];
        node.total_calls += 1;
        if node.recursion == 0 {
            node.start = Some(now);
        }
        node.recursion += 1;
    }

    /// `cProfileManager::StopProfile` + `cProfileNode::Return`: decrement the
    /// recursion counter, accumulate elapsed time on the 1→0 edge, and pop to
    /// the parent when the outermost scope closes.
    pub fn leave(&mut self, now: Instant) {
        if !self.enabled {
            return;
        }
        let node = &mut self.nodes[self.current];
        if node.recursion > 0 {
            node.recursion -= 1;
        }
        if node.recursion == 0 {
            if let Some(start) = node.start.take() {
                node.total_time += now.saturating_duration_since(start);
            }
            if let Some(parent) = node.parent {
                self.current = parent;
            }
        }
    }

    /// The child nodes of arena node `node`, as `(index, node)` pairs, in
    /// first-entered order. Pass [`ROOT`] for the top-level scopes.
    pub fn children(&self, node: usize) -> impl Iterator<Item = (usize, &ProfileNode)> {
        self.nodes[node].children.iter().map(|&i| (i, &self.nodes[i]))
    }

    /// Formatted, fixed-column tree dump — the `Profile output.txt` body
    /// (PARITY of `cMainWindow::PrintProfileInformation`,
    /// cMainWindow.cpp:1076-1123: index, name, calls, time (s), time/call,
    /// percentage-of-total, percentage-of-parent, plus a synthetic
    /// "Unlogged" row under any node that has children). Times are seconds
    /// from `Instant` rather than the original's `ticks × 0.001`.
    pub fn dump(&self) -> String {
        self.dump_at(Instant::now())
    }

    fn dump_at(&self, now: Instant) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "{:>10}{:<40}{:<8}{:<12}{:<15}{:<12}{:<20}",
            "", "Name", "Calls", "Time (s)", "Time per call", "Percent", "Parent percent"
        );
        let elapsed = self.time_elapsed_at(now).as_secs_f64();
        self.dump_children(&mut out, ROOT, elapsed.max(f64::MIN_POSITIVE), elapsed, 1);
        out
    }

    fn dump_children(
        &self,
        out: &mut String,
        parent: usize,
        elapsed: f64,
        parent_time: f64,
        level: usize,
    ) {
        let children = &self.nodes[parent].children;
        if children.is_empty() {
            return;
        }
        let indent = " ".repeat(level);
        let idx_width = 10usize.saturating_sub(level).max(1);
        let mut sum = Duration::ZERO;
        let parent_time = parent_time.max(f64::MIN_POSITIVE);
        for (i, &child) in children.iter().enumerate() {
            let node = &self.nodes[child];
            sum += node.total_time;
            let t = node.total_time.as_secs_f64();
            let per_call = if node.total_calls > 0 {
                t / node.total_calls as f64
            } else {
                0.0
            };
            let _ = writeln!(
                out,
                "{indent}{i:<idx_width$}{:<40}{:<8}{:<12.6}{:<15.6}{:>10.5}%{:>18.5}%",
                node.name,
                node.total_calls,
                t,
                per_call,
                t / elapsed * 100.0,
                t / parent_time * 100.0,
            );
            self.dump_children(out, child, elapsed, t, level + 1);
        }
        // "Unlogged" = parent time not attributed to any child scope.
        let unlogged = parent_time - sum.as_secs_f64();
        let i = children.len();
        let _ = writeln!(
            out,
            "{indent}{i:<idx_width$}{:<40}{:<8}{:<12.6}{:<15}{:>10.5}%{:>18.5}%",
            "Unlogged",
            "-",
            unlogged,
            "-",
            unlogged / elapsed * 100.0,
            unlogged / parent_time * 100.0,
        );
    }
}

impl Default for ProfileManager {
    fn default() -> Self {
        Self::new(true)
    }
}

thread_local! {
    /// The manager `profile_scope!` records into. The game loop is
    /// single-threaded, so a `thread_local` stands in for the original's
    /// `g_ProfileManager` global. Starts disabled until the P key.
    static PROFILER: RefCell<ProfileManager> = RefCell::new(ProfileManager::new(false));
}

/// Run `f` against the process-wide profile manager.
pub fn with_profiler<R>(f: impl FnOnce(&mut ProfileManager) -> R) -> R {
    PROFILER.with(|p| f(&mut p.borrow_mut()))
}

/// The exit-dump destination (`Profile output.txt`). The original wrote it
/// to the install dir, which is read-only under UAC.
pub fn exit_dump_path() -> std::path::PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("operation-purge")
        .join("Profile output.txt")
}

/// Write the global profile tree to `Profile output.txt` on exit (PARITY:
/// `cMainWindow` dumped the profile on shutdown). Writes nothing if the
/// profiler never recorded a scope.
pub fn write_exit_dump() -> std::io::Result<()> {
    let Some(text) = with_profiler(|p| p.children(ROOT).next().is_some().then(|| p.dump()))
    else {
        return Ok(());
    };
    let path = exit_dump_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, text)
}

/// RAII scope guard — the Rust analog of the `PROFILE("name")` macro used
/// at ~25 sites in `cOperationPurge.cpp`. Enters a scope in the global
/// [`PROFILER`] on construction and closes it on drop.
pub struct ProfileScope {
    _priv: (),
}

impl ProfileScope {
    pub fn enter(name: &'static str) -> Self {
        with_profiler(|p| p.enter(name, Instant::now()));
        Self { _priv: () }
    }
}

impl Drop for ProfileScope {
    fn drop(&mut self) {
        with_profiler(|p| p.leave(Instant::now()));
    }
}

/// Enters a named profile scope for the remainder of the current block
/// (the RAII `PROFILE(name)` twin). Records into the global manager; a no-op
/// while profiling is disabled.
#[macro_export]
macro_rules! profile_scope {
    ($name:expr) => {
        let _profile_scope = $crate::profiler::ProfileScope::enter($name);
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(base: Instant, ms: u64) -> Instant {
        base + Duration::from_millis(ms)
    }

    fn summary(m: &ProfileManager, node: usize) -> Vec<(&'static str, u32, Duration)> {
        m.children(node)
            .map(|(_, n)| (n.name, n.total_calls, n.total_time))
            .collect()
    }

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn nested_scopes_build_a_tree_with_injected_durations() {
        let base = Instant::now();
        let mut m = ProfileManager::new(true);
        m.enter("frame", at(base, 0));
        m.enter("sim", at(base, 1));
        m.leave(at(base, 6));
        m.enter("render", at(base, 6));
        m.leave(at(base, 16));
        m.leave(at(base, 20));

        assert_eq!(summary(&m, ROOT), [("frame", 1, ms(20))]);
        let (frame, _) = m.children(ROOT).next().unwrap();
        assert_eq!(summary(&m, frame), [("sim", 1, ms(5)), ("render", 1, ms(10))]);
    }

    #[test]
    fn recursive_reentry_times_once_and_counts_calls() {
        let base = Instant::now();
        let mut m = ProfileManager::new(true);
        m.enter("recurse", at(base, 0));
        m.enter("recurse", at(base, 2));
        m.enter("recurse", at(base, 4));
        m.leave(at(base, 6));
        m.leave(at(base, 8));
        m.leave(at(base, 10));
        assert_eq!(summary(&m, ROOT), [("recurse", 3, ms(10))]);
    }

    #[test]
    fn repeated_sibling_scopes_accumulate_into_one_node() {
        let base = Instant::now();
        let mut m = ProfileManager::new(true);
        for i in 0..3 {
            m.enter("tick", at(base, i * 10));
            m.leave(at(base, i * 10 + 4));
        }
        assert_eq!(summary(&m, ROOT), [("tick", 3, ms(12))]);
    }

    #[test]
    fn disabled_manager_records_nothing() {
        let base = Instant::now();
        let mut m = ProfileManager::new(false);
        m.enter("frame", at(base, 0));
        m.leave(at(base, 10));
        assert!(m.children(ROOT).next().is_none());
    }

    #[test]
    fn session_clock_only_counts_enabled_time() {
        let base = Instant::now();
        let mut m = ProfileManager::new(false);
        m.set_enabled_at(true, at(base, 100));
        m.set_enabled_at(false, at(base, 130));
        m.set_enabled_at(true, at(base, 500));
        assert_eq!(m.time_elapsed_at(at(base, 510)), ms(40));
    }

    #[test]
    fn dump_has_header_names_and_unlogged_row() {
        let base = Instant::now();
        let mut m = ProfileManager::new(false);
        m.set_enabled_at(true, base);
        m.enter("frame", at(base, 0));
        m.enter("sim", at(base, 1));
        m.leave(at(base, 6));
        m.leave(at(base, 20));

        let dump = m.dump_at(at(base, 40));
        assert!(dump.contains("Parent percent"));
        assert!(dump.contains("frame"));
        assert!(dump.contains("sim"));
        assert!(dump.contains("Unlogged"));
        // frame = 20ms of 40ms enabled.
        assert!(dump.contains("50.00000%"));
    }

    #[test]
    fn global_scope_macro_records_into_the_thread_local() {
        with_profiler(|p| p.set_enabled(true));
        {
            profile_scope!("outer");
            {
                profile_scope!("inner");
            }
        }
        let names = with_profiler(|p| {
            let (outer, node) = p.children(ROOT).next().unwrap();
            let names = (node.name, p.children(outer).next().map(|(_, n)| n.name));
            p.set_enabled(false);
            names
        });
        assert_eq!(names, ("outer", Some("inner")));
    }
}
