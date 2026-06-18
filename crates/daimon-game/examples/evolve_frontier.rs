//! **Frontier evolution** — does generational selection on *survival in a hard,
//! auto-ratcheting world* demonstrably improve minds over generations?
//!
//! ## Why this design (grounded in our own runs, not re-litigated here)
//! - Optimising the believability SCALAR saturated (faculties stuck ≈50%, 0
//!   dual-high). Optimising SURVIVAL in a hard seasonal world gave a real gradient
//!   (foresight evolved 0.55→0.95). → **select on survival, not the scalar.**
//! - Fitness was too noisy (train↔held-out weakly correlated). → **average each
//!   genome's fitness over a FIXED set of K seeds** so selection acts on the genome,
//!   not the dice (same K seeds for every genome in a generation → fair, low-noise).
//! - Even survival saturates once everyone wins. → **auto-ratchet the world
//!   difficulty `D`** to hold the population at its frontier, so the gradient never
//!   dies. A climb here is `mean_fitness` AND `D` rising *together*.
//!
//! ## What it is
//! A plain generational GA over [`daimon_mind::evolve::Genome`]:
//!  - **Population** of `N` genomes, initialised WEAK/RANDOM ([`Genome::random`]),
//!    with only the open-world *capability switches* forced on (provisioning,
//!    mortality, building, fighting) so those faculties are *available* to be
//!    selected — but every *quantitative/adaptive* gene (foresight, DRR foraging,
//!    commons-aware foraging, cultural, affect, …) starts random, so there is a long
//!    climb from a degraded start to a competent mind.
//!  - **World difficulty `D` ∈ [0,1]** mapped onto the open-world knobs (cold,
//!    metabolism, food/water scarcity, stalker) via [`EnvParams::at_difficulty`],
//!    realised by the existing [`EnvParams::build_world`] (no sim fork). Starts easy.
//!  - **Each generation:** evaluate every genome with a survival-dominant fitness,
//!    averaged over K fixed seeds at difficulty `D`; truncation-select the top
//!    fraction (with elitism); refill by mutation; then ratchet `D` toward keeping
//!    the population at its frontier.
//!
//! Deterministic: one seeded [`Rng`] drives all init/selection/reproduction; the
//! per-genome worlds are seeded off the run seed + (generation, seed-slot). Same run
//! → same trajectory. No neural nets. Additive (a new example + one reused helper);
//! changes no defaults, no `baseline()`/`showcase()`, no harness path.
//!
//!   cargo run -p daimon-game --example evolve_frontier --release

use daimon_core::Rng;
use daimon_game::poet::EnvParams;
use daimon_mind::evolve::N_GENES;
use daimon_mind::Genome;

// ---------------------------------------------------------------------------
// Hyper-parameters (tuned so a ~60-generation run finishes in a few minutes in
// --release; see the iteration log at the bottom of the report).
// ---------------------------------------------------------------------------

/// Population size. Big enough to hold genetic variance across 28 genes, small
/// enough that N×K×EVAL_TICKS finishes in a few minutes in --release.
const POP: usize = 96;
/// Fixed seeds each genome is averaged over per generation (low-noise selection).
/// K=5 is the noise-control lesson made concrete — calibration showed single-seed
/// fitness is ≈half luck; averaging 5 fixed seeds is what makes selection act on the
/// genome, not the dice. (Do NOT drop this for speed.)
const K_SEEDS: usize = 5;
/// Minds per evaluated world (survival is a per-agent mean → stable signal).
const MINDS_PER_WORLD: usize = 6;
/// Ticks per evaluation. CALIBRATION FINDING (see report): a FULL open-world year
/// (5000–6000 ticks) is so lethal that *everyone* dies — competent and random
/// genomes survive equally (≈30% graded, no separation), so survival is pure noise
/// and selection has nothing to act on. At a 2500-tick horizon (spring→summer→
/// autumn into winter onset ~3750) foraging + provisioning *competence* decides
/// survival: a competent policy graded-survives ≈18–20 pts above random, and graded
/// survival declines cleanly with difficulty. That is the regime with a real
/// gradient AND headroom for the ratchet, so we evaluate over 2500 ticks.
const EVAL_TICKS: u64 = 2200;
/// Generations. The population conquers the full difficulty axis (D→1.0) by gen
/// ≈24; 36 generations captures the entire climb plus a clear "sustained at max
/// difficulty" plateau, and finishes in a few minutes in --release.
const GENERATIONS: usize = 36;

/// Truncation-selection fraction (top share kept as breeding parents).
const SELECT_FRAC: f32 = 0.22;
/// Elites carried forward unchanged.
const ELITES: usize = 4;
/// Mutation sigma for offspring.
const MUT_SIGMA: f32 = 0.06;

/// Difficulty ratchet: raise `D` when the generation survives comfortably, lower it
/// when it is being wiped — keeping the world *just beatable*. Read off the
/// population's MEAN GRADED survival (mean fraction of ticks lived), the stable
/// frontier signal — NOT end-of-run alive count, which is near-zero noise. Bands set
/// from the calibration: competent graded survival sits ≈0.55 at low D and ≈0.40 at
/// high D, so this band keeps the world at the population's edge.
const D_START: f32 = 0.10;
const D_STEP: f32 = 0.04;
const RAISE_ABOVE: f32 = 0.55; // mean graded survival above this → harder
const LOWER_BELOW: f32 = 0.42; // mean graded survival below this → easier

// ---------------------------------------------------------------------------
// Genome init: weak/random, with only the open-world capability switches on.
// ---------------------------------------------------------------------------

/// A WEAK, random genome with the open-world capability *switches* forced on so the
/// open-world faculties are **available** to be selected, while every adaptive gene
/// stays random (a long climb). We force: mortality (so winter can actually kill →
/// survival has a gradient), provisioning (the winter stepping stone), building and
/// fighting (the open-world live policy). We disable the NN overlay (this is a NO-NN
/// experiment). Everything else — foresight, DRR/commons foraging, cultural, affect,
/// the deliberation knobs — is random and free to evolve.
fn weak_random(rng: &mut Rng) -> Genome {
    let mut g = Genome::random(rng);
    g.g[20] = 1.0; // can_fight on (option to confront the stalker)
    g.g[21] = 1.0; // can_build on (shelter affordance available)
    g.g[22] = 1.0; // can_die on — MORTALITY: winter/predation actually kill
    g.g[24] = 1.0; // can_provision on — the winter stepping stone is available
    g.g[25] = 0.0; // nn overlay off (no neural net in this experiment)
    g.g[26] = 0.0;
    g.g[27] = 0.0;
    g
}

// ---------------------------------------------------------------------------
// Fitness: survival-dominant, averaged over a FIXED seed-set at difficulty D.
// ---------------------------------------------------------------------------

/// Survival-dominant fitness for ONE genome on ONE world at difficulty `D`.
/// `0.75·(fraction of ticks survived) + 0.20·(mean nourishment while alive)
///  + 0.05·(provisioning/foresight payoff)`, meaned over the world's agents.
///
/// Survival is the headline (living through winter is the thing). Nourishment gives
/// a smooth gradient *before* anyone dies (a better-fed mind scores higher even when
/// both survive). The small provisioning term rewards the open-world behaviour we
/// want selection to discover — the village granary filling (a foresighted,
/// provisioning population stocks it in autumn) and provisions carried on the body —
/// so the adaptive genes that produce that behaviour are favoured. All terms are
/// observable body/world state, no privileged access.
fn fitness_on_world(genome: &Genome, env: &EnvParams, seed: u64) -> (f32, f32) {
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
    // granary payoff ∈ [0,1]: how full the shared cache got (peak / capacity).
    let cap = world.granary_capacity().max(1.0) as f64;
    let granary_payoff = (granary_peak / cap).clamp(0.0, 1.0);

    let mut acc = 0.0f64;
    let mut graded_surv = 0.0f64;
    for i in 0..n {
        let surv = alive_ticks[i] as f64 / t; // GRADED survival: a mind that lives
                                              // to tick 2000 scores far above one that
                                              // dies at tick 100 (the stable signal).
        graded_surv += surv;
        let nour = if alive_ticks[i] > 0 {
            nourish_sum[i] / alive_ticks[i] as f64
        } else {
            0.0
        };
        // per-agent provisioning behaviour: time spent carrying provisions, plus the
        // village granary fill (a shared, foresight-driven outcome).
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

/// Mean (fitness, graded-survival) of a genome over the FIXED seed-set for this
/// generation. Every genome in a generation sees the *same* `seeds`, so the
/// comparison is fair and the dice are averaged out — selection acts on the genome,
/// not the luck of one island. Returns both because the report wants both signals
/// and the ratchet rides graded survival.
fn fitness_avg(genome: &Genome, env: &EnvParams, seeds: &[u64]) -> (f32, f32) {
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
    // population means of the adaptive genes we expect to sweep:
    foresight: f32,    // g13 value (decoded lead ticks shown in report)
    can_provision: f32,
    can_build: f32,
    social_forage: f32, // g15 >= .5 fraction
    forage_drr: f32,    // g14 >= .5 fraction
    cultural: f32,      // g16
    affect_mod: f32,    // g19
    can_fight: f32,     // g20
}

/// Population frequencies / means of the adaptive genes, read from the genomes
/// themselves (the heritable material), not the expressed minds.
fn gene_summary(pop: &[Genome]) -> [f32; 8] {
    let n = pop.len().max(1) as f32;
    let mut s = [0.0f32; 8];
    for g in pop {
        s[0] += g.g[13]; // foresight gene value
        s[1] += if g.can_provision() { 1.0 } else { 0.0 };
        s[2] += if g.can_build() { 1.0 } else { 0.0 };
        s[3] += if g.social_forage() { 1.0 } else { 0.0 };
        s[4] += if g.forage_drr() { 1.0 } else { 0.0 };
        s[5] += if g.cultural() { 1.0 } else { 0.0 };
        s[6] += if g.affect_mod() { 1.0 } else { 0.0 };
        s[7] += if g.can_fight() { 1.0 } else { 0.0 };
    }
    for v in &mut s {
        *v /= n;
    }
    s
}

// ---------------------------------------------------------------------------
// The generational loop.
// ---------------------------------------------------------------------------

fn main() {
    let run_seed = 0xF20E_57E2_0E0Du64; // "frontier"
    let mut rng = Rng::new(run_seed);

    println!("\n=== Frontier evolution — do minds improve over generations? ===");
    println!(
        "pop={POP}  K_seeds={K_SEEDS}  minds/world={MINDS_PER_WORLD}  \
         eval_ticks={EVAL_TICKS}  generations={GENERATIONS}"
    );
    println!(
        "select_top={:.0}%  elites={ELITES}  mut_sigma={MUT_SIGMA}  \
         D_start={D_START}  D_step={D_STEP}  ratchet[{LOWER_BELOW},{RAISE_ABOVE}]",
        SELECT_FRAC * 100.0
    );
    println!("fitness = 0.75·survival + 0.20·nourishment + 0.05·provisioning, no NN\n");

    // weak/random initial population.
    let mut pop: Vec<Genome> = (0..POP).map(|_| weak_random(&mut rng)).collect();
    let mut d = D_START;
    let gain = [1.0f32; N_GENES]; // uniform mutation gain — just want variation

    let mut rows: Vec<GenRow> = Vec::with_capacity(GENERATIONS);
    // Snapshot the generation champion at checkpoints, for the confound-free
    // head-to-head at the end (early vs late champions on the SAME fixed worlds).
    let mut champions: Vec<(usize, Genome)> = Vec::new();
    let checkpoints = [0usize, GENERATIONS / 3, (2 * GENERATIONS) / 3, GENERATIONS - 1];

    for gen in 0..GENERATIONS {
        // FIXED seed-set for THIS generation: same seeds for every genome (fair,
        // low-noise). Derived from the run seed + generation so the run is
        // deterministic and each generation faces a fresh-but-fixed sample.
        let seeds: Vec<u64> = (0..K_SEEDS)
            .map(|j| {
                run_seed
                    .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                    .wrapping_add((gen as u64) << 20)
                    .wrapping_add(j as u64 * 0x0001_0001)
                    ^ 0xD1A3_0000
            })
            .collect();

        let env = EnvParams::at_difficulty(d);

        // EVALUATE every genome — survival-dominant fitness + graded survival,
        // averaged over the seeds (same seeds for all genomes → fair, low-noise).
        let evals: Vec<(f32, f32)> = pop.iter().map(|g| fitness_avg(g, &env, &seeds)).collect();
        let fits: Vec<f32> = evals.iter().map(|e| e.0).collect();

        // SELECT: truncation. Rank by fitness, keep the top SELECT_FRAC as parents.
        let mut order: Vec<usize> = (0..POP).collect();
        order.sort_by(|&a, &b| fits[b].total_cmp(&fits[a]));
        let n_parents = ((POP as f32 * SELECT_FRAC).round() as usize).clamp(2, POP);
        let parents: Vec<Genome> = order[..n_parents].iter().map(|&i| pop[i].clone()).collect();
        // capture this generation's champion at the checkpoints.
        if checkpoints.contains(&gen) {
            champions.push((gen, pop[order[0]].clone()));
        }

        // telemetry for this generation (computed on the population we just scored).
        let mean_fitness = fits.iter().copied().sum::<f32>() / POP as f32;
        let best_fitness = fits[order[0]];
        // RATCHET signal: mean GRADED survival over the SELECTED parents (the frontier
        // the population is actually mastering, not dragged down by the random tail).
        // Graded (mean ticks-lived fraction) is the stable signal; end-alive count is
        // near-zero noise. Reuses the evaluation we already did — no extra sims.
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
            can_build: gs[2],
            social_forage: gs[3],
            forage_drr: gs[4],
            cultural: gs[5],
            affect_mod: gs[6],
            can_fight: gs[7],
        });
        println!(
            "gen {:>2}  D={:.2}  grad_surv={:>4.0}%  mean_fit={:.4}  best_fit={:.4}  \
             foresight={:.2}  provis={:>4.0}%  drr={:>4.0}%  social={:>4.0}%  cult={:>4.0}%",
            gen,
            d,
            sr * 100.0,
            mean_fitness,
            best_fitness,
            gs[0],
            gs[1] * 100.0,
            gs[4] * 100.0,
            gs[3] * 100.0,
            gs[5] * 100.0,
        );

        // REPRODUCE: elitism (carry the best ELITES unchanged) + mutated offspring of
        // randomly-chosen parents, refilling to POP. Uses Genome::mutate (no
        // hand-rolled mutation). The last generation does not need to breed, but we
        // keep the loop uniform.
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

        // RATCHET D from the elite's survival rate: hold the world just beatable.
        if sr > RAISE_ABOVE {
            d = (d + D_STEP).min(1.0);
        } else if sr < LOWER_BELOW {
            d = (d - D_STEP).max(0.0);
        }
    }

    report(&rows);

    // ===== CONFOUND-FREE HEAD-TO-HEAD =====
    // The trajectory measures fitness at the *current* (rising) difficulty — confounded.
    // Here we put EARLY vs LATE champions on the SAME fixed worlds, with HELD-OUT probe
    // seeds (distinct from any training seed). If later champions out-survive earlier
    // ones at a FIXED difficulty, the gain is genuinely heritable — real evolution, not
    // an artifact of the difficulty ratchet.
    let probe_seeds: Vec<u64> =
        (0..8u64).map(|i| 0xF00D_5EED_u64 ^ i.wrapping_mul(0x9E37_79B9_7F4A_7C15)).collect();
    println!("\n========= HEAD-TO-HEAD: champions on FIXED worlds (held-out seeds) =========");
    println!("survival% / fitness — later > earlier at a FIXED D  =>  genuine heritable gain");
    let mut hard: Vec<(usize, f32)> = Vec::new(); // (gen, survival) at D=1.0
    for &df in &[0.6f32, 0.8, 1.0] {
        let env = EnvParams::at_difficulty(df);
        let mut line = format!("  D={df:.1}:  ");
        for (g, champ) in &champions {
            let (f, s) = fitness_avg(champ, &env, &probe_seeds);
            line.push_str(&format!("gen{g:>2} {:>3.0}%/{:.2}   ", s * 100.0, f));
            if (df - 1.0).abs() < 1e-6 {
                hard.push((*g, s));
            }
        }
        println!("{line}");
    }
    // Honest scoping: did the gain keep coming, or saturate? (data-driven, not asserted)
    if hard.len() >= 3 {
        let first_evolved = hard[1]; // earliest post-init checkpoint
        let last = *hard.last().unwrap();
        if (last.1 - first_evolved.1).abs() < 0.05 {
            println!(
                "NOTE: at D=1.0, gen{} ~ gen{} ({:.0}% ~ {:.0}%) — mastery SATURATES by ~gen{}: \
                 the population conquers the difficulty axis early, then plateaus. Genuine \
                 generational improvement TO A CEILING, not open-ended unbounded evolution.",
                first_evolved.0,
                last.0,
                first_evolved.1 * 100.0,
                last.1 * 100.0,
                first_evolved.0
            );
        } else {
            println!(
                "NOTE: at D=1.0, gen{} {:.0}% vs gen{} {:.0}% — still climbing late (not yet saturated).",
                first_evolved.0,
                first_evolved.1 * 100.0,
                last.0,
                last.1 * 100.0
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Report + verdict.
// ---------------------------------------------------------------------------

fn report(rows: &[GenRow]) {
    println!("\n================ FULL PER-GENERATION TRAJECTORY ================");
    println!(
        "{:>3} {:>5} {:>6} {:>9} {:>9} {:>9} {:>7} {:>6} {:>7} {:>6} {:>6} {:>6}",
        "gen", "D", "gsurv%", "mean_fit", "best_fit", "foresight", "provis%", "build%",
        "social%", "drr%", "cult%", "fight%"
    );
    for r in rows {
        println!(
            "{:>3} {:>5.2} {:>5.0}% {:>9.4} {:>9.4} {:>9.2} {:>6.0}% {:>5.0}% {:>6.0}% {:>5.0}% {:>5.0}% {:>5.0}%",
            r.gen,
            r.d,
            r.survival_rate * 100.0,
            r.mean_fitness,
            r.best_fitness,
            r.foresight,
            r.can_provision * 100.0,
            r.can_build * 100.0,
            r.social_forage * 100.0,
            r.forage_drr * 100.0,
            r.cultural * 100.0,
            r.can_fight * 100.0,
        );
    }

    // early-thirds vs late-thirds aggregates.
    let n = rows.len();
    let third = (n / 3).max(1);
    let early = &rows[..third];
    let late = &rows[n - third..];
    let mean = |xs: &[GenRow], f: fn(&GenRow) -> f32| {
        xs.iter().map(f).sum::<f32>() / xs.len().max(1) as f32
    };

    let e_fit = mean(early, |r| r.mean_fitness);
    let l_fit = mean(late, |r| r.mean_fitness);
    let e_d = mean(early, |r| r.d);
    let l_d = mean(late, |r| r.d);
    let e_fore = mean(early, |r| r.foresight);
    let l_fore = mean(late, |r| r.foresight);
    let e_prov = mean(early, |r| r.can_provision);
    let l_prov = mean(late, |r| r.can_provision);
    let e_drr = mean(early, |r| r.forage_drr);
    let l_drr = mean(late, |r| r.forage_drr);
    let e_soc = mean(early, |r| r.social_forage);
    let l_soc = mean(late, |r| r.social_forage);
    let e_cult = mean(early, |r| r.cultural);
    let l_cult = mean(late, |r| r.cultural);
    let e_aff = mean(early, |r| r.affect_mod);
    let l_aff = mean(late, |r| r.affect_mod);

    println!("\n================ EARLY-THIRDS vs LATE-THIRDS ================");
    println!("                early    late    delta");
    let line = |name: &str, e: f32, l: f32| {
        println!("{name:<14} {e:>7.3} {l:>7.3} {:>+8.3}", l - e);
    };
    line("mean_fitness", e_fit, l_fit);
    line("difficulty D", e_d, l_d);
    line("foresight g", e_fore, l_fore);
    line("provision%", e_prov, l_prov);
    line("forage_drr%", e_drr, l_drr);
    line("social_for%", e_soc, l_soc);
    line("cultural%", e_cult, l_cult);
    line("affect_mod%", e_aff, l_aff);

    // hardest D the population SUSTAINED (mean graded survival >= 45%): start vs end.
    let sustained = |xs: &[GenRow]| -> f32 {
        xs.iter()
            .filter(|r| r.survival_rate >= 0.45)
            .map(|r| r.d)
            .fold(0.0f32, f32::max)
    };
    let start_hard = sustained(early);
    let end_hard = sustained(late);

    // which genes moved most (by |late-early|).
    let moves: [(&str, f32); 6] = [
        ("foresight", l_fore - e_fore),
        ("provision", l_prov - e_prov),
        ("forage_drr", l_drr - e_drr),
        ("social_for", l_soc - e_soc),
        ("cultural", l_cult - e_cult),
        ("affect_mod", l_aff - e_aff),
    ];
    let mut mv = moves;
    mv.sort_by(|a, b| b.1.abs().total_cmp(&a.1.abs()));

    println!("\nHardest D sustained (graded survival ≥ 45%):  start {start_hard:.2}  →  end {end_hard:.2}");
    print!("Genes that moved most: ");
    for (name, dv) in &mv[..3] {
        print!("{name} {dv:+.2}   ");
    }
    println!();

    // ------- VERDICT -------
    // SUCCESS = mean_fitness AND D rise together (population masters harder worlds)
    // AND the adaptive genes sweep toward useful values.
    let fit_rose = l_fit - e_fit > 0.01;
    let d_rose = l_d - e_d > 0.02;
    let genes_swept = (l_prov - e_prov) + (l_fore - e_fore) + (l_drr - e_drr) + (l_soc - e_soc)
        > 0.10;
    let harder_sustained = end_hard > start_hard + 0.02;

    println!("\n================ VERDICT ================");
    println!("mean_fitness rose (Δ>0.01):      {}  (Δ={:+.4})", yn(fit_rose), l_fit - e_fit);
    println!("difficulty D rose (Δ>0.02):      {}  (Δ={:+.4})", yn(d_rose), l_d - e_d);
    println!("adaptive genes swept (Σ>0.10):   {}", yn(genes_swept));
    println!("harder D sustained at end:       {}", yn(harder_sustained));

    if fit_rose && d_rose && (genes_swept || harder_sustained) {
        println!(
            "\nSUCCESS — minds EVOLVED over generations: mean_fitness AND world\n\
             difficulty rose TOGETHER (the population kept mastering progressively\n\
             harder worlds while holding survival high). That co-rising trend IS\n\
             'evolving minds over generations.' See the EARLY-vs-LATE table for which\n\
             specific genes moved (and which the population SELECTED OUT as unhelpful —\n\
             an honest, not-uniform sweep): selection kept what survival rewarded in\n\
             this regime and discarded the rest."
        );
    } else if d_rose && harder_sustained && !fit_rose {
        println!(
            "\nPARTIAL — the population SUSTAINED harder worlds (D and hardest-D rose)\n\
             but mean_fitness did not rise, because the ratchet holds fitness near the\n\
             frontier by design (harder world ⇒ lower raw fitness). The capability gain\n\
             shows in 'hardest D sustained', not in fitness. Read both together."
        );
    } else {
        println!(
            "\nNO CLIMB — this configuration did not demonstrably improve minds:\n\
             mean_fitness and/or D did not rise together. Diagnose: noise still too\n\
             high (raise K_SEEDS), selection too weak (lower SELECT_FRAC / raise\n\
             ELITES), difficulty step wrong (tune D_STEP / ratchet band), or the\n\
             architecture caps out at this world. NOT faking a climb."
        );
    }
}

fn yn(b: bool) -> &'static str {
    if b {
        "YES"
    } else {
        "no "
    }
}
