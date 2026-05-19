# Project corpus row metadata

Project rows are defined in [`project-rows.mjs`](./project-rows.mjs).

Each entry in `PROJECT_ROW_DEFINITIONS` is the **single metadata source** for
benchmark/guard rows, including the canonical fixture-pin values (repo URLs and
commit hashes). `project-fixtures.sh` reads those pins at runtime via
`tsz_load_fixture_pins_from_rows` and exports them as shell variables.
Shell env vars that are already set take precedence, so CI and local overrides
still work (e.g. `UTILITY_TYPES_REPO=<mirror>` before sourcing `project-fixtures.sh`).

A row currently includes:

- `name`: canonical lowercase hyphenated row id (used by scripts and dashboards)
- `label`: human label
- `owner`: track ownership summary
- `family`: project family name for reporting
- `fixture_dir`: fixture-relative POSIX directory name
- `source_dir`: logical fixture-relative POSIX source root used for file counting
- `guard_set`: `"required"`, `"canary"`, or `null`
- `benchmark_set`: `"required"` or `"canary"`
- `category`: `external` or `generated`
- `readme_candidates`: ordered list of fixture-relative POSIX README file candidates (fallback: `README.md`)

Path-like fields must be relative POSIX paths. They may not be absolute, use
backslashes, contain empty segments, or contain `.` / `..` segments. `source_dir`
alone may be `"."` for generated fixtures whose source root is the fixture root.
Row names must use lowercase hyphenated slugs because runners use them as stable
identifiers and artifact/log path components.

External rows (cloned from GitHub) also include pin fields:

- `repo`: canonical git clone URL (https://)
- `ref`: pinned commit hash
- `repo_env`: shell variable name that holds the URL (for overrides, e.g. `UTILITY_TYPES_REPO`)
- `ref_env`: shell variable name that holds the ref (e.g. `UTILITY_TYPES_REF`)

Some generated rows include expected-count fields:

- `expected_generated`: expected number of generated source files
- `expected_generated_env`: shell variable name for this count (e.g. `TYPE_CHALLENGES_SOLUTIONS_EXPECTED_GENERATED`)
- `expected_test_cases`: expected number of generated test-case files, if a future generated row needs that pin
- `expected_test_cases_env`: shell variable name for this count

All `*_env` fields must be valid shell variable identifiers because
`project-fixtures.sh` exports them for benchmark and compile-guard scripts.
Pinned values and expected counts must keep their matching `*_env` field so
the shared fixture loader can publish defaults while still honoring overrides.
`fixture_dir` values must be unique across rows because runners materialize
fixtures under those directories.

Derived arrays are exported for runners:

- `REQUIRED_PROJECT_ROWS`
- `COMPILE_GUARD_REQUIRED_ROWS`
- `COMPILE_CANARY_PROJECT_ROWS`
- `COMPILE_GUARD_CANARY_PROJECT_ROWS`
- `REQUIRED_COMPATIBILITY_FIELDS`

`scripts/bench/project-fixtures.sh` loads pins from `project-rows.mjs` at
runtime and uses these lists in both benchmark and compile-guard flows.

To add a new row:

1. Add one object to `PROJECT_ROW_DEFINITIONS` in [`project-rows.mjs`](./project-rows.mjs).
   - Set `guard_set` / `benchmark_set` to control which runners pick it up.
   - For external (git clone) rows, add `repo`, `ref`, `repo_env`, and `ref_env`.
   - For rows with a fixed number of generated files, add `expected_generated` / `expected_generated_env`.
2. If the row is handled in the compile guard, add a case in `run_project_row` inside
   [`project-compile-guard.sh`](../ci/project-compile-guard.sh) for fixture setup + `check_project`.
3. If the row is benchmarked as a full project row, add the corresponding
   `run_*_project_benchmarks` helper and `run_isolated` invocation in
   [`bench-vs-tsgo.sh`](./bench-vs-tsgo.sh).
4. If the row is a new git-clone fixture, add a `tsz_write_*_config` function
   to [`project-fixtures.sh`](./project-fixtures.sh) that emits a `tsconfig.tsz-guard.json`
   for the fixture directory.

Validation:

- `node scripts/bench/validate-project-metadata.mjs`

This command fails when any required metadata field is missing or malformed,
when a `repo_env` row lacks a `repo` URL, or when a `ref_env` row lacks a `ref` hash.
It also validates fixture/source/README paths before benchmark or compile-guard
runners consume them, and validates shell variable field names before runtime
pin loading exports them.
