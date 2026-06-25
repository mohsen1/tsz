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
# (basename, line, column, code). Sourced so the delta logic has one tested home.
# shellcheck source=scripts/ci/lib/project-tsc-oracle.sh
source "$ROOT_DIR/scripts/ci/lib/project-tsc-oracle.sh"

# Whether to run the per-row tsc oracle to subtract genuine tsc errors from
# tsz's diagnostics before deciding a row's pass/fail. On by default; disable
# with TSZ_PROJECT_COMPILE_TSC_ORACLE=0 (e.g. when no tsc is available and every
# fixture is known tsc-clean). The oracle only runs for rows where tsz exits
# with diagnostics (a "nonzero exit"); crashes, timeouts, and OOMs are failures
# regardless of what tsc does.
TSC_ORACLE_ENABLED="${TSZ_PROJECT_COMPILE_TSC_ORACLE:-1}"
TSC_ORACLE_RESULT_CACHE_DIR="${TSZ_PROJECT_COMPILE_TSC_ORACLE_CACHE_DIR:-$FIXTURE_ROOT/.tsc-oracle-cache}"
# Resolved once: the tsc oracle command words and a content hash of the command
# for the oracle-cache key. Empty TSC_ORACLE_CMD means no oracle is available,
# in which case the gate falls back to counting every tsz diagnostic (the
# pre-oracle behavior) so a missing tsc never silently passes a real FP.
TSC_ORACLE_CMD=()
TSC_ORACLE_CMD_HASH=""
if [[ "$TSC_ORACLE_ENABLED" == "1" ]]; then
  while IFS= read -r _oracle_word; do
    [[ -n "$_oracle_word" ]] && TSC_ORACLE_CMD+=("$_oracle_word")
  done < <(tsz_project_oracle_tsc_command)
  if [[ "${#TSC_ORACLE_CMD[@]}" -gt 0 ]]; then
    TSC_ORACLE_CMD_HASH="$(printf '%s\n' "${TSC_ORACLE_CMD[@]}" | sha256_of_stdin)"
  fi
fi

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

count_ts_files() {
  local src_dir="$1"
  { find "$src_dir" \( -path '*/node_modules/*' -o -path '*/.next/*' \) -prune -o \( -name '*.ts' -o -name '*.tsx' -o -name '*.mts' -o -name '*.cts' \) -type f -print 2>/dev/null || true; } \
    | wc -l | tr -d ' '
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
# self-hosted Cloud Run runner ships only npm — yarn (7 rows), pnpm (10), and bun
# (1) are not on PATH, and npm's DEFAULT global prefix is neither writable nor on
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

  echo "[pm-setup] node=$(command -v node || echo -) npm=$(command -v npm || echo -) corepack=$(command -v corepack || echo -) yarn=$(command -v yarn || echo -) pnpm=$(command -v pnpm || echo -) bun=$(command -v bun || echo -)" >&2
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

# Run (or cache-hit) the per-row tsc oracle. tsc's stdout/stderr is captured to
# the per-row $oracle_log; on return LAST_TSC_ORACLE_RC holds tsc's exit code.
# The oracle is cached on (tsc command, tsconfig content, compiled-source
# identity) — independent of the tsz binary — so the per-row tsc run is skipped
# whenever the fixture and tsc are unchanged, keeping CI cost flat across tsz
# rebuilds. Returns 0 when an oracle log was produced (fresh or cached), 1 when
# no oracle is available so callers can fall back to counting all tsz lines.
LAST_TSC_ORACLE_RC=""
run_project_tsc_oracle() {
  local name="$1" tsconfig="$2" src_dir="$3" oracle_log="$4"
  LAST_TSC_ORACLE_RC=""
  [[ "${#TSC_ORACLE_CMD[@]}" -gt 0 ]] || return 1

  local _ofp="" _ocache=""
  if [[ "${TSZ_PROJECT_COMPILE_TSC_ORACLE_CACHE:-1}" == "1" && -n "$TSC_ORACLE_CMD_HASH" ]]; then
    _ofp="$(tsz_tsc_oracle_fingerprint "$name" "$tsconfig" "$src_dir" "$TSC_ORACLE_CMD_HASH" 2>/dev/null || true)"
    [[ -n "$_ofp" ]] && _ocache="$TSC_ORACLE_RESULT_CACHE_DIR/${name}"
  fi

  if [[ -n "$_ocache" && -f "$_ocache" && -f "${_ocache}.log" ]]; then
    local _cached_ofp=""
    IFS= read -r _cached_ofp < "$_ocache" 2>/dev/null || true
    _cached_ofp="${_cached_ofp#FINGERPRINT=}"
    if [[ "$_cached_ofp" == "$_ofp" ]]; then
      local _cached_orc=""
      _cached_orc="$(awk -F= '/^RC=/{print $2; exit}' "$_ocache" 2>/dev/null || true)"
      cp "${_ocache}.log" "$oracle_log" 2>/dev/null || true
      LAST_TSC_ORACLE_RC="${_cached_orc:-0}"
      echo "(tsc oracle cache hit: ${_ofp:0:12})"
      return 0
    fi
  fi

  local orc=0
  echo "Running tsc oracle: ${TSC_ORACLE_CMD[*]} --noEmit -p $tsconfig"
  run_with_timeout "$PROJECT_TIMEOUT" \
    "${TSC_ORACLE_CMD[@]}" --noEmit -p "$tsconfig" >"$oracle_log" 2>&1 || orc=$?
  LAST_TSC_ORACLE_RC="$orc"

  if [[ -n "$_ocache" ]]; then
    { printf 'FINGERPRINT=%s\nRC=%s\n' "$_ofp" "$orc"; } > "${_ocache}.tmp" 2>/dev/null \
      && mv "${_ocache}.tmp" "$_ocache" 2>/dev/null || true
    cp "$oracle_log" "${_ocache}.log" 2>/dev/null || true
  fi
  return 0
}

check_project() {
  local name="$1"
  local tsconfig="$2"
  local src_dir="${3:-$(dirname "$tsconfig")}"
  local tsc_exit_codes="${4:-}"
  local log="$FIXTURE_ROOT/${name}.log"

  # Result cache: skip recompilation when the tsz binary, tsconfig content, and
  # compiled-source identity are all unchanged from a prior run (see
  # scripts/ci/lib/project-compile-fingerprint.sh). The cache file is named
  # per-project; the stored fingerprint is validated on read so stale entries are
  # overwritten rather than accumulated. Disable with TSZ_PROJECT_COMPILE_RESULT_CACHE=0.
  local _fp="" _cache_file=""
  if [[ "${TSZ_PROJECT_COMPILE_RESULT_CACHE:-1}" == "1" ]]; then
    _fp="$(compute_compile_fingerprint "$name" "$tsconfig" "$src_dir" 2>/dev/null || true)"
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
  echo "Running: $TSZ_RUN_BIN --noEmit -p $tsconfig"
  local rc=0 exit_class="" diagnostic_delta="" timeout_unmeasured=0
  run_with_timeout "$PROJECT_TIMEOUT" \
    env \
      TSZ_USE_EMBEDDED_LIBS=1 \
      RUST_MIN_STACK="${TSZ_RUST_MIN_STACK:-536870912}" \
      "$TSZ_RUN_BIN" --noEmit -p "$tsconfig" >"$log" 2>&1 || rc=$?

  if [[ "$rc" -ne 0 ]]; then
    exit_class="$(project_failure_class "$([[ "$rc" -eq 124 ]] && echo "timeout" || echo "nonzero exit")" "$rc")"
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
    local oracle_consulted=0 tsz_only_count="" oracle_log=""
    if [[ "$exit_class" == "nonzero exit" && -z "$tsc_exit_codes" && "${#TSC_ORACLE_CMD[@]}" -gt 0 ]]; then
      oracle_log="$FIXTURE_ROOT/${name}.tsc.log"
      if run_project_tsc_oracle "$name" "$tsconfig" "$src_dir" "$oracle_log"; then
        oracle_consulted=1
        tsc_exit_codes="$LAST_TSC_ORACLE_RC"
        tsz_only_count="$(tsz_only_delta_lines "$log" "$oracle_log" | tsz_count_diagnostic_lines)"
      fi
    fi

    if [[ "$oracle_consulted" == "1" && "${tsz_only_count:-1}" -eq 0 ]]; then
      # tsz matched tsc exactly: no tsz-only diagnostics. Record a green/pass
      # row carrying both sides so the artifact shows the agreed-on tsc errors,
      # and do NOT count a gate failure. rc is normalized to 0 so the cached
      # decision replays as a pass and the row state derives to green.
      rc=0
      exit_class="exit success"
      diagnostic_delta="$(tsc_and_tsz_oracle_delta "$log" "$oracle_log")"
      record_project_compatibility "$name" "exit success" "check" "none" \
        "$diagnostic_delta" "$file_count" "$LAST_PEAK_RSS_BYTES" "0" \
        "$tsconfig" "$src_dir" "$tsc_exit_codes"
      if [[ -n "$LAST_TSC_ORACLE_RC" && "$LAST_TSC_ORACLE_RC" != "0" ]]; then
        echo "${name} matches tsc (tsc also reports errors; 0 tsz-only diagnostics)."
      else
        echo "${name} compiled successfully (tsc oracle clean)."
      fi
      echo "::endgroup::"
    else
      FAILURES=$((FAILURES + 1))
      if [[ "$oracle_consulted" == "1" ]]; then
        # Report only the tsz-only diagnostics as the actionable delta, with the
        # tsc context preserved for triage.
        diagnostic_delta="$(tsz_only_and_tsc_context_delta "$log" "$oracle_log")"
      fi
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
        echo "error: ${name} ${timeout_note}" >&2
      elif [[ "$oracle_consulted" == "1" ]]; then
        echo "error: ${name} has ${tsz_only_count} tsz-only diagnostic(s) tsc does not report" >&2
      else
        echo "error: ${name} failed with exit code ${rc}" >&2
      fi
      sed -n '1,160p' "$log" >&2 || true
      echo "::endgroup::"
      if [[ "$ALLOW_FAILURES" == "1" ]]; then
        echo "::warning::${name} did not compile; continuing because TSZ_PROJECT_COMPILE_ALLOW_FAILURES=1"
      fi
    fi
  else
    record_project_compatibility "$name" "exit success" "check" "none" "" \
      "$file_count" "$LAST_PEAK_RSS_BYTES" "0" "$tsconfig" "$src_dir" "$tsc_exit_codes"
    echo "${name} compiled successfully."
    echo "::endgroup::"
  fi
  # A timeout without confirmed CPU-bound evidence (contention, or no CPU
  # sample at all) is unmeasured, not a result: caching it would persist a
  # possibly-false failure for as long as the fingerprint stays stable.
  if [[ "$timeout_unmeasured" == "1" ]]; then
    echo "::warning::${name} timeout lacks CPU-bound evidence (contention or missing sample); result not cached"
  elif [[ -n "$_cache_file" ]]; then
    write_compile_cache "$_fp" "$rc" "$file_count" "$exit_class" "$diagnostic_delta" "$_cache_file"
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
    tslib-project)
      ensure_git_fixture "tslib" "$TSLIB_REPO" "$TSLIB_REF" "$FIXTURE_ROOT/tslib"
      tsz_write_tslib_config "$FIXTURE_ROOT/tslib/tsconfig.tsz-guard.json"
      check_project "$name" "$FIXTURE_ROOT/tslib/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/tslib/modules"
      ;;
    eventemitter3-project)
      ensure_git_fixture "eventemitter3" "$EVENTEMITTER3_REPO" "$EVENTEMITTER3_REF" "$FIXTURE_ROOT/eventemitter3"
      tsz_write_eventemitter3_config "$FIXTURE_ROOT/eventemitter3/tsconfig.tsz-guard.json"
      check_project "$name" "$FIXTURE_ROOT/eventemitter3/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/eventemitter3"
      ;;
    yocto-queue-project)
      ensure_git_fixture "yocto-queue" "$YOCTO_QUEUE_REPO" "$YOCTO_QUEUE_REF" "$FIXTURE_ROOT/yocto-queue"
      tsz_write_yocto_queue_config "$FIXTURE_ROOT/yocto-queue/tsconfig.tsz-guard.json"
      check_project "$name" "$FIXTURE_ROOT/yocto-queue/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/yocto-queue"
      ;;
    p-limit-project)
      ensure_git_fixture "p-limit" "$P_LIMIT_REPO" "$P_LIMIT_REF" "$FIXTURE_ROOT/p-limit"
      tsz_write_p_limit_config "$FIXTURE_ROOT/p-limit/tsconfig.tsz-guard.json"
      check_project "$name" "$FIXTURE_ROOT/p-limit/tsconfig.tsz-guard.json" "$FIXTURE_ROOT/p-limit"
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
        "pnpm install --frozen-lockfile --ignore-scripts" "." "tsconfig.json" "src"
      ;;
    excalidraw-project)
      run_application_row "excalidraw-project" "excalidraw" "$EXCALIDRAW_REPO" "$EXCALIDRAW_REF" \
        "yarn install --frozen-lockfile --ignore-scripts" "." "packages/excalidraw/tsconfig.json" "packages/excalidraw"
      ;;
    dub-project)
      run_application_row "dub-project" "dub" "$DUB_REPO" "$DUB_REF" \
        "pnpm install --frozen-lockfile --ignore-scripts" "." "apps/web/tsconfig.json" "apps/web"
      ;;
    formbricks-project)
      run_application_row "formbricks-project" "formbricks" "$FORMBRICKS_REPO" "$FORMBRICKS_REF" \
        "pnpm install --frozen-lockfile --ignore-scripts" "." "apps/web/tsconfig.json" "apps/web"
      ;;
    typebot-project)
      run_application_row "typebot-project" "typebot" "$TYPEBOT_REPO" "$TYPEBOT_REF" \
        "bun install --frozen-lockfile" "." "apps/builder/tsconfig.json" "apps/builder"
      ;;
    lobe-chat-project)
      run_application_row "lobe-chat-project" "lobe-chat" "$LOBE_CHAT_REPO" "$LOBE_CHAT_REF" \
        "pnpm install --frozen-lockfile --ignore-scripts" "." "tsconfig.json" "src"
      ;;
    supabase-studio-project)
      run_application_row "supabase-studio-project" "supabase-studio" "$SUPABASE_STUDIO_REPO" "$SUPABASE_STUDIO_REF" \
        "pnpm install --frozen-lockfile --ignore-scripts" "." "apps/studio/tsconfig.json" "apps/studio"
      ;;
    infisical-project)
      run_application_row "infisical-project" "infisical" "$INFISICAL_REPO" "$INFISICAL_REF" \
        "npm ci --ignore-scripts" "." "frontend/tsconfig.json" "frontend"
      ;;
    payload-project)
      run_application_row "payload-project" "payload" "$PAYLOAD_REPO" "$PAYLOAD_REF" \
        "pnpm install --frozen-lockfile --ignore-scripts" "." "packages/payload/tsconfig.json" "packages/payload/src"
      ;;
    medusa-project)
      run_application_row "medusa-project" "medusa" "$MEDUSA_REPO" "$MEDUSA_REF" \
        "yarn install --immutable --mode=skip-build" "." "packages/medusa/tsconfig.json" "packages/medusa/src"
      ;;
    outline-project)
      run_application_row "outline-project" "outline" "$OUTLINE_REPO" "$OUTLINE_REF" \
        "yarn install --immutable --mode=skip-build" "." "tsconfig.json" "app"
      ;;
    trigger-dev-project)
      run_application_row "trigger-dev-project" "trigger-dev" "$TRIGGER_DEV_REPO" "$TRIGGER_DEV_REF" \
        "pnpm install --frozen-lockfile --ignore-scripts" "." "apps/webapp/tsconfig.json" "apps/webapp/app"
      ;;
    joplin-project)
      run_application_row "joplin-project" "joplin" "$JOPLIN_REPO" "$JOPLIN_REF" \
        "yarn install --immutable --mode=skip-build" "." "packages/app-desktop/tsconfig.json" "packages/app-desktop"
      ;;
    directus-project)
      run_application_row "directus-project" "directus" "$DIRECTUS_REPO" "$DIRECTUS_REF" \
        "pnpm install --frozen-lockfile --ignore-scripts" "." "api/tsconfig.json" "api/src"
      ;;
    n8n-project)
      run_application_row "n8n-project" "n8n" "$N8N_REPO" "$N8N_REF" \
        "pnpm install --frozen-lockfile --ignore-scripts" "." "packages/cli/tsconfig.json" "packages/cli/src"
      ;;
    cal-com-project)
      run_application_row "cal-com-project" "cal-com" "$CAL_COM_REPO" "$CAL_COM_REF" \
        "yarn install --immutable --mode=skip-build" "." "apps/web/tsconfig.json" "apps/web"
      ;;
    documenso-project)
      run_application_row "documenso-project" "documenso" "$DOCUMENSO_REPO" "$DOCUMENSO_REF" \
        "npm ci --ignore-scripts" "." "apps/remix/tsconfig.json" "apps/remix"
      ;;
    affine-project)
      run_application_row "affine-project" "affine" "$AFFINE_REPO" "$AFFINE_REF" \
        "yarn install --immutable --mode=skip-build" "." "packages/frontend/core/tsconfig.json" "packages/frontend/core/src"
      ;;
    immich-server-project)
      run_application_row "immich-server-project" "immich-server" "$IMMICH_SERVER_REPO" "$IMMICH_SERVER_REF" \
        "pnpm install --frozen-lockfile --ignore-scripts" "." "server/tsconfig.json" "server/src"
      ;;
    rocketchat-project)
      run_application_row "rocketchat-project" "rocketchat" "$ROCKETCHAT_REPO" "$ROCKETCHAT_REF" \
        "yarn install --immutable --mode=skip-build" "." "apps/meteor/tsconfig.json" "apps/meteor/client"
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
