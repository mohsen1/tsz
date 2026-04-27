**2026-04-27 04:20:00**

# Claim: Readonly property + assigned-parameter alias narrowing parity (TS 6.0.3)

Branch: `fix/cfa-readonly-alias-narrowing-20260427-0420`

## Scope
Fix two symmetrical bugs in alias-based narrowing in
`crates/tsz-checker/src/flow/control_flow/condition_narrowing.rs`:

1. Property access alias targets (e.g. `this.x`, `outer.obj.kind`) currently
   invalidate narrowing whenever any function-wide assignment of the base
   expression exists. This is incorrect when the target chain ends at a
   `readonly` property — tsc's `isConstantReference` returns true and
   narrowing is preserved.
2. Simple identifier alias targets (e.g. parameters or `let` locals) only
   look at on-path antecedent assignments. This misses tsc's
   `isParameterOrMutableLocalVariable(s) && !isSymbolAssigned(s)` rule,
   which treats any function-wide assignment of the symbol as
   invalidating alias narrowing.

## Test
- Drives `controlFlowAliasing.ts` (C11 + f26/f27 expectations) toward
  fingerprint parity.
- Adds a checker unit test pinning the C11 readonly + parameter
  reassignment expectation.

## Risk
Targets a single helper. No solver or boundary changes.
