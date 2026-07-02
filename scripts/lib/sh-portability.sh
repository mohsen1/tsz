#!/usr/bin/env bash
# Portable shell helpers that stay within the Bash feature set shipped by
# macOS system `/bin/bash` (3.2). CI and contributor tooling must run there
# without requiring a Homebrew Bash, so avoid Bash 4+ builtins/expansions in
# `scripts/**/*.sh`. `scripts/lib/check-sh-portability.py` enforces this; the
# helpers below cover the common cases that would otherwise reach for a Bash 4
# feature.
#
# Source guard: safe to source multiple times.
if [[ -n "${_TSZ_SH_PORTABILITY_SOURCED:-}" ]]; then
  return 0 2>/dev/null || true
fi
_TSZ_SH_PORTABILITY_SOURCED=1

# portable_read_lines <array_name>
#
# Bash 3.2-safe replacement for `mapfile -t <array_name>` / `readarray -t`.
# Reads newline-delimited stdin and stores one line per element into the named
# array, discarding the trailing newline. A final line with no trailing newline
# is still captured. The target array is reset to empty first, so callers get
# the same "replace, don't append" semantics as `mapfile -t`.
#
# Example:
#   portable_read_lines names < <(list_names)
#   for name in "${names[@]}"; do ...; done
portable_read_lines() {
  if [[ $# -ne 1 ]]; then
    echo "portable_read_lines: expected exactly one array name" >&2
    return 2
  fi
  local __prl_array_name="$1"
  local __prl_line
  # Reset the destination array to empty.
  eval "${__prl_array_name}=()"
  # `|| [[ -n $__prl_line ]]` captures a final unterminated line. Bash 3.1+
  # array append (`+=`) of a double-quoted parameter expansion is never
  # word-split, globbed, or re-evaluated -- arbitrary line content is safe.
  while IFS= read -r __prl_line || [[ -n "$__prl_line" ]]; do
    eval "${__prl_array_name}+=(\"\$__prl_line\")"
  done
}
