# TypeScript Unsoundness Catalog Implementation Audit

This document provides an analysis of the TypeScript compatibility layer implementation against the 44 known unsoundness rules documented in `specs/TS_UNSOUNDNESS_CATALOG.md`.

## Quick Summary

| Metric | Value |
|--------|-------|
| **Total Rules** | 44 |
| **Fully Implemented** | 9 (20.5%) |
| **Partially Implemented** | 11 (25.0%) |
| **Not Implemented** | 24 (54.5%) |
| **Overall Completion** | 33.0% |

## Phase Breakdown

| Phase | Description | Completion |
|-------|-------------|------------|
| **Phase 1** | Hello World (Bootstrapping) | 80.0% |
| **Phase 2** | Business Logic (Common Patterns) | 40.0% |
| **Phase 3** | Library (Complex Types) | 20.0% |
| **Phase 4** | Feature (Edge Cases) | 25.9% |

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

### ✅ Fully Implemented Rules (9)

| # | Rule | Phase | Files | Notes |
|---|------|-------|-------|-------|
| 1 | The "Any" Type | P1 | `lawyer.rs`, `compat.rs` | `AnyPropagationRules` handles top/bottom semantics |
| 3 | Covariant Mutable Arrays | P1 | `subtype.rs` | Array covariance implemented |
| 6 | Void Return Exception | P1 | `subtype.rs` | `allow_void_return` flag |
| 8 | Unchecked Indexed Access | P4 | `subtype.rs` | `no_unchecked_indexed_access` flag |
| 9 | Legacy Null/Undefined | P4 | `compat.rs`, `subtype.rs` | `strict_null_checks` flag |
| 13 | Weak Type Detection | P4 | `compat.rs` | `violates_weak_type()` implemented |
| 14 | Optionality vs Undefined | P2 | `compat.rs`, `subtype.rs` | `exact_optional_property_types` flag |
| 17 | Instantiation Depth Limit | P4 | `subtype.rs` | Recursion depth check |
| 35 | Recursion Depth Limiter | P4 | `subtype.rs` | Same as #17 |

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

### ❌ Critical Missing Rules (High Priority)

#### Enum Rules (All Missing)

| # | Rule | Description |
|---|------|-------------|
| 7 | Open Numeric Enums | `number` ↔ `Enum` bidirectional assignability |
| 24 | Cross-Enum Incompatibility | Different enum types should be rejected (nominal) |
| 34 | String Enums | String literals NOT assignable to string enums |

**Impact**: Cannot properly type-check code using enums. This is a significant gap.

#### Class Rules (All Missing)

| # | Rule | Description |
|---|------|-------------|
| 5 | Nominal Classes | Private/protected members switch to nominal typing |
| 18 | Static Side Rules | `typeof Class` comparison special handling |
| 43 | Abstract Classes | Abstract class constructor checking |

**Impact**: Class-heavy codebases will have incorrect type checking.

#### Phase 2 Blockers (Missing)

| # | Rule | Description |
|---|------|-------------|
| 10 | Literal Widening | `widen_literal()` for mutable bindings needed |
| 19 | Covariant `this` | `this` in parameters should be covariant |

**Impact**: These block Phase 2 (Business Logic) completion.

## Key Interdependencies

The catalog rules have important dependencies:

1. **Weak Type Detection (#13)** ↔ **Excess Properties (#4)** ↔ **Freshness**
   - All three work together for object literal checks
   - Currently: #13 is ✅, #4 is ⚠️, Freshness is ⚠️

2. **Apparent Types (#12)** → **Object Trifecta (#20)** → **Primitive Boxing (#33)**
   - Primitive type handling chain
   - Currently: All ⚠️ (partially implemented)

3. **Void Return (#6)** → **Constructor Void (#28)**
   - #6 is ✅, #28 is ❌
   - Need to extend void exception to constructors

4. **Enum Open (#7)** → **Cross-Enum (#24)** → **String Enum (#34)**
   - Enum assignability rules build on each other
   - Currently: All ❌ (missing)

## Test Coverage

Estimated test coverage by rule:
- **> 90%**: Rules #1, #3, #9, #13 (4 rules)
- **70-90%**: Rules #6, #8, #14, #17, #35 (5 rules)
- **50-70%**: Rules #2, #20, #31 (3 rules)
- **< 50%**: Rules #4, #11, #12, #15, #16, #21, #30, #33 (8 rules)
- **0%**: All 24 missing rules

## Priority Recommendations

### Immediate (Phase 1 completion)

1. **Complete Rule #20** (Object trifecta):
   - Finish primitive assignability to `Object` interface
   - This is blocking lib.d.ts compatibility

2. **Complete Rule #11** (Error poisoning):
   - Implement `Union(Error, T)` suppression
   - Critical for good error messages

### Short-term (Phase 2 blockers)

3. **Implement Rule #10** (Literal widening):
   - Add `widen_literal()` to lowering pass
   - Essential for `let`/`var` bindings

4. **Implement Rule #19** (Covariant `this`):
   - Make `this` covariant in method parameters
   - Critical for fluent APIs

### Medium-term (Enum support)

5. **Implement Rule #7** (Open Numeric Enums):
   - Add number ↔ Enum bidirectional assignability
   - Foundation for other enum rules

6. **Implement Rule #24** (Cross-Enum):
   - Add nominal checking between different enum types
   - Depends on #7

7. **Implement Rule #34** (String Enums):
   - Make string enums opaque (reject string literals)
   - Independent of numeric enum rules

### Long-term (Class support)

8. **Implement Rule #5** (Nominal Classes):
   - Add private/protected member detection
   - Switch to nominal comparison when present

9. **Implement Rule #18** (Static Side):
   - Add `typeof Class` special handling
   - Handle protected static members nominally

10. **Implement Rule #43** (Abstract Classes):
    - Add abstract class constructor checking
    - Prevent instantiation of abstract classes

## Architecture Notes

The implementation follows the **Judge vs. Lawyer** architecture:

- **Judge** (`SubtypeChecker`): Implements sound set-theoretic subtyping
- **Lawyer** (`CompatChecker` + `AnyPropagationRules`): Applies TypeScript-specific unsound rules

### Key Files

| File | Purpose |
|------|---------|
| `src/solver/compat.rs` | Compatibility layer - applies unsound rules |
| `src/solver/subtype.rs` | Core structural subtype checking (Judge) |
| `src/solver/lawyer.rs` | `AnyPropagationRules` and `FreshnessTracker` |
| `src/solver/unsoundness_audit.rs` | This audit system |
| `src/bin/audit_unsoundness.rs` | CLI tool for running audits |

## References

- TypeScript Unsoundness Catalog: `specs/TS_UNSOUNDNESS_CATALOG.md`
- Solver Architecture: `specs/SOLVER.md`
- Implementation Phases: Catalog Section "Implementation Priority"
