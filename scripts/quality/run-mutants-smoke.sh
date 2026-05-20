#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

package="${TSZ_MUTANTS_PACKAGE:-tsz-common}"
file_glob="${TSZ_MUTANTS_FILE:-crates/tsz-common/src/**/*.rs}"

scripts/safe-run.sh --limit "${TSZ_MUTANTS_MEMORY_LIMIT:-75%}" -- \
  cargo mutants --package "$package" --file "$file_glob" --list
