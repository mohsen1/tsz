---
status: ready
---

# fix/checker-generator-return-contextual-type-fingerprints

## Claim

Async generator return diagnostics for `Promise.resolve(variable)` should report the variable's non-contextual widened object type when the fixed argument cannot satisfy the contextual `TReturn`.

## Evidence

- `generatorReturnContextualType.ts` expected `Type '{ x: string; }' is not assignable to type '{ x: "x"; }'` for `Promise.resolve(ret)`, but tsz reported the contextually narrowed source `{ x: "x"; }`.
- The fix rolls back the contextual call diagnostic for fixed-argument async return calls and replaces it with the non-contextual return-call source display at the return statement.
- Unit coverage in `async_return_widening_tests.rs` now locks the widened source display for both `AsyncGenerator` and `AsyncIterator`.
