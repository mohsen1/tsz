# shellcheck shell=bash
record_project_compatibility() {
    local name="$1"
    local exit_class="$2"
    local phase="$3"
    local diagnostic_status="$4"
    local diagnostic_delta="${5:-}"
    local files_reached="${6-}"
    local peak_memory_bytes="${7:-}"
    local tsc_exit_codes="${8:-}"
    local tsz_exit_codes="${9:-}"
    local tsgo_exit_codes="${10:-}"
    local tsconfig_path="${11:-}"
    local source_root="${12:-}"
    local fixture_sources
    local tsz_command_env_prefix=""
    local files_reached_reason=""

    [ -z "$PROJECT_COMPATIBILITY_JSONL" ] && return
    fixture_sources="$(tsz_project_fixture_sources "$name")"
    if [ "$name" = "large-ts-repo" ] && [ -n "${LARGE_TS_NODE_OPTIONS:-}" ]; then
        tsz_command_env_prefix="${tsz_command_env_prefix:+$tsz_command_env_prefix }NODE_OPTIONS=$LARGE_TS_NODE_OPTIONS"
    fi
    if [ -n "${TSZ_USE_EMBEDDED_LIBS:-}" ]; then
        tsz_command_env_prefix="${tsz_command_env_prefix:+$tsz_command_env_prefix }TSZ_USE_EMBEDDED_LIBS=$TSZ_USE_EMBEDDED_LIBS"
    fi
    if [ -n "${TSZ_LIB_DIR:-}" ]; then
        tsz_command_env_prefix="${tsz_command_env_prefix:+$tsz_command_env_prefix }TSZ_LIB_DIR=$TSZ_LIB_DIR"
    fi
    if [ -n "${TSZ_RUST_MIN_STACK:-}" ]; then
        tsz_command_env_prefix="${tsz_command_env_prefix:+$tsz_command_env_prefix }RUST_MIN_STACK=$TSZ_RUST_MIN_STACK"
    fi
    if [ -z "$files_reached" ]; then
        files_reached_reason="${PROJECT_EVIDENCE_FILES_REACHED_REASON:-runner did not count}"
    fi

    local peak_memory_bytes_reason=""
    if [ -z "$peak_memory_bytes" ]; then
        peak_memory_bytes_reason="$(peak_rss_unavailable_reason)"
        if [ -z "$peak_memory_bytes_reason" ]; then
            peak_memory_bytes_reason="process exited before sampling"
        fi
    fi

    COMPAT_JSONL_FILE="$PROJECT_COMPATIBILITY_JSONL" \
    COMPAT_OUTPUT_ROOT="$TEMP_DIR" \
    COMPAT_FIXTURE_ROOT="$EXTERNAL_BENCH_DIR" \
    COMPAT_NAME="$name" \
    COMPAT_EXIT_CLASS="$exit_class" \
    COMPAT_PHASE="$phase" \
    COMPAT_DIAGNOSTIC_STATUS="$diagnostic_status" \
    COMPAT_DIAGNOSTIC_DELTA="$diagnostic_delta" \
    COMPAT_FILES_REACHED="$files_reached" \
    COMPAT_FILES_REACHED_REASON="$files_reached_reason" \
    COMPAT_PEAK_MEMORY_BYTES="$peak_memory_bytes" \
    COMPAT_PEAK_MEMORY_BYTES_REASON="$peak_memory_bytes_reason" \
    COMPAT_TSC_EXIT_CODES="$tsc_exit_codes" \
    COMPAT_TSZ_EXIT_CODES="$tsz_exit_codes" \
    COMPAT_TSGO_EXIT_CODES="$tsgo_exit_codes" \
    COMPAT_TSCONFIG_PATH="$tsconfig_path" \
    COMPAT_SOURCE_ROOT="$source_root" \
    COMPAT_TSZ_COMMAND_ENV_PREFIX="$tsz_command_env_prefix" \
    COMPAT_FIXTURE_SOURCES="$fixture_sources" \
    COMPAT_EVIDENCE_SCHEMA="${PROJECT_EVIDENCE_SCHEMA:-}" \
    COMPAT_SEMANTIC_COMPLETION="${PROJECT_EVIDENCE_SEMANTIC_COMPLETION:-}" \
    COMPAT_ROOT_FILES="${PROJECT_EVIDENCE_TSZ_ROOT_FILES:-}" \
    COMPAT_SOURCE_FILES="${PROJECT_EVIDENCE_TSZ_SOURCE_FILES:-}" \
    COMPAT_ROOT_FILE_FINGERPRINT="${PROJECT_EVIDENCE_TSZ_ROOT_FINGERPRINT:-}" \
    COMPAT_SOURCE_FILE_FINGERPRINT="${PROJECT_EVIDENCE_TSZ_SOURCE_FINGERPRINT:-}" \
    COMPAT_ORACLE_ROOT_FILES="${PROJECT_EVIDENCE_TSC_ROOT_FILES:-}" \
    COMPAT_ORACLE_SOURCE_FILES="${PROJECT_EVIDENCE_TSC_SOURCE_FILES:-}" \
    COMPAT_ORACLE_ROOT_FILE_FINGERPRINT="${PROJECT_EVIDENCE_TSC_ROOT_FINGERPRINT:-}" \
    COMPAT_ORACLE_SOURCE_FILE_FINGERPRINT="${PROJECT_EVIDENCE_TSC_SOURCE_FINGERPRINT:-}" \
    COMPAT_DIAGNOSTIC_RECORDS="${PROJECT_EVIDENCE_TSZ_DIAGNOSTIC_RECORDS:-}" \
    COMPAT_DIAGNOSTIC_FINGERPRINT="${PROJECT_EVIDENCE_TSZ_DIAGNOSTIC_FINGERPRINT:-}" \
    COMPAT_ORACLE_DIAGNOSTIC_RECORDS="${PROJECT_EVIDENCE_TSC_DIAGNOSTIC_RECORDS:-}" \
    COMPAT_ORACLE_DIAGNOSTIC_FINGERPRINT="${PROJECT_EVIDENCE_TSC_DIAGNOSTIC_FINGERPRINT:-}" \
    COMPAT_STUB_INVENTORY_SCHEMA="${PROJECT_EVIDENCE_STUB_INVENTORY_SCHEMA:-}" \
    COMPAT_STUBBED_MODULES="${PROJECT_EVIDENCE_STUBBED_MODULES:-}" \
    COMPAT_STUBBED_ANY_MEMBERS="${PROJECT_EVIDENCE_STUBBED_ANY_MEMBERS:-}" \
    COMPAT_STUB_INVENTORY_FINGERPRINT="${PROJECT_EVIDENCE_STUB_INVENTORY_FINGERPRINT:-}" \
    node "$PROJECT_ROOT/scripts/ci/project-compatibility.mjs" record
}

first_line() {
    local text="$1"
    printf '%s' "${text%%$'\n'*}"
}

# run_isolated <label> <command...>
#
# Runs a fixture function and swallows any non-zero exit so one bad fixture
# (network blip on git clone, pnpm install flake, OOM during a project bench,
# tsgo segfault on large-ts-repo) doesn't abort the entire bench run.
# Records a degraded CSV row so the failure surfaces in the published JSON
# instead of vanishing silently.
#
# Note: bash suspends `set -e` inside functions called from a conditional
# context (cmd || ..., cmd && ..., if cmd, etc.), so commands inside the
# fixture that fail individually won't abort the function automatically.
# This matches the prior `fixture_call || true` behavior; it just adds
# logging + a tracked failure row.
run_isolated() {
    local label="$1"; shift
    local rc=0
    "$@" || rc=$?
    if (( rc != 0 )); then
        echo -e "${YELLOW}warning:${NC} fixture '$label' exited rc=$rc — continuing with remaining benchmarks" >&2
        record_fixture_failure "$label" "$rc"
    fi
    return 0
}

is_project_compatibility_row() {
    local candidate="$1"
    # Fast path: use the compat row set pre-loaded from project-rows.mjs by
    # project-fixtures.sh at module init, avoiding a Node.js process spawn.
    if [[ -n "${_TSZ_PACKED_COMPAT_ROWS:-}" ]]; then
        [[ "|${_TSZ_PACKED_COMPAT_ROWS}|" == *"|${candidate}|"* ]]
        return
    fi
    # Fallback: spawn Node.js (used when project-fixtures.sh was not sourced or
    # when _TSZ_PACKED_COMPAT_ROWS was not populated).
    command -v node >/dev/null 2>&1 || return 1

    PROJECT_ROW_NAME="$candidate" \
    TSZ_PROJECT_ROWS_MJS="$PROJECT_ROOT/scripts/bench/project-rows.mjs" \
    node --input-type=module <<'NODE'
import { pathToFileURL } from "node:url";

const rowModule = await import(pathToFileURL(process.env.TSZ_PROJECT_ROWS_MJS));
const compatibilityRows = new Set([
  ...rowModule.REQUIRED_PROJECT_ROWS,
  ...rowModule.COMPILE_CANARY_PROJECT_ROWS,
]);
process.exit(compatibilityRows.has(process.env.PROJECT_ROW_NAME) ? 0 : 1);
NODE
}

# Append a degraded row to RESULTS_CSV when a fixture group fails outright
# (no individual benchmarks were recorded). The schema matches existing error
# rows: name, lines, kb, tsz_ms, tsgo_ms, tsz_lps, tsgo_lps, winner, ratio, status.
record_fixture_failure() {
    local label="$1" rc="$2"
    if is_project_compatibility_row "$label"; then
        record_project_compatibility \
            "$label" \
            "runner error" \
            "fixture setup" \
            "fixture failed" \
            "fixture failed before project benchmark recorded compatibility (rc=${rc})" \
            "0" \
            "" \
            "" \
            "" \
            "" \
            "" \
            ""
    fi
    RESULTS_CSV="${RESULTS_CSV}${label},0,0,ERR,ERR,N/A,N/A,error,0,fixture failed (rc=${rc})\n"
}

record_benchmark_source() {
    local name="$1"
    local file="$2"
    [ -z "${BENCHMARK_SOURCES_JSONL:-}" ] && return
    [ ! -f "$file" ] && return

    SOURCE_NAME="$name" \
    SOURCE_FILE="$file" \
    PROJECT_ROOT_VALUE="$PROJECT_ROOT" \
    UTILITY_TYPES_DIR_VALUE="$UTILITY_TYPES_DIR" \
    UTILITY_TYPES_REF_VALUE="$UTILITY_TYPES_REF" \
    TS_TOOLBELT_DIR_VALUE="$TS_TOOLBELT_DIR" \
    TS_TOOLBELT_REF_VALUE="$TS_TOOLBELT_REF" \
    TS_ESSENTIALS_DIR_VALUE="$TS_ESSENTIALS_DIR" \
    TS_ESSENTIALS_REF_VALUE="$TS_ESSENTIALS_REF" \
    BENCHMARK_SOURCES_JSONL_VALUE="$BENCHMARK_SOURCES_JSONL" \
    node <<'NODE'
const crypto = require("node:crypto");
const fs = require("node:fs");
const path = require("node:path");

const name = process.env.SOURCE_NAME || "";
const file = process.env.SOURCE_FILE || "";
const root = process.env.PROJECT_ROOT_VALUE || "";
const out = process.env.BENCHMARK_SOURCES_JSONL_VALUE || "";

function relativeIfInside(base, target) {
  if (!base) return null;
  const rel = path.relative(base, target);
  return rel && !rel.startsWith("..") && !path.isAbsolute(rel) ? rel.split(path.sep).join("/") : null;
}

function originFor(absPath) {
  const rootRel = relativeIfInside(root, absPath);
  if (rootRel?.startsWith("TypeScript/")) {
    return { origin: "typescript", path: rootRel };
  }

  const external = [
    ["utility-types", process.env.UTILITY_TYPES_DIR_VALUE, process.env.UTILITY_TYPES_REF_VALUE],
    ["ts-toolbelt", process.env.TS_TOOLBELT_DIR_VALUE, process.env.TS_TOOLBELT_REF_VALUE],
    ["ts-essentials", process.env.TS_ESSENTIALS_DIR_VALUE, process.env.TS_ESSENTIALS_REF_VALUE],
  ];
  for (const [origin, dir, ref] of external) {
    const rel = relativeIfInside(dir || "", absPath);
    if (rel) return { origin, ref: ref || null, path: `${origin}/${rel}` };
  }

  if (rootRel) return { origin: "workspace", path: rootRel };
  return { origin: "generated", path: path.basename(absPath) };
}

try {
  const absPath = path.resolve(file);
  const content = fs.readFileSync(absPath, "utf8").replace(/\s+$/u, "");
  const source = originFor(absPath);
  fs.appendFileSync(out, `${JSON.stringify({
    name,
    source: {
      ...source,
      sha256: crypto.createHash("sha256").update(content).digest("hex"),
      content,
    },
  })}\n`, "utf8");
} catch {
  // Source metadata improves website robustness but must never break a bench run.
}
NODE
}

run_benchmark() {
    local name="$1"
    local file="$2"
    local extra_args="${3:-}"

    # Skip if filter is set and name doesn't match
    if [ -n "$FILTER" ] && ! echo "$name" | grep -qE "$FILTER"; then
        return
    fi

    BENCHMARKS_RUN=$((BENCHMARKS_RUN + 1))

    # Read the fixture once via `wc -lc` (newline + byte counts in a single
    # pass) instead of opening the same source twice (`wc -l` then `wc -c`); wc
    # prints requested counts in the canonical lines-then-bytes order. `|| true`
    # keeps a missing/unreadable fixture (empty output -> `read` hits EOF) from
    # aborting under `set -e`; the zeroed defaults preserve the prior
    # "0 lines, 0KB" behavior for a vanished fixture.
    local lines bytes
    read -r lines bytes < <(wc -lc < "$file" 2>/dev/null) || true
    lines="${lines:-0}"
    bytes="${bytes:-0}"
    local kb=$((bytes / 1024))
    local info="${lines} lines, ${kb}KB"

    # Benchmark fixtures must be valid TypeScript for the reference compiler.
    # If tsc fails or times out, treat the fixture as invalid benchmark input and
    # skip it. Capture combined output from this one validation run so a fixture
    # error can be surfaced without a second `tsc` invocation that re-parses the
    # same source (run_project_benchmark already reads diagnostics from its
    # captured check log rather than re-running the compiler).
    local tsc_check=0
    local tsc_output=""
    tsc_output="$(run_with_timeout "$BENCH_TIMEOUT" $TSC --noEmit $extra_args "$file" 2>&1)" || tsc_check=$?
    if [ "$tsc_check" -ne 0 ]; then
        if [ "$tsc_check" -eq 124 ]; then
            echo -e "${YELLOW}$name${NC} - ${YELLOW}SKIP${NC} (tsc timeout after ${BENCH_TIMEOUT}s)"
        else
            local tsc_error
            tsc_error="$(first_line "$tsc_output")"
            echo -e "${YELLOW}$name${NC} - ${YELLOW}SKIP${NC} (tsc fixture error)"
            echo -e "  ${CYAN}tsc error:${NC} $tsc_error" >&2
        fi
        return
    fi

    record_benchmark_source "$name" "$file"

    # Pre-validate with timeout: record errors/timeouts in summary table. As
    # with tsc above, capture each compiler's output here so the error branch
    # can report diagnostics without re-running (re-parsing) the fixture.
    local tsz_check=0
    local tsz_output=""
    tsz_output="$(run_with_timeout "$BENCH_TIMEOUT" ${TSZ_LIB_DIR:+env TSZ_LIB_DIR="$TSZ_LIB_DIR"} $TSZ --noEmit $extra_args "$file" 2>&1)" || tsz_check=$?
    local tsgo_check=0
    local tsgo_output=""
    tsgo_output="$(run_with_timeout "$BENCH_TIMEOUT" $TSGO --noEmit $extra_args "$file" 2>&1)" || tsgo_check=$?

    if [ "$tsz_check" -ne 0 ] || [ "$tsgo_check" -ne 0 ]; then
        local status=""
        local tsz_ms="N/A"
        local tsgo_ms="N/A"
        local tsz_lps="N/A"
        local tsgo_lps="N/A"
        local winner="error"
        local ratio="0"

        echo -e "${YELLOW}$name${NC} - ${RED}ERROR${NC}"

        if [ "$tsz_check" -eq 124 ]; then
            status="tsz timeout"
            tsz_ms="TIMEOUT"
            echo -e "  ${CYAN}tsz:${NC} timed out after ${BENCH_TIMEOUT}s" >&2
        elif [ "$tsz_check" -ne 0 ]; then
            status="tsz error"
            tsz_ms="ERR"
            local tsz_error
            tsz_error="$(first_line "$tsz_output")"
            echo -e "  ${CYAN}tsz error:${NC} $tsz_error" >&2
        fi

        if [ "$tsgo_check" -eq 124 ]; then
            status="${status:+${status}; }tsgo timeout"
            tsgo_ms="TIMEOUT"
            echo -e "  ${CYAN}tsgo:${NC} timed out after ${BENCH_TIMEOUT}s" >&2
        elif [ "$tsgo_check" -ne 0 ]; then
            status="${status:+${status}; }tsgo error"
            tsgo_ms="ERR"
            local tsgo_error
            tsgo_error="$(first_line "$tsgo_output")"
            echo -e "  ${CYAN}tsgo error:${NC} $tsgo_error" >&2
        fi

        status="${status:+${status}; }tsc ok"

        RESULTS_CSV="${RESULTS_CSV}${name},${lines},${kb},${tsz_ms},${tsgo_ms},${tsz_lps},${tsgo_lps},${winner},${ratio},${status}\n"
        return
    fi

    echo -e "${GREEN}$name${NC} ($info)"

    # Run benchmark and capture JSON output.
    # Wrap commands with the repo timeout runner to kill runs that hit infinite loops.
    # Normal single-file runs complete in <5s, so 15s is generous.
    # Use --ignore-failure so hyperfine continues even if a rare iteration is killed.
    local run_timeout=15
    local json_file=$(mktemp)
    local hyperfine_output=""
    local hyperfine_status=0
    hyperfine_output="$(hyperfine \
        --warmup "$WARMUP" \
        --min-runs "$MIN_RUNS" \
        --max-runs "$MAX_RUNS" \
        --style full \
        --ignore-failure \
        --export-json "$json_file" \
        -n "tsz" "bash $BENCH_TIMEOUT_RUNNER $run_timeout -- ${TSZ_LIB_DIR:+env TSZ_LIB_DIR=$TSZ_LIB_DIR} $TSZ --noEmit $extra_args $file 2>/dev/null" \
        -n "tsgo" "bash $BENCH_TIMEOUT_RUNNER $run_timeout -- $TSGO --noEmit $extra_args $file 2>/dev/null" 2>&1)" || hyperfine_status=$?
    if [ "$hyperfine_status" -ne 0 ]; then
        printf '%s\n' "$hyperfine_output"
        local status="hyperfine error"
        RESULTS_CSV="${RESULTS_CSV}${name},${lines},${kb},ERR,ERR,N/A,N/A,error,0,${status}\n"
        rm -f "$json_file"
        return
    fi

    # Extract times and calculate throughput
    if [ -f "$json_file" ] && command -v jq &>/dev/null; then
        local tsz_exit_status
        local tsgo_exit_status
        tsz_exit_status="$(hyperfine_exit_status_for "$json_file" "tsz" || true)"
        tsgo_exit_status="$(hyperfine_exit_status_for "$json_file" "tsgo" || true)"
        local hyperfine_ok=true
        [ "$tsz_exit_status" != "ok" ] && hyperfine_ok=false
        [ "$tsgo_exit_status" != "ok" ] && hyperfine_ok=false
        print_hyperfine_comparison_output "$hyperfine_output" "$hyperfine_ok"
        if [ "$hyperfine_ok" != true ]; then
            local status=""
            [ "$tsz_exit_status" != "ok" ] && status="tsz ${tsz_exit_status}"
            [ "$tsgo_exit_status" != "ok" ] && status="${status:+${status}; }tsgo ${tsgo_exit_status}"
            echo -e "${YELLOW}$name${NC} - ${RED}ERROR${NC} (${status})" >&2
            RESULTS_CSV="${RESULTS_CSV}${name},${lines},${kb},ERR,ERR,N/A,N/A,error,0,${status}\n"
            rm -f "$json_file"
            return
        fi

        local tsz_mean=$(hyperfine_mean_for "$json_file" "tsz")
        local tsgo_mean=$(hyperfine_mean_for "$json_file" "tsgo")

        if [ -n "$tsz_mean" ] && [ -n "$tsgo_mean" ] && [ "$tsz_mean" != "0" ] && [ "$tsgo_mean" != "0" ]; then
            # Calculate throughput (lines/sec) and format times (2 decimal places)
            local tsz_lps=$(printf "%.0f" "$(echo "$lines / $tsz_mean" | bc -l 2>/dev/null)" 2>/dev/null || echo "N/A")
            local tsgo_lps=$(printf "%.0f" "$(echo "$lines / $tsgo_mean" | bc -l 2>/dev/null)" 2>/dev/null || echo "N/A")
            local tsz_ms=$(printf "%.2f" "$(echo "$tsz_mean * 1000" | bc -l 2>/dev/null)" 2>/dev/null || echo "N/A")
            local tsgo_ms=$(printf "%.2f" "$(echo "$tsgo_mean * 1000" | bc -l 2>/dev/null)" 2>/dev/null || echo "N/A")

            # Determine winner and calculate speedup ratio
            local winner="tsgo"
            local ratio
            if (( $(echo "$tsz_mean < $tsgo_mean" | bc -l) )); then
                winner="tsz"
                ratio=$(printf "%.2f" "$(echo "$tsgo_mean / $tsz_mean" | bc -l 2>/dev/null)" 2>/dev/null || echo "N/A")
            else
                ratio=$(printf "%.2f" "$(echo "$tsz_mean / $tsgo_mean" | bc -l 2>/dev/null)" 2>/dev/null || echo "N/A")
            fi

            RESULTS_CSV="${RESULTS_CSV}${name},${lines},${kb},${tsz_ms},${tsgo_ms},${tsz_lps},${tsgo_lps},${winner},${ratio},\n"
        fi
    else
        printf '%s\n' "$hyperfine_output"
    fi
    rm -f "$json_file"
}

run_project_benchmark() {
    local name="$1"
    local tsconfig="$2"
    local src_dir="$3"
    local peak_memory_bytes=""
    local check_log_dir="${TEMP_DIR:-${TMPDIR:-/tmp}}"
    local check_log_prefix
    check_log_prefix="$(printf '%s' "$name" | tr -c '[:alnum:]_.-' '_')"
    local tsc_check_log="$check_log_dir/${check_log_prefix}.tsc-check.log"
    local tsz_check_log="$check_log_dir/${check_log_prefix}.tsz-check.log"
    local tsgo_check_log="$check_log_dir/${check_log_prefix}.tsgo-check.log"

    update_project_peak_memory() {
        local observed="${LAST_PEAK_RSS_BYTES:-0}"
        if [[ "$observed" =~ ^[0-9]+$ ]] && [ "$observed" -gt 0 ]; then
            if [ -z "$peak_memory_bytes" ] || [ "$observed" -gt "$peak_memory_bytes" ]; then
                peak_memory_bytes="$observed"
            fi
        fi
    }

    # Skip if filter is set and name doesn't match
    if [ -n "$FILTER" ] && ! echo "$name" | grep -qE "$FILTER"; then
        return
    fi

    BENCHMARKS_RUN=$((BENCHMARKS_RUN + 1))

    # Count TS-family files from the same tsconfig used for project-mode
    # compilation. This keeps full-project metadata aligned with the files
    # passed to `tsz/tsgo --noEmit -p`.
    local lines
    local bytes
    local file_count
    read -r lines bytes file_count < <(project_tsconfig_stats "$tsconfig" "$src_dir")
    local kb=$((bytes / 1024))
    local info="${lines} lines, ${kb}KB (${file_count} project files)"

    # Fixture-side line/file totals are display and throughput metadata only.
    # Program admission comes exclusively from TSZ's fresh schema-v2 stats and
    # the pinned TS7 graph below; a directory walk can never certify a row.
    local -a project_node_prefix=()
    if [ "$name" = "large-ts-repo" ] && [ -n "$LARGE_TS_NODE_OPTIONS" ]; then
        project_node_prefix=(env "NODE_OPTIONS=$LARGE_TS_NODE_OPTIONS")
    fi

    local -a PROJECT_EVIDENCE_TSC_CMD=("$TSC")
    if [ "${#project_node_prefix[@]}" -gt 0 ]; then
        PROJECT_EVIDENCE_TSC_CMD=("${project_node_prefix[@]}" "$TSC")
    fi
    local -a PROJECT_EVIDENCE_TSZ_CMD=(env)
    if [ "$name" = "large-ts-repo" ] && [ -n "$LARGE_TS_NODE_OPTIONS" ]; then
        PROJECT_EVIDENCE_TSZ_CMD+=("NODE_OPTIONS=$LARGE_TS_NODE_OPTIONS")
    fi
    if [ -n "${TSZ_LIB_DIR:-}" ]; then
        PROJECT_EVIDENCE_TSZ_CMD+=("TSZ_LIB_DIR=$TSZ_LIB_DIR")
    fi
    if [ -n "${TSZ_RUST_MIN_STACK:-}" ]; then
        PROJECT_EVIDENCE_TSZ_CMD+=("RUST_MIN_STACK=$TSZ_RUST_MIN_STACK")
    fi
    PROJECT_EVIDENCE_TSZ_CMD+=("$TSZ")

    # One TSZ proof run must expose the exact admitted program and agree with
    # pinned TypeScript 7 before hyperfine is reachable. This applies equally to
    # large-ts-repo: an expensive row without proof is gray, never a speed win.
    local project_timeout=$((BENCH_TIMEOUT * 2))
    local evidence_ok=0
    collect_project_evidence "$name" "$tsconfig" "$src_dir" \
        "$tsz_check_log" "$tsc_check_log" || evidence_ok=$?
    update_project_peak_memory
    local tsc_exit_codes="${PROJECT_EVIDENCE_TSC_RC:-}"
    local tsz_check="${PROJECT_EVIDENCE_TSZ_RC:-70}"
    local tsgo_check=0
    if [ "$evidence_ok" -eq 0 ]; then
        if [ "${#project_node_prefix[@]}" -gt 0 ]; then
            run_with_timeout "$project_timeout" "${project_node_prefix[@]}" "$TSGO" \
                --noEmit -p "$tsconfig" >"$tsgo_check_log" 2>&1 || tsgo_check=$?
        else
            run_with_timeout "$project_timeout" "$TSGO" --noEmit -p "$tsconfig" \
                >"$tsgo_check_log" 2>&1 || tsgo_check=$?
        fi
        update_project_peak_memory
    fi

    if [ "$evidence_ok" -ne 0 ]; then
        local evidence_status="project evidence unavailable: ${PROJECT_EVIDENCE_REASON}"
        evidence_status="${evidence_status//,/;}"
        echo -e "${YELLOW}$name${NC} - ${YELLOW}NOT TIMED${NC} (${PROJECT_EVIDENCE_REASON})"
        record_project_compatibility "$name" "$PROJECT_EVIDENCE_EXIT_CLASS" "check" \
            "$PROJECT_EVIDENCE_DIAGNOSTIC_STATUS" "$PROJECT_EVIDENCE_DIAGNOSTIC_DELTA" \
            "${PROJECT_EVIDENCE_TSZ_SOURCE_FILES:-}" "$peak_memory_bytes" \
            "$tsc_exit_codes" "$tsz_check" "" "$tsconfig" "$src_dir"
        RESULTS_CSV="${RESULTS_CSV}${name},${lines},${kb},ERR,ERR,N/A,N/A,error,0,${evidence_status}\n"
        return
    fi

    if [ "$tsz_check" -ne 0 ] || [ "$tsgo_check" -ne 0 ]; then
        echo -e "${YELLOW}$name${NC} - ${YELLOW}NOT TIMED${NC} (ordinary compiler exit is nonzero)"
        record_project_compatibility "$name" "exit success" "check" "none" \
            "$PROJECT_EVIDENCE_DIAGNOSTIC_DELTA" "$PROJECT_EVIDENCE_TSZ_SOURCE_FILES" \
            "$peak_memory_bytes" "$tsc_exit_codes" "$tsz_check" "$tsgo_check" \
            "$tsconfig" "$src_dir"
        # Exact parity remains green compatibility, but nonzero execution is not
        # a timing sample. `winner:error` keeps every speed consumer out.
        RESULTS_CSV="${RESULTS_CSV}${name},${lines},${kb},ERR,ERR,N/A,N/A,error,0,\n"
        return
    fi

    echo -e "${GREEN}$name${NC} ($info)"

    # Run benchmark with -p (project mode).
    # Use longer per-run timeout for project benchmarks; very large fixtures
    # (6000+ files) need more headroom because tsz/tsgo cold runs can exceed
    # the default 120s on lower-spec CI runners.
    local run_timeout
    local proj_warmup
    local proj_min
    local proj_max
    if [ "$name" = "large-ts-repo" ]; then
        # tsz cold-checks 6000+ files in ~12min on a workstation; tsgo is much
        # faster (~2.5s) but kept the 10min ceiling for headroom on slow CI
        # runners. Bump to 25min so tsz can record a real number instead of
        # being treated as "unavailable", which previously hard-skipped the
        # tsz arm of this benchmark via a bench-script bypass.
        # BENCH_COLD=1 clears build-info before each run; a warmup would just
        # duplicate the same cold check and roughly double large fixture cost.
        run_timeout=1500
        proj_warmup=0
        proj_min=1
        proj_max=2
    else
        run_timeout=120
        proj_warmup="$WARMUP"
        proj_min="$MIN_RUNS"
        proj_max="$MAX_RUNS"
    fi
    local json_file=$(mktemp)
    local tsz_cmd_prefix=""
    local tsgo_cmd_prefix=""
    local tsz_env_assignments=""
    if [ "$name" = "large-ts-repo" ] && [ -n "$LARGE_TS_NODE_OPTIONS" ]; then
        tsz_env_assignments="NODE_OPTIONS=$LARGE_TS_NODE_OPTIONS"
        tsgo_cmd_prefix="env NODE_OPTIONS=$LARGE_TS_NODE_OPTIONS "
    fi
    if [ -n "${TSZ_LIB_DIR:-}" ]; then
        tsz_env_assignments="${tsz_env_assignments:+$tsz_env_assignments }TSZ_LIB_DIR=$TSZ_LIB_DIR"
    fi
    if [ -n "${TSZ_RUST_MIN_STACK:-}" ]; then
        tsz_env_assignments="${tsz_env_assignments:+$tsz_env_assignments }RUST_MIN_STACK=$TSZ_RUST_MIN_STACK"
    fi
    if [ -n "$tsz_env_assignments" ]; then
        tsz_cmd_prefix="env $tsz_env_assignments "
    fi
    local -a hyperfine_prepare_args=()
    if [[ "${BENCH_COLD:-0}" == "1" ]]; then
        local tsconfig_dir
        tsconfig_dir="$(dirname "$tsconfig")"
        hyperfine_prepare_args=(--prepare "find '${tsconfig_dir}' -name '*.tsbuildinfo' -delete 2>/dev/null; true")
    fi

    local hyperfine_status=0
    local hyperfine_output=""
    if [ "${#hyperfine_prepare_args[@]}" -gt 0 ]; then
        hyperfine_output="$(hyperfine \
            --warmup "$proj_warmup" \
            --min-runs "$proj_min" \
            --max-runs "$proj_max" \
            --style full \
            --ignore-failure \
            --export-json "$json_file" \
            "${hyperfine_prepare_args[@]}" \
            -n "tsz" "bash $BENCH_TIMEOUT_RUNNER $run_timeout -- ${tsz_cmd_prefix}$TSZ --noEmit -p $tsconfig 2>/dev/null" \
            -n "tsgo" "bash $BENCH_TIMEOUT_RUNNER $run_timeout -- ${tsgo_cmd_prefix}$TSGO --noEmit -p $tsconfig 2>/dev/null" 2>&1)" || hyperfine_status=$?
    else
        hyperfine_output="$(hyperfine \
            --warmup "$proj_warmup" \
            --min-runs "$proj_min" \
            --max-runs "$proj_max" \
            --style full \
            --ignore-failure \
            --export-json "$json_file" \
            -n "tsz" "bash $BENCH_TIMEOUT_RUNNER $run_timeout -- ${tsz_cmd_prefix}$TSZ --noEmit -p $tsconfig 2>/dev/null" \
            -n "tsgo" "bash $BENCH_TIMEOUT_RUNNER $run_timeout -- ${tsgo_cmd_prefix}$TSGO --noEmit -p $tsconfig 2>/dev/null" 2>&1)" || hyperfine_status=$?
    fi
    if [ "$hyperfine_status" -ne 0 ]; then
        printf '%s\n' "$hyperfine_output"
        local status="hyperfine error"
        record_project_compatibility "$name" "runner error" "timing" "hyperfine failed" "hyperfine failed while timing project row" "$PROJECT_EVIDENCE_TSZ_SOURCE_FILES" "$peak_memory_bytes" "$tsc_exit_codes" "" "" "$tsconfig" "$src_dir"
        RESULTS_CSV="${RESULTS_CSV}${name},${lines},${kb},ERR,ERR,N/A,N/A,error,0,${status}\n"
        rm -f "$json_file"
        return
    fi

    # Extract times and calculate throughput
    if [ -f "$json_file" ] && command -v jq &>/dev/null; then
        local tsz_exit_status
        local tsgo_exit_status
        tsz_exit_status="$(hyperfine_exit_status_for "$json_file" "tsz" || true)"
        tsgo_exit_status="$(hyperfine_exit_status_for "$json_file" "tsgo" || true)"
        local hyperfine_ok=true
        [ "$tsz_exit_status" != "ok" ] && hyperfine_ok=false
        [ "$tsgo_exit_status" != "ok" ] && hyperfine_ok=false
        print_hyperfine_comparison_output "$hyperfine_output" "$hyperfine_ok"
        if [ "$hyperfine_ok" != true ]; then
            local status=""
            [ "$tsz_exit_status" != "ok" ] && status="tsz ${tsz_exit_status}"
            [ "$tsgo_exit_status" != "ok" ] && status="${status:+${status}; }tsgo ${tsgo_exit_status}"
            echo -e "${YELLOW}$name${NC} - ${RED}ERROR${NC} (${status})" >&2
            local exit_class
            exit_class="$(project_failure_class "$status" $(exit_codes_from_status "$status"))"
            record_project_compatibility "$name" "$exit_class" "timing" "$(project_failure_status "$exit_class")" "$status" "$PROJECT_EVIDENCE_TSZ_SOURCE_FILES" "$peak_memory_bytes" "$tsc_exit_codes" "$tsz_exit_status" "$tsgo_exit_status" "$tsconfig" "$src_dir"
            RESULTS_CSV="${RESULTS_CSV}${name},${lines},${kb},ERR,ERR,N/A,N/A,error,0,${status}\n"
            rm -f "$json_file"
            return
        fi

        local tsz_mean=$(hyperfine_mean_for "$json_file" "tsz")
        local tsgo_mean=$(hyperfine_mean_for "$json_file" "tsgo")

        if [ -n "$tsz_mean" ] && [ -n "$tsgo_mean" ] && [ "$tsz_mean" != "0" ] && [ "$tsgo_mean" != "0" ]; then
            local tsz_lps=$(printf "%.0f" "$(echo "$lines / $tsz_mean" | bc -l 2>/dev/null)" 2>/dev/null || echo "N/A")
            local tsgo_lps=$(printf "%.0f" "$(echo "$lines / $tsgo_mean" | bc -l 2>/dev/null)" 2>/dev/null || echo "N/A")
            local tsz_ms=$(printf "%.2f" "$(echo "$tsz_mean * 1000" | bc -l 2>/dev/null)" 2>/dev/null || echo "N/A")
            local tsgo_ms=$(printf "%.2f" "$(echo "$tsgo_mean * 1000" | bc -l 2>/dev/null)" 2>/dev/null || echo "N/A")

            local winner="tsgo"
            local ratio
            if (( $(echo "$tsz_mean < $tsgo_mean" | bc -l) )); then
                winner="tsz"
                ratio=$(printf "%.2f" "$(echo "$tsgo_mean / $tsz_mean" | bc -l 2>/dev/null)" 2>/dev/null || echo "N/A")
            else
                ratio=$(printf "%.2f" "$(echo "$tsz_mean / $tsgo_mean" | bc -l 2>/dev/null)" 2>/dev/null || echo "N/A")
            fi

            record_project_compatibility "$name" "exit success" "check" "none" \
                "$PROJECT_EVIDENCE_DIAGNOSTIC_DELTA" \
                "$PROJECT_EVIDENCE_TSZ_SOURCE_FILES" "$peak_memory_bytes" \
                "$tsc_exit_codes" "0" "0" "$tsconfig" "$src_dir"
            RESULTS_CSV="${RESULTS_CSV}${name},${lines},${kb},${tsz_ms},${tsgo_ms},${tsz_lps},${tsgo_lps},${winner},${ratio},\n"
        fi
    else
        printf '%s\n' "$hyperfine_output"
    fi
    rm -f "$json_file"
}

JSON_EXPORTED=false
export_results_json() {
    [ "$JSON_OUTPUT" != true ] && return
    [ -z "$RESULTS_CSV" ] && return
    # Idempotent: the EXIT trap may also call this after the in-band
    # invocation; only one write is needed (and the second would just
    # produce a duplicate timestamped file under artifacts/).
    [ "$JSON_EXPORTED" = true ] && return
    JSON_EXPORTED=true

    local default_file="$PROJECT_ROOT/artifacts/bench-vs-tsgo-$(date +%Y%m%d-%H%M%S).json"
    local out_file="${JSON_FILE:-$default_file}"
    mkdir -p "$(dirname "$out_file")"

    local expanded_csv
    local project_readme_candidates_json="{}"
    local project_owner_families_json

    expanded_csv="$(echo -e "$RESULTS_CSV")"
    project_owner_families_json="$(tsz_project_owner_families_json)"
    if command -v node >/dev/null 2>&1; then
      project_readme_candidates_json="$(tsz_project_readme_candidates_json)"
    fi

    RESULTS_CSV_EXPANDED="$expanded_csv" \
    QUICK_MODE_VALUE="$QUICK_MODE" \
    FILTER_VALUE="$FILTER" \
    TSZ_BIN_VALUE="$TSZ" \
    TSZ_IS_OVERRIDE_VALUE="$TSZ_IS_OVERRIDE" \
    TSGO_BIN_VALUE="$TSGO" \
    TSC_BIN_VALUE="$TSC" \
    BENCH_PGO_MARKER_VALUE="$BENCH_PGO_MARKER" \
    BENCH_PGO_VALUE="${BENCH_PGO:-1}" \
    BENCH_REQUIRE_PGO_VALUE="${BENCH_REQUIRE_PGO:-0}" \
    BENCH_PGO_CACHE_VALUE="${BENCH_PGO_CACHE:-1}" \
    BENCH_PGO_SYNTHETIC_VALUE="${BENCH_PGO_SYNTHETIC:-1}" \
    BENCH_PGO_FETCH_UTILITY_TYPES_VALUE="${BENCH_PGO_FETCH_UTILITY_TYPES:-1}" \
    BENCH_PGO_FETCH_CORE_PROJECTS_VALUE="${BENCH_PGO_FETCH_CORE_PROJECTS:-0}" \
    BENCH_PGO_PANIC_UNWIND_VALUE="${BENCH_PGO_PANIC_UNWIND:-0}" \
    BENCH_PGO_EXTRA_INPUTS_VALUE="${BENCH_PGO_EXTRA_INPUTS:-}" \
    BENCH_PGO_TSZ_TIMEOUT_VALUE="$BENCH_PGO_TSZ_TIMEOUT" \
    LARGE_TS_DIR_VALUE="$LARGE_TS_DIR" \
    NEXTJS_DIR_VALUE="$NEXTJS_DIR" \
    NEXT_APP_BENCH_DIR_VALUE="$NEXT_APP_BENCH_DIR" \
    VITE_APP_BENCH_DIR_VALUE="$VITE_APP_BENCH_DIR" \
    RXJS_DIR_VALUE="$RXJS_DIR" \
    TYPE_FEST_DIR_VALUE="$TYPE_FEST_DIR" \
    ZOD_DIR_VALUE="$ZOD_DIR" \
    UTILITY_TYPES_DIR_VALUE="$UTILITY_TYPES_DIR" \
    TS_TOOLBELT_DIR_VALUE="$TS_TOOLBELT_DIR" \
    TS_ESSENTIALS_DIR_VALUE="$TS_ESSENTIALS_DIR" \
    BENCHMARKS_RUN_VALUE="$BENCHMARKS_RUN" \
    BENCH_SHARD_LABEL_VALUE="${TSZ_BENCH_SHARD_LABEL:-}" \
    BENCH_SHARD_FILTER_VALUE="${TSZ_BENCH_SHARD_FILTER:-$FILTER}" \
    COMPATIBILITY_JSONL_VALUE="$PROJECT_COMPATIBILITY_JSONL" \
    BENCHMARK_SOURCES_JSONL_VALUE="${BENCHMARK_SOURCES_JSONL:-}" \
    PROJECT_OWNER_FAMILIES_JSON_VALUE="$project_owner_families_json" \
    PROJECT_README_CANDIDATES_JSON_VALUE="$project_readme_candidates_json" \
    DIAGNOSTIC_SUBSYSTEMS_JSON_PATH="$PROJECT_ROOT/scripts/ci/diagnostic-subsystems.json" \
    ROW_UTILS_MODULE_PATH="$PROJECT_ROOT/scripts/bench/row-utils.mjs" \
    node --input-type=module - "$out_file" <<'NODE'
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";

const { hasExactProjectEvidence } = await import(
  pathToFileURL(process.env.ROW_UTILS_MODULE_PATH).href
);
const outFile = process.argv[2];
const PROJECT_OWNER_FAMILIES = JSON.parse(process.env.PROJECT_OWNER_FAMILIES_JSON_VALUE || "{}");
const projectOwnerFamilies = PROJECT_OWNER_FAMILIES;
const PROJECT_README_CANDIDATES = JSON.parse(process.env.PROJECT_README_CANDIDATES_JSON_VALUE || "{}");
const projectReadmeCandidates = PROJECT_README_CANDIDATES;

function readProjectReadmes() {
  const readmes = new Map();

  const directories = {
    "large-ts-repo": process.env.LARGE_TS_DIR_VALUE,
    nextjs: process.env.NEXTJS_DIR_VALUE,
    "nextjs-fresh-app": process.env.NEXT_APP_BENCH_DIR_VALUE,
    "vite-vanilla-ts-app": process.env.VITE_APP_BENCH_DIR_VALUE,
    "rxjs-project": process.env.RXJS_DIR_VALUE,
    "type-fest-project": process.env.TYPE_FEST_DIR_VALUE,
    "zod-project": process.env.ZOD_DIR_VALUE,
    "utility-types-project": process.env.UTILITY_TYPES_DIR_VALUE,
    "ts-toolbelt-project": process.env.TS_TOOLBELT_DIR_VALUE,
    "ts-essentials-project": process.env.TS_ESSENTIALS_DIR_VALUE,
  };

  for (const [name, directory] of Object.entries(directories)) {
    const candidates = Array.isArray(projectReadmeCandidates[name]) ? projectReadmeCandidates[name] : [];
    for (const candidate of candidates) {
      if (!candidate) continue;
      if (!directory) continue;
      try {
        const text = fs.readFileSync(path.join(directory, candidate), "utf8").trim();
        if (text) {
          readmes.set(name, text.length > 18000 ? `${text.slice(0, 18000).trimEnd()}\n\n...` : text);
          break;
        }
      } catch {
        // README is optional for fixtures that were not prepared in this run.
      }
    }
  }
  return readmes;
}

function readCompatibilityRows() {
  const file = process.env.COMPATIBILITY_JSONL_VALUE || "";
  if (!file) return new Map();
  try {
    const rows = fs.readFileSync(file, "utf8")
      .split(/\r?\n/)
      .map((line) => line.trim())
      .filter(Boolean)
      .map((line) => JSON.parse(line));
    return new Map(rows.map((row) => [row.name, row]));
  } catch {
    return new Map();
  }
}

function readSourceRows() {
  const file = process.env.BENCHMARK_SOURCES_JSONL_VALUE || "";
  if (!file) return new Map();
  try {
    const rows = fs.readFileSync(file, "utf8")
      .split(/\r?\n/)
      .map((line) => line.trim())
      .filter(Boolean)
      .map((line) => JSON.parse(line))
      .filter((row) => row?.name && row?.source?.content);
    return new Map(rows.map((row) => [row.name, row.source]));
  } catch {
    return new Map();
  }
}

function firstNonEmpty(...values) {
  for (const value of values) {
    const normalized = String(value ?? "").trim();
    if (normalized) return normalized;
  }
  return null;
}

function githubRunUrl(runId) {
  if (!runId || runId === "local") return null;
  const serverUrl = firstNonEmpty(process.env.GITHUB_SERVER_URL, "https://github.com");
  const repository = firstNonEmpty(process.env.GITHUB_REPOSITORY);
  if (!repository) return null;
  return `${serverUrl}/${repository}/actions/runs/${runId}`;
}

function compatibilityArtifactMetadata(recorded = {}, generatedAt = new Date().toISOString()) {
  const runId = firstNonEmpty(recorded.workflow_run_id, process.env.GITHUB_RUN_ID, "local");
  return {
    generated_at: firstNonEmpty(recorded.generated_at, generatedAt),
    source_commit: firstNonEmpty(recorded.source_commit, process.env.BENCH_TARGET_SHA, process.env.GITHUB_SHA, "local"),
    workflow_name: firstNonEmpty(recorded.workflow_name, process.env.GITHUB_WORKFLOW, "local"),
    workflow_run_id: runId,
    workflow_run_url: firstNonEmpty(recorded.workflow_run_url, githubRunUrl(runId)),
    workflow_run_attempt: firstNonEmpty(recorded.workflow_run_attempt, process.env.GITHUB_RUN_ATTEMPT),
    run_status: firstNonEmpty(
      recorded.run_status,
      process.env.GITHUB_ACTIONS === "true" ? "completed" : "local",
    ),
  };
}

// Factory (not constant) so each fallback row owns its own nested arrays/
// objects — downstream consumers must be free to mutate without aliasing.
function fallbackCompatibilityDefaults() {
  return {
    diagnostic_deltas: [],
    exit_codes: { tsc: [], tsz: [], tsgo: [] },
    files_reached: null,
    files_reached_reason: "runner did not count",
    peak_memory_bytes: null,
    peak_memory_bytes_reason: "not measured on platform",
  };
}

function fallbackCompatibility(row) {
  if (!projectOwnerFamilies[row.name]) return null;
  const status = String(row.status || "").toLowerCase();
  if (!status) {
    return {
      ...fallbackCompatibilityDefaults(),
      exit_class: "exit success",
      phase: "check",
      last_successful_phase: "check",
      diagnostic_status: "none",
    };
  }
  if (status.includes("fixture")) {
    return {
      ...fallbackCompatibilityDefaults(),
      exit_class: "fixture invalid",
      phase: "fixture setup",
      last_successful_phase: null,
      diagnostic_status: "tsc fixture failed",
    };
  }
  return {
    ...fallbackCompatibilityDefaults(),
    exit_class: status.includes("timeout") ? "timeout" : "nonzero exit",
    phase: "check",
    last_successful_phase: null,
    diagnostic_status: status.includes("tsz") ? "diagnostic mismatch or compiler error" : "compiler error",
  };
}

function normalizedDiagnosticDeltas(recorded) {
  if (!Array.isArray(recorded.diagnostic_deltas)) return [];
  return recorded.diagnostic_deltas
    .map((line) => String(line || "").trim())
    .filter(Boolean)
    .slice(0, 20);
}

function knownBlockersFrom(recorded, diagnosticSubsystems, diagnosticCodes) {
  const existing = Array.isArray(recorded.known_blockers) ? recorded.known_blockers : [];
  if (existing.length) {
    return existing.map(String).filter(Boolean).slice(0, 8);
  }

  const blockers = [];
  const add = (blocker) => {
    if (blocker && !blockers.includes(blocker) && blockers.length < 8) blockers.push(blocker);
  };
  const exitClass = String(recorded.exit_class || "");
  const phase = String(recorded.phase || "");

  if (exitClass === "timeout") add("timeout during project check");
  if (exitClass === "oom") add("OOM or killed during project check");
  if (exitClass === "crash") add("compiler crash during project check");
  if (exitClass === "fixture invalid") add("reference fixture invalid");
  if (exitClass === "runner error") add("benchmark runner error");
  if (exitClass === "tsz unavailable") add("tsz unavailable in benchmark runner");
  if (exitClass === "oracle unavailable") add("tsc oracle unavailable");
  if (phase && phase !== "check") add(`${phase} phase blocker`);

  for (const group of diagnosticSubsystems) {
    add(String(group?.subsystem || ""));
  }

  if (!blockers.length && diagnosticCodes.length) {
    add("unclassified diagnostic mismatch");
  }

  return blockers;
}

// The subsystem rules table is shared with `scripts/ci/diagnostic-subsystems.mjs`
// via `scripts/ci/diagnostic-subsystems.json`. Loading from JSON eliminates
// the drift between this heredoc and the canonical .mjs module (e.g. the
// previously missing `contextual-inference` row). The flatten into a single
// Map gives O(1) `subsystemForCode` lookups vs the old linear scan.
const DIAGNOSTIC_SUBSYSTEMS_TABLE = JSON.parse(
  fs.readFileSync(process.env.DIAGNOSTIC_SUBSYSTEMS_JSON_PATH, "utf8"),
);
const CODE_TO_SUBSYSTEM = new Map();
for (const rule of DIAGNOSTIC_SUBSYSTEMS_TABLE.rules) {
  for (const code of rule.codes) CODE_TO_SUBSYSTEM.set(code, rule.subsystem);
}

function subsystemForCode(code) {
  return CODE_TO_SUBSYSTEM.get(code) || "unclassified diagnostic";
}

// Single-pass aggregation over a row's diagnostic delta list. The previous
// `compatibilityFor()` path walked the same list five separate times
// (subsystems, codes, blocker fallback, output codes, reduction candidates);
// every bucket downstream now reads from the result of this one walk.
function aggregateDeltas(deltas) {
  const subsystemGroups = new Map();
  const codes = [];
  const codeSeen = new Set();
  const coded = [];
  const uncoded = [];
  for (const line of deltas) {
    const lineCodes = [];
    for (const match of line.matchAll(/\bTS\d{4,5}\b/g)) lineCodes.push(match[0]);
    if (lineCodes.length) {
      coded.push(line);
      for (const code of lineCodes) {
        if (!codeSeen.has(code) && codes.length < 8) {
          codeSeen.add(code);
          codes.push(code);
        }
      }
    } else {
      uncoded.push(line);
    }
    const groupKeys = lineCodes.length ? lineCodes : ["uncoded"];
    for (const code of groupKeys) {
      const subsystem = code === "uncoded" ? "uncoded diagnostic" : subsystemForCode(code);
      let group = subsystemGroups.get(subsystem);
      if (!group) {
        group = { subsystem, codes: [], count: 0, examples: [] };
        subsystemGroups.set(subsystem, group);
      }
      group.count += 1;
      if (code !== "uncoded" && !group.codes.includes(code) && group.codes.length < 8) {
        group.codes.push(code);
      }
      if (group.examples.length < 3) {
        group.examples.push(line);
      }
    }
  }
  return {
    subsystems: [...subsystemGroups.values()],
    codes,
    reductionCandidates: (coded.length ? coded : uncoded).slice(0, 5),
  };
}

function normalizedDiagnosticSubsystems(recorded, aggregate) {
  const existing = Array.isArray(recorded.diagnostic_subsystems) ? recorded.diagnostic_subsystems : [];
  if (existing.length) {
    return existing
      .map((group) => ({
        subsystem: String(group?.subsystem || "unclassified diagnostic"),
        codes: Array.isArray(group?.codes) ? group.codes.map(String).filter(Boolean).slice(0, 8) : [],
        count: Number.isFinite(Number(group?.count)) ? Number(group.count) : 0,
        examples: Array.isArray(group?.examples) ? group.examples.map(String).filter(Boolean).slice(0, 3) : [],
      }))
      .filter((group) => group.count > 0 || group.codes.length || group.examples.length)
      .slice(0, 8);
  }
  return aggregate.subsystems.slice(0, 8);
}

function lastSuccessfulPhaseFrom(recorded) {
  if (recorded.last_successful_phase !== undefined && recorded.last_successful_phase !== "") {
    return recorded.last_successful_phase;
  }
  if (recorded.exit_class === "exit success" && recorded.diagnostic_status === "none") return "check";
  return null;
}

function rowStateFrom(recorded) {
  if (recorded.state) return recorded.state;
  if (recorded.exit_class === "exit success" && recorded.diagnostic_status === "none") return "green";
  if (
    recorded.exit_class === "fixture invalid" ||
    recorded.exit_class === "tsz unavailable" ||
    recorded.exit_class === "oracle unavailable"
  ) return "gray";
  if (String(recorded.diagnostic_status || "").toLowerCase().includes("diagnostic mismatch")) return "yellow";
  return "red";
}

function reproFromRecorded(recorded) {
  if (recorded.repro && typeof recorded.repro === "object") return recorded.repro;
  return {
    tsconfig_path: null,
    source_root: null,
    first_failure_path: null,
    first_failure_line: null,
    first_failure_column: null,
    first_failure_code: null,
    reduced_repro_path: recorded.reduced_repro_path || null,
    command: null,
  };
}

// Mirror of residencyReason() in scripts/ci/project-compatibility.mjs but
// post-processor-side: the recorded row already passed the closed-vocabulary
// gate so unknown strings are propagated rather than rejected here.
function residencyReasonFor(value, recordedReason, fallback) {
  if (value !== null && value !== undefined && Number.isFinite(Number(value))) {
    return null;
  }
  const reason = String(recordedReason || "").trim();
  return reason || fallback;
}

function compatibilityFor(row, compatibilityRows) {
  const recorded = compatibilityRows.get(row.name) || fallbackCompatibility(row);
  if (!recorded) return {};
  const diagnosticDeltas = normalizedDiagnosticDeltas(recorded);
  // One walk over diagnosticDeltas populates subsystems, codes, and
  // reduction candidates. Issue #11598 traced repeated per-row scans of
  // this list to quadratic-feeling cost when row counts grew.
  const aggregate = aggregateDeltas(diagnosticDeltas);
  const diagnosticSubsystems = normalizedDiagnosticSubsystems(recorded, aggregate);
  const knownBlockers = knownBlockersFrom(recorded, diagnosticSubsystems, aggregate.codes);
  const state = rowStateFrom(recorded);
  return {
    compatibility: {
      ...compatibilityArtifactMetadata(recorded),
      state,
      exit_class: recorded.exit_class || "unknown",
      first_failure_class: recorded.first_failure_class ?? (state === "green" ? null : knownBlockers[0] || recorded.exit_class || null),
      owner_track: recorded.owner_track ?? null,
      phase: recorded.phase || "unknown",
      last_successful_phase: lastSuccessfulPhaseFrom(recorded),
      diagnostic_status: recorded.diagnostic_status || "unknown",
      evidence_schema: recorded.evidence_schema ?? null,
      semantic_completion: recorded.semantic_completion ?? null,
      root_files: recorded.root_files ?? null,
      source_files: recorded.source_files ?? null,
      root_file_fingerprint: recorded.root_file_fingerprint ?? null,
      source_file_fingerprint: recorded.source_file_fingerprint ?? null,
      oracle_root_files: recorded.oracle_root_files ?? null,
      oracle_source_files: recorded.oracle_source_files ?? null,
      oracle_root_file_fingerprint: recorded.oracle_root_file_fingerprint ?? null,
      oracle_source_file_fingerprint: recorded.oracle_source_file_fingerprint ?? null,
      diagnostic_records: recorded.diagnostic_records ?? null,
      diagnostic_fingerprint: recorded.diagnostic_fingerprint ?? null,
      oracle_diagnostic_records: recorded.oracle_diagnostic_records ?? null,
      oracle_diagnostic_fingerprint: recorded.oracle_diagnostic_fingerprint ?? null,
      stub_inventory_schema: recorded.stub_inventory_schema ?? null,
      stubbed_modules: recorded.stubbed_modules ?? null,
      stubbed_any_members: recorded.stubbed_any_members ?? null,
      stub_inventory_fingerprint: recorded.stub_inventory_fingerprint ?? null,
      oracle_classification: recorded.oracle_classification ?? "unknown",
      diagnostic_deltas: diagnosticDeltas,
      diagnostic_codes: aggregate.codes,
      diagnostic_subsystems: diagnosticSubsystems,
      primary_subsystem: recorded.primary_subsystem || diagnosticSubsystems[0]?.subsystem || null,
      reduction_candidates: aggregate.reductionCandidates,
      emit_status: recorded.emit_status || "not in scope (noEmit project check)",
      dts_status: recorded.dts_status || "not in scope (noEmit project check)",
      known_blockers: knownBlockers,
      reduced_repro_path: recorded.reduced_repro_path || null,
      repro: reproFromRecorded(recorded),
      exit_codes: recorded.exit_codes && typeof recorded.exit_codes === "object"
        ? {
            tsc: Array.isArray(recorded.exit_codes.tsc) ? recorded.exit_codes.tsc : [],
            tsz: Array.isArray(recorded.exit_codes.tsz) ? recorded.exit_codes.tsz : [],
            tsgo: Array.isArray(recorded.exit_codes.tsgo) ? recorded.exit_codes.tsgo : [],
          }
        : { tsc: [], tsz: [], tsgo: [] },
      semantic_owner_family: projectOwnerFamilies[row.name] || "not classified",
      files_reached: recorded.files_reached ?? null,
      files_reached_reason: residencyReasonFor(
        recorded.files_reached,
        recorded.files_reached_reason,
        "runner did not count",
      ),
      peak_memory_bytes: recorded.peak_memory_bytes ?? null,
      peak_memory_bytes_reason: residencyReasonFor(
        recorded.peak_memory_bytes,
        recorded.peak_memory_bytes_reason,
        "not measured on platform",
      ),
      fixture_sources: Array.isArray(recorded.fixture_sources) ? recorded.fixture_sources : [],
    },
  };
}

const csv = process.env.RESULTS_CSV_EXPANDED || "";
const projectReadmes = readProjectReadmes();
const compatibilityRows = readCompatibilityRows();
const sourceRows = readSourceRows();
const rows = csv
  .split(/\r?\n/)
  .map((line) => line.trim())
  .filter(Boolean)
  .map((line) => {
    const parts = line.split(",");
    while (parts.length < 10) parts.push("");
    const [name, lines, kb, tszMs, tsgoMs, tszLps, tsgoLps, winner, factor, status] = parts;
    const toNumber = (value) => {
      if (!value || value === "N/A" || value === "ERR") return null;
      const parsed = Number(value);
      return Number.isFinite(parsed) ? parsed : null;
    };
    return {
      name,
      lines: toNumber(lines),
      kb: toNumber(kb),
      ...(projectOwnerFamilies[name] ? { project_files: compatibilityRows.get(name)?.files_reached ?? null } : {}),
      tsz_ms: toNumber(tszMs),
      tsgo_ms: toNumber(tsgoMs),
      tsz_lps: toNumber(tszLps),
      tsgo_lps: toNumber(tsgoLps),
      winner: winner || null,
      factor: toNumber(factor),
      status: status || null,
      ...(sourceRows.has(name) ? { source: sourceRows.get(name) } : {}),
      ...(projectReadmes.has(name) ? { readme: projectReadmes.get(name) } : {}),
      ...compatibilityFor({ name, lines: toNumber(lines), status: status || null }, compatibilityRows),
    };
  });

const tszWins = rows.filter((row) => row.winner === "tsz").length;
const tsgoWins = rows.filter((row) => row.winner === "tsgo").length;
const errorCases = rows.filter((row) => row.status).length;

function hasCompletePhaseMetadata(compatibility) {
  return [
    "state",
    "phase",
    "last_successful_phase",
    "exit_class",
    "diagnostic_status",
  ].every((field) => Object.hasOwn(compatibility, field));
}

function isGreenRow(row) {
  if (row.status) return false;
  if (row.artifact_missing === true) return false;
  if (!row.compatibility) return true;
  return hasCompletePhaseMetadata(row.compatibility) &&
    hasExactProjectEvidence(row.compatibility, row.name) &&
    row.compatibility.state === "green" &&
    row.compatibility.exit_class === "exit success" &&
    row.compatibility.diagnostic_status === "none";
}

const greenTszWins = rows.filter((row) => row.winner === "tsz" && isGreenRow(row)).length;
const greenTsgoWins = rows.filter((row) => row.winner === "tsgo" && isGreenRow(row)).length;

function runnerEnvironment() {
  const cpus = os.cpus();
  const firstCpu = cpus[0] || {};
  const cpuModels = [...new Set(cpus.map((cpu) => cpu.model).filter(Boolean))];
  const totalMemoryBytes = os.totalmem();
  const githubActions = process.env.GITHUB_ACTIONS === "true"
    ? {
        run_id: process.env.GITHUB_RUN_ID || null,
        run_attempt: process.env.GITHUB_RUN_ATTEMPT || null,
        runner_os: process.env.RUNNER_OS || null,
        runner_arch: process.env.RUNNER_ARCH || null,
        workflow: process.env.GITHUB_WORKFLOW || null,
        job: process.env.GITHUB_JOB || null,
        ref: process.env.GITHUB_REF || null,
        sha: process.env.GITHUB_SHA || null,
      }
    : null;
  const cloudBuild = process.env.BUILD_ID ||
    process.env.PROJECT_ID ||
    process.env.TSZ_BENCH_MACHINE_TYPE
    ? {
        build_id: process.env.BUILD_ID || null,
        project_id: process.env.PROJECT_ID || null,
        region: process.env.LOCATION || process.env.CLOUDSDK_COMPUTE_REGION || null,
        machine_type: process.env.TSZ_BENCH_MACHINE_TYPE || null,
      }
    : null;

  return {
    platform: os.platform(),
    arch: os.arch(),
    release: os.release(),
    cpu_count: cpus.length || null,
    cpu_model: cpuModels[0] || null,
    cpu_models: cpuModels.length > 1 ? cpuModels.slice(0, 4) : undefined,
    cpu_speed_mhz: Number.isFinite(firstCpu.speed) ? firstCpu.speed : null,
    total_memory_bytes: Number.isFinite(totalMemoryBytes) ? totalMemoryBytes : null,
    ci: process.env.CI === "true",
    github_actions: githubActions,
    cloud_build: cloudBuild,
  };
}

function boolValue(value, defaultValue = false) {
  if (value === undefined || value === null || value === "") return defaultValue;
  return value === true || value === "1" || String(value).toLowerCase() === "true";
}

function numberValue(value, fallback = null) {
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : fallback;
}

function readPgoMarker(markerPath) {
  const fields = {};
  const trainingInputs = [];
  const trainingFailedInputs = [];
  if (!markerPath || !fs.existsSync(markerPath)) {
    return { found: false, fields, training_inputs: trainingInputs, training_failed_inputs: trainingFailedInputs };
  }

  const lines = fs.readFileSync(markerPath, "utf8").split(/\r?\n/);
  for (const line of lines) {
    if (!line.trim()) continue;
    const separator = line.indexOf("=");
    if (separator === -1) continue;
    const key = line.slice(0, separator);
    const value = line.slice(separator + 1);
    if (key === "training_input") {
      trainingInputs.push(value);
    } else if (key === "training_failed") {
      trainingFailedInputs.push(value);
    } else {
      fields[key] = value;
    }
  }

  return { found: true, fields, training_inputs: trainingInputs, training_failed_inputs: trainingFailedInputs };
}

function pgoConfig(fields) {
  return {
    synthetic: boolValue(fields.BENCH_PGO_SYNTHETIC ?? process.env.BENCH_PGO_SYNTHETIC_VALUE, true),
    fetch_utility_types: boolValue(
      fields.BENCH_PGO_FETCH_UTILITY_TYPES ?? process.env.BENCH_PGO_FETCH_UTILITY_TYPES_VALUE,
      true,
    ),
    fetch_core_projects: boolValue(
      fields.BENCH_PGO_FETCH_CORE_PROJECTS ?? process.env.BENCH_PGO_FETCH_CORE_PROJECTS_VALUE,
      false,
    ),
    panic_unwind: boolValue(fields.BENCH_PGO_PANIC_UNWIND ?? process.env.BENCH_PGO_PANIC_UNWIND_VALUE, false),
    extra_inputs: (fields.BENCH_PGO_EXTRA_INPUTS ?? process.env.BENCH_PGO_EXTRA_INPUTS_VALUE) || null,
    training_timeout_seconds: numberValue(
      fields.BENCH_PGO_TSZ_TIMEOUT ?? process.env.BENCH_PGO_TSZ_TIMEOUT_VALUE,
      null,
    ),
    cache_enabled: boolValue(process.env.BENCH_PGO_CACHE_VALUE, true),
  };
}

function measurementProfile() {
  const quickMode = process.env.QUICK_MODE_VALUE === "true";
  const tszOverride = process.env.TSZ_IS_OVERRIDE_VALUE === "true";
  const pgoRequested = !quickMode && !tszOverride && boolValue(process.env.BENCH_PGO_VALUE, true);
  const markerPath = process.env.BENCH_PGO_MARKER_VALUE || null;
  const marker = tszOverride ? readPgoMarker(null) : readPgoMarker(markerPath);
  const fields = marker.fields;
  const pgoOptimized = marker.found && (
    fields.optimized === "1" ||
    fields.binary_profile === "release-pgo" ||
    Boolean(fields["profile-use"])
  );
  const mode = tszOverride
    ? "tsz-override"
    : quickMode
      ? "quick-untrained"
      : pgoOptimized
        ? "release-pgo"
        : "release-untrained";
  const trainingInputCount = numberValue(fields.training_input_count, marker.training_inputs.length);
  const trainingFailureCount = numberValue(fields.training_failure_count, marker.training_failed_inputs.length);

  return {
    mode,
    tsz_binary_source: tszOverride ? "override" : "bench-dist",
    rust_target_cpu: fields.rust_target_cpu || null,
    profile_guided_optimization: {
      requested: pgoRequested,
      required: boolValue(process.env.BENCH_REQUIRE_PGO_VALUE, false),
      optimized: pgoOptimized,
      marker_path: markerPath,
      marker_found: marker.found,
      profile_use: fields["profile-use"] || null,
      profile_fingerprint: fields.profile_fingerprint || null,
      training_fingerprint: fields.training_fingerprint || null,
      profile_data_source: fields.profile_data_source || null,
      built_at: fields.built_at || null,
      llvm_profdata: fields.llvm_profdata || null,
      training_metadata_available: boolValue(fields.training_metadata_available, false),
      training_input_count: trainingInputCount,
      training_failure_count: trainingFailureCount,
      training_inputs: marker.training_inputs,
      training_failed_inputs: marker.training_failed_inputs,
      config: pgoConfig(fields),
    },
  };
}

const generatedAt = new Date().toISOString();
const currentMeasurementProfile = measurementProfile();
const payload = {
  ...compatibilityArtifactMetadata({}, generatedAt),
  benchmark_runner: "scripts/bench/bench-vs-tsgo.sh",
  runner_environment: runnerEnvironment(),
  measurement_profile: currentMeasurementProfile,
  validation: {
    hyperfine_exit_codes_required: true,
  },
  shard: {
    label: firstNonEmpty(process.env.BENCH_SHARD_LABEL_VALUE, process.env.FILTER_VALUE),
    filter: firstNonEmpty(process.env.BENCH_SHARD_FILTER_VALUE, process.env.FILTER_VALUE),
  },
  quick_mode: process.env.QUICK_MODE_VALUE === "true",
  filter: process.env.FILTER_VALUE || null,
  binaries: {
    tsz: process.env.TSZ_BIN_VALUE || null,
    tsgo: process.env.TSGO_BIN_VALUE || null,
    tsc: process.env.TSC_BIN_VALUE || null,
    tsz_profile: currentMeasurementProfile.mode,
  },
  totals: {
    benchmarks_run: Number(process.env.BENCHMARKS_RUN_VALUE || rows.length),
    rows: rows.length,
    tsz_wins: tszWins,
    tsgo_wins: tsgoWins,
    green_tsz_wins: greenTszWins,
    green_tsgo_wins: greenTsgoWins,
    error_cases: errorCases,
  },
  results: rows,
};

fs.writeFileSync(outFile, `${JSON.stringify(payload, null, 2)}\n`, "utf8");
NODE

    echo -e "${GREEN}JSON results written:${NC} $out_file"
}

is_benchmark_selected() {
    local name="$1"
    if [ -z "$FILTER" ]; then
        return 0
    fi
    echo "$name" | grep -qE "$FILTER"
}

filter_matches_any() {
    if [ -z "$FILTER" ]; then
        return 0
    fi

    local name
    for name in "$@"; do
        if echo "$name" | grep -qE "$FILTER"; then
            return 0
        fi
    done

    return 1
}

use_quick_subset_for() {
    if [ "$QUICK_MODE" != true ]; then
        return 1
    fi

    # If an explicit filter does not match the quick representative, keep the
    # quick run counts but scan the full candidate list so exact filters work.
    filter_matches_any "$@"
}
