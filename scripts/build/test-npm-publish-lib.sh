#!/bin/bash
# Exercise scripts/build/npm-publish-lib.sh against a stub `npm`.
#
# The release lane's publish step has no local suite -- that is precisely why
# #16126 went unnoticed for 43 versions. Every behaviour asserted here is one
# that a release would otherwise only exercise in production.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/build/npm-publish-lib.sh
source "$SCRIPT_DIR/npm-publish-lib.sh"

tmp_root="$(mktemp -d)"
trap 'rm -rf "$tmp_root"' EXIT

stub_bin="$tmp_root/bin"
mkdir -p "$stub_bin"
PATH="$stub_bin:$PATH"
export PATH

pass=0
fail=0

check() {
  local label="$1" expected="$2" actual="$3"
  if [ "$expected" = "$actual" ]; then
    pass=$((pass + 1))
    echo "  ok   $label"
  else
    fail=$((fail + 1))
    echo "  FAIL $label: expected '$expected', got '$actual'"
  fi
}

# Install a stub `npm` whose `view` behaviour is driven by NPM_STUB_MODE and
# whose `publish` invocations are appended to $tmp_root/published.
install_npm_stub() {
  cat >"$stub_bin/npm" <<'STUB'
#!/bin/bash
case "$1" in
  view)
    case "${NPM_STUB_MODE:-published}" in
      published)     echo "1.2.3"; exit 0 ;;
      missing-ver)   exit 0 ;;                                   # package exists, version does not
      e404)          echo "npm error code E404" >&2; exit 1 ;;
      unauthorized)  echo "npm error code E401 Unauthorized" >&2; exit 1 ;;
      network)       echo "npm error network ETIMEDOUT" >&2; exit 1 ;;
    esac
    ;;
  publish)
    echo "$PWD" >>"$NPM_STUB_PUBLISH_LOG"
    exit "${NPM_STUB_PUBLISH_STATUS:-0}"
    ;;
  whoami)
    # Every invocation is recorded so a test can assert the OIDC path does NOT
    # reach for an ambient credential that trusted publishing does not have.
    [ -n "${NPM_STUB_WHOAMI_LOG:-}" ] && echo called >>"$NPM_STUB_WHOAMI_LOG"
    if [ "${NPM_STUB_WHOAMI_STATUS:-0}" -eq 0 ]; then
      echo "${NPM_STUB_WHOAMI_USER:-stub-user}"
      exit 0
    fi
    echo "npm error code E401 Unauthorized" >&2
    exit 1
    ;;
esac
exit 0
STUB
  chmod +x "$stub_bin/npm"
}
install_npm_stub

export NPM_STUB_PUBLISH_LOG="$tmp_root/published"
: >"$NPM_STUB_PUBLISH_LOG"

make_pkg() {
  local dir="$tmp_root/$1" name="$2" version="$3"
  mkdir -p "$dir"
  printf '{"name":"%s","version":"%s"}\n' "$name" "$version" >"$dir/package.json"
  echo "$dir"
}

echo "npm_registry_version_state"
for mode_expect in "published:published" "missing-ver:absent" "e404:absent" \
                   "unauthorized:unknown" "network:unknown"; do
  mode="${mode_expect%%:*}"
  expect="${mode_expect##*:}"
  actual="$(NPM_STUB_MODE="$mode" npm_registry_version_state pkg 1.0.0 2>/dev/null)"
  check "$mode -> $expect" "$expect" "$actual"
done

echo
echo "npm_publish_if_needed"
pkg="$(make_pkg pkgA @scope/thing 0.1.59)"

: >"$NPM_STUB_PUBLISH_LOG"
NPM_STUB_MODE=published npm_publish_if_needed "$pkg" >/dev/null 2>&1
check "already published -> no publish" "0" "$(wc -l <"$NPM_STUB_PUBLISH_LOG" | tr -d ' ')"

: >"$NPM_STUB_PUBLISH_LOG"
NPM_STUB_MODE=e404 npm_publish_if_needed "$pkg" >/dev/null 2>&1
check "absent -> publishes once" "1" "$(wc -l <"$NPM_STUB_PUBLISH_LOG" | tr -d ' ')"

# The #16126 regression guard: an unreadable registry must NOT be read as
# "needs publishing". Before this fix the loop blundered into a doomed PUT.
: >"$NPM_STUB_PUBLISH_LOG"
NPM_STUB_MODE=unauthorized npm_publish_if_needed "$pkg" >/dev/null 2>&1
check "unauthorized -> refuses to publish" "0" "$(wc -l <"$NPM_STUB_PUBLISH_LOG" | tr -d ' ')"
NPM_STUB_MODE=unauthorized npm_publish_if_needed "$pkg" >/dev/null 2>&1
check "unauthorized -> non-zero exit" "1" "$?"

echo
echo "npm_verify_published"
pkg2="$(make_pkg pkgB @scope/other 0.1.59)"
NPM_STUB_MODE=published npm_verify_published "$pkg" "$pkg2" >/dev/null 2>&1
check "all present -> exit 0" "0" "$?"
NPM_STUB_MODE=e404 npm_verify_published "$pkg" "$pkg2" >/dev/null 2>&1
check "any absent -> exit 1" "1" "$?"
NPM_STUB_MODE=network npm_verify_published "$pkg" >/dev/null 2>&1
check "unreadable -> exit 1" "1" "$?"

echo
echo "npm_auth_preflight"
( NODE_AUTH_TOKEN=tok; unset ACTIONS_ID_TOKEN_REQUEST_URL; npm_auth_preflight >/dev/null 2>&1 )
check "token present -> ok" "0" "$?"
( unset NODE_AUTH_TOKEN; ACTIONS_ID_TOKEN_REQUEST_URL=https://example.invalid; npm_auth_preflight >/dev/null 2>&1 )
check "oidc available -> ok" "0" "$?"
( unset NODE_AUTH_TOKEN; unset ACTIONS_ID_TOKEN_REQUEST_URL; npm_auth_preflight >/dev/null 2>&1 )
check "neither -> fails" "1" "$?"

# A token that is SET but REJECTED is the second phase of #16126: the preflight
# passed on presence alone, provenance signed, and only the PUT failed -- with a
# 404 that reads as a missing package rather than an expired credential.
( NODE_AUTH_TOKEN=tok; export NPM_STUB_WHOAMI_STATUS=1
  unset ACTIONS_ID_TOKEN_REQUEST_URL
  npm_auth_preflight >/dev/null 2>&1 )
check "token set but registry rejects it -> fails" "1" "$?"

# The failure must name the real cause, not just exit non-zero: a job log that
# says 404 sends the next reader hunting for a missing package.
rejected_msg="$( ( NODE_AUTH_TOKEN=tok; export NPM_STUB_WHOAMI_STATUS=1
                   unset ACTIONS_ID_TOKEN_REQUEST_URL
                   npm_auth_preflight 2>&1 >/dev/null ) )"
case "$rejected_msg" in
  *"registry rejected it"*) check "rejection message names the credential" "0" "0" ;;
  *) check "rejection message names the credential" "0" "1 (got: ${rejected_msg:0:60})" ;;
esac

# The accepted path should report WHICH account the registry sees, so a token
# scoped to the wrong account is visible in the log without a second run.
accepted_msg="$( ( NODE_AUTH_TOKEN=tok; export NPM_STUB_WHOAMI_USER=release-bot
                   unset ACTIONS_ID_TOKEN_REQUEST_URL
                   npm_auth_preflight 2>/dev/null ) )"
case "$accepted_msg" in
  *release-bot*) check "accepted path reports the account" "0" "0" ;;
  *) check "accepted path reports the account" "0" "1 (got: ${accepted_msg:0:60})" ;;
esac

# Under OIDC there is no ambient credential, so probing for one would fail for a
# reason that is not a problem. The token probe must stay on the token path.
export NPM_STUB_WHOAMI_LOG="$tmp_root/whoami-calls"
: >"$NPM_STUB_WHOAMI_LOG"
( unset NODE_AUTH_TOKEN; ACTIONS_ID_TOKEN_REQUEST_URL=https://example.invalid
  npm_auth_preflight >/dev/null 2>&1 )
check "oidc path does not probe whoami" "0" "$(wc -l <"$NPM_STUB_WHOAMI_LOG" | tr -d ' ')"

: >"$NPM_STUB_WHOAMI_LOG"
( NODE_AUTH_TOKEN=tok; unset ACTIONS_ID_TOKEN_REQUEST_URL; npm_auth_preflight >/dev/null 2>&1 )
check "token path does probe whoami" "1" "$(wc -l <"$NPM_STUB_WHOAMI_LOG" | tr -d ' ')"
unset NPM_STUB_WHOAMI_LOG

echo
echo "npm_clear_placeholder_auth"
npmrc="$tmp_root/.npmrc"

printf '//registry.npmjs.org/:_authToken=${NODE_AUTH_TOKEN}\nregistry=https://registry.npmjs.org/\n' >"$npmrc"
( unset NODE_AUTH_TOKEN; npm_clear_placeholder_auth "$npmrc" >/dev/null 2>&1 )
check "unset token -> placeholder stripped" "0" "$(grep -c '_authToken' "$npmrc")"
check "unset token -> registry line kept" "1" "$(grep -c '^registry=' "$npmrc")"

printf '//registry.npmjs.org/:_authToken=${NODE_AUTH_TOKEN}\n' >"$npmrc"
( NODE_AUTH_TOKEN=real npm_clear_placeholder_auth "$npmrc" >/dev/null 2>&1 )
check "token set -> file untouched" "1" "$(grep -c '_authToken' "$npmrc")"

printf '//registry.npmjs.org/:_authToken=npm_realtokenvalue\n' >"$npmrc"
( unset NODE_AUTH_TOKEN; npm_clear_placeholder_auth "$npmrc" >/dev/null 2>&1 )
check "literal token -> never stripped" "1" "$(grep -c '_authToken' "$npmrc")"

echo
echo "passed: $pass, failed: $fail"
[ "$fail" -eq 0 ]
