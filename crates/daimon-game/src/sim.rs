//! The shared world that drives several real Daimon minds at once.
//!
//! Each agent here owns a genuine [`daimon_mind::Mind`]. The world does only
//! what `daimon-world` does for one agent, generalised to many sharing a space:
//! every cognitive tick it synthesises a [`Percept`] for each mind from a
//! start-of-tick snapshot (simultaneous update — nobody sees the future),
//! cycles the mind, and applies the returned [`Action`]. Cognition is entirely
//! the published crates; only the embodiment is new — which is exactly the
//! "give it a body / give it a society" step from the white paper.

use daimon_core::{
    Action, Dir, Drive, Entity, EntityId, EntityKind, Info, Percept, Pos, Rng, SelfState, WorldEvent,
};
use daimon_mind::{dialogue, Genome, Mind, Persona, Process, Thought};

/// One inhabitant: a mind, a body, and a little render state for smoothness.
pub struct Agent {
    pub id: EntityId,
    pub name: String,
    pub mind: Mind,
    pub body: SelfState,
    pub accent: u32, // 0xRRGGBB persona accent
    /// Interpolated grid position for smooth drawing.
    pub rx: f32,
    pub ry: f32,
    pub last: Option<Thought>,
    /// A short history of render positions for a glowing motion trail.
    pub trail: Vec<(f32, f32)>,
    /// Decays each frame; spikes when a reflex or deliberation fires.
    pub flash: f32,
    pub flash_kind: Process,
    /// Floating speech timer + text.
    pub say: Option<(String, f32)>,
    inbox: Vec<WorldEvent>,
}

pub struct Resource {
    pub id: EntityId,
    pub kind: EntityKind,
    pub pos: Pos,
    pub label: String,
    pub alive: bool,
    pub payload: f32,
    respawn_at: Option<u64>,
    pub pulse: f32,
}

pub struct Predator {
    pub id: EntityId,
    pub pos: Pos,
    pub rx: f32,
    pub ry: f32,
    cooldown_until: u64,
    move_period: u64,
}

pub struct GameWorld {
    pub w: i32,
    pub h: i32,
    pub sight: i32,
    pub tick: u64,
    pub agents: Vec<Agent>,
    pub resources: Vec<Resource>,
    pub predator: Predator,
    /// 0..1 wrap; drives the day/night mood.
    pub day: f32,
    /// Rolling log of inter-agent utterances (act-tag, text) for inspection/tests.
    pub spoken: Vec<(&'static str, String)>,
    /// Stigmergic pheromone field (w·h), deposited on productive routes and
    /// evaporating each tick — emergent worn paths. Zero unless agents are
    /// stigmergic, so non-stigmergic worlds stay bit-identical.
    pub pheromone: Vec<f32>,
    /// Times the stalker was driven off by ≥2 agents confronting it together, and
    /// times a lone striker was bitten — for observing whether collective defence
    /// *emerges* (never scripted).
    pub repels: u32,
    pub lone_strikes: u32,
    rng: Rng,
    next_id: u32,
}

fn personas() -> Vec<(Persona, u32)> {
    vec![
        (
            Persona::new("Kael").with_boldness(0.5).with_sociability(0.5).with_curiosity(0.5)
                .with_creed("I want to understand this place — and last long enough to."),
            0x6ea8ff,
        ),
        (
            Persona::new("Vell").with_boldness(0.5).with_sociability(0.3).with_curiosity(0.95)
                .with_creed("Everything here is a question I haven't answered yet."),
            0xb98cff,
        ),
        (
            Persona::new("Mira").with_boldness(0.4).with_sociability(0.95).with_curiosity(0.5)
                .with_creed("No one should have to face the stalker alone."),
            0xffb14e,
        ),
        (
            Persona::new("Sela").with_boldness(0.12).with_sociability(0.6).with_curiosity(0.4)
                .with_creed("Careful keeps you breathing. I watch before I step."),
            0x5fd6a0,
        ),
        (
            Persona::new("Roin").with_boldness(0.92).with_sociability(0.35).with_curiosity(0.6)
                .with_creed("Fear is a leash. I'd rather see what's out there."),
            0xff6a6a,
        ),
        (
            Persona::new("Bex").with_boldness(0.6).with_sociability(0.7).with_curiosity(0.7)
                .with_creed("Find the others, find the food, stay clever."),
            0xf0d24e,
        ),
    ]
}

/// Personas for a village of `n` — the six hand-written characters, then as many
/// procedurally-varied villagers as needed (deterministic from index), so the
/// society can scale past six for stress/scale tests without losing diversity.
fn gen_personas(n: usize) -> Vec<(Persona, u32)> {
    let mut v = personas();
    let names = ["Aria", "Doru", "Lio", "Nyx", "Pell", "Ravi", "Suri", "Tovi", "Wren", "Yara", "Zane", "Emi"];
    let hash = |s: usize| ((s.wrapping_mul(2654435761)) % 1000) as f32 / 1000.0;
    while v.len() < n {
        let k = v.len() - 6; // procedural index past the hand-written cast
        let name = if k < names.len() { names[k].to_string() } else { format!("V{k}") };
        let bold = 0.15 + 0.7 * hash(k * 3 + 1);
        let soc = 0.2 + 0.7 * hash(k * 3 + 2);
        let cur = 0.2 + 0.7 * hash(k * 3 + 3);
        // a distinct accent hue per procedural villager.
        let accent = {
            let r = (90.0 + 150.0 * hash(k * 7 + 1)) as u32;
            let g = (90.0 + 150.0 * hash(k * 7 + 2)) as u32;
            let b = (90.0 + 150.0 * hash(k * 7 + 3)) as u32;
            (r << 16) | (g << 8) | b
        };
        v.push((
            Persona::new(&name)
                .with_boldness(bold)
                .with_sociability(soc)
                .with_curiosity(cur)
                .with_creed("I make my own way here."),
            accent,
        ));
    }
    v.truncate(n);
    v
}

impl GameWorld {
    pub fn new(seed: u64, n_agents: usize) -> Self {
        Self::build(seed, n_agents, Mind::new)
    }

    /// Build a world whose agents all express the given cognitive [`Genome`] —
    /// the genome's escalation config and faculty switches apply to every agent,
    /// while persona deltas ride on top of each base character (so the cast stays
    /// diverse and the *architecture* is what varies). This is the seam the
    /// self-improvement pipeline optimises through.
    pub fn with_genome(seed: u64, n_agents: usize, genome: &Genome) -> Self {
        Self::build(seed, n_agents, |persona, s| genome.express(&persona, s))
    }

    fn build(seed: u64, n_agents: usize, express: impl Fn(Persona, u64) -> Mind) -> Self {
        let (w, h, sight) = (40, 26, 7);
        let mut rng = Rng::new(seed);
        let mut next_id = 1u32;
        let new_id = |next: &mut u32| {
            let id = EntityId(*next);
            *next += 1;
            id
        };
        let rpos = |rng: &mut Rng| Pos::new(rng.below(w as usize) as i32, rng.below(h as usize) as i32);

        // Resource supply scales with the population so a *village has enough
        // wells for its people* — a fair world is a survivable one. With a ~24-tick
        // respawn, n+1 springs supply ≈ (n+1)/24 drinks/tick, comfortably above a
        // 6-agent thirst demand of ≈0.18/tick; 4 springs (the old fixed count) sat
        // *below* it, making survival structurally impossible for the village no
        // matter the policy. Anticipation/foraging still have to *earn* survival —
        // this only makes the test fair, not trivial.
        let pop = n_agents.max(1);
        let n_food = pop + 3;
        let n_water = pop + 3;
        let mut resources = Vec::new();
        for i in 0..n_food {
            resources.push(Resource {
                id: new_id(&mut next_id),
                kind: EntityKind::Food,
                pos: rpos(&mut rng),
                label: format!("berries-{i}"),
                alive: true,
                payload: 0.45,
                respawn_at: None,
                pulse: rng.next_f32(),
            });
        }
        for i in 0..n_water {
            resources.push(Resource {
                id: new_id(&mut next_id),
                kind: EntityKind::Water,
                pos: rpos(&mut rng),
                label: format!("spring-{i}"),
                alive: true,
                payload: 0.55,
                respawn_at: None,
                pulse: rng.next_f32(),
            });
        }
        let curio_names = ["monolith", "glyph", "humming stone", "old shrine", "strange bloom"];
        for i in 0..5 {
            resources.push(Resource {
                id: new_id(&mut next_id),
                kind: EntityKind::Curio,
                pos: rpos(&mut rng),
                label: curio_names[i % curio_names.len()].to_string(),
                alive: true,
                payload: 0.0,
                respawn_at: None,
                pulse: rng.next_f32(),
            });
        }

        let mut agents = Vec::new();
        for (i, (persona, accent)) in gen_personas(n_agents).into_iter().enumerate() {
            let pos = rpos(&mut rng);
            let name = persona.name.clone();
            agents.push(Agent {
                id: new_id(&mut next_id),
                name,
                mind: express(persona, seed ^ (0x9e37 + i as u64 * 0x1111)),
                body: SelfState::new(pos),
                accent,
                rx: pos.x as f32,
                ry: pos.y as f32,
                last: None,
                trail: Vec::new(),
                flash: 0.0,
                flash_kind: Process::Routine,
                say: None,
                inbox: Vec::new(),
            });
        }

        let pid = new_id(&mut next_id);
        let ppos = rpos(&mut rng);
        let predator = Predator {
            id: pid,
            pos: ppos,
            rx: ppos.x as f32,
            ry: ppos.y as f32,
            cooldown_until: 0,
            move_period: 2,
        };

        GameWorld {
            w,
            h,
            sight,
            tick: 0,
            agents,
            resources,
            predator,
            day: 0.15,
            spoken: Vec::new(),
            pheromone: vec![0.0; (w * h) as usize],
            repels: 0,
            lone_strikes: 0,
            rng,
            next_id,
        }
    }

    fn clamp(&self, p: Pos) -> Pos {
        Pos::new(p.x.clamp(0, self.w - 1), p.y.clamp(0, self.h - 1))
    }

    /// Index into the pheromone field for a (clamped) cell.
    pub fn pidx(&self, p: Pos) -> usize {
        let p = self.clamp(p);
        (p.y * self.w + p.x) as usize
    }

    /// During aimless exploration, the worn-path direction: the neighbour with the
    /// most pheromone above the current cell (stigmergic following). `None` if no
    /// neighbour is meaningfully stronger.
    fn worn_path_dir(&self, from: Pos) -> Option<Dir> {
        let here = self.pheromone[self.pidx(from)];
        let mut best: Option<(Dir, f32)> = None;
        for d in Dir::ALL {
            let v = self.pheromone[self.pidx(from.step(d))];
            if v > here + 1e-4 && best.is_none_or(|(_, b)| v > b) {
                best = Some((d, v));
            }
        }
        best.map(|(d, _)| d)
    }

    /// Player action: drop a fresh patch of food at a world cell.
    pub fn feed(&mut self, wx: f32, wy: f32) {
        let pos = self.clamp(Pos::new(wx.round() as i32, wy.round() as i32));
        let id = EntityId(self.next_id);
        self.next_id += 1;
        self.resources.push(Resource {
            id,
            kind: EntityKind::Food,
            pos,
            label: "gift".into(),
            alive: true,
            payload: 0.5,
            respawn_at: None,
            pulse: 0.0,
        });
    }

    /// Index of the agent nearest to a world coordinate within `radius` cells.
    pub fn pick_agent(&self, wx: f32, wy: f32, radius: f32) -> Option<usize> {
        let mut best: Option<(usize, f32)> = None;
        for (i, a) in self.agents.iter().enumerate() {
            let d = ((a.rx - wx).powi(2) + (a.ry - wy).powi(2)).sqrt();
            if d <= radius && best.map(|(_, bd)| d < bd).unwrap_or(true) {
                best = Some((i, d));
            }
        }
        best.map(|(i, _)| i)
    }

    /// All currently-perceivable entities (resources, agents, predator).
    fn snapshot(&self) -> Vec<Entity> {
        let mut v = Vec::with_capacity(self.resources.len() + self.agents.len() + 1);
        for r in &self.resources {
            if r.alive {
                v.push(Entity { id: r.id, kind: r.kind, pos: r.pos, label: r.label.clone() });
            }
        }
        for a in &self.agents {
            v.push(Entity { id: a.id, kind: EntityKind::Agent, pos: a.body.pos, label: a.name.clone() });
        }
        v.push(Entity {
            id: self.predator.id,
            kind: EntityKind::Predator,
            pos: self.predator.pos,
            label: "the stalker".into(),
        });
        v
    }

    /// One cognitive tick across all minds (simultaneous update).
    pub fn step(&mut self) {
        self.tick += 1;
        self.day = (self.day + 0.0016).fract();
        let snap = self.snapshot();

        // collected cross-agent effects, applied after the loop.
        let mut consume: Vec<EntityId> = Vec::new();
        let mut tell: Vec<(EntityId, EntityId, Info)> = Vec::new(); // (listener, from, info)
        let mut strikers: Vec<usize> = Vec::new(); // agents who struck the stalker this tick

        // commons: each agent's current foraging claim (resource pos + urgency),
        // read from last tick's commitment so the update stays simultaneous.
        let claims: Vec<(EntityId, daimon_core::Pos, f32)> = self
            .agents
            .iter()
            .filter_map(|a| a.mind.forage_claim().map(|(p, u)| (a.id, p, u)))
            .collect();

        // culture: each agent's teachable affordance + its *prestige* (how well it
        // is doing — successful agents are worth learning from). Prestige-biased
        // transmission is the engine of cumulative culture.
        let teachers: Vec<(EntityId, daimon_core::Pos, f32, daimon_mind::Concept)> = self
            .agents
            .iter()
            .filter_map(|a| {
                a.mind.teachable_concept().map(|c| {
                    let b = a.body;
                    (a.id, b.pos, (b.health + b.energy + b.hydration) / 3.0, c)
                })
            })
            .collect();

        for i in 0..self.agents.len() {
            let me = self.agents[i].body;
            let my_id = self.agents[i].id;
            // hand this agent everyone *else's* claims so it can yield/disperse.
            let others: Vec<(daimon_core::Pos, f32)> =
                claims.iter().filter(|(id, _, _)| *id != my_id).map(|(_, p, u)| (*p, *u)).collect();
            self.agents[i].mind.set_contention(others);
            // maybe learn a form's meaning from the most successful visible peer.
            // Guarded by is_cultural so non-cultural worlds draw no RNG here and
            // stay bit-identical (the harness depends on determinism).
            if self.agents[i].mind.is_cultural() {
                if let Some((_, _, _, c)) = teachers
                    .iter()
                    .filter(|(id, p, _, _)| *id != my_id && p.manhattan(me.pos) <= self.sight)
                    .max_by(|a, b| a.2.total_cmp(&b.2))
                {
                    if self.rng.chance(0.06) {
                        self.agents[i].mind.adopt_concept(c);
                    }
                }
            }
            let visible: Vec<Entity> = snap
                .iter()
                .filter(|e| e.id != my_id && e.pos.manhattan(me.pos) <= self.sight)
                .cloned()
                .collect();
            let events = std::mem::take(&mut self.agents[i].inbox);
            let percept = Percept { tick: self.tick, me, visible, events };

            let thought = self.agents[i].mind.cycle(&percept);

            // flash on the expensive / instinctive paths
            match thought.process {
                Process::Reflex => {
                    self.agents[i].flash = 1.0;
                    self.agents[i].flash_kind = Process::Reflex;
                }
                Process::Deliberate => {
                    self.agents[i].flash = self.agents[i].flash.max(0.7);
                    self.agents[i].flash_kind = Process::Deliberate;
                }
                Process::Routine => {}
            }

            // resolve the action on my own body + collect external effects
            let stig = self.agents[i].mind.is_stigmergic();
            match &thought.action {
                Action::Move(d) => {
                    let mut dir = *d;
                    // stigmergy: while *aimlessly exploring* (curiosity leads), follow
                    // worn paths. Guarded by gene + curiosity + RNG so goal-directed
                    // movement and non-stigmergic worlds are untouched/bit-identical.
                    if stig
                        && self.agents[i].mind.drives().dominant().0 == Drive::Curiosity
                        && self.rng.chance(0.5)
                    {
                        if let Some(b) = self.worn_path_dir(me.pos) {
                            dir = b;
                        }
                    }
                    let np = self.clamp(me.pos.step(dir));
                    self.agents[i].body.pos = np;
                    if stig {
                        let idx = self.pidx(np);
                        self.pheromone[idx] += 0.05; // a faint trail where feet fall
                    }
                }
                Action::Eat(id) => {
                    if let Some(r) = self.resources.iter().find(|r| r.id == *id) {
                        if r.alive && r.kind == EntityKind::Food && r.pos.manhattan(me.pos) <= 1 {
                            self.agents[i].body.energy = (me.energy + r.payload).min(1.0);
                            consume.push(*id);
                            self.agents[i].inbox.push(WorldEvent::Ate { id: *id, energy: r.payload });
                            if stig {
                                let idx = self.pidx(me.pos);
                                self.pheromone[idx] += 1.0; // a strong mark: food was here
                            }
                        }
                    }
                }
                Action::Drink(id) => {
                    if let Some(r) = self.resources.iter().find(|r| r.id == *id) {
                        if r.alive && r.kind == EntityKind::Water && r.pos.manhattan(me.pos) <= 1 {
                            self.agents[i].body.hydration = (me.hydration + r.payload).min(1.0);
                            self.agents[i].inbox.push(WorldEvent::Drank { id: *id });
                            if stig {
                                let idx = self.pidx(me.pos);
                                self.pheromone[idx] += 1.0; // water was here
                            }
                        }
                    }
                }
                Action::Talk { to, text } => {
                    // Say something with *content*: share a resource we know of,
                    // so the listener can act on it. This is how knowledge spreads.
                    let places: Vec<(EntityId, EntityKind, Pos, String)> = self.agents[i]
                        .mind
                        .memory()
                        .places()
                        .filter(|(_, p)| matches!(p.kind, EntityKind::Food | EntityKind::Water))
                        .map(|(id, p)| (id, p.kind, p.pos, p.label.clone()))
                        .collect();
                    // compose a varied utterance — a real speech act keyed to who
                    // we're addressing and what we know, not one canned line.
                    let from_id = self.agents[i].id;
                    let known_place = if places.is_empty() {
                        None
                    } else {
                        Some(places[self.rng.below(places.len())].clone())
                    };
                    let known_danger = self.agents[i].mind.worst_danger();
                    let listener_name = self
                        .agents
                        .iter()
                        .find(|a| a.id == *to)
                        .map(|a| a.name.clone())
                        .unwrap_or_else(|| "friend".into());
                    let ctx = dialogue::SpeakCtx {
                        listener: &listener_name,
                        times_met: self.agents[i].mind.social().model(*to).map(|m| m.interactions).unwrap_or(0),
                        disposition: self.agents[i].mind.social().disposition(*to),
                        known_place,
                        known_danger,
                    };
                    let utt = dialogue::compose(&mut self.rng, &ctx);
                    self.agents[i].say = Some((short_say(&utt.text), 2.2));
                    self.agents[i].inbox.push(WorldEvent::Spoke { to: *to, text: text.clone() });
                    if self.spoken.len() < 8000 {
                        self.spoken.push((utt.act.tag(), utt.text.clone()));
                    }
                    tell.push((*to, from_id, utt.info));
                    // we mention a couple more places in passing — knowledge
                    // travels in bunches, so it actually spreads through the group.
                    let mut extra = places.clone();
                    for k in (1..extra.len()).rev() {
                        extra.swap(k, self.rng.below(k + 1));
                    }
                    for (id, kind, pos, label) in extra.into_iter().take(4) {
                        tell.push((*to, from_id, Info::ResourceAt { id, kind, pos, label }));
                    }
                }
                Action::Inspect(_) => {
                    self.agents[i].say = Some(("…fascinating.".into(), 1.6));
                }
                Action::Strike(id) => {
                    // record a strike at the stalker if adjacent; the *outcome*
                    // (repelled, or bitten) is resolved collectively below.
                    if *id == self.predator.id && me.pos.manhattan(self.predator.pos) <= 1 {
                        strikers.push(i);
                        self.agents[i].flash = 1.0;
                        self.agents[i].flash_kind = Process::Reflex;
                    }
                }
                Action::Rest => {
                    self.agents[i].body.energy = (me.energy + 0.03).min(1.0);
                }
                Action::Wait => {}
            }

            self.agents[i].last = Some(thought);
        }

        // deliver utterances to listeners (heard next tick)
        for (listener, from, info) in tell {
            if let Some(a) = self.agents.iter_mut().find(|a| a.id == listener) {
                a.inbox.push(WorldEvent::Told { from, info });
            }
        }

        // consume eaten resources (respawn in place after a while)
        for id in consume {
            let at = self.tick + 16 + self.rng.below(16) as u64;
            if let Some(r) = self.resources.iter_mut().find(|r| r.id == id) {
                r.alive = false;
                r.respawn_at = Some(at);
            }
        }

        // ── COLLECTIVE DEFENCE (world physics, not behaviour) ────────────────
        // The stalker yields to numbers: when two or more confront it together it
        // is driven off (and every confronter learns it worked); a lone striker is
        // simply bitten. Nothing here tells the agents to gather — this is only how
        // the world *responds* to being faced. Whether they rally is up to them.
        let near: Vec<usize> = strikers
            .iter()
            .copied()
            .filter(|&i| self.agents[i].body.pos.manhattan(self.predator.pos) <= 1)
            .collect();
        if near.len() >= 2 {
            let ap = self.predator.pos;
            let corners = [
                Pos::new(0, 0),
                Pos::new(self.w - 1, 0),
                Pos::new(0, self.h - 1),
                Pos::new(self.w - 1, self.h - 1),
            ];
            let far = corners.into_iter().max_by_key(|c| c.manhattan(ap)).unwrap_or(ap);
            self.predator.pos = far;
            self.predator.cooldown_until = self.tick + 24;
            self.repels += 1;
            let pid = self.predator.id;
            for &i in &near {
                self.agents[i].inbox.push(WorldEvent::Repelled { id: pid });
            }
        } else if near.len() == 1 {
            let i = near[0];
            self.agents[i].body.health = (self.agents[i].body.health - 0.15).max(0.05);
            self.lone_strikes += 1;
            let pid = self.predator.id;
            self.agents[i].inbox.push(WorldEvent::Hurt { id: pid, health: 0.15 });
        }

        self.metabolism();
        self.step_predator();
        self.respawn_due();
        // pheromone evaporates so worn paths reflect *recent* success and fade as
        // routes go stale (no RNG; a no-op when the field is all zero).
        for v in &mut self.pheromone {
            *v *= 0.97;
            if *v < 0.002 {
                *v = 0.0;
            }
        }
    }

    fn metabolism(&mut self) {
        let safe_radius = 3;
        let pred = self.predator.pos;
        for a in &mut self.agents {
            a.body.energy = (a.body.energy - 0.012).clamp(0.0, 1.0);
            a.body.hydration = (a.body.hydration - 0.014).clamp(0.0, 1.0);
            let safe = a.body.pos.manhattan(pred) > safe_radius;
            if safe && a.body.health < 1.0 {
                a.body.health = (a.body.health + 0.004).min(1.0);
            }
            if a.body.energy <= 0.0 || a.body.hydration <= 0.0 {
                a.body.health = (a.body.health - 0.02).max(0.0);
            }
            // bodies never truly die here — they slump to a low ebb and recover,
            // so the village persists for watching. Health floors at 0.05.
            a.body.health = a.body.health.max(0.05);
        }
    }

    fn step_predator(&mut self) {
        // target the nearest agent
        let target = self
            .agents
            .iter()
            .min_by_key(|a| a.body.pos.manhattan(self.predator.pos))
            .map(|a| (a.id, a.body.pos));
        let Some((tid, tpos)) = target else { return };
        let p = self.predator.pos;

        if p.manhattan(tpos) == 0 {
            self.strike(tid);
            return;
        }
        let new = if self.tick < self.predator.cooldown_until {
            let d = Dir::ALL[self.rng.below(4)];
            self.clamp(p.step(d))
        } else if self.tick.is_multiple_of(self.predator.move_period) {
            const AGGRO: i32 = 12;
            if p.manhattan(tpos) <= AGGRO {
                p.step(p.toward(tpos))
            } else {
                let d = Dir::ALL[self.rng.below(4)];
                self.clamp(p.step(d))
            }
        } else {
            p
        };
        self.predator.pos = new;
        if new == tpos {
            self.strike(tid);
        }
    }

    fn strike(&mut self, agent_id: EntityId) {
        if let Some(a) = self.agents.iter_mut().find(|a| a.id == agent_id) {
            a.body.health = (a.body.health - 0.2).max(0.05);
            a.inbox.push(WorldEvent::Hurt { id: self.predator.id, health: 0.2 });
            a.flash = 1.0;
        }
        // retreat to the farthest corner + cooldown
        let ap = self.agents.iter().find(|a| a.id == agent_id).map(|a| a.body.pos).unwrap_or(self.predator.pos);
        let corners = [
            Pos::new(0, 0),
            Pos::new(self.w - 1, 0),
            Pos::new(0, self.h - 1),
            Pos::new(self.w - 1, self.h - 1),
        ];
        let far = corners.into_iter().max_by_key(|c| c.manhattan(ap)).unwrap_or(Pos::new(0, 0));
        self.predator.pos = far;
        self.predator.cooldown_until = self.tick + 12;
    }

    fn respawn_due(&mut self) {
        let now = self.tick;
        for r in &mut self.resources {
            if !r.alive && r.respawn_at.map(|t| t <= now).unwrap_or(false) {
                r.alive = true;
                r.respawn_at = None;
            }
        }
    }

    /// Smoothly advance render positions toward grid positions, and decay timers.
    pub fn animate(&mut self, dt: f32) {
        let k = (dt * 9.0).min(1.0);
        for a in &mut self.agents {
            let (ox, oy) = (a.rx, a.ry);
            a.rx += (a.body.pos.x as f32 - a.rx) * k;
            a.ry += (a.body.pos.y as f32 - a.ry) * k;
            // lay down a motion-trail breadcrumb when the agent has moved enough.
            let moved = (a.rx - ox).hypot(a.ry - oy);
            let far_enough =
                a.trail.last().map(|&(lx, ly)| (a.rx - lx).hypot(a.ry - ly) > 0.18).unwrap_or(true);
            if moved > 0.02 && far_enough {
                a.trail.push((a.rx, a.ry));
                if a.trail.len() > 14 {
                    a.trail.remove(0);
                }
            }
            a.flash = (a.flash - dt * 1.6).max(0.0);
            if let Some((_, ref mut t)) = a.say {
                *t -= dt;
            }
            if a.say.as_ref().map(|(_, t)| *t <= 0.0).unwrap_or(false) {
                a.say = None;
            }
        }
        self.predator.rx += (self.predator.pos.x as f32 - self.predator.rx) * k;
        self.predator.ry += (self.predator.pos.y as f32 - self.predator.ry) * k;
        for r in &mut self.resources {
            r.pulse = (r.pulse + dt * 0.6).fract();
        }
    }
}

fn short_say(text: &str) -> String {
    let t = text.trim();
    if t.chars().count() <= 28 {
        t.to_string()
    } else {
        let s: String = t.chars().take(27).collect();
        format!("{s}…")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn village_runs_long_without_panicking() {
        // Hammer the whole multi-mind embodiment: percept synthesis, real
        // cognition, action resolution, predator, speech, respawn.
        let mut w = GameWorld::new(0xDA13, 6);
        for _ in 0..1200 {
            w.step();
            w.animate(0.016);
        }
        assert_eq!(w.agents.len(), 6);
        // every mind should have lived, formed beliefs, and stayed bounded.
        for a in &w.agents {
            assert!(a.mind.metrics().ticks >= 1000);
            assert!((0.0..=1.0).contains(&a.body.health));
            assert!(a.mind.memory().episode_count() > 0);
        }
        // determinism: same seed → same agent positions.
        let mut a = GameWorld::new(7, 4);
        let mut b = GameWorld::new(7, 4);
        for _ in 0..300 {
            a.step();
            b.step();
        }
        for (x, y) in a.agents.iter().zip(b.agents.iter()) {
            assert_eq!(x.body.pos, y.body.pos);
        }
    }

    #[test]
    fn feed_and_pick_work() {
        let mut w = GameWorld::new(1, 3);
        let before = w.resources.len();
        w.feed(5.0, 5.0);
        assert_eq!(w.resources.len(), before + 1);
        // an agent sits on its own render position, so picking there finds it.
        let a0 = (w.agents[0].rx, w.agents[0].ry);
        assert_eq!(w.pick_agent(a0.0, a0.1, 0.7), Some(0));
    }
}
