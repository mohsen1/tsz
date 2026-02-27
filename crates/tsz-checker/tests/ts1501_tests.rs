//! Tests for TS1501: Regular expression flag target validation.
//!
//! TS1501 was removed in tsc 6.0 — the regex flag target check is no longer emitted.
//! These tests verify that we do NOT emit TS1501.

use tsz_binder::BinderState;
use tsz_checker::CheckerState;
use tsz_checker::context::CheckerOptions;
use tsz_common::common::ScriptTarget;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn has_ts1501(source: &str, target: ScriptTarget) -> bool {
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

    checker.ctx.diagnostics.iter().any(|d| d.code == 1501)
}

#[test]
fn ts1501_not_emitted_for_regex_flags() {
    // tsc 6.0 removed TS1501 — regex flag target checks are no longer emitted
    assert!(!has_ts1501("var x = /foo/u;", ScriptTarget::ES5));
    assert!(!has_ts1501("var x = /foo/y;", ScriptTarget::ES5));
    assert!(!has_ts1501("var x = /foo/s;", ScriptTarget::ES2015));
    assert!(!has_ts1501("var x = /foo/d;", ScriptTarget::ES2018));
    assert!(!has_ts1501("var x = /foo/gim;", ScriptTarget::ES3));
    assert!(!has_ts1501("var x = /foo/us;", ScriptTarget::ES5));
}
