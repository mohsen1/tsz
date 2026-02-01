# How to Improve Conformance

## Quick Start

```bash
# Run conformance tests (default: actionable summary)
./scripts/conformance/run.sh

# Verbose mode (full category breakdown)
./scripts/conformance/run.sh --verbose

# Investigate specific failures
./scripts/conformance/run.sh --filter=StrictMode --print-test
```

## Prioritization Strategy

The conformance runner now shows **"Highest Impact Fixes"** sorted by estimated test improvements.
Focus on items that fix **systematic issues** (one fix → many tests), not individual test cases.

### How to Pick What to Work On

1. **Run conformance** - Look at "Highest Impact Fixes" section
2. **Pick the top item** that you can realistically implement
3. **Use `--filter` + `--print-test`** to see specific failing tests in that category
4. **Look for patterns** - What's the common root cause?
5. **Implement the fix** - Usually in solver or checker
6. **Verify** - Re-run conformance to see test count improvement

## Current Priority Queue (Feb 1, 2026)

| Priority | Root Cause | Missing Codes | Est. Tests | Notes |
|----------|------------|---------------|------------|-------|
| **P0** | Lib utility type resolution | TS2318, TS2583, TS2584 | ~1600+ | Partial, Pick, Record don't resolve |
| **P1** | Null/undefined checks | TS18050, TS18047-49 | ~700+ | strictNullChecks not enforced |
| **P2** | Module resolution | TS2307, TS2792 | ~500+ | node/bundler mode issues |
| **P3** | Operator type checking | TS2365, TS2362-63 | ~450+ | Binary ops on wrong types |
| **P4** | Strict mode in classes | TS1210 | ~100+ | eval/arguments in class body |

### Notes on TS2304 (Cannot find name)
TS2304 appears ~1400x in missing errors but has **multiple root causes**:
- Many are symptoms of lib resolution issues (P0)
- Some are module resolution (P2)
- Some are actual symbol resolution bugs

Don't chase TS2304 directly - fix the underlying issues.

## Guidelines

- **ALWAYS USE ask-gemini.mjs** to understand TypeScript behavior before implementing
- Read docs/architecture and docs/walkthrough for implementation patterns
- Keep files under ~3000 lines - split if needed
- Commit frequently with semantic commit messages

## Historical Fixes (for reference)

| Commit | Description | Tests Gained |
|--------|-------------|--------------|
| `fd88ac6` | Env-aware property resolver for array/lib methods | **+65** |
| `5eb3441` | TS6133 unused declaration checking | **+59** |
| `174ddd9` | TS2300/TS2451/TS2392 duplicate identifier checking | **+16** |
| `50d2fbec` | null/undefined assignability in non-strict mode | **+9** |

Pattern: **Implementing missing error categories** yields the best results.