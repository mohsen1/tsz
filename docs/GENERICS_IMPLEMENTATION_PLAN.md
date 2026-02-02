# Generics Implementation Plan

**Created:** 2025-02-02
**Status:** Strategic Plan for Breaking Through 60% Conformance Ceiling
**Related Documents:**
- `CONFORMANCE_DEEP_DIVE.md` - Overall conformance strategy
- `docs/architecture/NORTH_STAR.md` - Target architecture

---

## Executive Summary

Generic type system issues are the **primary blocker** to reaching 60%+ conformance. The current 49.7% pass rate masks deep problems in:
- Generic call inference (Priority #1)
- Type instantiation erasing nominal identity (Priority #2)
- Constraint validation (Priority #3)

This plan provides an **architecture-aligned** roadmap to fix generics systematically, following the Solver-First principles from NORTH_STAR.md.

**Expected Impact:**
- **Sprint A** (Fix Instantiation): +5-8% → 54-57%
- **Sprint B** (Fix Call Inference): +8-12% → 62-69%
- **Sprint C** (Fix Conditionals): +3-5% → 65-74%

---

## Problem Analysis

### Why Generics is the Bottleneck

Based on AI peer review and conformance validation:

| Error Code | Pass Rate | Tests | Root Cause (Generic) |
|------------|-----------|-------|----------------------|
| TS2304 | 7.2% | 615 fail | Many are inference failures, not scope bugs |
| TS2322 | 36.9% | 471 fail | Generic assignability issues |
| TS2345 | 31.8% | 182 fail | Generic function call inference |
| **Total** | **~25%** | **~1,268 tests** | **Generic-related failures** |

**Key Insight:** Many TS2304 "Cannot find name" errors are actually TS2304-in-disguise - the symbol exists, but type inference fails and produces `Error` type, which then causes downstream "Cannot find name" errors.

### Critical Bug: Structural Erasure

**Symptom:** Conformance tests show wrong types:
```typescript
class D<T> { constructor(a: T, private x: T, protected z: T) { } }
declare var d: D<string>;

// TSC reports: d.a does not exist on type D<string>
// tsz reports: d.a does not exist on type { x: string; z: string; isPrototypeOf: ... }
//                                                          ^^^^^^^^^^^^^^^^^^^^
//                                                          WRONG! This is an object literal, not D<string>
```

**Root Cause:** `src/solver/instantiate.rs` eagerly lowers `TypeKey::Application` (e.g., `Box<number>`) to `TypeKey::Object`, erasing nominal identity.

**Impact:**
- Confusing error messages
- Breaks nominal subtype checks (private/protected members)
- Wrong property access validation
- Affects ~353 tests (TS2339 failures)

---

## Sprint A: Fix Generic Instantiation (1-2 weeks)

**Goal:** Preserve nominal identity in generic types. Fix the "Object Literal" bug.

### Fix #1: Preserve Application Type in Instantiation

**File:** `src/solver/instantiate.rs` (Lines ~230-240)

**Current Code (BUGGY):**
```rust
fn instantiate_key(&self, application: TypeApplicationId) -> TypeId {
    let app = self.interner.get_application(application);
    let base_type = self.instantiate(app.base)?;  // Recurse

    // BUG: Resolves base to its structure, erasing nominal identity
    match self.interner.lookup(base_type) {
        TypeKey::Ref(sym_ref) => {
            // Get object shape for symbol
            let shape = self.get_symbol_shape(sym_ref)?;
            // Return object type, LOSING the Application wrapper!
            self.interner.intern_object(instantiated_shape)
        }
        // ...
    }
}
```

**Fixed Code:**
```rust
fn instantiate_key(&self, application: TypeApplicationId) -> TypeId {
    let app = self.interner.get_application(application);
    let base_type = self.instantiate(app.base)?;

    // CORRECT: Return new Application with instantiated args
    // Preserve nominal identity!
    match self.interner.lookup(base_type) {
        TypeKey::Ref(sym_ref) => {
            // Don't resolve to object shape
            // Return new Application with instantiated arguments
            let new_args = self.instantiate_list(app.args)?;
            self.interner.intern_application(TypeApplication {
                base: base_type,  // Keep the Ref!
                args: new_args,
            })
        }
        TypeKey::Application(_) => {
            // Nested application - recurse
            let new_base = self.instantiate_key(application)?;
            let new_args = self.instantiate_list(app.args)?;
            self.interner.intern_application(TypeApplication {
                base: new_base,
                args: new_args,
            })
        }
        // ...
    }
}
```

**Key Changes:**
1. Return `TypeKey::Application` instead of resolving to `TypeKey::Object`
2. Only lower to Object when:
   - Accessing a property: `get_property_type(app_type, "prop")`
   - Checking structural subtype against another Object
3. Preserve nominal type for diagnostics and member access

**Expected Impact:**
- +200-300 tests (fixes TS2339 wrong type issues)
- Better error messages (shows `D<string>` not object literal)
- Enables correct private/protected member checking

### Fix #2: Lower Application Only When Necessary

**File:** `src/solver/instantiate.rs`

**Add new helper:**
```rust
impl TypeInterner {
    /// Lower Application to Object only for property access or structural checks
    pub fn lower_application_to_object(&self, type_id: TypeId) -> TypeId {
        match self.lookup(type_id) {
            TypeKey::Application(app) => {
                // Resolve the Application to its object shape
                self.resolve_application_shape(app)
            }
            TypeKey::Ref(sym) => {
                // Get symbol's object shape
                self.get_symbol_shape(sym)
            }
            _ => type_id,  // Already lowered or not an Application
        }
    }
}
```

**Usage in property access:**
```rust
// In solver/operations_property.rs or similar:
fn get_property_type(&self, object_type: TypeId, prop_name: Atom) -> TypeId {
    // Lower Application to Object for property lookup
    let lowered = self.interner.lower_application_to_object(object_type);
    // Now search for property in object shape
    self.find_property_in_shape(lowered, prop_name)
}
```

**Expected Impact:** Property access works correctly for generic types while preserving nominal identity elsewhere.

---

## Sprint B: Fix Generic Call Inference (2-3 weeks)

**Goal:** Enable correct type parameter inference from function call arguments.

### Fix #3: Implement Variance-Aware Constraint Collection

**File:** `src/solver/operations.rs` (Lines ~350-450)

**Function:** `resolve_generic_call_inner`

**Current Issue:** Constraints collected without tracking variance (covariant vs contravariant positions).

**Fix Required:**
```rust
fn resolve_generic_call_inner(&self, call_expr: NodeIndex) -> TypeId {
    let fn_type = self.get_callee_type(call_expr);
    let type_params = self.get_type_params(fn_type);
    let args = self.get_call_arguments(call_expr);

    let mut constraints = ConstraintSet::new();

    for (i, arg_type) in args.iter().enumerate() {
        let param_type = self.get_function_param_type(fn_type, i);

        // CORRECT: Track position variance
        // Function parameters are contravariant
        // Add constraint: T <: arg_type  (note direction!)
        constraints.add_upper_bound(type_params[i], arg_type);
    }

    // For return type inference (contextual typing):
    if let Some(expected_type) = self.get_contextual_type(call_expr) {
        let return_type_param = self.get_return_type_param(fn_type);
        // Return type is covariant
        // Add constraint: expected_type <: T
        constraints.add_lower_bound(return_type_param, expected_type);
    }

    let solved = self.infer.solve_with_constraints(type_params, constraints)?;
    self.instantiate(fn_type, solved)
}
```

**Variance Rules:**
| Position | Variance | Constraint Direction |
|----------|----------|-------------------|
| Function parameter | Contravariant | `T <: arg_type` |
| Function return | Covariant | `expected_type <: T` |
| Class property | Covariant | `arg_type <: T` |

### Fix #4: Fix Best Common Type (BCT) Calculation

**File:** `src/solver/infer.rs` (Lines ~450-480)

**Function:** `resolve_from_candidates`

**Current Issue:** Filters by priority but doesn't merge candidates correctly.

**Fix Required:**
```rust
fn resolve_from_candidates(&self, var: TypeVar, candidates: Vec<TypeId>) -> TypeId {
    // Group candidates by priority
    let by_priority = group_by_priority(candidates);

    // Start with highest priority
    for (priority, group) in by_priority.iter() {
        if group.len() == 1 {
            return group[0];  // Single candidate, use it
        }

        // Multiple candidates at same priority - find BCT
        let bct = self.best_common_type(group);
        if bct.is_some() {
            return bct;
        }

        // No BCT found (incompatible types)
        // Try next priority level
    }

    TypeId::UNKNOWN  // No inference possible
}

fn best_common_type(&self, types: &[TypeId]) -> Option<TypeId> {
    // Algorithm:
    // 1. Start with first type as candidate
    // 2. For each subsequent type, find union/intersection
    // 3. If no common type, return None

    let mut candidate = types[0];

    for &ty in &types[1..] {
        // Try to find common supertype
        if self.is_subtype_of(candidate, ty) {
            // candidate is subtype of ty, use ty (more general)
            candidate = ty;
        } else if self.is_subtype_of(ty, candidate) {
            // ty is subtype of candidate, keep candidate
            continue;
        } else {
            // Incompatible types - no BCT
            return None;
        }
    }

    Some(candidate)
}
```

**Expected Impact:**
- +150-250 tests (fixes TS2345 generic call failures)
- Enables inference from multiple arguments
- Better contextual typing

### Fix #5: Contextual Typing for Generics

**File:** `src/solver/contextual.rs` or `src/checker/type_checking_queries.rs`

**Enable reverse inference:**
```rust
// In variable declaration with initializer:
let x: Box<number> = new Box(5);  // Should infer Box<number> for `new Box(5)`

// Algorithm:
// 1. Get type from annotation: Box<number>
// 2. Get callee type from expression: new Box<T>(value: T)
// 3. Match: Box<number> matches Box<T> where T = number
// 4. Infer T = number from annotation
// 5. Use T = number to check initializer: new Box(5)
```

**Expected Impact:**
- +50-100 tests (fixes contextual typing failures)
- Better type inference for callbacks

---

## Sprint C: Fix Conditional Types & Infer (2-3 weeks)

**Goal:** Enable advanced type manipulation features.

### Fix #6: Implement Proper `infer` Pattern Matching

**File:** `src/solver/evaluate_rules/infer_pattern.rs`

**Support patterns like:**
```typescript
type ReturnType<T> = T extends (...args: any[]) => infer R ? R : any;
// Extract return type of function type

type UnboxPromise<T> = T extends Promise<infer U> ? U : T;
// Extract type from Promise
```

**Implementation:**
```rust
fn match_infer_pattern(
    &self,
    pattern_type: TypeId,
    target_type: TypeId
) -> Option<Vec<(TypeVar, TypeId)>> {
    // pattern_type: (infer U)[] ? U : T
    // target_type: (x: number, y: string) => void

    // Extract the "infer U" from pattern
    let infer_var = extract_infer_var(pattern_type)?;

    // Match pattern structure against target
    if !self.structure_matches(pattern_type, target_type) {
        return None;
    }

    // Extract the type that replaces U
    let extracted = extract_type_at_position(target_type, infer_var.position);

    Some(vec![(infer_var, extracted)])
}
```

**Expected Impact:**
- +100-150 tests (enables utility types)
- `ReturnType`, `Parameters`, `Awaited` work
- Advanced library type definitions

### Fix #7: Conditional Type Deferral

**File:** `src/solver/evaluate_rules/conditional.rs` (Lines ~20-80)

**Fix proper deferral:**
```rust
fn evaluate_conditional(&self, cond: ConditionalTypeId) -> TypeId {
    let check_type = self.instantiate(cond.check_type)?;

    // CRITICAL: Check for unresolved inference variables
    if self.has_unresolved_inference_vars(check_type) {
        // Return the conditional itself - don't evaluate yet
        return self.interner.intern_conditional(cond.clone());
    }

    // Check extends clause
    if self.is_subtype_of(check_type, cond.extends) {
        self.instantiate(cond.true_branch)?
    } else {
        self.instantiate(cond.false_branch)?
    }
}
```

**Expected Impact:**
- +50-100 tests (conditional types work recursively)
- Proper handling of generic conditionals

---

## Sprint D: Polish & Edge Cases (1-2 weeks)

**Goal:** Handle remaining edge cases and optimize.

### Fix #8: Constraint Validation

**File:** `src/solver/operations.rs` (`solve_generic_instantiation`)

**Add runtime constraint checking:**
```rust
fn solve_generic_instantiation(&self,
    generic: TypeId,
    args: Vec<TypeId>
) -> Result<TypeId, InstantiationError> {
    let type_params = self.get_type_params(generic);

    // Check argument count
    if args.len() != type_params.len() {
        return Err(InstantiationError::WrongArgCount {
            expected: type_params.len(),
            got: args.len(),
        });
    }

    // Check each constraint
    for (i, arg) in args.iter().enumerate() {
        let param = &type_params[i];

        // Check: T extends U
        if let Some(constraint) = &param.constraint {
            if !self.is_subtype_of(*arg, *constraint) {
                return Err(InstantiationError::ConstraintViolation {
                    param: param.name,
                    constraint: *constraint,
                    arg: *arg,
                });
            }
        }
    }

    // All checks passed - instantiate
    self.instantiate_generic(generic, args)
}
```

**Expected Impact:**
- +30-50 tests (better error messages)
- More sound type checking

### Fix #9: Literal Widening in Const Contexts

**File:** `src/solver/infer.rs` (`widen_candidate_types`)

**Respect `const` contexts:**
```rust
fn widen_candidate_types(&self, candidates: Vec<TypeId>, context: &Context) -> Vec<TypeId> {
    let should_widen = !context.is_const_context();

    if !should_widen {
        return candidates;  // Keep literal types in const contexts
    }

    // Widen literals to their primitive types
    candidates.iter().map(|&ty| {
        match self.lookup(ty) {
            TypeKey::Literal(LiteralValue::String(s)) => TypeId::STRING,
            TypeKey::Literal(LiteralValue::Number(n)) => TypeId::NUMBER,
            TypeKey::Literal(LiteralValue::Boolean(b)) => TypeId::BOOLEAN,
            _ => ty,
        }
    }).collect()
}
```

**Expected Impact:**
- +20-30 tests (const assertions work)
- Better type precision

---

## Investigation Queue (Ordered by Priority)

### Week 1: Instantiation Deep Dive
1. **`src/solver/instantiate.rs:230-240`** - Fix Application preservation
2. **`src/solver/operations_property.rs`** - Property access on Applications
3. **`src/solver/subtype_rules/generics.rs`** - Subtyping with Applications

### Week 2-3: Call Inference
4. **`src/solver/operations.rs:350-450`** - `resolve_generic_call_inner`
5. **`src/solver/infer.rs:450-480`** - `resolve_from_candidates`, BCT
6. **`src/checker/callable_type.rs`** - Call expression checking

### Week 4-5: Conditionals
7. **`src/solver/evaluate_rules/conditional.rs:20-80`** - `evaluate_conditional`
8. **`src/solver/evaluate_rules/infer_pattern.rs`** - Pattern matching
9. **`src/solver/lower.rs`** - Lowering flag for distributivity

### Week 6-7: Polish
10. **`src/solver/operations.rs`** - Constraint validation
11. **`src/solver/infer.rs`** - Literal widening
12. **`src/checker/type_checking.rs`** - Generic validation

---

## Testing Strategy

### Unit Tests for Generics

**Create:** `src/solver/tests/generic_instantiation_tests.rs`

```rust
#[test]
fn test_preserve_application_identity() {
    // Box<number> should NOT be lowered to Object
    let box_num = instantiate_application("Box", vec![number]);

    // Check it's still an Application, not Object
    assert!(matches!(
        lookup(box_num),
        TypeKey::Application { .. }  // Still Application!
    ));

    // But property access should work
    let prop_type = get_property_type(box_num, "value");
    assert_eq!(prop_type, number);
}

#[test]
fn test_generic_call_inference() {
    // function foo<T>(x: T, y: T): T
    // foo(1, "hi") should infer T = never (incompatible)
    // foo(1, 2) should infer T = number
}
```

### Conformance Testing

**Track progress with specific error codes:**
```bash
# Before fixes
./scripts/conformance/run.sh --error-code=2345 --pass-rate-only
./scripts/conformance/run.sh --error-code=2322 --pass-rate-only

# After Sprint B (should see significant improvement)
./scripts/conformance/run.sh --error-code=2345 --pass-rate-only

# Test specific generic patterns
./scripts/conformance/run.sh --filter="generic" --max=50 --print-test
./scripts/conformance/run.sh --filter="infer" --max=20 --print-test
```

---

---

## Conformance Validation Results

**Date:** 2025-02-02
**Validation Method:** `./scripts/conformance/run.sh --print-test`

### ✅ Validated Findings

#### 1. Structural Erasure Bug (Priority #2) - CONFIRMED

**Test:** `conformance/classes/constructorDeclarations/constructorParameters/constructorParameterProperties.ts:19`

**TSC Output:**
```
Property 'a' does not exist on type 'D<string>'.
```

**tsz Output:**
```
Property 'a' does not exist on type '{ z: string; isPrototypeOf: { (v: Infinity): boolean };
propertyIsEnumerable: { (v: string): boolean }; ... }'.
```

**Analysis:** This is exactly the bug described in Sprint A, Fix #1. The `D<string>` type is being lowered to an object literal shape instead of preserving the `TypeKey::Application`. This confirms the `src/solver/instantiate.rs:230-240` issue is **CRITICAL**.

**Impact:** Affects error messages AND breaks nominal type checking (private/protected members).

---

#### 2. Private Member Checking in Generic Classes - BROKEN

**Test:** `conformance/classes/members/privateNames/privateNamesInGenericClasses.ts:25-26`

**TSC Errors:**
```
TS2322: Type 'C<string>' is not assignable to type 'C<number>'.
TS2322: Type 'C<number>' is not assignable to type 'C<string>'.
TS18013: Property '#foo' is not accessible outside class 'C' (3x)
```

**tsz Errors:**
```
TS2339: Property '#foo' does not exist (9x) - WRONG!
TS2564: Property not definitely assigned (1x)
```

**Analysis:** tsz is completely missing private member checks in generic classes and emitting wrong error codes. This is a downstream effect of the structural erasure bug - when `C<string>` is lowered to object literal, all nominal information (including private members) is lost.

**Impact:** ~50+ tests for generic private/protected member access are failing.

---

#### 3. Array Type Instantiation - BROKEN

**Test:** `conformance/parser/ecmascript5/Generics/parserObjectCreation1.ts:1`

**Code:**
```typescript
var autoToken: number[] = new Array<number[]>(1);
```

**TSC Error:**
```
TS2322: Type 'number[][]' is not assignable to type 'number[]'.
Type 'number[]' is not assignable to type 'number'.
```

**tsz Output:** `(no errors)`

**Analysis:** `new Array<number[]>(1)` should instantiate `Array<T>` with `T = number[]`, resulting in `number[][]`. tsz is not correctly computing the instantiated type or performing the assignability check.

**Impact:** Generic constructor instantiation is broken.

---

#### 4. Constraint Validation - PARTIALLY WORKING

**Test:** `conformance/expressions/functionCalls/typeArgumentInferenceWithConstraints.ts:11`

**Code:**
```typescript
function noGenericParams<T extends number>(n: string) { }
noGenericParams<{}>(''); // Error
```

**TSC Error:**
```
TS2344: Type '{}' does not satisfy the constraint 'number'.
```

**tsz Error:**
```
TS2344: Type '{}' does not satisfy the constraint 'number'.
```

**Analysis:** Basic constraint checking works (TS2344 is emitted). However, some constraint-related errors differ significantly from TSC, suggesting issues in constraint validation logic.

**Impact:** Constraint validation exists but has bugs in edge cases.

---

#### 5. Generic Type Inference - MAJOR ISSUES

**Test:** `conformance/expressions/functionCalls/typeArgumentInferenceWithConstraints.ts:33`

**Code:**
```typescript
function someGenerics3<T extends Window>(producer: () => T) { }
someGenerics3(() => ''); // Error
```

**TSC Error:**
```
TS2322: Type 'string' is not assignable to type 'Window'.
```

**tsz Errors:**
```
TS2318: Cannot find global type 'Window' (2x)
TS2345: Argument of type '() => string' is not assignable to parameter of type '() => error'.
```

**Analysis:** tsz is not inferring `T = string` from the arrow function return type. Instead, it's producing `error` type, suggesting inference failures. The missing `Window` type also causes cascading errors.

**Impact:** Generic call argument inference (Priority #1) is indeed broken.

---

### Test Statistics

| Filter | Tests | Key Issues |
|--------|-------|------------|
| `generic` + TS2322 | 50 tests | Constraint validation, assignability, Array instantiation |
| `generic` + TS2345 | 32 tests | Call argument inference failures |
| `typeArgumentInference` + TS2322 | 1 test | Constraint validation missing in some cases |
| TS2339 | 407 tests | Property access + structural erasure bug |
| TS2304 | 615 fail | Many are inference failures (not just scope) |

---

### Implementation Priority Update

Based on validation, the sprint order is correct:

1. **Sprint A** (Fix Structural Erasure) is BLOCKING everything else
   - Without preserving `TypeKey::Application`, private/protected checking cannot work
   - Error messages will remain confusing
   - Fix should unblock ~200-300 tests

2. **Sprint B** (Fix Call Inference) is second priority
   - Generic call inference is failing to infer type parameters
   - Affects ~150-250 tests

3. **Sprint D** (Constraint Validation) needs refinement
   - Basic checking exists but has edge case bugs
   - Requires fixing structural erasure first for correct behavior

---

## Architecture Alignment

### Following NORTH_STAR Principles

✅ **Solver-First:** ALL generic logic lives in `src/solver/`
- Checker delegates via `solver.instantiate()`, `solver.infer()`
- No type computation in Checker

✅ **Visitor Pattern:** Use `TypeVisitor` for traversing generic types
- Collect type parameters
- Find referenced types

✅ **Arena Allocation:** TypeId interned globally
- O(1) equality for types
- Shared across files

### Anti-Patterns to Avoid

❌ **Checker doing generic logic:**
```rust
// WRONG: Checker manually instantiating generics
fn check_generic_call(&mut self, call: NodeIndex) {
    let args = self.get_arg_types(call);
    let type_params = self.get_type_params_from_signature(call);
    // Manual instantiation logic - DON'T DO THIS
}

// CORRECT: Delegate to Solver
fn check_generic_call(&mut self, call: NodeIndex) -> TypeId {
    let callee_type = self.get_callee_type(call);
    let args = self.get_arg_types(call);
    self.solver.resolve_generic_call(callee_type, args)
}
```

❌ **Early lowering in wrong places:**
```rust
// WRONG: Lowering Application to Object in type resolution
fn resolve_member_access(&self, app: TypeApplicationId, member: Atom) -> TypeId {
    let obj = self.lower_application_to_object(app);
    self.find_property(obj, member)
}

// CORRECT: Preserve Application, lower only for property lookup
fn resolve_member_access(&self, app: TypeApplicationId, member: Atom) -> TypeId {
    // Check if symbol has member (nominal lookup)
    if let Some(member_type) = self.get_nominal_member(app, member) {
        return member_type;
    }
    // Fall back to structural lookup
    let obj = self.lower_application_to_object(app);
    self.find_property(obj, member)
}
```

---

## Risk Assessment

### High Risk Areas

1. **Cycle Detection in Instantiation**
   - **Risk:** Recursively instantiating generics could cause infinite loops
   - **Mitigation:** Coinductive semantics (GFP), cycle stack in Solver

2. **Performance Regression**
   - **Risk:** Preserving Applications instead of lowering may slow down property access
   - **Mitigation:** Cache lowered shapes, lazy evaluation

3. **Backward Compatibility**
   - **Risk:** Changes may break existing (buggy) behavior
   - **Mitigation:** Comprehensive test suite, gradual rollout

### Medium Risk Areas

1. **Variance Implementation**
   - Getting variance wrong could cause unsoundness
   - Test with contravariant function parameters

2. **Conditional Type Complexity**
   - Recursive conditionals are tricky
   - Add fuel counters to prevent infinite loops

---

## Success Metrics

### Sprint Milestones

| Sprint | Target Pass Rate | Key Deliverable |
|--------|-----------------|-----------------|
| **A** | 54-57% | Application identity preserved |
| **B** | 62-69% | Generic call inference works |
| **C** | 65-74% | Conditional types work |
| **D** | 67-77% | Edge cases polished |

### Long-Term Target

**80%+ conformance achievable** after:
- All generic features implemented
- Conditional types solid
- Utility types work
- Advanced patterns supported

---

## Next Steps

1. **Start with Sprint A, Fix #1** (Application preservation)
   - This is the highest impact fix
   - Should immediately improve 200-300 tests
   - Enables correct nominal type checking

2. **Set up generic-specific CI**
   ```bash
   # Track generic-specific pass rate
   ./scripts/conformance/run.sh --filter="generic|infer|conditional" --pass-rate-only
   ```

3. **Create unit tests for instantiation**
   - Test Application preservation
   - Test property access on Applications
   - Test nominal subtype checking

4. **Document lessons learned**
   - Update NORTH_STAR.md with generic patterns
   - Add examples of correct generic handling

---

**Remember:** We're not just fixing bugs - we're building a **correct, sound generic type system** following the Solver-First architecture. Every fix should move us closer to architectural ideals, not away from them.
