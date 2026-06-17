#!/usr/bin/env bash
# =============================================================================
# generate-skewed-fixture.sh — synthesize a check-COST-skewed monorepo
# =============================================================================
#
# The scale-cliff fixtures use uniform-size leaf files, so every file has
# ~equal check cost and static round-robin partitioning is already balanced.
# This fixture deliberately introduces a power-law spread of per-file *check*
# cost — a few very expensive files among many cheap ones, scattered (not
# clustered, not pool-aligned) — to exercise the checker pool scheduler's
# straggler behaviour. Cost-blind round-robin partitioning imbalances the bins
# by chance; LPT cost-balancing minimises the makespan. The win is robust to
# file ordering because the sizes vary continuously, not in two clean buckets.
#
# Check cost (not just parse/bind) is driven by DISTINCT structural
# assignability work: each "unit" emits a distinct N-property interface, a
# matching object literal, and a generic-constrained function call. Distinct
# types defeat the solver's memo caches, so cost tracks unit count linearly.
#
# Output: scripts/bench/scale-cliff/fixtures/monorepo-skew/
# Usage:  scripts/bench/scale-cliff/generate-skewed-fixture.sh [--clean]
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DIR="$SCRIPT_DIR/fixtures/monorepo-skew"

if [[ "${1:-}" == "--clean" || -d "$DIR" ]]; then
    rm -rf "$DIR"
fi
mkdir -p "$DIR/packages/p0/src"

cat >"$DIR/packages/p0/package.json" <<'JSON'
{ "name": "@skew/p0", "version": "0.0.0", "main": "src/index.ts", "type": "module" }
JSON
cat >"$DIR/tsconfig.json" <<'JSON'
{
    "compilerOptions": {
        "target": "ES2022",
        "module": "NodeNext",
        "moduleResolution": "NodeNext",
        "lib": ["ES2023", "ESNext"],
        "strict": true,
        "esModuleInterop": true,
        "skipLibCheck": true,
        "noEmit": true,
        "forceConsistentCasingInFileNames": true
    },
    "include": ["packages/**/src/**/*.ts"]
}
JSON

# Python drives the per-file size distribution deterministically (LCG), so the
# fixture is reproducible without engineering pathological positions.
FILES="${FILES:-360}" PROPS="${PROPS:-50}" SEED="${SEED:-1234567}" \
HEAVY_FRAC="${HEAVY_FRAC:-0.06}" HEAVY_MIN="${HEAVY_MIN:-90}" HEAVY_MAX="${HEAVY_MAX:-150}" \
SMALL_UNITS="${SMALL_UNITS:-1}" python3 - "$DIR/packages/p0/src" <<'PY'
import os, sys
srcdir = sys.argv[1]
FILES = int(os.environ["FILES"]); PROPS = int(os.environ["PROPS"])
SEED = int(os.environ["SEED"])
HEAVY_FRAC = float(os.environ["HEAVY_FRAC"])
HEAVY_MIN = int(os.environ["HEAVY_MIN"]); HEAVY_MAX = int(os.environ["HEAVY_MAX"])
SMALL_UNITS = int(os.environ["SMALL_UNITS"])

# Deterministic LCG (no external randomness; reproducible across machines).
state = SEED
def rnd():
    global state
    state = (state * 1103515245 + 12345) & 0x7fffffff
    return state / 0x7fffffff

def unit(k, props):
    # A distinct interface + literal + generic-constrained call. Distinct
    # property TYPES per unit defeat structural-relation memoisation.
    ip = " ".join(f"p{j}: {'number' if (j+k)%2==0 else 'string'};" for j in range(props))
    vp = " ".join(f"p{j}: {(j+k) if (j+k)%2==0 else repr(str(j+k))}," for j in range(props))
    return (f"interface I{k} {{ {ip} }}\n"
            f"const i{k}: I{k} = {{ {vp} }};\n"
            f"function f{k}<T extends I{k}>(x: T): T {{ return x; }}\n"
            f"const r{k} = f{k}(i{k});\n")

heavy = 0; total_units = 0
for fi in range(FILES):
    # Most files cheap (SMALL_UNITS); a scattered HEAVY_FRAC are expensive with
    # a CONTINUOUS magnitude in [HEAVY_MIN, HEAVY_MAX] — a power-law-ish tail.
    if rnd() < HEAVY_FRAC:
        units = HEAVY_MIN + int(rnd() * (HEAVY_MAX - HEAVY_MIN))
        heavy += 1
    else:
        units = SMALL_UNITS
    total_units += units
    body = "".join(unit(fi * 1000 + u, PROPS) for u in range(units))
    with open(os.path.join(srcdir, f"m{fi:04d}.ts"), "w") as fh:
        fh.write(f"// file {fi}, units={units}\n{body}")

with open(os.path.join(srcdir, "index.ts"), "w") as fh:
    fh.write("// barrel\n")
print(f"files={FILES} heavy={heavy} props={PROPS} total_units={total_units}")
PY

total=$(find "$DIR" -name '*.ts' | wc -l | tr -d ' ')
echo "built monorepo-skew: ${total} files"
echo "fixture: $DIR/tsconfig.json"
