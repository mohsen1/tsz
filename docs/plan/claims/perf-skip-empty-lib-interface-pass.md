# perf(core): skip empty lib interface validation pass

- **Date**: 2026-05-02
- **Branch**: `perf/skip-empty-lib-interface-pass`
- **Status**: claim
- **Workstream**: 5 (large-repo runtime/residency)

## Intent

Skip the post-merge standard-library interface validation pass when the program
has no affected lib interface names. With an empty filter,
`check_source_file_interfaces_only_filtered_post_merge` checks no interfaces, so
the current path still pays for baseline lib checks, synthetic lib binders, and
program-wide declaration-arena cloning without producing diagnostics.

## Planned Scope

- `crates/tsz-core/src/parallel/core.rs`
- `crates/tsz-core/tests/parallel_tests.rs`
- `docs/plan/claims/perf-skip-empty-lib-interface-pass.md`

## Verification

- `cargo fmt --check`
- `cargo test -p tsz-core affected_lib_interface_names`
- `cargo test -p tsz-core check_files_parallel`
- `scripts/bench/perf-hotspots.sh --quick`
