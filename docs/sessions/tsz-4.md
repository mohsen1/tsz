# Session tsz-4 - Declaration Emit (.d.ts file generation)

## Date: 2025-02-04

## Status: ACTIVE - Test-Driven Declaration Emit Implementation

### Executive Summary

Session tsz-4 is focused entirely on implementing declaration file generation (`tsc --declaration` or `-d`) using **test-driven development**. All work will be verified against TypeScript's test baselines in `scripts/emit/`.

### Goal: Match TypeScript's Declaration Output Exactly

**For every TypeScript test case, tsz must emit identical `.d.ts` output.**

Example:
```typescript
// Input (test.ts)
export function add(a: number, b: number): number {
    return a + b;
}
export class Calculator {
    private value: number;
    add(n: number): this { ... }
}
```

**Expected output** (must match tsc exactly):
```typescript
// test.d.ts
export declare function add(a: number, b: number): number;
export declare class Calculator {
    private value: number;
    add(n: number): this;
}
```

## Testing Infrastructure

### Existing Test Framework: `scripts/emit/`

The test infrastructure already exists:
- **Runner**: `scripts/emit/run.sh` - Runs emit tests against TypeScript baselines
- **Flags**: `--dts-only` - Test declaration emit only
- **Source**: `scripts/emit/src/runner.ts` - Compares tsz output vs tsc baselines
- **Baselines**: `TypeScript/tests/baselines/reference/` - TypeScript's test outputs

### How to Run Tests

```bash
# Test declaration emit only (run this frequently)
cd scripts/emit
./run.sh --dts-only

# Run specific number of tests
./run.sh --dts-only --max=100

# Filter by test name
./run.sh --dts-only --filter=class

# Verbose output for debugging
./run.sh --dts-only --verbose
```

## Current State

### ✅ Already Implemented
- `src/declaration_emitter.rs` - Basic declaration emitter (~1800 lines)
- Handles: functions, classes, interfaces, type aliases, enums, imports, exports
- Modifiers: public, private, protected, static, readonly, abstract
- Type parameters and constraints
- Heritage clauses (extends, implements)
- **Test infrastructure exists** in `scripts/emit/`

### ❌ Missing/Incomplete Features

**CRITICAL GAP - Type Reification:**
1. **TypeId → TypeScript syntax conversion** - MUST HAVE
   - Need to convert Solver's `TypeId` back into printable syntax
   - Required for inferred types in declarations
   - Example: `function add(a, b) { return a + b; }` → `declare function add(a: any, b: any): any;`
   - Implementation: Create `src/emitter/type_printer.rs` (new module)

2. **Solver integration** - MUST HAVE
   - DeclarationEmitter needs access to Checker/Solver for type queries
   - Must call `get_type_at_location()` when type annotations missing

3. **Test coverage gaps** - MUST FIX
   - Get baseline tests passing in `scripts/emit/`
   - Currently: JS emit works, declaration emit needs implementation

4. **Export filtering** - NEEDED
   - Use Binder's `is_exported` flag to filter output
   - Only emit exported symbols

5. **Import rewriting** - NEEDED
   - Generate correct `import` statements in `.d.ts` files
   - Handle type-only imports

## Implementation Plan (Test-Driven)

### Phase 1: Basic Type Reification (HIGH PRIORITY)

**Goal:** Get simple declaration tests passing

**Tasks:**
1. Create `src/emitter/type_printer.rs` module
2. Implement primitive type printing:
   - `TypeId::STRING` → `"string"`
   - `TypeId::NUMBER` → `"number"`
   - `TypeId::BOOLEAN` → `"boolean"`
   - `TypeId::ANY` → `"any"`
   - `TypeId::VOID` → `"void"`
3. Integrate with DeclarationEmitter
4. Add Solver/Checker context to emitter
5. **Run tests**: `./scripts/emit/run.sh --dts-only --max=50`
6. Fix failures until baseline matches

**Test Cases:**
- Functions with explicit types
- Variables with explicit types
- Simple class declarations

### Phase 2: Composite Types

**Goal:** Handle unions, intersections, arrays

**Tasks:**
1. Implement `TypeKey::Union` printing
   - Join with ` | `
   - Handle parentheses for precedence
2. Implement `TypeKey::Intersection` printing
   - Join with ` & `
3. Implement `TypeKey::Array` printing
   - Output: `string[]` format
4. **Run tests**: `./scripts/emit/run.sh --dts-only --filter=union`
5. Fix failures

**Test Cases:**
- Union types: `type X = string | number;`
- Array types: `const arr: string[];`
- Intersection types: `type Y = A & B;`

### Phase 3: Function and Object Types

**Goal:** Handle function signatures and object literals

**Tasks:**
1. Implement `TypeKey::Function` printing
   - Parameters and return type
   - Type parameters
2. Implement `TypeKey::Object` / `TypeKey::ObjectShape`
   - Property signatures
   - Method signatures
3. **Run tests**: `./scripts/emit/run.sh --dts-only --filter=function`
4. Fix failures

**Test Cases:**
- Function types: `type Fn = (x: number) => string;`
- Object literals: `const obj: { a: number; b: string; };`

### Phase 4: Advanced Features

**Goal:** Handle generics, mapped types, conditional types

**Tasks:**
1. Implement generic type printing
2. Handle type parameters with constraints
3. Implement literal types
4. **Run full test suite**: `./scripts/emit/run.sh --dts-only`
5. Fix remaining failures

## Architecture

### Data Flow
```
TypeScript Source → Parser → Binder (marks is_exported)
                                     ↓
                          Checker/Solver (infers TypeId)
                                     ↓
  ┌────────────────────────────────────────────────┐
  │ DeclarationEmitter                               │
  │   - Checks if type annotation exists in AST      │
  │   - If NO: calls TypePrinter.reify(type_id)     │
  │   - If YES: emits AST node directly             │
  └────────────────────────────────────────────────┘
                                     ↓
                    TypePrinter (NEW MODULE)
                      - Converts TypeId → TypeScript syntax
                      - Handles all TypeKey variants
                                     ↓
                          .d.ts output
```

### Key Components

**Binder** (`src/binder/`)
- Provides `is_exported` flag on symbols
- Already working ✓

**Solver** (`src/solver/`)
- Provides type inference and `TypeId`
- Has `TypeInterner` mapping TypeId → TypeKey
- Already working ✓

**DeclarationEmitter** (`src/declaration_emitter.rs`)
- Orchestration: decides what to emit
- Missing: Solver integration
- Missing: TypePrinter usage

**TypePrinter** (`src/emitter/type_printer.rs`) - **NEW**
- Converts TypeId → TypeScript syntax string
- MUST handle all TypeKey variants
- Output must match tsc exactly

## Code Skeleton (from Gemini)

### File: `src/emitter/type_printer.rs` (NEW)

```rust
use crate::solver::types::{TypeId, TypeKey, TypeInterner};
use crate::solver::types::IntrinsicKind;

pub struct TypePrinter<'a> {
    interner: &'a TypeInterner,
}

impl<'a> TypePrinter<'a> {
    pub fn new(interner: &'a TypeInterner) -> Self {
        Self { interner }
    }

    pub fn print_type(&self, type_id: TypeId) -> String {
        // 1. Check reserved IDs first (fast path)
        match type_id {
            TypeId::STRING => return "string".to_string(),
            TypeId::NUMBER => return "number".to_string(),
            TypeId::BOOLEAN => return "boolean".to_string(),
            TypeId::ANY => return "any".to_string(),
            TypeId::VOID => return "void".to_string(),
            _ => {}
        }

        // 2. Look up the structure
        let type_key = match self.interner.get(type_id) {
            Some(k) => k,
            None => return "any".to_string(),
        };

        // 3. Switch on structure
        match type_key {
            TypeKey::Intrinsic(kind) => self.print_intrinsic(kind),
            TypeKey::Union(types) => self.print_union(types),
            TypeKey::Intersection(types) => self.print_intersection(types),
            TypeKey::Array(elem_id) => format!("{}[]", self.print_type(elem_id)),
            TypeKey::Function(func) => self.print_function(func),
            TypeKey::Object(shape) => self.print_object(shape),
            _ => "any".to_string(),
        }
    }
}
```

### File: `src/declaration_emitter.rs` (MODIFY)

```rust
use crate::emitter::type_printer::TypePrinter;
use crate::solver::types::TypeId;

pub struct DeclarationEmitter<'a> {
    arena: &'a NodeArena,
    writer: SourceWriter,
    // NEW: Add solver access
    checker: &'a CheckerContext,  // or Solver directly
    // ... rest of fields
}

impl<'a> DeclarationEmitter<'a> {
    fn emit_variable_declaration(&mut self, node: NodeIndex) {
        // ... emit "declare const x" ...

        if let Some(type_annotation) = self.get_type_annotation(node) {
            // Existing: emit AST node directly
            self.emit_type(type_annotation);
        } else {
            // NEW: Reify inferred type from Solver
            let type_id = self.checker.get_type_at_location(node);
            let printer = TypePrinter::new(&self.checker.solver.interner);
            let type_text = printer.print_type(type_id);
            self.write(": ");
            self.write(&type_text);
        }

        self.write(";");
    }
}
```

## Session Coordination

**Other Sessions** (no conflicts):
- **tsz-1**: Parse errors (TS1005, TS1109, etc.)
- **tsz-2**: Module resolution (TS2307, TS2664, TS2322)
- **tsz-3**: Const type parameters, type system issues

**Declaration emit is independent** - no overlap with other sessions

## Success Criteria

### Definition of Done

1. ✅ All declaration tests pass: `./scripts/emit/run.sh --dts-only`
2. ✅ No regressions in JS emit tests
3. ✅ Output matches tsc byte-for-byte for all test cases
4. ✅ TypePrinter handles all TypeKey variants
5. ✅ DeclarationEmitter integrated with Solver

### Test Coverage

- Run `./scripts/emit/run.sh --dts-only` frequently (after every commit)
- Compare output against tsc baselines
- Fix mismatches immediately

## Commits

- `7142615c0` - docs: restructure tsz-4 session for declaration emit work
- `d18a96de5` - feat: add TypePrinter module for declaration emit

## Progress

### ✅ Completed (2026-02-04)

**Phase 1.1: TypePrinter Module Created**
- Created `src/emitter/type_printer.rs` module
- Implemented intrinsic type printing (all primitives)
- Added skeleton methods for all TypeKey variants
- Module compiles and all tests pass
- Fast path for TypeId < 100 (built-in types)
- Uses `TypeInterner::lookup()` for user-defined types

**Committed:**
- 246 lines added
- Test suite: 23 passed
- All pre-commit checks passed

### 🚧 In Progress

**Phase 1.2: Composite Type Printing**
- Need to implement: union, intersection, array printing
- Need to implement: object literal printing
- Need to implement: function type printing

### 📋 TODO

**Phase 1.3: Integration**
- Integrate TypePrinter with DeclarationEmitter
- Add Solver/Checker context access
- Test with real TypeScript files
- Run `./scripts/emit/run.sh --dts-only` to verify

## Next Steps

1. ✅ Reviewed existing DeclarationEmitter implementation
2. ✅ Identified test infrastructure in `scripts/emit/`
3. ✅ Got implementation plan from Gemini
4. ✅ Created `src/emitter/type_printer.rs` module
5. ✅ Implemented primitive type printing
6. **NEXT**: Implement composite types (union, intersection, array)
7. **NEXT**: Integrate with DeclarationEmitter
8. **NEXT**: Run tests and fix failures

## Resources

- Gemini conversation 2026-02-04: Declaration emit architecture and implementation plan
- File: `src/declaration_emitter.rs` - Current implementation
- File: `scripts/emit/src/runner.ts` - Test runner
- File: `docs/architecture/NORTH_STAR.md` - Architecture reference
- Command: `./scripts/emit/run.sh --dts-only` - Run declaration tests

---

## Notes

- **Test-driven development**: Run tests after every change
- **Match tsc exactly**: Output must be byte-identical
- **Start simple**: Primitives first, then composites
- **Use existing infrastructure**: `scripts/emit/` is already set up
