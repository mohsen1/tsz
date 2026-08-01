#!/bin/bash
# Publish helpers for .github/workflows/npm-publish.yml.
#
# This file only defines functions; sourcing it has no side effects, so the
# behaviour below can be exercised directly by test-npm-publish-lib.sh with a
# stub `npm` on PATH instead of only ever running inside a release.
#
# Background (#16126): npm-publish.yml reported success for two months while
# publishing nothing. Three separate mechanisms hid it, and each has a
# counterpart here:
#
#   1. `npm publish --dry-run` never performs the PUT that fails, so the
#      pack-check step structurally could not catch an auth problem.
#      -> npm_auth_preflight asserts a usable credential up front.
#   2. `publish_if_needed` treated *any* failing `npm view` as "not published
#      yet", so the lookup failure that signalled the problem also disarmed the
#      guard that would have reported it.
#      -> npm_registry_version_state separates "absent" from "unknown", and
#         only "absent" is allowed to proceed to a publish.
#   3. Nothing ever asserted that the versions actually landed.
#      -> npm_verify_published re-reads every package from the registry.

# Registry the release lane publishes to. Overridable for tests.
NPM_REGISTRY="${NPM_REGISTRY:-https://registry.npmjs.org}"

# Read a field out of a package directory's package.json without needing node's
# module resolution to agree with the caller's cwd.
npm_pkg_field() {
  local pkg_dir="$1" field="$2"
  node -p "JSON.parse(require('fs').readFileSync('${pkg_dir}/package.json','utf8')).${field}"
}

# `actions/setup-node` with `registry-url:` writes an .npmrc containing
#     //registry.npmjs.org/:_authToken=${NODE_AUTH_TOKEN}
# and npm expands that variable when it reads the file. When NODE_AUTH_TOKEN is
# unset the line still counts as a configured credential for the registry, so
# npm sends an empty bearer token and never attempts the OIDC exchange that
# trusted publishing needs -- and an unauthenticated PUT to a package that
# already exists is answered 404, not 403.
#
# Strip only the literal, unexpanded placeholder, and only when the variable is
# genuinely unset. A real token is never touched.
npm_clear_placeholder_auth() {
  local npmrc="${1:-${NPM_CONFIG_USERCONFIG:-$HOME/.npmrc}}"
  if [ -n "${NODE_AUTH_TOKEN:-}" ]; then
    return 0
  fi
  if [ ! -f "$npmrc" ]; then
    return 0
  fi
  if ! grep -q ':_authToken=\${NODE_AUTH_TOKEN}' "$npmrc"; then
    return 0
  fi
  echo "npmrc: dropping unexpanded _authToken placeholder so OIDC can run ($npmrc)"
  local tmp
  tmp="$(mktemp)"
  grep -v ':_authToken=\${NODE_AUTH_TOKEN}' "$npmrc" >"$tmp" || true
  mv "$tmp" "$npmrc"
}

# Fail loudly, before any package is touched, when the job has no way to
# authenticate. Without this the first symptom is an E404 on a PUT, which reads
# like a missing package rather than a missing credential.
npm_auth_preflight() {
  if [ -n "${NODE_AUTH_TOKEN:-}" ]; then
    echo "npm auth: using NODE_AUTH_TOKEN"
    return 0
  fi
  if [ -n "${ACTIONS_ID_TOKEN_REQUEST_URL:-}" ]; then
    echo "npm auth: no NODE_AUTH_TOKEN; relying on OIDC trusted publishing"
    return 0
  fi
  cat >&2 <<'EOF'
npm auth: no usable credential.

  NODE_AUTH_TOKEN is empty and no OIDC id-token is available
  (ACTIONS_ID_TOKEN_REQUEST_URL is unset).

  npm would answer the publish PUT with a 404 that looks like a missing
  package rather than a missing credential -- see #16126.

Fix one of:
  * grant the job `id-token: write` and register the trusted publisher
    (scripts/build/setup-npm-trusted-publishers.sh), or
  * set NODE_AUTH_TOKEN from a repository secret on the publish step.
EOF
  return 1
}

# Echo one of: published | absent | unknown
#
# The distinction is the point. `npm view` failing because the registry is
# unreachable, or because the caller is unauthenticated, is NOT evidence that a
# version needs publishing.
npm_registry_version_state() {
  local name="$1" version="$2"
  local out status
  set +e
  out="$(npm view "${name}@${version}" version --registry "$NPM_REGISTRY" 2>&1)"
  status=$?
  set -e

  if [ "$status" -eq 0 ]; then
    # An existing package with a missing version exits 0 and prints nothing.
    if [ -n "$(printf '%s' "$out" | tr -d '[:space:]')" ]; then
      echo published
    else
      echo absent
    fi
    return 0
  fi

  # E404 is the registry positively reporting "no such package or version".
  if printf '%s' "$out" | grep -qE 'E404|404 Not Found'; then
    echo absent
    return 0
  fi

  echo "npm view ${name}@${version} failed (exit ${status}):" >&2
  printf '%s\n' "$out" >&2
  echo unknown
}

npm_publish_if_needed() {
  local pkg_dir="$1"
  local name version state
  name="$(npm_pkg_field "$pkg_dir" name)"
  version="$(npm_pkg_field "$pkg_dir" version)"

  state="$(npm_registry_version_state "$name" "$version")"
  case "$state" in
    published)
      echo "${name}@${version} already exists; skipping"
      return 0
      ;;
    absent)
      echo "Publishing ${name}@${version}"
      (cd "$pkg_dir" && npm publish --access public --provenance)
      ;;
    *)
      echo "Refusing to publish ${name}@${version}: registry lookup was inconclusive." >&2
      echo "Treating an unreadable registry as 'needs publishing' is what hid #16126." >&2
      return 1
      ;;
  esac
}

# The guard that would have caught #16126 on its first occurrence: after the
# publish loop, every package must actually be readable at its own version.
npm_verify_published() {
  local failed=0 pkg_dir name version state
  for pkg_dir in "$@"; do
    name="$(npm_pkg_field "$pkg_dir" name)"
    version="$(npm_pkg_field "$pkg_dir" version)"
    state="$(npm_registry_version_state "$name" "$version")"
    if [ "$state" = "published" ]; then
      echo "verified ${name}@${version}"
    else
      echo "NOT PUBLISHED: ${name}@${version} (registry says: ${state})" >&2
      failed=1
    fi
  done
  if [ "$failed" -ne 0 ]; then
    echo "Release did not publish every package; failing the job." >&2
    return 1
  fi
  echo "all packages verified on ${NPM_REGISTRY}"
}
