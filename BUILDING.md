# Emergent building — design

**Goal (verbatim intent):** minds that can *build* — walls, floors, stairs, burrows
— where *what* they build and *why* is **emergent**, never scripted. The driver is
an innate **sense of safety/home**: every organism feels secure when sheltered
(animals burrow, sleep in caves; we build houses). Put that felt need in the mind
and it should drive the building on its own. Building itself must be **efficient in
Rust + wgpu 29**.

## The principle: script the *need*, not the *structure*

We add **one homeostatic need (Shelter/Security)** and **one generic affordance
(place/dig a block)** — and let the *existing* utility + Praxis + planning layers
discover that building reduces the need. No "build a house" action, no blueprint.

- **Enclosure(cell) ∈ [0,1]** — how protected a spot is: fraction of the 4 sides
  walled + roof overhead + burrow depth. Open ground = 0; a walled, roofed cell = 1.
- **Shelter need** rises with *exposure* `(1 − enclosure)`, amplified by **night**
  and **predator proximity** (the felt threat of being caught in the open). It is
  *satisfied* by being enclosed. It feeds **affect**: sheltered → calm/content;
  exposed at night → afraid (valence↓, arousal↑) — the body-feeling of "I need to
  get home / make myself safe."
- **Affordances:** `Place(kind, cell)` (wall / floor / pillar) and `Dig(cell)`
  (burrow down). Both are generic — the agent can place a block on an adjacent cell
  or dig into the earth. Nothing tells it *which*.
- **Emergence:** when the shelter need is high, the decide/plan layer scores
  actions by *expected need reduction* = the enclosure gained. Placing a wall on an
  *open* side raises enclosure → big drop in the need → high value → the agent does
  it. Repeating that, side by side, **surrounds itself → a shelter appears**.
  - A lone, anxious agent at night **walls itself in**.
  - On a slope or to escape a ground threat it builds **up** — and needs **floors +
    stairs** to stand on / climb, which emerge because a raised cell with no floor
    has no enclosure and no footing.
  - The cave instinct = **Dig**: burrowing down is the cheapest enclosure where the
    earth already walls 4 sides.
  - With stigmergy on (a gene), agents deposit pheromone on built sites → others
    add to the same structure → **co-built shelters / villages** emerge.
  - The *form follows the need gradient + terrain*, so the **what and why differ per
    agent and situation** — the exciting part.

## Cost (so it's a real trade-off, not free over-building)

Placing/digging costs **energy** (it tires you) — so the mind weighs *build shelter*
vs *rest / forage*. v1.5: a **material** (stone from rock nodes, wood from trees —
resources already on the island), tying shelter to foraging exactly like nest-
building. This is what stops every agent from trivially walling in instantly.

## Efficient rendering (wgpu 29)

Blocks are a **sparse voxel set** in the world (`HashMap<(i16,i16,i16), BlockKind>`).
Rendered by **GPU instancing**: one unit-cube mesh (36 verts) + a per-block
**instance buffer** (position, size, colour/kind). **One instanced draw call** for
*all* blocks regardless of count (`pass.draw(0..36, 0..n_blocks)` with an
instance-step vertex buffer). The instance buffer is rebuilt only when the structure
set changes (a dirty flag), never per frame — so thousands of blocks across a
thousand minds cost one draw and one occasional upload. New `blocks` pipeline sits
beside the existing `lit / water / add` passes; lit by the same faceted normals.

## Roadmap

1. **Sim** (`daimon-core` + `sim`): a `Structures` store; `Action::Place`/`Dig`;
   `enclosure(cell)`; energy cost; *all new RNG gene-gated* so the seeded harness
   stays bit-identical when building is off (the project's determinism discipline).
2. **Cognition** (`daimon-mind`): the **Shelter need** in the drive/appraisal
   system; affect coupling (sheltered = calm, exposed-at-night = afraid); the
   planner scoring `Place`/`Dig` by enclosure-gain so building is chosen, not told.
   A `can_build` gene (default off → ACs unchanged; on in the showcase).
3. **Render** (`daimon-game`): the instanced block pipeline; blocks drawn on the
   island, worn/lit like the terrain.
4. **Verify**: a believability criterion (ablation) — *exposed agents at night
   raise enclosure (build/dig); a no-shelter-need control does not* — plus the
   field study reports built-structure counts. Keep it honest: if shelters don't
   emerge, that's a finding, not a failure.

## Why this fits Daimon

This is the same move as Praxis (concept/goal genesis) and the healer demo: give a
*drive* + an *affordance*, and let architecture produce behaviour no one coded.
"Build a home because you feel safer there" is the most human version of that.
