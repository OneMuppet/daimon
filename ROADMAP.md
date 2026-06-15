# Roadmap — from concept to shippable runtime

Legend: ✅ built + tested in this repo · ◑ seam exists, implementation external ·
📋 designed-for, not yet built

This is an honest map of the distance between the concept here and a production
NPC runtime. The architecture is complete; most of what remains is *integration*
(a model, an engine) rather than unbuilt cognition.

## Cognition (the architecture)
- ✅ Cognitive cycle: perceive → appraise → reflex → decide → plan → act → reflect
- ✅ Drive system: homeostatic needs + intrinsic curiosity, persona-biased
- ✅ Dual-process control + rate-limited escalation policy (~10% slow-path)
- ✅ BDI intentions with commitment + hysteresis + critical-need override
- ✅ Tripartite memory: episodic (salience recall + forgetting), semantic, procedural
- ✅ Spatial memory (return to remembered resources)
- ✅ Theory of mind with relationship history
- ✅ Reflection / consolidation into durable knowledge
- ✅ Self-narration (legibility), tagged by cognitive mode
- ✅ Persona system → emergent behavioural diversity
- ✅ Deterministic, reproducible lives (seeded PRNG); 13 passing tests

## The System-2 seam
- ✅ `Deliberator` trait + serialisable `DeliberationContext` + offline default
- ◑ **`LlmDeliberator` backed by Claude** — render context → prompt, request a
      goal + chain-of-thought rationale + lessons, parse structured reply.
      ReAct / Reflexion / Tree-of-Thoughts compose behind the same trait.
- 📋 Caching / batching of deliberations across nearby agents
- 📋 Budget metering per agent (calls, tokens) — tie to Reins-style governance

## World & embodiment
- ✅ Deterministic grid-world testbed (food, water, curios, predator, townsfolk)
- 📋 Embodiment in a real engine (Bevy / Unity / Unreal) via the bounded action surface
- 📋 Perception from pixels (SIMA-style) instead of a structured percept
- 📋 Continuous space + navmesh pathing (replace greedy grid steps)
- 📋 A learned forward/world model for principled surprise + planning
      (MuZero-style), with MCTS for hard deliberations

## Memory & learning
- ✅ In-process memory with forgetting and confidence
- 📋 Vector-indexed episodic recall (embeddings) for large memory streams
- 📋 Persist a live mind to disk; resume; author starting memories/persona
- 📋 Cross-session learning (a character that remembers you next session)

## Scale & society
- ✅ Single agent, multiple background townsfolk
- 📋 Many fully-cognitive agents sharing a world (Project Sid / PIANO-style)
- 📋 Emergent culture, economy, and roles across a population
- 📋 Director/narrative layer that sets soft goals for a cast

## Production hardening
- 📋 Authoring tools (persona/creed editor, memory seeding, debugging the mind)
- 📋 Observability: trace a decision back through drives/memory/deliberation
- 📋 Governance integration (Reins): bounded contracts, budgets, HITL, kill-switch, audit
- 📋 Performance budget per frame; async deliberation off the main thread
