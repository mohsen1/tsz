# Parallel Checking Map

This map separates the production CLI diagnostic path from reusable core and
WASM entry points. Keep CLI behavior changes in the driver unless a core helper
explicitly needs new reusable semantics.

## Production CLI

- [`run_compile_inner`](../../crates/tsz-cli/src/driver/core.rs) reads CLI inputs,
  chooses parse-only fast paths, loads libs, builds the merged program, and then
  calls [`collect_diagnostics`](../../crates/tsz-cli/src/driver/check.rs) for
  semantic diagnostics.
- `--noCheck --noEmit` without declaration emit returns after
  [`parse_files_parallel`](../../crates/tsz-core/src/parallel/core.rs); it does
  not enter semantic checking.
- Normal CLI builds parse and bind user files with
  [`parse_and_bind_parallel_with_libs_and_target`](../../crates/tsz-core/src/parallel/core.rs),
  then run semantic diagnostics through `collect_diagnostics`.
- `collect_diagnostics` owns CLI file scheduling, cache/watch invalidation,
  checker reuse policy, diagnostic filtering, and diagnostic ordering.

## CLI Scheduler Modes

- No cache: first builds and CI check all eligible files in one batch. Small
  projects and large wildcard barrels stay sequential to avoid Rayon overhead
  and nondeterministic interner/dependency observation; larger no-emit projects
  can use chunked checker reuse before falling back to per-file Rayon work.
- Cache present: watch or incremental-style runs use a sequential dependency
  work queue so export-hash invalidation can cascade from changed files to their
  dependents.
- WASM builds use the sequential fallback inside `collect_diagnostics`, because
  the Rayon branch is compiled only for non-WASM targets.

## Reusable Core And WASM APIs

- [`check_files_parallel`](../../crates/tsz-core/src/parallel/core.rs) is reusable
  core infrastructure used by core tests and WASM-facing APIs. It creates
  per-file checker state and returns per-file results, but it does not own CLI
  cache behavior or CLI diagnostic policy.
- [`check_functions_parallel`](../../crates/tsz-core/src/parallel/core.rs) is a
  lower-level function-body checking helper and test-harness entry point.
- [`crates/tsz-wasm`](../../crates/tsz-wasm/src/wasm_api/program.rs) and
  [`tsz-core` WASM APIs](../../crates/tsz-core/src/api/wasm/program.rs) call
  `check_files_parallel` directly after their own parse/bind setup.
