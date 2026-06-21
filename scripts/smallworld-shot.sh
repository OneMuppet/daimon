#!/usr/bin/env bash
# Headless WebGPU screenshot of the Daimon: Smallworld web build — the iterate-on-the-look
# loop. Standalone (the sim runs in-wasm, no server), so virtual-time just advances frames.
# Usage: scripts/smallworld-shot.sh [out.png]
#   NOBUILD=1  skip the wasm rebuild (screenshot the existing crates/daimon-game/web build)
#   WAIT=ms    virtual-time budget (default 12000 — lets the island settle + minds move)
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"; cd "$ROOT"
OUT="${1:-/tmp/smallworld.png}"
PORT="${PORT:-18793}"
WAIT="${WAIT:-12000}"
CHROME="${CHROME:-/Applications/Google Chrome.app/Contents/MacOS/Google Chrome}"

if [[ "${NOBUILD:-0}" != "1" ]]; then
  echo "[smallworld-shot] building wasm web…"
  ./scripts/build-web.sh >/tmp/smallworld-build.log 2>&1 || { echo "build failed — see /tmp/smallworld-build.log"; tail -5 /tmp/smallworld-build.log; exit 1; }
fi

pkill -f "http.server $PORT" 2>/dev/null || true; sleep 0.2
( cd crates/daimon-game/web && python3 -m http.server "$PORT" --bind 127.0.0.1 >/tmp/smallworld-serve.log 2>&1 ) &
SRV=$!
sleep 1
rm -f "$OUT"
"$CHROME" --headless=new --enable-unsafe-webgpu --use-angle=metal \
  --virtual-time-budget="$WAIT" --window-size=1600,1000 --screenshot="$OUT" \
  "http://127.0.0.1:$PORT/index.html" >/tmp/smallworld-chrome.log 2>&1 || true
kill "$SRV" 2>/dev/null || true
[ -f "$OUT" ] && echo "[smallworld-shot] wrote $OUT" || { echo "[smallworld-shot] FAILED (see /tmp/smallworld-chrome.log)"; exit 1; }
