# perf(core): skip empty lib interface validation pass

- **Date**: 2026-05-02
- **Branch**: `perf/skip-empty-lib-interface-pass`
- **Status**: ready
- **Workstream**: 5 (large-repo runtime/residency)

## Intent

Skip the post-merge standard-library interface validation pass when the program
has no affected lib interface names, and skip individual lib files that do not
contain any affected interface. With an empty/nonmatching filter,
`check_source_file_interfaces_only_filtered_post_merge` checks no interfaces, so
the current path still pays for baseline lib checks, synthetic lib binders, and
program-wide declaration-arena cloning without producing diagnostics.

## Planned Scope

- `crates/tsz-core/src/parallel/core.rs`
- `crates/tsz-core/tests/parallel_tests.rs`
- `docs/plan/claims/perf-skip-empty-lib-interface-pass.md`

## Verification

- `cargo fmt --check` (pass)
- `cargo test -p tsz-core affected_lib_interface_names` (pass)
- `cargo test -p tsz-core lib_file_contains_affected_interface` (pass)
- `cargo test -p tsz-core check_files_parallel` (pass: 20 passed, 5 ignored)
- `scripts/bench/perf-hotspots.sh --quick` (pass, artifact:
  `artifacts/perf/hotspots-20260501-231846.json`)
- Large-repo bounded RSS sample:
  `RUST_BACKTRACE=1 scripts/safe-run.sh --limit 75% --interval 2 --verbose -- .target-bench/dist/tsz --extendedDiagnostics --noEmit -p /Users/dutchess/code/large-ts-repo/tsconfig.flat.bench.json`
  stayed below the 12288MB guard and was manually stopped after repeated
  plateau samples, with observed peak 11756MB. During the run, worker threads
  reported an unrelated comparator panic from Rust's slice sort total-order
  check; the sample was stopped with exit 143 after recording the RSS plateau.
