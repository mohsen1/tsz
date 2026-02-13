# Circular Reference Detection Implementation Plan

**Date**: 2026-02-13
**Error Code**: TS2456 "Type alias circularly references itself"
**Status**: Design Complete - Ready to Implement

## Problem

TypeScript detects when type aliases form circular references without structural wrapping:

**Invalid** (should error TS2456):
```typescript
type A = B;
type B = A;  // Circular

type C = D;
type D = E;
type E = C;  // 3-way circular

type F = F;  // Self-referential
```

**Valid** (should NOT error):
```typescript
type List = { value: number; next: List | null };  // Wrapped in object
type Tree = { children: Tree[] };  // Wrapped in array
```

**Current tsz behavior**: No errors emitted for circular references

## Root Cause

The circular reference detection exists in `get_type_of_symbol` (state_type_analysis.rs:1028-1060) but:
1. Only returns a Lazy placeholder
2. Doesn't emit any error
3. Doesn't distinguish between valid recursive types and invalid circular references

## Detection Algorithm

The key distinction is **direct vs structural reference**:

- **Direct**: `type A = B` - the RHS is just a type reference (TypeReference node)
- **Structural**: `type A = { x: B }` - the RHS contains the reference within a structure

### Detection Steps

1. When lowering a type alias in `compute_type_of_symbol` (around line 2080):
   ```rust
   if flags & symbol_flags::TYPE_ALIAS != 0 {
       let alias_type = self.get_type_from_type_node(type_alias.type_node);
       // Check if alias_type contains direct circular reference
   }
   ```

2. Check if the resolved type is a Lazy(DefId) that matches the current type alias being resolved

3. If yes, and the type_node is a simple TypeReference (not wrapped in object/array/union), emit TS2456

### Implementation Approach

#### Option A: Check During Type Lowering (Preferred)

Add check in `compute_type_of_symbol` after `get_type_from_type_node`:

```rust
if flags & symbol_flags::TYPE_ALIAS != 0 {
    let alias_type = self.get_type_from_type_node(type_alias.type_node);

    // Check for invalid circular reference
    if self.is_direct_circular_reference(sym_id, alias_type, type_alias.type_node) {
        // Emit TS2456 error
        let name = self.get_symbol_name(sym_id);
        self.error_at_node(
            decl_idx,
            &format!("Type alias '{}' circularly references itself.", name),
            2456,
        );
        return (TypeId::ERROR, params);
    }

    // Rest of type alias handling...
}
```

Helper function:
```rust
fn is_direct_circular_reference(
    &self,
    sym_id: SymbolId,
    resolved_type: TypeId,
    type_node: NodeIndex,
) -> bool {
    // 1. Check if resolved_type is Lazy(DefId) pointing to sym_id
    if let Some(def_id) = get_lazy_def_id(self.ctx.types, resolved_type) {
        if self.ctx.def_to_symbol.get(&def_id) == Some(&sym_id) {
            // 2. Check if type_node is a simple type reference (not wrapped)
            return self.is_simple_type_reference(type_node);
        }
    }

    // Also check if resolved_type is a union/intersection where ANY member
    // is a direct circular reference
    match self.ctx.types.lookup(resolved_type) {
        Some(TypeKey::Union(members)) | Some(TypeKey::Intersection(members)) => {
            let list = self.ctx.types.type_list(members);
            for &member in list {
                if self.is_direct_circular_reference(sym_id, member, type_node) {
                    return true;
                }
            }
        }
        _ => {}
    }

    false
}

fn is_simple_type_reference(&self, type_node: NodeIndex) -> bool {
    let Some(node) = self.ctx.arena.get(type_node) else {
        return false;
    };

    // Type reference without structural wrapping
    node.kind == syntax_kind_ext::TYPE_REFERENCE
        || node.kind == syntax_kind_ext::IDENTIFIER
}
```

#### Option B: Check During get_type_of_symbol (Alternative)

Add check when circular reference is detected (line 1028):

```rust
if self.ctx.symbol_resolution_set.contains(&sym_id) {
    let symbol = self.ctx.binder.get_symbol(sym_id);
    if let Some(symbol) = symbol {
        let flags = symbol.flags;
        if flags & symbol_flags::TYPE_ALIAS != 0 {
            // This is a circular type alias reference
            // Check if it's a direct/invalid circular reference
            if self.is_invalid_circular_type_alias(sym_id) {
                // Emit TS2456 error once
                if !self.ctx.circular_type_aliases_reported.contains(&sym_id) {
                    self.ctx.circular_type_aliases_reported.insert(sym_id);
                    let name = self.get_symbol_name(sym_id);
                    self.error_for_symbol(
                        sym_id,
                        &format!("Type alias '{}' circularly references itself.", name),
                        2456,
                    );
                }
            }
        }
        // Return Lazy placeholder as before...
    }
}
```

**Problem with Option B**: We're checking during recursion detection, but we don't have context about whether it's a "direct" reference at that point.

**Recommendation**: Use Option A - check during type lowering where we have both the type node and the resolved type.

## Files to Modify

1. **`crates/tsz-checker/src/state_type_analysis.rs`**
   - Add `is_direct_circular_reference` helper (around line 2100)
   - Add check in `compute_type_of_symbol` TYPE_ALIAS branch (around line 2080)
   - Add `is_simple_type_reference` helper

2. **`crates/tsz-common/src/diagnostics.rs`** (if error message not already defined)
   - Add TS2456 error code and message

3. **Test file**: `src/tests/circular_type_tests.rs` (new file)
   - Add unit tests for circular type detection

## Test Cases

```typescript
// Test 1: Simple circular (should error)
type A = B;
type B = A;

// Test 2: 3-way circular (should error)
type C = D;
type D = E;
type E = C;

// Test 3: Self-referential (should error)
type F = F;

// Test 4: Valid recursive (should NOT error)
type List = { value: number; next: List | null };

// Test 5: Circular through union (should error)
type G = H | string;
type H = G;

// Test 6: Valid recursive through union (should NOT error)
type Node = { left: Node | null; right: Node | null };

// Test 7: Circular with generics (should error)
type J<T> = K<T>;
type K<T> = J<T>;

// Test 8: Valid generic recursive (should NOT error)
type Box<T> = { value: T; next: Box<T> | null };
```

## Implementation Steps

1. ✅ Investigate and understand circular reference patterns
2. ✅ Create minimal test cases
3. ✅ Design detection algorithm
4. ⬜ Implement `is_simple_type_reference` helper
5. ⬜ Implement `is_direct_circular_reference` helper
6. ⬜ Add check in `compute_type_of_symbol` TYPE_ALIAS branch
7. ⬜ Add unit tests
8. ⬜ Run conformance tests to verify fix
9. ⬜ Commit with clear message

## Expected Impact

**Tests Fixed**: ~20-30 tests
- All circular reference test cases
- Tests that depend on proper circular detection

**Difficulty**: Medium
**Estimated Time**: 3-4 hours
- 1 hour: Implementation
- 1 hour: Testing and debugging
- 1 hour: Edge case handling
- 1 hour: Conformance test verification

## References

- Test files with TS2456:
  - `TypeScript/tests/cases/conformance/externalModules/typeOnly/circular2.ts`
  - `TypeScript/tests/cases/conformance/externalModules/typeOnly/circular4.ts`
  - `TypeScript/tests/cases/compiler/circularBaseTypes.ts`

- Code locations:
  - `crates/tsz-checker/src/state_type_analysis.rs:1004` - `get_type_of_symbol`
  - `crates/tsz-checker/src/state_type_analysis.rs:2080` - TYPE_ALIAS handling in `compute_type_of_symbol`
