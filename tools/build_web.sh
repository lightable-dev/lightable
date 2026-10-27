#!/bin/sh
# Builds the browser version into target/web (open index.html from a local
# web server, e.g. `python3 -m http.server -d target/web 8080`).
# Needs wasm-bindgen-cli at the version in Cargo.lock:
#   cargo install wasm-bindgen-cli --version 0.2.129 --locked
# and, for the smallest download, wasm-opt from binaryen (skipped with a
# warning when missing; WASM_OPT_REQUIRED=1 makes it an error, as in CI).
set -eu
cd "$(dirname "$0")/.."
OUT=target/web
# Size-optimised profile (Cargo.toml: opt-level "s", LTO, one codegen unit).
cargo build -p lightable --profile wasm-release --target wasm32-unknown-unknown
rm -rf "$OUT"
mkdir -p "$OUT"
wasm-bindgen --target web --no-typescript --out-dir "$OUT" \
  target/wasm32-unknown-unknown/wasm-release/lightable.wasm
if command -v wasm-opt >/dev/null 2>&1; then
  # The WebAssembly features rustc enables by default for this target.
  wasm-opt -Oz --enable-bulk-memory --enable-nontrapping-float-to-int \
    --enable-sign-ext --enable-mutable-globals --enable-reference-types \
    --enable-multivalue \
    "$OUT/lightable_bg.wasm" -o "$OUT/lightable_bg.wasm"
elif [ "${WASM_OPT_REQUIRED:-0}" = 1 ]; then
  echo "wasm-opt not found" >&2
  exit 1
else
  echo "warning: wasm-opt not found; the .wasm is not size-optimised further" >&2
fi
# What the page shows while the app loads: the default example's overlay
# and change list, made by the comparison now (not checked in).
A=crates/lightable/assets
cargo run --release -p lightable-diff --example web_preview -- \
  "$A/bracket-rev-b.dxf" "$A/bracket-rev-c.dxf" "Rev B" "Rev C" \
  "Steel bracket, Rev B to Rev C" "$OUT" \
  "Example: the drafter clouded only the bolt note."
cp web/main.js "$OUT/"
# The change list goes into the page; the progress line needs the unpacked
# size of the app.
WASM_BYTES=$(wc -c < "$OUT/lightable_bg.wasm" | tr -d ' ')
awk -v list="$OUT/changes.html" -v bytes="$WASM_BYTES" '
  /<!-- CHANGES -->/ { while ((getline line < list) > 0) print "      " line; next }
  { gsub(/__WASM_BYTES__/, bytes); print }
' web/index.html > "$OUT/index.html"
rm "$OUT/changes.html"
# The public demo page (lightable.dev/demo/), only once approved videos are
# in web/demo/ (`node tools/demo.mjs approve`); drafts never go there.
# The page needs all five feature videos in both shapes and their posters
# (their captions are in the picture: no subtitle files).
PAGES="/"
DEMO_FILES="web/demo/index.html"
for f in unclouded tiny resave blocks big; do
  for s in landscape vertical; do
    DEMO_FILES="$DEMO_FILES web/demo/$f-$s.mp4 web/demo/$f-$s.jpg"
  done
done
DEMO_READY=1
for file in $DEMO_FILES; do [ -f "$file" ] || DEMO_READY=0; done
if [ "$DEMO_READY" = 1 ]; then
  mkdir -p "$OUT/demo"
  cp $DEMO_FILES "$OUT/demo/"
  PAGES="/ /demo/"
fi
{
  echo '<?xml version="1.0" encoding="UTF-8"?>'
  echo '<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">'
  for page in $PAGES; do echo "  <url><loc>https://lightable.dev$page</loc></url>"; done
  echo '</urlset>'
} > "$OUT/sitemap.xml"
printf 'User-agent: *\nAllow: /\nSitemap: https://lightable.dev/sitemap.xml\n' > "$OUT/robots.txt"
ls -l "$OUT"
