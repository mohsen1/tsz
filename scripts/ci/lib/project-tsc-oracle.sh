# shellcheck shell=bash
# Per-row tsc oracle helpers for scripts/ci/project-compile-guard.sh.
#
# This file is sourced (never executed) by the compile guard and by
# scripts/ci/test-project-tsc-oracle.mjs so the tsz-only delta logic and the
# oracle-cache key have a single, unit-tested home.
#
# Why a per-row tsc oracle exists: the project-compile guard runs only tsz and,
# historically, counted every tsz diagnostic as a tsz false positive. That is
# only correct when the fixture is tsc-clean. Some corpus rows have *genuine*
# tsc errors in their own source (e.g. neverthrow, superstruct), so at perfect
# tsz<->tsc parity those rows still emitted diagnostics and could never pass.
#
# The oracle subtracts tsc's own diagnostics from tsz's: a row passes the gate
# when tsz's diagnostics MATCH tsc's (an empty tsz-only delta). tsc-clean rows
# have an empty tsc side, so the delta equals tsz's full output and the gate is
# unchanged (a no-op) for the required rows. Rows where tsc also errors only
# need tsz to agree on the tsc-flagged locations.
#
# Diagnostic identity is (basename, line, column, code): tsz and tsc both
# receive the *same* guard tsconfig, so a diagnostic they agree on shares that
# 4-tuple. We match on location+code rather than message text so wording
# differences between the two compilers never manufacture or hide a delta, and
# we basename-normalize the path so an absolute-vs-relative emit difference
# between the compilers does not split an otherwise-identical diagnostic.
#
# Portability: every awk program here stays within POSIX awk (no gawk-only
# 3-argument match()/gensym extensions) so the helpers behave identically under
# the Linux gawk in CI and the BSD awk used for local verification on macOS.

# The shared awk parser body. Defines a `parse(line)` function that sets the
# globals _ok/_base/_line/_col/_code for a recognized diagnostic line, or
# _ok=0 otherwise. Recognizes both formatter shapes tsc/tsz emit:
#   path(line,col): error TSnnnn        (default, used when output is piped)
#   path:line:col - error TSnnnn        (pretty formatter)
# Kept in one string so the identity-key and delta programs cannot drift.
_TSZ_ORACLE_AWK_PARSER='
function basename(p,   n, parts) {
  n = split(p, parts, "/")
  return parts[n]
}
function parse(line,   loc, before, a, code) {
  _ok = 0
  # path(line,col): (error|warning) TSnnnn
  if (match(line, /\([0-9]+,[0-9]+\): (error|warning) TS[0-9]+/)) {
    loc = substr(line, RSTART, RLENGTH)
    before = substr(line, 1, RSTART - 1)
    split(loc, a, /[(,)]/)   # a[2]=line a[3]=col
    if (!match(loc, /TS[0-9]+/)) return
    code = substr(loc, RSTART, RLENGTH)
    _base = basename(before); _line = a[2]; _col = a[3]; _code = code; _ok = 1
    return
  }
  # path:line:col - (error|warning) TSnnnn  (pretty formatter)
  if (match(line, /:[0-9]+:[0-9]+ - (error|warning) TS[0-9]+/)) {
    loc = substr(line, RSTART, RLENGTH)
    before = substr(line, 1, RSTART - 1)
    split(loc, a, /[: ]/)    # a[2]=line a[3]=col
    if (!match(loc, /TS[0-9]+/)) return
    code = substr(loc, RSTART, RLENGTH)
    _base = basename(before); _line = a[2]; _col = a[3]; _code = code; _ok = 1
    return
  }
}
'

# Resolve the exact pinned npm tsc for the oracle. The legacy TypeScript corpus
# submodule does not contain the TypeScript 7 native compiler, and arbitrary
# top-level node_modules shims may point at a different release. The shared
# scripts installation is prepared by ensure-pinned-typescript.sh; verify both
# package metadata and the compiler's reported version before accepting it.
# TSZ_PROJECT_TSC_ORACLE_BIN remains an explicit test override: when it names a
# tsc.js it is run via node, and when it names an executable it is run directly.
# Emits the command words (one per line) so callers run it as an array; emits
# nothing when the exact pinned compiler is unavailable.
tsz_project_oracle_tsc_command() {
  if [[ -n "${TSZ_PROJECT_TSC_ORACLE_BIN+x}" ]]; then
    local override="$TSZ_PROJECT_TSC_ORACLE_BIN"
    if [[ "$override" == *.js && -f "$override" ]]; then
      printf 'node\n%s\n' "$override"
    elif [[ -x "$override" ]]; then
      printf '%s\n' "$override"
    fi
    return 0
  fi

  command -v node >/dev/null 2>&1 || return 0

  local root="${ROOT_DIR:-.}"
  local versions_file="$root/scripts/conformance/typescript-versions.json"
  local package_json="$root/scripts/node_modules/typescript/package.json"
  local tsc_js="$root/scripts/node_modules/typescript/lib/tsc.js"
  [[ -f "$versions_file" && -f "$package_json" && -f "$tsc_js" ]] || return 0

  local pinned_version="" installed_version="" reported_version=""
  pinned_version="$(node -e "const fs = require('fs'); const cfg = JSON.parse(fs.readFileSync(process.argv[1], 'utf8')); const current = cfg.current || ''; const mapped = current && cfg.mappings && cfg.mappings[current] && cfg.mappings[current].npm; const fallback = cfg.default && cfg.default.npm; process.stdout.write(mapped || fallback || '');" "$versions_file" 2>/dev/null)" || return 0
  installed_version="$(node -e "const fs = require('fs'); const pkg = JSON.parse(fs.readFileSync(process.argv[1], 'utf8')); process.stdout.write(pkg.version || '');" "$package_json" 2>/dev/null)" || return 0
  [[ -n "$pinned_version" && "$installed_version" == "$pinned_version" ]] || return 0

  reported_version="$(node "$tsc_js" --version 2>/dev/null)" || return 0
  reported_version="${reported_version#Version }"
  [[ "$reported_version" == "$pinned_version" ]] || return 0

  printf 'node\n%s\n' "$tsc_js"
}

# Emit the canonical identity key (basename<TAB>line<TAB>col<TAB>code) for every
# parsable diagnostic line on stdin, one key per line. Unparsable lines
# (banners, "Found N errors", blanks) are dropped.
tsz_diagnostic_identity_keys() {
  awk "$_TSZ_ORACLE_AWK_PARSER"'
    {
      sub(/\r$/, "")
      parse($0)
      if (_ok) print _base "\t" _line "\t" _col "\t" _code
    }
  '
}

# Count parsable diagnostic lines on stdin.
tsz_count_diagnostic_lines() {
  tsz_diagnostic_identity_keys | awk 'END { print NR + 0 }'
}

# Write the tsz-only diagnostic delta to stdout: every parsable tsz diagnostic
# line whose identity key is absent from the tsc oracle output. Unparsable tsz
# lines (e.g. a crash banner) are preserved so a tsz-side hard failure is never
# silently dropped by the subtraction. Usage:
#   tsz_only_delta_lines <tsz_log> <tsc_log>
tsz_only_delta_lines() {
  local tsz_log="$1"
  local tsc_log="$2"
  # Materialize the tsc identity keys to a temp file and feed both files to a
  # single awk via the portable FNR==NR two-file idiom. Passing a multi-line
  # value through `awk -v` is rejected by BSD awk ("newline in string"), so the
  # key set must arrive as a file, never as a variable.
  local tsc_keys_file
  tsc_keys_file="$(mktemp)"
  tsz_diagnostic_identity_keys < "$tsc_log" > "$tsc_keys_file" 2>/dev/null || true
  awk -v keyfile="$tsc_keys_file" "$_TSZ_ORACLE_AWK_PARSER"'
    # First file (tsc identity keys): one TAB-joined key per line. Compared by
    # FILENAME (not FNR==NR) so an empty key file does not misclassify the first
    # tsz line as a key.
    FILENAME == keyfile { if ($0 != "") seen[$0] = 1; next }
    # Second file (tsz log): emit only the tsz-only lines.
    {
      sub(/\r$/, "")
      if ($0 ~ /^[[:space:]]*$/) next
      parse($0)
      # Unparsable line: keep it (a banner or crash note must not be subtracted).
      if (!_ok) { print; next }
      key = _base "\t" _line "\t" _col "\t" _code
      # Parsable line: keep it only when tsc did not flag the same identity.
      if (!(key in seen)) print
    }
  ' "$tsc_keys_file" "$tsz_log" 2>/dev/null || true
  rm -f "$tsc_keys_file"
}

# Emit a `source: line` prefixed body for every parsable diagnostic line in a
# log, capped, so the recorder's per-source partition (project-compatibility.mjs)
# attributes each line correctly. Unparsable lines (banners) are skipped.
#   tsz_label_diagnostic_lines <source-label> <log> [max]
tsz_label_diagnostic_lines() {
  local label="$1" log="$2" max="${3:-20}"
  awk "$_TSZ_ORACLE_AWK_PARSER"'
    BEGIN { seen = 0 }
    {
      sub(/\r$/, "")
      if ($0 ~ /^[[:space:]]*$/) next
      parse($0)
      if (!_ok) next
      print label ": " $0
      seen += 1
      if (seen >= max) exit
    }
  ' label="$label" max="$max" "$log" 2>/dev/null || true
}

# Pass-case delta: the agreed-on diagnostics, labelled per source. Shows the tsc
# errors first (the genuine fixture errors tsz reproduced) then the tsz lines, so
# the green-row artifact records exactly what both compilers reported.
tsc_and_tsz_oracle_delta() {
  local tsz_log="$1" tsc_log="$2"
  tsz_label_diagnostic_lines "tsc" "$tsc_log" 10
  tsz_label_diagnostic_lines "tsz" "$tsz_log" 10
}

# Fail-case delta: the actionable tsz-only diagnostics (labelled `tsz:`) plus a
# few tsc context lines (labelled `tsc:`) so triage sees both the divergence and
# the tsc baseline it was measured against.
tsz_only_and_tsc_context_delta() {
  local tsz_log="$1" tsc_log="$2"
  tsz_only_delta_lines "$tsz_log" "$tsc_log" | awk '
    BEGIN { seen = 0 }
    {
      if ($0 ~ /^[[:space:]]*$/) next
      print "tsz: " $0
      seen += 1
      if (seen >= 14) exit
    }
  '
  tsz_label_diagnostic_lines "tsc" "$tsc_log" 6
}

# Oracle-cache fingerprint for a project row. Independent of the tsz binary
# (tsc's result depends only on tsconfig content + compiled-source identity), so
# the oracle cache survives tsz rebuilds. Reuses the source-identity helpers
# from project-compile-fingerprint.sh (must be sourced first). Returns empty on
# failure so callers treat the oracle cache as unavailable rather than stale.
#   <name>|tsc|<tsc command hash>|<tsconfig hash>|<source identity>
tsz_tsc_oracle_fingerprint() {
  local name="$1" tsconfig="$2" src_dir="${3:-}" tsc_cmd_hash="${4:-}"

  local tsconfig_hash=""
  [[ -f "$tsconfig" ]] && tsconfig_hash="$(sha256_of_file "$tsconfig")"
  [[ -f "$tsconfig" && -z "$tsconfig_hash" ]] && return

  local fixture_dir fixture_phys
  fixture_dir="$(dirname "$tsconfig")"
  [[ -n "$src_dir" ]] || src_dir="$fixture_dir"
  fixture_phys="$(tsz_fingerprint_resolve_physical "$fixture_dir" 2>/dev/null || true)"

  local source_id="" toplevel=""
  toplevel="$(git -C "$fixture_dir" rev-parse --show-toplevel 2>/dev/null || true)"
  if [[ -n "$toplevel" && -n "$fixture_phys" && "$toplevel" == "$fixture_phys" ]]; then
    local git_ref="" dirty_marker="" source_tree_marker=""
    git_ref="$(git -C "$fixture_dir" rev-parse HEAD 2>/dev/null || true)"
    dirty_marker="$(git -C "$fixture_dir" diff HEAD 2>/dev/null | sha256_of_stdin)"
    source_tree_marker="$(hash_source_tree "$src_dir")"
    source_id="git:${git_ref}:${dirty_marker}:tree:${source_tree_marker}"
  else
    source_id="tree:$(hash_source_tree "$src_dir")"
  fi

  printf '%s' "${name}|tsc|${tsc_cmd_hash}|${tsconfig_hash}|${source_id}"
}
