#!/usr/bin/env bash
# =============================================================================
# generate-fixtures-1m.sh — synthesize a 1M-LOC stress fixture (monorepo-007)
# =============================================================================
#
# Companion to generate-fixtures.sh. Where monorepo-001..006 cap at ~5k files /
# ~60k LOC, monorepo-007 targets the "mega-fast LSP on 1M LOC without project
# references" stress test:
#
#   100 packages × 100 files × ~100 LOC = ~1,000,000 LOC across ~10,000 files
#
# Files are realistic-shaped: 5 exports each, contextual call sites, modest
# generic usage, cross-package imports forming a directed acyclic graph that
# exercises module resolution, symbol delegation, and binder topology without
# any project-reference scaffolding.
#
# A 1M LOC test takes 10-20 seconds to generate. Disk: ~30 MB.
#
# Usage:
#   scripts/bench/scale-cliff/generate-fixtures-1m.sh         # default sizes
#   scripts/bench/scale-cliff/generate-fixtures-1m.sh --clean # remove + regenerate
#   PKG_COUNT=50 FILES_PER_PKG=50 scripts/bench/scale-cliff/generate-fixtures-1m.sh
#
# Output: scripts/bench/scale-cliff/fixtures/monorepo-007/
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FIXTURES_DIR="$SCRIPT_DIR/fixtures"
FIXTURE_NAME="${FIXTURE_NAME:-monorepo-007}"
TARGET_DIR="$FIXTURES_DIR/$FIXTURE_NAME"

PKG_COUNT="${PKG_COUNT:-100}"
FILES_PER_PKG="${FILES_PER_PKG:-100}"
EXPORTS_PER_FILE="${EXPORTS_PER_FILE:-5}"
XPKG_IMPORTS_PER_PKG="${XPKG_IMPORTS_PER_PKG:-3}"
HEAVY_BARREL="${HEAVY_BARREL:-1}"
XPKG_MAPPED="${XPKG_MAPPED:-1}"

CLEAN=false
if [[ "${1:-}" == "--clean" ]]; then CLEAN=true; fi

if [[ "$CLEAN" == true && -d "$TARGET_DIR" ]]; then
    echo "Removing existing fixture at $TARGET_DIR"
    rm -rf "$TARGET_DIR"
fi

mkdir -p "$TARGET_DIR"

write_tsconfig() {
    cat >"$TARGET_DIR/tsconfig.json" <<'JSON'
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
        "forceConsistentCasingInFileNames": true,
        "resolveJsonModule": true
    },
    "include": ["packages/**/src/**/*.ts"]
}
JSON
}

# Real-shaped file ≈ 100 LOC: 5 interface exports, 5 const exports, 5 generic
# functions, 5 call sites consuming them. No imports inside the leaf so every
# leaf is pure type-check work in isolation.
write_realistic_leaf() {
    local file="$1"
    local pkg_idx="$2"
    local file_idx="$3"
    local n="$EXPORTS_PER_FILE"
    {
        echo "// auto-generated realistic leaf — pkg $pkg_idx file $file_idx"
        echo "// shape: interfaces + values + generics + call sites"
        echo ""
        for i in $(seq 1 "$n"); do
            cat <<TS
export interface Leaf_${pkg_idx}_${file_idx}_${i}<T = unknown> {
    readonly value: T;
    readonly tag: "leaf-${pkg_idx}-${file_idx}-${i}";
    readonly nested: { depth: number; name: string };
    transform<U>(fn: (input: T) => U): Leaf_${pkg_idx}_${file_idx}_${i}<U>;
}

export const leaf_${pkg_idx}_${file_idx}_${i}: Leaf_${pkg_idx}_${file_idx}_${i}<number> = {
    value: ${i},
    tag: "leaf-${pkg_idx}-${file_idx}-${i}",
    nested: { depth: ${i}, name: "n${pkg_idx}_${file_idx}_${i}" },
    transform<U>(fn: (input: number) => U): Leaf_${pkg_idx}_${file_idx}_${i}<U> {
        const next = fn(this.value);
        return {
            value: next,
            tag: "leaf-${pkg_idx}-${file_idx}-${i}",
            nested: this.nested,
            transform: this.transform as unknown as Leaf_${pkg_idx}_${file_idx}_${i}<U>["transform"],
        };
    },
};

export function compute_${pkg_idx}_${file_idx}_${i}<T extends Leaf_${pkg_idx}_${file_idx}_${i}>(input: T): T["value"] {
    return input.value;
}

TS
        done
        # Call sites consuming local exports — exercises within-file inference.
        echo "// Call sites — local inference workload"
        for i in $(seq 1 "$n"); do
            echo "const _used_${i} = compute_${pkg_idx}_${file_idx}_${i}(leaf_${pkg_idx}_${file_idx}_${i});"
        done
        echo "export const _file_sum_${pkg_idx}_${file_idx} = ["
        for i in $(seq 1 "$n"); do
            echo "    _used_${i},"
        done
        echo "].reduce((a, b) => a + b, 0);"
    } >"$file"
}

# Barrel re-exports every file in a package.
write_pkg_barrel() {
    local file="$1"
    local pkg_idx="$2"
    {
        echo "// auto-generated package barrel — pkg $pkg_idx"
        for f in $(seq 1 "$FILES_PER_PKG"); do
            echo "export * from \"./leaf${f}\";"
        done
    } >"$file"
}

# Cross-package importer: imports from N adjacent packages, computes a sum.
write_xpkg_importer() {
    local file="$1"
    local pkg_idx="$2"
    local n="$XPKG_IMPORTS_PER_PKG"
    {
        echo "// auto-generated cross-package importer — pkg $pkg_idx"
        local k=0
        local target_pkg
        for k in $(seq 1 "$n"); do
            target_pkg=$(( (pkg_idx + k) % PKG_COUNT ))
            echo "import { _file_sum_${target_pkg}_1 as _import_${k} } from \"../../p${target_pkg}/src/leaf1\";"
        done
        echo ""
        echo "export const xpkg_sum_${pkg_idx} = ["
        for k in $(seq 1 "$n"); do
            echo "    _import_${k},"
        done
        echo "].reduce((a, b) => a + b, 0);"
    } >"$file"
}

# Cross-package mapped/conditional types — stresses cross-file type evaluation.
write_xpkg_mapped() {
    local file="$1"
    local pkg_idx="$2"
    local nxt=$(( (pkg_idx + 1) % PKG_COUNT ))
    {
        cat <<TS
// auto-generated cross-package mapped/conditional consumer — pkg ${pkg_idx}
import type * as Target from "../../p${nxt}/src/index";

type AllValues<T> = { [K in keyof T]: T[K] extends { value: infer V } ? V : never };
type LeafKeys<T> = { [K in keyof T]: T[K] extends { tag: \`leaf-\${number}-\${number}-\${number}\` } ? K : never }[keyof T];
type Distill<T> = T extends { tag: infer Tag } ? Tag : never;
type DeepRead<T, D extends number = 0> = D extends 4 ? T : { [K in keyof T]: DeepRead<T[K], 1> };

export type XPkgValues_${pkg_idx} = AllValues<typeof Target>;
export type XPkgLeafKeys_${pkg_idx} = LeafKeys<typeof Target>;
export type XPkgTags_${pkg_idx} = Distill<Target[keyof typeof Target]>;
export type XPkgDeepRead_${pkg_idx} = DeepRead<typeof Target>;
TS
    } >"$file"
}

write_pkg_json() {
    local dir="$1"
    local name="$2"
    cat >"$dir/package.json" <<JSON
{
    "name": "${name}",
    "version": "0.0.0",
    "main": "src/index.ts",
    "type": "module"
}
JSON
}

build() {
    echo "Generating $FIXTURE_NAME with PKG_COUNT=$PKG_COUNT, FILES_PER_PKG=$FILES_PER_PKG, EXPORTS_PER_FILE=$EXPORTS_PER_FILE"
    write_tsconfig

    local total_files=0
    for p in $(seq 0 $((PKG_COUNT - 1))); do
        local pkg_dir="$TARGET_DIR/packages/p${p}/src"
        mkdir -p "$pkg_dir"
        write_pkg_json "$TARGET_DIR/packages/p${p}" "@cliff-1m/p${p}"

        for f in $(seq 1 "$FILES_PER_PKG"); do
            write_realistic_leaf "$pkg_dir/leaf${f}.ts" "$p" "$f"
            total_files=$((total_files + 1))
        done

        write_pkg_barrel "$pkg_dir/index.ts" "$p"
        total_files=$((total_files + 1))

        # Cross-package importer (skip p0 — needs no prior pkg).
        if (( p > 0 )); then
            write_xpkg_importer "$pkg_dir/xpkg_uses.ts" "$p"
            total_files=$((total_files + 1))
        fi

        if [[ "$XPKG_MAPPED" == "1" ]] && (( p < PKG_COUNT - 1 )); then
            write_xpkg_mapped "$pkg_dir/xpkg_mapped.ts" "$p"
            total_files=$((total_files + 1))
        fi

        if (( p % 10 == 0 )); then
            echo "  pkg $p / $PKG_COUNT done"
        fi
    done

    # Single heavy barrel re-exporting every package's barrel.
    if [[ "$HEAVY_BARREL" == "1" ]]; then
        mkdir -p "$TARGET_DIR/packages"
        {
            echo "// HEAVY BARREL: re-exports every package's index"
            for p in $(seq 0 $((PKG_COUNT - 1))); do
                echo "export * from \"./p${p}/src/index\";"
            done
        } >"$TARGET_DIR/packages/heavy_barrel.ts"
        total_files=$((total_files + 1))
    fi

    echo
    echo "Built $FIXTURE_NAME: $total_files files"
    local loc
    loc=$(find "$TARGET_DIR/packages" -name '*.ts' -exec cat {} + 2>/dev/null | wc -l | tr -d ' ')
    echo "Total LOC: $loc"
}

build
