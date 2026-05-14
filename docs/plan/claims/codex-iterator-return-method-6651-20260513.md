# Claim: object literal `return` methods satisfy Iterator intersections

Status: ready

Issue: #6651

Branch: `codex/iterator-return-method-6651-20260513`

Summary:
- Suppress stale pre-contextual `TS2304` diagnostics that are anchored to object literal method names before contextual rechecking.
- Preserve concrete contextual property types for object literal methods so keyword-named methods such as `return()` do not collapse to `any` in intersection targets.
- Add a regression covering `Iterator<number> & { return(): IteratorReturnResult<void> }`.

Validation:
- `cargo test -p tsz-checker --test ts2322_tests iterator_intersection_return_method_name_is_not_unresolved_identifier -- --nocapture`
- `cargo fmt --all --check`
