# Conformance Tests 100-199 - Session 2

**Date**: 2026-02-12
**Starting Point**: 80% pass rate (80/100 tests)
**Final Result**: 82% pass rate (82/100 tests) - **+2 tests**

## Summary

Implemented TS2458 error code to detect duplicate AMD module name assignments, improving the pass rate by 2 percentage points.

## Work Completed

### Implemented: TS2458 - Duplicate AMD Module Names (+2 tests)

**Error Code**: TS2458 - "An AMD module cannot have multiple name assignments"

**Problem**: TypeScript validates that AMD modules can only have one `///<amd-module name='...'/>` directive. Files with multiple directives should emit TS2458 at the second (and subsequent) directive locations.

**Implementation**:

1. **Added extraction function** in `triple_slash_validator.rs`:
   - `extract_amd_module_names(source: &str) -> Vec<(String, usize)>`
   - Parses source text for `///<amd-module name='...'/>` directives
   - Returns vector of (module_name, line_number) tuples
   - Supports both single and double quotes

2. **Added validation check** in `state_checking.rs`:
   - `check_amd_module_names(&mut self, source_text: &str)`
   - Called during post-checks phase after triple-slash reference validation
   - Emits TS2458 at the position of the second and subsequent directives
   - Only errors if 2+ directives found (single directive is valid)

3. **Added unit tests**:
   - `test_extract_amd_module_names()` - tests extraction of multiple directives
   - `test_extract_amd_module_names_no_duplicates()` - tests single directive case

**Files Modified**:
- `crates/tsz-checker/src/triple_slash_validator.rs` (+54 lines)
- `crates/tsz-checker/src/state_checking.rs` (+51 lines)

**Tests Fixed**:
- `amdModuleName2.ts` - has two `///<amd-module name='...'/>` directives
- Likely one more test (pass rate went from 80% to 82%)

**Commit**: `4416431f8` - "feat: implement TS2458 - detect duplicate AMD module name assignments"

## Attempted: TS2449 - Ambient Class Declaration Order (not completed)

**Error Code**: TS2449 - "Class used before its declaration"

**Problem**: We incorrectly emit TS2449 for ambient (declare) classes even when they're used before declaration. TypeScript allows this because ambient declarations don't have ordering constraints - they're just type declarations, not runtime code.

**Approach Attempted**:
- Added check in `check_heritage_class_before_declaration()` to skip validation if declaration has `declare` modifier
- Used `self.ctx.has_modifier(&class_decl.modifiers, SyntaxKind::DeclareKeyword as u16)`
- Worked for same-file cases but not for cross-file references

**Issue Discovered**:
- Cross-file symbol resolution needs investigation
- Test `ambientClassDeclarationWithExtends.ts` has ambient class E in file1 but referenced from file2
- The `decl_file_idx != u32::MAX` check doesn't correctly identify cross-file symbols
- Needs deeper understanding of how symbols track their source file

**Status**: Reverted changes, requires more investigation

## Current State

**Pass Rate**: 82/100 (82.0%)

**Top Error Code Mismatches**:
- TS2345: missing=1, extra=2 (Argument type)
- TS2322: missing=1, extra=2 (Type assignment)
- TS2339: missing=0, extra=3 (Property doesn't exist)
- TS2351: missing=0, extra=2 (Not constructable)
- TS2304: missing=1, extra=1 (Cannot find name)
- TS2449: missing=0, extra=1 (Class used before declaration) - still needs fix

## Remaining High-Priority Issues

### False Positives (7 tests - we emit errors when TypeScript doesn't)

1. **TS2351 (2 tests)** - "This expression is not constructable"
   - `ambientExternalModuleWithoutInternalImportDeclaration.ts`
   - `ambientExternalModuleWithInternalImportDeclaration.ts`
   - Issue: Export assignments + import equals for ambient modules
   - Complexity: High - requires understanding export assignment type resolution

2. **TS2449 (1 test)** - "Class used before declaration"
   - `ambientClassDeclarationWithExtends.ts`
   - Issue: Cross-file ambient class references
   - Complexity: Medium - needs symbol file tracking investigation

3. **TS2322 (2 tests)** - Type assignment errors
4. **TS2345 (2 tests)** - Argument type errors
5. **TS2339/TS2488 (2 tests)** - Property/type errors

### Missing Error Codes (4 tests)

- TS2305, TS2714, TS8009, TS8010, TS2551, TS2580, TS1210, TS7039 - various not implemented

### Close Tests (6 tests - differ by 1-2 error codes)

Best candidates for next quick wins if error codes can be addressed.

## Key Learnings

### Triple-Slash Directive Validation

The checker has a pattern for validating triple-slash directives:
1. Extract directives from source text with line numbers
2. Validate during post-checks phase
3. Emit errors at the directive position using line number calculation
4. Follows same pattern as reference path validation (TS6053)

This pattern can be extended for other directive validations like:
- `///<amd-dependency .../>`
- Potentially other pragma-style directives

### Ambient Declaration Handling

Ambient declarations (`declare class`, `declare enum`, etc.) have special semantics:
- No ordering constraints within a file
- Can be split across multiple declarations (declaration merging)
- Don't execute at runtime, only provide type information
- Cross-file references need careful handling

The `DeclareKeyword` modifier can be checked with:
```rust
self.ctx.has_modifier(&decl.modifiers, SyntaxKind::DeclareKeyword as u16)
```

### Symbol File Tracking

The binder tracks symbol source files with `decl_file_idx`:
- Unclear whether `u32::MAX` means "same file" or "different file"
- Cross-file symbol resolution needs more investigation
- May need to compare source file indices directly rather than using magic values

## Metrics

- **Tests Fixed**: 2
- **Commits**: 1
- **Pass Rate Improvement**: +2.0 percentage points (80% → 82%)
- **Code Added**: ~105 lines (including tests)
- **Time**: ~2 hours

## Next Steps

### Immediate Opportunities

1. **TS2351 Investigation** (2 tests)
   - Debug why export assignments in ambient modules aren't recognized as constructable
   - May be related to how we resolve `export = ClassName` types

2. **TS2449 Cross-File Fix** (1 test + related)
   - Investigate symbol file tracking in binder
   - Determine correct way to identify cross-file references
   - May need to store source file index with symbol and compare directly

3. **Other False Positives** (4 tests)
   - Focus on tests where we emit errors TypeScript doesn't
   - These are often quicker to fix than implementing new error codes

### Long-term

- Implement missing error codes for "all-missing" category tests
- Address "wrong-code" tests where both sides have errors but codes differ

## Test Commands

```bash
# Run conformance tests for this slice
./scripts/conformance.sh run --max=100 --offset=100

# Analyze failures by category
./scripts/conformance.sh analyze --max=100 --offset=100 --category close
./scripts/conformance.sh analyze --max=100 --offset=100 --category false-positive

# Run unit tests
cargo nextest run -p tsz-checker test_extract
```
