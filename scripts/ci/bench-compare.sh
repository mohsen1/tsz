#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

BASE_REF="${BASE_REF:-${GITHUB_BASE_REF:-main}}"
BASE_REMOTE="origin/${BASE_REF}"
BASE_SHA="${BASE_SHA:-}"

RUN_ROOT="${RUNNER_TEMP:-/tmp}/tsz-perf-compare-${GITHUB_RUN_ID:-local}"
BASE_WORKTREE="${RUN_ROOT}/base"
OUT_DIR="${RUN_ROOT}/out"
TARGET_DIR="${TSZ_PERF_TARGET_DIR:-${RUN_ROOT}/target}"

SAMPLE_SIZE="${TSZ_PERF_SAMPLE_SIZE:-20}"
WARMUP_TIME="${TSZ_PERF_WARMUP_TIME:-1}"
MEASUREMENT_TIME="${TSZ_PERF_MEASUREMENT_TIME:-2}"
FAIL_THRESHOLD_PCT="${TSZ_PERF_FAIL_THRESHOLD_PCT:-5}"
FAIL_ON_REGRESSION="${TSZ_PERF_FAIL_ON_REGRESSION:-0}"

if [ -n "${TSZ_PERF_BENCHES:-}" ]; then
  IFS=',' read -r -a BENCHES <<< "${TSZ_PERF_BENCHES}"
else
  BENCHES=(
    "phase_timing_bench"
    "solver_bench"
    "real_world_bench"
  )
fi

BASELINE_NAME="pr-base"

mkdir -p "${RUN_ROOT}" "${OUT_DIR}"

cleanup() {
  git -C "${ROOT_DIR}" worktree remove --force "${BASE_WORKTREE}" >/dev/null 2>&1 || true
}
trap cleanup EXIT

if [ -n "${BASE_SHA}" ]; then
  echo "Fetching base commit: ${BASE_SHA}"
  git -C "${ROOT_DIR}" fetch --no-tags --depth=1 origin "${BASE_SHA}"
  BASE_SPEC="${BASE_SHA}"
else
  echo "Fetching base ref: ${BASE_REMOTE}"
  git -C "${ROOT_DIR}" fetch --no-tags --depth=1 origin "${BASE_REF}"
  BASE_SPEC="${BASE_REMOTE}"
fi

if [ -d "${BASE_WORKTREE}" ]; then
  git -C "${ROOT_DIR}" worktree remove --force "${BASE_WORKTREE}" >/dev/null 2>&1 || true
fi
git -C "${ROOT_DIR}" worktree add --detach "${BASE_WORKTREE}" "${BASE_SPEC}" >/dev/null
BASE_WORKTREE_SHA="$(git -C "${BASE_WORKTREE}" rev-parse --short HEAD)"

export CARGO_TARGET_DIR="${TARGET_DIR}"
export CARGO_INCREMENTAL=0
export CARGO_TERM_COLOR=always

BENCH_ARGS=(
  "--noplot"
  "--sample-size" "${SAMPLE_SIZE}"
  "--warm-up-time" "${WARMUP_TIME}"
  "--measurement-time" "${MEASUREMENT_TIME}"
)

for bench in "${BENCHES[@]}"; do
  base_log="${OUT_DIR}/base-${bench}.txt"
  head_log="${OUT_DIR}/head-${bench}.txt"

  echo "::group::base/${bench}"
  (
    cd "${BASE_WORKTREE}"
    cargo bench --bench "${bench}" -- "${BENCH_ARGS[@]}" --save-baseline "${BASELINE_NAME}" > "${base_log}" 2>&1
  )
  echo "::endgroup::"

  echo "::group::head/${bench}"
  (
    cd "${ROOT_DIR}"
    cargo bench --bench "${bench}" -- "${BENCH_ARGS[@]}" --baseline "${BASELINE_NAME}" > "${head_log}" 2>&1
  )
  echo "::endgroup::"
done

parse_changes() {
  local bench="$1"
  local file="$2"
  awk -v bench="${bench}" '
function emit_row() {
  if (case_name != "" && change_mid != "" && status != "") {
    printf "%s\t%s\t%s\t%s\n", bench, case_name, change_mid, status;
    case_name = "";
    change_mid = "";
    status = "";
  }
}
{
  line = $0;
  gsub(/\r/, "", line);
  gsub(/\x1B\[[0-9;]*[A-Za-z]/, "", line);

  if (line ~ /^[[:space:]]*change:[[:space:]]*$/) {
    in_change_block = 1;
    next;
  }

  if (line ~ /change:[[:space:]]+\[/) {
    change_line = line;
    sub(/^.*change:[[:space:]]+\[/, "", change_line);
    sub(/\].*$/, "", change_line);
    split(change_line, change_parts, /[[:space:]]+/);
    if (length(change_parts) >= 2) {
      change_mid = change_parts[2];
      gsub(/%/, "", change_mid);
    }
    in_change_block = 0;
    next;
  }

  if (in_change_block && line ~ /^[[:space:]]*time:[[:space:]]+\[/) {
    change_line = line;
    sub(/^.*time:[[:space:]]+\[/, "", change_line);
    sub(/\].*$/, "", change_line);
    split(change_line, change_parts, /[[:space:]]+/);
    if (length(change_parts) >= 2) {
      change_mid = change_parts[2];
      gsub(/%/, "", change_mid);
    }
    in_change_block = 0;
    next;
  }

  if (line ~ /time:[[:space:]]+\[/) {
    split(line, parts, /time:[[:space:]]+\[/);
    maybe_case = parts[1];
    gsub(/^[[:space:]]+|[[:space:]]+$/, "", maybe_case);
    if (maybe_case != "") {
      case_name = maybe_case;
    }
  } else if (line ~ /^[A-Za-z0-9_.:\/-]+$/) {
    if (line !~ /^Benchmarking/ &&
        line !~ /^Running / &&
        line !~ /^Gnuplot / &&
        line !~ /^Found / &&
        line !~ /^Finished / &&
        line !~ /^warning:/) {
      case_name = line;
    }
  }

  if (line ~ /Performance has regressed\./) {
    status = "regressed";
    emit_row();
    next;
  }
  if (line ~ /Performance has improved\./) {
    status = "improved";
    emit_row();
    next;
  }
  if (line ~ /No change in performance detected\./ || line ~ /Change within noise threshold\./) {
    status = "no_change";
    emit_row();
    next;
  }
}
END {
  emit_row();
}
' "${file}"
}

RESULTS_TSV="${OUT_DIR}/results.tsv"
printf "bench\tcase\tmid_change_pct\tstatus\n" > "${RESULTS_TSV}"
for bench in "${BENCHES[@]}"; do
  parse_changes "${bench}" "${OUT_DIR}/head-${bench}.txt" >> "${RESULTS_TSV}"
done

if [ "$(wc -l < "${RESULTS_TSV}")" -le 1 ]; then
  echo "No benchmark comparison rows were parsed from criterion output." >&2
  exit 1
fi

SUMMARY_MD="${OUT_DIR}/summary.md"
{
  echo "# Performance Bench Comparison"
  echo ""
  echo "- Base ref: \`${BASE_REMOTE}\`"
  echo "- Base SHA: \`${BASE_WORKTREE_SHA}\`"
  echo "- Head SHA: \`$(git -C "${ROOT_DIR}" rev-parse --short HEAD)\`"
  echo "- Criterion args: \`${BENCH_ARGS[*]}\`"
  echo "- Regression threshold: \`${FAIL_THRESHOLD_PCT}%\` (statistically significant regressions only)"
  echo "- Fail on regression: \`${FAIL_ON_REGRESSION}\`"
  echo ""
  echo "| Bench | Case | Mid change | Status |"
  echo "|---|---|---:|---|"
} > "${SUMMARY_MD}"

improved_count=0
regressed_count=0
no_change_count=0
regressed_over_threshold=0

while IFS=$'\t' read -r bench case mid status; do
  if [ "${bench}" = "bench" ]; then
    continue
  fi

  printf '| `%s` | `%s` | %+0.2f%% | %s |\n' "${bench}" "${case}" "${mid}" "${status}" >> "${SUMMARY_MD}"

  case "${status}" in
    improved)
      improved_count=$((improved_count + 1))
      ;;
    regressed)
      regressed_count=$((regressed_count + 1))
      if awk -v m="${mid}" -v t="${FAIL_THRESHOLD_PCT}" 'BEGIN { exit !(m > t) }'; then
        regressed_over_threshold=$((regressed_over_threshold + 1))
      fi
      ;;
    *)
      no_change_count=$((no_change_count + 1))
      ;;
  esac
done < "${RESULTS_TSV}"

avg_change="$(awk 'NR>1 { sum += $3; n += 1 } END { if (n == 0) { printf "0.00" } else { printf "%.2f", sum / n } }' "${RESULTS_TSV}")"

{
  echo ""
  echo "- Cases improved (significant): ${improved_count}"
  echo "- Cases regressed (significant): ${regressed_count}"
  echo "- Cases with no significant change: ${no_change_count}"
  echo "- Average mid-change across parsed cases: ${avg_change}%"
} >> "${SUMMARY_MD}"

if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
  cat "${SUMMARY_MD}" >> "${GITHUB_STEP_SUMMARY}"
fi

echo "Wrote comparison artifacts to: ${OUT_DIR}"

if [ "${regressed_over_threshold}" -gt 0 ]; then
  if [ "${FAIL_ON_REGRESSION}" = "1" ]; then
    echo "Detected ${regressed_over_threshold} statistically significant regressions above ${FAIL_THRESHOLD_PCT}%." >&2
    exit 1
  fi
  echo "Detected ${regressed_over_threshold} statistically significant regressions above ${FAIL_THRESHOLD_PCT}% (report-only mode)." >&2
fi
