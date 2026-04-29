# fix(checker): anchor TS2590 at the property context that triggered union complexity

- **Date**: 2026-04-29
- **Branch**: `fix/checker-ts2590-anchor-on-property-context`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance and Fingerprints)

## Intent

`normalizedIntersectionTooComplex.ts` is fingerprint-only on TS2590:
position differs.

```ts
const comp = ctor({ common: "ok", ref: x => console.log(x) });
```

- tsc:  `test.ts:37:40` (the value `x` of property `ref`)
- tsz:  `test.ts:37:14` (the call target `ctor`)

The complexity is produced inside the contextual type for the `ref`
property — `Func<Big[T]> | Obj<Big[T]>` instantiated against the
18-member `Big` mapped type — so tsc anchors the diagnostic at the
property value that introduced the complex contextual type. tsz's
emission lives in
`crates/tsz-checker/src/types/computation/call/mod.rs:559-566`:

```rust
if self.ctx.types.take_union_too_complex() {
    self.error_at_node(idx, /* call expression */, …, TS2590);
}
```

The anchor (`idx`) is the call-expression node. The solver's global
`union_too_complex` flag captures the fact that complexity occurred
but not the node that triggered it.

## Required Fix (architectural)

Thread a "current diagnostic node" context from the checker into the
solver's union construction so the complexity-detection site can stamp
the offending node onto the diagnostic. Concretely:

1. Add a `current_diagnostic_node: Cell<Option<NodeIndex>>` (or RefCell
   stack) on `CheckerContext` (or the solver query DB shim).
2. Push the property-value node when typing each property of an object
   literal argument to a call (and similarly for array element
   inference, JSX attribute inference).
3. When `take_union_too_complex` fires, return both the boolean and the
   captured `NodeIndex` so the call-site can `error_at_node(captured, …)`
   with a fallback to the call expression.

Alternative quick heuristic (less robust): when emitting TS2590 for a
call with an object-literal argument, walk the literal's properties
and pick the first property whose contextual type contains a generic
application (rough proxy for "this property triggers the complex
union"). Won't match tsc exactly across all cases but would close the
common cases like the test target.

## Files Likely Involved

- `crates/tsz-checker/src/types/computation/call/mod.rs` (TS2590 emit site)
- `crates/tsz-solver/src/operations/union_normalization*.rs`
  (`union_too_complex` flag set)
- `crates/tsz-checker/src/context/...` (current-node context plumbing)

## Verification

- `./scripts/conformance/conformance.sh run --filter "normalizedIntersectionTooComplex" --verbose`
  — should flip from fingerprint-only to PASS
- New unit test locking the anchor at the property value
- Targeted regression: search for similar TS2590 fingerprint-only tests
  where the expected line/col is inside an object-literal argument
