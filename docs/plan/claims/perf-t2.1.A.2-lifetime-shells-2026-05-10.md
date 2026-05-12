# perf(checker): T2.1.A.2 — empty lifetime-class shell types

- **Date**: 2026-05-10
- **Branch**: `perf/t2.1.A.2-lifetime-shells-2026-05-10`
- **PR**: TBD
- **Status**: claim
- **Workstream**: T2.1.A.2 (PERFORMANCE_PLAN.md §6)

## Intent

Second half of T2.1.A from PERFORMANCE_PLAN.md §6. T2.1.A.1 (#5034)
shipped the inventory + manifest + CI guard. This PR introduces the
empty `WorkerContext` / `FileSession` / `SpeculationScope` /
`LspPersistentCache` shell types that subsequent T2.1.B+ PRs will
populate.

The shells are intentionally empty in this initial pass. They exist
as named types so:

1. Reviewers can grep for them and see where the architecture is
   heading, even before fields move.
2. The field-lifetime manifest's destination shells become real types,
   not just doc-comment strings.
3. Future PRs are smaller and more reviewable: T2.1.B/C/D each migrate
   one bucket of fields into one of these shells, without also having
   to introduce the type.

## Files Touched

- `crates/tsz-checker/src/context/lifetime_shells.rs` — new module,
  ~140 LOC. Defines `WorkerContext`, `FileSession`, `SpeculationScope`,
  `LspPersistentCache`. Each is `#[derive(Debug, Default)]` with a
  `const fn new()` constructor. Module-level docstring documents the
  population policy: do not add behavior until the corresponding
  T2.1.B/C/D PR migrates the relevant fields.
- `crates/tsz-checker/src/context/mod.rs` — declare and re-export the
  new module's types.

Two unit tests included:

- `shells_implement_default()` — future migrations can wire shells in
  via `Default::default()`.
- `shells_can_be_constructed_const()` — verifies `const fn new()` so
  static initialization is available.

## Verification

- `cargo check -p tsz-checker` clean
- Pre-commit hook (fmt, clippy, arch guard, full nextest suite) — to
  be confirmed before push
- The field-lifetime inventory (#5034) still passes — these shells
  are sibling types of `CheckerContext`, so they don't change the
  227-field count

## Conformance

No semantic change. New types are not yet referenced from any
existing checker code path. Conformance unaffected.
