# Parser Error Recovery Differences

**Status**: NEEDS INVESTIGATION
**Discovered**: 2026-02-05
**Component**: Parser
**Conformance Impact**: ~40% of parser tests fail due to error recovery differences

## Problem

tsz's parser error recovery produces different (usually more) errors than TSC when parsing malformed code. This causes many parser conformance tests to fail.

## Conformance Stats

- Parser tests: 50.3% pass rate (420/835)
- Top error mismatches:
  - TS2304: missing=35, extra=84 (cannot find name)
  - TS1005: missing=29, extra=32 (token expected)
  - TS1109: missing=11, extra=28 (expression expected)
  - TS1128: missing=2, extra=28 (declaration expected)

**Note**: Many TS2304 errors are caused by the default lib loading bug (see DEFAULT_LIB_LOADING_BUG.md).

## Examples

### Example 1: Missing closing paren in if statement

```typescript
// parserErrorRecoveryIfStatement2.ts
class Foo {
  f1() {
    if (a
  }
  f2() { }
}
```

| Compiler | Errors |
|----------|--------|
| TSC | `TS1005: ')' expected` at (4,3) |
| tsz | `TS2304: Cannot find name 'a'` at (3,9) |

TSC detects the missing `)` and reports a parser error.
tsz parses it differently and reports a semantic error about 'a' being undefined.

### Example 2: Unclosed function call with return keyword

```typescript
// parserErrorRecovery_ArgumentList1.ts
function foo() {
   bar(
   return x;
}
```

| Compiler | Errors |
|----------|--------|
| TSC | `TS1135: Argument expression expected` (1 error) |
| tsz | 4 errors including `TS2304: Cannot find name 'return'` |

TSC recognizes `return` as a keyword and reports one precise error.
tsz parses `return` as an identifier, producing multiple cascading errors.

### Example 3: Missing closing brace in method

```typescript
// parserErrorRecovery_Block3.ts
class C {
    private a(): boolean {

    private b(): boolean {
    }
}
```

| Compiler | Errors |
|----------|--------|
| TSC | `TS1128: Declaration or statement expected` (1 error) |
| tsz | 5 errors including multiple TS1005, TS2304 |

## Root Cause

TSC's parser has sophisticated error recovery that:
1. Recognizes keywords in unexpected contexts
2. Recovers to a known state and continues parsing
3. Produces fewer, more meaningful errors

tsz's parser:
1. Sometimes parses keywords as identifiers during error recovery
2. Continues parsing in a corrupted state
3. Produces cascading errors

## Severity

- **User Impact**: Medium - more errors shown for malformed code, but valid code works fine
- **Conformance Impact**: High - many parser tests fail
- **Fix Complexity**: Very High - requires fundamental parser architecture changes

## Fixes Applied

### Modifier keywords as property names (commit 3b8061e)

Fixed issue where `class C { public }` caused TS1068 errors because the parser was consuming `public` as a modifier expecting more tokens after it. The fix treats modifier keywords as property names when followed by `}` or EOF.

**File**: `src/parser/state_statements.rs` - `should_stop_class_member_modifier()`

## Recommended Approach

1. **Short-term**: Focus on other conformance improvements
2. **Long-term**: Study TSC's error recovery patterns and implement similar logic
3. **Specific fixes**: Can fix individual patterns like "don't parse keywords as identifiers"

## Related Files

- `src/parser/state_*.rs` - Parser implementation
- `src/parser/scanner.rs` - Tokenizer

## Testing

Run parser conformance tests:
```bash
./scripts/conformance.sh run --filter parser
```
