# Conditional Type Alias Lazy Resolution Bug

## Status
**Impact**: ~84 TS2322 false positives
**Difficulty**: High (requires solver architecture changes)
**Priority**: Medium (high impact but complex fix)

## Minimal Reproduction

```typescript
type Test = true extends true ? "yes" : "no";
const x: Test = "yes";  // TS2322: Type 'string' is not assignable to type 'Test'
```

Expected: No error (tsc accepts this)
Actual: TS2322 error

## Root Cause

1. **TypeLowering creates Lazy types for ALL type aliases**
   - Location: `crates/tsz-solver/src/lower.rs:2570`
   - All type alias references become `Lazy(def_id)`, regardless of whether they're generic

2. **Lazy types aren't resolved during assignability checks**
   - `resolve_lazy()` is never called during subtype/assignability checking
   - The conditional type IS evaluated correctly and cached in `symbol_types`
   - But the cached value isn't retrieved because Lazy resolution doesn't happen

3. **The attempted fix didn't work**
   - Tried: Return structural type directly for non-generic aliases in `type_reference_symbol_type()`
   - Problem: Type references go through TypeLowering, which bypasses checker's type resolution
   - TypeNode uses TypeLowering directly, which always creates Lazy types

## Investigation Notes

### Code Paths

1. Variable type annotation resolution:
   ```
   get_type_from_type_node()
   → TypeNodeChecker::check()
   → TypeNodeChecker::get_type_from_type_reference()
   → TypeLowering::lower_identifier_type()
   → Creates Lazy(def_id) [line 2570]
   ```

2. Type alias body evaluation (works correctly):
   ```
   compute_type_of_symbol()
   → get_type_from_type_node(type_alias.type_node)
   → Evaluates conditional: true extends true ? "yes" : "no" → "yes"
   → Caches in symbol_types
   ```

3. Assignability checking (fails to resolve):
   ```
   check_assignment()
   → is_assignable_to()
   → Lazy type is NOT resolved
   → resolve_lazy() is never called!
   ```

### Debug Findings

- Added `eprintln!` to `resolve_lazy()` - it's NEVER called during type checking
- The Lazy → structural type resolution doesn't happen in the subtype checker
- This is a solver architecture issue, not a checker issue

## Required Fix

The solver's subtype/assignability checker needs to resolve Lazy types before comparing them.

### Option 1: Resolve Lazy in subtype checker (recommended)
- Modify `crates/tsz-solver/src/compat.rs` or `subtype.rs`
- Before comparing types, resolve Lazy types via `TypeResolver::resolve_lazy()`
- Similar to how Lazy is already resolved in some places (see line 514 in compat.rs)

### Option 2: Don't create Lazy for non-generic type aliases
- Modify TypeLowering to distinguish generic vs non-generic aliases
- Requires passing additional context to lowering about whether alias has type params
- More invasive change

### Option 3: Eager evaluation during lowering
- When lowering a type alias reference, immediately resolve and return the body
- Cache the evaluation to avoid re-computing
- May break nominal typing for type aliases

## Recommendation

**Defer this fix** until solver architecture is more mature. This requires:
1. Understanding all code paths where Lazy types need resolution
2. Ensuring resolution doesn't break nominal typing or other semantics
3. Performance testing (eager vs lazy evaluation trade-offs)

**Short-term**: Focus on simpler bugs with higher ROI for conformance pass rate.

## Related Code

- `crates/tsz-solver/src/lower.rs:2554-2572` - Creates Lazy types
- `crates/tsz-checker/src/context.rs:1995-2030` - `resolve_lazy()` implementation
- `crates/tsz-solver/src/compat.rs:512-517` - Example of Lazy resolution
- `crates/tsz-checker/src/state_type_resolution.rs:814-868` - Type alias handling
