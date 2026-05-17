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
PROJECT_COMPATIBILITY_JSONL="${TSZ_PROJECT_COMPILE_COMPATIBILITY_JSONL:-$FIXTURE_ROOT/project-compatibility.jsonl}"
PROJECT_COMPATIBILITY_SUMMARY="${TSZ_PROJECT_COMPILE_COMPATIBILITY_SUMMARY:-$FIXTURE_ROOT/project-compatibility-summary.json}"
FAILURES=0
LAST_PEAK_RSS_BYTES=0
TYPE_CHALLENGES_PROJECT_MANIFESTS_WRITTEN=0
TYPE_CHALLENGES_SOLUTIONS_MANIFEST_WRITTEN=0
TYPE_CHALLENGES_PAIRING_REPORT_WRITTEN=0

# shellcheck source=scripts/bench/project-fixtures.sh
source "$ROOT_DIR/scripts/bench/project-fixtures.sh"

if [[ ! -x "$TSZ_BIN" ]]; then
  echo "error: TSZ_BIN is not executable: $TSZ_BIN" >&2
  exit 1
fi

mkdir -p "$FIXTURE_ROOT"
rm -f "$FIXTURE_ROOT/type-challenges-readiness-pairing.json"
rm -rf "$FIXTURE_ROOT/type-challenges-assertions"
: > "$PROJECT_COMPATIBILITY_JSONL"

run_with_timeout() {
  local timeout_secs="$1"
  shift

  LAST_PEAK_RSS_BYTES=0
  "$@" &
  local pid=$!
  local timeout_file
  timeout_file="$(mktemp)"
  rm -f "$timeout_file"
  local rss_file=""
  local rss_monitor_pid=""
  (
    sleep "$timeout_secs"
    touch "$timeout_file"
    kill -9 "$pid" 2>/dev/null || true
  ) &
  local watchdog_pid=$!
  if measure_peak_rss_enabled; then
    rss_file=$(mktemp)
    printf '0\n' > "$rss_file"
    (
      local peak_kb=0
      local rss_kb
      while kill -0 "$pid" 2>/dev/null; do
        rss_kb="$(process_tree_rss_kb "$pid" || true)"
        if [[ "$rss_kb" =~ ^[0-9]+$ ]] && [ "$rss_kb" -gt "$peak_kb" ]; then
          peak_kb="$rss_kb"
          printf '%s\n' "$((peak_kb * 1024))" > "$rss_file"
        fi
        sleep 1
      done
    ) &
    rss_monitor_pid=$!
  fi

  local exit_code=0
  wait "$pid" 2>/dev/null || exit_code=$?

  local timed_out=0
  if [ -e "$timeout_file" ]; then
    timed_out=1
  fi
  rm -f "$timeout_file"

  kill "$watchdog_pid" 2>/dev/null || true
  wait "$watchdog_pid" 2>/dev/null || true
  if [ -n "$rss_monitor_pid" ]; then
    kill "$rss_monitor_pid" 2>/dev/null || true
    wait "$rss_monitor_pid" 2>/dev/null || true
  fi
  if [ -n "$rss_file" ]; then
    LAST_PEAK_RSS_BYTES="$(cat "$rss_file" 2>/dev/null || echo 0)"
    rm -f "$rss_file"
  fi

  if [[ "$timed_out" -eq 1 && "$exit_code" -eq 137 ]]; then
    return 124
  fi
  return "$exit_code"
}

measure_peak_rss_enabled() {
  case "${TSZ_PROJECT_COMPILE_PEAK_RSS:-}" in
    1|true|TRUE|yes|YES) return 0 ;;
    0|false|FALSE|no|NO) return 1 ;;
  esac

  [ "${CI:-}" = "true" ] && [ "$(uname -s 2>/dev/null || echo unknown)" = "Linux" ]
}

process_tree_rss_kb() {
  local root_pid="$1"

  ps -e -o pid=,ppid=,rss= 2>/dev/null | awk -v root="$root_pid" '
    {
      pid[NR] = $1
      ppid[NR] = $2
      rss[NR] = $3
      count = NR
    }
    END {
      live[root] = 1
      changed = 1
      while (changed) {
        changed = 0
        for (i = 1; i <= count; i += 1) {
          if (live[ppid[i]] && !live[pid[i]]) {
            live[pid[i]] = 1
            changed = 1
          }
        }
      }
      total = 0
      for (i = 1; i <= count; i += 1) {
        if (live[pid[i]]) total += rss[i]
      }
      print total
    }
  '
}

count_ts_files() {
  local src_dir="$1"
  { find "$src_dir" \( -path '*/node_modules/*' -o -path '*/.next/*' \) -prune -o \( -name '*.ts' -o -name '*.tsx' -o -name '*.mts' -o -name '*.cts' \) -type f -print 2>/dev/null || true; } \
    | wc -l | tr -d ' '
}

diagnostic_lines_from_file() {
  local label="$1"
  local file="$2"

  awk -v label="$label" '
    {
      sub(/\r$/, "")
      if ($0 ~ /^[[:space:]]*$/) {
        next
      }
      print label ": " $0
      seen += 1
      if (seen >= 20) {
        exit
      }
    }
  ' "$file" 2>/dev/null || true
}

project_failure_class() {
  local status="$1"
  shift || true

  if [[ "$status" == *"timeout"* ]]; then
    echo "timeout"
    return
  fi

  local code
  for code in "$@"; do
    case "$code" in
      124|142)
        echo "timeout"
        return
        ;;
      137)
        echo "oom"
        return
        ;;
      132|134|136|139)
        echo "crash"
        return
        ;;
    esac
  done

  echo "nonzero exit"
}

project_failure_status() {
  case "$1" in
    timeout) echo "compiler timed out" ;;
    oom) echo "compiler OOM or killed" ;;
    crash) echo "compiler crashed" ;;
    *) echo "diagnostic mismatch or compiler error" ;;
  esac
}

record_project_compatibility() {
  local name="$1"
  local exit_class="$2"
  local phase="$3"
  local diagnostic_status="$4"
  local diagnostic_delta="${5:-}"
  local files_reached="${6:-0}"
  local peak_memory_bytes="${7:-}"
  local tsz_exit_codes="${8:-}"
  local tsconfig_path="${9:-}"
  local source_root="${10:-}"

  COMPAT_JSONL_FILE="$PROJECT_COMPATIBILITY_JSONL" \
  COMPAT_NAME="$name" \
  COMPAT_EXIT_CLASS="$exit_class" \
  COMPAT_PHASE="$phase" \
  COMPAT_DIAGNOSTIC_STATUS="$diagnostic_status" \
  COMPAT_DIAGNOSTIC_DELTA="$diagnostic_delta" \
  COMPAT_FILES_REACHED="$files_reached" \
  COMPAT_PEAK_MEMORY_BYTES="$peak_memory_bytes" \
  COMPAT_TSZ_EXIT_CODES="$tsz_exit_codes" \
  COMPAT_TSCONFIG_PATH="$tsconfig_path" \
  COMPAT_SOURCE_ROOT="$source_root" \
  COMPAT_FIXTURE_ROOT="$FIXTURE_ROOT" \
  node scripts/ci/project-compatibility.mjs record
}

write_project_compatibility_summary() {
  SUMMARY_JSONL_FILE="$PROJECT_COMPATIBILITY_JSONL" \
  SUMMARY_OUTPUT_FILE="$PROJECT_COMPATIBILITY_SUMMARY" \
  SUMMARY_PROJECT_SET="$PROJECT_SET" \
  SUMMARY_PROJECT_FILTER="$PROJECT_FILTER" \
  SUMMARY_ALLOW_FAILURES="$ALLOW_FAILURES" \
  SUMMARY_FAILURES="$FAILURES" \
  node scripts/ci/project-compatibility.mjs summary
}

trap write_project_compatibility_summary EXIT

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
  local manifest_json="$compile_dir/type-challenges-template-manifest.json"
  local test_cases_manifest_json="$compile_dir/type-challenges-test-cases-manifest.json"

  rm -rf "$compile_dir"
  mkdir -p "$compile_dir/questions" "$compile_dir/test-cases/questions" "$compile_dir/utils"

  local template
  while IFS= read -r template; do
    local rel="${template#"$source_dir"/}"
    mkdir -p "$compile_dir/$(dirname "$rel")"
    cp "$template" "$compile_dir/$rel"
    printf '\nexport {};\n' >> "$compile_dir/$rel"
  done < <(find "$source_dir/questions" -maxdepth 2 -name template.ts | sort)

  local test_cases
  while IFS= read -r test_cases; do
    local rel="${test_cases#"$source_dir"/}"
    mkdir -p "$compile_dir/test-cases/$(dirname "$rel")"
    cp "$test_cases" "$compile_dir/test-cases/$rel"
  done < <(find "$source_dir/questions" -maxdepth 2 -name test-cases.ts | sort)

  TYPE_CHALLENGES_REPO="$TYPE_CHALLENGES_REPO" \
  TYPE_CHALLENGES_REF="$TYPE_CHALLENGES_REF" \
  TYPE_CHALLENGES_EXPECTED_GENERATED="$TYPE_CHALLENGES_EXPECTED_GENERATED" \
  node scripts/ci/type-challenges-template-manifest.mjs \
    "$source_dir" \
    "$compile_dir" \
    "$manifest_json"

  TYPE_CHALLENGES_REPO="$TYPE_CHALLENGES_REPO" \
  TYPE_CHALLENGES_REF="$TYPE_CHALLENGES_REF" \
  TYPE_CHALLENGES_EXPECTED_TEST_CASES="$TYPE_CHALLENGES_EXPECTED_TEST_CASES" \
  node scripts/ci/type-challenges-test-cases-manifest.mjs \
    "$source_dir" \
    "$compile_dir/test-cases" \
    "$test_cases_manifest_json"

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

  TYPE_CHALLENGES_PROJECT_MANIFESTS_WRITTEN=1
}

write_type_challenges_solutions_config() {
  tsz_write_type_challenges_solutions_config \
    "$FIXTURE_ROOT/type-challenges-solutions" \
    "$FIXTURE_ROOT/type-challenges-solutions/.tsz-compile"

  TYPE_CHALLENGES_SOLUTIONS_MANIFEST_WRITTEN=1
}

write_type_challenges_pairing_report() {
  local template_manifest="$FIXTURE_ROOT/type-challenges/.tsz-compile/type-challenges-template-manifest.json"
  local test_cases_manifest="$FIXTURE_ROOT/type-challenges/.tsz-compile/type-challenges-test-cases-manifest.json"
  local solutions_manifest="$FIXTURE_ROOT/type-challenges-solutions/.tsz-compile/type-challenges-solutions-manifest.json"
  local output="$FIXTURE_ROOT/type-challenges-readiness-pairing.json"

  if [[ "$TYPE_CHALLENGES_PROJECT_MANIFESTS_WRITTEN" == "1" \
    && "$TYPE_CHALLENGES_SOLUTIONS_MANIFEST_WRITTEN" == "1" \
    && -f "$template_manifest" && -f "$test_cases_manifest" && -f "$solutions_manifest" ]]; then
    node scripts/ci/type-challenges-pairing-report.mjs \
      "$template_manifest" \
      "$test_cases_manifest" \
      "$solutions_manifest" \
      "$output"
    TYPE_CHALLENGES_PAIRING_REPORT_WRITTEN=1
  else
    rm -f "$output"
  fi
}

write_type_challenges_assertion_candidates() {
  local pairing_report="$FIXTURE_ROOT/type-challenges-readiness-pairing.json"
  local type_challenges_compile_dir="$FIXTURE_ROOT/type-challenges/.tsz-compile"
  local solutions_compile_dir="$FIXTURE_ROOT/type-challenges-solutions/.tsz-compile"
  local output_dir="$FIXTURE_ROOT/type-challenges-assertions"
  local manifest="$output_dir/type-challenges-assertions-manifest.json"

  if [[ "$TYPE_CHALLENGES_PAIRING_REPORT_WRITTEN" == "1" && -f "$pairing_report" ]]; then
    node scripts/ci/type-challenges-assertion-candidates.mjs \
      "$pairing_report" \
      "$type_challenges_compile_dir" \
      "$solutions_compile_dir" \
      "$output_dir" \
      "$manifest"
  else
    rm -rf "$output_dir"
  fi
}

ensure_type_challenges_assertion_tsc() {
  if [[ -n "${TYPE_CHALLENGES_ASSERTION_TSC_BIN+x}" ]]; then
    return 0
  fi

  if [[ -x scripts/node_modules/.bin/tsc || -x node_modules/.bin/tsc ]]; then
    return 0
  fi

  if ! command -v npm >/dev/null 2>&1; then
    echo "warn: npm not found; Type Challenges assertion classifier will report tsc unavailable" >&2
    return 0
  fi

  echo "Installing scripts Node dependencies for Type Challenges assertion classifier"
  (cd scripts && npm install --silent)
  if [[ ! -x scripts/node_modules/.bin/tsc ]]; then
    echo "warn: scripts Node install did not provide tsc; Type Challenges assertion classifier will report tsc unavailable" >&2
  fi
}

write_type_challenges_assertion_classification() {
  local candidate_dir="$FIXTURE_ROOT/type-challenges-assertions"
  local manifest="$candidate_dir/type-challenges-assertions-manifest.json"
  local output="$candidate_dir/type-challenges-assertions-classification.json"
  local clean_dir="$FIXTURE_ROOT/type-challenges-assertions-tsc-clean"
  local clean_manifest="$clean_dir/type-challenges-assertions-tsc-clean-manifest.json"
  local clean_output="$clean_dir/type-challenges-assertions-tsc-clean-classification.json"

  if [[ -f "$manifest" ]]; then
    ensure_type_challenges_assertion_tsc
    node scripts/ci/type-challenges-assertion-classifier.mjs \
      "$candidate_dir" \
      "$manifest" \
      "$output"
    node scripts/ci/type-challenges-assertion-clean-subset.mjs \
      "$candidate_dir" \
      "$manifest" \
      "$output" \
      "$clean_dir" \
      "$clean_manifest"
    if [[ -f "$clean_manifest" ]] \
      && node -e 'const fs = require("fs"); const manifest = JSON.parse(fs.readFileSync(process.argv[1], "utf8")); process.exit(Number(manifest.counts?.tscAcceptedAssertions || 0) > 0 ? 0 : 1)' "$clean_manifest"; then
      node scripts/ci/type-challenges-assertion-classifier.mjs \
        "$clean_dir" \
        "$clean_manifest" \
        "$clean_output"
    fi
    node scripts/ci/type-challenges-assertion-compatibility.mjs \
      "$output" \
      "$candidate_dir" \
      "$PROJECT_COMPATIBILITY_JSONL" \
      "$FIXTURE_ROOT" \
      "$clean_manifest" \
      "$clean_output" \
      "$clean_dir"
  fi
}

check_project() {
  local name="$1"
  local tsconfig="$2"
  local src_dir="${3:-$(dirname "$tsconfig")}"
  local log="$FIXTURE_ROOT/${name}.log"
  local file_count
  file_count="$(count_ts_files "$src_dir")"

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
    local exit_class
    local diagnostic_delta
    exit_class="$(project_failure_class "$([[ "$rc" -eq 124 ]] && echo "timeout" || echo "nonzero exit")" "$rc")"
    diagnostic_delta="$(diagnostic_lines_from_file "tsz" "$log")"
    record_project_compatibility \
      "$name" \
      "$exit_class" \
      "check" \
      "$(project_failure_status "$exit_class")" \
      "$diagnostic_delta" \
      "$file_count" \
      "$LAST_PEAK_RSS_BYTES" \
      "$rc" \
      "$tsconfig" \
      "$src_dir"
    if [[ "$rc" -eq 124 ]]; then
      echo "error: ${name} timed out after ${PROJECT_TIMEOUT}s" >&2
    else
      echo "error: ${name} failed with exit code ${rc}" >&2
    fi
    sed -n '1,160p' "$log" >&2 || true
    echo "::endgroup::"
    if [[ "$ALLOW_FAILURES" == "1" ]]; then
      echo "::warning::${name} did not compile; continuing because TSZ_PROJECT_COMPILE_ALLOW_FAILURES=1"
    fi
    return 0
  fi

  record_project_compatibility "$name" "exit success" "check" "none" "" "$file_count" "$LAST_PEAK_RSS_BYTES" "0" "$tsconfig" "$src_dir"
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
  check_project "utility-types-project" "$FIXTURE_ROOT/utility-types/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/utility-types/src"
fi

if should_check_project "ts-essentials-project"; then
  ensure_git_fixture "ts-essentials" "$TS_ESSENTIALS_REPO" "$TS_ESSENTIALS_REF" "$FIXTURE_ROOT/ts-essentials"
  write_ts_essentials_config
  check_project "ts-essentials-project" "$FIXTURE_ROOT/ts-essentials/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/ts-essentials/lib"
fi

if should_check_project "rxjs-project"; then
  ensure_git_fixture "rxjs" "$RXJS_REPO" "$RXJS_REF" "$FIXTURE_ROOT/rxjs"
  write_rxjs_config
  check_project "rxjs-project" "$FIXTURE_ROOT/rxjs/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/rxjs/$(tsz_rxjs_src_root "$FIXTURE_ROOT/rxjs")"
fi

if should_check_project "type-fest-project"; then
  ensure_git_fixture "type-fest" "$TYPE_FEST_REPO" "$TYPE_FEST_REF" "$FIXTURE_ROOT/type-fest"
  write_type_fest_config
  check_project "type-fest-project" "$FIXTURE_ROOT/type-fest/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/type-fest/source"
fi

if [[ "$INCLUDE_GENERATED_APPS" == "1" ]] \
  && { should_check_project "vite-vanilla-ts-app" || should_check_project "nextjs-fresh-app"; }; then
  if ! command -v node >/dev/null 2>&1 || ! command -v npm >/dev/null 2>&1; then
    echo "error: node and npm are required for generated app project compile guards" >&2
    exit 1
  fi

  if should_check_project "vite-vanilla-ts-app"; then
    node scripts/bench/generate-vite-app-fixture.mjs "$FIXTURE_ROOT/vite-vanilla-ts-live"
    check_project "vite-vanilla-ts-app" "$FIXTURE_ROOT/vite-vanilla-ts-live/tsconfig.json" "$FIXTURE_ROOT/vite-vanilla-ts-live/src"
  fi

  if should_check_project "nextjs-fresh-app"; then
    node scripts/bench/generate-next-app-fixture.mjs "$FIXTURE_ROOT/next-app-live"
    check_project "nextjs-fresh-app" "$FIXTURE_ROOT/next-app-live/tsconfig.json" "$FIXTURE_ROOT/next-app-live"
  fi
fi
}

run_canary_projects() {
if should_check_project "ts-toolbelt-project"; then
  ensure_git_fixture "ts-toolbelt" "$TS_TOOLBELT_REPO" "$TS_TOOLBELT_REF" "$FIXTURE_ROOT/ts-toolbelt"
  write_ts_toolbelt_config
  check_project "ts-toolbelt-project" "$FIXTURE_ROOT/ts-toolbelt/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/ts-toolbelt/sources"
fi

if should_check_project "zod-project"; then
  ensure_git_fixture "zod" "$ZOD_REPO" "$ZOD_REF" "$FIXTURE_ROOT/zod"
  write_zod_config
  check_project "zod-project" "$FIXTURE_ROOT/zod/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/zod"
fi

if should_check_project "kysely-project"; then
  ensure_git_fixture "kysely" "$KYSELY_REPO" "$KYSELY_REF" "$FIXTURE_ROOT/kysely"
  write_kysely_config
  check_project "kysely-project" "$FIXTURE_ROOT/kysely/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/kysely/src"
fi

if should_check_project "type-challenges-project"; then
  ensure_git_fixture "type-challenges" "$TYPE_CHALLENGES_REPO" "$TYPE_CHALLENGES_REF" "$FIXTURE_ROOT/type-challenges"
  write_type_challenges_config
  check_project "type-challenges-project" "$FIXTURE_ROOT/type-challenges/.tsz-compile/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/type-challenges/.tsz-compile/questions"
fi

if should_check_project "type-challenges-solutions-project"; then
  ensure_git_fixture "type-challenges-solutions" "$TYPE_CHALLENGES_SOLUTIONS_REPO" "$TYPE_CHALLENGES_SOLUTIONS_REF" "$FIXTURE_ROOT/type-challenges-solutions"
  write_type_challenges_solutions_config
  check_project "type-challenges-solutions-project" "$FIXTURE_ROOT/type-challenges-solutions/.tsz-compile/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/type-challenges-solutions/.tsz-compile/solutions"
fi

if should_check_project "type-challenges-assertion-candidates"; then
  ensure_git_fixture "type-challenges" "$TYPE_CHALLENGES_REPO" "$TYPE_CHALLENGES_REF" "$FIXTURE_ROOT/type-challenges"
  write_type_challenges_config
  ensure_git_fixture "type-challenges-solutions" "$TYPE_CHALLENGES_SOLUTIONS_REPO" "$TYPE_CHALLENGES_SOLUTIONS_REF" "$FIXTURE_ROOT/type-challenges-solutions"
  write_type_challenges_solutions_config
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

write_type_challenges_pairing_report
write_type_challenges_assertion_candidates
write_type_challenges_assertion_classification

if [[ "$FAILURES" -gt 0 ]]; then
  echo "Project compile failures: $FAILURES"
  if [[ "$ALLOW_FAILURES" != "1" ]]; then
    exit 1
  fi
fi
