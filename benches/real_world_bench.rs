//! Real-world Emitter Benchmark
//!
//! Measures emitter throughput on synthetic TypeScript-like source files
//! of varying sizes to simulate real-world scenarios.

use criterion::{Criterion, Throughput, black_box, criterion_group, criterion_main};
use tsz::{emitter::Printer, parser::ParserState};

/// Generate a synthetic TypeScript file with many class definitions
fn generate_large_ts(class_count: usize) -> String {
    let mut source = String::with_capacity(class_count * 500);
    source.push_str("// Generated TypeScript file for benchmarking\n\n");

    for i in 0..class_count {
        source.push_str(&format!(
            r#"
export class Component{i} {{
    private readonly id: number = {i};
    private name: string;
    private items: Array<string> = [];

    constructor(name: string) {{
        this.name = name;
    }}

    public getId(): number {{
        return this.id;
    }}

    public getName(): string {{
        return this.name;
    }}

    public setName(name: string): void {{
        this.name = name;
    }}

    public addItem(item: string): void {{
        this.items.push(item);
    }}

    public getItems(): Array<string> {{
        return [...this.items];
    }}

    public static create(name: string): Component{i} {{
        return new Component{i}(name);
    }}
}}
"#,
            i = i
        ));
    }

    source
}

/// Benchmark: Parse + Emit large synthetic file
fn bench_large_synthetic_parse_emit(c: &mut Criterion) {
    let source = generate_large_ts(100); // ~50KB file
    let bytes = source.len() as u64;

    let mut group = c.benchmark_group("real_world");
    group.throughput(Throughput::Bytes(bytes));
    group.sample_size(20);

    group.bench_function("synthetic_100_classes_full_pipeline", |b| {
        b.iter(|| {
            let mut parser = ParserState::new("synthetic.ts".to_string(), source.clone());
            let root = parser.parse_source_file();

            let capacity = source.len() * 3 / 2;
            let mut printer = Printer::with_capacity(&parser.arena, capacity);
            printer.emit(root);
            black_box(printer.take_output())
        })
    });

    group.finish();
}

/// Benchmark: Just emit (pre-parsed synthetic file)
fn bench_large_synthetic_emit_only(c: &mut Criterion) {
    let source = generate_large_ts(100);
    let bytes = source.len() as u64;

    // Pre-parse once
    let mut parser = ParserState::new("synthetic.ts".to_string(), source.clone());
    let root = parser.parse_source_file();

    let mut group = c.benchmark_group("real_world");
    group.throughput(Throughput::Bytes(bytes));
    group.sample_size(20);

    group.bench_function("synthetic_100_classes_emit_only", |b| {
        b.iter(|| {
            let capacity = source.len() * 3 / 2;
            let mut printer = Printer::with_capacity(&parser.arena, capacity);
            printer.emit(root);
            black_box(printer.take_output())
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_large_synthetic_parse_emit,
    bench_large_synthetic_emit_only
);

criterion_main!(benches);
