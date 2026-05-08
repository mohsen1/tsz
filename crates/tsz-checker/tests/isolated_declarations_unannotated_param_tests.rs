//! Issue #3709: under `--isolatedDeclarations`, an unannotated parameter
//! in an exported function must report TS9011 even when no initializer
//! is present. tsz previously gated TS9011 on `param.initializer.is_some()`,
//! so the `function f(x) { ... }` shape — the most common case — was
//! silently accepted and a `.d.ts` was emitted.

use crate::CheckerState;
use crate::context::CheckerOptions;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn check_isolated(source: &str) -> Vec<u32> {
    let mut parser = ParserState::new("repro.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions {
        isolated_declarations: true,
        emit_declarations: true,
        ..CheckerOptions::default()
    };
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "repro.ts".to_string(),
        options,
    );

    checker.check_source_file(root);
    checker.ctx.diagnostics.iter().map(|d| d.code).collect()
}

/// `export function f(x) { return x; }` — `x` is unannotated and there
/// is no initializer; tsc still emits TS9011.
#[test]
fn unannotated_param_without_initializer_reports_ts9011() {
    let codes = check_isolated("export function f(x) { return x; }\n");
    assert!(
        codes.contains(&9011),
        "expected TS9011 for unannotated parameter `x`, got codes {codes:?}"
    );
    assert!(
        codes.contains(&9007),
        "expected co-located TS9007 for the missing return-type annotation, got codes {codes:?}"
    );
}

/// Anchor: an annotated parameter must NOT trigger TS9011, even though
/// the function still misses an explicit return type (TS9007 only).
#[test]
fn annotated_param_with_missing_return_type_only_reports_ts9007() {
    let codes = check_isolated("export function f(x: number) { return x; }\n");
    assert!(
        !codes.contains(&9011),
        "did not expect TS9011 when parameter is annotated, got codes {codes:?}"
    );
    assert!(
        codes.contains(&9007),
        "expected TS9007 for missing return type, got codes {codes:?}"
    );
}

/// `this` is implicit and tsc does not require it to be annotated for
/// declaration emission.
#[test]
fn unannotated_this_parameter_does_not_report_ts9011() {
    let codes = check_isolated("export function f(this, x: number): number { return x; }\n");
    assert!(
        !codes.contains(&9011),
        "did not expect TS9011 for the implicit `this` parameter, got codes {codes:?}"
    );
}
