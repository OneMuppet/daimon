#!/usr/bin/env bash
# Build Daimon: Smallworld for the web (WebGPU) and stage it for serving.
#
#   ./scripts/build-web.sh && python3 -m http.server -d crates/daimon-game/web 8080
#   then open http://localhost:8080  (needs a WebGPU-capable browser)
set -euo pipefail
cd "$(dirname "$0")/.."

# cargo-installed binaries may not be on a non-interactive PATH.
export PATH="$HOME/.cargo/bin:$PATH"

if ! command -v wasm-bindgen >/dev/null 2>&1; then
  echo "wasm-bindgen CLI not found. Install a version matching the wasm-bindgen"
  echo "crate (see Cargo.lock), e.g.:"
  echo "    cargo install wasm-bindgen-cli --version <X.Y.Z>"
  exit 1
fi

echo "==> cargo build (wasm32, release)"
cargo build -p daimon-game --lib --target wasm32-unknown-unknown --release

echo "==> wasm-bindgen"
wasm-bindgen target/wasm32-unknown-unknown/release/daimon_game.wasm \
  --out-dir crates/daimon-game/web \
  --target web \
  --no-typescript

echo
echo "Done. Serve and open:"
echo "    python3 -m http.server -d crates/daimon-game/web 8080"
echo "    open http://localhost:8080"
