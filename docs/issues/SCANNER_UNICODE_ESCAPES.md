# Scanner: Unicode Escapes in Identifiers

**Status**: NEEDS FIX
**Discovered**: 2026-02-05
**Component**: Scanner (scanner_impl.rs)
**Conformance Impact**: Minor (~5 tests)

## Problem

tsz doesn't properly handle Unicode escape sequences (`\uXXXX`) that appear after the first character of an identifier.

### Example

```typescript
class C\u0032 {
}
```

`\u0032` is the Unicode escape for "2", so this should be parsed as `class C2`. But tsz produces:
- TS1005: '{' expected
- TS1068: Unexpected token (x2)

### Root Cause

In `src/scanner/scanner_impl.rs`, the `scan_identifier()` function (line 1433-1458) handles basic identifier scanning:

```rust
while self.pos < self.end {
    let ch = self.char_code_unchecked(self.pos);
    if !is_identifier_part(ch) {
        break;
    }
    self.pos += self.char_len_at(self.pos);
}
```

When it encounters `\`, which is not an identifier part character, it stops scanning. It doesn't check for Unicode escape sequences mid-identifier.

There IS code for handling identifiers that START with Unicode escapes (`scan_identifier_with_escapes()`), but not for escapes within identifiers.

## Expected Behavior

When a backslash is encountered within an identifier, the scanner should:
1. Check if it starts a valid Unicode escape (`\uXXXX` or `\u{XXXXX}`)
2. If valid and the escaped code point is an identifier part, include it in the identifier
3. Otherwise, stop scanning the identifier

## Fix Approach

Modify `scan_identifier()` to:
1. When `ch == CharacterCodes::BACKSLASH`, call `peek_unicode_escape()`
2. If valid escape that is an identifier part, consume it and continue
3. If valid escape but not identifier part, or invalid escape, break

This requires switching from zero-allocation mode to allocation mode when escapes are encountered, similar to how `scan_identifier_with_escapes()` works.

## Complexity

- **Medium**: The change is localized to the scanner but requires careful handling of:
  - Memory allocation (escapes require building a String)
  - Proper escape validation
  - Edge cases with invalid escapes

## Related Files

- `src/scanner/scanner_impl.rs`:
  - `scan_identifier()` (line 1433)
  - `scan_identifier_with_escapes()` (line 1500)
  - `peek_unicode_escape()` (line 1462)
  - `scan_unicode_escape_value()` (line 1544)

## Testing

Test with files containing Unicode escapes in identifiers:
```bash
echo 'class C\u0032 {}' > /tmp/test.ts
./target/debug/tsz /tmp/test.ts
```
