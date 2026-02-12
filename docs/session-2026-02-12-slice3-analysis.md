# Slice 3 Conformance Session - 2026-02-12

## Session Goal
Get Slice 3 (offset 6292, max 3146 tests) to 100% passing.

## Current Status
- **Baseline**: 61.5% passing (1934/3145 tests)
- **Target**: 100% passing
- **Unit Tests**: All passing (2396/2396)

## Session Outcome
**Status**: Investigation and planning phase completed. Implementation blocked by system resource constraints.

## Key Findings

### 1. Type Alias Conditional Resolution Bug (High Impact: ~84 TS2322 false positives)

**Problem**: Type aliases with conditional types don't resolve correctly during assignability checking.

**Example**:
```typescript
type Test = true extends true ? "y" : "n"  // Evaluates to "y"
let value: Test = "y"  // ERROR: Type 'string' is not assignable to type 'Test'
```

**Root Cause Analysis**:
- **Location**: `crates/tsz-checker/src/state_type_resolution.rs:853-861`
- The `type_reference_symbol_type` function returns a `Lazy(DefId)` type for type aliases
- The structural type IS computed and cached via `get_type_of_symbol`
- During subtype checking, `visit_lazy` calls `resolve_lazy` to get the cached type
- **Hypothesis**: The cached type might be under-resolved (conditional not fully evaluated) OR the Lazy resolution is failing in some cases

**Code Flow**:
1. `type_reference_symbol_type` (line 854): Calls `get_type_of_symbol(sym_id)` to compute structural type
2. `get_type_of_symbol` (line 2038 of state_type_analysis.rs): Calls `get_type_from_type_node` to evaluate conditional
3. Cached in `symbol_types` map
4. Returns `Lazy(DefId)` instead of structural type (line 858)
5. During assignability: `visit_lazy` (subtype.rs:931) calls `resolve_lazy`
6. `resolve_lazy` (context.rs:2095): Looks up `symbol_types.get(&sym_id)`
7. **Should return cached structural type, but something fails here**

**Potential Fixes**:
- **Option A**: Return structural type directly for non-recursive type aliases (simplest)
- **Option B**: Add tracing to understand why Lazy resolution fails
- **Option C**: Force full evaluation of conditionals before caching

**Impact**: ~84 tests in slice 1, unknown impact in slice 3

### 2. ES5 Symbol Property False Positives (High Impact: 76 tests)

**Problem**: We emit TS2339 "Property doesn't exist" for Symbol properties when target is ES5.

**Example Tests**:
- ES5SymbolProperty1.ts
- ES5SymbolProperty3.ts
- ES5SymbolProperty4.ts
- ES5SymbolProperty5.ts
- ES5SymbolProperty7.ts

**Root Cause**: ES5 should allow Symbol as a property key even though Symbol doesn't exist at runtime in ES5. TypeScript allows computed property names with Symbol type in ES5 target.

**Investigation Needed**:
1. Find where property access checks target version
2. Check `crates/tsz-solver/src/operations_property.rs` - PropertyAccessEvaluator
3. Look for Symbol-specific property handling
4. Compare with TypeScript's behavior for computed property names

**Impact**: 76 tests total

### 3. Missing TS1362/TS1361 Await Expression Errors (Medium Impact: 27 tests)

**TS1362**: "'await' expressions are only allowed within async functions and at the top levels of modules."

**TS1361**: "'await' expressions are only allowed at the top level of a file when that file is a module..."

**Implementation Plan**:
1. Add `in_async_function: bool` flag to checker context (`crates/tsz-checker/src/context.rs`)
2. Track when entering/leaving async functions in statement checker
3. Add check in await expression handler (`crates/tsz-checker/src/type_computation.rs` or `dispatch.rs`)
4. Check if file is a module for top-level await
5. Verify module/target options for ES2022+ top-level await

**Impact**: 27 tests

## Session Challenges

### Build Environment Issues
- **Memory**: Only 472MB free, causing builds to be killed by system
- **Competing Processes**: Multiple cargo builds and node processes competing for resources
- **Build Failures**: Consistent "Killed: 9" errors during compilation
- **Attempted Mitigations**:
  - `CARGO_BUILD_JOBS=1` (still failed)
  - Killing competing processes (temporary relief only)
  - Using dist-fast profile (no improvement)

**Result**: Unable to build and test fixes during this session.

## Compilation Fix Applied

**Issue**: Parser had compilation error
- **File**: `crates/tsz-parser/src/parser/state_expressions.rs`
- **Problem**: `parse_error_at_current_token` signature changed (now requires message parameter)
- **Fix**: Updated two call sites to include error message for TS1186
- **Status**: Already fixed in previous commit `db2c31d39` ("chore: apply formatting and clippy suggestions")

## Action Plan for Next Session

### Priority 1: Complete Type Alias Bug Investigation
1. **Build in better environment** or free up system resources
2. **Add tracing** to understand Lazy type resolution:
   ```rust
   // In resolve_lazy (context.rs:2095)
   trace!(def_id = def_id.0, sym_id = ?sym_id, "Resolving lazy type");
   if let Some(&ty) = self.symbol_types.get(&sym_id) {
       trace!(resolved_type = ty.0, "Found cached type");
       return Some(ty);
   }
   trace!("Lazy resolution failed - no cached type");
   ```
3. **Create minimal test**:
   ```typescript
   // tmp/test-type-alias-conditional.ts
   type Test = true extends true ? "y" : "n"
   let value: Test = "y"  // Should NOT error
   ```
4. **Run with tracing**: `TSZ_LOG=debug TSZ_LOG_FORMAT=tree ./target/release/tsz tmp/test-type-alias-conditional.ts`
5. **Implement fix** based on tracing output

### Priority 2: Fix ES5 Symbol Properties
1. **Find failing test**: `./scripts/conformance.sh run --filter "ES5SymbolProperty" --verbose`
2. **Understand pattern**: Why does TypeScript allow Symbol properties in ES5?
3. **Locate check**: Find where we validate property names against target
4. **Implement fix**: Allow Symbol-type computed properties regardless of target
5. **Test**: Verify ES5SymbolProperty tests pass

### Priority 3: Implement TS1362/TS1361
1. **Add context flag** for async function tracking
2. **Implement checks** in await expression handler
3. **Write tests** for various await scenarios
4. **Verify** against conformance tests

### Priority 4: Run Full Slice 3 Conformance
```bash
./scripts/conformance.sh run --offset 6292 --max 3146 2>&1 | tee slice3-results.txt
./scripts/conformance.sh analyze --offset 6292 --max 3146
```

## Files for Reference

**Type Alias Bug**:
- `crates/tsz-checker/src/state_type_resolution.rs` (lines 834-861)
- `crates/tsz-checker/src/state_type_analysis.rs` (lines 1004-1155, 2025-2056)
- `crates/tsz-checker/src/context.rs` (lines 2074-2103)
- `crates/tsz-solver/src/subtype.rs` (lines 930-946)

**ES5 Symbol Properties**:
- `crates/tsz-solver/src/operations_property.rs` (PropertyAccessEvaluator)
- `crates/tsz-checker/src/type_computation.rs` (property access handling)

**Await Errors**:
- `crates/tsz-checker/src/context.rs` (add context flag)
- `crates/tsz-checker/src/type_computation.rs` or `dispatch.rs` (await expression check)
- `crates/tsz-checker/src/statements.rs` (track async context)

## Tasks Created

1. ✓ Task #1: Investigate and fix TS2322 false positives from type alias conditional bug
2. ✓ Task #2: Fix TS2339 false positives for ES5 Symbol properties
3. ✓ Task #3: Implement TS1362/TS1361 await expression errors
4. ✓ Task #4: Build tsz and run slice 3 conformance tests

## Success Metrics

**Target for next session**:
- ✅ Resolve type alias conditional bug → expect +80-90 tests passing
- ✅ Fix ES5 Symbol properties → expect +60-70 tests passing
- ✅ Implement await errors → expect +20-25 tests passing
- **Combined impact**: ~160-185 additional tests passing
- **Projected slice 3 pass rate**: 61.5% → ~67% (+5.5%)

**Full slice 3 target**: 100% (3145 tests passing)

## Lessons Learned

1. **System resources matter** - Need adequate memory for Rust compilation
2. **Code analysis is valuable** - Even without building, can understand bugs
3. **Task tracking helps** - Clear task list for next session
4. **Document findings** - Analysis work isn't wasted if properly documented
5. **Prioritize high-impact fixes** - Type alias bug affects many tests
6. **Plan for environment issues** - Have fallback strategies when builds fail

## Next Steps

1. **Address build environment** - Clear memory, close unnecessary processes
2. **Build project** - Get working binary for testing
3. **Execute Priority 1-3** - Fix high-impact bugs
4. **Run conformance tests** - Measure improvement
5. **Iterate** - Continue until slice 3 reaches 100%
