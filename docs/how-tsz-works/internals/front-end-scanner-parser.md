# Front End: Scanner and Parser

The front end is the only part of the pipeline that touches raw source bytes. It
turns a UTF-8 file into a flat token stream (`tsz-scanner`) and then into a
syntax-only AST stored in an arena (`tsz-parser`). Both crates are deliberately
*pre-semantic*: they decide what a span of text **is** structurally, never what
it **means** as a type. The scanner answers "what token starts here?"; the
parser answers "what grammar production is this?". Neither one resolves a name,
computes a type, decides assignability, or instantiates a generic — those
questions belong to the [binder](binder.md), [checker](checker-context-and-state.md),
and [solver](solver-relations.md) layers downstream.

Two design choices dominate this layer and recur throughout the document. First,
the scanner is **zero-copy**: the source is held once as an `Arc<str>` and every
position is a byte offset into it, so identifiers, literals, and comments are
slices, not freshly allocated strings (see `crates/tsz-scanner/src/scanner_impl.rs`,
`struct ScannerState`). Second, the AST is a **thin arena**: every node is a
16-byte header (`crates/tsz-parser/src/parser/node.rs`, `struct Node`) indexed by
a `NodeIndex(u32)`, with the bulky per-node payload living in one of ~90 typed
side pools. Identifiers are interned to `AstAtom(u32)` so name comparison is a
`u32` equality, not a string compare.

---

## Owns / Must not own

| Concern | Scanner owns | Parser owns | Neither owns (downstream) |
| --- | --- | --- | --- |
| Token boundaries, token `SyntaxKind` | yes | — | — |
| String/identifier interning to `AstAtom` | yes (mints) | inherits the interner | — |
| Numeric/string/template/regex literal text + flags | yes | — | literal *type* (`solver`) |
| Trivia and comment ranges | yes (classifies) | caches ranges on the file | — |
| Contextual rescans (`>`, `/`, `<`, `?`, `#`, template, JSX) | yes (executes) | yes (decides when) | — |
| Grammar productions, node shapes, child wiring | — | yes | — |
| Syntactic diagnostics (TS1xxx family) | a few (TS1010, TS1124, TS1185…) | most | — |
| Parser context flags (async, generator, disallow-`in`…) | — | yes | — |
| Speculative lookahead / backtracking | provides snapshots | drives it | — |
| Symbols, scopes, flow graph | — | — | binder |
| Assignability, inference, narrowing, evaluation | — | — | solver |
| Type display / printer | — | — | solver printer |

The hard rule: a scanner or parser change may alter *which tokens or nodes* are
produced and *which syntactic diagnostic* fires, but it must never reach forward
into type semantics. If a front-end fix "needs a type", the question is in the
wrong layer.

---

## Part 1 — The Scanner (`crates/tsz-scanner`)

### Module map

| File | Role |
| --- | --- |
| `src/lib.rs` | `SyntaxKind` enum (token kinds `0..=166`), token-class predicates, `text_to_keyword` / `keyword_to_text_static` / `punctuation_to_text_static`, ECMAScript identifier predicates |
| `src/scanner_impl.rs` | `ScannerState`, `TokenFlags`, the main `scan()` loop, position helpers, `save_state`/`restore_state` snapshots, diagnostics buffer |
| `src/scanner_impl/identifiers.rs` | identifier scanning, Unicode escape (`\uXXXX`, `\u{…}`) handling, private identifiers |
| `src/scanner_impl/numbers.rs` | numeric and BigInt literal scanning, separators, leading-zero/legacy-octal rules |
| `src/scanner_impl/strings.rs` | string literal scanning and escape decoding |
| `src/scanner_impl/templates.rs` | template literal scanning + `re_scan_template_token` |
| `src/scanner_impl/slash.rs` | `re_scan_slash_token` (regex literal re-tokenization) |
| `src/scanner_impl/jsx.rs` | JSX text/identifier/attribute scanning and JSX rescans |
| `src/scanner_impl/jsdoc.rs` | JSDoc comment-token scanning |
| `src/rescan.rs` | contextual rescans for `>`, `*=`, `<`, `?`, `#`, and invalid-identifier rescue |
| `src/char_codes.rs` | named character-code constants (`CharacterCodes`) used instead of magic numbers |

### `SyntaxKind`: the token alphabet

`SyntaxKind` is `#[repr(u16)]` and enumerates *only* the values the scanner can
produce, `Unknown = 0` through `DeferKeyword = 166`
(`crates/tsz-scanner/src/lib.rs`). AST node kinds are a disjoint, higher numeric
range owned by the parser (`syntax_kind_ext`, starting at `167`), so a single
`u16` distinguishes "is this a token or a tree node?" by magnitude. The enum
order is load-bearing: classification predicates are pure range checks against
named boundary constants rather than match arms. For example
`token_is_keyword` is `t >= BreakKeyword && t <= DeferKeyword`,
`token_is_reserved_word` is `BreakKeyword..=WithKeyword`,
`token_is_strict_mode_reserved_word` is `ImplementsKeyword..=YieldKeyword`, and
`token_is_assignment_operator` is `EqualsToken..=CaretEqualsToken`. `try_from_u16`
guards the boundary so a node-kind `u16` can never be mistaken for a token.

Keyword recognition is a single `match` on the string slice in
`text_to_keyword`; `string_to_token` falls back to `SyntaxKind::Identifier`. Note
that `text_to_keyword` returns the keyword kind for *all* keywords including
contextual ones (`type`, `async`, `of`, `satisfies`, `accessor`, `defer`…). The
scanner does not know whether `type` is contextually a keyword here — it always
classifies `type` as `TypeKeyword`, and the parser decides from context whether
that is the `type` keyword or an identifier named `type`.

### `ScannerState`: zero-copy mutable cursor

The scanner is a mutable cursor, not a pull-iterator of owned tokens
(`scanner_impl.rs`, `struct ScannerState`). Its key fields:

```
source: Arc<str>          // the whole file, shared, never re-copied
pos: usize                // byte offset of the scan cursor (end of current token)
full_start_pos: usize     // start of leading trivia for the current token
token_start: usize        // start of the token text itself (after trivia)
token: SyntaxKind         // the current token kind
token_value: String       // decoded value (only when it differs from the slice)
token_flags: u32          // bitset of TokenFlags
token_atom: AstAtom       // interned handle for identifier/keyword tokens
interner: Interner        // per-file string interner (later moved to the arena)
skip_trivia: bool         // whether scan() loops past whitespace/comments
allow_astral_identifier_chars: bool  // target-gated (ES2015+) astral id support
```

All positions are **byte** offsets. For the ASCII-only files that dominate
TypeScript this equals the character index; for multi-byte UTF-8, helpers like
`char_code_unchecked`, `char_len_at`, and `char_code_at` keep the cursor on char
boundaries (`scanner_impl.rs`). `char_code_unchecked` has a fast path: ASCII
bytes (`< 128`) are returned directly as their code, and only non-ASCII bytes pay
for a UTF-8 decode (with a char-boundary back-scan as a safety net).

`TokenFlags` (a `#[repr(u32)]` bitset) records out-of-band facts about the token
that the kind alone cannot express: `PrecedingLineBreak` (essential for
automatic semicolon insertion), `Unterminated`, `UnterminatedAtEof` (used to
split TS1126 "Unexpected end of text" from TS1002 "Unterminated string literal"),
`Scientific`, `Octal`, `HexSpecifier`, `BinarySpecifier`, `ContainsSeparator`,
`UnicodeEscape`, `ContainsInvalidEscape`, `ExtendedUnicodeEscape`,
`ContainsLeadingZero`, and `PrecedingJSDocComment`. The parser reads these flags
to make grammar and diagnostic decisions (for example ASI consults
`has_preceding_line_break`).

### The `scan()` loop

`scan()` (`scanner_impl.rs`, `pub fn scan`) resets per-token state — clears
`token_flags`, `token_value`, and `token_atom` — then runs a `loop` that reads
the byte at `pos` and dispatches on it. The `loop` exists because trivia is
skipped *in place*: when `skip_trivia` is set and the cursor is on whitespace, a
newline, a BOM, or a comment, the loop `continue`s rather than returning a trivia
token, so the next non-trivia token is what `scan()` ultimately yields. Newlines
always set `TokenFlags::PrecedingLineBreak` first, even while being skipped,
because that bit must survive onto the following real token.

The dispatch is a large `match ch` over `CharacterCodes` constants:

- **Single-char punctuation** (`{`, `}`, `(`, `)`, `[`, `]`, `;`, `,`, `~`, `@`,
  `:`) advances one byte and returns immediately.
- **Multi-char operators** maximal-munch by peeking ahead. `!` widens to `!=`
  then `!==`; `=` widens to `==`, `===`, `=>`; `.` becomes `...`, or — when
  followed by a digit — defers to `scan_number()` for a leading-dot float like
  `.5`.
- **`/`** is special: it first checks for `//` (single-line comment) and `/*`
  (multi-line comment, which emits TS1010 `'*/' expected.` via `push_diag` if
  unterminated), then `/=`, and only otherwise a bare `SlashToken`. The scanner
  never produces a regex literal from `scan()`; division-vs-regex is resolved by
  the parser through `re_scan_slash_token` (see below).
- **`"` / `'`** call `scan_string`; **`` ` ``** calls `scan_template_literal`;
  **`#`** scans a `PrivateIdentifier` (or a bare `HashToken` if not followed by an
  identifier start); **digits** call `scan_number`; **`\`** begins a
  Unicode-escaped identifier.
- Conflict markers (`<<<<<<<`, `=======`, `>>>>>>>`, `|||||||`) are detected for
  `=`, `<`, `>`, `|` via `is_conflict_marker_trivia` and reported as TS1185.

Anything that is neither operator, literal, nor identifier start falls through to
`SyntaxKind::Unknown`, which is the scanner's way of handing an "invalid
character" up to the parser without aborting; the parser then emits TS1127 and
skips it (see `parse_expected` below).

### Identifiers and interning

When the cursor is on an identifier-start byte, `scan_identifier`
(`scanner_impl/identifiers.rs`) advances over identifier-part bytes, then:

1. classifies the slice with `text_to_keyword` (so `class` becomes `ClassKeyword`,
   `foo` becomes `Identifier`);
2. interns the slice into `token_atom` via `self.interner.intern(text_slice)`;
3. deliberately leaves `token_value` empty.

This is the zero-allocation fast path: an identifier token carries only a `u32`
atom and its source span. `get_token_value_ref` resolves the text on demand —
first from the interned atom, then from `token_value` (for strings/templates
where the decoded value differs from the raw slice), and finally as a raw source
slice (`scanner_impl.rs`, `get_token_value_ref`).

The interner (`crates/tsz-common/src/interner/mod.rs`, `struct Interner`) is a
per-file `FxHashMap<Arc<str>, AstAtom>` plus a `Vec<Arc<str>>`. `AstAtom(0)` is
reserved for the empty string (`AstAtom::NONE`), so a non-`NONE` atom always
names a real string. `intern_common` can pre-seed the table with ~110 common
keywords and identifiers (`COMMON_STRINGS`) for cache locality. Critically, the
**parser later takes ownership of this interner** (`arena.set_interner(...)` in
`parse_source_file`) so identifier text stays resolvable long after the scanner
is gone.

`AstAtom` is a distinct type from the program-wide `Atom` minted by the
concurrent `ShardedInterner`. The two namespaces use *incompatible* encodings —
`AstAtom` indices are sequential within one file's arena, while `Atom` packs a
shard index into its low bits (`(local_index << 6) | shard`). Comparing or
resolving an `AstAtom` against an `Atom` is a logic error that the type system
turns into a compile error; the only correct bridge is to resolve to a string and
re-intern. This is exactly the [interner/`DefId`](solver-types-intern-def.md)
boundary that keeps per-file syntax separate from the program-wide type universe.

Unicode escapes mid-identifier (e.g. `foo`) force `scan_identifier` into an
allocation path (`continue_identifier_with_escapes`) that decodes the escape and
sets `TokenFlags::UnicodeEscape`. `peek_unicode_escape` validates `\uXXXX` and
`\u{…}` forms without consuming, and `scan_unicode_escape_value` consumes them.
Astral (non-BMP) identifier characters are gated by `is_identifier_start` /
`is_identifier_part`, which AND the Unicode tables with `allow_astral_identifier_chars`
— set from the language version in `set_language_version`
(`allow_astral_identifier_chars = language_version.supports_es2015()`). On ES5 an
astral identifier scans as `Unknown` and is only rescued in narrow contexts via
`re_scan_unknown_token_as_identifier_name`.

### Numbers, strings, templates, regex

Numeric scanning (`scanner_impl/numbers.rs`) handles decimal, hex (`0x`), binary
(`0b`), octal (`0o`), legacy octal, BigInt (`n` suffix), scientific notation, and
numeric separators (`1_000`). It records the relevant `TokenFlags` and tracks the
first invalid separator position so the parser can locate the diagnostic; leading
zeros set `ContainsLeadingZero` for the TS1121/TS1489 family. The scanner records
the *syntactic* facts; whether a literal is a valid numeric *type* is the checker's
problem.

String scanning (`scanner_impl/strings.rs`) decodes escapes into `token_value` and
flags `ContainsInvalidEscape`/`Unterminated`/`UnterminatedAtEof`. Template scanning
(`templates.rs`) produces `NoSubstitutionTemplateLiteral`, `TemplateHead`,
`TemplateMiddle`, and `TemplateTail` and exposes `re_scan_template_token` so the
parser can re-tokenize the `}` that closes a `${…}` span back into a
`TemplateMiddle`/`TemplateTail`.

### Contextual rescans

A *rescan* re-interprets the most recently scanned token using context the parser
has since acquired. Each rescan in `src/rescan.rs` documents a precondition on
`self.token` and is a no-op when the precondition fails. The full set:

| Rescan | Precondition token | Produces |
| --- | --- | --- |
| `re_scan_greater_token` | `GreaterThanToken` | `>=`, `>>`, `>>>`, `>>=`, `>>>=` |
| `re_scan_asterisk_equals_token` | `AsteriskEqualsToken` | splits to `*` + `=` |
| `re_scan_less_than_token` | `LessThanToken` | `</` (`LessThanSlashToken`) in JSX |
| `re_scan_question_token` | `QuestionToken` | `?.`, `??`, `??=` |
| `re_scan_hash_token` | `HashToken` | `PrivateIdentifier` |
| `re_scan_invalid_identifier` | `Unknown` (non-empty value) | identifier/keyword rescue |
| `re_scan_unknown_token_as_identifier_name` | `Unknown` | raw astral-id rescue |
| `re_scan_slash_token` (`slash.rs`) | `SlashToken` / `SlashEqualsToken` | `RegularExpressionLiteral` |
| `re_scan_template_token` (`templates.rs`) | template tokens | `TemplateMiddle`/`TemplateTail` |
| `re_scan_jsx_token` (`jsx.rs`) | any | JSX text / angle / brace tokens |

The `>>` case is the canonical example of why rescans exist. In
`Array<Map<string, number>>` the scanner greedily lexes `>>` as a single
right-shift token; the parser, knowing it is closing two type-argument lists,
must split it. The scanner can't split it eagerly because in `a >> b` the `>>`
really is a shift. So the parser keeps the greedy token and asks
`re_scan_greater_token` to widen the *next* `>` only when grammar requires it.
`re_scan_slash_token` is the dual: division and regex share the `/` byte, so the
parser decides from expression context whether to re-tokenize a `SlashToken` into
a `RegularExpressionLiteral`, including flag validation (`g`/`i`/`m`/`s`/`u`/`v`/
`y`/`d`, duplicate detection, and the incompatible `u`+`v` pair) recorded as
`RegexFlagError`s.

### JSX scanning

JSX needs a different lexical grammar — text content is mostly opaque, identifiers
may contain hyphens (`data-testid`), and `{` opens an expression hole. The parser
flips into JSX mode by calling `re_scan_jsx_token` (`scanner_impl/jsx.rs`), which
resets `pos` to `full_start_pos` (so JSX text includes leading whitespace, matching
tsc's `pos = tokenStart = fullStartPos`) and retroactively drops any scanner
diagnostics emitted in the rescanned range (a `7x`-style false positive produced in
JS mode is invalid once the span is known to be JSX text). `scan_jsx_identifier`
extends an identifier across `-` hyphens, and `scan_jsx_attribute_value` /
`re_scan_jsx_attribute_value` handle quoted attribute values.

### Scanner snapshots

`save_state` returns a `ScannerSnapshot` capturing `pos`, the three start
positions, `token`, `token_value`, `token_flags`, `token_atom`, separator state,
the regex-error list, and the *length* of `scanner_diagnostics`. `restore_state`
truncates `scanner_diagnostics` back to that length, so diagnostics emitted during
a failed lookahead are undone. This snapshot is the primitive on which all parser
lookahead and speculation is built.

---

## Part 2 — The Parser (`crates/tsz-parser`)

### The thin-AST representation

The central performance idea is in `crates/tsz-parser/src/parser/node.rs`: a
`Node` is a 16-byte `#[repr(C)]` header — `kind: u16`, `flags: u16`, `pos: u32`,
`end: u32`, `data_index: u32` — so four nodes fit in a 64-byte cache line (the
module docs note this is ~13x better cache locality than a 208-byte enum-per-node
design). `data_index == u32::MAX` (`Node::NO_DATA`) means "leaf token with no
payload"; otherwise it indexes into the **typed pool** selected by `kind`.

`NodeArena` (re-exported as `NodeArena`, backed by `NodeArenaInner`) holds:

- `nodes: Vec<Node>` — the headers, indexed by `NodeIndex`;
- `extended_info: Vec<ExtendedNodeInfo>` — parallel to `nodes`, carrying the
  parent pointer and other rarely-touched fields;
- the `interner` transferred from the scanner;
- ~90 typed data pools, one per AST family.

The pools are declared exactly once, in `node_pools.rs`'s `for_each_node_pool!`
table (`identifiers => IdentifierData`, `binary_exprs => BinaryExprData`,
`call_exprs => CallExprData`, `type_refs => TypeRefData`, … `source_files =>
SourceFileData`). That single macro table generates the `NodeArenaInner` struct
fields, the `NodeArenaPoolLengths` snapshot, the `clear` logic, and the
`pool_checkpoint`/`restore_pool_checkpoint` used by speculation — so a new node
family can only be added by editing the table, and none of the generated surfaces
can drift out of sync.

`NodeIndex` is a `u32` newtype (`crates/tsz-parser/src/parser/base.rs`) with a
`max + into_option` sentinel, so `NodeIndex::NONE` is `u32::MAX` and "optional
child" costs no extra bytes. `NodeList` is a `Vec<NodeIndex>` plus `pos`/`end` and
a `has_trailing_comma` flag. `TextRange` is a plain `{ pos, end }`.

### Bottom-up construction and the parent invariant

Children are created before their parents. A constructor reserves the index its
node *will* occupy with `reserve_parent()`, wires that index into its children's
`extended_info.parent` via `set_parent` / `set_parent_list`, then pushes the
parent through the `push_data_node!` macro (`node_arena/mod.rs`). The macro pushes
the typed payload, the header, and a default `ExtendedNodeInfo` in lockstep and
`debug_assert`s that the reserved index still equals where the node landed — so
the `parent == nodes.len()` invariant is enforced uniformly rather than
open-coded per constructor.

Identifier text resolution illustrates the front end's defensive identity model.
`resolve_identifier_text` (`node_arena/mod.rs`) prefers the parsed
`escaped_text` spelling and only falls back to resolving `data.atom` through the
interner — so a stale or cross-file atom can never silently corrupt a non-ASCII
name (a regression that incremental reparse, which re-syncs the interner, once
exposed).

### `ParserState`: driver and recovery ledger

`ParserState` (`crates/tsz-parser/src/parser/state.rs`) owns the `scanner`, the
`arena`, the `current_token`, the `parse_diagnostics` vector, and a large set of
recovery bookkeeping fields. Two field families matter for understanding parser
behavior:

- **`context_flags: u32`** is a bitset of parsing-context predicates such as
  `CONTEXT_FLAG_ASYNC`, `CONTEXT_FLAG_GENERATOR`, `CONTEXT_FLAG_DISALLOW_IN`
  (for `for(...)` initializers), `CONTEXT_FLAG_IN_CLASS`,
  `CONTEXT_FLAG_AMBIENT`, `CONTEXT_FLAG_IN_DECORATOR`,
  `CONTEXT_FLAG_DISALLOW_CONDITIONAL_TYPES` (inside `infer T extends U`), and many
  more. These flip the grammar locally: e.g. `await` is an identifier outside an
  async context but an `AwaitExpression` keyword inside one, and `in` is disallowed
  as a binary operator in a for-initializer.
- **One-shot recovery flags** — `suppress_object_literal_comma_once`,
  `abort_object_literal_recovery_once`, `deferred_module_close_braces`,
  `pending_array_binding_tail_recovery`, `recovered_template_literal_property_in_object`,
  and dozens more. Each encodes a single tsc-parity recovery decision made at one
  site and consumed at another, with a comment describing the exact malformed
  input it tracks. They exist because matching tsc's *error recovery shape*
  (which token a cascade error lands on) is as much a parity requirement as
  matching the happy-path AST.

The parser is created with `ParserState::new(file_name, source_text)` (or
`new_with_language_version`). It passes `source_text` straight into the scanner
without cloning (zero-copy) and pre-sizes the arena with `source_text.len() / 20`
estimated nodes.

### Token cursor

`token()` returns `current_token`; `next_token()` is the one-line engine:
`self.current_token = self.scanner.scan()` (`state.rs`). Position helpers
`token_pos()`, `token_full_start()`, and `token_end()` read the scanner's byte
offsets and convert to `u32`. `parse_optional(kind)` consumes the token if it
matches and returns whether it did; `parse_expected(kind)` consumes it or emits a
diagnostic (detailed under recovery).

### The top-level walk: `parse_source_file`

`parse_source_file` (`crates/tsz-parser/src/parser/state_statements.rs`) is the
entry point that the binder's `lib_loader` and the project pipeline call. Its
sequence:

```
parse_source_file()
├─ scanner.scan_shebang_trivia()         // skip a leading #! line
├─ next_token()                          // prime current_token
├─ parse_source_file_statements()        // the statement loop (with stray-brace recovery)
├─ get_comment_ranges(source)            // cache comment spans once (O(N), reused by LSP hover)
├─ drain scanner diagnostics into parse_diagnostics  // e.g. conflict markers TS1185
├─ parse_diagnostics.sort_by(compare)    // tsc compareDiagnostics order
├─ add_token(EndOfFileToken, ...)
├─ arena.set_interner(scanner.interner().clone())  // hand the interner to the arena
└─ arena.add_source_file(..., SourceFileData { statements, comments, text, ... })
```

`SourceFileData` (`node.rs`) carries the statement list, the EOF token, the file
name, the shared `Arc<str>` source text, the language version/variant/script kind,
`is_declaration_file`, and the cached `comments` vector. The diagnostic sort uses
`ParseDiagnostic::compare`, a total order over `(start, length, code, message)`
mirroring tsc's `compareDiagnostics`, so position ties resolve deterministically
regardless of the order scanner- and parser-side errors were produced.

### Worked example: `const x = a < b ? c : d;`

Tracing this statement names the real functions that run:

1. `parse_source_file` primes the cursor; `next_token()` scans `const` →
   `ConstKeyword`.
2. The statement loop dispatches a variable statement. The variable-declaration-list
   path consumes `const`, then parses the declarator: the binding name `x` becomes
   an identifier node (`add_identifier` with `IdentifierData` whose `atom` came from
   `scanner.get_token_atom()`).
3. `parse_expected(EqualsToken)` consumes `=`.
4. The initializer is an assignment-expression parse. It descends through binary
   precedence to parse `a < b`. Here the scanner produced `a` (`Identifier`), then
   `<` (`LessThanToken`). The binary-expression parser treats `<` as the
   relational operator and builds a `BinaryExprData` for `a < b`.
5. The result `a < b` is the condition of a conditional expression: `?` (rescanned
   if needed by `re_scan_question_token`, here a bare `QuestionToken`), the `c`
   branch, `:` , the `d` branch → a `ConditionalExprData` node.
6. `parse_semicolon` consumes `;`. The variable statement node is finished with
   its `pos`/`end` spanning `const`…`;`.

Every node is appended to `arena.nodes` with its payload in the matching pool, and
parent pointers are set bottom-up. No type was computed: the parser does not know
or care whether `a < b` is a valid comparison.

### Speculation and lookahead

Many grammar decisions in TypeScript are not LL(k) for fixed k. The two
mechanisms:

- **Cheap lookahead** — `look_ahead_is` (`parser/parse_rules/utils.rs`) snapshots
  *only the scanner*, scans one token, runs a predicate, and restores. Variants
  include `look_ahead_is_on_same_line` (returns false across a line break so ASI is
  respected), `look_ahead_is_async_declaration`, `look_ahead_is_module_declaration`,
  and `look_ahead_is_type_alias_declaration`. These answer questions like "is
  `async` here followed by `function`?" or "is `type` here starting a type alias,
  or is it just an identifier named `type`?".

- **Full speculation** — when a decision requires actually running a `parse_*`
  routine (which mutates the scanner, current token, context flags, diagnostics,
  the arena, *and* the one-shot recovery flags), the cheap snapshot is not enough.
  `speculation.rs` defines `ParserCheckpoint`, capturing every mutable field
  including `arena.nodes.len()`, `arena.extended_info.len()`, the full
  `NodeArenaPoolLengths` (`arena.pool_checkpoint()`), the diagnostics length, the
  scanner snapshot, the scanner-diagnostics high-water mark, and the recovery
  flags. `speculate(body)` runs `body`, then *always* rolls back via
  `restore_speculation_checkpoint`; callers use it for "would this parse?" probes.
  Restoring the pool lengths is what prevents a failed speculation from leaving
  orphaned identifier/`type_ref` entries that would inflate peak memory on
  generic-heavy files.

The canonical use is arrow-function disambiguation: `(a, b)` could begin a
parenthesized expression or an arrow parameter list, and only the lookahead for a
following `=>` (or a `:` return annotation) settles it.

### Error recovery and diagnostics

The parser produces *syntactic* diagnostics (the TS1xxx family) as
`ParseDiagnostic { start, length, message, code }`. Three pillars:

1. **Cascade suppression.** `should_report_error` (`state.rs`) returns true for the
   first error and thereafter only when the cursor has advanced more than
   `ERROR_SUPPRESSION_DISTANCE` (= 3) tokens past `last_error_pos`. This mimics
   tsc's behavior of not piling "';' expected" on top of "')' expected" for a
   single missing token. Specific code combinations that tsc *does* emit at nearby
   positions get explicit escape hatches (e.g.
   `last_error_was_leading_zero_at_other_pos`,
   `last_error_was_element_access_missing_argument_at_other_pos`), which check the
   last diagnostic's code and position to decide whether the companion error
   should still fire.

2. **Invalid-character handling.** A scanner `Unknown` token surfaces in
   `parse_expected` as TS1127 "Invalid character"; the parser emits it, advances
   past the byte, and re-checks for the expected token — exactly mirroring tsc's
   `scanError` callback that consumes the bad character during scanning.

3. **Forced emission and dedup.** `parse_expected` has a `force_emit` path for the
   high-value missing-delimiter cases (a missing `)` before `{`/`}`/identifier,
   missing `}` or `</` at EOF) that bypasses the distance suppression, matching
   tsc's `parseExpected` which always emits TS1005 at the current position unless an
   error already exists at the *exact* same start. `last_error_was_unterminated_literal`
   suppresses cascades after TS1002/TS1160/TS1161/TS1126, because an unterminated
   literal consumes past closing delimiters and the follow-on "missing )" is noise.
   `parse_error_at_current_token` even peeks one token ahead to suppress a spurious
   "')' expected." when the next token is a `;`.

Scanner-side diagnostics (conflict markers TS1185, unterminated block comment
TS1010, numeric-literal errors, regex flag errors) are accumulated in the
scanner's own buffer during scanning and merged into `parse_diagnostics` at the
end of `parse_source_file`. The `scanner_diagnostics_high_water_mark` field lets
`parse_error_at` reproduce tsc's `parseErrorAtPosition` "lastError" dedup across
the scanner/parser boundary, so a scanner TS1124 at a position correctly
suppresses a parser TS1005 the parser would otherwise emit at the same spot.

The parser also offers misspelling help: `spelling.rs` implements tsc's
`viableKeywordSuggestions` (keywords of length > 2) and Levenshtein-distance
matching for the "did you mean a keyword?" path in
`parseErrorForMissingSemicolonAfter`.

### Recursion guard

Deeply nested input (long `a.b.c.d…` chains, nested generics, nested parentheses)
could overflow the native stack. `enter_recursion` (`state.rs`) increments
`recursion_depth` and, once it reaches `MAX_PARSER_RECURSION_DEPTH` (= `1_000`,
`crates/tsz-common/src/limits/mod.rs`), emits a "Maximum recursion depth exceeded"
diagnostic and returns `false` so the caller bails out gracefully instead of
crashing. `exit_recursion` decrements with a saturating subtraction. This is a
*fuel* guard: a flat ceiling on syntactic nesting, distinct from the solver's
semantic recursion budgets.

### TypeScript- and JSX-specific parsing

The parser, not the scanner, owns the TypeScript-vs-JavaScript grammar split. The
state modules are sharded by family so each grammar area is locatable:

| Module(s) | Grammar area |
| --- | --- |
| `state_statements*.rs` | statements, statement-list recovery, keyword statements |
| `state_declarations*.rs` | functions, classes, interfaces, type aliases, enums, modules, exports |
| `state_expressions*.rs` | binary/unary/call/member/arrow/literal/object/regex expressions |
| `state_types*.rs`, `state_type_parameters.rs` | type annotations, advanced types, type parameters |
| `state_types_jsx*.rs` | JSX elements and the JSX type surface |
| `state_variable_declarations.rs` | variable-declaration lists and binding |
| `state_import_attributes.rs` | `import ... with { ... }` attributes |
| `state_*_recovery.rs`, `state/recovery.rs` | family-specific error recovery |
| `parse_rules/utils.rs` | shared lookahead and token-classification helpers |

TypeScript-only constructs — type annotations (`x: T`), generics (`<T>`),
`as`/`satisfies` assertions, `interface`/`type`/`enum`/`namespace` declarations,
parameter properties, `declare`, definite-assignment `!`, and decorators — are all
parsed here into the `syntax_kind_ext` node kinds (`TYPE_REFERENCE = 184`,
`CONDITIONAL_TYPE = 195`, `MAPPED_TYPE = 201`, …). The parser builds the *shape* of
a `MappedType` or `ConditionalType`; it never evaluates one — that is the
[solver's evaluation kernel](solver-evaluation.md).

JSX parsing alternates between normal expression parsing and JSX scanning. After
an opening `<`, the parser drives JSX-mode tokens via `re_scan_jsx_token` for text
content and `scan_jsx_identifier` for tag/attribute names, parses `{…}` holes back
in expression mode, and matches closing tags — emitting TS17008 "JSX element has no
corresponding closing tag" when they are unbalanced. Whether JSX is even enabled,
and how a JSX element type-checks, is decided later in
[checker JSX handling](checker-jsx-properties-accessors-enums.md).

### Trivia and comments

Whitespace, newlines, and comments are *trivia*: in the default `skip_trivia`
mode the `scan()` loop consumes them without emitting tokens, preserving only the
`PrecedingLineBreak` flag and the `full_start_pos`/`token_start` gap. Comments are
not stored per node. Instead, `parse_source_file` computes
`tsz_common::comments::get_comment_ranges` **once** over the whole source and
stores the `Vec<CommentRange>` on `SourceFileData.comments`, so later consumers
(LSP hover, JSDoc, documentation) do a range lookup instead of an O(N) rescan per
request. JSDoc comment *tokens* (as opposed to ranges) have dedicated scanning in
`scanner_impl/jsdoc.rs` for the paths that need structured JSDoc.

---

## Caches and invariants

- **Per-file string interner.** `Interner` maps `Arc<str> → AstAtom`. It is owned
  by the scanner during lexing and *moved into the arena*
  (`arena.set_interner(scanner.interner().clone())`) at the end of
  `parse_source_file`, so identifier text stays resolvable for the binder, LSP, and
  diagnostics. Invariant: `AstAtom(0)` is the empty string; a non-`NONE` atom names
  a real interned string. Incremental reparse
  (`parse_source_file_statements_from_offset`) re-syncs the interner for the same
  reason — without it, atoms minted in the suffix would resolve to `""`.
- **Atom-namespace isolation.** `AstAtom` (per-file) and `Atom` (program-wide,
  sharded) are distinct types with incompatible encodings; mixing them is a compile
  error. Cross-namespace use must resolve-then-re-intern, never copy the raw `u32`.
  This is the front end's edge of the [intern/`DefId`](solver-types-intern-def.md)
  identity model.
- **Comment-range cache.** Computed once per file in `parse_source_file`, stored on
  `SourceFileData.comments`, never recomputed.
- **Scanner snapshots are state, not caches.** `save_state`/`restore_state` and the
  parser's `ParserCheckpoint` are roll-back primitives. The invariant that makes
  rollback correct: every field a speculative `parse_*` can mutate is in the
  checkpoint — scanner state, current token, context flags, last-error position,
  diagnostics length, arena `nodes`/`extended_info` lengths, **all** typed-pool
  lengths, the scanner-diagnostics high-water mark, and the one-shot recovery
  flags. `restore_speculation_checkpoint` truncates the pools and arena back to
  their captured lengths.
- **Parent-pointer invariant.** Nodes are built bottom-up; `reserve_parent` +
  `push_data_node!` enforce `parent == nodes.len()` with a `debug_assert`. A child's
  `extended_info.parent` is always a strictly smaller `NodeIndex` than its parent's.
- **Thin-node invariant.** `data_index == u32::MAX` ⇔ the node is a payload-less
  leaf; otherwise `data_index` is a valid index into the pool selected by `kind`.
  The `node_pools.rs` macro table guarantees the pool set, the snapshot struct, and
  the clear/checkpoint logic stay mutually consistent.

---

## Edge cases and tsc parity

- **`>>` vs nested generics.** The scanner munches `>>`/`>>>` greedily;
  `re_scan_greater_token` re-splits only when the parser is closing type-argument
  lists. Eager splitting would break `a >> b`.
- **Division vs regex.** `/` always scans as comment / `SlashToken` / `SlashEqualsToken`;
  the parser re-tokenizes to a `RegularExpressionLiteral` via `re_scan_slash_token`
  based on expression context, validating flags and the incompatible `u`+`v` pair.
- **`?.` followed by a digit.** `re_scan_question_token` deliberately does *not*
  widen `?.` when the next char is a digit, because `a ? .5 : b` is a ternary whose
  true-branch is the float `.5`, not optional chaining (`rescan.rs`).
- **Unterminated literals at EOF.** `UnterminatedAtEof` lets the parser pick TS1126
  "Unexpected end of text" over TS1002 "Unterminated string literal", and
  `last_error_was_unterminated_literal` suppresses the cascade of missing-delimiter
  errors that an unterminated literal would otherwise trigger.
- **Invalid characters.** Truly unrecognized bytes scan as `Unknown`; the parser
  emits TS1127, skips the byte, and retries — matching tsc's scanner `scanError`
  consume-and-continue behavior rather than aborting.
- **Astral identifiers under ES5.** Non-BMP identifier characters are gated by the
  language version; on ES5 they scan as `Unknown` and are only rescued in narrow
  positions by `re_scan_unknown_token_as_identifier_name`, while braced
  `\u{…}` escapes remain invalid recovery tokens (their raw text starts with `\`).
- **Contextual keywords.** The scanner classifies `type`, `async`, `of`,
  `satisfies`, `accessor`, etc. as keyword kinds unconditionally; the parser uses
  `is_identifier_or_contextual_keyword` and lookahead to decide whether a given
  occurrence is the keyword or a plain identifier (e.g. a variable literally named
  `type`).
- **ASI and line breaks.** `PrecedingLineBreak` is propagated onto the token after
  skipped trivia, and `look_ahead_is_on_same_line` refuses to treat
  `namespace`/`module`/`type` followed by a line break as a declaration head, so the
  parser reproduces tsc's automatic-semicolon-insertion decisions.
- **Diagnostic ordering.** Scanner and parser diagnostics are merged and then sorted
  by `ParseDiagnostic::compare` (`(start, length, code, message)`), mirroring tsc's
  `compareDiagnostics`, so output is deterministic regardless of production order.
- **Conflict markers.** `<<<<<<<`/`=======`/`>>>>>>>`/`|||||||` runs are detected as
  TS1185 in both normal and JSX scanning modes, before the angle-bracket paths, so a
  merge-conflict marker inside JSX children is not mistaken for a nested tag.

---

## Where the front end hands off

The `SourceFileData` node (and the arena behind it, with its transferred interner)
is the front end's deliverable. From here:

- the [binder](binder.md) walks the AST to create symbols, scopes, hoisting, and
  the flow-graph skeleton — still with no type computation;
- the [checker](checker-context-and-state.md) orchestrates the AST, attaches source
  locations and diagnostics, and asks the solver for every semantic answer;
- the [solver](solver-relations.md) owns relations, inference, instantiation,
  evaluation, and narrowing;
- the [emitter](emitter.md) consumes the same syntax to produce JS/DTS.

For the full cross-layer trace from bytes to emitted output, see
[end-to-end timeline](end-to-end-timeline.md).
