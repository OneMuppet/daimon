//! The real-reasoner seam — System 2 backed by a language model.
//!
//! This is the rung that turns Daimon's templated offline deliberation into
//! genuine, open-vocabulary thought. [`LlmDeliberator`] renders the agent's
//! situation into a prompt, asks a model for a goal + rationale + lessons, and
//! parses the reply into a [`Deliberation`] — the exact same type the offline
//! heuristic returns, so it drops into the cognitive cycle unchanged.
//!
//! The HTTP is *injected* through the [`Transport`] trait. That means the whole
//! prompt-build → parse path is tested with a fake transport and **no network**
//! (see the contract test below); a real Anthropic transport lives behind the
//! `llm-http` feature. Render-in, structured-out — ReAct / Reflexion /
//! Tree-of-Thoughts all fit behind this same call.

use crate::deliberate::{Deliberation, DeliberationContext, Deliberator, Lesson};
use daimon_core::{Drive, EntityKind, GoalKind, Pos};

/// How the deliberator reaches the model. Injected so it can be faked in tests.
pub trait Transport {
    /// Send a provider request body (JSON) and return the response body (JSON).
    fn send(&self, request_json: &str) -> Result<String, String>;
}

/// System 2, backed by a model reached through `T`.
pub struct LlmDeliberator<T: Transport> {
    transport: T,
    pub model: String,
    pub max_tokens: u32,
}

impl<T: Transport> LlmDeliberator<T> {
    pub fn new(transport: T, model: impl Into<String>) -> Self {
        Self { transport, model: model.into(), max_tokens: 512 }
    }

    /// Render the agent's situation into a compact, model-readable brief.
    pub fn render_context(ctx: &DeliberationContext) -> String {
        let me = ctx.world.me();
        let pos = me.map(|m| m.pos).unwrap_or(Pos::new(0, 0));
        let mut s = String::new();
        s.push_str(&format!("You are {}. Creed: \"{}\".\n", ctx.persona.name, ctx.persona.creed));
        if let Some(m) = me {
            s.push_str(&format!(
                "Body: health {:.0}%, energy {:.0}%, water {:.0}%. Position ({},{}).\n",
                m.health * 100.0, m.energy * 100.0, m.hydration * 100.0, pos.x, pos.y
            ));
        }
        s.push_str("Drives now: ");
        for (d, v) in ctx.drives.iter() {
            s.push_str(&format!("{} {:.0}%, ", d.name(), v * 100.0));
        }
        s.push_str(&format!("\nSurprise: {:.0}%.\n", ctx.surprise * 100.0));
        s.push_str("In view: ");
        for b in ctx.world.beliefs().filter(|b| b.visible) {
            s.push_str(&format!("{} at ({},{}); ", b.entity.label, b.entity.pos.x, b.entity.pos.y));
        }
        s.push('\n');
        let mut facts = ctx.memory.facts();
        if let Some((_, f)) = facts.next() {
            s.push_str(&format!("You recall: {}.\n", f.statement));
        }
        s.push_str(
            "\nChoose ONE goal from: forage, hydrate, flee, investigate, socialize, explore, recover.\n\
             Reply with ONLY JSON: {\"goal\":\"<one>\",\"rationale\":\"<first-person reason>\",\"lessons\":[\"<optional>\"]}",
        );
        s
    }

    /// Build the provider request body (Anthropic Messages shape).
    pub fn build_request(&self, ctx: &DeliberationContext) -> String {
        let brief = Self::render_context(ctx);
        serde_json::json!({
            "model": self.model,
            "max_tokens": self.max_tokens,
            "system": "You are the deliberative mind of an autonomous game character. \
                       Think briefly, in character, and answer only with the requested JSON.",
            "messages": [{ "role": "user", "content": brief }],
        })
        .to_string()
    }

    /// Parse the provider reply into a `Deliberation`, resolving goal keywords to
    /// concrete targets from the world.
    pub fn parse_reply(reply_json: &str, ctx: &DeliberationContext) -> Option<Deliberation> {
        let v: serde_json::Value = serde_json::from_str(reply_json).ok()?;
        // The model's text is in content[0].text (Anthropic shape); fall back to
        // treating the whole reply as the inner object.
        let inner_text = v
            .get("content")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("text"))
            .and_then(|t| t.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| reply_json.to_string());
        let obj: serde_json::Value = serde_json::from_str(&inner_text).ok()?;

        let goal_kw = obj.get("goal")?.as_str()?.to_lowercase();
        let rationale = obj
            .get("rationale")
            .and_then(|r| r.as_str())
            .unwrap_or("(no reason given)")
            .to_string();
        let lessons = obj
            .get("lessons")
            .and_then(|l| l.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|x| x.as_str())
                    .map(|s| Lesson { key: "llm".into(), statement: s.to_string(), confidence: 0.6 })
                    .collect()
            })
            .unwrap_or_default();

        let goal = resolve_goal(&goal_kw, ctx)?;
        Some(Deliberation { goal, rationale, lessons })
    }
}

/// Resolve a goal keyword to a concrete [`GoalKind`] using current beliefs.
fn resolve_goal(kw: &str, ctx: &DeliberationContext) -> Option<GoalKind> {
    let pos = ctx.world.me().map(|m| m.pos).unwrap_or(Pos::new(0, 0));
    Some(match kw {
        "forage" => GoalKind::Forage,
        "hydrate" => GoalKind::Hydrate,
        "explore" => GoalKind::Explore,
        "recover" => GoalKind::Recover,
        "flee" => ctx
            .world
            .nearest_threat(pos)
            .map(|t| GoalKind::Flee(t.entity.id))
            .unwrap_or(GoalKind::Explore),
        "investigate" => ctx
            .world
            .nearest_of(EntityKind::Curio, pos)
            .map(|c| GoalKind::Investigate(c.entity.id))
            .unwrap_or(GoalKind::Explore),
        "socialize" => ctx
            .world
            .visible_of(EntityKind::Agent)
            .first()
            .map(|a| GoalKind::Socialize(a.id))
            .unwrap_or(GoalKind::Explore),
        _ => return None,
    })
}

/// When the model is unreachable or replies unusably, fall back to a safe,
/// in-character choice rather than freezing.
fn fallback(ctx: &DeliberationContext) -> Deliberation {
    let dom = ctx.drives.dominant().0;
    Deliberation {
        goal: match dom {
            Drive::Hunger => GoalKind::Forage,
            Drive::Thirst => GoalKind::Hydrate,
            _ => GoalKind::Explore,
        },
        rationale: "the words wouldn't come — I'll trust my gut and keep moving.".into(),
        lessons: vec![],
    }
}

impl<T: Transport> Deliberator for LlmDeliberator<T> {
    fn name(&self) -> &'static str {
        "llm"
    }
    fn deliberate(&mut self, ctx: &DeliberationContext) -> Deliberation {
        let req = self.build_request(ctx);
        match self.transport.send(&req) {
            Ok(reply) => Self::parse_reply(&reply, ctx).unwrap_or_else(|| fallback(ctx)),
            Err(_) => fallback(ctx),
        }
    }
}

/// A real Anthropic transport (blocking HTTP). Reads `ANTHROPIC_API_KEY`.
/// Only compiled with `--features llm-http`.
#[cfg(feature = "llm-http")]
pub struct AnthropicTransport {
    api_key: String,
    base: String,
}

#[cfg(feature = "llm-http")]
impl AnthropicTransport {
    pub fn from_env() -> Result<Self, String> {
        let api_key = std::env::var("ANTHROPIC_API_KEY")
            .map_err(|_| "ANTHROPIC_API_KEY not set".to_string())?;
        Ok(Self { api_key, base: "https://api.anthropic.com/v1/messages".to_string() })
    }
}

#[cfg(feature = "llm-http")]
impl Transport for AnthropicTransport {
    fn send(&self, request_json: &str) -> Result<String, String> {
        let client = reqwest::blocking::Client::new();
        let resp = client
            .post(&self.base)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .body(request_json.to_string())
            .send()
            .map_err(|e| e.to_string())?;
        resp.text().map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persona::Persona;
    use crate::theory_of_mind::TheoryOfMind;
    use daimon_core::{DriveSystem, Entity, EntityId, Memory, Percept, SelfState, WorldModel};

    /// A transport that returns a canned Anthropic-shaped reply — no network.
    struct FakeTransport {
        canned: String,
        pub last_request: std::cell::RefCell<String>,
    }
    impl Transport for FakeTransport {
        fn send(&self, request_json: &str) -> Result<String, String> {
            *self.last_request.borrow_mut() = request_json.to_string();
            Ok(self.canned.clone())
        }
    }

    #[test]
    fn contract_prompt_build_and_parse() {
        // a world the agent can see a curio in
        let mut world = WorldModel::default();
        world.integrate(&Percept {
            tick: 1,
            me: SelfState::new(Pos::new(5, 5)),
            visible: vec![Entity {
                id: EntityId(3),
                kind: EntityKind::Curio,
                pos: Pos::new(6, 5),
                label: "monolith".into(),
            }],
            events: vec![],
        });
        let persona = Persona::new("Vell");
        let drives = DriveSystem::default();
        let memory = Memory::default();
        let social = TheoryOfMind::default();
        let ctx = DeliberationContext {
            tick: 1,
            persona: &persona,
            drives: &drives,
            world: &world,
            memory: &memory,
            social: &social,
            surprise: 0.4,
        };

        let canned = serde_json::json!({
            "content": [{ "type": "text",
                "text": "{\"goal\":\"investigate\",\"rationale\":\"that monolith is unlike anything\",\"lessons\":[\"curios are worth a look\"]}" }]
        })
        .to_string();
        let mut delib = LlmDeliberator::new(
            FakeTransport { canned, last_request: std::cell::RefCell::new(String::new()) },
            "claude-sonnet-4-6",
        );

        // the prompt must actually carry the situation
        let req = delib.build_request(&ctx);
        assert!(req.contains("monolith"));
        assert!(req.contains("Vell"));

        let d = delib.deliberate(&ctx);
        assert!(matches!(d.goal, GoalKind::Investigate(EntityId(3))));
        assert!(d.rationale.contains("monolith"));
        assert_eq!(d.lessons.len(), 1);
    }

    #[test]
    fn unusable_reply_falls_back() {
        let world = WorldModel::default();
        let persona = Persona::new("Kael");
        let drives = DriveSystem::default();
        let memory = Memory::default();
        let social = TheoryOfMind::default();
        let ctx = DeliberationContext {
            tick: 1,
            persona: &persona,
            drives: &drives,
            world: &world,
            memory: &memory,
            social: &social,
            surprise: 0.0,
        };
        let mut delib = LlmDeliberator::new(
            FakeTransport { canned: "not json".into(), last_request: std::cell::RefCell::new(String::new()) },
            "m",
        );
        let d = delib.deliberate(&ctx); // must not panic; returns a safe goal
        assert!(matches!(d.goal, GoalKind::Forage | GoalKind::Hydrate | GoalKind::Explore));
    }
}
