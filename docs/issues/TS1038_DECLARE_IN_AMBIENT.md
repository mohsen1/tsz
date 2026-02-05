# TS1038: 'declare' modifier in already ambient context

**Status**: FIXED
**Discovered**: 2026-02-05
**Component**: Checker / Grammar Checker
**Conformance Impact**: ~2 parser tests fail

## Problem

tsz does not emit TS1038 when a `declare` modifier is used inside an already ambient context (like inside a `declare namespace`).

## Error Code

| Code | Message |
|------|---------|
| TS1038 | A 'declare' modifier cannot be used in an already ambient context. |

## Examples

### Example 1: declare function inside declare namespace

```typescript
// parserFunctionDeclaration1.ts
declare namespace M {
  declare function F();  // Error: TS1038
}
```

**Expected**: TS1038
**Actual**: No error

### Example 2: declare var inside declare namespace

```typescript
declare namespace chrome {
    declare var tabId: number;  // Error: TS1038
}
```

**Expected**: TS1038
**Actual**: No error

## Current Unit Tests Are Wrong

The file `src/tests/parser_ts1038_tests.rs` has unit tests that incorrectly claim TypeScript allows these patterns. The tests should be updated to expect TS1038 errors.

## Implementation Notes

In TSC, this check is performed by the grammar checker during the binding phase. It tracks whether we're inside an ambient context and errors when a `declare` modifier appears in that context.

## Recommended Approach

1. Track ambient context in the checker (already partially done via `is_ambient_declaration()`)
2. When visiting declarations with `declare` modifier, check if already in ambient context
3. Emit TS1038 if so

## Related Files

- `src/checker/state_checking_members.rs` - Has ambient tracking logic
- `src/tests/parser_ts1038_tests.rs` - Incorrect unit tests that need fixing

## Testing

```bash
./scripts/conformance.sh run --filter "parserFunctionDeclaration" --verbose
```
