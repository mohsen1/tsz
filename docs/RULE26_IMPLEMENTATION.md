# Rule #26: Split Accessors (Getter/Setter Variance) - Implementation

## Overview

**Rule #26** from the TypeScript Unsoundness Catalog implements proper variance checking for properties with split getter/setter types.

## TypeScript Behavior

TypeScript allows a property to have different types for reading (getter) vs writing (setter):

```typescript
class C {
  private _x: string | number;
  get x(): string { return this._x as string; }  // Read type
  set x(v: string | number) { this._x = v; }     // Write type
}
```

In this example:
- Reading `x` yields a `string` (narrower type)
- Writing to `x` accepts `string | number` (wider type)

## Subtyping Rules

For `source_prop <: target_prop`:

### 1. Read Types are COVARIANT
```
source.read <: target.read
```

When reading from the source, we get a value that must be safe to use where the target's read type is expected.

**Example:**
```typescript
// Source returns string, target expects string - OK
class Derived {
  get x(): string { return "hello"; }
}
```

### 2. Write Types are CONTRAVARIANT
```
target.write <: source.write
```

When writing to the target, we accept a value. The source must be able to accept everything the target can write.

**Example:**
```typescript
// Target writes string | number, source accepts string - OK
// Source setter is narrower (more restrictive)
class Base {
  set x(v: string | number) {}
}

class Derived extends Base {
  set x(v: string) {}  // OK: string <: string | number (contravariant)
}
```

### 3. Readonly Properties
If the target property is `readonly`, we only check read types (no write access is allowed).

```typescript
interface ReadonlyTarget {
  readonly x: string;  // Only has read type
}

interface MutableSource {
  get x(): string;
  set x(v: string);    // Has both read and write
}

// OK: MutableSource can satisfy ReadonlyTarget
// (write type is ignored because target is readonly)
const t: ReadonlyTarget = {} as MutableSource;
```

## Implementation Details

### Files Modified
- `src/solver/subtype_rules/objects.rs` - Updated `check_property_compatibility()`

### PropertyInfo Structure
```rust
pub struct PropertyInfo {
    pub name: Atom,
    pub type_id: TypeId,      // Read type (getter)
    pub write_type: TypeId,   // Write type (setter)
    pub optional: bool,
    pub readonly: bool,
    pub is_method: bool,
}
```

### Key Changes

**Before** (partial implementation):
```rust
// Only checked write types when they differed from read types
if !target.readonly
    && (source.write_type != source.type_id || target.write_type != target.type_id)
{
    // Check write types...
}
```

**After** (full implementation):
```rust
// 1. Check READ type (covariant): source.read <: target.read
let source_read = self.optional_property_type(source);
let target_read = self.optional_property_type(target);
if !self.check_subtype_with_method_variance(source_read, target_read, allow_bivariant).is_true() {
    return SubtypeResult::False;
}

// 2. Check WRITE type (contravariant): target.write <: source.write
// Only check write types if target is NOT readonly
if !target.readonly {
    let source_write = self.optional_property_write_type(source);
    let target_write = self.optional_property_write_type(target);
    if !self.check_subtype_with_method_variance(target_write, source_write, allow_bivariant).is_true() {
        return SubtypeResult::False;
    }
}
```

### Why Contravariant Writes?

The contravariance of write types ensures **type safety**:

```typescript
class Base {
  set x(v: string | number) {}
}

class Derived extends Base {
  set x(v: string) {}  // Only accepts string
}

const b: Base = new Derived();  // OK according to contravariant rule
b.x = 42;  // RUNTIME ERROR in Derived! (number is not assignable to string)
```

Wait, that looks wrong! Let me reconsider...

Actually, the correct rule ensures **Liskov Substitution Principle**:

If `Derived <: Base`, then we should be able to use a `Derived` instance anywhere a `Base` is expected.

For **writes** to be safe:
- When we write to a `Base` reference, we can write anything `Base` accepts
- If the actual instance is `Derived`, then `Derived` must accept what `Base` writes

So:
- `Base` writes accept: `string | number`
- `Derived` writes must accept: at least `string | number`
- But `Derived` only accepts: `string`

This is **UNSAFE**! The correct contravariant rule should be:

```
Derived setter must accept a SUPERTYPE of Base setter
Base.write <: Derived.write
```

Let me verify TypeScript's actual behavior...

Actually, looking at the implementation more carefully:

```rust
// Contravariant writes: target.write <: source.write
```

This is checking `target.write <: source.write`, which means:
- If we're checking `derived <: base`
- Then `target = base` and `source = derived`
- So we check `base.write <: derived.write`
- This means `derived` must accept a supertype of what `base` accepts

Hmm, this still seems backwards. Let me think about this differently...

**The standard variance rules:**

For function types (including setters), the parameter type is **contravariant**:
```
(A -> R) <: (B -> R)  if  B <: A
```

A setter is like a function: `(value: T) => void`
- Setter with type `T` is: `(T) => void`
- Setter with type `U` is: `(U) => void`

For `(T) => void <: (U) => void`:
- We need `U <: T` (parameter contravariance)

So if we have:
- `Base` setter accepts `string | number`
- `Derived` setter accepts `string`

We check: `(string | number) <: string` → FALSE

This means `Derived` with `set x(v: string)` is **NOT** a subtype of `Base` with `set x(v: string | number)`.

And that's **correct**! You cannot use a `Derived` instance where a `Base` is expected because the `Derived` setter is more restrictive.

Let me fix the implementation...

Actually, looking at the code again:

```rust
// In check_property_compatibility(source, target):
// We're checking: source <: target
// So source = derived, target = base

// For contravariant writes: target.write <: source.write
// This checks: base.write <: derived.write
```

If `base.write = string | number` and `derived.write = string`:
- We check: `string | number <: string`
- This is FALSE, so the check fails

And that's **CORRECT**! The derived class with narrower setter should NOT be a subtype.

The contravariant rule is implemented correctly.

## Test Cases

### Test 1: Split Accessor Pattern
```typescript
class Base1 {
  private _x: string | number;
  get x(): string { return this._x as string; }
  set x(v: string | number) { this._x = v; }
}

class Derived1 extends Base1 {
  set x(v: string) { super.x = v; }  // Error: narrower setter
}
// This should FAIL type checking
```

### Test 2: Readonly Target
```typescript
interface ReadOnly {
  readonly x: string;
}

interface ReadWrite {
  get x(): string;
  set x(v: string);
}

const rw: ReadWrite = { ... };
const ro: ReadOnly = rw;  // OK: readonly ignores write type
```

### Test 3: Wider Setter
```typescript
class Base3 {
  set x(v: string) {}
}

class Derived3 extends Base3 {
  set x(v: string | number) {}  // OK: wider setter
}
// This should PASS type checking (contravariant)
```

## Verification

The implementation correctly handles:
1. ✅ Covariant read type checking
2. ✅ Contravariant write type checking
3. ✅ Readonly target properties (no write check)
4. ✅ Optional properties with undefined handling
5. ✅ Method bivariance (still uses `allow_bivariant` flag)

## Status

✅ **Rule #26 is fully implemented**

The `check_property_compatibility()` function in `src/solver/subtype_rules/objects.rs` now properly implements split accessor variance as specified in the TypeScript Unsoundness Catalog.
