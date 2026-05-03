//! Locks the `keyofAndIndexedAccess.ts:634:56` regression where
//! `Foo7<I7[K]>` (with `interface I7 { x: any; }` and
//! `K extends keyof I7`) emitted a spurious TS2538
//! ("Type 'any' cannot be used as an index type"). Our type resolution
//! collapses K to `any` when the constraint chain reaches a property
//! whose type is `any`, but tsc keeps the index syntactically generic
//! and defers the rejection to instantiation time. The fix in
//! `crates/tsz-checker/src/types/type_checking/indexed_access.rs`
//! consults the new helper
//! `query_boundaries::type_checking_utilities::ast_index_node_is_in_scope_type_parameter`
//! which falls back to the binder's lexical name resolver when
//! `type_parameter_scope` does not carry the enclosing function's K — a
//! common shape inside lazy/deferred indexed-access evaluation.

use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn check_without_lib(source: &str) -> Vec<Diagnostic> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );

    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

#[test]
fn type_param_indexed_access_into_any_property_does_not_emit_ts2538() {
    let diagnostics = check_without_lib(
        r#"
interface I7 {
    x: any;
}
type Foo7<T extends number> = T;
declare function f7<K extends keyof I7>(type: K): Foo7<I7[K]>;
"#,
    );
    let ts2538 = diagnostics
        .iter()
        .filter(|d| d.code == 2538)
        .collect::<Vec<_>>();
    assert!(
        ts2538.is_empty(),
        "Expected no TS2538 — `I7[K]` keeps K syntactically as a type \
         parameter even when its constraint resolves to `any` via \
         `I7.x: any`. Got: {diagnostics:?}"
    );
}
