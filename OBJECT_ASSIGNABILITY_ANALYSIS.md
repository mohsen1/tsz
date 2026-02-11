# Object Assignability Bug - Deep Analysis

## Problem
`let x: Object = {}` fails with TS2322, but tsc accepts it.

## Root Cause Identified

### How Primitives Work (ALREADY IMPLEMENTED)
When checking `string` assignable to Object interface:
1. Source is intrinsic STRING
2. Target is Lazy(DefId) pointing to Object interface
3. Target resolves to object type with `{toString: Function, valueOf: Function, ...}`
4. `is_boxed_primitive_subtype` is called (subtype_rules/intrinsics.rs:388)
5. Gets boxed String interface type
6. String interface HAS toString method
7. Structural check: String interface <: Object interface succeeds!

### Why Empty Object Fails (THE BUG)
When checking `{}` assignable to Object interface:
1. Source is object type with NO properties (empty object literal)
2. Target is Lazy(DefId) pointing to Object interface
3. Target resolves to object type with methods
4. Structural check: `{}` has no properties, Object has methods
5. **FAILS** - no property matching

### The TypeScript Rule
In TypeScript, ALL objects inherit Object.prototype methods.
So `{}` inherently HAS toString, valueOf, etc. through prototype chain.
The Object interface should accept any object type.

## Attempted Solutions

### Solution 1: Add boxed type for Object (WRONG)
- Object isn't a primitive that needs boxing
- It IS the interface that other things box to

### Solution 2: Special-case Object interface (CORRECT but COMPLEX)
Need to:
1. Identify if target is the global Object interface (not just any object)
2. Check if source is any object type (not null/undefined)
3. Return true without structural check

**Challenge:** How to identify "global Object interface"?
- It's a Lazy(DefId) type
- No existing helper to check "is this the Object interface?"
- Could check name + structure, but fragile

### Solution 3: Mark Object members as optional (WRONG)
- Would break other checks
- Not how TypeScript models it

## Existing Infrastructure

**Relevant code:**
- `crates/tsz-solver/src/compat.rs:618` - `is_assignable_impl`
- `crates/tsz-solver/src/subtype_rules/intrinsics.rs:388` - `is_boxed_primitive_subtype`
- `crates/tsz-solver/src/subtype.rs:2992` - Lazy type resolution

**Tests that pass:**
- `test_object_trifecta_object_interface_accepts_primitives` - primitives to Object works
- `test_object_trifecta_assignability` - {} type accepts everything

**What's missing:**
- No test for object LITERAL to Object INTERFACE
- No helper to identify global Object interface

## Correct Fix (Estimated 4-6 hours)

### Step 1: Add Object interface identification
In `TypeResolver` trait:
```rust
fn get_global_object_interface(&self) -> Option<TypeId> {
    None
}
```

Implement in checker to return the DefId of lib.d.ts Object interface.

### Step 2: Add check in is_assignable_impl
In `compat.rs` around line 648, before empty object check:
```rust
// Special case: any object type assignable to global Object interface
if let Some(object_iface) = self.resolver.get_global_object_interface() {
    if target == object_iface && self.is_object_type(source) {
        return true;
    }
}
```

### Step 3: Implement is_object_type helper
```rust
fn is_object_type(&self, type_id: TypeId) -> bool {
    // Check if type is an object (not primitive, not null/undefined/void)
    match self.interner.lookup(type_id) {
        Some(TypeKey::Object(_)) |
        Some(TypeKey::ObjectWithIndex(_)) => true,
        Some(TypeKey::Union(members)) => {
            // All union members must be objects
            let members = self.interner.type_list(members);
            members.iter().all(|&m| self.is_object_type(m))
        }
        _ => false
    }
}
```

### Step 4: Write unit test
```rust
#[test]
fn test_empty_object_literal_to_object_interface() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    // Create Object interface
    let object_iface = interner.object(vec![
        PropertyInfo::method("toString", ...),
    ]);
    let def_id = DefId(1);
    env.insert_def(def_id, object_iface);
    env.set_global_object_interface(interner.lazy(def_id));

    let empty_obj = interner.object(Vec::new());
    let mut checker = CompatChecker::with_resolver(&interner, &env);

    assert!(checker.is_assignable(empty_obj, interner.lazy(def_id)));
}
```

### Step 5: Update checker to register Object interface
In checker initialization, after loading lib.d.ts:
```rust
if let Some(object_sym) = self.resolve_global_symbol("Object") {
    if let Some(object_type) = self.get_type_of_symbol(object_sym) {
        self.solver_env.set_global_object_interface(object_type);
    }
}
```

## Risk Assessment

**Low Risk:**
- Isolated change
- Only affects Object interface specifically
- Has clear test coverage

**Medium Risk:**
- Need to ensure Object is correctly identified
- Could accidentally allow wrong assignments if identification is too broad

**Testing Required:**
- Unit tests for the new logic
- Conformance tests should improve by ~88
- Verify no regressions in existing tests

## Alternative: Prototype Chain Awareness (HARD - 20+ hours)

Make the type system aware of prototype chains:
- Track `[[Prototype]]` for object types
- During structural check, look up prototype chain
- Would fix this AND other prototype-related issues
- Much larger architectural change

## Impact

**Fixes:** 88 TS2322 false positives in slice 4
**Examples:**
- `interfaceWithPropertyOfEveryType.ts`
- Any test with `property: Object` assigned object literal

## Status

**Investigation:** COMPLETE
**Implementation:** NOT STARTED (requires 4-6 hours)
**Priority:** MEDIUM (clear fix, medium impact)
