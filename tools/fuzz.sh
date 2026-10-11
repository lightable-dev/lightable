#!/usr/bin/env bash
# Seeds and runs the lightable-dxf fuzz targets (needs a nightly toolchain and
# `cargo install cargo-fuzz`).
#
# Usage: tools/fuzz.sh SECONDS [TARGET...]    (default targets: all three)
#
# Seeds every target's corpus from tests/corpus and the fuzz regression
# inputs, then runs the targets in parallel for SECONDS each. Crashes land in
# crates/lightable-dxf/fuzz/artifacts/<target>/; add each one to
# crates/lightable-dxf/tests/fuzz-regressions/ once fixed.
set -eu
SECONDS_TO_RUN=${1:?usage: fuzz.sh SECONDS [TARGET...]}
shift
TARGETS=("$@")
[ ${#TARGETS[@]} -eq 0 ] && TARGETS=(reader tokenizer text)
ROOT=$(cd "$(dirname "$0")/.." && pwd)
FUZZ="$ROOT/crates/lightable-dxf/fuzz"

for t in "${TARGETS[@]}"; do
    mkdir -p "$FUZZ/corpus/$t"
    cp -n "$ROOT"/tests/corpus/*.dxf "$FUZZ/corpus/$t/" 2>/dev/null || true
    cp -n "$ROOT"/crates/lightable-dxf/tests/fuzz-regressions/* "$FUZZ/corpus/$t/" 2>/dev/null || true
done

cd "$ROOT/crates/lightable-dxf"
cargo +nightly fuzz build --sanitizer none >/dev/null
pids=()
for t in "${TARGETS[@]}"; do
    # -max_len keeps executions fast; larger seeds are truncated, not dropped.
    # No sanitizer: lightable-dxf has no unsafe code, and Rust's overflow and bounds
    # checks stay on (debug assertions). This runs many times faster.
    cargo +nightly fuzz run --sanitizer none "$t" -- -max_total_time="$SECONDS_TO_RUN" -max_len=16384 \
        -timeout=10 -print_final_stats=1 > "$FUZZ/$t.log" 2>&1 &
    pids+=($!)
done
status=0
for i in "${!pids[@]}"; do
    if ! wait "${pids[$i]}"; then
        echo "${TARGETS[$i]}: FAILED, see $FUZZ/${TARGETS[$i]}.log"
        status=1
    else
        echo "${TARGETS[$i]}: ok ($(grep -E 'stat::number_of_executed_units' "$FUZZ/${TARGETS[$i]}.log" | awk '{print $2}') runs)"
    fi
done
exit $status
