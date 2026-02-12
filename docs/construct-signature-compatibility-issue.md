# Construct Signature Compatibility Issue (TS2403 False Positives)

## Problem
We're emitting false positive TS2403 errors when variable declarations use interfaces with construct signatures.

## Affected Tests
- ~10-15 false positive TS2403 tests
- Example: `TwoInternalModulesThatMergeEachWithExportedInterfacesOfTheSameName.ts`

## Root Cause
When checking if types are identical for variable redeclaration:
```typescript
interface Line {
    new (start: Point, end: Point);  // No return type specified
}

interface Line {
    start: Point;
    end: Point;
}

// Should be compatible, but we emit TS2403
var l: { start: Point; end: Point; new (s: Point, e: Point); }
var l: Line;
```

The merged `Line` interface has both construct signature and properties. When checking if:
- Type 1: Object literal with construct signature
- Type 2: Interface reference to `Line`

These should be bidirectionally compatible, but our check is failing.

## Investigation Findings

### Simple Cases Work
```typescript
// ✓ Works
interface Foo { new (): void; }
var x: { new (): void };
var x: Foo;

// ✓ Works
interface Point { x: number; y: number; }
var p: { x: number; y: number };
var p: Point;
```

### Combination Fails
```typescript
// ✗ Fails with TS2403
interface Line {
    new (s: number, e: number);
    start: number;
    end: number;
}
var l: { new (s: number, e: number): unknown; start: number; end: number };
var l: Line;
```

## Observations

1. **Return Type Shows as `unknown`**: Error message shows:
   ```
   Variable 'l' must be of type '{ new (s: number, e: number): unknown; ... }',
   but here has type 'Line'.
   ```

2. **Construct Signature Default Return Type**: When a construct signature doesn't specify a return type, TypeScript defaults it to the containing type (self-reference). We may be handling this incorrectly.

3. **Works in tsc**: TypeScript compiler accepts these as compatible.

## Potential Root Causes

### 1. Construct Signature Return Type Handling
When merging interfaces, construct signatures without explicit return types might be:
- Getting `unknown` instead of self-reference
- Not being resolved correctly during type comparison

### 2. Callable vs Object Comparison
The interface becomes a Callable type (has construct signatures), while the literal is an Object type. The bidirectional subtype check in `are_types_identical_for_redeclaration` might not properly handle:
- Callable → Object direction
- Object → Callable direction

### 3. Type Resolution Before Comparison
The `ensure_refs_resolved` calls in `are_var_decl_types_compatible` might not be fully resolving the interface reference before comparison.

## Related Code Locations
- `crates/tsz-solver/src/compat.rs`: `are_types_identical_for_redeclaration`
- `crates/tsz-checker/src/assignability_checker.rs`: `are_var_decl_types_compatible`
- `crates/tsz-checker/src/interface_type.rs`: Interface merging with construct signatures

## Impact
Fixing this would resolve ~10-15 false positive TS2403 tests involving interface merging with construct signatures.

## Next Steps for Future Fix
1. Investigate construct signature return type defaults during interface creation
2. Check if Callable types are properly compared with Object types in subtype checking
3. Add debug tracing to see what types are being compared
4. Consider if construct signature self-reference needs special handling
5. Write unit tests for construct signature bidirectional compatibility

## Workaround
None - this affects correct TypeScript code that should compile.
