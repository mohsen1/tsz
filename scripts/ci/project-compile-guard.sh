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
RESULT_CACHE_DIR="${TSZ_PROJECT_COMPILE_RESULT_CACHE_DIR:-$FIXTURE_ROOT/.result-cache}"
FAILURES=0
LAST_PEAK_RSS_BYTES=0
TYPE_CHALLENGES_SOLUTIONS_MANIFEST_WRITTEN=0

fail() {
  echo "error: $*" >&2
  exit 1
}

resolve_existing_parent_path() {
  local file="$1"
  local label="$2"
  local parent
  parent="$(dirname "$file")"
  if [[ ! -d "$parent" ]]; then
    fail "$label parent directory does not exist: $file"
  fi
  local parent_abs
  parent_abs="$(cd "$parent" && pwd -P)"
  printf '%s/%s\n' "$parent_abs" "$(basename "$file")"
}

validate_project_compatibility_artifact_paths() {
  local fixture_abs
  fixture_abs="$(cd "$FIXTURE_ROOT" && pwd -P)"

  local jsonl_abs
  local summary_abs
  jsonl_abs="$(resolve_existing_parent_path "$PROJECT_COMPATIBILITY_JSONL" "project compatibility JSONL")"
  summary_abs="$(resolve_existing_parent_path "$PROJECT_COMPATIBILITY_SUMMARY" "project compatibility summary")"

  case "$jsonl_abs" in
    "$fixture_abs"/*) ;;
    *) fail "project compatibility JSONL must stay inside fixture root: $PROJECT_COMPATIBILITY_JSONL" ;;
  esac
  case "$summary_abs" in
    "$fixture_abs"/*) ;;
    *) fail "project compatibility summary must stay inside fixture root: $PROJECT_COMPATIBILITY_SUMMARY" ;;
  esac

  if [[ "$jsonl_abs" == "$summary_abs" ]]; then
    fail "project compatibility JSONL and summary paths must be distinct: $PROJECT_COMPATIBILITY_JSONL"
  fi
  if [[ -e "$jsonl_abs" && ! -f "$jsonl_abs" ]]; then
    fail "project compatibility JSONL path is not a file: $PROJECT_COMPATIBILITY_JSONL"
  fi
  if [[ -e "$summary_abs" && ! -f "$summary_abs" ]]; then
    fail "project compatibility summary path is not a file: $PROJECT_COMPATIBILITY_SUMMARY"
  fi
}

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

# Stable sha256 hash of a file (Linux sha256sum / macOS shasum).
sha256_of_file() {
  sha256sum "$1" 2>/dev/null | awk '{print $1}' \
    || shasum -a 256 "$1" 2>/dev/null | awk '{print $1}' || true
}

# Compute tsz binary hash once at startup for all per-project fingerprints.
_TSZ_BINARY_HASH="$(sha256_of_file "$TSZ_BIN")"

mkdir -p "$FIXTURE_ROOT"
mkdir -p "$RESULT_CACHE_DIR"
validate_project_compatibility_artifact_paths
rm -f "$FIXTURE_ROOT/type-challenges-readiness-pairing.json"
rm -rf "$FIXTURE_ROOT/type-challenges-assertions"
: > "$PROJECT_COMPATIBILITY_JSONL"

run_with_timeout() {
  local timeout_secs="$1"
  shift

  # Empty (not "0") is the "no positive sample yet" sentinel so the
  # record-time reason logic can distinguish it from a deliberate zero.
  LAST_PEAK_RSS_BYTES=""
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
    : > "$rss_file"
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
    LAST_PEAK_RSS_BYTES="$(cat "$rss_file" 2>/dev/null || true)"
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

# Echoes the structured reason peak-RSS sampling did not produce a value, or
# empty when sampling is active (in which case a missing value means the
# process exited before the first sample). Reasons must be in the closed
# vocabulary documented in scripts/ci/project-compatibility.mjs.
peak_rss_unavailable_reason() {
  case "${TSZ_PROJECT_COMPILE_PEAK_RSS:-}" in
    0|false|FALSE|no|NO)
      printf 'measurement disabled\n'
      return
      ;;
    1|true|TRUE|yes|YES)
      return
      ;;
  esac

  if [ "${CI:-}" != "true" ] || [ "$(uname -s 2>/dev/null || echo unknown)" != "Linux" ]; then
    printf 'not measured on platform\n'
  fi
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

  local peak_memory_bytes_reason=""
  if [ -z "$peak_memory_bytes" ]; then
    peak_memory_bytes_reason="$(peak_rss_unavailable_reason)"
    if [ -z "$peak_memory_bytes_reason" ]; then
      peak_memory_bytes_reason="process exited before sampling"
    fi
  fi

  COMPAT_JSONL_FILE="$PROJECT_COMPATIBILITY_JSONL" \
  COMPAT_OUTPUT_ROOT="$FIXTURE_ROOT" \
  COMPAT_NAME="$name" \
  COMPAT_EXIT_CLASS="$exit_class" \
  COMPAT_PHASE="$phase" \
  COMPAT_DIAGNOSTIC_STATUS="$diagnostic_status" \
  COMPAT_DIAGNOSTIC_DELTA="$diagnostic_delta" \
  COMPAT_FILES_REACHED="$files_reached" \
  COMPAT_PEAK_MEMORY_BYTES="$peak_memory_bytes" \
  COMPAT_PEAK_MEMORY_BYTES_REASON="$peak_memory_bytes_reason" \
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

ensure_generated_app_tools() {
  if ! command -v node >/dev/null 2>&1 || ! command -v npm >/dev/null 2>&1; then
    echo "error: node and npm are required for generated app project compile guards" >&2
    exit 1
  fi
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
  (cd scripts && npm install --silent --include=dev)
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

# Atomically writes a compile result to the result cache (tmp-then-rename).
# The fingerprint is stored inside the file so stale entries self-prune on the
# next read rather than accumulating one file per fingerprint per project.
# write_compile_cache <fp> <rc> <file_count> <exit_class> <diagnostic_delta> <cache_file>
write_compile_cache() {
  local _fp="$1" _rc="$2" _files="$3" _class="$4" _delta="$5" _cf="$6"
  { printf 'FINGERPRINT=%s\nRC=%s\nFILES=%s\nCLASS=%s\nDELTA_START\n' \
      "$_fp" "$_rc" "$_files" "$_class"
    printf '%s' "$_delta"; } \
    > "${_cf}.tmp" 2>/dev/null \
    && mv "${_cf}.tmp" "$_cf" 2>/dev/null || true
}

# Fingerprint for a check_project invocation.
# Key: tsz binary hash (computed once at startup) + tsconfig content + fixture git HEAD.
# Returns empty on failure — callers treat empty as "caching unavailable".
compute_compile_fingerprint() {
  local name="$1" tsconfig="$2"
  # The key is composed of fixed-length hashes — no need to hash it again.
  # Return empty if critical components are missing so callers disable caching.
  [[ -z "$_TSZ_BINARY_HASH" ]] && return
  local tsconfig_hash=""
  [[ -f "$tsconfig" ]] && tsconfig_hash="$(sha256_of_file "$tsconfig")"
  [[ -f "$tsconfig" && -z "$tsconfig_hash" ]] && return
  # git -C already performs the upward .git search, handles worktrees (.git files),
  # and returns empty (non-zero exit suppressed) when outside any repo.
  local git_ref=""
  git_ref="$(git -C "$(dirname "$tsconfig")" rev-parse HEAD 2>/dev/null || true)"
  printf '%s' "${name}|${_TSZ_BINARY_HASH}|${tsconfig_hash}|${git_ref}"
}

check_project() {
  local name="$1"
  local tsconfig="$2"
  local src_dir="${3:-$(dirname "$tsconfig")}"
  local tsc_exit_codes="${4:-}"
  local log="$FIXTURE_ROOT/${name}.log"

  # Result cache: skip recompilation when tsz binary, tsconfig content, and
  # fixture git HEAD are all unchanged from a prior run. The cache file is named
  # per-project; the stored fingerprint is validated on read so stale entries are
  # overwritten rather than accumulated. Disable with TSZ_PROJECT_COMPILE_RESULT_CACHE=0.
  local _fp="" _cache_file=""
  if [[ "${TSZ_PROJECT_COMPILE_RESULT_CACHE:-1}" == "1" ]]; then
    _fp="$(compute_compile_fingerprint "$name" "$tsconfig" 2>/dev/null || true)"
    [[ -n "$_fp" ]] && _cache_file="$RESULT_CACHE_DIR/${name}"
  fi

  if [[ -n "$_cache_file" && -f "$_cache_file" ]]; then
    local _cached_fp="" _cached_rc="" _cached_files="" _cached_class="" _cached_delta="" _in_delta=0
    local _line
    while IFS= read -r _line; do
      if [[ "$_in_delta" == "1" ]]; then
        _cached_delta="${_cached_delta}${_line}"$'\n'
      else
        case "$_line" in
          FINGERPRINT=*)
            _cached_fp="${_line#FINGERPRINT=}"
            # Bail early on mismatch — avoids reading the full delta body.
            [[ "$_cached_fp" != "$_fp" ]] && break
            ;;
          RC=*)          _cached_rc="${_line#RC=}" ;;
          FILES=*)       _cached_files="${_line#FILES=}" ;;
          CLASS=*)       _cached_class="${_line#CLASS=}" ;;
          DELTA_START)   _in_delta=1 ;;
        esac
      fi
    done < "$_cache_file"
    if [[ "$_cached_fp" == "$_fp" ]]; then
      echo "::group::${name}"
      echo "(result cache hit: ${_fp:0:12})"
      if [[ "${_cached_rc:-1}" -ne 0 ]]; then
        FAILURES=$((FAILURES + 1))
        record_project_compatibility \
          "$name" "${_cached_class:-nonzero exit}" "check" \
          "$(project_failure_status "${_cached_class:-nonzero exit}")" \
          "${_cached_delta:-}" "${_cached_files:-0}" "" \
          "${_cached_rc:-1}" "$tsconfig" "$src_dir" "$tsc_exit_codes"
        echo "error: ${name} failed (cached result)" >&2
        echo "::endgroup::"
        if [[ "$ALLOW_FAILURES" == "1" ]]; then
          echo "::warning::${name} did not compile; continuing because TSZ_PROJECT_COMPILE_ALLOW_FAILURES=1"
        fi
      else
        record_project_compatibility "$name" "exit success" "check" "none" "" \
          "${_cached_files:-0}" "" "0" "$tsconfig" "$src_dir" "$tsc_exit_codes"
        echo "${name} compiled successfully."
        echo "::endgroup::"
      fi
      return 0
    fi
  fi

  # Only reached on cache miss — avoid running find on cache hits.
  local file_count
  file_count="$(count_ts_files "$src_dir")"

  echo "::group::${name}"
  echo "Running: $TSZ_BIN --noEmit -p $tsconfig"
  local rc=0 exit_class="" diagnostic_delta=""
  run_with_timeout "$PROJECT_TIMEOUT" \
    env \
      TSZ_USE_EMBEDDED_LIBS=1 \
      RUST_MIN_STACK="${TSZ_RUST_MIN_STACK:-536870912}" \
      "$TSZ_BIN" --noEmit -p "$tsconfig" >"$log" 2>&1 || rc=$?

  if [[ "$rc" -ne 0 ]]; then
    FAILURES=$((FAILURES + 1))
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
  else
    record_project_compatibility "$name" "exit success" "check" "none" "" \
      "$file_count" "$LAST_PEAK_RSS_BYTES" "0" "$tsconfig" "$src_dir" "$tsc_exit_codes"
    echo "${name} compiled successfully."
    echo "::endgroup::"
  fi
  [[ -n "$_cache_file" ]] && \
    write_compile_cache "$_fp" "$rc" "$file_count" "$exit_class" "$diagnostic_delta" "$_cache_file"
  return 0
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
    valibot-project)
      ensure_git_fixture "valibot" "$VALIBOT_REPO" "$VALIBOT_REF" "$FIXTURE_ROOT/valibot"
      tsz_write_valibot_config "$FIXTURE_ROOT/valibot/tsconfig.tsz-guard.json"
      check_project "$name" "$FIXTURE_ROOT/valibot/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/valibot/library/src"
      ;;
    msw-project)
      ensure_git_fixture "msw" "$MSW_REPO" "$MSW_REF" "$FIXTURE_ROOT/msw"
      tsz_write_msw_config "$FIXTURE_ROOT/msw/tsconfig.tsz-guard.json"
      check_project "$name" "$FIXTURE_ROOT/msw/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/msw/src"
      ;;
    comlink-project)
      ensure_git_fixture "comlink" "$COMLINK_REPO" "$COMLINK_REF" "$FIXTURE_ROOT/comlink"
      tsz_write_comlink_config "$FIXTURE_ROOT/comlink/tsconfig.tsz-guard.json"
      check_project "$name" "$FIXTURE_ROOT/comlink/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/comlink/src"
      ;;
    effect-project)
      ensure_git_fixture "effect" "$EFFECT_REPO" "$EFFECT_REF" "$FIXTURE_ROOT/effect"
      tsz_write_effect_config "$FIXTURE_ROOT/effect/tsconfig.tsz-guard.json"
      check_project "$name" "$FIXTURE_ROOT/effect/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/effect/packages/effect/src"
      ;;
    drizzle-orm-project)
      ensure_git_fixture "drizzle-orm" "$DRIZZLE_ORM_REPO" "$DRIZZLE_ORM_REF" "$FIXTURE_ROOT/drizzle-orm"
      tsz_write_drizzle_orm_config "$FIXTURE_ROOT/drizzle-orm/tsconfig.tsz-guard.json"
      check_project "$name" "$FIXTURE_ROOT/drizzle-orm/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/drizzle-orm/drizzle-orm/src"
      ;;
    ts-rest-project)
      ensure_git_fixture "ts-rest" "$TS_REST_REPO" "$TS_REST_REF" "$FIXTURE_ROOT/ts-rest"
      tsz_write_ts_rest_config "$FIXTURE_ROOT/ts-rest/tsconfig.tsz-guard.json"
      check_project "$name" "$FIXTURE_ROOT/ts-rest/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/ts-rest/libs/ts-rest/core/src"
      ;;
    ofetch-project)
      ensure_git_fixture "ofetch" "$OFETCH_REPO" "$OFETCH_REF" "$FIXTURE_ROOT/ofetch"
      tsz_write_ofetch_config "$FIXTURE_ROOT/ofetch/tsconfig.tsz-guard.json"
      check_project "$name" "$FIXTURE_ROOT/ofetch/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/ofetch/src"
      ;;
    vite-vanilla-ts-app)
      if [[ "$INCLUDE_GENERATED_APPS" != "1" ]]; then
        return 0
      fi
      ensure_generated_app_tools
      node scripts/bench/generate-vite-app-fixture.mjs "$FIXTURE_ROOT/vite-vanilla-ts-live"
      check_project "$name" "$FIXTURE_ROOT/vite-vanilla-ts-live/tsconfig.json" "$FIXTURE_ROOT/vite-vanilla-ts-live/src"
      ;;
    nextjs-fresh-app)
      if [[ "$INCLUDE_GENERATED_APPS" != "1" ]]; then
        return 0
      fi
      ensure_generated_app_tools
      node scripts/bench/generate-next-app-fixture.mjs "$FIXTURE_ROOT/next-app-live"
      check_project "$name" "$FIXTURE_ROOT/next-app-live/tsconfig.json" "$FIXTURE_ROOT/next-app-live"
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
  return 0
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
  return 0
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

exit 0
