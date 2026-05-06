//! Tests for TS1501: Regular expression flag target validation.
//!
//! TypeScript 6.0 still emits TS1501 for the `v` regular-expression flag
//! when targeting below ES2024.

use tsz_binder::BinderState;
use tsz_checker::CheckerState;
use tsz_checker::context::CheckerOptions;
use tsz_common::common::ScriptTarget;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn diagnostics_for(source: &str, target: ScriptTarget) -> Vec<tsz_common::diagnostics::Diagnostic> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions {
            target,
            ..Default::default()
        },
    );

    checker.check_source_file(root);

    checker.ctx.diagnostics
}

fn has_ts1501(source: &str, target: ScriptTarget) -> bool {
    diagnostics_for(source, target)
        .iter()
        .any(|d| d.code == 1501)
}

fn ts1501_diagnostic(source: &str, target: ScriptTarget) -> tsz_common::diagnostics::Diagnostic {
    diagnostics_for(source, target)
        .into_iter()
        .find(|d| d.code == 1501)
        .expect("expected TS1501 diagnostic")
}

#[test]
fn v_flag_requires_es2024_or_later() {
    assert!(has_ts1501("var x = /foo/v;", ScriptTarget::ES5));
    assert!(has_ts1501("var x = /foo/v;", ScriptTarget::ES2022));
    assert!(has_ts1501("var x = /foo/v;", ScriptTarget::ES2023));
    assert!(has_ts1501("const r = /[a&&b]/v;", ScriptTarget::ES2022));
}

#[test]
fn v_flag_ts1501_points_to_flag() {
    let diagnostic = ts1501_diagnostic("const r = /[a&&b]/v;", ScriptTarget::ES2022);

    assert_eq!(diagnostic.start, 18);
    assert_eq!(diagnostic.length, 1);
    assert_eq!(
        diagnostic.message_text,
        "This regular expression flag is only available when targeting 'es2024' or later."
    );
}

#[test]
fn v_flag_is_allowed_for_es2024_or_later() {
    assert!(!has_ts1501("var x = /foo/v;", ScriptTarget::ES2024));
    assert!(!has_ts1501("var x = /foo/v;", ScriptTarget::ES2025));
    assert!(!has_ts1501("var x = /foo/v;", ScriptTarget::ESNext));
}

#[test]
fn ts1501_not_emitted_for_other_regex_flags() {
    assert!(!has_ts1501("var x = /foo/u;", ScriptTarget::ES5));
    assert!(!has_ts1501("var x = /foo/y;", ScriptTarget::ES5));
    assert!(!has_ts1501("var x = /foo/s;", ScriptTarget::ES2015));
    assert!(!has_ts1501("var x = /foo/d;", ScriptTarget::ES2018));
    assert!(!has_ts1501("var x = /foo/gim;", ScriptTarget::ES3));
    assert!(!has_ts1501("var x = /foo/us;", ScriptTarget::ES5));
}
