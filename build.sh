#!/usr/bin/env sh
# Build the WASM replayer and start a local web server.
set -e
cd "$(dirname "$0")"

rustup target add wasm32-unknown-unknown
cargo build --release --target wasm32-unknown-unknown --lib
cp target/wasm32-unknown-unknown/release/modplayer.wasm www/
cp target/wasm32-unknown-unknown/release/modplayer.wasm www-fb/
# Mandelbrot SIMD kernel (no_core, needs RUSTC_BOOTSTRAP)
RUSTC_BOOTSTRAP=1 rustc --edition 2021 --target wasm32-unknown-unknown --crate-type cdylib \
  -C opt-level=3 -C target-feature=+simd128 -C panic=abort -C strip=symbols \
  -o www/mandel.wasm kernel/mandel_wasm.rs
cp www/mandel.wasm www-fb/
echo "WASM: $(wc -c < www/modplayer.wasm) Bytes"

if [ "$1" != "--no-serve" ]; then
  echo "Öffne http://localhost:8080"
  cd www && python3 -m http.server 8080
fi
