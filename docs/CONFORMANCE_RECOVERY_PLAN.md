# Conformance Recovery Plan

## Latest Results (2,000 tests, 2026-01-20)
- Pass rate: **29.4%** (588 / 2,000) ✅ up from 24.7%
- Speed: **73 tests/sec**
- Crashes: 3 ✅ down from 865
- OOM: 0 ✅ down from 37
- Timeouts: 2 ✅ down from 57

## Completed Actions

### ✅ 1. Restored full directive→CompilerOptions parity in workers
Added comprehensive support for all major TypeScript directives:
- target, module, moduleResolution, jsx
- strict mode flags (noImplicitAny, strictNullChecks, etc.)
- lib, noLib, skipLibCheck
- allowJs, checkJs
- experimentalDecorators, emitDecoratorMetadata
- useDefineForClassFields, esModuleInterop
- isolatedModules, resolveJsonModule
- Source maps, declaration options
- Output options, type checking flags

### ✅ 2. Fixed script kind detection
- Proper ScriptKind for .js/.jsx/.tsx/.json files
- Fixed crashes from treating JS files as TS

### ✅ 3. Improved lib file loading
- Both TSC and WASM now load lib.d.ts when not using @noLib
- Default lib integration working

## Remaining Issues

### Top Missing Errors (we should emit but don't)
| Error | Count | Description |
|-------|-------|-------------|
| TS2318 | 1,974x | Cannot find global type (expected with @noLib tests) |
| TS2583 | 536x | Cannot find name (need ES2015+ lib) |
| TS2711 | 232x | Cannot assign to 'exports' (CommonJS) |
| TS2304 | 228x | Cannot find name |
| TS2792 | 226x | Cannot find module |

### Top Extra Errors (we emit but shouldn't)
| Error | Count | Description |
|-------|-------|-------------|
| TS2571 | 870x | Object is of type 'unknown' |
| TS2300 | 646x | Duplicate identifier |
| TS2322 | 282x | Type not assignable |
| TS2304 | 278x | Cannot find name |
| TS1005 | 219x | Expected token (parser) |

## Next Steps (priority order)

### 1. Fix TS2571 "Object is of type 'unknown'" (870x extra)
- Root cause: Type inference returning `unknown` instead of proper types
- Check `get_type_of_node` for cases where we return `TypeId::UNKNOWN` incorrectly
- May be related to unresolved generics or missing type narrowing

### 2. Fix TS2300 "Duplicate identifier" (646x extra)
- Root cause: Module/global symbol merging issue
- `merge_bind_results` injects file_locals into globals for external modules
- External modules should NOT have their exports in global scope

### 3. Add TS2318 "Cannot find global type" check
- TSC emits this when noLib is set and required types (Array, Boolean, etc.) are missing
- We need to add this check when `noLib: true` to match TSC behavior
- ~1,974 missing errors come from tests that intentionally use @noLib

### 4. Fix remaining crashes (3)
- `compiler/checkJsFiles6.ts` - undefined flags
- `compiler/commonJsExportTypeDeclarationError.ts` - undefined flags
- `conformance/decorators/invalid/decoratorOnAwait.ts` - Debug failure

## Success Criteria
- Pass rate: 50%+ (currently 29.4%)
- Crashes: 0 (currently 3)
- TS2571 extra: <100x (currently 870x)
- TS2300 extra: <100x (currently 646x)
