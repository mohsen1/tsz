#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

normalize_github_url() {
  local raw="$1"
  local url="$raw"

  if [[ "$url" =~ ^git@github\.com: ]]; then
    url="https://github.com/${url#git@github.com:}"
  elif [[ "$url" =~ ^ssh://git@github\.com/ ]]; then
    url="https://github.com/${url#ssh://git@github.com/}"
  fi

  url="${url%.git}"
  url="${url%/}"
  printf '%s\n' "$url"
}

origin_url="$(git config --get remote.origin.url || true)"
if [[ -z "$origin_url" ]]; then
  echo "error: remote.origin.url is not configured" >&2
  exit 1
fi

expected_url="$(normalize_github_url "$origin_url")"

extract_workspace_package_field() {
  local field="$1"
  awk -F '"' -v key="$field" '
    BEGIN { in_workspace_pkg = 0 }
    /^\[workspace\.package\]/ { in_workspace_pkg = 1; next }
    /^\[/ { if (in_workspace_pkg) exit }
    in_workspace_pkg && $1 ~ "^[[:space:]]*" key "[[:space:]]*=" {
      print $2
      exit
    }
  ' Cargo.toml
}

repository="$(extract_workspace_package_field "repository")"
homepage="$(extract_workspace_package_field "homepage")"

if [[ "$repository" != "$expected_url" ]]; then
  echo "error: workspace.package.repository mismatch" >&2
  echo "expected: $expected_url" >&2
  echo "actual:   $repository" >&2
  exit 1
fi

if [[ "$homepage" != "$expected_url" ]]; then
  echo "error: workspace.package.homepage mismatch" >&2
  echo "expected: $expected_url" >&2
  echo "actual:   $homepage" >&2
  exit 1
fi

echo "workspace package metadata matches origin URL: $expected_url"
