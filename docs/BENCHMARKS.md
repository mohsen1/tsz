# Benchmarks

## Overview

This document describes the benchmark suite for the tsz compiler.
All benchmarks use [Criterion.rs](https://github.com/bheisler/criterion.rs) for statistical analysis.

## Running Benchmarks

```bash
# Run all benchmarks (with resource limits)
./scripts/bench.sh

# Run specific benchmark
./scripts/bench.sh emitter_bench

# Run scanner benchmarks only
./scripts/bench.sh scanner_bench

# Run parser benchmarks only
./scripts/bench.sh parser_bench
```

## Emitter Benchmarks

Located in: `benches/emitter_bench.rs`

### 1. `emit_simple`
- **Purpose**: Baseline emitter performance on trivial code
- **Source**: Simple function with arithmetic and console.log
- **Metrics**: Time per iteration
- **Use case**: Sanity check, regression detection

### 2. `emit_medium`
- **Purpose**: Real-world emitter performance on typical code
- **Source**: Interface + Class with methods (UserService example)
- **Features tested**: Interfaces, classes, methods, arrays, optional chaining
- **Metrics**: Time per iteration
- **Use case**: Representative of actual codebases

### 3. `emit_complex`
- **Purpose**: Stress test with advanced TypeScript features
- **Source**: Generics, async/await, conditional types, repository pattern
- **Features tested**: Type parameters, async functions, mapped types
- **Metrics**: Time per iteration
- **Use case**: Performance ceiling, optimization target

### 4. `emitter_throughput`
- **Purpose**: Measure bytes/second emission rate
- **Variants**:
  - 10 functions × 5 statements
  - 20 functions × 10 statements
  - 50 functions × 5 statements
  - 100 functions × 5 statements
- **Metrics**: **Throughput (bytes/sec)**, time per iteration
- **Use case**: Scalability analysis, compare with TypeScript-Go

### 5. `emitter_write`
- **Purpose**: Isolate string building overhead
- **Method**: Pre-parse AST, only measure emission loop
- **Metrics**: Time per iteration (excluding parsing)
- **Use case**: Identify bottlenecks (is it parsing or emitting?)

### 6. `printer_comparison`
- **Purpose**: Compare ThinPrinter vs legacy Printer
- **Metrics**: Throughput comparison
- **Use case**: Validate that ThinNodeArena refactor improved performance

### 7. `emitter_sourcemap`
- **Purpose**: Measure source map generation overhead
- **Variants**:
  - Without source map tracking
  - With position tracking (always-on in ThinPrinter)
- **Metrics**: Throughput with/without tracking
- **Use case**: Understand cost of source map support

## Expected Performance Targets

Based on Phase 6.1 goals:

| Benchmark | Target | Notes |
|-----------|--------|-------|
| `emit_simple` | < 50 µs | Trivial code should be instant |
| `emit_medium` | < 200 µs | Typical codebase performance |
| `emitter_throughput/100fn_5stmt` | > 50 MB/s | Must beat TypeScript-Go (~40 MB/s) |
| `emitter_write/emit_only` | < 100 µs | Pure emission without parsing |

## Interpreting Results

Criterion generates detailed reports in `target/criterion/`:

```bash
# View HTML report
open target/criterion/report/index.html

# Command-line summary
cargo bench --bench emitter_bench | grep -A 3 "time:"
```

### Key Metrics

- **time**: Mean execution time (lower is better)
- **thrpt**: Throughput in bytes/sec (higher is better)
- **change**: % change vs previous run (watch for regressions)

### Regression Detection

Criterion automatically detects performance regressions:
- **Green**: Performance improved or stable (< 5% change)
- **Yellow**: Possible regression (5-10% slower)
- **Red**: Significant regression (> 10% slower)

## Benchmark Maintenance

### Adding New Benchmarks

1. Add test function to `benches/emitter_bench.rs`
2. Follow naming convention: `bench_emit_<feature>`
3. Use `black_box()` to prevent compiler optimization
4. Add to `criterion_group!` macro
5. Document expected performance target above

### Updating Test Sources

Test sources (SIMPLE_SOURCE, MEDIUM_SOURCE, COMPLEX_SOURCE) should represent:
- **Simple**: < 10 lines, basic syntax
- **Medium**: 20-50 lines, common patterns (classes, interfaces)
- **Complex**: 50-100 lines, advanced features (generics, async)

Avoid:
- Comments (skews byte counts)
- External dependencies (hurts reproducibility)
- Platform-specific code (Windows vs Unix paths)

## CI Integration

Benchmarks should run on:
- **Every PR**: Detect regressions before merge
- **Weekly**: Track long-term trends
- **Before releases**: Validate performance claims

## Performance Monitoring

Track these metrics over time:
1. **Absolute performance**: Are we getting faster?
2. **Scalability**: Does throughput stay constant as input grows?
3. **Consistency**: Is variance low (< 5%)?

## Troubleshooting

### Benchmark Fails with OOM
- Ensure you're using `./scripts/bench.sh` (applies memory limits)
- Adjust memory limit with `TSZ_MAX_RSS_MB` env var (default: 8192)
- Reduce test source size if needed

### High Variance (> 10%)
- Close other applications
- Run benchmarks on idle system
- Check for thermal throttling (CPU temperature)

### Unrealistic Results
- Verify `black_box()` is used (prevents dead code elimination)
- Check that parsing isn't accidentally included in emit-only benches
- Ensure warm-up iterations are sufficient

## References

- [Criterion.rs User Guide](https://bheisler.github.io/criterion.rs/book/)
- [Rust Performance Book](https://nnethercote.github.io/perf-book/)
- [TypeScript-Go Benchmarks](https://github.com/ruiafonsopereira/typescript-to-go)
