//! Benchmarks for the Rust scanner implementation.
//!
//! Run with: cargo bench --bench scanner_bench
//!
//! These benchmarks help track performance of the scanner against various
//! TypeScript source files and identify serialization overhead.

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use wasm::scanner::SyntaxKind;
use wasm::scanner_impl::ScannerState;

/// Sample TypeScript source for basic benchmarking
const SMALL_SOURCE: &str = r#"
const x: number = 42;
const y: string = "hello";
function add(a: number, b: number): number {
    return a + b;
}
"#;

/// Medium-sized TypeScript source
const MEDIUM_SOURCE: &str = r#"
import { Component, OnInit } from '@angular/core';
import { Observable, Subject, BehaviorSubject } from 'rxjs';
import { map, filter, takeUntil } from 'rxjs/operators';

interface User {
    id: number;
    name: string;
    email: string;
    roles: string[];
    metadata?: Record<string, unknown>;
}

type UserRole = 'admin' | 'user' | 'guest';

class UserService {
    private users: Map<number, User> = new Map();
    private currentUser$ = new BehaviorSubject<User | null>(null);
    private destroy$ = new Subject<void>();

    constructor() {
        this.initializeUsers();
    }

    private initializeUsers(): void {
        const defaultUsers: User[] = [
            { id: 1, name: 'Alice', email: 'alice@example.com', roles: ['admin'] },
            { id: 2, name: 'Bob', email: 'bob@example.com', roles: ['user'] },
        ];
        defaultUsers.forEach(user => this.users.set(user.id, user));
    }

    getUser(id: number): User | undefined {
        return this.users.get(id);
    }

    getUsers(): User[] {
        return Array.from(this.users.values());
    }

    async fetchUserFromApi(id: number): Promise<User> {
        const response = await fetch(`/api/users/${id}`);
        if (!response.ok) {
            throw new Error(`Failed to fetch user: ${response.statusText}`);
        }
        return response.json();
    }

    getCurrentUser(): Observable<User | null> {
        return this.currentUser$.asObservable();
    }

    setCurrentUser(user: User | null): void {
        this.currentUser$.next(user);
    }

    ngOnDestroy(): void {
        this.destroy$.next();
        this.destroy$.complete();
    }
}

// Type guards
function isAdmin(user: User): boolean {
    return user.roles.includes('admin');
}

function hasRole(user: User, role: UserRole): boolean {
    return user.roles.includes(role);
}

// Generic utility functions
function filterByPredicate<T>(items: T[], predicate: (item: T) => boolean): T[] {
    return items.filter(predicate);
}

function mapToProperty<T, K extends keyof T>(items: T[], key: K): T[K][] {
    return items.map(item => item[key]);
}

// Template literal types
type EventName<T extends string> = `on${Capitalize<T>}`;
type ClickEvent = EventName<'click'>; // "onClick"

// Conditional types
type NonNullableFields<T> = {
    [K in keyof T]: NonNullable<T[K]>;
};

// Mapped types
type ReadonlyUser = Readonly<User>;
type PartialUser = Partial<User>;
type RequiredUser = Required<User>;

export { UserService, User, UserRole, isAdmin, hasRole };
"#;

/// Generate a large synthetic TypeScript source
fn generate_large_source(lines: usize) -> String {
    let mut source = String::with_capacity(lines * 50);
    source.push_str("// Generated TypeScript source for benchmarking\n\n");

    for i in 0..lines {
        match i % 5 {
            0 => source.push_str(&format!("const var{}: number = {};\n", i, i)),
            1 => source.push_str(&format!("const str{}: string = \"value{}\";\n", i, i)),
            2 => source.push_str(&format!(
                "function fn{}(x: number): number {{ return x * {}; }}\n",
                i, i
            )),
            3 => source.push_str(&format!(
                "interface I{} {{ value: number; name: string; }}\n",
                i
            )),
            _ => source.push_str(&format!("type T{} = {{ id: {}; data: string }};\n", i, i)),
        }
    }

    source
}

/// Count tokens in source
fn count_tokens(source: &str) -> usize {
    let mut scanner = ScannerState::new(source.to_string(), true);
    let mut count = 0;
    loop {
        let token = scanner.scan();
        if token == SyntaxKind::EndOfFileToken {
            break;
        }
        count += 1;
    }
    count
}

/// Benchmark: Scan small source file
fn bench_scan_small(c: &mut Criterion) {
    c.bench_function("scan_small", |b| {
        b.iter(|| {
            let mut scanner = ScannerState::new(black_box(SMALL_SOURCE.to_string()), true);
            loop {
                let token = scanner.scan();
                if token == SyntaxKind::EndOfFileToken {
                    break;
                }
                black_box(token);
            }
        })
    });
}

/// Benchmark: Scan medium source file
fn bench_scan_medium(c: &mut Criterion) {
    c.bench_function("scan_medium", |b| {
        b.iter(|| {
            let mut scanner = ScannerState::new(black_box(MEDIUM_SOURCE.to_string()), true);
            loop {
                let token = scanner.scan();
                if token == SyntaxKind::EndOfFileToken {
                    break;
                }
                black_box(token);
            }
        })
    });
}

/// Benchmark: Scan with throughput measurement
fn bench_scan_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("scanner_throughput");

    for size in [100, 500, 1000, 5000].iter() {
        let source = generate_large_source(*size);
        let bytes = source.len() as u64;

        group.throughput(Throughput::Bytes(bytes));
        group.bench_with_input(BenchmarkId::from_parameter(size), &source, |b, source| {
            b.iter(|| {
                let mut scanner = ScannerState::new(black_box(source.clone()), true);
                loop {
                    let token = scanner.scan();
                    if token == SyntaxKind::EndOfFileToken {
                        break;
                    }
                    black_box(token);
                }
            })
        });
    }

    group.finish();
}

/// Benchmark: Scanner with token value extraction
fn bench_scan_with_values(c: &mut Criterion) {
    c.bench_function("scan_with_values", |b| {
        b.iter(|| {
            let mut scanner = ScannerState::new(black_box(MEDIUM_SOURCE.to_string()), true);
            loop {
                let token = scanner.scan();
                if token == SyntaxKind::EndOfFileToken {
                    break;
                }
                // Also extract token value and position
                let _value = scanner.get_token_value();
                let _start = scanner.get_token_start();
                let _end = scanner.get_token_end();
                black_box(token);
            }
        })
    });
}

/// Benchmark: String allocation (Vec<char> creation)
fn bench_string_allocation(c: &mut Criterion) {
    let mut group = c.benchmark_group("string_allocation");

    for size in [1000, 10000, 50000].iter() {
        let source: String = "x".repeat(*size);

        group.bench_with_input(BenchmarkId::from_parameter(size), &source, |b, source| {
            b.iter(|| {
                let chars: Vec<char> = black_box(source.chars().collect());
                black_box(chars)
            })
        });
    }

    group.finish();
}

/// Benchmark: Keyword lookup
fn bench_keyword_lookup(c: &mut Criterion) {
    use wasm::scanner::text_to_keyword;

    let keywords = vec![
        "const",
        "let",
        "var",
        "function",
        "class",
        "interface",
        "type",
        "enum",
        "if",
        "else",
        "while",
        "for",
        "return",
        "import",
        "export",
        "from",
        "as",
        "async",
        "await",
        "notakeyword",
        "identifier",
        "someOtherWord",
    ];

    c.bench_function("keyword_lookup", |b| {
        b.iter(|| {
            for kw in &keywords {
                black_box(text_to_keyword(kw));
            }
        })
    });
}

criterion_group!(
    benches,
    bench_scan_small,
    bench_scan_medium,
    bench_scan_throughput,
    bench_scan_with_values,
    bench_string_allocation,
    bench_keyword_lookup,
);

criterion_main!(benches);
