# Claim: fix tuple literal widening against distributive conditional union (#6401)

Status: ready
Owner: Codex
Branch: fix-tuple-literal-union-6401-20260513
Issue: #6401

## Scope

Investigate and fix the false TS2322 where a tuple literal assigned to a distributive conditional tuple union is widened to `[string, string]` instead of preserving literal tuple elements.

## Initial validation target

- focused regression for #6401
- targeted checker/solver tests affected by tuple literal assignment or contextual typing
- `cargo fmt --all -- --check`

## Notes

Conformance is currently solved on main; this slice must not introduce conformance risk beyond a focused false-positive fix.
