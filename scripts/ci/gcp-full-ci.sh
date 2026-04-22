#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

export CARGO_TERM_COLOR="${CARGO_TERM_COLOR:-never}"
export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-1}"
export CARGO_HOME="${CARGO_HOME:-$ROOT_DIR/.ci-cache/cargo-home}"
export CARGO_PROFILE_DIST_FAST_LTO="${CARGO_PROFILE_DIST_FAST_LTO:-false}"
export RUST_MIN_STACK="${RUST_MIN_STACK:-8388608}"
export RUST_TEST_TIMEOUT="${RUST_TEST_TIMEOUT:-300}"
export NPM_CONFIG_CACHE="${NPM_CONFIG_CACHE:-$ROOT_DIR/.ci-cache/npm}"
export npm_config_cache="$NPM_CONFIG_CACHE"
export PATH="$CARGO_HOME/bin:$HOME/.cargo/bin:/usr/local/cargo/bin:$PATH"

mkdir -p "$CARGO_HOME" "$NPM_CONFIG_CACHE"

HOST_CPUS="$(getconf _NPROCESSORS_ONLN 2>/dev/null || nproc 2>/dev/null || echo 8)"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-$HOST_CPUS}"

cap_workers() {
  local requested="$1"
  if (( requested < HOST_CPUS )); then
    printf '%s\n' "$requested"
  else
    printf '%s\n' "$HOST_CPUS"
  fi
}

SHARD_COUNT="${TSZ_CI_SHARDS:-4}"

default_shard_workers() {
  local usable per
  usable=$((HOST_CPUS - 8))
  if (( usable < SHARD_COUNT )); then
    usable="$HOST_CPUS"
  fi
  per=$((usable / SHARD_COUNT))
  if (( per < 20 )); then
    per=20
  elif (( per > 64 )); then
    per=64
  fi
  cap_workers "$per"
}

default_emit_workers() {
  local workers
  workers="$(default_shard_workers)"
  if (( workers > 32 )); then
    workers=32
  fi
  cap_workers "$workers"
}

default_fourslash_workers() {
  local usable per
  usable=$((HOST_CPUS - 16))
  if (( usable < SHARD_COUNT )); then
    usable="$HOST_CPUS"
  fi
  per=$((usable / SHARD_COUNT))
  if (( per < 8 )); then
    per=8
  elif (( per > 16 )); then
    per=16
  fi
  cap_workers "$per"
}

default_conformance_workers() {
  local workers
  workers=$((HOST_CPUS - 8))
  if (( workers < 1 )); then
    workers="$HOST_CPUS"
  elif (( workers > 216 )); then
    workers=216
  fi
  cap_workers "$workers"
}

EMIT_WORKERS="${TSZ_CI_EMIT_WORKERS:-${TSZ_CI_SHARD_WORKERS:-$(default_emit_workers)}}"
FOURSLASH_WORKERS="${TSZ_CI_FOURSLASH_WORKERS:-${TSZ_CI_SHARD_WORKERS:-$(default_fourslash_workers)}}"
CONFORMANCE_WORKERS="${TSZ_CI_CONFORMANCE_WORKERS:-$(default_conformance_workers)}"
EMIT_CHUNK="${TSZ_CI_EMIT_CHUNK:-4000}"
METRICS_DIR="${TSZ_CI_METRICS_DIR:-.ci-metrics}"
LOG_DIR="${TSZ_CI_LOG_DIR:-.ci-logs}"
if [[ "$METRICS_DIR" != /* ]]; then
  METRICS_DIR="$ROOT_DIR/$METRICS_DIR"
fi
if [[ "$LOG_DIR" != /* ]]; then
  LOG_DIR="$ROOT_DIR/$LOG_DIR"
fi
SYNTHETIC_GIT_CHECKOUT=0

mkdir -p "$METRICS_DIR" "$LOG_DIR"

ci_section() {
  printf '\n==> %s\n' "$*"
}

timed() {
  local name="$1"
  shift
  local start end rc
  start="$(date +%s)"
  echo "CI_START ${name} $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  set +e
  "$@"
  rc="$?"
  set -e
  end="$(date +%s)"
  echo "CI_END ${name} rc=${rc} duration_seconds=$((end - start))"
  return "$rc"
}

num_or_zero() {
  local value="${1:-}"
  if [[ "$value" =~ ^[0-9]+$ ]]; then
    printf '%s\n' "$value"
  else
    printf '0\n'
  fi
}

suite_needs_group() {
  local suite="$1" group="$2"
  case "$suite" in
    all|full)
      return 0
      ;;
  esac

  case "$group" in
    lint)
      [[ "$suite" == "lint" ]]
      ;;
    unit)
      [[ "$suite" == "unit" ]]
      ;;
    wasm)
      [[ "$suite" == "wasm" ]]
      ;;
    node)
      [[ "$suite" == "conformance" || "$suite" == "emit" || "$suite" == "fourslash" ]]
      ;;
    *)
      return 1
      ;;
  esac
}

ensure_host_tools() {
  local suite="${1:-all}"
  ci_section "Install host tools"

  if command -v apt-get >/dev/null 2>&1; then
    export DEBIAN_FRONTEND=noninteractive
    local apt_packages=(
      build-essential
      ca-certificates
      curl
      git
      jq
      python3
      pkg-config
    )
    if suite_needs_group "$suite" wasm; then
      apt_packages+=(binaryen)
    fi
    if suite_needs_group "$suite" node; then
      apt_packages+=(nodejs npm)
    fi

    apt-get update -qq
    apt-get install -y --no-install-recommends "${apt_packages[@]}"
  fi

  if command -v rustup >/dev/null 2>&1; then
    if suite_needs_group "$suite" lint; then
      rustup component add clippy rustfmt
    fi
    if suite_needs_group "$suite" wasm; then
      rustup target add wasm32-unknown-unknown
    fi
  fi

  if suite_needs_group "$suite" unit && ! command -v cargo-nextest >/dev/null 2>&1; then
    curl -LsSf https://get.nexte.st/latest/linux | tar zxf - -C /usr/local/bin
  fi

  if suite_needs_group "$suite" wasm && ! command -v wasm-pack >/dev/null 2>&1; then
    curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh
  fi

  rustc -V
  cargo -V
  if command -v node >/dev/null 2>&1; then
    node -v
  fi
  if command -v npm >/dev/null 2>&1; then
    npm -v
  fi
  nproc
}

ensure_source_git_context() {
  ci_section "Ensure git metadata"

  if git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    return 0
  fi

  SYNTHETIC_GIT_CHECKOUT=1
  git init
  git config user.email "cloud-build@thirdface-ai-oauth.iam.gserviceaccount.com"
  git config user.name "Cloud Build"
  git remote add origin "${TSZ_CI_REPO_URL:-https://github.com/mohsen1/tsz.git}"
  git add -A
  git commit -q -m "cloud build source snapshot"
}

init_typescript_submodule() {
  ci_section "Init TypeScript submodule"
  local ref_file="$ROOT_DIR/scripts/ci/typescript-submodule-ref"
  local expected_ref
  expected_ref="$(tr -d '[:space:]' < "$ref_file")"

  if [[ -f TypeScript/.tsz-cache-ref ]]; then
    local cached_ref
    cached_ref="$(tr -d '[:space:]' < TypeScript/.tsz-cache-ref)"
    if [[ "$cached_ref" == "$expected_ref" && -f TypeScript/src/lib/es5.d.ts ]]; then
      echo "Using cached TypeScript source tree at ${cached_ref}"
      return 0
    fi
    echo "Discarding stale TypeScript cache: ${cached_ref} != ${expected_ref}" >&2
    rm -rf TypeScript
  fi

  if [[ "$SYNTHETIC_GIT_CHECKOUT" -eq 0 ]] && git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    local gitlink_ref
    gitlink_ref="$(git ls-tree HEAD TypeScript | awk '{print $3}')"
    if [[ -n "$gitlink_ref" && "$gitlink_ref" != "$expected_ref" ]]; then
      echo "error: scripts/ci/typescript-submodule-ref is stale: ${expected_ref} != ${gitlink_ref}" >&2
      return 1
    fi
    git submodule update --init --depth 1 -- TypeScript
  else
    rm -rf TypeScript
    git clone --filter=blob:none https://github.com/microsoft/TypeScript.git TypeScript
    git -C TypeScript fetch --depth 1 origin "$expected_ref"
    git -C TypeScript checkout --detach FETCH_HEAD
  fi

  test -f TypeScript/src/lib/es5.d.ts
}

run_lint() {
  ci_section "Lint"
  cargo fmt --check
  scripts/arch/check-workspace-metadata.sh
  scripts/check-crate-root-files.sh
  cargo clippy \
    -p tsz-common -p tsz-scanner -p tsz-parser -p tsz-binder \
    -p tsz-solver -p tsz-checker -p tsz-emitter -p tsz-lowering -p tsz-lsp \
    --all-targets -- -D warnings
  scripts/arch/check-checker-boundaries.sh
}

nextest_allow_no_tests() {
  set +e
  cargo nextest run --profile ci "$@"
  local rc="$?"
  set -e
  if [[ "$rc" -eq 0 || "$rc" -eq 4 ]]; then
    return 0
  fi
  return "$rc"
}

run_unit_tests() {
  ci_section "Workspace nextest suites"
  cargo nextest run --profile ci --no-tests=pass \
    -p tsz-common \
    -p tsz-scanner \
    -p tsz-parser \
    -p tsz-binder \
    -p tsz-solver \
    -p tsz-checker \
    -p tsz-emitter \
    -p tsz-lsp \
    -p tsz-core
}

build_test_binaries() {
  ci_section "Build dist-fast test binaries"
  local binaries=(
    .target/dist-fast/tsz
    .target/dist-fast/tsz-server
    .target/dist-fast/tsz-conformance
    .target/dist-fast/generate-tsc-cache
  )
  local missing=0
  local bin
  for bin in "${binaries[@]}"; do
    if [[ ! -x "$bin" ]]; then
      missing=1
      break
    fi
  done
  local trusted_cache=0
  if [[ "${TSZ_CI_TRUST_DIST_FAST_CACHE:-0}" == "1" ]]; then
    trusted_cache=1
  elif [[ -f .ci-cache/dist-fast-cache-hit ]]; then
    local cache_commit expected_commit
    cache_commit="$(tr -d '[:space:]' < .ci-cache/dist-fast-cache-hit)"
    expected_commit="${COMMIT_SHA:-${REVISION_ID:-}}"
    if [[ -n "$expected_commit" && "$cache_commit" == "$expected_commit" ]]; then
      trusted_cache=1
    fi
  fi

  if [[ "$missing" -eq 0 && "$trusted_cache" -eq 1 ]]; then
    echo "Using cached dist-fast binaries"
    ls -lh "${binaries[@]}"
    mkdir -p .target/release
    ln -sf "$ROOT_DIR/.target/dist-fast/tsz-server" .target/release/tsz-server
    return 0
  fi

  cargo build --profile dist-fast -p tsz-cli --bin tsz --bin tsz-server
  cargo build --profile dist-fast -p tsz-conformance --bin tsz-conformance --bin generate-tsc-cache
  mkdir -p .target/release
  ln -sf "$ROOT_DIR/.target/dist-fast/tsz-server" .target/release/tsz-server
  ls -lh "${binaries[@]}"
}

build_wasm() {
  ci_section "WASM build"
  (
    cd crates/tsz-wasm
    wasm-pack build --target nodejs --out-dir ../../pkg --no-opt
  )
  mkdir -p pkg/lib
  cp -R TypeScript/src/lib/. pkg/lib/
}

prep_node_artifacts() {
  ci_section "Prep Node harnesses"
  (
    cd scripts
    if [[ ! -x node_modules/.bin/tsc ]]; then
      npm install --silent
    else
      echo "Using cached scripts/node_modules"
    fi
    cd emit
    npx tsc -p tsconfig.json
  )
  ./scripts/fourslash/run-fourslash.sh --prep-only
}

read_conformance_results() {
  local last_run_path="$1"
  python3 - "$last_run_path" <<'PY' 2>/dev/null || echo "0 0"
import sys

passed = 0
recorded = 0
with open(sys.argv[1], encoding="utf-8", errors="replace") as f:
    for line in f:
        if line.startswith(("PASS ", "FAIL ", "CRASH ", "TIMEOUT ")):
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

run_conformance() {
  ci_section "Conformance"
  mkdir -p "$LOG_DIR/conformance"
  local log_file="$LOG_DIR/conformance/full.log"
  local last_run="scripts/conformance/conformance-last-run.txt"
  rm -f "$last_run"

  set +e
  ./scripts/conformance/conformance.sh run --workers "$CONFORMANCE_WORKERS" >"$log_file" 2>&1
  local rc="$?"
  set -e

  grep -a 'FINAL RESULTS:' "$log_file" | tail -1 || true

  local total_passed=0 total_tests=0
  if [[ -f "$last_run" ]]; then
    read -r total_passed total_tests < <(read_conformance_results "$last_run")
  fi
  total_passed="$(num_or_zero "$total_passed")"
  total_tests="$(num_or_zero "$total_tests")"

  printf '{"rc":%s,"passed":%s,"total":%s,"workers":%s}\n' \
    "$rc" "$total_passed" "$total_tests" "$CONFORMANCE_WORKERS" \
    > "$METRICS_DIR/conformance.json"
  echo "Conformance workers: ${CONFORMANCE_WORKERS}"
  echo "Conformance wrapper exit: ${rc}"
  echo "Conformance aggregate: ${total_passed}/${total_tests}"

  if [[ "$rc" -ne 0 ]]; then
    echo "error: conformance wrapper failed" >&2
    show_log_tail "$log_file"
    return 1
  fi

  baseline="$(jq -r '.summary.passed // 0' scripts/conformance/conformance-snapshot.json)"
  baseline_total="$(jq -r '.summary.total_tests // .summary.total // 0' scripts/conformance/conformance-snapshot.json)"
  if [[ "$baseline_total" -gt 0 && "$total_tests" -lt "$baseline_total" ]]; then
    echo "error: conformance coverage is incomplete: ${total_tests} < ${baseline_total}" >&2
    show_log_tail "$log_file"
    return 1
  fi
  if [[ "$baseline" -gt 0 && "$total_passed" -lt "$baseline" ]]; then
    echo "error: conformance regression: ${total_passed} < ${baseline}" >&2
    show_log_tail "$log_file"
    return 1
  fi
}

run_emit_shards() {
  ci_section "Emit shards"
  mkdir -p "$LOG_DIR/emit"
  export TSZ_BIN="$ROOT_DIR/.target/dist-fast/tsz"
  echo "Emit shard config: shards=${SHARD_COUNT} workers_per_shard=${EMIT_WORKERS} chunk=${EMIT_CHUNK}"

  for shard in $(seq 0 $((SHARD_COUNT - 1))); do
    (
      set +e
      offset=$((shard * EMIT_CHUNK))
      detail_json="$METRICS_DIR/emit-detail-${shard}.json"
      ./scripts/emit/run.sh --skip-build --max="$EMIT_CHUNK" --offset="$offset" --concurrency="$EMIT_WORKERS" \
        --json-out="$detail_json" \
        >"$LOG_DIR/emit/shard-${shard}.log" 2>&1
      rc="$?"
      js_p="$(jq -r '.summary.jsPass // 0' "$detail_json" 2>/dev/null || echo 0)"
      js_t="$(jq -r '.summary.jsTotal // 0' "$detail_json" 2>/dev/null || echo 0)"
      js_s="$(jq -r '.summary.jsSkip // 0' "$detail_json" 2>/dev/null || echo 0)"
      js_to="$(jq -r '.summary.jsTimeout // 0' "$detail_json" 2>/dev/null || echo 0)"
      dts_p="$(jq -r '.summary.dtsPass // 0' "$detail_json" 2>/dev/null || echo 0)"
      dts_t="$(jq -r '.summary.dtsTotal // 0' "$detail_json" 2>/dev/null || echo 0)"
      dts_s="$(jq -r '.summary.dtsSkip // 0' "$detail_json" 2>/dev/null || echo 0)"
      js_p="$(num_or_zero "$js_p")"
      js_t="$(num_or_zero "$js_t")"
      js_s="$(num_or_zero "$js_s")"
      js_to="$(num_or_zero "$js_to")"
      dts_p="$(num_or_zero "$dts_p")"
      dts_t="$(num_or_zero "$dts_t")"
      dts_s="$(num_or_zero "$dts_s")"
      printf '{"shard":%s,"rc":%s,"js_passed":%s,"js_total":%s,"js_skipped":%s,"js_timeouts":%s,"dts_passed":%s,"dts_total":%s,"dts_skipped":%s}\n' \
        "$shard" "$rc" "$js_p" "$js_t" "$js_s" "$js_to" "$dts_p" "$dts_t" "$dts_s" \
        > "$METRICS_DIR/emit-shard-${shard}.json"
      if [[ "$rc" -ne 0 ]]; then
        show_log_tail "$LOG_DIR/emit/shard-${shard}.log"
      fi
      echo "EMIT_SHARD shard=${shard} rc=${rc} js=${js_p}/${js_t} skip=${js_s} timeout=${js_to} dts=${dts_p}/${dts_t} skip=${dts_s}"
      exit 0
    ) &
  done
  wait
}

aggregate_emit() {
  ci_section "Aggregate emit"
  local js_passed=0 js_total=0 js_skipped=0 js_timeouts=0 dts_passed=0 dts_total=0 dts_skipped=0 shard_count=0
  for f in "$METRICS_DIR"/emit-shard-*.json; do
    [[ -f "$f" ]] || continue
    js_passed=$((js_passed + $(jq -r '.js_passed' "$f")))
    js_total=$((js_total + $(jq -r '.js_total' "$f")))
    js_skipped=$((js_skipped + $(jq -r '.js_skipped // 0' "$f")))
    js_timeouts=$((js_timeouts + $(jq -r '.js_timeouts // 0' "$f")))
    dts_passed=$((dts_passed + $(jq -r '.dts_passed' "$f")))
    dts_total=$((dts_total + $(jq -r '.dts_total' "$f")))
    dts_skipped=$((dts_skipped + $(jq -r '.dts_skipped // 0' "$f")))
    shard_count=$((shard_count + 1))
  done

  echo "Emit shards: ${shard_count}/${SHARD_COUNT}"
  echo "Emit aggregate: JS ${js_passed}/${js_total} (skip=${js_skipped}, timeout=${js_timeouts}), DTS ${dts_passed}/${dts_total} (skip=${dts_skipped})"

  if [[ "$shard_count" -lt "$SHARD_COUNT" || "$js_total" -eq 0 ]]; then
    echo "error: emit shard coverage is not trustworthy" >&2
    show_log_tails "$LOG_DIR/emit"
    return 1
  fi

  js_rate="$(awk -v p="$js_passed" -v t="$js_total" 'BEGIN { if (t > 0) printf "%.1f", (p / t) * 100; else print "0.0" }')"
  dts_rate="$(awk -v p="$dts_passed" -v t="$dts_total" 'BEGIN { if (t > 0) printf "%.1f", (p / t) * 100; else print "0.0" }')"
  jq -n \
    --arg suite "emit" \
    --arg js_pass_rate "$js_rate" \
    --argjson js_passed "$js_passed" \
    --argjson js_total "$js_total" \
    --argjson js_skipped "$js_skipped" \
    --argjson js_timeouts "$js_timeouts" \
    --arg dts_pass_rate "$dts_rate" \
    --argjson dts_passed "$dts_passed" \
    --argjson dts_total "$dts_total" \
    --argjson dts_skipped "$dts_skipped" \
    '{suite:$suite, js_pass_rate:$js_pass_rate, js_passed:$js_passed, js_total:$js_total, js_skipped:$js_skipped, js_timeouts:$js_timeouts, dts_pass_rate:$dts_pass_rate, dts_passed:$dts_passed, dts_total:$dts_total, dts_skipped:$dts_skipped}' \
    > "$METRICS_DIR/emit.json"

  base_js="$(jq -r '.summary.jsPass // 0' scripts/emit/emit-snapshot.json)"
  base_dts="$(jq -r '.summary.dtsPass // 0' scripts/emit/emit-snapshot.json)"
  if [[ "$base_js" -gt 0 && "$js_passed" -lt "$base_js" ]]; then
    echo "error: emit JS regression: ${js_passed} < ${base_js}" >&2
    show_log_tails "$LOG_DIR/emit"
    return 1
  fi
  if [[ "$base_dts" -gt 0 && "$dts_passed" -lt "$base_dts" ]]; then
    echo "error: emit DTS regression: ${dts_passed} < ${base_dts}" >&2
    show_log_tails "$LOG_DIR/emit"
    return 1
  fi
}

run_fourslash_shards() {
  ci_section "Fourslash shards"
  mkdir -p "$LOG_DIR/fourslash"
  echo "Fourslash shard config: shards=${SHARD_COUNT} workers_per_shard=${FOURSLASH_WORKERS}"

  for shard in $(seq 0 $((SHARD_COUNT - 1))); do
    (
      set +e
      ./scripts/fourslash/run-fourslash.sh \
        --skip-cargo-build \
        --skip-ts-build \
        --shard="${shard}/${SHARD_COUNT}" \
        --workers="$FOURSLASH_WORKERS" --memory-limit=512 \
        >"$LOG_DIR/fourslash/shard-${shard}.log" 2>&1
      rc="$?"
      results="$(grep -a '^Results:' "$LOG_DIR/fourslash/shard-${shard}.log" | tail -1 || true)"
      passed="$(echo "$results" | grep -oE 'Results:[[:space:]]*[0-9]+ passed' | grep -oE '[0-9]+' | head -1 || true)"
      total="$(echo "$results" | grep -oE 'out of [0-9]+' | grep -oE '[0-9]+' | head -1 || true)"
      passed="$(num_or_zero "$passed")"
      total="$(num_or_zero "$total")"
      printf '{"shard":%s,"rc":%s,"passed":%s,"total":%s}\n' "$shard" "$rc" "$passed" "$total" \
        > "$METRICS_DIR/fourslash-shard-${shard}.json"
      if [[ "$rc" -ne 0 ]]; then
        show_log_tail "$LOG_DIR/fourslash/shard-${shard}.log"
      fi
      echo "FOURSLASH_SHARD shard=${shard} rc=${rc} passed=${passed} total=${total}"
      exit 0
    ) &
  done
  wait
}

aggregate_fourslash() {
  ci_section "Aggregate fourslash"
  local total_passed=0 total_tests=0 shard_count=0
  for f in "$METRICS_DIR"/fourslash-shard-*.json; do
    [[ -f "$f" ]] || continue
    total_passed=$((total_passed + $(jq -r '.passed' "$f")))
    total_tests=$((total_tests + $(jq -r '.total' "$f")))
    shard_count=$((shard_count + 1))
  done

  echo "Fourslash shards: ${shard_count}/${SHARD_COUNT}"
  echo "Fourslash aggregate: ${total_passed}/${total_tests}"

  if [[ "$shard_count" -lt "$SHARD_COUNT" || "$total_tests" -eq 0 ]]; then
    echo "error: fourslash shard coverage is not trustworthy" >&2
    show_log_tails "$LOG_DIR/fourslash"
    return 1
  fi

  baseline="$(jq -r '.summary.passed // .passed // 0' scripts/fourslash/fourslash-snapshot.json)"
  if [[ "$baseline" -gt 0 ]]; then
    tolerance="$(awk "BEGIN {printf \"%d\", $baseline * 0.001 + 1}")"
    floor=$((baseline - tolerance))
    if [[ "$total_passed" -lt "$floor" ]]; then
      echo "error: fourslash regression: ${total_passed} < ${baseline} (floor=${floor})" >&2
      show_log_tails "$LOG_DIR/fourslash"
      return 1
    fi
  fi
}

run_common_setup() {
  local suite="${1:-all}"
  timed ensure_host_tools ensure_host_tools "$suite"
  timed ensure_source_git_context ensure_source_git_context
  timed init_typescript_submodule init_typescript_submodule
}

run_all_suites() {
  timed run_lint run_lint
  timed run_unit_tests run_unit_tests
  timed build_test_binaries build_test_binaries
  timed build_wasm build_wasm
  timed prep_node_artifacts prep_node_artifacts
  timed run_conformance run_conformance
  timed run_emit_shards run_emit_shards
  timed aggregate_emit aggregate_emit
  timed run_fourslash_shards run_fourslash_shards
  timed aggregate_fourslash aggregate_fourslash
}

main() {
  local suite="${1:-${TSZ_CI_SUITE:-all}}"

  run_common_setup "$suite"

  case "$suite" in
    all|full)
      run_all_suites
      ;;
    lint)
      timed run_lint run_lint
      ;;
    unit)
      timed run_unit_tests run_unit_tests
      ;;
    wasm)
      timed build_wasm build_wasm
      ;;
    conformance)
      timed build_test_binaries build_test_binaries
      timed run_conformance run_conformance
      ;;
    emit)
      timed build_test_binaries build_test_binaries
      timed prep_node_artifacts prep_node_artifacts
      timed run_emit_shards run_emit_shards
      timed aggregate_emit aggregate_emit
      ;;
    fourslash)
      timed build_test_binaries build_test_binaries
      timed prep_node_artifacts prep_node_artifacts
      timed run_fourslash_shards run_fourslash_shards
      timed aggregate_fourslash aggregate_fourslash
      ;;
    *)
      echo "error: unknown CI suite '${suite}'" >&2
      echo "valid suites: all, lint, unit, wasm, conformance, emit, fourslash" >&2
      return 2
      ;;
  esac
}

main "$@"
