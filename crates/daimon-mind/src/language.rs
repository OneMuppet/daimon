//! Procedural narration — situational, varied first-person lines instead of one
//! templated sentence repeated forever.
//!
//! This is *not* language understanding (that's the LLM seam). It is a grammar
//! that composes a thought from concrete, current facts — the thing the agent is
//! heading for and *where* it is, who is nearby, what it just remembered, whether
//! it was startled — so two consecutive forages don't read identically and a
//! player can't find the bottom by watching the text. With an LLM in the
//! deliberator these lines become genuine; until then this keeps the surface
//! honest about the agent's actual state.

use crate::thought::Process;
use daimon_core::{Drive, GoalKind, Rng};
use std::fmt::Write;

/// Everything concrete the narrator can weave in.
pub struct Phrasing<'a> {
    pub name: &'a str,
    pub goal: &'a GoalKind,
    pub process: Process,
    pub drive: Drive,
    pub surprise: f32,
    /// Label of the thing the agent is acting on ("the humming stone", "spring-1").
    pub target: Option<&'a str>,
    /// Where that thing (or the agent) is.
    pub coord: Option<(i32, i32)>,
    /// A short recalled fact/episode the agent is leaning on.
    pub memory: Option<&'a str>,
    /// A known agent's name (for social lines).
    pub other: Option<&'a str>,
    /// Whether the agent is staying with a standing commitment.
    pub holding: bool,
}

fn pick<'a>(rng: &mut Rng, opts: &[&'a str]) -> &'a str {
    opts[rng.below(opts.len())]
}

/// Write a coordinate as `(x,y)` into `out`.
fn write_coord(out: &mut String, x: i32, y: i32) {
    let _ = write!(out, "({x},{y})");
}

/// Compose one inner-monologue line, writing into `out` (no per-call allocation).
pub fn decision_line(rng: &mut Rng, p: &Phrasing, out: &mut String) {
    let _ = write!(out, "[{}] ", p.name);

    if p.process == Process::Reflex {
        // instinct: terse, no deliberation
        out.push_str(pick(rng, &[
            "no time to think — I run.",
            "move, now.",
            "instinct takes over — go.",
            "every nerve says flee.",
        ]));
        return;
    }

    // The lead clause draws its RNG *first* (preserving the original order, where
    // `lead_clause` was evaluated before the holding/deliberate prefix), then the
    // prefix is drawn and spliced in ahead of the lead — so the bytes read
    // `[name] <prefix><lead>` while the draw order stays byte-identical.
    let prefix_at = out.len();
    lead_clause(rng, p, out);
    if p.holding {
        let prefix = pick(rng, &[
            "still on it: ",
            "seeing it through: ",
            "no, I started this — ",
            "staying with it. ",
        ]);
        out.insert_str(prefix_at, prefix);
    } else if p.process == Process::Deliberate {
        let prefix = pick(rng, &["(weighing it) ", "let me think — ", "alright: ", ""]);
        out.insert_str(prefix_at, prefix);
    }

    // optional grounding clause
    if p.surprise > 0.5 {
        out.push(' ');
        out.push_str(pick(rng, &[
            "— that caught me off guard.",
            "— didn't see that coming.",
            "— still rattled.",
            "— the world just shifted.",
        ]));
    } else if let Some(mem) = p.memory {
        if rng.chance(0.5) {
            out.push(' ');
            out.push_str(pick(rng, &["I remember", "last I knew,", "going on what I recall:"]));
            let _ = write!(out, " {mem}.");
        }
    }
    // grounding net: a thought should name something concrete. If nothing
    // numeric slipped in and we know where we're headed, say where.
    if let Some((x, y)) = p.coord {
        if !out.chars().any(|c| c.is_ascii_digit()) {
            let _ = write!(out, " (near ({x},{y}))");
        }
    }
}

fn lead_clause(rng: &mut Rng, p: &Phrasing, out: &mut String) {
    let tgt = p.target.unwrap_or("it");
    match p.goal {
        GoalKind::Forage => {
            out.push_str(pick(rng, &["I'm hungry", "my stomach's insisting", "hunger's gnawing", "I could eat"]));
            out.push_str(" — ");
            if p.target.is_some() {
                let _ = write!(out, "the {tgt} at ");
                if let Some((x, y)) = p.coord {
                    write_coord(out, x, y);
                }
                out.push_str(" will do");
            } else {
                out.push_str(pick(rng, &["time to find food", "I'll forage", "something to eat, somewhere"]));
            }
            out.push('.');
        }
        GoalKind::Hydrate => {
            out.push_str(pick(rng, &["throat's dry", "I'm parched", "thirst is winning", "need water"]));
            out.push_str(" — ");
            if p.target.is_some() {
                let _ = write!(out, "heading to the {tgt} at ");
                if let Some((x, y)) = p.coord {
                    write_coord(out, x, y);
                }
            } else {
                out.push_str(pick(rng, &["I'll find a spring", "water, wherever it is", "off to drink"]));
            }
            out.push('.');
        }
        GoalKind::Flee(_) => {
            out.push_str(pick(rng, &["the stalker's close", "that predator again", "not worth the risk", "danger"]));
            out.push_str(" — ");
            out.push_str(pick(rng, &["I'm putting distance between us", "backing away fast", "I'm gone", "anywhere but here"]));
            out.push('.');
        }
        GoalKind::Confront(_) => {
            out.push_str(pick(rng, &["enough running", "it keeps coming", "I won't be prey", "stand my ground"]));
            out.push_str(" — ");
            out.push_str(pick(rng, &["I face the stalker", "I move on it", "let's see who flinches", "together we can turn it"]));
            out.push('.');
        }
        GoalKind::Investigate(_) => match rng.below(4) {
            0 => {
                let _ = write!(out, "that {tgt} keeps pulling at me — closer look");
            }
            1 => {
                let _ = write!(out, "I still don't understand the {tgt}");
            }
            2 => {
                let _ = write!(out, "what *is* the {tgt}? I have to know");
            }
            _ => {
                let _ = write!(out, "the {tgt} at ");
                if let Some((x, y)) = p.coord {
                    write_coord(out, x, y);
                }
                out.push_str(" deserves study");
            }
        },
        GoalKind::Socialize(_) => {
            let who = p.other.unwrap_or("them");
            match rng.below(4) {
                0 => {
                    let _ = write!(out, "{who}'s nearby — I'd like to talk");
                }
                1 => {
                    let _ = write!(out, "good, {who}; I trust them");
                }
                2 => {
                    let _ = write!(out, "maybe {who} knows something I don't");
                }
                _ => {
                    let _ = write!(out, "I don't want to be alone — {who} it is");
                }
            }
        }
        GoalKind::Explore => match rng.below(4) {
            0 => out.push_str("there's ground I haven't seen"),
            1 => out.push_str("the map's got blanks"),
            2 => {
                out.push_str("what's past ");
                if let Some((x, y)) = p.coord {
                    write_coord(out, x, y);
                }
                out.push('?');
            }
            _ => out.push_str("restless — let's wander"),
        },
        GoalKind::Recover => match rng.below(4) {
            0 => out.push_str("I'm spent — resting while it's safe"),
            1 => out.push_str("need to catch my breath"),
            2 => out.push_str("a moment to recover"),
            _ => {
                out.push_str("safe enough at ");
                if let Some((x, y)) = p.coord {
                    write_coord(out, x, y);
                }
                out.push_str(" to rest");
            }
        },
        GoalKind::Shelter => match rng.below(4) {
            0 => out.push_str("too exposed out here — I'll wall myself in"),
            1 => out.push_str("I want walls around me, not open ground"),
            2 => out.push_str("closing the gap; I'll feel safer enclosed"),
            _ => {
                out.push_str("building up the sides here at ");
                if let Some((x, y)) = p.coord {
                    write_coord(out, x, y);
                }
            }
        },
        GoalKind::Mourn => {
            // reminisce about the named dead friend (the continuing bond made audible).
            let who = p.other.or(p.target).unwrap_or("them");
            match rng.below(5) {
                0 => {
                    let _ = write!(out, "I keep seeing {who}. I can't just carry on");
                }
                1 => {
                    let _ = write!(out, "{who} is gone — I need a moment with it");
                }
                2 => {
                    let _ = write!(out, "the others move on; I'm still back there with {who}");
                }
                3 => {
                    let _ = write!(out, "I sit a while. {who} would have liked it quiet here");
                }
                _ => out.push_str("the grief is heavy — I withdraw into it for now"),
            }
        }
        GoalKind::Provision => match rng.below(5) {
            0 => out.push_str("winter is coming — better store some while there's plenty"),
            1 => out.push_str("the season won't last; I'm laying provisions by"),
            2 => out.push_str("gathering a surplus for the cold — the granary needs filling"),
            3 => {
                out.push_str("hauling stores to the cache near ");
                if let Some((x, y)) = p.coord {
                    write_coord(out, x, y);
                }
            }
            _ => out.push_str("stock now, eat later — that's how you last the winter"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use daimon_core::EntityId;

    #[test]
    fn lines_vary_and_ground() {
        let mut rng = Rng::new(1);
        let mut seen = std::collections::HashSet::new();
        let mut grounded = 0;
        let n = 200;
        for _ in 0..n {
            let p = Phrasing {
                name: "Kael",
                goal: &GoalKind::Investigate(EntityId(1)),
                process: Process::Deliberate,
                drive: Drive::Curiosity,
                surprise: 0.1,
                target: Some("humming stone"),
                coord: Some((12, 7)),
                memory: Some("water lies east"),
                other: None,
                holding: false,
            };
            let mut line = String::new();
            decision_line(&mut rng, &p, &mut line);
            if line.contains("humming stone") || line.contains("(12,7)") || line.contains("water") {
                grounded += 1;
            }
            seen.insert(line);
        }
        // variety: many distinct lines from one situation
        assert!(seen.len() as f32 / n as f32 > 0.3, "variety {}", seen.len());
        // grounding: nearly all reference concrete state
        assert!(grounded as f32 / n as f32 > 0.8, "grounded {grounded}/{n}");
    }
}
