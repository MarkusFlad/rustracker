#!/usr/bin/env sh
# Build the WASM replayer and start a local web server.
set -e
cd "$(dirname "$0")"

rustup target add wasm32-unknown-unknown
cargo build --release --target wasm32-unknown-unknown --lib
cp target/wasm32-unknown-unknown/release/modplayer.wasm www/
echo "WASM: $(wc -c < www/modplayer.wasm) Bytes"

if [ "$1" != "--no-serve" ]; then
  echo "Öffne http://localhost:8080"
  cd www && python3 -m http.server 8080
fi
