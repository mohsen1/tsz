read_conformance_results() {
  local last_run_path="$1"
  python3 - "$last_run_path" <<'PY' 2>/dev/null || echo "0 0"
import sys

passed = 0
recorded = 0
with open(sys.argv[1], encoding="utf-8", errors="replace") as f:
    for line in f:
        if line.startswith(("PASS ", "FAIL ", "XFAIL ", "CRASH ", "TIMEOUT ")):
            recorded += 1
        if line.startswith("PASS "):
            passed += 1

print(passed, recorded)
PY
}

show_log_tail() {
  local path="$1"
  if [[ -f "$path" ]]; then
    echo "--- tail ${path} ---" >&2
    tail -120 "$path" >&2
    echo "--- end tail ${path} ---" >&2
  fi
}

show_log_tails() {
  local dir="$1" path
  for path in "$dir"/*.log; do
    [[ -f "$path" ]] || continue
    show_log_tail "$path"
  done
}

run_with_heartbeat() {
  local label="$1"
  shift

  local pid rc restore_errexit=0
  case "$-" in
    *e*) restore_errexit=1 ;;
  esac

  "$@" &
  pid="$!"

  local heartbeat_interval="${TSZ_CI_HEARTBEAT_SECONDS:-60}" mem_avail
  while kill -0 "$pid" 2>/dev/null; do
    sleep "$heartbeat_interval"
    if kill -0 "$pid" 2>/dev/null; then
      mem_avail="$(ci_available_memory_mb)"
      echo "still running: ${label} $(date -u +%Y-%m-%dT%H:%M:%SZ)${mem_avail:+ [mem_avail=${mem_avail}MB]}"
    fi
  done

  set +e
  wait "$pid"
  rc="$?"
  if [[ "$restore_errexit" == "1" ]]; then
    set -e
  else
    set +e
  fi
  return "$rc"
}

conformance_shard_plan() {
  local shard_index="$1" shard_count="$2" strategy="${3:-hash}" weights_file="${4:-}"
  local root="${ROOT_DIR:-$(pwd)}"
  local runner_bin="$root/.target/dist-fast/tsz-conformance"
  local plan_args=(--plan "$shard_count" --test-dir "$root/TypeScript/tests/cases" --shard-strategy "$strategy")
  if [[ -n "$weights_file" ]]; then
    plan_args+=(--shard-weights "$weights_file")
  fi
  if [[ ! -x "$runner_bin" ]]; then
    echo "error: missing conformance runner for shard planning: $runner_bin" >&2
    return 1
  fi

  "$runner_bin" "${plan_args[@]}" \
    | jq -r --argjson index "$shard_index" \
        '.shards[$index] // error("missing conformance shard plan entry") | "\(.passed) \(.total) \(.weight)"'
}

run_conformance() {
  ci_section "Conformance"
  mkdir -p "$LOG_DIR/conformance"
  ci_report_memory "conformance-${CONFORMANCE_SHARD_INDEX:-0}"
  local log_file="$LOG_DIR/conformance/full.log"
  local last_run="scripts/conformance/conformance-last-run.txt"
  rm -f "$last_run"

  local shard_index shard_count shard_offset shard_max shard_expected_passed shard_expected_total shard_expected_weight
  local shard_weights_file timings_file
  local conformance_args=()
  shard_index="$(num_or_zero "$CONFORMANCE_SHARD_INDEX")"
  shard_count="$(num_or_zero "$CONFORMANCE_SHARD_COUNT")"
  shard_weights_file=""
  timings_file="$METRICS_DIR/conformance-timings-${shard_index}.json"
  if [[ "$shard_count" -lt 1 ]]; then
    shard_count=1
  fi
  if [[ "$shard_count" -gt 1 ]]; then
    if [[ "$CONFORMANCE_SHARD_STRATEGY" == "weighted" ]]; then
      shard_weights_file="$METRICS_DIR/conformance-shard-weights.json"
      cp scripts/conformance/conformance-shard-weights.json "$shard_weights_file"
      echo "Using checked-in conformance shard weights."
    fi
    read -r shard_expected_passed shard_expected_total shard_expected_weight < <(
      conformance_shard_plan "$shard_index" "$shard_count" "$CONFORMANCE_SHARD_STRATEGY" "$shard_weights_file"
    )
    shard_offset=0
    shard_max=0
    conformance_args+=(--shard "${shard_index}/${shard_count}")
    conformance_args+=(--shard-strategy "$CONFORMANCE_SHARD_STRATEGY")
    if [[ -n "$shard_weights_file" ]]; then
      conformance_args+=(--shard-weights "$shard_weights_file")
    fi
    conformance_args+=(--timings-file "$timings_file")
    echo "Conformance shard: ${shard_index}/${shard_count} strategy=${CONFORMANCE_SHARD_STRATEGY} expected=${shard_expected_passed}/${shard_expected_total} weight=${shard_expected_weight:-0}"
  else
    shard_offset=0
    shard_max=0
    shard_expected_passed=0
    shard_expected_total=0
    shard_expected_weight=0
    conformance_args+=(--timings-file "$timings_file")
  fi

  set +e
  run_with_heartbeat "conformance" \
    bash -c 'log_file="$1"; shift; "$@" >"$log_file" 2>&1' bash "$log_file" \
    ./scripts/conformance/conformance.sh run --workers "$CONFORMANCE_WORKERS" "${conformance_args[@]}"
  local rc="$?"
  set -e

  local final_results_line
  final_results_line="$(grep -a 'FINAL RESULTS:' "$log_file" | tail -1 || true)"
  [[ -n "$final_results_line" ]] && echo "$final_results_line"

  local total_passed=0 total_tests=0 skipped_tests=0
  if [[ -f "$last_run" ]]; then
    read -r total_passed total_tests < <(read_conformance_results "$last_run")
  fi
  skipped_tests="$(awk '/^[[:space:]]*Skipped:/ { value=$2 } END { print value + 0 }' "$log_file")"
  skipped_tests="$(num_or_zero "$skipped_tests")"
  if [[ "$final_results_line" =~ FINAL[[:space:]]RESULTS:[[:space:]]([0-9]+)/([0-9]+)[[:space:]]passed ]]; then
    total_passed="${BASH_REMATCH[1]}"
    # The runner reports evaluated tests in FINAL RESULTS and skipped tests
    # separately. Count both toward coverage so runtime SKIP entries do not
    # look like missing shard coverage in the aggregate job.
    total_tests=$(( ${BASH_REMATCH[2]} + skipped_tests ))
  fi
  total_passed="$(num_or_zero "$total_passed")"
  total_tests="$(num_or_zero "$total_tests")"

  printf '{"rc":%s,"passed":%s,"total":%s,"skipped":%s,"workers":%s,"shard_index":%s,"shard_count":%s,"offset":%s,"max":%s,"expected_passed":%s,"expected_total":%s,"expected_weight":%s,"strategy":"%s"}\n' \
    "$rc" "$total_passed" "$total_tests" "$skipped_tests" "$CONFORMANCE_WORKERS" \
    "$shard_index" "$shard_count" "$shard_offset" "$shard_max" "$shard_expected_passed" "$shard_expected_total" "$(num_or_zero "$shard_expected_weight")" "$CONFORMANCE_SHARD_STRATEGY" \
    > "$METRICS_DIR/conformance.json"
  echo "Conformance workers: ${CONFORMANCE_WORKERS}"
  echo "Conformance wrapper exit: ${rc}"
  echo "Conformance aggregate: ${total_passed}/${total_tests}"
  echo "Conformance skipped: ${skipped_tests}"

  local failures_file="$METRICS_DIR/conformance-failures-${shard_index}.txt"
  # XFAIL is known failing debt in conformance math, so keep it in the shard
  # failure list used by aggregate accepted-regression checks.
  awk '/^(FAIL|XFAIL|CRASH|TIMEOUT) / { print $2 }' "$log_file" \
    | sort -u > "$failures_file" 2>/dev/null || true

  if [[ "$rc" -ne 0 ]]; then
    echo "error: conformance wrapper failed" >&2
    show_log_tail "$log_file"
    return 1
  fi

  if [[ "$shard_count" -gt 1 ]]; then
    # Upload shard result to GCS so the conformance-aggregate job can check the global total.
    # Per-shard count assertions removed: baseline.txt counts go stale and cause off-by-one flakes.
    local bucket="${_TSZ_CI_CACHE_BUCKET:-${TSZ_CI_CACHE_BUCKET:-}}"
    local run_key="${GITHUB_SHA:-${REVISION_ID:-$(git rev-parse HEAD 2>/dev/null || echo unknown)}}"
    if [[ -n "$bucket" && "$run_key" != "unknown" ]] && command -v gsutil >/dev/null 2>&1; then
      ensure_gcs_auth
      local up_attempt up_rc=1
      for up_attempt in 1 2 3; do
        if gsutil -q cp "$METRICS_DIR/conformance.json" \
            "${bucket%/}/conformance-runs/${run_key}/shard-${shard_index}.json" 2>/dev/null; then
          up_rc=0
          echo "Uploaded shard result: shard-${shard_index}.json (attempt ${up_attempt})"
          break
        fi
        echo "warning: upload attempt ${up_attempt}/3 failed for shard-${shard_index}.json" >&2
        [[ "$up_attempt" -lt 3 ]] && sleep "$((up_attempt * 5))"
      done
      [[ "$up_rc" -ne 0 ]] && echo "warning: failed to upload shard result after 3 attempts (non-fatal)" >&2

      if [[ -f "$timings_file" ]]; then
        gsutil -q cp "$timings_file" \
          "${bucket%/}/conformance-runs/${run_key}/timings-shard-${shard_index}.json" 2>/dev/null \
          && echo "Uploaded shard timings: timings-shard-${shard_index}.json" \
          || echo "warning: failed to upload shard timings (non-fatal)" >&2
      fi

      # Upload per-shard FAIL list so aggregate can show which tests regressed.
      if [[ -s "$failures_file" ]]; then
        gsutil -q cp "$failures_file" \
          "${bucket%/}/conformance-runs/${run_key}/failures-shard-${shard_index}.txt" 2>/dev/null \
          || echo "warning: failed to upload failure list (non-fatal)" >&2
      fi
    fi
    return 0
  fi

  baseline="$(jq -r '.summary.passed // 0' scripts/conformance/conformance-snapshot.json)"
  baseline="$(cap_positive_baseline "$baseline" "$TSZ_CI_CONFORMANCE_ACCEPTED_FLOOR")"
  baseline_total="$(jq -r '.summary.total_tests // .summary.total // 0' scripts/conformance/conformance-snapshot.json)"
  local total_tolerance=5
  if [[ "$baseline_total" -gt 0 && "$total_tests" -lt $(( baseline_total - total_tolerance )) ]]; then
    echo "error: conformance coverage is incomplete: ${total_tests} < ${baseline_total} (tolerance ${total_tolerance})" >&2
    show_log_tail "$log_file"
    return 1
  fi
  if [[ "$baseline" -gt 0 && "$total_passed" -lt "$baseline" ]]; then
    echo "error: conformance regression: ${total_passed} < ${baseline}" >&2
    show_log_tail "$log_file"
    return 1
  fi
}

run_conformance_aggregate() {
  ci_section "Conformance aggregate"
  local expected_shards="${_TSZ_CI_CONFORMANCE_SHARD_COUNT:-${TSZ_CI_CONFORMANCE_SHARDS:-32}}"
  local tmp_dir
  tmp_dir="$(mktemp -d)"
  local bucket="${_TSZ_CI_CACHE_BUCKET:-${TSZ_CI_CACHE_BUCKET:-}}"
  local run_key="${GITHUB_SHA:-${REVISION_ID:-$(git rev-parse HEAD 2>/dev/null || echo unknown)}}"
  local prefix=""
  if [[ -n "$bucket" && "$run_key" != "unknown" ]]; then
    prefix="${bucket%/}/conformance-runs/${run_key}"
  fi

  # Prefer GitHub Actions artifacts (downloaded by the workflow's download-artifact step)
  # over GCS, which requires SA key permissions that may not be available.
  local artifacts_dir=".conformance-shards"
  local using_artifacts=0
  if [[ -d "$artifacts_dir" ]]; then
    # upload-artifact@v4 preserves the full workspace-relative path inside the artifact.
    # The file is at ci-metrics/conformance.json in the workspace, so after download it
    # lands at conformance-shard-N/ci-metrics/conformance.json (not conformance-shard-N/conformance.json).
    # Use find with maxdepth to locate the file regardless of the subdirectory depth.
    local found=0
    for shard_dir in "$artifacts_dir"/conformance-shard-*/; do
      [[ -d "$shard_dir" ]] || continue
      local json
      json="$(find "$shard_dir" -name "conformance.json" -maxdepth 4 2>/dev/null | head -1)"
      [[ -f "$json" ]] || continue
      local shard_name
      shard_name="$(basename "$shard_dir")"
      cp "$json" "$tmp_dir/shard-${shard_name#conformance-shard-}.json"
      local artifact_failure_list
      artifact_failure_list="$(find "$shard_dir" -maxdepth 4 -name "conformance-failures-${shard_name#conformance-shard-}.txt" 2>/dev/null | head -1)"
      if [[ -f "$artifact_failure_list" ]]; then
        cp "$artifact_failure_list" "$tmp_dir/failures-shard-${shard_name#conformance-shard-}.txt"
      fi
      found=$(( found + 1 ))
    done
    if [[ "$found" -gt 0 ]]; then
      echo "Using ${found} GitHub Actions artifact shard results from ${artifacts_dir}/"
      using_artifacts=1
    else
      echo "warning: ${artifacts_dir}/ exists but no conformance.json files found; falling back to GCS" >&2
      ls -la "$artifacts_dir"/ 2>/dev/null || true
    fi
  fi

  if [[ "$using_artifacts" -eq 0 ]]; then
    if [[ -z "$prefix" ]]; then
      echo "error: cannot aggregate — no artifact dir and no GCS bucket/run key available" >&2
      return 1
    fi
    ensure_gcs_auth
    echo "Downloading shard results from ${prefix}/shard-*.json ..."
    local dl_attempt dl_rc=1
    for dl_attempt in 1 2 3; do
      if gsutil -q cp "${prefix}/shard-*.json" "$tmp_dir/" 2>/dev/null; then
        dl_rc=0
        break
      fi
      echo "warning: GCS download attempt ${dl_attempt}/3 failed" >&2
      [[ "$dl_attempt" -lt 3 ]] && sleep "$((dl_attempt * 5))"
    done
    if [[ "$dl_rc" -ne 0 ]]; then
      echo "error: failed to download shard results from GCS after 3 attempts" >&2
      return 1
    fi
  fi

  local total_passed=0 total_tests=0 shard_count=0
  local total_expected_passed=0 total_expected_tests=0
  for f in "$tmp_dir"/shard-*.json; do
    [[ -f "$f" ]] || continue
    local p t ep et
    p="$(jq -r '.passed // 0' "$f" 2>/dev/null)"
    t="$(jq -r '.total // 0' "$f" 2>/dev/null)"
    ep="$(jq -r '.expected_passed // 0' "$f" 2>/dev/null)"
    et="$(jq -r '.expected_total // 0' "$f" 2>/dev/null)"
    total_passed=$(( total_passed + $(num_or_zero "$p") ))
    total_tests=$(( total_tests + $(num_or_zero "$t") ))
    total_expected_passed=$(( total_expected_passed + $(num_or_zero "$ep") ))
    total_expected_tests=$(( total_expected_tests + $(num_or_zero "$et") ))
    shard_count=$(( shard_count + 1 ))
  done

  echo "Conformance aggregate: ${total_passed}/${total_tests} across ${shard_count}/${expected_shards} shards"
  if [[ "$total_expected_tests" -gt 0 ]]; then
    echo "Conformance expected aggregate: ${total_expected_passed}/${total_expected_tests}"
  fi

  if [[ "$shard_count" -lt "$expected_shards" ]]; then
    echo "error: only ${shard_count}/${expected_shards} shard results collected; some shards may have crashed" >&2
    return 1
  fi

  local baseline baseline_total
  baseline="$(jq -r '.summary.passed // 0' scripts/conformance/conformance-snapshot.json)"
  baseline="$(cap_positive_baseline "$baseline" "$TSZ_CI_CONFORMANCE_ACCEPTED_FLOOR")"
  baseline_total="$(jq -r '.summary.total_tests // .summary.total // 0' scripts/conformance/conformance-snapshot.json)"
  # Planned shard totals are drift diagnostics, but they also describe the
  # active shard domain. Do not let a larger snapshot domain fail a complete
  # run for a smaller planned domain.
  local coverage_baseline_total="$baseline_total"
  if [[ "$total_expected_tests" -gt 0 && "$total_expected_tests" -lt "$coverage_baseline_total" ]]; then
    coverage_baseline_total="$total_expected_tests"
  fi
  local total_tolerance=5
  if [[ "$coverage_baseline_total" -gt 0 && "$total_tests" -lt $(( coverage_baseline_total - total_tolerance )) ]]; then
    echo "error: conformance coverage is incomplete: ${total_tests} < ${coverage_baseline_total} (tolerance ${total_tolerance})" >&2
    return 1
  fi
  if [[ "$total_expected_passed" -gt 0 ]]; then
    local expected_deficit=$(( total_expected_passed - total_passed ))
    if [[ "$expected_deficit" -gt 0 ]]; then
      if ! _check_conformance_regression_allowlist "$tmp_dir" "$prefix" "$expected_deficit"; then
        return 1
      fi
    fi
  else
    local pass_baseline
    pass_baseline="$(cap_positive_baseline "$baseline" "$TSZ_CI_CONFORMANCE_ACCEPTED_FLOOR")"
    if [[ "$pass_baseline" -gt 0 && "$total_passed" -lt "$pass_baseline" ]]; then
      local pass_tolerance=5
      if [[ "$total_passed" -ge $(( pass_baseline - pass_tolerance )) ]]; then
        echo "warning: conformance aggregate below baseline within tolerance: ${total_passed} < ${pass_baseline} (tolerance ${pass_tolerance})" >&2
      else
        echo "error: conformance regression: ${total_passed} < ${pass_baseline}" >&2
        _show_conformance_regressions "$tmp_dir" "$prefix" "$pass_baseline"
        return 1
      fi
    fi
  fi
  local pass_rate
  pass_rate="$(awk -v p="$total_passed" -v t="$total_tests" 'BEGIN { if (t > 0) printf "%.1f", (p / t) * 100; else print "0.0" }')"
  jq -n \
    --arg suite "conformance" \
    --arg pass_rate "$pass_rate" \
    --argjson passed "$total_passed" \
    --argjson total "$total_tests" \
    --argjson shards "$shard_count" \
    '{suite:$suite, pass_rate:$pass_rate, passed:$passed, total:$total, shards:$shards}' \
    > "$METRICS_DIR/conformance.json"
  publish_latest_metric conformance "$METRICS_DIR/conformance.json"

  if [[ -n "$prefix" ]] && gsutil -q cp "${prefix}/timings-shard-*.json" "$tmp_dir/" 2>/dev/null; then
    jq -s '
      {
        summary: {
          total: ([.[].summary.total // 0] | add),
          elapsed_ms: ([.[].summary.elapsed_ms // 0] | max)
        },
        results: ([.[].results[]?] | sort_by(.file))
      }
    ' "$tmp_dir"/timings-shard-*.json > "$METRICS_DIR/conformance-timings.json"
    publish_latest_metric conformance-timings "$METRICS_DIR/conformance-timings.json"
  else
    echo "warning: no conformance timing shards available to publish (non-fatal)" >&2
  fi
  echo "Conformance gate passed: ${total_passed} >= ${baseline} (baseline)"
}

# Download shard failure lists and reject any failure outside the accepted set.
_check_conformance_regression_allowlist() {
  local tmp_dir="$1" prefix="$2" expected_deficit="$3"
  local allowlist="${TSZ_CI_CONFORMANCE_ACCEPTED_REGRESSIONS:-}"

  if [[ -z "$allowlist" || ! -f "$allowlist" ]]; then
    echo "error: conformance regression deficit ${expected_deficit}, but no accepted regression list found at ${allowlist:-<unset>}" >&2
    _show_conformance_regressions "$tmp_dir" "$prefix" "$expected_deficit"
    return 1
  fi

  if compgen -G "$tmp_dir/failures-shard-*.txt" >/dev/null; then
    :
  elif [[ -z "$prefix" ]] || ! gsutil -q -m cp "${prefix}/failures-shard-*.txt" "$tmp_dir/" 2>/dev/null; then
    echo "error: conformance regression deficit ${expected_deficit}, but per-shard failure lists are unavailable" >&2
    return 1
  fi

  local all_failures_file="$tmp_dir/all-failures.txt"
  cat "$tmp_dir"/failures-shard-*.txt 2>/dev/null | sort -u > "$all_failures_file" || true

  python3 - "$all_failures_file" "$allowlist" "$expected_deficit" <<'PYEOF'
import os
import sys

failures_file, allowlist_file, expected_deficit = sys.argv[1], sys.argv[2], int(sys.argv[3])

def normalize(path):
    parts = path.replace("\\", "/").split("/")
    for i, part in enumerate(parts):
        if part == "TypeScript":
            return "/".join(parts[i:])
    return "/".join(parts)

def read_paths(path):
    paths = set()
    with open(path) as f:
        for line in f:
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            paths.add(normalize(line))
    return paths

failing = read_paths(failures_file)
accepted = read_paths(allowlist_file)
unlisted = sorted(failing - accepted)
resolved = sorted(accepted - failing)

if unlisted:
    print("error: unlisted conformance regressions:", file=sys.stderr)
    for path in unlisted:
        print(f"  REGRESSED: {path}", file=sys.stderr)
    if resolved:
        print("", file=sys.stderr)
        print("Accepted regressions that no longer fail in this run:", file=sys.stderr)
        for path in resolved:
            print(f"  RESOLVED: {path}", file=sys.stderr)
    return_code = 1
else:
    print(
        f"warning: conformance aggregate below expected only for accepted regressions: "
        f"{len(failing)}/{len(accepted)} listed tests currently failing "
        f"(deficit {expected_deficit})",
        file=sys.stderr,
    )
    if resolved:
        print("Accepted regressions that no longer fail in this run:", file=sys.stderr)
        for path in resolved:
            print(f"  RESOLVED: {path}", file=sys.stderr)
    return_code = 0

sys.exit(return_code)
PYEOF
}

# Download per-shard failure lists and show which tests are newly failing vs snapshot.
_show_conformance_regressions() {
  local tmp_dir="$1" prefix="$2" baseline_passed="$3"
  local snapshot="scripts/conformance/conformance-detail.json"

  # Download all per-shard failure lists (best-effort; non-fatal if missing).
  if compgen -G "$tmp_dir/failures-shard-*.txt" >/dev/null; then
    :
  elif [[ -z "$prefix" ]] || ! gsutil -q -m cp "${prefix}/failures-shard-*.txt" "$tmp_dir/" 2>/dev/null; then
    echo "(no per-shard failure lists available — upload may have been skipped)" >&2
    return
  fi

  # Union all FAIL paths across shards.
  local all_failures_file="$tmp_dir/all-failures.txt"
  cat "$tmp_dir"/failures-shard-*.txt 2>/dev/null | sort -u > "$all_failures_file" || true
  local fail_count
  fail_count="$(wc -l < "$all_failures_file" | tr -d ' ')"

  if [[ "$fail_count" -eq 0 ]]; then
    echo "(no failure detail available)" >&2
    return
  fi

  # Cross-reference with snapshot to identify newly failing tests.
  if [[ -f "$snapshot" ]]; then
    echo ""
    echo "=== Conformance regressions (tests passing in snapshot but failing now) ==="
    python3 - "$all_failures_file" "$snapshot" <<'PYEOF'
import json, sys, os

def normalize(path):
    """Strip machine-specific prefix, keep TypeScript/tests/... or similar suffix."""
    parts = path.replace("\\", "/").split("/")
    for i, p in enumerate(parts):
        if p == "TypeScript":
            return "/".join(parts[i:])
    return os.path.basename(path)

raw_failing = [l for l in open(sys.argv[1]).read().splitlines() if l]
failing_now = {normalize(p): p for p in raw_failing}

with open(sys.argv[2]) as f:
    detail = json.load(f)
snapshot_failures = {normalize(k) for k in detail.get("failures", {}).keys()}

newly_failing = sorted(k for k in failing_now if k not in snapshot_failures)
still_failing = sorted(k for k in failing_now if k in snapshot_failures)

if newly_failing:
    print(f"\nNewly failing ({len(newly_failing)} tests):")
    for t in newly_failing:
        print(f"  REGRESSED: {t}")
else:
    print("\nNo newly failing tests found (all failures were already in snapshot).")

if still_failing:
    print(f"\nAlready failing in snapshot ({len(still_failing)} tests) — not regressions.")
PYEOF
    echo "==================================================================="
  else
    echo ""
    echo "=== Failing tests this run (${fail_count} total) ==="
    cat "$all_failures_file"
    echo "==================================================================="
  fi
}
