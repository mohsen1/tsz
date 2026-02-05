# Session TSZ-4: Nominality & Accessibility (The Lawyer Layer)

**Started**: 2026-02-05
**Status**: 🔄 ACTIVE
**Focus**: Implement TypeScript-specific compatibility rules (Lawyer Layer)

## Session Scope

### Problem Statement

TypeScript has many quirks and "unsound" behaviors that violate pure set-theoretic subtyping. The **Judge** (SubtypeChecker) implements strict structural typing, but TypeScript needs compatibility rules like:

1. **`any` propagation** - `any` is assignable to/from everything
2. **Function bivariance** - Parameters are bivariant in legacy mode
3. **Excess property checking** - Fresh object literals trigger EPC
4. **Private/Protected brands** - Classes with private members are nominal
5. **Enum nominality** - Enum members are nominally typed
6. **Constructor accessibility** - Private/protected constructor checks

### Why This Matters

Without the Lawyer layer, tsz would be **too strict** and fail conformance. Users expect these TypeScript quirks, even when they're technically unsound.

## Architecture: Judge vs Lawyer

Per `NORTH_STAR.md` Section 3.3 and `src/solver/lawyer.rs`:

- **Judge (SubtypeChecker)**: Pure structural subtyping
- **Lawyer (CompatChecker)**: Applies TypeScript quirks as overrides
- **Key Principle**: Lawyer never makes types MORE compatible - only LESS compatible

## Current State

### ✅ Already Implemented

1. **AnyPropagationRules** (`src/solver/lawyer.rs`)
   - `allow_any_suppression` flag
   - `any_propagation_mode()` returns `AnyPropagationMode::All` or `TopLevelOnly`
   - Integration with SubtypeChecker via `AnyPropagationMode` parameter

2. **Freshness Tracking** (`src/solver/freshness.rs`)
   - `is_fresh_object_type()` checks freshness flag
   - `widen_freshness()` strips freshness after assignment
   - Called from `state_checking.rs:495` during variable declaration

3. **Enum Nominality** (`src/solver/lawyer.rs`)
   - `enum_assignability_override()` in CompatChecker
   - Uses `def_id` for nominal identity

4. **Private Brands** (`src/solver/lawyer.rs`)
   - `private_brand_assignability_override()` in CompatChecker
   - Uses `SymbolId` comparison for private members

5. **Constructor Accessibility** (`src/solver/lawyer.rs`)
   - `constructor_accessibility_override()` checks visibility
   - Validates scope (class/subclass/external)

## Tasks (Priority Order)

### Task 1: Verify `any` Propagation Works Correctly

**Status**: Needs Testing & Verification

**Goal**: Ensure `any` propagation works as expected in all scenarios

**Test Cases**:
```typescript
// any should be assignable to everything
let x: any = 42;
let y: string = x; // Should work

// everything should be assignable to any
let z: any = "hello";
let w: number = z; // Should work

// any in function parameters
function foo(arg: any): void {}
foo(42); // Should work
foo("hello"); // Should work

// any with strict mode
// @strict
let a: any = 42;
let b: string = a; // Should still work in strict mode
```

**Files to check**:
- `src/solver/subtype.rs` - Look for `AnyPropagationMode` usage
- `src/solver/lawyer.rs` - Verify `any_propagation_mode()` logic
- `src/solver/compat.rs` - Check `CompatChecker::is_subtype_of()` integration

**Deliverables**:
- [ ] Add integration tests for `any` propagation
- [ ] Verify `AnyPropagationMode::All` vs `TopLevelOnly` behavior
- [ ] Document any edge cases found

### Task 2: Verify Function Bivariance in Legacy Mode

**Status**: Needs Verification

**Goal**: Ensure function parameters are bivariant unless `strictFunctionTypes` is enabled

**Test Cases**:
```typescript
// Legacy mode (no strictFunctionTypes): parameters bivariant
type A = (x: string) => void;
type B = (x: any) => void;
let f: A = (x: string) => {};
let g: B = f; // Should work (bivariance)

// Methods are always bivariant regardless of strictFunctionTypes
interface Foo { method(x: string): void; }
interface Bar { method(x: any): void; }
let foo: Foo;
let bar: Bar = foo; // Should work (methods bivariant)
```

**Files to check**:
- `src/solver/subtype.rs` - Function variance checking
- `src/solver/compat.rs` - `strictFunctionTypes` flag handling

**Deliverables**:
- [ ] Add tests for function parameter variance
- [ ] Verify method vs function difference
- [ ] Document strictFunctionTypes behavior

### Task 3: Verify Excess Property Checking (EPC)

**Status**: ✅ COMPLETE (Verified in TSZ-6 Priority 5)

Freshness stripping was verified in TSZ-6 with 10 passing tests.

**Test Coverage**:
- Fresh literals trigger EPC ✅
- Non-fresh sources don't trigger EPC ✅
- Nested objects handled correctly ✅

### Task 4: Verify Private/Protected Brands

**Status**: Needs Verification

**Goal**: Ensure classes with private members are nominally typed

**Test Cases**:
```typescript
class A { private x: number = 1; }
class B { private x: number = 1; }

let a: A = new B(); // Should error: different private declarations
let b: B = new A(); // Should error: different private declarations

class C extends A {}
let c: A = new C(); // Should work: subclass inherits brand
```

**Files to check**:
- `src/solver/lawyer.rs` - `private_brand_assignability_override()`
- `src/checker/accessibility.rs` - Private member detection

**Deliverables**:
- [ ] Add integration tests for private brands
- [ ] Verify subclass inherits parent brand
- [ ] Test protected member behavior

### Task 5: Verify Enum Nominality

**Status**: Needs Verification

**Goal**: Ensure enum members are nominally typed

**Test Cases**:
```typescript
enum E { A = 0, B = 1 }
enum F { A = 0, B = 1 }

let x: E.A = E.B;  // Should error: different members
let y: E.A = F.A;  // Should error: different enums
let z: E.A = 0;    // Should work: numeric enum to number
let w: number = E.A; // Should work: numeric enum to number
```

**Files to check**:
- `src/solver/lawyer.rs` - `enum_assignability_override()`
- `src/solver/types.rs` - `TypeKey::Enum` structure

**Deliverables**:
- [ ] Add integration tests for enum nominality
- [ ] Verify numeric vs string enum differences
- [ ] Test enum member assignability

### Task 6: Verify Constructor Accessibility

**Status**: Needs Verification

**Goal**: Ensure private/protected constructors are checked correctly

**Test Cases**:
```typescript
class A { private constructor() {} }
let a = new A();  // Should error: TS2673

class B { protected constructor() {} }
let b = new B();  // Should error: TS2674

class C extends B { constructor() { super(); } }
let c = new C();  // Should work: subclass access
```

**Files to check**:
- `src/solver/lawyer.rs` - `constructor_accessibility_override()`
- `src/checker/accessibility.rs` - Constructor detection

**Deliverables**:
- [ ] Add integration tests for constructor accessibility
- [ ] Verify private vs protected vs public
- [ ] Test subclass constructor access

## Implementation Notes

### Mandatory Gemini Workflow

Per AGENTS.md, **must** ask Gemini TWO questions before implementing:

#### Question 1: Approach Validation (PRE-implementation)
```bash
./scripts/ask-gemini.mjs --include=src/solver "I need to verify/implement [FEATURE] for TSZ-4.
Here's my understanding: [PROBLEM DESCRIPTION].
Planned approach: [YOUR PLAN].

Questions:
1. Is this approach correct?
2. What files/functions should I modify?
3. What edge cases should I test?
4. Are there TypeScript behaviors I need to match?"
```

#### Question 2: Implementation Review (POST-implementation)
```bash
./scripts/ask-gemini.mjs --pro --include=src/solver "I implemented [FEATURE] for TSZ-4.
Changes: [PASTE CODE OR DIFF].

Please review:
1. Is this correct for TypeScript?
2. Did I miss any edge cases?
3. Are there type system bugs?

Be brutal - tell me specifically what to fix."
```

## Related Sessions

- **TSZ-1**: Judge Layer (Core Type Relations) - Foundation
- **TSZ-6**: Literal Type Widening & Const Assertions - ✅ COMPLETE
- **TSZ-5**: Multi-Pass Generic Inference - ✅ COMPLETE

## Success Criteria

- [ ] Task 1: `any` propagation verified with tests
- [ ] Task 2: Function bivariance verified with tests
- [ ] Task 3: EPC verified (✅ DONE in TSZ-6)
- [ ] Task 4: Private/protected brands verified with tests
- [ ] Task 5: Enum nominality verified with tests
- [ ] Task 6: Constructor accessibility verified with tests
- [ ] All Lawyer layer features have comprehensive test coverage
- [ ] Conformance tests pass for Lawyer layer scenarios

## Work Log

### 2026-02-05: Session Initialized

**Context**: TSZ-6 complete (widening & const assertions). Following AGENTS.md hook guidance, asked Gemini to recommend next session.

**Gemini Recommendation**: Continue with TSZ-4 (Lawyer Layer) because:
1. High conformance impact
2. Architectural integrity (prevents logic leakage)
3. Minimal dependencies (can implement independently)

**Investigation**: Reviewed `src/solver/lawyer.rs` and found:
- `AnyPropagationRules` already implemented ✅
- All major Lawyer features already exist:
  - `any` propagation ✅
  - Freshness tracking ✅
  - Enum nominality ✅
  - Private brands ✅
  - Constructor accessibility ✅

**Conclusion**: The Lawyer layer is **implemented** but needs **verification and testing**.

**Strategy**: Shift from implementation to **verification/testing**. Each task will:
1. Create comprehensive integration tests
2. Verify feature works correctly
3. Document any edge cases
4. Fix bugs if found

**Next Task**: Task 1 - Verify `any` Propagation

**Commit**: Session file created (initial task list)
