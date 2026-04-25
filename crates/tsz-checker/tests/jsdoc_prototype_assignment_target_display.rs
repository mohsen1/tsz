//! TS2322 source/target display for `/** @type {T} */ Foo.prototype = X`.
//!
//! Regression for `typeTagPrototypeAssignment.ts`: a JSDoc `@type` annotation
//! on a `Foo.prototype = X` assignment declares the prototype's type, not the
//! source RHS type. The diagnostic source must be the RHS's actual type
//! (`number` for `12`), not the JSDoc-declared target (`string`). This is the
//! same shape as the existing CommonJS `module.exports = X` carve-out.

use rustc_hash::FxHashSet;
use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn diagnostics_for_js(source: &str) -> Vec<(u32, String)> {
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions {
        allow_js: true,
        check_js: true,
        no_implicit_any: false,
        ..CheckerOptions::default()
    };
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.js".to_string(),
        options,
    );
    let _: FxHashSet<u32> = FxHashSet::default(); // keep import alive in case
    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

/// `/** @type {string} */ C.prototype = 12` must emit
/// `Type 'number' is not assignable to type 'string'.` — source uses the RHS's
/// actual type (`number`), not the JSDoc-declared target type (`string`).
#[test]
fn ts2322_for_prototype_jsdoc_assignment_uses_rhs_type_for_source() {
    let diags = diagnostics_for_js(
        r#"
function C() {}
/** @type {string} */
C.prototype = 12
"#,
    );
    let ts2322: Vec<_> = diags.iter().filter(|(c, _)| *c == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly one TS2322; got: {diags:?}"
    );
    let msg = &ts2322[0].1;
    assert!(
        msg.contains("'number'") && msg.contains("'string'"),
        "TS2322 must show source as 'number' (the RHS type) and target as 'string' (the JSDoc target); got: {msg:?}"
    );
    assert!(
        !msg.contains("Type 'string' is not assignable to type 'string'"),
        "TS2322 must not collapse both sides to the JSDoc-declared target type; got: {msg:?}"
    );
}
