//! Phase Timing Benchmark
//!
//! Measures parse, bind, and check times SEPARATELY on TypeScript files of
//! increasing sizes. This answers the critical question:
//!
//!   "Is declaration-level incremental checking worth the complexity,
//!    or is whole-file re-checking fast enough in Rust?"
//!
//! If checking a 2000-line file takes 5ms total, declaration-level saves ~4ms.
//! Nobody perceives that. If it takes 200ms, saving 180ms matters.

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use std::time::Duration;
use tsz_solver::{QueryCache, TypeInterner};

fn generate_ts_file_range(start_index: usize, decl_count: usize) -> String {
    let mut src = String::with_capacity(decl_count * 400);
    src.push_str("// Generated TypeScript file for phase timing benchmark\n\n");

    for offset in 0..decl_count {
        let i = start_index + offset;
        match i % 5 {
            0 => {
                // Function with type annotations and body
                src.push_str(&format!(
                    r#"function process{i}(input: string, count: number): {{ result: string; total: number }} {{
    let accumulated = "";
    for (let j = 0; j < count; j++) {{
        accumulated += input;
    }}
    const total = accumulated.length * count;
    if (total > 100) {{
        return {{ result: accumulated.slice(0, 100), total: 100 }};
    }}
    return {{ result: accumulated, total }};
}}

"#
                ));
            }
            1 => {
                // Interface with several members
                src.push_str(&format!(
                    r#"interface Config{i} {{
    readonly id: number;
    name: string;
    enabled: boolean;
    tags: string[];
    metadata?: Record<string, unknown>;
    process(input: string): string;
    validate(): boolean;
}}

"#
                ));
            }
            2 => {
                // Class with constructor, methods, properties
                src.push_str(&format!(
                    r#"class Service{i} {{
    private readonly id: number;
    private data: Map<string, number>;
    public name: string;

    constructor(id: number, name: string) {{
        this.id = id;
        this.name = name;
        this.data = new Map();
    }}

    public getId(): number {{
        return this.id;
    }}

    public setData(key: string, value: number): void {{
        this.data.set(key, value);
    }}

    public getData(key: string): number | undefined {{
        return this.data.get(key);
    }}

    public toJSON(): {{ id: number; name: string }} {{
        return {{ id: this.id, name: this.name }};
    }}
}}

"#
                ));
            }
            3 => {
                // Type aliases with unions, intersections, generics
                src.push_str(&format!(
                    r#"type Result{i}<T, E = Error> = {{ ok: true; value: T }} | {{ ok: false; error: E }};
type Handler{i} = (event: string, data: unknown) => void;
type Partial{i}<T> = {{ [K in keyof T]?: T[K] }};

"#
                ));
            }
            4 => {
                // Enum + const declarations
                src.push_str(&format!(
                    r#"enum Status{i} {{
    Active = "active",
    Inactive = "inactive",
    Pending = "pending",
    Error = "error",
}}

const DEFAULT_CONFIG_{i}: {{ status: Status{i}; retries: number }} = {{
    status: Status{i}.Active,
    retries: 3,
}};

"#
                ));
            }
            _ => panic!("internal error: unsupported source segment kind"),
        }
    }
    src
}

/// Generate a TypeScript file with N top-level declarations.
/// Mix of functions, classes, interfaces, type aliases — realistic variety.
fn generate_ts_file(decl_count: usize) -> String {
    generate_ts_file_range(0, decl_count)
}

/// Count approximate lines in generated source
fn count_lines(s: &str) -> usize {
    s.lines().count()
}

fn generate_multi_file_repo(file_count: usize, decls_per_file: usize) -> Vec<(String, String)> {
    (0..file_count)
        .map(|file_idx| {
            (
                format!("bench/file{file_idx}.ts"),
                generate_ts_file_range(file_idx * decls_per_file, decls_per_file),
            )
        })
        .collect()
}

fn strict_checker_options() -> tsz_core::checker::context::CheckerOptions {
    tsz_core::checker::context::CheckerOptions {
        strict: true,
        no_implicit_any: true,
        strict_null_checks: true,
        strict_function_types: true,
        ..Default::default()
    }
}

fn bench_phase_timing(c: &mut Criterion) {
    let mut group = c.benchmark_group("phase_timing");
    group.warm_up_time(Duration::from_secs(2));
    group.measurement_time(Duration::from_secs(5));
    group.sample_size(30);

    // Test at different file sizes
    for decl_count in [25, 50, 100, 200, 400] {
        let source = generate_ts_file(decl_count);
        let lines = count_lines(&source);
        let label = format!("{decl_count}decls_{lines}lines");

        // Phase 1: Parse only
        group.bench_with_input(BenchmarkId::new("1_parse", &label), &source, |b, src| {
            b.iter(|| {
                let mut parser =
                    tsz_core::parser::ParserState::new("bench.ts".to_string(), src.clone());
                let _root = parser.parse_source_file();
                criterion::black_box(&parser);
            });
        });

        // Phase 2: Parse + Bind
        group.bench_with_input(
            BenchmarkId::new("2_parse_bind", &label),
            &source,
            |b, src| {
                b.iter(|| {
                    let mut parser =
                        tsz_core::parser::ParserState::new("bench.ts".to_string(), src.clone());
                    let root = parser.parse_source_file();

                    let mut binder = tsz_core::binder::BinderState::new();
                    binder.bind_source_file(parser.get_arena(), root);
                    criterion::black_box(&binder);
                });
            },
        );

        // Phase 3: Parse + Bind + Check (full pipeline)
        group.bench_with_input(
            BenchmarkId::new("3_parse_bind_check", &label),
            &source,
            |b, src| {
                b.iter(|| {
                    let mut parser =
                        tsz_core::parser::ParserState::new("bench.ts".to_string(), src.clone());
                    let root = parser.parse_source_file();

                    let mut binder = tsz_core::binder::BinderState::new();
                    binder.bind_source_file(parser.get_arena(), root);

                    let interner = TypeInterner::new();
                    let query_cache = QueryCache::new(&interner);
                    let options = tsz_core::checker::context::CheckerOptions {
                        strict: true,
                        no_implicit_any: true,
                        strict_null_checks: true,
                        strict_function_types: true,
                        ..Default::default()
                    };

                    let mut checker = tsz_core::checker::state::CheckerState::new(
                        parser.get_arena(),
                        &binder,
                        &query_cache,
                        "bench.ts".to_string(),
                        options,
                    );
                    checker.check_source_file(root);

                    criterion::black_box(checker.ctx.diagnostics.len());
                });
            },
        );
    }

    group.finish();
}

/// Compare: check with warm cache vs cold cache
/// This shows how much `TypeCache` reuse matters
fn bench_cache_reuse(c: &mut Criterion) {
    let mut group = c.benchmark_group("cache_reuse");
    group.warm_up_time(Duration::from_secs(2));
    group.measurement_time(Duration::from_secs(5));
    group.sample_size(20);

    let source = generate_ts_file(100);
    let lines = count_lines(&source);

    // Pre-parse and pre-bind (these would be cached in a real LSP)
    let mut parser = tsz_core::parser::ParserState::new("bench.ts".to_string(), source.clone());
    let root = parser.parse_source_file();
    let mut binder = tsz_core::binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let label = format!("100decls_{lines}lines");

    // Cold check (no cache — what Salsa Phase 1 does)
    group.bench_with_input(
        BenchmarkId::new("cold_check", &label),
        &source,
        |b, _src| {
            b.iter(|| {
                let interner = TypeInterner::new();
                let query_cache = QueryCache::new(&interner);
                let options = tsz_core::checker::context::CheckerOptions {
                    strict: true,
                    no_implicit_any: true,
                    strict_null_checks: true,
                    strict_function_types: true,
                    ..Default::default()
                };

                let mut checker = tsz_core::checker::state::CheckerState::new(
                    parser.get_arena(),
                    &binder,
                    &query_cache,
                    "bench.ts".to_string(),
                    options,
                );
                checker.check_source_file(root);
                criterion::black_box(checker.ctx.diagnostics.len());
            });
        },
    );

    group.finish();
}

fn bench_multi_file_phase_timing(c: &mut Criterion) {
    let mut group = c.benchmark_group("multi_file_phase_timing");
    group.warm_up_time(Duration::from_secs(2));
    group.measurement_time(Duration::from_secs(5));
    group.sample_size(20);

    let lib_paths =
        tsz_core::config::resolve_default_lib_files(tsz_core::emitter::ScriptTarget::ESNext)
            .expect("default lib resolution should succeed for benchmark");
    let lib_path_refs: Vec<&std::path::Path> =
        lib_paths.iter().map(std::path::PathBuf::as_path).collect();
    let lib_files = tsz_core::parallel::load_lib_files_for_binding_strict(&lib_path_refs)
        .expect("default lib loading should succeed for benchmark");
    let checker_options = strict_checker_options();

    for file_count in [4usize, 16, 64] {
        let files = generate_multi_file_repo(file_count, 25);
        let prebound =
            tsz_core::parallel::parse_and_bind_parallel_with_libs(files.clone(), &lib_files);
        let program = tsz_core::parallel::merge_bind_results(prebound);
        let residency = program.residency_stats();
        let label = format!(
            "{file_count}files_25decls_{}arenas_{}globals",
            residency.unique_arena_count, residency.global_symbol_count
        );

        group.bench_with_input(
            BenchmarkId::new("merge_bind_residency", &label),
            &files,
            |b, files| {
                b.iter(|| {
                    let bind_results = tsz_core::parallel::parse_and_bind_parallel_with_libs(
                        files.clone(),
                        &lib_files,
                    );
                    let program = tsz_core::parallel::merge_bind_results(bind_results);
                    criterion::black_box(program.residency_stats());
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("check_files_with_stats", &label),
            &files,
            |b, files| {
                b.iter(|| {
                    let bind_results = tsz_core::parallel::parse_and_bind_parallel_with_libs(
                        files.clone(),
                        &lib_files,
                    );
                    let program = tsz_core::parallel::merge_bind_results(bind_results);
                    let (_result, stats) = tsz_core::parallel::check_files_with_stats(
                        &program,
                        &checker_options,
                        &lib_files,
                    );
                    criterion::black_box(stats.program_residency);
                    criterion::black_box(stats.diagnostic_count);
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_phase_timing,
    bench_cache_reuse,
    bench_multi_file_phase_timing
);
criterion_main!(benches);
