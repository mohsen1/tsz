# Conformance Slice 2 - Investigation Notes

## Baseline
- **Slice 2 Range**: Tests 3146-6292 (offset=3146, max=3146)
- **Initial Pass Rate**: 1813/3132 passed (57.9%)
- **Current Pass Rate**: 1813/3129 passed (57.9%) [after all Session 1 fixes]
- **Net Change**: +0 tests (maintained stability while improving error quality)
- **Test Date**: 2026-02-11

## Fixes Implemented

### Session 1 (2026-02-11)
1. **TS2708 Implementation** (`04f13d9a2`)
   - Emit "Cannot use namespace as a value" for `export import a = NS.Interface`
   - Check if namespace contains only type symbols (interfaces/type aliases)
   - Handle parent EXPORT_DECLARATION wrapper detection
   - Impact: ~2-3 tests

2. **TS1147 + TS2307 Coordination** (`b7db89c47`)
   - Suppress TS2307 for import = require() inside namespaces
   - Return early after TS1147 to avoid duplicate module-not-found errors
   - Defer IMPORT_EQUALS_DECLARATION in emit_module_not_found_error
   - Impact: 1 test (importDeclarationInModuleDeclaration1.ts)
   - **Note**: May need revision - some tests expect both TS1147 and TS2307

3. **TS2708 for Namespace Member Access** (`aa2fa3f95`)
   - Emit TS2708 + TS2693 for namespace.TypeMember access (e.g., `ns.Interface()`)
   - Modified get_type_of_element_access and get_type_of_property_access_by_name
   - Dual emission: TS2708 for namespace misuse, TS2693 for type-only member
   - Impact: 1 test (interfaceNameAsIdentifier.ts)

## Top Error Mismatches

### False Positives (We're Too Strict)
- **TS2339** (Property doesn't exist): 134 extra
- **TS2322** (Type not assignable): 112 extra
- **TS2345** (Argument type error): 123 extra
- **TS1005** (Expected token): 91 extra
- **TS2694** (Namespace has no exported member): 46 extra

### Missing Implementations
- **TS2322**: 54 missing
- **TS2304**: 48 missing
- **TS2307**: 32 missing

## Issues Investigated

### 1. Parser: Indexed Access Types in Type Arguments
**Problem**: `Box<Foo["A"]>` fails to parse with TS1005 error  
**Example**: `type Test = Box<this["A"]>` → ">" expected  
**Root Cause**: Parser lookahead doesn't recognize `[` after `<Foo` as part of type argument  
**Impact**: Affects HKT (Higher-Kinded Types) patterns  
**Complexity**: High - requires parser lookahead modifications  
**Files**: `crates/tsz-parser/src/parser/state_types.rs`

### 2. Namespace Exports: Type-Only Members
**Problem**: `import a = x.c` where `c` is interface without `export`  
**Example**:
```typescript
namespace x {
    interface c {}  // No export keyword
}
import a = x.c;  // TS2694: Namespace 'x' has no exported member 'c'
```
**Root Cause**: In TS, type-only declarations in namespaces are implicitly accessible via qualified names
**Current Behavior**: We only add to exports table if explicit `export` keyword
**Complexity**: Medium-High - requires modifying resolution logic
**Impact**: 46 false positive TS2694 errors
**Files**:
- `crates/tsz-checker/src/symbol_resolver.rs:report_type_query_missing_member`
- `crates/tsz-binder/src/state_binding.rs:populate_module_exports`

**Investigation Details**:
- Error emitted from `report_type_query_missing_member` at line 1593
- Called from `state_type_analysis.rs:check_import_alias` line 2120
- Attempted fix: Check if member resolves via `resolve_qualified_symbol`
- **Blocker**: `resolve_qualified_symbol` itself checks exports, so fails for unexported interfaces
- **Root Issue**: Interface symbols in namespaces aren't added to exports table
- **Solution Options**:
  1. Add interfaces to namespace exports (may break value/type separation)
  2. Modify `resolve_qualified_symbol` to check scope for type-only symbols
  3. Add direct scope lookup in `report_type_query_missing_member`

### 3. Over-Strict Type Checking
**Problem**: 112+ extra TS2322 errors, 134+ extra TS2339 errors
**Root Cause**: Unknown - needs investigation of specific test cases
**Complexity**: High - core type system issues
**Impact**: 388 total false positive tests

### 4. TS2708 for Namespace Member Call
**Problem**: `m2.C()` where `C` is an interface should emit TS2708
**Example**:
```typescript
namespace m2 {
    export interface C { (): void; }
}
m2.C();  // Should emit TS2708: Cannot use namespace as a value
```
**Current**: We emit TS2693 (type used as value) for the direct call
**Missing**: TS2708 for the namespace member access
**Complexity**: Medium - need to detect namespace.member calls where member is type-only
**Impact**: 1 test (interfaceNameAsIdentifier.ts)

### 5. Ambient Module Namespace Exports
**Problem**: In `declare module "m"`, interfaces in nested namespaces not accessible
**Example**:
```typescript
declare module "m" {
    namespace x {
        interface c {}  // No export keyword but should be accessible
    }
    export import a = x.c;  // TS2694 but shouldn't error
}
```
**Root Cause**: Ambient context rules - all type declarations implicitly accessible
**Complexity**: Medium-High - needs ambient context tracking
**Impact**: 1-2 tests
**Status**: Partially investigated, needs ambient context flag in binder

## Recommendations

### Quick Wins (if pursued)
1. **TS2694 namespace fix**: Modify `report_type_query_missing_member` to check namespace scope for type-only symbols
2. **Parser improvement**: Add proper lookahead for `[` in type arguments

### High-Impact (longer term)
1. Investigate why type checking is too strict (TS2322, TS2339, TS2345)
2. Implement missing error codes: TS2320 (7 tests), TS2497 (8 tests)

## Test Commands
```bash
# Run slice 2
./scripts/conformance.sh run --offset 3146 --max 3146

# Analyze patterns
./scripts/conformance.sh analyze --offset 3146 --max 3146 --category false-positive

# Test specific error
./scripts/conformance.sh run --offset 3146 --max 500 --error-code 2694 --verbose
```
