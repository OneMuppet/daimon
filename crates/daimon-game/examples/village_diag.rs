//! Diagnose the big-village die-off: build the live showcase world exactly as
//! `Game::new` does and trace living_count vs season over time, so we can see
//! WHETHER the wipe correlates with winter (cold), steady starvation, or the
//! stalker. Throwaway tool.

use daimon_game::sim::GameWorld;
use daimon_mind::Genome;

fn main() {
    let mut genome = Genome::showcase();
    genome.g[21] = 1.0; // can_build
    genome.g[22] = 1.0; // can_die
    genome.g[23] = 1.0; // can_grieve
    genome.g[24] = 1.0; // can_provision

    // the same big village as Game::new
    let mut world = GameWorld::with_genome_sized(0x61, 64, &genome, 124, 84, 7);
    let open: bool = std::env::var("OPEN").map(|s| s == "1").unwrap_or(false);
    if open {
        world.set_open_world(true);
        world.open_world_cold_scale = 0.15;
        world.granary_food = 250.0;
    }
    // MAT=1 traces the live materials economy (the showcase default): wood/stone stocks
    // grow as minds gather, and walls consume them.
    let mat: bool = std::env::var("MAT").map(|s| s == "1").unwrap_or(true);
    if mat {
        world.set_materials_world(true);
    }
    world.soften_stalker();
    // WILD=1 (default) traces the live natural ecosystem (wolves/bears/deer) so we can
    // watch mind survival against the wildlife, plus the herd staying renewable.
    let wild: bool = std::env::var("WILD").map(|s| s == "1").unwrap_or(true);
    if wild {
        world.set_wildlife(true);
    }
    eprintln!("(OPEN={open} MAT={mat} WILD={wild})");

    println!("tick  alive/64  walls  wolf  bear  deer(live)  killers");
    let mut wolf_kills = 0usize;
    let mut bear_kills = 0usize;
    let mut prev_cause: std::collections::HashMap<u32, &'static str> = std::collections::HashMap::new();
    for t in 0..=6000u32 {
        if t % 200 == 0 {
            let live_deer = world.deer.iter().filter(|d| !world.deer_hidden(d)).count();
            println!(
                "{t:5}  alive {:>3}  walls {:>4}  wolf {:>2}  bear {:>2}  deer {:>2}  (wolfkills {wolf_kills} bearkills {bear_kills})",
                world.living_count(),
                world.walls.len(),
                world.wolves.len(),
                world.bears.len(),
                live_deer,
            );
        }
        if world.living_count() == 0 {
            println!("ALL DEAD by tick {t} (season {:?})", world.season());
            break;
        }
        world.step();
        // tally fresh deaths by cause (a mind that just died this tick).
        for a in &world.agents {
            if !a.alive {
                let id = a.id.0;
                if prev_cause.insert(id, a.death_cause).is_none() {
                    match a.death_cause {
                        "a wolf" => wolf_kills += 1,
                        "a bear" => bear_kills += 1,
                        _ => {}
                    }
                }
            }
        }
    }
    let final_alive = world.living_count();
    println!("FINAL: {final_alive}/64 alive  | wolf-kills {wolf_kills}  bear-kills {bear_kills}");

    // CLUSTER REPORT: re-derive the wall footprints exactly as the renderer does (8-conn
    // flood-fill) and classify each by the same shape rules, so we can confirm the village
    // has VARIED building types without eyeballing a screenshot.
    if mat {
        use std::collections::HashSet;
        use daimon_core::Pos;
        let cells: HashSet<Pos> = world.walls.iter().copied().collect();
        let mut sorted: Vec<Pos> = cells.iter().copied().collect();
        sorted.sort_by_key(|p| (p.y, p.x));
        let mut seen: HashSet<Pos> = HashSet::new();
        let (mut home, mut longh, mut gran, mut tower) = (0, 0, 0, 0);
        let mut sizes: Vec<(i32, i32, usize)> = Vec::new();
        for &start in &sorted {
            if seen.contains(&start) {
                continue;
            }
            let mut stack = vec![start];
            let mut comp = Vec::new();
            seen.insert(start);
            while let Some(p) = stack.pop() {
                comp.push(p);
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        if dx == 0 && dy == 0 {
                            continue;
                        }
                        let n = Pos::new(p.x + dx, p.y + dy);
                        if cells.contains(&n) && seen.insert(n) {
                            stack.push(n);
                        }
                    }
                }
            }
            let (mut x0, mut x1, mut z0, mut z1) = (i32::MAX, i32::MIN, i32::MAX, i32::MIN);
            for p in &comp {
                x0 = x0.min(p.x); x1 = x1.max(p.x); z0 = z0.min(p.y); z1 = z1.max(p.y);
            }
            let bw = (x1 - x0 + 1).min(7);
            let bd = (z1 - z0 + 1).min(7);
            let (bwf, bdf) = (bw as f32, bd as f32);
            let span = bwf.max(bdf);
            let area = bwf * bdf;
            let elong = span / span.min(bwf).min(bdf).max(1.0);
            sizes.push((bw, bd, comp.len()));
            let centre = Pos::new((x0 + x1) / 2, (z0 + z1) / 2);
            let near_heart = centre.manhattan(world.granary) <= 18;
            // mirror the renderer's hash-driven classifier (geo::hash_unit, key 91).
            let chash = {
                let anchor = comp.iter().min_by_key(|p| (p.y, p.x)).copied().unwrap();
                ((anchor.x as i64) << 20 ^ anchor.y as i64) as u64
            };
            let roll = daimon_game::geo::hash_unit(chash, 91);
            let granary_ok = area >= 9.0 && (roll < if near_heart { 0.55 } else { 0.22 });
            if elong >= 2.0 && span >= 4.0 { longh += 1; }
            else if granary_ok { gran += 1; }
            else if area <= 12.0 && comp.len() >= 3 && roll > 0.78 { tower += 1; }
            else if area >= 16.0 && roll > 0.62 { longh += 1; }
            else { home += 1; }
        }
        let total = home + longh + gran + tower;
        eprintln!("CLUSTERS: {total} buildings — homes {home}, longhouses {longh}, granaries {gran}, watchtowers {tower}");
        eprintln!("(note: the per-cluster hash jitter splits some of these in the live render; this is the shape-only census)");
        let big = sizes.iter().filter(|(w, d, _)| *w >= 5 || *d >= 5).count();
        eprintln!("FOOTPRINTS: {} clusters, {big} with a 5+ axis (bigger buildings)", sizes.len());
    }
}
