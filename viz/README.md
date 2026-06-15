# Training visualisation

A self-contained, dependency-free animation of **how Daimon's minds are trained
over generations to reach the ultimate end goal** — driven by *real data* exported
from the autogenesis loop (not a mock-up).

```bash
./scripts/viz-training.sh          # regenerate data + serve on :8090
# or, manually:
cargo run -p daimon-game --example autogenesis_trace --release   # writes training_data.json
python3 -m http.server -d viz 8090 && open http://localhost:8090
```

It works in any browser (no WebGPU needed). What it shows, generation by generation:

- **The five facets of a believable life** (survival, safety, balance, expression,
  exploration) filling toward their target bars — each turning green as it clears —
  and the weighted aggregate, with the verdict pill flipping to **END-GOAL REACHED**.
- **Fitness across generations** — champion vs. population mean, the whole
  population as faint dots, and the target threshold; the loop self-halts the
  moment it clears every facet.
- **What the loop learned matters** — per-gene sensitivity bars (the loop mutates
  high-impact genes harder; the foraging/anticipation genes rise to the top).
- **The road here, honest dead ends included** — the four outer-loop turns
  (anticipation +, DRR −, commons − → +, the diagnosis + fair-world breakthrough).
- **The cross-disciplinary mechanism stack** and the **held-out validation** on
  unseen seeds.

This is the *training* view. For the live village (the trained behaviour embodied),
see `scripts/build-web.sh` (WebGPU).
