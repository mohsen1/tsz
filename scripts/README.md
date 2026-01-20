# Scripts

Testing and build scripts for Project Zang.

## Important: Docker Required

**Conformance tests MUST run in Docker.** Direct execution can cause:
- Infinite loops (freezes your machine)
- Out of memory crashes
- System instability

## Quick Start

```bash
# Show all available commands
node scripts/help.mjs

# Run conformance tests (500 tests, in Docker)
./conformance/run-conformance.sh

# Run with options
./conformance/run-conformance.sh --max=100           # Fewer tests
./conformance/run-conformance.sh --all               # All tests
./conformance/run-conformance.sh --verbose           # Detailed output
./conformance/run-conformance.sh --category=compiler # Compiler tests only
```

## Scripts

| Script | Purpose |
|--------|---------|
| `conformance/run-conformance.sh` | Run conformance tests (Docker) |
| `scripts/test.sh` | Run Rust unit tests (Docker) |
| `scripts/build-wasm.sh` | Build WASM module |
| `scripts/run-single-test.mjs` | Debug single file (host) |
| `scripts/validate-wasm.mjs` | Validate WASM loads |
| `scripts/help.mjs` | Show all commands |

## Docker Configuration

The conformance runner uses these limits:
- **Memory**: 4GB (prevents OOM from killing host)
- **CPUs**: 2 (prevents CPU saturation)
- **PIDs**: 100 (prevents fork bombs)
- **Timeout**: 600s default (kills runaway tests)
