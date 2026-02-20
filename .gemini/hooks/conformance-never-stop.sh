#!/usr/bin/env bash
set -euo pipefail

project_dir="${GEMINI_PROJECT_DIR:-$(pwd)}"
prompt_file="${project_dir}/scripts/gemini-loop.prompt.conformance.txt"
manager_marker="${project_dir}/.gemini-fleet-manager"

# Fleet manager sessions create a marker file in repo root.
# If present, disable this hook and let scripts/gemini-loop.sh control looping.
if [[ -f "$manager_marker" ]]; then
  exit 0
fi

if [[ ! -f "$prompt_file" ]]; then
  echo "Missing prompt file: $prompt_file" >&2
  exit 2
fi

cat "$prompt_file" >&2
exit 2
