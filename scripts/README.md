# Scripts

Testing and build scripts for Project Zang.

## Quick Start

```bash
# Show all available commands
node scripts/help.mjs

# Run unit tests
./scripts/test.sh

# Run conformance tests (500 tests)
./scripts/conformance/run.sh

# Run with options
./scripts/conformance/run.sh --max=100           # Fewer tests
./scripts/conformance/run.sh --all               # All tests
./scripts/conformance/run.sh --verbose           # Detailed output
./scripts/conformance/run.sh --category=compiler # Compiler tests only
```

## Scripts

| Script | Purpose |
|--------|---------|
| `scripts/conformance/run.sh` | Run type checker conformance tests |
| `scripts/emit/run.sh` | Run emit tests (JS + declaration output) |
| `scripts/run-fourslash.sh` | Run language service fourslash tests |
| `scripts/test.sh` | Run Rust unit tests |
| `scripts/bench.sh` | Run benchmarks |
| `scripts/precommit-microbench.sh` | Fast local microbenchmark regression gate |
| `scripts/build-wasm.sh` | Build WASM module |
| `scripts/setup.sh` | One-stop setup (submodule, deps, hooks) |
| `scripts/run-single-test.mjs` | Debug single file (host) |
| `scripts/validate-wasm.mjs` | Validate WASM loads |
| `scripts/help.mjs` | Show all commands |

## Resource Protection

Test and benchmark scripts apply resource limits to protect the host:
- **Memory**: 8GB default via ulimit (configurable with `TSZ_MAX_RSS_MB`)
- **Timeout**: 300s for tests, 600s for benchmarks
- **Per-test timeout**: Configured in `.config/nextest.toml` profiles
