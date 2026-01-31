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

## Investigation Findings (Jan 31, 2026)

### Current State
- Pass rate: ~47.9% (2000 tests)
- Key issue categories identified below

### Top Extra Errors (false positives we emit)
| Code | Count | Description | Root Cause |
|------|-------|-------------|------------|
| TS2339 | 267x | Property does not exist | Lib type resolution issues |
| TS2532 | 141x | Object is possibly undefined | Overly aggressive undefined checks |
| TS1005 | 111x | Expected token | Parser differences |
| TS1128 | 89x | Declaration expected | Parser differences |
| TS2322 | 80x | Type not assignable | Type comparison issues |

### Top Missing Errors (tsc emits but we don't)
| Code | Count | Description | Root Cause |
|------|-------|-------------|------------|
| TS2304 | 241x | Cannot find name | Symbol resolution for lib.d.ts types |
| TS2318 | 228x | Cannot find global type | Utility types (Partial, Required, etc.) not resolved |
| TS2584 | 124x | Cannot find name - need lib | Same as TS2304, lib-specific |
| TS2300 | 108x | Duplicate identifier | Duplicate detection gaps |
| TS2711/2712 | 127x | Cannot find Required/NonNullable | Global utility types |

### Root Cause Analysis

1. **Lib.d.ts Symbol Resolution**: Many failures stem from lib.d.ts types not being properly resolved during type checking. This affects:
   - Utility types: `Partial<T>`, `Required<T>`, `NonNullable<T>`, `Pick`, `Record`, etc.
   - Array methods: `.values()`, `.entries()` not found on arrays
   - Symbol: `Symbol.iterator` not resolving correctly

2. **Mapped Type / Keyof Evaluation**: When `keyof T` is used in mapped types (like `Partial`), the evaluation chain isn't working correctly. Tests show that inline `{ [K in "a"]: T }` works but `{ [K in keyof T]: T[K] }` doesn't produce expected type checking errors.

3. **Parser Differences**: TS1005/TS1128 errors suggest some parsing edge cases are handled differently.

### Recommended High-Impact Fixes

1. **Fix lib.d.ts symbol resolution** - would address TS2304, TS2318, TS2584, TS2711, TS2712 (estimated 500+ test impact)
2. **Fix mapped type evaluation with keyof** - would fix utility type behavior
3. **Review TS2532/TS2339 false positives** - likely overly strict undefined/property checks

### Low-Hanging Fruit Categories
These categories have 0% pass rate and may have simple fixes:
- `asyncGenerators` (0/3)
- `classBody` (0/2)
- `staticIndexSignature` (0/7)
- `indexMemberDeclarations` (0/4)
- `binaryAndOctalIntegerLiteral` (0/7)
- `arbitraryModuleNamespaceIdentifiers` (0/4)