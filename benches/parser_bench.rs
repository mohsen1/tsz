//! Benchmarks for the Rust parser implementation.
//!
//! Run with: cargo bench --bench parser_bench
//!
//! These benchmarks help track:
//! - Parse time for various file sizes
//! - AST node allocation overhead
//! - Serialization overhead (critical for JS boundary)

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use wasm::parser::ParserState;

/// Small TypeScript source
const SMALL_SOURCE: &str = r#"
const x: number = 42;
const y: string = "hello";
function add(a: number, b: number): number {
    return a + b;
}
"#;

/// Medium TypeScript source with classes and interfaces
const MEDIUM_SOURCE: &str = r#"
import { Component } from '@angular/core';

interface User {
    id: number;
    name: string;
    email: string;
}

class UserService {
    private users: Map<number, User> = new Map();

    constructor() {
        this.initializeUsers();
    }

    private initializeUsers(): void {
        const defaultUsers: User[] = [
            { id: 1, name: 'Alice', email: 'alice@example.com' },
            { id: 2, name: 'Bob', email: 'bob@example.com' },
        ];
        defaultUsers.forEach(user => this.users.set(user.id, user));
    }

    getUser(id: number): User | undefined {
        return this.users.get(id);
    }

    getUsers(): User[] {
        return Array.from(this.users.values());
    }

    async fetchUser(id: number): Promise<User> {
        const response = await fetch('/api/users/' + id);
        return response.json();
    }
}

function processUser(user: User): string {
    return user.name + ' <' + user.email + '>';
}

export { UserService, User, processUser };
"#;

/// Complex TypeScript source with generics and conditional types
const COMPLEX_SOURCE: &str = r#"
// Generic type definitions
type DeepReadonly<T> = {
    readonly [P in keyof T]: T[P] extends object ? DeepReadonly<T[P]> : T[P];
};

type DeepPartial<T> = {
    [P in keyof T]?: T[P] extends object ? DeepPartial<T[P]> : T[P];
};

// Conditional type utilities
type UnwrapPromise<T> = T extends Promise<infer U> ? U : T;
type ExtractReturnType<T> = T extends (...args: unknown[]) => infer R ? R : never;

// Complex class with decorators
class DataManager<T extends { id: number }> {
    private data: Map<number, T> = new Map();
    private subscribers: Set<(data: T[]) => void> = new Set();

    add(item: T): void {
        this.data.set(item.id, item);
        this.notify();
    }

    remove(id: number): boolean {
        const result = this.data.delete(id);
        if (result) {
            this.notify();
        }
        return result;
    }

    update(id: number, partial: Partial<T>): T | undefined {
        const existing = this.data.get(id);
        if (existing) {
            const updated = { ...existing, ...partial };
            this.data.set(id, updated);
            this.notify();
            return updated;
        }
        return undefined;
    }

    get(id: number): T | undefined {
        return this.data.get(id);
    }

    getAll(): T[] {
        return Array.from(this.data.values());
    }

    subscribe(callback: (data: T[]) => void): () => void {
        this.subscribers.add(callback);
        return () => this.subscribers.delete(callback);
    }

    private notify(): void {
        const data = this.getAll();
        this.subscribers.forEach(cb => cb(data));
    }
}

// Mapped type with template literal
type Getters<T> = {
    [K in keyof T as `get${Capitalize<string & K>}`]: () => T[K];
};

type Setters<T> = {
    [K in keyof T as `set${Capitalize<string & K>}`]: (value: T[K]) => void;
};

// Interface with index signature
interface DynamicRecord {
    [key: string]: unknown;
    id: number;
    type: string;
}

// Function overloads
function process(value: string): string;
function process(value: number): number;
function process(value: string | number): string | number {
    if (typeof value === 'string') {
        return value.toUpperCase();
    }
    return value * 2;
}

// Arrow functions with various patterns
const identity = <T>(x: T): T => x;
const compose = <A, B, C>(f: (a: A) => B, g: (b: B) => C) => (a: A): C => g(f(a));
const pipe = <T>(...fns: ((x: T) => T)[]): ((x: T) => T) =>
    (x: T) => fns.reduce((acc, fn) => fn(acc), x);

export { DataManager, DynamicRecord, Getters, Setters, process, identity, compose, pipe };
"#;

/// Generate large synthetic source
fn generate_large_source(classes: usize, methods_per_class: usize) -> String {
    let mut source = String::with_capacity(classes * methods_per_class * 100);
    source.push_str("// Generated TypeScript for benchmarking\n\n");

    for c in 0..classes {
        source.push_str(&format!("interface I{} {{\n", c));
        source.push_str(&format!("    id: number;\n"));
        source.push_str(&format!("    name: string;\n"));
        source.push_str(&format!("}}\n\n"));

        source.push_str(&format!("class Class{} implements I{} {{\n", c, c));
        source.push_str(&format!("    id: number = {};\n", c));
        source.push_str(&format!("    name: string = \"class{}\";\n\n", c));

        for m in 0..methods_per_class {
            source.push_str(&format!(
                "    method{}(x: number, y: string): number {{\n",
                m
            ));
            source.push_str(&format!("        const result = x * {};\n", m));
            source.push_str(&format!("        console.log(y);\n"));
            source.push_str(&format!("        return result;\n"));
            source.push_str(&format!("    }}\n\n"));
        }

        source.push_str(&format!("}}\n\n"));
    }

    source
}

/// Benchmark: Parse small source
fn bench_parse_small(c: &mut Criterion) {
    c.bench_function("parse_small", |b| {
        b.iter(|| {
            let mut parser =
                ParserState::new("bench.ts".to_string(), black_box(SMALL_SOURCE.to_string()));
            let root = parser.parse_source_file();
            black_box(root)
        })
    });
}

/// Benchmark: Parse medium source
fn bench_parse_medium(c: &mut Criterion) {
    c.bench_function("parse_medium", |b| {
        b.iter(|| {
            let mut parser =
                ParserState::new("bench.ts".to_string(), black_box(MEDIUM_SOURCE.to_string()));
            let root = parser.parse_source_file();
            black_box(root)
        })
    });
}

/// Benchmark: Parse complex source (generics, conditional types)
fn bench_parse_complex(c: &mut Criterion) {
    c.bench_function("parse_complex", |b| {
        b.iter(|| {
            let mut parser = ParserState::new(
                "bench.ts".to_string(),
                black_box(COMPLEX_SOURCE.to_string()),
            );
            let root = parser.parse_source_file();
            black_box(root)
        })
    });
}

/// Benchmark: Parse throughput for various sizes
fn bench_parse_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("parser_throughput");

    for (classes, methods) in [(5, 5), (10, 10), (20, 10), (50, 5)].iter() {
        let source = generate_large_source(*classes, *methods);
        let bytes = source.len() as u64;
        let label = format!("{}c_{}m", classes, methods);

        group.throughput(Throughput::Bytes(bytes));
        group.bench_with_input(BenchmarkId::new("parse", &label), &source, |b, source| {
            b.iter(|| {
                let mut parser =
                    ParserState::new("bench.ts".to_string(), black_box(source.clone()));
                let root = parser.parse_source_file();
                black_box(root)
            })
        });
    }

    group.finish();
}

// Benchmark removed - get_source_file_json no longer exists in Parser

/// Benchmark: Node allocation overhead
fn bench_node_allocation(c: &mut Criterion) {
    c.bench_function("node_allocation", |b| {
        b.iter(|| {
            let mut parser = ParserState::new("bench.ts".to_string(), MEDIUM_SOURCE.to_string());
            let root = parser.parse_source_file();
            let count = parser.get_node_count();
            black_box((root, count))
        })
    });
}

/// Benchmark: Incremental parsing simulation (re-parse)
fn bench_incremental_reparse(c: &mut Criterion) {
    c.bench_function("reparse_simulation", |b| {
        // Simulate editing by slightly modifying source
        let mut sources: Vec<String> = Vec::new();
        for i in 0..5 {
            let modified =
                MEDIUM_SOURCE.replace("const defaultUsers", &format!("const defaultUsers{}", i));
            sources.push(modified);
        }

        b.iter(|| {
            for source in &sources {
                let mut parser = ParserState::new("bench.ts".to_string(), source.clone());
                let root = parser.parse_source_file();
                black_box(root);
            }
        })
    });
}

// =============================================================================
// Parser Benchmarks - Cache-Optimized 16-byte Nodes
// =============================================================================

/// Simple source for Parser (no classes/interfaces/types yet)
criterion_group!(
    benches,
    bench_parse_small,
    bench_parse_medium,
    bench_parse_complex,
    bench_parse_throughput,
    bench_node_allocation,
    bench_incremental_reparse,
);

criterion_main!(benches);
