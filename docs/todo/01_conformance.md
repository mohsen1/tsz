# How to improve conformance

1. Make sure you sync with origin/main frequently and commit often
2. Run ./conformance/run.sh to get a good picture of what's failing
3. Pick the highest-impact task and execute it. Prefer "the biggest bang for the buck". Goal is to improve conformance pass rate
4. Use scripts/ask-gemini.mjs to ask a few questions from various angles to help you write code
5. Write code with full respect for the existing codebase and architecture. Always check with documentation and architecture.
6. Use ask-gemini for a code review.
7. Verify with `./conformance/run.sh`, mark done work in todo documents, commit and push.

## IMPORTANT:
- ALWAYS USE ask-gemini.mjs to ask questions. Non-negotiable.
- DO NOT ask questions from me (the user) - make autonomous decisions and start working immediately
- Read docs/architecture and docs/walkthrough to understand how things should be done
- Use Skills 
  - rust-analyzer-lsp
  - code-simplifier
  - rust-skills
- Do not let a file size get too large. If it does, split it into smaller files. Keep files under ~3000 lines.

---

## Investigation Findings (Feb 1, 2026)

### Current State
- Pass rate: **46.6%** (5,774/12,378 tests)
- Key issue categories identified below

### Top Extra Errors (false positives we emit)
| Code | Count | Description | Root Cause |
|------|-------|-------------|------------|
| TS2339 | 1759x | Property does not exist | Lib type resolution issues |
| TS1127 | 1563x | Invalid character | Scanner producing Unknown tokens |
| TS2322 | 1157x | Type not assignable | Type comparison issues |
| TS1005 | 1125x | Expected token | Parser/ASI differences |
| TS2345 | 1049x | Argument not assignable | Type checking issues |
| TS2304 | 794x | Cannot find name | Symbol resolution |
| TS7006 | 620x | Parameter implicitly has any | Type inference |
| TS1128 | 457x | Declaration expected | Parser differences |

### Top Missing Errors (tsc emits but we don't)
| Code | Count | Description | Root Cause |
|------|-------|-------------|------------|
| TS2304 | 1437x | Cannot find name | Symbol resolution for lib.d.ts |
| TS2318 | 1403x | Cannot find global type | Utility types not resolved |
| TS2322 | 719x | Type not assignable | Missing type checks |
| TS18050 | 679x | Value cannot be used here | Value/type distinction |
| TS2307 | 599x | Cannot find module | Module resolution |
| TS2300 | 564x | Duplicate identifier | Duplicate detection |
| TS2365 | 528x | Operator cannot be applied | Operator type checking |
| TS2339 | 498x | Property does not exist | Missing property checks |

### Scanner/Parser Investigation (Feb 1, 2026)

#### Fixes Applied
1. **Unicode identifier handling** - Changed `is_identifier_start()` to use `char::is_alphabetic()` for Unicode characters instead of simplistic `ch > 127`. This correctly rejects Unicode punctuation, symbols, and whitespace.

2. **Unicode identifier continuation** - Improved `is_identifier_part()` to use `char::is_alphanumeric()` and explicitly allow ZWNJ/ZWJ per ECMAScript spec.

3. **Statement recognition** - Added `UsingKeyword` and `AwaitKeyword` to `is_statement_start()` for ES2024 using declarations.

4. **JSDoc UTF-8 handling** - Fixed multi-byte character handling in JSDoc scanner using `char_len_at()`.

#### Remaining TS1127 Investigation
The 1563 extra TS1127 errors persist. Scanner produces `SyntaxKind::Unknown` in these scenarios:
- Backslash not followed by valid unicode escape (line ~862)
- Default case when no identifier start matches (line ~910)
- scan_jsdoc_token fallback (line ~2375)
- Backtick in scan_jsdoc_comment_text_token (line ~2413)

**Root cause still unknown** - Need to find specific test cases producing these errors to debug further.

### Root Cause Analysis

1. **Lib.d.ts Symbol Resolution**: Many failures stem from lib.d.ts types not being properly resolved during type checking. This affects:
   - Utility types: `Partial<T>`, `Required<T>`, `NonNullable<T>`, `Pick`, `Record`, etc.
   - Array methods: `.values()`, `.entries()` not found on arrays
   - Symbol: `Symbol.iterator` not resolving correctly

2. **Mapped Type / Keyof Evaluation**: When `keyof T` is used in mapped types (like `Partial`), the evaluation chain isn't working correctly.

3. **Parser/Scanner Differences**: TS1127/TS1005/TS1128 errors need deeper investigation.

### Recommended High-Impact Fixes

1. **Investigate TS1127 source** - Find specific tests producing "Invalid character" errors
2. **Fix lib.d.ts symbol resolution** - would address TS2304, TS2318 (estimated 1000+ test impact)
3. **Fix mapped type evaluation with keyof** - would fix utility type behavior
4. **Review TS2339/TS2322 balance** - appearing in both extra and missing lists

### Low-Hanging Fruit Categories
Categories with 0% pass rate that may have simple fixes:
- `StrictMode` (0/20)
- `forStatements` (0/3)
- `labeledStatements` (0/5 or 0/8)
- `RealWorld` (0/2)
- `Fuzz` (0/2)
- `MissingTokens` (0/2)