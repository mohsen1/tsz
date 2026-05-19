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
TYPE_CHALLENGES_SOLUTIONS_MANIFEST_WRITTEN=0

# shellcheck source=scripts/bench/project-fixtures.sh
source "$ROOT_DIR/scripts/bench/project-fixtures.sh"
tsz_sync_project_row_groups
if command -v node >/dev/null 2>&1; then
  tsz_validate_project_row_metadata
fi

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
  local tsc_exit_codes="${11:-}"
  local fixture_sources
  fixture_sources="$(tsz_project_fixture_sources "$name")"

  COMPAT_JSONL_FILE="$PROJECT_COMPATIBILITY_JSONL" \
  COMPAT_OUTPUT_ROOT="$FIXTURE_ROOT" \
  COMPAT_NAME="$name" \
  COMPAT_EXIT_CLASS="$exit_class" \
  COMPAT_PHASE="$phase" \
  COMPAT_DIAGNOSTIC_STATUS="$diagnostic_status" \
  COMPAT_DIAGNOSTIC_DELTA="$diagnostic_delta" \
  COMPAT_FILES_REACHED="$files_reached" \
  COMPAT_PEAK_MEMORY_BYTES="$peak_memory_bytes" \
  COMPAT_TSZ_EXIT_CODES="$tsz_exit_codes" \
  COMPAT_TSC_EXIT_CODES="$tsc_exit_codes" \
  COMPAT_TSCONFIG_PATH="$tsconfig_path" \
  COMPAT_SOURCE_ROOT="$source_root" \
  COMPAT_FIXTURE_ROOT="$FIXTURE_ROOT" \
  COMPAT_FIXTURE_SOURCES="$fixture_sources" \
  node scripts/ci/project-compatibility.mjs record
}

write_project_compatibility_summary() {
  SUMMARY_JSONL_FILE="$PROJECT_COMPATIBILITY_JSONL" \
  SUMMARY_OUTPUT_FILE="$PROJECT_COMPATIBILITY_SUMMARY" \
  SUMMARY_OUTPUT_ROOT="$FIXTURE_ROOT" \
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

write_type_challenges_solutions_config() {
  tsz_write_type_challenges_solutions_config \
    "$FIXTURE_ROOT/type-challenges-solutions" \
    "$FIXTURE_ROOT/type-challenges-solutions/.tsz-compile"

  TYPE_CHALLENGES_SOLUTIONS_MANIFEST_WRITTEN=1
}

type_challenges_tsc_bin() {
  if [[ -n "${TYPE_CHALLENGES_ASSERTION_TSC_BIN+x}" ]]; then
    if [[ -x "$TYPE_CHALLENGES_ASSERTION_TSC_BIN" ]]; then
      printf '%s\n' "$TYPE_CHALLENGES_ASSERTION_TSC_BIN"
    fi
    return 0
  fi

  if [[ -x scripts/node_modules/.bin/tsc ]]; then
    printf '%s\n' "scripts/node_modules/.bin/tsc"
    return 0
  fi
  if [[ -x node_modules/.bin/tsc ]]; then
    printf '%s\n' "node_modules/.bin/tsc"
    return 0
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

check_type_challenges_solutions_tsc_oracle() {
  local tsconfig="$FIXTURE_ROOT/type-challenges-solutions/.tsz-compile/tsconfig.tsz-guard.json"
  local src_dir="$FIXTURE_ROOT/type-challenges-solutions/.tsz-compile/solutions"
  local log="$FIXTURE_ROOT/type-challenges-solutions-project.tsc.log"
  local file_count
  file_count="$(count_ts_files "$src_dir")"

  ensure_type_challenges_assertion_tsc

  local tsc_bin
  tsc_bin="$(type_challenges_tsc_bin)"
  if [[ -z "$tsc_bin" ]]; then
    FAILURES=$((FAILURES + 1))
    record_project_compatibility \
      "type-challenges-solutions-project" \
      "fixture invalid" \
      "fixture setup" \
      "tsc oracle unavailable" \
      "tsc: Type Challenges solutions oracle is unavailable" \
      "$file_count" \
      "" \
      "" \
      "$tsconfig" \
      "$src_dir" \
      "127"
    echo "error: Type Challenges solutions project requires a tsc oracle, but no tsc binary is available" >&2
    return 1
  fi

  local rc=0
  run_with_timeout "$PROJECT_TIMEOUT" "$tsc_bin" --noEmit -p "$tsconfig" >"$log" 2>&1 || rc=$?
  if [[ "$rc" -ne 0 ]]; then
    FAILURES=$((FAILURES + 1))
    local diagnostic_delta
    if [[ "$rc" -eq 124 ]]; then
      diagnostic_delta="tsc: Type Challenges solutions project timed out after ${PROJECT_TIMEOUT}s"
    else
      diagnostic_delta="$(diagnostic_lines_from_file "tsc" "$log")"
    fi
    record_project_compatibility \
      "type-challenges-solutions-project" \
      "fixture invalid" \
      "fixture setup" \
      "tsc fixture failed" \
      "$diagnostic_delta" \
      "$file_count" \
      "$LAST_PEAK_RSS_BYTES" \
      "" \
      "$tsconfig" \
      "$src_dir" \
      "$rc"
    echo "error: type-challenges-solutions-project failed the tsc oracle check" >&2
    sed -n '1,160p' "$log" >&2 || true
    return 1
  fi

  return 0
}

check_project() {
  local name="$1"
  local tsconfig="$2"
  local src_dir="${3:-$(dirname "$tsconfig")}"
  local tsc_exit_codes="${4:-}"
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
      "$src_dir" \
      "$tsc_exit_codes"
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

  record_project_compatibility "$name" "exit success" "check" "none" "" "$file_count" "$LAST_PEAK_RSS_BYTES" "0" "$tsconfig" "$src_dir" "$tsc_exit_codes"
  echo "${name} compiled successfully."
  echo "::endgroup::"
}

should_check_project() {
  local name="$1"
  [[ -z "$PROJECT_FILTER" || "$name" =~ $PROJECT_FILTER ]]
}

run_project_row() {
  local name="$1"

  case "$name" in
    utility-types-project)
      ensure_git_fixture "utility-types" "$UTILITY_TYPES_REPO" "$UTILITY_TYPES_REF" "$FIXTURE_ROOT/utility-types"
      write_utility_types_config
      check_project "$name" "$FIXTURE_ROOT/utility-types/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/utility-types/src"
      ;;
    ts-essentials-project)
      ensure_git_fixture "ts-essentials" "$TS_ESSENTIALS_REPO" "$TS_ESSENTIALS_REF" "$FIXTURE_ROOT/ts-essentials"
      write_ts_essentials_config
      check_project "$name" "$FIXTURE_ROOT/ts-essentials/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/ts-essentials/lib"
      ;;
    rxjs-project)
      ensure_git_fixture "rxjs" "$RXJS_REPO" "$RXJS_REF" "$FIXTURE_ROOT/rxjs"
      write_rxjs_config
      check_project "$name" "$FIXTURE_ROOT/rxjs/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/rxjs/$(tsz_rxjs_src_root "$FIXTURE_ROOT/rxjs")"
      ;;
    type-fest-project)
      ensure_git_fixture "type-fest" "$TYPE_FEST_REPO" "$TYPE_FEST_REF" "$FIXTURE_ROOT/type-fest"
      write_type_fest_config
      check_project "$name" "$FIXTURE_ROOT/type-fest/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/type-fest/source"
      ;;
    ts-toolbelt-project)
      ensure_git_fixture "ts-toolbelt" "$TS_TOOLBELT_REPO" "$TS_TOOLBELT_REF" "$FIXTURE_ROOT/ts-toolbelt"
      write_ts_toolbelt_config
      check_project "$name" "$FIXTURE_ROOT/ts-toolbelt/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/ts-toolbelt/sources"
      ;;
    zod-project)
      ensure_git_fixture "zod" "$ZOD_REPO" "$ZOD_REF" "$FIXTURE_ROOT/zod"
      write_zod_config
      check_project "$name" "$FIXTURE_ROOT/zod/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/zod"
      ;;
    kysely-project)
      ensure_git_fixture "kysely" "$KYSELY_REPO" "$KYSELY_REF" "$FIXTURE_ROOT/kysely"
      write_kysely_config
      check_project "$name" "$FIXTURE_ROOT/kysely/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/kysely/src"
      ;;
    type-challenges-solutions-project)
      ensure_git_fixture "type-challenges-solutions" "$TYPE_CHALLENGES_SOLUTIONS_REPO" "$TYPE_CHALLENGES_SOLUTIONS_REF" "$FIXTURE_ROOT/type-challenges-solutions"
      write_type_challenges_solutions_config
      if check_type_challenges_solutions_tsc_oracle; then
        check_project "$name" "$FIXTURE_ROOT/type-challenges-solutions/.tsz-compile/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/type-challenges-solutions/.tsz-compile/solutions" "0"
      elif [[ "$ALLOW_FAILURES" == "1" ]]; then
        echo "::warning::type-challenges-solutions-project tsc oracle failed; continuing because TSZ_PROJECT_COMPILE_ALLOW_FAILURES=1"
      fi
      ;;
    *)
      echo "error: unknown project row in compile-guard map: $name" >&2
      return 1
      ;;
  esac
}

run_required_projects() {
  local name
  for name in "${TSZ_COMPILE_GUARD_REQUIRED_ROWS[@]}"; do
    if should_check_project "$name"; then
      if ! run_project_row "$name"; then
        return 1
      fi
    fi
  done

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
  local name
  for name in "${TSZ_COMPILE_GUARD_CANARY_ROWS[@]}"; do
    if should_check_project "$name"; then
      if ! run_project_row "$name"; then
        return 1
      fi
    fi
  done
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
  if [[ "$ALLOW_FAILURES" != "1" ]]; then
    exit 1
  fi
fi
