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

### Overall Completion: **60.2%**

| Status | Count | Percentage |
|--------|-------|------------|
| ✅ Fully Implemented | 21 | 47.7% |
| ⚠️ Partially Implemented | 11 | 25.0% |
| ❌ Not Implemented | 12 | 27.3% |
| **Total Rules** | **44** | **100%** |

### Phase Breakdown

| Phase | Description | Completion | Status |
|-------|-------------|------------|--------|
| **Phase 1** | Hello World (Bootstrapping) | 80.0% | Nearly complete |
| **Phase 2** | Business Logic (Common Patterns) | 80.0% | Nearly complete |
| **Phase 3** | Library (Complex Types) | 40.0% | In progress |
| **Phase 4** | Feature (Edge Cases) | 56.9% | Good progress |

---

## ✅ Fully Implemented Rules (21)

| # | Rule | Phase | What It Does |
|---|------|-------|--------------|
| 1 | The "Any" Type | P1 | `any` is assignable to everything and vice versa |
| 3 | Covariant Mutable Arrays | P1 | `Dog[]` is assignable to `Animal[]` (unsound but common) |
| 5 | Nominal Classes (Private Members) | P4 | Classes with private/protected switch to nominal typing |
| 6 | Void Return Exception | P1 | `() => void` accepts `() => T` (caller ignores return) |
| 7 | Open Numeric Enums | P4 | `number` ↔ `Enum` bidirectional assignability |
| 8 | Unchecked Indexed Access | P4 | `T[K]` doesn't add `undefined` by default |
| 9 | Legacy Null/Undefined | P4 | Without `strictNullChecks`, `null`/`undefined` assignable to everything |
| 10 | Literal Widening | P2 | `let x = "hello"` widens to `string` (not `"hello"`) |
| 13 | Weak Type Detection | P4 | Interfaces with only optional props reject unrelated objects |
| 14 | Optionality vs Undefined | P2 | Optional props `x?: T` behave as `x: T \| undefined` (legacy) |
| 17 | Instantiation Depth Limit | P4 | Hard limit (~50) on generic instantiation depth |
| 18 | Class Static Side Rules | P4 | `typeof Class` comparison for static side |
| 19 | Covariant `this` Types | P2 | `this` in method params is covariant (not contravariant) |
| 24 | Cross-Enum Incompatibility | P4 | Different enum types are nominal (incompatible) |
| 25 | Index Signature Consistency | P3 | All properties must match index signature type |
| 28 | Constructor Void Exception | P4 | `new () => void` accepts concrete constructors |
| 29 | Global Function Type | P4 | `Function` is untyped supertype for callables |
| 34 | String Enums | P4 | String literals NOT assignable to string enums |
| 35 | Recursion Depth Limiter | P4 | Same as #17 (duplicate) |
| 37 | unique symbol | P4 | Nominal symbol types via declaration identity |
| 43 | Abstract Class Instantiation | P4 | Cannot instantiate abstract classes |

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

## ❌ Remaining Missing Rules (12)

### Phase 3 Rules (Library)

| # | Rule | Description | Impact |
|---|------|-------------|--------|
| 40 | Distributivity Disabling | `[T] extends [U]` disables union distribution | Blocks Exclude/Extract utility types |
| 41 | Key Remapping (`as never`) | `Omit` implementation via mapped types | Blocks Omit utility type |

### Phase 4 Rules (Feature)

| # | Rule | Description | Impact |
|---|------|-------------|--------|
| 22 | Template String Expansion Limits | Limit template literal union size (~100k) | Performance issues with large unions |
| 23 | Comparison Operator Overlap | Forbid `x === y` if types have no overlap | Type narrowing edge cases |
| 26 | Split Accessors | Getter/setter can have different types | Accessor variance |
| 27 | Homomorphic Mapped Types | Mapped types over primitives use apparent type | Partial<string> handling |
| 32 | Best Common Type Inference | Array literal inference algorithm | Array type inference |
| 36 | JSX Intrinsic Lookup | Case-sensitive JSX tag resolution | JSX type checking |
| 38 | Correlated Unions | Cross-product limitation for union access | Union type access |
| 39 | import type Erasure | Type-only imports don't exist in value space | Module resolution |
| 42 | CFA Invalidation in Closures | Narrowing reset in callbacks | Flow analysis in closures |
| 44 | Module Augmentation Merging | Interface merging across modules | Declaration merging |

---

## Priority Recommendations

### Immediate (Complete Phase 1)

1. **Complete Rule #20** (Object trifecta)
   - Finish primitive assignability to `Object` interface
   - Blocks lib.d.ts compatibility

2. **Complete Rule #11** (Error poisoning)
   - Implement `Union(Error, T)` suppression
   - Critical for good error messages

### Short-term (Complete Phase 3)

3. **Implement Rule #40** (Distributivity Disabling)
   - Handle `[T] extends [U]` tuple wrapping
   - Essential for Exclude/Extract

4. **Implement Rule #41** (Key Remapping)
   - Handle `as never` in mapped types
   - Essential for Omit

### Medium-term (Performance & Edge Cases)

5. **Implement Rule #22** (Template String Limits)
   - Add cardinality check for template literal unions
   - Prevents performance issues

6. **Implement Rule #42** (CFA Invalidation)
   - Reset narrowing in closures
   - Important for flow analysis correctness

---

## Key Files

| File | Purpose |
|------|---------|
| `specs/TS_UNSOUNDNESS_CATALOG.md` | Complete catalog of all 44 rules |
| `docs/UNSOUNDNESS_AUDIT.md` | Implementation status audit |
| `src/solver/compat.rs` | Compatibility layer - applies unsound rules |
| `src/solver/lawyer.rs` | `AnyPropagationRules` and `FreshnessTracker` |
| `src/solver/subtype.rs` | Core structural subtype checking (Judge) |
| `src/solver/subtype_rules/*.rs` | Organized subtype rules by category |
| `src/checker/state.rs` | Enum and class assignability overrides |
| `src/checker/class_type.rs` | Class instance and constructor types |

---

## References

- **TypeScript Unsoundness Catalog**: `specs/TS_UNSOUNDNESS_CATALOG.md` (531 lines, complete specification)
- **Implementation Audit**: `docs/UNSOUNDNESS_AUDIT.md` (current status)
- **Solver Architecture**: `specs/SOLVER.md` (Judge/Lawyer design)

---

**Last Updated**: 2026-01-24
**Next Review**: After Phase 3 completion
