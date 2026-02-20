#!/usr/bin/env bash
set -euo pipefail

project_dir="${GEMINI_PROJECT_DIR:-$(pwd)}"
prompt_file="${project_dir}/scripts/gemini-loop.prompt.conformance.txt"

if [[ ! -f "$prompt_file" ]]; then
  echo "Missing prompt file: $prompt_file" >&2
  exit 2
fi

cat "$prompt_file" >&2
exit 2
