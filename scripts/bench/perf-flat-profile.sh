#!/usr/bin/env bash
#
# Headless symbolicated flat self-time profile for tsz (#13934).
#
# `scripts/bench/perf-hotspots.sh` tells you a case is slow (wall-clock vs
# tsgo); this tells you *which function* is hot. It builds tsz with symbols
# retained, samples a run with `samply`, and resolves the sampled frame
# addresses against the binary (macOS `atos`), printing a ranked self-time /
# inclusive table of demangled tsz frames.
#
# Why a dedicated tool: `samply record --save-only` stores raw addresses, not
# names, and the bench/dist profiles strip symbols — so headless parsing yields
# `0x44f8`-style frames. This captures the working recipe (debug=2 + strip=false
# build, then atos against the per-frame lib-relative address) so perf
# investigations (#12101, #13242, #13250, …) don't fly blind.
#
# Usage:
#   scripts/bench/perf-flat-profile.sh path/to/file.ts
#   scripts/bench/perf-flat-profile.sh -p path/to/tsconfig.json
#   scripts/bench/perf-flat-profile.sh -p tsconfig.json --iterations 12 --top 30
#   scripts/bench/perf-flat-profile.sh -p tsconfig.json --json-file /tmp/profile.json
#   scripts/bench/perf-flat-profile.sh --no-build -p tsconfig.json   # reuse symbol build
#
# Options:
#   -p <tsconfig>      Type-check a project (passed to tsz as `-p`).
#   --iterations N     Loop the target N times under the sampler (default 8) so
#                      short runs accumulate enough samples.
#   --top N            Rows to print (default 25).
#   --no-build         Skip the symbol-retaining build; reuse the existing binary.
#   --bin <path>       Use this tsz binary instead of building.
#   --json-file <path> Write the ranked flat profile as JSON.
#   --help
#
# macOS only for symbol resolution (uses `atos`). On Linux, swap the resolver
# for `llvm-symbolizer`/`addr2line` over the same lib-relative addresses.
#
# Attribution is to the innermost *non-inlined* function: `atos` rolls an
# inlined leaf up to the function it was inlined into, so e.g. `main`'s body
# reads as its enclosing symbol. Read `incl%` to find the structural entry
# point to change; `self%` for where cycles are actually spent.
#
# NOTE: builds a *separate* symbol-retaining dist-fast binary; it does not touch
# the PGO bench binary used by bench-vs-tsgo. The debug=2+strip=false artifacts
# are large (tens of GB with deps debug info) — reclaim with
# `rm -rf <target>/dist-fast` or `scripts/setup/clean.sh` when done.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$ROOT_DIR"

ITERATIONS=8
TOP=25
NO_BUILD=false
BIN_OVERRIDE=""
JSON_FILE=""
declare -a TSZ_ARGS=()

usage() { sed -n '2,40p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; }

while [[ $# -gt 0 ]]; do
    case "$1" in
        -p) TSZ_ARGS+=(-p "$2"); shift 2 ;;
        --iterations) ITERATIONS="$2"; shift 2 ;;
        --top) TOP="$2"; shift 2 ;;
        --no-build) NO_BUILD=true; shift ;;
        --bin) BIN_OVERRIDE="$2"; shift 2 ;;
        --json-file) JSON_FILE="$2"; shift 2 ;;
        --help|-h) usage; exit 0 ;;
        *) TSZ_ARGS+=("$1"); shift ;;
    esac
done

if [[ ${#TSZ_ARGS[@]} -eq 0 ]]; then
    echo "error: pass a .ts file or '-p <tsconfig>' to profile" >&2
    usage; exit 2
fi
for tool in samply atos python3; do
    command -v "$tool" >/dev/null 2>&1 || { echo "error: '$tool' not found in PATH" >&2; exit 1; }
done

# Resolve the target dir the same way cargo does: $CARGO_TARGET_DIR wins,
# otherwise the per-worktree `.target` from .cargo/config.toml.
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT_DIR/.target}"
TSZ_BIN="${BIN_OVERRIDE:-$TARGET_DIR/dist-fast/tsz}"

if [[ "$NO_BUILD" != true && -z "$BIN_OVERRIDE" ]]; then
    echo "Building dist-fast tsz with symbols retained (debug=2, strip=false)..."
    CARGO_PROFILE_DIST_FAST_DEBUG=2 CARGO_PROFILE_DIST_FAST_STRIP=false \
        cargo build --quiet --profile dist-fast -p tsz-cli --bin tsz
fi
[[ -x "$TSZ_BIN" ]] || { echo "error: tsz binary not found at $TSZ_BIN" >&2; exit 1; }

# Auto-detect the binary's __TEXT vmaddr base (Rust macho arm64 is 0x100000000,
# but read it rather than assume) so atos resolves lib-relative frame addresses.
TEXT_BASE="$(otool -l "$TSZ_BIN" 2>/dev/null \
    | awk '/segname __TEXT/{t=1} t&&/vmaddr/{print $2; exit}')"
TEXT_BASE="${TEXT_BASE:-0x100000000}"

PROFILE_JSON="$(mktemp -t tsz-flatprof-XXXX).json.gz"
trap 'rm -f "$PROFILE_JSON"' EXIT

echo "Sampling: $TSZ_BIN --noEmit ${TSZ_ARGS[*]}  (x${ITERATIONS})"
samply record --save-only -o "$PROFILE_JSON" -- \
    bash -c 'for _ in $(seq "$1"); do "$2" --noEmit "${@:3}" >/dev/null 2>&1; done' \
    _ "$ITERATIONS" "$TSZ_BIN" "${TSZ_ARGS[@]}" >/dev/null 2>&1 || true

# Parse + symbolicate + rank entirely in Python so attribution is correct:
#   * self  — leaf frame, aggregated by resolved name (a singular leaf can't
#             double-count, so self is exact).
#   * incl  — every frame's address is resolved up front, then per sample the
#             set of *distinct names* on the stack is counted once. Deduping on
#             the resolved name (not the frame/func index) is essential: inlining
#             splits one source function across many frames, and counting those
#             separately would push a function past 100% inclusive.
python3 - "$PROFILE_JSON" "$TSZ_BIN" "$TEXT_BASE" "$TOP" "$JSON_FILE" "${TSZ_ARGS[@]}" <<'PY'
import gzip, json, sys, re, subprocess, collections
from pathlib import Path
prof = json.load(gzip.open(sys.argv[1]))
binary, base, top, json_file = sys.argv[2], int(sys.argv[3], 16), int(sys.argv[4]), sys.argv[5]
tsz_args = sys.argv[6:]

# Per-thread frame address arrays differ, so resolve names per (thread, frame).
threads = []
unique = set()
for th in prof.get("threads", []):
    fr, st = th["frameTable"], th["stackTable"]
    addr, sframe, sprefix = fr["address"], st["frame"], st["prefix"]
    threads.append((addr, sframe, sprefix, th["samples"]["stack"]))
    for a in addr:
        if a and a > 0:
            unique.add(a)

# Batch-resolve every distinct address against the binary (chunked for argv).
addrs = sorted(unique)
raw = {}
for i in range(0, len(addrs), 400):
    chunk = addrs[i:i + 400]
    out = subprocess.run(
        ["atos", "-o", binary, "-arch", "arm64", "-l", hex(base)]
        + [hex(base + a) for a in chunk],
        capture_output=True, text=True).stdout.splitlines()
    for a, line in zip(chunk, out):
        raw[a] = line

def demangle(line):
    n = line.split(" (in ")[0].strip()
    n = re.sub(r"::h[0-9a-f]{16}$", "", n)
    for a, b in (("$LT$", "<"), ("$GT$", ">"), ("$u20$", " "), ("$C$", ","),
                 ("$RF$", "&"), ("$u7b$", "{"), ("$u7d$", "}"), ("$BP$", "*")):
        n = n.replace(a, b)
    return n.replace("..", "::") if ".." in n and "::" not in n else n

name = {a: demangle(line) for a, line in raw.items()}

self_ct, incl_ct, total = collections.Counter(), collections.Counter(), 0
for addr, sframe, sprefix, stacks in threads:
    for s in stacks:
        if s is None:
            continue
        total += 1
        la = addr[sframe[s]]
        if la and la > 0 and la in name:
            self_ct[name[la]] += 1
        seen, cur = set(), s
        while cur is not None:
            fa = addr[sframe[cur]]
            nm = name.get(fa) if fa and fa > 0 else None
            if nm and nm not in seen:
                seen.add(nm)
            cur = sprefix[cur]
        for nm in seen:
            incl_ct[nm] += 1

if total == 0:
    print("No samples captured — try a larger --iterations or a longer-running target.",
          file=sys.stderr)
    sys.exit(1)

print()
print(f"=== tsz flat profile: {total} samples (self / inclusive) ===")
print(f"{'self%':>7} {'incl%':>7}  function")
rows = []
for nm, sc in self_ct.most_common(top):
    incl = incl_ct.get(nm, 0)
    self_pct = 100 * sc / total
    incl_pct = 100 * incl / total
    rows.append({
        "function": nm,
        "self_samples": sc,
        "inclusive_samples": incl,
        "self_percent": self_pct,
        "inclusive_percent": incl_pct,
    })
    print(f"{self_pct:6.1f}% {incl_pct:6.1f}%  {nm}")

if json_file:
    payload = {
        "schema_version": 1,
        "samples": total,
        "binary": binary,
        "text_base": hex(base),
        "args": tsz_args,
        "top": top,
        "rows": rows,
    }
    path = Path(json_file)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf8")
    print(f"flat profile JSON written to {json_file}")
PY
