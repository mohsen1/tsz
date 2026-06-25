#!/usr/bin/env bash
#
# #14345 Stage-2 fp-ts gauge harness.
# ===================================
# fp-ts is tsc-CLEAN (oracle = 0 errors), so every tsz diagnostic on the guard
# tsconfig is a false positive. This harness runs tsz with TSZ_TYPEPARAM_DECL_IDENTITY
# OFF and ON, emits the diagnostic identity keys (basename TAB line TAB col TAB code),
# and computes:
#   - OFF total (the byte-parity baseline, ~1477)
#   - ON total
#   - ON-only keys (FPs present ONLY when flag ON = regressions/over-fire)
#   - OFF-only keys (FPs cleared by the flag = the 278 + brick fixes)
#   - per-code breakdown of each
#
# Usage: stage2-fpts-gauge.sh [tsz-bin] [outdir]
set -u

TSZ_BIN="${1:-/Users/mohsen/code/tsz-stage2/.target/release/tsz}"
OUTDIR="${2:-/tmp/stage2-gauge}"
FPTS_DIR="/Users/mohsen/code/tsz/.target/project-compile-guard/fp-ts"
TSCONFIG="$FPTS_DIR/tsconfig.tsz-guard.json"

mkdir -p "$OUTDIR"

if [[ ! -x "$TSZ_BIN" ]]; then
  echo "error: tsz binary not found: $TSZ_BIN" >&2
  exit 2
fi

# md5 fingerprint of the binary (detect a worktree swap mid-run).
BIN_MD5="$(md5 -q "$TSZ_BIN" 2>/dev/null || md5sum "$TSZ_BIN" | cut -d' ' -f1)"
echo "# tsz bin md5: $BIN_MD5"

# canonical identity key extractor: basename<TAB>line<TAB>col<TAB>code
# tsz diag lines look like: path/to/file.ts(LINE,COL): error TSNNNN: msg
# BSD-awk compatible (no 3-arg match): parse with index/substr.
keys() {
  awk '
    {
      sub(/\r$/, "")
      # locate the "(line,col): error TS" marker
      p = match($0, /\([0-9]+,[0-9]+\): error TS[0-9]+/)
      if (p == 0) next
      path = substr($0, 1, p - 1)
      n = split(path, parts, "/")
      base = parts[n]
      rest = substr($0, p)               # "(line,col): error TScode: ..."
      # strip leading "("
      rest = substr(rest, 2)             # "line,col): error TScode: ..."
      ci = index(rest, ",")
      line = substr(rest, 1, ci - 1)
      rest = substr(rest, ci + 1)        # "col): error TScode: ..."
      pi = index(rest, ")")
      col = substr(rest, 1, pi - 1)
      # extract code after "error TS"
      ei = index($0, "error TS")
      code = substr($0, ei + 6)          # "TScode: ..."
      sub(/:.*$/, "", code)              # "TScode"
      sub(/ .*$/, "", code)
      print base "\t" line "\t" col "\t" code
    }
  '
}

run_flag() {
  local flag="$1" out="$2"
  RAYON_NUM_THREADS="${RAYON_NUM_THREADS:-8}" TSZ_TYPEPARAM_DECL_IDENTITY="$flag" \
    TSZ_USE_EMBEDDED_LIBS=1 RUST_MIN_STACK=536870912 \
    "$TSZ_BIN" --noEmit -p "$TSCONFIG" > "$out" 2>&1 || true
}

echo "# running flag OFF..."
run_flag 0 "$OUTDIR/off.raw"
echo "# running flag ON..."
run_flag 1 "$OUTDIR/on.raw"

keys < "$OUTDIR/off.raw" | sort > "$OUTDIR/off.keys"
keys < "$OUTDIR/on.raw"  | sort > "$OUTDIR/on.keys"

OFF_N=$(wc -l < "$OUTDIR/off.keys" | tr -d ' ')
ON_N=$(wc -l < "$OUTDIR/on.keys" | tr -d ' ')

# ON-only = regressions (over-fire): in ON, not in OFF
comm -13 "$OUTDIR/off.keys" "$OUTDIR/on.keys" > "$OUTDIR/on_only.keys"
# OFF-only = fixes (cleared by flag): in OFF, not in ON
comm -23 "$OUTDIR/off.keys" "$OUTDIR/on.keys" > "$OUTDIR/off_only.keys"

ON_ONLY_N=$(wc -l < "$OUTDIR/on_only.keys" | tr -d ' ')
OFF_ONLY_N=$(wc -l < "$OUTDIR/off_only.keys" | tr -d ' ')

echo
echo "==== Stage-2 fp-ts gauge ===="
echo "OFF total (byte-parity baseline) : $OFF_N"
echo "ON  total                        : $ON_N"
echo "OFF-only (cleared by flag = fixes): $OFF_ONLY_N"
echo "ON-only  (flag-introduced = REGR) : $ON_ONLY_N"
echo "net (ON - OFF)                    : $((ON_N - OFF_N))"
echo
echo "---- ON-only (regressions) by code ----"
awk '{print $4}' "$OUTDIR/on_only.keys" | sort | uniq -c | sort -rn
echo
echo "---- OFF-only (fixes) by code ----"
awk '{print $4}' "$OUTDIR/off_only.keys" | sort | uniq -c | sort -rn
echo
echo "# bin md5 (re-check): $(md5 -q "$TSZ_BIN" 2>/dev/null || md5sum "$TSZ_BIN" | cut -d' ' -f1)"
