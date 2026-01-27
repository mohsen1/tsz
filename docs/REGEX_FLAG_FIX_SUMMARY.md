# Regex Flag Error Detection Implementation

## Problem
The TypeScript compiler was not detecting or reporting regex flag errors. Valid regex syntax errors were being silently ignored, causing incorrect behavior.

## Solution
Implemented comprehensive regex flag error detection in the scanner and parser.

## Changes Made

### 1. Scanner (`src/scanner_impl.rs`)
- **Changed `RegexFlagError` from simple enum to struct**:
  ```rust
  pub struct RegexFlagError {
      pub kind: RegexFlagErrorKind,
      pub pos: usize,
  }

  pub enum RegexFlagErrorKind {
      Duplicate,
      InvalidFlag,
      IncompatibleFlags,
  }
  ```

- **Updated scanner state to track multiple errors**:
  ```rust
  // Old: Single error
  regex_flag_error: RegexFlagError,
  regex_flag_error_pos: Option<usize>,

  // New: Vector of errors
  regex_flag_errors: Vec<RegexFlagError>,
  ```

- **Modified flag scanning logic to collect ALL errors** (not just first):
  - Detects each duplicate flag occurrence
  - Detects each invalid flag character
  - Detects incompatible u/v flag combinations
  - Records position of each error

### 2. Parser (`src/parser/state.rs`)
- **Added error emission in `parse_regex_literal()`**:
  ```rust
  // Capture errors BEFORE parse_expected clears them
  let flag_errors: Vec<_> = self.scanner.get_regex_flag_errors().to_vec();

  self.parse_expected(SyntaxKind::RegularExpressionLiteral);

  // Emit errors for all regex flag issues
  for error in flag_errors {
      let (message, code) = match error.kind {
          RegexFlagErrorKind::Duplicate => ("Duplicate regular expression flag.", 1500),
          RegexFlagErrorKind::InvalidFlag => ("Unknown regular expression flag.", 1499),
          RegexFlagErrorKind::IncompatibleFlags => ("The Unicode 'u' flag and the Unicode Sets 'v' flag cannot be set simultaneously.", 1502),
      };
      self.parse_error_at(error.pos as u32, 1, message, code);
  }
  ```

### 3. Tests (`src/regex_flag_tests.rs`)
Added comprehensive unit tests covering:
- Invalid flag characters (TS1499)
- Duplicate flags (TS1500)
- Incompatible u/v flags (TS1502)
- Multiple errors in a single regex
- Valid regexes (no errors)

## Error Codes
- **TS1499**: Unknown regular expression flag
- **TS1500**: Duplicate regular expression flag
- **TS1502**: The Unicode 'u' flag and the Unicode Sets 'v' flag cannot be set simultaneously

## Examples

### Input:
```typescript
const r1 = /test/x;      // Invalid flag
const r2 = /test/gg;     // Duplicate flag
const r3 = /test/uv;     // Incompatible flags
const r4 = /test/ggxx;   // Mixed errors
```

### Output:
```
TS1499: Unknown regular expression flag. @ position 11
TS1500: Duplicate regular expression flag. @ position 11
TS1502: The Unicode 'u' flag and the Unicode Sets 'v' flag cannot be set simultaneously. @ position 12
TS1500: Duplicate regular expression flag. @ position 11
TS1499: Unknown regular expression flag. @ position 12
TS1499: Unknown regular expression flag. @ position 13
```

## Validation
All 9 unit tests pass:
- ✓ test_invalid_flag_x
- ✓ test_duplicate_flag_gg
- ✓ test_incompatible_flags_uv
- ✓ test_multiple_invalid_flags
- ✓ test_mixed_errors
- ✓ test_valid_regex_no_errors
- ✓ test_valid_regex_no_flags
- ✓ test_all_valid_flags
- ✓ test_complex_incompatible_flags

## Key Implementation Details

1. **Multiple Error Detection**: The scanner now records ALL flag errors, not just the first one. This matches TypeScript's behavior of reporting each issue separately.

2. **Error Position Tracking**: Each error includes the exact character position of the problematic flag, enabling precise error reporting.

3. **Timing is Critical**: Errors must be captured in the parser BEFORE calling `parse_expected()` because it calls `next_token()` which clears the scanner's error state.

4. **UTF-8 Safe**: The implementation uses `char_len_at()` for proper UTF-8 character handling when advancing through flag characters.

## Impact
- Developers now get accurate error messages for invalid regex flags
- Prevents runtime errors from malformed regex literals
- Improves TypeScript compatibility
- Reduces false negatives in type checking
