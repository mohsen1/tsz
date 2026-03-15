//! Tests for TS2347: Untyped function calls may not accept type arguments.

use crate::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn get_error_codes(source: &str) -> Vec<u32> {
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
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    checker.ctx.diagnostics.iter().map(|d| d.code).collect()
}

#[test]
fn test_bind_function_like_values_without_call_signatures_reject_type_arguments() {
    let codes = get_error_codes(
        r#"
declare var anyVar: any;
anyVar<string>("hello");
anyVar<number>();
anyVar<{}>(undefined);

interface SubFunc {
    bind(): void;
    prop: number;
}
declare var subFunc: SubFunc;
subFunc<number>(0);
subFunc<string>("");
subFunc<any>();
"#,
    );

    let count = codes.iter().filter(|&&code| code == 2347).count();
    assert_eq!(
        count, 6,
        "Should emit TS2347 for any and bind-based Function-like calls with type arguments, got: {codes:?}"
    );
}

#[test]
fn test_new_this_property_with_generic_construct_signature_no_ts2347() {
    // `new this.Map_<K, V>()` in a property initializer should NOT emit TS2347
    // when `Map_` has type `{ new<K, V>(): any }` — the construct signature IS generic.
    // The `any` type comes from `this` being unresolved during class construction,
    // not from the member's declared type lacking type parameters.
    let source = r#"
class MyMap<K, V> {
    constructor(private readonly Map_: { new<K, V>(): any }) {}
    private readonly store = new this.Map_<K, V>();
}
"#;
    let errors = get_error_codes(source);
    assert!(
        !errors.contains(&2347),
        "Should NOT emit TS2347 for `new this.Map_<K, V>()` when Map_ has generic construct sig.\nErrors: {errors:?}"
    );
}

#[test]
fn test_new_this_property_without_generic_construct_still_emits_ts2347() {
    // `new this.x<T>()` when `x` has type `any` and no known construct sig
    // SHOULD emit TS2347.
    let source = r#"
class Foo {
    x: any;
    y = new this.x<string>();
}
"#;
    let errors = get_error_codes(source);
    assert!(
        errors.contains(&2347),
        "SHOULD emit TS2347 for `new this.x<T>()` when x is declared as `any`.\nErrors: {errors:?}"
    );
}
