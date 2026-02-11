# Phase 4 Planning - Visitor Pattern Expansion

**Date**: February 2026
**Status**: Planning Stage (Ready for Approval)
**Scope**: Expand visitor pattern adoption across codebase

---

## Phase 4 Overview

Building on the foundation established in Phases 0-3, Phase 4 will expand the visitor pattern adoption and helper function library. This phase offers multiple implementation paths depending on priorities.

---

## Core Options for Phase 4

### Option A: Helper Library Expansion ⭐ RECOMMENDED START
**Focus**: Create additional extraction and analysis helpers

**Difficulty**: Easy
**Timeline**: 3-5 days
**Risk**: Low
**Value**: Medium-High
**Foundation for**: Options B, C, D

#### New Helper Functions to Create

1. **Composite Type Extractors**
   ```rust
   /// Extract either array element or tuple elements
   pub fn extract_array_or_tuple(db: &dyn TypeDatabase, type_id: TypeId)
       -> Option<Either<TypeId, TupleListId>>

   /// Extract union OR intersection members
   pub fn extract_composite_members(db: &dyn TypeDatabase, type_id: TypeId)
       -> Option<TypeListId>
   ```

2. **Conditional Extractors**
   ```rust
   /// Extract shape if type is object (regular or with index)
   pub fn extract_any_object_shape(db: &dyn TypeDatabase, type_id: TypeId)
       -> Option<ObjectShapeId>

   /// Extract element if array, or first element if tuple
   pub fn extract_first_collection_element(db: &dyn TypeDatabase, type_id: TypeId)
       -> Option<TypeId>
   ```

3. **Traversal Helpers**
   ```rust
   /// Apply function to all type members in union/intersection
   pub fn for_each_composite_member<F>(
       db: &dyn TypeDatabase,
       type_id: TypeId,
       f: F
   ) -> bool
   where F: FnMut(TypeId) -> bool

   /// Check if any type member in union/intersection matches predicate
   pub fn any_composite_member_matches<F>(
       db: &dyn TypeDatabase,
       type_id: TypeId,
       f: F
   ) -> bool
   where F: Fn(TypeId) -> bool
   ```

#### Refactoring Targets Using New Helpers

1. `is_contextually_sensitive()` (line ~1728)
   - Currently has multiple type checks
   - Can use new composite extractors
   - Medium complexity refactoring

2. Union/Intersection handling in type constraint functions
   - Multiple functions iterate over members
   - New traversal helpers would simplify these
   - Medium complexity refactoring (5-10 functions)

#### Success Criteria
- [ ] 8-10 new helper functions created
- [ ] All functions well-documented with examples
- [ ] 3-5 existing functions refactored to use new helpers
- [ ] All tests passing (3549/3549)
- [ ] Zero regressions

---

### Option B: Selective Function Refactoring
**Focus**: Refactor 5-10 operations.rs functions with visitor pattern

**Difficulty**: Medium
**Timeline**: 1-2 weeks
**Risk**: Medium (complex logic requires careful handling)
**Value**: High (cleaner, more maintainable code)

#### High-Value Refactoring Candidates

1. **type_contains_placeholder()** (line 1514)
   - **Current**: Recursive traversal with match on all type keys
   - **Issue**: Duplicates traversal logic for each type category
   - **Opportunity**: Create visitor-based traversal helpers
   - **Effort**: Medium
   - **Value**: High (used in type inference)

2. **is_contextually_sensitive()** (line ~1728)
   - **Current**: Checks multiple type categories for contextual sensitivity
   - **Opportunity**: Use composite helper functions
   - **Effort**: Medium
   - **Value**: Medium

3. **type_contains_reference()** (likely similar to type_contains_placeholder)
   - **Current**: Recursive type traversal
   - **Opportunity**: Generalize traversal pattern
   - **Effort**: Medium
   - **Value**: Medium

4. **resolve_call dispatch logic** (line 340)
   - **Current**: Match on multiple type keys
   - **Opportunity**: Use visitor for dispatch
   - **Effort**: Hard (complex call resolution logic)
   - **Value**: Medium-High

5. **Constraint collection functions**
   - Multiple functions that traverse types
   - Could benefit from traversal helpers
   - **Effort**: Medium (multiple functions)
   - **Value**: High (important for type inference)

#### Implementation Strategy

1. Start with simpler functions (is_contextually_sensitive)
2. Create helper functions as needed
3. Test thoroughly after each refactoring
4. Document patterns for similar functions
5. Move to more complex functions

#### Success Criteria
- [ ] 5-10 functions refactored
- [ ] 30-50 lines of direct TypeKey matching eliminated
- [ ] All tests passing
- [ ] Code quality improved (Clippy, formatting)
- [ ] Refactorings well-documented

---

### Option C: Module Refactoring (Larger Scope)
**Focus**: Apply visitor patterns to other modules

**Difficulty**: Hard
**Timeline**: 2-3 weeks
**Risk**: Medium-High (larger scope, more surface area)
**Value**: Very High (major code improvement across modules)

#### Target Modules

1. **compat.rs** (1,637 LOC)
   - 40-60 match statements on TypeKey
   - Heavy pattern matching for type compatibility
   - Good candidate for helper functions
   - **Estimated**: 3-5 days

2. **narrowing.rs** (3,087 LOC)
   - Type narrowing logic
   - Multiple type cases to handle
   - Could benefit from conditional extractors
   - **Estimated**: 5-7 days

3. **subtype.rs** (4,520 LOC)
   - Largest type system module
   - Heavy use of type matching
   - Most impact potential
   - **Estimated**: 7-10 days

#### Approach

1. Start with compat.rs (smallest, lower risk)
2. Document patterns found
3. Apply to narrowing.rs
4. Tackle subtype.rs with accumulated knowledge

#### Success Criteria
- [ ] compat.rs refactored (20+ functions)
- [ ] narrowing.rs refactored (15+ functions)
- [ ] 100+ TypeKey matches eliminated
- [ ] All tests passing
- [ ] Documentation of patterns

---

### Option D: Visitor Pattern Enhancements
**Focus**: Improve visitor pattern infrastructure

**Difficulty**: Medium-High
**Timeline**: 1-2 weeks
**Risk**: Medium (architectural changes)
**Value**: High (enables more powerful usage)

#### Enhancement Ideas

1. **Trait-Based Visitor**
   ```rust
   pub trait TypeVisitor {
       fn visit_union(&mut self, members: TypeListId) -> VisitResult;
       fn visit_object(&mut self, shape: ObjectShapeId) -> VisitResult;
       // ... other type categories
   }
   ```
   - Allows multiple implementations
   - Better for complex traversals
   - Can replace switch-statement-like code

2. **Builder Pattern for Visitors**
   ```rust
   TypeVisitorBuilder::new(db, type_id)
       .with_union_handler(|members| { /* ... */ })
       .with_object_handler(|shape| { /* ... */ })
       .with_default_handler(|_| { /* ... */ })
       .build()
   ```
   - More ergonomic for complex visitors
   - Clear intent in code

3. **Recursive Visitor**
   ```rust
   pub struct RecursiveTypeVisitor { /* ... */ }
   // Automatically traverses nested types
   // Calls visitor for each type encountered
   ```
   - Solves recursive traversal pattern
   - Used in type_contains_placeholder, etc.

4. **Performance Optimizations**
   - Memoized visitor results
   - Lazy classification
   - Visit result caching

#### Success Criteria
- [ ] At least one enhancement implemented
- [ ] Backward compatible with existing visitor
- [ ] Documented with examples
- [ ] Used in at least 2 refactored functions
- [ ] Performance neutral or better

---

## Recommended Phase 4 Sequence

### Stage 1: Foundation (Week 1)
1. **Option A - Helper Library Expansion**
   - Create 8-10 new helper functions
   - Focus on composite extractors and traversal
   - Light refactoring (2-3 functions)
   - Goal: Proven patterns for more helpers

2. **Quick Win**: Apply helpers to 1-2 simple functions
   - Build confidence
   - Demonstrate value
   - Get feedback

### Stage 2: Selective Refactoring (Week 2)
3. **Option B - Selective Function Refactoring**
   - Start with medium-complexity functions
   - Use new helpers from Stage 1
   - 5-10 functions targeted
   - Goal: Broader adoption demonstration

4. **Stretch Goal**: Begin module refactoring (compat.rs)
   - Low-risk module to start with
   - Use accumulated patterns
   - Goal: Prove patterns work at larger scale

### Stage 3: Consolidation (Week 3, if time)
5. **Option D - Visitor Pattern Enhancements**
   - Implement trait-based visitor if time permits
   - Create supporting infrastructure
   - Goal: Better foundation for future work

---

## Success Metrics for Phase 4

### Code Quality
| Metric | Target | How to Measure |
|--------|--------|----------------|
| **Test Pass Rate** | 100% | cargo test --lib |
| **Clippy Warnings** | 0 | cargo clippy |
| **New TypeKey matches eliminated** | 30+ | grep analysis |
| **Breaking Changes** | 0 | Review git diff |

### Scope Completion
| Metric | Target | How to Measure |
|--------|--------|----------------|
| **New helpers created** | 8+ | Code review |
| **Functions refactored** | 10+ | Git log analysis |
| **Documentation** | Comprehensive | Review docs/ |

### Developer Experience
| Metric | Target | Assessment |
|--------|--------|-----------|
| **Pattern clarity** | Clear | Code review |
| **Adoption ease** | High | Developer feedback |
| **Example quality** | Excellent | Doc review |

---

## Risk Mitigation

### Testing Strategy
1. Run full test suite after each helper creation
2. Run tests after each function refactoring
3. Run conformance suite after module work
4. Automated regression detection

### Code Review Strategy
1. Small, semantically-meaningful commits
2. Clear commit messages explaining changes
3. Documentation updates in same commit
4. Examples provided for new patterns

### Rollback Plan
1. Each phase can be reverted independently
2. Git history preserves original code
3. No destructive operations used
4. Backwards compatibility maintained

---

## Timeline Estimate

| Stage | Activities | Estimated Time |
|-------|-----------|-----------------|
| **Week 1** | Helper library expansion + quick wins | 3-5 days |
| **Week 2** | Selective function refactoring + module start | 3-5 days |
| **Week 3** | Visitor enhancements + documentation | 2-3 days |
| **Documentation** | Reports and learnings | 1-2 days |
| **Total** | Phase 4 Complete | 2-3 weeks |

---

## Decision Points

### Before Starting Phase 4
- [ ] Approval of Phase 3 work
- [ ] Consensus on Option A as starting point
- [ ] Resource allocation (time commitment)
- [ ] Priority: breadth (more functions) vs depth (better helpers)?

### Mid-Phase Review (after Stage 1)
- [ ] Evaluate helper effectiveness
- [ ] Decide whether to continue with Option B or pivot
- [ ] Assess timeline feasibility
- [ ] Gather developer feedback on patterns

### Phase 4 Completion
- [ ] All targets achieved?
- [ ] Code quality maintained?
- [ ] Ready for Phase 5 or broader adoption?

---

## Post-Phase 4 Considerations

### Phase 5 Opportunities
1. **Broader Module Refactoring**
   - Apply learnings to more modules
   - Consider expression_ops.rs, instantiate.rs, etc.

2. **API Stability**
   - Lock down visitor API
   - Commit to long-term support
   - Version for semantic compatibility

3. **Developer Guide**
   - "How to Use Visitor Pattern in Type System"
   - Code patterns and anti-patterns
   - Performance considerations

4. **Performance Analysis**
   - Measure impact of refactorings
   - Optimize hot paths
   - Benchmark compile times

---

## Conclusion

Phase 4 offers multiple pathways to expand visitor pattern adoption. The recommended sequence (A → B → D) builds progressively from foundation to broader application, with clear milestones and exit points.

The codebase is well-positioned for Phase 4:
- ✅ Foundation solid (Phases 0-3 complete)
- ✅ Patterns proven in practice
- ✅ All tests passing
- ✅ Zero technical debt from previous phases
- ✅ Clear opportunities identified

Proceeding with Phase 4 will significantly improve codebase maintainability and establish visitor pattern as the standard for type system code.

---

## Sign-Off

**Planning Status**: ✅ Complete
**Recommended Start**: Week following Phase 3 completion
**Confidence Level**: ⭐⭐⭐⭐⭐ Very High
**Ready to Proceed**: Yes ✅

---

**Generated**: February 2026
**For**: Type System Refactoring Project
**Authored by**: Claude Code Refactoring Agent
