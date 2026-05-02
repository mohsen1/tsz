---
name: typeInferenceConflictingCandidates retry vs union widening trade-off
description: Analysis of why naive retry-skip on literal mismatches helps one test but regresses three; documents the tsc heuristic we are missing
type: claim
status: deferred
date: 2026-05-03
---

# Claim

`typeInferenceConflictingCandidates.ts` is a fingerprint-only mismatch
where tsz emits the right error code (TS2345) but with widened types in
the message:

```
// Test
declare function g<T>(a: T, b: T, c: (t: T) => T): T;
g("", 3, a => a);

// tsc baseline
error TS2345: Argument of type '3' is not assignable to parameter of type '""'.

// tsz current
error TS2345: Argument of type 'number' is not assignable to parameter of type 'string'.
```

## Root cause

In `crates/tsz-checker/src/types/computation/call/inner.rs:2151-2152`,
the generic-call retry decision is:

```rust
} else {
    true  // always retry when no contextual type
}
```

The retry re-runs argument collection with sanitized contextual types,
which widens literal types (`3` → `number`). When the second call
re-emits `ArgumentTypeMismatch`, the `actual` and `expected` carried in
the result are the widened types, so TS2345 prints the wrong names.

## Naive fix attempted

Skip retry when both `actual` and `expected` are literal types:

```rust
} else if let CallResult::ArgumentTypeMismatch { actual, expected, .. } = &result
    && is_literal_type(self.ctx.types, *actual)
    && is_literal_type(self.ctx.types, *expected)
{
    false
} else {
    true
}
```

This makes the target test pass but regresses three other tests where
the retry was correctly producing a successful call by widening:

- `fixTypeParameterInSignatureWithRestParameters.ts`:
  ```ts
  function bar<T>(item1: T, item2: T) {}
  bar(1, "");  // tsc: ok (T = number | string after retry)
  ```
- `genericRestArgs.ts`
- `destructuringParameterDeclaration1ES5.ts`

In all three, the call has multiple literal candidates that **conflict**
under first-wins inference but **succeed** under union widening — tsc
expects no error.

## Why the simple gate fails

tsc's algorithm is asymmetric:

1. Collect type-parameter candidates from each argument site.
2. If a single candidate consistently wins, use it (and emit TS2345 with
   literal types if a remaining arg is incompatible).
3. If multiple incompatible candidates exist, **widen the union** of
   them (`""` ∪ `3` → `string ∪ number`) and re-check.
4. If the widened-union check passes, emit no error.
5. If the widened-union check still fails, emit TS2345 — but with the
   **original literal** `actual`/`expected`, not the widened ones.

Step 5 is the critical bit we're missing. tsz's retry produces step 4's
widened types and uses them for both the success path and the error path
indiscriminately.

## What a real fix needs

One of:

1. **Two-pass error preservation**: keep the first-call's
   `ArgumentTypeMismatch` payload around, run the retry, and if the
   retry also fails, swap the widened payload back to the original
   literal payload before emitting TS2345.

2. **Detect "would the union widen successfully?"**: before retrying,
   compute the union of all literal candidates and check if it satisfies
   the constraint. If yes → retry (current behavior). If no → skip
   retry, preserving the literal error.

3. **Match tsc's actual `inferTypes` algorithm**: this is the proper fix
   but would require a non-trivial refactor of the candidate collection
   and inference loop.

Approach (1) is the smallest diff. Approach (2) is more semantically
faithful. Approach (3) is the right long-term direction.

## Reproducer / harness

Compile-and-run probe:
```fish
echo 'declare function g<T>(a: T, b: T, c: (t: T) => T): T;
g("", 3, a => a);' > /tmp/test-ticc.ts
.target/dist-fast/tsz /tmp/test-ticc.ts
```

Conformance:
```
.target/dist-fast/tsz-conformance --filter typeInferenceConflictingCandidates \
  --verbose --cache-file scripts/conformance/tsc-cache-full.json
```

The retry decision lives in
`crates/tsz-checker/src/types/computation/call/inner.rs:2135-2156`.
The `CallResult::ArgumentTypeMismatch { actual, expected, .. }` payload
is in
`crates/tsz-solver/src/operations/core/call_evaluator.rs:135`.

## Status

Deferred. Naive gate over-corrects. Needs approach (1) or (2) above.
