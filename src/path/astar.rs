//! A* core: binary-heap open list (`cPriorityQueue`) + the search loop
//! (`cPathFinder::FindPath`). Costs are Euclidean with an
//! admissible heuristic, so paths are shortest.
//!
//! This module ports the room-graph-agnostic core only: [`AStar::search`]
//! takes caller-supplied `neighbors`/`is_goal` closures (`cPathFinder`'s
//! `GetNeighbors`/`IsGoalPoint` virtuals live in `path/mod.rs`, with
//! `cHighLevelPathFinder`/`cLowLevelPathFinder`).

use super::nodes::NodeArena;
use crate::math::equals;
use glam::Vec2;

/// `cPathFinder::GetNodeDistance` (`cPathFinder.cpp:320-323`): `g` cost.
///
/// FIX: the C++ used `g = squared distance` and `h = 3.0 * squared distance`
/// (`cPathFinder.cpp:14,320-328`), a greedy search that took needlessly long
/// routes.
fn node_distance(a: Vec2, b: Vec2) -> f32 {
    a.distance(b)
}

/// `cPathFinder::GetNodeHeuristic` (`cPathFinder.cpp:325-328`): `h`
/// estimate, never more than the true remaining cost.
fn node_heuristic(a: Vec2, b: Vec2) -> f32 {
    node_distance(a, b)
}

fn total_cost(arena: &NodeArena, idx: usize) -> f32 {
    let n = arena.get(idx);
    n.cost_from_start + n.cost_to_goal
}

/// Binary-heap open list, replacing `cPriorityQueue` (`cPriorityQueue.h/.cpp`).
/// Heap elements are [`NodeArena`] indices; ordering is by `g+h`
/// ("NodeTotalGreater" in the C++, used as a `std::push_heap`/`pop_heap`
/// comparator that makes position 0 the *smallest* total — i.e. a min-heap).
#[derive(Default)]
struct OpenList {
    heap: Vec<usize>,
}

impl OpenList {
    fn new() -> Self {
        Self { heap: Vec::new() }
    }

    fn is_empty(&self) -> bool {
        self.heap.is_empty()
    }

    /// `cPriorityQueue::Clear` (`cPriorityQueue.cpp:69-72`).
    fn clear(&mut self) {
        self.heap.clear();
    }

    /// `cPriorityQueue::Push` (`cPriorityQueue.cpp:41-49`): append then
    /// sift up (`std::push_heap` over the whole range).
    fn push(&mut self, arena: &NodeArena, idx: usize) {
        self.heap.push(idx);
        let last = self.heap.len() - 1;
        Self::sift_up(&mut self.heap, last, arena);
    }

    /// `cPriorityQueue::Pop` (`cPriorityQueue.cpp:19-35`): `std::pop_heap`
    /// (swap front/back, sift down the new front) then `pop_back`.
    fn pop(&mut self, arena: &NodeArena) -> usize {
        let front = self.heap[0];
        let last = self.heap.len() - 1;
        self.heap.swap(0, last);
        self.heap.pop();
        if !self.heap.is_empty() {
            Self::sift_down(&mut self.heap, 0, arena);
        }
        front
    }

    /// `cPriorityQueue::UpdateNode` (`cPriorityQueue.cpp:51-66`): PARITY —
    /// **O(n)** linear scan for the node matching `idx`'s location by
    /// epsilon equality (`Equals`, not index/pointer identity — mirrors
    /// the C++ comparing `pt2Location`), then a `std::push_heap`-style
    /// sift-up restricted to `[0, found_index]` (i.e. the found element is
    /// treated as freshly appended to a heap of that shorter length, and
    /// only sifted upward — never downward). This is the exact quirky
    /// "O(n + log n)" algorithm the comment in `cPriorityQueue.cpp:51`
    /// documents, not a full correct reheapify.
    fn update_node(&mut self, arena: &NodeArena, idx: usize) {
        let target = arena.get(idx).location;
        if let Some(pos) = self
            .heap
            .iter()
            .position(|&i| equals(arena.get(i).location.x, target.x) && equals(arena.get(i).location.y, target.y))
        {
            Self::sift_up(&mut self.heap[..=pos], pos, arena);
        }
    }

    fn sift_up(heap: &mut [usize], mut i: usize, arena: &NodeArena) {
        while i > 0 {
            let parent = (i - 1) / 2;
            if total_cost(arena, heap[parent]) > total_cost(arena, heap[i]) {
                heap.swap(parent, i);
                i = parent;
            } else {
                break;
            }
        }
    }

    fn sift_down(heap: &mut [usize], mut i: usize, arena: &NodeArena) {
        let n = heap.len();
        loop {
            let l = 2 * i + 1;
            let r = 2 * i + 2;
            let mut smallest = i;
            if l < n && total_cost(arena, heap[l]) < total_cost(arena, heap[smallest]) {
                smallest = l;
            }
            if r < n && total_cost(arena, heap[r]) < total_cost(arena, heap[smallest]) {
                smallest = r;
            }
            if smallest == i {
                break;
            }
            heap.swap(i, smallest);
            i = smallest;
        }
    }
}

/// Weighted A* search core, replacing `cPathFinder`'s node-list +
/// open-list + `FindPath` loop. Owns its [`NodeArena`] and open list so
/// repeated searches reuse the same pool (mirrors the C++'s `static`
/// `s_NodeList`/`s_OpenList`, `cPathFinder.cpp:17-18`).
pub struct AStar {
    arena: NodeArena,
    open: OpenList,
}

impl AStar {
    pub fn new() -> Self {
        Self {
            arena: NodeArena::new(),
            open: OpenList::new(),
        }
    }

    /// `cPathFinder::FindPath` (`cPathFinder.cpp:206-303`), generalized
    /// over caller-supplied `is_goal`/`neighbors`. Returns the
    /// waypoint sequence from `start` to the discovered goal point
    /// (inclusive of both ends), or `None` if the open list empties
    /// without reaching a goal.
    pub fn search(
        &mut self,
        start: Vec2,
        goal: Vec2,
        mut is_goal: impl FnMut(Vec2) -> bool,
        mut neighbors: impl FnMut(Vec2) -> Vec<Vec2>,
    ) -> Option<Vec<Vec2>> {
        self.arena.reset();
        self.open.clear();

        let start_idx = self.arena.get_node(start);
        {
            let n = self.arena.get_mut(start_idx);
            n.on_open = true;
            n.on_closed = false;
            n.parent = None;
            n.cost_from_start = 0.0;
            n.cost_to_goal = node_heuristic(start, goal);
        }
        self.open.push(&self.arena, start_idx);

        while !self.open.is_empty() {
            let best_idx = self.open.pop(&self.arena);
            let best_loc = self.arena.get(best_idx).location;

            if is_goal(best_loc) {
                return Some(self.reconstruct_path(best_idx));
            }

            let parent_loc = self.arena.get(best_idx).parent.map(|p| self.arena.get(p).location);
            let best_cost_from_start = self.arena.get(best_idx).cost_from_start;

            for neighbor_loc in neighbors(best_loc) {
                if let Some(pl) = parent_loc {
                    if equals(pl.x, neighbor_loc.x) && equals(pl.y, neighbor_loc.y) {
                        // "pt2Neighbor is the node just searched" (cPathFinder.cpp:281-285).
                        continue;
                    }
                }

                let new_g = best_cost_from_start + node_distance(best_loc, neighbor_loc);
                let new_h = node_heuristic(neighbor_loc, goal);

                let actual_idx = self.arena.get_node(neighbor_loc);
                let (on_open, on_closed, existing_g) = {
                    let n = self.arena.get(actual_idx);
                    (n.on_open, n.on_closed, n.cost_from_start)
                };

                // PARITY: `cPathFinder.cpp:294` — only skip if the node is
                // already open-or-closed AND its existing cost is already
                // at least as good; ties re-relax (matches `<=`, not `<`).
                if !((on_open || on_closed) && existing_g <= new_g) {
                    {
                        let n = self.arena.get_mut(actual_idx);
                        n.on_closed = false;
                        n.parent = Some(best_idx);
                        n.cost_from_start = new_g;
                        n.cost_to_goal = new_h;
                    }

                    if on_open {
                        self.open.update_node(&self.arena, actual_idx);
                    } else {
                        self.open.push(&self.arena, actual_idx);
                        self.arena.get_mut(actual_idx).on_open = true;
                    }
                }
            }

            self.arena.get_mut(best_idx).on_closed = true;
        }

        None
    }

    fn reconstruct_path(&self, goal_idx: usize) -> Vec<Vec2> {
        // PARITY: `cPathFinder.cpp:225-237` — walk parents back to the
        // start (whose `pParent == NULL`), collecting in reverse, then
        // reverse once.
        let mut reversed = Vec::new();
        let mut cur = goal_idx;
        loop {
            reversed.push(self.arena.get(cur).location);
            match self.arena.get(cur).parent {
                Some(p) => cur = p,
                None => break,
            }
        }
        reversed.reverse();
        reversed
    }
}

impl Default for AStar {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Simple 5x5 grid graph with 4-directional neighbors and a blocked
    /// column, used to exercise the generic search core without any
    /// room-graph specifics.
    fn grid_neighbors(blocked: &[(i32, i32)]) -> impl Fn(Vec2) -> Vec<Vec2> + '_ {
        move |p: Vec2| {
            let (x, y) = (p.x as i32, p.y as i32);
            let candidates = [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)];
            candidates
                .into_iter()
                .filter(|c| !blocked.contains(c))
                .filter(|&(cx, cy)| (0..5).contains(&cx) && (0..5).contains(&cy))
                .map(|(cx, cy)| Vec2::new(cx as f32, cy as f32))
                .collect()
        }
    }

    #[test]
    fn finds_direct_path_on_open_grid() {
        let mut astar = AStar::new();
        let start = Vec2::new(0.0, 0.0);
        let goal = Vec2::new(3.0, 0.0);
        let path = astar
            .search(start, goal, |p| p == goal, grid_neighbors(&[]))
            .expect("path must be found on an open grid");
        assert_eq!(path.first().copied(), Some(start));
        assert_eq!(path.last().copied(), Some(goal));
        // Manhattan-shortest on an open 4-connected grid is 3 steps -> 4 waypoints.
        assert_eq!(path.len(), 4);
    }

    #[test]
    fn routes_around_a_wall() {
        let mut astar = AStar::new();
        // Block the whole column x=2 except at y=4, forcing a detour.
        let blocked: Vec<(i32, i32)> = (0..4).map(|y| (2, y)).collect();
        let start = Vec2::new(0.0, 0.0);
        let goal = Vec2::new(4.0, 0.0);
        let path = astar
            .search(start, goal, |p| p == goal, grid_neighbors(&blocked))
            .expect("path must route around the wall");
        assert_eq!(path.first().copied(), Some(start));
        assert_eq!(path.last().copied(), Some(goal));
        // Must pass through the only opening at (2,4).
        assert!(path.iter().any(|&p| p == Vec2::new(2.0, 4.0)));
    }

    #[test]
    fn returns_none_when_goal_is_unreachable() {
        let mut astar = AStar::new();
        // Fully wall off column x=2.
        let blocked: Vec<(i32, i32)> = (0..5).map(|y| (2, y)).collect();
        let start = Vec2::new(0.0, 0.0);
        let goal = Vec2::new(4.0, 0.0);
        let path = astar.search(start, goal, |p| p == goal, grid_neighbors(&blocked));
        assert!(path.is_none());
    }
}
