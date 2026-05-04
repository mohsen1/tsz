//! Per-file `@jsx` / `@jsxFrag` pragma factories must mark the imports
//! they reference as used, so subsequent unused-import checks
//! (TS6133 / TS6192) don't false-positive on the import that brought
//! the factory into scope.
//!
//! Mirrors the `inlineJsxAndJsxFragPragma.tsx` conformance test
//! (subset that doesn't depend on cross-file resolver behavior).

use tsz_checker::CheckerState;
use tsz_common::checker_options::{CheckerOptions, JsxMode};
use tsz_common::diagnostics::Diagnostic;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn check_jsx_react(source: &str, file_name: &str) -> Vec<Diagnostic> {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let opts = CheckerOptions {
        jsx_mode: JsxMode::React,
        no_unused_locals: true,
        ..CheckerOptions::default()
    };
    let interner = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &interner,
        file_name.to_string(),
        opts,
    );
    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

/// `<><div></div></>` with `@jsx h, @jsxFrag Fragment` and both factories
/// imported. Both `h` and `Fragment` are used: `h` for the `<div>`,
/// `Fragment` for the `<>...</>`. Neither should be flagged TS6133 /
/// TS6192.
#[test]
fn pragma_factory_for_fragment_with_intrinsic_marks_both_factories_referenced() {
    let source = "\
/**
 * @jsx h
 * @jsxFrag Fragment
 */
declare function h(): void;
declare function Fragment(): void;
declare namespace JSX {
    interface IntrinsicElements { [e: string]: any; }
    interface Element {}
}
const _frag = <><div></div></>;
";
    let diags = check_jsx_react(source, "preacty.tsx");
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    let unused_codes: Vec<u32> = codes
        .iter()
        .copied()
        .filter(|c| matches!(c, 6133 | 6192))
        .collect();
    assert!(
        unused_codes.is_empty(),
        "Expected no TS6133/TS6192 — both `h` and `Fragment` are used by JSX, got: {diags:?}"
    );
}

/// `<></>` with `@jsx h, @jsxFrag null` — `null` is the user-driven
/// opt-out sentinel for the fragment factory, but the JSX factory `h`
/// is still conceptually used (fragments compile to `h(null, …)`).
#[test]
fn pragma_jsx_factory_with_null_jsxfrag_sentinel_still_marks_jsx_factory_referenced() {
    let source = "\
/* @jsx jsx */
/* @jsxfrag null */
declare function jsx(): void;
declare namespace JSX {
    interface IntrinsicElements { [e: string]: any; }
    interface Element {}
}
const _frag = <></>;
";
    let diags = check_jsx_react(source, "snabbdomy-only-fragment.tsx");
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    let unused_codes: Vec<u32> = codes
        .iter()
        .copied()
        .filter(|c| matches!(c, 6133 | 6192))
        .collect();
    assert!(
        unused_codes.is_empty(),
        "Expected no TS6133/TS6192 — `jsx` is used by the fragment, got: {diags:?}"
    );
}
