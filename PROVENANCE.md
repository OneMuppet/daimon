# Provenance

Daimon is an original, independently developed cognitive architecture and Rust
implementation for autonomous game agents, created beginning 2026-06-13.

## What it builds on

Daimon composes widely published, public-domain concepts from AI, cognitive
science, and game development:

- **Belief–Desire–Intention agents** (Bratman 1987; Rao & Georgeff 1995) — the
  goal/intention/commitment loop.
- **Dual-process cognition** (Kahneman 2011; Booch et al., *Thinking Fast and
  Slow in AI*, AAAI 2021; DeepMind Talker–Reasoner 2024) — the System-1/System-2
  controller and escalation policy.
- **Homeostatic and intrinsic motivation** (Hull's drive reduction; Schmidhuber's
  formal theory of curiosity 2010; Pathak et al. ICM 2017; Friston's free-energy
  principle 2010) — the drive system and curiosity.
- **The memory-stream + reflection pattern** (Park et al., *Generative Agents*,
  UIST 2023) and the declarative/procedural division of classic cognitive
  architectures (SOAR — Laird et al. 1987; ACT-R — Anderson et al. 2004).
- **Skill libraries / lifelong learning** (Wang et al., *Voyager*, TMLR 2023).
- **Agentic LLM reasoning patterns** that fit the deliberator seam (ReAct — Yao
  et al. 2023; Reflexion — Shinn et al. 2023; Tree of Thoughts — Yao et al. 2023).
- **Theory of mind for agents** (Rabinowitz et al., *Machine Theory of Mind*,
  ICML 2018).
- **Game-AI planning** (GOAP — Orkin 2006; HTN — Erol, Hendler & Nau 1994;
  behaviour trees — Colledanchise & Ögren 2018).
- **Model-based planning** as a documented future direction (MuZero —
  Schrittwieser et al. 2020; MCTS survey — Browne et al. 2012).

All of the above are cited in `WHITEPAPER.md` with authors, venues, and URLs.

## What it was NOT built from

- No source code, internal documentation, design documents, or other non-public
  material from any third-party product (Inworld, NVIDIA ACE, Altera, DeepMind,
  Stanford, or any other) was available to, accessed by, or used by the author.
  All third-party systems referenced were observable only through their public
  research papers and public marketing descriptions.
- All code, types, wire-level data structures, the world simulation, the
  escalation policy, the commitment/hysteresis scheme, and the persona system in
  this repository were authored from scratch for this project.
- Domain vocabulary used here (drive, belief, episodic/semantic/procedural
  memory, theory of mind, deliberation, reflection, behaviour tree, GOAP, HTN) is
  standard terminology defined in the public literature listed above.

## Authorship

Initial design and implementation: 2026, with AI-assisted development (Claude
Code). The architecture, its composition, and the Rust codebase are original
work; the intellectual lineage is the public literature cited throughout.
