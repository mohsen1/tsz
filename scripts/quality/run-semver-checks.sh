#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

baseline_rev="${TSZ_SEMVER_BASELINE_REV:-}"
packages=(${TSZ_SEMVER_PACKAGES:-tsz-common tsz-scanner tsz-parser tsz-binder tsz-solver tsz-lowering tsz-checker tsz-emitter tsz-lsp tsz-core tsz-cli})

if [[ -z "$baseline_rev" ]]; then
  baseline_rev="$(git tag --merged HEAD --list 'v[0-9]*' --sort=-v:refname | head -n1)"
fi

if [[ -z "$baseline_rev" ]]; then
  echo "No SemVer baseline found. Fetch tags or set TSZ_SEMVER_BASELINE_REV." >&2
  exit 2
fi

if ! git rev-parse --verify "${baseline_rev}^{commit}" >/dev/null 2>&1; then
  echo "SemVer baseline '${baseline_rev}' is not available. Fetch history or set TSZ_SEMVER_BASELINE_REV." >&2
  exit 2
fi

if [[ "$(git rev-parse HEAD)" == "$(git rev-parse "${baseline_rev}^{commit}")" ]]; then
  echo "Current HEAD matches SemVer baseline '${baseline_rev}'; nothing to compare."
  exit 0
fi

for package in "${packages[@]}"; do
  echo "==> SemVer: ${package} vs ${baseline_rev}"
  scripts/safe-run.sh --limit "${TSZ_SEMVER_MEMORY_LIMIT:-75%}" -- \
    cargo semver-checks --package "$package" --baseline-rev "$baseline_rev"
done
