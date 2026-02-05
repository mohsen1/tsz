# TS1183: declare function with body

**Status**: FIXED (commit d8257f5)
**Discovered**: 2026-02-05
**Component**: Checker / Grammar Checker
**Conformance Impact**: ~1 parser test fails

## Problem

tsz does not emit TS1183 when a `declare function` has a body. The error IS implemented for class methods in declared classes, but NOT for standalone function declarations with `declare` modifier.

## Error Code

| Code | Message |
|------|---------|
| TS1183 | An implementation cannot be declared in ambient contexts. |

## Example

```typescript
// parserFunctionDeclaration2.ts
declare function Foo() {
}
```

**Expected**: TS1183 "An implementation cannot be declared in ambient contexts"
**Actual**: No error

## Current Implementation

The error IS correctly emitted for:
- Methods in `declare class` (checked in `state_checking_members.rs:1412-1423`)
- Accessors in `declare class` (checked in `state_checking_members.rs:1639-1647`, `1732-1740`)
- Accessors in type literals/interfaces (checked in `state_checking_members.rs:930-941`)

The error is NOT emitted for:
- `declare function Foo() { }` - standalone declared function with body

## Implementation Location

The check should be added in `check_function_declaration` (`src/checker/state_checking_members.rs:2108`).

Proposed check:
```rust
// After getting the function node
if !func.body.is_none() && self.has_declare_modifier(&func.modifiers) {
    self.error_at_node(
        func_idx,
        "An implementation cannot be declared in ambient contexts.",
        diagnostic_codes::IMPLEMENTATION_CANNOT_BE_IN_AMBIENT_CONTEXT,
    );
}
```

## Testing

```bash
./scripts/conformance.sh run --filter "parserFunctionDeclaration" --verbose
```
