//! Regression coverage for issue #2942: JSDoc pragmas like `@jsxRuntime`,
//! `@jsxImportSource`, and `@jsxFrag` must be parsed as complete tags
//! followed by a whitespace boundary, not as raw substring/prefix matches.
//!
//! Each "invalid" case below was previously misrecognized by tsz and changed
//! JSX checking even though tsc treats the comment as an unrelated/unknown
//! JSDoc tag. The "valid" controls protect against an over-eager fix that
//! would also drop legitimate pragmas.

use tsz_checker::CheckerState;
use tsz_common::checker_options::{CheckerOptions, JsxMode};
use tsz_common::diagnostics::Diagnostic;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn check_with_jsx_mode(source: &str, file_name: &str, jsx_mode: JsxMode) -> Vec<Diagnostic> {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let opts = CheckerOptions {
        jsx_mode,
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
    // TS2875 (missing `<source>/jsx-runtime` for automatic mode) is gated on
    // `report_unresolved_imports`, which mirrors the production CLI default
    // when `noResolve` is off. The unit-test default is `false`, so enable it
    // explicitly to exercise the import-source diagnostic path.
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

fn diag_codes(diags: &[Diagnostic]) -> Vec<u32> {
    diags.iter().map(|d| d.code).collect()
}

// ---------------------------------------------------------------------------
// @jsxRuntime
// ---------------------------------------------------------------------------

/// `@jsxRuntimeautomatic` (no whitespace separator) is NOT the `@jsxRuntime`
/// pragma — tsc keeps the configured classic runtime, so the missing
/// `React` factory must surface (TS2874), not the missing
/// `react/jsx-runtime` module (TS2875).
#[test]
fn jsx_runtime_prefix_tag_does_not_switch_to_automatic() {
    let source = "\
/** @jsxRuntimeautomatic */

declare namespace JSX {
    interface IntrinsicElements { div: {}; }
    interface Element {}
}

const _e = <div />;
";
    let diags = check_with_jsx_mode(source, "prefix.tsx", JsxMode::React);
    let codes = diag_codes(&diags);
    assert!(
        codes.contains(&2874),
        "Expected TS2874 (React factory not in scope) — pragma must be ignored. Got: {diags:?}"
    );
    assert!(
        !codes.contains(&2875),
        "TS2875 must NOT fire — `@jsxRuntimeautomatic` is not a real pragma. Got: {diags:?}"
    );
}

/// `@jsxRuntime automaticx` (junk suffix on the value) is also ignored;
/// only `classic` and `automatic` are recognized values.
#[test]
fn jsx_runtime_invalid_value_with_suffix_does_not_switch_modes() {
    let source = "\
/** @jsxRuntime automaticx */

declare namespace JSX {
    interface IntrinsicElements { div: {}; }
    interface Element {}
}

const _e = <div />;
";
    let diags = check_with_jsx_mode(source, "valuex.tsx", JsxMode::React);
    let codes = diag_codes(&diags);
    assert!(
        codes.contains(&2874),
        "Expected TS2874 — `automaticx` is not a valid value, pragma must be ignored. Got: {diags:?}"
    );
    assert!(
        !codes.contains(&2875),
        "TS2875 must NOT fire for invalid value `automaticx`. Got: {diags:?}"
    );
}

/// Control: a properly-formed `@jsxRuntime automatic` does switch to
/// automatic runtime, so `react/jsx-runtime` is required (TS2875) and
/// the React-factory check (TS2874) does not apply.
#[test]
fn jsx_runtime_valid_automatic_switches_modes() {
    let source = "\
/** @jsxRuntime automatic */

declare namespace JSX {
    interface IntrinsicElements { div: {}; }
    interface Element {}
}

const _e = <div />;
";
    let diags = check_with_jsx_mode(source, "valid-auto.tsx", JsxMode::React);
    let codes = diag_codes(&diags);
    assert!(
        !codes.contains(&2874),
        "TS2874 must NOT fire — automatic runtime doesn't require React in scope. Got: {diags:?}"
    );
    assert!(
        codes.contains(&2875),
        "Expected TS2875 (missing `react/jsx-runtime`) under automatic mode. Got: {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// @jsxImportSource
// ---------------------------------------------------------------------------

/// `@jsxImportSourcex preact` is NOT the `@jsxImportSource` pragma — tsz
/// previously parsed package `x` out of it. After the fix it falls back to
/// the default (`react`), so the deferred TS2875 references
/// `react/jsx-runtime`, not `x/jsx-runtime`.
#[test]
fn jsx_import_source_prefix_tag_does_not_set_package() {
    let source = "\
/** @jsxImportSourcex preact */

declare namespace JSX {
    interface IntrinsicElements { div: {}; }
    interface Element {}
}

const _e = <div />;
";
    let diags = check_with_jsx_mode(source, "import-prefix.tsx", JsxMode::ReactJsx);
    let ts2875: Vec<&Diagnostic> = diags.iter().filter(|d| d.code == 2875).collect();
    assert!(
        !ts2875.is_empty(),
        "Expected TS2875 referencing the default `react/jsx-runtime`. Got: {diags:?}"
    );
    for d in &ts2875 {
        assert!(
            !d.message_text.contains("'x/jsx-runtime'"),
            "TS2875 must not mention `x/jsx-runtime` — `@jsxImportSourcex` is not a real pragma. \
             Got message: {}",
            d.message_text
        );
        assert!(
            !d.message_text.contains("'preact/jsx-runtime'"),
            "TS2875 must not mention `preact/jsx-runtime` — invalid pragma must not change source. \
             Got message: {}",
            d.message_text
        );
    }
}

/// Control: a real `@jsxImportSource preact` does change the runtime path.
#[test]
fn jsx_import_source_valid_pragma_changes_package() {
    let source = "\
/** @jsxImportSource preact */

declare namespace JSX {
    interface IntrinsicElements { div: {}; }
    interface Element {}
}

const _e = <div />;
";
    let diags = check_with_jsx_mode(source, "import-valid.tsx", JsxMode::ReactJsx);
    let ts2875_with_preact = diags
        .iter()
        .any(|d| d.code == 2875 && d.message_text.contains("'preact/jsx-runtime'"));
    assert!(
        ts2875_with_preact,
        "Expected TS2875 referencing `preact/jsx-runtime` under valid pragma. Got: {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// @jsxFrag
// ---------------------------------------------------------------------------

/// `@jsxFragx Fragment` is NOT the `@jsxFrag` pragma — tsz previously
/// extracted `x` as the fragment factory. After the fix the pragma is
/// ignored, so the default fragment factory (`React.Fragment`) governs the
/// scope check, which here means TS2874 fires for missing `React`.
#[test]
fn jsx_frag_prefix_tag_does_not_set_fragment_factory() {
    let source = "\
/** @jsx h */
/** @jsxFragx Fragment */

declare function h(): void;
declare namespace JSX {
    interface IntrinsicElements { [e: string]: any; }
    interface Element {}
}

const _frag = <></>;
";
    let diags = check_with_jsx_mode(source, "frag-prefix.tsx", JsxMode::React);
    // The fragment factory pragma is not recognized, so tsc emits TS17017
    // ("An @jsxFrag pragma is required when using an @jsx pragma with JSX
    // fragments"). Either way, what we MUST NOT see is a TS2874 message
    // mentioning the bogus identifier `x`.
    for d in &diags {
        assert!(
            !d.message_text.contains("'x'"),
            "Diagnostic must not reference `x` — `@jsxFragx` is not a real pragma. \
             Got: {d:?}"
        );
    }
}

/// Control: `@jsxFragment Foo` (long form) is recognized, so the fragment
/// factory becomes `Foo` and TS2874 references `Foo` if missing.
#[test]
fn jsx_fragment_long_form_pragma_sets_fragment_factory() {
    let source = "\
/** @jsx h */
/** @jsxFragment Foo */

declare function h(): void;
declare namespace JSX {
    interface IntrinsicElements { [e: string]: any; }
    interface Element {}
}

const _frag = <></>;
";
    let diags = check_with_jsx_mode(source, "frag-long.tsx", JsxMode::React);
    let mentions_foo = diags
        .iter()
        .any(|d| d.code == 2874 && d.message_text.contains("'Foo'"));
    assert!(
        mentions_foo,
        "Expected TS2874 mentioning `Foo` from `@jsxFragment Foo`. Got: {diags:?}"
    );
}
