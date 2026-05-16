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

UTILITY_TYPES_REPO="${UTILITY_TYPES_REPO:-https://github.com/piotrwitek/utility-types.git}"
UTILITY_TYPES_REF="${UTILITY_TYPES_REF:-2ee1f6ecb241651ab22390fee7ee5349942efda2}"
TS_ESSENTIALS_REPO="${TS_ESSENTIALS_REPO:-https://github.com/ts-essentials/ts-essentials.git}"
TS_ESSENTIALS_REF="${TS_ESSENTIALS_REF:-5abe8700b42068048bd3c368e0531b6defe56558}"
RXJS_REPO="${RXJS_REPO:-https://github.com/ReactiveX/rxjs.git}"
RXJS_REF="${RXJS_REF:-e5351d02e225e275ac0e497c7b66eaa5f0c88791}"
TYPE_FEST_REPO="${TYPE_FEST_REPO:-https://github.com/sindresorhus/type-fest.git}"
TYPE_FEST_REF="${TYPE_FEST_REF:-4005f60b65a7bd224154d6da46f45a63b42ce70f}"

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
    if [[ "$rc" -eq 124 ]]; then
      echo "error: ${name} timed out after ${PROJECT_TIMEOUT}s" >&2
    else
      echo "error: ${name} failed with exit code ${rc}" >&2
    fi
    sed -n '1,160p' "$log" >&2 || true
    echo "::endgroup::"
    return "$rc"
  fi

  echo "${name} compiled successfully."
  echo "::endgroup::"
}

should_check_project() {
  local name="$1"
  [[ -z "$PROJECT_FILTER" || "$name" =~ $PROJECT_FILTER ]]
}

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
