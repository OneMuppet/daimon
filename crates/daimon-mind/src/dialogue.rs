//! Dialogue — varied speech acts, not one canned line.
//!
//! What an agent says depends on *who* it's talking to (how long they've known
//! each other, how they feel), *what it knows* (a resource worth sharing, a
//! danger worth warning about), and chance. The result is a mix of speech acts —
//! greet, share, warn, ask, reminisce — each with several phrasings, so a player
//! eavesdropping on the village hears conversation, not a loop. The *content*
//! still rides along as [`Info`] so the listener can act on it.

use daimon_core::{EntityId, EntityKind, Info, Pos, Rng};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Act {
    Greet,
    Share,
    Warn,
    Ask,
    Reminisce,
}

impl Act {
    pub fn tag(self) -> &'static str {
        match self {
            Act::Greet => "greet",
            Act::Share => "share",
            Act::Warn => "warn",
            Act::Ask => "ask",
            Act::Reminisce => "reminisce",
        }
    }
}

pub struct Utterance {
    pub act: Act,
    pub text: String,
    pub info: Info,
}

/// What the speaker brings to the exchange.
pub struct SpeakCtx<'a> {
    pub listener: &'a str,
    pub times_met: u32,
    pub disposition: f32,
    /// A resource the speaker could share (id, kind, pos, label).
    pub known_place: Option<(EntityId, EntityKind, Pos, String)>,
    /// A danger the speaker could warn about.
    pub known_danger: Option<Pos>,
}

fn pick<'a>(rng: &mut Rng, opts: &[&'a str]) -> &'a str {
    opts[rng.below(opts.len())]
}

/// Compose one utterance, choosing a speech act that fits the situation.
pub fn compose(rng: &mut Rng, ctx: &SpeakCtx) -> Utterance {
    // build the set of acts available right now, then choose among them.
    let mut acts: Vec<Act> = vec![Act::Ask];
    if ctx.times_met == 0 {
        acts.push(Act::Greet);
        acts.push(Act::Greet); // first meetings skew toward hello
    } else {
        acts.push(Act::Greet);
    }
    if ctx.known_place.is_some() {
        acts.push(Act::Share);
        acts.push(Act::Share);
    }
    if ctx.known_danger.is_some() {
        acts.push(Act::Warn);
    }
    if ctx.times_met >= 2 && ctx.disposition > 0.2 {
        acts.push(Act::Reminisce);
    }
    let act = acts[rng.below(acts.len())];
    let who = ctx.listener;

    match act {
        Act::Greet => Utterance {
            act,
            text: pick(rng, &[
                &format!("Hello, {who}."),
                &format!("Well met, {who}."),
                &format!("Oh — {who}, it's you."),
                &format!("Peace, {who}."),
                &format!("Light on your path, {who}."),
                &format!("Ah, {who} — a welcome face."),
                &format!("Good to see you, {who}."),
                &format!("You're a sight, {who}."),
            ])
            .to_string(),
            info: Info::Greeting,
        },
        Act::Share => {
            let (id, kind, pos, label) = ctx.known_place.clone().unwrap();
            let w = if kind == EntityKind::Water { "water" } else { "food" };
            let text = pick(rng, &[
                &format!("There's {w} at ({},{}).", pos.x, pos.y),
                &format!("You'll find the {label} near ({},{}).", pos.x, pos.y),
                &format!("If you're after {w}, try ({},{}).", pos.x, pos.y),
                &format!("{who}, the {label} is at ({},{}).", pos.x, pos.y),
                &format!("I found {w} over at ({},{}) — go while it lasts.", pos.x, pos.y),
                &format!("The {label}? ({},{}). Don't tell the stalker.", pos.x, pos.y),
            ])
            .to_string();
            Utterance { act, text, info: Info::ResourceAt { id, kind, pos, label } }
        }
        Act::Warn => {
            let pos = ctx.known_danger.unwrap();
            let text = pick(rng, &[
                &format!("Careful — the ground near ({},{}) is bad.", pos.x, pos.y),
                &format!("Keep clear of ({},{}), {who}.", pos.x, pos.y),
                &format!("The stalker haunts ({},{}). Mind yourself.", pos.x, pos.y),
            ])
            .to_string();
            Utterance { act, text, info: Info::DangerAt { pos } }
        }
        Act::Ask => Utterance {
            act,
            text: pick(rng, &[
                &format!("Seen anything strange out there, {who}?"),
                &format!("Where's the good water these days, {who}?"),
                &format!("How've you fared, {who}?"),
                &format!("Any sign of the stalker, {who}?"),
                &format!("Found anywhere safe to rest, {who}?"),
                &format!("What've you turned up lately, {who}?"),
            ])
            .to_string(),
            info: Info::Greeting,
        },
        Act::Reminisce => Utterance {
            act,
            text: pick(rng, &[
                &format!("Good to cross paths again, {who}."),
                &format!("We keep meeting, {who} — I'm glad of it."),
                &format!("Still out here, {who}? So am I."),
                &format!("Feels like we've walked this whole valley, {who}."),
                &format!("Every time, it's you and me, {who}."),
            ])
            .to_string(),
            info: Info::Greeting,
        },
    }
}
