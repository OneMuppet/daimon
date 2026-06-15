#!/usr/bin/env bash
# Daimon demo — watch one engine produce five distinct minds.
#
#   ./scripts/demo.sh            # narrated life for each persona
#   ./scripts/demo.sh --quiet    # just the end-of-life reviews
set -euo pipefail
cd "$(dirname "$0")/.."

TICKS="${TICKS:-300}"
QUIET="${1:-}"

echo "Building…"
cargo build --release --quiet

PERSONAS=(balanced curious timid social bold)
for p in "${PERSONAS[@]}"; do
  echo
  echo "════════════════════════════════════════════════════════════════════════"
  echo "  PERSONA: $p   (${TICKS} ticks, seed 0xDA13)"
  echo "════════════════════════════════════════════════════════════════════════"
  ./target/release/daimon --persona "$p" --ticks "$TICKS" $QUIET
done

echo
echo "Same code, same world — five different lives. Try: cargo test"
