//! Contextual-callback re-entry microbench (#13250).
//!
//! Targets the `raw_block_body_callback_mismatch` re-entry surface in
//! `call_checker/diagnostics.rs`. Deeply nested, overloaded generic callbacks
//! force the block-body callback-mismatch derivation to re-run per overload
//! candidate and per call site. Without an outer per-arg memo, each re-entry
//! re-snapshots checker state, re-infers the callback body under contextual
//! typing, and re-scans diagnostics.
//!
//! A/B is driven by the kill-switch env var on a single binary
//! (`TSZ_DISABLE_CALLBACK_MISMATCH_MEMO=1` reverts to the legacy recompute) so
//! the measurement isolates the memo from codegen drift.

use criterion::{Criterion, criterion_group, criterion_main};
use tsz_solver::construction::{QueryCache, TypeInterner};

/// Emit an overload set whose callback parameter type differs per overload, so
/// overload resolution re-derives the callback-mismatch check repeatedly for
/// the same inline block-body argument.
fn overloaded_pipe_decls() -> String {
    let mut src = String::new();
    // A function with many overloads, each taking a callback whose parameter
    // and return shape differs.  Overload resolution evaluates every candidate
    // that "succeeds" against the inline callback, re-running the mismatch
    // derivation for the same callback node.
    src.push_str(
        r#"
interface Box<T> { value: T; map<U>(f: (v: T) => U): Box<U>; }
declare function pipe<A>(f: (a: A) => A): A;
declare function pipe<A, B>(f1: (a: A) => B, f2: (b: B) => A): A;
declare function pipe<A, B, C>(f1: (a: A) => B, f2: (b: B) => C, f3: (c: C) => A): A;
declare function each<T>(xs: T[], f: (x: T) => void): void;
declare function reduceTo<T, U>(xs: T[], f: (acc: U, x: T) => U, init: U): U;
"#,
    );
    src
}

/// One deeply nested callback chain that re-enters the contextual-callback
/// mismatch derivation many times: nested `map`/`reduce`/`pipe` block-body
/// callbacks, each forcing a contextual re-inference of the inner callback.
fn nested_callback_chain(i: usize) -> String {
    format!(
        r#"
function chain{i}(box: Box<number>, items: number[]) {{
    const r{i} = box
        .map((v) => {{
            const inner = reduceTo(items, (acc, x) => {{
                const step = pipe(
                    (a: number) => {{ return a + v; }},
                    (b: number) => {{ return b * x; }},
                    (c: number) => {{ return c - acc; }},
                );
                return acc + step;
            }}, 0);
            each(items, (y) => {{
                const z = box.map((w) => {{ return w + y + inner; }});
                void z;
            }});
            return inner + v;
        }})
        .map((n) => {{
            return reduceTo(items, (acc, x) => {{ return acc + x + n; }}, 0);
        }});
    return r{i};
}}
"#
    )
}

fn generate_source(chain_count: usize) -> String {
    let mut src = String::with_capacity(chain_count * 800 + 512);
    src.push_str(&overloaded_pipe_decls());
    for i in 0..chain_count {
        src.push_str(&nested_callback_chain(i));
    }
    src
}

/// A chain whose inline block-body callbacks force the mismatch derivation to
/// return `Some` (a real callback return-type/body mismatch), so the parity
/// dump also covers the error-emitting outcome of the memoized derivation.
fn mismatching_callback_chain(i: usize) -> String {
    format!(
        r#"
interface Typed{i} {{ map(f: (v: number) => string): void; }}
declare const typed{i}: Typed{i};
function mismatch{i}(items: number[]) {{
    // Callback body returns `number` where `string` is expected.
    typed{i}.map((v) => {{
        const doubled = v * 2;
        return doubled;
    }});
    // Overloaded call where every overload re-derives the callback mismatch.
    each(items, (x) => {{
        const r = reduceTo(items, (acc, y) => {{ return acc + y + x; }}, 0);
        const bad: string = r;
        void bad;
    }});
}}
"#
    )
}

fn generate_mismatch_source(chain_count: usize) -> String {
    let mut src = String::with_capacity(chain_count * 600 + 512);
    src.push_str(&overloaded_pipe_decls());
    for i in 0..chain_count {
        src.push_str(&mismatching_callback_chain(i));
    }
    src
}

fn check_diagnostics(source: &str) -> Vec<(u32, u32, u32, String)> {
    let mut parser = tsz_core::parser::ParserState::new("bench.ts".to_string(), source.to_string());
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
    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.start, d.length, d.message_text.clone()))
        .collect()
}

fn check_once(source: &str) -> usize {
    check_diagnostics(source).len()
}

fn bench_contextual_callback_reentry(c: &mut Criterion) {
    // Parity-dump mode (#13250): write the normalized diagnostic set for each
    // fixture size to /tmp so the memo-on and memo-off runs can be diffed for
    // byte-identical output. Gated so it never affects timing samples.
    if std::env::var_os("CBM_PARITY_DUMP").is_some() {
        use std::fmt::Write as _;
        let mut out = String::new();
        for &n in &[20usize, 40, 80] {
            let source = generate_source(n);
            let diags = check_diagnostics(&source);
            let _ = writeln!(out, "=== chains_{n}: {} diagnostics ===", diags.len());
            for (code, start, length, msg) in &diags {
                let _ = writeln!(out, "TS{code} @{start}+{length}: {msg}");
            }
        }
        for &n in &[5usize, 20, 40] {
            let source = generate_mismatch_source(n);
            let diags = check_diagnostics(&source);
            let _ = writeln!(out, "=== mismatch_{n}: {} diagnostics ===", diags.len());
            for (code, start, length, msg) in &diags {
                let _ = writeln!(out, "TS{code} @{start}+{length}: {msg}");
            }
        }
        let path = std::env::var("CBM_PARITY_DUMP").unwrap_or_default();
        let path = if path.is_empty() || path == "1" {
            "/tmp/cbm_parity.txt".to_string()
        } else {
            path
        };
        std::fs::write(&path, out).expect("write parity dump");
        return;
    }
    let mut group = c.benchmark_group("contextual_callback_reentry");
    group.sample_size(20);
    for &n in &[20usize, 40, 80] {
        let source = generate_source(n);
        group.bench_function(format!("chains_{n}"), |b| {
            b.iter(|| criterion::black_box(check_once(&source)));
        });
    }
    group.finish();
}

criterion_group!(benches, bench_contextual_callback_reentry);
criterion_main!(benches);
