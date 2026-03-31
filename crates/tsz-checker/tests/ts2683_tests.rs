//! Tests for TS2683: 'this' implicitly has type 'any' because it does not have a type annotation.
//! This fires when noImplicitThis is on and `this` is used in a regular function
//! (not arrow) without a `this:` parameter annotation.

use tsz_binder::BinderState;
use tsz_checker::{CheckerOptions, CheckerState};
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn get_diagnostics(source: &str) -> Vec<(u32, String)> {
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
            strict: true,
            strict_null_checks: true,
            no_implicit_this: true,
            ..CheckerOptions::default()
        },
    );

    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn has_error(source: &str, code: u32) -> bool {
    get_diagnostics(source).iter().any(|d| d.0 == code)
}

#[test]
fn nested_function_in_class_constructor_emits_ts2683() {
    // `this` inside a regular function nested in a class constructor gets TS2683
    let src = r#"
class C {
    x!: number;
    constructor() {
        this.x = function() { return this.x; }();
    }
}
"#;
    assert!(has_error(src, 2683));
}

#[test]
fn nested_function_in_class_method_emits_ts2683() {
    // `this` inside a regular function nested in a class method gets TS2683
    let src = r#"
class C {
    data: number[] = [];
    findRaw() {
        this.data.find(function(d) {
            return d === this.data.length;
        });
    }
}
"#;
    assert!(has_error(src, 2683));
}

#[test]
fn direct_class_method_this_no_ts2683() {
    // `this` directly in a class method should NOT get TS2683 — it's typed by the class
    let src = r#"
class C {
    x: number = 1;
    method() { return this.x; }
}
"#;
    assert!(!has_error(src, 2683));
}

#[test]
fn arrow_in_class_method_no_ts2683() {
    // `this` in an arrow function inside a class method inherits the class `this`
    let src = r#"
class C {
    x: number = 1;
    method() {
        const f = () => this.x;
    }
}
"#;
    assert!(!has_error(src, 2683));
}

#[test]
fn class_constructor_direct_this_no_ts2683() {
    // `this` directly in a constructor should NOT get TS2683
    let src = r#"
class C {
    x!: number;
    constructor() { this.x = 1; }
}
"#;
    assert!(!has_error(src, 2683));
}

#[test]
fn object_literal_method_this_no_ts2683() {
    // `this` in an object literal method should NOT get TS2683
    // (it has a contextual owner)
    let src = r#"
var obj = {
    msg: "hello",
    start: function() { return this.msg; }
};
"#;
    assert!(!has_error(src, 2683));
}

#[test]
fn standalone_function_emits_ts2683() {
    // `this` in a standalone function should get TS2683
    let src = "function foo() { return this; }";
    assert!(has_error(src, 2683));
}

// --- Tests from upstream (067fb8ba41) ---

#[test]
fn explicit_this_param_suppresses_ts2683() {
    // `this` in a function with explicit `this: any` parameter should NOT get TS2683
    let src = r#"
const foo = function (this: any) {
    var a = this.blocks;
};
"#;
    assert!(!has_error(src, 2683));
}

#[test]
fn explicit_this_param_unknown_suppresses_ts2683() {
    // `this` in a function with explicit `this: unknown` parameter should NOT get TS2683
    let src = r#"
class Foo {
    static y = function(this: unknown) { console.log(this); }
}
"#;
    assert!(!has_error(src, 2683));
}

#[test]
fn no_explicit_this_param_still_emits_ts2683() {
    // `this` in a function without explicit `this` parameter should still get TS2683
    let src = r#"
const foo = function () {
    var a = this;
};
"#;
    assert!(has_error(src, 2683));
}

#[test]
fn explicit_this_param_in_nested_class_function_suppresses_ts2683() {
    // `this` in a function nested in a class method, but with explicit `this` param,
    // should NOT get TS2683
    let src = r#"
class C {
    method() {
        const inner = function(this: C) {
            return this;
        };
    }
}
"#;
    assert!(!has_error(src, 2683));
}

#[test]
fn function_declaration_with_this_param_suppresses_ts2683() {
    // `this` in a function declaration with explicit `this` parameter
    let src = r#"
function foo(this: { x: number }) {
    return this.x;
}
"#;
    assert!(!has_error(src, 2683));
}

// --- Additional tests (this session) ---

#[test]
fn explicit_this_param_no_ts2683() {
    // `this` in a function with explicit `this:` parameter should NOT get TS2683
    let src = "function foo(this: string) { return this; }";
    assert!(!has_error(src, 2683));
}

#[test]
fn explicit_this_param_object_type_no_ts2683() {
    // `this` with an object-typed explicit `this` parameter should NOT get TS2683
    let src = r#"
function bigger(this: {}) {
    return this;
}
"#;
    assert!(!has_error(src, 2683));
}

#[test]
fn explicit_this_param_union_type_no_ts2683() {
    // `this` with a union-typed explicit `this` parameter should NOT get TS2683
    let src = r#"
function bar(this: string | number) {
    if (typeof this === "string") {
        const x: string = this;
    }
}
"#;
    assert!(!has_error(src, 2683));
}

#[test]
fn property_assignment_any_receiver_no_ts2683() {
    // `this` in a function assigned to a property of an `any`-typed object
    // should NOT get TS2683 — `this` contextually becomes `any`
    let src = r#"
type Foo = any;
const foo: Foo = {};
foo.bar = function () {
    const self: Foo = this;
};
"#;
    assert!(!has_error(src, 2683));
}

#[test]
fn nested_function_in_class_with_explicit_this_still_emits_ts2683() {
    // Even if the class has `this`, a nested regular function creates its own `this`
    // binding, so TS2683 should still fire for the nested function
    let src = r#"
class C {
    value = 42;
    method() {
        function inner() {
            return this;
        }
    }
}
"#;
    assert!(has_error(src, 2683));
}

#[test]
fn static_field_function_expression_emits_ts2683() {
    let src = r#"
class C {
    static value = 1;
    static fnExpr = function () {
        return this.value + 1;
    };
}
"#;

    assert!(has_error(src, 2683));
}

#[test]
fn nested_function_inside_static_field_iife_emits_ts2683() {
    let src = r#"
class C {
    static value = (() => {
        function inner() {
            return this.value + 1;
        }
        return inner();
    })();
}
"#;

    assert!(has_error(src, 2683));
}

#[test]
fn nested_regular_function_inside_contextual_object_method_emits_ts2683() {
    let src = r#"
interface Options<Context, Data> {
    context: Context;
    produce(this: Context): Data;
}

declare function defineOptions<Context, Data>(options: Options<Context, Data>): [Context, Data];

defineOptions({
    context: { value: 5 },
    produce() {
        function inner() {
            return this;
        }
        return inner();
    },
});
"#;

    let diags = get_diagnostics(src);
    assert!(
        diags.iter().any(|d| d.0 == 2683),
        "Expected TS2683, got diagnostics: {diags:?}"
    );
}

#[test]
fn generic_callback_this_context_suppresses_ts2683() {
    let src = r#"
declare let $: {
    each<T>(items: T[], callback: (this: T, index: number, value: T) => void): void;
};
declare let lines: string[];

$.each(lines, function () {
    this.trim();
});
"#;

    let diags = get_diagnostics(src);
    assert!(
        !diags.iter().any(|d| d.0 == 2683),
        "Expected contextual generic callback `this` to suppress TS2683, got: {diags:?}"
    );
}

// =========================================================================
// JS constructor function tests
// =========================================================================

fn get_js_diagnostics(source: &str) -> Vec<(u32, String)> {
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.js".to_string(),
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            no_implicit_this: true,
            no_implicit_any: true,
            check_js: true,
            allow_js: true,
            ..CheckerOptions::default()
        },
    );

    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

#[test]
fn js_constructor_function_with_this_assignments_no_ts2683() {
    // In JS files, function declarations with `this.prop = value` are constructor
    // functions. tsc types `this` as the constructed instance and does NOT emit
    // TS2683 even with noImplicitThis.
    let src = r#"
function Instance() {
    this.i = 'simple'
}
var i = new Instance();
"#;

    let diags = get_js_diagnostics(src);
    assert!(
        !diags.iter().any(|d| d.0 == 2683),
        "Expected JS constructor function to suppress TS2683, got: {diags:?}"
    );
}

#[test]
fn js_constructor_function_with_prototype_no_ts2683() {
    // Constructor function with both `this.prop` assignments and prototype methods
    // should not emit TS2683.
    let src = r#"
function A() {
    this.x = 1
}
A.prototype.z = function f(n) {
    return n + this.x
}
var a = new A()
"#;

    let diags = get_js_diagnostics(src);
    let ts2683_diags: Vec<_> = diags.iter().filter(|d| d.0 == 2683).collect();
    assert!(
        ts2683_diags.is_empty(),
        "Expected JS constructor with prototype method to suppress TS2683, got: {ts2683_diags:?}"
    );
}

#[test]
fn js_nested_function_without_constructor_pattern_emits_ts2683() {
    // A nested regular function inside a class that uses `this` should get TS2683,
    // since the nested function creates its own `this` binding.
    let src = r#"
class Foo {
    bar() {
        function inner() {
            return this.toString()
        }
    }
}
"#;

    // Use TS mode since TS2683 for nested functions in classes is the primary case
    let diags = get_diagnostics(src);
    assert!(
        diags.iter().any(|d| d.0 == 2683),
        "Expected nested function in class method to emit TS2683, got: {diags:?}"
    );
}
