# Session tsz-4 - Declaration Emit (.d.ts file generation)

## Date: 2026-02-04

## Status: ACTIVE - Fixing Test Infrastructure

### Current Task: Migrate Test Runner to Native CLI ✅ (COMPLETED)

**What was done:**
1. Created `scripts/emit/src/cli-transpiler.ts` - CLI-based transpiler using native tsz binary
2. Modified `scripts/emit/src/runner.ts` to use CliTranspiler instead of WASM workers
3. Updated `scripts/emit/run.sh` to remove WASM build requirement
4. Verified tests run correctly with full type checking enabled

**Test Results:**
```bash
cd scripts/emit && ./run.sh --dts-only --verbose
# Working! Detects differences in DTS output
```

**Why Critical**: Can now use TDD for remaining features (Enums, Lazy Types) with working test infrastructure.

### Goals

Match TypeScript's declaration output exactly using **test-driven development**. All work will be verified against TypeScript's test baselines in `scripts/emit/`.

**For every TypeScript test case, tsz must emit identical `.d.ts` output.**

### Current Status

**✅ Completed:**
- CLI declaration emit with type inference verified working
- TypePrinter handles: primitives, unions, intersections, tuples, objects, functions, generics
- DeclarationEmitter uses inferred types from type cache
- Test infrastructure can compare DTS output (status display fixed)
- **Test runner migrated to CLI-based testing** ✅ NEW

**⏳ In Progress:**
- Implementing missing TypePrinter features

**📋 TODO (Prioritized Order):**
1. **[IN PROGRESS] Implement Enums** - Numeric, string, and const enum declaration emit (user-facing feature)
2. **Implement Lazy Types** - Handle `TypeKey::Lazy(DefId)` for circular references (internal refactor)
3. **Fix failing tests** - Use test runner to identify and fix declaration emit issues

## Testing Infrastructure

### Existing Test Framework: `scripts/emit/`

The test infrastructure already exists:
- **Runner**: `scripts/emit/run.sh` - Runs emit tests against TypeScript baselines
- **Flags**: `--dts-only` - Test declaration emit only
- **Source**: `scripts/emit/src/runner.ts` - Compares tsz output vs tsc baselines
- **Baselines**: `TypeScript/tests/baselines/reference/` - TypeScript's test outputs

**Current Limitation**: Runner uses WASM workers which lack type checking.

### How to Run Tests (Manual CLI Testing)

```bash
# Manual testing with CLI (current workaround)
tsz --declaration test.ts
cat test.d.ts
```

## Architecture

### Data Flow
```
TypeScript Source → Parser → Binder → Checker/Solver (infers TypeId)
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
- ✅ Now integrates with type cache
- Uses TypePrinter for inferred types

**TypePrinter** (`src/emitter/type_printer.rs`)
- Converts TypeId → TypeScript syntax string
- ✅ Handles: primitives, composites, functions, objects, tuples, generics
- ⏳ TODO: Lazy types, enums, conditional types

## Implementation Progress

### ✅ Phase 1: Basic Type Reification (COMPLETED)

**Tasks:**
1. ✅ Created `src/emitter/type_printer.rs` module
2. ✅ Implemented intrinsic type printing (all primitives)
3. ✅ Integrated with DeclarationEmitter
4. ✅ Added Solver/Checker context to emitter
5. ✅ CLI declaration emit works end-to-end

**Test Results:**
- CLI: ✅ Working (verified with `tsz --declaration`)
- Test runner: ⏳ Blocked by WASM API limitations

### ✅ Phase 2: Composite Types (COMPLETED)

**Tasks:**
1. ✅ Union types: `A | B | C` (joined with " | ")
2. ✅ Intersection types: `A & B & C` (joined with " & ")
3. ✅ Tuple types: `[A, B, C]` with optional/rest support
4. ✅ Object types: `{ prop: Type; ... }`
5. ✅ Function types: `<T>(a: Type, b: Type) => ReturnType`
6. ✅ Generic type applications: `Base<Args>`

**Total: 389 lines of type printing code**

### ✅ Phase 3: Integration & Type Inference (COMPLETED)

**Tasks:**
1. ✅ Added `TypeCache::merge()` method
2. ✅ Modified `DeclarationEmitter` to accept `TypeCache` and `TypeInterner`
3. ✅ Updated `emit_outputs()` to pass type caches from compilation
4. ✅ Modified property declaration emit to reify inferred types
5. ✅ Fixed CLI to always create `local_cache` for type checking results

**Verification:**
```typescript
// Input (test.ts)
export function add(a: number, b: number): number {
    return a + b;
}
export const x: string = "hello";

// Output (test.d.ts) - MATCHES tsc
export declare function add(a: number, b: number): number;
export declare const x: string;
```

### ⏳ Phase 4: Test Infrastructure Migration (IN PROGRESS)

**Problem:** Test runner uses WASM API which lacks type checking

**Solution:** Migrate test runner to invoke native CLI binary

**Status:** TODO

### 📋 Phase 5: Advanced TypePrinter Features (TODO)

**Tasks:**
1. Lazy Types - Handle `TypeKey::Lazy(DefId)` for circular references
2. Enum Types - Numeric, string, and const enums
3. Conditional Types
4. Mapped Types
5. Template Literal Types

## Success Criteria

- [ ] Test runner migrated to CLI-based testing
- [ ] Lazy types implemented and tested
- [ ] Enum types implemented and tested
- [ ] CLI declaration emit matches tsc for all test cases
- [ ] No regressions in previously passing tests

## Resources

- File: `src/declaration_emitter.rs` - Declaration emitter implementation
- File: `src/emitter/type_printer.rs` - Type reification module
- File: `scripts/emit/src/runner.ts` - Test runner
- Command: `./scripts/emit/run.sh --dts-only` - Run declaration tests
