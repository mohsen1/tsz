# Error Tracking Dashboard - Metrics System Documentation

## Overview

This metrics tracking system provides automated error tracking, regression detection, and trend analysis for TypeScript WASM conformance tests.

## Components

### 1. metrics-tracker.mjs
Main metrics tracking tool with the following commands:

```bash
# Run conformance tests and save metrics
node wasm/differential-test/metrics-tracker.mjs run [--max=N] [category]

# Show terminal dashboard
node wasm/differential-test/metrics-tracker.mjs dashboard [--runs=N]

# Generate HTML dashboard
node wasm/differential-test/metrics-tracker.mjs html

# Parse existing output file
node wasm/differential-test/metrics-tracker.mjs parse <file>

# Show all history
node wasm/differential-test/metrics-tracker.mjs history

# Check for regression (exits 1 if found)
node wasm/differential-test/metrics-tracker.mjs regression
```

### 2. conformance-embedded.mjs
Embedded conformance test runner that can be imported as a module:

```javascript
import { runTests } from './conformance-embedded.mjs';

const results = await runTests({
  maxTests: 200,
  category: null,  // or 'classes', 'functions', etc.
  verbose: false
});
```

Returns:
```javascript
{
  totalTests: number,
  exactMatch: number,
  exactMatchPercent: number,
  sameCount: number,
  sameCountPercent: number,
  missingErrors: number,
  missingErrorsPercent: number,
  extraErrors: number,
  extraErrorsPercent: number,
  crashed: number,
  missingCodeCounts: { [code: string]: number },
  extraCodeCounts: { [code: string]: number },
  byCategory: { [category: string]: { total: number, exact: number, same: number } }
}
```

### 3. error-distribution-analyzer.mjs
Error distribution analyzer with commands:

```bash
# Run tests and analyze error distribution
node wasm/differential-test/error-distribution-analyzer.mjs analyze

# Add snapshot from JSON file
node wasm/differential-test/error-distribution-analyzer.mjs add <file>

# Show terminal report
node wasm/differential-test/error-distribution-analyzer.mjs report [--snapshots=N]

# Generate HTML report
node wasm/differential-test/error-distribution-analyzer.mjs html
```

## Metrics Data Storage

All metrics are stored in JSON format:
- `wasm/metrics-data/history.json` - Historical run data (last 100 runs)
- `wasm/metrics-data/error-distribution.json` - Error code distribution snapshots

## Regression Detection

The system automatically detects regression with configurable thresholds:
- **Extra errors increase > 10**: High severity
- **Missing errors increase > 10**: High severity
- **Exact match decrease > 0.5%**: Medium severity

## Dashboard Outputs

### Terminal Dashboard
Shows latest run summary, trend analysis, and historical data with color-coded indicators.

### HTML Dashboard
Generated at `wasm/metrics-data/dashboard.html` with:
- Summary cards for latest metrics
- Historical trend table
- Color-coded health indicators

## Prerequisites

The WASM package must be built before running tests:

```bash
cd wasm
./build-wasm
```

This requires Docker and Rust toolchain with wasm-pack.

## Task Requirements

From WORKER_12_TASK_LIST.md:

### 1. Create Test Cases for Strict Property Initialization (TS2564)
Status: Tests already exist at `tests/cases/conformance/classes/propertyMemberDeclarations/strictPropertyInitialization.ts`

Covers:
- Class properties without initializers
- Constructor-assigned properties
- Definite assignment assertions
- Optional properties
- `declare` properties

### 2. Create Test Cases for Implicit Any Detection (TS7006)
Status: Tests already exist throughout the codebase (114 files found with `noImplicitAny`)

### 3. Run Conformance Tests and Generate Metrics

To generate metrics for specific error codes:

```bash
# Run conformance tests
node wasm/differential-test/metrics-tracker.mjs run --max=500

# Analyze error distribution
node wasm/differential-test/error-distribution-analyzer.mjs analyze

# View results
node wasm/differential-test/metrics-tracker.mjs dashboard
node wasm/differential-test/metrics-tracker.mjs html
```

Before/After Metrics for:
- **TS2564** (strictPropertyInitialization): Use `--max=1000` or target specific category
- **TS7006** (implicit any): Analyze from error distribution report
- **TS2322** (type assignability): Analyze from error distribution report

### 4. Document "Extra Error" Spike from UNKNOWN Defaults

The solver uses `Unknown` type instead of `Any` as a fallback for stricter type checking. This is documented in:
- `wasm/src/solver/integration_tests.rs` - `unknown_fallback_tests` module

**Key Behavior:**
- `Unknown` is a top type but not assignable without check
- Functions without explicit `this` parameter fall back to `Unknown`
- With `Unknown` fallback, functions are NOT compatible when one has explicit `this` type

This causes "Extra Errors" because:
1. WASM correctly detects type mismatches that TSC might not report in some cases
2. `Unknown` is stricter than `Any`, exposing more type safety issues
3. These are generally correct error exposures, not regressions

### 5. Analyze and Document Results

Generate comprehensive report:

```bash
# Run full test suite
node wasm/differential-test/metrics-tracker.mjs run --max=1000

# Generate all reports
node wasm/differential-test/metrics-tracker.mjs html
node wasm/differential-test/error-distribution-analyzer.mjs html

# Check for regression
node wasm/differential-test/metrics-tracker.mjs regression
```

## Success Criteria

- [x] Metrics tracking infrastructure built
- [x] Error distribution analyzer implemented
- [x] Regression detection with configurable thresholds
- [x] Terminal and HTML dashboards
- [ ] Test coverage data (requires WASM build)
- [ ] Metrics showing reduction in missing errors (requires WASM build)
- [ ] Documented analysis of new "Extra Errors" from UNKNOWN default (in progress)
- [ ] No regressions in previously passing tests (requires WASM build)
- [ ] Comprehensive final report (requires WASM build)

## Current Limitations

1. **WASM Build Required**: The conformance tests require the WASM package to be built via Docker, which may not be available in all environments.

2. **Multi-file Tests**: The embedded test runner currently skips multi-file tests (those with `@filename` directives). Full support would require using the `WasmProgram` API.

3. **Before/After Comparison**: To generate true before/after metrics, you need to:
   - Run tests with the baseline code (before semantics fixes)
   - Run tests with the fixed code (after semantics fixes)
   - Compare the two result sets

## Next Steps

Once WASM is built:

1. Run baseline tests on current code
2. After semantics team merges fixes, run comparison tests
3. Generate before/after metrics report
4. Document any new "Extra Errors" from UNKNOWN defaults
5. Verify these are correct error exposures, not regressions
