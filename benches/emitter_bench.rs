//! Emitter Benchmark
//!
//! Measures emitter throughput (bytes/sec) for Phase 6.1 performance analysis.

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use wasm::{emitter::Printer, parser::ParserState};

// =============================================================================
// Test Sources
// =============================================================================

const SIMPLE_SOURCE: &str = r#"
function add(a: number, b: number): number {
    return a + b;
}

const result = add(1, 2);
console.log(result);
"#;

const MEDIUM_SOURCE: &str = r#"
interface User {
    id: number;
    name: string;
    email?: string;
}

class UserService {
    private users: User[] = [];

    addUser(user: User): void {
        this.users.push(user);
    }

    getUser(id: number): User | undefined {
        return this.users.find(u => u.id === id);
    }

    getAllUsers(): User[] {
        return this.users;
    }
}

const service = new UserService();
service.addUser({ id: 1, name: "Alice" });
service.addUser({ id: 2, name: "Bob", email: "bob@example.com" });

const alice = service.getUser(1);
console.log(alice?.name);
"#;

const COMPLEX_SOURCE: &str = r#"
type DeepPartial<T> = {
    [P in keyof T]?: T[P] extends object ? DeepPartial<T[P]> : T[P];
};

interface Config {
    server: {
        host: string;
        port: number;
    };
    database: {
        url: string;
        pool: number;
    };
}

async function fetchData<T>(url: string): Promise<T> {
    const response = await fetch(url);
    return response.json();
}

class Repository<T extends { id: number }> {
    constructor(private items: T[] = []) {}

    async find(predicate: (item: T) => boolean): Promise<T | undefined> {
        return this.items.find(predicate);
    }

    async save(item: T): Promise<void> {
        const existing = this.items.findIndex(i => i.id === item.id);
        if (existing >= 0) {
            this.items[existing] = item;
        } else {
            this.items.push(item);
        }
    }
}

const config: DeepPartial<Config> = {
    server: { host: "localhost" }
};
"#;

/// Generate a large source with many functions for throughput testing
fn generate_large_source(functions: usize, statements_per_fn: usize) -> String {
    let mut source = String::with_capacity(functions * statements_per_fn * 80);
    source.push_str("// Generated source for emitter benchmarking\n\n");

    for f in 0..functions {
        source.push_str(&format!(
            "function fn{}(x: number, y: number): number {{\n",
            f
        ));
        for s in 0..statements_per_fn {
            source.push_str(&format!("    let v{} = x + {};\n", s, s));
        }
        source.push_str("    return x + y;\n");
        source.push_str("}\n\n");
    }

    // Add some calls
    for f in 0..functions {
        source.push_str(&format!("const r{} = fn{}(1, 2);\n", f, f));
    }

    source
}

// =============================================================================
// Emitter Benchmarks
// =============================================================================

/// Benchmark: Emit simple source
fn bench_emit_simple(c: &mut Criterion) {
    c.bench_function("emit_simple", |b| {
        b.iter(|| {
            let mut parser =
                ParserState::new("bench.ts".to_string(), SIMPLE_SOURCE.to_string());
            let root = parser.parse_source_file();

            let mut printer = Printer::new(&parser.arena);
            printer.emit(root);
            black_box(printer.take_output())
        })
    });
}

/// Benchmark: Emit medium source
fn bench_emit_medium(c: &mut Criterion) {
    c.bench_function("emit_medium", |b| {
        b.iter(|| {
            let mut parser =
                ParserState::new("bench.ts".to_string(), MEDIUM_SOURCE.to_string());
            let root = parser.parse_source_file();

            let mut printer = Printer::new(&parser.arena);
            printer.emit(root);
            black_box(printer.take_output())
        })
    });
}

/// Benchmark: Emit complex source with generics
fn bench_emit_complex(c: &mut Criterion) {
    c.bench_function("emit_complex", |b| {
        b.iter(|| {
            let mut parser =
                ParserState::new("bench.ts".to_string(), COMPLEX_SOURCE.to_string());
            let root = parser.parse_source_file();

            let mut printer = Printer::new(&parser.arena);
            printer.emit(root);
            black_box(printer.take_output())
        })
    });
}

/// Benchmark: Emit throughput for various sizes
fn bench_emit_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("emitter_throughput");

    for (functions, statements) in [(10, 5), (20, 10), (50, 5), (100, 5)].iter() {
        let source = generate_large_source(*functions, *statements);
        let bytes = source.len() as u64;
        let label = format!("{}fn_{}stmt", functions, statements);

        group.throughput(Throughput::Bytes(bytes));
        group.bench_with_input(
            BenchmarkId::new("emit", &label),
            &source,
            |b, source| {
                b.iter(|| {
                    let mut parser = ParserState::new("bench.ts".to_string(), source.clone());
                    let root = parser.parse_source_file();

                    let mut printer = Printer::new(&parser.arena);
                    printer.emit(root);
                    black_box(printer.take_output())
                })
            },
        );
    }

    group.finish();
}

/// Benchmark: Emitter string building (isolate write operations)
fn bench_emit_write_performance(c: &mut Criterion) {
    let mut group = c.benchmark_group("emitter_write");

    // Pre-parse to isolate emit time
    let source = generate_large_source(50, 10);

    group.bench_function("emit_only", |b| {
        // Parse once outside the benchmark loop
        let mut parser = ParserState::new("bench.ts".to_string(), source.clone());
        let root = parser.parse_source_file();

        b.iter(|| {
            let mut printer = Printer::new(&parser.arena);
            printer.emit(root);
            black_box(printer.take_output())
        })
    });

    group.finish();
}

// Benchmark removed - legacy Printer no longer exists, only Printer is supported

/// Benchmark: Source map generation overhead
fn bench_emit_with_sourcemap(c: &mut Criterion) {
    let mut group = c.benchmark_group("emitter_sourcemap");

    let source = generate_large_source(20, 10);
    let bytes = source.len() as u64;

    group.throughput(Throughput::Bytes(bytes));

    // Without source map
    group.bench_function("without_sourcemap", |b| {
        b.iter(|| {
            let mut parser = ParserState::new("bench.ts".to_string(), source.clone());
            let root = parser.parse_source_file();

            let mut printer = Printer::new(&parser.arena);
            printer.emit(root);
            black_box(printer.take_output())
        })
    });

    // With source map tracking (position tracking is always on in Printer)
    group.bench_function("with_position_tracking", |b| {
        b.iter(|| {
            let mut parser = ParserState::new("bench.ts".to_string(), source.clone());
            let root = parser.parse_source_file();

            let mut printer = Printer::new(&parser.arena);
            printer.emit(root);
            let output = printer.take_output();
            black_box(output)
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_emit_simple,
    bench_emit_medium,
    bench_emit_complex,
    bench_emit_throughput,
    bench_emit_write_performance,
    bench_emit_with_sourcemap,
);

criterion_main!(benches);
