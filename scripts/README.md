# WASM Test Scripts

This directory contains scripts for testing individual aspects of the WASM TypeScript compiler implementation.

## Scripts

| Script | Purpose | Usage Example |
|--------|---------|---------------|
| `help.mjs` | Show all available test commands | `node wasm/scripts/help.mjs` |
| `run-single-test.mjs` | Test individual TypeScript files | `node wasm/scripts/run-single-test.mjs tests/cases/compiler/2dArrays.ts` |
| `compare-baselines.mjs` | Compare outputs against TypeScript baselines | `node wasm/scripts/compare-baselines.mjs 100 compiler` |
| `run-batch-tests.mjs` | Run multiple tests in sequence | `node wasm/scripts/run-batch-tests.mjs` |
| `validate-wasm.mjs` | Validate WASM module loads correctly | `node wasm/scripts/validate-wasm.mjs` |

## Quick Start

```bash
# See all available commands
node wasm/scripts/help.mjs

# Test a specific file with detailed output  
node wasm/scripts/run-single-test.mjs tests/cases/compiler/arrayLiterals.ts --verbose

# Compare first 50 compiler tests against baselines
node wasm/scripts/compare-baselines.mjs 50 compiler
```

## Development Workflow

1. **Start with validation**: `node wasm/scripts/validate-wasm.mjs`
2. **Test specific functionality**: `node wasm/scripts/run-single-test.mjs path/to/test.ts --verbose`  
3. **Compare against TypeScript**: `node wasm/scripts/compare-baselines.mjs 10 compiler`

For comprehensive testing, use the main conformance runner:
```bash
./conformance/run-conformance.sh --max=1000
```

See [../TESTING.md](../TESTING.md) for complete testing guide.