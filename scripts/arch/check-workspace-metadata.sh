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

extract_package_field() {
  local cargo_toml="$1"
  local field="$2"

  awk -v key="$field" '
    BEGIN { in_package = 0 }
    /^\[package\]/ { in_package = 1; next }
    /^\[/ { if (in_package) exit }
    !in_package { next }

    $0 ~ "^[[:space:]]*" key "\\.workspace[[:space:]]*=[[:space:]]*true[[:space:]]*$" {
      print "__WORKSPACE__"
      exit
    }

    $0 ~ "^[[:space:]]*" key "[[:space:]]*=" {
      if (match($0, /"[^"]+"/)) {
        print substr($0, RSTART + 1, RLENGTH - 2)
      }
      exit
    }
  ' "$cargo_toml"
}

failures=()

workspace_repository="$(extract_workspace_package_field "repository")"
workspace_homepage="$(extract_workspace_package_field "homepage")"

if [[ "$workspace_repository" != "$expected_url" ]]; then
  failures+=("workspace.package.repository mismatch (expected: $expected_url, actual: ${workspace_repository:-<missing>})")
fi

if [[ "$workspace_homepage" != "$expected_url" ]]; then
  failures+=("workspace.package.homepage mismatch (expected: $expected_url, actual: ${workspace_homepage:-<missing>})")
fi

validate_crate_field() {
  local cargo_toml="$1"
  local field="$2"
  local value="$3"

  if [[ -z "$value" ]]; then
    failures+=("$cargo_toml: missing package.$field (set $field.workspace = true or $field = \"$expected_url\")")
    return
  fi

  if [[ "$value" == "__WORKSPACE__" ]]; then
    return
  fi

  if [[ "$value" != "$expected_url" ]]; then
    failures+=("$cargo_toml: package.$field mismatch (expected workspace value $expected_url, actual: $value)")
  fi
}

for cargo_toml in crates/*/Cargo.toml; do
  [[ -f "$cargo_toml" ]] || continue

  crate_repository="$(extract_package_field "$cargo_toml" "repository")"
  crate_homepage="$(extract_package_field "$cargo_toml" "homepage")"

  validate_crate_field "$cargo_toml" "repository" "$crate_repository"
  validate_crate_field "$cargo_toml" "homepage" "$crate_homepage"
done

if [[ "${#failures[@]}" -gt 0 ]]; then
  echo "error: workspace metadata validation failed." >&2
  for failure in "${failures[@]}"; do
    echo "  - $failure" >&2
  done
  exit 1
fi

echo "workspace package and crate metadata match origin URL: $expected_url"
