# Claim: fix default type parameter substitution in conditional constraints (#6559)

Status: claim
Owner: Codex
Branch: codex/default-type-param-conditional-6559-20260513
PR: TBD
Issue: #6559

## Scope
- Investigate and fix the TS2345 false positive where defaulted generic type parameters are not substituted inside nested conditional constraints.
- Add focused regression coverage for the `Chainable<Config = {}>` repro.

## Validation plan
- Targeted checker regression for #6559.
- Broader checker/conformance validation if solver instantiation code changes.
