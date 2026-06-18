//! **Red Queen co-evolution experiment** — co-evolve a predator against the minds.
//!
//! THE QUESTION (two parts):
//!  1. **Open-ended?** Do BOTH sides keep improving with no permanent winner
//!     (co-climbing / oscillating), unlike the metabolic frontier that saturated to
//!     a ceiling by ~gen 8?
//!  2. **Sharper minds?** As the predator smartens, do the SOPHISTICATED faculties
//!     (foresight, fight, build/shelter, social foraging, provisioning) SWEEP — vs
//!     the metabolic frontier where they were purged for being metabolically costly?
//!
//! METHOD: two populations — minds (28-gene [`Genome`]) and predators (5-gene
//! [`PredatorStrategy`]). Each generation, every mind is scored by survival against
//! a sample of the CURRENT predators and every predator by catch-rate against a
//! sample of the CURRENT minds, both averaged over a FIXED seed-set (low-noise
//! selection). Truncation + elitism on both, then mutate. The ENVIRONMENT is held
//! fixed at a moderate difficulty so any survival change is attributable to the
//! evolving predator, not a difficulty ratchet (the Red Queen IS the moving
//! difficulty). Deterministic, no NN, additive.
//!
//! ARBITER (confound-free): early vs late mind champions are re-scored against a
//! FIXED, HELD-OUT predator panel (distinct from training); symmetrically early vs
//! late predator champions against a fixed held-out mind panel. Genuine heritable
//! gain shows as later > earlier on the held-out panel.
//!
//!   cargo run -p daimon-game --example redqueen --release

use daimon_core::Rng;
use daimon_game::redqueen::{mind_fitness, predator_fitness, weak_mind};
use daimon_game::sim::PredatorStrategy;
use daimon_mind::evolve::N_GENES;
use daimon_mind::Genome;

// ---------------------------------------------------------------------------
// Hyper-parameters — fixed UP FRONT (process discipline: one clean run).
// Sized so the whole run finishes in a few–~15 min in --release.
//   cost ≈ GENERATIONS · (MIND_POP·SAMPLE·K + PRED_POP·SAMPLE·K) · EVAL_TICKS
// ---------------------------------------------------------------------------

/// Mind population. Holds genetic variance across 28 genes.
const MIND_POP: usize = 30;
/// Predator population. Smaller — only 5 genes, so less variance to maintain.
const PRED_POP: usize = 18;
/// Opponents sampled from the other population to score each individual against
/// (the current champion + a random sample → the fitness tracks the live arms
/// race, not one fixed foe).
const SAMPLE: usize = 3;
/// Fixed seeds each pairing is averaged over (the noise-control lesson).
const K_SEEDS: usize = 2;
/// Ticks per duel. In this CLOSED predator arena the dynamics are stationary (no
/// seasons), and calibration shows the contested predator/prey signal is fully
/// present by ~1200 ticks (gen-0: mind-survival ≈0.75, catch ≈0.40, a sharp mind
/// ≈1.0) — so 1200 keeps the signal while ~halving the per-duel cost vs 2200.
const EVAL_TICKS: u64 = 1200;
/// Generations. Long enough to see whether the race saturates or keeps moving;
/// sized with the population/sample so the whole run finishes in ~12 min --release.
const GENERATIONS: usize = 50;

/// Truncation fraction (top share kept as parents) and elites, per side.
const MIND_SELECT: f32 = 0.25;
const PRED_SELECT: f32 = 0.30;
const MIND_ELITES: usize = 3;
const PRED_ELITES: usize = 2;
const MIND_SIGMA: f32 = 0.06;
const PRED_SIGMA: f32 = 0.10;

// ---------------------------------------------------------------------------
// Telemetry.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct Row {
    mind_surv: f32,    // mean survival of the mind population vs current predators
    pred_catch: f32,   // mean catch-rate of the predator population vs current minds
    // sophisticated mind faculties (population frequency / mean):
    foresight: f32,    // mean decoded lead ticks
    can_fight: f32,
    can_build: f32,
    social: f32,
    provision: f32,
    drr: f32,
    // predator faculties (population frequency):
    p_persist: f32,
    p_ambush: f32,
    p_fast: f32,
    p_weakest: f32,
    p_isolated: f32,
    p_aggro: f32,      // mean aggro range
}

fn mind_genes(pop: &[Genome]) -> [f32; 6] {
    let n = pop.len().max(1) as f32;
    let mut s = [0.0f32; 6];
    for g in pop {
        s[0] += g.foresight();
        s[1] += if g.can_fight() { 1.0 } else { 0.0 };
        s[2] += if g.can_build() { 1.0 } else { 0.0 };
        s[3] += if g.social_forage() { 1.0 } else { 0.0 };
        s[4] += if g.can_provision() { 1.0 } else { 0.0 };
        s[5] += if g.forage_drr() { 1.0 } else { 0.0 };
    }
    for v in &mut s {
        *v /= n;
    }
    s
}

fn pred_genes(pop: &[PredatorStrategy]) -> [f32; 6] {
    let n = pop.len().max(1) as f32;
    let mut s = [0.0f32; 6];
    for p in pop {
        s[0] += if p.pursues_relentlessly() { 1.0 } else { 0.0 };
        s[1] += if p.ambushes() { 1.0 } else { 0.0 };
        s[2] += if p.is_fast() { 1.0 } else { 0.0 };
        s[3] += if p.targets_weakest() { 1.0 } else { 0.0 };
        s[4] += if p.targets_isolated() { 1.0 } else { 0.0 };
        s[5] += p.aggro_range() as f32;
    }
    for v in &mut s {
        *v /= n;
    }
    s
}

/// Deterministic fixed seed-set for a generation (same seeds for every individual).
fn gen_seeds(run_seed: u64, gen: usize, k: usize) -> Vec<u64> {
    (0..k)
        .map(|j| {
            run_seed
                .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                .wrapping_add((gen as u64) << 20)
                .wrapping_add(j as u64 * 0x0001_0001)
                ^ 0xD1A3_0000
        })
        .collect()
}

/// Pick `SAMPLE` opponents: always include the current best (index 0 after sort),
/// fill the rest with a deterministic random sample. Keeps the coupling honest —
/// each side faces the *current* frontier of the other, not a frozen foe.
fn sample_opponents<T: Clone>(sorted: &[T], k: usize, rng: &mut Rng) -> Vec<T> {
    let k = k.min(sorted.len());
    let mut out = Vec::with_capacity(k);
    out.push(sorted[0].clone()); // current champion
    while out.len() < k {
        out.push(sorted[rng.below(sorted.len())].clone());
    }
    out
}

fn main() {
    let run_seed = 0x5EED_2EE5_0E0Du64; // "red queen"
    let mut rng = Rng::new(run_seed);

    println!("\n=== Red Queen co-evolution — predator vs minds ===");
    println!(
        "mind_pop={MIND_POP}  pred_pop={PRED_POP}  sample={SAMPLE}  K_seeds={K_SEEDS}  \
         eval_ticks={EVAL_TICKS}  generations={GENERATIONS}"
    );
    println!(
        "FIXED predator-dominated arena (harsh closed world, predator is the killer) — \
         the Red Queen is the moving difficulty.\n\
         mind fitness = mean survival vs current predators;  predator fitness = mean catch-rate vs current minds.\n"
    );

    // weak/random init for BOTH sides.
    let mut minds: Vec<Genome> = (0..MIND_POP).map(|_| weak_mind(&mut rng)).collect();
    let mut preds: Vec<PredatorStrategy> =
        (0..PRED_POP).map(|_| PredatorStrategy::random(&mut rng)).collect();

    let mut rows: Vec<Row> = Vec::with_capacity(GENERATIONS);
    let checkpoints = [0usize, GENERATIONS / 3, (2 * GENERATIONS) / 3, GENERATIONS - 1];
    let mut mind_champs: Vec<(usize, Genome)> = Vec::new();
    let mut pred_champs: Vec<(usize, PredatorStrategy)> = Vec::new();

    println!(
        "{:>3} {:>7} {:>7} | {:>6} {:>6} {:>6} {:>6} {:>6} {:>6} | {:>6} {:>6} {:>6} {:>6} {:>6} {:>6}",
        "gen", "mSurv%", "pCtch%", "fore", "fight%", "bld%", "soc%", "prov%", "drr%", "pers%",
        "amb%", "fast%", "weak%", "iso%", "aggro"
    );

    // Carry-forward rankings: each side's opponent sample is drawn from the OTHER
    // side's ranking from the PREVIOUS generation (champion-first). This is the
    // standard, cheap way to keep the coupling honest — each side faces the current
    // frontier of the other (a one-generation lag, negligible for the dynamics) —
    // WITHOUT a costly proxy-evaluation pass each generation. Identity on gen 0.
    let mut mind_rank: Vec<usize> = (0..minds.len()).collect();
    let mut pred_rank: Vec<usize> = (0..preds.len()).collect();

    for gen in 0..GENERATIONS {
        let seeds = gen_seeds(run_seed, gen, K_SEEDS);

        // opponent samples (champion-first + random), fixed for the whole generation,
        // drawn from the previous generation's rankings.
        let preds_sorted: Vec<PredatorStrategy> = pred_rank.iter().map(|&i| preds[i]).collect();
        let minds_sorted: Vec<Genome> = mind_rank.iter().map(|&i| minds[i].clone()).collect();
        let pred_sample = sample_opponents(&preds_sorted, SAMPLE, &mut rng);
        let mind_sample = sample_opponents(&minds_sorted, SAMPLE, &mut rng);

        // --- FULL averaged evaluation (the selection signal) ---
        let mind_fit: Vec<f32> = minds
            .iter()
            .map(|m| mind_fitness(m, &pred_sample, EVAL_TICKS, &seeds))
            .collect();
        let pred_fit: Vec<f32> = preds
            .iter()
            .map(|p| predator_fitness(p, &mind_sample, EVAL_TICKS, &seeds))
            .collect();

        // rank.
        let mut m_ord: Vec<usize> = (0..minds.len()).collect();
        m_ord.sort_by(|&a, &b| mind_fit[b].total_cmp(&mind_fit[a]));
        let mut p_ord: Vec<usize> = (0..preds.len()).collect();
        p_ord.sort_by(|&a, &b| pred_fit[b].total_cmp(&pred_fit[a]));

        // capture champions at checkpoints.
        if checkpoints.contains(&gen) {
            mind_champs.push((gen, minds[m_ord[0]].clone()));
            pred_champs.push((gen, preds[p_ord[0]]));
        }

        // telemetry (means over the populations we just scored).
        let mind_surv = mind_fit.iter().copied().sum::<f32>() / minds.len() as f32;
        let pred_catch = pred_fit.iter().copied().sum::<f32>() / preds.len() as f32;
        let mg = mind_genes(&minds);
        let pg = pred_genes(&preds);
        rows.push(Row {
            mind_surv,
            pred_catch,
            foresight: mg[0],
            can_fight: mg[1],
            can_build: mg[2],
            social: mg[3],
            provision: mg[4],
            drr: mg[5],
            p_persist: pg[0],
            p_ambush: pg[1],
            p_fast: pg[2],
            p_weakest: pg[3],
            p_isolated: pg[4],
            p_aggro: pg[5],
        });
        println!(
            "{:>3} {:>6.0}% {:>6.0}% | {:>6.1} {:>5.0}% {:>5.0}% {:>5.0}% {:>5.0}% {:>5.0}% | \
             {:>5.0}% {:>5.0}% {:>5.0}% {:>5.0}% {:>5.0}% {:>6.1}",
            gen,
            mind_surv * 100.0,
            pred_catch * 100.0,
            mg[0],
            mg[1] * 100.0,
            mg[2] * 100.0,
            mg[3] * 100.0,
            mg[4] * 100.0,
            mg[5] * 100.0,
            pg[0] * 100.0,
            pg[1] * 100.0,
            pg[2] * 100.0,
            pg[3] * 100.0,
            pg[4] * 100.0,
            pg[5],
        );

        // --- REPRODUCE both sides (truncation + elitism + mutate) ---
        minds = breed_minds(&minds, &m_ord, &mut rng);
        preds = breed_preds(&preds, &p_ord, &mut rng);
        // breeding places the elites (champion first) at the front, so identity
        // ordering on the bred population is champion-first — what next generation's
        // opponent sampling wants.
        mind_rank = (0..minds.len()).collect();
        pred_rank = (0..preds.len()).collect();
    }

    report(&rows);
    head_to_head(&mind_champs, &pred_champs);
    verdict(&rows, &mind_champs, &pred_champs);
}

fn breed_minds(pop: &[Genome], order: &[usize], rng: &mut Rng) -> Vec<Genome> {
    let n = pop.len();
    let n_parents = ((n as f32 * MIND_SELECT).round() as usize).clamp(2, n);
    let parents: Vec<Genome> = order[..n_parents].iter().map(|&i| pop[i].clone()).collect();
    let gain = [1.0f32; N_GENES];
    let mut next: Vec<Genome> = Vec::with_capacity(n);
    for &i in &order[..MIND_ELITES.min(n_parents)] {
        next.push(pop[i].clone());
    }
    while next.len() < n {
        let p = &parents[rng.below(parents.len())];
        next.push(p.mutate(MIND_SIGMA, &gain, rng));
    }
    next
}

fn breed_preds(pop: &[PredatorStrategy], order: &[usize], rng: &mut Rng) -> Vec<PredatorStrategy> {
    let n = pop.len();
    let n_parents = ((n as f32 * PRED_SELECT).round() as usize).clamp(2, n);
    let parents: Vec<PredatorStrategy> = order[..n_parents].iter().map(|&i| pop[i]).collect();
    let mut next: Vec<PredatorStrategy> = Vec::with_capacity(n);
    for &i in &order[..PRED_ELITES.min(n_parents)] {
        next.push(pop[i]);
    }
    while next.len() < n {
        let p = &parents[rng.below(parents.len())];
        next.push(p.mutate(PRED_SIGMA, rng));
    }
    next
}

// ---------------------------------------------------------------------------
// Report.
// ---------------------------------------------------------------------------

fn report(rows: &[Row]) {
    let n = rows.len();
    let third = (n / 3).max(1);
    let early = &rows[..third];
    let late = &rows[n - third..];
    let mean = |xs: &[Row], f: fn(&Row) -> f32| xs.iter().map(f).sum::<f32>() / xs.len() as f32;

    println!("\n================ EARLY-THIRDS vs LATE-THIRDS ================");
    println!("                  early    late    delta");
    let line = |name: &str, e: f32, l: f32| println!("{name:<16} {e:>7.3} {l:>7.3} {:>+8.3}", l - e);
    line("mind survival", mean(early, |r| r.mind_surv), mean(late, |r| r.mind_surv));
    line("pred catch", mean(early, |r| r.pred_catch), mean(late, |r| r.pred_catch));
    println!("  -- sophisticated MIND faculties --");
    line("foresight(ticks)", mean(early, |r| r.foresight), mean(late, |r| r.foresight));
    line("can_fight%", mean(early, |r| r.can_fight), mean(late, |r| r.can_fight));
    line("can_build%", mean(early, |r| r.can_build), mean(late, |r| r.can_build));
    line("social_forage%", mean(early, |r| r.social), mean(late, |r| r.social));
    line("provision%", mean(early, |r| r.provision), mean(late, |r| r.provision));
    line("forage_drr%", mean(early, |r| r.drr), mean(late, |r| r.drr));
    println!("  -- PREDATOR faculties --");
    line("persistence%", mean(early, |r| r.p_persist), mean(late, |r| r.p_persist));
    line("ambush%", mean(early, |r| r.p_ambush), mean(late, |r| r.p_ambush));
    line("fast%", mean(early, |r| r.p_fast), mean(late, |r| r.p_fast));
    line("target_weakest%", mean(early, |r| r.p_weakest), mean(late, |r| r.p_weakest));
    line("target_isolated%", mean(early, |r| r.p_isolated), mean(late, |r| r.p_isolated));
    line("aggro_range", mean(early, |r| r.p_aggro), mean(late, |r| r.p_aggro));
}

// ---------------------------------------------------------------------------
// Confound-free head-to-head on HELD-OUT panels.
// ---------------------------------------------------------------------------

/// A fixed panel of predators DISTINCT from anything in training: a spread of hand-
/// set hunting strategies (incumbent, fast-chaser, ambusher, weakest-picker,
/// isolated-picker) so the arbiter is a *standard* test the minds never trained on.
fn heldout_predators() -> Vec<PredatorStrategy> {
    vec![
        PredatorStrategy::default(), // incumbent stalker
        PredatorStrategy { g: [0.6, 0.0, 1.0, 1.0, 1.0] }, // wide-aggro relentless fast chaser
        PredatorStrategy { g: [0.5, 0.3, 0.0, 1.0, 0.0] }, // ambusher targeting the weakest
        PredatorStrategy { g: [1.0, 1.0, 0.0, 0.0, 0.0] }, // max-aggro isolated-picker, patrols
        PredatorStrategy { g: [0.4, 0.0, 1.0, 0.0, 0.0] }, // fast nearest-chaser
    ]
}

/// A fixed panel of minds DISTINCT from training: a spread from a weak baseline to
/// the strong showcase, so the predator arbiter faces standard prey it never trained
/// on. All open-world capable so the arena is well-formed.
fn heldout_minds() -> Vec<Genome> {
    let make = |f: fn(&mut [f32; N_GENES])| {
        let mut g = Genome::showcase().g;
        g[22] = 1.0; // mortal (arena needs catchable prey)
        g[24] = 1.0; // provisioning available
        f(&mut g);
        Genome { g }
    };
    vec![
        // weak prey: foresight off, no fight/build/social
        make(|g| {
            g[13] = 0.0;
            g[15] = 0.0;
            g[20] = 0.0;
            g[21] = 0.0;
        }),
        // mid prey: some foresight, social on
        make(|g| {
            g[13] = 0.4;
            g[20] = 0.0;
        }),
        // strong prey: full showcase faculties (fight/build available)
        make(|g| {
            g[20] = 1.0;
            g[21] = 1.0;
        }),
    ]
}

fn head_to_head(
    mind_champs: &[(usize, Genome)],
    pred_champs: &[(usize, PredatorStrategy)],
) {
    // held-out probe seeds, distinct from every training seed.
    let probe: Vec<u64> =
        (0..8u64).map(|i| 0xF00D_5EED_u64 ^ i.wrapping_mul(0x9E37_79B9_7F4A_7C15)).collect();
    let hp = heldout_predators();
    let hm = heldout_minds();

    println!("\n========= HEAD-TO-HEAD #1: MIND champions vs a FIXED HELD-OUT predator panel =========");
    println!("survival% on a panel the minds never trained on — later > earlier => sharper minds");
    for (g, champ) in mind_champs {
        let s = mind_fitness(champ, &hp, EVAL_TICKS, &probe);
        println!("  mind gen {:>2}:  survival {:>4.0}%", g, s * 100.0);
    }

    println!("\n========= HEAD-TO-HEAD #2: PREDATOR champions vs a FIXED HELD-OUT mind panel =========");
    println!("catch-rate% on prey the predator never trained on — later > earlier => sharper predator");
    for (g, champ) in pred_champs {
        let c = predator_fitness(champ, &hm, EVAL_TICKS, &probe);
        println!("  pred gen {:>2}:  catch {:>4.0}%", g, c * 100.0);
    }
}

// ---------------------------------------------------------------------------
// Verdict.
// ---------------------------------------------------------------------------

fn verdict(
    rows: &[Row],
    mind_champs: &[(usize, Genome)],
    pred_champs: &[(usize, PredatorStrategy)],
) {
    let n = rows.len();
    let third = (n / 3).max(1);
    let early = &rows[..third];
    let late = &rows[n - third..];
    let mean = |xs: &[Row], f: fn(&Row) -> f32| xs.iter().map(f).sum::<f32>() / xs.len() as f32;

    // --- (1) open-endedness: did BOTH keep moving with no permanent winner? ---
    // We look at whether catch-rate stayed in a contested band (neither pinned ~0
    // nor ~1) AND whether the gene populations kept changing (the race never froze).
    let catch_e = mean(early, |r| r.pred_catch);
    let catch_l = mean(late, |r| r.pred_catch);
    let surv_e = mean(early, |r| r.mind_surv);
    let surv_l = mean(late, |r| r.mind_surv);
    // contested = catch-rate never fully pinned at an extreme in the late phase.
    let late_catch_min = late.iter().map(|r| r.pred_catch).fold(1.0f32, f32::min);
    let late_catch_max = late.iter().map(|r| r.pred_catch).fold(0.0f32, f32::max);
    let contested = late_catch_max < 0.95 && late_catch_min > 0.05;
    // oscillation: sign changes in the catch-rate trajectory (the seesaw of an arms
    // race rather than a monotone runaway).
    let mut sign_changes = 0;
    let mut prev = 0.0f32;
    for w in rows.windows(2) {
        let d = w[1].pred_catch - w[0].pred_catch;
        if d.abs() > 0.01 {
            if prev != 0.0 && d.signum() != prev.signum() {
                sign_changes += 1;
            }
            prev = d;
        }
    }
    let oscillating = sign_changes >= 4;

    // --- held-out arbiter deltas (the heritable-gain ladders) ---
    let probe: Vec<u64> =
        (0..8u64).map(|i| 0xF00D_5EED_u64 ^ i.wrapping_mul(0x9E37_79B9_7F4A_7C15)).collect();
    let hp = heldout_predators();
    let hm = heldout_minds();
    let mind_ladder: Vec<(usize, f32)> = mind_champs
        .iter()
        .map(|(g, c)| (*g, mind_fitness(c, &hp, EVAL_TICKS, &probe)))
        .collect();
    let pred_ladder: Vec<(usize, f32)> = pred_champs
        .iter()
        .map(|(g, c)| (*g, predator_fitness(c, &hm, EVAL_TICKS, &probe)))
        .collect();
    // gain = last vs first-evolved checkpoint (index 1, the first post-init capture).
    let mind_gain = if mind_ladder.len() >= 2 {
        mind_ladder.last().unwrap().1 - mind_ladder[1].1
    } else {
        0.0
    };
    let pred_gain = if pred_ladder.len() >= 2 {
        pred_ladder.last().unwrap().1 - pred_ladder[1].1
    } else {
        0.0
    };

    // --- (2) sharper minds: did sophisticated faculties sweep? ---
    let fore_d = mean(late, |r| r.foresight) - mean(early, |r| r.foresight);
    let fight_d = mean(late, |r| r.can_fight) - mean(early, |r| r.can_fight);
    let build_d = mean(late, |r| r.can_build) - mean(early, |r| r.can_build);
    let social_d = mean(late, |r| r.social) - mean(early, |r| r.social);
    let prov_d = mean(late, |r| r.provision) - mean(early, |r| r.provision);
    // "sharper" = the sophisticated faculties net-rose (vs the frontier where they were purged).
    let soph_sum = fight_d + build_d + social_d + prov_d + (fore_d / 45.0);
    let sharper = soph_sum > 0.15;

    println!("\n================ VERDICT ================");
    println!(
        "Open-endedness:  contested band={}  oscillating(sign-changes={})={}  \
         (early catch {:.0}% surv {:.0}% → late catch {:.0}% surv {:.0}%)",
        yn(contested),
        sign_changes,
        yn(oscillating),
        catch_e * 100.0,
        surv_e * 100.0,
        catch_l * 100.0,
        surv_l * 100.0
    );
    println!(
        "Heritable gain (held-out):  mind ladder Δ={:+.2}  predator ladder Δ={:+.2}",
        mind_gain, pred_gain
    );
    print!("Sharper minds:  Σsoph Δ={soph_sum:+.2}  (");
    print!("foresight {fore_d:+.1}t  fight {fight_d:+.2}  build {build_d:+.2}  ");
    println!("social {social_d:+.2}  provision {prov_d:+.2})");

    let open_ended = (contested || oscillating) && pred_gain.abs() < 0.6 && pred_gain > -0.4;
    let both_improved = mind_gain > 0.03 && pred_gain > 0.03;

    println!();
    if open_ended && sharper {
        println!(
            "VERDICT: OPEN-ENDED + SHARPER — the arms race stayed contested (no permanent\n\
             winner) AND the sophisticated mind faculties were SELECTED UP as the predator\n\
             smartened (Σsoph Δ={soph_sum:+.2}), the opposite of the metabolic frontier that\n\
             purged them. Held-out ladders: mind {mind_gain:+.2}, predator {pred_gain:+.2}."
        );
    } else if (open_ended || both_improved) && (sharper || soph_sum > 0.05) {
        println!(
            "VERDICT: PARTIAL — there is real co-evolution signal (open-ended={} / both-improved={} / \
             sharper-ish Σsoph Δ={soph_sum:+.2}) but it does not clear the full OPEN-ENDED+SHARPER bar.\n\
             Read the trajectory + ladders for which leg is weak.",
            yn(open_ended),
            yn(both_improved)
        );
    } else {
        println!(
            "VERDICT: SATURATED / one-side-dominant (NULL) — the race did not stay contested\n\
             and/or the sophisticated faculties did not sweep (Σsoph Δ={soph_sum:+.2}). Honestly\n\
             reported: co-evolving this predator did NOT produce open-ended, sharper minds in\n\
             this configuration. (catch late band [{:.0}%,{:.0}%]; sign-changes={sign_changes})",
            late_catch_min * 100.0,
            late_catch_max * 100.0
        );
    }
}

fn yn(b: bool) -> &'static str {
    if b {
        "YES"
    } else {
        "no"
    }
}
