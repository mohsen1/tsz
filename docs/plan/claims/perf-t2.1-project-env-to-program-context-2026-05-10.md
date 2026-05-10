# perf(checker): T2.1 PR 5B — rename ProjectEnv to ProgramContext

- **Date**: 2026-05-10
- **Branch**: `perf/t2.1-project-env-to-program-context-2026-05-10`
- **PR**: TBD
- **Status**: claim
- **Workstream**: T2.1 PR 5B (PERFORMANCE_PLAN.md §11 PR 5B)

## Intent

Implements **PR 5B** from `docs/plan/PERFORMANCE_PLAN.md` §11 sequence:

> ### PR 5B: `ProjectEnv` -> `ProgramContext`
>
> Goal: no-behavior refactor that names the program-stable layer.
>
> Done when:
> - Conformance is unchanged.
> - No perf regression beyond noise.
> - No new unsafe thread-safety implementations are introduced.

The plan §1 calls this out explicitly: `ProjectEnv` should absorb / rename
into `ProgramContext`, not duplicate. This PR does the rename with no
semantic change.

## Files Touched

- `crates/tsz-checker/src/context/mod.rs` — rename `pub struct ProjectEnv`
  to `pub struct ProgramContext` and all internal references.
- `crates/tsz-checker/src/context/core.rs`,
  `constructors.rs`, `def_mapping.rs` — rename usages.
- `crates/tsz-checker/src/declarations/declarations_module_helpers.rs`,
  `crates/tsz-checker/src/types/module_augmentation.rs` — rename usages.
- `crates/tsz-cli/src/driver/check.rs`, `check_utils.rs`,
  `crates/tsz-cli/src/bin/tsz_server/check.rs` — rename driver
  call sites.
- `crates/tsz-core/src/parallel/skeleton.rs` — rename doc-comment refs.
- `crates/tsz-checker/tests/program_context_tests.rs` (renamed from
  `project_env_tests.rs`) — rename test names + content.
- `crates/tsz-checker/tests/cross_file_type_params_cache_tests.rs` —
  rename usage.
- `crates/tsz-checker/Cargo.toml` — update `[[test]]` `name` and `path`.
- `docs/plan/PERFORMANCE_PLAN.md` — update §1 status table, §6
  migration text, §11 PR 5B entry, §13 risk register, §15 reference
  index. Historical references kept as "formerly `ProjectEnv`" parens.

Also: `project_env` (snake_case) variable names renamed to
`program_context`.

## Verification

- `cargo check -p tsz-checker -p tsz-cli -p tsz-core` clean
- Pre-commit hook (fmt, clippy `-D warnings`, arch guard, full nextest
  suite) — to be confirmed before push
- `grep -rn "ProjectEnv\|project_env" crates/` returns zero matches
  in our tree
- `grep -n "ProjectEnv\|project_env" docs/plan/PERFORMANCE_PLAN.md`
  shows only historical "formerly ProjectEnv" references

## Conformance

No code-path change. Counter values, diagnostics, type computation,
and conformance snapshots are unaffected. This is a pure name change.
