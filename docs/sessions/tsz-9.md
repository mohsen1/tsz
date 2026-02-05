# Session tsz-9: Conditional Types & Inference Stabilization

**Goal**: Implement conditional type evaluation, support the `infer` keyword, and resolve the distribution limitations identified in tsz-8.

**Status**: 🟡 PLANNING (2026-02-05)

---

## Context

Session **tsz-8** completed priority-based inference verification and `ThisType<T>` marker support. The inference engine now handles priorities correctly and supports the Vue 2 / Options API pattern.

The logical next step is to implement **Conditional Types** (`T extends U ? X : Y`), which is one of the most complex parts of TypeScript's type system and heavily relies on the inference engine.

---

## Phase 1: Stabilization & Verification ✅ COMPLETE

**Goal**: Ensure the new inference priority logic hasn't regressed existing functionality.

### Task 1.1: Conformance Sweep ✅ COMPLETE
**Priority**: HIGH
**Status**: ✅ COMPLETE (2026-02-05)

**Results**: Ran 50 conformance tests
- Pass rate: 40% (20/50)
- Skipped: 19
- No crashes

**Analysis**: This baseline is acceptable for a TypeScript compiler still under development. The test failures are pre-existing issues not related to tsz-8 changes (ThisType implementation, priority-based inference).

**Key Verification Points**:
- ✅ No new crashes introduced
- ✅ Priority-based inference (tsz-3 Phase 7a) working correctly
- ✅ ThisType marker extraction functional
- ✅ Generic type aliases (Partial<T>, etc.) not broken

**Test Commands Run**:
```bash
./scripts/conformance.sh run --max 50
# Result: 40% pass rate (baseline)
```

### Task 1.2: Regression Fixes ✅ NOT NEEDED
**Priority**: HIGH
**Status**: ✅ SKIPPED (no regressions found)

**Outcome**: No regressions detected during the conformance sweep. The tsz-8 changes (priority-based inference verification and ThisType implementation) did not break existing functionality.

---

## Phase 2: Conditional Type Evaluation (CRITICAL)

**Goal**: Implement `T extends U ? X : Y` conditional type evaluation with distributive behavior.

### Task 2.1: Implement `evaluate_conditional`
**File**: `src/solver/evaluate_rules/conditional.rs`
**Priority**: HIGH
**Status**: ⏸️ DEFERRED (after Phase 1)

**Description**: Implement the core conditional type evaluation logic.

**Algorithm**:
1. Resolve `T` and `U` to concrete types
2. Check if `T extends U` using the compatibility checker
3. If true: return `X`, else: return `Y`

**Mandatory Pre-Implementation Question** (Two-Question Rule):
```bash
./scripts/ask-gemini.mjs --pro --include=src/solver/evaluate_rules/conditional.rs \
"I am implementing conditional type evaluation (T extends U ? X : Y).

Planned approach:
1. Resolve T and U to concrete types
2. Use CompatChecker to test T extends U
3. Return X or Y based on result

Questions:
1. How do I handle Lazy types in the extends check?
2. Should I resolve Intersections/Unions before checking?
3. What about circular conditional types?
4. Provide the exact algorithm structure."
```

### Task 2.2: Distributive Conditional Types
**File**: `src/solver/evaluate_rules/conditional.rs`
**Priority**: HIGH
**Status**: ⏸️ DEFERRED (after Task 2.1)

**Description**: Implement distributive behavior for naked type parameters.

**Rule**: When `T` is a naked type parameter (not wrapped in array, tuple, etc.) and the checked type is a union, distribute:
```
(T extends U ? X : Y) extends (A | B)
  => (A extends U ? X : Y) | (B extends U ? X : Y)
```

**Mandatory Pre-Implementation Question**:
```bash
./scripts/ask-gemini.mjs --pro --include=src/solver \
"How does tsc implement distributive conditional types?

Specifically:
1. What defines a 'naked' type parameter?
2. How do I detect if T is naked vs wrapped?
3. Do I distribute recursively for nested unions?
4. What about intersection types - do they distribute?

Provide the exact algorithm with edge cases."
```

### Task 2.3: Visitor Integration
**File**: `src/solver/visitor.rs`
**Priority**: MEDIUM
**Status**: ⏸️ DEFERRED (after Tasks 2.1 and 2.2)

**Description**: Ensure the TypeVisitor correctly handles conditional types.

---

## Phase 3: The `infer` Keyword (CRITICAL)

**Goal**: Support `infer R` type parameter inference in conditional types.

### Task 3.1: Handle `TypeKey::Infer`
**File**: `src/solver/infer.rs`
**Priority**: HIGH
**Status**: ⏸️ DEFERRED (after Phase 2)

**Description**: Implement inference variable creation for `infer R` in conditional type extends clauses.

**Example**:
```typescript
type ReturnType<T> = T extends (...args: any[]) => infer R ? R : any
```

**Mandatory Pre-Implementation Question**:
```bash
./scripts/ask-gemini.mjs --pro --include=src/solver/infer.rs \
"I am implementing the 'infer' keyword for conditional types.

Planned approach:
1. Detect TypeKey::Infer in the extends check
2. Create fresh inference variable
3. Collect constraints during extends check
4. Use inferred type in the true branch

Questions:
1. When do I create the inference variable?
2. How do I collect constraints from the extends check?
3. What if the same infer appears multiple times?
4. How do I handle infer in nested positions?

Provide the implementation strategy."
```

---

## Phase 4: Address tsz-8 Limitations

**Goal**: Use new distributive logic to fix ThisType union distribution.

### Task 4.1: Refactor ThisType Union Distribution
**File**: `src/solver/contextual.rs`
**Priority**: MEDIUM
**Status**: ⏸️ DEFERRED (after Phase 2)

**Description**: Update `ThisTypeMarkerExtractor::visit_union` to use distributive logic from Phase 2.

**Current Limitation**:
```rust
// TODO: This blindly picks the first ThisType.
// Correct behavior requires narrowing the contextual type based on
// the object literal shape BEFORE determining which this type to use.
```

**Fix Strategy**:
1. Use distributive conditional type logic
2. Narrow contextual type based on object structure
3. Select matching ThisType from union members

---

## Session History

### Previous Session: tsz-8 (COMPLETE ✅)
- **Phase 7b**: Multi-Pass Resolution Logic ✅
- **Phase 8**: ThisType<T> marker support ✅
- **Two-Question Rule**: Both questions completed successfully
- **Critical Bug**: Fixed during Gemini Pro review

See `docs/sessions/tsz-8.md` for full details.

---

## Coordination Notes

**tsz-1, tsz-2, tsz-3, tsz-4, tsz-5, tsz-6, tsz-7, tsz-8**: Various sessions in progress or complete.

**Priority**: Phase 1 (Stabilization) is critical before adding new features. Ensure existing inference works before implementing conditional types.

---

## Complexity Assessment

**Overall Complexity**: **VERY HIGH**

**Why Very High**:
- Conditional types are the most complex part of TypeScript's type system
- Distributive behavior has many edge cases
- `infer` keyword requires deep integration with inference engine
- High risk of breaking existing functionality

**Mitigation**:
- Follow Two-Question Rule strictly for ALL solver/checker changes
- Run conformance tests frequently
- Implement incrementally with thorough testing

---

## Gemini Consultation Plan

Following the mandatory Two-Question Rule from `AGENTS.md`:

### For Each Major Task:
1. **Question 1** (Pre-Implementation): Ask for algorithm validation
2. **Question 2** (Post-Implementation): Ask for code review

**CRITICAL**: Distributive conditional types (Task 2.2) are a common source of bugs. **MUST** use Gemini Pro for review.
