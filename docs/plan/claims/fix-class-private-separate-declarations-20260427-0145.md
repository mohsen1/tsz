**2026-04-27 01:45:58**

# Class private member separate declarations elaboration

## Scope
Fix conformance test `interfaceExtendsClassWithPrivate2` and similar cases by:
1. Emitting the "Types have separate declarations of a private property 'x'." elaboration alongside TS2415 (class incorrectly extends) and TS2420 (class incorrectly implements interface) when both the derived class and the base/interface have a same-named private member with compatible types.
2. Fixing the wrong "Property 'x' is private in type 'D' but not in type 'I'" message when both class and interface have private members at the same name (with the same brand or with compatible types) — should be either suppressed, or use the "separate declarations" wording.

## Files to touch
- `crates/tsz-checker/src/classes/class_checker.rs` (TS2415 elaboration)
- `crates/tsz-checker/src/classes/class_implements_checker/core.rs` (TS2420 message correction)

## Tests
Add a unit test in `crates/tsz-checker/tests/private_brands.rs` (or near it) locking in the elaboration text.

## Verification
- `./scripts/conformance/conformance.sh run --filter "interfaceExtendsClassWithPrivate2" --verbose` → PASS
- `cargo nextest run -p tsz-checker --lib` no regressions
- Full conformance: net positive (no regressions).
