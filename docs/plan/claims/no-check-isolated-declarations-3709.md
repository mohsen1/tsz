---
status: WIP
issue: 3709
agent: claude (auto-loop)
started: 2026-05-07 22:39:06 UTC
---

# `--noCheck` skips `--isolatedDeclarations` diagnostics (#3709)

## Problem
`tsz --noCheck --declaration --emitDeclarationOnly --isolatedDeclarations a.ts`
silently emits `a.d.ts` and exits 0. tsc reports TS9007/TS9011/etc. and
refuses to emit, because those diagnostics gate declaration emission and
are not type-checking work.

## Fix
- Add a public `tsz_checker::run_isolated_declarations_pass(arena, binder,
  source_file, file_name, options)` helper that runs the existing
  `check_isolated_declarations`, `check_isolated_decl_class_expressions`,
  and `check_isolated_decl_augmentations` walks behind a clean public API
  (the CLI is forbidden from importing `tsz_checker::state::*` per
  `test_cli_must_not_import_checker_internals`).
- In `crates/tsz-cli/src/driver/check.rs`, where `options.no_check` short-
  circuits past `check_file_for_parallel`, also invoke the isolated-decl
  pass for each file when `compiler_options.isolated_declarations` is set.
  Construct a fresh per-file `BinderState`; the pass needs symbol scope
  information but no cross-file resolution.

## Out of scope (separate gaps)
- `f(x)` (no initializer): tsz emits TS7006 ("implicit any") instead of
  TS9011 ("parameter must have explicit type annotation"). Requires
  loosening the `param.initializer.is_some()` gate in
  `check_isolated_decl_function_params` and routing TS7006 through the
  isolated-declaration code path. Pre-existing in regular check mode too.
- tsz still emits `a.d.ts` after surfacing TS9007. tsc suppresses emit
  when isolated-declaration errors fire. Requires CLI emit-gate work that
  this PR does not touch.
