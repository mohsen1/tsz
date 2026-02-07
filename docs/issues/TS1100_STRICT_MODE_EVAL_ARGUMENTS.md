# TS1100: Invalid use of 'eval' or 'arguments' in strict mode

**Status**: NEEDS FIX
**Discovered**: 2026-02-05
**Component**: Checker (grammar checking)
**Conformance Impact**: ~11 parser tests fail

## Problem

tsz does not emit TS1100 when `eval` or `arguments` are used as parameter names, variable names, or other binding identifiers in strict mode.

## Error Code

| Code | Message |
|------|---------|
| TS1100 | Invalid use of 'eval' in strict mode. |
| TS1100 | Invalid use of 'arguments' in strict mode. |

## Examples

### Example 1: eval as parameter name

```typescript
"use strict";
function foo(eval) {}  // Error: TS1100
```

**Expected**: TS1100 "Invalid use of 'eval' in strict mode."
**Actual**: No error

### Example 2: arguments as variable name

```typescript
"use strict";
let arguments = 1;  // Error: TS1100
```

**Expected**: TS1100 "Invalid use of 'arguments' in strict mode."
**Actual**: No error

### Example 3: In class (implicit strict mode)

```typescript
class C {
    method(eval) {}  // Error: TS1100 (class bodies are strict)
}
```

**Expected**: TS1100
**Actual**: No error

## When This Error Applies

Per ECMAScript strict mode rules:
1. `eval` and `arguments` cannot be used as:
   - Function parameter names
   - Variable names (let, const, var)
   - Function names
   - Class names
   - Catch clause binding names
   - Property shorthand in destructuring

2. Strict mode is active in:
   - Files with `"use strict"` directive
   - ES modules (always strict)
   - Class bodies (always strict)
   - Functions with strict directive

## Implementation Notes

This check should be added to the grammar checker phase, likely in:
- `src/checker/state_checking_members.rs` - when checking parameter declarations
- `src/checker/statements.rs` - when checking variable declarations

The check needs to:
1. Detect if we're in strict mode context
2. Check if the binding name is `eval` or `arguments`
3. Emit TS1100 if both conditions are true

## Testing

```bash
cd crates/conformance && cargo run --release --bin tsz-conformance -- \
  --filter "strict" --all \
  --test-dir ../TypeScript/tests/cases \
  --tsz-binary ../.target/release/tsz \
  --cache-file ../tsc-cache-full.json
```

## Related

- ECMA-262 strict mode restrictions
- TypeScript grammar checker
