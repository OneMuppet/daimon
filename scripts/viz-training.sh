#!/usr/bin/env bash
# Regenerate the training trace and serve the training visualisation.
#
#   ./scripts/viz-training.sh        # build data + serve on :8090
#   then open http://localhost:8090
set -euo pipefail
cd "$(dirname "$0")/.."

echo "==> exporting training trace (runs the autogenesis loop)…"
cargo run -q -p daimon-game --example autogenesis_trace --release

echo
echo "==> serving viz/ at http://localhost:8090  (Ctrl-C to stop)"
python3 -m http.server -d viz 8090
