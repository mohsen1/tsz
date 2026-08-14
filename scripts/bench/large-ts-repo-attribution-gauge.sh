#!/usr/bin/env bash
#
# Emit and optionally run a repeatable attribution plan for the #14351
# large-ts-repo timeout path.
#
# Usage:
#   scripts/bench/large-ts-repo-attribution-gauge.sh --json-file /tmp/large.json
#   scripts/bench/large-ts-repo-attribution-gauge.sh --prepare --run-measure
#   scripts/bench/large-ts-repo-attribution-gauge.sh --prepare --run-profile --iterations 6
#
# Plan-only mode is the default so low-disk workers can verify the gauge without
# building symbol-retaining artifacts. `--prepare` fetches/deps the fixture;
# `--run-measure` uses measure-tsz's snapshot+CPU-share protocol; `--run-profile`
# delegates to perf-flat-profile.sh.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BENCH_TARGET_DIR="$PROJECT_ROOT/.target-bench"
EXTERNAL_BENCH_DIR="${EXTERNAL_BENCH_DIR:-$BENCH_TARGET_DIR/external}"

# shellcheck source=scripts/bench/project-fixtures.sh
source "$SCRIPT_DIR/project-fixtures.sh"
# shellcheck source=scripts/bench/lib/large-ts-repo-fixture.sh
source "$SCRIPT_DIR/lib/large-ts-repo-fixture.sh"

LARGE_TS_LOCAL_DIR="${LARGE_TS_LOCAL_DIR:-${HOME}/code/large-ts-repo}"
LARGE_TS_DIR="$(tsz_large_ts_repo_default_dir "$EXTERNAL_BENCH_DIR")"
LARGE_TS_NODE_OPTIONS="${LARGE_TS_NODE_OPTIONS:---max-old-space-size=8192}"
TSZ_RUST_MIN_STACK="${TSZ_RUST_MIN_STACK:-536870912}"

TARGET_DIR="${CARGO_TARGET_DIR:-$PROJECT_ROOT/.target}"
TSZ_BIN="${TSZ:-$TARGET_DIR/dist-fast/tsz}"
JSON_FILE="$PROJECT_ROOT/artifacts/perf/large-ts-repo-attribution-$(date +%Y%m%d-%H%M%S).json"
TSCONFIG_OVERRIDE=""
ITERATIONS=4
TOP=25
TIMEOUT=1500
RUNS=1
PREPARE=false
RUN_MEASURE=false
RUN_PROFILE=false
MEASURE_EXIT_CODE=""
PROFILE_EXIT_CODE=""

usage() {
    awk 'NR > 1 { if (!/^#/) exit; sub(/^# ?/, ""); print }' "${BASH_SOURCE[0]}"
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --json-file) JSON_FILE="$2"; shift 2 ;;
        --fixture-dir) LARGE_TS_DIR="$2"; shift 2 ;;
        --tsconfig) TSCONFIG_OVERRIDE="$2"; shift 2 ;;
        --tsz-bin) TSZ_BIN="$2"; shift 2 ;;
        --iterations) ITERATIONS="$2"; shift 2 ;;
        --top) TOP="$2"; shift 2 ;;
        --timeout) TIMEOUT="$2"; shift 2 ;;
        --runs) RUNS="$2"; shift 2 ;;
        --prepare) PREPARE=true; shift ;;
        --run-measure) RUN_MEASURE=true; shift ;;
        --run-profile) RUN_PROFILE=true; shift ;;
        --help|-h) usage; exit 0 ;;
        *) echo "large-ts-repo-attribution-gauge: unknown option: $1" >&2; usage >&2; exit 2 ;;
    esac
done

for numeric in "$ITERATIONS" "$TOP" "$TIMEOUT" "$RUNS"; do
    if ! [[ "$numeric" =~ ^[0-9]+$ ]] || [ "$numeric" -le 0 ]; then
        echo "large-ts-repo-attribution-gauge: numeric options must be positive integers" >&2
        exit 2
    fi
done

if [[ "$PREPARE" == true ]]; then
    tsz_ensure_large_ts_repo_fixture "$LARGE_TS_DIR" "$LARGE_TS_REPO" "$LARGE_TS_REF" || exit 1
fi

TSCONFIG="$TSCONFIG_OVERRIDE"
if [ -z "$TSCONFIG" ]; then
    TSCONFIG="$(tsz_large_ts_repo_select_tsconfig "$LARGE_TS_DIR" 2>/dev/null || true)"
fi
FIXTURE_READY=false
if [ -d "$LARGE_TS_DIR/.git" ] && [ -n "$TSCONFIG" ] && [ -f "$TSCONFIG" ]; then
    FIXTURE_READY=true
fi

write_plan_json() {
    mkdir -p "$(dirname "$JSON_FILE")"
    TSZ_GAUGE_JSON_FILE="$JSON_FILE" \
    TSZ_GAUGE_PROJECT_ROOT="$PROJECT_ROOT" \
    TSZ_GAUGE_FIXTURE_DIR="$LARGE_TS_DIR" \
    TSZ_GAUGE_TSCONFIG="$TSCONFIG" \
    TSZ_GAUGE_FIXTURE_READY="$FIXTURE_READY" \
    TSZ_GAUGE_TSZ_BIN="$TSZ_BIN" \
    TSZ_GAUGE_NODE_OPTIONS="$LARGE_TS_NODE_OPTIONS" \
    TSZ_GAUGE_RUST_MIN_STACK="$TSZ_RUST_MIN_STACK" \
    TSZ_GAUGE_ITERATIONS="$ITERATIONS" \
    TSZ_GAUGE_TOP="$TOP" \
    TSZ_GAUGE_TIMEOUT="$TIMEOUT" \
    TSZ_GAUGE_RUNS="$RUNS" \
    TSZ_GAUGE_PREPARE="$PREPARE" \
    TSZ_GAUGE_RUN_MEASURE="$RUN_MEASURE" \
    TSZ_GAUGE_RUN_PROFILE="$RUN_PROFILE" \
    TSZ_GAUGE_MEASURE_EXIT_CODE="$MEASURE_EXIT_CODE" \
    TSZ_GAUGE_PROFILE_EXIT_CODE="$PROFILE_EXIT_CODE" \
    python3 <<'PY'
import json
import os
from pathlib import Path

root = Path(os.environ["TSZ_GAUGE_PROJECT_ROOT"])
json_file = Path(os.environ["TSZ_GAUGE_JSON_FILE"])
fixture_dir = os.environ["TSZ_GAUGE_FIXTURE_DIR"]
tsconfig = os.environ["TSZ_GAUGE_TSCONFIG"] or None
tsz_bin = os.environ["TSZ_GAUGE_TSZ_BIN"]
bench_json = str(json_file.with_suffix(".bench.json"))
measure_json = str(json_file.with_suffix(".measure.json"))
profile_json = str(json_file.with_suffix(".profile.json"))

def rel(path):
    try:
        return str(Path(path).resolve().relative_to(root.resolve()))
    except ValueError:
        return str(path)

def int_env(name):
    value = os.environ.get(name, "")
    return int(value) if value else None

commands = {
    "bench_row": [
        "scripts/bench/bench-vs-tsgo.sh",
        "--quick",
        "--filter",
        "^large-ts-repo$",
        "--json-file",
        bench_json,
    ],
    "measure": None,
    "profile": None,
}
if tsconfig:
    commands["measure"] = [
        "scripts/bench/measure-tsz.sh",
        "--bin",
        tsz_bin,
        "--timeout",
        os.environ["TSZ_GAUGE_TIMEOUT"],
        "--runs",
        os.environ["TSZ_GAUGE_RUNS"],
        "--json-file",
        measure_json,
        "--label",
        "large-ts-repo",
        "--",
        "--noEmit",
        "-p",
        tsconfig,
    ]
    commands["profile"] = [
        "scripts/bench/perf-flat-profile.sh",
        "--json-file",
        profile_json,
        "--no-build",
        "--bin",
        tsz_bin,
        "--iterations",
        os.environ["TSZ_GAUGE_ITERATIONS"],
        "--top",
        os.environ["TSZ_GAUGE_TOP"],
        "-p",
        tsconfig,
    ]

payload = {
    "schema_version": 1,
    "row": "large-ts-repo",
    "goal": "profile #14351 P2 evaluator-recursion vs variance attribution",
    "fixture": {
        "dir": fixture_dir,
        "ready": os.environ["TSZ_GAUGE_FIXTURE_READY"] == "true",
        "tsconfig": tsconfig,
        "source_dir": str(Path(fixture_dir) / "packages"),
        "repo_env": "LARGE_TS_REPO",
        "ref_env": "LARGE_TS_REF",
    },
    "environment": {
        "node_options": os.environ["TSZ_GAUGE_NODE_OPTIONS"],
        "rust_min_stack": os.environ["TSZ_GAUGE_RUST_MIN_STACK"],
        "tsz_bin": tsz_bin,
        "tsz_bin_exists": Path(tsz_bin).is_file(),
    },
    "settings": {
        "iterations": int(os.environ["TSZ_GAUGE_ITERATIONS"]),
        "top": int(os.environ["TSZ_GAUGE_TOP"]),
        "timeout_s": int(os.environ["TSZ_GAUGE_TIMEOUT"]),
        "runs": int(os.environ["TSZ_GAUGE_RUNS"]),
    },
    "artifacts": {
        "bench_json": bench_json,
        "measure_json": measure_json if tsconfig else None,
        "profile_json": profile_json if tsconfig else None,
    },
    "commands": commands,
    "run": {
        "prepare_requested": os.environ["TSZ_GAUGE_PREPARE"] == "true",
        "measure_requested": os.environ["TSZ_GAUGE_RUN_MEASURE"] == "true",
        "profile_requested": os.environ["TSZ_GAUGE_RUN_PROFILE"] == "true",
        "measure_exit_code": int_env("TSZ_GAUGE_MEASURE_EXIT_CODE"),
        "profile_exit_code": int_env("TSZ_GAUGE_PROFILE_EXIT_CODE"),
    },
}

json_file.parent.mkdir(parents=True, exist_ok=True)
json_file.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf8")
print(f"large-ts-repo attribution plan written to {rel(json_file)}")
PY
}

write_plan_json

if [[ "$RUN_MEASURE" == false && "$RUN_PROFILE" == false ]]; then
    exit 0
fi
if [ -z "$TSCONFIG" ] || [ ! -f "$TSCONFIG" ]; then
    echo "large-ts-repo-attribution-gauge: no flat tsconfig found; rerun with --prepare or --tsconfig" >&2
    exit 2
fi
if [ ! -x "$TSZ_BIN" ]; then
    echo "large-ts-repo-attribution-gauge: tsz binary not executable: $TSZ_BIN" >&2
    exit 2
fi

RUN_EXIT=0
if [[ "$RUN_MEASURE" == true ]]; then
    set +e
    NODE_OPTIONS="$LARGE_TS_NODE_OPTIONS" RUST_MIN_STACK="$TSZ_RUST_MIN_STACK" \
        "$SCRIPT_DIR/measure-tsz.sh" \
        --bin "$TSZ_BIN" \
        --timeout "$TIMEOUT" \
        --runs "$RUNS" \
        --json-file "${JSON_FILE%.json}.measure.json" \
        --label "large-ts-repo" \
        -- --noEmit -p "$TSCONFIG"
    MEASURE_EXIT_CODE=$?
    set -e
    if [ "$MEASURE_EXIT_CODE" -ne 0 ] && [ "$RUN_EXIT" -eq 0 ]; then
        RUN_EXIT="$MEASURE_EXIT_CODE"
    fi
fi

if [[ "$RUN_PROFILE" == true ]]; then
    set +e
    NODE_OPTIONS="$LARGE_TS_NODE_OPTIONS" RUST_MIN_STACK="$TSZ_RUST_MIN_STACK" \
        "$SCRIPT_DIR/perf-flat-profile.sh" \
        --json-file "${JSON_FILE%.json}.profile.json" \
        --no-build \
        --bin "$TSZ_BIN" \
        --iterations "$ITERATIONS" \
        --top "$TOP" \
        -p "$TSCONFIG"
    PROFILE_EXIT_CODE=$?
    set -e
    if [ "$PROFILE_EXIT_CODE" -ne 0 ] && [ "$RUN_EXIT" -eq 0 ]; then
        RUN_EXIT="$PROFILE_EXIT_CODE"
    fi
fi

write_plan_json
exit "$RUN_EXIT"
