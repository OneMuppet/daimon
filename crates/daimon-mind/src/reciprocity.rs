//! Reciprocity — how cooperation survives among self-interested agents.
//!
//! A believable society needs more than individuals who survive; it needs agents
//! who *cooperate without being suckers*. The iterated Prisoner's Dilemma is the
//! canonical model: each round both parties choose to cooperate (C) or defect (D);
//! mutual cooperation pays `R`, mutual defection `P`, and a defector exploiting a
//! cooperator gets `T > R > P > S`. A single round favours defection — yet in
//! *repeated* play, **reciprocity** (Trivers 1971) turns the tables. Axelrod's
//! 1981 tournaments found **tit-for-tat** — cooperate first, then mirror — beats
//! every cleverer scheme: it is nice, retaliatory, forgiving, and never exploited
//! for long. Nowak & Sigmund (1998) extended this to reputation. This is the
//! formal basis for NPCs that form alliances, hold grudges, and forgive — and the
//! resolution, in principle, of the individual-vs-social tension the believability
//! metric surfaced (a cooperator who isn't a sucker can bond *and* thrive).
//!
//! Self-contained and deterministic; a round-robin tournament reproduces Axelrod's
//! result — tit-for-tat is the robust winner.

use serde::{Deserialize, Serialize};

/// Standard iterated-PD payoffs: temptation > reward > punishment > sucker.
const T: f64 = 5.0;
const R: f64 = 3.0;
const P: f64 = 1.0;
const S: f64 = 0.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Strategy {
    /// Always cooperate — kind, but exploitable.
    AllC,
    /// Always defect — the lone-round optimum.
    AllD,
    /// Tit-for-tat: cooperate first, then mirror the opponent's last move.
    Tft,
    /// Grim: cooperate until the opponent defects once, then defect forever.
    Grim,
}

impl Strategy {
    /// Cooperate? Given my own and the opponent's move history so far.
    fn cooperate(self, _mine: &[bool], opp: &[bool]) -> bool {
        match self {
            Strategy::AllC => true,
            Strategy::AllD => false,
            Strategy::Tft => *opp.last().unwrap_or(&true), // cooperate first, then mirror
            Strategy::Grim => opp.iter().all(|&c| c),      // forgive nothing
        }
    }
}

/// Play `rounds` of the iterated PD; return each side's total payoff.
pub fn play(a: Strategy, b: Strategy, rounds: usize) -> (f64, f64) {
    let (mut ha, mut hb): (Vec<bool>, Vec<bool>) = (Vec::new(), Vec::new());
    let (mut sa, mut sb) = (0.0, 0.0);
    for _ in 0..rounds {
        let ca = a.cooperate(&ha, &hb);
        let cb = b.cooperate(&hb, &ha);
        let (pa, pb) = match (ca, cb) {
            (true, true) => (R, R),
            (false, false) => (P, P),
            (true, false) => (S, T),
            (false, true) => (T, S),
        };
        sa += pa;
        sb += pb;
        ha.push(ca);
        hb.push(cb);
    }
    (sa, sb)
}

/// A round-robin tournament: every strategy plays every strategy (incl. itself);
/// returns each strategy's total score. Reproduces Axelrod's finding.
pub fn tournament(field: &[Strategy], rounds: usize) -> Vec<(Strategy, f64)> {
    field
        .iter()
        .map(|&me| {
            let total: f64 = field.iter().map(|&other| play(me, other, rounds).0).sum();
            (me, total)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defector_exploits_a_pure_cooperator() {
        // one round of exploitation each — the sucker's trap.
        let (c, d) = play(Strategy::AllC, Strategy::AllD, 20);
        assert!(d > c, "AllD ({d}) should exploit AllC ({c})");
    }

    #[test]
    fn tit_for_tat_is_not_exploited() {
        // TFT loses only the first round to a defector, then mirrors — nearly even.
        let (tft, d) = play(Strategy::Tft, Strategy::AllD, 50);
        assert!(d - tft <= T - S + 1e-9, "TFT must not be exploited for long: tft={tft} d={d}");
    }

    #[test]
    fn reciprocity_wins_the_tournament() {
        // Axelrod's result: in a mixed field with defectors present, tit-for-tat is
        // the robust winner — it cooperates with cooperators and resists defectors.
        let field = [Strategy::AllC, Strategy::AllD, Strategy::Tft, Strategy::Grim];
        let scores = tournament(&field, 50);
        let get = |s: Strategy| scores.iter().find(|(x, _)| *x == s).unwrap().1;
        let tft = get(Strategy::Tft);
        let best = scores.iter().map(|(_, v)| *v).fold(f64::MIN, f64::max);
        assert!((tft - best).abs() < 1e-9, "TFT should top the field: {scores:?}");
        // and unconditional cooperation is strictly worse (it gets exploited).
        assert!(tft > get(Strategy::AllC), "reciprocity beats naive cooperation");
    }
}
