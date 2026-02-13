# Array Augmentation Bug - 2026-02-13

## Problem

Top-level interface declarations that augment built-in types (Array, Promise, Map, etc.) are not properly merged in script files.

## Reproduction

```typescript
// arrayAugment.ts (script file - no imports/exports)
interface Array<T> {
    split: (parts: number) => T[][];
}

var x = [''];
var y = x.split(4);  // ERROR in TSZ: Property 'split' does not exist
```

**TSZ**: Error TS2339 - property not found
**TSC**: No error - interface properly merged

## Root Cause

**Location**: `crates/tsz-binder/src/state_binding.rs:485-490`

Interface declarations are only added to `global_augmentations` when inside a `declare global` block:

```rust
if self.in_global_augmentation {
    self.global_augmentations
        .entry(name.to_string())
        .or_default()
        .push(crate::state::GlobalAugmentation::new(idx));
}
```

However, TypeScript also allows global augmentation through top-level declarations in script files.

## TypeScript Behavior

In a **script file** (no imports/exports):
- Top-level declarations are global by default
- `interface Array<T> { ... }` merges with built-in Array
- No `declare global` wrapper needed

In a **module file** (has imports/exports):
- Top-level declarations are module-scoped
- Must use `declare global { interface Array<T> { ... } }`
- This currently works in TSZ

## Current Status

The augmentation mechanism DOES work when properly triggered:

```typescript
export {};  // Make it a module

declare global {
    interface Array<T> {
        myMethod: (x: number) => T[];
    }
}

const arr = [1, 2, 3];
arr.myMethod(5);  // ✅ WORKS in TSZ
```

## Solution Approach

When binding interface declarations, check if:
1. We're in global scope (not inside a module)
2. The interface name matches a known built-in type (Array, Promise, etc.)

If both conditions are true, add to `global_augmentations` even without `declare global`.

### Implementation Sketch

```rust
// In bind_interface_declaration
if self.current_scope_is_global() && is_built_in_global_type(name) {
    self.global_augmentations
        .entry(name.to_string())
        .or_default()
        .push(crate::state::GlobalAugmentation::new(idx));
}
```

Built-in types to check:
- Array
- Promise
- Map
- Set
- WeakMap
- WeakSet
- ReadonlyArray
- (possibly others from lib.d.ts)

## Impact

- Affects conformance tests using built-in type augmentation
- `arrayAugment.ts` fails due to this
- Likely affects other tests with similar patterns

## Notes

- This is orthogonal to the higher-order inference and mapped type issues
- Fix is localized to the binder
- Must distinguish script files from module files
- Must not break existing `declare global` behavior
