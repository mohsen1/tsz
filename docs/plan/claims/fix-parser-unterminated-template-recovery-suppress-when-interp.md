---
name: unterminated-template recovery suppresses synthetic markers when tail template has interpolation
status: claimed
timestamp: 2026-05-04 16:32:00
branch: fix/parser-unterminated-template-recovery-suppress-when-interp
---

# Claim

Workstream 1 (Diagnostic Conformance) — TS1005 fingerprint parity for
the `\`,` recovery markers emitted at unterminated-template tails.

## Problem

`report_unterminated_template_recovery_delimiters` emits two synthetic
diagnostics (`',' expected.` at the closing backtick, `'}' expected.` at
the recovery boundary) whenever the unterminated template's source tail
ends with `\`<text>\`,`. tsc emits these markers ONLY when the recovered
tail template's content is **plain** — without any `${...}`
interpolation. With interpolation, tsc treats the tail as continuation
of the outer unterminated template's text and surfaces no synthetic
markers.

The pre-fix heuristic looked for the opening backtick *inside the tail
buffer*, but the unterminated span's `start` is itself a backtick
position — the opening backtick lives BEFORE `start` in the source.
The heuristic therefore returned `None` for the segment lookup and
never suppressed the markers, regardless of interpolation.

Conformance:
- `labeledStatementDeclarationListInLoopNoCrash3.ts` (interpolated tail
  `\`height: var(--button-size-${fontType}-height)\`,`) — tsc emits no
  markers; tsz emitted both. Fingerprint mismatch.
- `labeledStatementDeclarationListInLoopNoCrash4.ts` (plain tail
  `\`height: var foo\`,`) — tsc emits both markers; tsz emits both.
  Already passing.

## Fix

Walk the source backwards from `abs_close_backtick` (= `start +
backtick_before_comma`) to locate the opening backtick of the recovered
tail template. If the segment between the opening backtick and the
closing backtick contains `${`, suppress both synthetic markers.

```rust
let abs_close_backtick = start as usize + backtick_before_comma;
let opening_backtick = source[..abs_close_backtick].rfind('`');
let opening_backtick_segment_has_interp =
    opening_backtick.is_some_and(|open| {
        source[open + 1..abs_close_backtick].contains("${")
    });
if opening_backtick_segment_has_interp {
    return;
}
```

NoCrash3 vs NoCrash4 are now distinguished by the opening backtick's
content, not by the tail buffer alone.

## Files Touched

- `crates/tsz-parser/src/parser/state_expressions_literals.rs`
  (+18 / -1) — refine
  `report_unterminated_template_recovery_delimiters` to walk the
  source preceding the unterminated span when looking for the opening
  backtick.
- `crates/tsz-parser/tests/state_statement_tests.rs` (+38 / 0) — new
  structural test
  `parse_unterminated_template_recovery_suppresses_markers_when_tail_template_has_interpolation`.

## Tests

- New: `parse_unterminated_template_recovery_suppresses_markers_when_tail_template_has_interpolation`
  — locks the suppression rule when the tail template contains `${`.
- Existing: `parse_unterminated_template_recovery_reports_comma_after_next_template_literal`
  and `parse_unterminated_template_recovery_reports_missing_close_brace_at_eof`
  still pass — locks the inverse: plain tail templates DO get
  synthetic markers.
- Crate suite: `cargo nextest run -p tsz-parser` — 773/773 pass.

## Conformance impact

`12413 → 12416 (+3)`. Improvements include `labeledStatementDeclarationListInLoopNoCrash3.ts`,
`importedEnumMemberMergedWithExportedAliasIsError.ts`,
`reverseMappedTypeIntersectionConstraint.ts`, and
`interfaceExtendsObjectIntersectionErrors.ts`. The single listed
regression (`inferenceExactOptionalProperties2.ts`) is pre-existing
drift on plain main (verified by stash + rerun: identical output
without the fix).
