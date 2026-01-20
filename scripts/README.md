# Scripts

Testing and build scripts for Project Zang.

## Scripts

| Script | Purpose | Usage |
|--------|---------|-------|
| `help.mjs` | Show all available commands | `node scripts/help.mjs` |
| `run-single-test.mjs` | Test individual TypeScript files | `node scripts/run-single-test.mjs path/to/test.ts` |
| `validate-wasm.mjs` | Validate WASM module loads correctly | `node scripts/validate-wasm.mjs` |
| `test.sh` | Run Rust unit tests in Docker | `./scripts/test.sh` |
| `build-wasm.sh` | Build WASM module | `./scripts/build-wasm.sh` |

## Quick Start

```bash
# See all available commands
node scripts/help.mjs

# Test a specific file
node scripts/run-single-test.mjs TypeScript/tests/cases/compiler/2dArrays.ts

# Run conformance tests (in Docker)
./conformance/run-conformance.sh --max=100
```

## ⚠️ Safety Warning

**Always run conformance tests in Docker.** Direct execution can cause:
- Infinite loops that freeze your machine
- Out of memory crashes
- System instability

See `docs/TESTING_CLEANUP_PLAN.md` for details.
