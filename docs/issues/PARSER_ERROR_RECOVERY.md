# Parser Error Recovery Differences

**Status**: NEEDS INVESTIGATION
**Discovered**: 2026-02-05
**Component**: Parser
**Conformance Impact**: ~40% of parser tests fail due to error recovery differences

## Problem

tsz's parser error recovery produces different (usually more) errors than TSC when parsing malformed code. This causes many parser conformance tests to fail.

## Conformance Stats (Updated 2026-02-05)

- Parser tests: 53.1% pass rate (460/867)
- Scanner tests: 50.0% pass rate (21/42)
- Top error mismatches:
  - TS2304: missing=35, extra=88 (cannot find name) - mostly lib loading bug
  - TS1005: missing=25, extra=29 (token expected)
  - TS1109: missing=11, extra=24 (expression expected)
  - TS1128: missing=2, extra=27 (declaration expected)
  - TS2552: missing=7, extra=19 (name typo suggestion)
  - TS1100: missing=11, extra=0 (invalid use of eval/arguments) - strict mode validation (requires checker)

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

### TS1132 for leading comma in enum members (commit later)

Fixed `enum E { , }` to emit TS1132 "Enum member expected" instead of generic TS1003 "Identifier expected".

**File**: `src/parser/state_declarations.rs` - `parse_enum_members()`

### TS1357 for invalid tokens after enum member name (commit 0299d37)

Fixed `enum E { a: 1 }` to emit TS1357 "An enum member name must be followed by a ',', '=', or '}'" instead of TS1005.

**File**: `src/parser/state_declarations.rs` - `parse_enum_members()`

### TS1005 for shorthand properties with non-identifiers (commit 569e5bd)

Fixed `{ class }`, `{ "" }`, and `{ 0 }` to emit TS1005 "':' expected" because shorthand properties only work with identifiers.

**File**: `src/parser/state_expressions.rs` - `parse_property_assignment()`

### TS1162 for optional markers in object literals (commit 1277c58)

Fixed `{ name? }` to emit TS1162 "An object member cannot be declared optional" instead of TS1005.

**File**: `src/parser/state_expressions.rs` - `parse_property_assignment()`

### TS1097 for empty extends list (commit bb510a5)

Fixed `class C extends { }` to emit TS1097 "'extends' list cannot be empty."

**File**: `src/parser/state_declarations.rs` - `parse_heritage_clauses()`

### TS1172 for duplicate extends in interface (commit 5655b17)

Fixed `interface I extends A extends B {}` to emit TS1172 "'extends' clause already seen."

**File**: `src/parser/state_declarations.rs` - `parse_heritage_clauses()`

### TS1123 for empty variable declaration list (commit a3b49d1)

Fixed `var ;` to emit TS1123 "Variable declaration list cannot be empty."

**File**: `src/parser/state_statements.rs` - `parse_variable_statement()`

### TS1123 for empty var declarations in for-in/for-of (commit 328f3f5)

Fixed `for (var in X)` and `for (var of X)` to emit TS1123 "Variable declaration list cannot be empty."

**File**: `src/parser/state_declarations.rs` - `parse_for_variable_declaration_list()`

### TS1091/TS1188 for multiple declarations in for-in/for-of (commit faf70c5)

Fixed `for (var a, b in X)` to emit TS1091 and `for (var a, b of X)` to emit TS1188.

**File**: `src/parser/state_declarations.rs` - `parse_for_in_statement_rest()`, `parse_for_of_statement_rest()`

### TS1121 for legacy octal literals (commit fe29c42)

Fixed `01`, `0777`, `01.0` to emit TS1121 "Octal literals are not allowed. Use the syntax '0o1'."

**File**: `src/parser/state_expressions.rs` - `parse_numeric_literal()`

### TS1260 for keywords with unicode escapes (commit 5c6313b)

Fixed `\u0076ar` (var with escape) to emit TS1260 "Keywords cannot contain escape characters."

**File**: `src/parser/state.rs` - `consume_keyword()`, `check_keyword_with_escape()`

### TS1124 for missing exponent digits (commit a5ed8a7)

Fixed `1e+`, `1e-`, `1e` to emit TS1124 "Digit expected."

**File**: `src/parser/state_expressions.rs` - `parse_numeric_literal()`

### TS1125 for hex literals without digits (commit b78a0e5)

Fixed `0x`, `0X` to emit TS1125 "Hexadecimal digit expected."

**File**: `src/parser/state_expressions.rs` - `parse_numeric_literal()`

### TS1177 for binary literals without digits (commit 1082341)

Fixed `0b`, `0B` to emit TS1177 "Binary digit expected."

**File**: `src/parser/state_expressions.rs` - `parse_numeric_literal()`

### TS1178 for octal literals without digits (commit 1082341)

Fixed `0o`, `0O` to emit TS1178 "Octal digit expected."

**File**: `src/parser/state_expressions.rs` - `parse_numeric_literal()`

### TS1489 for decimals with leading zeros (commit 8e33a8e, improved 40d09e1)

Fixed `08`, `009`, `08.5` to emit TS1489 "Decimals with leading zeros are not allowed."
These are numbers starting with `0` that contain non-octal digits (8 or 9).

Note: The scanner only sets the Octal flag when the first digit after `0` is 0-7.
Numbers like `08` where the first digit is 8 or 9 don't have this flag set, so the
check inspects the token text directly for the leading zero pattern.

**File**: `src/parser/state_expressions.rs` - `parse_numeric_literal()`

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
cd conformance-rust && cargo run --release --bin tsz-conformance -- \
  --filter parser --all \
  --test-dir ../TypeScript/tests/cases \
  --tsz-binary ../.target/release/tsz \
  --cache-file ../tsc-cache-full.json
```
