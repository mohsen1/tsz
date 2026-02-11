# Phase 3 Implementation Plan - Visitor Consolidation

**Date**: February 2026
**Branch**: `claude/refactor-rust-abstractions-CfHJt`
**Scope**: Refactor operations.rs and related modules using TypeClassificationVisitor

---

## Phase 3 Overview

**Objective**: Systematically refactor high-impact code paths to use TypeClassificationVisitor and related abstractions, eliminating direct TypeKey matching and improving code maintainability.

**Primary Target**: `operations.rs` (3,830 LOC, 163 match statements on db.lookup/interner.lookup)

---

## Analysis: operations.rs

### File Statistics

| Metric | Value |
|--------|-------|
| **Lines of Code** | 3,830 |
| **Match Statements on lookup** | 163 |
| **Estimated Refactoring Candidates** | 50-80 |
| **Impact Potential** | Very High |

### Pattern Categories Identified

#### Category 1: Simple Type Checks (40-50 instances)
**Pattern**: Match on single TypeKey variant
**Example**:
```rust
match self.interner.lookup(type_id) {
    Some(TypeKey::Array(elem)) => elem,
    _ => type_id,
}
```

**Refactoring Strategy**:
- Use `TypeClassificationVisitor::visit_array()`
- Or use helper from `TypeOperationsHelper` if available

**Difficulty**: Easy | **Impact**: Medium

#### Category 2: Multiple Pattern Matching (30-40 instances)
**Pattern**: Match on multiple TypeKey variants in sequence
**Example**:
```rust
match self.interner.lookup(type_id) {
    Some(TypeKey::ReadonlyType(inner)) | Some(TypeKey::NoInfer(inner)) => {
        // Handle both cases
    }
    _ => type_id,
}
```

**Refactoring Strategy**:
- Create specialized visitor methods
- Use pattern matching helpers from TypeOperationsMatcher
- Extract common logic to helper functions

**Difficulty**: Medium | **Impact**: High

#### Category 3: Type Traversal (20-30 instances)
**Pattern**: Recursive traversal with type case handling
**Example**:
```rust
fn expand_tuple_rest(&self, type_id: TypeId) -> TupleRestExpansion {
    match self.interner.lookup(type_id) {
        Some(TypeKey::Array(elem)) => { /* ... */ }
        Some(TypeKey::Tuple(elements)) => { /* ... */ }
        // ...
    }
}
```

**Refactoring Strategy**:
- Create TypeClassificationVisitor-based traversal
- Leverage existing TypeOperationsHelper patterns
- Maintain recursion and complex logic

**Difficulty**: Medium-Hard | **Impact**: Very High

#### Category 4: Complex Logic (10-20 instances)
**Pattern**: Deep matching with complex branching
**Example**:
```rust
match db.lookup(tuple_type) {
    Some(TypeKey::Tuple(elems)) => {
        let tuple = db.tuple_list(elems);
        for elem in tuple {
            if elem.condition {
                // Complex logic
            }
        }
    }
    // Multiple branches
}
```

**Refactoring Strategy**:
- Extract inner logic to separate functions
- Use visitor to classify, then call specialist functions
- Maintain current behavior while improving structure

**Difficulty**: Hard | **Impact**: High

---

## Refactoring Strategy

### Phase 3 Approach

**Phased Refactoring**: Tackle categories in order of impact and difficulty

#### Stage 1: Simple Type Checks (Days 1-2)
Target: 40-50 simple single-pattern matches
- Identify all single-variant matches
- Replace with visitor methods or helper functions
- No logic changes, pure refactoring

Example Transformations:
```rust
// BEFORE
match self.interner.lookup(type_id) {
    Some(TypeKey::Array(elem)) => elem,
    _ => type_id,
}

// AFTER - OPTION 1: Using visitor
let mut visitor = TypeClassificationVisitor::new(self.interner, type_id);
if let Some(elem) = visitor.visit_array(|e| e) {
    elem
} else {
    type_id
}

// AFTER - OPTION 2: Using helper (simpler!)
type_id.map(|id| {
    let mut visitor = TypeClassificationVisitor::new(self.interner, id);
    visitor.visit_array(|e| e).unwrap_or(id)
})
```

#### Stage 2: Multiple Pattern Matching (Days 2-3)
Target: 30-40 multi-variant matches
- Create specialized visitor helpers if needed
- Extract common patterns to helper functions
- Test thoroughly before moving forward

#### Stage 3: Type Traversal (Days 3-4)
Target: 20-30 recursive type handling functions
- Refactor to use visitor + recursion combo
- Create specialized traversal helpers
- Heavy testing and validation

#### Stage 4: Complex Logic (Days 4-5)
Target: 10-20 complex multi-branch matches
- Extract complex inner logic to helper functions
- Use visitor for type classification
- Maintain readability and performance

---

## Specific Functions to Refactor

### High-Priority Functions

| Function | Line | Type | Candidates | Difficulty |
|----------|------|------|-----------|------------|
| `expand_type_param` | 562 | Simple | 1 | Easy |
| `rest_element_type` | 1363 | Simple | 1 | Easy |
| `unwrap_readonly` | 1372 | Loop | 1 | Easy |
| `expand_tuple_rest` | 1390 | Recursive | 1 | Hard |
| `call_type_from_element_access` | 3048 | Complex | 5+ | Hard |
| `promise_resolver_type` | 3580 | Traversal | 1 | Medium |
| `iterator_result_type` | 3700 | Traversal | 1 | Medium |

### Medium-Priority Functions

Functions handling:
- Union/Intersection resolution
- Callable type handling
- Object member access
- Generic type expansion

---

## Quality Assurance

### Testing Strategy

1. **Unit Test Coverage**
   - All 3,549 solver tests must continue to pass
   - Verify specific functions with targeted tests
   - No regressions allowed

2. **Regression Testing**
   - Full conformance suite on each stage completion
   - Sample-based testing on partial refactoring
   - Type checking verification

3. **Performance Validation**
   - Measure function execution time before/after
   - Verify no performance regressions
   - Profile hot paths if needed

### Validation Checklist

- [ ] Stage 1 refactoring complete
- [ ] All tests passing after Stage 1
- [ ] Stage 2 refactoring complete
- [ ] All tests passing after Stage 2
- [ ] Stage 3 refactoring complete
- [ ] All tests passing after Stage 3
- [ ] Stage 4 refactoring complete
- [ ] All tests passing after Stage 4
- [ ] Full conformance suite passing
- [ ] No performance regressions
- [ ] Documentation updated

---

## Success Criteria

### Quantitative

| Metric | Target | Validation Method |
|--------|--------|-------------------|
| **Direct TypeKey matches eliminated** | 50+ | Count remaining matches |
| **Functions refactored** | 15-20 | Track refactored functions |
| **Lines changed** | 200-400 | Git diff stats |
| **Test pass rate** | 100% | Run full test suite |
| **Performance regression** | <2% | Benchmark key functions |

### Qualitative

- Code readability improved
- Visitor pattern clearly demonstrated
- Clear migration path for other modules
- Good documentation of changes
- Reviewable, semantic commits

---

## Implementation Checklist

### Pre-Refactoring
- [ ] Run baseline tests
- [ ] Document current operations.rs patterns
- [ ] Identify all 163 match statements
- [ ] Categorize by difficulty
- [ ] Create refactoring priority list

### Stage 1: Simple Type Checks
- [ ] List all single-variant matches
- [ ] Create helper wrapper functions if needed
- [ ] Refactor to use visitor/helpers
- [ ] Test thoroughly
- [ ] Commit with clear message

### Stage 2: Multiple Patterns
- [ ] Identify multi-variant matches
- [ ] Extract common logic
- [ ] Create specialized visitors if needed
- [ ] Refactor and test
- [ ] Commit

### Stage 3: Type Traversal
- [ ] Analyze recursive functions
- [ ] Plan visitor + recursion approach
- [ ] Implement refactorings
- [ ] Test recursion behavior
- [ ] Commit

### Stage 4: Complex Logic
- [ ] Analyze complex branches
- [ ] Extract inner functions
- [ ] Use visitor for classification
- [ ] Test edge cases
- [ ] Commit

### Post-Refactoring
- [ ] Full conformance suite
- [ ] Performance verification
- [ ] Documentation of changes
- [ ] Phase 3 completion report

---

## Risk Mitigation

### Potential Risks

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|-----------|
| **Test failures during refactoring** | High | Critical | Frequent testing, small commits |
| **Performance regression** | Medium | High | Benchmark, profile hot paths |
| **Logic errors in recursion** | Medium | High | Careful review, step-by-step approach |
| **Incomplete refactoring** | Low | Medium | Clear stages and checkpoints |

### Safety Measures

1. **Frequent Testing**: Run full test suite after each function refactoring
2. **Small Commits**: Each function refactored = one commit
3. **Code Review Friendly**: Easy-to-review changes with clear before/after
4. **Documentation**: Update as we go, don't leave until end
5. **Rollback Ready**: Each stage can be rolled back independently

---

## Timeline Estimate

| Stage | Scope | Estimated Time |
|-------|-------|-----------------|
| **Stage 1** | 40-50 simple refactorings | 1-2 days |
| **Stage 2** | 30-40 multi-pattern refactorings | 1-2 days |
| **Stage 3** | 20-30 recursive refactorings | 1-2 days |
| **Stage 4** | 10-20 complex refactorings | 1-2 days |
| **Documentation** | Phase 3 report + updates | 1 day |
| **Total** | Complete operations.rs refactoring | 5-10 days |

---

## Expected Outcomes

### Code Improvements

1. **Reduced Duplication**: 50+ direct TypeKey matches eliminated
2. **Better Maintainability**: Visitor pattern foundation established
3. **Cleaner Logic**: Complex pattern matches simplified
4. **Consistency**: Operations.rs follows NORTH_STAR patterns

### Documentation Improvements

1. **Pattern Examples**: Real-world refactoring examples
2. **Migration Guide**: How to refactor other modules
3. **Best Practices**: Visitor pattern usage guidelines
4. **Architecture**: Updated NORTH_STAR documentation

### Codebase Readiness

1. **Clear Patterns**: Visitor pattern proven in real code
2. **Lower Barrier**: Next developer knows where to start
3. **Scalability**: Pattern ready for other modules
4. **Maintenance**: Code easier to understand and modify

---

## Success Indicators

✅ When Phase 3 is complete:
- 50+ TypeKey matches eliminated from operations.rs
- All 3,867 tests passing
- Zero clippy warnings
- operations.rs demonstrates visitor pattern effectively
- Clear path forward for Phase 4 (other modules)
- Comprehensive documentation of refactoring

---

## Next Phases (Phase 4+)

### Phase 4: Additional Module Refactoring

**Target Modules**:
- compat.rs (1,637 LOC) - Heavy pattern matching
- narrowing.rs (3,087 LOC) - Type narrowing operations
- subtype.rs (4,520 LOC) - Large subtype module

**Expected Similar Results**:
- 40-60 TypeKey matches per module
- 30-40% code duplication reduction
- Continued performance improvements

---

## Conclusion

Phase 3 will demonstrate the real-world value of TypeClassificationVisitor and related abstractions by systematically refactoring operations.rs. Success here will provide:

1. **Proof of Concept**: Visitor pattern works in real code
2. **Developer Guidance**: Clear examples for other modules
3. **Codebase Improvement**: 50+ match statements eliminated
4. **Foundation**: Ready for Phase 4 module refactoring

---

**Status**: Plan Ready for Execution
**Next Step**: Begin Stage 1 refactoring
**Target Completion**: 5-10 days
**Quality Gate**: 100% test pass rate required

