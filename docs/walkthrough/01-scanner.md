# Scanner Module Deep Dive

The scanner (lexer) transforms source text into a stream of tokens. It's the first phase of compilation and establishes the foundation for efficient source handling throughout the pipeline.

## File Structure

```
src/
├── scanner.rs              Token types, SyntaxKind enum
├── scanner_impl.rs         Core tokenization engine
├── scanner_tests.rs        Token classification tests
├── scanner_impl_tests.rs   Tokenization tests
├── char_codes.rs           Character code constants
└── interner.rs             String interning (Atom)
```

## Core Data Structures

### ScannerState

**Location**: `scanner_impl.rs` - struct `ScannerState`

```rust
pub struct ScannerState {
    source: Arc<str>,                    // Shared source (zero-copy)
    pos: usize,                          // Current byte position
    end: usize,                          // End byte position
    full_start_pos: usize,               // Start including trivia
    token_start: usize,                  // Start excluding trivia
    token: SyntaxKind,                   // Current token type
    token_value: String,                 // Token text value
    token_flags: u32,                    // TokenFlags bitfield
    skip_trivia: bool,                   // Skip whitespace/comments?
    interner: Interner,                  // String deduplication
    token_atom: Atom,                    // Interned identifier
    // ... additional fields
}
```

### 📍 KEY: SyntaxKind Enum

**Location**: `scanner.rs` - enum `SyntaxKind`

167 token types organized by category:

| Category | Range | Examples |
|----------|-------|----------|
| Trivia | 2-8 | Comments, whitespace, newlines, shebangs |
| Literals | 9-18 | Numbers, strings, templates, JSX text |
| Punctuation | 19-79 | 60+ operators and brackets |
| Identifiers | 80-81 | Regular and private (`#id`) |
| Keywords | 83-166 | Reserved + contextual keywords |

### TokenFlags

**Location**: `scanner_impl.rs` - enum `TokenFlags`

16 flags packed into a single `u32`:

```rust
pub enum TokenFlags {
    None = 0,
    PrecedingLineBreak = 1,      // For ASI detection
    Unterminated = 4,            // Unclosed string/comment
    Scientific = 16,             // 1e10 notation
    HexSpecifier = 64,           // 0x prefix
    BinarySpecifier = 128,       // 0b prefix
    OctalSpecifier = 256,        // 0o prefix
    ContainsSeparator = 512,     // 1_000 syntax
    ContainsInvalidSeparator = 16384,
    // ...
}
```

## Tokenization Process

### Main Scan Loop

**Location**: `scanner_impl.rs` - method `scan()`

```rust
pub fn scan(&mut self) -> SyntaxKind
```

The scanner uses a massive match statement on character codes:

1. **Single-character tokens**: `{}()[];,~@:`
2. **Multi-character operators**: `+= -= *= /= ++ -- && || ?? =>`
3. **Comments**: `//` and `/* */`
4. **String literals**: `"..."` and `'...'`
5. **Template literals**: `` `...` `` with `${}`
6. **Numbers**: All numeric formats
7. **Identifiers/keywords**: Names and reserved words
8. **JSX**: JSX-specific tokens

### Process Flow

```
Source: "const x = 42;"
         │
         ▼
    ┌─────────────┐
    │ skip_trivia │ (if enabled)
    └──────┬──────┘
           │
           ▼
    ┌─────────────┐
    │ full_start  │ = pos (including trivia)
    └──────┬──────┘
           │
           ▼
    ┌─────────────┐
    │ token_start │ = pos (excluding trivia)
    └──────┬──────┘
           │
           ▼
    ┌─────────────┐
    │ match char  │ → Dispatch to specialized scanner
    └──────┬──────┘
           │
           ▼
    Return SyntaxKind
```

## Literal Scanning

### Numeric Literals

**Location**: `scanner_impl.rs` - method `scan_number()`

Supports all JavaScript numeric formats:

| Format | Example | Detection |
|--------|---------|-----------|
| Decimal | `42`, `3.14` | Starts with digit |
| Hexadecimal | `0xFF` | Prefix `0x` or `0X` |
| Binary | `0b1010` | Prefix `0b` or `0B` |
| Octal | `0o755` | Prefix `0o` or `0O` |
| BigInt | `123n` | Suffix `n` |
| Scientific | `1e10` | Contains `e` or `E` |
| Separators | `1_000_000` | Underscores between digits |

**⚠️ GAP: Numeric Separator Validation**
- Invalid separators detected (`1__0`, `_1`, `1_`) but error recovery is basic
- Edge cases with octal escapes in templates not fully handled

### String Literals

**Location**: `scanner_impl.rs` - method `scan_string()`

```rust
fn scan_string(&mut self, quote: char) -> SyntaxKind
```

- Handles escape sequences: `\n`, `\r`, `\t`, `\\`, `\'`, `\"`
- Line continuations: backslash before newline
- Sets `Unterminated` flag for unclosed strings

### Template Literals

**Location**: `scanner_impl.rs` - method `scan_template_literal()`

Complex state machine for template strings:

```typescript
`head ${expr} middle ${expr2} tail`
 ^^^^^ TemplateHead
              ^^^^^^ TemplateMiddle
                              ^^^^ TemplateTail
```

- Substitution detection: `${expr}`
- Escape sequence handling
- **📍 KEY**: Rescan methods for parser context switching

## Context-Sensitive Rescanning

The parser calls rescan methods when context changes token interpretation:

| Method | Purpose |
|--------|---------|
| `re_scan_greater_token()` | `>` vs `>>` vs `>>>` for generics |
| `re_scan_slash_token()` | `/` vs regex literal |
| `re_scan_template_token()` | Template continuation after `}` |
| `re_scan_jsx_token()` | JSX context switching |
| `re_scan_jsx_attribute_value()` | JSX attribute parsing |
| `re_scan_less_than_token()` | JSX `</` detection |
| `re_scan_hash_token()` | Private fields `#id` |
| `re_scan_question_token()` | Optional chaining `?.` vs `?` |

### Example: Greater-Than Disambiguation

**Location**: `scanner_impl.rs` - method `re_scan_greater_token()`

```typescript
// Problem: Is ">>" one token or two?
Map<string, Map<string, number>>
//                            ^^ Could be >> or > >

// Solution: Parser calls re_scan_greater_token() when expecting >
// Scanner returns single > and repositions
```

## JSX Support

**Location**: `scanner_impl.rs` - JSX-related methods

JSX requires special tokenization rules:

```rust
scan_jsx_identifier()         // Allows hyphens: data-testid
re_scan_jsx_token()           // Switch to JSX mode
scan_jsx_attribute_value()    // String or expression
re_scan_jsx_attribute_value() // Re-parse attribute
```

**JSX identifiers** can contain hyphens (unlike regular identifiers):
```typescript
<my-component data-testid="foo" />
```

## Performance Optimizations

### 1. Zero-Copy Architecture

**Location**: `scanner_impl.rs` - `source` field

```rust
source: Arc<str>,  // Shared reference, NOT Vec<char>
```

Benefits:
- No duplication across scanner, parser, AST phases
- String slices: `&source[start..end]` without allocation
- 4x memory savings for ASCII (1 byte vs 4 for char)

### 2. String Interning

**Location**: `interner.rs` - struct `Interner`

```rust
// 64-way sharded hash map for thread safety
const SHARD_BITS: usize = 6;  // 2^6 = 64 shards

// Returns Atom (u32) instead of String
fn intern(&self, s: &str) -> Atom
```

Benefits:
- O(1) identifier comparison: `atom_a == atom_b`
- Pre-interned common keywords (100+ words)
- Deduplication saves memory for repeated identifiers

### 3. Fast ASCII Path

**Location**: `scanner_impl.rs` - method `char_code_unchecked()`

```rust
#[inline(always)]
fn char_code_unchecked(&self) -> u32 {
    let b = self.source.as_bytes()[self.pos];
    if b < 128 {
        b as u32  // Fast path: single byte ASCII
    } else {
        // UTF-8 fallback
    }
}
```

### 4. Buffer Reuse

**Location**: `scanner_impl.rs` - in `scan()` method

```rust
self.token_value.clear();  // Reuse allocation
```

## State Snapshots for Lookahead

The parser needs to look ahead without consuming tokens:

**Location**: `scanner_impl.rs` - struct `ScannerSnapshot`

```rust
pub struct ScannerSnapshot {
    pos: usize,
    end: usize,
    full_start_pos: usize,
    token_start: usize,
    token: SyntaxKind,
    token_value: String,
    token_flags: u32,
    // ... 10 fields total
}

// Usage in parser:
let snapshot = self.scanner.save_state();
// ... lookahead logic ...
self.scanner.restore_state(snapshot);
```

## Integration with Parser

### Access Methods

| Method | Purpose |
|--------|---------|
| `get_token()` | Current token type |
| `get_token_value()` | Token text (owned) |
| `get_token_text()` | Token text (borrowed) |
| `get_token_start()` | Start position (excluding trivia) |
| `get_token_end()` | End position |
| `get_token_full_start()` | Start position (including trivia) |
| `get_token_flags()` | TokenFlags bitfield |
| `has_preceding_line_break()` | For ASI detection |
| `get_token_atom()` | Interned identifier |

### Zero-Copy Accessors

**Location**: `scanner_impl.rs` - accessor methods

```rust
get_token_value_ref()  // Uses interned atom when possible
get_token_text_ref()   // Direct source slice
source_slice()         // Direct reference slicing
```

## Known Gaps

### ⚠️ GAP: Unicode Support

**Location**: `scanner_impl.rs` - in `char_code_unchecked()`

```rust
// Simplified check: all non-ASCII treated as potential identifier chars
if ch > 127 { /* treat as identifier */ }
```

**Issue**: Should use proper Unicode category tables (`ID_Start`, `ID_Continue`)
**Impact**: May incorrectly accept/reject some Unicode identifiers

### ⚠️ GAP: Comment Nesting

**Location**: `scanner_impl.rs` - comment scanning logic

```typescript
/* outer /* inner */ */  // Edge case not fully handled
```

**Issue**: Simplified approach doesn't track nested `/* */`
**Impact**: Rare edge case, but differs from TSC behavior

### ⚠️ GAP: Octal Escapes in Templates

**Location**: `scanner_impl.rs` - template escape handling

```rust
// Comment in code: "octal in template is complex"
```

**Issue**: Octal escape sequences in template literals not fully implemented
**Impact**: May misparse legacy code with octal escapes

### ⚠️ GAP: Regex Flag Validation

**Location**: `scanner_impl.rs` - regex scanning

```rust
// Lists valid flags: g, i, m, s, u, v, y, d
// But doesn't validate combinations or report errors
```

**Issue**: Invalid flag combinations accepted
**Impact**: Should error on invalid regex flags

## Test Coverage

### Test Files

| File | Coverage |
|------|----------|
| `scanner_impl_tests.rs` | 36+ test cases |
| `scanner_tests.rs` | 13 classification tests |

### Covered Areas

- Empty input, whitespace, newlines
- All punctuation and operators
- String literals with escapes
- All numeric formats (decimal, hex, binary, octal, BigInt)
- Numeric separators (valid and invalid)
- Identifiers and keywords
- Comments (single and multi-line)
- Template literals
- Complex expressions
- Identifier interning
- Zero-copy accessors

---

**Next**: [02-parser.md](./02-parser.md) - Parser Module
