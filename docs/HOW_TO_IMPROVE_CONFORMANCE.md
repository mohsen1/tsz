# How to Improve Conformance

> **Important**: Read `conformance-reality-check.md` first for realistic expectations.
> Conformance improvements require deep architectural understanding, not quick fixes.

## Quick Start

```bash
# Download cache from GitHub (fastest) and run tests
./scripts/conformance.sh all

# Or just download the cache
./scripts/conformance.sh download

# Run a quick conformance check
./scripts/conformance.sh run --max 100
```

## Related Documentation

Before diving in, review these documents:

- **`conformance-reality-check.md`** - Honest assessment of complexity (READ THIS FIRST)
- **`conformance-analysis-slice3.md`** - Baseline metrics and failure patterns
- **`conformance-fix-guide.md`** - Step-by-step implementation workflow
- **`conformance-work-session-summary.md`** - Detailed investigation examples

**Key takeaway**: Budget 1-2 days per test fix. There are no "quick wins."

## Finding Low-Hanging Fruits

### The `analyze` Command (Recommended)

The fastest way to find wins is the `analyze` command. It categorizes every failing test and ranks them by impact:

```bash
# Analyze all tests
./scripts/conformance.sh analyze

# Analyze a slice (e.g., slice 1 of 4)
./scripts/conformance.sh analyze --offset 0 --max 3101

# Filter by category
./scripts/conformance.sh analyze --category false-positive  # Tests where we emit errors but tsc doesn't
./scripts/conformance.sh analyze --category all-missing      # Tests where tsc emits errors but we don't
./scripts/conformance.sh analyze --category close            # Tests within 1-2 error codes of passing
./scripts/conformance.sh analyze --category wrong-code       # Both emit errors but different codes

# Show more results per section
./scripts/conformance.sh analyze --top 50
```

The analysis output includes:

1. **Summary** — total counts per category plus the most impactful error codes to fix
2. **False Positives** — tests where tsc says no errors but we emit errors (fixing these = instant passing tests)
3. **All Missing** — tests where tsc expects errors but we emit nothing (need new diagnostics)
4. **Close to Passing** — tests that differ by only 1-2 error codes (easiest individual fixes)
5. **Top error codes** — ranked by how many tests they affect, separately for false-positives, all-missing, and wrong-code

### Reading the Analysis Output

```
ANALYSIS SUMMARY
  Total failing tests analyzed: 855
  False positives (expected=[], we emit errors):  308   ← Fix these for instant wins
  All missing (expected errors, we emit none):    239   ← Need new diagnostic checks
  Wrong codes (both have errors, codes differ):   308   ← Closer but not matching
  Close to passing (diff <= 2 codes):             97    ← Easiest individual fixes

  Top false-positive error codes (fix = instant wins):
    TS2339: 72 tests     ← Property-does-not-exist false positives
    TS2345: 68 tests     ← Argument-not-assignable false positives
    TS2322: 62 tests     ← Type-not-assignable false positives
```

**Priority order:**
1. Fix **false-positive error codes** with highest count — each fix can flip many tests to PASS
2. Fix **close-to-passing** tests — small targeted fixes
3. Implement **missing diagnostics** with highest count — adds new PASS tests

### Step-by-Step Workflow

```bash
# 1. Run analyze to find what to fix
./scripts/conformance.sh analyze --offset 0 --max 3101

# 2. Pick a high-impact error code (e.g., TS2339 with 72 false-positive tests)
#    Find specific tests to investigate
./scripts/conformance.sh analyze --category false-positive | grep TS2339

# 3. Compare tsz vs tsc on a specific test
./.target/release/tsz TypeScript/tests/cases/compiler/someTest.ts 2>&1
npx tsc --noEmit --pretty false TypeScript/tests/cases/compiler/someTest.ts 2>&1

# 4. Fix the compiler code

# 5. Rebuild and verify
cargo build --release -p tsz-cli --bin tsz
./scripts/conformance.sh run --offset 0 --max 3101

# 6. Run unit tests
cargo nextest run -p tsz-checker -p tsz-solver

# 7. Commit
```

### Other Useful Commands

```bash
# Run with specific error code filter
./scripts/conformance.sh run --error-code 2339 --verbose --max 50

# Run with verbose to see per-test expected/actual codes
./scripts/conformance.sh run --verbose --max 50

# Filter by test name pattern
./scripts/conformance.sh run --filter "strict"

# Run full conformance (no limits)
./scripts/conformance.sh run
```

### Slicing for Parallel Work

Split the full test suite across multiple agents/workers:

```bash
# Get total test count
./scripts/conformance.sh run --max 1 2>&1 | grep "Found.*test files"

# Calculate slices (e.g., 4 slices for ~12000 tests = ~3000 each)
TOTAL=12404
QUARTER=$((($TOTAL + 3) / 4))  # 3101

# Run each slice
./scripts/conformance.sh analyze --offset 0 --max $QUARTER           # Slice 1
./scripts/conformance.sh analyze --offset $QUARTER --max $QUARTER    # Slice 2
./scripts/conformance.sh analyze --offset $((QUARTER*2)) --max $QUARTER  # Slice 3
./scripts/conformance.sh analyze --offset $((QUARTER*3)) --max $QUARTER  # Slice 4
```

## Fix Complexity Guide (Updated Based on Real Investigation)

⚠️ **These ratings are based on actual investigation** (see `conformance-reality-check.md`).

| Error Range | Type | Actual Complexity | Estimated Time |
|-------------|------|-------------------|----------------|
| **TS1xxx** | Parser/syntax | MEDIUM-HIGH (parser recovery) | 2-5 days |
| **TS2300** | Duplicate identifier | MEDIUM (declaration merging) | 1-3 days |
| **TS2304** | Symbol resolution | MEDIUM (imports, namespaces) | 1-3 days |
| **TS2339** | Property access | HIGH (type narrowing, private names) | 3-7 days |
| **TS2322/2345** | Type compatibility | HIGH (Solver subtype, flow analysis) | 3-7 days |
| **TS2454** | Used before assigned | HIGH (definite assignment, CFA) | 5-10 days |
| **TS18047/18048** | Possibly null/undefined | HIGH (flow analysis) | 5-10 days |
| **TS7006** | Implicit any | MEDIUM (contextual typing) | 1-3 days |

**Reality**: Even "simple" fixes require understanding multiple interacting systems.
See `conformance-fix-guide.md` for detailed workflow and examples.

## Cache Management

The conformance tests use a pre-generated cache of expected TSC errors. This cache is keyed by TypeScript version (submodule SHA).

### Downloading the Cache

The fastest way to get the cache is to download it from GitHub:

```bash
# Download from GitHub artifacts (requires gh CLI)
./scripts/conformance.sh download

# Or use the script directly
./scripts/download-tsc-cache.sh
```

Requirements:
- GitHub CLI (`gh`) - install with `brew install gh`
- `jq` - install with `brew install jq`
- Authenticated: `gh auth login`

### Generating the Cache Locally

If download is unavailable, generate locally:

```bash
# Generate with default workers (takes ~10-15 minutes)
./scripts/conformance.sh generate --no-cache

# Or with custom options
./.target/release/generate-tsc-cache \
  --test-dir ./TypeScript/tests/cases \
  --output ./tsc-cache-full.json \
  --workers 16
```

### How Cache Generation Works

For each test file:
1. Parse `@-directives` from comments (e.g., `// @target: es6`)
2. Create a `tsconfig.json` with those compiler options
3. Run `tsc --noEmit` with that configuration
4. Store the resulting error codes

### Supported Directive Types

- **Boolean**: `@strict: true`, `@noImplicitAny: false`
- **String/enum**: `@target: es6`, `@module: commonjs`, `@jsx: react`
- **List**: `@lib: es6,dom`, `@types: node,jest`
- **Numeric**: `@maxNodeModuleJsDepth: 2`

Test harness-specific directives (`@filename`, `@noCheck`, `@skip`, etc.) are filtered out.
See `docs/specs/TSC_DIRECTIVES.md` for the full list.

## GitHub Workflow

The cache is automatically generated when TypeScript version changes via `.github/workflows/tsc-cache.yml`.

**Triggers:**
- Push to main/rust with changes to TypeScript submodule
- Changes to `scripts/typescript-versions.json`
- Manual dispatch

**What it does:**
1. Checks if cache exists for current TypeScript SHA
2. If not, builds `generate-tsc-cache` and runs it
3. Stores cache in GitHub Actions cache and artifacts

**CI integration:**
The CI workflow (`ci.yml`) automatically restores the cache before running conformance tests.

## Verifying Results

If you suspect a cache mismatch:

```bash
# Compare outputs for a specific test
./.target/release/tsz <test_file> 2>&1
npx tsc --noEmit --pretty false <test_file> 2>&1
```

## Updating TypeScript Version

When updating the TypeScript submodule:

1. Update the submodule: `git submodule update --remote TypeScript`
2. Find the commit date and matching nightly version
3. Update `scripts/typescript-versions.json` with the SHA mapping
4. Push - the `tsc-cache.yml` workflow will generate the new cache
