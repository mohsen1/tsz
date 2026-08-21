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
# Diagnostic identity is exact normalized project-relative path, span, code, and
# message. Global diagnostics use a synthetic path/span plus their message.
# Keys are compared as multisets: missing diagnostics, extras, and duplicate
# multiplicity all matter. This prevents basename collisions and strict-subset
# false greens while allowing absolute/relative spellings of the same project
# path to agree.
#
# Portability: every awk program here stays within POSIX awk (no gawk-only
# 3-argument match()/gensym extensions) so the helpers behave identically under
# the Linux gawk in CI and the BSD awk used for local verification on macOS.

# The shared awk parser body. Defines a `parse(line)` function that sets the
# globals _ok/_base/_line/_col/_code/_message for a recognized diagnostic line,
# or _ok=0 otherwise. Recognizes both formatter shapes tsc/tsz emit:
#   path(line,col): error TSnnnn        (default, used when output is piped)
#   path:line:col - error TSnnnn        (pretty formatter)
#   error TSnnnn                        (global/config diagnostic)
# Kept in one string so the identity-key and delta programs cannot drift.
_TSZ_ORACLE_AWK_PARSER='
function normalize_path(p,   normalized_root, normalized_cwd, candidate) {
  gsub(/\\/, "/", p)
  sub(/^\.\//, "", p)
  normalized_root = root
  gsub(/\\/, "/", normalized_root)
  sub(/\/$/, "", normalized_root)
  normalized_cwd = cwd
  gsub(/\\/, "/", normalized_cwd)
  sub(/\/$/, "", normalized_cwd)
  if (p !~ /^\// && normalized_cwd != "") {
    candidate = normalized_cwd "/" p
    if (normalized_root != "" && index(candidate, normalized_root "/") == 1) {
      p = substr(candidate, length(normalized_root) + 2)
    }
  }
  if (normalized_root != "" && index(p, normalized_root "/") == 1) {
    p = substr(p, length(normalized_root) + 2)
  }
  return p
}
function normalize_message(message) {
  # The colon and one following ASCII space belong to the diagnostic transport
  # shape. Every later byte is message identity: repeated spaces can be part of
  # a quoted path/glob and must not be collapsed into a false parity match.
  sub(/^: ?/, "", message)
  return message
}
function identity_key(   key) {
  key = _base "\t" _line "\t" _col "\t" _code
  return key "\t" _message
}
function parse(line,   loc, before, a, code, message, diagnostic_end) {
  _ok = 0
  _message = ""
  # Global/config diagnostics have no source location. Give them a stable
  # synthetic location and normalized message identity so TS18003 and its peers
  # participate in oracle parity without conflating different config failures.
  if (match(line, /^(error|warning) TS[0-9]+/)) {
    loc = substr(line, RSTART, RLENGTH)
    if (!match(loc, /TS[0-9]+/)) return
    code = substr(loc, RSTART, RLENGTH)
    message = normalize_message(substr(line, length(loc) + 1))
    _base = "<global>"; _line = 0; _col = 0; _code = code
    _message = message; _ok = 1
    return
  }
  # path(line,col): (error|warning) TSnnnn
  if (match(line, /\([0-9]+,[0-9]+\): (error|warning) TS[0-9]+/)) {
    diagnostic_end = RSTART + RLENGTH
    loc = substr(line, RSTART, RLENGTH)
    before = substr(line, 1, RSTART - 1)
    split(loc, a, /[(,)]/)   # a[2]=line a[3]=col
    if (!match(loc, /TS[0-9]+/)) return
    code = substr(loc, RSTART, RLENGTH)
    message = normalize_message(substr(line, diagnostic_end))
    _base = normalize_path(before); _line = a[2]; _col = a[3]; _code = code
    _message = message; _ok = 1
    return
  }
  # path:line:col - (error|warning) TSnnnn  (pretty formatter)
  if (match(line, /:[0-9]+:[0-9]+ - (error|warning) TS[0-9]+/)) {
    diagnostic_end = RSTART + RLENGTH
    loc = substr(line, RSTART, RLENGTH)
    before = substr(line, 1, RSTART - 1)
    split(loc, a, /[: ]/)    # a[2]=line a[3]=col
    if (!match(loc, /TS[0-9]+/)) return
    code = substr(loc, RSTART, RLENGTH)
    message = normalize_message(substr(line, diagnostic_end))
    _base = normalize_path(before); _line = a[2]; _col = a[3]; _code = code
    _message = message; _ok = 1
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

# Emit the canonical identity key for every parsable diagnostic line on stdin,
# one key per line. Keys are path<TAB>line<TAB>col<TAB>code<TAB>message; global
# keys use `<global><TAB>0<TAB>0`. Unparsable lines are dropped. The optional
# argument is the project root used to normalize absolute paths.
tsz_diagnostic_identity_keys() {
  local root="${1:-}"
  awk -v root="$root" -v cwd="$PWD" "$_TSZ_ORACLE_AWK_PARSER"'
    {
      sub(/\r$/, "")
      parse($0)
      if (_ok) print identity_key()
    }
  '
}

# Count parsable diagnostic lines on stdin.
tsz_count_diagnostic_lines() {
  tsz_diagnostic_identity_keys "${1:-}" | awk 'END { print NR + 0 }'
}

# Emit one length-delimited record per diagnostic, with every indented
# continuation attached to its owning primary. A single record stays on one
# output line, so sorting preserves duplicate multiplicity without letting two
# diagnostics exchange reason chains. The optional argument is the project
# root for path identity.
tsz_diagnostic_record_keys() {
  local root="${1:-}"
  awk -v root="$root" -v cwd="$PWD" "$_TSZ_ORACLE_AWK_PARSER"'
    function flush_record() {
      if (in_diagnostic) print record
      in_diagnostic = 0
      record = ""
    }
    {
      sub(/\r$/, "")
      parse($0)
      if (_ok) {
        flush_record()
        primary = identity_key()
        record = length(primary) ":" primary
        in_diagnostic = 1
        next
      }
      if (in_diagnostic && $0 ~ /^[[:space:]]+/ && $0 !~ /^[[:space:]]*$/) {
        record = record "\t" length($0) ":" $0
        next
      }
      if ($0 !~ /^[[:space:]]*$/) flush_record()
    }
    END { flush_record() }
  '
}

# Emit every nonblank line that is neither a parsed diagnostic, an attached
# continuation, nor a known compiler summary. Agreement is never inferred from
# output the harness did not understand.
tsz_diagnostic_unparsed_lines() {
  local root="${1:-}"
  awk -v root="$root" -v cwd="$PWD" "$_TSZ_ORACLE_AWK_PARSER"'
    {
      sub(/\r$/, "")
      parse($0)
      if (_ok) { in_diagnostic = 1; next }
      if (in_diagnostic && $0 ~ /^[[:space:]]+/ && $0 !~ /^[[:space:]]*$/) next
      if ($0 ~ /^[[:space:]]*$/) next
      in_diagnostic = 0
      if ($0 ~ /^Found [0-9]+ errors?( in [0-9]+ files?)?\.$/) next
      print
    }
  '
}

tsz_diagnostic_log_is_covered() {
  local log="$1" root="${2:-}" unmatched
  unmatched="$(tsz_diagnostic_unparsed_lines "$root" < "$log")"
  [[ -z "$unmatched" ]]
}

tsz_diagnostic_multisets_agree() {
  local tsz_log="$1" tsc_log="$2" root="${3:-}"
  local tsz_keys_file tsc_keys_file tsz_unparsed_file tsc_unparsed_file
  tsz_keys_file="$(mktemp)"
  tsc_keys_file="$(mktemp)"
  tsz_unparsed_file="$(mktemp)"
  tsc_unparsed_file="$(mktemp)"
  tsz_diagnostic_record_keys "$root" < "$tsz_log" | LC_ALL=C sort > "$tsz_keys_file"
  tsz_diagnostic_record_keys "$root" < "$tsc_log" | LC_ALL=C sort > "$tsc_keys_file"
  tsz_diagnostic_unparsed_lines "$root" < "$tsz_log" > "$tsz_unparsed_file"
  tsz_diagnostic_unparsed_lines "$root" < "$tsc_log" > "$tsc_unparsed_file"
  local rc=0
  cmp -s "$tsz_keys_file" "$tsc_keys_file" || rc=$?
  if [[ "$rc" -eq 0 && ( -s "$tsz_unparsed_file" || -s "$tsc_unparsed_file" ) ]]; then
    rc=1
  fi
  rm -f "$tsz_keys_file" "$tsc_keys_file" \
    "$tsz_unparsed_file" "$tsc_unparsed_file"
  return "$rc"
}

# Emit `<record-count><TAB><sha256>` for the exact normalized diagnostic
# records in a compiler log. The fingerprint covers sorted, length-delimited
# records, so diagnostic multiplicity and continuation ownership remain part of
# the persisted evidence rather than being reduced to a set of error codes.
# `project-compile-fingerprint.sh` must be sourced first for sha256_of_file.
tsz_diagnostic_record_stats() {
  local log="$1" root="${2:-}" records count fingerprint
  records="$(mktemp)" || return 1
  if ! tsz_diagnostic_record_keys "$root" < "$log" | LC_ALL=C sort > "$records"; then
    rm -f "$records"
    return 1
  fi
  count="$(wc -l < "$records" | tr -d '[:space:]')"
  fingerprint="$(sha256_of_file "$records")"
  rm -f "$records"
  [[ "$count" =~ ^(0|[1-9][0-9]*)$ && "$fingerprint" =~ ^[0-9a-f]{64}$ ]] || return 1
  printf '%s\t%s\n' "$count" "$fingerprint"
}

# Write the tsz-only diagnostic delta to stdout: every parsable tsz diagnostic
# line whose identity key is absent from the tsc oracle output. Unparsable tsz
# lines (e.g. a crash banner) are preserved so a tsz-side hard failure is never
# silently dropped by the subtraction. Usage:
#   tsz_only_delta_lines <tsz_log> <tsc_log>
tsz_only_delta_lines() {
  local tsz_log="$1"
  local tsc_log="$2"
  local root="${3:-}"
  # Materialize the tsc identity keys to a temp file and feed both files to a
  # single awk via the portable FNR==NR two-file idiom. Passing a multi-line
  # value through `awk -v` is rejected by BSD awk ("newline in string"), so the
  # key set must arrive as a file, never as a variable.
  local tsc_keys_file
  tsc_keys_file="$(mktemp)"
  tsz_diagnostic_identity_keys "$root" < "$tsc_log" > "$tsc_keys_file" 2>/dev/null || true
  awk -v keyfile="$tsc_keys_file" -v root="$root" -v cwd="$PWD" "$_TSZ_ORACLE_AWK_PARSER"'
    # First file (tsc identity keys): one TAB-joined key per line. Compared by
    # FILENAME (not FNR==NR) so an empty key file does not misclassify the first
    # tsz line as a key.
    FILENAME == keyfile { if ($0 != "") seen[$0] += 1; next }
    # Second file (tsz log): emit only the tsz-only lines.
    {
      sub(/\r$/, "")
      if ($0 ~ /^[[:space:]]*$/) next
      parse($0)
      # Unparsable line: keep it (a banner or crash note must not be subtracted).
      if (!_ok) { print; next }
      key = identity_key()
      # Parsable line: subtract exactly one matching oracle occurrence.
      if (seen[key] > 0) seen[key] -= 1
      else print
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

# Fail-case delta: exact multiset differences in both directions. Extra tsz
# diagnostics are labelled `tsz:`; diagnostics missing from tsz are `tsc:`.
tsz_only_and_tsc_context_delta() {
  local tsz_log="$1" tsc_log="$2" root="${3:-}"
  tsz_only_delta_lines "$tsz_log" "$tsc_log" "$root" | awk '
    BEGIN { seen = 0 }
    {
      if ($0 ~ /^[[:space:]]*$/) next
      print "tsz: " $0
      seen += 1
      if (seen >= 10) exit
    }
  '
  # Symmetric missing-diagnostic side: reverse the multiset subtraction so a
  # strict tsz subset remains visible instead of looking like an empty delta.
  tsz_only_delta_lines "$tsc_log" "$tsz_log" "$root" | awk '
    BEGIN { seen = 0 }
    {
      if ($0 ~ /^[[:space:]]*$/) next
      print "tsc: " $0
      seen += 1
      if (seen >= 10) exit
    }
  '
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

  local source_id=""
  source_id="$(tsz_compile_input_identity "$tsconfig" "$src_dir")"
  [[ -n "$source_id" ]] || return

  printf '%s' "${name}|tsc|${tsc_cmd_hash}|${tsconfig_hash}|${source_id}"
}
