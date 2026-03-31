use crate::binder::BinderState;
use crate::checker::context::CheckerOptions;
use crate::checker::state::CheckerState;
use crate::parser::ParserState;
use tsz_solver::TypeInterner;

#[test]
fn test_jsdoc_constructor_overload_ts2394() {
    // Reproduces overloadTag2.ts: JSDoc @overload on constructor where
    // one overload has fewer params than the implementation.
    // The overload `@overload @param {number} a` takes 1 param but
    // the implementation takes 2 (a, b), so TS2394 should be emitted.
    let source = r#"export class Foo {
    #a = true ? 1 : "1"
    #b

    /**
     * @constructor
     * @overload
     * @param {string} a
     * @param {number} b
     */
    /**
     * @constructor
     * @overload
     * @param {number} a
     */
    /**
     * @constructor
     * @overload
     * @param {string} a
     *//**
     * @constructor
     * @param {number | string} a
     */
    constructor(a, b) {
        this.#a = a
        this.#b = b
    }
}
var a = new Foo()
var b = new Foo('str')
var c = new Foo(2)
var d = new Foo('str', 2)
"#;
    let mut parser = ParserState::new("overloadTag2.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut opts = CheckerOptions::default();
    opts.check_js = true;
    opts.allow_js = true;
    // strict mode enables noImplicitAny etc.
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "overloadTag2.js".to_string(),
        opts,
    );
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2394),
        "Expected TS2394 for incompatible JSDoc constructor overload, got: {codes:?}"
    );
}

#[test]
fn test_overload_compatibility_parameter_property() {
    let source = r#"
class C1 {
    constructor(public p1: string);
    constructor(public p3: any) {}
}
"#;
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
        CheckerOptions::default(),
    );
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    // TSC does not report TS2394 here.
    assert!(
        !codes.contains(&2394),
        "Unexpected TS2394 for overload compatibility with parameter property, got: {codes:?}"
    );
}

#[test]
fn test_overload_compatibility_untyped_impl_params_and_return() {
    let source = r#"
function f(x: string): string;
function f(x) { return x; }
"#;
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
        CheckerOptions::default(),
    );
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2394),
        "Unexpected TS2394 for untyped overload implementation, got: {codes:?}"
    );
}
