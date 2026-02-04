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

### 2026-02-04: Static Block TDZ - COMPLETE ✅

**Root Cause Discovered and Fixed** (commit fea4b95f5):
The issue was NOT with TDZ checking logic, but with static block traversal:
- `check_class_member` in `state_checking_members.rs` was falling through to default case
- Static blocks were treated as expressions instead of statement blocks
- `get_type_of_node` was called, which didn't traverse the block statements
- Result: NO type checking happened for any code inside static blocks

**Fix Applied**:
Added specific case for `CLASS_STATIC_BLOCK_DECLARATION` in `check_class_member`:
```rust
syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION => {
    if let Some(block) = self.ctx.arena.get_block(node) {
        self.check_unreachable_code_in_block(&block.statements.nodes);
        for &stmt_idx in &block.statements.nodes {
            self.check_statement(stmt_idx);
        }
    }
}
```

**Implementation Details**:
- `is_variable_used_before_declaration_in_static_block` method in flow_analysis.rs
- Checks if symbol is block-scoped (let, const, class, enum)
- Compares usage position vs declaration position in source
- Verifies usage is inside a static block using `find_enclosing_static_block`
- Emits TS2448: "Block-scoped variable '{0}' used before its declaration"
- Added TS2448 diagnostic code and message to diagnostics.rs (commit 0e8d667a7)

**Test Results** (classStaticBlockUseBeforeDef3.ts):
```typescript
class Baz {
    static {
        console.log(FOO);   // line 17
    }
}
const FOO = "FOO";  // line 21
```
✅ tsc: `error TS2448: Block-scoped variable 'FOO' used before its declaration.`
✅ tsz: `error TS2448: Block-scoped variable 'FOO' used before its declaration.`

Perfect match!

**Next Steps**:
1. Implement computed property TDZ
2. Implement heritage clause TDZ
3. Run conformance tests to measure impact

## Success Criteria

- [x] Static block TDZ method implemented
- [x] Static block traversal fixed (root cause)
- [x] TS2448 diagnostic added
- [x] Test case passes (classStaticBlockUseBeforeDef3.ts)
- [x] All work committed and pushed
- [ ] Computed property TDZ implemented
- [ ] Heritage clause TDZ implemented
- [ ] Conformance test run to measure impact

## Notes

- This task required fixing a missing traversal handler, not just TDZ logic
- The fix enables ALL type checking for static blocks, not just TDZ
- Static blocks were completely untype-checked before this fix
- Reference: `docs/walkthrough/04-checker.md` documents this as a known gap
