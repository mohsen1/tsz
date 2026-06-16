#!/usr/bin/env bash
#
# Shared project fixture metadata and config writers for benchmark and CI
# project-compile guards. Fixture pins (repo URLs and commit hashes) live in
# project-rows.mjs as the single source of truth and are loaded at runtime
# by tsz_load_fixture_pins_from_rows. Shell env vars override the defaults.

if [ -z "${TSZ_PROJECT_FIXTURES_ROOT:-}" ] && [ -n "${BASH_SOURCE[0]:-}" ]; then
  TSZ_PROJECT_FIXTURES_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
elif [ -z "${TSZ_PROJECT_FIXTURES_ROOT:-}" ] && [ -n "${SCRIPT_DIR:-}" ]; then
  TSZ_PROJECT_FIXTURES_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
elif [ -z "${TSZ_PROJECT_FIXTURES_ROOT:-}" ] && [ -n "${ROOT_DIR:-}" ]; then
  TSZ_PROJECT_FIXTURES_ROOT="$(cd "$ROOT_DIR" && pwd)"
elif [ -z "${TSZ_PROJECT_FIXTURES_ROOT:-}" ]; then
  TSZ_PROJECT_FIXTURES_ROOT="$(cd "$(pwd)" && pwd)"
fi

TSZ_PROJECT_ROWS_MJS="$TSZ_PROJECT_FIXTURES_ROOT/scripts/bench/project-rows.mjs"

# Canonical project row groups for runners that share fixture handling.
# Keep row names here aligned with the project-corpus workflows.
TSZ_COMPILE_GUARD_REQUIRED_ROWS=(
  "utility-types-project"
  "ts-essentials-project"
  "rxjs-project"
  "type-fest-project"
  "vite-vanilla-ts-app"
  "nextjs-fresh-app"
  "comlink-project"
)

TSZ_COMPILE_GUARD_CANARY_ROWS=(
  "ts-toolbelt-project"
  "zod-project"
  "kysely-project"
  "type-challenges-solutions-project"
  "valibot-project"
  "msw-project"
  "effect-project"
  "drizzle-orm-project"
  "ts-rest-project"
  "ofetch-project"
  "ts-pattern-project"
  "trpc-project"
  "tanstack-query-project"
  "tanstack-router-project"
  "zustand-project"
  "jotai-project"
  "fp-ts-project"
  "io-ts-project"
  "immer-project"
  "remeda-project"
  "ts-morph-project"
  "arktype-project"
  "superstruct-project"
  "runtypes-project"
  "hotscript-project"
  "typebox-project"
  "class-transformer-project"
  "type-graphql-project"
  "neverthrow-project"
  "xstate-project"
  "mobx-project"
)

# Row metadata pre-loaded by tsz_load_fixture_pins_from_rows (pipe-delimited).
# Set once at module init; subsequent tsz_sync_project_row_groups calls use
# these values directly so no additional Node.js processes are needed.
_TSZ_PROJECT_METADATA_LOADED=""
_TSZ_PACKED_GUARD_REQUIRED_ROWS=""
_TSZ_PACKED_CANARY_ROWS=""
_TSZ_PACKED_COMPAT_ROWS=""

_tsz_unpack_row_groups() {
  IFS='|' read -ra TSZ_COMPILE_GUARD_REQUIRED_ROWS <<< "$_TSZ_PACKED_GUARD_REQUIRED_ROWS"
  IFS='|' read -ra TSZ_COMPILE_GUARD_CANARY_ROWS <<< "$_TSZ_PACKED_CANARY_ROWS"
}

tsz_sync_project_row_groups() {
  # Fast path: use row groups already loaded by tsz_load_fixture_pins_from_rows.
  if [[ -n "${_TSZ_PACKED_GUARD_REQUIRED_ROWS:-}" && -n "${_TSZ_PACKED_CANARY_ROWS:-}" ]]; then
    _tsz_unpack_row_groups
    return 0
  fi

  # Fallback: trigger the consolidated metadata load (handles the case where node
  # was unavailable at module init) then retry the fast path.
  tsz_load_fixture_pins_from_rows
  if [[ -n "${_TSZ_PACKED_GUARD_REQUIRED_ROWS:-}" && -n "${_TSZ_PACKED_CANARY_ROWS:-}" ]]; then
    _tsz_unpack_row_groups
  fi
}

tsz_project_owner_families_json() {
  TSZ_PROJECT_ROWS_MJS="$TSZ_PROJECT_ROWS_MJS" node --input-type=module <<'NODE'
import { pathToFileURL } from "node:url";
const rowModule = await import(pathToFileURL(process.env.TSZ_PROJECT_ROWS_MJS || process.cwd() + "/scripts/bench/project-rows.mjs"));
const { COMPATIBILITY_CORPUS_ROWS } = rowModule;

const entries = [];
for (const row of COMPATIBILITY_CORPUS_ROWS) {
  entries.push([row.name, row.family]);
}
console.log(JSON.stringify(Object.fromEntries(entries)));
NODE
}

tsz_validate_project_row_metadata() {
  node "$TSZ_PROJECT_FIXTURES_ROOT/scripts/bench/validate-project-metadata.mjs"
}

tsz_project_readme_candidates_json() {
  TSZ_PROJECT_ROWS_MJS="$TSZ_PROJECT_ROWS_MJS" node --input-type=module <<'NODE'
import { pathToFileURL } from "node:url";
const rowModule = await import(pathToFileURL(process.env.TSZ_PROJECT_ROWS_MJS || process.cwd() + "/scripts/bench/project-rows.mjs"));
const { COMPATIBILITY_CORPUS_ROWS } = rowModule;

const entries = [];
for (const row of COMPATIBILITY_CORPUS_ROWS) {
  const candidates = row.readme_candidates || ["README.md"];
  entries.push([row.name, candidates]);
}
console.log(JSON.stringify(Object.fromEntries(entries)));
NODE
}

tsz_generated_fixture_source() {
  local row_name="$1"
  local fixture_dir="$2"
  local provenance_path="$fixture_dir/.tsz-fixture-provenance.json"

  [ -n "$fixture_dir" ] || return 0
  [ -f "$provenance_path" ] || return 0
  command -v node >/dev/null 2>&1 || return 0

  TSZ_FIXTURE_PROVENANCE_ROW="$row_name" \
  TSZ_FIXTURE_PROVENANCE_FILE="$provenance_path" \
  node --input-type=module <<'NODE'
import fs from "node:fs";

const rowName = process.env.TSZ_FIXTURE_PROVENANCE_ROW || "generated-project";
const file = process.env.TSZ_FIXTURE_PROVENANCE_FILE || "";

try {
  const provenance = JSON.parse(fs.readFileSync(file, "utf8"));
  const hashes = provenance.file_hashes && typeof provenance.file_hashes === "object"
    ? provenance.file_hashes
    : {};
  const lockHash = hashes["package-lock.json"];
  const packageHash = hashes["package.json"];
  const ref = lockHash
    ? `package-lock:${lockHash}`
    : packageHash
      ? `package-json:${packageHash}`
      : null;
  const template = String(provenance.template_name || rowName).trim();
  const generator = String(provenance.generator_script || "generated").trim();

  if (template && generator && ref) {
    console.log(`${template}|generated:${generator}|${ref}`);
  }
} catch {
  // Fixture provenance improves auditability but must not make project setup fail.
}
NODE
}

tsz_load_fixture_pins_from_rows() {
  # Idempotency guard: skip if already loaded in this process.
  [[ -n "${_TSZ_PROJECT_METADATA_LOADED:-}" ]] && return 0
  command -v node >/dev/null 2>&1 || return 0

  local assignments
  assignments="$(TSZ_PROJECT_ROWS_MJS="$TSZ_PROJECT_ROWS_MJS" node --input-type=module <<'NODE'
import { pathToFileURL } from "node:url";
const {
  PROJECT_ROW_DEFINITIONS,
  COMPILE_GUARD_REQUIRED_ROWS,
  COMPILE_CANARY_PROJECT_ROWS,
  REQUIRED_PROJECT_ROWS,
} = await import(pathToFileURL(process.env.TSZ_PROJECT_ROWS_MJS));

const PIN_FIELDS = [
  ["repo_env", "repo"],
  ["ref_env", "ref"],
  ["expected_generated_env", "expected_generated"],
  ["expected_test_cases_env", "expected_test_cases"],
];

for (const row of PROJECT_ROW_DEFINITIONS) {
  for (const [envField, valueField] of PIN_FIELDS) {
    if (row[envField] && row[valueField] !== undefined) {
      process.stdout.write(row[envField] + "=" + row[valueField] + "\n");
    }
  }
}

// Emit row group metadata so callers avoid separate Node.js invocations.
// __TSZ_GUARD_REQUIRED__ = compile-guard required rows (guard_set=required).
// __TSZ_CANARY__         = compile-guard canary rows (guard_set=canary).
// __TSZ_COMPAT__         = all rows tracked for compatibility reporting
//                          (benchmark_set=required ∪ guard_set=canary).
const guardRequired = Array.isArray(COMPILE_GUARD_REQUIRED_ROWS) ? COMPILE_GUARD_REQUIRED_ROWS : [];
const canary = Array.isArray(COMPILE_CANARY_PROJECT_ROWS) ? COMPILE_CANARY_PROJECT_ROWS : [];
const benchRequired = Array.isArray(REQUIRED_PROJECT_ROWS) ? REQUIRED_PROJECT_ROWS : [];
// Some rows have both benchmark_set=required and guard_set=canary (e.g. ts-toolbelt, zod,
// kysely), so a Set dedup is required before joining.
const compatSet = new Set([...benchRequired, ...canary]);

if (guardRequired.length > 0)
  process.stdout.write("__TSZ_GUARD_REQUIRED__=" + guardRequired.join("|") + "\n");
if (canary.length > 0)
  process.stdout.write("__TSZ_CANARY__=" + canary.join("|") + "\n");
if (compatSet.size > 0)
  process.stdout.write("__TSZ_COMPAT__=" + [...compatSet].join("|") + "\n");
NODE
  )" || return 0

  local varname value
  while IFS='=' read -r varname value; do
    [ -z "$varname" ] && continue
    case "$varname" in
      __TSZ_GUARD_REQUIRED__) _TSZ_PACKED_GUARD_REQUIRED_ROWS="$value" ;;
      __TSZ_CANARY__)         _TSZ_PACKED_CANARY_ROWS="$value" ;;
      __TSZ_COMPAT__)         _TSZ_PACKED_COMPAT_ROWS="$value" ;;
      *)
        if [[ -z "${!varname+x}" ]]; then
          export "$varname=$value"
        fi
        ;;
    esac
  done <<< "$assignments"

  _TSZ_PROJECT_METADATA_LOADED=1
}

tsz_load_fixture_pins_from_rows

tsz_project_slowdown_failure_factor() {
  printf '%s\n' "${TSZ_BENCH_PROJECT_SLOWDOWN_FAILURE_FACTOR:-8}"
}

tsz_project_slowdown_failure_reached() {
  local tsz_mean="$1"
  local tsgo_mean="$2"
  local threshold
  threshold="$(tsz_project_slowdown_failure_factor)"

  [[ "$threshold" =~ ^[0-9]+([.][0-9]+)?$ ]] || return 1
  (( $(echo "$threshold > 0" | bc -l) )) || return 1
  (( $(echo "$tsz_mean / $tsgo_mean >= $threshold" | bc -l) ))
}

tsz_project_fixture_sources() {
  case "$1" in
    utility-types-project)
      printf 'utility-types|%s|%s\n' "$UTILITY_TYPES_REPO" "$UTILITY_TYPES_REF"
      ;;
    ts-toolbelt-project)
      printf 'ts-toolbelt|%s|%s\n' "$TS_TOOLBELT_REPO" "$TS_TOOLBELT_REF"
      ;;
    ts-essentials-project)
      printf 'ts-essentials|%s|%s\n' "$TS_ESSENTIALS_REPO" "$TS_ESSENTIALS_REF"
      ;;
    rxjs-project)
      printf 'rxjs|%s|%s\n' "$RXJS_REPO" "$RXJS_REF"
      ;;
    type-fest-project)
      printf 'type-fest|%s|%s\n' "$TYPE_FEST_REPO" "$TYPE_FEST_REF"
      ;;
    zod-project)
      printf 'zod|%s|%s\n' "$ZOD_REPO" "$ZOD_REF"
      ;;
    kysely-project)
      printf 'kysely|%s|%s\n' "$KYSELY_REPO" "$KYSELY_REF"
      ;;
    nextjs)
      printf 'nextjs|%s|%s\n' "$NEXTJS_REPO" "$NEXTJS_REF"
      ;;
    large-ts-repo)
      printf 'large-ts-repo|%s|%s\n' "$LARGE_TS_REPO" "$LARGE_TS_REF"
      ;;
    type-challenges-solutions-project)
      printf 'type-challenges-solutions|%s|%s\n' "$TYPE_CHALLENGES_SOLUTIONS_REPO" "$TYPE_CHALLENGES_SOLUTIONS_REF"
      ;;
    valibot-project)
      printf 'valibot|%s|%s\n' "$VALIBOT_REPO" "$VALIBOT_REF"
      ;;
    msw-project)
      printf 'msw|%s|%s\n' "$MSW_REPO" "$MSW_REF"
      ;;
    comlink-project)
      printf 'comlink|%s|%s\n' "$COMLINK_REPO" "$COMLINK_REF"
      ;;
    effect-project)
      printf 'effect|%s|%s\n' "$EFFECT_REPO" "$EFFECT_REF"
      ;;
    drizzle-orm-project)
      printf 'drizzle-orm|%s|%s\n' "$DRIZZLE_ORM_REPO" "$DRIZZLE_ORM_REF"
      ;;
    ts-rest-project)
      printf 'ts-rest|%s|%s\n' "$TS_REST_REPO" "$TS_REST_REF"
      ;;
    ofetch-project)
      printf 'ofetch|%s|%s\n' "$OFETCH_REPO" "$OFETCH_REF"
      ;;
    ts-pattern-project)
      printf 'ts-pattern|%s|%s\n' "$TS_PATTERN_REPO" "$TS_PATTERN_REF"
      ;;
    trpc-project)
      printf 'trpc|%s|%s\n' "$TRPC_REPO" "$TRPC_REF"
      ;;
    tanstack-query-project)
      printf 'tanstack-query|%s|%s\n' "$TANSTACK_QUERY_REPO" "$TANSTACK_QUERY_REF"
      ;;
    tanstack-router-project)
      printf 'tanstack-router|%s|%s\n' "$TANSTACK_ROUTER_REPO" "$TANSTACK_ROUTER_REF"
      ;;
    zustand-project)
      printf 'zustand|%s|%s\n' "$ZUSTAND_REPO" "$ZUSTAND_REF"
      ;;
    jotai-project)
      printf 'jotai|%s|%s\n' "$JOTAI_REPO" "$JOTAI_REF"
      ;;
    fp-ts-project)
      printf 'fp-ts|%s|%s\n' "$FP_TS_REPO" "$FP_TS_REF"
      ;;
    io-ts-project)
      printf 'io-ts|%s|%s\n' "$IO_TS_REPO" "$IO_TS_REF"
      ;;
    immer-project)
      printf 'immer|%s|%s\n' "$IMMER_REPO" "$IMMER_REF"
      ;;
    remeda-project)
      printf 'remeda|%s|%s\n' "$REMEDA_REPO" "$REMEDA_REF"
      ;;
    ts-morph-project)
      printf 'ts-morph|%s|%s\n' "$TS_MORPH_REPO" "$TS_MORPH_REF"
      ;;
    arktype-project)
      printf 'arktype|%s|%s\n' "$ARKTYPE_REPO" "$ARKTYPE_REF"
      ;;
    superstruct-project)
      printf 'superstruct|%s|%s\n' "$SUPERSTRUCT_REPO" "$SUPERSTRUCT_REF"
      ;;
    runtypes-project)
      printf 'runtypes|%s|%s\n' "$RUNTYPES_REPO" "$RUNTYPES_REF"
      ;;
    hotscript-project)
      printf 'hotscript|%s|%s\n' "$HOTSCRIPT_REPO" "$HOTSCRIPT_REF"
      ;;
    typebox-project)
      printf 'typebox|%s|%s\n' "$TYPEBOX_REPO" "$TYPEBOX_REF"
      ;;
    class-transformer-project)
      printf 'class-transformer|%s|%s\n' "$CLASS_TRANSFORMER_REPO" "$CLASS_TRANSFORMER_REF"
      ;;
    type-graphql-project)
      printf 'type-graphql|%s|%s\n' "$TYPE_GRAPHQL_REPO" "$TYPE_GRAPHQL_REF"
      ;;
    neverthrow-project)
      printf 'neverthrow|%s|%s\n' "$NEVERTHROW_REPO" "$NEVERTHROW_REF"
      ;;
    xstate-project)
      printf 'xstate|%s|%s\n' "$XSTATE_REPO" "$XSTATE_REF"
      ;;
    mobx-project)
      printf 'mobx|%s|%s\n' "$MOBX_REPO" "$MOBX_REF"
      ;;
    vite-vanilla-ts-app)
      local fixture_dir="${VITE_APP_BENCH_DIR:-}"
      [ -n "$fixture_dir" ] || [ -z "${FIXTURE_ROOT:-}" ] || fixture_dir="$FIXTURE_ROOT/vite-vanilla-ts-live"
      [ -n "$fixture_dir" ] || [ -z "${EXTERNAL_BENCH_DIR:-}" ] || fixture_dir="$EXTERNAL_BENCH_DIR/vite-vanilla-ts-live"
      tsz_generated_fixture_source "$1" "$fixture_dir"
      ;;
    nextjs-fresh-app)
      local fixture_dir="${NEXT_APP_BENCH_DIR:-}"
      [ -n "$fixture_dir" ] || [ -z "${FIXTURE_ROOT:-}" ] || fixture_dir="$FIXTURE_ROOT/next-app-live"
      [ -n "$fixture_dir" ] || [ -z "${EXTERNAL_BENCH_DIR:-}" ] || fixture_dir="$EXTERNAL_BENCH_DIR/next-app-live"
      tsz_generated_fixture_source "$1" "$fixture_dir"
      ;;
  esac
}

tsz_ensure_git_fixture() {
  local name="$1"
  local repo="$2"
  local ref="$3"
  local dir="$4"
  local reclone_dirty="${5:-0}"

  mkdir -p "$(dirname "$dir")"
  if [[ ! -d "$dir/.git" ]]; then
    echo "Cloning ${name} fixture..."
    tsz_remove_fixture_dir "$name" "$dir"
    git clone --quiet --no-tags --depth 1 "$repo" "$dir"
  fi

  if [[ "$reclone_dirty" == "1" ]] \
    && [[ -n "$(git -C "$dir" status --porcelain 2>/dev/null)" ]]; then
    echo "${name} fixture is dirty; recloning for reproducibility..."
    tsz_remove_fixture_dir "$name" "$dir"
    git clone --quiet --no-tags --depth 1 "$repo" "$dir"
  fi

  if [[ -n "$ref" ]]; then
    local current_ref
    current_ref="$(git -C "$dir" rev-parse HEAD 2>/dev/null || true)"
    if [[ "$current_ref" != "$ref" ]]; then
      echo "Pinning ${name} to ${ref:0:12}..."
      git -C "$dir" fetch --quiet --depth 1 origin "$ref"
      git -C "$dir" checkout --quiet --detach FETCH_HEAD
    fi
  fi
}

tsz_physical_path_for_maybe_missing() {
  local path="$1"
  local parent base parent_physical

  [[ -n "$path" ]] || return 1
  parent="$(dirname "$path")"
  base="$(basename "$path")"
  parent_physical="$(cd "$parent" && pwd -P)" || return 1
  printf '%s/%s\n' "$parent_physical" "$base"
}

tsz_remove_fixture_dir() {
  local name="$1"
  local dir="$2"
  local target root cwd home

  target="$(tsz_physical_path_for_maybe_missing "$dir")" || {
    echo "Refusing to remove ${name} fixture with unresolved path: $dir" >&2
    return 1
  }
  root="$(cd "$TSZ_PROJECT_FIXTURES_ROOT" && pwd -P)"
  home="${HOME:-}"
  case "$target" in
    ""|"/"|"$home"|"$root")
      echo "Refusing to remove unsafe ${name} fixture path: $target" >&2
      return 1
      ;;
  esac

  cwd="$(pwd -P)"
  case "$cwd" in
    "$target"|"$target"/*)
      cd "$root"
      ;;
  esac

  rm -rf "$target"
}

tsz_rxjs_src_root() {
  local fixture_dir="$1"
  if [[ -d "$fixture_dir/packages/rxjs/src/internal" ]]; then
    printf '%s\n' "packages/rxjs/src"
  else
    printf '%s\n' "src"
  fi
}

tsz_write_utility_types_config() {
  local output="$1"
  cat > "$output" <<'JSON'
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

tsz_write_ts_toolbelt_config() {
  local output="$1"
  cat > "$output" <<'JSON'
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

tsz_write_ts_essentials_config() {
  local output="$1"
  cat > "$output" <<'JSON'
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

tsz_write_rxjs_config() {
  local output="$1"
  local rxjs_src_root="$2"
  cat > "$output" <<JSON
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

tsz_write_type_fest_config() {
  local output="$1"
  cat > "$output" <<'JSON'
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

tsz_write_zod_config() {
  local output="$1"
  cat > "$output" <<'JSON'
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

tsz_write_kysely_globals() {
  local output="$1"
  cat > "$output" <<'GLOBALSEOF'
declare const Buffer: {
  isBuffer(value: unknown): boolean;
  compare(left: unknown, right: unknown): number;
};
GLOBALSEOF
}

tsz_write_kysely_config() {
  local output="$1"
  cat > "$output" <<'JSON'
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

tsz_write_basic_external_project_config() {
  local output="$1"
  local source_dir="$2"
  # Optional 3rd arg: extra compilerOptions JSON lines (each must include its
  # own trailing comma) injected verbatim. Used by sources whose real tsconfig
  # enables options the shared baseline omits (e.g. allowImportingTsExtensions
  # for projects that import sibling modules with explicit `.ts` extensions).
  local extra_compiler_options="${3:-}"
  local extra_include="${4:-}"
  cat > "$output" <<JSON
{
  "compilerOptions": {
    "target": "es2022",
    "module": "esnext",
    "strict": true,
    "lib": ["es2022", "dom", "dom.iterable"],
    "types": [],
    "skipLibCheck": true,
    "noEmit": true,
    "forceConsistentCasingInFileNames": true,
    "moduleResolution": "bundler",
    "allowSyntheticDefaultImports": true,
    "esModuleInterop": true,
${extra_compiler_options}    "resolveJsonModule": true
  },
  "include": ["${source_dir}/**/*.ts", "${source_dir}/**/*.tsx"${extra_include}],
  "exclude": [
    "**/*.test.ts",
    "**/*.test.tsx",
    "**/*.spec.ts",
    "**/*.spec.tsx",
    "**/__tests__/**",
    "**/node_modules/**",
    "**/dist/**",
    "**/build/**"
  ]
}
JSON
}

tsz_write_valibot_config() {
  tsz_write_basic_external_project_config "$1" "library/src"
}

tsz_write_msw_config() {
  tsz_write_basic_external_project_config "$1" "src"
}

tsz_write_comlink_config() {
  tsz_write_basic_external_project_config "$1" "src"
}

tsz_write_effect_config() {
  tsz_write_basic_external_project_config "$1" "packages/effect/src"
}

tsz_write_drizzle_orm_external_stubs() {
  local output="$1"
  local fixture_dir
  fixture_dir="$(dirname "$output")"

  mkdir -p \
    "$fixture_dir/node_modules/@cloudflare/workers-types" \
    "$fixture_dir/node_modules/bun-types"

  cat > "$fixture_dir/tsz-bench-external-module.d.ts" <<'TYPES'
declare const tszBenchExternalModule: any;
export = tszBenchExternalModule;
TYPES

  cat > "$fixture_dir/tsz-bench-external-named-modules.d.ts" <<'TYPES'
type Buffer = any;
declare const Buffer: {
  isBuffer(value: unknown): boolean;
  compare(left: unknown, right: unknown): number;
  from(value: unknown, encoding?: string): Buffer;
};

interface ErrorConstructor {
  captureStackTrace?(targetObject: object, constructorOpt?: Function): void;
}

interface DurableObjectStorage {
  [key: string]: any;
}

type SqlStorageCursor<T = Record<string, unknown>> = any;
type SqlStorageValue = any;
type D1Response = any;

declare module '@aws-sdk/client-rds-data' {
  export class BeginTransactionCommand {
    constructor(input?: any);
  }
  export interface ColumnMetadata {
    name?: any;
    [key: string]: any;
  }
  export class CommitTransactionCommand {
    constructor(input?: any);
  }
  export class ExecuteStatementCommand {
    input: any;
    constructor(input?: any);
  }
  export interface ExecuteStatementCommandOutput {
    records?: Field[][];
    columnMetadata?: ColumnMetadata[];
    [key: string]: any;
  }
  export interface Field {
    arrayValue?: {
      stringValues?: any;
      longValues?: any;
      doubleValues?: any;
      booleanValues?: any;
      arrayValues?: any;
      [key: string]: any;
    };
    blobValue?: any;
    booleanValue?: any;
    doubleValue?: any;
    isNull?: any;
    longValue?: any;
    stringValue?: any;
    [key: string]: any;
  }
  export class RDSDataClient {
    constructor(config?: RDSDataClientConfig);
    send(command: any): Promise<any>;
    [key: string]: any;
  }
  export interface RDSDataClientConfig {
    [key: string]: any;
  }
  export class RollbackTransactionCommand {
    constructor(input?: any);
  }
  export const TypeHint: any;
  export type TypeHint = any;
}

declare module '@electric-sql/pglite' {
  export const PGlite: any;
  export type PGlite<T = any, U = any, V = any, W = any> = any;
  export const PGliteOptions: any;
  export type PGliteOptions<T = any, U = any, V = any, W = any> = any;
  export const QueryOptions: any;
  export type QueryOptions<T = any, U = any, V = any, W = any> = any;
  export const Results: any;
  export type Results<T = any, U = any, V = any, W = any> = any;
  export const Row: any;
  export type Row<T = any, U = any, V = any, W = any> = any;
  export const Transaction: any;
  export type Transaction<T = any, U = any, V = any, W = any> = any;
  export const types: any;
  export type types<T = any, U = any, V = any, W = any> = any;
}

declare module '@libsql/client' {
  export const Client: any;
  export type Client<T = any, U = any, V = any, W = any> = any;
  export const Config: any;
  export type Config<T = any, U = any, V = any, W = any> = any;
  export const InArgs: any;
  export type InArgs<T = any, U = any, V = any, W = any> = any;
  export const InStatement: any;
  export type InStatement<T = any, U = any, V = any, W = any> = any;
  export const ResultSet: any;
  export type ResultSet<T = any, U = any, V = any, W = any> = any;
  export const Transaction: any;
  export type Transaction<T = any, U = any, V = any, W = any> = any;
  export const createClient: any;
  export type createClient<T = any, U = any, V = any, W = any> = any;
}

declare module '@libsql/client-wasm' {
  export const Client: any;
  export type Client<T = any, U = any, V = any, W = any> = any;
  export const Config: any;
  export type Config<T = any, U = any, V = any, W = any> = any;
  export const createClient: any;
  export type createClient<T = any, U = any, V = any, W = any> = any;
}

declare module '@libsql/client/http' {
  export const Client: any;
  export type Client<T = any, U = any, V = any, W = any> = any;
  export const Config: any;
  export type Config<T = any, U = any, V = any, W = any> = any;
  export const createClient: any;
  export type createClient<T = any, U = any, V = any, W = any> = any;
}

declare module '@libsql/client/node' {
  export const Client: any;
  export type Client<T = any, U = any, V = any, W = any> = any;
  export const Config: any;
  export type Config<T = any, U = any, V = any, W = any> = any;
  export const createClient: any;
  export type createClient<T = any, U = any, V = any, W = any> = any;
}

declare module '@libsql/client/sqlite3' {
  export const Client: any;
  export type Client<T = any, U = any, V = any, W = any> = any;
  export const Config: any;
  export type Config<T = any, U = any, V = any, W = any> = any;
  export const createClient: any;
  export type createClient<T = any, U = any, V = any, W = any> = any;
}

declare module '@libsql/client/web' {
  export const Client: any;
  export type Client<T = any, U = any, V = any, W = any> = any;
  export const Config: any;
  export type Config<T = any, U = any, V = any, W = any> = any;
  export const createClient: any;
  export type createClient<T = any, U = any, V = any, W = any> = any;
}

declare module '@libsql/client/ws' {
  export const Client: any;
  export type Client<T = any, U = any, V = any, W = any> = any;
  export const Config: any;
  export type Config<T = any, U = any, V = any, W = any> = any;
  export const createClient: any;
  export type createClient<T = any, U = any, V = any, W = any> = any;
}

declare module '@miniflare/d1' {
  export const D1Database: any;
  export type D1Database<T = any, U = any, V = any, W = any> = any;
}

declare module '@neondatabase/serverless' {
  export const Client: any;
  export type Client<T = any, U = any, V = any, W = any> = any;
  export const FullQueryResults: any;
  export type FullQueryResults<T = any, U = any, V = any, W = any> = any;
  export const HTTPQueryOptions: any;
  export type HTTPQueryOptions<T = any, U = any, V = any, W = any> = any;
  export const HTTPTransactionOptions: any;
  export type HTTPTransactionOptions<T = any, U = any, V = any, W = any> = any;
  export const NeonQueryFunction: any;
  export type NeonQueryFunction<T = any, U = any, V = any, W = any> = any;
  export const NeonQueryPromise: any;
  export type NeonQueryPromise<T = any, U = any, V = any, W = any> = any;
  export const Pool: any;
  export type Pool<T = any, U = any, V = any, W = any> = any;
  export const PoolClient: any;
  export type PoolClient<T = any, U = any, V = any, W = any> = any;
  export const PoolConfig: any;
  export type PoolConfig<T = any, U = any, V = any, W = any> = any;
  export const QueryArrayConfig: any;
  export type QueryArrayConfig<T = any, U = any, V = any, W = any> = any;
  export const QueryConfig: any;
  export type QueryConfig<T = any, U = any, V = any, W = any> = any;
  export const QueryResult: any;
  export type QueryResult<T = any, U = any, V = any, W = any> = any;
  export const QueryResultRow: any;
  export type QueryResultRow<T = any, U = any, V = any, W = any> = any;
  export const neon: any;
  export type neon<T = any, U = any, V = any, W = any> = any;
  export const neonConfig: any;
  export type neonConfig<T = any, U = any, V = any, W = any> = any;
  export const types: any;
  export type types<T = any, U = any, V = any, W = any> = any;
}

declare module '@netlify/db' {
  export const getDatabase: any;
  export type getDatabase<T = any, U = any, V = any, W = any> = any;
}

declare module '@op-engineering/op-sqlite' {
  export const OPSQLiteConnection: any;
  export type OPSQLiteConnection<T = any, U = any, V = any, W = any> = any;
  export const QueryResult: any;
  export type QueryResult<T = any, U = any, V = any, W = any> = any;
}

declare module '@opentelemetry/api' {
  export const Span: any;
  export type Span<T = any, U = any, V = any, W = any> = any;
  export const Tracer: any;
  export type Tracer<T = any, U = any, V = any, W = any> = any;
}

declare module '@planetscale/database' {
  export const Client: any;
  export type Client<T = any, U = any, V = any, W = any> = any;
  export const Config: any;
  export type Config<T = any, U = any, V = any, W = any> = any;
  export const Connection: any;
  export type Connection<T = any, U = any, V = any, W = any> = any;
  export const ExecutedQuery: any;
  export type ExecutedQuery<T = any, U = any, V = any, W = any> = any;
  export const Transaction: any;
  export type Transaction<T = any, U = any, V = any, W = any> = any;
}

declare module '@prisma/client' {
  export const Prisma: any;
  export type Prisma<T = any, U = any, V = any, W = any> = any;
}

declare module '@prisma/client/extension' {
  export const PrismaClient: any;
  export type PrismaClient<T = any, U = any, V = any, W = any> = any;
}

declare module '@tidbcloud/serverless' {
  export const Config: any;
  export type Config<T = any, U = any, V = any, W = any> = any;
  export const Connection: any;
  export type Connection<T = any, U = any, V = any, W = any> = any;
  export const ExecuteOptions: any;
  export type ExecuteOptions<T = any, U = any, V = any, W = any> = any;
  export const FullResult: any;
  export type FullResult<T = any, U = any, V = any, W = any> = any;
  export const Tx: any;
  export type Tx<T = any, U = any, V = any, W = any> = any;
  export const connect: any;
  export type connect<T = any, U = any, V = any, W = any> = any;
}

declare module '@upstash/redis' {
  export const Redis: any;
  export type Redis<T = any, U = any, V = any, W = any> = any;
}

declare module '@vercel/postgres' {
  export const QueryArrayConfig: any;
  export type QueryArrayConfig<T = any, U = any, V = any, W = any> = any;
  export const QueryConfig: any;
  export type QueryConfig<T = any, U = any, V = any, W = any> = any;
  export const QueryResult: any;
  export type QueryResult<T = any, U = any, V = any, W = any> = any;
  export const QueryResultRow: any;
  export type QueryResultRow<T = any, U = any, V = any, W = any> = any;
  export const VercelClient: any;
  export type VercelClient<T = any, U = any, V = any, W = any> = any;
  export const VercelPool: any;
  export type VercelPool<T = any, U = any, V = any, W = any> = any;
  export const VercelPoolClient: any;
  export type VercelPoolClient<T = any, U = any, V = any, W = any> = any;
  export const sql: any;
  export type sql<T = any, U = any, V = any, W = any> = any;
  export const types: any;
  export type types<T = any, U = any, V = any, W = any> = any;
}

declare module '@xata.io/client' {
  export const SQLPluginResult: any;
  export type SQLPluginResult<T = any, U = any, V = any, W = any> = any;
  export const SQLQueryResult: any;
  export type SQLQueryResult<T = any, U = any, V = any, W = any> = any;
}

declare module 'better-sqlite3' {
  export const Database: any;
  export type Database<T = any, U = any, V = any, W = any> = any;
  export const RunResult: any;
  export type RunResult<T = any, U = any, V = any, W = any> = any;
  export const Statement: any;
  export type Statement<T = any, U = any, V = any, W = any> = any;
}

declare module 'bun' {
  export const SQL: any;
  export type SQL<T = any, U = any, V = any, W = any> = any;
  export const SQLOptions: any;
  export type SQLOptions<T = any, U = any, V = any, W = any> = any;
  export const SavepointSQL: any;
  export type SavepointSQL<T = any, U = any, V = any, W = any> = any;
  export const TransactionSQL: any;
  export type TransactionSQL<T = any, U = any, V = any, W = any> = any;
}

declare module 'bun:sqlite' {
  export const Database: any;
  export type Database<T = any, U = any, V = any, W = any> = any;
  export const Statement: any;
  export type Statement<T = any, U = any, V = any, W = any> = any;
}

declare module 'drizzle-orm' {
  export const sql: any;
  export type sql<T = any, U = any, V = any, W = any> = any;
}

declare module 'drizzle-orm/Gel-core' {
  export const except: any;
  export type except<T = any, U = any, V = any, W = any> = any;
  export const exceptAll: any;
  export type exceptAll<T = any, U = any, V = any, W = any> = any;
  export const intersect: any;
  export type intersect<T = any, U = any, V = any, W = any> = any;
  export const intersectAll: any;
  export type intersectAll<T = any, U = any, V = any, W = any> = any;
  export const union: any;
  export type union<T = any, U = any, V = any, W = any> = any;
  export const unionAll: any;
  export type unionAll<T = any, U = any, V = any, W = any> = any;
}

declare module 'drizzle-orm/gel-core' {
  export const except: any;
  export type except<T = any, U = any, V = any, W = any> = any;
  export const exceptAll: any;
  export type exceptAll<T = any, U = any, V = any, W = any> = any;
  export const intersect: any;
  export type intersect<T = any, U = any, V = any, W = any> = any;
  export const intersectAll: any;
  export type intersectAll<T = any, U = any, V = any, W = any> = any;
  export const union: any;
  export type union<T = any, U = any, V = any, W = any> = any;
  export const unionAll: any;
  export type unionAll<T = any, U = any, V = any, W = any> = any;
}

declare module 'drizzle-orm/mysql-core' {
  export const except: any;
  export type except<T = any, U = any, V = any, W = any> = any;
  export const exceptAll: any;
  export type exceptAll<T = any, U = any, V = any, W = any> = any;
  export const intersect: any;
  export type intersect<T = any, U = any, V = any, W = any> = any;
  export const intersectAll: any;
  export type intersectAll<T = any, U = any, V = any, W = any> = any;
  export const union: any;
  export type union<T = any, U = any, V = any, W = any> = any;
  export const unionAll: any;
  export type unionAll<T = any, U = any, V = any, W = any> = any;
}

declare module 'drizzle-orm/pg-core' {
  export const except: any;
  export type except<T = any, U = any, V = any, W = any> = any;
  export const exceptAll: any;
  export type exceptAll<T = any, U = any, V = any, W = any> = any;
  export const intersect: any;
  export type intersect<T = any, U = any, V = any, W = any> = any;
  export const intersectAll: any;
  export type intersectAll<T = any, U = any, V = any, W = any> = any;
  export const union: any;
  export type union<T = any, U = any, V = any, W = any> = any;
  export const unionAll: any;
  export type unionAll<T = any, U = any, V = any, W = any> = any;
}

declare module 'drizzle-orm/singlestore-core' {
  export const except: any;
  export type except<T = any, U = any, V = any, W = any> = any;
  export const intersect: any;
  export type intersect<T = any, U = any, V = any, W = any> = any;
  export const minus: any;
  export type minus<T = any, U = any, V = any, W = any> = any;
  export const union: any;
  export type union<T = any, U = any, V = any, W = any> = any;
  export const unionAll: any;
  export type unionAll<T = any, U = any, V = any, W = any> = any;
}

declare module 'drizzle-orm/sqlite-core' {
  export const except: any;
  export type except<T = any, U = any, V = any, W = any> = any;
  export const intersect: any;
  export type intersect<T = any, U = any, V = any, W = any> = any;
  export const union: any;
  export type union<T = any, U = any, V = any, W = any> = any;
  export const unionAll: any;
  export type unionAll<T = any, U = any, V = any, W = any> = any;
}

declare module 'expo-sqlite' {
  export const SQLiteDatabase: any;
  export type SQLiteDatabase<T = any, U = any, V = any, W = any> = any;
  export const SQLiteRunResult: any;
  export type SQLiteRunResult<T = any, U = any, V = any, W = any> = any;
  export const SQLiteStatement: any;
  export type SQLiteStatement<T = any, U = any, V = any, W = any> = any;
  export const addDatabaseChangeListener: any;
  export type addDatabaseChangeListener<T = any, U = any, V = any, W = any> = any;
  export const useEffect: any;
  export type useEffect<T = any, U = any, V = any, W = any> = any;
  export const useState: any;
  export type useState<T = any, U = any, V = any, W = any> = any;
}

declare module 'gel' {
  export const Client: any;
  export type Client<T = any, U = any, V = any, W = any> = any;
  export const ConnectOptions: any;
  export type ConnectOptions<T = any, U = any, V = any, W = any> = any;
  export const DateDuration: any;
  export type DateDuration<T = any, U = any, V = any, W = any> = any;
  export const Duration: any;
  export type Duration<T = any, U = any, V = any, W = any> = any;
  export const LocalDate: any;
  export type LocalDate<T = any, U = any, V = any, W = any> = any;
  export const LocalDateTime: any;
  export type LocalDateTime<T = any, U = any, V = any, W = any> = any;
  export const LocalTime: any;
  export type LocalTime<T = any, U = any, V = any, W = any> = any;
  export const RelativeDuration: any;
  export type RelativeDuration<T = any, U = any, V = any, W = any> = any;
  export const createClient: any;
  export type createClient<T = any, U = any, V = any, W = any> = any;
}

declare module 'gel/dist/transaction' {
  export const Transaction: any;
  export type Transaction<T = any, U = any, V = any, W = any> = any;
}

declare module 'knex' {
  export const Knex: any;
  export type Knex<T = any, U = any, V = any, W = any> = any;
}

declare module 'kysely' {
  export const ColumnType: any;
  export type ColumnType<T = any, U = any, V = any, W = any> = any;
}

declare module 'mysql2' {
  export const Connection: any;
  export type Connection<T = any, U = any, V = any, W = any> = any;
  export const Pool: any;
  export type Pool<T = any, U = any, V = any, W = any> = any;
  export const PoolOptions: any;
  export type PoolOptions<T = any, U = any, V = any, W = any> = any;
  export const createPool: any;
  export type createPool<T = any, U = any, V = any, W = any> = any;
}

declare module 'mysql2/promise' {
  export const Connection: any;
  export type Connection<T = any, U = any, V = any, W = any> = any;
  export const FieldPacket: any;
  export type FieldPacket<T = any, U = any, V = any, W = any> = any;
  export const OkPacket: any;
  export type OkPacket<T = any, U = any, V = any, W = any> = any;
  export const Pool: any;
  export type Pool<T = any, U = any, V = any, W = any> = any;
  export const PoolConnection: any;
  export type PoolConnection<T = any, U = any, V = any, W = any> = any;
  export const QueryOptions: any;
  export type QueryOptions<T = any, U = any, V = any, W = any> = any;
  export const ResultSetHeader: any;
  export type ResultSetHeader<T = any, U = any, V = any, W = any> = any;
  export const RowDataPacket: any;
  export type RowDataPacket<T = any, U = any, V = any, W = any> = any;
}

declare module 'node:events' {
  export const once: any;
  export type once<T = any, U = any, V = any, W = any> = any;
}

declare module 'pg' {
  export const Client: any;
  export type Client<T = any, U = any, V = any, W = any> = any;
  export const PoolClient: any;
  export type PoolClient<T = any, U = any, V = any, W = any> = any;
  export const QueryArrayConfig: any;
  export type QueryArrayConfig<T = any, U = any, V = any, W = any> = any;
  export const QueryConfig: any;
  export type QueryConfig<T = any, U = any, V = any, W = any> = any;
  export const QueryResult: any;
  export type QueryResult<T = any, U = any, V = any, W = any> = any;
  export const QueryResultRow: any;
  export type QueryResultRow<T = any, U = any, V = any, W = any> = any;
}

declare module 'postgres' {
  export const Row: any;
  export type Row<T = any, U = any, V = any, W = any> = any;
  export const RowList: any;
  export type RowList<T = any, U = any, V = any, W = any> = any;
  export const Sql: any;
  export type Sql<T = any, U = any, V = any, W = any> = any;
  export const TransactionSql: any;
  export type TransactionSql<T = any, U = any, V = any, W = any> = any;
}

declare module 'react' {
  export const useEffect: any;
  export type useEffect<T = any, U = any, V = any, W = any> = any;
  export const useReducer: any;
  export type useReducer<T = any, U = any, V = any, W = any> = any;
  export const useState: any;
  export type useState<T = any, U = any, V = any, W = any> = any;
}

declare module 'sql.js' {
  export const BindParams: any;
  export type BindParams<T = any, U = any, V = any, W = any> = any;
  export const Database: any;
  export type Database<T = any, U = any, V = any, W = any> = any;
}
TYPES

  cat > "$fixture_dir/node_modules/bun-types/index.d.ts" <<'TYPES'
// Package marker for `/// <reference types="bun-types" />`.
TYPES

  cat > "$fixture_dir/node_modules/@cloudflare/workers-types/index.d.ts" <<'TYPES'
interface D1Database {
  [key: string]: any;
}

interface D1PreparedStatement {
  [key: string]: any;
  bind(...values: any[]): D1PreparedStatement;
}

interface D1Result<T = unknown> {
  [key: string]: any;
  results: T[];
}
TYPES
}

tsz_write_drizzle_orm_config() {
  # drizzle-orm imports sibling modules with explicit `.ts` extensions and its
  # upstream tsconfig permits them with allowImportingTsExtensions plus noEmit.
  # It also maps `~/*` to package-internal imports. This generated config lives
  # one directory above drizzle-orm's own tsconfig, so the target is rooted at
  # drizzle-orm/src rather than upstream's config-relative src. The fixture does
  # not install the optional driver dependency graph, so unknown package imports
  # use local any stubs while package-internal paths still bind source.
  tsz_write_drizzle_orm_external_stubs "$1"
  tsz_write_basic_external_project_config "$1" "drizzle-orm/src" \
    '    "baseUrl": ".",
    "paths": {
      "~/*": ["drizzle-orm/src/*"],
      "*": ["tsz-bench-external-module.d.ts"]
    },
    "allowImportingTsExtensions": true,
    "ignoreDeprecations": "6.0",
' \
    ', "tsz-bench-external-named-modules.d.ts"'
}

tsz_write_ts_rest_config() {
  tsz_write_basic_external_project_config "$1" "libs/ts-rest/core/src"
}

tsz_write_ofetch_config() {
  # ofetch's source imports sibling modules with explicit `.ts` extensions
  # (e.g. `export * from "./base.ts";`), which its real tsconfig.json permits
  # via allowImportingTsExtensions: true (paired with noEmit, as here). Without
  # the option tsc emits TS5097 on every such import and cascades into
  # TS2304/TS2339; tsz matches that tsc behavior, so the guard config must
  # carry the option the upstream project actually sets.
  tsz_write_basic_external_project_config "$1" "src" \
    '    "allowImportingTsExtensions": true,
'
}

tsz_write_ts_pattern_config() {
  tsz_write_basic_external_project_config "$1" "src"
}

tsz_write_trpc_external_stubs() {
  # trpc's real root tsconfig sets `"types": ["node", "vitest/globals"]`, so the
  # upstream build resolves `NodeJS.Timeout` (used in packages/server/src/adapters/ws.ts)
  # via @types/node. The shared bench baseline pins `"types": []` and the fixture
  # clone has no node_modules, so without a stub tsz/tsc both emit spurious
  # TS2833 "Cannot find namespace 'NodeJS'". Mirror the kysely/type-graphql stub
  # pattern: alias the referenced members as `= any` so the global resolves
  # without unmasking unrelated assignability diffs.
  local output="$1"
  local fixture_dir
  fixture_dir="$(dirname "$output")"
  cat > "$fixture_dir/tsz-bench-globals.d.ts" <<'TYPES'
declare namespace NodeJS {
  type Timeout = any;
  type Timer = any;
  type Immediate = any;
  type ErrnoException = any;
}
TYPES
}

tsz_write_trpc_config() {
  tsz_write_trpc_external_stubs "$1"
  tsz_write_basic_external_project_config "$1" "packages/server/src" "" \
    ', "tsz-bench-globals.d.ts"'
}
tsz_write_tanstack_query_config() {
  tsz_write_basic_external_project_config "$1" "packages/query-core/src"
}
tsz_write_tanstack_router_config() {
  tsz_write_basic_external_project_config "$1" "packages/router-core/src"
}
tsz_write_zustand_config() {
  # zustand imports sibling modules with explicit `.ts` extensions; its real
  # tsconfig sets allowImportingTsExtensions: true (paired with noEmit).
  tsz_write_basic_external_project_config "$1" "src" \
    '    "allowImportingTsExtensions": true,
'
}
tsz_write_jotai_config() {
  # jotai imports sibling modules with explicit `.ts` extensions; its real
  # tsconfig sets allowImportingTsExtensions: true (paired with noEmit).
  tsz_write_basic_external_project_config "$1" "src" \
    '    "allowImportingTsExtensions": true,
'
}
tsz_write_fp_ts_config() {
  tsz_write_basic_external_project_config "$1" "src"
}
tsz_write_io_ts_config() {
  tsz_write_basic_external_project_config "$1" "src"
}
tsz_write_immer_config() {
  # immer imports sibling modules with explicit `.ts` extensions; its real
  # tsconfig sets allowImportingTsExtensions: true (paired with noEmit).
  tsz_write_basic_external_project_config "$1" "src" \
    '    "allowImportingTsExtensions": true,
'
}
tsz_write_remeda_config() {
  tsz_write_basic_external_project_config "$1" "packages/remeda/src"
}
tsz_write_ts_morph_config() {
  tsz_write_basic_external_project_config "$1" "packages/ts-morph/src"
}
tsz_write_arktype_config() {
  # arktype imports sibling modules with explicit `.ts` extensions; its real
  # tsconfig sets allowImportingTsExtensions: true (paired with noEmit).
  tsz_write_basic_external_project_config "$1" "ark/type" \
    '    "allowImportingTsExtensions": true,
'
}
tsz_write_superstruct_config() {
  tsz_write_basic_external_project_config "$1" "src"
}
tsz_write_runtypes_config() {
  # runtypes imports sibling modules with explicit `.ts` extensions; its real
  # tsconfig sets allowImportingTsExtensions: true (paired with noEmit).
  tsz_write_basic_external_project_config "$1" "src" \
    '    "allowImportingTsExtensions": true,
'
}
tsz_write_hotscript_config() {
  tsz_write_basic_external_project_config "$1" "src"
}
tsz_write_typebox_config() {
  tsz_write_basic_external_project_config "$1" "src"
}
tsz_write_class_transformer_config() {
  tsz_write_basic_external_project_config "$1" "src"
}
tsz_write_type_graphql_external_stubs() {
  local output="$1"
  local fixture_dir
  fixture_dir="$(dirname "$output")"

  cat > "$fixture_dir/tsz-bench-external-module.d.ts" <<'TYPES'
declare const tszBenchExternalModule: any;
export = tszBenchExternalModule;
TYPES

  cat > "$fixture_dir/tsz-bench-external-named-modules.d.ts" <<'TYPES'
declare module 'graphql' {
  export const GraphQLSchema: any;
  export type GraphQLSchema = any;
  export const GraphQLObjectType: any;
  export type GraphQLObjectType = any;
  export const GraphQLInputObjectType: any;
  export type GraphQLInputObjectType = any;
  export const GraphQLInterfaceType: any;
  export type GraphQLInterfaceType = any;
  export const GraphQLUnionType: any;
  export type GraphQLUnionType = any;
  export const GraphQLEnumType: any;
  export type GraphQLEnumType = any;
  export const GraphQLScalarType: any;
  export type GraphQLScalarType = any;
  export const GraphQLField: any;
  export type GraphQLField<T = any, U = any, V = any> = any;
  export const GraphQLFieldConfig: any;
  export type GraphQLFieldConfig<T = any, U = any, V = any> = any;
  export const GraphQLFieldConfigMap: any;
  export type GraphQLFieldConfigMap<T = any, U = any> = any;
  export const GraphQLInputFieldConfig: any;
  export type GraphQLInputFieldConfig = any;
  export const GraphQLInputFieldConfigMap: any;
  export type GraphQLInputFieldConfigMap = any;
  export const GraphQLArgument: any;
  export type GraphQLArgument = any;
  export const GraphQLArgumentConfig: any;
  export type GraphQLArgumentConfig = any;
  export const GraphQLEnumValueConfigMap: any;
  export type GraphQLEnumValueConfigMap = any;
  export const GraphQLIsTypeOfFn: any;
  export type GraphQLIsTypeOfFn<T = any, U = any> = any;
  export const GraphQLResolveInfo: any;
  export type GraphQLResolveInfo = any;
  export const GraphQLOutputType: any;
  export type GraphQLOutputType = any;
  export const GraphQLInputType: any;
  export type GraphQLInputType = any;
  export const GraphQLNamedType: any;
  export type GraphQLNamedType = any;
  export const GraphQLNonNull: any;
  export type GraphQLNonNull<T = any> = any;
  export const GraphQLList: any;
  export type GraphQLList<T = any> = any;
  export const GraphQLNullableType: any;
  export type GraphQLNullableType = any;
  export const GraphQLType: any;
  export type GraphQLType = any;
  export const GraphQLTypeResolver: any;
  export type GraphQLTypeResolver<T = any, U = any> = any;
  export const GraphQLFieldResolver: any;
  export type GraphQLFieldResolver<T = any, U = any, V = any, W = any> = any;
  export const GraphQLString: any;
  export const GraphQLInt: any;
  export const GraphQLFloat: any;
  export const GraphQLBoolean: any;
  export const GraphQLID: any;
  export const execute: any;
  export const parse: any;
  export const buildSchema: any;
  export const printSchema: any;
  export const getIntrospectionQuery: any;
  export const introspectionFromSchema: any;
  export const IntrospectionQuery: any;
  export type IntrospectionQuery = any;
  export const Source: any;
  export type Source = any;
  export const DocumentNode: any;
  export type DocumentNode = any;
  export const GraphQLFormattedError: any;
  export type GraphQLFormattedError = any;
  export const GraphQLError: any;
  export type GraphQLError = any;
}

declare module 'graphql/type' {
  export const GraphQLObjectType: any;
  export type GraphQLObjectType = any;
  export const GraphQLInputObjectType: any;
  export type GraphQLInputObjectType = any;
  export const GraphQLInterfaceType: any;
  export type GraphQLInterfaceType = any;
  export const GraphQLUnionType: any;
  export type GraphQLUnionType = any;
}

declare module 'graphql/language' {
  export const DirectiveLocation: any;
  export type DirectiveLocation = any;
}

declare module 'reflect-metadata' {}

declare module 'semver' {
  export const gte: any;
  export type gte = any;
}

declare module 'glob' {
  export const sync: any;
  export type sync = any;
}

declare module 'class-validator' {
  export const ValidatorOptions: any;
  export type ValidatorOptions = any;
  export const validate: any;
  export type validate = any;
}

declare namespace NodeJS {
  type ErrnoException = any;
  type Timeout = any;
  interface ProcessEnv { [k: string]: string | undefined }
}
declare var global: any;
TYPES
}

tsz_write_type_graphql_config() {
  # type-graphql uses `@/` path aliases (baseUrl-relative imports) and depends
  # on graphql, reflect-metadata, and other external packages. Provide path
  # mapping plus module stubs so tsc resolves without installing the full
  # dependency graph.
  tsz_write_type_graphql_external_stubs "$1"
  tsz_write_basic_external_project_config "$1" "src" \
    '    "baseUrl": ".",
    "ignoreDeprecations": "6.0",
    "paths": {
      "@/*": ["src/*"],
      "*": ["tsz-bench-external-module.d.ts"]
    },
    "experimentalDecorators": true,
    "emitDecoratorMetadata": true,
' \
    ', "tsz-bench-external-named-modules.d.ts"'
}
tsz_write_neverthrow_config() {
  tsz_write_basic_external_project_config "$1" "src"
}
tsz_write_xstate_config() {
  # xstate imports sibling modules with explicit `.ts` extensions; its real
  # tsconfig sets allowImportingTsExtensions: true (paired with noEmit).
  tsz_write_basic_external_project_config "$1" "packages/core/src" \
    '    "allowImportingTsExtensions": true,
'
}
tsz_write_mobx_config() {
  tsz_write_basic_external_project_config "$1" "packages/mobx/src"
}

# The full Next.js row uses a sparse source checkout, not an installed Next.js
# monorepo. Keep a bench-owned config so tsc/tsgo can validate the source graph
# without requiring vendored compiled packages, React, Jest, or Node typings.
tsz_write_nextjs_bench_globals() {
  local output="$1"
  cat > "$output" <<'TYPES'
declare const process: any;
declare const require: any;
declare const __dirname: string;
declare const __filename: string;
declare const global: any;

declare module '*' {
  const defaultExport: any;
  export default defaultExport;
}

declare module '*.json' {
  const value: any;
  export default value;
}
TYPES
}

tsz_write_nextjs_config() {
  local output="$1"
  cat > "$output" <<'JSON'
{
  "extends": "./tsconfig.json",
  "compilerOptions": {
    "noEmit": true,
    "noCheck": true,
    "skipLibCheck": true,
    "ignoreDeprecations": "6.0",
    "target": "ES2020",
    "lib": ["DOM", "DOM.Iterable", "ES2020"],
    "types": [],
    "paths": {
      "next/dist/compiled/*": ["./tsz-bench-external-module.d.ts"],
      "next/dist/*": ["./src/*"],
      "*": ["./tsz-bench-external-module.d.ts"]
    }
  },
  "include": [
    "src/**/*.ts",
    "src/**/*.tsx",
    "tsz-bench-globals.d.ts",
    "tsz-bench-external-module.d.ts"
  ],
  "exclude": [
    "src/**/*.test.ts",
    "src/**/*.test.tsx",
    "src/**/*.stories.ts",
    "src/**/*.stories.tsx",
    "src/**/__tests__/**",
    "src/**/__mocks__/**"
  ]
}
JSON
}

tsz_write_nextjs_external_module() {
  local output="$1"
  cat > "$output" <<'TYPES'
declare const value: any;
export default value;
TYPES
}

tsz_write_type_challenges_solutions_config() {
  local source_dir="$1"
  local compile_dir="$2"

  # Preserve the existing manifest JSON across runs so that
  # type-challenges-solutions-manifest.mjs can reuse cached outputSha256,
  # declarations, and semanticFamilies for unchanged source entries.
  # Only the solutions directory is selectively cleaned below.
  mkdir -p "$compile_dir/solutions"

  local generated=0
  local manifest_tsv="$compile_dir/type-challenges-solutions-manifest.tsv"
  local manifest_json="$compile_dir/type-challenges-solutions-manifest.json"
  printf 'output\tsource\tsourceSha256\tid\tlevel\ttitle\n' > "$manifest_tsv"

  # Build a cache of sourceSha256 values from the existing manifest so we can
  # skip re-extracting solution code for unchanged source files. Keep the cache
  # in a TSV temp file instead of a Bash associative array so this script still
  # runs under macOS Bash 3.2 in local lint.
  local _tsz_source_sha_cache_tsv
  _tsz_source_sha_cache_tsv="$(mktemp "${TMPDIR:-/tmp}/tsz-source-sha-cache.XXXXXX")"
  : > "$_tsz_source_sha_cache_tsv"
  if [[ -f "$manifest_json" ]] && command -v node >/dev/null 2>&1; then
    local _cache_raw _cache_ref
    # Single Node.js invocation: first line = source.ref, rest = stem<TAB>sha pairs.
    # Two-call pattern avoided to halve process-start and JSON-parse overhead.
    _cache_raw="$(
      MANIFEST_FILE="$manifest_json" node --input-type=module <<'NODE'
import fs from "node:fs";
try {
  const m = JSON.parse(fs.readFileSync(process.env.MANIFEST_FILE, "utf8"));
  process.stdout.write((m.source?.ref ?? "") + "\n");
  for (const e of (m.entries ?? [])) {
    const stem = e.challenge?.sourceStem;
    const sha = e.challenge?.sourceSha256;
    if (stem && sha) process.stdout.write(stem + "\t" + sha + "\n");
  }
} catch {}
NODE
    )"
    IFS= read -r _cache_ref <<< "$_cache_raw"
    if [[ "$_cache_ref" == "$TYPE_CHALLENGES_SOLUTIONS_REF" ]]; then
      printf '%s\n' "${_cache_raw#*$'\n'}" > "$_tsz_source_sha_cache_tsv"
    fi
  fi

  local markdown
  while IFS= read -r markdown; do
    local id title level base output tmp source_sha256
    id="$(awk -F': ' '/^id: / { print $2; exit }' "$markdown")"
    title="$(awk -F': ' '/^title: / { print $2; exit }' "$markdown")"
    level="$(awk -F': ' '/^level: / { print $2; exit }' "$markdown")"
    base="$(basename "$markdown" .md)"
    output="$compile_dir/solutions/${base}.ts"
    tmp="$compile_dir/solutions/.${base}.tmp"
    source_sha256="$(perl -MDigest::SHA=sha256_hex -0777 -ne 'print sha256_hex($_)' "$markdown")"

    # Skip extraction when output is current: source SHA unchanged and file exists.
    local cached_source_sha256
    cached_source_sha256="$(
      awk -F '\t' -v stem="$base" '$1 == stem { print $2; exit }' \
        "$_tsz_source_sha_cache_tsv"
    )"
    if [[ -f "$output" ]] && [[ "$cached_source_sha256" == "$source_sha256" ]]; then
      generated=$((generated + 1))
      printf '%s\t%s\t%s\t%s\t%s\t%s\n' \
        "solutions/${base}.ts" \
        "en/${base}.md" \
        "$source_sha256" \
        "$id" \
        "$level" \
        "${title//$'\t'/ }" \
        >> "$manifest_tsv"
      continue
    fi

    perl -0ne '
      my ($solution) = /## Solution\n(.*?)(?:\n## References|\z)/s;
      next unless defined $solution;

      my @order;
      my %block_by_name;
      while ($solution =~ /```(?:ts|typescript)\n(.*?)```/sg) {
        my $block = $1;
        my @names;
        while ($block =~ /^\s*(?:export\s+)?(?:declare\s+)?(?:type|interface|namespace)\s+([A-Za-z_\$][A-Za-z0-9_\$]*)/mg) {
          push @names, $1;
        }
        while ($block =~ /^\s*declare\s+(?:function|const)\s+([A-Za-z_\$][A-Za-z0-9_\$]*)/mg) {
          push @names, $1;
        }
        next unless @names;

        for my $name (@names) {
          push @order, $name unless exists $block_by_name{$name};
          $block_by_name{$name} = $block;
        }
      }

      my %emitted;
      for my $name (@order) {
        next if $emitted{$block_by_name{$name}}++;
        print $block_by_name{$name};
        print "\n" unless $block_by_name{$name} =~ /\n\z/;
      }
    ' "$markdown" > "$tmp"

    if [[ ! -s "$tmp" ]]; then
      rm -f "$tmp"
      continue
    fi

    {
      printf '// Generated from ghaiklor/type-challenges-solutions %s\n' "$TYPE_CHALLENGES_SOLUTIONS_REF"
      printf '// Source: en/%s.md\n' "$base"
      printf '// Challenge id: %s; level: %s; title: %s\n\n' "$id" "$level" "$title"
      cat "$tmp"
      printf '\nexport {};\n'
    } > "$output"
    rm -f "$tmp"
    generated=$((generated + 1))
    printf '%s\t%s\t%s\t%s\t%s\t%s\n' \
      "solutions/${base}.ts" \
      "en/${base}.md" \
      "$source_sha256" \
      "$id" \
      "$level" \
      "${title//$'\t'/ }" \
      >> "$manifest_tsv"
  done < <(find "$source_dir/en" -maxdepth 1 -name '*.md' ! -name 'index.md' | sort)

  # Remove stale solution files from sources that no longer exist.
  local _tsz_current_outputs_tsv
  _tsz_current_outputs_tsv="$(mktemp "${TMPDIR:-/tmp}/tsz-current-outputs.XXXXXX")"
  awk -F '\t' 'NR > 1 && $1 != "" { print $1 }' "$manifest_tsv" > "$_tsz_current_outputs_tsv"
  local _ts_file
  while IFS= read -r _ts_file; do
    local _ts_rel="solutions/$(basename "$_ts_file")"
    grep -Fxq "$_ts_rel" "$_tsz_current_outputs_tsv" || rm -f "$_ts_file"
  done < <(find "$compile_dir/solutions" -maxdepth 1 -name '*.ts' 2>/dev/null)
  rm -f "$_tsz_source_sha_cache_tsv" "$_tsz_current_outputs_tsv"

  if [[ "$generated" -eq 0 ]]; then
    echo "error: no Type Challenges solution sources were generated from $source_dir/en" >&2
    return 1
  fi
  if [[ "$generated" -ne "$TYPE_CHALLENGES_SOLUTIONS_EXPECTED_GENERATED" ]]; then
    echo "error: generated ${generated} Type Challenges solution sources; expected ${TYPE_CHALLENGES_SOLUTIONS_EXPECTED_GENERATED} for ${TYPE_CHALLENGES_SOLUTIONS_REF}" >&2
    return 1
  fi

  TYPE_CHALLENGES_SOLUTIONS_REPO="$TYPE_CHALLENGES_SOLUTIONS_REPO" \
  TYPE_CHALLENGES_SOLUTIONS_REF="$TYPE_CHALLENGES_SOLUTIONS_REF" \
  TYPE_CHALLENGES_SOLUTIONS_EXPECTED_GENERATED="$TYPE_CHALLENGES_SOLUTIONS_EXPECTED_GENERATED" \
  node "$TSZ_PROJECT_FIXTURES_ROOT/scripts/ci/type-challenges-solutions-manifest.mjs" \
    "$manifest_tsv" \
    "$manifest_json"
  rm -f "$manifest_tsv"

  cat > "$compile_dir/type-challenges-globals.d.ts" <<'TYPES'
type Equal<X, Y> =
  (<T>() => T extends X ? 1 : 2) extends
  (<T>() => T extends Y ? 1 : 2)
    ? true
    : false;

interface TreeNode {
  val: number;
  left: TreeNode | null;
  right: TreeNode | null;
}
TYPES

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
    "ignoreDeprecations": "6.0"
  },
  "include": ["solutions/**/*.ts", "type-challenges-globals.d.ts"]
}
JSON
}
