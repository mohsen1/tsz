#!/usr/bin/env bash
# =============================================================================
# generate-fixtures.sh — synthesize the monorepo-001..006 scale-cliff fixtures
# =============================================================================
#
# Per docs/plan/PERF_ARCHITECTURAL_PLAN.md "Step 0", these fixtures interpolate
# from "tsz wins" (small single-package) to "tsz explodes" (monorepo with all
# the multipliers active). The point is to find the EXACT scale at which a
# tsz/tsgo per-file ratio breaks linearity.
#
# Each fixture is a self-contained directory with a tsconfig and a synthetic
# package layout. The shape is real enough to exercise the code paths we care
# about (project-relative imports, lib refs, mapped types, barrels) without
# depending on a 6086-file external repo.
#
# Output: scripts/bench/scale-cliff/fixtures/monorepo-NNN/
#
# Usage:
#   scripts/bench/scale-cliff/generate-fixtures.sh         # default sizes
#   scripts/bench/scale-cliff/generate-fixtures.sh --clean # remove + regenerate
#
# Then run the bench driver:
#   scripts/bench/scale-cliff/run-cliff.sh
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FIXTURES_DIR="$SCRIPT_DIR/fixtures"

CLEAN=false
if [[ "${1:-}" == "--clean" ]]; then CLEAN=true; fi

if [[ "$CLEAN" == true && -d "$FIXTURES_DIR" ]]; then
    echo "Removing existing fixtures at $FIXTURES_DIR"
    rm -rf "$FIXTURES_DIR"
fi

mkdir -p "$FIXTURES_DIR"

# -----------------------------------------------------------------------------
# Helpers
# -----------------------------------------------------------------------------

write_tsconfig() {
    local dir="$1"
    cat >"$dir/tsconfig.json" <<'JSON'
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

# Write a "leaf" file with N exports (interfaces/types/values).
write_leaf_file() {
    local file="$1"
    local n="$2"
    {
        echo "// auto-generated leaf module"
        for i in $(seq 1 "$n"); do
            echo "export interface Leaf${i} { value: number; tag: \"leaf-${i}\"; }"
            echo "export const leaf${i}: Leaf${i} = { value: ${i}, tag: \"leaf-${i}\" };"
        done
    } >"$file"
}

# Write an "importer" file that imports K exports from a leaf and re-exports.
write_importer_file() {
    local file="$1"
    local leaf_path="$2"
    local k="$3"
    {
        echo "// auto-generated importer"
        local imports=""
        for i in $(seq 1 "$k"); do
            imports+="leaf${i}, Leaf${i}"
            if (( i < k )); then imports+=", "; fi
        done
        echo "import { ${imports} } from \"${leaf_path}\";"
        echo "export { ${imports} };"
        echo "export const sum = "
        for i in $(seq 1 "$k"); do
            printf "  leaf%d.value" "$i"
            if (( i < k )); then echo " +"; else echo ";"; fi
        done
    } >"$file"
}

# Write a barrel file that re-exports from many imports.
write_barrel_file() {
    local file="$1"
    local sources=("${@:2}")
    {
        echo "// auto-generated barrel"
        for src in "${sources[@]}"; do
            echo "export * from \"${src}\";"
        done
    } >"$file"
}

# Write a file that uses mapped/conditional types crossing package boundaries.
write_xpkg_mapped_file() {
    local file="$1"
    local target_pkg_path="$2"
    {
        cat <<TS
// auto-generated cross-package mapped/conditional consumer
import type * as Target from "${target_pkg_path}";

type AllValues<T> = { [K in keyof T]: T[K] extends { value: infer V } ? V : never };
type LeafKeys<T> = { [K in keyof T]: T[K] extends { tag: \`leaf-\${number}\` } ? K : never }[keyof T];
type Distill<T> = T extends { tag: infer Tag } ? Tag : never;

export type AllTargetValues = AllValues<typeof Target>;
export type AllTargetLeafKeys = LeafKeys<typeof Target>;
export type AllTargetTags = Distill<Target[keyof typeof Target]>;
TS
    } >"$file"
}

# Write a package.json with a name. Exports field omitted to keep resolver work
# concentrated in node-style path lookups (matches the typical large-monorepo
# shape).
write_package_json() {
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

# -----------------------------------------------------------------------------
# Fixture builders
# -----------------------------------------------------------------------------

# monorepo-001: 1 package, 100 files, no cross-package work.
build_001() {
    local dir="$FIXTURES_DIR/monorepo-001"
    rm -rf "$dir"
    mkdir -p "$dir/packages/p0/src"
    write_package_json "$dir/packages/p0" "@cliff/p0"
    for i in $(seq 1 100); do
        write_leaf_file "$dir/packages/p0/src/leaf${i}.ts" 5
    done
    {
        echo "// barrel"
        for i in $(seq 1 100); do
            echo "export * from \"./leaf${i}\";"
        done
    } >"$dir/packages/p0/src/index.ts"
    write_tsconfig "$dir"
    echo "  built monorepo-001 (1 pkg, 100 files)"
}

# monorepo-002: 10 packages, 100 files each (1000 files), no cross-package work.
build_002() {
    local dir="$FIXTURES_DIR/monorepo-002"
    rm -rf "$dir"
    for p in $(seq 0 9); do
        mkdir -p "$dir/packages/p${p}/src"
        write_package_json "$dir/packages/p${p}" "@cliff/p${p}"
        for i in $(seq 1 100); do
            write_leaf_file "$dir/packages/p${p}/src/leaf${i}.ts" 5
        done
        {
            echo "// barrel"
            for i in $(seq 1 100); do
                echo "export * from \"./leaf${i}\";"
            done
        } >"$dir/packages/p${p}/src/index.ts"
    done
    write_tsconfig "$dir"
    echo "  built monorepo-002 (10 pkgs, 100 files each = 1000 files)"
}

# monorepo-003: 50 packages, project-relative imports between adjacent pkgs.
build_003() {
    local dir="$FIXTURES_DIR/monorepo-003"
    rm -rf "$dir"
    for p in $(seq 0 49); do
        mkdir -p "$dir/packages/p${p}/src"
        write_package_json "$dir/packages/p${p}" "@cliff/p${p}"
        for i in $(seq 1 100); do
            write_leaf_file "$dir/packages/p${p}/src/leaf${i}.ts" 5
        done
        {
            echo "// barrel"
            for i in $(seq 1 100); do
                echo "export * from \"./leaf${i}\";"
            done
        } >"$dir/packages/p${p}/src/index.ts"
        # Cross-package importer (project-relative path)
        if (( p > 0 )); then
            local prev=$((p - 1))
            cat >"$dir/packages/p${p}/src/uses_prev.ts" <<TS
// uses prior package via project-relative path
import * as Prev from "../../p${prev}/src/index";
export const sumPrev = Prev.leaf1.value + Prev.leaf2.value;
TS
        fi
    done
    write_tsconfig "$dir"
    echo "  built monorepo-003 (50 pkgs, project-relative imports = ~5000 files)"
}

# monorepo-004: monorepo-003 + a shared lib/global declarations file.
build_004() {
    local dir="$FIXTURES_DIR/monorepo-004"
    rm -rf "$dir"
    cp -R "$FIXTURES_DIR/monorepo-003/" "$dir/"
    mkdir -p "$dir/packages/shared/src"
    write_package_json "$dir/packages/shared" "@cliff/shared"
    cat >"$dir/packages/shared/src/globals.ts" <<'TS'
// Shared lib/global declarations
declare global {
    interface CliffSharedContext {
        version: number;
        readonly tag: "cliff-shared";
    }
    var __cliffShared: CliffSharedContext;
}
export {};
TS
    cat >"$dir/packages/shared/src/index.ts" <<'TS'
export * from "./globals";
TS
    # Have every package import the shared globals to force cross-file delegation.
    for p in $(seq 0 49); do
        cat >"$dir/packages/p${p}/src/uses_shared.ts" <<TS
import "../../shared/src/index";
export const tag = __cliffShared.tag;
TS
    done
    echo "  built monorepo-004 (monorepo-003 + shared globals)"
}

# monorepo-005: monorepo-004 + one heavy barrel that re-exports everything.
build_005() {
    local dir="$FIXTURES_DIR/monorepo-005"
    rm -rf "$dir"
    cp -R "$FIXTURES_DIR/monorepo-004/" "$dir/"
    {
        echo "// HEAVY BARREL: re-exports every package's index"
        for p in $(seq 0 49); do
            echo "export * from \"./p${p}/src/index\";"
        done
    } >"$dir/packages/heavy_barrel.ts"
    # Make one file per package import the heavy barrel.
    for p in $(seq 0 49); do
        cat >"$dir/packages/p${p}/src/uses_barrel.ts" <<TS
import * as Barrel from "../../heavy_barrel";
export const fromBarrel = Barrel;
TS
    done
    echo "  built monorepo-005 (monorepo-004 + heavy barrel)"
}

# monorepo-006: monorepo-005 + mapped/conditional types crossing package boundaries.
build_006() {
    local dir="$FIXTURES_DIR/monorepo-006"
    rm -rf "$dir"
    cp -R "$FIXTURES_DIR/monorepo-005/" "$dir/"
    # In each package, add a file that does mapped/conditional work over the
    # NEXT package's exports. That's the "type queries crossing boundaries"
    # case the expert called out.
    for p in $(seq 0 48); do
        local nxt=$((p + 1))
        write_xpkg_mapped_file \
            "$dir/packages/p${p}/src/xpkg_mapped.ts" \
            "../../p${nxt}/src/index"
    done
    echo "  built monorepo-006 (monorepo-005 + mapped types across pkg boundaries)"
}

# -----------------------------------------------------------------------------
# Driver
# -----------------------------------------------------------------------------

build_001
build_002
build_003
build_004
build_005
build_006

echo
echo "Done. Fixtures at $FIXTURES_DIR"
echo "Run scripts/bench/scale-cliff/run-cliff.sh to bench them."
