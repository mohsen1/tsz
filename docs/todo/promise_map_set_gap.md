# Promise, Map, Set Implementation Gap

## Date: January 29, 2026

## Issue
TS2339 has 283x extra errors in 1000-test sample, largely due to missing support for Promise, Map, Set and other built-in types.

## Current Implementation

### ✅ Types WITH Hardcoded Properties
**File**: `src/solver/apparent.rs` (lines 13-80)
- String methods (toUpperCase, toLowerCase, includes, indexOf, etc.)
- String property (length)
- Number methods (toString, toFixed, etc.)
- Boolean methods (toString, valueOf)
- BigInt methods (toString, valueOf)
- Object methods (toString, valueOf, hasOwnProperty, etc.)
- Symbol methods (description, toString, valueOf)

**File**: `src/solver/operations.rs` (lines 3158-3427)
- Array methods (map, filter, reduce, push, pop, includes, etc.)
- Array property (length)
- Tuple methods (entries, keys, values, etc.)

### ❌ Types WITHOUT Hardcoded Properties

**Essential Missing Types**:
1. **Promise** - Core async primitive
   - Methods: resolve, reject, then, catch, finally, race, all, any
   - Should return: Promise<T> for chainable methods

2. **Map<K, V>** - Key-value store
   - Methods: get, set, has, delete, clear, forEach, entries, keys, values
   - Properties: size

3. **Set<T>** - Unique value store
   - Methods: add, has, delete, clear, forEach, entries, keys, values
   - Properties: size

4. **WeakMap<K, V>** - Garbage-collectible key-value store
   - Methods: get, set, has, delete

5. **WeakSet<T>** - Garbage-collectible value store
   - Methods: add, has, delete

6. **RegExp** - Regular expressions
   - Methods: test, exec, match, replace, search, split
   - Properties: source, flags, lastIndex, dotAll, global, ignoreCase, multiline, sticky

7. **Date** - Date/time
   - Methods: toString, valueOf, getTime, setTime, etc.

8. **Error** - Errors
   - Properties: message, stack, name, cause

9. **JSON** - JSON parsing
   - Methods: parse, stringify

10. **Math** - Math functions
    - Properties: E, PI, etc.
    - Methods: abs, ceil, floor, max, min, random, round, etc.

## Impact

### TS2339 Errors from Missing Types
```typescript
// Promise usage (causes TS2339)
Promise.resolve(42)
  .then(x => console.log(x))
  .catch(e => console.error(e));

// Map usage (causes TS2339)
const map = new Map<string, number>();
map.set("key", 42);
map.get("key");
map.has("key");

// Set usage (causes TS2339)
const set = new Set<number>();
set.add(42);
set.has(42);
```

All of these emit TS2339 because the properties/methods don't exist in our type system.

## Conformance Impact

**Current Count**: 283x TS2339 errors in 1000-test sample

**Estimated Breakdown**:
- Promise methods: ~100x errors
- Map methods: ~50x errors
- Set methods: ~30x errors
- RegExp methods: ~30x errors
- Date methods: ~20x errors
- Other built-ins: ~53x errors

## Solution Options

### Option 1: Add to Hardcoded Lists (Quick Fix)
**Pros**:
- Fast to implement (1-2 days)
- Immediate reduction in TS2339 errors
- Clear implementation pattern exists (see arrays)

**Cons**:
- Maintenance burden
- Need to track TypeScript changes
- Doesn't scale to all built-in types

**Estimated Effort**: 2-3 days
**Expected Impact**: Reduce TS2339 by 40-50%

### Option 2: Parse from lib.d.ts (Proper Solution)
**Pros**:
- Scalable
- Automatic updates with TypeScript
- Matches TypeScript exactly
- No maintenance burden

**Cons**:
- Complex implementation
- Requires lib file loading infrastructure
- May have circular dependency issues

**Estimated Effort**: 5-7 days
**Expected Impact**: Reduce TS2339 by 90-95%

### Option 3: Hybrid Approach
- Hardcode most common types (Promise, Map, Set)
- Parse lib.d.ts for everything else
- Balance quick wins with long-term maintainability

**Estimated Effort**: 3-4 days
**Expected Impact**: Reduce TS2339 by 70-80%

## Recommendation

Given the high impact (283x errors), start with **Option 1** (hardcoded lists):

1. **Phase 1**: Add Promise methods (highest priority, most used)
   - resolve, reject, then, catch, finally, race, all, any
   - ~100x error reduction

2. **Phase 2**: Add Map/Set methods (high priority)
   - get, set, has, delete, clear, forEach, entries, keys, values
   - size property
   - ~80x error reduction

3. **Phase 3**: Add other common types
   - RegExp, Date, JSON, Math
   - ~103x error reduction

Total: **~2-3 days** for **~280x error reduction**

## Files to Modify

### For Promise Support
Create `src/solver/promise.rs` or add to `src/solver/apparent.rs`:
```rust
const PROMISE_METHODS: &[(&str, &[&str])] = &[
    ("resolve", &["T"]),
    ("reject", &["T"]),
    ("then", &["T"]),
    ("catch", &["T"]),
    ("finally", &["T"]),
    ("race", &["T"]),
    ("all", &["T"]),
    ("any", &["T"]),
];
```

### For Map/Set Support
Create `src/solver/collections.rs` or add to `src/solver/operations.rs`:
```rust
pub fn resolve_map_property(...) -> PropertyAccessResult {
    match prop_name {
        "size" => /* ... */,
        "get" => /* generic handling */,
        "set" => /* generic handling */,
        // etc.
    }
}
```

## Priority

**HIGHEST** - This is the #1 extra error (283x) and affects fundamental TypeScript functionality

## Next Steps

1. Create implementation plan for Promise/Map/Set
2. Start with Promise (most impactful)
3. Test with conformance suite
4. Measure error reduction
5. Iterate and refine
