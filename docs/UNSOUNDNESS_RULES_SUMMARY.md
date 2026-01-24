# Unsoundness Rules: What They Are and What's Left

## What Are Unsoundness Rules?

**Unsoundness rules** are intentional deviations from mathematically sound set-theoretic typing that TypeScript uses for pragmatic, real-world reasons. They allow TypeScript to be more permissive than strict type theory would allow.

### The Judge vs. Lawyer Architecture

The project uses a **Judge/Lawyer** architecture:

- **Judge (Core Solver)**: Implements mathematically sound set-theoretic subtyping
  - Located in `src/solver/subtype.rs`
  - Handles structural type checking correctly
  - No TypeScript-specific quirks

- **Lawyer (Compatibility Layer)**: Applies TypeScript-specific unsound rules
  - Located in `src/solver/compat.rs` and `src/solver/lawyer.rs`
  - Wraps the Judge to match TypeScript's behavior
  - Applies the 44 unsoundness rules before delegating to Judge

### Why Do We Need Them?

TypeScript intentionally breaks sound type theory to:
1. **Backward compatibility** - Support JavaScript patterns that aren't type-safe
2. **Developer ergonomics** - Make common patterns easier (e.g., method bivariance)
3. **Pragmatic trade-offs** - Accept some unsoundness for better developer experience

**Example**: Function method bivariance (#2)
```typescript
class Animal { }
class Dog extends Animal { }

class Box {
  setValue(value: Animal) { }  // Method parameter
}

class DogBox extends Box {
  setValue(value: Dog) { }  // Narrower parameter - should be error in sound theory
}

const box: Box = new DogBox();
box.setValue(new Animal());  // Runtime error! But TypeScript allows it
```

In sound type theory, this should be rejected (contravariant parameters). TypeScript allows it for methods (bivariant) because it's more ergonomic for OOP patterns.

---

## Implementation Status

### Overall Completion: **33.0%**

| Status | Count | Percentage |
|--------|-------|------------|
| ✅ Fully Implemented | 9 | 20.5% |
| ⚠️ Partially Implemented | 11 | 25.0% |
| ❌ Not Implemented | 24 | 54.5% |
| **Total Rules** | **44** | **100%** |

### Phase Breakdown

| Phase | Description | Completion | Critical Rules |
|-------|-------------|------------|----------------|
| **Phase 1** | Hello World (Bootstrapping) | 80.0% | #1, #3, #6, #11, #20 |
| **Phase 2** | Business Logic (Common Patterns) | 40.0% | #2, #4, #10, #14, #19 |
| **Phase 3** | Library (Complex Types) | 20.0% | #25, #30, #40, #21, #41 |
| **Phase 4** | Feature (Edge Cases) | 25.9% | Enums, Classes, JSX, etc. |

---

## ✅ Fully Implemented Rules (9)

| # | Rule | Phase | What It Does |
|---|------|-------|--------------|
| 1 | The "Any" Type | P1 | `any` is assignable to everything and vice versa |
| 3 | Covariant Mutable Arrays | P1 | `Dog[]` is assignable to `Animal[]` (unsound but common) |
| 6 | Void Return Exception | P1 | `() => void` accepts `() => T` (caller ignores return) |
| 8 | Unchecked Indexed Access | P4 | `T[K]` doesn't add `undefined` by default |
| 9 | Legacy Null/Undefined | P4 | Without `strictNullChecks`, `null`/`undefined` assignable to everything |
| 13 | Weak Type Detection | P4 | Interfaces with only optional props reject unrelated objects |
| 14 | Optionality vs Undefined | P2 | Optional props `x?: T` behave as `x: T \| undefined` (legacy) |
| 17 | Instantiation Depth Limit | P4 | Hard limit (~50) on generic instantiation depth |
| 35 | Recursion Depth Limiter | P4 | Same as #17 (duplicate) |

---

## ⚠️ Partially Implemented Rules (11)

| # | Rule | Phase | What's Missing |
|---|------|-------|----------------|
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

---

## ❌ Critical Missing Rules (24)

### 🔴 Phase 2 Blockers (Must Implement Next)

| # | Rule | Description | Impact |
|---|------|-------------|--------|
| 10 | Literal Widening | `let x = "hello"` widens to `string` (not `"hello"`) | Blocks `let`/`var` bindings |
| 19 | Covariant `this` | `this` in method params is covariant (not contravariant) | Blocks fluent APIs |

### 🔴 Enum Rules (All Missing - High Priority)

| # | Rule | Description | Impact |
|---|------|-------------|--------|
| 7 | Open Numeric Enums | `number` ↔ `Enum` bidirectional assignability | Cannot type-check numeric enums |
| 24 | Cross-Enum Incompatibility | Different enum types are nominal (incompatible) | Cannot distinguish enum types |
| 34 | String Enums | String literals NOT assignable to string enums | Cannot type-check string enums |

**Impact**: Cannot properly type-check code using enums. This is a significant gap.

### 🔴 Class Rules (All Missing - High Priority)

| # | Rule | Description | Impact |
|---|------|-------------|--------|
| 5 | Nominal Classes | Private/protected members switch to nominal typing | Class-heavy codebases fail |
| 18 | Static Side Rules | `typeof Class` comparison special handling | Static member checking fails |
| 43 | Abstract Classes | Abstract class constructor checking | Abstract class instantiation not checked |

**Impact**: Class-heavy codebases will have incorrect type checking.

### Other Missing Rules (Lower Priority)

| # | Rule | Phase | Description |
|---|------|-------|-------------|
| 22 | Template String Expansion Limits | P3 | Limit template literal union size (~100k) |
| 23 | Comparison Operator Overlap | P4 | Forbid `x === y` if types have no overlap |
| 25 | Index Signature Consistency | P3 | All properties must match index signature type |
| 26 | Split Accessors | P4 | Getter/setter can have different types |
| 27 | Homomorphic Mapped Types | P3 | Mapped types over primitives use apparent type |
| 28 | Constructor Void Exception | P4 | `new () => void` accepts concrete constructors |
| 29 | Global Function Type | P4 | `Function` is untyped supertype for callables |
| 32 | Best Common Type Inference | P4 | Array literal inference algorithm |
| 36 | JSX Intrinsic Lookup | P4 | Case-sensitive JSX tag resolution |
| 37 | unique symbol | P4 | Nominal symbol types |
| 38 | Correlated Unions | P4 | Cross-product limitation for union access |
| 39 | import type Erasure | P4 | Type-only imports don't exist in value space |
| 40 | Distributivity Disabling | P3 | `[T] extends [U]` disables union distribution |
| 41 | Key Remapping (`as never`) | P3 | `Omit` implementation via mapped types |
| 42 | CFA Invalidation in Closures | P4 | Narrowing reset in callbacks |
| 44 | Module Augmentation Merging | P4 | Interface merging across modules |

---

## Priority Recommendations

### Immediate (Complete Phase 1)

1. **Complete Rule #20** (Object trifecta)
   - Finish primitive assignability to `Object` interface
   - Blocks lib.d.ts compatibility

2. **Complete Rule #11** (Error poisoning)
   - Implement `Union(Error, T)` suppression
   - Critical for good error messages

### Short-term (Phase 2 Blockers)

3. **Implement Rule #10** (Literal widening)
   - Add `widen_literal()` to lowering pass
   - Essential for `let`/`var` bindings

4. **Implement Rule #19** (Covariant `this`)
   - Make `this` covariant in method parameters
   - Critical for fluent APIs

### Medium-term (Enum Support)

5. **Implement Rule #7** (Open Numeric Enums)
   - Add number ↔ Enum bidirectional assignability
   - Foundation for other enum rules

6. **Implement Rule #24** (Cross-Enum)
   - Add nominal checking between different enum types
   - Depends on #7

7. **Implement Rule #34** (String Enums)
   - Make string enums opaque (reject string literals)
   - Independent of numeric enum rules

### Long-term (Class Support)

8. **Implement Rule #5** (Nominal Classes)
   - Add private/protected member detection
   - Switch to nominal comparison when present

9. **Implement Rule #18** (Static Side)
   - Add `typeof Class` special handling
   - Handle protected static members nominally

10. **Implement Rule #43** (Abstract Classes)
    - Add abstract class constructor checking
    - Prevent instantiation of abstract classes

---

## Key Files

| File | Purpose |
|------|---------|
| `specs/TS_UNSOUNDNESS_CATALOG.md` | Complete catalog of all 44 rules |
| `docs/UNSOUNDNESS_AUDIT.md` | Implementation status audit |
| `src/solver/compat.rs` | Compatibility layer - applies unsound rules |
| `src/solver/lawyer.rs` | `AnyPropagationRules` and `FreshnessTracker` |
| `src/solver/subtype.rs` | Core structural subtype checking (Judge) |

---

## References

- **TypeScript Unsoundness Catalog**: `specs/TS_UNSOUNDNESS_CATALOG.md` (531 lines, complete specification)
- **Implementation Audit**: `docs/UNSOUNDNESS_AUDIT.md` (current status)
- **Solver Architecture**: `specs/SOLVER.md` (Judge/Lawyer design)

---

**Last Updated**: 2026-01-24  
**Next Review**: After Phase 2 blocker implementation
