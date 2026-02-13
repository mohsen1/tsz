# Session: TS2439 Fix and Type Resolution Investigation

**Date**: 2026-02-13  
**Starting Pass Rate**: 83/100 (83.0%)  
**Ending Pass Rate**: 84/100 (84.0%)  
**Net Gain**: +1 test (+1 percentage point)

## Accomplishments

### ✅ Implemented TS2439 - Relative Imports in Ambient Modules

**File**: `crates/tsz-checker/src/import_checker.rs`

**Change**: Added validation in `check_import_equals_declaration()` to detect when import declarations in ambient modules use relative module paths.

```rust
// TS2439: Import or export declaration in an ambient module declaration
// cannot reference module through relative module name
let is_relative_path = imported_module.starts_with("./") || imported_module.starts_with("../");
if is_relative_path {
    self.error_at_node(
        import.module_specifier,
        diagnostic_messages::IMPORT_OR_EXPORT_DECLARATION_IN_AN_AMBIENT_MODULE_DECLARATION_CANNOT_REFERENCE_M,
        diagnostic_codes::IMPORT_OR_EXPORT_DECLARATION_IN_AN_AMBIENT_MODULE_DECLARATION_CANNOT_REFERENCE_M,
    );
}
```

**Test Fixed**: `ambientExternalModuleWithRelativeExternalImportDeclaration.ts`

**Example**:
```typescript
declare module "OuterModule" {
    import m2 = require("./SubModule");  // Now correctly emits TS2439
}
```

## Investigation: Type Resolution Issues

### Problem Discovered

Multiple false-positive tests are failing due to a systematic type resolution issue where imported types resolve to incorrect global types:

1. **Constructor → AbortController**: In `anonClassDeclarationEmitIsAnon.ts`, the imported `Constructor<T>` type alias resolves to `AbortController<{}>` instead
2. **Iterator → AbstractRange**: In `argumentsObjectIterator02_ES6.ts`, the iterator type resolves to `AbstractRange<any>` instead
3. **Const Enum Members**: In `amdModuleConstEnumUsage.ts`, `CharCode.A` emits TS2339 as if the property doesn't exist

### Evidence

Running test file `tmp/test-anon-class-main.ts`:
```
error TS2345: Argument of type '{ new (): User }' is not assignable to parameter of type 'AbortController<{}>'.
```

The `Timestamped` function's type parameter constraint `TBase extends Constructor` is resolving `Constructor` incorrectly.

### Root Cause Hypothesis

The issue appears to be in symbol/import resolution:
- Import bindings aren't being resolved correctly
- Type aliases from imports are resolving to unrelated global types
- This could be related to lib.d.ts types leaking, or import resolution falling back to globals

### Affected Tests (8 false positives)

All showing symptoms of type resolution issues:
- `anonClassDeclarationEmitIsAnon.ts` - TS2345  
- `amdModuleConstEnumUsage.ts` - TS2339
- `amdLikeInputDeclarationEmit.ts` - TS2339
- `argumentsObjectIterator02_ES6.ts` - TS2488  
- `amdDeclarationEmitNoExtraDeclare.ts` - TS2322, TS2345
- `ambientClassDeclarationWithExtends.ts` - TS2322, TS2449
- `ambientExternalModuleWithInternalImportDeclaration.ts` - TS2708
- `ambientExternalModuleWithoutInternalImportDeclaration.ts` - TS2351

## Recommendations for Next Session

### High Priority: Fix Type Resolution

The type resolution issue is blocking 8 false-positive tests (50% of remaining failures). Fixing this one root cause would likely jump us from 84% to 90%+ pass rate.

**Investigation Steps**:
1. Add tracing to import resolution to see where `Constructor` resolves to `AbortController`
2. Check if `resolve_identifier_symbol` is correctly looking up imported symbols
3. Verify that type aliases from imports are being stored correctly in the type environment
4. Check if there's an issue with how constraints are stored/retrieved for type parameters

**Key Files to Investigate**:
- `crates/tsz-checker/src/symbol_resolver.rs` - Symbol resolution
- `crates/tsz-checker/src/type_checking_queries.rs:resolve_identifier_symbol()`
- `crates/tsz-checker/src/state_type_resolution.rs` - Type alias lowering
- `crates/tsz-checker/src/type_parameter.rs` - Type parameter constraints

### Alternative: Wrong-Code Tests

If type resolution is too complex, focus on wrong-code tests (4 remaining) by implementing missing error codes:
- TS2305, TS2714, TS2551 - each would fix 1 test

## Testing Commands

```bash
# Build
cargo build --profile dist-fast -p tsz-cli

# Run conformance tests
./scripts/conformance.sh run --max=100 --offset=100

# Analyze failures
./scripts/conformance.sh analyze --max=100 --offset=100 --category false-positive

# Test specific file
./.target/dist-fast/tsz tmp/test-anon-class-main.ts --declaration

# Unit tests
cargo nextest run -p tsz-checker
```

## Status

- **Current**: 84/100 (84.0%)
- **Target**: 85/100 (85.0%) - need +1 test
- **Blocker**: Type resolution issue affecting 8 tests
- **Commit**: TS2439 fix committed and pushed to main
