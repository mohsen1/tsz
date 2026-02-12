# Conformance Test Investigation - Slice 1 (2026-02-12)

## Current Status
**Pass Rate:** 68.3% (2,145/3,139 passing)
**Slice:** 1 of 4 (tests 0-3,145)

## Error Distribution

### False Positives (320 tests - we emit, TSC doesn't)
- **TS2345:** 122 tests - Argument type not assignable
- **TS2322:** 107 tests - Type not assignable  
- **TS2339:** 95 tests - Property does not exist
- TS7006: 34 tests - Implicitly has 'any' type
- TS2769: 26 tests - No overload matches

### Missing Implementations (282 tests - TSC emits, we don't)
- **TS2792:** 15 tests - Cannot find module (NOT IMPLEMENTED)
- TS2538: 9 tests - Cannot index with type  
- TS2323: 9 tests - Duplicate identifier
- TS2301: 8 tests - Export assignment cannot be used
- TS1191: 8 tests - Import assertions

### Close to Passing (244 tests - diff ≤ 2 codes)
Many tests missing only 1-2 specific error codes like TS2740, TS2552, TS2693.

## Critical Bug Discovered

### Symbol() Returns DecoratorMetadata

**Root Cause:** When `esnext.decorators` lib is loaded, interface merging between:
- `es2015.symbol.d.ts`: `interface SymbolConstructor { (x?: string): symbol }`
- `esnext.decorators.d.ts`: `interface SymbolConstructor { readonly metadata: unique symbol }`

Results in `Symbol('test')` returning `DecoratorMetadata` instead of `symbol`.

**Impact:** Affects 320+ tests (all the false positive TS2345/TS2322/TS2339 errors)

**Status:** Bug documented in `docs/bugs/symbol-decorator-metadata-bug.md`. Needs deep investigation of interface lowering and call signature resolution.

## Investigation Methodology

1. Ran conformance tests on slice 1 → 68.3% pass rate
2. Used `analyze` mode to categorize failures
3. Identified high-impact patterns (false positives > missing implementations)
4. Traced Symbol() type resolution through:
   - Binder (symbol merging) ✓
   - Checker (identifier resolution) ✓  
   - Solver (call resolution) ✓
   - **Bug in**: Interface lowering or CallableShape building ✗

## Next Steps

### High Priority
1. **Fix Symbol() bug** - Would improve ~320 tests (major impact)
   - Add tracing to `interface_type.rs:lower_interface_declarations`
   - Check CallableShape call signature return types
   - Create minimal unit test reproducing the bug

### Medium Priority  
2. **Implement TS2792** - Would fix 15 tests
   - Module resolution error code
   - Check existing module resolution logic

3. **Fix "close to passing" tests** - 244 tests needing 1-2 error codes
   - TS2740: Missing property errors
   - TS2552: Cannot find name (specific cases)
   - TS2693: Type used as value

### Low Priority
4. Implement other missing error codes (TS2538, TS2323, etc.)

## Recommendations

- Focus on Symbol() bug first - highest impact
- Run unit tests frequently (`cargo nextest run`)
- Commit and sync after each fix
- Consider using test-driven approach for new error codes

## Files of Interest

- `crates/tsz-checker/src/interface_type.rs` - Interface lowering
- `crates/tsz-binder/src/state.rs` - Symbol merging
- `crates/tsz-solver/src/operations.rs` - Call resolution
- `crates/tsz-checker/src/type_computation_complex.rs` - Type resolution
