#!/usr/bin/env bash
# Run the active three-crate rewrite test suite as one strict nextest pass.
#
# The legacy black-box corpus is intentionally not a Cargo test population.
# Once a case is ported to the public service/CLI surface it joins this command
# and must pass; there is no known-failures baseline for active rewrite tests.
#
# Usage:
#   scripts/ci/unit-nextest.sh --junit-dir DIR \
#     ( --packages "pkg [pkg...]" | --workspace ) [--allow-no-reports]

set -Eeuo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

usage() {
  sed -n '2,/^$/p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

JUNIT_DIR=""
PACKAGES=""
WORKSPACE=0
ALLOW_NO_REPORTS=0

while [[ "$#" -gt 0 ]]; do
  case "$1" in
    --junit-dir)
      JUNIT_DIR="${2:?--junit-dir needs a value}"
      shift 2
      ;;
    --packages)
      PACKAGES="${2:?--packages needs a value}"
      shift 2
      ;;
    --workspace)
      WORKSPACE=1
      shift
      ;;
    --allow-no-reports)
      ALLOW_NO_REPORTS=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown argument '$1'" >&2
      exit 2
      ;;
  esac
done

if [[ -z "$JUNIT_DIR" ]]; then
  echo "error: --junit-dir is required" >&2
  exit 2
fi
if [[ "$WORKSPACE" == "1" && -n "$PACKAGES" ]]; then
  echo "error: --packages and --workspace are mutually exclusive" >&2
  exit 2
fi
if [[ "$WORKSPACE" == "0" && -z "$PACKAGES" ]]; then
  echo "error: one of --packages or --workspace is required" >&2
  exit 2
fi

allowed_packages=" tsz-core tsz-cli tsz-conformance "
nextest_args=()
if [[ "$WORKSPACE" == "1" ]]; then
  nextest_args+=(--workspace)
else
  package=""
  for package in $PACKAGES; do
    if [[ "$allowed_packages" != *" $package "* ]]; then
      echo "error: package '$package' is not in the active rewrite workspace" >&2
      exit 2
    fi
    nextest_args+=(-p "$package")
  done
fi

if [[ -n "${CARGO_BUILD_JOBS:-}" ]]; then
  nextest_args+=(--build-jobs "$CARGO_BUILD_JOBS")
fi
if [[ -n "${UNIT_NEXTEST_TEST_THREADS:-}" ]]; then
  nextest_args+=(--test-threads "$UNIT_NEXTEST_TEST_THREADS")
fi

mkdir -p "$JUNIT_DIR"
junit_candidates=(
  "$ROOT_DIR/target/nextest/signoff/junit.xml"
  "$ROOT_DIR/.target/nextest/signoff/junit.xml"
)
rm -f "${junit_candidates[@]}"

set +e
cargo nextest run --profile signoff --cargo-profile ci-unit "${nextest_args[@]}"
rc="$?"
set -e

junit_source=""
candidate=""
for candidate in "${junit_candidates[@]}"; do
  if [[ -f "$candidate" ]]; then
    junit_source="$candidate"
    break
  fi
done
if [[ -n "$junit_source" ]]; then
  cp "$junit_source" "$JUNIT_DIR/rewrite.xml"
fi

if [[ "$rc" == "4" && "$ALLOW_NO_REPORTS" == "1" ]]; then
  echo "unit-nextest: selection contained no active tests"
  exit 0
fi
if [[ "$rc" != "0" ]]; then
  echo "error: active rewrite tests failed (nextest rc=$rc)" >&2
  exit "$rc"
fi
if [[ -z "$junit_source" ]]; then
  echo "error: nextest passed but did not produce its configured junit report" >&2
  exit 3
fi

echo "unit-nextest: strict rewrite suite passed; junit=$JUNIT_DIR/rewrite.xml"
