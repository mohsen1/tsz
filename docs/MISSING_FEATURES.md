# TSZ Missing Features Analysis

**Generated from conformance test results**
**Total Failing Tests**: 4,883 / 12,583 (38.8%)

---

## Feature Gaps Summary

TSZ is missing or has incomplete implementations of features across several areas. This document catalogs what's not yet built.

### Quick Stats

- **626 error codes** never emitted by TSZ (affecting 1,927 tests)
- **202 error codes** partially implemented (missing in some cases)
- **1,280 tests** with false positives (we're too strict)
- **1,441 tests** with missing errors (we're too lenient)

---

## 1. Unused Variable Detection

### TS7026 - Unused variable declaration

**Status**: NOT IMPLEMENTED
**Impact**: 33 tests failing

**What it checks**:
```typescript
const unused = 5;  // TS7026: 'unused' is declared but never used
const used = 5;
console.log(used);
```

**Where it should be implemented**:
- `crates/tsz-checker/src/` - Need to track variable usage and report unused declarations
- Integrate with control flow analysis

**Notes**:
- Requires tracking which symbols are referenced
- Optional check (only in strict mode or with special flags)
- Should be relatively straightforward to implement

---

## 2. Access Modifier Validation

### TS2551 - Property is private

**Status**: NOT IMPLEMENTED
**Impact**: 25 tests failing

**What it checks**:
```typescript
class Foo {
    private x = 5;
}
const foo = new Foo();
foo.x;  // TS2551: Property 'x' is private
```

**Related codes**:
- TS2371 - Type is private
- TS2576 - Property in base class is private
- TS4142 - Export is private
- TS4143 - Module augmentation can only augment public members

**Where to implement**:
- `crates/tsz-checker/src/accessibility.rs` and `crates/tsz-checker/src/private_checker.rs`
- Need to check access modifiers when accessing members
- Validate class/interface member privacy across files

**Dependencies**:
- Symbol resolution (TS2304) must work correctly first
- Member lookup must work for all member types

**Complexity**: Medium

---

## 3. Object Literal Patterns

### TS2585 - Object class pattern

**Status**: NOT IMPLEMENTED
**Impact**: 17 tests failing

**What it checks**:
```typescript
const Point = function(this: Point, x: number, y: number) {
    this.x = x;
    this.y = y;
};
interface Point {
    x: number;
    y: number;
}

const p = new Point(0, 0);  // TS2585: Constructor signature missing
```

**Pattern**: Object constructor functions with external interface declaration

**Where to implement**:
- `crates/tsz-checker/src/declarations.rs`
- Constructor pattern detection
- Object literal factory function validation

**Complexity**: Medium-High

---

## 4. Readonly Property Violations

### TS2528 - Can't assign to readonly

**Status**: NOT IMPLEMENTED
**Impact**: 17 tests failing

**What it checks**:
```typescript
interface Foo {
    readonly x: number;
}
const foo: Foo = { x: 5 };
foo.x = 10;  // TS2528: Cannot assign to 'x' because it is a read-only property
```

**Related codes**:
- TS2540 - Cannot assign to const
- TS2741 - Property missing in type (partial - readonly not enforced)
- TS2823 - Type is readonly (related: array readonly issues)

**Where to implement**:
- `crates/tsz-checker/src/assignability_checker.rs`
- Track readonly modifier on properties
- Check property assignments for readonly flag
- `crates/tsz-checker/src/expr.rs` - member access validation

**Current Status**: Solver can represent readonly types, but Checker doesn't validate

**Complexity**: Low-Medium

---

## 5. Super Expression Validation

### TS1100 - Invalid use of super

**Status**: NOT IMPLEMENTED
**Impact**: 17 tests failing

**What it checks**:
```typescript
class Base {
    foo() {}
}
class Derived extends Base {
    foo() {
        super.foo();  // Valid
    }
    invalid() {
        super.foo();  // TS1100: super must be followed by . or [
    }
}
super.foo();  // TS1100: Super calls outside class
```

**Context Rules**:
- `super()` only valid in constructor
- `super.method()` only valid in methods
- `super[expression]` for indexed access
- Only valid in class methods and constructors

**Where to implement**:
- `crates/tsz-checker/src/statements.rs` - Constructor validation
- `crates/tsz-checker/src/expr.rs` - Super expression checking
- Track current class/constructor context in CheckerContext

**Complexity**: Low

---

## 6. Initializer Not Allowed

### TS1011 - Initializer not allowed

**Status**: NOT IMPLEMENTED
**Impact**: 17 tests failing

**What it checks**:
```typescript
// Type parameters can't have initializers in some contexts
class Foo<T = string> {}  // OK in class

declare class Bar<T = string>;  // TS1011: Parameter declaration expected

// Function parameters
function foo(x?: number = 5) {}  // OK (optional with default)
function bar(x? = 5) {}  // TS1011: Parameter must have type

// Const declarations
const x: number = 5;  // OK
const x = 5;  // OK

// But in interfaces
interface Foo {
    x = 5;  // TS1011: Initializer not allowed in object type literal
}
```

**Context-dependent**: Initializers allowed in some places, forbidden in others

**Where to implement**:
- `crates/tsz-parser/src/` - Parse-time validation
- Some checks may be in `crates/tsz-checker/src/type_checking_utilities.rs`

**Complexity**: Low

---

## 7. Type Ordering and Canonicalization

### TS2823 - Type is readonly (related type operations)

**Status**: NOT FULLY IMPLEMENTED
**Impact**: 17 tests failing

**What it checks**:
```typescript
type ReadonlyArray = readonly any[];
type WritableArray = any[];

// Type operations on readonly
let x: readonly number[] = [1, 2, 3];
let y: number[] = x;  // TS2322 & TS2823: Array is readonly

// Mapped type readonly handling
type Readonly<T> = {
    readonly [K in keyof T]: T[K];
};
```

**Related features**:
- Readonly types need proper solver support (partially done)
- Readonly propagation through type operations
- Readonly preservation in generics
- `ReadonlyArray` vs `readonly T[]` normalization

**Where to implement**:
- `crates/tsz-solver/src/` - Type operations preserve readonly
- `crates/tsz-checker/src/` - Validation of readonly assignments

**Current Status**: Parser and Solver support readonly, but not fully enforced in checking

**Complexity**: Medium

---

## 8. Module Augmentation & Merging

### TS2708 - Module augmentation

**Status**: NOT IMPLEMENTED
**Impact**: ~50+ tests affected

**What it checks**:
```typescript
declare module "fs" {
    export function myFunc(): void;
}
```

**Related codes**:
- TS2503 - Module augmentation scope
- TS2671 - Cannot add more module augmentations
- TS2503 - Module has multiple augmentations in same file
- TS2432 - Can't merge interface with class

**Features needed**:
- Detect `declare module` statements
- Merge declarations from augmentations into original module
- Validate augmentation scope rules
- Prevent conflicting augmentations

**Where to implement**:
- `crates/tsz-binder/src/lib_loader.rs` - Built-in lib loading
- `crates/tsz-checker/src/symbol_resolver.rs` - Module merging
- `crates/tsz-checker/src/declarations.rs` - Declaration validation

**Dependencies**:
- Module resolution (TS2307) must work
- Import handling must be complete

**Complexity**: High

---

## 9. Export Validation

### TS2458 - AMD module name not found

**Status**: NOT IMPLEMENTED
**Impact**: Various module tests

**What it checks**:
```typescript
// AMD modules
///<amd:module name="myName" />
declare module "other" {}

export class Foo {}
```

**Related codes**:
- TS2305 - Module export not found (partially implemented)
- TS2449 - Can't redeclare exported symbol

**Features needed**:
- AMD module naming validation
- Export statement processing
- Re-export handling
- Export alias validation

**Where to implement**:
- `crates/tsz-checker/src/declarations.rs` - Export processing
- `crates/tsz-checker/src/symbol_resolver.rs` - Module export lookup
- `crates/tsz-binder/src/state.rs` - Binding exports

**Current Status**: Basic export parsing works, but validation incomplete

**Complexity**: Medium-High

---

## 10. Enums and Constants

### TS2540 - Cannot assign to const

**Status**: PARTIALLY IMPLEMENTED
**Impact**: ~30+ tests

**What it checks**:
```typescript
const x = 5;
x = 10;  // TS2540: Cannot assign to 'x' because it is a constant
```

**Also affects**:
- TS2628 - Enum can't be reassigned
- TS2432 - Type merging (enums)
- TS2639 - Object member can't be const

**Where to implement**:
- Already have infrastructure but need to activate for all contexts
- `crates/tsz-checker/src/assignability_checker.rs`

**Current Status**: Parser tracks const flag, need to enforce in Checker

**Complexity**: Low

---

## 11. Extends Validation

### TS2411 - Type parameter exceeds constraint

**Status**: PARTIALLY IMPLEMENTED
**Impact**: 21 tests (all missing) + more in "wrong codes"

**What it checks**:
```typescript
interface Foo extends number {  // TS2411: Interface can only extend object types
}

class Bar extends string {  // TS2411: Class can only extend object types
}

function foo<T extends number>(x: T) {  // OK - constraint
    let y: T extends string ? true : false;  // Type operations
}

type Partial<T extends Record<string, any>> = {  // TS2411: If not constrainable
    [K in keyof T]?: T[K];
};
```

**Features needed**:
- Validate `extends` clauses in class/interface declarations
- Type parameter constraint checking
- Conditional type condition validation
- Generic type parameter bounds

**Where to implement**:
- `crates/tsz-checker/src/declarations.rs` - Class/interface extends validation
- `crates/tsz-checker/src/interface_type.rs` - Interface inheritance
- `crates/tsz-solver/src/` - Already has constraint infrastructure

**Current Status**: Solver has type constraints, but Checker doesn't validate interface extends

**Complexity**: Low-Medium

---

## 12. Type Inference Completeness

### TS2353 - Property doesn't exist (object literal)

**Status**: PARTIALLY IMPLEMENTED
**Impact**: 18 tests (all missing) + many in wrong codes

**What it checks**:
```typescript
const obj = { x: 5 };
obj.y;  // TS2353: 'y' does not exist on type '{ x: number }'

// Function return type inference
function getObj() {
    return { x: 5 };
}
getObj().y;  // TS2353: 'y' doesn't exist
```

**Root cause**: Object literal type inference not precise enough in some cases

**Where to implement**:
- `crates/tsz-checker/src/expr.rs` - Object literal type creation
- `crates/tsz-solver/src/` - Object type representation
- Improve literal type tracking

**Current Status**: Basic object inference works but edge cases missing

**Complexity**: Medium

---

## 13. Duplicate Identifier Detection

### TS2300 - Duplicate identifier

**Status**: PARTIALLY IMPLEMENTED
**Impact**: 39 tests (all missing) + 66 in wrong codes

**What it checks**:
```typescript
const x = 5;
const x = 10;  // TS2300: Duplicate declaration 'x'

var x = 5;  // TS2300: Duplicate (var can be redeclared in function scope)
var x = 10;

function foo() {}
function foo() {}  // TS2300: Duplicate (unless overload)

interface Foo {}
interface Foo {}  // OK - declaration merging

class Foo {}
class Foo {}  // TS2300: Duplicate class (no merging)
```

**Context-dependent**:
- `const`/`let`: Never allowed to redeclare (same block scope)
- `var`: Allowed to redeclare in same function scope
- Functions: Allowed if overloads, error if duplicate implementations
- Classes: Never allowed to redeclare
- Interfaces: Allowed (declaration merging)
- Enums: Never allowed to redeclare

**Where to implement**:
- `crates/tsz-binder/src/state_binding.rs` - Symbol binding logic
- Need to track all declarations and validate against rules
- Check before merging symbols

**Current Status**: Some duplicate checking exists but rules incomplete

**Complexity**: Low

---

## 14. Function Overloading

### TS2769 - Overload not found

**Status**: PARTIALLY IMPLEMENTED
**Impact**: 49 tests (false positives) + more in wrong codes

**What it checks**:
```typescript
function foo(x: number): number;
function foo(x: string): string;
function foo(x: number | string) {
    return x;
}

foo(5);        // OK - matches first overload
foo("hello");  // OK - matches second overload
foo(true);     // TS2769: No overload matches
```

**Related codes**:
- TS2339 - Property not found (on unions of overloaded functions)
- TS2344 - Constraint violation in overloads
- TS2683 - Function lacking return type

**Features needed**:
- Store multiple overload signatures
- Match arguments to correct overload
- Better error messages
- Constructor overloads

**Where to implement**:
- `crates/tsz-checker/src/call_checker.rs` - Call signature matching
- `crates/tsz-checker/src/` - Overload resolution strategy
- `crates/tsz-solver/src/` - Type matching with overloads

**Current Status**: Has overload support but matching logic incomplete/too strict

**Complexity**: Medium

---

## 15. Control Flow Analysis & Narrowing

### Incomplete Type Narrowing

**Status**: PARTIALLY IMPLEMENTED
**Impact**: 100+ tests

**What it needs**:
```typescript
const x: string | number = 5;

if (typeof x === "string") {
    x.toUpperCase();  // OK - narrowed to string
} else {
    x.toFixed();      // OK - narrowed to number
}

function isFoo(x: any): x is Foo {
    return x instanceof Foo;
}

const obj: unknown = 5;
if (isFoo(obj)) {
    obj.foo();  // Should be narrowed to Foo (MISSING?)
}
```

**Related codes**:
- TS7006 - Parameter implicitly any (missing narrowing)
- TS2693 - Type narrowing incomplete
- TS2448 - Block scoped binding in incorrect place

**Where to implement**:
- `crates/tsz-checker/src/flow_analyzer.rs` - Type guard evaluation
- `crates/tsz-checker/src/control_flow_narrowing.rs` - Narrowing rules
- `crates/tsz-checker/src/reachability_analyzer.rs` - Dead code analysis

**Current Status**: Basic narrowing works but user-defined type guards may be incomplete

**Complexity**: High

---

## 16. Syntax Error Recovery

### TS1005 - Expression expected (parser errors)

**Status**: PARTIALLY IMPLEMENTED
**Impact**: 56 tests (false positives) + 114 missing

**What happens**:
```typescript
const x = ;  // TS1005: Expression expected

function foo(x?:) {}  // TS1005: Expression expected

const y: => void;  // TS1005: '=>' expected
```

**Issue**: Parser too strict or too lenient in error recovery

**Where to implement**:
- `crates/tsz-parser/src/parser/state.rs` - Error recovery
- `crates/tsz-parser/src/parser/state_expressions.rs` - Expression parsing
- May need grammar refinements

**Current Status**: Parser has error recovery but some rules may be wrong

**Complexity**: Medium

---

## 17. Parameter Type Inference

### TS7006 - Parameter implicitly any

**Status**: PARTIALLY IMPLEMENTED
**Impact**: 52 tests (false positives) + 17 all missing

**What it checks**:
```typescript
function foo(x) {  // TS7006: Parameter 'x' implicitly has an 'any' type
    return x.toString();
}

// This is OK in non-strict mode
// But needs to be reported in strict mode
```

**Features needed**:
- Detect parameters without type annotations
- Only report in strict mode
- Allow inference from usage (should infer from return type)
- Constructor parameters

**Where to implement**:
- `crates/tsz-checker/src/declarations.rs` - Function declaration validation
- `crates/tsz-solver/src/infer.rs` - Parameter type inference

**Current Status**: Can report but over-reporting or not reporting correctly

**Complexity**: Low-Medium

---

## 18. Type Compatibility Edge Cases

### TS2322 - Not assignable (false positives and missing)

**Status**: PARTIALLY IMPLEMENTED
**Impact**: 241 false positives + 126 all missing + 85 quick wins

**Most impactful issue** - appears in ~600+ tests total

**What fails**:
```typescript
const x: readonly number[] = [1, 2, 3];
const y: number[] = x;  // TS2322 - assignment compatibility issue

interface Foo { x: 1 }
const foo: Foo = { x: 1 };  // Literal type vs declared type

// Function variance
const f1: (x: any) => void = (x: never) => {};
const f2: (x: never) => void = (x: any) => {};  // Bivariance rules
```

**Where to implement**:
- `crates/tsz-checker/src/assignability_checker.rs` - Main file
- `crates/tsz-solver/src/judge.rs` & `lawyer.rs` - Type compatibility logic
- Refinement of assignment rules

**Current Status**: Has basic assignment checking but many edge cases wrong

**Complexity**: High

---

## 19. Module Resolution

### TS2307 - Module not found (partial)

**Status**: PARTIALLY IMPLEMENTED
**Impact**: 217 tests missing

**What it checks**:
```typescript
import { foo } from "nonexistent";  // TS2307: Cannot find module 'nonexistent'

import { foo } from "./foo";  // TS2307: Cannot find module './foo'
```

**Features needed**:
- Check if imported module file exists
- Validate path resolution
- Check package.json exports
- Handle .d.ts files

**Where to implement**:
- `crates/tsz-cli/src/driver_resolution.rs` - Module resolution logic
- `crates/tsz-checker/src/symbol_resolver.rs` - Import validation

**Current Status**: Basic module loading works but error checking incomplete

**Complexity**: Medium

---

## Summary by Implementation Effort

### Low Effort (2-4 hours each)

1. **TS2300** - Duplicate identifier detection
2. **TS1100** - Invalid super usage
3. **TS1011** - Initializer not allowed
4. **TS2540** - Cannot assign to const
5. **TS7026** - Unused variables
6. **TS2528** - Readonly property assignment

### Medium Effort (4-8 hours each)

1. **TS2411** - Type extends validation
2. **TS2769** - Overload resolution refinement
3. **TS2353** - Object literal type precision
4. **TS2585** - Object constructor pattern
5. **TS2307** - Module not found detection
6. **TS7006** - Parameter type inference

### High Effort (10+ hours each)

1. **TS2322** - Assignment compatibility (most impactful)
2. **TS2339** - Property access (widespread)
3. **TS2345** - Function argument checking (widespread)
4. **TS2551** - Private member access
5. **TS2708** - Module augmentation
6. **Flow Analysis** - Type narrowing and dead code

---

## Implementation Dependencies

```
Foundation (must-have first):
  - TS2304 (Name resolution) ← Foundation
  - TS2300 (Duplicate detection) ← Depends on resolution

Properties & Access:
  - TS2339 (Property exists) ← Name resolution
  - TS2353 (Object property) ← Property exists
  - TS2551 (Private access) ← Name resolution + access modifiers

Types & Assignment:
  - TS2322 (Assignment) ← Type system (mostly done)
  - TS2345 (Function args) ← Call resolution + assignment
  - TS2769 (Overload) ← Call resolution

Advanced:
  - TS2708 (Module augment) ← Module resolution + merging
  - TS2411 (Type extends) ← Generic constraints (ready)
  - Flow analysis ← Type narrowing infrastructure (ready)
```

---

## Recommended Implementation Order

**For maximum test pass rate improvement:**

1. **Fix false positives** (TS2339, TS2345, TS2322)
   - ~580 tests become pass immediately
   - Requires refining existing checks, not new features

2. **Complete TS2300** (duplicate detection)
   - ~39 all-missing + quick fixes
   - Simple predicate: symbol already in scope?

3. **Complete TS2411** (type extends)
   - ~21 all-missing + 18 quick wins
   - Mostly enabled infrastructure already

4. **Implement TS2551** (private members)
   - ~25 tests + enables further checks
   - Access modifier logic exists, needs connection

5. **Fix TS2769** (overload resolution)
   - ~49 false positives + refinement
   - Call checking structure exists

This prioritization would likely reach **75%+ pass rate** before tackling the long tail of 626 unimplemented codes.

---

## Tracking Progress

As features are implemented:
- Update this document with completion status
- Link to implementation PRs/commits
- Note any blocker dependencies discovered
- Update test pass rate percentages

Current status: 61.2% (7,700 / 12,583)
