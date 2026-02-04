# Session tsz-2: TDZ (Temporal Dead Zone) Checking

**Started**: 2026-02-04
**Goal**: Implement TDZ checking to detect variables used before declaration in class contexts

## Problem Statement

TypeScript enforces Temporal Dead Zone (TDZ) rules to prevent variables from being used before they're declared. Currently, several methods in `src/checker/flow_analysis.rs` are stubbed and return `false`, causing the compiler to miss these errors.

## Scope

Implement TDZ checking for:
1. **Static Blocks**: Variables used in static blocks before their declaration
2. **Computed Properties**: Variables used in computed property names `[expr]` before declaration
3. **Heritage Clauses**: Variables used in `extends`/`implements` clauses before declaration

## Target File

`src/checker/flow_analysis.rs`

### Methods to Implement

```rust
pub(crate) fn is_variable_used_before_declaration_in_static_block(
    &self,
    _sym_id: SymbolId,
    _usage_idx: NodeIndex,
) -> bool {
    // TODO: Implement TDZ checking for static blocks
    false
}

pub(crate) fn is_variable_used_before_declaration_in_computed_property(
    &self,
    _sym_id: SymbolId,
    _usage_idx: NodeIndex,
) -> bool {
    // TODO: Implement TDZ checking for computed properties
    false
}

pub(crate) fn is_variable_used_before_declaration_in_heritage_clause(
    &self,
    _sym_id: SymbolId,
    _usage_idx: NodeIndex,
) -> bool {
    // TODO: Implement TDZ checking for heritage clauses
    false
}
```

## Implementation Strategy

### 1. Static Blocks
- Check if usage is inside a `static {}` block
- Check if the variable is an instance member (not static)
- Compare text spans: usage before declaration
- Emit TS2454: "Variable used before assignment"

### 2. Computed Properties
- Check if usage is inside a computed property name `[expr]`
- Check if variable is declared in the same class
- Ensure variable is available in outer scope or declared before class
- Emit TS2454 if violated

### 3. Heritage Clauses
- Check if usage is in `extends` or `implements` clause
- Check if variable is a member of the class being defined
- Class doesn't exist yet when heritage clause is evaluated
- Emit TS2454 if violated

## Success Criteria

- [ ] All three TDZ methods implemented
- [ ] Tests pass for static block TDZ
- [ ] Tests pass for computed property TDZ
- [ ] Tests pass for heritage clause TDZ
- [ ] No regressions in existing tests
- [ ] All work committed and pushed

## Notes

- This task is well-isolated and doesn't require broad architectural changes
- It directly addresses conformance gaps in TS2454 errors
- Reference: `docs/walkthrough/04-checker.md` documents this as a known gap
