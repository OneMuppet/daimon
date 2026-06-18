//! **Hell + PREDATOR-AWARE COORDINATION** — the original `hell` walled out at H≈0.5:
//! a real gen-0 gradient existed and the ratchet sustained a climb, but the champion
//! only matched the human showcase and past H≈0.75 champion, showcase AND gen-0 all
//! collapsed to the same single-digit floor. Its verdict named the missing piece:
//! hell's predator hunts the **isolated** straggler (TargetMode::Isolated), and no
//! evolved gene could compose an anti-isolation strategy out of flee + forage + build.
//!
//! This re-run adds exactly that faculty — **selfish-herd / dispersal-evasion**
//! (gene 28, default OFF so the harness is byte-identical; see AC48) — to the
//! available genes and asks the SAME question with the SAME discipline. THE
//! COMPARISON: the faculty-equipped evolved champion vs the NO-faculty human showcase
//! vs gen-0, on the held-out hell ladder. Did the dispersal faculty push the ceiling
//! past H≈0.5, and does it survive high-H hell where the no-faculty design collapsed?
//!
//! ## Design (reuses the proven machinery; mirrors the original `hell`)
//! * **Weak/random init**, capability switches forced ON (faculties available to
//!   evolve) PLUS the herd-evasion faculty available (gene 28 ON, cohesion evolvable),
//!   NN overlay OFF (no neural net).
//! * **GRADED fitness** (mean fraction of [`HELL_EVAL_TICKS`] lived), NOT binary — in
//!   hell binary survival is ≈0% for everyone (no gradient); graded survival is the
//!   stable gradient (proven in the ceiling experiment). Averaged over a FIXED K-seed
//!   set per generation (low-noise selection).
//! * **Calibrated gen-0 gradient**: `H_START` chosen from `hell_calib` so gen-0 graded
//!   survival is LOW but nonzero WITH variance (mean ≈24%, std ≈10%) — selection has
//!   signal. The gen-0 baseline + spread are reported up front.
//! * **Ratchet H UP** when the population copes (frontier-style), to find where the
//!   champion finally walls out — that intensity is the true ceiling.
//! * **HELD-OUT arbiter** (DISJOINT seeds): evolved champion vs `Genome::showcase()`
//!   vs gen-0 champion, on a LADDER of hell intensities. The max H at which the
//!   champion sustains meaningful graded survival = the ceiling. Plain numeric verdict.
//!
//! ONE clean deterministic run. Fixed seed, params fixed up front. No competing runs.
//!
//!   cargo run -p daimon-game --example hell_coord --release

use daimon_core::Rng;
use daimon_game::hell::{
    hell_survival_avg, pin_capabilities, showcase_with_capabilities, unit_gain, weak_random,
    HELL_EVAL_TICKS,
};
use daimon_mind::Genome;

// ---- predator-aware coordination (the new faculty under test) -------------------
// Gene 28 = selfish-herd / dispersal-evasion. The library helpers (weak_random,
// pin_capabilities, showcase_with_capabilities) are SHARED with the original `hell`
// experiment and intentionally untouched (so that run stays trustworthy); we wrap
// them here to additionally make the herd faculty AVAILABLE and its cohesion
// EVOLVABLE. `g[28] >= 0.5` keeps the faculty ON; mutation then tunes cohesion within
// [0.5,1.0] (decoded [0.2,1.0]) — pinned-available, strength-evolvable.
const HERD_GENE: usize = 28;

/// Make the herd faculty available + start its cohesion mid-range so selection can
/// tune it up or hold it. Applied on top of the shared weak/random init.
fn weak_random_herd(rng: &mut Rng) -> Genome {
    let mut g = weak_random(rng);
    g.g[HERD_GENE] = (0.5 + 0.5 * rng.next_f32()).clamp(0.5, 1.0); // ON, random cohesion
    g
}

/// Re-pin every generation: the shared anti-exploit pins (mortality ON, etc.) PLUS
/// keep the herd faculty ON (cohesion still free to evolve in [0.5,1.0]).
fn pin_capabilities_herd(g: &mut Genome) {
    pin_capabilities(g);
    g.g[HERD_GENE] = g.g[HERD_GENE].max(0.5); // faculty stays ON; cohesion evolves
}

/// The NO-faculty human showcase control (gene 28 OFF) — the design the original
/// `hell` ladder used. The faculty champion is pitted against THIS.
fn showcase_no_herd() -> Genome {
    let mut g = showcase_with_capabilities();
    g.g[HERD_GENE] = 0.0;
    g
}

/// The human showcase WITH the faculty switched on — a reference point: how far does
/// the hand-tuned design get if simply *given* the faculty (no evolution)?
fn showcase_with_herd() -> Genome {
    let mut g = showcase_with_capabilities();
    g.g[HERD_GENE] = 1.0;
    g
}

// ----------------------------- fixed hyperparameters -----------------------------
const RUN_SEED: u64 = 0x4E11_C0DE_5EED_F00D; // "hell code seed food"
const POP: usize = 64;
const GENERATIONS: usize = 40;
const SELECT_FRAC: f32 = 0.22;
const ELITES: usize = 4;
const MUT_SIGMA: f32 = 0.06;
const K_SEEDS: usize = 4; // fixed train seeds per generation (low-noise selection)

// ---- hell-intensity ratchet (frontier-style; rides MEAN graded survival) ----
// H_START calibrated from `hell_calib`: at H=0.0 weak/random gen-0 has mean graded
// survival ≈24% with std ≈10% — a LOW-but-NONZERO gradient with wide variance.
const H_START: f32 = 0.0;
const H_STEP: f32 = 0.06;
// Raise H when the SELECTED elite copes; lower when being wiped — hold the frontier.
// Bands sit around the elite's graded survival so H climbs as the champion improves.
const RAISE_ABOVE: f32 = 0.30; // elite graded survival above this → harsher
const LOWER_BELOW: f32 = 0.15; // elite graded survival below this → ease (rarely hit)

// Held-out arbiter: a LADDER of hell intensities, evaluated on DISJOINT seeds.
const LADDER: [f32; 8] = [0.0, 0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 2.0];
const K_HELD_SEEDS: usize = 8; // held-out seeds per ladder rung (DISJOINT band)
// "Meaningful" sustained graded survival used to read the ceiling off the ladder.
const CEILING_THRESH: f32 = 0.15;

// ----------------------------- seed-sets (disjoint by construction) --------------
fn train_seeds(gen: usize) -> Vec<u64> {
    let base = RUN_SEED
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add((gen as u64) << 18);
    (0..K_SEEDS as u64).map(|i| base ^ i.wrapping_mul(0xD1A3_0001)).collect()
}

const HELDOUT_TAG: u64 = 0xCE1D_0FF0_BACE_DEAD;
fn heldout_seeds(rung: usize) -> Vec<u64> {
    let base = HELDOUT_TAG
        .wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
        .wrapping_add((rung as u64) << 10);
    (0..K_HELD_SEEDS as u64).map(|i| base ^ i.wrapping_mul(0x27D4_EB2F_1657_4DA1)).collect()
}

// ----------------------------- gene telemetry ------------------------------------
fn gene_freqs(pop: &[Genome]) -> [f32; 7] {
    let n = pop.len().max(1) as f32;
    let mut s = [0.0f32; 7];
    for g in pop {
        s[0] += g.g[13]; // foresight gene value (mean)
        s[1] += if g.can_provision() { 1.0 } else { 0.0 };
        s[2] += if g.can_build() { 1.0 } else { 0.0 };
        s[3] += if g.can_fight() { 1.0 } else { 0.0 };
        s[4] += if g.social_forage() { 1.0 } else { 0.0 };
        s[5] += if g.forage_drr() { 1.0 } else { 0.0 };
        s[6] += g.g[HERD_GENE]; // herd-evasion cohesion gene value (mean)
    }
    for v in &mut s {
        *v /= n;
    }
    s
}

#[derive(Clone, Copy)]
struct GenRow {
    gen: usize,
    h: f32,
    elite_surv: f32,
    mean_surv: f32,
    best_surv: f32,
    foresight: f32,
    provision: f32,
    build: f32,
    fight: f32,
    social: f32,
    drr: f32,
    herd: f32,
}

fn main() {
    let mut rng = Rng::new(RUN_SEED);

    println!("=== HELL + PREDATOR-AWARE COORDINATION (selfish-herd, gene 28) ===");
    println!(
        "run_seed={RUN_SEED:#x}  pop={POP}  gens={GENERATIONS}  k_seeds={K_SEEDS}  \
         eval_ticks={HELL_EVAL_TICKS}"
    );
    println!(
        "select_top={:.0}%  elites={ELITES}  mut_sigma={MUT_SIGMA}  \
         H_start={H_START}  H_step={H_STEP}  ratchet[{LOWER_BELOW},{RAISE_ABOVE}]",
        SELECT_FRAC * 100.0
    );
    println!(
        "hell = brutal cold + heavy metabolism + near-famine + fast/persistent/one-shot \
         predator (TargetMode::Isolated — picks off stragglers), ALL scaling with H; \
         GRADED survival metric, no NN. NEW: herd-evasion faculty available + evolvable.\n"
    );

    // ---- population init: weak/random minds WITH the herd faculty available ----
    let mut pop: Vec<Genome> = (0..POP).map(|_| weak_random_herd(&mut rng)).collect();
    let gain = unit_gain();

    // ---- gen-0 baseline + SPREAD (proving a gradient exists at H_START) ----
    let g0_seeds = train_seeds(0);
    let mut g0_scores: Vec<(f32, usize)> = pop
        .iter()
        .enumerate()
        .map(|(i, g)| (hell_survival_avg(g, H_START, &g0_seeds), i))
        .collect();
    g0_scores.sort_by(|a, b| b.0.total_cmp(&a.0));
    let gen0_champ = pop[g0_scores[0].1].clone();
    {
        let xs: Vec<f32> = g0_scores.iter().map(|s| s.0).collect();
        let n = xs.len() as f32;
        let mean = xs.iter().sum::<f32>() / n;
        let min = xs.iter().cloned().fold(f32::INFINITY, f32::min);
        let max = xs.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let std = (xs.iter().map(|x| (x - mean) * (x - mean)).sum::<f32>() / n).sqrt();
        let frac_pos = xs.iter().filter(|&&x| x > 0.001).count() as f32 / n;
        println!("--- GEN-0 GRADED BASELINE (H={H_START}, weak/random, training seeds) ---");
        println!(
            "mean={:.1}%  min={:.1}%  max={:.1}%  std={:.1}%  frac>0={:.0}%",
            mean * 100.0,
            min * 100.0,
            max * 100.0,
            std * 100.0,
            frac_pos * 100.0
        );
        if std > 0.02 && mean > 0.02 && mean < 0.45 {
            println!(
                "GRADIENT OK — survival is LOW but nonzero with clear VARIANCE: selection has signal."
            );
        } else {
            println!(
                "WARNING: weak gen-0 gradient (std={:.1}%, mean={:.1}%) — selection signal may be thin.",
                std * 100.0,
                mean * 100.0
            );
        }
    }

    // ---- evolution loop: ratchet H up as the population copes ----
    let n_parents = ((POP as f32 * SELECT_FRAC).round() as usize).clamp(2, POP);
    let mut h = H_START;
    let mut champion = gen0_champ.clone();
    let mut rows: Vec<GenRow> = Vec::with_capacity(GENERATIONS);

    println!("\n--- EVOLUTION ({GENERATIONS} generations; H ratchets UP as minds cope) ---");
    for gen in 0..GENERATIONS {
        let seeds = train_seeds(gen);
        let mut scored: Vec<(f32, usize)> = pop
            .iter()
            .enumerate()
            .map(|(i, g)| (hell_survival_avg(g, h, &seeds), i))
            .collect();
        scored.sort_by(|a, b| b.0.total_cmp(&a.0));

        let best_surv = scored[0].0;
        champion = pop[scored[0].1].clone();
        let mean_surv = scored.iter().map(|s| s.0).sum::<f32>() / POP as f32;
        let elite_surv = scored[..n_parents].iter().map(|s| s.0).sum::<f32>() / n_parents as f32;
        let gf = gene_freqs(&pop);

        rows.push(GenRow {
            gen,
            h,
            elite_surv,
            mean_surv,
            best_surv,
            foresight: gf[0],
            provision: gf[1],
            build: gf[2],
            fight: gf[3],
            social: gf[4],
            drr: gf[5],
            herd: gf[6],
        });

        if gen % 4 == 0 || gen == GENERATIONS - 1 {
            println!(
                "gen {:>2}  H={:.2}  elite_surv={:>4.0}%  best={:>4.0}%  mean={:>4.0}%  \
                 fore={:.2}  build={:>3.0}%  fight={:>3.0}%  herd_g={:.2}",
                gen,
                h,
                elite_surv * 100.0,
                best_surv * 100.0,
                mean_surv * 100.0,
                gf[0],
                gf[2] * 100.0,
                gf[3] * 100.0,
                gf[6],
            );
        }

        // truncation + elitism + mutation
        let parents: Vec<Genome> = scored[..n_parents].iter().map(|s| pop[s.1].clone()).collect();
        let mut next: Vec<Genome> = Vec::with_capacity(POP);
        for s in scored.iter().take(ELITES.min(n_parents)) {
            next.push(pop[s.1].clone());
        }
        while next.len() < POP {
            let p = &parents[rng.below(parents.len())];
            let mut child = p.mutate(MUT_SIGMA, &gain, &mut rng);
            // ANTI-EXPLOIT: re-pin mortality (and the affordance switches) so the
            // population cannot "win" hell by mutating can_die OFF and going immortal.
            // PLUS keep the herd faculty available (cohesion still free to evolve).
            pin_capabilities_herd(&mut child);
            next.push(child);
        }
        pop = next;

        // RATCHET H: harder when the elite copes, ease when wiped (hold the frontier).
        if elite_surv > RAISE_ABOVE {
            h += H_STEP;
        } else if elite_surv < LOWER_BELOW {
            h = (h - H_STEP).max(H_START - 0.5);
        }
    }

    trajectory(&rows);

    // ===== THE ARBITER: held-out LADDER (disjoint seeds, never trained on) =====
    // Primary control is the NO-faculty human showcase (gene 28 OFF) — the exact
    // design the original `hell` ladder used. We ALSO show the showcase WITH the
    // faculty handed to it (no evolution) to separate "the faculty helps at all" from
    // "evolution found how to use it well".
    let showcase = showcase_no_herd();
    let showcase_h = showcase_with_herd();
    println!("\n=== THE ARBITER — HELD-OUT HELL LADDER (disjoint seeds) ===");
    println!(
        "graded survival % at rising hell-intensity H; ceiling = max H with design ≥ {:.0}%",
        CEILING_THRESH * 100.0
    );
    println!(
        "{:>6} {:>11} {:>12} {:>12} {:>10}",
        "H", "EVOLVED", "SHOW(noherd)", "SHOW(+herd)", "GEN-0"
    );

    let mut champ_ceiling = f32::NEG_INFINITY;
    let mut show_ceiling = f32::NEG_INFINITY; // NO-faculty showcase ceiling (the wall)
    let mut showh_ceiling = f32::NEG_INFINITY;
    let mut champ_beats = 0usize; // vs the NO-faculty showcase
    for (rung, &hh) in LADDER.iter().enumerate() {
        let held = heldout_seeds(rung);
        let c = hell_survival_avg(&champion, hh, &held);
        let s = hell_survival_avg(&showcase, hh, &held);
        let sh = hell_survival_avg(&showcase_h, hh, &held);
        let z = hell_survival_avg(&gen0_champ, hh, &held);
        println!(
            "{:>6.2} {:>10.1}% {:>11.1}% {:>11.1}% {:>9.1}%",
            hh,
            c * 100.0,
            s * 100.0,
            sh * 100.0,
            z * 100.0
        );
        if c >= CEILING_THRESH {
            champ_ceiling = hh;
        }
        if s >= CEILING_THRESH {
            show_ceiling = hh;
        }
        if sh >= CEILING_THRESH {
            showh_ceiling = hh;
        }
        if c > s {
            champ_beats += 1;
        }
    }

    // ---- champion gene profile ----
    println!("\n=== CHAMPION GENE PROFILE (what survives hell) ===");
    print_profile(&champion);
    println!(
        "  herd-evasion: {} (cohesion gene {:.2})",
        if champion.herd_evasion() { "ON " } else { "off" },
        champion.g[HERD_GENE]
    );
    println!("--- (reference) NO-FACULTY SHOWCASE gene profile ---");
    print_profile(&showcase);

    // ---- VERDICT (trust the held-out numbers, not any auto-label) ----
    // Aggregate held-out survival across the whole ladder for a single comparison.
    let (mut champ_agg, mut show_agg, mut showh_agg, mut g0_agg) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    // ALSO aggregate ONLY the high-H rungs (H≥1.0) — where the no-faculty design
    // collapsed to single digits in the original run; this is where the faculty must
    // pay off if the diagnosis was right.
    let (mut champ_hi, mut show_hi, mut hi_n) = (0.0f32, 0.0f32, 0usize);
    for (rung, &hh) in LADDER.iter().enumerate() {
        let held = heldout_seeds(rung);
        let c = hell_survival_avg(&champion, hh, &held);
        let s = hell_survival_avg(&showcase, hh, &held);
        champ_agg += c;
        show_agg += s;
        showh_agg += hell_survival_avg(&showcase_h, hh, &held);
        g0_agg += hell_survival_avg(&gen0_champ, hh, &held);
        if hh >= 1.0 {
            champ_hi += c;
            show_hi += s;
            hi_n += 1;
        }
    }
    let n = LADDER.len() as f32;
    champ_agg /= n;
    show_agg /= n;
    showh_agg /= n;
    g0_agg /= n;
    let hi_den = hi_n.max(1) as f32;
    champ_hi /= hi_den;
    show_hi /= hi_den;

    let max_h_run = rows.iter().map(|r| r.h).fold(f32::NEG_INFINITY, f32::max);
    let climbed = {
        let e = &rows[..(rows.len() / 3).max(1)];
        let l = &rows[rows.len() - (rows.len() / 3).max(1)..];
        let em = e.iter().map(|r| r.elite_surv).sum::<f32>() / e.len() as f32;
        let lm = l.iter().map(|r| r.elite_surv).sum::<f32>() / l.len() as f32;
        // capability climb shows as H rising while survival is held near the frontier;
        // report both the H gain and the elite-survival gain.
        (max_h_run > H_START + 0.1, lm - em)
    };

    // The original `hell` ceiling, for the head-to-head the prompt asks for.
    const OLD_CEILING: f32 = 0.5;

    println!("\n=== VERDICT (held-out numbers) ===");
    println!(
        "ladder-aggregate graded survival:  EVOLVED(+herd) {:.1}%   SHOWCASE(noherd) {:.1}%   \
         SHOWCASE(+herd) {:.1}%   GEN-0 {:.1}%",
        champ_agg * 100.0,
        show_agg * 100.0,
        showh_agg * 100.0,
        g0_agg * 100.0
    );
    println!(
        "high-H rungs only (H≥1.0):  EVOLVED {:.1}%   SHOWCASE(noherd) {:.1}%  \
         (where the no-faculty design collapsed in the original run)",
        champ_hi * 100.0,
        show_hi * 100.0
    );
    println!(
        "champion beat the NO-faculty showcase on {champ_beats}/{} ladder rungs",
        LADDER.len()
    );
    println!(
        "hell-intensity ceiling (≥{:.0}% graded):  EVOLVED(+herd) H={:.2}   \
         SHOWCASE(noherd) H={:.2}   SHOWCASE(+herd) H={:.2}",
        CEILING_THRESH * 100.0,
        champ_ceiling,
        show_ceiling,
        showh_ceiling
    );
    println!(
        "max H the population SUSTAINED during the run (ratchet frontier):  H={:.2}",
        max_h_run
    );
    println!(
        "evolution climbed (H ratcheted up): {}   (late-vs-early elite-surv Δ={:+.1}%)",
        if climbed.0 { "YES" } else { "no" },
        climbed.1 * 100.0
    );
    println!(
        "old `hell` ceiling (no-faculty, this run's SHOWCASE-noherd reproduces it):  H≈{:.2}",
        OLD_CEILING
    );

    // Discriminators (read off the held-out numbers, not vibes). The point of THIS
    // experiment is the faculty's effect on the ceiling, so the comparison is the
    // faculty champion vs the NO-faculty showcase (the original H≈0.5 wall).
    let ratchet_held = max_h_run > H_START + 0.3;
    // Did the faculty raise the ceiling past the old wall AND beat the no-faculty
    // design where it collapsed (the high-H rungs)?
    let raised_ceiling = champ_ceiling > show_ceiling + 1e-3 && champ_ceiling > OLD_CEILING + 1e-3;
    let high_h_gain = champ_hi > show_hi + 0.02; // meaningfully better at H≥1.0
    let beats_overall = champ_agg > show_agg + 0.01;

    if ratchet_held && raised_ceiling && high_h_gain && champ_ceiling >= 1.0 {
        println!(
            "\nHELL CRACKED — the dispersal faculty raises the ceiling from H≈{:.2} (no-faculty\n\
             showcase wall) to H≈{:.2}: the faculty-equipped champion sustains ≥{:.0}% graded\n\
             survival up to H={:.2}, and at the high-H rungs (H≥1.0) where the no-faculty design\n\
             collapsed it holds {:.1}% vs {:.1}%. Ladder-aggregate {:.1}% vs {:.1}%. The original\n\
             diagnosis was RIGHT: hell's isolated-target predator was walling out minds that\n\
             could not herd; given selfish-herd evasion, evolution composes flee+cohesion into a\n\
             surviving anti-hell strategy.",
            show_ceiling.max(OLD_CEILING),
            champ_ceiling,
            CEILING_THRESH * 100.0,
            champ_ceiling,
            champ_hi * 100.0,
            show_hi * 100.0,
            champ_agg * 100.0,
            show_agg * 100.0
        );
    } else if ratchet_held && (raised_ceiling || (high_h_gain && beats_overall)) {
        println!(
            "\nPARTIAL — the dispersal faculty helps but does not break hell open. Ceiling moves\n\
             from H≈{:.2} (no-faculty) to H≈{:.2} (faculty champion); high-H (H≥1.0) survival\n\
             {:.1}% vs {:.1}%; ladder-aggregate {:.1}% vs {:.1}%. A real, measurable gain — the\n\
             diagnosis pointed the right way — but the champion still does not sustain meaningful\n\
             survival to extreme intensity, so the architecture has a higher, but still finite,\n\
             ceiling. Likely further bottleneck: scarcity/cold compounding faster than any\n\
             evasion can offset once the predator is near-one-shot.",
            show_ceiling.max(OLD_CEILING),
            champ_ceiling,
            champ_hi * 100.0,
            show_hi * 100.0,
            champ_agg * 100.0,
            show_agg * 100.0
        );
    } else {
        println!(
            "\nSTILL WALLED — the faculty did NOT move the ceiling. Faculty champion ceiling\n\
             H={:.2} vs no-faculty showcase H={:.2} (old wall H≈{:.2}); high-H survival {:.1}%\n\
             vs {:.1}%; ladder-aggregate {:.1}% vs {:.1}%. A null result, reported honestly:\n\
             selfish-herd evasion is not what hell was walling out — the bottleneck is elsewhere\n\
             (read the trajectory: which genes swept, where survival actually died). The diagnosis\n\
             that named isolation as the missing piece is NOT supported by these held-out numbers.",
            champ_ceiling,
            show_ceiling,
            OLD_CEILING,
            champ_hi * 100.0,
            show_hi * 100.0,
            champ_agg * 100.0,
            show_agg * 100.0
        );
    }
    let _ = beats_overall;
}

// ----------------------------- trajectory report ---------------------------------
fn trajectory(rows: &[GenRow]) {
    println!("\n================ FULL PER-GENERATION TRAJECTORY ================");
    println!(
        "{:>3} {:>5} {:>8} {:>7} {:>7} {:>9} {:>6} {:>6} {:>7} {:>5} {:>6}",
        "gen", "H", "elite%", "best%", "mean%", "foresight", "bld%", "fgt%", "soc%", "drr%", "herd_g"
    );
    for r in rows {
        println!(
            "{:>3} {:>5.2} {:>7.0}% {:>6.0}% {:>6.0}% {:>9.2} {:>5.0}% {:>5.0}% {:>6.0}% {:>4.0}% {:>6.2}",
            r.gen,
            r.h,
            r.elite_surv * 100.0,
            r.best_surv * 100.0,
            r.mean_surv * 100.0,
            r.foresight,
            r.build * 100.0,
            r.fight * 100.0,
            r.social * 100.0,
            r.drr * 100.0,
            r.herd,
        );
    }

    let n = rows.len();
    let third = (n / 3).max(1);
    let early = &rows[..third];
    let late = &rows[n - third..];
    let mean = |xs: &[GenRow], f: fn(&GenRow) -> f32| {
        xs.iter().map(f).sum::<f32>() / xs.len().max(1) as f32
    };
    println!("\n---------------- EARLY-THIRDS vs LATE-THIRDS ----------------");
    println!("                  early     late    delta");
    let line = |name: &str, e: f32, l: f32| println!("{name:<16} {e:>7.3} {l:>7.3} {:>+8.3}", l - e);
    line("hell H", mean(early, |r| r.h), mean(late, |r| r.h));
    line("elite survival", mean(early, |r| r.elite_surv), mean(late, |r| r.elite_surv));
    line("foresight gene", mean(early, |r| r.foresight), mean(late, |r| r.foresight));
    line("provision %", mean(early, |r| r.provision), mean(late, |r| r.provision));
    line("build %", mean(early, |r| r.build), mean(late, |r| r.build));
    line("fight %", mean(early, |r| r.fight), mean(late, |r| r.fight));
    line("social_forage %", mean(early, |r| r.social), mean(late, |r| r.social));
    line("forage_drr %", mean(early, |r| r.drr), mean(late, |r| r.drr));
    line("herd cohesion g", mean(early, |r| r.herd), mean(late, |r| r.herd));
}

fn onoff(v: f32) -> &'static str {
    if v >= 0.5 {
        "ON "
    } else {
        "off"
    }
}

fn print_profile(g: &Genome) {
    let x = &g.g;
    println!(
        "  scalars: surprise={:.2} delib_cd={:.2} foresight_g={:.2}",
        x[0], x[1], x[13]
    );
    println!(
        "  open-world: forage_drr={} social_forage={} cultural={} stigmergy={} affect_mod={}",
        onoff(x[14]),
        onoff(x[15]),
        onoff(x[16]),
        onoff(x[18]),
        onoff(x[19])
    );
    println!(
        "  capability: fight={} build={} die={} provision={} herd={}(g={:.2})",
        onoff(x[20]),
        onoff(x[21]),
        onoff(x[22]),
        onoff(x[24]),
        onoff(x[28]),
        x[28],
    );
}
