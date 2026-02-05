# How to Improve Conformance

## Quick Start

```bash
# Download cache from GitHub (fastest) and run tests
./scripts/conformance.sh all

# Or just download the cache
./scripts/conformance.sh download

# Run tests on specific error code
./scripts/conformance.sh run --error-code 2339 --max 50
```

## Finding Low-Hanging Fruits

### Step 1: Look at "Extra" Errors (False Positives)
These block valid code from compiling - highest user impact:

```bash
# Current top error mismatches
./scripts/conformance.sh run 2>&1 | grep -A15 "Top Error Code"
```

From our last run:
```
TS2339: missing=357, extra=621   # 621 false positives!
TS1005: missing=472, extra=283   # Was 972 before ASI fix
TS2322: missing=284, extra=426   # Type assignability false positives
TS2345: missing=84, extra=334    # Argument type false positives
```

### Step 2: Investigate High-Volume "Extra" Codes
```bash
# Pick the top "extra" error code and filter tests
./scripts/conformance.sh run --error-code 2339 --verbose --max 50 2>&1
```

### Step 3: Find Patterns
Look for **repeated failures with same root cause**:
```bash
# Get a specific failing test and compare
./.target/release/tsz <test_file> --noEmit 2>&1
npx tsc <test_file> --noEmit 2>&1
```

### Step 4: Prioritize by Fix Complexity

| Error Range | Type | Typical Complexity |
|-------------|------|-------------------|
| **TS1xxx** | Parser | Often simple (1-line fixes like ASI) |
| **TS2304** | Symbol resolution | Medium |
| **TS2339** | Property access | Medium-Hard |
| **TS2322/2345** | Type compatibility | Hard (Solver) |

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
./.target/release/tsz <test_file> --noEmit 2>&1
npx tsc <test_file> --noEmit 2>&1
```

## Updating TypeScript Version

When updating the TypeScript submodule:

1. Update the submodule: `git submodule update --remote TypeScript`
2. Find the commit date and matching nightly version
3. Update `scripts/typescript-versions.json` with the SHA mapping
4. Push - the `tsc-cache.yml` workflow will generate the new cache
