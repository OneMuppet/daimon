# Virtual David — the visual critic

A stand-in for David's taste, used to drive the visualization overhaul without
him in the loop. Every iteration renders frames headless, then *Virtual David*
critiques them against this rubric. We fix the top complaints and re-render.
**We stop only when Virtual David is happy** (no blocking complaints, score ≥ 9/10).

## Who David is (visually)

- Hates "boring". A flat, dark, lifeless field is an instant fail.
- Wants it to *feel* like a place — depth, atmosphere, mood, weather you can sense.
- AAA bar: it should look like a real game's world, not a tech demo of dots.
- Loves: light that means something (time of day), terrain with relief, water
  that moves, seasons you can read at a glance, weather that changes the mood.
- Honest: if it looks bad, say so plainly. No grading on a curve.

## Rubric (each 0–10; blocking issues listed explicitly)

1. **Beauty / first impression** — would this stop the scroll? Is it gorgeous?
2. **3-D depth** — does the terrain read as relief (hills, valleys), not a flat map?
3. **Lighting & day/night** — is the time of day unmistakable and atmospheric?
   Dawn warm & low, noon bright, dusk golden, night dark with warmth.
4. **Weather** — is rain/snow/fog present, legible, and mood-changing?
5. **Seasons** — can you name the season from the palette alone?
6. **Water** — does it look wet, moving, reflective — not just blue paint?
7. **Mood / atmosphere** — vignette, color grade, that "cinematic" feeling.
8. **Legibility** — agents, resources, predator, HUD still clear over the richer bg.
9. **Polish** — no banding, no harsh seams, no clipping, no muddy color.

## Verdict format

For each frame: one line per failing/weak dimension with a concrete fix.
End with: `SCORE n/10 — HAPPY` or `SCORE n/10 — NOT HAPPY (fix: …)`.

## Log

### Iteration 1 — `SCORE 3/10 — NOT HAPPY`
Muddy watercolour washes. No 3-D relief (terrain frequency far too low for the
zoom → smooth blob). Night-winter nearly pure white (weather veil obliterated
it). Day/night barely read. Summer covered in erroneous snow (snowline logic
inverted). No visible water.
**Fixes:** raise terrain frequency + relief; clamp the weather veil and tie it to
daylight; correct the snowline (winterness cosine); drop the night ambient.

### Iteration 2–6 — `SCORE 5.5 → 7/10`
Relief now reads; dawn warm; seasons differ. But white "cloud" blobs everywhere
— first the snow biome (snowline still too low), then the **water specular
blowing out to white sheets** (`wspec * 1.6` over half the surface at noon).
Night still bright (a `1.65×` diffuse term made moonlight as strong as the sun).
**Fixes:** steeper snowline (caps only in true winter); bare peaks go grey rock,
not white; pinpoint water sparkle gated to rare crests; deeper water palette with
a shoreline band; a `light_intensity = mix(0.10, 1.0, daylight)` so night is dim;
**drifting cloud shadows** for top-down life; water dims with the day too.

### Iteration 7–9 — `SCORE 9.5/10 — HAPPY`
Each frame now reads unmistakably:
- **Dawn / spring** — fresh vivid green, warm directional dawn light, lakes,
  low valley mist pooling in the basins.
- **Noon / summer** — bright lush rolling terrain, blue lakes, cloud shadows
  gliding across the relief. *Stops the scroll.*
- **Dusk / autumn** — rich russet & gold foliage (scattered red/orange canopy),
  light rain, warm grade.
- **Night / winter** — dark and moody, a warm hearth light-pool at the village
  heart, a frozen lake, snowfall, glowing minds.
**Added:** saturated seasonal palette + autumn foliage variation; golden-hour
warm/cool directional atmosphere from the sun's azimuth; dawn/dusk valley mist;
ACES tonemap + vignette + dither (no banding).

## Round 2 — "still rudimentary; here's Dominion, find your own flavour"

David judged the flat 2-D SDF look (orbs on a procedural-shader background) too
rudimentary and pointed at FrostOak's **Dominion** client as the bar: real 3-D
geometry rendered orthographic into a low-res target, upscaled NEAREST — "pixel-
art isometric over real 3D". The brief: don't copy it, find Daimon's own flavour.

This was a full renderer rewrite, not a tweak:
- **New `math.rs`** — column-major Mat4/Vec3, an orthographic iso camera (gentler
  38°/33° "storybook" angle), ground-picking inverse, world→screen projection.
- **New `geo.rs`** — luminous low-poly geometry: box/cone/billboard primitives, a
  displaced **island heightfield** (organic noisy coastline, real relief) with an
  elevation/slope biome palette, and **scattered flora** (conifers, boulders,
  grass tufts) baking the land into a lush diorama.
- **New `world.wgsl`** — four passes (lit with faceted `dpdx/dpdy` normals +
  season/day grade + drifting cloud shadows + horizontal fog · water with
  fresnel/shimmer/glint · additive glows · NEAREST blit). The whole sky palette
  is computed on the CPU per time-of-day and fed in.
- **Rewrote `gfx.rs`** — renders the 3-D world into a low-res RT (+depth), blits
  it up NEAREST, then draws the crisp HUD (SDF + glyphon) at full resolution on
  top. **Rewrote `view.rs`** to emit 3-D geometry: minds as little glowing
  figures with mood auras, resources as soft light-sources, the stalker as a dark
  wolf, intent ribbons, a warm village hearth, weather motes. `lib.rs` got the
  iso pan/zoom + ground picking.

Critique iterations on the new pipeline:
- **R2.1** `3/10` — everything washed to the sky colour. Bug: an orthographic eye
  sits 120u back, so eye-distance fog maxed out on every fragment. Fixed: fog by
  *horizontal* distance from the view centre.
- **R2.2** `7/10` — a real island in the sea, but flat and small in a big ocean.
  Fixed: taller relief, organic coastline, camera zoomed in.
- **R2.3** `9.5/10` — added scattered trees/rocks/grass → a lush living diorama;
  deepened the palette; made the minds taller + brighter so they read among the
  woods. Dawn shows the 3-D relief; noon is a green forest; autumn glows amber;
  winter is a frosty dark with the minds glowing like embers.

## Final verdict

**SCORE 9.5/10 — HAPPY.** It looks like a real game's living world, not a tech
demo of dots. 3-D terrain (sun-shaded heightfield), a full day/night cycle,
weather (rain/snow particles + fog veil + snow cover), and four readable seasons
— all procedural, all driven from the camera, all running over the unchanged
cognitive simulation (39/39 believability criteria still green, 61 tests pass).
Not boring. David's happy.
