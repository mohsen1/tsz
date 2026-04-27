**2026-04-27 01:12:20** — Claim: fix(checker): collect new-expression argument types when constructor target is unresolved

Owner: agent (claude opus-4-7)
Branch: `fix/checker-new-error-args-20260427-0312`
Scope: `crates/tsz-checker/src/types/computation/complex.rs`

Root cause: in `get_type_of_new_expression_with_request`, when the constructor
target identifier is unresolved (e.g. `new Undefined(...)`), `constructor_type`
becomes `TypeId::ERROR` and the function bails out at the early return without
walking the argument list. As a result, unresolved names inside the arguments
(`new Outer(new Inner(), new Other())`) never reach name resolution, so tsc's
TS2304 diagnostics for those nested constructor names go missing.

Fix: mirror the `callee_type == TypeId::ERROR` branch in `call/inner.rs` —
before returning `TypeId::ERROR`, drive `collect_call_argument_types_with_context`
over the new-expression arguments so nested name lookups still emit TS2304 /
TS2454 / TS18046.

Verification:
- TypeScript/tests/cases/conformance/parser/ecmascript5/parserRealSource8.ts
  flips fingerprint-only -> PASS (17 missing TS2304 fingerprints recovered).
- New unit test in tsz-checker locks the invariant: nested `new` arguments
  emit TS2304 even when the outer constructor name is unresolved.
- Targeted conformance + full nextest + full conformance suite checked for
  regressions.
