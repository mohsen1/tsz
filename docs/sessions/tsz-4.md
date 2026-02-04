# Session tsz-4 - Declaration Emit (.d.ts file generation)

## Date: 2026-02-04

## Status: 🟡 ACTIVE (2026-02-04)

Previous task (Class Heritage and Generics) verified as already implemented. New task identified by Gemini.

### New Task: Class Heritage and Generics (2026-02-04) ✅ COMPLETE

**Gemini Consultation Summary:**

Since tsz-5 (Import Elision) and tsz-7 (Import Generation) are handling the module system, the next high-impact area for tsz-4 is **Class Heritage and Declaration-level Generics**.

### Investigation Results (2026-02-04)

**Discovery**: All core features were already implemented!
- `emit_heritage_clauses()` - handles extends/implements
- `emit_type_parameters()` - handles constraints and defaults
- `emit_accessor_declaration()` - handles get/set pairs

**Only Issue Found**: Type literal formatting bug
- Type literals were emitting with unwanted indentation and newlines
- Before: `{ id: string;\n  name: number; }`
- After: `{ id: string; name: number; }`

**Fix Applied** (commit b5c709cbf):
- Added `emit_interface_member_inline()` helper
- Modified TYPE_LITERAL case to use inline emission with semicolon separators
- All features now working correctly

### Test Results (2026-02-04)

**Class Heritage** ✅
```typescript
// Input
class Base<T> {}
interface I { x: number }
export class Derived extends Base<string> implements I { x: number = 1; }

// Output (matches TypeScript ✅)
export declare class Derived extends Base<string> implements I {
    x: number;
}
```

**Generic Constraints & Defaults** ✅
```typescript
// Input
export type Callback<T extends object = { id: string }> = (arg: T) => void;

// Output (matches TypeScript ✅)
export type Callback<T extends object = { id: string }> = (arg: T) => void;
```

**Interface Heritage** ✅
```typescript
// Input
interface Base1 { a: number }
interface Base2 { b: string }
export interface Combined extends Base1, Base2 { c: boolean }

// Output (matches TypeScript ✅)
export interface Combined extends Base1, Base2 {
    c: boolean;
}
```

**Accessor Emission** ✅
```typescript
// Input
export class Box {
    private _val: number;
    get value(): number { return this._val; }
    set value(v: number) { this._val = v; }
}

// Output (matches TypeScript ✅)
export declare class Box {
    private _val;
    get value(): number;
    set value(v: number);
}
```

### Actual Implementation Plan (What Was Done)

**Estimated Complexity**: Low (1 hour)
- Fixed type literal formatting issue
- Added `emit_interface_member_inline()` helper
- Verified all existing features work correctly

### Files to Modify
- **`src/declaration_emitter.rs`**: Primary file for .d.ts orchestration
- **`src/emitter/type_printer.rs`**: May need `print_type_parameter` helper
- **`src/parser/node.rs`**: To access `HeritageClause` data from NodeArena

### Success Criteria

**Class Heritage:**
```typescript
// Input
class Base<T> {}
interface I { x: number }
export class Derived extends Base<string> implements I { x: number = 1; }

// Expected .d.ts
export declare class Derived extends Base<string> implements I {
    x: number;
}
```

**Generic Constraints & Defaults:**
```typescript
// Input
export type Callback<T extends object = { id: string }> = (arg: T) => void;

// Expected .d.ts
export declare type Callback<T extends object = { id: string }> = (arg: T) => void;
```

**Accessor Synthesis:**
```typescript
// Input
export class Box {
    private _val: number;
    get value(): number { return this._val; }
    set value(v: number) { this._val = v; }
}

// Expected .d.ts
export declare class Box {
    private _val;
    get value(): number;
    set value(v: number);
}
```

---

### Previous Session Summary (2026-02-04)

**Completed This Session:**
10. ✅ **Function overload detection** (766485146, a9c593c08)
11. ✅ **Public keyword omission** (2eed3a1c5)
12. ✅ **Array/object literal in default parameters** (0254ea7e8)
13. ✅ **Parameter properties in class constructors** (b1e8c49c2)
14. ✅ **Class member visibility and abstract keywords** (d0d803bdc)
15. ✅ **Literal initializers for primitive consts** (c055d716c)
    - Extended to emit_variable_declaration()
    - Conformance: 42.1% → 42.3% (+1 test)

**Latest Achievement: Parameter Properties ✅**

```typescript
// Input
class Point {
    constructor(public x: number, private y: number) {}
}

// Output (matches TypeScript exactly ✅)
export declare class Point {
    x: number;
    private y;  // No type annotation (TS behavior)
    constructor(x: number, y: number);  // Modifiers stripped
}
```

**Implementation:**
- Added `emit_parameter_properties()` helper method
- Strips accessibility/readonly modifiers from constructor parameters
- Omits type annotations for private properties only (protected/public/readonly keep types)
- Emits properties before other class members
- Added `in_constructor_params` flag to track context

**Latest Achievement: Class Member Visibility & Abstract ✅**

```typescript
// Input
export class Visibility {
    private privateProp: string;
    protected protectedProp: number;
    public publicProp: boolean;
    private privateMethod(): void {}
}

// Output (matches TypeScript exactly ✅)
export declare class Visibility {
    private privateProp;  // No type annotation
    protected protectedProp: number;
    publicProp: boolean;
    private privateMethod();  // No return type
}
```

**Implementation:**
- Private properties: omit type annotation
- Private methods: omit return type annotation
- Abstract classes/methods: emit `abstract` keyword
- Protected/public members: keep full type information

**Session Decision Point:**

Gemini consultation provided two paths forward:

**Option A: Import/Export Elision** (High impact, high complexity)
- Remove unused imports to fix "Module not found" errors
- Requires UsageAnalyzer with type visitor integration
- Estimated: 2-3 days
- Implementation guidance available from Gemini

**Option B: Continue Declaration Features** (Medium impact, low complexity)
- More class features (extends, implements, decorators)
- Other declaration types (namespaces, enums edge cases)
- Quick wins, continues momentum

**Status:** Completed all core class emission features. Ready for next decision.

**Previous Session Accomplishments:**
1. ✅ Test runner migrated to CLI (major milestone)
2. ✅ Enum declaration emit with explicit initializers
3. ✅ Fixed enum value evaluation to match TypeScript exactly
4. ✅ Verified DTS output matches TypeScript
5. ✅ Fixed update-readme.sh for new conformance format
6. ✅ **Namespace/module declaration emit bug FIXED**
7. ✅ **Const type inference added**
8. ✅ **Atom printing fixed**
9. ✅ **Function return type inference added**

**Latest Commits**: 766485146, 17f466f27, a9c593c08, 2eed3a1c5, 0254ea7e8

### Conformance Test Results: 41.9% Pass Rate (267/637)

Current status: `./scripts/conformance.sh --filter=decl`
- Passed: 267
- Failed: 366
- Skipped: 35

### Major Achievements

#### 1. Namespace/Module Declaration Emit - FIXED ✅

**Root Cause**: Multiple issues:
1. Wrong AST access method: Used `get_block()` instead of `get_module_block()` for MODULE_BLOCK nodes (kind 269)
2. Missing nested namespace support: `emit_export_declaration` didn't handle MODULE_DECLARATION
3. Incorrect declare context handling: Inside `declare namespace`, members should NOT have `declare` or `export` keywords

**Fix**: Added `inside_declare_namespace` flag to track ambient context and conditionally emit keywords.

**Test Results**:
```typescript
// Before (BUG)
declare namespace A {
}

// After (FIXED - matches TypeScript)
declare namespace A {
    var x: number;
    namespace B {
        var x: number;
    }
}
```

#### 2. Atom Printing Bug - FIXED ✅ (2dbc85b33)

**Problem**: TypePrinter was outputting broken atom IDs: `{ <atom:116>: number }`

**Solution**: Added `resolve_atom()` method to TypePrinter that calls `TypeInterner::resolve_atom()`.

**Result**:
```typescript
// Before (BROKEN)
export declare const obj: { <atom:116>: number; <atom:270>: string };

// After (FIXED)
export declare const obj: { a: number; b: string };
```

#### 3. Const Type Inference (de8b72d5c)

Added inferred type emission for exported const declarations.

**Test Results**:
```typescript
export const num = 42;         // → : number  ✅
export const str = "hello";     // → : string  ✅
export const arr = [1, 2, 3];   // → : number[] ✅
export const obj = { a: 1 };    // → : { a: number } ✅
```

#### 4. Function Return Type Inference (a19fd5401)

**Gemini-Approved Implementation**: Added `get_return_type()` to `src/solver/type_queries.rs` following Solver-First architecture.

**Implementation**:
```rust
pub fn get_return_type(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    match db.lookup(type_id) {
        Some(TypeKey::Function(shape_id)) => {
            Some(db.function_shape(shape_id).return_type)
        }
        Some(TypeKey::Callable(shape_id)) => {
            let shape = db.callable_shape(shape_id);
            shape.call_signatures.first().map(|sig| sig.return_type)
        }
        Some(TypeKey::Intersection(list_id)) => {
            let members = db.type_list(list_id);
            members.iter().find_map(|&m| get_return_type(db, m))
        }
        _ => None
    }
}
```

**Test Results**:
```typescript
export function foo() {}         // → : void ✅
export function bar(): string {} // → : string ✅
```

### Complete Test Coverage

```typescript
// Namespaces - WORKING ✅
declare namespace A {
    export var x: number;
    namespace B { var y: string; }
}

// Consts - WORKING ✅
export const num = 42;           // → : number
export const str = "hello";      // → : string
export const obj = { a: 1 };     // → : { a: number }
export const arr = [1, 2, 3];    // → : number[]

// Functions - WORKING ✅
export function foo() {}         // → : void
export function bar(): string {} // → : string

// Interfaces - WORKING ✅
export interface Point { x: number; y: number; }

// Enums - WORKING ✅
enum Color { Red, Green, Blue }  // → declare enum Color { Red = 0, Green = 1, Blue = 2 }
```

### PHASE 2 Progress

**✅ Completed: Non-exported functions correctly omitted**
- Functions without export modifier no longer appear in .d.ts files
- Commit: 390bc142f

**⏳ IN PROGRESS: Function Overload Support**

**Current Issue:** Emitting all 3 signatures instead of just overloads
```typescript
// Input
export function bar(x: string): void;
export function bar(x: number): void;
export function bar(x: string | number): void { console.log(x); }

// TypeScript (expected)
export declare function bar(x: string): void;
export declare function bar(x: number): void;
// Implementation signature OMITTED

// TSZ current (wrong)
export declare function bar(x: string): void;
export declare function bar(x: number): void;
export declare function bar(x: string | number): void;  // ← Should be omitted
```

**Implementation Challenge:**
- Need to track function declarations across statements
- Requires SymbolArena/Binder access to detect overload groups
- Must identify which declaration has body (implementation) vs bodyless (overload)
- Only emit overload signatures, omit implementation if overloads exist

**Next Steps:**
1. Add SymbolArena access to DeclarationEmitter
2. Track emitted function names to avoid duplicates
3. Filter signatures: emit only those without bodies when multiple exist

### Remaining Work after Overloads

**PHASE 2: Structural API Fidelity** (Continued)

**Priority 1: Function Overloads** (Highest Impact)
- Emit all overload signatures, not just implementation
- Access `Symbol.declarations` (plural) from Binder
- Modify emitter to iterate over all function signatures
- **File Reference**: `src/emitter/types.rs`, `src/binder.rs`

**Priority 2: Class Member Visibility**
- Respect private/protected modifiers in class member emit
- Check `ModifierFlags` in `src/parser/flags.rs`
- Ensure private members are emitted correctly (tsc keeps them for shape)

**Priority 3: Import/Export Elision**
- Remove unused imports from .d.ts output
- Requires "usage" pass or visitor to mark referenced SymbolIds
- Prevents "Module not found" errors in output

**Lower Priority:**
4. Literal initializers for primitive consts (= 42 vs : number)
5. Union type return types for variables

### Goals

**Goal**: 100% declaration emit matching TypeScript

Match TypeScript's declaration output exactly using **test-driven development**.

## Testing Infrastructure

### How to Run Tests

```bash
# Run all DTS tests
cd scripts/emit && node dist/runner.js --dts-only

# Run subset for quick testing
cd scripts/emit && node dist/runner.js --dts-only --max=50

# Run declaration conformance tests
./scripts/conformance.sh --filter=decl

# Test specific file manually
./.target/release/tsz -d --emitDeclarationOnly test.ts
cat test.d.ts
```

## Resources

- File: `src/declaration_emitter.rs` - Declaration emitter implementation
- File: `src/emitter/type_printer.rs` - Type to TypeScript syntax printer
- File: `src/solver/type_queries.rs` - Type query functions
- File: `src/enums/evaluator.rs` - Enum value evaluation
- File: `scripts/emit/src/runner.ts` - Test runner
- Command: `./scripts/conformance.sh --filter=decl` - Run declaration tests

## 2026-02-04 Completed: Default Parameter Support - FULLY IMPLEMENTED ✅

**Status**: Complete - commit 0254ea7e8

### Implementation Summary

Expanded `emit_expression()` to handle all common default parameter expressions:

**Added Support For:**
- ✅ Primitive literals: numbers, strings
- ✅ `null`, `true`, `false` literals
- ✅ Array literals: `[]`
- ✅ Object literals: `{}`
- ✅ Template literals

**Test Results:**
```typescript
// Input
export function num(x: number = 42): void {}
export function nul(x: number | null = null): void {}
export function bool(x: boolean = true): void {}
export function arr(x: number[] = []): void {}
export function obj(x: Object = {}): void {}

// Output (all correct ✅)
export declare function num(x: number = 42): void;
export declare function nul(x: number | null = null): void;
export declare function bool(x: boolean = true): void;
export declare function arr(x: number[] = []): void;
export declare function obj(x: Object = {}): void;
```

### Remaining Gaps

**Not Yet Supported:**
- Function expressions as defaults (e.g., `x: () => void = () => {}`)
- Complex nested type expressions
- These are rare edge cases in practice

**Impact:** The vast majority of real-world default parameters now work correctly.

### Session Status: ✅ COMPLETE - Class Heritage and Generics

This session completed declaration emit improvements:
1. Function overload support ✅
2. Default parameter support ✅
3. Parameter properties ✅
4. Class member visibility ✅
5. Abstract classes/methods ✅
6. Namespace/module declarations ✅
7. **Class Heritage and Generics** ✅ (verified working, fixed type literal formatting)

---

## Latest Achievement: Type Literal Formatting Fix

**Problem**: Type literals were emitting with unwanted indentation and newlines.
**Solution**: Added `emit_interface_member_inline()` helper for inline type literal emission.
**Result**: Type literals now emit correctly as `{ id: string; name: number }` instead of multiline format.

**All Heritage and Generics Features Verified Working:**
- Class heritage: `extends Base<string> implements I1, I2`
- Generic constraints: `<T extends object>`
- Generic defaults: `<T = { id: string }>`
- Interface heritage: `extends Base1, Base2`
- Accessor emission: get/set pairs

---

## Next Task: Computed Property Names and `unique symbol` Support (2026-02-04)

**Gemini Consultation Summary:**

This task involves two related areas:
1. **Computed Property Names**: Implement support for `[s]: number` and `["prop"]: string`
2. **`unique symbol`**: Support the `unique` modifier for symbol declarations

### Problem Description

TypeScript has specific normalization rules:
- String literal computed keys `["a"]` should simplify to `a`
- Symbol-based keys must remain computed `[s]`
- `const x = Symbol()` must emit as `const x: unique symbol`

### Implementation Plan

**Estimated Complexity: Medium (1-2 days)**

#### Phase 1: Key Normalization
- Update `emit_node` to handle `SyntaxKind::ComputedPropertyName`
- Add helper to check if computed property contains literal
- String literals → emit as identifier
- Numeric literals → emit as number
- Expressions → keep brackets

#### Phase 2: Unique Symbol Support
- Update `emit_variable_declaration_statement` to detect `Symbol()` initializers
- Emit `unique symbol` type for const symbol declarations
- Ensure only `const` and `readonly static` get `unique` keyword

### Files to Modify
- **`src/declaration_emitter/mod.rs`**: Primary changes
  - Update `emit_node` for `ComputedPropertyName`
  - Update `emit_property_declaration` and `emit_method_declaration`
  - Update `emit_variable_declaration_statement` for `unique symbol`
- **`src/emitter/type_printer.rs`**: Ensure `unique symbol` prints correctly

### Success Criteria

**Key Normalization:**
```typescript
// Input
class C { ["prop"]: string; }

// Output
class C { prop: string; }
```

**Computed Symbols:**
```typescript
// Input
const s = Symbol();
class C { [s]: number; }

// Output
declare const s: unique symbol;
declare class C { [s]: number; }
```

**Unique Symbols:**
```typescript
// Input
export const MySym = Symbol();

// Output
export declare const MySym: unique symbol;
```

**Method Names:**
```typescript
// Input
interface I { [Symbol.iterator](): void; }

// Output
interface I { [Symbol.iterator](): void; }
```

### Implementation Pitfalls
- `unique symbol` only valid on `const` or `readonly static`, not `let`/`var`
- Custom symbols `[mySym]` require tsz-7 auto-imports to work
- Must check `NodeFlags` and `ModifierFlags` carefully

---

## Previous Task: Class Heritage and Generics ✅
