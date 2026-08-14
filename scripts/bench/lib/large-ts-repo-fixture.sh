#!/usr/bin/env bash
#
# Shared large-ts-repo fixture helpers for benchmark and attribution tooling.

tsz_large_ts_repo_default_dir() {
    local external_bench_dir="$1"
    local local_dir="${LARGE_TS_LOCAL_DIR:-${HOME}/code/large-ts-repo}"

    if [ -n "${LARGE_TS_DIR:-}" ]; then
        printf '%s\n' "$LARGE_TS_DIR"
    elif [ "${TSZ_BENCH_ALLOW_LOCAL_FIXTURE:-0}" = "1" ] && [ -d "$local_dir/.git" ]; then
        printf '%s\n' "$local_dir"
    else
        printf '%s/large-ts-repo\n' "$external_bench_dir"
    fi
}

tsz_large_ts_repo_write_flat_tsconfig() {
    local fixture_dir="$1"
    local flat_tsconfig="$fixture_dir/tsconfig.flat.json"

    if [ -f "$fixture_dir/tsconfig.flat.bench.json" ] || [ -f "$flat_tsconfig" ]; then
        return 0
    fi

    local extends_base=""
    if [ -f "$fixture_dir/tsconfig.base.json" ]; then
        extends_base='"extends": "./tsconfig.base.json",'
    fi

    cat > "$flat_tsconfig" << FLATEOF
{
  ${extends_base}
  "compilerOptions": {
    "target": "ES2023",
    "lib": ["ES2024", "esnext.disposable"],
    "module": "NodeNext",
    "moduleResolution": "NodeNext",
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "noEmit": true,
    "forceConsistentCasingInFileNames": true,
    "resolveJsonModule": true,
    "noUnusedLocals": false,
    "noUnusedParameters": false
  },
  "include": ["packages/**/src/**/*.ts"]
}
FLATEOF
}

tsz_large_ts_repo_select_tsconfig() {
    local fixture_dir="$1"

    if [ -f "$fixture_dir/tsconfig.flat.bench.json" ]; then
        printf '%s\n' "$fixture_dir/tsconfig.flat.bench.json"
    elif [ -f "$fixture_dir/tsconfig.flat.json" ]; then
        printf '%s\n' "$fixture_dir/tsconfig.flat.json"
    else
        return 1
    fi
}

tsz_ensure_large_ts_repo_fixture() {
    local fixture_dir="$1"
    local repo="$2"
    local ref="$3"

    mkdir -p "$(dirname "$fixture_dir")"

    if [ ! -d "$fixture_dir/.git" ]; then
        echo "Cloning large-ts-repo fixture..."
        if ! git clone --quiet --no-tags --depth 1 "$repo" "$fixture_dir"; then
            echo "ERROR: failed to clone large-ts-repo fixture from ${repo}" >&2
            return 1
        fi
    fi

    # Never let a directory that is not its own git checkout be treated as a
    # pinned fixture (see tsz_git_fixture_is_standalone_repo in
    # project-fixtures.sh — same #17469 aliasing hazard applies here).
    if ! tsz_git_fixture_is_standalone_repo "$fixture_dir"; then
        echo "ERROR: large-ts-repo fixture at ${fixture_dir} is not a standalone git checkout" >&2
        return 1
    fi

    if [ -n "$ref" ]; then
        local current_ref
        current_ref="$(git -C "$fixture_dir" rev-parse HEAD 2>/dev/null || echo "")"
        if [ "$current_ref" != "$ref" ]; then
            echo "Pinning large-ts-repo to ${ref:0:12}..."
            if ! git -C "$fixture_dir" fetch --quiet --depth 1 origin "$ref"; then
                echo "ERROR: failed to fetch large-ts-repo pin ${ref:0:12} from ${repo}" \
                    "— the upstream may have rewritten history; re-pin the fixture to a served commit" >&2
                return 1
            fi
            if ! git -C "$fixture_dir" checkout --quiet --detach FETCH_HEAD; then
                echo "ERROR: failed to check out fetched large-ts-repo pin ${ref:0:12}" >&2
                return 1
            fi
        fi

        if [[ "$ref" =~ ^[0-9a-f]{40}$ ]]; then
            current_ref="$(git -C "$fixture_dir" rev-parse HEAD 2>/dev/null || echo "")"
            if [ "$current_ref" != "$ref" ]; then
                echo "ERROR: large-ts-repo fixture HEAD is ${current_ref:0:12}, expected pin ${ref:0:12}" >&2
                return 1
            fi
        fi
    fi

    if ! command -v pnpm >/dev/null 2>&1; then
        echo "error: pnpm not found. Install pnpm to prepare large-ts-repo dependencies." >&2
        return 1
    fi

    local deps_stamp="$fixture_dir/.deps-installed"
    if [ ! -f "$deps_stamp" ] \
        || [ "$fixture_dir/pnpm-lock.yaml" -nt "$deps_stamp" ] \
        || [ "$fixture_dir/package.json" -nt "$deps_stamp" ] \
        || [ "$fixture_dir/pnpm-workspace.yaml" -nt "$deps_stamp" ]; then
        echo "Installing large-ts-repo dependencies..."
        pnpm --dir "$fixture_dir" install --frozen-lockfile --silent
        touch "$deps_stamp"
    fi

    tsz_large_ts_repo_write_flat_tsconfig "$fixture_dir"
}
