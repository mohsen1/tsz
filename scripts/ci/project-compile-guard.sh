#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

DEFAULT_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT_DIR/.target}"
TSZ_BIN="${TSZ_BIN:-$DEFAULT_TARGET_DIR/dist-fast/tsz}"
FIXTURE_ROOT="${TSZ_PROJECT_COMPILE_FIXTURE_ROOT:-$ROOT_DIR/.target/project-compile-guard}"
PROJECT_TIMEOUT="${TSZ_PROJECT_COMPILE_TIMEOUT:-90}"
INCLUDE_GENERATED_APPS="${TSZ_PROJECT_COMPILE_INCLUDE_GENERATED_APPS:-1}"
# Skip category:"application" canary rows. These clone+install real apps (deps
# can take minutes each) and are compatibility canaries excluded from the
# benchmark corpus, so latency-sensitive callers (e.g. bench-publish's PGO
# timing step, capped at 30m) set this to 1 to avoid blowing their job budget
# on rows that produce no benchmark data.
SKIP_APPLICATIONS="${TSZ_PROJECT_COMPILE_SKIP_APPLICATIONS:-0}"
PROJECT_FILTER="${TSZ_PROJECT_COMPILE_FILTER:-}"
PROJECT_SET="${TSZ_PROJECT_COMPILE_SET:-required}"
ALLOW_FAILURES="${TSZ_PROJECT_COMPILE_ALLOW_FAILURES:-0}"
PROJECT_COMPATIBILITY_JSONL="${TSZ_PROJECT_COMPILE_COMPATIBILITY_JSONL:-$FIXTURE_ROOT/project-compatibility.jsonl}"
PROJECT_COMPATIBILITY_SUMMARY="${TSZ_PROJECT_COMPILE_COMPATIBILITY_SUMMARY:-$FIXTURE_ROOT/project-compatibility-summary.json}"
RESULT_CACHE_DIR="${TSZ_PROJECT_COMPILE_RESULT_CACHE_DIR:-$FIXTURE_ROOT/.result-cache}"
PROJECT_PERF_COUNTERS="${TSZ_PROJECT_COMPILE_PERF_COUNTERS:-0}"
PROJECT_STATS_READER="$ROOT_DIR/scripts/ci/project-compile-stats.mjs"
COMPILE_RESULT_CACHE_SCHEMA=5
FAILURES=0
LAST_PEAK_RSS_BYTES=0
LAST_TIMEOUT_CPU_SECONDS=""
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

# Measurement-protocol primitives: binary snapshotting and CPU-share evidence
# for wall timeouts (issue #13174).
# shellcheck source=scripts/bench/lib/measure-protocol.sh
source "$ROOT_DIR/scripts/bench/lib/measure-protocol.sh"
MIN_CPU_SHARE_PCT="${TSZ_PROJECT_COMPILE_MIN_CPU_SHARE_PCT:-$TSZ_MEASURE_DEFAULT_MIN_CPU_SHARE_PCT}"

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

# Result-cache fingerprint helpers (sha256_of_file, compute_compile_fingerprint,
# hash_source_tree). Sourced so the no-op fast-path key has one tested home.
# shellcheck source=scripts/ci/lib/project-compile-fingerprint.sh
source "$ROOT_DIR/scripts/ci/lib/project-compile-fingerprint.sh"

# Per-row tsc oracle helpers (tsz_only_delta_lines, tsz_count_diagnostic_lines,
# tsz_project_oracle_tsc_command, tsz_tsc_oracle_fingerprint). The gate counts a
# tsz diagnostic as a false positive only when tsc does not flag the same
# normalized project-relative path/span/code/message occurrence. Sourced so the
# symmetric multiset logic has one tested home.
# shellcheck source=scripts/ci/lib/project-tsc-oracle.sh
source "$ROOT_DIR/scripts/ci/lib/project-tsc-oracle.sh"

# Whether to run the per-row tsc oracle before deciding a row's pass/fail. On by
# default; disable with TSZ_PROJECT_COMPILE_TSC_ORACLE=0. For a normal nonzero
# tsz exit it subtracts genuine tsc errors; for tsz success it catches false
# negatives where pinned TypeScript 7 still reports a diagnostic. Crashes,
# timeouts, and OOMs remain failures regardless of what tsc reports.
TSC_ORACLE_ENABLED="${TSZ_PROJECT_COMPILE_TSC_ORACLE:-1}"
TSC_ORACLE_RESULT_CACHE_DIR="${TSZ_PROJECT_COMPILE_TSC_ORACLE_CACHE_DIR:-$FIXTURE_ROOT/.tsc-oracle-cache}"
# Resolved once: the tsc oracle command words and a content hash of the command
# for the oracle-cache key. Empty TSC_ORACLE_CMD means no oracle is available;
# ordinary results are then gray/non-evidence and never cached.
TSC_ORACLE_CMD=()
TSC_ORACLE_CMD_HASH=""
TSC_ORACLE_BUILTIN_LIB_DIR="${TSZ_PROJECT_TSC_ORACLE_BUILTIN_LIB_DIR:-}"
TSC_ORACLE_NATIVE_EXE=""
if [[ "$TSC_ORACLE_ENABLED" == "1" ]]; then
  while IFS= read -r _oracle_word; do
    [[ -n "$_oracle_word" ]] && TSC_ORACLE_CMD+=("$_oracle_word")
  done < <(tsz_project_oracle_tsc_command)
  if [[ "${#TSC_ORACLE_CMD[@]}" -gt 0 ]]; then
    if [[ -z "$TSC_ORACLE_BUILTIN_LIB_DIR" ]]; then
      for _oracle_word in "${TSC_ORACLE_CMD[@]}"; do
        case "$_oracle_word" in
          */lib/tsc.js)
            _oracle_get_exe="$(dirname "$_oracle_word")/getExePath.js"
            if [[ -f "$_oracle_get_exe" ]]; then
              TSC_ORACLE_NATIVE_EXE="$(node --input-type=module -e \
                'import { pathToFileURL } from "node:url"; const m = await import(pathToFileURL(process.argv[1]).href); process.stdout.write(m.default());' \
                "$_oracle_get_exe" 2>/dev/null || true)"
              [[ -n "$TSC_ORACLE_NATIVE_EXE" ]] \
                && TSC_ORACLE_BUILTIN_LIB_DIR="$(dirname "$TSC_ORACLE_NATIVE_EXE")"
            fi
            break
            ;;
        esac
      done
    fi
    TSC_ORACLE_CMD_HASH="$({
      printf 'protocol=single-threaded-stable-v2\n'
      printf 'builtin-lib-dir=%s\n' "$TSC_ORACLE_BUILTIN_LIB_DIR"
      printf 'native-exe=%s\n' "$TSC_ORACLE_NATIVE_EXE"
      [[ -f "$TSC_ORACLE_NATIVE_EXE" ]] \
        && printf 'native-content=%s\n' "$(sha256_of_file "$TSC_ORACLE_NATIVE_EXE")"
      for _oracle_word in "${TSC_ORACLE_CMD[@]}"; do
        printf 'word=%s\n' "$_oracle_word"
        [[ -f "$_oracle_word" ]] \
          && printf 'content=%s\n' "$(sha256_of_file "$_oracle_word")"
      done
    } | sha256_of_stdin)"
  fi
fi
export _TSZ_TSC_ORACLE_HASH="${TSC_ORACLE_CMD_HASH:-unavailable}"

mkdir -p "$FIXTURE_ROOT"

# Snapshot the tsz binary to an immutable content-addressed copy and run that
# copy for every project. TSZ_BIN often points at a live shared build output
# (dist-fast/tsz) that a sibling session can overwrite mid-run; measuring the
# live path would attribute a foreign binary's results to this run's binary
# hash. The snapshot is hash-verified, so _TSZ_BINARY_HASH is the hash of the
# binary that actually ran. Disable with TSZ_PROJECT_COMPILE_SNAPSHOT_BIN=0
# when TSZ_BIN is already an immutable path.
if [[ "${TSZ_PROJECT_COMPILE_SNAPSHOT_BIN:-1}" == "1" ]]; then
  _snapshot_out="$(tsz_snapshot_binary "$TSZ_BIN" "$FIXTURE_ROOT/.bin-snapshot")" \
    || fail "could not snapshot tsz binary for measurement: $TSZ_BIN"
  read -r TSZ_RUN_BIN _TSZ_BINARY_HASH <<< "$_snapshot_out"
  tsz_prune_binary_snapshots "$TSZ_BIN" "$FIXTURE_ROOT/.bin-snapshot" "$TSZ_RUN_BIN"
else
  TSZ_RUN_BIN="$TSZ_BIN"
  # Compute tsz binary hash once at startup for all per-project fingerprints.
  _TSZ_BINARY_HASH="$(sha256_of_file "$TSZ_BIN")"
fi
mkdir -p "$RESULT_CACHE_DIR"
[[ -n "$TSC_ORACLE_CMD_HASH" ]] && mkdir -p "$TSC_ORACLE_RESULT_CACHE_DIR"
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
  # CPU seconds the process tree consumed before a wall-timeout kill; empty
  # when the run did not time out or no sample could be taken. Callers use it
  # to distinguish CPU-bound timeouts from CPU-contention false timeouts.
  LAST_TIMEOUT_CPU_SECONDS=""
  "$@" &
  local pid=$!
  # The watchdog writes the pre-kill CPU sample here; the file's existence is
  # the timed-out marker and its content the contention evidence (#13174).
  local cpu_time_file
  cpu_time_file="$(mktemp)"
  rm -f "$cpu_time_file"
  local rss_file=""
  local rss_monitor_pid=""
  local watchdog_pid
  watchdog_pid="$(tsz_start_timeout_watchdog "$timeout_secs" "$pid" "$cpu_time_file")"
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
  if [ -e "$cpu_time_file" ]; then
    timed_out=1
    LAST_TIMEOUT_CPU_SECONDS="$(cat "$cpu_time_file" 2>/dev/null || true)"
  fi
  rm -f "$cpu_time_file"

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

LAST_ROOT_FILES=""
LAST_SOURCE_FILES=""
LAST_ROOT_FILE_FINGERPRINT=""
LAST_SOURCE_FILE_FINGERPRINT=""
LAST_SEMANTIC_COMPLETION=""
LAST_COMPILER_STATS_REASON=""

# Read the fresh `--perf-counters-json` machine contract. Schema-v2 canonical
# counts and ordered path arrays are mandatory; directory counts and extended text are deliberately
# not fallbacks because they do not prove what entered the compiler program.
read_project_compiler_stats() {
  local stats_file="$1" base_dir="$2" project_root="$3"
  LAST_ROOT_FILES=""
  LAST_SOURCE_FILES=""
  LAST_ROOT_FILE_FINGERPRINT=""
  LAST_SOURCE_FILE_FINGERPRINT=""
  LAST_SEMANTIC_COMPLETION=""
  LAST_COMPILER_STATS_REASON=""

  if [[ ! -f "$stats_file" ]]; then
    LAST_COMPILER_STATS_REASON="compiler stats missing"
    return 1
  fi

  local parsed=""
  if ! parsed="$(node "$PROJECT_STATS_READER" compiler-stats "$stats_file" "$base_dir" "$project_root" 2>/dev/null)"; then
    LAST_COMPILER_STATS_REASON="compiler stats malformed"
    return 1
  fi
  IFS=$'\t' read -r LAST_ROOT_FILES LAST_SOURCE_FILES \
    LAST_ROOT_FILE_FINGERPRINT LAST_SOURCE_FILE_FINGERPRINT \
    LAST_SEMANTIC_COMPLETION <<< "$parsed"
  if [[ ! "$LAST_ROOT_FILES" =~ ^(0|[1-9][0-9]*)$ ]] \
    || [[ ! "$LAST_SOURCE_FILES" =~ ^(0|[1-9][0-9]*)$ ]] \
    || [[ ! "$LAST_ROOT_FILE_FINGERPRINT" =~ ^[0-9a-f]{64}$ ]] \
    || [[ ! "$LAST_SOURCE_FILE_FINGERPRINT" =~ ^[0-9a-f]{64}$ ]] \
    || [[ "$LAST_SEMANTIC_COMPLETION" != "complete" ]]; then
    LAST_ROOT_FILES=""
    LAST_SOURCE_FILES=""
    LAST_ROOT_FILE_FINGERPRINT=""
    LAST_SOURCE_FILE_FINGERPRINT=""
    LAST_SEMANTIC_COMPLETION=""
    LAST_COMPILER_STATS_REASON="compiler stats malformed"
    return 1
  fi
  return 0
}

diagnostic_lines_from_file() {
  local label="$1"
  local file="$2"
  local max="${3:-20}"

  awk -v label="$label" -v max="$max" '
    {
      sub(/\r$/, "")
      if ($0 ~ /^[[:space:]]*$/) {
        next
      }
      print label ": " $0
      seen += 1
      if (seen >= max) {
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
  # Empty is meaningful: the compiler did not produce trustworthy processed-
  # file evidence. Do not coerce it to zero, which would erase the reason.
  local files_reached="${6-}"
  local peak_memory_bytes="${7:-}"
  local tsz_exit_codes="${8:-}"
  local tsconfig_path="${9:-}"
  local source_root="${10:-}"
  local tsc_exit_codes="${11:-}"
  local files_reached_reason="${12:-}"
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
  COMPAT_SEMANTIC_COMPLETION="${LAST_SEMANTIC_COMPLETION:-}" \
  COMPAT_DIAGNOSTIC_DELTA="$diagnostic_delta" \
  COMPAT_FILES_REACHED="$files_reached" \
  COMPAT_FILES_REACHED_REASON="$files_reached_reason" \
  COMPAT_PEAK_MEMORY_BYTES="$peak_memory_bytes" \
  COMPAT_PEAK_MEMORY_BYTES_REASON="$peak_memory_bytes_reason" \
  COMPAT_TSZ_EXIT_CODES="$tsz_exit_codes" \
  COMPAT_TSC_EXIT_CODES="$tsc_exit_codes" \
  COMPAT_TSCONFIG_PATH="$tsconfig_path" \
  COMPAT_SOURCE_ROOT="$source_root" \
  COMPAT_FIXTURE_ROOT="$FIXTURE_ROOT" \
  COMPAT_FIXTURE_SOURCES="$fixture_sources" \
  COMPAT_TSZ_COMMAND_ENV_PREFIX="TSZ_USE_EMBEDDED_LIBS=1 RUST_MIN_STACK=${TSZ_RUST_MIN_STACK:-536870912}" \
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

# Install a real application's dependencies so the type-checker resolves its
# framework imports (react/next/...) instead of emitting a TS2307 wall. The
# compile still runs with skipLibCheck, so node_modules only needs to RESOLVE,
# not deep-check. Returns non-zero when the install fails so the caller records the
# row as "fixture invalid" (gray, advisory) instead of letting a dependency-less
# compile emit a TS2307 wall that the tsc-oracle delta scores as a tsz-only
# regression. Bounded by the job timeout.
install_application_deps() {
  local dir="$1" cmd="$2"
  [ -z "$cmd" ] && return 0
  if [ ! -d "$dir" ]; then
    echo "warn: application install dir missing: $dir" >&2
    return 1
  fi
  setup_application_package_managers
  local pm="${cmd%% *}"
  # Pick a working entrypoint for the app's package manager. The bootstrap above
  # tries to put a real `yarn`/`pnpm`/`bun` on PATH; if the direct binary still
  # isn't there, fall back through independent mechanisms so one PM family's
  # failure doesn't blank the rest:
  #   - corepack (on PATH from the bootstrap) runs the app's pinned yarn/pnpm.
  #   - Yarn Berry projects vendor their release in `.yarn/releases/*.cjs`; run
  #     that directly with node when no yarn entrypoint resolved at all.
  case "$pm" in
    yarn | pnpm)
      if ! command -v "$pm" >/dev/null 2>&1; then
        if command -v corepack >/dev/null 2>&1; then
          echo "info: $pm not on PATH; running via corepack" >&2
          cmd="corepack $cmd"
        elif [ "$pm" = "yarn" ]; then
          local vendored
          vendored="$(ls "$dir"/.yarn/releases/yarn-*.cjs 2>/dev/null | head -1 || true)"
          if [ -n "$vendored" ]; then
            echo "info: yarn not on PATH; using the project's vendored release $vendored" >&2
            cmd="node $vendored ${cmd#yarn }"
          fi
        fi
      fi
      ;;
  esac
  echo "Installing application deps: (cd $dir && $cmd)"
  if ! ( cd "$dir" && run_with_timeout "${TSZ_APP_INSTALL_TIMEOUT:-360}" bash -c "$cmd" ); then
    echo "warn: application dep install failed: $dir" >&2
    return 1
  fi
  return 0
}

# Provision the package managers the application fixtures need, once per job. The
# hosted runner image ships only npm — yarn (7 rows), pnpm (10), and bun (1) are
# not on PATH, and npm's DEFAULT global prefix is neither writable nor on
# PATH, so a bare `npm i -g corepack` neither installs nor becomes invocable.
# That left the whole application set "<pm>: command not found" -> fixture-invalid
# (gray), which is what emptied the compatibility dashboard. Provision into a
# runner-writable prefix we also put on PATH, using INDEPENDENT mechanisms so no
# single failure blocks a whole PM family:
#   - corepack (ships with Node; installed to our prefix if absent) + `enable
#     --install-directory` drops real yarn/pnpm shims on PATH that resolve each
#     app's pinned version.
#   - pnpm and bun also ship standalone static-binary installers that need
#     neither npm-global nor corepack, fetched directly as fallbacks.
# The runner reaches the registry/CDNs (the npm apps' `npm ci` already works).
# Exports persist across rows via the shared shell env. Best-effort throughout: a
# still-missing PM leaves the row fixture-invalid (gray), never a false tsz
# regression. The `[pm-setup]` line surfaces exactly what resolved in the CI log.
_TSZ_PM_SETUP_DONE="${_TSZ_PM_SETUP_DONE:-0}"
setup_application_package_managers() {
  [ "$_TSZ_PM_SETUP_DONE" = "1" ] && return 0
  _TSZ_PM_SETUP_DONE=1
  local pm_home="${TSZ_PM_HOME:-${HOME:-/tmp}/.tsz-pm}"
  mkdir -p "$pm_home/bin" 2>/dev/null || true
  case ":$PATH:" in *":$pm_home/bin:"*) ;; *) export PATH="$pm_home/bin:$PATH" ;; esac
  export NPM_CONFIG_PREFIX="$pm_home"
  export COREPACK_ENABLE_DOWNLOAD_PROMPT=0
  export COREPACK_HOME="$pm_home/corepack"

  # Several application fixtures pin an engines.node / packageManager that the
  # self-hosted runner's baked-in Node (v18 at time of writing) does not satisfy.
  # Recent pnpm releases refuse to even START on Node < 22.13, and pnpm enforces
  # a ROOT project's engines.node UNCONDITIONALLY (engine-strict cannot disable
  # it -- https://pnpm.io/settings), so directus/supabase-studio/umami/
  # immich-server have no install-flag bypass: they need a modern Node. Best-effort
  # fetch a current Node 22 LTS into the PM home and prepend it so corepack/node/
  # npm/yarn/pnpm/bun all run under it. Every failure path falls back to the
  # runner Node, so this never regresses a row that installed before.
  local node_dir="$pm_home/node"
  if ! { command -v node >/dev/null 2>&1 \
    && node -e 'process.exit(Number(process.versions.node.split(".")[0]) >= 22 ? 0 : 1)' 2>/dev/null; } \
    && [ ! -x "$node_dir/bin/node" ]; then
    local node_os=linux node_arch=x64
    case "$(uname -s 2>/dev/null)" in Darwin) node_os=darwin ;; esac
    case "$(uname -m 2>/dev/null)" in aarch64 | arm64) node_arch=arm64 ;; esac
    local node_base="https://nodejs.org/dist/latest-v22.x"
    local node_file=""
    # `|| true`: a network failure here must not abort the (set -euo pipefail)
    # guard run -- it degrades to the runner Node.
    node_file="$(curl -fsSL "$node_base/SHASUMS256.txt" 2>/dev/null \
      | grep -oE "node-v22\.[0-9.]+-${node_os}-${node_arch}\.tar\.gz" | head -1)" || true
    if [ -n "$node_file" ]; then
      echo "[pm-setup] provisioning ${node_file} for application fixture installs" >&2
      mkdir -p "$node_dir" 2>/dev/null || true
      { curl -fsSL "$node_base/$node_file" -o "$pm_home/node22.tar.gz" 2>/dev/null \
        && tar -xzf "$pm_home/node22.tar.gz" -C "$node_dir" --strip-components=1 2>/dev/null; } || true
      rm -f "$pm_home/node22.tar.gz" 2>/dev/null || true
    fi
  fi
  if [ -x "$node_dir/bin/node" ]; then
    case ":$PATH:" in *":$node_dir/bin:"*) ;; *) export PATH="$node_dir/bin:$PATH" ;; esac
  fi

  # rocketchat vendors devoto13/yarn-plugin-engines, which hard-fails Yarn Berry
  # "Project validation" when the runner Node is older than the project's pin
  # ("The current node version ... does not satisfy the required version ...").
  # This env disables that plugin's gate; it is a no-op when the plugin is absent.
  export PLUGIN_YARN_ENGINES_DISABLE=1

  # corepack -> yarn + pnpm at each app's pinned version.
  command -v corepack >/dev/null 2>&1 || npm i -g corepack >/dev/null 2>&1 || true
  corepack enable --install-directory "$pm_home/bin" >/dev/null 2>&1 \
    || corepack enable >/dev/null 2>&1 || true

  # Standalone fallbacks (static binaries; no npm-global / corepack needed).
  if ! command -v pnpm >/dev/null 2>&1; then
    curl -fsSL https://get.pnpm.io/install.sh 2>/dev/null \
      | env PNPM_HOME="$pm_home/bin" SHELL=bash sh - >/dev/null 2>&1 || true
  fi
  if ! command -v bun >/dev/null 2>&1; then
    curl -fsSL https://bun.sh/install 2>/dev/null \
      | env BUN_INSTALL="$pm_home" bash >/dev/null 2>&1 || true
  fi

  echo "[pm-setup] node=$(command -v node || echo -) ($(node --version 2>/dev/null || echo '?')) npm=$(command -v npm || echo -) corepack=$(command -v corepack || echo -) yarn=$(command -v yarn || echo -) pnpm=$(command -v pnpm || echo -) bun=$(command -v bun || echo -)" >&2
}

# Generic handler for category:"application" canary rows: clone the pinned
# fixture, install its deps, compile with the app's OWN tsconfig (which carries
# the right jsx/paths), then reclaim node_modules disk so a shard can run
# several apps sequentially. Args:
#   name fixture_dir repo ref install_cmd install_root app_tsconfig src_dir
run_application_row() {
  local name="$1" fdir="$2" repo="$3" ref="$4" install_cmd="$5" install_root="$6" app_tsconfig="$7" src_rel="$8"
  if [[ "$SKIP_APPLICATIONS" == "1" ]]; then
    echo "Skipping application row $name (TSZ_PROJECT_COMPILE_SKIP_APPLICATIONS=1)"
    return 0
  fi
  local root="$FIXTURE_ROOT/$fdir"
  # Clone/install failures are HARNESS faults, not tsz regressions. Record them as
  # "fixture invalid" (-> gray, advisory) rather than letting the dependency-less
  # compile emit a TS2307 wall that the tsc-oracle delta scores as a tsz-only RED.
  # This is the prerequisite that lets the canary become a blocking no-regression
  # gate without a flaky network/install producing a false regression.
  if ! ensure_git_fixture "$fdir" "$repo" "$ref" "$root"; then
    echo "warn: $name: clone/checkout failed; recording fixture invalid (gray, advisory)" >&2
    record_project_compatibility "$name" "fixture invalid" "fixture setup" \
      "application clone failed" "harness: clone/checkout failed for ${repo}@${ref}" \
      "0" "" "" "$root/$app_tsconfig" "$root/$src_rel"
    return 0
  fi
  if ! install_application_deps "$root/$install_root" "$install_cmd"; then
    echo "warn: $name: dependency install failed; recording fixture invalid (gray, advisory)" >&2
    record_project_compatibility "$name" "fixture invalid" "fixture setup" \
      "application install failed" "harness: dependency install failed in ${install_root} (${install_cmd})" \
      "0" "" "" "$root/$app_tsconfig" "$root/$src_rel"
    if [[ "${TSZ_APP_KEEP_NODE_MODULES:-0}" != "1" ]]; then
      find "$root" -type d -name node_modules -prune -exec rm -rf {} + 2>/dev/null || true
    fi
    return 0
  fi
  check_project "$name" "$root/$app_tsconfig" "$root/$src_rel"
  if [[ "${TSZ_APP_KEEP_NODE_MODULES:-0}" != "1" ]]; then
    find "$root" -type d -name node_modules -prune -exec rm -rf {} + 2>/dev/null || true
  fi
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

type_challenges_tsc_command() {
  if [[ -n "${TYPE_CHALLENGES_ASSERTION_TSC_BIN+x}" ]]; then
    if [[ -x "$TYPE_CHALLENGES_ASSERTION_TSC_BIN" ]]; then
      printf '%s\n' "$TYPE_CHALLENGES_ASSERTION_TSC_BIN"
    fi
    return 0
  fi

  tsz_project_oracle_tsc_command
}

ensure_type_challenges_assertion_tsc() {
  if [[ -n "${TYPE_CHALLENGES_ASSERTION_TSC_BIN+x}" ]]; then
    return 0
  fi

  if [[ -n "$(tsz_project_oracle_tsc_command)" ]]; then
    return 0
  fi

  echo "Ensuring pinned TypeScript for Type Challenges assertion classifier"
  if ! scripts/setup/ensure-pinned-typescript.sh scripts; then
    echo "warn: pinned TypeScript setup failed; Type Challenges assertion classifier will report tsc unavailable" >&2
    return 0
  fi
  if [[ -z "$(tsz_project_oracle_tsc_command)" ]]; then
    echo "warn: pinned TypeScript command is unavailable; Type Challenges assertion classifier will report tsc unavailable" >&2
  fi
}

check_type_challenges_solutions_tsc_oracle() {
  local tsconfig="$FIXTURE_ROOT/type-challenges-solutions/.tsz-compile/tsconfig.tsz-guard.json"
  local src_dir="$FIXTURE_ROOT/type-challenges-solutions/.tsz-compile/solutions"
  local log="$FIXTURE_ROOT/type-challenges-solutions-project.tsc.log"

  ensure_type_challenges_assertion_tsc

  local tsc_command=()
  local tsc_word
  while IFS= read -r tsc_word; do
    [[ -n "$tsc_word" ]] && tsc_command+=("$tsc_word")
  done < <(type_challenges_tsc_command)
  if [[ "${#tsc_command[@]}" -eq 0 ]]; then
    FAILURES=$((FAILURES + 1))
    record_project_compatibility \
      "type-challenges-solutions-project" \
      "fixture invalid" \
      "fixture setup" \
      "tsc oracle unavailable" \
      "tsc: Type Challenges solutions oracle is unavailable" \
      "" \
      "" \
      "" \
      "$tsconfig" \
      "$src_dir" \
      "127" \
      "not in scope"
    echo "error: Type Challenges solutions project requires a tsc oracle, but no tsc binary is available" >&2
    return 1
  fi

  local rc=0
  run_with_timeout "$PROJECT_TIMEOUT" "${tsc_command[@]}" \
    --singleThreaded --stableTypeOrdering true --noEmit -p "$tsconfig" \
    >"$log" 2>&1 || rc=$?
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
      "" \
      "$LAST_PEAK_RSS_BYTES" \
      "" \
      "$tsconfig" \
      "$src_dir" \
      "$rc" \
      "not in scope"
    echo "error: type-challenges-solutions-project failed the tsc oracle check" >&2
    sed -n '1,160p' "$log" >&2 || true
    return 1
  fi

  return 0
}

# Atomically writes a compile result to the result cache (tmp-then-rename).
# The fingerprint is stored inside the file so stale entries self-prune on the
# next read rather than accumulating one file per fingerprint per project.
# write_compile_cache <fp> <result_rc> <tsz_rc> <root_files> <source_files>
#   <root_fp> <source_fp> <files_reason> <tsc_root_files> <tsc_source_files>
#   <tsc_root_fp> <tsc_source_fp> <tsc_rc>
#   <exit_class> <diagnostic_status> <diagnostic_delta> <cache_file>
write_compile_cache() {
  local _fp="$1" _result_rc="$2" _tsz_rc="$3" _roots="$4" _sources="$5"
  local _root_fp="$6" _source_fp="$7" _files_reason="$8"
  local _tsc_roots="$9" _tsc_sources="${10}" _tsc_root_fp="${11}" _tsc_source_fp="${12}"
  local _tsc_rc="${13}" _class="${14}" _status="${15}" _delta="${16}" _cf="${17}"
  { printf 'SCHEMA=%s\nFINGERPRINT=%s\nRC=%s\nTSZ_RC=%s\nSEMANTIC_COMPLETION=complete\nROOT_FILES=%s\nSOURCE_FILES=%s\nROOT_FINGERPRINT=%s\nSOURCE_FINGERPRINT=%s\nFILES_REASON=%s\nTSC_ROOT_FILES=%s\nTSC_SOURCE_FILES=%s\nTSC_ROOT_FINGERPRINT=%s\nTSC_SOURCE_FINGERPRINT=%s\nTSC_RC=%s\nCLASS=%s\nSTATUS=%s\nDELTA_START\n' \
      "$COMPILE_RESULT_CACHE_SCHEMA" "$_fp" "$_result_rc" "$_tsz_rc" \
      "$_roots" "$_sources" "$_root_fp" "$_source_fp" "$_files_reason" \
      "$_tsc_roots" "$_tsc_sources" "$_tsc_root_fp" "$_tsc_source_fp" \
      "$_tsc_rc" "$_class" "$_status"
    # Terminate the delta body so the cache reader's `read` loop retains its
    # final diagnostic line (command substitution strips trailing newlines).
    printf '%s\n' "$_delta"; } \
    > "${_cf}.tmp" 2>/dev/null \
    && mv "${_cf}.tmp" "$_cf" 2>/dev/null || true
}

# Ask pinned TypeScript 7 for exact ordered root/source graph paths. `--showConfig`
# exposes config roots without library files; `--listFilesOnly` exposes
# the resolved source graph; the parser removes only canonical built-in
# `lib*.d.ts` paths and retains project/@types declaration files. Both exact
# counts and normalized fingerprints are required for evidence.
# Extended-diagnostics `Files:` is never a fallback because it includes libs.
LAST_TSC_ROOT_FILES=""
LAST_TSC_ROOT_FINGERPRINT=""
LAST_TSC_ROOT_FILES_REASON=""
LAST_TSC_SOURCE_FILES=""
LAST_TSC_SOURCE_FINGERPRINT=""
LAST_TSC_SOURCE_FILES_REASON=""
run_project_tsc_graph_counts() {
  local name="$1" tsconfig="$2" src_dir="$3"
  LAST_TSC_ROOT_FILES=""
  LAST_TSC_ROOT_FINGERPRINT=""
  LAST_TSC_ROOT_FILES_REASON=""
  LAST_TSC_SOURCE_FILES=""
  LAST_TSC_SOURCE_FINGERPRINT=""
  LAST_TSC_SOURCE_FILES_REASON=""
  if [[ "${#TSC_ORACLE_CMD[@]}" -eq 0 ]]; then
    LAST_TSC_ROOT_FILES_REASON="tsc root oracle unavailable"
    LAST_TSC_SOURCE_FILES_REASON="tsc source oracle unavailable"
    return 1
  fi
  if [[ -z "$TSC_ORACLE_BUILTIN_LIB_DIR" || ! -d "$TSC_ORACLE_BUILTIN_LIB_DIR" ]]; then
    LAST_TSC_ROOT_FILES_REASON="tsc built-in library identity unavailable"
    LAST_TSC_SOURCE_FILES_REASON="tsc built-in library identity unavailable"
    return 1
  fi

  local fp="" cache_file=""
  fp="$(tsz_tsc_oracle_fingerprint "$name" "$tsconfig" "$src_dir" "$TSC_ORACLE_CMD_HASH" 2>/dev/null || true)"
  if [[ "${TSZ_PROJECT_COMPILE_TSC_ORACLE_CACHE:-0}" == "1" && -n "$fp" ]]; then
    cache_file="$TSC_ORACLE_RESULT_CACHE_DIR/${name}.graph-counts"
  fi

  if [[ -n "$cache_file" && -f "$cache_file" ]]; then
    local cached_schema="" cached_fp="" cached_roots="" cached_sources=""
    local cached_root_fp="" cached_source_fp=""
    local cached_root_reason="" cached_source_reason="" line
    while IFS= read -r line; do
      case "$line" in
        SCHEMA=*) cached_schema="${line#SCHEMA=}" ;;
        FINGERPRINT=*) cached_fp="${line#FINGERPRINT=}" ;;
        ROOT_FILES=*) cached_roots="${line#ROOT_FILES=}" ;;
        SOURCE_FILES=*) cached_sources="${line#SOURCE_FILES=}" ;;
        ROOT_FINGERPRINT=*) cached_root_fp="${line#ROOT_FINGERPRINT=}" ;;
        SOURCE_FINGERPRINT=*) cached_source_fp="${line#SOURCE_FINGERPRINT=}" ;;
        ROOT_REASON=*) cached_root_reason="${line#ROOT_REASON=}" ;;
        SOURCE_REASON=*) cached_source_reason="${line#SOURCE_REASON=}" ;;
      esac
    done < "$cache_file"
    if [[ "$cached_schema" == "3" && "$cached_fp" == "$fp" ]]; then
      LAST_TSC_ROOT_FILES="$cached_roots"
      LAST_TSC_SOURCE_FILES="$cached_sources"
      LAST_TSC_ROOT_FINGERPRINT="$cached_root_fp"
      LAST_TSC_SOURCE_FINGERPRINT="$cached_source_fp"
      LAST_TSC_ROOT_FILES_REASON="$cached_root_reason"
      LAST_TSC_SOURCE_FILES_REASON="$cached_source_reason"
      if [[ "$LAST_TSC_ROOT_FILES" =~ ^(0|[1-9][0-9]*)$ \
        && "$LAST_TSC_SOURCE_FILES" =~ ^(0|[1-9][0-9]*)$ \
        && "$LAST_TSC_ROOT_FINGERPRINT" =~ ^[0-9a-f]{64}$ \
        && "$LAST_TSC_SOURCE_FINGERPRINT" =~ ^[0-9a-f]{64}$ ]]; then
        return
      fi
    fi
  fi

  local project_root config_dir
  project_root="$(tsz_fingerprint_project_root "$(dirname "$tsconfig")")"
  config_dir="$(dirname "$tsconfig")"
  local show_config_file show_config_err rc=0 roots="" root_fp="" root_graph="" root_reason=""
  show_config_file="$(mktemp "$FIXTURE_ROOT/${name}.show-config.XXXXXX")"
  show_config_err="${show_config_file}.err"
  run_with_timeout "$PROJECT_TIMEOUT" \
    "${TSC_ORACLE_CMD[@]}" --singleThreaded --stableTypeOrdering true \
    --showConfig -p "$tsconfig" \
    >"$show_config_file" 2>"$show_config_err" || rc=$?
  if [[ "$rc" -ne 0 ]]; then
    root_reason="tsc showConfig unavailable (exit ${rc})"
  elif ! root_graph="$(node "$PROJECT_STATS_READER" show-config-roots "$show_config_file" "$config_dir" "$project_root" 2>/dev/null)"; then
    root_reason="tsc showConfig malformed"
  else
    IFS=$'\t' read -r roots root_fp <<< "$root_graph"
  fi
  rm -f "$show_config_file" "$show_config_err"

  if [[ "$roots" =~ ^(0|[1-9][0-9]*)$ && "$root_fp" =~ ^[0-9a-f]{64}$ ]]; then
    LAST_TSC_ROOT_FILES="$roots"
    LAST_TSC_ROOT_FINGERPRINT="$root_fp"
  else
    LAST_TSC_ROOT_FILES_REASON="${root_reason:-tsc root oracle unavailable}"
  fi

  local list_file list_err list_rc=0 sources="" source_fp="" source_graph="" source_reason=""
  list_file="$(mktemp "$FIXTURE_ROOT/${name}.list-files.XXXXXX")"
  list_err="${list_file}.err"
  run_with_timeout "$PROJECT_TIMEOUT" \
    "${TSC_ORACLE_CMD[@]}" --singleThreaded --stableTypeOrdering true \
    --listFilesOnly -p "$tsconfig" \
    >"$list_file" 2>"$list_err" || list_rc=$?
  if [[ "$list_rc" -ne 0 ]]; then
    source_reason="tsc listFilesOnly unavailable (exit ${list_rc})"
  elif ! source_graph="$(node "$PROJECT_STATS_READER" list-files-graph "$list_file" "$TSC_ORACLE_BUILTIN_LIB_DIR" "$project_root" 2>/dev/null)"; then
    source_reason="tsc listFilesOnly malformed"
  else
    IFS=$'\t' read -r sources source_fp <<< "$source_graph"
  fi
  rm -f "$list_file" "$list_err"

  if [[ "$sources" =~ ^(0|[1-9][0-9]*)$ && "$source_fp" =~ ^[0-9a-f]{64}$ ]]; then
    LAST_TSC_SOURCE_FILES="$sources"
    LAST_TSC_SOURCE_FINGERPRINT="$source_fp"
  else
    LAST_TSC_SOURCE_FILES_REASON="${source_reason:-tsc source oracle unavailable}"
  fi
  if [[ -n "$cache_file" && -n "$LAST_TSC_ROOT_FILES" \
    && -n "$LAST_TSC_SOURCE_FILES" && -n "$LAST_TSC_ROOT_FINGERPRINT" \
    && -n "$LAST_TSC_SOURCE_FINGERPRINT" ]]; then
    { printf 'SCHEMA=3\nFINGERPRINT=%s\nROOT_FILES=%s\nSOURCE_FILES=%s\nROOT_FINGERPRINT=%s\nSOURCE_FINGERPRINT=%s\nROOT_REASON=%s\nSOURCE_REASON=%s\n' \
        "$fp" "$LAST_TSC_ROOT_FILES" "$LAST_TSC_SOURCE_FILES" \
        "$LAST_TSC_ROOT_FINGERPRINT" "$LAST_TSC_SOURCE_FINGERPRINT" \
        "$LAST_TSC_ROOT_FILES_REASON" "$LAST_TSC_SOURCE_FILES_REASON"; } \
      > "${cache_file}.tmp" 2>/dev/null \
      && mv "${cache_file}.tmp" "$cache_file" 2>/dev/null || true
  fi
  [[ -n "$LAST_TSC_ROOT_FILES" && -n "$LAST_TSC_SOURCE_FILES" \
    && -n "$LAST_TSC_ROOT_FINGERPRINT" && -n "$LAST_TSC_SOURCE_FINGERPRINT" ]]
}

# Run (or cache-hit) the per-row tsc oracle. tsc's stdout/stderr is captured to
# the per-row $oracle_log; on return LAST_TSC_ORACLE_RC holds tsc's exit code.
# The oracle is cached on (tsc command, tsconfig content, compiled-source
# identity) — independent of the tsz binary — so the per-row tsc run is skipped
# whenever the fixture and tsc are unchanged, keeping CI cost flat across tsz
# rebuilds. Returns 0 only for an ordinary completed tsc exit (0-4); unavailable,
# timeout, crash, or malformed cached status returns 1 and is non-evidence.
LAST_TSC_ORACLE_RC=""
run_project_tsc_oracle() {
  local name="$1" tsconfig="$2" src_dir="$3" oracle_log="$4"
  LAST_TSC_ORACLE_RC=""
  [[ "${#TSC_ORACLE_CMD[@]}" -gt 0 ]] || return 1

  local _ofp="" _ocache=""
  if [[ "${TSZ_PROJECT_COMPILE_TSC_ORACLE_CACHE:-0}" == "1" && -n "$TSC_ORACLE_CMD_HASH" ]]; then
    _ofp="$(tsz_tsc_oracle_fingerprint "$name" "$tsconfig" "$src_dir" "$TSC_ORACLE_CMD_HASH" 2>/dev/null || true)"
    [[ -n "$_ofp" ]] && _ocache="$TSC_ORACLE_RESULT_CACHE_DIR/${name}"
  fi

  if [[ -n "$_ocache" && -f "$_ocache" && -f "${_ocache}.log" ]]; then
    local _cached_schema="" _cached_ofp="" _cached_orc="" _cached_log_sha="" _actual_log_sha=""
    local _cached_log_copy=""
    local _cache_line
    while IFS= read -r _cache_line; do
      case "$_cache_line" in
        SCHEMA=*) _cached_schema="${_cache_line#SCHEMA=}" ;;
        FINGERPRINT=*) _cached_ofp="${_cache_line#FINGERPRINT=}" ;;
        RC=*) _cached_orc="${_cache_line#RC=}" ;;
        LOG_SHA256=*) _cached_log_sha="${_cache_line#LOG_SHA256=}" ;;
      esac
    done < "$_ocache"
    if [[ "$_cached_schema" == "2" && "$_cached_ofp" == "$_ofp" \
      && "$_cached_log_sha" =~ ^[0-9a-f]{64}$ ]]; then
      case "$_cached_orc" in
        0|1|2|3|4)
          _cached_log_copy="$(mktemp "${oracle_log}.cache.XXXXXX" 2>/dev/null || true)"
          if [[ -n "$_cached_log_copy" ]] \
            && cp "${_ocache}.log" "$_cached_log_copy" 2>/dev/null; then
            _actual_log_sha="$(sha256_of_file "$_cached_log_copy")"
          fi
          if [[ "$_actual_log_sha" == "$_cached_log_sha" ]] \
            && mv "$_cached_log_copy" "$oracle_log" 2>/dev/null; then
            LAST_TSC_ORACLE_RC="$_cached_orc"
            echo "(tsc oracle cache hit: ${_ofp:0:12})"
            return 0
          fi
          if [[ -n "$_cached_log_copy" ]]; then rm -f "$_cached_log_copy"; fi
          ;;
      esac
    fi
  fi

  local orc=0
  echo "Running tsc oracle: ${TSC_ORACLE_CMD[*]} --noEmit -p $tsconfig"
  run_with_timeout "$PROJECT_TIMEOUT" \
    "${TSC_ORACLE_CMD[@]}" --singleThreaded --stableTypeOrdering true \
    --noEmit -p "$tsconfig" >"$oracle_log" 2>&1 || orc=$?
  LAST_TSC_ORACLE_RC="$orc"

  local oracle_completed=0
  case "$orc" in
    0|1|2|3|4) oracle_completed=1 ;;
  esac

  if [[ "$oracle_completed" == "1" && -n "$_ocache" ]]; then
    local _stage_log="" _stage_meta="" _log_sha=""
    _stage_log="$(mktemp "${_ocache}.log.tmp.XXXXXX" 2>/dev/null || true)"
    _stage_meta="$(mktemp "${_ocache}.meta.tmp.XXXXXX" 2>/dev/null || true)"
    if [[ -n "$_stage_log" && -n "$_stage_meta" ]] \
      && cp "$oracle_log" "$_stage_log" 2>/dev/null; then
      _log_sha="$(sha256_of_file "$_stage_log")"
      if [[ "$_log_sha" =~ ^[0-9a-f]{64}$ ]] \
        && printf 'SCHEMA=2\nFINGERPRINT=%s\nRC=%s\nLOG_SHA256=%s\n' \
          "$_ofp" "$orc" "$_log_sha" > "$_stage_meta" 2>/dev/null \
        && mv "$_stage_log" "${_ocache}.log" 2>/dev/null; then
        # Metadata is the commit marker and is always published after the log.
        mv "$_stage_meta" "$_ocache" 2>/dev/null || true
      fi
    fi
    if [[ -n "$_stage_log" ]]; then rm -f "$_stage_log"; fi
    if [[ -n "$_stage_meta" ]]; then rm -f "$_stage_meta"; fi
  fi
  [[ "$oracle_completed" == "1" ]]
}

check_project() {
  local name="$1"
  local tsconfig="$2"
  local src_dir="${3:-$(dirname "$tsconfig")}"
  local tsc_exit_codes="${4:-}"
  local log="$FIXTURE_ROOT/${name}.log"
  LAST_SEMANTIC_COMPLETION=""

  # Result cache: skip recompilation when the tsz binary, pinned-oracle protocol,
  # and conservative full fixture-row compile-input identity are all unchanged
  # from a prior run (see
  # scripts/ci/lib/project-compile-fingerprint.sh). The cache file is named
  # per-project; the stored fingerprint is validated on read so stale entries are
  # overwritten rather than accumulated. Reuse is correctness-sensitive and
  # therefore opt-in with TSZ_PROJECT_COMPILE_RESULT_CACHE=1.
  local _fp="" _cache_file=""
  if [[ "${TSZ_PROJECT_COMPILE_RESULT_CACHE:-0}" == "1" && "$PROJECT_PERF_COUNTERS" != "1" ]]; then
    _fp="$(compute_compile_fingerprint "$name" "$tsconfig" "$src_dir" 2>/dev/null || true)"
    [[ -n "$_fp" ]] && _cache_file="$RESULT_CACHE_DIR/${name}"
  fi

  if [[ -n "$_cache_file" && -f "$_cache_file" ]]; then
    local _cached_schema="" _cached_fp="" _cached_rc="" _cached_tsz_rc=""
    local _cached_semantic_completion=""
    local _cached_roots="" _cached_sources="" _cached_files_reason=""
    local _cached_root_fp="" _cached_source_fp=""
    local _cached_tsc_roots="" _cached_tsc_sources="" _cached_tsc_rc="" _cached_class=""
    local _cached_tsc_root_fp="" _cached_tsc_source_fp=""
    local _cached_status="" _cached_delta="" _in_delta=0
    local _line
    while IFS= read -r _line; do
      if [[ "$_in_delta" == "1" ]]; then
        _cached_delta="${_cached_delta}${_line}"$'\n'
      else
        case "$_line" in
          SCHEMA=*)      _cached_schema="${_line#SCHEMA=}" ;;
          FINGERPRINT=*)
            _cached_fp="${_line#FINGERPRINT=}"
            # Bail early on mismatch — avoids reading the full delta body.
            [[ "$_cached_fp" != "$_fp" ]] && break
            ;;
          RC=*)          _cached_rc="${_line#RC=}" ;;
          TSZ_RC=*)      _cached_tsz_rc="${_line#TSZ_RC=}" ;;
          SEMANTIC_COMPLETION=*) _cached_semantic_completion="${_line#SEMANTIC_COMPLETION=}" ;;
          ROOT_FILES=*)  _cached_roots="${_line#ROOT_FILES=}" ;;
          SOURCE_FILES=*) _cached_sources="${_line#SOURCE_FILES=}" ;;
          ROOT_FINGERPRINT=*) _cached_root_fp="${_line#ROOT_FINGERPRINT=}" ;;
          SOURCE_FINGERPRINT=*) _cached_source_fp="${_line#SOURCE_FINGERPRINT=}" ;;
          FILES_REASON=*) _cached_files_reason="${_line#FILES_REASON=}" ;;
          TSC_ROOT_FILES=*) _cached_tsc_roots="${_line#TSC_ROOT_FILES=}" ;;
          TSC_SOURCE_FILES=*) _cached_tsc_sources="${_line#TSC_SOURCE_FILES=}" ;;
          TSC_ROOT_FINGERPRINT=*) _cached_tsc_root_fp="${_line#TSC_ROOT_FINGERPRINT=}" ;;
          TSC_SOURCE_FINGERPRINT=*) _cached_tsc_source_fp="${_line#TSC_SOURCE_FINGERPRINT=}" ;;
          TSC_RC=*)      _cached_tsc_rc="${_line#TSC_RC=}" ;;
          CLASS=*)       _cached_class="${_line#CLASS=}" ;;
          STATUS=*)      _cached_status="${_line#STATUS=}" ;;
          DELTA_START)   _in_delta=1 ;;
        esac
      fi
    done < "$_cache_file"
    local _cached_stats_valid=0
    if [[ "$_cached_roots" =~ ^(0|[1-9][0-9]*)$ \
      && "$_cached_sources" =~ ^(0|[1-9][0-9]*)$ \
      && "$_cached_root_fp" =~ ^[0-9a-f]{64}$ \
      && "$_cached_source_fp" =~ ^[0-9a-f]{64}$ \
      && "$_cached_semantic_completion" == "complete" ]]; then
      _cached_stats_valid=1
    fi
    local _cached_tsc_stats_valid=0
    if [[ -z "$_cached_tsc_roots$_cached_tsc_sources$_cached_tsc_root_fp$_cached_tsc_source_fp" ]]; then
      _cached_tsc_stats_valid=1
    elif [[ "$_cached_tsc_roots" =~ ^(0|[1-9][0-9]*)$ \
      && "$_cached_tsc_sources" =~ ^(0|[1-9][0-9]*)$ \
      && "$_cached_tsc_root_fp" =~ ^[0-9a-f]{64}$ \
      && "$_cached_tsc_source_fp" =~ ^[0-9a-f]{64}$ ]]; then
      _cached_tsc_stats_valid=1
    fi
    if [[ ( "$_cached_class" == "exit success" || "$_cached_class" == "nonzero exit" ) \
      && "$_cached_sources" != "0" ]]; then
      if [[ ! "$_cached_tsc_roots" =~ ^(0|[1-9][0-9]*)$ \
        || ! "$_cached_tsc_sources" =~ ^(0|[1-9][0-9]*)$ \
        || ! "$_cached_tsc_root_fp" =~ ^[0-9a-f]{64}$ \
        || ! "$_cached_tsc_source_fp" =~ ^[0-9a-f]{64}$ ]]; then
        _cached_tsc_stats_valid=0
      fi
      case "$_cached_tsc_rc" in
        0|1|2|3|4) ;;
        *) _cached_tsc_stats_valid=0 ;;
      esac
    fi
    # A cached green verdict must still carry internally exact graph and exit
    # evidence. Corrupt/partial cache text is a miss, never compiler parity.
    if [[ "$_cached_rc" == "0" && ( "$_cached_roots" != "$_cached_tsc_roots" \
      || "$_cached_sources" != "$_cached_tsc_sources" \
      || "$_cached_root_fp" != "$_cached_tsc_root_fp" \
      || "$_cached_source_fp" != "$_cached_tsc_source_fp" \
      || "$_cached_tsz_rc" != "$_cached_tsc_rc" ) ]]; then
      _cached_tsc_stats_valid=0
    fi
    if [[ "$_cached_schema" == "$COMPILE_RESULT_CACHE_SCHEMA" \
      && "$_cached_fp" == "$_fp" && "$_cached_stats_valid" == "1" \
      && "$_cached_tsc_stats_valid" == "1" ]]; then
      LAST_SEMANTIC_COMPLETION="$_cached_semantic_completion"
      echo "::group::${name}"
      echo "(result cache hit: ${_fp:0:12})"
      local _cached_files_reached="$_cached_sources"
      [[ -n "$_cached_files_reason" ]] && _cached_files_reached=""
      if [[ -n "$_cached_files_reason" ]]; then
        echo "(cached compiler stats: ${_cached_files_reason})"
      else
        echo "(cached compiler stats: root_files=${_cached_roots} source_files=${_cached_sources} root_graph=${_cached_root_fp:0:12} source_graph=${_cached_source_fp:0:12} tsc_root_files=${_cached_tsc_roots:-unavailable} tsc_source_files=${_cached_tsc_sources:-unavailable})"
      fi
      if [[ "${_cached_rc:-1}" -ne 0 ]]; then
        FAILURES=$((FAILURES + 1))
        record_project_compatibility \
          "$name" "${_cached_class:-nonzero exit}" "check" \
          "${_cached_status:-$(project_failure_status "${_cached_class:-nonzero exit}")}" \
          "${_cached_delta:-}" "$_cached_files_reached" "" \
          "${_cached_tsz_rc:-1}" "$tsconfig" "$src_dir" \
          "${_cached_tsc_rc:-$tsc_exit_codes}" "$_cached_files_reason"
        echo "error: ${name} failed (cached result)" >&2
        echo "::endgroup::"
        if [[ "$ALLOW_FAILURES" == "1" ]]; then
          echo "::warning::${name} did not compile; continuing because TSZ_PROJECT_COMPILE_ALLOW_FAILURES=1"
        fi
      else
        record_project_compatibility \
          "$name" "${_cached_class:-exit success}" "check" \
          "${_cached_status:-none}" "${_cached_delta:-}" \
          "$_cached_sources" "" "${_cached_tsz_rc:-0}" "$tsconfig" "$src_dir" \
          "${_cached_tsc_rc:-$tsc_exit_codes}"
        echo "${name} compiled successfully."
        echo "::endgroup::"
      fi
      return 0
    fi
  fi

  # On a cache miss, resolve exact pinned-TS7 root and source-graph sequences.
  # Missing either count or fingerprint makes an ordinary compile non-evidence; textual
  # extended-diagnostics totals are never substituted.
  local tsc_root_files="" tsc_source_files="" tsc_root_fp="" tsc_source_fp=""
  run_project_tsc_graph_counts "$name" "$tsconfig" "$src_dir" || true
  tsc_root_files="$LAST_TSC_ROOT_FILES"
  tsc_source_files="$LAST_TSC_SOURCE_FILES"
  tsc_root_fp="$LAST_TSC_ROOT_FINGERPRINT"
  tsc_source_fp="$LAST_TSC_SOURCE_FINGERPRINT"
  local tsc_graph_evidence_valid=0
  if [[ "$tsc_root_files" =~ ^(0|[1-9][0-9]*)$ \
    && "$tsc_source_files" =~ ^(0|[1-9][0-9]*)$ \
    && "$tsc_root_fp" =~ ^[0-9a-f]{64}$ \
    && "$tsc_source_fp" =~ ^[0-9a-f]{64}$ ]]; then
    tsc_graph_evidence_valid=1
  fi

  echo "::group::${name}"
  local canonical_stats stats_file
  if [[ "$PROJECT_PERF_COUNTERS" == "1" ]]; then
    mkdir -p "$FIXTURE_ROOT/perf-counters"
    canonical_stats="$FIXTURE_ROOT/perf-counters/${name}.perf.json"
    echo "Running: $TSZ_RUN_BIN --extendedDiagnostics --perf-counters-json <fresh> --noEmit -p $tsconfig"
  else
    mkdir -p "$FIXTURE_ROOT/compiler-stats"
    canonical_stats="$FIXTURE_ROOT/compiler-stats/${name}.json"
    echo "Running: $TSZ_RUN_BIN --perf-counters-json <fresh> --noEmit -p $tsconfig"
  fi
  rm -f "$canonical_stats" "${canonical_stats}.invalid"
  stats_file="$(mktemp "${canonical_stats}.fresh.XXXXXX")"
  rm -f "$stats_file"

  local rc=0 exit_class="" diagnostic_delta="" timeout_unmeasured=0
  if [[ "$PROJECT_PERF_COUNTERS" == "1" ]]; then
    run_with_timeout "$PROJECT_TIMEOUT" \
      env \
        TSZ_USE_EMBEDDED_LIBS=1 \
        RUST_MIN_STACK="${TSZ_RUST_MIN_STACK:-536870912}" \
        TSZ_PERF_COUNTERS=1 \
        "$TSZ_RUN_BIN" \
        --extendedDiagnostics \
        --perf-counters-json "$stats_file" \
        --noEmit \
        -p "$tsconfig" >"$log" 2>&1 || rc=$?
  else
    run_with_timeout "$PROJECT_TIMEOUT" \
      env \
        TSZ_USE_EMBEDDED_LIBS=1 \
        RUST_MIN_STACK="${TSZ_RUST_MIN_STACK:-536870912}" \
        "$TSZ_RUN_BIN" --perf-counters-json "$stats_file" --noEmit -p "$tsconfig" \
        >"$log" 2>&1 || rc=$?
  fi

  local compiler_rc="$rc" root_files="" source_files="" root_fp="" source_fp="" files_reason=""
  local stats_valid=0 files_reached="" tsc_root_mismatch=0 tsc_source_mismatch=0
  local graph_project_root
  graph_project_root="$(tsz_fingerprint_project_root "$(dirname "$tsconfig")")"
  if read_project_compiler_stats "$stats_file" "$(dirname "$tsconfig")" "$graph_project_root"; then
    stats_valid=1
    root_files="$LAST_ROOT_FILES"
    source_files="$LAST_SOURCE_FILES"
    root_fp="$LAST_ROOT_FILE_FINGERPRINT"
    source_fp="$LAST_SOURCE_FILE_FINGERPRINT"
    files_reached="$source_files"
    mv "$stats_file" "$canonical_stats" 2>/dev/null || true
    echo "Compiler stats: root_files=${root_files} source_files=${source_files} root_graph=${root_fp:0:12} source_graph=${source_fp:0:12} tsc_root_files=${tsc_root_files:-unavailable} tsc_source_files=${tsc_source_files:-unavailable}"
  else
    files_reason="$LAST_COMPILER_STATS_REASON"
    [[ -f "$stats_file" ]] && mv "$stats_file" "${canonical_stats}.invalid" 2>/dev/null || true
    echo "Compiler stats unavailable: ${files_reason}"
  fi

  if [[ "$stats_valid" == "1" && -n "$tsc_root_files" \
    && ( "$root_files" -ne "$tsc_root_files" || "$root_fp" != "$tsc_root_fp" ) ]]; then
    tsc_root_mismatch=1
  fi
  if [[ "$stats_valid" == "1" && -n "$tsc_source_files" \
    && ( "$source_files" -ne "$tsc_source_files" || "$source_fp" != "$tsc_source_fp" ) ]]; then
    tsc_source_mismatch=1
  fi

  local compiler_exit_class=""
  if [[ "$rc" -ne 0 ]]; then
    compiler_exit_class="$(project_failure_class "$([[ "$rc" -eq 124 ]] && echo "timeout" || echo "nonzero exit")" "$rc")"
  fi

  # A diagnostic oracle is evidence in both directions. For a normal nonzero
  # tsz result it removes genuine tsc diagnostics from the false-positive
  # delta. For tsz RC0 it proves pinned TypeScript 7 is also clean; a nonzero
  # tsc result is a tsz false negative and can never become a green row.
  local oracle_consulted=0 diagnostic_oracle_available=0
  local tsz_only_count="" tsc_only_count="" diagnostics_agree=0
  local tsz_diagnostic_count="" tsc_diagnostic_count="" diagnostic_evidence_reason=""
  local oracle_log="" oracle_identity_root="" tsc_false_negative=0
  local success_diagnostic_mismatch=0
  oracle_identity_root="$(tsz_fingerprint_project_root "$(dirname "$tsconfig")")"
  if [[ -n "$tsc_exit_codes" ]]; then
    tsz_diagnostic_count="$(tsz_count_diagnostic_lines "$oracle_identity_root" < "$log")"
    if ! tsz_diagnostic_log_is_covered "$log" "$oracle_identity_root"; then
      diagnostic_evidence_reason="unparsed compiler diagnostic output"
    elif [[ "$tsc_exit_codes" != "0" ]]; then
      diagnostic_evidence_reason="static tsc exit is insufficient diagnostic evidence"
    else
      diagnostic_oracle_available=1
      if [[ "$rc" -eq 0 && "$tsz_diagnostic_count" -eq 0 ]]; then
        diagnostics_agree=1
      elif [[ "$rc" -eq 0 ]]; then
        success_diagnostic_mismatch=1
      fi
    fi
  fi
  if [[ "$stats_valid" == "1" && "$source_files" -gt 0 \
    && -z "$tsc_exit_codes" && "${#TSC_ORACLE_CMD[@]}" -gt 0 \
    && ( "$rc" -eq 0 || "$compiler_exit_class" == "nonzero exit" ) ]]; then
    oracle_log="$FIXTURE_ROOT/${name}.tsc.log"
    if run_project_tsc_oracle "$name" "$tsconfig" "$src_dir" "$oracle_log"; then
      oracle_consulted=1
      tsc_exit_codes="$LAST_TSC_ORACLE_RC"
      tsz_only_count="$(tsz_only_delta_lines "$log" "$oracle_log" "$oracle_identity_root" | tsz_count_diagnostic_lines "$oracle_identity_root")"
      tsc_only_count="$(tsz_only_delta_lines "$oracle_log" "$log" "$oracle_identity_root" | tsz_count_diagnostic_lines "$oracle_identity_root")"
      tsz_diagnostic_count="$(tsz_count_diagnostic_lines "$oracle_identity_root" < "$log")"
      tsc_diagnostic_count="$(tsz_count_diagnostic_lines "$oracle_identity_root" < "$oracle_log")"
      if ! tsz_diagnostic_log_is_covered "$log" "$oracle_identity_root" \
        || ! tsz_diagnostic_log_is_covered "$oracle_log" "$oracle_identity_root"; then
        diagnostic_evidence_reason="unparsed compiler diagnostic output"
      elif { [[ "$rc" -ne 0 && "$tsz_diagnostic_count" -eq 0 ]] \
        || [[ "$LAST_TSC_ORACLE_RC" -ne 0 && "$tsc_diagnostic_count" -eq 0 ]]; }; then
        diagnostic_evidence_reason="nonzero compiler exit without parsed diagnostics"
      else
        diagnostic_oracle_available=1
        if tsz_diagnostic_multisets_agree "$log" "$oracle_log" "$oracle_identity_root"; then
          diagnostics_agree=1
        fi
      fi
      if [[ "$rc" -eq 0 && "$LAST_TSC_ORACLE_RC" -ne 0 ]]; then
        tsc_false_negative=1
      elif [[ "$rc" -eq 0 && "$diagnostics_agree" != "1" ]]; then
        success_diagnostic_mismatch=1
      fi
    fi
  fi
  local oracle_evidence_missing=0
  if [[ "$stats_valid" == "1" && "$source_files" -gt 0 \
    && ( "$rc" -eq 0 || "$compiler_exit_class" == "nonzero exit" ) \
    && ( "$tsc_graph_evidence_valid" != "1" \
      || "$diagnostic_oracle_available" != "1" ) ]]; then
    oracle_evidence_missing=1
  fi

  # `result_rc` is the harness verdict. `compiler_rc` remains the actual tsz
  # exit code recorded in the compatibility artifact.
  local result_rc="$rc" diagnostic_status=""

  if [[ "$stats_valid" != "1" ]]; then
    result_rc=70
    exit_class="runner error"
    diagnostic_status="$files_reason"
    diagnostic_delta="harness: ${files_reason}; --perf-counters-json did not produce schema v2 counts plus exact root/source path sequences"
    local compiler_lines
    compiler_lines="$(diagnostic_lines_from_file "tsz" "$log" 19)"
    [[ -n "$compiler_lines" ]] && diagnostic_delta="${diagnostic_delta}"$'\n'"${compiler_lines}"
    files_reached=""
  elif [[ "$source_files" -eq 0 ]]; then
    result_rc=65
    exit_class="fixture invalid"
    diagnostic_status="zero source files processed"
    files_reason="zero source files processed"
    files_reached=""
    diagnostic_delta="harness: tsz selected ${root_files} root file(s) but processed zero source files; row is non-evidence"
    local zero_lines
    zero_lines="$(diagnostic_lines_from_file "tsz" "$log" 19)"
    [[ -n "$zero_lines" ]] && diagnostic_delta="${diagnostic_delta}"$'\n'"${zero_lines}"
  elif [[ "$oracle_evidence_missing" == "1" ]]; then
    result_rc=69
    exit_class="oracle unavailable"
    diagnostic_status="pinned TypeScript 7 evidence unavailable"
    diagnostic_delta="harness: exact pinned TypeScript 7 project evidence is required"
    if [[ -z "$tsc_root_files" ]]; then
      diagnostic_delta="${diagnostic_delta}"$'\n'"harness: ${LAST_TSC_ROOT_FILES_REASON:-tsc root oracle unavailable}"
    fi
    if [[ -z "$tsc_source_files" ]]; then
      diagnostic_delta="${diagnostic_delta}"$'\n'"harness: ${LAST_TSC_SOURCE_FILES_REASON:-tsc source oracle unavailable}"
    fi
    if [[ "$diagnostic_oracle_available" != "1" ]]; then
      diagnostic_delta="${diagnostic_delta}"$'\n'"harness: ${diagnostic_evidence_reason:-tsc diagnostic oracle unavailable or incomplete}"
    fi
    local unavailable_lines
    unavailable_lines="$(diagnostic_lines_from_file "tsz" "$log" 16)"
    [[ -n "$unavailable_lines" ]] \
      && diagnostic_delta="${diagnostic_delta}"$'\n'"${unavailable_lines}"
  elif [[ "$tsc_root_mismatch" == "1" || "$tsc_source_mismatch" == "1" \
    || "$tsc_false_negative" == "1" || "$success_diagnostic_mismatch" == "1" ]]; then
    result_rc=66
    exit_class="${compiler_exit_class:-exit success}"
    if [[ "$tsc_false_negative" == "1" \
      && ( "$tsc_root_mismatch" == "1" || "$tsc_source_mismatch" == "1" ) ]]; then
      diagnostic_status="project graph and false-negative diagnostic mismatch"
    elif [[ "$tsc_false_negative" == "1" ]]; then
      diagnostic_status="TypeScript 7 diagnostic mismatch (tsz false negative)"
    elif [[ "$success_diagnostic_mismatch" == "1" ]]; then
      diagnostic_status="TypeScript 7 diagnostic mismatch after tsz exit success"
    elif [[ "$tsc_root_mismatch" == "1" && "$tsc_source_mismatch" == "1" ]]; then
      diagnostic_status="project root/source graph diagnostic mismatch"
    elif [[ "$tsc_root_mismatch" == "1" ]]; then
      diagnostic_status="project root-file diagnostic mismatch"
    else
      diagnostic_status="project source-file diagnostic mismatch"
    fi
    diagnostic_delta=""
    if [[ "$tsc_root_mismatch" == "1" ]]; then
      if [[ "$root_files" -ne "$tsc_root_files" ]]; then
        diagnostic_delta="harness: root file count mismatch (tsz=${root_files}, TypeScript7=${tsc_root_files})"
      else
        diagnostic_delta="harness: root path sequence mismatch at equal count ${root_files} (tsz=${root_fp:0:12}, TypeScript7=${tsc_root_fp:0:12})"
      fi
    fi
    if [[ "$tsc_source_mismatch" == "1" ]]; then
      local source_mismatch_line=""
      if [[ "$source_files" -ne "$tsc_source_files" ]]; then
        source_mismatch_line="harness: source file count mismatch (tsz=${source_files}, TypeScript7=${tsc_source_files})"
      else
        source_mismatch_line="harness: source path sequence mismatch at equal count ${source_files} (tsz=${source_fp:0:12}, TypeScript7=${tsc_source_fp:0:12})"
      fi
      [[ -n "$diagnostic_delta" ]] \
        && diagnostic_delta="${diagnostic_delta}"$'\n'"${source_mismatch_line}" \
        || diagnostic_delta="$source_mismatch_line"
    fi
    if [[ "$tsc_false_negative" == "1" ]]; then
      local tsc_false_negative_lines
      tsc_false_negative_lines="$(tsz_label_diagnostic_lines "tsc" "$oracle_log" 18)"
      if [[ -z "$tsc_false_negative_lines" ]]; then
        tsc_false_negative_lines="$(diagnostic_lines_from_file "tsc" "$oracle_log" 18)"
      fi
      [[ -n "$tsc_false_negative_lines" ]] \
        && diagnostic_delta="${diagnostic_delta}"$'\n'"${tsc_false_negative_lines}"
    fi
    if [[ "$success_diagnostic_mismatch" == "1" ]]; then
      local success_delta
      if [[ "$oracle_consulted" == "1" ]]; then
        success_delta="$(tsz_only_and_tsc_context_delta "$log" "$oracle_log" "$oracle_identity_root")"
      else
        success_delta="$(diagnostic_lines_from_file "tsz" "$log" 18)"
      fi
      if [[ -n "$success_delta" ]]; then
        [[ -n "$diagnostic_delta" ]] \
          && diagnostic_delta="${diagnostic_delta}"$'\n'"${success_delta}" \
          || diagnostic_delta="$success_delta"
      fi
    fi
    local mismatch_lines
    if [[ "$success_diagnostic_mismatch" != "1" ]]; then
      mismatch_lines="$(diagnostic_lines_from_file "tsz" "$log" 19)"
    fi
    [[ -n "$mismatch_lines" ]] && diagnostic_delta="${diagnostic_delta}"$'\n'"${mismatch_lines}"
  elif [[ "$rc" -ne 0 ]]; then
    exit_class="$compiler_exit_class"
    diagnostic_status="$(project_failure_status "$exit_class")"
    diagnostic_delta="$(diagnostic_lines_from_file "tsz" "$log")"
    local timeout_note=""
    if [[ "$rc" -eq 124 ]]; then
      timeout_note="$(tsz_timeout_contention_note "$PROJECT_TIMEOUT" \
        "$LAST_TIMEOUT_CPU_SECONDS" "$MIN_CPU_SHARE_PCT")"
      diagnostic_delta="tsz: ${timeout_note}"$'\n'"$(diagnostic_lines_from_file "tsz" "$log" 19)"
      if ! tsz_timeout_is_cpu_bound "$PROJECT_TIMEOUT" "$LAST_TIMEOUT_CPU_SECONDS" "$MIN_CPU_SHARE_PCT"; then
        timeout_unmeasured=1
      fi
    fi

    # tsc oracle: only a plain "nonzero exit" (tsz produced diagnostics, no
    # crash/timeout/OOM) is a candidate for tsz<->tsc parity. Subtract tsc's own
    # diagnostics; the row passes when the tsz-only delta is empty. Crashes,
    # timeouts, and OOMs are failures regardless of what tsc reports.
    #
    # A caller that already passed static tsc exit codes (the type-challenges
    # row, whose dedicated oracle runs and gates before check_project) is left
    # untouched so that row keeps its bespoke oracle and we do not run tsc twice.
    if [[ "$oracle_consulted" == "1" && "$diagnostics_agree" == "1" \
      && "$compiler_rc" -eq "$LAST_TSC_ORACLE_RC" ]]; then
      # tsz matched tsc exactly: symmetric diagnostic multiset and ordinary
      # exit code agree. Record a green/pass
      # row carrying both sides so the artifact shows the agreed-on tsc errors,
      # and do NOT count a gate failure. Only the harness result is normalized
      # to zero; the real nonzero compiler exit is retained in artifacts/cache.
      result_rc=0
      exit_class="exit success"
      diagnostic_status="none"
      diagnostic_delta="$(tsc_and_tsz_oracle_delta "$log" "$oracle_log")"
      record_project_compatibility "$name" "exit success" "check" "none" \
        "$diagnostic_delta" "$files_reached" "$LAST_PEAK_RSS_BYTES" "$compiler_rc" \
        "$tsconfig" "$src_dir" "$tsc_exit_codes"
      if [[ -n "$LAST_TSC_ORACLE_RC" && "$LAST_TSC_ORACLE_RC" != "0" ]]; then
        echo "${name} matches tsc (tsc also reports errors; 0 tsz-only diagnostics)."
      else
        echo "${name} compiled successfully (tsc oracle clean)."
      fi
      echo "::endgroup::"
    else
      if [[ "$oracle_consulted" == "1" ]]; then
        diagnostic_delta="$(tsz_only_and_tsc_context_delta "$log" "$oracle_log" "$oracle_identity_root")"
        if [[ "$diagnostics_agree" != "1" && "${tsz_only_count:-0}" -eq 0 \
          && "${tsc_only_count:-0}" -eq 0 ]]; then
          diagnostic_delta="harness: diagnostic message/continuation ownership mismatch"$'\n'"${diagnostic_delta}"
        fi
        if [[ "$compiler_rc" -ne "$LAST_TSC_ORACLE_RC" ]]; then
          local exit_mismatch_line="harness: compiler exit mismatch (tsz=${compiler_rc}, TypeScript7=${LAST_TSC_ORACLE_RC})"
          [[ -n "$diagnostic_delta" ]] \
            && diagnostic_delta="${exit_mismatch_line}"$'\n'"${diagnostic_delta}" \
            || diagnostic_delta="$exit_mismatch_line"
        fi
      fi
      diagnostic_status="exact diagnostic mismatch or compiler-exit mismatch"
      if [[ "$compiler_rc" -eq 124 ]]; then
        echo "error: ${name} ${timeout_note}" >&2
      elif [[ "$oracle_consulted" == "1" ]]; then
        echo "error: ${name} differs from TypeScript 7 (tsz-only=${tsz_only_count:-unknown}, tsc-only=${tsc_only_count:-unknown}, exits=${compiler_rc}/${LAST_TSC_ORACLE_RC})" >&2
      else
        echo "error: ${name} failed with exit code ${rc}" >&2
      fi
    fi
  else
    result_rc=0
    exit_class="exit success"
    diagnostic_status="none"
  fi

  if [[ "$result_rc" -ne 0 ]]; then
    FAILURES=$((FAILURES + 1))
    record_project_compatibility \
      "$name" "$exit_class" "check" "$diagnostic_status" \
      "$diagnostic_delta" "$files_reached" "$LAST_PEAK_RSS_BYTES" \
      "$compiler_rc" "$tsconfig" "$src_dir" "$tsc_exit_codes" "$files_reason"
    if [[ "$exit_class" == "runner error" ]]; then
      echo "error: ${name} has no trustworthy compiler stats (${files_reason})" >&2
    elif [[ "$exit_class" == "fixture invalid" ]]; then
      echo "error: ${name} processed zero source files; result is non-evidence" >&2
    elif [[ "$exit_class" == "oracle unavailable" ]]; then
      echo "error: ${name} lacks exact pinned TypeScript 7 graph/diagnostic evidence" >&2
    else
      if [[ "$tsc_root_mismatch" == "1" ]]; then
        echo "error: ${name} root graph differs from TypeScript 7 (counts=${root_files}/${tsc_root_files}, paths=${root_fp:0:12}/${tsc_root_fp:0:12})" >&2
      fi
      if [[ "$tsc_source_mismatch" == "1" ]]; then
        echo "error: ${name} source graph differs from TypeScript 7 (counts=${source_files}/${tsc_source_files}, paths=${source_fp:0:12}/${tsc_source_fp:0:12})" >&2
      fi
      if [[ "$tsc_false_negative" == "1" ]]; then
        echo "error: ${name} exited successfully but TypeScript 7 reports diagnostics (tsz false negative)" >&2
      fi
    fi
    sed -n '1,160p' "$log" >&2 || true
    echo "::endgroup::"
    if [[ "$ALLOW_FAILURES" == "1" ]]; then
      echo "::warning::${name} did not produce compatibility evidence; continuing because TSZ_PROJECT_COMPILE_ALLOW_FAILURES=1"
    fi
  elif [[ "$compiler_rc" -eq 0 ]]; then
    record_project_compatibility "$name" "exit success" "check" "none" "" \
      "$files_reached" "$LAST_PEAK_RSS_BYTES" "0" "$tsconfig" "$src_dir" "$tsc_exit_codes"
    echo "${name} compiled successfully."
    echo "::endgroup::"
  fi
  # A timeout without confirmed CPU-bound evidence (contention, or no CPU
  # sample at all) is unmeasured, not a result: caching it would persist a
  # possibly-false failure for as long as the fingerprint stays stable.
  if [[ "$timeout_unmeasured" == "1" ]]; then
    echo "::warning::${name} timeout lacks CPU-bound evidence (contention or missing sample); result not cached"
  elif [[ "$stats_valid" != "1" ]]; then
    echo "::warning::${name} missing/malformed compiler stats are non-evidence; result not cached"
  elif [[ "$oracle_evidence_missing" == "1" ]]; then
    echo "::warning::${name} incomplete pinned TypeScript 7 evidence; result not cached"
  elif [[ -n "$_cache_file" ]]; then
    write_compile_cache "$_fp" "$result_rc" \
      "$compiler_rc" \
      "$root_files" "$source_files" "$root_fp" "$source_fp" "$files_reason" \
      "$tsc_root_files" "$tsc_source_files" "$tsc_root_fp" "$tsc_source_fp" \
      "$tsc_exit_codes" "$exit_class" "$diagnostic_status" "$diagnostic_delta" \
      "$_cache_file"
  fi
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
    ts-pattern-project)
      ensure_git_fixture "ts-pattern" "$TS_PATTERN_REPO" "$TS_PATTERN_REF" "$FIXTURE_ROOT/ts-pattern"
      tsz_write_ts_pattern_config "$FIXTURE_ROOT/ts-pattern/tsconfig.tsz-guard.json"
      check_project "$name" "$FIXTURE_ROOT/ts-pattern/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/ts-pattern/src"
      ;;
    radash-project)
      ensure_git_fixture "radash" "$RADASH_REPO" "$RADASH_REF" "$FIXTURE_ROOT/radash"
      tsz_write_radash_config "$FIXTURE_ROOT/radash/tsconfig.tsz-guard.json"
      check_project "$name" "$FIXTURE_ROOT/radash/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/radash/src"
      ;;
    valtio-project)
      ensure_git_fixture "valtio" "$VALTIO_REPO" "$VALTIO_REF" "$FIXTURE_ROOT/valtio"
      tsz_write_valtio_config "$FIXTURE_ROOT/valtio/tsconfig.tsz-guard.json"
      check_project "$name" "$FIXTURE_ROOT/valtio/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/valtio/src"
      ;;
    scule-project)
      ensure_git_fixture "scule" "$SCULE_REPO" "$SCULE_REF" "$FIXTURE_ROOT/scule"
      tsz_write_scule_config "$FIXTURE_ROOT/scule/tsconfig.tsz-guard.json"
      check_project "$name" "$FIXTURE_ROOT/scule/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/scule/src"
      ;;
    mitt-project)
      ensure_git_fixture "mitt" "$MITT_REPO" "$MITT_REF" "$FIXTURE_ROOT/mitt"
      tsz_write_mitt_config "$FIXTURE_ROOT/mitt/tsconfig.tsz-guard.json"
      check_project "$name" "$FIXTURE_ROOT/mitt/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/mitt/src"
      ;;
    change-case-project)
      ensure_git_fixture "change-case" "$CHANGE_CASE_REPO" "$CHANGE_CASE_REF" "$FIXTURE_ROOT/change-case"
      tsz_write_change_case_config "$FIXTURE_ROOT/change-case/tsconfig.tsz-guard.json"
      check_project "$name" "$FIXTURE_ROOT/change-case/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/change-case/packages/change-case/src"
      ;;
    tiny-invariant-project)
      ensure_git_fixture "tiny-invariant" "$TINY_INVARIANT_REPO" "$TINY_INVARIANT_REF" "$FIXTURE_ROOT/tiny-invariant"
      tsz_write_tiny_invariant_config "$FIXTURE_ROOT/tiny-invariant/tsconfig.tsz-guard.json"
      check_project "$name" "$FIXTURE_ROOT/tiny-invariant/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/tiny-invariant/src"
      ;;
    ts-belt-project)
      ensure_git_fixture "ts-belt" "$TS_BELT_REPO" "$TS_BELT_REF" "$FIXTURE_ROOT/ts-belt"
      tsz_write_ts_belt_config "$FIXTURE_ROOT/ts-belt/tsconfig.tsz-guard.json"
      check_project "$name" "$FIXTURE_ROOT/ts-belt/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/ts-belt/src"
      ;;
    ts-extras-project)
      ensure_git_fixture "ts-extras" "$TS_EXTRAS_REPO" "$TS_EXTRAS_REF" "$FIXTURE_ROOT/ts-extras"
      tsz_write_ts_extras_config "$FIXTURE_ROOT/ts-extras/tsconfig.tsz-guard.json"
      check_project "$name" "$FIXTURE_ROOT/ts-extras/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/ts-extras/source"
      ;;
    superjson-project)
      ensure_git_fixture "superjson" "$SUPERJSON_REPO" "$SUPERJSON_REF" "$FIXTURE_ROOT/superjson"
      tsz_write_superjson_config "$FIXTURE_ROOT/superjson/tsconfig.tsz-guard.json"
      check_project "$name" "$FIXTURE_ROOT/superjson/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/superjson/src"
      ;;
    trpc-project)
      ensure_git_fixture "trpc" "$TRPC_REPO" "$TRPC_REF" "$FIXTURE_ROOT/trpc"
      tsz_write_trpc_config "$FIXTURE_ROOT/trpc/tsconfig.tsz-guard.json"
      check_project "$name" "$FIXTURE_ROOT/trpc/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/trpc/packages/server/src"
      ;;
    tanstack-query-project)
      ensure_git_fixture "tanstack-query" "$TANSTACK_QUERY_REPO" "$TANSTACK_QUERY_REF" "$FIXTURE_ROOT/tanstack-query"
      tsz_write_tanstack_query_config "$FIXTURE_ROOT/tanstack-query/tsconfig.tsz-guard.json"
      check_project "$name" "$FIXTURE_ROOT/tanstack-query/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/tanstack-query/packages/query-core/src"
      ;;
    tanstack-router-project)
      ensure_git_fixture "tanstack-router" "$TANSTACK_ROUTER_REPO" "$TANSTACK_ROUTER_REF" "$FIXTURE_ROOT/tanstack-router"
      tsz_write_tanstack_router_config "$FIXTURE_ROOT/tanstack-router/tsconfig.tsz-guard.json"
      check_project "$name" "$FIXTURE_ROOT/tanstack-router/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/tanstack-router/packages/router-core/src"
      ;;
    zustand-project)
      ensure_git_fixture "zustand" "$ZUSTAND_REPO" "$ZUSTAND_REF" "$FIXTURE_ROOT/zustand"
      tsz_write_zustand_config "$FIXTURE_ROOT/zustand/tsconfig.tsz-guard.json"
      check_project "$name" "$FIXTURE_ROOT/zustand/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/zustand/src"
      ;;
    jotai-project)
      ensure_git_fixture "jotai" "$JOTAI_REPO" "$JOTAI_REF" "$FIXTURE_ROOT/jotai"
      tsz_write_jotai_config "$FIXTURE_ROOT/jotai/tsconfig.tsz-guard.json"
      check_project "$name" "$FIXTURE_ROOT/jotai/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/jotai/src"
      ;;
    fp-ts-project)
      ensure_git_fixture "fp-ts" "$FP_TS_REPO" "$FP_TS_REF" "$FIXTURE_ROOT/fp-ts"
      tsz_write_fp_ts_config "$FIXTURE_ROOT/fp-ts/tsconfig.tsz-guard.json"
      check_project "$name" "$FIXTURE_ROOT/fp-ts/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/fp-ts/src"
      ;;
    io-ts-project)
      ensure_git_fixture "io-ts" "$IO_TS_REPO" "$IO_TS_REF" "$FIXTURE_ROOT/io-ts"
      tsz_write_io_ts_config "$FIXTURE_ROOT/io-ts/tsconfig.tsz-guard.json"
      check_project "$name" "$FIXTURE_ROOT/io-ts/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/io-ts/src"
      ;;
    immer-project)
      ensure_git_fixture "immer" "$IMMER_REPO" "$IMMER_REF" "$FIXTURE_ROOT/immer"
      tsz_write_immer_config "$FIXTURE_ROOT/immer/tsconfig.tsz-guard.json"
      check_project "$name" "$FIXTURE_ROOT/immer/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/immer/src"
      ;;
    remeda-project)
      ensure_git_fixture "remeda" "$REMEDA_REPO" "$REMEDA_REF" "$FIXTURE_ROOT/remeda"
      tsz_write_remeda_config "$FIXTURE_ROOT/remeda/tsconfig.tsz-guard.json"
      check_project "$name" "$FIXTURE_ROOT/remeda/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/remeda/packages/remeda/src"
      ;;
    ts-morph-project)
      ensure_git_fixture "ts-morph" "$TS_MORPH_REPO" "$TS_MORPH_REF" "$FIXTURE_ROOT/ts-morph"
      tsz_write_ts_morph_config "$FIXTURE_ROOT/ts-morph/tsconfig.tsz-guard.json"
      check_project "$name" "$FIXTURE_ROOT/ts-morph/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/ts-morph/packages/ts-morph/src"
      ;;
    arktype-project)
      ensure_git_fixture "arktype" "$ARKTYPE_REPO" "$ARKTYPE_REF" "$FIXTURE_ROOT/arktype"
      tsz_write_arktype_config "$FIXTURE_ROOT/arktype/tsconfig.tsz-guard.json"
      check_project "$name" "$FIXTURE_ROOT/arktype/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/arktype/ark/type"
      ;;
    superstruct-project)
      ensure_git_fixture "superstruct" "$SUPERSTRUCT_REPO" "$SUPERSTRUCT_REF" "$FIXTURE_ROOT/superstruct"
      tsz_write_superstruct_config "$FIXTURE_ROOT/superstruct/tsconfig.tsz-guard.json"
      check_project "$name" "$FIXTURE_ROOT/superstruct/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/superstruct/src"
      ;;
    runtypes-project)
      ensure_git_fixture "runtypes" "$RUNTYPES_REPO" "$RUNTYPES_REF" "$FIXTURE_ROOT/runtypes"
      tsz_write_runtypes_config "$FIXTURE_ROOT/runtypes/tsconfig.tsz-guard.json"
      check_project "$name" "$FIXTURE_ROOT/runtypes/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/runtypes/src"
      ;;
    hotscript-project)
      ensure_git_fixture "hotscript" "$HOTSCRIPT_REPO" "$HOTSCRIPT_REF" "$FIXTURE_ROOT/hotscript"
      tsz_write_hotscript_config "$FIXTURE_ROOT/hotscript/tsconfig.tsz-guard.json"
      check_project "$name" "$FIXTURE_ROOT/hotscript/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/hotscript/src"
      ;;
    typebox-project)
      ensure_git_fixture "typebox" "$TYPEBOX_REPO" "$TYPEBOX_REF" "$FIXTURE_ROOT/typebox"
      tsz_write_typebox_config "$FIXTURE_ROOT/typebox/tsconfig.tsz-guard.json"
      check_project "$name" "$FIXTURE_ROOT/typebox/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/typebox/src"
      ;;
    class-transformer-project)
      ensure_git_fixture "class-transformer" "$CLASS_TRANSFORMER_REPO" "$CLASS_TRANSFORMER_REF" "$FIXTURE_ROOT/class-transformer"
      tsz_write_class_transformer_config "$FIXTURE_ROOT/class-transformer/tsconfig.tsz-guard.json"
      check_project "$name" "$FIXTURE_ROOT/class-transformer/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/class-transformer/src"
      ;;
    type-graphql-project)
      ensure_git_fixture "type-graphql" "$TYPE_GRAPHQL_REPO" "$TYPE_GRAPHQL_REF" "$FIXTURE_ROOT/type-graphql"
      tsz_write_type_graphql_config "$FIXTURE_ROOT/type-graphql/tsconfig.tsz-guard.json"
      check_project "$name" "$FIXTURE_ROOT/type-graphql/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/type-graphql/src"
      ;;
    neverthrow-project)
      ensure_git_fixture "neverthrow" "$NEVERTHROW_REPO" "$NEVERTHROW_REF" "$FIXTURE_ROOT/neverthrow"
      tsz_write_neverthrow_config "$FIXTURE_ROOT/neverthrow/tsconfig.tsz-guard.json"
      check_project "$name" "$FIXTURE_ROOT/neverthrow/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/neverthrow/src"
      ;;
    xstate-project)
      ensure_git_fixture "xstate" "$XSTATE_REPO" "$XSTATE_REF" "$FIXTURE_ROOT/xstate"
      tsz_write_xstate_config "$FIXTURE_ROOT/xstate/tsconfig.tsz-guard.json"
      check_project "$name" "$FIXTURE_ROOT/xstate/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/xstate/packages/core/src"
      ;;
    mobx-project)
      ensure_git_fixture "mobx" "$MOBX_REPO" "$MOBX_REF" "$FIXTURE_ROOT/mobx"
      tsz_write_mobx_config "$FIXTURE_ROOT/mobx/tsconfig.tsz-guard.json"
      check_project "$name" "$FIXTURE_ROOT/mobx/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/mobx/packages/mobx/src"
      ;;
    # --- application canary rows (category:"application"): install deps, compile
    #     with the app's own tsconfig, drop node_modules. See run_application_row. ---
    umami-project)
      run_application_row "umami-project" "umami" "$UMAMI_REPO" "$UMAMI_REF" \
        "pnpm install --frozen-lockfile --ignore-scripts" "." "$(tsz_project_application_tsconfig "$name")" "src"
      ;;
    excalidraw-project)
      run_application_row "excalidraw-project" "excalidraw" "$EXCALIDRAW_REPO" "$EXCALIDRAW_REF" \
        "yarn install --frozen-lockfile --ignore-scripts --ignore-engines" "." "$(tsz_project_application_tsconfig "$name")" "packages/excalidraw"
      ;;
    dub-project)
      run_application_row "dub-project" "dub" "$DUB_REPO" "$DUB_REF" \
        "pnpm install --frozen-lockfile --ignore-scripts" "." "$(tsz_project_application_tsconfig "$name")" "apps/web"
      ;;
    formbricks-project)
      run_application_row "formbricks-project" "formbricks" "$FORMBRICKS_REPO" "$FORMBRICKS_REF" \
        "pnpm install --frozen-lockfile --ignore-scripts" "." "$(tsz_project_application_tsconfig "$name")" "apps/web"
      ;;
    typebot-project)
      run_application_row "typebot-project" "typebot" "$TYPEBOT_REPO" "$TYPEBOT_REF" \
        "bun install --frozen-lockfile --ignore-scripts" "." "$(tsz_project_application_tsconfig "$name")" "apps/builder"
      ;;
    lobe-chat-project)
      run_application_row "lobe-chat-project" "lobe-chat" "$LOBE_CHAT_REPO" "$LOBE_CHAT_REF" \
        "pnpm install --no-frozen-lockfile --ignore-scripts" "." "$(tsz_project_application_tsconfig "$name")" "src"
      ;;
    supabase-studio-project)
      run_application_row "supabase-studio-project" "supabase-studio" "$SUPABASE_STUDIO_REPO" "$SUPABASE_STUDIO_REF" \
        "pnpm install --frozen-lockfile --ignore-scripts" "." "$(tsz_project_application_tsconfig "$name")" "apps/studio"
      ;;
    infisical-project)
      run_application_row "infisical-project" "infisical" "$INFISICAL_REPO" "$INFISICAL_REF" \
        "npm ci --ignore-scripts" "." "$(tsz_project_application_tsconfig "$name")" "frontend"
      ;;
    payload-project)
      run_application_row "payload-project" "payload" "$PAYLOAD_REPO" "$PAYLOAD_REF" \
        "pnpm install --frozen-lockfile --ignore-scripts" "." "$(tsz_project_application_tsconfig "$name")" "packages/payload/src"
      ;;
    medusa-project)
      run_application_row "medusa-project" "medusa" "$MEDUSA_REPO" "$MEDUSA_REF" \
        "yarn install --immutable --mode=skip-build" "." "$(tsz_project_application_tsconfig "$name")" "packages/medusa/src"
      ;;
    outline-project)
      run_application_row "outline-project" "outline" "$OUTLINE_REPO" "$OUTLINE_REF" \
        "yarn install --immutable --mode=skip-build" "." "$(tsz_project_application_tsconfig "$name")" "app"
      ;;
    trigger-dev-project)
      run_application_row "trigger-dev-project" "trigger-dev" "$TRIGGER_DEV_REPO" "$TRIGGER_DEV_REF" \
        "pnpm install --frozen-lockfile --ignore-scripts" "." "$(tsz_project_application_tsconfig "$name")" "apps/webapp/app"
      ;;
    joplin-project)
      run_application_row "joplin-project" "joplin" "$JOPLIN_REPO" "$JOPLIN_REF" \
        "yarn install --immutable --mode=skip-build" "." "$(tsz_project_application_tsconfig "$name")" "packages/app-desktop"
      ;;
    directus-project)
      run_application_row "directus-project" "directus" "$DIRECTUS_REPO" "$DIRECTUS_REF" \
        "pnpm install --frozen-lockfile --ignore-scripts" "." "$(tsz_project_application_tsconfig "$name")" "api/src"
      ;;
    n8n-project)
      run_application_row "n8n-project" "n8n" "$N8N_REPO" "$N8N_REF" \
        "pnpm install --frozen-lockfile --ignore-scripts" "." "$(tsz_project_application_tsconfig "$name")" "packages/cli/src"
      ;;
    cal-com-project)
      run_application_row "cal-com-project" "cal-com" "$CAL_COM_REPO" "$CAL_COM_REF" \
        "yarn install --immutable --mode=skip-build" "." "$(tsz_project_application_tsconfig "$name")" "apps/web"
      ;;
    documenso-project)
      run_application_row "documenso-project" "documenso" "$DOCUMENSO_REPO" "$DOCUMENSO_REF" \
        "npm install --ignore-scripts --no-audit --no-fund" "." "$(tsz_project_application_tsconfig "$name")" "apps/remix"
      ;;
    affine-project)
      run_application_row "affine-project" "affine" "$AFFINE_REPO" "$AFFINE_REF" \
        "yarn install --immutable --mode=skip-build" "." "$(tsz_project_application_tsconfig "$name")" "packages/frontend/core/src"
      ;;
    immich-server-project)
      run_application_row "immich-server-project" "immich-server" "$IMMICH_SERVER_REPO" "$IMMICH_SERVER_REF" \
        "pnpm install --frozen-lockfile --ignore-scripts" "." "$(tsz_project_application_tsconfig "$name")" "server/src"
      ;;
    rocketchat-project)
      run_application_row "rocketchat-project" "rocketchat" "$ROCKETCHAT_REPO" "$ROCKETCHAT_REF" \
        "yarn install --immutable --mode=skip-build" "." "$(tsz_project_application_tsconfig "$name")" "apps/meteor/client"
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

# True when the canary row at 0-based array position $1 belongs to this shard.
# Sharding is keyed on the row's stable index in TSZ_COMPILE_GUARD_CANARY_ROWS
# (a deterministic, ordered list) so the union of all shards equals the full
# set with no overlap, independent of which rows are filtered out by
# should_check_project. When _TSZ_CI_CANARY_SHARD_COUNT is unset (local and
# `all`-set runs) every index is in-shard, preserving the serial behavior.
canary_row_in_shard() {
  local index="$1"
  local count="${_TSZ_CI_CANARY_SHARD_COUNT:-}"
  if [[ -z "$count" ]]; then
    return 0
  fi
  if [[ ! "$count" =~ ^[1-9][0-9]*$ ]]; then
    fail "_TSZ_CI_CANARY_SHARD_COUNT must be a positive integer, got: $count"
  fi
  local shard="${_TSZ_CI_CANARY_SHARD_INDEX:-0}"
  if [[ ! "$shard" =~ ^(0|[1-9][0-9]*)$ ]] || (( shard >= count )); then
    fail "_TSZ_CI_CANARY_SHARD_INDEX must be an integer in [0, $count), got: $shard"
  fi
  if (( index % count == shard )); then
    return 0
  fi
  return 1
}

run_canary_projects() {
  local name
  local index=0
  for name in "${TSZ_COMPILE_GUARD_CANARY_ROWS[@]}"; do
    if canary_row_in_shard "$index" && should_check_project "$name"; then
      if ! run_project_row "$name"; then
        return 1
      fi
    fi
    index=$((index + 1))
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
