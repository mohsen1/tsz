# Scripts

Testing and build scripts for Project Zang.

## Quick Start

```bash
# Show all available commands
node scripts/help.mjs

# Run unit tests
./scripts/test.sh

# Run conformance tests (500 tests)
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
| `conformance/run-conformance.sh` | Run conformance tests |
| `scripts/test.sh` | Run Rust unit tests |
| `scripts/bench.sh` | Run benchmarks |
| `scripts/build-wasm.sh` | Build WASM module |
| `scripts/install-hooks.sh` | Install git pre-commit hooks |
| `scripts/run-single-test.mjs` | Debug single file (host) |
| `scripts/validate-wasm.mjs` | Validate WASM loads |
| `scripts/help.mjs` | Show all commands |

## Resource Protection

Test and benchmark scripts apply resource limits to protect the host:
- **Memory**: 8GB default via ulimit (configurable with `TSZ_MAX_RSS_MB`)
- **Timeout**: 300s for tests, 600s for benchmarks
- **Per-test timeout**: Configured in `.config/nextest.toml` profiles
