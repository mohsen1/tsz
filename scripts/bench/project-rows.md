# Project corpus row metadata

Project rows are defined in [`project-rows.mjs`](./project-rows.mjs).

Each entry in `PROJECT_ROW_DEFINITIONS` is the single metadata source for benchmark/guard rows.
A row currently includes:

- `name`: canonical row id (used by scripts and dashboards)
- `label`: human label
- `owner`: track ownership summary
- `family`: project family name for reporting
- `fixture_dir`: fixture directory name
- `source_dir`: logical source root used for file counting
- `guard_set`: `"required"`, `"canary"`, or `null`
- `benchmark_set`: `"required"` or `"canary"`
- `category`: `external` or `generated`
- `readme_candidates`: ordered list of README file candidates for human-friendly labels (fallbacks to `README.md`)

Derived arrays are exported for runners:

- `REQUIRED_PROJECT_ROWS`
- `COMPILE_GUARD_REQUIRED_ROWS`
- `COMPILE_CANARY_PROJECT_ROWS`
- `COMPILE_GUARD_CANARY_PROJECT_ROWS`
- `REQUIRED_COMPATIBILITY_FIELDS`

`scripts/bench/project-fixtures.sh` reads `project-rows.mjs` at runtime and uses
these lists in both benchmark and compile-guard flows, while benchmark owner family
labels are passed from the same metadata into JSON output.

To add a new row:

1. Add one object to `PROJECT_ROW_DEFINITIONS` in [`project-rows.mjs`](./project-rows.mjs) and choose values for `guard_set` / `benchmark_set`.
2. If the row is handled in compile guard, add a case in `run_project_row` inside [`project-compile-guard.sh`](../ci/project-compile-guard.sh) for fixture setup + `check_project`.
3. If the row is benchmarked as a full project row, add the corresponding `run_*_project_benchmarks` helper and `run_isolated` invocation in [`bench-vs-tsgo.sh`](./bench-vs-tsgo.sh).

Validation:

- `node scripts/bench/validate-project-metadata.mjs`

This command fails when any required metadata field is missing or malformed.
