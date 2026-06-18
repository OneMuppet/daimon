//! **Ceiling probe** — is the gen-12 saturation of frontier evolution a WORLD
//! ceiling or an ARCHITECTURE ceiling?
//!
//! `evolve_frontier` proved minds evolve over generations on an auto-ratcheting
//! world, but it SATURATES by ~gen 12 once the population masters difficulty D=1.0 —
//! because the bounded difficulty knob is *clamped* at 1.0. This experiment removes
//! that clamp: it lets difficulty climb UNBOUNDED past 1.0 (genuinely harsher worlds
//! via [`EnvParams::at_difficulty_unbounded`] / [`EnvParamsX`]) and asks whether the
//! population keeps improving.
//!
//! The verdict it returns is ONE of:
//!  - **WORLD-CEILING** — minds kept improving as D rose past 1.0 (the gen-12 plateau
//!    was just the bounded knob); open-endedness ⇒ keep supplying harder worlds.
//!  - **ARCHITECTURE-CEILING** — the population walled out at some D=X (survival can't
//!    recover over many generations) while the world could still get harder;
//!    open-endedness ⇒ richer minds (bigger genome / new faculties / new tasks).
//!  - **WORLD-ENCODING-LIMITED** — every harshness knob maxed out and minds still
//!    coped; need NEW harshness mechanisms to probe further.
//!
//! Design is the PROVEN frontier design, verbatim except the ratchet is UNCAPPED:
//! survival-selection, K fixed-seed low-noise fitness, weak/random init, an
//! auto-difficulty ratchet with NO D≤1.0 cap. Deterministic (one seeded RNG drives
//! everything; per-genome worlds seeded off run seed + gen + slot). Additive: a new
//! example + one reused helper; changes no defaults, no `baseline()`/`showcase()`, no
//! harness path, and does not touch the committed `evolve_frontier.rs`.
//!
//!   cargo run -p daimon-game --example ceiling --release

use daimon_core::Rng;
use daimon_game::poet::{EnvParams, EnvParamsX};
use daimon_mind::evolve::N_GENES;
use daimon_mind::Genome;

// ---------------------------------------------------------------------------
// Hyper-parameters — mirror evolve_frontier, but more generations (we must give the
// architecture every chance to keep climbing before we call a ceiling) and an
// uncapped ratchet.
// ---------------------------------------------------------------------------

const POP: usize = 64;
const K_SEEDS: usize = 5;
const MINDS_PER_WORLD: usize = 6;
const EVAL_TICKS: u64 = 2200;
/// Up to ~100 gens to find the wall; we ALSO stop early if D plateaus for
/// `PLATEAU_GENS` generations (the ratchet has stalled ⇒ the population has found its
/// ceiling, no point burning more sims).
const GENERATIONS: usize = 100;
const PLATEAU_GENS: usize = 15;

const SELECT_FRAC: f32 = 0.22;
const ELITES: usize = 4;
const MUT_SIGMA: f32 = 0.06;

/// Difficulty ratchet — SAME bands as the proven frontier run, but D is UNCAPPED on
/// the way up (no `.min(1.0)`). This is the only behavioural change vs evolve_frontier.
const D_START: f32 = 0.10;
const D_STEP: f32 = 0.04;
const RAISE_ABOVE: f32 = 0.55; // mean graded survival above this → harder
const LOWER_BELOW: f32 = 0.42; // mean graded survival below this → easier

/// Graded-survival bar for "the population SUSTAINED this difficulty" (the capability
/// frontier). Matches the frontier report's threshold.
const SUSTAIN_BAR: f32 = 0.45;

// ---------------------------------------------------------------------------
// Genome init: weak/random with only the open-world capability switches on
// (identical to evolve_frontier::weak_random — same starting line).
// ---------------------------------------------------------------------------

fn weak_random(rng: &mut Rng) -> Genome {
    let mut g = Genome::random(rng);
    g.g[20] = 1.0; // can_fight on
    g.g[21] = 1.0; // can_build on
    g.g[22] = 1.0; // can_die on — MORTALITY
    g.g[24] = 1.0; // can_provision on
    g.g[25] = 0.0; // nn overlay off (no NN in this experiment)
    g.g[26] = 0.0;
    g.g[27] = 0.0;
    g
}

// ---------------------------------------------------------------------------
// Fitness: survival-dominant, averaged over a FIXED seed-set — identical formula to
// evolve_frontier, but on the UNBOUNDED env carrier so it works past D=1.0.
// ---------------------------------------------------------------------------

fn fitness_on_world(genome: &Genome, env: &EnvParamsX, seed: u64) -> (f32, f32) {
    let genomes: Vec<Genome> = (0..MINDS_PER_WORLD).map(|_| genome.clone()).collect();
    let mut world = env.build_world(seed, &genomes);
    let n = world.agents.len().max(1);
    let mut alive_ticks = vec![0u64; n];
    let mut nourish_sum = vec![0.0f64; n];
    let mut carry_sum = vec![0.0f64; n];
    let mut granary_peak = 0.0f64;

    for _ in 0..EVAL_TICKS {
        world.step();
        granary_peak = granary_peak.max(world.granary_food as f64);
        for (i, a) in world.agents.iter().enumerate() {
            if a.alive {
                alive_ticks[i] += 1;
                nourish_sum[i] += ((a.body.energy + a.body.hydration) * 0.5) as f64;
                carry_sum[i] += a.body.carrying as f64;
            }
        }
    }

    let t = EVAL_TICKS as f64;
    let cap = world.granary_capacity().max(1.0) as f64;
    let granary_payoff = (granary_peak / cap).clamp(0.0, 1.0);

    let mut acc = 0.0f64;
    let mut graded_surv = 0.0f64;
    for i in 0..n {
        let surv = alive_ticks[i] as f64 / t;
        graded_surv += surv;
        let nour = if alive_ticks[i] > 0 {
            nourish_sum[i] / alive_ticks[i] as f64
        } else {
            0.0
        };
        let carry = if alive_ticks[i] > 0 {
            (carry_sum[i] / alive_ticks[i] as f64).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let provision = 0.5 * granary_payoff + 0.5 * carry;
        acc += 0.75 * surv + 0.20 * nour + 0.05 * provision;
    }
    ((acc / n as f64) as f32, (graded_surv / n as f64) as f32)
}

fn fitness_avg(genome: &Genome, env: &EnvParamsX, seeds: &[u64]) -> (f32, f32) {
    let mut fsum = 0.0f32;
    let mut ssum = 0.0f32;
    for &sd in seeds {
        let (f, s) = fitness_on_world(genome, env, sd);
        fsum += f;
        ssum += s;
    }
    let k = seeds.len().max(1) as f32;
    (fsum / k, ssum / k)
}

// ---------------------------------------------------------------------------
// Per-generation telemetry.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct GenRow {
    gen: usize,
    d: f32,
    survival_rate: f32,
    mean_fitness: f32,
    best_fitness: f32,
    foresight: f32,
    can_provision: f32,
    forage_drr: f32,
    social_forage: f32,
    cultural: f32,
}

fn gene_summary(pop: &[Genome]) -> [f32; 5] {
    let n = pop.len().max(1) as f32;
    let mut s = [0.0f32; 5];
    for g in pop {
        s[0] += g.g[13]; // foresight gene value
        s[1] += if g.can_provision() { 1.0 } else { 0.0 };
        s[2] += if g.forage_drr() { 1.0 } else { 0.0 };
        s[3] += if g.social_forage() { 1.0 } else { 0.0 };
        s[4] += if g.cultural() { 1.0 } else { 0.0 };
    }
    for v in &mut s {
        *v /= n;
    }
    s
}

// ---------------------------------------------------------------------------
// The generational loop — UNCAPPED ratchet.
// ---------------------------------------------------------------------------

fn main() {
    let run_seed = 0xCE11_1AC2_0E0Du64; // "ceiling"
    let mut rng = Rng::new(run_seed);

    println!("\n=== CEILING PROBE — world-ceiling or architecture-ceiling? ===");
    println!(
        "pop={POP}  K_seeds={K_SEEDS}  minds/world={MINDS_PER_WORLD}  \
         eval_ticks={EVAL_TICKS}  max_generations={GENERATIONS}  plateau_stop={PLATEAU_GENS}"
    );
    println!(
        "select_top={:.0}%  elites={ELITES}  mut_sigma={MUT_SIGMA}  \
         D_start={D_START}  D_step={D_STEP}  ratchet[{LOWER_BELOW},{RAISE_ABOVE}]  D UNCAPPED",
        SELECT_FRAC * 100.0
    );
    println!("fitness = 0.75·survival + 0.20·nourishment + 0.05·provisioning, no NN\n");

    let mut pop: Vec<Genome> = (0..POP).map(|_| weak_random(&mut rng)).collect();
    let mut d = D_START;
    let gain = [1.0f32; N_GENES];

    let mut rows: Vec<GenRow> = Vec::with_capacity(GENERATIONS);
    // Snapshot checkpoints for the head-to-head: gen0, the old-saturation gen12, a
    // mid checkpoint, and the final generation. Captured as we pass them.
    let mut champions: Vec<(usize, Genome)> = Vec::new();
    let mut want_checkpoints = vec![0usize, 12, GENERATIONS / 2, GENERATIONS - 1];

    // plateau detection on the SUSTAINED frontier D.
    let mut best_sustained_d = 0.0f32;
    let mut gens_since_improve = 0usize;
    let mut last_gen = 0usize;

    for gen in 0..GENERATIONS {
        last_gen = gen;
        let seeds: Vec<u64> = (0..K_SEEDS)
            .map(|j| {
                run_seed
                    .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                    .wrapping_add((gen as u64) << 20)
                    .wrapping_add(j as u64 * 0x0001_0001)
                    ^ 0xD1A3_0000
            })
            .collect();

        let env = EnvParams::at_difficulty_unbounded(d);

        let evals: Vec<(f32, f32)> = pop.iter().map(|g| fitness_avg(g, &env, &seeds)).collect();
        let fits: Vec<f32> = evals.iter().map(|e| e.0).collect();

        let mut order: Vec<usize> = (0..POP).collect();
        order.sort_by(|&a, &b| fits[b].total_cmp(&fits[a]));
        let n_parents = ((POP as f32 * SELECT_FRAC).round() as usize).clamp(2, POP);
        let parents: Vec<Genome> = order[..n_parents].iter().map(|&i| pop[i].clone()).collect();

        // capture checkpoint champions (the best genome of this generation).
        if want_checkpoints.contains(&gen) {
            champions.push((gen, pop[order[0]].clone()));
            want_checkpoints.retain(|&c| c != gen);
        }

        let mean_fitness = fits.iter().copied().sum::<f32>() / POP as f32;
        let best_fitness = fits[order[0]];
        let sr = order[..n_parents].iter().map(|&i| evals[i].1).sum::<f32>() / n_parents as f32;
        let gs = gene_summary(&pop);
        rows.push(GenRow {
            gen,
            d,
            survival_rate: sr,
            mean_fitness,
            best_fitness,
            foresight: gs[0],
            can_provision: gs[1],
            forage_drr: gs[2],
            social_forage: gs[3],
            cultural: gs[4],
        });
        println!(
            "gen {:>3}  D={:.2}  grad_surv={:>4.0}%  mean_fit={:.4}  best_fit={:.4}  \
             foresight={:.2}  provis={:>4.0}%  drr={:>4.0}%  social={:>4.0}%  cult={:>4.0}%",
            gen,
            d,
            sr * 100.0,
            mean_fitness,
            best_fitness,
            gs[0],
            gs[1] * 100.0,
            gs[2] * 100.0,
            gs[3] * 100.0,
            gs[4] * 100.0,
        );
        use std::io::Write;
        let _ = std::io::stdout().flush(); // flush per gen so progress is visible

        // track the hardest D the population SUSTAINED (graded survival ≥ bar).
        if sr >= SUSTAIN_BAR && d > best_sustained_d {
            best_sustained_d = d;
            gens_since_improve = 0;
        } else {
            gens_since_improve += 1;
        }

        // REPRODUCE: elitism + mutated offspring.
        let mut next: Vec<Genome> = Vec::with_capacity(POP);
        let n_elite = ELITES.min(n_parents);
        for &i in &order[..n_elite] {
            next.push(pop[i].clone());
        }
        while next.len() < POP {
            let p = &parents[rng.below(parents.len())];
            next.push(p.mutate(MUT_SIGMA, &gain, &mut rng));
        }
        pop = next;

        // RATCHET D — UNCAPPED on the way up (the whole point of this experiment).
        if sr > RAISE_ABOVE {
            d += D_STEP; // NO .min(1.0)
        } else if sr < LOWER_BELOW {
            d = (d - D_STEP).max(0.0);
        }

        // early stop: the ratchet has stalled (no new sustained frontier for a while).
        if gens_since_improve >= PLATEAU_GENS && gen >= 20 {
            println!(
                "\n[early stop] sustained frontier D plateaued at {best_sustained_d:.2} for \
                 {PLATEAU_GENS} generations — the population has found its wall."
            );
            break;
        }
    }

    // make sure we have a final-gen champion even on early stop.
    if !champions.iter().any(|(g, _)| *g == last_gen) {
        // re-evaluate the final population to grab its champion deterministically.
        let seeds: Vec<u64> = (0..K_SEEDS)
            .map(|j| {
                run_seed
                    .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                    .wrapping_add((last_gen as u64) << 20)
                    .wrapping_add(j as u64 * 0x0001_0001)
                    ^ 0xD1A3_0000
            })
            .collect();
        let env = EnvParams::at_difficulty_unbounded(d);
        let fits: Vec<f32> = pop.iter().map(|g| fitness_avg(g, &env, &seeds).0).collect();
        let mut best = 0usize;
        for i in 1..pop.len() {
            if fits[i] > fits[best] {
                best = i;
            }
        }
        champions.push((last_gen, pop[best].clone()));
    }

    report(&rows, best_sustained_d);
    head_to_head(&champions, best_sustained_d);
    use std::io::Write;
    let _ = std::io::stdout().flush();
}

// ---------------------------------------------------------------------------
// Report.
// ---------------------------------------------------------------------------

fn report(rows: &[GenRow], best_sustained_d: f32) {
    println!("\n================ FULL PER-GENERATION TRAJECTORY ================");
    println!(
        "{:>3} {:>5} {:>6} {:>9} {:>9} {:>9} {:>7} {:>6} {:>7} {:>6}",
        "gen", "D", "gsurv%", "mean_fit", "best_fit", "foresight", "provis%", "drr%",
        "social%", "cult%"
    );
    for r in rows {
        println!(
            "{:>3} {:>5.2} {:>5.0}% {:>9.4} {:>9.4} {:>9.2} {:>6.0}% {:>5.0}% {:>6.0}% {:>5.0}%",
            r.gen,
            r.d,
            r.survival_rate * 100.0,
            r.mean_fitness,
            r.best_fitness,
            r.foresight,
            r.can_provision * 100.0,
            r.forage_drr * 100.0,
            r.social_forage * 100.0,
            r.cultural * 100.0,
        );
    }

    // peak D reached (regardless of sustained) and peak sustained D.
    let peak_d = rows.iter().map(|r| r.d).fold(0.0f32, f32::max);
    println!("\nPeak D the ratchet reached:                 {peak_d:.2}");
    println!("Max D SUSTAINED (graded survival ≥ {:.0}%):    {best_sustained_d:.2}", SUSTAIN_BAR * 100.0);
    use std::io::Write;
    let _ = std::io::stdout().flush();
}

// ---------------------------------------------------------------------------
// Confound-free head-to-head + the verdict.
// ---------------------------------------------------------------------------

fn head_to_head(champions: &[(usize, Genome)], best_sustained_d: f32) {
    // Accumulate ALL endgame text into a String so we can both print it AND write it
    // to a results file via std::fs (process-controlled I/O, immune to any stdout
    // capture/buffering quirks on the run host).
    let mut out = String::new();

    // HELD-OUT probe seeds — distinct from any training seed (training seeds are
    // derived with the 0xD1A3_0000 mask; these use a different base entirely).
    let probe_seeds: Vec<u64> =
        (0..8u64).map(|i| 0xF00D_5EED_u64 ^ i.wrapping_mul(0x9E37_79B9_7F4A_7C15)).collect();

    // difficulty ladder that now goes ABOVE 1.0.
    let max_d = best_sustained_d.max(1.0);
    let ladder = [1.0f32, 1.5, 2.0, max_d];

    out.push_str("\n========= HEAD-TO-HEAD: champions on FIXED worlds (held-out seeds) =========\n");
    out.push_str("survival% / fitness — later > earlier at a FIXED D  =>  genuine heritable gain\n");
    out.push_str(&format!("  {:<8}", "D \\ gen"));
    for (g, _) in champions {
        out.push_str(&format!("  gen{g:>3}        "));
    }
    out.push('\n');

    // store survival per (D, champion) for the verdict logic.
    let mut table: Vec<(f32, Vec<(usize, f32)>)> = Vec::new();
    let mut seen_d: Vec<f32> = Vec::new();
    for &df in &ladder {
        if seen_d.iter().any(|&x| (x - df).abs() < 1e-3) {
            continue; // skip a duplicate (e.g. max_d == 1.0 or 2.0)
        }
        seen_d.push(df);
        let env = EnvParams::at_difficulty_unbounded(df);
        let mut line = format!("  D={df:<5.2}");
        let mut row: Vec<(usize, f32)> = Vec::new();
        for (g, champ) in champions {
            let (f, s) = fitness_avg(champ, &env, &probe_seeds);
            line.push_str(&format!("  {:>3.0}%/{:.2}      ", s * 100.0, f));
            row.push((*g, s));
        }
        out.push_str(&line);
        out.push('\n');
        table.push((df, row));
    }

    verdict(&table, champions, best_sustained_d, &mut out);

    // emit: print to stdout AND persist to a results file, force-synced to disk
    // before we return (the run host's stdout capture is lossy for buffered output,
    // so the fsync'd file is the source of truth).
    print!("{out}");
    use std::io::Write;
    let _ = std::io::stdout().flush();
    let path = "ceiling_result.txt"; // cwd (project root) — avoid per-process /tmp ns
    match std::fs::File::create(path) {
        Ok(mut f) => {
            let _ = f.write_all(out.as_bytes());
            let _ = f.flush();
            let _ = f.sync_all(); // force to disk before exit
            println!("\n[results also written to {path}]");
            let _ = std::io::stdout().flush();
        }
        Err(e) => eprintln!("could not write {path}: {e}"),
    }
}

// ---------------------------------------------------------------------------
// THE VERDICT.
// ---------------------------------------------------------------------------

fn verdict(
    table: &[(f32, Vec<(usize, f32)>)],
    champions: &[(usize, Genome)],
    best_sustained_d: f32,
    out: &mut String,
) {
    out.push_str("\n================ VERDICT ================\n");

    // Find the gen-12 champion (the old saturation point) and the final champion.
    let g12 = champions.iter().min_by_key(|(g, _)| (*g as i64 - 12).abs()).map(|(g, _)| *g);
    let g_final = champions.last().map(|(g, _)| *g).unwrap_or(0);

    // For the head-to-head logic we ask: at the HARDER rungs (D ≥ 1.5), does the
    // final champion out-survive the gen-12 champion by a clear margin?
    let surv_at = |df_target: f32, gen: usize| -> Option<f32> {
        table
            .iter()
            .find(|(df, _)| (df - df_target).abs() < 1e-3)
            .and_then(|(_, row)| row.iter().find(|(g, _)| *g == gen).map(|(_, s)| *s))
    };

    let mut late_beats_g12_hard = false; // final clearly > gen12 at some D≥1.5
    let mut late_ge_g12_hard = false; // final ≈≥ gen12 at some D≥1.5 (copes too)
    let mut max_margin = 0.0f32;
    if let (Some(g12), gf) = (g12, g_final) {
        for (df, _) in table {
            if *df < 1.5 - 1e-3 {
                continue;
            }
            if let (Some(s12), Some(sf)) = (surv_at(*df, g12), surv_at(*df, gf)) {
                let margin = sf - s12;
                max_margin = max_margin.max(margin);
                if margin > 0.05 {
                    late_beats_g12_hard = true;
                }
                if sf >= s12 - 0.03 {
                    late_ge_g12_hard = true;
                }
            }
        }
    }

    // Did the population push the FRONTIER past the old 1.0 plateau?
    let pushed_past_1 = best_sustained_d > 1.0 + 1e-3;

    out.push_str(&format!(
        "gen-12 champion = old saturation point;  final champion = gen {g_final}\n\
         max sustained frontier D = {best_sustained_d:.2}  (old bounded plateau was D=1.0)\n\
         late champion's best survival margin over gen-12 at D≥1.5 = {:+.0} pts\n",
        max_margin * 100.0
    ));

    if pushed_past_1 && late_beats_g12_hard {
        out.push_str(&format!(
            "\nVERDICT: **WORLD-CEILING.**\n\
             Minds KEPT IMPROVING as D rose past 1.0 — the population sustained D={best_sustained_d:.2} \
             (well above the old bounded plateau of 1.0), and the late champion out-survives the \
             gen-12 champion at the HARDER rungs (D≥1.5) by up to {:+.0} pts on held-out worlds. \
             The gen-12 saturation in evolve_frontier was the BOUNDED KNOB hitting its 1.0 clamp, \
             not the 28-gene mind walling out. Open-endedness ⇒ keep supplying harder/new worlds.\n",
            max_margin * 100.0
        ));
    } else if !pushed_past_1 && !late_beats_g12_hard {
        out.push_str(&format!(
            "\nVERDICT: **ARCHITECTURE-CEILING.**\n\
             The world kept getting genuinely harder (cold/metabolism/starvation are uncapped past \
             D=1.0) but the population WALLED OUT: it could not sustain difficulty past D={best_sustained_d:.2}, \
             and over the full run the late champion does NOT beat the gen-12 champion at the harder \
             rungs (best margin {:+.0} pts). Many generations did not let survival recover. The 28-gene \
             mind is the limit. Open-endedness ⇒ richer minds (bigger genome / new faculties / new tasks).\n",
            max_margin * 100.0
        ));
    } else if late_ge_g12_hard && !pushed_past_1 {
        // The minds COPE at higher D (final ≈ gen12) but the ratchet never SUSTAINED
        // past 1.0 at the 45% bar — and we should check whether knobs saturated.
        out.push_str(&format!(
            "\nVERDICT: **WORLD-ENCODING-LIMITED (leaning).**\n\
             The late champion copes about as well as gen-12 even at D≥1.5 (margin {:+.0} pts) yet the \
             ratchet never SUSTAINED the 45%% bar past D={best_sustained_d:.2}. The stalker (bite→1.3, \
             period→1) and scarcity (patch floor) knobs SATURATE past D=1.0, so harsher worlds lean \
             increasingly on cold/metabolism alone. Read with the trajectory: if survival hovers at the \
             ratchet band without collapsing, the encoding — not the architecture — is the active limit; \
             new harshness mechanisms (multiple predators, shorter year) are needed to probe further.\n",
            max_margin * 100.0
        ));
    } else {
        // Mixed: frontier rose past 1.0 but late doesn't clearly beat g12 (or vice
        // versa). Report honestly without forcing a clean label.
        out.push_str(&format!(
            "\nVERDICT: **MIXED / INCONCLUSIVE — reported honestly.**\n\
             frontier pushed past D=1.0: {}  (max sustained D={best_sustained_d:.2});  \
             late champion clearly beats gen-12 at D≥1.5: {}  (best margin {:+.0} pts).\n\
             The two signals don't agree cleanly. Likely the frontier crept past 1.0 on cold/metabolism \
             alone (stalker + scarcity saturated) while held-out survival gains were within noise. Lean: \
             the WORLD-ENCODING is the active limit near this D; a bigger run or new harshness mechanisms \
             would sharpen the call. NOT forcing a clean world/architecture verdict the data doesn't support.\n",
            yn(pushed_past_1),
            yn(late_beats_g12_hard),
            max_margin * 100.0
        ));
    }
}

fn yn(b: bool) -> &'static str {
    if b {
        "YES"
    } else {
        "no"
    }
}
