#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

DEFAULT_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT_DIR/.target}"
TSZ_BIN="${TSZ_BIN:-$DEFAULT_TARGET_DIR/dist-fast/tsz}"
FIXTURE_ROOT="${TSZ_PROJECT_COMPILE_FIXTURE_ROOT:-$ROOT_DIR/.target/project-compile-guard}"
PROJECT_TIMEOUT="${TSZ_PROJECT_COMPILE_TIMEOUT:-90}"
INCLUDE_GENERATED_APPS="${TSZ_PROJECT_COMPILE_INCLUDE_GENERATED_APPS:-1}"
PROJECT_FILTER="${TSZ_PROJECT_COMPILE_FILTER:-}"
PROJECT_SET="${TSZ_PROJECT_COMPILE_SET:-required}"
ALLOW_FAILURES="${TSZ_PROJECT_COMPILE_ALLOW_FAILURES:-0}"
FAILURES=0

# shellcheck source=scripts/bench/project-fixtures.sh
source "$ROOT_DIR/scripts/bench/project-fixtures.sh"

if [[ ! -x "$TSZ_BIN" ]]; then
  echo "error: TSZ_BIN is not executable: $TSZ_BIN" >&2
  exit 1
fi

mkdir -p "$FIXTURE_ROOT"

run_with_timeout() {
  local timeout_secs="$1"
  shift

  "$@" &
  local pid=$!
  perl -e 'sleep shift; kill 9, shift' "$timeout_secs" "$pid" &
  local watchdog_pid=$!

  local exit_code=0
  wait "$pid" 2>/dev/null || exit_code=$?

  kill "$watchdog_pid" 2>/dev/null || true
  wait "$watchdog_pid" 2>/dev/null || true

  if [[ "$exit_code" -eq 137 ]]; then
    return 124
  fi
  return "$exit_code"
}

ensure_git_fixture() {
  tsz_ensure_git_fixture "$@" 0
}

write_utility_types_config() {
  tsz_write_utility_types_config "$FIXTURE_ROOT/utility-types/tsconfig.tsz-guard.json"
}

write_ts_toolbelt_config() {
  tsz_write_ts_toolbelt_config "$FIXTURE_ROOT/ts-toolbelt/tsconfig.tsz-guard.json"
}

write_ts_essentials_config() {
  tsz_write_ts_essentials_config "$FIXTURE_ROOT/ts-essentials/tsconfig.tsz-guard.json"
}

write_rxjs_config() {
  tsz_write_rxjs_config \
    "$FIXTURE_ROOT/rxjs/tsconfig.tsz-guard.json" \
    "$(tsz_rxjs_src_root "$FIXTURE_ROOT/rxjs")"
}

write_type_fest_config() {
  tsz_write_type_fest_config "$FIXTURE_ROOT/type-fest/tsconfig.tsz-guard.json"
}

write_zod_config() {
  tsz_write_zod_config "$FIXTURE_ROOT/zod/tsconfig.tsz-guard.json"
}

write_kysely_config() {
  tsz_write_kysely_globals "$FIXTURE_ROOT/kysely/tsz-bench-globals.d.ts"
  tsz_write_kysely_config "$FIXTURE_ROOT/kysely/tsconfig.tsz-guard.json"
}

write_type_challenges_config() {
  local source_dir="$FIXTURE_ROOT/type-challenges"
  local compile_dir="$source_dir/.tsz-compile"

  rm -rf "$compile_dir"
  mkdir -p "$compile_dir/questions" "$compile_dir/utils"

  local template
  while IFS= read -r template; do
    local rel="${template#"$source_dir"/}"
    mkdir -p "$compile_dir/$(dirname "$rel")"
    cp "$template" "$compile_dir/$rel"
    printf '\nexport {};\n' >> "$compile_dir/$rel"
  done < <(find "$source_dir/questions" -maxdepth 2 -name template.ts | sort)

  cp "$source_dir/utils/index.d.ts" "$compile_dir/utils/index.d.ts"
  cat > "$compile_dir/tsconfig.tsz-guard.json" <<'JSON'
{
  "compilerOptions": {
    "target": "es2017",
    "lib": ["ESNext"],
    "module": "commonjs",
    "moduleResolution": "node",
    "strict": true,
    "noEmit": true,
    "types": [],
    "noImplicitReturns": true,
    "noUnusedLocals": false,
    "noUnusedParameters": false,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "ignoreDeprecations": "6.0",
    "baseUrl": ".",
    "paths": {
      "@type-challenges/utils": ["utils/index.d.ts"]
    }
  },
  "include": ["questions/**/template.ts", "utils/index.d.ts"]
}
JSON
}

write_type_challenges_solutions_config() {
  tsz_write_type_challenges_solutions_config \
    "$FIXTURE_ROOT/type-challenges-solutions" \
    "$FIXTURE_ROOT/type-challenges-solutions/.tsz-compile"
}

check_project() {
  local name="$1"
  local tsconfig="$2"
  local log="$FIXTURE_ROOT/${name}.log"

  echo "::group::${name}"
  echo "Running: $TSZ_BIN --noEmit -p $tsconfig"
  local rc=0
  run_with_timeout "$PROJECT_TIMEOUT" \
    env \
      TSZ_USE_EMBEDDED_LIBS=1 \
      RUST_MIN_STACK="${TSZ_RUST_MIN_STACK:-536870912}" \
      "$TSZ_BIN" --noEmit -p "$tsconfig" >"$log" 2>&1 || rc=$?

  if [[ "$rc" -ne 0 ]]; then
    FAILURES=$((FAILURES + 1))
    if [[ "$rc" -eq 124 ]]; then
      echo "error: ${name} timed out after ${PROJECT_TIMEOUT}s" >&2
    else
      echo "error: ${name} failed with exit code ${rc}" >&2
    fi
    sed -n '1,160p' "$log" >&2 || true
    echo "::endgroup::"
    if [[ "$ALLOW_FAILURES" == "1" ]]; then
      echo "::warning::${name} did not compile; continuing because TSZ_PROJECT_COMPILE_ALLOW_FAILURES=1"
      return 0
    fi
    return "$rc"
  fi

  echo "${name} compiled successfully."
  echo "::endgroup::"
}

should_check_project() {
  local name="$1"
  [[ -z "$PROJECT_FILTER" || "$name" =~ $PROJECT_FILTER ]]
}

run_required_projects() {
if should_check_project "utility-types-project"; then
  ensure_git_fixture "utility-types" "$UTILITY_TYPES_REPO" "$UTILITY_TYPES_REF" "$FIXTURE_ROOT/utility-types"
  write_utility_types_config
  check_project "utility-types-project" "$FIXTURE_ROOT/utility-types/tsconfig.tsz-guard.json"
fi

if should_check_project "ts-essentials-project"; then
  ensure_git_fixture "ts-essentials" "$TS_ESSENTIALS_REPO" "$TS_ESSENTIALS_REF" "$FIXTURE_ROOT/ts-essentials"
  write_ts_essentials_config
  check_project "ts-essentials-project" "$FIXTURE_ROOT/ts-essentials/tsconfig.tsz-guard.json"
fi

if should_check_project "rxjs-project"; then
  ensure_git_fixture "rxjs" "$RXJS_REPO" "$RXJS_REF" "$FIXTURE_ROOT/rxjs"
  write_rxjs_config
  check_project "rxjs-project" "$FIXTURE_ROOT/rxjs/tsconfig.tsz-guard.json"
fi

if should_check_project "type-fest-project"; then
  ensure_git_fixture "type-fest" "$TYPE_FEST_REPO" "$TYPE_FEST_REF" "$FIXTURE_ROOT/type-fest"
  write_type_fest_config
  check_project "type-fest-project" "$FIXTURE_ROOT/type-fest/tsconfig.tsz-guard.json"
fi

if [[ "$INCLUDE_GENERATED_APPS" == "1" ]] \
  && { should_check_project "vite-vanilla-ts-app" || should_check_project "nextjs-fresh-app"; }; then
  if ! command -v node >/dev/null 2>&1 || ! command -v npm >/dev/null 2>&1; then
    echo "error: node and npm are required for generated app project compile guards" >&2
    exit 1
  fi

  if should_check_project "vite-vanilla-ts-app"; then
    node scripts/bench/generate-vite-app-fixture.mjs "$FIXTURE_ROOT/vite-vanilla-ts-live"
    check_project "vite-vanilla-ts-app" "$FIXTURE_ROOT/vite-vanilla-ts-live/tsconfig.json"
  fi

  if should_check_project "nextjs-fresh-app"; then
    node scripts/bench/generate-next-app-fixture.mjs "$FIXTURE_ROOT/next-app-live"
    check_project "nextjs-fresh-app" "$FIXTURE_ROOT/next-app-live/tsconfig.json"
  fi
fi
}

run_canary_projects() {
if should_check_project "ts-toolbelt-project"; then
  ensure_git_fixture "ts-toolbelt" "$TS_TOOLBELT_REPO" "$TS_TOOLBELT_REF" "$FIXTURE_ROOT/ts-toolbelt"
  write_ts_toolbelt_config
  check_project "ts-toolbelt-project" "$FIXTURE_ROOT/ts-toolbelt/tsconfig.tsz-guard.json"
fi

if should_check_project "zod-project"; then
  ensure_git_fixture "zod" "$ZOD_REPO" "$ZOD_REF" "$FIXTURE_ROOT/zod"
  write_zod_config
  check_project "zod-project" "$FIXTURE_ROOT/zod/tsconfig.tsz-guard.json"
fi

if should_check_project "kysely-project"; then
  ensure_git_fixture "kysely" "$KYSELY_REPO" "$KYSELY_REF" "$FIXTURE_ROOT/kysely"
  write_kysely_config
  check_project "kysely-project" "$FIXTURE_ROOT/kysely/tsconfig.tsz-guard.json"
fi

if should_check_project "type-challenges-project"; then
  ensure_git_fixture "type-challenges" "$TYPE_CHALLENGES_REPO" "$TYPE_CHALLENGES_REF" "$FIXTURE_ROOT/type-challenges"
  write_type_challenges_config
  check_project "type-challenges-project" "$FIXTURE_ROOT/type-challenges/.tsz-compile/tsconfig.tsz-guard.json"
fi

if should_check_project "type-challenges-solutions-project"; then
  ensure_git_fixture "type-challenges-solutions" "$TYPE_CHALLENGES_SOLUTIONS_REPO" "$TYPE_CHALLENGES_SOLUTIONS_REF" "$FIXTURE_ROOT/type-challenges-solutions"
  write_type_challenges_solutions_config
  check_project "type-challenges-solutions-project" "$FIXTURE_ROOT/type-challenges-solutions/.tsz-compile/tsconfig.tsz-guard.json"
fi
}

case "$PROJECT_SET" in
  required)
    run_required_projects
    ;;
  canary)
    run_canary_projects
    ;;
  all)
    run_required_projects
    run_canary_projects
    ;;
  *)
    echo "error: unknown TSZ_PROJECT_COMPILE_SET: $PROJECT_SET" >&2
    exit 2
    ;;
esac

if [[ "$FAILURES" -gt 0 ]]; then
  echo "Project compile failures: $FAILURES"
fi
