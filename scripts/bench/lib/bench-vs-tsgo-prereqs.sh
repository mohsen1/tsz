print_header() {
    echo
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${BOLD}  $1${NC}"
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
}

hash_stdin_sha256() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum | awk '{print $1}'
    else
        shasum -a 256 | awk '{print $1}'
    fi
}

hash_file_sha256() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    else
        shasum -a 256 "$1" | awk '{print $1}'
    fi
}

pgo_profile_fingerprint() {
    {
        printf 'rustc=%s\n' "$(rustc -vV 2>/dev/null | tr '\n' ';')"
        printf 'BENCH_PGO_SYNTHETIC=%s\n' "${BENCH_PGO_SYNTHETIC:-1}"
        printf 'BENCH_PGO_FETCH_UTILITY_TYPES=%s\n' "${BENCH_PGO_FETCH_UTILITY_TYPES:-1}"
        printf 'BENCH_PGO_FETCH_CORE_PROJECTS=%s\n' "${BENCH_PGO_FETCH_CORE_PROJECTS:-0}"
        printf 'BENCH_PGO_PANIC_UNWIND=%s\n' "${BENCH_PGO_PANIC_UNWIND:-0}"
        printf 'BENCH_PGO_EXTRA_INPUTS=%s\n' "${BENCH_PGO_EXTRA_INPUTS:-}"
        printf 'BENCH_RUST_TARGET_CPU=%s\n' "${BENCH_RUST_TARGET_CPU:-native}"
        printf 'UTILITY_TYPES_REF=%s\n' "$UTILITY_TYPES_REF"
        printf 'TS_TOOLBELT_REF=%s\n' "$TS_TOOLBELT_REF"
        printf 'TS_ESSENTIALS_REF=%s\n' "$TS_ESSENTIALS_REF"
        printf 'RXJS_REF=%s\n' "$RXJS_REF"
        printf 'TYPE_FEST_REF=%s\n' "$TYPE_FEST_REF"
        if git -C "$PROJECT_ROOT/TypeScript" rev-parse HEAD >/dev/null 2>&1; then
            printf 'TypeScript=%s\n' "$(git -C "$PROJECT_ROOT/TypeScript" rev-parse HEAD)"
        fi
        git -C "$PROJECT_ROOT" ls-files \
            Cargo.lock \
            Cargo.toml \
            .cargo/config.toml \
            'crates/**/Cargo.toml' \
            'crates/**/*.rs' \
            scripts/bench/project-fixtures.sh \
            scripts/bench/project-rows.mjs \
            scripts/bench/bench-vs-tsgo.sh \
            'scripts/bench/lib/bench-vs-tsgo-*.sh' |
            sort |
            while IFS= read -r file; do
                [ -f "$PROJECT_ROOT/$file" ] || continue
                printf '%s  %s\n' "$(hash_file_sha256 "$PROJECT_ROOT/$file")" "$file"
            done
    } | hash_stdin_sha256
}

pgo_training_fingerprint() {
    {
        printf 'BENCH_PGO_SYNTHETIC=%s\n' "${BENCH_PGO_SYNTHETIC:-1}"
        printf 'BENCH_PGO_FETCH_UTILITY_TYPES=%s\n' "${BENCH_PGO_FETCH_UTILITY_TYPES:-1}"
        printf 'BENCH_PGO_FETCH_CORE_PROJECTS=%s\n' "${BENCH_PGO_FETCH_CORE_PROJECTS:-0}"
        printf 'BENCH_PGO_PANIC_UNWIND=%s\n' "${BENCH_PGO_PANIC_UNWIND:-0}"
        printf 'BENCH_PGO_EXTRA_INPUTS=%s\n' "${BENCH_PGO_EXTRA_INPUTS:-}"
        printf 'BENCH_PGO_TSZ_TIMEOUT=%s\n' "$BENCH_PGO_TSZ_TIMEOUT"
        printf 'BENCH_RUST_TARGET_CPU=%s\n' "${BENCH_RUST_TARGET_CPU:-native}"
        local label
        for label in "${BENCH_PGO_TRAINING_INPUTS[@]}"; do
            printf 'training_input=%s\n' "$label"
        done
        for label in "${BENCH_PGO_TRAINING_FAILED_INPUTS[@]}"; do
            printf 'training_failed=%s\n' "$label"
        done
    } | hash_stdin_sha256
}

write_pgo_training_metadata() {
    local out_file="$1"
    local profile_fingerprint="$2"
    local llvm_profdata="$3"
    local training_fingerprint
    training_fingerprint="$(pgo_training_fingerprint)"
    {
        printf 'profile_fingerprint=%s\n' "$profile_fingerprint"
        printf 'training_fingerprint=%s\n' "$training_fingerprint"
        printf 'llvm_profdata=%s\n' "$llvm_profdata"
        printf 'training_metadata_available=1\n'
        printf 'BENCH_PGO_SYNTHETIC=%s\n' "${BENCH_PGO_SYNTHETIC:-1}"
        printf 'BENCH_PGO_FETCH_UTILITY_TYPES=%s\n' "${BENCH_PGO_FETCH_UTILITY_TYPES:-1}"
        printf 'BENCH_PGO_FETCH_CORE_PROJECTS=%s\n' "${BENCH_PGO_FETCH_CORE_PROJECTS:-0}"
        printf 'BENCH_PGO_PANIC_UNWIND=%s\n' "${BENCH_PGO_PANIC_UNWIND:-0}"
        printf 'BENCH_PGO_EXTRA_INPUTS=%s\n' "${BENCH_PGO_EXTRA_INPUTS:-}"
        printf 'BENCH_PGO_TSZ_TIMEOUT=%s\n' "$BENCH_PGO_TSZ_TIMEOUT"
        printf 'BENCH_RUST_TARGET_CPU=%s\n' "${BENCH_RUST_TARGET_CPU:-native}"
        printf 'training_input_count=%s\n' "${#BENCH_PGO_TRAINING_INPUTS[@]}"
        printf 'training_failure_count=%s\n' "${#BENCH_PGO_TRAINING_FAILED_INPUTS[@]}"
        local label
        for label in "${BENCH_PGO_TRAINING_INPUTS[@]}"; do
            printf 'training_input=%s\n' "$label"
        done
        for label in "${BENCH_PGO_TRAINING_FAILED_INPUTS[@]}"; do
            printf 'training_failed=%s\n' "$label"
        done
    } > "$out_file"
}

write_pgo_training_unavailable_metadata() {
    local out_file="$1"
    local profile_fingerprint="$2"
    local llvm_profdata="$3"
    {
        printf 'profile_fingerprint=%s\n' "$profile_fingerprint"
        printf 'training_fingerprint=\n'
        printf 'llvm_profdata=%s\n' "$llvm_profdata"
        printf 'training_metadata_available=0\n'
        printf 'BENCH_PGO_SYNTHETIC=%s\n' "${BENCH_PGO_SYNTHETIC:-1}"
        printf 'BENCH_PGO_FETCH_UTILITY_TYPES=%s\n' "${BENCH_PGO_FETCH_UTILITY_TYPES:-1}"
        printf 'BENCH_PGO_FETCH_CORE_PROJECTS=%s\n' "${BENCH_PGO_FETCH_CORE_PROJECTS:-0}"
        printf 'BENCH_PGO_PANIC_UNWIND=%s\n' "${BENCH_PGO_PANIC_UNWIND:-0}"
        printf 'BENCH_PGO_EXTRA_INPUTS=%s\n' "${BENCH_PGO_EXTRA_INPUTS:-}"
        printf 'BENCH_PGO_TSZ_TIMEOUT=%s\n' "$BENCH_PGO_TSZ_TIMEOUT"
        printf 'BENCH_RUST_TARGET_CPU=%s\n' "${BENCH_RUST_TARGET_CPU:-native}"
        printf 'training_input_count=0\n'
        printf 'training_failure_count=0\n'
    } > "$out_file"
}

write_pgo_marker() {
    local marker="$1"
    local profile_use="$2"
    local metadata_file="$3"
    local profile_data_source="$4"
    {
        printf 'optimized=1\n'
        printf 'binary_profile=release-pgo\n'
        printf 'profile-use=%s\n' "$profile_use"
        printf 'built_at=%s\n' "$(date -u +%FT%TZ)"
        printf 'profile_data_source=%s\n' "$profile_data_source"
        # Codegen target of the optimized binary build (the training-metadata
        # BENCH_RUST_TARGET_CPU line below can be stale on PGO cache reuse).
        printf 'rust_target_cpu=%s\n' "${BENCH_RUST_TARGET_CPU:-native}"
        if [ -f "$metadata_file" ]; then
            cat "$metadata_file"
        else
            printf 'training_metadata_available=0\n'
        fi
    } > "$marker"
}

print_subheader() {
    echo
    echo -e "${CYAN}▶ $1${NC}"
    echo -e "${CYAN}─────────────────────────────────────────────────────────────────────────────${NC}"
}

file_info() {
    local file="$1"
    local lines=$(wc -l < "$file" 2>/dev/null | tr -d ' ')
    local bytes=$(wc -c < "$file" 2>/dev/null | tr -d ' ')
    local kb=$((bytes / 1024))
    echo "${lines} lines, ${kb}KB"
}

measure_peak_rss_enabled() {
    case "${TSZ_BENCH_PROJECT_PEAK_RSS:-}" in
        1|true|TRUE|yes|YES) return 0 ;;
        0|false|FALSE|no|NO) return 1 ;;
    esac

    [ "${CI:-}" = "true" ] && [ "$(uname -s 2>/dev/null || echo unknown)" = "Linux" ]
}

# Echoes the structured reason peak-RSS sampling is not active, or empty when
# sampling is active (a subsequent empty measurement then means the process
# exited before any sample). Reasons come from the closed vocabulary owned by
# scripts/ci/project-compatibility.mjs.
peak_rss_unavailable_reason() {
    case "${TSZ_BENCH_PROJECT_PEAK_RSS:-}" in
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
                if (live[pid[i]]) {
                    total += rss[i]
                }
            }
            print total + 0
        }
    '
}

# Run a command with a timeout (in seconds). Returns the command's exit code,
# or 124 if it was killed due to timeout (matching GNU timeout convention).
# Usage: run_with_timeout <seconds> <command...>
run_with_timeout() {
    local timeout_secs="$1"
    shift
    # Empty (not "0") is the "no positive sample yet" sentinel so the
    # record-time reason logic can distinguish it from a deliberate zero.
    LAST_PEAK_RSS_BYTES=""

    # Run the command in a background subshell
    "$@" &
    local pid=$!
    local rss_file=""
    local rss_monitor_pid=""

    # Watchdog: SIGKILL directly after timeout (SIGTERM can be ignored by Rust binaries)
    ( sleep "$timeout_secs" && kill -KILL "$pid" 2>/dev/null || true ) &
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

    # Wait for the main process (|| true for set -e safety)
    local exit_code=0
    wait "$pid" 2>/dev/null || exit_code=$?

    # Clean up watchdog (|| true since it may have already exited)
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

    # SIGKILL exit code is 137 (128+9)
    if [ "$exit_code" -eq 137 ]; then
        return 124
    fi
    return "$exit_code"
}

run_cargo_build() {
    local description="$1"
    shift

    (cd "$PROJECT_ROOT" && run_with_timeout "$BENCH_CARGO_BUILD_TIMEOUT" env "$@")
    local exit_code=$?
    if [ "$exit_code" -eq 0 ]; then
        return 0
    fi

    if [ "$exit_code" -eq 124 ]; then
        echo -e "${RED}✗ $description timed out after ${BENCH_CARGO_BUILD_TIMEOUT}s${NC}"
    else
        echo -e "${RED}✗ $description failed (exit $exit_code)${NC}"
    fi
    return "$exit_code"
}

capture_diagnostic_lines() {
    local label="$1"
    local timeout_secs="$2"
    shift 2

    { run_with_timeout "$timeout_secs" "$@" 2>&1 || true; } \
        | awk -v label="$label" '
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
        '
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

append_diagnostic_delta() {
    local existing="$1"
    local addition="$2"

    if [ -z "$addition" ]; then
        printf '%s' "$existing"
    elif [ -z "$existing" ]; then
        printf '%s' "$addition"
    else
        printf '%s\n%s' "$existing" "$addition"
    fi
}

hyperfine_mean_for() {
    local json_file="$1"
    local command_name="$2"
    jq -r --arg command_name "$command_name" \
        '.results[] | select(.command == $command_name) | .mean // empty' \
        "$json_file" 2>/dev/null || true
}

hyperfine_exit_status_for() {
    local json_file="$1"
    local command_name="$2"
    local exit_count
    local nonzero_count
    local exit_codes

    exit_count=$(jq -r --arg command_name "$command_name" \
        '[.results[] | select(.command == $command_name) | .exit_codes[]?] | length' \
        "$json_file" 2>/dev/null || echo "0")
    if [ "$exit_count" = "0" ]; then
        echo "missing exit codes"
        return 1
    fi

    nonzero_count=$(jq -r --arg command_name "$command_name" \
        '[.results[] | select(.command == $command_name) | .exit_codes[]? | select(. != 0)] | length' \
        "$json_file" 2>/dev/null || echo "1")
    if [ "$nonzero_count" != "0" ]; then
        exit_codes=$(jq -r --arg command_name "$command_name" \
            '[.results[] | select(.command == $command_name) | .exit_codes[]?] | unique | map(tostring) | join("|")' \
            "$json_file" 2>/dev/null || echo "unknown")
        echo "exit codes ${exit_codes}"
        return 1
    fi

    echo "ok"
    return 0
}

# Print hyperfine's own captured comparison output for a two-command run.
#
# hyperfine prints its "Summary\n  <faster> ran\n  N.NN times faster than
# <slower>" comparison unconditionally whenever `--ignore-failure` let a
# killed-by-timeout or non-zero-exit command finish "successfully" alongside
# a clean one: that comparison is `ceiling_or_error_time / other_time`, not a
# measurement of the losing side, the exact "42.99x faster" fabrication
# #16196 found from a killed `large-ts-repo` row. #16196's own fix
# (`row-utils.mjs`'s `didNotFinish`) only reaches the structured JSON/CSV
# path — hyperfine's raw stdout is a distinct emitter of the same fabricated
# number and streams straight to the console/CI log before this script ever
# inspects an exit code, so the JSON-side gate can't intercept it. When `ok`
# is not "true", strip the trailing Summary block (detected on a
# color-code-stripped copy so the line count still matches the original,
# still-colored text) and print an explicit note instead; the per-benchmark
# timing lines above it stay, since those are real per-command
# wall-clock measurements even when one side was killed or errored.
print_hyperfine_comparison_output() {
    local output="$1"
    local ok="$2"
    if [ "$ok" = true ]; then
        printf '%s\n' "$output"
        return
    fi
    local plain
    plain="$(printf '%s\n' "$output" | sed 's/\x1b\[[0-9;]*m//g')"
    local summary_line
    summary_line="$(printf '%s\n' "$plain" | grep -n '^Summary$' | head -1 | cut -d: -f1)"
    if [ -n "$summary_line" ] && [ "$summary_line" -gt 1 ]; then
        printf '%s\n' "$output" | sed -n "1,$((summary_line - 1))p"
        echo "  (comparison ratio suppressed: a killed/errored run makes it ceiling/other_time, not a measurement — see #16196)"
    else
        printf '%s\n' "$output"
    fi
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
        slowdown) echo "runtime slowdown" ;;
        "oracle unavailable") echo "tsc oracle unavailable" ;;
        *) echo "diagnostic mismatch or compiler error" ;;
    esac
}

exit_codes_from_status() {
    local status="$1"
    printf '%s\n' "$status" | sed -E 's/[^0-9]+/\
/g' | sed '/^$/d'
}

# Resolve the directory backing the project-file-stats cache. That cache only
# avoids re-line-counting unchanged fixture sources when it OUTLIVES a single
# bench invocation; a previous default placed it under the per-run `$TEMP_DIR`,
# which the harness `rm -rf`s on EXIT, so the persistence machinery was dead and
# every row re-read every fixture from cold (issue #10923). Anchor the default
# to the run-surviving, gitignored `$BENCH_TARGET_DIR` (alongside the cached
# binary and external fixtures). An explicit `TSZ_PROJECT_FILE_STATS_CACHE_DIR`
# always wins; `$TMPDIR` is only the last resort when no persistent dir is known.
bench_project_file_stats_cache_dir() {
    printf '%s\n' "${TSZ_PROJECT_FILE_STATS_CACHE_DIR:-${BENCH_TARGET_DIR:-${TMPDIR:-/tmp}}/project-file-stats-cache}"
}

# Single-pass aggregate of (lines, bytes, files) under a TypeScript source
# tree. Used as the offline fallback when `project-file-stats.mjs` cannot load
# the TypeScript package (e.g. tsc tooling not yet installed). Walks the tree
# once and reads each file once via `wc -lc`, instead of the historical
# triple-walk + double-read pattern.
sum_ts_stats() {
    local src_dir="$1"
    local lines=0
    local bytes=0
    local files=0
    local file
    local lc
    local bc
    while IFS= read -r -d '' file; do
        # `wc -lc` reads each file once and outputs both counts; redirecting
        # via stdin keeps the file name out of the output. When `wc` fails,
        # the substitution is empty and the `read` leaves both vars unset.
        read -r lc bc <<<"$(wc -lc <"$file" 2>/dev/null)"
        [[ -n "$lc" && -n "$bc" ]] || continue
        lines=$((lines + lc))
        bytes=$((bytes + bc))
        files=$((files + 1))
    done < <(find "$src_dir" \( -path '*/node_modules/*' -o -path '*/.next/*' \) -prune -o \
        \( -name '*.ts' -o -name '*.tsx' -o -name '*.mts' -o -name '*.cts' \) -type f -print0 2>/dev/null)
    echo "$lines $bytes $files"
}

project_tsconfig_stats() {
    local tsconfig="$1"
    local fallback_src_dir="$2"
    local stats
    local cache_dir
    cache_dir="$(bench_project_file_stats_cache_dir)"

    if stats="$(TSC_TOOL_DIR_VALUE="$TSC_TOOL_DIR" TSC_BIN_VALUE="$TSC" \
        TSZ_PROJECT_FILE_STATS_CACHE_DIR="$cache_dir" \
        node "$SCRIPT_DIR/project-file-stats.mjs" "$tsconfig" 2>/dev/null)"; then
        echo "$stats"
        return
    fi

    sum_ts_stats "$fallback_src_dir"
}

# Timeout for pre-validation checks (seconds). Generous enough for heavy
# type-level libraries but catches infinite loops.
BENCH_TIMEOUT="${BENCH_TIMEOUT:-60}"

typescript_tool_entry_path() {
    printf '%s/node_modules/typescript/bin/tsc\n' "$1"
}

typescript_version_is_exact() {
    [[ "$1" =~ ^[0-9]+\.[0-9]+\.[0-9]+([+-][0-9A-Za-z.-]+)?$ ]]
}

typescript_tool_entry_is_valid() {
    local tool_dir="$1"
    local expected_version="$2"
    local package_json="$tool_dir/node_modules/typescript/package.json"
    local entry
    entry="$(typescript_tool_entry_path "$tool_dir")"
    [[ -n "$expected_version" && -f "$package_json" && -x "$entry" ]] || return 1

    local installed_version=""
    installed_version="$(node -e "const fs = require('fs'); const pkg = JSON.parse(fs.readFileSync(process.argv[1], 'utf8')); process.stdout.write(pkg.version || '');" "$package_json" 2>/dev/null)" || return 1
    [[ "$installed_version" == "$expected_version" ]] || return 1

    local reported_version=""
    reported_version="$("$entry" --version 2>/dev/null)" || return 1
    reported_version="${reported_version#Version }"
    [[ "$reported_version" == "$expected_version" ]]
}

ensure_tsgo() {
    # Honor explicit TSGO override when provided by caller.
    if [ -n "$TSGO" ]; then
        if [ ! -x "$TSGO" ]; then
            echo -e "${RED}✗ TSGO is set but not executable: $TSGO${NC}"
            exit 1
        fi
        return
    fi

    if ! command -v npm &>/dev/null; then
        echo -e "${RED}✗ npm not found${NC}"
        echo "  npm is required to auto-install tsgo ($TSGO_NPM_SPEC)"
        exit 1
    fi

    local expected_version=""
    if [[ "$TSGO_NPM_SPEC" == typescript@* ]]; then
        expected_version="${TSGO_NPM_SPEC#typescript@}"
    fi
    if ! typescript_version_is_exact "$expected_version"; then
        echo -e "${RED}✗ TSGO_NPM_SPEC must name an exact typescript package version: $TSGO_NPM_SPEC${NC}"
        exit 1
    fi

    mkdir -p "$TSGO_TOOL_DIR"
    local spec_file="$TSGO_TOOL_DIR/.tsgo-spec"
    local installed_spec=""
    if [ -f "$spec_file" ]; then
        installed_spec="$(cat "$spec_file")"
    fi

    if ! typescript_tool_entry_is_valid "$TSGO_TOOL_DIR" "$expected_version" \
        || [ "$installed_spec" != "$TSGO_NPM_SPEC" ]; then
        echo -e "${CYAN}Installing tsgo locally (${TSGO_NPM_SPEC})...${NC}"
        if ! run_with_timeout "$BENCH_NPM_INSTALL_TIMEOUT" npm install \
            --prefix "$TSGO_TOOL_DIR" \
            --no-audit \
            --no-fund \
            --include=optional \
            --loglevel=error \
            "$TSGO_NPM_SPEC" >/dev/null; then
            echo -e "${RED}✗ tsgo install timed out after ${BENCH_NPM_INSTALL_TIMEOUT}s${NC}"
            echo -e "${RED}  command: npm install --prefix \"$TSGO_TOOL_DIR\" \"$TSGO_NPM_SPEC\"${NC}"
            exit 1
        fi
        printf '%s\n' "$TSGO_NPM_SPEC" > "$spec_file"
    fi

    if ! typescript_tool_entry_is_valid "$TSGO_TOOL_DIR" "$expected_version"; then
        echo -e "${RED}✗ tsgo install failed validation: expected TypeScript $expected_version at $TSGO_LOCAL_BIN${NC}"
        exit 1
    fi

    TSGO="$TSGO_LOCAL_BIN"
}

resolve_tsc_npm_spec() {
    local sha=""
    if [ -d "$PROJECT_ROOT/TypeScript" ]; then
        sha="$(git -C "$PROJECT_ROOT/TypeScript" rev-parse HEAD 2>/dev/null || echo "")"
    fi

    node -e "const v=require('./scripts/conformance/typescript-versions.json'); const sha=process.argv[1]; const current=v.current && v.mappings?.[v.current]?.npm; const m=sha && v.mappings?.[sha]; console.log(m?.npm || (sha ? v.default?.npm : current) || '');" "$sha"
}

ensure_tsc() {
    # Honor explicit TSC override when provided by caller.
    if [ -n "$TSC" ]; then
        if [ ! -x "$TSC" ]; then
            echo -e "${RED}✗ TSC is set but not executable: $TSC${NC}"
            exit 1
        fi
        return
    fi

    if ! command -v npm &>/dev/null; then
        echo -e "${RED}✗ npm not found${NC}"
        echo "  npm is required to auto-install tsc"
        exit 1
    fi

    local resolved_spec="$TSC_NPM_SPEC"
    if [ -z "$resolved_spec" ]; then
        resolved_spec="$(resolve_tsc_npm_spec)"
    fi
    if [ -z "$resolved_spec" ]; then
        echo -e "${RED}✗ Unable to resolve tsc npm spec${NC}"
        echo "  Set TSC_NPM_SPEC or update scripts/conformance/typescript-versions.json."
        exit 1
    fi
    if ! typescript_version_is_exact "$resolved_spec"; then
        echo -e "${RED}✗ TSC_NPM_SPEC must be an exact TypeScript version: $resolved_spec${NC}"
        exit 1
    fi

    mkdir -p "$TSC_TOOL_DIR"
    local spec_file="$TSC_TOOL_DIR/.tsc-spec"
    local installed_spec=""
    if [ -f "$spec_file" ]; then
        installed_spec="$(cat "$spec_file")"
    fi

    if ! typescript_tool_entry_is_valid "$TSC_TOOL_DIR" "$resolved_spec" \
        || [ "$installed_spec" != "$resolved_spec" ]; then
        echo -e "${CYAN}Installing tsc locally (${resolved_spec})...${NC}"
        if ! run_with_timeout "$BENCH_NPM_INSTALL_TIMEOUT" npm install \
            --prefix "$TSC_TOOL_DIR" \
            --no-audit \
            --no-fund \
            --include=optional \
            --loglevel=error \
            "typescript@${resolved_spec}" >/dev/null; then
            echo -e "${RED}✗ tsc install timed out after ${BENCH_NPM_INSTALL_TIMEOUT}s${NC}"
            echo -e "${RED}  command: npm install --prefix \"$TSC_TOOL_DIR\" \"typescript@${resolved_spec}\"${NC}"
            exit 1
        fi
        printf '%s\n' "$resolved_spec" > "$spec_file"
    fi

    if ! typescript_tool_entry_is_valid "$TSC_TOOL_DIR" "$resolved_spec"; then
        echo -e "${RED}✗ tsc install failed validation: expected TypeScript $resolved_spec at $TSC_LOCAL_BIN${NC}"
        exit 1
    fi

    TSC="$TSC_LOCAL_BIN"
}

# Run the PGO instrumented binary over a workload mix that exercises the same
# code paths the website plots: lib loading, mapped/conditional types, deep
# generics, project-mode resolution, best-common-type, inference, and CFA
# stress. Always trains on generated synthetic inputs first (available even in
# a clean checkout), then layers on whichever external bench fixtures are
# present. utility-types/ts-toolbelt/ts-essentials can be opportunistically
# fetched by the caller; the rest are picked up only when prior bench runs left
# them in place.
#
# Set BENCH_PGO_EXTRA_INPUTS to a colon-separated list of additional
# tsconfig or .ts paths to include in training. Set BENCH_PGO_VERBOSE=1
# to surface the per-input wall time.
collect_pgo_workload() {
    local pgo_tsz="$1"
    local env_prefix=()
    [[ -n "${TSZ_LIB_DIR:-}" ]] && env_prefix=("TSZ_LIB_DIR=$TSZ_LIB_DIR")
    BENCH_PGO_TRAINING_INPUTS=()
    BENCH_PGO_TRAINING_FAILED_INPUTS=()

    _pgo_run() {
        local label="$1"
        shift
        local run_status=0
        BENCH_PGO_TRAINING_INPUTS+=("$label")
        if [[ "${BENCH_PGO_VERBOSE:-0}" == "1" ]]; then
            local t0 t1
            t0=$(date +%s)
            if run_with_timeout "$BENCH_PGO_TSZ_TIMEOUT" env ${env_prefix[@]+"${env_prefix[@]}"} "$@" >/dev/null 2>&1; then
                run_status=0
            else
                run_status=$?
                BENCH_PGO_TRAINING_FAILED_INPUTS+=("$label:$run_status")
                if [ "$run_status" -eq 124 ]; then
                    echo -e "${YELLOW}PGO training input \"$label\" timed out after ${BENCH_PGO_TSZ_TIMEOUT}s${NC}"
                else
                    echo -e "${YELLOW}PGO training input \"$label\" failed (exit $run_status)${NC}"
                fi
            fi
            t1=$(date +%s)
            echo -e "  ${CYAN}pgo${NC} $label ($((t1 - t0))s)"
        else
            if run_with_timeout "$BENCH_PGO_TSZ_TIMEOUT" env ${env_prefix[@]+"${env_prefix[@]}"} "$@" >/dev/null 2>&1; then
                run_status=0
            else
                run_status=$?
                BENCH_PGO_TRAINING_FAILED_INPUTS+=("$label:$run_status")
                if [ "$run_status" -eq 124 ]; then
                    echo -e "${YELLOW}PGO training input \"$label\" timed out after ${BENCH_PGO_TSZ_TIMEOUT}s${NC}"
                else
                    echo -e "${YELLOW}PGO training input \"$label\" failed (exit $run_status)${NC}"
                fi
            fi
        fi
    }

    # 1. Tiny inline expression — exercises argv parsing, lib-resolver
    #    bootstrap, scanner/parser/binder warm-up paths.
    echo "const x: number = 1; type T<U> = U extends string ? U[] : U; const y: T<string> = ['a'];" \
        | _pgo_run "stdin:scalar" "$pgo_tsz" --noEmit /dev/stdin

    # 2. Generated stress cases mirror the benchmark shards directly, so the
    #    profile is not dominated by one project fixture or one tiny input.
    if [[ "${BENCH_PGO_SYNTHETIC:-1}" == "1" ]]; then
        local pgo_tmp
        pgo_tmp="$(mktemp -d "$BENCH_TARGET_DIR/pgo-train.XXXXXX")"
        local -a generated_inputs=()

        generate_complex_file 50 "$pgo_tmp/complex_generics.ts"
        generated_inputs+=("$pgo_tmp/complex_generics.ts")
        generate_deeppartial_optional_chain_file 50 "$pgo_tmp/deeppartial_optional_chain.ts"
        generated_inputs+=("$pgo_tmp/deeppartial_optional_chain.ts")
        generate_recursive_utility_alias_file 30 "$pgo_tmp/recursive_utility_alias.ts"
        generated_inputs+=("$pgo_tmp/recursive_utility_alias.ts")
        generate_shallow_optional_chain_file 50 "$pgo_tmp/shallow_optional_chain.ts"
        generated_inputs+=("$pgo_tmp/shallow_optional_chain.ts")
        generate_union_file 100 "$pgo_tmp/union_members.ts"
        generated_inputs+=("$pgo_tmp/union_members.ts")
        generate_recursive_generic_file 25 "$pgo_tmp/recursive_generic.ts"
        generated_inputs+=("$pgo_tmp/recursive_generic.ts")
        generate_conditional_distribution_file 50 "$pgo_tmp/conditional_dist.ts"
        generated_inputs+=("$pgo_tmp/conditional_dist.ts")
        generate_mapped_type_file 100 "$pgo_tmp/mapped_type.ts"
        generated_inputs+=("$pgo_tmp/mapped_type.ts")
        generate_template_literal_file 50 "$pgo_tmp/template_literal.ts"
        generated_inputs+=("$pgo_tmp/template_literal.ts")
        generate_deep_subtype_file 30 "$pgo_tmp/deep_subtype.ts"
        generated_inputs+=("$pgo_tmp/deep_subtype.ts")
        generate_intersection_file 50 "$pgo_tmp/intersection.ts"
        generated_inputs+=("$pgo_tmp/intersection.ts")
        generate_infer_stress_file 15 "$pgo_tmp/infer_stress.ts"
        generated_inputs+=("$pgo_tmp/infer_stress.ts")
        generate_cfa_stress_file 50 "$pgo_tmp/cfa_stress.ts"
        generated_inputs+=("$pgo_tmp/cfa_stress.ts")
        generate_bct_stress_file 50 "$pgo_tmp/bct_stress.ts"
        generated_inputs+=("$pgo_tmp/bct_stress.ts")
        generate_constraint_conflict_file 30 "$pgo_tmp/constraint_conflict.ts"
        generated_inputs+=("$pgo_tmp/constraint_conflict.ts")
        generate_mapped_complex_template_file 50 "$pgo_tmp/mapped_complex_template.ts"
        generated_inputs+=("$pgo_tmp/mapped_complex_template.ts")

        local generated
        for generated in "${generated_inputs[@]}"; do
            _pgo_run "synthetic:$(basename "$generated")" \
                "$pgo_tsz" --noEmit "$generated"
        done
        rm -rf "$pgo_tmp"
    fi

    # 3. utility-types: small (~150 src files) but very heavy on mapped/
    #    conditional types — the shape that dominates the website plot.
    if [ -d "$UTILITY_TYPES_DIR" ] && [ -f "$UTILITY_TYPES_DIR/tsconfig.flat.json" ]; then
        _pgo_run "utility-types" \
            "$pgo_tsz" --noEmit -p "$UTILITY_TYPES_DIR/tsconfig.flat.json"
    fi

    # 4. ts-toolbelt + ts-essentials are useful deep-generic training inputs,
    #    but too slow for the default cold bench-prepare path. Keep them opt-in
    #    for deliberate local/CI experiments.
    if [[ "${BENCH_PGO_FETCH_CORE_PROJECTS:-0}" == "1" ]]; then
        if [ -d "$TS_TOOLBELT_DIR" ] && [ -f "$TS_TOOLBELT_DIR/tsconfig.flat.json" ]; then
            _pgo_run "ts-toolbelt" \
                "$pgo_tsz" --noEmit -p "$TS_TOOLBELT_DIR/tsconfig.flat.json"
        fi
        if [ -d "$TS_ESSENTIALS_DIR" ] && [ -f "$TS_ESSENTIALS_DIR/tsconfig.flat.json" ]; then
            _pgo_run "ts-essentials" \
                "$pgo_tsz" --noEmit -p "$TS_ESSENTIALS_DIR/tsconfig.flat.json"
        fi
    fi
    if [ -d "$RXJS_DIR" ] && [ -f "$RXJS_DIR/tsconfig.flat.json" ]; then
        _pgo_run "rxjs" "$pgo_tsz" --noEmit -p "$RXJS_DIR/tsconfig.flat.json"
    fi
    if [ -d "$TYPE_FEST_DIR" ] && [ -f "$TYPE_FEST_DIR/tsconfig.flat.json" ]; then
        _pgo_run "type-fest" "$pgo_tsz" --noEmit -p "$TYPE_FEST_DIR/tsconfig.flat.json"
    fi
    if [ -d "$VITE_APP_BENCH_DIR" ] && [ -f "$VITE_APP_BENCH_DIR/tsconfig.json" ]; then
        _pgo_run "vite-vanilla-ts-app" "$pgo_tsz" --noEmit -p "$VITE_APP_BENCH_DIR/tsconfig.json"
    fi
    if [ -d "$NEXT_APP_BENCH_DIR" ] && [ -f "$NEXT_APP_BENCH_DIR/tsconfig.json" ]; then
        _pgo_run "nextjs-fresh-app" "$pgo_tsz" --noEmit -p "$NEXT_APP_BENCH_DIR/tsconfig.json"
    fi

    # 5. TypeScript compiler test fixture — kept for back-compat with the
    #    pre-existing training input. Only triggers when the upstream
    #    TypeScript submodule is checked out locally (rare; tracked in
    #    `.claude/CLAUDE.md` §19.5).
    if [ -f "$PROJECT_ROOT/TypeScript/tests/cases/compiler/manyConstExports.ts" ]; then
        for _i in 1 2; do
            _pgo_run "manyConstExports.ts" \
                "$pgo_tsz" --noEmit \
                "$PROJECT_ROOT/TypeScript/tests/cases/compiler/manyConstExports.ts"
        done
    fi

    # 6. Caller-provided extras (colon-separated). Useful when adding a new
    #    benchmark fixture: warm PGO against it before measuring.
    if [ -n "${BENCH_PGO_EXTRA_INPUTS:-}" ]; then
        local IFS=":"
        # shellcheck disable=SC2206
        local extras=( ${BENCH_PGO_EXTRA_INPUTS} )
        for input in "${extras[@]}"; do
            [ -z "$input" ] && continue
            if [ -f "$input" ] && [[ "$input" == *tsconfig*.json ]]; then
                _pgo_run "extra:$(basename "$input")" \
                    "$pgo_tsz" --noEmit -p "$input"
            elif [ -f "$input" ]; then
                _pgo_run "extra:$(basename "$input")" \
                    "$pgo_tsz" --noEmit "$input"
            fi
        done
    fi
}

check_prerequisites() {
    print_header "Prerequisites Check"

    # Check hyperfine
    if ! command -v hyperfine &>/dev/null; then
        echo -e "${RED}✗ hyperfine not found${NC}"
        echo "  Install with: brew install hyperfine"
        exit 1
    fi
    echo -e "${GREEN}✓${NC} hyperfine $(hyperfine --version | head -1)"

    # Check jq (optional, for results table)
    if command -v jq &>/dev/null; then
        echo -e "${GREEN}✓${NC} jq $(jq --version)"
    else
        echo -e "${YELLOW}○${NC} jq not found (optional, install for results table)"
    fi

    # Check for lib assets directory used by tsz
    if [ -n "${TSZ_LIB_DIR:-}" ]; then
        if [ ! -d "$TSZ_LIB_DIR" ]; then
            echo -e "${RED}✗ lib directory not found: $TSZ_LIB_DIR${NC}"
            echo "  Set TSZ_LIB_DIR or ensure crates/tsz-core/data/lib exists."
            exit 1
        fi
        echo -e "${GREEN}✓${NC} tsz lib assets: $TSZ_LIB_DIR"
    else
        echo -e "${GREEN}✓${NC} tsz lib assets: retained at crates/tsz-core/data/lib"
    fi

    # Check/build tsz with the dedicated benchmark target directory unless
    # caller provided TSZ.
    local need_rebuild=false

    if [ "$TSZ_IS_OVERRIDE" = true ]; then
        if [ "$FORCE_REBUILD" = true ]; then
            echo -e "${YELLOW}Ignoring --rebuild because TSZ override is set: $TSZ${NC}"
        fi
        if [ ! -x "$TSZ" ]; then
            echo -e "${RED}✗ TSZ is set but not executable: $TSZ${NC}"
            exit 1
        fi
    elif [ "$FORCE_REBUILD" = true ]; then
        echo -e "${YELLOW}Force rebuild requested...${NC}"
        need_rebuild=true
    elif [ ! -x "$TSZ" ]; then
        echo -e "${YELLOW}Binary not found, building...${NC}"
        need_rebuild=true
    else
        # Verify binary is recent (rebuilt if any Rust source in the workspace
        # changed since the last benchmark build).
        local newest_src
        newest_src="$(find "$PROJECT_ROOT" \
            \( -path "$BENCH_TARGET_DIR" -o -path "$PROJECT_ROOT/.git" \) -prune -o \
            -type f -name "*.rs" -newer "$TSZ" -print -quit 2>/dev/null)"
        if [ -n "$newest_src" ]; then
            echo -e "${YELLOW}Source changed since last build, rebuilding...${NC}"
            need_rebuild=true
        elif [[ "${BENCH_REQUIRE_PGO:-0}" == "1" && ! -f "$BENCH_PGO_MARKER" ]]; then
            echo -e "${YELLOW}Existing tsz binary is not marked PGO optimized, rebuilding...${NC}"
            need_rebuild=true
        fi
    fi

    if [ "$need_rebuild" = true ]; then
        echo -e "${CYAN}Building tsz with dist profile (LTO=fat, codegen-units=1)${NC}"
        echo -e "${CYAN}Target directory: $BENCH_TARGET_DIR${NC}"

        # PGO (Profile-Guided Optimization): collect profile data then rebuild.
        # This typically gives a better optimized binary for full benchmark
        # runs, but quick mode prefers a deterministic fast rebuild.
        local pgo_dir="$BENCH_TARGET_DIR/pgo-data"
        local pgo_merged="$pgo_dir/merged.profdata"
        local pgo_training_metadata="$pgo_dir/profile.metadata"
        # Cross-run profdata cache so iterative bench dev doesn't redo the
        # instrumented-build + training cycle when the source/toolchain/training
        # workload fingerprint has not changed since the last bench run.
        local pgo_cache_dir="$BENCH_TARGET_DIR/pgo-cache"
        local pgo_cache_profdata="$pgo_cache_dir/merged.profdata"
        local pgo_cache_marker="$pgo_cache_dir/profile.fingerprint"
        local pgo_cache_metadata="$pgo_cache_dir/profile.metadata"
        local bench_rust_target_cpu="${BENCH_RUST_TARGET_CPU:-native}"
        local bench_rust_target_flags="-Ctarget-cpu=${bench_rust_target_cpu}"
        local pgo_target_dir
        local optimized_target_dir
        local pgo_profile_data_source="fresh"
        local pgo_optimized=false
        mkdir -p "$BENCH_TARGET_DIR"
        pgo_target_dir="$(mktemp -d "$BENCH_TARGET_DIR/pgo-build.XXXXXX")"
        optimized_target_dir="$(mktemp -d "$BENCH_TARGET_DIR/build.XXXXXX")"
        local llvm_profdata
        llvm_profdata="$(ls "$(rustc --print sysroot)"/lib/rustlib/*/bin/llvm-profdata 2>/dev/null | head -1 || true)"
        local use_pgo=true
        if [[ "$QUICK_MODE" == true || "${BENCH_PGO:-1}" != "1" ]]; then
            use_pgo=false
        fi

        if [ "$use_pgo" = true ] && [ -n "$llvm_profdata" ] && [ -x "$llvm_profdata" ]; then
            local skip_pgo_collect=false
            local profdata_ready=false
            local pgo_fingerprint
            pgo_fingerprint="$(pgo_profile_fingerprint)"
            if [[ "${BENCH_PGO_CACHE:-1}" == "1" && -f "$pgo_cache_profdata" && -f "$pgo_cache_marker" ]]; then
                local cached_fingerprint
                cached_fingerprint="$(tr -d '[:space:]' < "$pgo_cache_marker" 2>/dev/null || true)"
                if [[ "$cached_fingerprint" == "$pgo_fingerprint" ]]; then
                    echo -e "${CYAN}PGO cache hit: reusing $pgo_cache_profdata${NC}"
                    mkdir -p "$pgo_dir"
                    cp "$pgo_cache_profdata" "$pgo_merged"
                    if [ -f "$pgo_cache_metadata" ]; then
                        cp "$pgo_cache_metadata" "$pgo_training_metadata"
                    else
                        write_pgo_training_unavailable_metadata "$pgo_training_metadata" "$pgo_fingerprint" "$llvm_profdata"
                    fi
                    pgo_profile_data_source="cache"
                    skip_pgo_collect=true
                    profdata_ready=true
                else
                    echo -e "${YELLOW}PGO cache stale (training fingerprint changed); regenerating profile data${NC}"
                fi
            fi

            if [ "$skip_pgo_collect" = false ]; then
                echo -e "${CYAN}PGO Step 1/3: Building instrumented binary...${NC}"
                rm -rf "$pgo_dir"
                mkdir -p "$pgo_dir"
                # Keep trainer codegen aligned with the final dist build by
                # default; otherwise LLVM discards a lot of profile counts as
                # CFG hash mismatches during profile-use. BENCH_PGO_PANIC_UNWIND=1
                # is still useful when deliberately training on crashy inputs,
                # because panic=abort can skip LLVM's profiling atexit flush.
                local pgo_generate_rustflags="-Cprofile-generate=$pgo_dir ${bench_rust_target_flags}"
                if [[ "${BENCH_PGO_PANIC_UNWIND:-0}" == "1" ]]; then
                    pgo_generate_rustflags="$pgo_generate_rustflags -Cpanic=unwind"
                fi
                run_cargo_build \
                    "PGO Step 1/3: instrumented binary build" \
                    CARGO_TARGET_DIR="$pgo_target_dir" \
                    CARGO_INCREMENTAL=0 \
                    RUSTFLAGS="$pgo_generate_rustflags" \
                    cargo build --profile dist -p tsz-cli --bin tsz || true

                # Ensure representative external bench fixtures are present so
                # PGO trains on workload shapes the website actually measures,
                # not a single-token const expression. The clones are best-effort:
                # transient network failures should not abort bench-prepare; PGO
                # still trains on generated synthetic inputs.
                if [[ "${BENCH_PGO_FETCH_UTILITY_TYPES:-1}" == "1" ]]; then
                    if ! ensure_utility_types_fixture; then
                        echo -e "${YELLOW}Warning: utility-types fetch failed; continuing without it for PGO training${NC}"
                    fi
                fi
                if [[ "${BENCH_PGO_FETCH_CORE_PROJECTS:-0}" == "1" ]]; then
                    if ! ensure_ts_toolbelt_fixture; then
                        echo -e "${YELLOW}Warning: ts-toolbelt fetch failed; continuing without it for PGO training${NC}"
                    fi
                    if ! ensure_ts_essentials_fixture; then
                        echo -e "${YELLOW}Warning: ts-essentials fetch failed; continuing without it for PGO training${NC}"
                    fi
                fi

                echo -e "${CYAN}PGO Step 2/3: Collecting profile data...${NC}"
                local pgo_tsz="$pgo_target_dir/dist/tsz"
                collect_pgo_workload "$pgo_tsz"

                # An empty glob (bash without nullglob) passes a literal "*.profraw"
                # path to llvm-profdata and fails; array-glob + -e avoids the fork
                # and the TOCTOU window between a count check and the merge call.
                local profraw_files=("$pgo_dir"/*.profraw)
                if [[ -e "${profraw_files[0]}" ]]; then
                    "$llvm_profdata" merge -o "$pgo_merged" "${profraw_files[@]}"
                    write_pgo_training_metadata "$pgo_training_metadata" "$pgo_fingerprint" "$llvm_profdata"
                    profdata_ready=true
                    if [[ "${BENCH_PGO_CACHE:-1}" == "1" ]]; then
                        mkdir -p "$pgo_cache_dir"
                        cp "$pgo_merged" "$pgo_cache_profdata"
                        cp "$pgo_training_metadata" "$pgo_cache_metadata"
                        printf '%s\n' "$pgo_fingerprint" > "$pgo_cache_marker"
                    fi
                else
                    echo -e "${YELLOW}PGO training produced no profraw files; skipping PGO optimization${NC}"
                fi
            fi

            if [[ "$profdata_ready" == true ]]; then
                echo -e "${CYAN}PGO Step 3/3: Building optimized binary with profile data...${NC}"
                if ! run_cargo_build \
                    "PGO Step 3/3: optimized binary build" \
                    CARGO_TARGET_DIR="$optimized_target_dir" \
                    CARGO_INCREMENTAL=0 \
                    RUSTFLAGS="-Cprofile-use=$pgo_merged ${bench_rust_target_flags}" \
                    cargo build --profile dist -p tsz-cli --bin tsz; then
                    if [[ "${BENCH_REQUIRE_PGO:-0}" == "1" ]]; then
                        echo -e "${RED}✗ PGO dist build failed and BENCH_REQUIRE_PGO=1${NC}"
                        exit 1
                    fi
                    # LLVM PGO can fail when the profile-use link step encounters
                    # incompatible bitcode/ProfileSummary metadata in this toolchain.
                    echo -e "${YELLOW}PGO dist build failed; falling back to a clean standard dist build${NC}"
                    rm -rf "$optimized_target_dir"
                    optimized_target_dir="$(mktemp -d "$BENCH_TARGET_DIR/build.XXXXXX")"
                    run_cargo_build \
                        "PGO Step 3 fallback: standard dist build" \
                        CARGO_TARGET_DIR="$optimized_target_dir" \
                        CARGO_INCREMENTAL=0 \
                        RUSTFLAGS="$bench_rust_target_flags" \
                        cargo build --profile dist -p tsz-cli --bin tsz
                else
                    pgo_optimized=true
                fi
            else
                if [[ "${BENCH_REQUIRE_PGO:-0}" == "1" ]]; then
                    echo -e "${RED}✗ PGO profile data unavailable and BENCH_REQUIRE_PGO=1${NC}"
                    exit 1
                fi
                echo -e "${YELLOW}PGO Step 3/3: no profile data available; using standard dist build${NC}"
                rm -rf "$optimized_target_dir"
                optimized_target_dir="$(mktemp -d "$BENCH_TARGET_DIR/build.XXXXXX")"
                run_cargo_build \
                    "PGO Step 3: standard dist build" \
                    CARGO_TARGET_DIR="$optimized_target_dir" \
                    CARGO_INCREMENTAL=0 \
                    RUSTFLAGS="$bench_rust_target_flags" \
                    cargo build --profile dist -p tsz-cli --bin tsz
            fi
            mkdir -p "$TSZ_OUTPUT_DIR"
            install -m 755 "$optimized_target_dir/dist/tsz" "$TSZ"
            if [[ "$pgo_optimized" == true ]]; then
                write_pgo_marker "$BENCH_PGO_MARKER" "$pgo_merged" "$pgo_training_metadata" "$pgo_profile_data_source"
            else
                rm -f "$BENCH_PGO_MARKER"
            fi
            rm -rf "$optimized_target_dir"
            rm -rf "$pgo_target_dir"
        else
            if [[ "$QUICK_MODE" == true || "${BENCH_PGO:-1}" != "1" ]]; then
                echo -e "${YELLOW}PGO skipped (quick mode or BENCH_PGO=0); using standard dist build${NC}"
            else
                echo -e "${YELLOW}PGO unavailable (llvm-profdata not found), using standard build${NC}"
            fi
            if [[ "${BENCH_REQUIRE_PGO:-0}" == "1" ]]; then
                echo -e "${RED}✗ PGO is required for this benchmark run${NC}"
                exit 1
            fi
            run_cargo_build \
                "Standard dist build" \
                CARGO_TARGET_DIR="$optimized_target_dir" \
                CARGO_INCREMENTAL=0 \
                RUSTFLAGS="$bench_rust_target_flags" \
                cargo build --profile dist -p tsz-cli --bin tsz
            mkdir -p "$TSZ_OUTPUT_DIR"
            install -m 755 "$optimized_target_dir/dist/tsz" "$TSZ"
            rm -f "$BENCH_PGO_MARKER"
            rm -rf "$optimized_target_dir"
            rm -rf "$pgo_target_dir"
        fi
    fi

    # Preflight: the binary must execute on this machine. A target-cpu above
    # this host's ISA dies with SIGILL (exit 132) and would otherwise surface
    # only as per-row crash artifacts at measurement time (#12764, #13248).
    local tsz_version
    local tsz_preflight_status=0
    tsz_version="$("$TSZ" --version 2>&1)" || tsz_preflight_status=$?
    if [ "$tsz_preflight_status" -ne 0 ]; then
        echo -e "${RED}✗ tsz binary preflight failed (exit ${tsz_preflight_status}): $TSZ${NC}"
        echo -e "${RED}  Built with target-cpu=${BENCH_RUST_TARGET_CPU:-native}; this machine may not support that ISA.${NC}"
        exit 1
    fi

    echo -e "${GREEN}✓${NC} tsz: ${tsz_version%%$'\n'*}"
    echo -e "   Binary: $TSZ"
    echo -e "   Size: $(ls -lh "$TSZ" | awk '{print $5}')"
    echo -e "   Built: $(stat -c '%y' "$TSZ" 2>/dev/null | cut -d. -f1 || stat -f '%Sm' -t '%Y-%m-%d %H:%M:%S' "$TSZ" 2>/dev/null || echo 'unknown')"

    # Check/install tsgo
    ensure_tsgo
    echo -e "${GREEN}✓${NC} tsgo: $($TSGO --version 2>&1 | head -1)"
    echo -e "   Binary: $TSGO"

    # Check/install tsc
    ensure_tsc
    echo -e "${GREEN}✓${NC} tsc: $($TSC --version 2>&1 | head -1)"
    echo -e "   Binary: $TSC"
}

RESULTS_CSV=""
BENCHMARKS_RUN=0
PROJECT_COMPATIBILITY_JSONL=""
LAST_PEAK_RSS_BYTES=0
