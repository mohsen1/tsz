# Slice 2 Conformance Investigation

**Slice**: Tests 1424-2848 (second quarter of 5695 total)
**Current Pass Rate**: 69.5% (989/1422 tests passing)
**Failing Tests**: 433

## Top Issues by Impact

### False Positives (we emit errors incorrectly)
1. **TS2339** - 47 false positives: "Property does not exist on type"
2. **TS2322** - 43 false positives: "Type is not assignable"
3. **TS2345** - 29 false positives: "Argument is not assignable"
4. **TS1005** - 24 false positives: "';' expected" (parser issues, mostly decorators)

### Missing Implementations (we don't emit when we should)
1. **TS2304** - 25 missing: "Cannot find name"
2. **TS2322** - 20 missing: "Type is not assignable"
3. **TS2339** - 17 missing: "Property does not exist on type"
4. **TS2300** - 15 missing: "Duplicate identifier"

### Not Implemented (never emitted)
1. **TS2323** - 9 tests: "Cannot redeclare exported variable"
2. **TS2792** - 8 tests: Module resolution error (we emit TS2307 instead)
3. **TS2741** - 8 tests: "Property is missing in type" (elaboration)
4. **TS1191** - 8 tests: Import/export statement errors

## Investigation Results

### typeof with Rest Parameters (TS2304)
**Tests affected**: declFileForInterfaceWithRestParams.ts and similar

**Issue**: When checking `typeof x` where `x` is a rest parameter like `foo(...x): typeof x`, we incorrectly emit TS2304 "Cannot find name 'x'".

**Root Cause**: Parameters in type signatures aren't visible to typeof expressions during type resolution. The value resolver (`resolve_value_symbol_for_lowering`) only checks `file_locals`, missing signature-local parameters.

**Attempted Fix**: Added `parameter_scope` to CheckerContext to track parameters, but the fix was incomplete. The parameter scope needs to be populated during type environment building, not just during checking phase. Reverted in commit 7ffde4189 (then reset).

**Code Locations**:
- `crates/tsz-checker/src/symbol_resolver.rs:1015` - `resolve_value_symbol_for_lowering()`
- `crates/tsz-checker/src/state_type_analysis.rs:597` - Called from `get_type_from_type_query()`
- `crates/tsz-checker/src/interface_type.rs:91` - Where interface signatures are processed

### TS2792 vs TS2307
**Tests affected**: 8 tests with monorepo/symlink setups

**Issue**: We emit TS2307 "Cannot find module" but TSC emits TS2792 for specific module resolution scenarios.

**Status**: TS2792 is not implemented in our codebase at all. Would require:
1. Adding TS2792 to diagnostics
2. Identifying when to emit TS2792 vs TS2307
3. Tests involve complex edge cases (symlinks, monorepos)

**Code Location**: `crates/tsz-checker/src/import_checker.rs`

### TS1005 Decorator False Positives
**Tests affected**: 24 tests, mostly decorator-related

**Example**: decoratorReferences.ts expects no errors but we emit TS1005 + TS1068

**Issue**: Parser is stricter than TSC's for decorator syntax. Test case:
```typescript
@y(null as T)
method(@y x, y) {}  // Parser expects '{' before seeing @y
```

**Status**: Parser-level issue, would require parser changes to match TSC's flexibility.

### TS2300 Duplicate Identifier (Partial)
**Tests affected**: 9 tests

**Status**: We DO emit TS2300 for some cases (e.g., duplicate class members) but miss:
1. Numeric literals with different spellings (`0b11` vs `3`)
2. Interface members duplicated across files
3. Certain export/import merge scenarios

**Code Location**: Likely needs work in binder or checker for cross-file duplicate detection.

## Recommendations for Next Session

1. **Quick Wins**: Focus on suppressing specific false positives rather than implementing missing features. For example, identify specific patterns causing TS2339/TS2322 false positives and add targeted checks.

2. **typeof with Rest Parameters**: This affects multiple tests and has clear value. The fix requires:
   - Understanding exact code path during interface type building
   - Properly scoping parameters during type annotation resolution
   - Using tracing to debug when parameter_scope should be populated

3. **Avoid**: Complex monorepo/symlink issues (TS2792), decorator parser issues, cross-file duplicate detection until simpler issues are resolved.

4. **Testing Strategy**: Pick one specific failing test, understand it completely, fix it, verify with `cargo nextest run`, then commit before moving to next issue.

## Useful Commands

```bash
# Analyze my slice
./scripts/conformance.sh analyze --offset 1424 --max 1424 --top 20

# Run my slice
./scripts/conformance.sh run --offset 1424 --max 1424

# Test specific error code
./scripts/conformance.sh run --offset 1424 --max 1424 --error-code 2304 --verbose

# Run unit tests
cargo nextest run
```
