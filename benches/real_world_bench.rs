//! Real-world Emitter Benchmark
//!
//! Measures emitter throughput on actual TypeScript compiler source files
//! instead of synthetic test data.

use criterion::{Criterion, Throughput, black_box, criterion_group, criterion_main};
use wasm::{thin_emitter::ThinPrinter, thin_parser::ThinParserState};

const CHECKER_TS: &str = include_str!("../../src/compiler/checker.ts");

/// Benchmark: Parse + Emit checker.ts (3.1 MB real TypeScript file)
fn bench_checker_parse_emit(c: &mut Criterion) {
    let bytes = CHECKER_TS.len() as u64;

    let mut group = c.benchmark_group("real_world");
    group.throughput(Throughput::Bytes(bytes));
    group.sample_size(10); // Fewer samples for large file

    group.bench_function("checker_ts_full_pipeline", |b| {
        b.iter(|| {
            let mut parser = ThinParserState::new("checker.ts".to_string(), CHECKER_TS.to_string());
            let root = parser.parse_source_file();

            // Pre-allocate based on source size (1.5x for downleveling)
            let capacity = CHECKER_TS.len() * 3 / 2;
            let mut printer = ThinPrinter::with_capacity(&parser.arena, capacity);
            printer.emit(root);
            black_box(printer.take_output())
        })
    });

    group.finish();
}

/// Benchmark: Just emit (pre-parsed checker.ts)
fn bench_checker_emit_only(c: &mut Criterion) {
    let bytes = CHECKER_TS.len() as u64;

    // Pre-parse once
    let mut parser = ThinParserState::new("checker.ts".to_string(), CHECKER_TS.to_string());
    let root = parser.parse_source_file();

    let mut group = c.benchmark_group("real_world");
    group.throughput(Throughput::Bytes(bytes));
    group.sample_size(10);

    group.bench_function("checker_ts_emit_only", |b| {
        b.iter(|| {
            let capacity = CHECKER_TS.len() * 3 / 2;
            let mut printer = ThinPrinter::with_capacity(&parser.arena, capacity);
            printer.emit(root);
            black_box(printer.take_output())
        })
    });

    group.finish();
}

criterion_group!(benches, bench_checker_parse_emit, bench_checker_emit_only,);

criterion_main!(benches);
