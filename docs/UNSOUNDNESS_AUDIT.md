# TypeScript Unsoundness Catalog Implementation Audit

This document provides an analysis of the TypeScript compatibility layer implementation against the 44 known unsoundness rules documented in `specs/TS_UNSOUNDNESS_CATALOG.md`.

## Quick Summary

| Metric | Value |
|--------|-------|
| **Total Rules** | 44 |
| **Fully Implemented** | 29 (65.9%) |
| **Partially Implemented** | 8 (18.2%) |
| **Not Implemented** | 7 (15.9%) |
| **Overall Completion** | 75.0% |

## Phase Breakdown

| Phase | Description | Completion |
|-------|-------------|------------|
| **Phase 1** | Hello World (Bootstrapping) | 100% (5/5 rules) |
| **Phase 2** | Business Logic (Common Patterns) | 80% (4/5 rules) |
| **Phase 3** | Library (Complex Types) | 80% (4/5 rules) |
| **Phase 4** | Feature (Edge Cases) | 69% (16/29 rules) |

## Running the Audit

A CLI tool is provided to generate audit reports:

```bash
# Show summary report
cargo run --bin audit_unsoundness

# Show full matrix table
cargo run --bin audit_unsoundness -- --matrix

# Show only missing rules
cargo run --bin audit_unsoundness -- --missing

# Show rules by phase
cargo run --bin audit_unsoundness -- --phase 1

# Show rules by status
cargo run --bin audit_unsoundness -- --status full
cargo run --bin audit_unsoundness -- --status partial
cargo run --bin audit_unsoundness -- --status missing
```

## Implementation Status by Rule

### ✅ Fully Implemented Rules (28)

| # | Rule | Phase | Files | Notes |
|---|------|-------|-------|-------|
| 1 | The "Any" Type | P1 | `lawyer.rs`, `compat.rs` | `AnyPropagationRules` handles top/bottom semantics |
| 3 | Covariant Mutable Arrays | P1 | `subtype.rs` | Array covariance implemented |
| 5 | Nominal Classes (Private Members) | P4 | `class_type.rs`, `compat.rs` | Private brand properties for nominal comparison |
| 6 | Void Return Exception | P1 | `subtype.rs` | `allow_void_return` flag |
| 7 | Open Numeric Enums | P4 | `state.rs`, `enum_checker.rs` | Bidirectional number ↔ enum assignability |
| 8 | Unchecked Indexed Access | P4 | `subtype.rs` | `no_unchecked_indexed_access` flag |
| 9 | Legacy Null/Undefined | P4 | `compat.rs`, `subtype.rs` | `strict_null_checks` flag |
| 10 | Literal Widening | P2 | `literals.rs` | `check_literal_to_intrinsic()` |
| 11 | Error Poisoning | P1 | `intern.rs`, `compat.rs`, `subtype.rs` | `Union(Error, T)` normalizes to Error |
| 13 | Weak Type Detection | P4 | `compat.rs` | `violates_weak_type()` implemented |
| 14 | Optionality vs Undefined | P2 | `compat.rs`, `subtype.rs` | `exact_optional_property_types` flag |
| 17 | Instantiation Depth Limit | P4 | `subtype.rs` | Recursion depth check |
| 18 | Class Static Side Rules | P4 | `class_type.rs` | `get_class_constructor_type()` |
| 19 | Covariant `this` Types | P2 | `subtype.rs`, `functions.rs` | `type_contains_this_type()` detection |
| 20 | Object vs object vs {} | P1 | `compat.rs`, `subtype.rs` | All three variants: {} / object / Object |
| 24 | Cross-Enum Incompatibility | P4 | `state.rs` | Nominal enum comparison |
| 25 | Index Signature Consistency | P3 | `objects.rs` | Property-vs-index validation |
| 26 | Split Accessors (Getter/Setter Variance) | P4 | `types.rs`, `objects.rs` | PropertyInfo has read_type and write_type |
| 27 | Homomorphic Mapped Types over Primitives | P4 | `keyof.rs`, `apparent.rs`, `mapped.rs` | keyof of primitives calls apparent_primitive_keyof() |
| 28 | Constructor Void Exception | P4 | `functions.rs` | `allow_void_return` in constructors |
| 29 | Global `Function` Type | P4 | `subtype.rs`, `intrinsics.rs` | `is_callable_type()` |
| 32 | Best Common Type (BCT) Inference | P4 | `infer.rs`, `type_computation.rs` | best_common_type() algorithm |
| 34 | String Enums | P4 | `state.rs` | Opaque string enum handling |
| 35 | Recursion Depth Limiter | P4 | `subtype.rs` | Same as #17 |
| 37 | `unique symbol` | P4 | `subtype.rs` | `TypeKey::UniqueSymbol` handling |
| 40 | Distributivity Disabling | P3 | `lower.rs`, `conditional.rs` | `[T]` tuple wrapping prevents distribution |
| 41 | Key Remapping (`as never`) | P3 | `mapped.rs` | `as never` filters properties (Omit utility) |
| 42 | CFA Invalidation in Closures | P4 | `flow_analysis.rs`, `function_type.rs`, `context.rs` | `inside_closure_depth` tracking, `is_mutable_binding()` check |
| 43 | Abstract Class Instantiation | P4 | `class_type.rs`, `state.rs` | `abstract_constructor_types` tracking |

| # | Rule | Phase | Files | Notes |
|---|------|-------|-------|-------|
| 1 | The "Any" Type | P1 | `lawyer.rs`, `compat.rs` | `AnyPropagationRules` handles top/bottom semantics |
| 3 | Covariant Mutable Arrays | P1 | `subtype.rs` | Array covariance implemented |
| 5 | Nominal Classes (Private Members) | P4 | `class_type.rs`, `compat.rs` | Private brand properties for nominal comparison |
| 6 | Void Return Exception | P1 | `subtype.rs` | `allow_void_return` flag |
| 7 | Open Numeric Enums | P4 | `state.rs`, `enum_checker.rs` | Bidirectional number ↔ enum assignability |
| 8 | Unchecked Indexed Access | P4 | `subtype.rs` | `no_unchecked_indexed_access` flag |
| 9 | Legacy Null/Undefined | P4 | `compat.rs`, `subtype.rs` | `strict_null_checks` flag |
| 10 | Literal Widening | P2 | `literals.rs` | `check_literal_to_intrinsic()` |
| 13 | Weak Type Detection | P4 | `compat.rs` | `violates_weak_type()` implemented |
| 14 | Optionality vs Undefined | P2 | `compat.rs`, `subtype.rs` | `exact_optional_property_types` flag |
| 17 | Instantiation Depth Limit | P4 | `subtype.rs` | Recursion depth check |
| 18 | Class Static Side Rules | P4 | `class_type.rs` | `get_class_constructor_type()` |
| 19 | Covariant `this` Types | P2 | `subtype.rs`, `functions.rs` | `type_contains_this_type()` detection |
| 24 | Cross-Enum Incompatibility | P4 | `state.rs` | Nominal enum comparison |
| 25 | Index Signature Consistency | P3 | `objects.rs` | Property-vs-index validation |
| 26 | Split Accessors (Getter/Setter Variance) | P4 | `types.rs`, `objects.rs` | PropertyInfo has read_type and write_type |
| 27 | Homomorphic Mapped Types over Primitives | P4 | `keyof.rs`, `apparent.rs`, `mapped.rs` | keyof of primitives calls apparent_primitive_keyof() |
| 28 | Constructor Void Exception | P4 | `functions.rs` | `allow_void_return` in constructors |
| 29 | Global `Function` Type | P4 | `subtype.rs`, `intrinsics.rs` | `is_callable_type()` |
| 32 | Best Common Type (BCT) Inference | P4 | `infer.rs`, `type_computation.rs` | best_common_type() algorithm |
| 34 | String Enums | P4 | `state.rs` | Opaque string enum handling |
| 35 | Recursion Depth Limiter | P4 | `subtype.rs` | Same as #17 |
| 37 | `unique symbol` | P4 | `subtype.rs` | `TypeKey::UniqueSymbol` handling |
| 43 | Abstract Class Instantiation | P4 | `class_type.rs`, `state.rs` | `abstract_constructor_types` tracking |

### ⚠️ Partially Implemented Rules (11)

| # | Rule | Phase | Gap |
|---|------|-------|-----|
| 2 | Function Bivariance | P2 | Method vs function differentiation incomplete |
| 4 | Freshness / Excess Properties | P2 | `FreshnessTracker` exists but not integrated |
| 12 | Apparent Members of Primitives | P4 | Full primitive to apparent type lowering needed |
| 15 | Tuple-Array Assignment | P4 | Array to Tuple rejection incomplete |
| 16 | Rest Parameter Bivariance | P4 | `(...args: any[]) => void` incomplete |
| 21 | Intersection Reduction | P3 | Disjoint object literal reduction incomplete |
| 30 | keyof Contravariance | P3 | Union -> Intersection inversion partial |
| 31 | Base Constraint Assignability | P4 | Type parameter checking partial |
| 33 | Object vs Primitive boxing | P4 | `Intrinsic::Number` vs `Ref(Symbol::Number)` distinction |

### ❌ Remaining Missing Rules (6)

| # | Rule | Phase | Description |
|---|------|-------|-------------|
| 22 | Template String Expansion Limits | P4 | Cardinality check (abort > 100k items) |
| 23 | Comparison Operator Overlap | P4 | `compute_overlap(A, B)` query needed |
| 36 | JSX Intrinsic Lookup | P4 | Case-sensitive tag resolution |
| 38 | Correlated Unions | P4 | Cross-product limitation |
| 39 | `import type` Erasure | P4 | Value vs type space check |
| 44 | Module Augmentation Merging | P4 | Interface merging across modules |

## Key Interdependencies

The catalog rules have important dependencies:

1. **Weak Type Detection (#13)** ↔ **Excess Properties (#4)** ↔ **Freshness**
   - All three work together for object literal checks
   - Currently: #13 is ✅, #4 is ⚠️, Freshness is ⚠️

2. **Apparent Types (#12)** → **Object Trifecta (#20)** → **Primitive Boxing (#33)**
   - Primitive type handling chain
   - Currently: All ⚠️ (partially implemented)

3. **Void Return (#6)** → **Constructor Void (#28)**
   - Both #6 and #28 are now ✅ (fully implemented)

4. **Enum Open (#7)** → **Cross-Enum (#24)** → **String Enum (#34)**
   - Enum assignability rules build on each other
   - All three are now ✅ (fully implemented)

## Test Coverage

Estimated test coverage by rule:
- **> 90%**: Rules #1, #3, #9, #11, #13, #26, #40 (7 rules)
- **70-90%**: Rules #5, #6, #7, #8, #10, #14, #17, #18, #19, #20, #24, #25, #27, #28, #29, #32, #34, #35, #37, #41, #43 (21 rules)
- **50-70%**: Rules #2, #31 (2 rules)
- **< 50%**: Rules #4, #12, #15, #16, #21, #30, #33 (7 rules)
- **0%**: All 7 remaining missing rules

## Priority Recommendations

### Immediate (Phase 3 completion)

1. **Complete Rule #21** (Intersection Reduction):
   - Finish disjoint object literal reduction to never
   - Important for impossible type detection

2. **Complete Rule #30** (keyof Contravariance):
   - Implement Union -> Intersection inversion for keyof
   - Critical for Pick and Omit utility types

### Short-term (Phase 2/4 completion)

3. **Complete Rule #4** (Freshness/Excess Properties):
   - Integrate FreshnessTracker with type lowering
   - Critical for object literal validation

4. **Complete Rule #2** (Function Bivariance):
   - Implement interface call signature bivariance
   - Location: src/solver/subtype_rules/functions.rs:569-623

### Medium-term (Phase 4 completion)

5. **Implement Rule #22** (Template String Expansion Limits):
   - Add cardinality check for template literal unions
   - Prevents performance issues with large unions

## Architecture Notes

The implementation follows the **Judge vs. Lawyer** architecture:

- **Judge** (`SubtypeChecker`): Implements sound set-theoretic subtyping
- **Lawyer** (`CompatChecker` + `AnyPropagationRules`): Applies TypeScript-specific unsound rules

### Key Files

| File | Purpose |
|------|---------|
| `src/solver/compat.rs` | Compatibility layer - applies unsound rules |
| `src/solver/subtype.rs` | Core structural subtype checking (Judge) |
| `src/solver/subtype_rules/*.rs` | Organized subtype rules by category |
| `src/solver/lawyer.rs` | `AnyPropagationRules` and `FreshnessTracker` |
| `src/solver/unsoundness_audit.rs` | This audit system |
| `src/bin/audit_unsoundness.rs` | CLI tool for running audits |
| `src/checker/state.rs` | Enum and class assignability overrides |
| `src/checker/class_type.rs` | Class instance and constructor types |
| `src/checker/enum_checker.rs` | Enum type utilities |

## References

- TypeScript Unsoundness Catalog: `specs/TS_UNSOUNDNESS_CATALOG.md`
- Solver Architecture: `specs/SOLVER.md`
- Implementation Phases: Catalog Section "Implementation Priority"

---

**Last Updated**: 2025-01-25
**Next Review**: After Phase 3 completion (currently 80%)
