#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"
source scripts/ci/suite-metadata.sh
# shellcheck source=scripts/ci/lib/typescript-corpus.sh
source scripts/ci/lib/typescript-corpus.sh

export CARGO_TERM_COLOR="${CARGO_TERM_COLOR:-never}"
export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-1}"
export CARGO_HOME="${TSZ_CI_CARGO_HOME:-$ROOT_DIR/.ci-cache/cargo-home}"
SCCACHE_VERSION="${SCCACHE_VERSION:-0.9.1}"
# Pinned fallback versions for GitHub-hosted runners. These only fire behind
# the `command -v` guards below.
NEXTEST_VERSION="${NEXTEST_VERSION:-0.9.137}"
export CARGO_PROFILE_DIST_FAST_LTO="${CARGO_PROFILE_DIST_FAST_LTO:-false}"
export RUST_MIN_STACK="${RUST_MIN_STACK:-16777216}"
export RUST_TEST_TIMEOUT="${RUST_TEST_TIMEOUT:-300}"
export NPM_CONFIG_CACHE="${NPM_CONFIG_CACHE:-$ROOT_DIR/.ci-cache/npm}"
export npm_config_cache="$NPM_CONFIG_CACHE"
export PATH="$CARGO_HOME/bin:$HOME/.cargo/bin:/usr/local/cargo/bin:$PATH"

mkdir -p "$CARGO_HOME" "$NPM_CONFIG_CACHE"

# These frozen thresholds describe the retired implementation checkpoint. R0's
# scheduled workflow runs the retained conformance/emit/fourslash commands with
# continue-on-error and uploads their real metrics, so falling below a threshold
# is an observation rather than a rewrite merge-gate failure. Keep the historical
# comparison logic intact until each semantic family graduates into the active
# capability floor.
# Snapshot refreshed 2026-07-25 at 5a1aa359: macOS measures 11342/12043.
# This floor is deliberately BELOW that. The effective gate is
# min(snapshot.passed, this floor), so the floor is what actually binds, and a
# macOS-measured number pinned here would make the Linux nightly permanently
# red — there is a recorded Linux/macOS delta of roughly 43 tests, cause not
# yet isolated (clusters in projects/* and module-resolution). 11250 leaves
# ~92 tests of headroom, more than twice the recorded delta, while narrowing
# the tolerated regression from 323 tests to 92. Tighten toward the observed
# Linux number once the first nightly heavy-lane run reports it.
TSZ_CI_CONFORMANCE_ACCEPTED_FLOOR="${TSZ_CI_CONFORMANCE_ACCEPTED_FLOOR:-11250}"
# Optional accepted-regression list for temporary conformance runways. Keep this
# path-based, not count-based: fixing one listed test must not let a new
# unlisted regression pass CI under the same aggregate deficit.
TSZ_CI_CONFORMANCE_ACCEPTED_REGRESSIONS="${TSZ_CI_CONFORMANCE_ACCEPTED_REGRESSIONS:-scripts/conformance/conformance-accepted-regressions.txt}"
# Calibrated to the TypeScript 7 corpus (submodule SHA 4d4f005c): 11563 JS- and
# 1390 DTS-eligible baselines. The prior 13526/1486 values were TypeScript 6-era
# corpus totals and no longer describe the pinned corpus.
# JS floor is 11562: jsdocDisallowedInTypescript's checked-in baseline is a
# stale 6.0.0 artifact — tsz's output for the `var g` line is byte-identical
# to tsc 7.0.2 (the corpus package.json is 6.0.0 while the oracle is 7.0.2).
# Recalibrate to 11563 when the corpus baseline is regenerated at 7.0.2 AND
# the hof/hof2 parameter-position Corsa garbage-tail recovery lands.
TSZ_CI_JS_ACCEPTED_FLOOR="${TSZ_CI_JS_ACCEPTED_FLOOR:-11562}"
# DTS floor tracks the measured value exactly: declaration emit is a
# deterministic text comparison against checked-in baselines with no platform
# variance, so there is no headroom to reserve. 1372 -> 1375 after #15917,
# 1375 -> 1377 after #16909 (const-asserted computed index-signature order)
# and #16919 (concrete/dynamic computed index-signature bucket order).
TSZ_CI_DTS_ACCEPTED_FLOOR="${TSZ_CI_DTS_ACCEPTED_FLOOR:-1377}"

cap_positive_baseline() {
  local baseline="$1"
  local accepted_floor="$2"
  if [[ "$baseline" =~ ^[0-9]+$ && "$accepted_floor" =~ ^[0-9]+$ \
    && "$baseline" -gt 0 && "$accepted_floor" -gt 0 \
    && "$baseline" -gt "$accepted_floor" ]]; then
    printf '%s\n' "$accepted_floor"
  else
    printf '%s\n' "$baseline"
  fi
}

HOST_CPUS="$(getconf _NPROCESSORS_ONLN 2>/dev/null || nproc 2>/dev/null || echo 8)"
# shellcheck source=scripts/ci/ci-resources.sh
source "$(dirname "${BASH_SOURCE[0]}")/ci-resources.sh"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-$(default_cargo_build_jobs)}"
# Cap nextest test-thread parallelism on GitHub-hosted runners. Override with
# TSZ_CI_UNIT_TEST_THREADS after measuring the active rewrite suite.
export UNIT_NEXTEST_TEST_THREADS="${TSZ_CI_UNIT_TEST_THREADS:-2}"
echo "info: CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS} (HOST_CPUS=${HOST_CPUS})" >&2

SHARD_COUNT="${TSZ_CI_SHARDS:-4}"

EMIT_WORKERS="${TSZ_CI_EMIT_WORKERS:-${TSZ_CI_SHARD_WORKERS:-$(default_emit_workers)}}"
FOURSLASH_WORKERS="${TSZ_CI_FOURSLASH_WORKERS:-${TSZ_CI_SHARD_WORKERS:-$(default_fourslash_workers)}}"
CONFORMANCE_WORKERS="${TSZ_CI_CONFORMANCE_WORKERS:-$(default_conformance_workers)}"
CONFORMANCE_SHARD_INDEX="${_TSZ_CI_CONFORMANCE_SHARD_INDEX:-${TSZ_CI_CONFORMANCE_SHARD_INDEX:-0}}"
CONFORMANCE_SHARD_COUNT="${_TSZ_CI_CONFORMANCE_SHARD_COUNT:-${TSZ_CI_CONFORMANCE_SHARDS:-1}}"
CONFORMANCE_SHARD_STRATEGY="${TSZ_CI_CONFORMANCE_SHARD_STRATEGY:-hash}"
EMIT_CHUNK="${TSZ_CI_EMIT_CHUNK:-4000}"
EMIT_TIMEOUT_MS="${TSZ_CI_EMIT_TIMEOUT_MS:-60000}"
METRICS_DIR="${TSZ_CI_METRICS_DIR:-ci-metrics}"
LOG_DIR="${TSZ_CI_LOG_DIR:-.ci-logs}"
if [[ "$METRICS_DIR" != /* ]]; then
  METRICS_DIR="$ROOT_DIR/$METRICS_DIR"
fi
if [[ "$LOG_DIR" != /* ]]; then
  LOG_DIR="$ROOT_DIR/$LOG_DIR"
fi
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

publish_latest_metric() {
  local suite="$1"
  local file="$2"
  if [[ ! -f "$file" ]]; then
    return 0
  fi
  echo "Recorded ${suite} metrics at ${file}"
}

write_emit_metric() {
  local out="$1"
  local js_passed="$2" js_total="$3" js_skipped="$4" js_timeouts="$5"
  local dts_passed="$6" dts_total="$7" dts_skipped="$8"

  local js_rate dts_rate
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
    > "$out"
}

suite_needs_group() {
  ci_suite_needs_group "$@"
}

ensure_host_tools() {
  local suite="${1:-all}"
  ci_section "Install host tools"

  if [[ "${TSZ_CI_SKIP_HOST_APT:-0}" == "1" ]]; then
    echo "Skipping apt host tool installation (TSZ_CI_SKIP_HOST_APT=1)"
  elif command -v apt-get >/dev/null 2>&1; then
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
  fi

  if suite_needs_group "$suite" unit && ! command -v cargo-nextest >/dev/null 2>&1; then
    curl -LsSf "https://get.nexte.st/${NEXTEST_VERSION}/linux" | tar zxf - -C /usr/local/bin
  fi

  if suite_needs_group "$suite" rust_compile; then
    setup_sccache
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

setup_sccache() {
  if command -v sccache >/dev/null 2>&1; then
    echo "sccache $(sccache --version 2>&1 | head -1) already available"
    return 0
  fi

  local arch platform
  arch="$(uname -m)"
  if [[ "$arch" == "aarch64" ]]; then
    platform="aarch64-unknown-linux-musl"
  else
    platform="x86_64-unknown-linux-musl"
  fi

  local url="https://github.com/mozilla/sccache/releases/download/v${SCCACHE_VERSION}/sccache-v${SCCACHE_VERSION}-${platform}.tar.gz"
  local tmp_dir install_dir
  tmp_dir="$(mktemp -d)"
  # Prefer system bin dirs with write access, fall back to CARGO_HOME/bin or ~/bin
  if [[ -w /usr/local/bin ]]; then
    install_dir=/usr/local/bin
  elif [[ -d "$CARGO_HOME/bin" ]]; then
    install_dir="$CARGO_HOME/bin"
  else
    install_dir="$HOME/.local/bin"
    mkdir -p "$install_dir"
    export PATH="$install_dir:$PATH"
  fi
  echo "Downloading sccache v${SCCACHE_VERSION} → ${install_dir}..."
  if curl -fsSL "$url" -o "$tmp_dir/sccache.tar.gz" 2>/dev/null; then
    tar -xzf "$tmp_dir/sccache.tar.gz" -C "$tmp_dir" 2>/dev/null
    local bin="$tmp_dir/sccache-v${SCCACHE_VERSION}-${platform}/sccache"
    if [[ -f "$bin" ]]; then
      install -m 755 "$bin" "$install_dir/sccache"
    fi
  fi
  rm -rf "$tmp_dir"

  if command -v sccache >/dev/null 2>&1; then
    echo "sccache installed: $(sccache --version 2>&1 | head -1)"
  else
    echo "warning: sccache install failed; builds will proceed without it" >&2
  fi
}

configure_sccache() {
  if ! command -v sccache >/dev/null 2>&1; then
    return 0
  fi

  export SCCACHE_DIR="${SCCACHE_DIR:-$ROOT_DIR/.ci-cache/sccache}"
  export RUSTC_WRAPPER="sccache"
  export CARGO_INCREMENTAL="0"  # incompatible with sccache
  export SCCACHE_LOG="${SCCACHE_LOG:-warn}"
  mkdir -p "$SCCACHE_DIR"
  echo "sccache: local cache dir=${SCCACHE_DIR}"
  sccache --stop-server 2>/dev/null || true
  if sccache --start-server; then
    echo "sccache server started"
  else
    echo "warning: sccache server failed to start; unsetting RUSTC_WRAPPER" >&2
    unset RUSTC_WRAPPER
    export CARGO_INCREMENTAL="1"
  fi
}

ensure_source_git_context() {
  ci_section "Ensure git metadata"

  if git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    return 0
  fi

  git init
  git config user.email "ci@tsz.local"
  git config user.name "TSZ CI"
  git remote add origin "${TSZ_CI_REPO_URL:-https://github.com/tsz-org/tsz.git}"
  git add -A
  git commit -q -m "ci source snapshot"
}

init_typescript_corpus() {
  ci_section "Init TypeScript corpus"
  materialize_typescript_corpus
}

run_lint() {
  ci_section "Rewrite foundation gate"
  cargo fmt --all --check || return $?
  cargo check --workspace --all-targets || return $?
  cargo clippy --profile ci-lint --workspace \
    --all-targets -- -D warnings || return $?

  python3 scripts/arch/arch_guard.py || return $?
  python3 scripts/reset/verify-legacy-inline.py || return $?
  (
    cd scripts/arch
    python3 -m unittest discover -p "test_arch_guard*.py" -v
  ) || return $?

  python3 scripts/lib/check-sh-portability.py || return $?
  python3 scripts/lib/test_check_sh_portability.py || return $?
  python3 scripts/ci/test_typescript_corpus_init.py || return $?
  node scripts/bench/test-typescript-tool-resolution.mjs || return $?
  scripts/ci/check-unit-gate-contracts.sh || return $?

  if command -v sccache >/dev/null 2>&1; then
    echo "::group::sccache stats"
    sccache --show-stats || true
    echo "::endgroup::"
  fi
}

# The active rewrite workspace is intentionally explicit. The architecture
# guard independently rejects additional members, and every selected test must
# pass; the retired known-failures inventory does not apply to this suite.
_UNIT_TEST_PACKAGES=(
  tsz-core
  tsz-cli
  tsz-conformance
)

# Resolve the active package set for run_unit_tests. A narrow override is
# accepted only when every name belongs to the clean-slate workspace.
unit_test_packages() {
  local override="${_TSZ_CI_UNIT_PACKAGES_OVERRIDE:-}"
  if [[ -z "$override" ]]; then
    printf '%s\n' "${_UNIT_TEST_PACKAGES[@]}"
    return
  fi

  local known=" ${_UNIT_TEST_PACKAGES[*]} "
  local crate
  for crate in $override; do
    if [[ "$known" != *" $crate "* ]]; then
      echo "error: _TSZ_CI_UNIT_PACKAGES_OVERRIDE contains unknown crate '$crate'" >&2
      echo "  valid crates:${known}" >&2
      return 2
    fi
  done
  for crate in $override; do
    printf '%s\n' "$crate"
  done
}

run_unit_tests() {
  ci_section "Strict rewrite nextest suite"
  local packages
  packages="$(unit_test_packages)" || return "$?"

  local extra_flags=()
  if [[ -n "${_TSZ_CI_UNIT_PACKAGES_OVERRIDE:-}" ]]; then
    echo "info: narrowed unit run to: ${_TSZ_CI_UNIT_PACKAGES_OVERRIDE}"
    extra_flags+=(--allow-no-reports)
  fi
  scripts/ci/unit-nextest.sh --junit-dir "$LOG_DIR/unit-junit" \
    --packages "$packages" ${extra_flags[@]+"${extra_flags[@]}"}
}

build_unit_test_archive() {
  ci_section "Build unit test archive"
  echo "Unit archive fanout is disabled; GitHub Actions jobs run unit suites directly."
}

run_unit_shard() {
  ci_section "Unit shard"
  echo "Unit shard fanout is disabled; running the local unit suite."
  run_unit_tests
}

build_test_binaries() {
  ci_section "Build dist-fast test binaries"
  local binaries=(
    .target/dist-fast/tsz
    .target/dist-fast/tsz-lsp
    .target/dist-fast/tsz-server
    .target/dist-fast/try-tsz
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
    expected_commit="${COMMIT_SHA:-${REVISION_ID:-${GITHUB_SHA:-}}}"
    if [[ -z "$expected_commit" ]]; then
      expected_commit="$(git rev-parse HEAD 2>/dev/null || true)"
    fi
    if [[ -n "$expected_commit" && "$cache_commit" == "$expected_commit" ]]; then
      trusted_cache=1
    fi
  fi

  if [[ "$missing" -eq 0 && "$trusted_cache" -eq 1 ]]; then
    echo "Using cached dist-fast binaries"
    ls -lh "${binaries[@]}"
    mkdir -p .target/release
    ln -sf "$ROOT_DIR/.target/dist-fast/tsz-lsp" .target/release/tsz-lsp
    ln -sf "$ROOT_DIR/.target/dist-fast/tsz-server" .target/release/tsz-server
    return 0
  fi

  local heartbeat_pid heartbeat_interval cargo_rc
  heartbeat_interval="${TSZ_CI_DIST_BUILD_HEARTBEAT_SECONDS:-60}"
  (
    while true; do
      sleep "$heartbeat_interval"
      echo "dist-fast cargo build still running at $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    done
  ) &
  heartbeat_pid="$!"

  ci_report_memory "pre-dist-build"

  # Wrap with safe-run.sh so cargo is killed gracefully before the kernel
  # OOM-killer fires and silently kills the GitHub Actions runner process.
  set +e
  CARGO_INCREMENTAL=0 "$ROOT_DIR/scripts/safe-run.sh" --limit "${TSZ_CI_DIST_BUILD_MEMORY_LIMIT_PCT:-88}%" -- \
    cargo build --profile dist-fast \
      --jobs "$CARGO_BUILD_JOBS" \
      -p tsz-cli \
      -p tsz-conformance \
      --bin tsz \
      --bin tsz-lsp \
      --bin tsz-server \
      --bin try-tsz \
      --bin tsz-conformance \
      --bin generate-tsc-cache
  cargo_rc="$?"
  set -e
  kill "$heartbeat_pid" >/dev/null 2>&1 || true
  wait "$heartbeat_pid" 2>/dev/null || true
  if [[ "$cargo_rc" -ne 0 ]]; then
    return "$cargo_rc"
  fi
  mkdir -p .target/release
  ln -sf "$ROOT_DIR/.target/dist-fast/tsz-lsp" .target/release/tsz-lsp
  ln -sf "$ROOT_DIR/.target/dist-fast/tsz-server" .target/release/tsz-server
  ls -lh "${binaries[@]}"
}

prep_node_artifacts() {
  ci_section "Prep Node harnesses"
  ./scripts/setup/ensure-pinned-typescript.sh scripts
  (
    cd scripts
    echo "Using pinned scripts/node_modules"
    cd emit
    npx tsc -p tsconfig.json
  )
  ./scripts/fourslash/run-fourslash.sh --prep-only
}

maybe_prep_node_artifacts() {
  if [[ "${TSZ_CI_NODE_HARNESS_PREPPED:-0}" == "1" ]]; then
    echo "info: skipping prep_node_artifacts (TSZ_CI_NODE_HARNESS_PREPPED=1)"
    return 0
  fi
  prep_node_artifacts
}

# shellcheck source=scripts/ci/lib/full-ci-conformance.sh
source scripts/ci/lib/full-ci-conformance.sh


validate_emit_aggregate_counts() {
  local js_passed="$1" js_total="$2" js_skipped="$3" js_timeouts="$4"
  local dts_passed="$5" dts_total="$6" dts_skipped="$7"
  local files_count="$8" expected_shards="$9"

  echo "Emit aggregate: JS ${js_passed}/${js_total} (skip=${js_skipped}, timeout=${js_timeouts}), DTS ${dts_passed}/${dts_total} across ${files_count}/${expected_shards} shards"

  if [[ "$files_count" -lt "$expected_shards" ]]; then
    echo "error: only ${files_count}/${expected_shards} emit shards collected; some shards may have crashed" >&2
    return 1
  fi
  if [[ "$js_total" -eq 0 ]]; then
    echo "error: emit aggregate has zero JS tests — something is wrong" >&2
    return 1
  fi

  local base_js base_dts
  base_js="$(jq -r '.summary.jsPass // 0'  scripts/emit/emit-snapshot.json)"
  base_dts="$(jq -r '.summary.dtsPass // 0' scripts/emit/emit-snapshot.json)"
  base_js="$(cap_positive_baseline "$base_js" "$TSZ_CI_JS_ACCEPTED_FLOOR")"
  base_dts="$(cap_positive_baseline "$base_dts" "$TSZ_CI_DTS_ACCEPTED_FLOOR")"
  if [[ "$base_js" -gt 0 && "$js_passed" -lt "$base_js" ]]; then
    echo "error: emit JS regression: ${js_passed} < ${base_js}" >&2
    return 1
  fi
  if [[ "$base_dts" -gt 0 && "$dts_passed" -lt "$base_dts" ]]; then
    echo "error: emit DTS regression: ${dts_passed} < ${base_dts}" >&2
    return 1
  fi
  echo "Emit OK: JS ${js_passed}/${js_total}, DTS ${dts_passed}/${dts_total}"
}

# Direction check for emit (#16171). The count comparison above is the whole
# emit gate today, and it cannot see two things:
#
#   * a swap — one row fixed and another broken leaves jsPass unchanged;
#   * a ratchet-down — cap_positive_baseline is min(baseline, floor), so
#     TSZ_CI_JS_ACCEPTED_FLOOR is a ceiling (an anti-unsatisfiability valve),
#     not a floor. A hand-refreshed emit-snapshot.json that lands while emit is
#     regressed takes the count bar down with it.
#
# emit-snapshot.json's detailFingerprint only proves emit-detail.json matches
# its own summary: internal consistency, not direction. So diff the named
# failing-row set out of the committed emit-detail.json, the way conformance
# diffs its failure set. Fails closed — if the per-shard detail JSON is missing
# there is no direction evidence, and a gate that silently skips when its data
# is absent is the same bug class this closes.
validate_emit_regression_set() {
  if [[ "$#" -eq 0 ]]; then
    echo "error: emit regression set check received no per-test detail JSON" >&2
    echo "hint: emit shards write ci-metrics/emit-detail-N.json via run.sh --json-out" >&2
    return 1
  fi
  python3 scripts/ci/check-emit-regression-set.py \
    --baseline scripts/emit/emit-detail.json "$@" || return 1
}

run_emit_shard() {
  ci_section "Emit shard"
  local shard_index shard_count
  shard_index="$(num_or_zero "${_TSZ_CI_EMIT_SHARD_INDEX:-0}")"
  shard_count="$(num_or_zero "${_TSZ_CI_EMIT_SHARD_COUNT:-1}")"
  local chunk="${EMIT_CHUNK:-2000}"
  local offset=$(( shard_index * chunk ))

  mkdir -p "$LOG_DIR/emit"
  export TSZ_BIN="$ROOT_DIR/.target/dist-fast/tsz"
  echo "Emit shard ${shard_index}/${shard_count}: offset=${offset} chunk=${chunk} workers=${EMIT_WORKERS} timeout_ms=${EMIT_TIMEOUT_MS}"

  local detail_json="$METRICS_DIR/emit-detail-${shard_index}.json"
  local shard_json="$METRICS_DIR/emit-shard-${shard_index}.json"
  local emit_args=(
    --skip-build
    --concurrency="$EMIT_WORKERS"
    --timeout="${EMIT_TIMEOUT_MS:-60000}"
    --json-out="$detail_json"
  )
  # Only restrict to a chunk when actually sharding; with one shard, run everything.
  if [[ "$shard_count" -gt 1 ]]; then
    emit_args+=(--max="$chunk" --offset="$offset")
  fi
  set +e
  ./scripts/emit/run.sh "${emit_args[@]}" \
    >"$LOG_DIR/emit/shard-${shard_index}.log" 2>&1
  local rc="$?"
  set -e

  local js_p js_t js_s js_to dts_p dts_t dts_s
  js_p="$(jq -r '.summary.jsPass // 0'    "$detail_json" 2>/dev/null || echo 0)"
  js_t="$(jq -r '.summary.jsTotal // 0'   "$detail_json" 2>/dev/null || echo 0)"
  js_s="$(jq -r '.summary.jsSkip // 0'    "$detail_json" 2>/dev/null || echo 0)"
  js_to="$(jq -r '.summary.jsTimeout // 0' "$detail_json" 2>/dev/null || echo 0)"
  dts_p="$(jq -r '.summary.dtsPass // 0'  "$detail_json" 2>/dev/null || echo 0)"
  dts_t="$(jq -r '.summary.dtsTotal // 0' "$detail_json" 2>/dev/null || echo 0)"
  dts_s="$(jq -r '.summary.dtsSkip // 0'  "$detail_json" 2>/dev/null || echo 0)"
  js_p="$(num_or_zero "$js_p")"
  js_t="$(num_or_zero "$js_t")"
  js_s="$(num_or_zero "$js_s")"
  js_to="$(num_or_zero "$js_to")"
  dts_p="$(num_or_zero "$dts_p")"
  dts_t="$(num_or_zero "$dts_t")"
  dts_s="$(num_or_zero "$dts_s")"

  local result_json
  result_json="$(printf '{"shard":%s,"rc":%s,"js_passed":%s,"js_total":%s,"js_skipped":%s,"js_timeouts":%s,"dts_passed":%s,"dts_total":%s,"dts_skipped":%s}' \
    "$shard_index" "$rc" "$js_p" "$js_t" "$js_s" "$js_to" "$dts_p" "$dts_t" "$dts_s")"
  echo "$result_json" > "$shard_json"
  echo "EMIT_SHARD shard=${shard_index} rc=${rc} js=${js_p}/${js_t} skip=${js_s} timeout=${js_to} dts=${dts_p}/${dts_t}"

  if [[ "$shard_count" -eq 1 ]]; then
    ci_section "Emit aggregate"
    validate_emit_aggregate_counts "$js_p" "$js_t" "$js_s" "$js_to" "$dts_p" "$dts_t" "$dts_s" 1 1 || return 1
    validate_emit_regression_set "$detail_json" || return 1
    write_emit_metric "$METRICS_DIR/emit.json" \
      "$js_p" "$js_t" "$js_s" "$js_to" \
      "$dts_p" "$dts_t" "$dts_s"
    publish_latest_metric emit "$METRICS_DIR/emit.json"
  fi
  return 0
}

# Recombine the per-shard emit results from GitHub Actions artifacts and re-run the full-corpus floor
# over the summed counts. This is the required emit leaf for multi-shard runs;
# single-shard runs validate inline in run_emit_shard above.
run_emit_aggregate() {
  ci_section "Emit aggregate"
  local expected_shards="${_TSZ_CI_EMIT_SHARD_COUNT:-1}"
  expected_shards="$(num_or_zero "$expected_shards")"
  [[ "$expected_shards" -lt 1 ]] && expected_shards=1
  local tmp_dir
  tmp_dir="$(mktemp -d)"

  # upload-artifact preserves the workspace-relative path, so the file lands at
  # emit-shard-N/ci-metrics/emit-shard-N.json.
  local artifacts_dir=".emit-shards"
  local using_artifacts=0
  if [[ -d "$artifacts_dir" ]]; then
    local found=0
    for shard_dir in "$artifacts_dir"/emit-shard-*/; do
      [[ -d "$shard_dir" ]] || continue
      local json
      json="$(find "$shard_dir" -name "emit-shard-*.json" -maxdepth 4 2>/dev/null | head -1)"
      [[ -f "$json" ]] || continue
      local shard_name
      shard_name="$(basename "$shard_dir")"
      cp "$json" "$tmp_dir/shard-${shard_name#emit-shard-}.json"
      # The same artifact carries the per-test detail (ci.yml uploads
      # emit-detail-N.json alongside emit-shard-N.json). It feeds the
      # failing-row direction check below.
      local detail
      detail="$(find "$shard_dir" -name "emit-detail-*.json" -maxdepth 4 2>/dev/null | head -1)"
      if [[ -f "$detail" ]]; then
        cp "$detail" "$tmp_dir/detail-${shard_name#emit-shard-}.json"
      fi
      found=$(( found + 1 ))
    done
    if [[ "$found" -gt 0 ]]; then
      echo "Using ${found} GitHub Actions artifact shard results from ${artifacts_dir}/"
      using_artifacts=1
    else
      echo "warning: ${artifacts_dir}/ exists but no emit-shard-*.json files found" >&2
      ls -la "$artifacts_dir"/ 2>/dev/null || true
    fi
  fi

  if [[ "$using_artifacts" -eq 0 ]]; then
    echo "error: cannot aggregate emit results — GitHub artifact shard data is missing" >&2
    return 1
  fi

  local js_p=0 js_t=0 js_s=0 js_to=0 dts_p=0 dts_t=0 dts_s=0
  local shard_count=0 failed_shards=0
  for f in "$tmp_dir"/shard-*.json; do
    [[ -f "$f" ]] || continue
    js_p=$(( js_p   + $(num_or_zero "$(jq -r '.js_passed // 0'   "$f" 2>/dev/null)") ))
    js_t=$(( js_t   + $(num_or_zero "$(jq -r '.js_total // 0'    "$f" 2>/dev/null)") ))
    js_s=$(( js_s   + $(num_or_zero "$(jq -r '.js_skipped // 0'  "$f" 2>/dev/null)") ))
    js_to=$(( js_to + $(num_or_zero "$(jq -r '.js_timeouts // 0' "$f" 2>/dev/null)") ))
    dts_p=$(( dts_p + $(num_or_zero "$(jq -r '.dts_passed // 0'  "$f" 2>/dev/null)") ))
    dts_t=$(( dts_t + $(num_or_zero "$(jq -r '.dts_total // 0'   "$f" 2>/dev/null)") ))
    dts_s=$(( dts_s + $(num_or_zero "$(jq -r '.dts_skipped // 0' "$f" 2>/dev/null)") ))
    if [[ "$(num_or_zero "$(jq -r '.rc // 0' "$f" 2>/dev/null)")" -ne 0 ]]; then
      failed_shards=$(( failed_shards + 1 ))
    fi
    shard_count=$(( shard_count + 1 ))
  done

  if [[ "$failed_shards" -gt 0 ]]; then
    echo "warning: ${failed_shards} emit shard(s) returned non-zero rc; aggregate still applies the full-corpus floor" >&2
  fi

  validate_emit_aggregate_counts "$js_p" "$js_t" "$js_s" "$js_to" "$dts_p" "$dts_t" "$dts_s" "$shard_count" "$expected_shards" || return 1
  if compgen -G "$tmp_dir/detail-*.json" >/dev/null; then
    validate_emit_regression_set "$tmp_dir"/detail-*.json || return 1
  else
    # No detail in the artifacts: fail closed rather than pass on counts alone.
    validate_emit_regression_set || return 1
  fi
  write_emit_metric "$METRICS_DIR/emit.json" \
    "$js_p" "$js_t" "$js_s" "$js_to" \
    "$dts_p" "$dts_t" "$dts_s"
  publish_latest_metric emit "$METRICS_DIR/emit.json"
}

run_fourslash_shard() {
  ci_section "Fourslash shard"
  local shard_index shard_count
  shard_index="$(num_or_zero "${_TSZ_CI_FOURSLASH_SHARD_INDEX:-0}")"
  shard_count="$(num_or_zero "${_TSZ_CI_FOURSLASH_SHARD_COUNT:-8}")"

  mkdir -p "$LOG_DIR/fourslash"
  ci_report_memory "fourslash-${shard_index}"
  echo "Fourslash shard ${shard_index}/${shard_count}: workers=${FOURSLASH_WORKERS}"

  local detail_json="$METRICS_DIR/fourslash-shard-${shard_index}.json"
  set +e
  run_with_heartbeat "fourslash-${shard_index}" \
    bash -c 'log_file="$1"; shift; "$@" 2>&1 | tee "$log_file"; exit "${PIPESTATUS[0]}"' bash "$LOG_DIR/fourslash/shard-${shard_index}.log" \
    env FOURSLASH_LOG_START=1 \
    ./scripts/fourslash/run-fourslash.sh \
    --skip-cargo-build \
    --skip-ts-build \
    --shard="${shard_index}/${shard_count}" \
    --shard-strategy="${TSZ_CI_FOURSLASH_SHARD_STRATEGY:-weighted}" \
    --workers="$FOURSLASH_WORKERS" \
    --timeout="${TSZ_CI_FOURSLASH_TIMEOUT_MS:-25000}" \
    --memory-limit=512 \
    --json-out="$detail_json"
  local rc="$?"
  set -e

  local results passed total timed_out
  results="$(grep -a '^Results:' "$LOG_DIR/fourslash/shard-${shard_index}.log" | tail -1 || true)"
  if [[ -f "$detail_json" ]]; then
    passed="$(jq -r '.summary.passed // 0' "$detail_json")"
    total="$(jq -r '.summary.total // 0' "$detail_json")"
    timed_out="$(jq -r '.summary.timedOut // 0' "$detail_json")"
  else
    passed="$(echo "$results" | grep -oE 'Results:[[:space:]]*[0-9]+ passed' | grep -oE '[0-9]+' | head -1 || true)"
    total="$(echo "$results" | grep -oE 'out of [0-9]+' | grep -oE '[0-9]+' | head -1 || true)"
    timed_out=0
  fi
  passed="$(num_or_zero "$passed")"
  total="$(num_or_zero "$total")"
  timed_out="$(num_or_zero "$timed_out")"

  if [[ -f "$detail_json" ]]; then
    local enriched_json
    enriched_json="${detail_json}.enriched"
    jq \
      --argjson shard "$shard_index" \
      --argjson rc "$rc" \
      --argjson passed "$passed" \
      --argjson total "$total" \
      --argjson timed_out "$timed_out" \
      '. + {shard:$shard, rc:$rc, passed:$passed, total:$total, timedOut:$timed_out, slowest:(.summary.slowest // [])}' \
      "$detail_json" >"$enriched_json"
    mv "$enriched_json" "$detail_json"
  else
    printf '{"shard":%s,"rc":%s,"passed":%s,"total":%s,"timedOut":%s,"slowest":[]}\n' \
      "$shard_index" "$rc" "$passed" "$total" "$timed_out" >"$detail_json"
  fi

  echo "FOURSLASH_SHARD shard=${shard_index} rc=${rc} passed=${passed}/${total} timeout=${timed_out}"
  if [[ -f "$detail_json" ]]; then
    echo "Fourslash slowest tests for shard ${shard_index}:"
    jq -r '.slowest[:10][]? | "  \(.elapsed)ms \(.status) \(.name)"' "$detail_json" || true
  fi
  if [[ "$rc" -ne 0 ]]; then
    show_log_tail "$LOG_DIR/fourslash/shard-${shard_index}.log"
  fi

  return 0
}

run_fourslash_aggregate() {
  ci_section "Fourslash aggregate"
  local expected_shards="${_TSZ_CI_FOURSLASH_SHARD_COUNT:-${TSZ_CI_FOURSLASH_SHARDS:-8}}"
  local tmp_dir
  tmp_dir="$(mktemp -d)"

  local artifacts_dir=".fourslash-shards"
  local found=0
  if [[ -d "$artifacts_dir" ]]; then
    for shard_dir in "$artifacts_dir"/fourslash-shard-*/; do
      [[ -d "$shard_dir" ]] || continue
      local json
      json="$(find "$shard_dir" -name "fourslash-shard-*.json" -maxdepth 4 2>/dev/null | head -1)"
      [[ -f "$json" ]] || continue
      local shard_name
      shard_name="$(basename "$shard_dir")"
      cp "$json" "$tmp_dir/shard-${shard_name#fourslash-shard-}.json"
      found=$(( found + 1 ))
    done
  fi
  if [[ "$found" -eq 0 ]]; then
    echo "error: cannot aggregate fourslash results — GitHub artifact shard data is missing" >&2
    return 1
  fi
  echo "Using ${found} GitHub Actions artifact shard results from ${artifacts_dir}/"

  local total_passed=0 total_tests=0 shard_count=0 failed_shards=0 timed_out=0
  for f in "$tmp_dir"/shard-*.json; do
    [[ -f "$f" ]] || continue
    total_passed=$((total_passed + $(num_or_zero "$(jq -r '.passed // .summary.passed // 0' "$f")")))
    total_tests=$((total_tests   + $(num_or_zero "$(jq -r '.total // .summary.total // 0'  "$f")")))
    timed_out=$((timed_out + $(num_or_zero "$(jq -r '.timedOut // .summary.timedOut // 0' "$f")")))
    if [[ "$(num_or_zero "$(jq -r '.rc // 0' "$f")")" -ne 0 ]]; then
      failed_shards=$((failed_shards + 1))
    fi
    shard_count=$((shard_count + 1))
  done

  echo "Fourslash aggregate: ${total_passed}/${total_tests} across ${shard_count}/${expected_shards} shards (timeout=${timed_out}, failed_shards=${failed_shards})"
  echo "Fourslash aggregate slowest tests:"
  jq -s -r '[.[] | (.slowest // .summary.slowest // [])[]] | sort_by(.elapsed) | reverse | .[:10][]? | "  \(.elapsed)ms \(.status) \(.name)"' "$tmp_dir"/shard-*.json || true
  if [[ "$failed_shards" -gt 0 ]]; then
    echo "warning: ${failed_shards} fourslash shard(s) returned non-zero; aggregate still applies the baseline floor" >&2
  fi

  if [[ "$shard_count" -lt "$expected_shards" ]]; then
    echo "error: only ${shard_count}/${expected_shards} fourslash shards collected; some shards may have crashed" >&2
    return 1
  fi
  if [[ "$total_tests" -eq 0 ]]; then
    echo "error: fourslash aggregate has zero tests" >&2
    return 1
  fi

  local baseline
  baseline="$(jq -r '.summary.passed // .passed // (.pass | length) // 0' scripts/fourslash/fourslash-snapshot.json)"
  if [[ "$baseline" -gt 0 ]]; then
    local tolerance floor
    tolerance="$(awk "BEGIN {printf \"%d\", $baseline * 0.001 + 1}")"
    floor=$((baseline - tolerance))
    if [[ "$total_passed" -lt "$floor" ]]; then
      echo "error: fourslash regression: ${total_passed} < ${baseline} (floor=${floor})" >&2
      return 1
    fi
  fi
  local pass_rate
  pass_rate="$(awk -v p="$total_passed" -v t="$total_tests" 'BEGIN { if (t > 0) printf "%.1f", (p / t) * 100; else print "0.0" }')"
  jq -n \
    --arg suite "fourslash" \
    --arg pass_rate "$pass_rate" \
    --argjson passed "$total_passed" \
    --argjson total "$total_tests" \
    --argjson shards "$shard_count" \
    '{suite:$suite, pass_rate:$pass_rate, passed:$passed, total:$total, shards:$shards}' \
    > "$METRICS_DIR/fourslash.json"
  publish_latest_metric fourslash "$METRICS_DIR/fourslash.json"
  echo "Fourslash OK: ${total_passed}/${total_tests}"
}

run_dist_binaries() {
  ci_section "Build dist-fast binaries"
  ci_report_memory "dist-binaries"
  timed build_test_binaries build_test_binaries
  show_sccache_stats
}

show_sccache_stats() {
  if command -v sccache >/dev/null 2>&1 && [[ -n "${RUSTC_WRAPPER:-}" ]]; then
    sccache --show-stats 2>/dev/null || true
  fi
}

# Advisory sccache hit-rate floor. Reads the JSON stats sccache accumulated for
# this suite's compiles, records the cache-hit ratio as a published metric, and
# emits a ::warning:: when the ratio falls below SCCACHE_HIT_RATE_FLOOR. This
# makes a silent cold-cache regression visible instead of only surfacing as
# wall-clock drift (#13605 items 3-4). Guarded on RUSTC_WRAPPER:
# dist-binaries disables sccache by design, so it has no stats and is skipped.
# Never fails the suite.
record_sccache_metric() {
  local suite="${1:-unit}"
  # No sccache, or sccache not actually wired as the rustc wrapper: nothing to do.
  if ! command -v sccache >/dev/null 2>&1 || [[ -z "${RUSTC_WRAPPER:-}" ]]; then
    return 0
  fi
  if ! command -v jq >/dev/null 2>&1; then
    return 0
  fi

  local stats_json
  stats_json="$(sccache --show-stats --stats-format=json 2>/dev/null || true)"
  if [[ -z "$stats_json" ]]; then
    echo "sccache: no JSON stats available for ${suite} (non-fatal)"
    return 0
  fi

  # cache_hits/cache_misses are objects of per-language counters; sum the values.
  # compile_requests is the total work seen. Fall back to 0 on any parse miss so
  # an sccache version change never breaks the suite.
  local hits misses requests
  hits="$(printf '%s' "$stats_json" | jq -r '[.stats.cache_hits.counts // {} | .[]] | add // 0' 2>/dev/null || echo 0)"
  misses="$(printf '%s' "$stats_json" | jq -r '[.stats.cache_misses.counts // {} | .[]] | add // 0' 2>/dev/null || echo 0)"
  requests="$(printf '%s' "$stats_json" | jq -r '.stats.compile_requests // 0' 2>/dev/null || echo 0)"
  hits="$(num_or_zero "$hits")"
  misses="$(num_or_zero "$misses")"
  requests="$(num_or_zero "$requests")"

  local total=$((hits + misses))
  local hit_rate
  hit_rate="$(awk -v h="$hits" -v t="$total" 'BEGIN { if (t > 0) printf "%.1f", (h / t) * 100; else print "0.0" }')"

  local out="$METRICS_DIR/sccache-${suite}.json"
  jq -n \
    --arg suite "sccache-${suite}" \
    --arg hit_rate "$hit_rate" \
    --argjson hits "$hits" \
    --argjson misses "$misses" \
    --argjson requests "$requests" \
    '{suite:$suite, hit_rate:$hit_rate, hits:$hits, misses:$misses, requests:$requests}' \
    > "$out"
  publish_latest_metric "sccache-${suite}" "$out"
  echo "sccache ${suite}: hit_rate=${hit_rate}% hits=${hits} misses=${misses} requests=${requests}"

  # Floor check is advisory and only meaningful once enough compiles ran — a
  # near-empty suite (everything no-op) would otherwise trip on tiny denominators.
  local floor="${SCCACHE_HIT_RATE_FLOOR:-40}"
  local min_total="${SCCACHE_HIT_RATE_MIN_TOTAL:-50}"
  if [[ "$total" -ge "$min_total" ]]; then
    local below
    below="$(awk -v r="$hit_rate" -v f="$floor" 'BEGIN { print (r + 0 < f + 0) ? 1 : 0 }')"
    if [[ "$below" == "1" ]]; then
      echo "::warning::sccache ${suite} hit-rate ${hit_rate}% is below the ${floor}% floor (hits=${hits} misses=${misses}); cache may be cold"
    fi
  fi
}

run_node_harness_prep() {
  ci_section "Prep node harnesses (emit + fourslash)"
  timed prep_node_artifacts prep_node_artifacts
}

run_lsp_e2e_smoke() {
  ci_section "LSP protocol smoke"
  local bin="$ROOT_DIR/.target/dist-fast/tsz-lsp"
  if [[ ! -x "$bin" ]]; then
    echo "error: expected executable dist-fast LSP binary at $bin" >&2
    return 1
  fi
  node scripts/lsp/e2e-smoke.mjs "$bin"
}

suite_needs_typescript_source() {
  local suite="$1"
  ci_suite_has_cache "$suite" typescript-source
}

run_common_setup() {
  local suite="${1:-all}"
  timed ensure_host_tools ensure_host_tools "$suite"
  timed ensure_source_git_context ensure_source_git_context
  if suite_needs_typescript_source "$suite"; then
    timed init_typescript_corpus init_typescript_corpus
  else
    # Skipping corpus initialization avoids downloading source for suites that
    # do not consume the TypeScript test tree.
    echo "info: skipping init_typescript_corpus (suite '$suite' does not need TS source)"
  fi
  if suite_needs_group "$suite" rust_compile; then
    if [[ "${TSZ_CI_DISABLE_SCCACHE:-0}" == "1" ]]; then
      echo "sccache: disabled by TSZ_CI_DISABLE_SCCACHE=1"
    else
      configure_sccache
    fi
  fi
}

main() {
  local suite="${1:-${TSZ_CI_SUITE:-}}"

  if [[ -z "$suite" ]]; then
    echo "usage: $0 $(ci_suite_usage full)" >&2
    return 2
  fi

  if ! ci_suite_is_known full "$suite"; then
    echo "error: unknown CI suite '${suite}'" >&2
    echo "valid suites: $(ci_suite_list full ', ')" >&2
    return 2
  fi

  run_common_setup "$suite"

  case "$suite" in
    dist-binaries)
      run_dist_binaries
      ;;
    node-harness-prep)
      run_node_harness_prep
      ;;
    lint)
      timed run_lint run_lint
      ;;
    unit)
      timed run_unit_tests run_unit_tests
      record_sccache_metric unit
      ;;
    lsp-e2e)
      timed run_lsp_e2e_smoke run_lsp_e2e_smoke
      ;;
    conformance)
      timed build_test_binaries build_test_binaries
      timed run_conformance run_conformance
      ;;
    conformance-aggregate)
      timed run_conformance_aggregate run_conformance_aggregate
      ;;
    emit-shard)
      timed build_test_binaries build_test_binaries
      timed maybe_prep_node_artifacts maybe_prep_node_artifacts
      timed run_emit_shard run_emit_shard
      ;;
    emit-aggregate)
      timed run_emit_aggregate run_emit_aggregate
      ;;
    fourslash-shard)
      timed build_test_binaries build_test_binaries
      timed maybe_prep_node_artifacts maybe_prep_node_artifacts
      timed run_fourslash_shard run_fourslash_shard
      ;;
    fourslash-aggregate)
      timed run_fourslash_aggregate run_fourslash_aggregate
      ;;
    *)
      echo "error: unknown CI suite '${suite}'" >&2
      echo "valid suites: $(ci_suite_list full ', ')" >&2
      return 2
      ;;
  esac
}

main "$@"
