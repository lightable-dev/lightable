#!/bin/sh
# Builds the browser version into target/web (open index.html from a local
# web server, e.g. `python3 -m http.server -d target/web 8080`).
# Needs wasm-bindgen-cli at the version in Cargo.lock:
#   cargo install wasm-bindgen-cli --version 0.2.129 --locked
set -eu
cd "$(dirname "$0")/.."
OUT=target/web
cargo build -p lightable --release --target wasm32-unknown-unknown
rm -rf "$OUT"
mkdir -p "$OUT"
wasm-bindgen --target web --no-typescript --out-dir "$OUT" \
  target/wasm32-unknown-unknown/release/lightable.wasm
cp web/index.html web/main.js "$OUT/"
ls -l "$OUT"
