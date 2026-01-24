# Enum Type Checking Analysis - TS2322 Issues

## Current Implementation

The enum type checking logic is primarily in:
- `src/checker/state.rs` - `enum_assignability_override()` function
- `src/checker/enum_checker.rs` - Helper utilities for enum type detection
- `src/solver/compat.rs` - CompatChecker with enum override support

## Current Enum Assignability Rules

From `enum_assignability_override()` in state.rs:

```rust
fn enum_assignability_override(&self, source: TypeId, target: TypeId, env: Option<&TypeEnvironment>) -> Option<bool> {
    // 1. Same enum type - assignable
    if let (Some(source_enum), Some(target_enum)) = (source_enum, target_enum) {
        return Some(source_enum == target_enum);
    }

    // 2. Numeric enum member → number - ASSIGNED (unsound but TS allows it)
    if source_enum is Numeric:
        return Some(is_assignable(NUMBER, target));

    // 3. number → Numeric enum - ASSIGNED if number is assignable to enum
    if target_enum is Numeric:
        return Some(is_assignable(source, NUMBER));

    // 4. String literal → String enum - NOT ASSIGNED (opacity)
    if target_enum is String && source is Literal:
        return Some(false);

    // 5. STRING → String enum - NOT ASSIGNED
    if target_enum is String && source == STRING:
        return Some(false);

    // 6. String enum → STRING - NOT ASSIGNED (different from numeric)
    if source_enum is String && target == STRING:
        return Some(false);
}
```

## TypeScript's Actual Enum Behavior

### Numeric Enums

```typescript
enum NumericEnum { A, B, C }

// Enum members are assignable to number (unsound but allowed)
let x: number = NumericEnum.A;  // ✅ OK

// number is assignable to enum (via type assertion or through number)
let y: NumericEnum = 0;  // ❌ TS2322: Type '0' is not assignable to type 'NumericEnum'
```

**Current Status**: The code checks `is_assignable(source, NUMBER)` for number → enum, but this should fail (TypeScript doesn't allow this without type assertion).

### String Enums

```typescript
enum StringEnum { A = "a", B = "b" }

// String enum members are NOT assignable to string
let x: string = StringEnum.A;  // ❌ TS2322: Type 'StringEnum.A' is not assignable to type 'string'

// String literals are NOT assignable to string enum
let y: StringEnum = "a";  // ❌ TS2322: Type '"a"' is not assignable to type 'StringEnum'

// But through type assertion:
let z: StringEnum = "a" as StringEnum;  // ✅ OK
```

**Current Status**: The code correctly handles string enum opacity (lines 5894-5917).

### Const vs Non-Const Enums

```typescript
const enum ConstEnum { A, B }
enum RegularEnum { C, D }

// Const enum members are literal types (0, 1)
let x: 0 = ConstEnum.A;  // ✅ OK - literal type

// Regular enum members are the enum type itself
let y: RegularEnum = RegularEnum.C;  // ✅ OK
```

**Current Status**: `enum_checker.rs` has `is_const_enum_type()` but this distinction is not fully utilized in assignability checks.

## Issues Found

### 1. **Missing TS2322: number → Numeric enum**
The current code at line 5881-5892 checks `is_assignable(source, NUMBER)` for number → enum, which would incorrectly allow:
```typescript
enum E { A }
let x: E = 5;  // Should error TS2322
```

**Fix**: This should return `Some(false)` when target is a numeric enum and source is NUMBER or a numeric literal that's not an enum member.

### 2. **String enum to string assignability**
Lines 5910-5917 prevent string enum → string, which is correct. However, we need to verify this works for all string enum cases.

### 3. **Const enum member types**
Const enum members should have literal types (e.g., `0`, `"a"`) rather than the enum type. The current code doesn't distinguish this in assignability.

### 4. **Computed enum members**
Enum members with computed values should have the computed type, not just NUMBER or STRING.

### 5. **Enum declaration merging**
When enums are merged across declarations, the types should combine properly.

## Recommended Fixes

### Fix 1: Disallow number → Numeric enum (without type assertion)

```rust
// In enum_assignability_override(), around line 5881:
if let Some(target_enum) = target_enum
    && self.enum_kind(target_enum) == Some(EnumKind::Numeric)
{
    // number is NOT assignable to numeric enum without type assertion
    if source == TypeId::NUMBER {
        return Some(false);
    }
    // Number literals are also not assignable unless they match an enum member
    if let Some(TypeKey::Literal(LiteralValue::Number(_))) = self.ctx.types.lookup(source) {
        return Some(false);
    }
}
```

### Fix 2: Verify string enum → string blocking

The current code at lines 5910-5917 looks correct, but we should add tests to ensure it works.

### Fix 3: Handle const enum literal types

When a const enum member is accessed, its type should be the literal type, not the enum type:

```rust
fn get_enum_member_type(&self, enum_type: TypeId, member_name: &str) -> TypeId {
    if self.is_const_enum_type(enum_type) {
        // For const enums, return the literal type
        self.get_enum_member_literal_value(enum_type, member_name)
    } else {
        // For regular enums, return the enum type
        enum_type
    }
}
```

## Test Cases Needed

```typescript
// Numeric enum tests
enum Numeric { A, B }
let n1: number = Numeric.A;  // Should be OK (unsound but TS allows)
let n2: Numeric = 5;  // Should ERROR TS2322
let n3: Numeric = 0;  // Should ERROR TS2322 (0 is not assignable to Numeric even though Numeric.A = 0)

// String enum tests
enum String { A = "a", B = "b" }
let s1: string = String.A;  // Should ERROR TS2322
let s2: String = "a";  // Should ERROR TS2322

// Const enum tests
const enum Const { A = 0, B = 1 }
let c1: 0 = Const.A;  // Should be OK
let c2: Const = 0;  // Should ERROR TS2322

// Heterogeneous enum tests
enum Mixed { A, B = "b" }
let m1: number = Mixed.A;  // Should ERROR TS2322 (not purely numeric)
let m2: Mixed.A;  // Type should be Mixed, not number
```

## Compilation Issues

Note: The codebase currently has compilation errors due to duplicate function definitions between:
- `src/checker/state.rs`
- `src/checker/flow_analysis.rs`
- `src/checker/type_checking.rs`

These duplicates were introduced during a refactoring to extract the flow_analysis module. The enum type checking changes will need to wait for these compilation issues to be resolved.
