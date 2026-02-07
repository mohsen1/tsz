# TSZ Conformance Test Runner

High-performance Rust implementation for testing the tsz TypeScript compiler against the official TypeScript test suite.

## Overview

This is a two-phase conformance testing system:

1. **Phase 1 (Cache Generation)**: Run official TSC compiler on all test files to generate a golden master cache
2. **Phase 2 (Test Execution)**: Run tsz on test files and compare results against TSC cache

## Performance

- **Cache Generation**: 232 tests/sec (28.8s for 6,695 tests)
- **Conformance Testing**: 577 tests/sec (11.6s for 6,695 tests)
- **Parallel Execution**: Uses 16 workers by default (configurable)
- **Current Pass Rate**: 88.7% (5,941/6,695 tests)

## Quick Start

From the repository root:

```bash
# Generate TSC cache (required first time)
./scripts/conformance.sh generate

# Run full conformance test suite
./scripts/conformance.sh run

# Run with options
./scripts/conformance.sh run --verbose              # Show per-test results
./scripts/conformance.sh run --max 1000            # Test first 1000 files
./scripts/conformance.sh run --filter "strict"      # Filter by pattern
./scripts/conformance.sh run --workers 32          # Use 32 workers
```

## Architecture

### Components

- **`cache.rs`**: TSC cache module with blake3 hashing and fast metadata validation
- **`runner.rs`**: Parallel test execution using tokio
- **`test_parser.rs`**: `@` directive regex parser (strict, target, module, etc.)
- **`tsc_results.rs`**: Result structures with AtomicUsize stats and DashMap error tracking
- **`tsz_wrapper.rs`**: tsz compiler integration (spawns process for compilation)
- **`cli.rs`**: Clap CLI argument parsing

### Key Features

- **Lock-free statistics**: AtomicUsize counters + DashMap for concurrent error tracking
- **Fast cache validation**: mtime/size pre-check avoids reading file content
- **Blake3 hashing**: Fast cryptographic hashing for cache keys
- **Parallel execution**: Tokio async runtime with configurable workers
- **Virtual file system**: Temp file creation for isolated test compilation

## Test Result Format

Tests compare tsz error codes against TSC error codes:

```rust
pub enum TestResult {
    Pass,                                    // Results match perfectly
    Fail { expected, actual, missing, extra }, // Error code differences
    Skipped(&'static str),                  // @skip, @noCheck, or no cache
    Crashed,                                // Compiler panic
}
```

## TSC Cache Format

```json
{
  "<blake3-hash>": {
    "metadata": {
      "mtime_ms": 1234567890,
      "size": 1234
    },
    "error_codes": [2304, 2322, ...]
  }
}
```

## Building

```bash
cd crates/conformance
cargo build --release
```

Binaries are created at:
- `.target/release/generate-tsc-cache` - Cache generator
- `.target/release/tsz-conformance` - Test runner

## Direct Usage

### Generate TSC Cache

```bash
generate-tsc-cache \
  --test-dir TypeScript/tests/cases/conformance \
  --workers 16 \
  --output tsc-cache-full.json
```

### Run Tests

```bash
tsz-conformance \
  --test-dir TypeScript/tests/cases/conformance \
  --cache-file tsc-cache-full.json \
  --tsz-binary ./.target/release/tsz \
  --workers 16
```

## CLI Options

| Option | Description | Default |
|--------|-------------|---------|
| `--test-dir <PATH>` | Test directory path | `./TypeScript/tests/cases` |
| `--cache-file <PATH>` | Path to TSC cache JSON | `./tsc-cache.json` |
| `--tsz-binary <PATH>` | Path to tsz binary | `../target/release/tsz` |
| `--workers <N>` | Number of parallel workers | `num_cpus - 1` |
| `--max <N>` | Maximum number of tests to run | 99999 |
| `--verbose` | Show per-test results | false |
| `--print-test` | Print test file names while running | false |
| `--filter <PATTERN>` | Filter test files by pattern | - |
| `--all` | Run all tests (no limit) | - |
| `--cache-status` | Show cache status | - |
| `--cache-clear` | Clear the cache | - |

## Performance Optimizations

1. **DashMap**: Lock-free concurrent hash map for error frequency tracking
2. **AtomicUsize**: Lock-free counters for statistics
3. **Metadata pre-check**: Fast cache validation without reading file content
4. **Streaming deserialization**: Uses `BufReader` for large cache files
5. **Rayon parallelism**: CPU-bound cache generation uses Rayon thread pool

## Future Improvements

To achieve 1500+ tests/sec goal:
- [ ] In-memory tsz compilation (eliminate process spawning overhead)
- [ ] Virtual file system support (avoid disk I/O)
- [ ] Library mode integration (call tsz directly instead of spawning)

## Implementation Details

### Concurrency Model

- **Tokio** async runtime for I/O-bound test execution
- **Rayon** for CPU-bound cache generation
- **Arc<T>** for sharing immutable state across workers
- **AtomicUsize** for lock-free counter updates
- **DashMap** for concurrent error frequency tracking

### Thread Safety

No `Mutex` held across `.await` points → no deadlocks
All shared state is either:
- Immutable (`Arc<T>`)
- Lock-free (`AtomicUsize`, `DashMap`)
- Short-lived scoped locks

## License

MIT
