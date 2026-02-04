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

## Progress

### 2026-02-04: Static Block TDZ (Partial Implementation)

**Implemented** (commit b549afdcd):
- `is_variable_used_before_declaration_in_static_block` method
- Checks if symbol is block-scoped (let, const, class, enum)
- Compares usage position vs declaration position in source
- Verifies usage is inside a static block using `find_enclosing_static_block`

**Working**:
- Detects TDZ when symbol IS resolved by binder
- Example: Module-level `const` used inside static block after being declared

**Limitation**:
- Forward references NOT yet handled
- When a variable is used before declaration in source order, the binder hasn't created a symbol yet
- Example case (fails to detect):
  ```typescript
  class Baz {
      static {
          console.log(FOO);   // should error - TDZ violation
      }
  }
  const FOO = "FOO";  // declared after static block
  ```
- In this case, `resolve_identifier_symbol` returns `None` because FOO hasn't been bound yet
- The checker emits "cannot find name" instead of TDZ error

**Next Steps**:
1. Handle forward references: detect when "cannot find name" is actually a TDZ violation
2. Implement computed property TDZ
3. Implement heritage clause TDZ

## Success Criteria

- [x] Static block TDZ method implemented (partial - needs forward reference handling)
- [ ] Forward reference TDZ detection
- [ ] Computed property TDZ implemented
- [ ] Heritage clause TDZ implemented
- [ ] Tests pass for all TDZ cases
- [ ] No regressions in existing tests
- [ ] All work committed and pushed

## Notes

- This task is well-isolated and doesn't require broad architectural changes
- It directly addresses conformance gaps in TS2454 errors
- Reference: `docs/walkthrough/04-checker.md` documents this as a known gap
- Forward reference detection may require checking all top-level declarations in the file
