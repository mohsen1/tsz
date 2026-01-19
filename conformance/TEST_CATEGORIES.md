# Test Categories

## Directory Structure

TypeScript conformance tests are organized in:
```
tests/cases/
├── compiler/      - Core compiler tests
├── conformance/   - Language conformance tests
└── projects/      - Multi-file project tests
```

## Category Details

### compiler/
Core compiler functionality tests including:
- Type inference
- Control flow analysis
- Declaration emit
- Module resolution
- Error recovery

### conformance/
Language feature conformance tests organized by feature:
- `expressions/` - Expression type checking
- `statements/` - Statement handling
- `types/` - Type system features
- `classes/` - Class-related tests
- `interfaces/` - Interface tests
- `generics/` - Generic type tests
- `decorators/` - Decorator tests
- `jsx/` - JSX support tests

### projects/
Multi-file project tests that verify:
- Cross-file type checking
- Module resolution
- Declaration file generation
- Project references

## Running Tests

### Using npm scripts (recommended)

```bash
# Run conformance tests only
npm run test:conformance

# Run compiler tests only
npm run test:compiler

# Run project tests only
npm run test:projects

# Run all categories
npm run test:all

# Limit number of tests
npm run test:100
npm run test:500
npm run test:1000

# Verbose output with details
npm run test:verbose

# Build TypeScript sources first
npm run build
```

### Using the shell script

```bash
# Run with default settings (conformance only)
./conformance/run-conformance.sh

# Run all test categories
./conformance/run-conformance.sh --all

# Run specific categories
./conformance/run-conformance.sh --category=conformance
./conformance/run-conformance.sh --category=compiler
./conformance/run-conformance.sh --category=projects
./conformance/run-conformance.sh --category=conformance,compiler,projects

# Limit number of tests
./conformance/run-conformance.sh --max=100
./conformance/run-conformance.sh --max=500

# Control parallelism
./conformance/run-conformance.sh --workers=8
./conformance/run-conformance.sh --sequential

# Force rebuild Docker image
./conformance/run-conformance.sh --rebuild
```

### Direct TypeScript execution

```bash
# After building with `npm run build`
node dist/runner.js --category=conformance --max=100 --verbose
node dist/runner.js --category=compiler,projects
```

### Integration with main test script

The main `scripts/test.sh` can also run conformance tests:

```bash
# Run conformance tests from main test script
./scripts/test.sh --conformance

# Run specific category
./scripts/test.sh --conformance compiler
./scripts/test.sh --conformance projects

# Run all categories
./scripts/test.sh --conformance all
```

## Pass Rate Tracking

The runner outputs:
- **Exact Match**: Identical error codes to tsc
- **Same Count**: Same number of errors (different codes)
- **Missing Errors**: Errors tsc produces that we miss
- **Extra Errors**: Errors we produce that tsc doesn't
- **Per-Category Statistics**: Pass rates broken down by test category

### Example Output

```
══════════════════════════════════════════════════════════════
CONFORMANCE TEST RESULTS
══════════════════════════════════════════════════════════════

Overall Pass Rate: 45.2%
Exact Match Rate:  42.8%

Summary:
  Total:        500
  Passed:       226
  Failed:       274
  Crashed:      0

By Category:
  conformance: 180/400 (45.0%)
  compiler:    46/100 (46.0%)

══════════════════════════════════════════════════════════════
```

Target: 95%+ exact match rate

## Implementation Status

- [x] Single-file test support
- [x] Multi-file test support (using WasmProgram API)
- [x] Test directive parsing (@strict, @target, etc.)
- [x] lib.d.ts loading
- [x] Diagnostic comparison
- [x] Pass rate reporting
- [x] Per-category pass rate tracking
- [x] Category filtering (conformance, compiler, projects)
- [x] Verbose mode with error code analysis
- [x] Unified runner supporting all 3 categories
- [x] Shell script integration with --category flag
- [ ] Baseline file comparison
- [ ] Incremental testing (skip unchanged)
- [ ] Test isolation (sandbox)
