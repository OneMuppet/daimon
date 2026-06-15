//! Imagination — a learned forward model and **empowerment**.
//!
//! Two pieces of new ground:
//!
//! * **Forward model.** The agent learns its own dynamics: "from here, if I step
//!   east, where do I end up?" On an open grid that's trivial, but the world has
//!   structure (edges, walls, things that block), and the agent *discovers* it —
//!   learning, for instance, that a move into a wall leaves it where it was. A
//!   fresh agent assumes every step succeeds; an experienced one knows better.
//!
//! * **Empowerment** (Klyubin et al.; Salge et al.). A principled,
//!   information-theoretic intrinsic drive: the agent values being in states from
//!   which it can reach the *most distinct futures* — i.e. where it has the most
//!   control. Formally empowerment is the channel capacity from an action
//!   sequence to the resulting state, `max_p I(A→S')`; we use the standard
//!   tractable lower bound — the **count of distinct states reachable in `k`
//!   steps** under the *learned* model (log of which is an empowerment bound).
//!   An agent that maximises this flees dead-ends and seeks open ground with no
//!   one telling it to. It is, arguably, the mathematics of wanting to stay free.

use daimon_core::{Dir, Pos};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::collections::BTreeSet;

fn dir_code(d: Dir) -> u8 {
    match d {
        Dir::North => 0,
        Dir::South => 1,
        Dir::East => 2,
        Dir::West => 3,
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ForwardModel {
    /// Learned transition: (x, y, dir) -> resulting (x, y).
    #[serde(with = "daimon_core::serdeutil::vecmap")]
    trans: BTreeMap<(i32, i32, u8), (i32, i32)>,
    /// Prediction bookkeeping for measuring how well it has learned.
    pub predictions: u32,
    pub hits: u32,
}

impl ForwardModel {
    /// What the model believes a step does (None = never tried from here).
    pub fn predict(&self, p: Pos, d: Dir) -> Option<Pos> {
        self.trans
            .get(&(p.x, p.y, dir_code(d)))
            .map(|&(x, y)| Pos::new(x, y))
    }

    /// Observe an actual transition; score the prior prediction, then learn it.
    pub fn learn(&mut self, from: Pos, d: Dir, to: Pos) {
        if let Some(pred) = self.predict(from, d) {
            self.predictions += 1;
            if pred == to {
                self.hits += 1;
            }
        }
        self.trans.insert((from.x, from.y, dir_code(d)), (to.x, to.y));
    }

    /// Best guess at the result of a step: learned if known, else assume the
    /// step just succeeds (the optimistic default of an inexperienced agent).
    fn step_belief(&self, p: Pos, d: Dir, w: i32, h: i32) -> Pos {
        self.predict(p, d).unwrap_or_else(|| {
            let np = p.step(d);
            Pos::new(np.x.clamp(0, w - 1), np.y.clamp(0, h - 1))
        })
    }

    /// Empowerment of a state: how many distinct cells are reachable within `k`
    /// steps under the learned model. More reachable futures = more control.
    pub fn empowerment(&self, start: Pos, k: u8, w: i32, h: i32) -> usize {
        let mut seen: BTreeSet<(i32, i32)> = BTreeSet::new();
        seen.insert((start.x, start.y));
        let mut frontier = vec![start];
        for _ in 0..k {
            let mut next = Vec::new();
            for &p in &frontier {
                for d in Dir::ALL {
                    let np = self.step_belief(p, d, w, h);
                    if seen.insert((np.x, np.y)) {
                        next.push(np);
                    }
                }
            }
            if next.is_empty() {
                break;
            }
            frontier = next;
        }
        seen.len()
    }

    /// Whether the model has learned that a step from `p` in `d` is blocked.
    pub fn known_blocked(&self, p: Pos, d: Dir) -> bool {
        self.predict(p, d) == Some(p)
    }

    /// IMAGINATION: search the *learned* model for a route from `from` to a cell
    /// adjacent to `target`, returning the first step. Breadth-first over the
    /// agent's mental map — it plans a path around walls it has discovered
    /// instead of walking face-first into them. Unknown cells are assumed
    /// traversable (optimistic), so the agent will probe, learn, and re-route.
    pub fn plan_to(&self, from: Pos, target: Pos, w: i32, h: i32) -> Option<Dir> {
        use std::collections::{BTreeMap, VecDeque};
        if from.manhattan(target) <= 1 {
            return None;
        }
        let mut came: BTreeMap<(i32, i32), Dir> = BTreeMap::new();
        let mut q: VecDeque<Pos> = VecDeque::new();
        q.push_back(from);
        came.insert((from.x, from.y), Dir::North); // sentinel; overwritten for real nodes
        let mut nodes = 0;
        while let Some(p) = q.pop_front() {
            nodes += 1;
            if nodes > 4000 {
                break;
            }
            for d in Dir::ALL {
                let np = self.step_belief(p, d, w, h);
                if np == p || came.contains_key(&(np.x, np.y)) {
                    continue;
                }
                // record the FIRST move that reaches np by threading back to start
                let first = if p == from { d } else { came[&(p.x, p.y)] };
                came.insert((np.x, np.y), first);
                if np.manhattan(target) <= 1 || np == target {
                    return Some(first);
                }
                q.push_back(np);
            }
        }
        None
    }

    /// The cardinal step that leads to the most empowered next cell (with a
    /// caller-supplied tie-break index for determinism). `None` if no move helps.
    pub fn most_empowering(&self, from: Pos, k: u8, w: i32, h: i32, tiebreak: usize) -> Option<Dir> {
        let mut best: Option<(Dir, usize)> = None;
        let dirs = Dir::ALL;
        for i in 0..4 {
            let d = dirs[(i + tiebreak) % 4];
            let np = self.step_belief(from, d, w, h);
            if np == from {
                continue; // a move that does nothing can't help
            }
            let e = self.empowerment(np, k, w, h);
            if best.map(|(_, be)| e > be).unwrap_or(true) {
                best = Some((d, e));
            }
        }
        best.map(|(d, _)| d)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // a 6x6 grid with a 1-wide dead-end corridor on the left and an open room on
    // the right. blocked cells return the same cell for any entering move.
    fn walls() -> BTreeSet<(i32, i32)> {
        // wall off a pocket: make column x=2 a wall except one gap, leaving the
        // left as a cramped dead-end and the right as open room.
        let mut w = BTreeSet::new();
        for y in 0..6 {
            if y != 3 {
                w.insert((2, y));
            }
        }
        w
    }

    fn truth_step(p: Pos, d: Dir, walls: &BTreeSet<(i32, i32)>, w: i32, h: i32) -> Pos {
        let np = Pos::new((p.x + d.delta().0).clamp(0, w - 1), (p.y + d.delta().1).clamp(0, h - 1));
        if walls.contains(&(np.x, np.y)) {
            p
        } else {
            np
        }
    }

    #[test]
    fn learns_walls_and_open_is_more_empowered_than_deadend() {
        let walls = walls();
        let (w, h) = (6, 6);
        let mut fm = ForwardModel::default();
        // exhaustively experience every cell/dir transition (a thorough explorer)
        for x in 0..w {
            for y in 0..h {
                let p = Pos::new(x, y);
                if walls.contains(&(x, y)) {
                    continue;
                }
                for d in Dir::ALL {
                    fm.learn(p, d, truth_step(p, d, &walls, w, h));
                }
            }
        }
        // a cell deep in the left dead-end has fewer reachable futures than one in
        // the open right room.
        let deadend = fm.empowerment(Pos::new(0, 0), 4, w, h);
        let open = fm.empowerment(Pos::new(4, 3), 4, w, h);
        assert!(open > deadend, "open {open} should beat dead-end {deadend}");
    }
}
