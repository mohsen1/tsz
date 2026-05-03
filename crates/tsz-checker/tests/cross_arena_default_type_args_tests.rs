//! Locks the regression where a generic type alias referenced with fewer
//! arguments than declared (e.g. `IteratorResult<number>` for
//! `type IteratorResult<T, TReturn = any> = ...`) was producing a partial
//! `Application` because `get_type_params_for_symbol` returned a cached
//! placeholder Vec that had stripped each param's default. With defaults
//! missing, `fillMissingTypeArguments` short-circuited and no padding
//! occurred — leaving the Application with one fewer argument than the
//! definition's type-parameter count, which broke the variance fast-path
//! at `crates/tsz-solver/src/relations/subtype/rules/generics.rs:438`
//! (`if variances.len() == s_app.args.len()`). The fix in
//! `crates/tsz-checker/src/state/type_environment/core.rs:1541` drops the
//! `symbol_is_from_lib` gate from the placeholder-cache detection so the
//! slow-path re-collection runs whenever the cache is uniformly empty,
//! independently of arena origin.

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

/// Negative case: a class method whose return type is a generic alias
/// applied with fewer args than the alias declares (so the trailing args
/// must come from defaults) must not trigger TS2416 against an interface
/// method whose return type is the same alias applied with explicit args
/// when the explicit args are bidirectionally related to the defaults.
#[test]
fn alias_with_default_type_arg_implements_check_does_not_emit_ts2416() {
    let diagnostics = check_without_lib(
        r#"
interface IteratorYieldResult<TYield> {
    done?: false;
    value: TYield;
}
interface IteratorReturnResult<TReturn> {
    done: true;
    value: TReturn;
}
type IteratorResult<T, TReturn = any> =
    IteratorYieldResult<T> | IteratorReturnResult<TReturn>;

interface I<TR = any> {
    foo(): IteratorResult<number, TR>;
}

class C implements I<void> {
    foo(): IteratorResult<number> {
        return null as any;
    }
}
"#,
    );
    let ts2416 = diagnostics
        .iter()
        .filter(|d| d.code == 2416)
        .collect::<Vec<_>>();
    assert!(
        ts2416.is_empty(),
        "Expected no TS2416 — `IteratorResult<number>` with defaulted TReturn must \
         match `IteratorResult<number, void>` via fillMissingTypeArguments. Got: \
         {diagnostics:?}"
    );
}
