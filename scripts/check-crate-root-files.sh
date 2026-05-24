#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

THRESHOLD="${TSZ_CRATE_ROOT_FILE_THRESHOLD:-4}"
INPUT_BASE="${1:-${TSZ_CRATE_ROOT_FILE_BASE:-}}"

ensure_commit_available() {
  local ref="$1"

  if git rev-parse --verify "${ref}^{commit}" >/dev/null 2>&1; then
    return 0
  fi

  if git remote get-url origin >/dev/null 2>&1; then
    git fetch --no-tags --depth=1 origin "$ref" >/dev/null 2>&1 || true
  fi

  git rev-parse --verify "${ref}^{commit}" >/dev/null 2>&1
}

github_pull_request_base_sha() {
  if [[ -z "${GITHUB_EVENT_PATH:-}" || ! -f "${GITHUB_EVENT_PATH}" ]]; then
    return 0
  fi

  python3 - "$GITHUB_EVENT_PATH" <<'PY'
import json
import sys

try:
    with open(sys.argv[1], encoding="utf-8") as event_file:
        event = json.load(event_file)
except Exception:
    sys.exit(0)

base_sha = event.get("pull_request", {}).get("base", {}).get("sha")
if base_sha:
    print(base_sha)
PY
}

resolve_diff_base() {
  local candidate="$1"
  local resolved=""

  if [[ -n "$candidate" ]] && ensure_commit_available "$candidate"; then
    resolved="$(git rev-parse --verify "${candidate}^{commit}")"
  fi

  if [[ -z "$resolved" ]]; then
    local pr_base_sha
    pr_base_sha="$(github_pull_request_base_sha)"
    if [[ -n "$pr_base_sha" ]] && ensure_commit_available "$pr_base_sha"; then
      resolved="$(git rev-parse --verify "${pr_base_sha}^{commit}")"
    fi
  fi

  local base_ref="${GITHUB_BASE_REF:-main}"
  if [[ -z "$resolved" && -n "${GITHUB_BASE_REF:-}" ]] && git remote get-url origin >/dev/null 2>&1; then
    git fetch --no-tags --depth=1 origin \
      "${GITHUB_BASE_REF}:refs/remotes/origin/${GITHUB_BASE_REF}" >/dev/null 2>&1 || true
  fi

  if [[ -z "$resolved" ]] && git rev-parse --verify "origin/${base_ref}^{commit}" >/dev/null 2>&1; then
    resolved="$(git merge-base HEAD "origin/${base_ref}" 2>/dev/null || true)"
    if [[ -z "$resolved" ]]; then
      resolved="$(git rev-parse --verify "origin/${base_ref}^{commit}")"
    fi
  elif [[ -z "$resolved" ]] && git rev-parse --verify "${base_ref}^{commit}" >/dev/null 2>&1; then
    resolved="$(git merge-base HEAD "$base_ref" 2>/dev/null || true)"
    if [[ -z "$resolved" ]]; then
      resolved="$(git rev-parse --verify "${base_ref}^{commit}")"
    fi
  elif [[ -z "$resolved" ]]; then
    resolved="$(git rev-list --max-parents=0 HEAD 2>/dev/null | tail -n1 || true)"
  fi

  printf '%s\n' "$resolved"
}

BASE_REF="$(resolve_diff_base "$INPUT_BASE")"
if [[ -z "$BASE_REF" ]]; then
  echo "error: could not determine a diff base for root-file checks" >&2
  exit 1
fi

ADDED_ROOT_RS_FILES=()
while IFS= read -r file_path; do
  [[ -z "$file_path" ]] && continue
  ADDED_ROOT_RS_FILES+=("$file_path")
done < <(
  git diff --name-only --diff-filter=A "${BASE_REF}..HEAD" \
    | grep -E '^crates/[^/]+/src/[^/]+\.rs$' \
    | grep -Ev '/(lib|main)\.rs$' \
    || true
)

if [[ "${#ADDED_ROOT_RS_FILES[@]}" -eq 0 ]]; then
  echo "crate-root check: no new root-level crate modules detected"
  exit 0
fi

failures=()
for file_path in "${ADDED_ROOT_RS_FILES[@]}"; do
  crate_name="$(echo "$file_path" | cut -d/ -f2)"
  crate_src_dir="$ROOT_DIR/crates/$crate_name/src"

  if [[ ! -d "$crate_src_dir" ]]; then
    continue
  fi

  non_facade_count="$(
    find "$crate_src_dir" -maxdepth 1 -type f -name '*.rs' ! -name 'lib.rs' ! -name 'main.rs' \
      | wc -l | tr -d ' '
  )"

  if [[ "$non_facade_count" -gt "$THRESHOLD" ]]; then
    failures+=(
      "$file_path (crate: $crate_name, root non-facade files: $non_facade_count, threshold: $THRESHOLD)"
    )
  fi
done

if [[ "${#failures[@]}" -gt 0 ]]; then
  echo "error: root-file policy violation(s) detected." >&2
  echo "new root-level modules are not allowed once a crate exceeds $THRESHOLD non-facade root files." >&2
  for failure in "${failures[@]}"; do
    echo "  - $failure" >&2
  done
  echo "move new modules into a domain folder (for example: api/, core/, passes/, diagnostics/)." >&2
  exit 1
fi

echo "crate-root check passed (${#ADDED_ROOT_RS_FILES[@]} new root-level file(s) inspected)."
