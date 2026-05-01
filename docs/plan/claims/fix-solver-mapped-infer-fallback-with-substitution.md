# fix(solver): preserve Application form for conditional infer matching after IndexAccess substitution

- **Date**: 2026-05-01
- **Branch**: `fix/solver-mapped-infer-fallback-with-substitution`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance — wrong-code)

## Intent

When a mapped per-key conditional has the shape
`S[K] extends Pattern<infer T, infer U> ? ... : Y`, the solver was falling
through to `Y` because the raw `S[K]` (an `IndexAccess`) doesn't match the
`Application` pattern, and by the time the conditional fully evaluated
`check_type`, `try_expand_application_for_conditional_check` had already
unfolded the resulting `Application` into its structural object body —
losing the Application-vs-Application binding path entirely.

This PR teaches `try_application_infer_match` to recover an `Application`
form for the source before invoking `match_infer_pattern`, with two
fallbacks: evaluate the raw type once (handles the IndexAccess case
where evaluation yields an Application directly), and consult the solver's
`display_alias` side table (handles the case where evaluation lands on the
Application's evaluated body — `display_alias[body] = Application` is
already recorded by `store_display_alias_preferring_application` for every
evaluated generic application).

The pattern is the core of the `LibraryManagedAttributes` /
`InferredPropTypes` chain in
`compiler/conformance/jsx/tsxLibraryManagedAttributes.tsx`.

## Files Touched

- `crates/tsz-solver/src/evaluation/evaluate_rules/conditional.rs` —
  extend `try_application_infer_match` source recovery (~25 LOC change
  in one function).
- `crates/tsz-checker/src/tests/mapped_infer_with_substitution_tests.rs`
  — three locking unit tests: direct conditional substitution, the
  full `mapped[K] -> S[K] extends Pattern<infer T, infer U>` chain, and
  a name-renamed cover for the anti-hardcoding rule.
- `crates/tsz-checker/src/lib.rs` — wire the new test module.

## Verification

- `cargo nextest run -p tsz-checker -p tsz-solver --lib` — 8648 tests pass.
- Three new locking unit tests pass.
- Smoke conformance:
  - `--filter infer` → 83/87 PASS (4 failing are all pre-existing).
  - `--filter mapped` → 53/60 PASS (7 failing are all pre-existing).
- The parent `tsxLibraryManagedAttributes.tsx` is fingerprint-only and
  has additional unrelated bugs (alias preservation in JSX prop chain,
  default-arg display) — see
  `~/.../memory/project_tsxLibraryManagedAttributes_multi_bug.md` for
  the full bug decomposition. This PR fixes only bug #1 of that test;
  the test stays fingerprint-only after this PR but with a smaller diff.
