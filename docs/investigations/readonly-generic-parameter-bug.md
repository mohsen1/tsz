# Readonly<T> Generic Parameter Resolution Bug

## Issue Summary
When using `Readonly<P>` where `P` is a type parameter with a constraint, TSZ incorrectly resolves the type to `unknown` instead of a proper mapped type. This causes TS2339 false positives.

## Reproduction
```typescript
interface Props {
    onFoo?(value: string): boolean;
}

// ✓ Works: Direct type parameter
function test1<P extends Props>(props: P) {
    props.onFoo;  // OK
}

// ✗ Fails: Readonly-wrapped type parameter
function test2<P extends Props>(props: Readonly<P>) {
    props.onFoo;  // Error TS2339: Property 'onFoo' does not exist on type 'unknown'
}
```

## Key Findings

### 1. User-Defined Mapped Types Work
```typescript
// This works perfectly:
type MyReadonly<T> = { readonly [P in keyof T]: T[P] };
function test<P extends Props>(props: MyReadonly<P>) {
    props.onFoo;  // OK!
}
```

**Insight**: The mapped type infrastructure works correctly. The bug is specific to the name "Readonly" (and likely other built-in utility types).

### 2. Even Locally-Defined "Readonly" Fails
```typescript
// Defining our own Readonly in the same file:
type Readonly<T> = { readonly [P in keyof T]: T[P] };

function test<P extends Props>(props: Readonly<P>) {
    props.onFoo;  // Still fails!
}
```

**Insight**: The issue isn't just about lib.d.ts resolution - there's something special about the NAME "Readonly".

### 3. Potential Root Causes Investigated

#### A. `is_mapped_type_utility` Check (state_type_resolution.rs:374)
```rust
if self.is_mapped_type_utility(name) {
    // ...process type args...
    return TypeId::ANY;  // Fallback for missing global types
}
```
- When `Readonly` can't be resolved, returns `TypeId::ANY`
- But error message says "unknown", suggesting further transformation

#### B. Type Parameter Extraction Issue (state_type_resolution.rs:277-292)
```rust
// Only checks for INTERFACE type parameters, not TYPE ALIAS parameters!
if let Some(iface) = self.ctx.arena.get_interface(node)
    && let Some(ref tpl) = iface.type_parameters
{
    // Extract params...
    found = true;
}
```
- Type alias type parameters are not extracted
- Causes fallback to `resolve_lib_type_by_name` even for local type aliases

#### C. Symbol Resolution Priority (symbol_resolver.rs:427)
```rust
// Checks LIB symbols FIRST, before local symbols!
if !ignore_libs {
    for lib_ctx in &self.ctx.lib_contexts {
        if let Some(lib_sym_id) = lib_ctx.binder.file_locals.get(name) {
            // Returns lib symbol, preventing local symbols from shadowing
            return TypeSymbolResolution::Type(sym_id);
        }
    }
}
```
- Lib symbols shadow local symbols
- Local "Readonly" type alias can't override built-in Readonly

## Attempted Fixes (Reverted)

1. **Add type alias parameter extraction** - Partial success, but didn't fix the issue
2. **Check for local symbols before lib symbols** - Broke namespace handling tests
3. **Skip `resolve_lib_type_by_name` for local symbols** - Didn't resolve the underlying issue

## Next Steps

1. **Deeper Investigation Needed**: Use `tsz-tracing` skill to trace type resolution for `Readonly<P>` vs `MyReadonly<P>`
2. **Compare with TypeScript**: Check how tsc handles `Readonly` name resolution specifically
3. **Find where ANY→UNKNOWN conversion happens**: The code returns ANY but error shows UNKNOWN
4. **Test Impact**: This affects **148 TS2339 false positives** in slice 3 conformance tests

## Related Code Locations

- `crates/tsz-checker/src/state_type_resolution.rs`: Type reference resolution
  - Line 374: `handle_missing_global_type_with_args`
  - Lines 277-292: Type parameter extraction (missing type alias support)
  - Lines 297-299: Fallback `resolve_lib_type_by_name` call
- `crates/tsz-checker/src/symbol_resolver.rs`:
  - Line 427: Lib symbol priority over local symbols
- `crates/tsz-checker/src/type_checking.rs`:
  - Line 1776: `is_mapped_type_utility` list

## Test Impact
- Conformance slice 3: 1810/3144 passed (57.6%)
- TS2339 false positives: 150 (many related to this bug)
- Estimated potential improvement: 50-100+ tests if fixed

## Status
**Investigation in progress** - Root cause identified but fix requires more careful handling of symbol resolution priorities and type parameter extraction for type aliases.
