# shellcheck shell=bash
# The PROJECT_EVIDENCE_* outputs are this sourced library's caller-facing API;
# PROJECT_ROOT is likewise supplied by the benchmark entry point.
# shellcheck disable=SC1091,SC2034,SC2153
# Exact project-admission proof shared by the benchmark runner and the project
# compile guard's machine contracts. A benchmark row is never timing-eligible
# merely because a fixture walk or compiler exit says it succeeded: TSZ must
# expose its actual schema-v2 program graph and exactly match pinned TS7's graph,
# diagnostic records, and ordinary exit code first.

# The graph parser and diagnostic normalizer are the same implementation used by
# scripts/ci/project-compile-guard.sh. Keep this library source-only so focused
# fake-compiler tests can exercise the real gate without preparing the corpus.
if ! declare -F tsz_fingerprint_project_root >/dev/null 2>&1; then
  # shellcheck source=scripts/ci/lib/project-compile-fingerprint.sh
  source "$PROJECT_ROOT/scripts/ci/lib/project-compile-fingerprint.sh"
fi
if ! declare -F tsz_diagnostic_multisets_agree >/dev/null 2>&1; then
  # shellcheck source=scripts/ci/lib/project-tsc-oracle.sh
  source "$PROJECT_ROOT/scripts/ci/lib/project-tsc-oracle.sh"
fi

PROJECT_EVIDENCE_STATS_READER="${PROJECT_EVIDENCE_STATS_READER:-$PROJECT_ROOT/scripts/ci/project-compile-stats.mjs}"
PROJECT_EVIDENCE_STUB_INVENTORY_READER="${PROJECT_EVIDENCE_STUB_INVENTORY_READER:-$PROJECT_ROOT/scripts/bench/lib/fixture-stub-inventory.mjs}"

project_evidence_reset() {
  PROJECT_EVIDENCE_SCHEMA=""
  PROJECT_EVIDENCE_REASON=""
  PROJECT_EVIDENCE_EXIT_CLASS="oracle unavailable"
  PROJECT_EVIDENCE_DIAGNOSTIC_STATUS="pinned TypeScript 7 evidence unavailable"
  PROJECT_EVIDENCE_DIAGNOSTIC_DELTA=""
  PROJECT_EVIDENCE_FILES_REACHED_REASON="compiler stats missing"
  PROJECT_EVIDENCE_TSZ_RC=""
  PROJECT_EVIDENCE_TSC_RC=""
  PROJECT_EVIDENCE_SEMANTIC_COMPLETION=""
  PROJECT_EVIDENCE_TSZ_ROOT_FILES=""
  PROJECT_EVIDENCE_TSZ_SOURCE_FILES=""
  PROJECT_EVIDENCE_TSZ_ROOT_FINGERPRINT=""
  PROJECT_EVIDENCE_TSZ_SOURCE_FINGERPRINT=""
  PROJECT_EVIDENCE_TSC_ROOT_FILES=""
  PROJECT_EVIDENCE_TSC_SOURCE_FILES=""
  PROJECT_EVIDENCE_TSC_ROOT_FINGERPRINT=""
  PROJECT_EVIDENCE_TSC_SOURCE_FINGERPRINT=""
  PROJECT_EVIDENCE_TSZ_DIAGNOSTIC_RECORDS=""
  PROJECT_EVIDENCE_TSC_DIAGNOSTIC_RECORDS=""
  PROJECT_EVIDENCE_TSZ_DIAGNOSTIC_FINGERPRINT=""
  PROJECT_EVIDENCE_TSC_DIAGNOSTIC_FINGERPRINT=""
  PROJECT_EVIDENCE_STUB_INVENTORY_SCHEMA=""
  PROJECT_EVIDENCE_STUBBED_MODULES=""
  PROJECT_EVIDENCE_STUBBED_ANY_MEMBERS=""
  PROJECT_EVIDENCE_STUB_INVENTORY_FINGERPRINT=""
}

project_evidence_fail() {
  PROJECT_EVIDENCE_REASON="$1"
  PROJECT_EVIDENCE_EXIT_CLASS="${2:-oracle unavailable}"
  PROJECT_EVIDENCE_DIAGNOSTIC_STATUS="${3:-pinned TypeScript 7 evidence unavailable}"
  PROJECT_EVIDENCE_DIAGNOSTIC_DELTA="harness: $PROJECT_EVIDENCE_REASON"
  return 1
}

project_evidence_tsc_lib_dir() {
  if [[ -n "${PROJECT_EVIDENCE_TSC_BUILTIN_LIB_DIR:-}" \
    && -d "$PROJECT_EVIDENCE_TSC_BUILTIN_LIB_DIR" ]]; then
    (cd "$PROJECT_EVIDENCE_TSC_BUILTIN_LIB_DIR" && pwd -P)
    return
  fi
  local word candidate=""
  for word in "${PROJECT_EVIDENCE_TSC_CMD[@]}"; do
    if [[ "$word" == */bin/tsc && -e "$word" ]]; then
      candidate="$(dirname "$word")/../lib"
    fi
  done
  [[ -n "$candidate" && -d "$candidate" ]] && (cd "$candidate" && pwd -P)
}

project_evidence_read_tsz_stats() {
  local stats_file="$1" config_dir="$2" project_root="$3" parsed=""
  if [[ ! -f "$stats_file" ]]; then
    PROJECT_EVIDENCE_FILES_REACHED_REASON="compiler stats missing"
    return 1
  fi
  if ! parsed="$(node "$PROJECT_EVIDENCE_STATS_READER" compiler-stats \
      "$stats_file" "$config_dir" "$project_root" 2>/dev/null)"; then
    PROJECT_EVIDENCE_FILES_REACHED_REASON="compiler stats malformed"
    return 1
  fi
  IFS=$'\t' read -r PROJECT_EVIDENCE_TSZ_ROOT_FILES \
    PROJECT_EVIDENCE_TSZ_SOURCE_FILES PROJECT_EVIDENCE_TSZ_ROOT_FINGERPRINT \
    PROJECT_EVIDENCE_TSZ_SOURCE_FINGERPRINT \
    PROJECT_EVIDENCE_SEMANTIC_COMPLETION <<< "$parsed"
  [[ "$PROJECT_EVIDENCE_TSZ_ROOT_FILES" =~ ^(0|[1-9][0-9]*)$ \
    && "$PROJECT_EVIDENCE_TSZ_SOURCE_FILES" =~ ^(0|[1-9][0-9]*)$ \
    && "$PROJECT_EVIDENCE_TSZ_ROOT_FINGERPRINT" =~ ^[0-9a-f]{64}$ \
    && "$PROJECT_EVIDENCE_TSZ_SOURCE_FINGERPRINT" =~ ^[0-9a-f]{64}$ ]] || {
      PROJECT_EVIDENCE_FILES_REACHED_REASON="compiler stats malformed"
      return 1
    }
  case "$PROJECT_EVIDENCE_SEMANTIC_COMPLETION" in
    complete|deferred|cycle|limit) ;;
    *) PROJECT_EVIDENCE_FILES_REACHED_REASON="compiler stats malformed"; return 1 ;;
  esac
  PROJECT_EVIDENCE_FILES_REACHED_REASON=""
}

project_evidence_ordinary_rc() {
  [[ "$1" =~ ^[0-4]$ ]]
}

# collect_project_evidence <name> <tsconfig> <source-root> <tsz-log> <tsc-log>
#
# Callers provide PROJECT_EVIDENCE_TSZ_CMD and PROJECT_EVIDENCE_TSC_CMD arrays.
# On success, PROJECT_EVIDENCE_SCHEMA=2 and every TSZ/TS7 count/fingerprint is
# populated. On failure the reason/status globals describe a non-timing row.
collect_project_evidence() {
  local name="$1" tsconfig="$2" src_dir="$3" tsz_log="$4" tsc_log="$5"
  project_evidence_reset
  if ! declare -p PROJECT_EVIDENCE_TSZ_CMD >/dev/null 2>&1 \
    || ! declare -p PROJECT_EVIDENCE_TSC_CMD >/dev/null 2>&1; then
    project_evidence_fail "compiler command unavailable"
    return 1
  fi
  if [[ "${#PROJECT_EVIDENCE_TSZ_CMD[@]}" -eq 0 \
    || "${#PROJECT_EVIDENCE_TSC_CMD[@]}" -eq 0 ]]; then
    project_evidence_fail "compiler command unavailable"
    return 1
  fi
  if [[ ! -f "$PROJECT_EVIDENCE_STATS_READER" ]]; then
    project_evidence_fail "project stats reader unavailable"
    return 1
  fi
  if [[ ! -f "$PROJECT_EVIDENCE_STUB_INVENTORY_READER" ]]; then
    PROJECT_EVIDENCE_FILES_REACHED_REASON="fixture stub inventory unavailable"
    project_evidence_fail "fixture stub inventory reader unavailable" \
      "runner error" "fixture stub inventory unavailable"
    return 1
  fi

  local stub_evidence=""
  if ! stub_evidence="$(node "$PROJECT_EVIDENCE_STUB_INVENTORY_READER" \
      row-evidence "$PROJECT_ROOT" "$name" 2>/dev/null)"; then
    PROJECT_EVIDENCE_FILES_REACHED_REASON="fixture stub inventory unavailable"
    project_evidence_fail "fixture stub inventory unavailable or malformed" \
      "runner error" "fixture stub inventory unavailable"
    return 1
  fi
  IFS=$'\t' read -r PROJECT_EVIDENCE_STUB_INVENTORY_SCHEMA \
    PROJECT_EVIDENCE_STUBBED_MODULES PROJECT_EVIDENCE_STUBBED_ANY_MEMBERS \
    PROJECT_EVIDENCE_STUB_INVENTORY_FINGERPRINT <<< "$stub_evidence"
  if [[ "$PROJECT_EVIDENCE_STUB_INVENTORY_SCHEMA" != "1" \
    || ! "$PROJECT_EVIDENCE_STUBBED_MODULES" =~ ^(0|[1-9][0-9]*)$ \
    || ! "$PROJECT_EVIDENCE_STUBBED_ANY_MEMBERS" =~ ^(0|[1-9][0-9]*)$ \
    || ! "$PROJECT_EVIDENCE_STUB_INVENTORY_FINGERPRINT" =~ ^[0-9a-f]{64}$ ]]; then
    PROJECT_EVIDENCE_FILES_REACHED_REASON="fixture stub inventory unavailable"
    project_evidence_fail "fixture stub inventory malformed" \
      "runner error" "fixture stub inventory unavailable"
    return 1
  fi
  if [[ "$PROJECT_EVIDENCE_STUBBED_MODULES" -gt 0 \
    || "$PROJECT_EVIDENCE_STUBBED_ANY_MEMBERS" -gt 0 ]]; then
    PROJECT_EVIDENCE_FILES_REACHED_REASON="fixture dependency stubs present"
    project_evidence_fail \
      "fixture dependency stubs erase semantic coverage (modules=${PROJECT_EVIDENCE_STUBBED_MODULES}, any-members=${PROJECT_EVIDENCE_STUBBED_ANY_MEMBERS})" \
      "fixture invalid" "fixture dependency stubs present"
    return 1
  fi

  local timeout="${PROJECT_EVIDENCE_TIMEOUT:-$((BENCH_TIMEOUT * 2))}"
  local config_dir project_root tsc_lib_dir work_dir show_file list_file stats_file
  config_dir="$(dirname "$tsconfig")"
  # Dynamic scope lets the canonical fingerprint helper identify the fixture
  # row root even for nested application tsconfigs.
  local FIXTURE_ROOT="${EXTERNAL_BENCH_DIR:-${FIXTURE_ROOT:-$config_dir}}"
  project_root="$(tsz_fingerprint_project_root "$config_dir")"
  tsc_lib_dir="$(project_evidence_tsc_lib_dir)"
  if [[ -z "$tsc_lib_dir" ]]; then
    project_evidence_fail "pinned TypeScript 7 built-in library identity unavailable"
    return 1
  fi

  work_dir="${TEMP_DIR:-${TMPDIR:-/tmp}}"
  show_file="$(mktemp "$work_dir/${name}.show-config.XXXXXX")" || return 1
  list_file="$(mktemp "$work_dir/${name}.list-files.XXXXXX")" || {
    rm -f "$show_file"
    return 1
  }
  stats_file="$(mktemp "$work_dir/${name}.compiler-stats.XXXXXX")" || {
    rm -f "$show_file" "$list_file"
    return 1
  }
  rm -f "$stats_file"
  : > "$tsz_log"
  : > "$tsc_log"

  local show_rc=0 list_rc=0 tsc_rc=0 tsz_rc=0 parsed=""
  run_with_timeout "$timeout" "${PROJECT_EVIDENCE_TSC_CMD[@]}" \
    --singleThreaded --stableTypeOrdering true --showConfig -p "$tsconfig" \
    > "$show_file" 2>/dev/null || show_rc=$?
  if [[ "$show_rc" -ne 0 ]] || ! parsed="$(node "$PROJECT_EVIDENCE_STATS_READER" \
      show-config-roots "$show_file" "$config_dir" "$project_root" 2>/dev/null)"; then
    rm -f "$show_file" "$list_file" "$stats_file"
    project_evidence_fail "TypeScript 7 showConfig graph unavailable or malformed"
    return 1
  fi
  IFS=$'\t' read -r PROJECT_EVIDENCE_TSC_ROOT_FILES \
    PROJECT_EVIDENCE_TSC_ROOT_FINGERPRINT <<< "$parsed"

  run_with_timeout "$timeout" "${PROJECT_EVIDENCE_TSC_CMD[@]}" \
    --singleThreaded --stableTypeOrdering true --listFilesOnly -p "$tsconfig" \
    > "$list_file" 2>/dev/null || list_rc=$?
  if [[ "$list_rc" -ne 0 ]] || ! parsed="$(node "$PROJECT_EVIDENCE_STATS_READER" \
      list-files-graph "$list_file" "$tsc_lib_dir" "$project_root" 2>/dev/null)"; then
    rm -f "$show_file" "$list_file" "$stats_file"
    project_evidence_fail "TypeScript 7 source graph unavailable or malformed"
    return 1
  fi
  IFS=$'\t' read -r PROJECT_EVIDENCE_TSC_SOURCE_FILES \
    PROJECT_EVIDENCE_TSC_SOURCE_FINGERPRINT <<< "$parsed"

  run_with_timeout "$timeout" "${PROJECT_EVIDENCE_TSC_CMD[@]}" \
    --singleThreaded --stableTypeOrdering true --noEmit -p "$tsconfig" \
    > "$tsc_log" 2>&1 || tsc_rc=$?
  PROJECT_EVIDENCE_TSC_RC="$tsc_rc"

  run_with_timeout "$timeout" "${PROJECT_EVIDENCE_TSZ_CMD[@]}" \
    --perf-counters-json "$stats_file" --noEmit -p "$tsconfig" \
    > "$tsz_log" 2>&1 || tsz_rc=$?
  PROJECT_EVIDENCE_TSZ_RC="$tsz_rc"

  if ! project_evidence_read_tsz_stats "$stats_file" "$config_dir" "$project_root"; then
    local stats_reason="$PROJECT_EVIDENCE_FILES_REACHED_REASON"
    rm -f "$show_file" "$list_file" "$stats_file"
    project_evidence_fail "$stats_reason" "runner error" "$stats_reason"
    return 1
  fi
  if [[ "$PROJECT_EVIDENCE_SEMANTIC_COMPLETION" != "complete" ]]; then
    rm -f "$show_file" "$list_file" "$stats_file"
    # Valid telemetry is retained below, but schema 2 denotes exact admission
    # proof and therefore remains unset for every incomplete completion.
    local incomplete_class="exit success"
    if [[ "$tsz_rc" -ne 0 ]]; then
      incomplete_class="$(project_failure_class "$([[ "$tsz_rc" -eq 124 ]] && echo timeout || echo "nonzero exit")" "$tsz_rc")"
    fi
    project_evidence_fail "semantic completion ${PROJECT_EVIDENCE_SEMANTIC_COMPLETION}" \
      "$incomplete_class" "semantic completion ${PROJECT_EVIDENCE_SEMANTIC_COMPLETION}"
    return 1
  fi
  rm -f "$show_file" "$list_file" "$stats_file"

  if [[ "$PROJECT_EVIDENCE_TSZ_ROOT_FILES" -eq 0 \
    || "$PROJECT_EVIDENCE_TSZ_SOURCE_FILES" -eq 0 ]]; then
    PROJECT_EVIDENCE_FILES_REACHED_REASON="zero source files processed"
    project_evidence_fail "TSZ admitted a zero-file project graph" \
      "fixture invalid" "zero source files processed"
    return 1
  fi
  if [[ "$PROJECT_EVIDENCE_TSC_ROOT_FILES" -eq 0 \
    || "$PROJECT_EVIDENCE_TSC_SOURCE_FILES" -eq 0 ]]; then
    project_evidence_fail "TypeScript 7 admitted a zero-file project graph"
    return 1
  fi

  if [[ "$PROJECT_EVIDENCE_TSZ_ROOT_FILES" -ne "$PROJECT_EVIDENCE_TSC_ROOT_FILES" \
    || "$PROJECT_EVIDENCE_TSZ_ROOT_FINGERPRINT" != "$PROJECT_EVIDENCE_TSC_ROOT_FINGERPRINT" ]]; then
    project_evidence_fail \
      "root graph mismatch (tsz=${PROJECT_EVIDENCE_TSZ_ROOT_FILES}/${PROJECT_EVIDENCE_TSZ_ROOT_FINGERPRINT:0:12}, TypeScript7=${PROJECT_EVIDENCE_TSC_ROOT_FILES}/${PROJECT_EVIDENCE_TSC_ROOT_FINGERPRINT:0:12})" \
      "exit success" "project root-file diagnostic mismatch"
    return 1
  fi
  if [[ "$PROJECT_EVIDENCE_TSZ_SOURCE_FILES" -ne "$PROJECT_EVIDENCE_TSC_SOURCE_FILES" \
    || "$PROJECT_EVIDENCE_TSZ_SOURCE_FINGERPRINT" != "$PROJECT_EVIDENCE_TSC_SOURCE_FINGERPRINT" ]]; then
    project_evidence_fail \
      "source graph mismatch (tsz=${PROJECT_EVIDENCE_TSZ_SOURCE_FILES}/${PROJECT_EVIDENCE_TSZ_SOURCE_FINGERPRINT:0:12}, TypeScript7=${PROJECT_EVIDENCE_TSC_SOURCE_FILES}/${PROJECT_EVIDENCE_TSC_SOURCE_FINGERPRINT:0:12})" \
      "exit success" "project source-file diagnostic mismatch"
    return 1
  fi

  if ! project_evidence_ordinary_rc "$tsz_rc" || ! project_evidence_ordinary_rc "$tsc_rc"; then
    local failure_class
    failure_class="$(project_failure_class "nonzero exit" "$tsz_rc" "$tsc_rc")"
    project_evidence_fail "compiler did not complete with an ordinary exit (tsz=${tsz_rc}, TypeScript7=${tsc_rc})" \
      "$failure_class" "$(project_failure_status "$failure_class")"
    return 1
  fi
  if ! tsz_diagnostic_log_is_covered "$tsz_log" "$project_root" \
    || ! tsz_diagnostic_log_is_covered "$tsc_log" "$project_root"; then
    project_evidence_fail "unparsed compiler diagnostic output"
    return 1
  fi

  local tsz_diag_stats tsc_diag_stats
  tsz_diag_stats="$(tsz_diagnostic_record_stats "$tsz_log" "$project_root")" || {
    project_evidence_fail "TSZ diagnostic evidence unavailable"
    return 1
  }
  tsc_diag_stats="$(tsz_diagnostic_record_stats "$tsc_log" "$project_root")" || {
    project_evidence_fail "TypeScript 7 diagnostic evidence unavailable"
    return 1
  }
  IFS=$'\t' read -r PROJECT_EVIDENCE_TSZ_DIAGNOSTIC_RECORDS \
    PROJECT_EVIDENCE_TSZ_DIAGNOSTIC_FINGERPRINT <<< "$tsz_diag_stats"
  IFS=$'\t' read -r PROJECT_EVIDENCE_TSC_DIAGNOSTIC_RECORDS \
    PROJECT_EVIDENCE_TSC_DIAGNOSTIC_FINGERPRINT <<< "$tsc_diag_stats"

  if [[ "$tsz_rc" -ne 0 && "$PROJECT_EVIDENCE_TSZ_DIAGNOSTIC_RECORDS" -eq 0 ]] \
    || [[ "$tsc_rc" -ne 0 && "$PROJECT_EVIDENCE_TSC_DIAGNOSTIC_RECORDS" -eq 0 ]]; then
    project_evidence_fail "nonzero compiler exit without parsed diagnostics"
    return 1
  fi
  if [[ "$tsz_rc" -ne "$tsc_rc" ]]; then
    local exit_delta
    exit_delta="$(tsz_only_and_tsc_context_delta "$tsz_log" "$tsc_log" "$project_root")"
    project_evidence_fail "compiler exit mismatch (tsz=${tsz_rc}, TypeScript7=${tsc_rc})" \
      "nonzero exit" "exact diagnostic mismatch or compiler-exit mismatch"
    [[ -n "$exit_delta" ]] \
      && PROJECT_EVIDENCE_DIAGNOSTIC_DELTA="${PROJECT_EVIDENCE_DIAGNOSTIC_DELTA}"$'\n'"${exit_delta}"
    return 1
  fi
  if ! tsz_diagnostic_multisets_agree "$tsz_log" "$tsc_log" "$project_root"; then
    local mismatch_delta
    mismatch_delta="$(tsz_only_and_tsc_context_delta "$tsz_log" "$tsc_log" "$project_root")"
    project_evidence_fail "exact diagnostic record mismatch" \
      "$([[ "$tsz_rc" -eq 0 ]] && echo "exit success" || echo "nonzero exit")" \
      "exact diagnostic mismatch or compiler-exit mismatch"
    [[ -n "$mismatch_delta" ]] \
      && PROJECT_EVIDENCE_DIAGNOSTIC_DELTA="${PROJECT_EVIDENCE_DIAGNOSTIC_DELTA}"$'\n'"${mismatch_delta}"
    return 1
  fi

  PROJECT_EVIDENCE_SCHEMA=2
  PROJECT_EVIDENCE_EXIT_CLASS="exit success"
  PROJECT_EVIDENCE_DIAGNOSTIC_STATUS="none"
  PROJECT_EVIDENCE_DIAGNOSTIC_DELTA="$(tsc_and_tsz_oracle_delta "$tsz_log" "$tsc_log")"
  PROJECT_EVIDENCE_REASON="exact TypeScript 7 project evidence"
  PROJECT_EVIDENCE_FILES_REACHED_REASON=""
  return 0
}

project_evidence_reset
