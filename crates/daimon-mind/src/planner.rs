//! The planner — turning an intention into the next few moves.
//!
//! Daimon's planner is intentionally humble. Rather than compute a perfect plan
//! once, it produces a *short* plan toward the goal and trusts the cognitive
//! cycle to re-plan when the world shifts. This is the lesson of game-industry
//! planners — GOAP (Orkin, *The A.I. of F.E.A.R.*, GDC 2006) and HTN planning
//! (Erol, Hendler & Nau, 1994): decompose a goal into ordered primitive
//! actions, but stay ready to throw the plan away. In a world where the river
//! can dry up and the predator keeps moving, cheap frequent re-planning beats
//! expensive optimal planning every time.

use daimon_core::{
    Action, Dir, EntityId, EntityKind, Goal, GoalKind, Memory, Plan, Pos, Rng, WorldModel,
};
use std::collections::BTreeMap;

/// How many steps ahead the planner commits before the cycle re-evaluates.
const HORIZON: usize = 4;

/// Carried-provision load at which a provisioning agent stops gathering and heads
/// for the granary to store. A round-trip rhythm, not a hoard.
const PROVISION_LOAD: f32 = 0.6;

/// Region size for the danger map (a coarse grid the agent learns to avoid).
pub const REGION: i32 = 4;

/// A learned danger field: region → how badly things have gone there.
pub type Danger = BTreeMap<(i32, i32), f32>;

pub fn region_of(p: Pos) -> (i32, i32) {
    (p.x.div_euclid(REGION), p.y.div_euclid(REGION))
}

fn danger_at(danger: &Danger, p: Pos) -> f32 {
    *danger.get(&region_of(p)).unwrap_or(&0.0)
}

/// Predator-aware coordination (selfish-herd) parameters handed to the flee path:
/// `cohesion` ∈ [0,1] (how heavily the anti-isolation pull weighs against fleeing)
/// and the positions of the agent's visible allies (the local prey group). `None`
/// ⇒ the faculty is off and the flee step is the plain straight-away-from-predator
/// move (byte-identical to the incumbent).
#[derive(Clone, Copy)]
pub struct Herd<'a> {
    pub cohesion: f32,
    pub allies: &'a [Pos],
}

/// Build a fresh plan for `goal`. `rng` supplies exploration jitter; `danger`
/// is the learned map of places to avoid.
pub fn plan_for(
    goal: &Goal,
    world: &WorldModel,
    memory: &Memory,
    danger: &Danger,
    rng: &mut Rng,
    tick: u64,
) -> Plan {
    plan_for_with(goal, world, memory, danger, rng, tick, None, &[], 0.0, None)
}

/// As [`plan_for`], but with an optional `forage_override` (a resource `(id, pos)`
/// chosen upstream, e.g. by drive-reduction-rate selection) and commons-awareness:
/// `contention` lists peers' claimed resource positions with their urgency, and
/// `my_urgency` is this agent's own — so the planner can yield contested tiles to
/// the more-urgent and disperse across resources.
#[allow(clippy::too_many_arguments)]
pub fn plan_for_with(
    goal: &Goal,
    world: &WorldModel,
    memory: &Memory,
    danger: &Danger,
    rng: &mut Rng,
    tick: u64,
    forage_override: Option<(EntityId, Pos)>,
    contention: &[(Pos, f32)],
    my_urgency: f32,
    herd: Option<Herd>,
) -> Plan {
    let mut steps: Vec<Action> = Vec::new();
    fill_plan_steps(
        &mut steps, goal, world, memory, danger, rng, forage_override, contention, my_urgency, herd,
    );
    Plan::new(goal.clone(), steps, tick)
}

/// Fill `out` (cleared first) with the ordered actions pursuing `goal`. The
/// allocation-free core of [`plan_for_with`]: the hot path passes a reused buffer
/// so a (re)plan touches the heap only when the buffer must grow.
#[allow(clippy::too_many_arguments)]
pub fn fill_plan_steps(
    out: &mut Vec<Action>,
    goal: &Goal,
    world: &WorldModel,
    memory: &Memory,
    danger: &Danger,
    rng: &mut Rng,
    forage_override: Option<(EntityId, Pos)>,
    contention: &[(Pos, f32)],
    my_urgency: f32,
    herd: Option<Herd>,
) {
    let me = world.me();
    let pos = me.map(|m| m.pos).unwrap_or(Pos::new(0, 0));
    out.clear();

    match &goal.kind {
        GoalKind::Flee(threat) => flee_steps(out, *threat, world, danger, pos, rng, herd),
        // approach the threat and strike when adjacent — ignoring the learned
        // danger field (you can't confront a thing while avoiding it).
        GoalKind::Confront(threat) => match world.belief(*threat) {
            Some(b) => path_to(out, pos, b.entity.pos, Action::Strike(*threat)),
            None => wander(out, pos, world, danger, rng),
        },
        GoalKind::Forage => match forage_override {
            Some((id, tp)) => path_to(out, pos, tp, Action::Eat(id)),
            None => seek_then(out, EntityKind::Food, world, memory, danger, pos, rng, contention, my_urgency, Action::Eat),
        },
        GoalKind::Hydrate => match forage_override {
            Some((id, tp)) => path_to(out, pos, tp, Action::Drink(id)),
            None => seek_then(out, EntityKind::Water, world, memory, danger, pos, rng, contention, my_urgency, Action::Drink),
        },
        GoalKind::Investigate(id) => {
            seek_specific(out, *id, world, danger, pos, rng, Action::Inspect(*id))
        }
        GoalKind::Socialize(id) => seek_specific(
            out,
            *id,
            world,
            danger,
            pos,
            rng,
            Action::Talk {
                to: *id,
                text: greeting(*id, memory),
            },
        ),
        GoalKind::Explore => wander(out, pos, world, danger, rng),
        GoalKind::Recover => out.push(Action::Rest),
        // SHELTER: enclose the self. If a side is still open, wall the cell on the
        // gap the body senses; once fully enclosed there is nothing to build, so
        // rest (safe inside) — never a scripted hut, just "close the next gap".
        GoalKind::Shelter => match me.and_then(|m| m.shelter_gap) {
            Some(d) => out.push(Action::Build(pos.step(d))),
            None => out.push(Action::Rest),
        },
        // MOURN (loss-oriented coping): withdraw and be still. The grieving mind
        // pulls back from foraging and social initiative — it idles in place and
        // reminisces (the reminiscence is the narration). Resting also lets it
        // recover, so withdrawal is not self-destructive. The Dual-Process swing
        // back to ordinary goals is driven in `decide`, not here.
        GoalKind::Mourn => out.push(Action::Rest),
        // PROVISION (stock up against winter): a two-phase plan the body's senses
        // drive. If carrying a surplus and the granary's direction is known, step
        // toward it and Store; otherwise step toward the nearest harvestable source
        // and Gather. The thresholds/dirs come from the world via `SelfState` (just
        // as Shelter reads `shelter_gap`), so this is "take the next provisioning
        // step", never a scripted stockpile routine. Falls back to wandering to find
        // a source if neither direction is sensed.
        GoalKind::Provision => {
            let carrying = me.map(|m| m.carrying).unwrap_or(0.0);
            let store_dir = me.and_then(|m| m.store_dir);
            let gather_dir = me.and_then(|m| m.gather_dir);
            match (carrying >= PROVISION_LOAD, store_dir, gather_dir) {
                // carrying a load and the cache is somewhere: head there and store.
                (true, Some(d), _) => step_then(out, pos, d, Action::Store),
                // room to carry more and a source is known: head there and gather.
                (false, _, Some(d)) => step_then(out, pos, d, Action::Gather),
                // carrying a load but the cache is right here (no dir): store now.
                (true, None, _) => out.push(Action::Store),
                // WINTER homing: nothing to gather, not carrying — but the hearth has
                // a direction (the cache/warmth is over there). Walk home; the world
                // auto-draws the stores once we're in the hearth's radius.
                (false, Some(d), None) => {
                    out.push(Action::Move(d));
                    out.push(Action::Move(d));
                }
                // nothing sensed: stay put by the warmth / look around.
                (false, None, None) => out.push(Action::Rest),
            }
        }
    }
}

/// Move to evade a threat for a few steps.
///
/// With `herd = None` (the faculty off) this is the incumbent behaviour: pick the
/// direction that *increases* distance from the predator the most — flee straight
/// away. Byte-identical to the original, drawing no RNG.
///
/// With `herd = Some(..)` (predator-aware coordination on) the step COMPOSES two
/// terms — flee away from the predator AND pull toward the local prey group (the
/// **selfish herd**, Hamilton 1971): each candidate direction is scored by the
/// distance it gains from the predator *plus* a cohesion-weighted reduction in the
/// agent's isolation (distance to the group centroid). Moving toward the group
/// dilutes this agent's individual risk and, crucially, stops it being the lone
/// straggler an isolated-target predator picks off — while never stepping *toward*
/// the predator (the flee term dominates when the group lies past the threat).
/// Fully deterministic — derived from the perceived predator + ally positions, no
/// RNG draw — so the gate stays clean.
fn flee_steps(
    out: &mut Vec<Action>,
    threat: EntityId,
    world: &WorldModel,
    danger: &Danger,
    pos: Pos,
    rng: &mut Rng,
    herd: Option<Herd>,
) {
    let Some(b) = world.belief(threat) else {
        wander(out, pos, world, danger, rng);
        return;
    };
    let tp = b.entity.pos;

    // INCUMBENT flee (faculty off, or no allies to herd toward): straight away.
    let cohesion = match herd {
        Some(h) if h.cohesion > 0.0 && !h.allies.is_empty() => h.cohesion,
        _ => {
            let best = Dir::ALL
                .into_iter()
                .max_by_key(|d| pos.step(*d).manhattan(tp))
                .unwrap_or(Dir::North);
            for _ in 0..3 {
                out.push(Action::Move(best));
            }
            return;
        }
    };
    let allies = herd.expect("cohesion>0 implies herd present").allies;

    // group centroid — the heart of the herd to pull toward (selfish-herd geometry).
    let n = allies.len() as i32;
    let cx = allies.iter().map(|p| p.x).sum::<i32>() / n;
    let cy = allies.iter().map(|p| p.y).sum::<i32>() / n;
    let centroid = Pos::new(cx, cy);
    let nearest_ally = allies
        .iter()
        .min_by_key(|a| a.manhattan(pos))
        .copied()
        .unwrap_or(centroid);

    // score each candidate step: gain distance from the predator, AND cut isolation
    // (distance to the centroid / nearest ally). Cohesion sets the trade-off. We
    // re-derive the same flee gain the incumbent maximises, then add the herd term.
    let flee_gain = |np: Pos| (np.manhattan(tp) - pos.manhattan(tp)) as f32;
    // isolation reduction: how much closer to the herd the step gets us (positive is
    // good). Blend centroid (the geometry) with the nearest ally (the concrete
    // neighbour to shelter beside).
    let iso_gain = |np: Pos| {
        let c = (centroid.manhattan(pos) - centroid.manhattan(np)) as f32;
        let a = (nearest_ally.manhattan(pos) - nearest_ally.manhattan(np)) as f32;
        0.6 * c + 0.4 * a
    };
    let best = Dir::ALL
        .into_iter()
        .max_by(|&d1, &d2| {
            let np1 = pos.step(d1);
            let np2 = pos.step(d2);
            // flee term anchors the score (you never walk into the predator); the
            // herd term, scaled by cohesion, biases among the safe-ish directions.
            let s1 = flee_gain(np1) + cohesion * iso_gain(np1) - danger_at(danger, np1) * 0.5;
            let s2 = flee_gain(np2) + cohesion * iso_gain(np2) - danger_at(danger, np2) * 0.5;
            s1.total_cmp(&s2)
        })
        .unwrap_or(Dir::North);
    let _ = rng; // herd path is deterministic; rng kept for signature parity.
    for _ in 0..3 {
        out.push(Action::Move(best));
    }
}

/// Head to the nearest known entity of `kind`, then perform `finish`. If none is
/// known, wander to look for one.
#[allow(clippy::too_many_arguments)]
fn seek_then(
    out: &mut Vec<Action>,
    kind: EntityKind,
    world: &WorldModel,
    memory: &Memory,
    danger: &Danger,
    pos: Pos,
    rng: &mut Rng,
    contention: &[(Pos, f32)],
    my_urgency: f32,
    finish: impl Fn(EntityId) -> Action,
) {
    // balance need against risk: pick the resource that minimises travel *plus*
    // a penalty for known danger — a closer berry in a dangerous spot loses to a
    // slightly farther safe one — *plus* a commons penalty so the agent yields a
    // tile already claimed by a more-urgent peer and disperses off crowded ones.
    // Fall back to spatial memory if none in view.
    // Commons is *conditional on contention*: dispersing only pays when resources
    // are genuinely scarce relative to the crowd. The agent estimates scarcity
    // from what it knows — known resources of this kind vs known agents — so when
    // there is plenty (a large, well-supplied village) it does NOT needlessly
    // disperse off a perfectly good nearby tile. (Finding from the scale test:
    // unconditional dispersion helps under scarcity but hurts when supply is ample.)
    let n_res = world.beliefs().filter(|b| b.entity.kind == kind).count().max(1);
    let n_agt = world.beliefs().filter(|b| b.entity.kind == EntityKind::Agent).count() + 1;
    let scarcity = ((n_agt as f32 / n_res as f32) - 1.0).clamp(0.0, 1.0);
    let crowd = |at: Pos| -> f32 {
        if scarcity <= 0.0 {
            return 0.0; // resources outnumber agents — no contention to manage
        }
        scarcity
            * contention
                .iter()
                .filter(|(cp, _)| cp.manhattan(at) <= 1)
                // yield to the more (or equally) urgent (heavy); merely disperse off
                // a tile a less-urgent peer holds (light).
                .map(|(_, urg)| if *urg >= my_urgency { 14.0 } else { 5.0 })
                .sum::<f32>()
    };
    let target = if contention.is_empty() {
        // INCUMBENT path (commons-awareness off): nearest visible by travel+danger,
        // else nearest remembered. Byte-identical to the original behaviour.
        world
            .beliefs()
            .filter(|b| b.entity.kind == kind)
            .min_by(|a, b| {
                let sa = a.entity.pos.manhattan(pos) as f32 + danger_at(danger, a.entity.pos) * 3.0;
                let sb = b.entity.pos.manhattan(pos) as f32 + danger_at(danger, b.entity.pos) * 3.0;
                sa.total_cmp(&sb)
            })
            .map(|b| (b.entity.id, b.entity.pos))
            .or_else(|| memory.nearest_place_of(kind, pos))
    } else {
        // COMMONS-AWARE path: candidates are everything the agent could go to —
        // visible *and* remembered — so the contention penalty disperses agents
        // across tiles they cannot currently see (scarce water is usually out of a
        // 7-tile sight). Without this, peers converge on the same remembered
        // spring and the yield/disperse rule never fires.
        let cost = |at: Pos| at.manhattan(pos) as f32 + danger_at(danger, at) * 3.0 + crowd(at);
        let mut cands: Vec<(EntityId, Pos)> = world
            .beliefs()
            .filter(|b| b.entity.kind == kind)
            .map(|b| (b.entity.id, b.entity.pos))
            .collect();
        for (id, place) in memory.places().filter(|(_, p)| p.kind == kind) {
            if !cands.iter().any(|(_, p)| *p == place.pos) {
                cands.push((id, place.pos));
            }
        }
        cands.into_iter().min_by(|(_, pa), (_, pb)| cost(*pa).total_cmp(&cost(*pb)))
    };

    match target {
        Some((id, tp)) => path_to(out, pos, tp, finish(id)),
        None => wander(out, pos, world, danger, rng),
    }
}

/// Head to a specific entity (predator-free target) and finish with `finish`.
fn seek_specific(
    out: &mut Vec<Action>,
    id: EntityId,
    world: &WorldModel,
    danger: &Danger,
    pos: Pos,
    rng: &mut Rng,
    finish: Action,
) {
    match world.belief(id) {
        Some(b) => path_to(out, pos, b.entity.pos, finish),
        None => wander(out, pos, world, danger, rng),
    }
}

/// Greedy Manhattan path of up to `HORIZON` steps toward `target`, finishing
/// with `finish` once adjacent/co-located. No obstacles in this world, so a
/// greedy step is also an optimal one.
fn path_to(out: &mut Vec<Action>, from: Pos, target: Pos, finish: Action) {
    let start = out.len();
    let mut cur = from;
    while cur.manhattan(target) > 1 && out.len() - start < HORIZON {
        let d = cur.toward(target);
        cur = cur.step(d);
        out.push(Action::Move(d));
    }
    if cur.manhattan(target) <= 1 {
        out.push(finish);
    }
    if out.len() == start {
        out.push(Action::Wait);
    }
}

/// One step in a sensed direction, then perform `finish` (which the world resolves
/// only if the agent is actually adjacent to / on the relevant feature). Used by
/// Provision, whose Gather/Store targets are sensed as a direction, not a belief id.
fn step_then(out: &mut Vec<Action>, _from: Pos, dir: Dir, finish: Action) {
    out.push(Action::Move(dir));
    out.push(finish);
}

/// Curiosity-biased wandering: prefer directions that lead away from where we
/// already are, with a little randomness so it never looks like a patrol.
fn wander(out: &mut Vec<Action>, pos: Pos, _world: &WorldModel, danger: &Danger, rng: &mut Rng) {
    let mut cur = pos;
    for _ in 0..2 {
        // jitter + slight push outward, strongly damped toward dangerous cells:
        // the agent has *learned* to give certain places a wide berth. The weights
        // live on the stack (fixed 4 directions) so wandering allocates nothing.
        let mut weights = [0.0f32; 4];
        for (w, d) in weights.iter_mut().zip(Dir::ALL.iter()) {
            let np = cur.step(*d);
            let base = 1.0 + (np.x.abs() + np.y.abs()) as f32 * 0.02 + rng.next_f32();
            *w = base / (1.0 + danger_at(danger, np) * 4.0);
        }
        let idx = rng.weighted(&weights).unwrap_or(0);
        let d = Dir::ALL[idx];
        cur = cur.step(d);
        out.push(Action::Move(d));
    }
}

/// Compose a greeting whose warmth reflects what we remember of the other agent.
fn greeting(id: EntityId, memory: &Memory) -> String {
    if let Some((_, label)) = memory.known_place(id) {
        format!("Hello {label} — good to see a familiar face.")
    } else {
        "Hello there. I'm friendly.".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use daimon_core::{Drive, Entity, Percept, SelfState};

    fn world_with(entities: Vec<Entity>, me: Pos) -> WorldModel {
        let mut wm = WorldModel::default();
        wm.integrate(&Percept {
            tick: 1,
            me: SelfState::new(me),
            visible: entities,
            events: vec![],
        });
        wm
    }

    fn ent(id: u32, kind: EntityKind, x: i32, y: i32) -> Entity {
        Entity {
            id: EntityId(id),
            kind,
            pos: Pos::new(x, y),
            label: "thing".into(),
        }
    }

    #[test]
    fn forage_plan_ends_in_eat_when_food_known() {
        let wm = world_with(vec![ent(1, EntityKind::Food, 3, 0)], Pos::new(0, 0));
        let mem = Memory::default();
        let mut rng = Rng::new(1);
        let goal = Goal {
            kind: GoalKind::Forage,
            origin: Drive::Hunger,
            priority: 0.8,
        };
        let plan = plan_for(&goal, &wm, &mem, &Danger::new(), &mut rng, 1);
        assert!(matches!(plan.steps.back(), Some(Action::Eat(EntityId(1)))));
    }

    #[test]
    fn flee_increases_distance_from_threat() {
        let wm = world_with(vec![ent(9, EntityKind::Predator, 1, 0)], Pos::new(0, 0));
        let mem = Memory::default();
        let mut rng = Rng::new(1);
        let goal = Goal {
            kind: GoalKind::Flee(EntityId(9)),
            origin: Drive::Survival,
            priority: 1.0,
        };
        let plan = plan_for(&goal, &wm, &mem, &Danger::new(), &mut rng, 1);
        // first move should not step toward the predator at (1,0).
        if let Some(Action::Move(d)) = plan.steps.front() {
            let np = Pos::new(0, 0).step(*d);
            assert!(np.manhattan(Pos::new(1, 0)) >= 1);
        } else {
            panic!("expected a move");
        }
    }
}
