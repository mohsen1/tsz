# Session TSZ-11: Control Flow Analysis Integration

**Started**: 2026-02-05
**Status**: 🔄 ACTIVE (Investigation Phase - Found Root Cause)

## Goal

Integrate the `FlowAnalyzer` into the main Checker loop so that narrowed types calculated by the Solver are actually used during type checking.

## Critical Discovery 🔍

**FlowAnalyzer.get_flow_type() ALREADY EXISTS and works correctly!**

Location: `src/checker/control_flow.rs:210`

This function:
- ✅ Walks the FlowNode graph backwards
- ✅ Applies narrowing correctly (instanceof, typeof, discriminant, etc.)
- ✅ Handles complex cases (loops, branches, fixed-point iteration)
- ✅ Has proper caching and cycle detection

## The Real Problem

**The main expression type checking path does NOT call get_flow_type()**

Current behavior when checking `animal.bark()`:
1. Checker calls `get_type_of_node()` 
2. → calls `get_type_of_symbol()` (state_type_analysis.rs:751)
3. → returns declared type from flow-INSENSITIVE cache (keyed only by SymbolId)
4. → returns `Animal` instead of narrowed type `Dog`

The flow-aware `get_flow_type()` is never called!

## Root Cause

`get_type_of_symbol()` uses a simple cache:
```rust
// src/checker/state_type_analysis.rs:757
if let Some(&cached) = self.ctx.symbol_types.get(&sym_id) {
    return cached; // Returns declared type, ignoring flow!
}
```

This cache is keyed only by `SymbolId`, completely ignoring flow context.

## The Fix

Based on Gemini consultation, need to:

1. **Find where identifier expressions are type-checked**
   - Likely in `src/checker/expr.rs` or similar
   - Function: `check_identifier` or Identifier match arm in `check_expression`

2. **Add FlowAnalyzer call**
   - Replace (or augment) direct `get_type_of_symbol()` call
   - Call `get_flow_type(reference, initial_type, flow_node)` instead
   - This is the entry point that exists but isn't being used

3. **Handle the FlowNode parameter**
   - Need to get the FlowNodeId for the current expression location
   - Binder should provide this mapping (NodeIndex → FlowNodeId)
   - May need to expose this from the Binder

## Plan

### Phase 1: Investigation (CURRENT)

**Task 1**: Find the identifier type-checking code
- [ ] Search for where `SyntaxKind::Identifier` is handled
- [ ] Find where `get_type_of_symbol` is called for expressions
- [ ] Understand how node_types cache is populated
- [ ] Locate the FlowNode mapping in Binder

**Task 2**: Understand the data flow
- [ ] How does Checker get FlowNodeId for a given NodeIndex?
- [ ] Is FlowAnalyzer available during expression checking?
- [ ] What's the call sequence from identifier to type resolution?

### Phase 2: Implementation

**Task 3**: Integrate FlowAnalyzer call
- [ ] Modify identifier checking to use `get_flow_type()`
- [ ] Wire up FlowNodeId lookup from Binder
- [ ] Ensure FlowAnalyzer is available in checking context
- [ ] Update caching strategy if needed

### Phase 3: Verification

**Task 4**: Test instanceof narrowing
```typescript
class Animal {}
class Dog extends Animal { bark() {} }
function test(animal: Animal) {
  if (animal instanceof Dog) {
    animal.bark(); // Should work!
  }
}
```

**Task 5**: Test typeof narrowing
**Task 6**: Test discriminant narrowing
**Task 7**: Run regression tests

## Success Criteria

1. ✅ instanceof narrowing works end-to-end
2. ✅ typeof narrowing works end-to-end  
3. ✅ discriminant narrowing works end-to-end
4. ✅ No regression in existing type checking
5. ✅ Acceptable performance

## Risks

### Risk 1: Finding the FlowNodeId
**Concern**: How do we get the FlowNodeId for the current expression?

**Mitigation**: Check if Binder exposes this mapping. If not, may need to add it.

### Risk 2: FlowAnalyzer availability
**Concern**: Is FlowAnalyzer available during expression checking?

**Mitigation**: Check existing usage patterns (e.g., type_checking.rs:2336)

### Risk 3: Performance
**Concern**: Walking flow graph on every identifier could be slow

**Mitigation**: FlowAnalyzer already has caching. The existing cache should work.

## References

- Previous Session: docs/sessions/history/tsz-10.md (Narrowing Infrastructure)
- FlowAnalyzer: src/checker/control_flow.rs:210
- Type Resolution: src/checker/state_type_analysis.rs:751
- Gemini Consultation: 3x consultations completed (following AGENTS.md workflow)

## Next Steps

1. Execute Task 1: Find identifier type-checking code
2. Document the call sequence
3. Identify the exact point where FlowAnalyzer should be called
4. Ask Gemini for implementation guidance if needed

---
**AGENTS.md Reminder**: All changes to `src/checker/` require mandatory two-question Gemini consultation.
