# Claim: fix default type parameter substitution in conditional constraints (#6559)

Status: ready
Owner: Codex
Branch: codex/default-type-param-conditional-6559-20260513
PR: #6563
Issue: #6559

## Scope
- Fixed the TS2345 false positive where bare all-defaulted generic aliases expanded for property access without substituting defaults into nested generic method conditional constraints.
- Added focused regression coverage for the `Chainable<Config = {}>` repro.

## Implementation notes
- `resolve_type_for_property_access_inner` now treats a bare `Lazy(DefId)` with all-defaulted type parameters as an application with filled defaults before resolving members.
- This aligns property-access resolution with assignability/type-resolution behavior for defaulted generic references.

## Validation
- `cargo test -p tsz-checker --test generic_call_inference_tests default_type_parameter_substitutes_inside_conditional_constraint -- --nocapture` passed.
- `cargo test -p tsz-checker --test generic_call_inference_tests` passed: 144 passed.
