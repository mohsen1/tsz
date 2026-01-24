# TypeScript Unsoundness Catalog Implementation Audit

This document provides an analysis of the TypeScript compatibility layer implementation against the 44 known unsoundness rules documented in `specs/TS_UNSOUNDNESS_CATALOG.md`.

## Quick Summary

| Metric | Value |
|--------|-------|
| **Total Rules** | 44 |
| **Fully Implemented** | 21 (47.7%) |
| **Partially Implemented** | 11 (25.0%) |
| **Not Implemented** | 12 (27.3%) |
| **Overall Completion** | 60.2% |

## Phase Breakdown

| Phase | Description | Completion |
|-------|-------------|------------|
| **Phase 1** | Hello World (Bootstrapping) | 80.0% |
| **Phase 2** | Business Logic (Common Patterns) | 80.0% |
| **Phase 3** | Library (Complex Types) | 40.0% |
| **Phase 4** | Feature (Edge Cases) | 56.9% |

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

### ✅ Fully Implemented Rules (21)

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
| 28 | Constructor Void Exception | P4 | `functions.rs` | `allow_void_return` in constructors |
| 29 | Global `Function` Type | P4 | `subtype.rs`, `intrinsics.rs` | `is_callable_type()` |
| 34 | String Enums | P4 | `state.rs` | Opaque string enum handling |
| 35 | Recursion Depth Limiter | P4 | `subtype.rs` | Same as #17 |
| 37 | `unique symbol` | P4 | `subtype.rs` | `TypeKey::UniqueSymbol` handling |
| 43 | Abstract Class Instantiation | P4 | `class_type.rs`, `state.rs` | `abstract_constructor_types` tracking |

### ⚠️ Partially Implemented Rules (11)

| # | Rule | Phase | Gap |
|---|------|-------|-----|
| 2 | Function Bivariance | P2 | Method vs function differentiation incomplete |
| 4 | Freshness / Excess Properties | P2 | `FreshnessTracker` exists but not integrated |
| 11 | Error Poisoning | P1 | `Union(Error, T)` suppression not implemented |
| 12 | Apparent Members of Primitives | P4 | Full primitive to apparent type lowering needed |
| 15 | Tuple-Array Assignment | P4 | Array to Tuple rejection incomplete |
| 16 | Rest Parameter Bivariance | P4 | `(...args: any[]) => void` incomplete |
| 20 | Object vs object vs {} | P1 | Primitive assignability to Object incomplete |
| 21 | Intersection Reduction | P3 | Disjoint object literal reduction incomplete |
| 30 | keyof Contravariance | P3 | Union -> Intersection inversion partial |
| 31 | Base Constraint Assignability | P4 | Type parameter checking partial |
| 33 | Object vs Primitive boxing | P4 | `Intrinsic::Number` vs `Ref(Symbol::Number)` distinction |

### ❌ Remaining Missing Rules (12)

| # | Rule | Phase | Description |
|---|------|-------|-------------|
| 22 | Template String Expansion Limits | P4 | Cardinality check (abort > 100k items) |
| 23 | Comparison Operator Overlap | P4 | `compute_overlap(A, B)` query needed |
| 26 | Split Accessors | P4 | Getter/setter variance (read_type/write_type) |
| 27 | Homomorphic Mapped Types over Primitives | P4 | Map over apparent types |
| 32 | Best Common Type (BCT) Inference | P4 | Array literal type inference algorithm |
| 36 | JSX Intrinsic Lookup | P4 | Case-sensitive tag resolution |
| 38 | Correlated Unions | P4 | Cross-product limitation |
| 39 | `import type` Erasure | P4 | Value vs type space check |
| 40 | Distributivity Disabling | P3 | `[T] extends [U]` tuple wrapping |
| 41 | Key Remapping (`as never`) | P3 | Mapped type property removal |
| 42 | CFA Invalidation in Closures | P4 | Narrowing reset for mutable bindings |
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
- **> 90%**: Rules #1, #3, #9, #13 (4 rules)
- **70-90%**: Rules #5, #6, #7, #8, #10, #14, #17, #18, #19, #24, #25, #28, #29, #34, #35, #37, #43 (17 rules)
- **50-70%**: Rules #2, #20, #31 (3 rules)
- **< 50%**: Rules #4, #11, #12, #15, #16, #21, #30, #33 (8 rules)
- **0%**: All 12 remaining missing rules

## Priority Recommendations

### Immediate (Phase 1 completion)

1. **Complete Rule #20** (Object trifecta):
   - Finish primitive assignability to `Object` interface
   - This is blocking lib.d.ts compatibility

2. **Complete Rule #11** (Error poisoning):
   - Implement `Union(Error, T)` suppression
   - Critical for good error messages

### Short-term (Phase 3 completion)

3. **Implement Rule #40** (Distributivity Disabling):
   - Handle `[T] extends [U]` tuple wrapping
   - Important for Exclude/Extract utility types

4. **Implement Rule #41** (Key Remapping):
   - Handle `as never` in mapped types
   - Important for Omit utility type

### Medium-term (Phase 4 completion)

5. **Implement Rule #22** (Template String Expansion Limits):
   - Add cardinality check for template literal unions
   - Prevents performance issues with large unions

6. **Implement Rule #42** (CFA Invalidation in Closures):
   - Reset narrowing for mutable bindings in closures
   - Important for correct flow analysis

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

**Last Updated**: 2026-01-24
**Next Review**: After Phase 3 completion
