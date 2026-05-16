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

UTILITY_TYPES_REPO="${UTILITY_TYPES_REPO:-https://github.com/piotrwitek/utility-types.git}"
UTILITY_TYPES_REF="${UTILITY_TYPES_REF:-2ee1f6ecb241651ab22390fee7ee5349942efda2}"
TS_TOOLBELT_REPO="${TS_TOOLBELT_REPO:-https://github.com/millsp/ts-toolbelt.git}"
TS_TOOLBELT_REF="${TS_TOOLBELT_REF:-b8a49285e3ed3a7d8bb8e0b433389eac46a5f140}"
TS_ESSENTIALS_REPO="${TS_ESSENTIALS_REPO:-https://github.com/ts-essentials/ts-essentials.git}"
TS_ESSENTIALS_REF="${TS_ESSENTIALS_REF:-5abe8700b42068048bd3c368e0531b6defe56558}"
RXJS_REPO="${RXJS_REPO:-https://github.com/ReactiveX/rxjs.git}"
RXJS_REF="${RXJS_REF:-e5351d02e225e275ac0e497c7b66eaa5f0c88791}"
TYPE_FEST_REPO="${TYPE_FEST_REPO:-https://github.com/sindresorhus/type-fest.git}"
TYPE_FEST_REF="${TYPE_FEST_REF:-4005f60b65a7bd224154d6da46f45a63b42ce70f}"
ZOD_REPO="${ZOD_REPO:-https://github.com/colinhacks/zod.git}"
ZOD_REF="${ZOD_REF:-93b0b6892cc0cfee8d0bec4e2e1242c7df771f95}"
KYSELY_REPO="${KYSELY_REPO:-https://github.com/kysely-org/kysely.git}"
KYSELY_REF="${KYSELY_REF:-d4911be21cd568d3694dc7f879f72390635226d7}"
TYPE_CHALLENGES_REPO="${TYPE_CHALLENGES_REPO:-https://github.com/type-challenges/type-challenges.git}"
TYPE_CHALLENGES_REF="${TYPE_CHALLENGES_REF:-0b0b0b18bcb7ac42dc22ce26ffb438231d4754b1}"

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
  local name="$1"
  local repo="$2"
  local ref="$3"
  local dir="$4"

  mkdir -p "$(dirname "$dir")"
  if [[ ! -d "$dir/.git" ]]; then
    echo "Cloning ${name} fixture..."
    rm -rf "$dir"
    git clone --quiet --no-tags --depth 1 "$repo" "$dir"
  fi

  local current_ref
  current_ref="$(git -C "$dir" rev-parse HEAD 2>/dev/null || true)"
  if [[ "$current_ref" != "$ref" ]]; then
    echo "Pinning ${name} to ${ref:0:12}..."
    git -C "$dir" fetch --quiet --depth 1 origin "$ref"
    git -C "$dir" checkout --quiet --detach FETCH_HEAD
  fi
}

write_utility_types_config() {
  cat > "$FIXTURE_ROOT/utility-types/tsconfig.tsz-guard.json" <<'JSON'
{
  "compilerOptions": {
    "strict": true,
    "lib": ["dom", "es2017"],
    "types": [],
    "target": "ES2015",
    "module": "commonjs",
    "skipLibCheck": true,
    "noEmit": true
  },
  "include": ["src/**/*.ts"],
  "exclude": ["src/**/*.snap.ts", "src/**/*.spec.ts"]
}
JSON
}

write_ts_toolbelt_config() {
  cat > "$FIXTURE_ROOT/ts-toolbelt/tsconfig.tsz-guard.json" <<'JSON'
{
  "compilerOptions": {
    "target": "ES2015",
    "module": "commonjs",
    "lib": ["esnext", "dom"],
    "types": [],
    "strict": false,
    "strictNullChecks": true,
    "strictFunctionTypes": true,
    "noImplicitAny": true,
    "noImplicitReturns": true,
    "noFallthroughCasesInSwitch": true,
    "esModuleInterop": true,
    "downlevelIteration": true,
    "forceConsistentCasingInFileNames": true,
    "skipLibCheck": true,
    "noEmit": true,
    "ignoreDeprecations": "6.0"
  },
  "include": ["sources/**/*.ts"],
  "exclude": ["tests/**/*", "scripts/**/*", "node_modules/**/*"]
}
JSON
}

write_ts_essentials_config() {
  cat > "$FIXTURE_ROOT/ts-essentials/tsconfig.tsz-guard.json" <<'JSON'
{
  "compilerOptions": {
    "target": "es2017",
    "module": "commonjs",
    "strict": true,
    "lib": ["es2018"],
    "types": [],
    "skipLibCheck": true,
    "noEmit": true,
    "forceConsistentCasingInFileNames": true
  },
  "include": ["lib/**/*.ts"],
  "exclude": ["test/**/*", "node_modules/**/*"]
}
JSON
}

write_rxjs_config() {
  local rxjs_src_root="src"
  if [[ -d "$FIXTURE_ROOT/rxjs/packages/rxjs/src/internal" ]]; then
    rxjs_src_root="packages/rxjs/src"
  fi
  cat > "$FIXTURE_ROOT/rxjs/tsconfig.tsz-guard.json" <<JSON
{
  "compilerOptions": {
    "target": "es2017",
    "module": "esnext",
    "strict": true,
    "lib": ["es2018", "dom"],
    "types": [],
    "skipLibCheck": true,
    "noEmit": true,
    "noCheck": true,
    "forceConsistentCasingInFileNames": true,
    "moduleResolution": "bundler"
  },
  "include": ["${rxjs_src_root}/internal/**/*.ts"],
  "exclude": [
    "**/*.spec.ts",
    "**/*.test.ts",
    "node_modules/**/*",
    "**/internal/observable/dom/**",
    "**/internal/umd.ts"
  ]
}
JSON
}

write_type_fest_config() {
  cat > "$FIXTURE_ROOT/type-fest/tsconfig.tsz-guard.json" <<'JSON'
{
  "compilerOptions": {
    "target": "es2017",
    "module": "esnext",
    "strict": true,
    "lib": ["es2022"],
    "types": [],
    "skipLibCheck": true,
    "noEmit": true,
    "forceConsistentCasingInFileNames": true,
    "moduleResolution": "bundler"
  },
  "include": ["source/**/*.d.ts", "index.d.ts"],
  "exclude": ["test-d/**/*", "node_modules/**/*"]
}
JSON
}

write_zod_config() {
  cat > "$FIXTURE_ROOT/zod/tsconfig.tsz-guard.json" <<'JSON'
{
  "compilerOptions": {
    "target": "es2017",
    "module": "esnext",
    "strict": true,
    "lib": ["es2022", "dom"],
    "types": [],
    "skipLibCheck": true,
    "noEmit": true,
    "forceConsistentCasingInFileNames": true,
    "moduleResolution": "bundler"
  },
  "include": ["src/**/*.ts", "packages/zod/src/**/*.ts"],
  "exclude": [
    "**/*.test.ts",
    "**/__tests__/**",
    "**/benchmarks/**",
    "node_modules/**/*"
  ]
}
JSON
}

write_kysely_config() {
  cat > "$FIXTURE_ROOT/kysely/tsz-bench-globals.d.ts" <<'GLOBALSEOF'
declare const Buffer: {
  isBuffer(value: unknown): boolean;
  compare(left: unknown, right: unknown): number;
};
GLOBALSEOF
  cat > "$FIXTURE_ROOT/kysely/tsconfig.tsz-guard.json" <<'JSON'
{
  "compilerOptions": {
    "target": "es2017",
    "module": "esnext",
    "strict": true,
    "lib": ["es2022", "dom"],
    "types": [],
    "skipLibCheck": true,
    "noEmit": true,
    "forceConsistentCasingInFileNames": true,
    "moduleResolution": "bundler"
  },
  "include": ["src/**/*.ts", "tsz-bench-globals.d.ts"],
  "exclude": [
    "**/*.test.ts",
    "test/**/*",
    "node_modules/**/*",
    "**/dialect/mssql/**",
    "**/util/object-utils.ts",
    "**/util/performance-now.ts"
  ]
}
JSON
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
