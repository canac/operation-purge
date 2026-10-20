//! Hierarchical scope profiler, replacing `Profile.h`/`cProfileManager`
//!.
//!
//! Shipped, user-visible feature (P/[/] keybindings toggle
//! running/visible/interaction; on-screen tree pane; 250 ms refresh; exit
//! dump to `Profile output.txt`) — not dev-only tooling. The original
//! measured raw `rdtsc` ticks over `QueryPerformanceFrequency` (a unit
//! mismatch that "worked" only because the overlay shows ratios); the
//! port uses `std::time::Instant`, a correct monotonic clock, everywhere
//! rdtsc appeared.
//!
//! Structure mirrors `Common/Profile.{h,cpp}` verbatim: a tree of named
//! nodes with a per-node recursion counter (`Call`/`Return`), a "current
//! node" cursor the `PROFILE(name)` scope walks, and a `cProfileIterator`
//! for read-out. The C++ used raw parent/child/sibling pointers; here the
//! tree is an index arena (`Vec<ProfileNode>`), which is the borrow-checker-
//! clean equivalent — node identity by `&'static str` value replaces the
//! original's static-string *pointer* comparison, a
//! correct substitute for the compiled-in scope names.

use std::cell::RefCell;
use std::fmt::Write as _;
use std::time::{Duration, Instant};

/// One node in the profile tree (`cProfileNode`). Children/parent are arena
/// indices into the owning [`ProfileManager`].
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

/// Global profiler state; the app owns one instance (replacing the
/// original's `g_ProfileManager` singleton). Node index 0 is
/// always the root.
pub struct ProfileManager {
    pub enabled: bool,
    nodes: Vec<ProfileNode>,
    current: usize,
    session_start: Instant,
    /// Test-only override for `time_elapsed()` so dump percentages are
    /// deterministic without a real wall-clock wait.
    elapsed_override: Option<Duration>,
}

impl ProfileManager {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            nodes: vec![ProfileNode::new("Root", None)],
            current: 0,
            session_start: Instant::now(),
            elapsed_override: None,
        }
    }

    /// `cProfileManager::SetProfilingEnabled` (toggled by the profiler-running
    /// keybinding).
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// `cProfileManager::Reset` — clears the whole tree, keeping only the
    /// root, and restarts the session clock.
    pub fn reset(&mut self) {
        self.nodes.truncate(1);
        self.nodes[0] = ProfileNode::new("Root", None);
        self.current = 0;
        self.session_start = Instant::now();
        self.elapsed_override = None;
    }

    /// Total profiling-session wall time (`GetTimeElapsed`).
    pub fn time_elapsed(&self) -> Duration {
        self.elapsed_override
            .unwrap_or_else(|| self.session_start.elapsed())
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

    /// Root node (read-only) — the iterator's entry point.
    pub fn root(&self) -> &ProfileNode {
        &self.nodes[0]
    }

    /// An iterator positioned over the root's children (`cProfileIterator`).
    pub fn iter(&self) -> ProfileIterator<'_> {
        ProfileIterator {
            mgr: self,
            parent: 0,
            child_pos: 0,
        }
    }

    fn children_of(&self, parent: usize) -> &[usize] {
        &self.nodes[parent].children
    }

    /// Formatted, fixed-column tree dump — the `Profile output.txt` body
    /// (PARITY of `cMainWindow::PrintProfileInformation`,
    /// cMainWindow.cpp:1076-1123: index, name, calls, time (s), time/call,
    /// percentage-of-total, percentage-of-parent, plus a synthetic
    /// "Unlogged" row under any node that has children). Times are seconds
    /// from `Instant` rather than the original's `ticks × 0.001`.
    pub fn dump(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "{:>10}{:<40}{:<8}{:<12}{:<15}{:<12}{:<20}",
            "", "Name", "Calls", "Time (s)", "Time per call", "Percent", "Parent percent"
        );
        let elapsed = self.time_elapsed().as_secs_f64().max(f64::MIN_POSITIVE);
        self.dump_children(&mut out, 0, elapsed, self.time_elapsed().as_secs_f64(), 1);
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
        let children = self.children_of(parent);
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

/// Read-only navigator over a [`ProfileManager`] tree (`cProfileIterator`):
/// walks the children of a "current parent", can descend into a child, and
/// ascend back to the grandparent.
pub struct ProfileIterator<'a> {
    mgr: &'a ProfileManager,
    parent: usize,
    child_pos: usize,
}

impl<'a> ProfileIterator<'a> {
    pub fn first(&mut self) {
        self.child_pos = 0;
    }

    pub fn next(&mut self) {
        self.child_pos += 1;
    }

    pub fn is_done(&self) -> bool {
        self.child_pos >= self.mgr.children_of(self.parent).len()
    }

    fn current_index(&self) -> Option<usize> {
        self.mgr.children_of(self.parent).get(self.child_pos).copied()
    }

    pub fn current_name(&self) -> Option<&'static str> {
        self.current_index().map(|i| self.mgr.nodes[i].name)
    }

    pub fn current_total_calls(&self) -> Option<u32> {
        self.current_index().map(|i| self.mgr.nodes[i].total_calls)
    }

    pub fn current_total_time(&self) -> Option<Duration> {
        self.current_index().map(|i| self.mgr.nodes[i].total_time)
    }

    /// Make the current child the new parent. Returns `true` if it has any
    /// children (`cProfileIterator::EnterChild`).
    pub fn enter_child(&mut self) -> bool {
        if let Some(i) = self.current_index() {
            self.parent = i;
            self.child_pos = 0;
            !self.mgr.children_of(i).is_empty()
        } else {
            false
        }
    }

    /// Make the current parent's parent the new parent
    /// (`cProfileIterator::EnterParent`).
    pub fn enter_parent(&mut self) {
        if let Some(gp) = self.mgr.nodes[self.parent].parent {
            self.parent = gp;
            self.child_pos = 0;
        }
    }
}

thread_local! {
    /// Process-wide manager the `profile_scope!` macro records into. The game
    /// loop is single-threaded, so a `thread_local` is the faithful analog of
    /// the original `g_ProfileManager` global. Enabled state
    /// starts `false` — the "profiler running" keybinding turns it on.
    static PROFILER: RefCell<ProfileManager> = RefCell::new(ProfileManager::new(false));
}

/// Run `f` against the process-wide profile manager.
pub fn with_profiler<R>(f: impl FnOnce(&mut ProfileManager) -> R) -> R {
    PROFILER.with(|p| f(&mut p.borrow_mut()))
}

/// Enable/disable the global profiler (the profiler-running keybinding).
pub fn set_enabled(enabled: bool) {
    with_profiler(|p| p.set_enabled(enabled));
}

/// Clear the global profile tree and restart its session clock.
pub fn reset() {
    with_profiler(|p| p.reset());
}

/// Snapshot of the global profile tree in `Profile output.txt` format.
pub fn dump() -> String {
    with_profiler(|p| p.dump())
}

/// The exit-dump destination (`Profile output.txt`). The original wrote it
/// to the install dir; the port moves it to `data_local_dir` alongside
/// `Logfile.txt` (broken-under-UAC deviation).
pub fn exit_dump_path() -> std::path::PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("operation-purge")
        .join("Profile output.txt")
}

/// Write the global profile tree to `Profile output.txt` on exit
/// (PARITY: `cMainWindow` dumped the profile on shutdown).
/// Only writes when the profiler has actually recorded something (an empty
/// run leaves no file, mirroring the lazy `Logfile.txt`).
pub fn write_exit_dump() -> std::io::Result<()> {
    let has_data = with_profiler(|p| !p.root().children.is_empty());
    if !has_data {
        return Ok(());
    }
    let path = exit_dump_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, dump())
}

/// RAII scope guard — the Rust analog of the `PROFILE("name")` macro used
/// at ~25 sites in `cOperationPurge.cpp`. Enters a scope in
/// the global [`PROFILER`] on construction and closes it on drop.
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

    #[test]
    fn nested_scopes_build_a_tree_with_injected_durations() {
        let base = Instant::now();
        let mut m = ProfileManager::new(true);
        m.enter("frame", at(base, 0));
        m.enter("sim", at(base, 1));
        m.leave(at(base, 6)); // sim = 5ms
        m.enter("render", at(base, 6));
        m.leave(at(base, 16)); // render = 10ms
        m.leave(at(base, 20)); // frame = 20ms

        let mut it = m.iter();
        it.first();
        assert_eq!(it.current_name(), Some("frame"));
        assert_eq!(it.current_total_calls(), Some(1));
        assert_eq!(it.current_total_time(), Some(Duration::from_millis(20)));

        assert!(it.enter_child());
        it.first();
        assert_eq!(it.current_name(), Some("sim"));
        assert_eq!(it.current_total_time(), Some(Duration::from_millis(5)));
        it.next();
        assert_eq!(it.current_name(), Some("render"));
        assert_eq!(it.current_total_time(), Some(Duration::from_millis(10)));
        it.next();
        assert!(it.is_done());
    }

    #[test]
    fn recursive_reentry_times_once_and_counts_calls() {
        let base = Instant::now();
        let mut m = ProfileManager::new(true);
        // Enter "recurse" three times without leaving (recursion), then
        // unwind. PARITY: only the outermost close accumulates time.
        m.enter("recurse", at(base, 0));
        m.enter("recurse", at(base, 2));
        m.enter("recurse", at(base, 4));
        m.leave(at(base, 6));
        m.leave(at(base, 8));
        m.leave(at(base, 10)); // outer close: total = 10ms
        let mut it = m.iter();
        it.first();
        assert_eq!(it.current_name(), Some("recurse"));
        assert_eq!(it.current_total_calls(), Some(3));
        assert_eq!(it.current_total_time(), Some(Duration::from_millis(10)));
    }

    #[test]
    fn repeated_sibling_scopes_accumulate_into_one_node() {
        let base = Instant::now();
        let mut m = ProfileManager::new(true);
        for i in 0..3 {
            m.enter("tick", at(base, i * 10));
            m.leave(at(base, i * 10 + 4));
        }
        let mut it = m.iter();
        it.first();
        assert_eq!(it.current_total_calls(), Some(3));
        assert_eq!(it.current_total_time(), Some(Duration::from_millis(12)));
    }

    #[test]
    fn disabled_manager_records_nothing() {
        let base = Instant::now();
        let mut m = ProfileManager::new(false);
        m.enter("frame", at(base, 0));
        m.leave(at(base, 10));
        assert!(m.iter().current_name().is_none());
    }

    #[test]
    fn dump_has_header_names_and_unlogged_row() {
        let base = Instant::now();
        let mut m = ProfileManager::new(true);
        m.enter("frame", at(base, 0));
        m.enter("sim", at(base, 1));
        m.leave(at(base, 6));
        m.leave(at(base, 20));
        m.elapsed_override = Some(Duration::from_millis(40));

        let dump = m.dump();
        assert!(dump.contains("Name"));
        assert!(dump.contains("Parent percent"));
        assert!(dump.contains("frame"));
        assert!(dump.contains("sim"));
        // "frame" has a child ("sim") ⇒ an Unlogged row is emitted for it.
        assert!(dump.contains("Unlogged"));
        // frame = 20ms of 40ms elapsed ⇒ 50%.
        assert!(dump.contains("50.00000%"));
    }

    #[test]
    fn reset_clears_the_tree() {
        let base = Instant::now();
        let mut m = ProfileManager::new(true);
        m.enter("a", at(base, 0));
        m.leave(at(base, 1));
        m.reset();
        assert!(m.iter().current_name().is_none());
        assert_eq!(m.root().name, "Root");
    }

    #[test]
    fn global_scope_macro_records_into_the_thread_local() {
        reset();
        set_enabled(true);
        {
            profile_scope!("outer");
            {
                profile_scope!("inner");
            }
        }
        let names = with_profiler(|p| {
            let mut it = p.iter();
            it.first();
            let outer = it.current_name();
            it.enter_child();
            it.first();
            (outer, it.current_name())
        });
        assert_eq!(names.0, Some("outer"));
        assert_eq!(names.1, Some("inner"));
        // Leave the global disabled so unrelated tests aren't affected.
        set_enabled(false);
    }
}
