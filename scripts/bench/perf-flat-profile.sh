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
#   scripts/bench/perf-flat-profile.sh --no-build -p tsconfig.json   # reuse symbol build
#
# Options:
#   -p <tsconfig>      Type-check a project (passed to tsz as `-p`).
#   --iterations N     Loop the target N times under the sampler (default 8) so
#                      short runs accumulate enough samples.
#   --top N            Rows to print (default 25).
#   --no-build         Skip the symbol-retaining build; reuse the existing binary.
#   --bin <path>       Use this tsz binary instead of building.
#   --help
#
# macOS only for symbol resolution (uses `atos`). On Linux, swap the resolver
# for `llvm-symbolizer`/`addr2line` over the same lib-relative addresses.
#
# NOTE: builds a *separate* symbol-retaining dist-fast binary; it does not touch
# the PGO bench binary used by bench-vs-tsgo.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$ROOT_DIR"

ITERATIONS=8
TOP=25
NO_BUILD=false
BIN_OVERRIDE=""
declare -a TSZ_ARGS=()

usage() { sed -n '2,40p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; }

while [[ $# -gt 0 ]]; do
    case "$1" in
        -p) TSZ_ARGS+=(-p "$2"); shift 2 ;;
        --iterations) ITERATIONS="$2"; shift 2 ;;
        --top) TOP="$2"; shift 2 ;;
        --no-build) NO_BUILD=true; shift ;;
        --bin) BIN_OVERRIDE="$2"; shift 2 ;;
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

# Parse the Firefox-profiler JSON: self-time by leaf frame address (reliable),
# inclusive by function (representative address per func). Emit one line per hot
# tsz-lib address: "<self> <incl> 0x<lib_relative_addr>".
ADDR_TABLE="$(python3 - "$PROFILE_JSON" "$TOP" <<'PY'
import gzip, json, sys, collections
prof = json.load(gzip.open(sys.argv[1]))
top = int(sys.argv[2])
libs = prof.get("libs", [])
tsz_lib = next((i for i, l in enumerate(libs) if l.get("name") == "tsz"), None)
self_ct = collections.Counter()
incl_ct = collections.Counter()
func_addr = {}
total = 0
for th in prof.get("threads", []):
    fr, st, ft = th["frameTable"], th["stackTable"], th["funcTable"]
    addr, ffunc, sframe, sprefix = fr["address"], fr["func"], st["frame"], st["prefix"]
    fres = ft.get("resource")
    rt = th.get("resourceTable", {}) or {}
    rlib = rt.get("lib")
    def in_tsz(func):
        if tsz_lib is None or fres is None or rlib is None:
            return True
        if func >= len(fres) or fres[func] is None or fres[func] >= len(rlib):
            return True
        return rlib[fres[func]] == tsz_lib
    for i in range(fr["length"]):
        a = addr[i]
        if a and a > 0:
            func_addr.setdefault(ffunc[i], a)
    for s in th["samples"]["stack"]:
        if s is None:
            continue
        total += 1
        leaf_fr = sframe[s]
        a = addr[leaf_fr]
        if a and a > 0 and in_tsz(ffunc[leaf_fr]):
            self_ct[a] += 1
        seen = set(); cur = s
        while cur is not None:
            fu = ffunc[sframe[cur]]
            if fu not in seen:
                seen.add(fu)
                if in_tsz(fu) and fu in func_addr:
                    incl_ct[func_addr[fu]] += 1
            cur = sprefix[cur]
print(f"#TOTAL {total}")
addrs = [a for a, _ in self_ct.most_common(top)]
for a in incl_ct:
    if a not in addrs:
        addrs.append(a)
for a in sorted(addrs, key=lambda x: self_ct.get(x, 0), reverse=True)[:top]:
    print(f"{self_ct.get(a,0)} {incl_ct.get(a,0)} 0x{a:x}")
PY
)"

TOTAL="$(echo "$ADDR_TABLE" | awk '/^#TOTAL/{print $2}')"
if [[ -z "$TOTAL" || "$TOTAL" -eq 0 ]]; then
    echo "No samples captured — try a larger --iterations or a longer-running target." >&2
    exit 1
fi

# Batch-resolve all addresses with a single atos call.
mapfile -t HEX < <(echo "$ADDR_TABLE" | awk '!/^#/{print $3}')
declare -a VMADDRS=()
for h in "${HEX[@]}"; do
    VMADDRS+=("$(python3 -c "print(hex($TEXT_BASE + $h))")")
done
mapfile -t SYMS < <(atos -o "$TSZ_BIN" -arch arm64 -l "$TEXT_BASE" "${VMADDRS[@]}" 2>/dev/null \
    | sed 's/::h[0-9a-f]\{16\}//g; s/\$LT\$/</g; s/\$GT\$/>/g; s/\$u20\$/ /g; s/\$C\$/,/g; s/\$RF\$/\&/g; s/ (in tsz).*//')

echo
printf '%s\n' "=== tsz flat profile: ${TOTAL} samples (self / inclusive) ==="
printf '%7s %7s  %s\n' "self%" "incl%" "function"
i=0
while read -r self incl _; do
    [[ "$self" == \#* ]] && continue
    sym="${SYMS[$i]:-?}"; i=$((i+1))
    sp="$(python3 -c "print(f'{100*$self/$TOTAL:.1f}')")"
    ip="$(python3 -c "print(f'{100*$incl/$TOTAL:.1f}')")"
    printf '%6s%% %6s%%  %s\n' "$sp" "$ip" "$sym"
done <<< "$ADDR_TABLE"
