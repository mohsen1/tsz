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
)

TSZ_COMPILE_GUARD_CANARY_ROWS=(
  "ts-toolbelt-project"
  "zod-project"
  "kysely-project"
  "type-challenges-solutions-project"
)

tsz_sync_project_row_groups() {
  if ! command -v node >/dev/null 2>&1; then
    return 0
  fi

  local required_rows
  local canary_rows

  required_rows="$(TSZ_PROJECT_ROWS_MJS="$TSZ_PROJECT_ROWS_MJS" node --input-type=module <<'NODE'
import { pathToFileURL } from "node:url";
const rowModule = await import(pathToFileURL(process.env.TSZ_PROJECT_ROWS_MJS || process.cwd() + "/scripts/bench/project-rows.mjs"));
console.log(rowModule.COMPILE_GUARD_REQUIRED_ROWS.join("\n"));
NODE
  )"
  canary_rows="$(TSZ_PROJECT_ROWS_MJS="$TSZ_PROJECT_ROWS_MJS" node --input-type=module <<'NODE'
import { pathToFileURL } from "node:url";
const rowModule = await import(pathToFileURL(process.env.TSZ_PROJECT_ROWS_MJS || process.cwd() + "/scripts/bench/project-rows.mjs"));
console.log(rowModule.COMPILE_CANARY_PROJECT_ROWS.join("\n"));
NODE
  )"

  TSZ_COMPILE_GUARD_REQUIRED_ROWS=()
  if [ -n "$required_rows" ]; then
    while IFS= read -r row_name; do
      [ -n "$row_name" ] && TSZ_COMPILE_GUARD_REQUIRED_ROWS+=("$row_name")
    done <<< "$required_rows"
  fi

  TSZ_COMPILE_GUARD_CANARY_ROWS=()
  if [ -n "$canary_rows" ]; then
    while IFS= read -r row_name; do
      [ -n "$row_name" ] && TSZ_COMPILE_GUARD_CANARY_ROWS+=("$row_name")
    done <<< "$canary_rows"
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

tsz_load_fixture_pins_from_rows() {
  command -v node >/dev/null 2>&1 || return 0

  local assignments
  assignments="$(TSZ_PROJECT_ROWS_MJS="$TSZ_PROJECT_ROWS_MJS" node --input-type=module <<'NODE'
import { pathToFileURL } from "node:url";
const { PROJECT_ROW_DEFINITIONS } = await import(
  pathToFileURL(process.env.TSZ_PROJECT_ROWS_MJS)
);

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
NODE
  )" || return 0

  local varname value
  while IFS='=' read -r varname value; do
    [ -z "$varname" ] && continue
    if [[ -z "${!varname+x}" ]]; then
      export "$varname=$value"
    fi
  done <<< "$assignments"
}

tsz_load_fixture_pins_from_rows

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
    type-challenges-project)
      printf 'type-challenges|%s|%s\n' "$TYPE_CHALLENGES_REPO" "$TYPE_CHALLENGES_REF"
      ;;
    type-challenges-solutions-project)
      printf 'type-challenges-solutions|%s|%s\n' "$TYPE_CHALLENGES_SOLUTIONS_REPO" "$TYPE_CHALLENGES_SOLUTIONS_REF"
      ;;
    type-challenges-assertion-candidates|type-challenges-assertions-tsc-clean)
      printf 'type-challenges|%s|%s\n' "$TYPE_CHALLENGES_REPO" "$TYPE_CHALLENGES_REF"
      printf 'type-challenges-solutions|%s|%s\n' "$TYPE_CHALLENGES_SOLUTIONS_REPO" "$TYPE_CHALLENGES_SOLUTIONS_REF"
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
    rm -rf "$dir"
    git clone --quiet --no-tags --depth 1 "$repo" "$dir"
  fi

  if [[ "$reclone_dirty" == "1" ]] \
    && [[ -n "$(git -C "$dir" status --porcelain 2>/dev/null)" ]]; then
    echo "${name} fixture is dirty; recloning for reproducibility..."
    rm -rf "$dir"
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

  rm -rf "$compile_dir"
  mkdir -p "$compile_dir/solutions"

  local generated=0
  local manifest_tsv="$compile_dir/type-challenges-solutions-manifest.tsv"
  local manifest_json="$compile_dir/type-challenges-solutions-manifest.json"
  printf 'output\tsource\tid\tlevel\ttitle\n' > "$manifest_tsv"

  local markdown
  while IFS= read -r markdown; do
    local id title level base output tmp
    id="$(awk -F': ' '/^id: / { print $2; exit }' "$markdown")"
    title="$(awk -F': ' '/^title: / { print $2; exit }' "$markdown")"
    level="$(awk -F': ' '/^level: / { print $2; exit }' "$markdown")"
    base="$(basename "$markdown" .md)"
    output="$compile_dir/solutions/${base}.ts"
    tmp="$compile_dir/solutions/.${base}.tmp"

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
    printf '%s\t%s\t%s\t%s\t%s\n' \
      "solutions/${base}.ts" \
      "en/${base}.md" \
      "$id" \
      "$level" \
      "${title//$'\t'/ }" \
      >> "$manifest_tsv"
  done < <(find "$source_dir/en" -maxdepth 1 -name '*.md' ! -name 'index.md' | sort)

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
