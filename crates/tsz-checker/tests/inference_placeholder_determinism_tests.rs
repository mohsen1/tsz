//! Regression coverage for #10917: inference-placeholder (`__infer_*`) names
//! must be deterministic across repeated runs and across parallel file checks.
//!
//! Inference placeholders used to be named from a process-global atomic
//! counter. Its value depended on how many placeholders earlier files had
//! allocated and on the interleaving of parallel file checks, so any
//! placeholder name that leaked into a diagnostic (e.g. an unresolved `infer`
//! witness) changed from one run to the next. Names are now derived from a
//! deterministic per-file counter namespaced by the file index, so the same
//! input always produces the same names.

use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_checker::test_utils::check_source;
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;

/// The placeholder id generator must be deterministic (identical sequence on a
/// re-begin of the same file), namespaced by file index (so distinct files can
/// never collide), and reset per file.
#[test]
fn placeholder_ids_are_deterministic_namespaced_and_reset() {
    let mut parser = ParserState::new("a.ts".to_string(), "const x = 1;".to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "a.ts".to_string(),
        CheckerOptions::default(),
    );

    // File 0: a fresh per-file scope yields 0, 1, 2, 3, ...
    checker.ctx.set_current_file_idx(0);
    checker.ctx.begin_file_inference_placeholders();
    let run0: Vec<u64> = (0..4)
        .map(|_| checker.ctx.next_inference_placeholder_id())
        .collect();
    assert_eq!(run0, vec![0, 1, 2, 3]);

    // Re-checking the same file must reproduce the identical sequence — this is
    // the cross-run determinism the old global counter violated.
    checker.ctx.begin_file_inference_placeholders();
    let run0_again: Vec<u64> = (0..4)
        .map(|_| checker.ctx.next_inference_placeholder_id())
        .collect();
    assert_eq!(
        run0, run0_again,
        "the same file must produce identical placeholder ids across runs"
    );

    // A different file occupies a disjoint namespace, so its ids can never
    // collide with file 0's — this is what previously required the global
    // counter and is now provided deterministically.
    checker.ctx.set_current_file_idx(7);
    checker.ctx.begin_file_inference_placeholders();
    let run7: Vec<u64> = (0..4)
        .map(|_| checker.ctx.next_inference_placeholder_id())
        .collect();
    let base = 7u64 << 32;
    assert_eq!(run7, vec![base, base + 1, base + 2, base + 3]);
    for id in &run0 {
        assert!(
            !run7.contains(id),
            "placeholder ids must not collide across files"
        );
    }
}

/// End-to-end guard: checking the same generic-inference-heavy source multiple
/// times in one process must yield byte-identical diagnostics, and no internal
/// placeholder may ever leak into a user diagnostic. With the old global
/// counter, a leaked placeholder name would drift between runs.
#[test]
fn repeated_checks_produce_identical_diagnostics() {
    // A mix of generic calls, higher-order composition, and conditional/`infer`
    // types that exercise the placeholder-allocating inference paths.
    let source = r#"
        declare function id<T>(x: T): T;
        declare function compose<A, B, C>(f: (a: A) => B, g: (b: B) => C): (a: A) => C;
        type Elem<T> = T extends Array<infer U> ? U : never;
        declare function head<T>(xs: T[]): Elem<T[]>;

        const a: string = id(123);
        const c = compose((n: number) => String(n), (s: string) => s.length);
        const bad: boolean = c(1);
        const h: string = head([1, 2, 3]);
    "#;

    let render = || {
        let mut messages: Vec<String> = check_source(source, "repro.ts", CheckerOptions::default())
            .into_iter()
            .map(|d| format!("{}:{}", d.code, d.message_text))
            .collect();
        messages.sort();
        messages
    };

    let first = render();
    assert!(
        !first.is_empty(),
        "expected the repro to produce diagnostics so the determinism check is meaningful"
    );
    for _ in 0..5 {
        assert_eq!(
            first,
            render(),
            "repeated checks of the same source must produce identical diagnostics"
        );
    }
    // No internal inference placeholder should ever leak into a user diagnostic.
    for message in &first {
        assert!(
            !message.contains("__infer_"),
            "diagnostic leaked an internal inference placeholder: {message}"
        );
    }
}
