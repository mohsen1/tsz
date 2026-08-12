//! Tests for TS2683: 'this' implicitly has type 'any' because it does not have a type annotation.
//! This fires when noImplicitThis is on and `this` is used in a regular function
//! (not arrow) without a `this:` parameter annotation.

use tsz_binder::BinderState;
use tsz_checker::test_utils::check_with_options_code_messages;
use tsz_checker::{CheckerOptions, CheckerState};
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;

fn get_diagnostics(source: &str) -> Vec<(u32, String)> {
    check_with_options_code_messages(
        source,
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            no_implicit_this: true,
            ..CheckerOptions::default()
        },
    )
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
fn js_plain_function_read_without_constructor_evidence_emits_ts2683() {
    let src = r#"
function plain() {
    return this.missing
}

plain
"#;

    let diags = get_js_diagnostics(src);
    assert!(
        diags.iter().any(|d| d.0 == 2683),
        "Expected TS2683 for plain JS function `this` read, got: {diags:?}"
    );
    assert!(
        !diags.iter().any(|d| d.0 == 2339),
        "Expected implicit-any `this` to avoid a TS2339 cascade, got: {diags:?}"
    );
}

#[test]
fn js_constructor_function_with_this_assignments_emits_ts2683() {
    // TypeScript 7 dropped JS constructor-function inference: a plain function with
    // `this.prop = value` is no longer typed as a constructed instance, so `this`
    // is implicitly `any` (TS2683) under noImplicitThis and `new` lacks a construct
    // signature (TS7009).
    let src = r#"
function Instance() {
    this.i = 'simple'
}
var i = new Instance();
"#;

    let diags = get_js_diagnostics(src);
    assert!(
        diags.iter().any(|d| d.0 == 2683),
        "Expected implicit-any `this` (TS2683) in a plain JS function, got: {diags:?}"
    );
    assert!(
        diags.iter().any(|d| d.0 == 7009),
        "Expected `new` on a non-constructor function to report TS7009, got: {diags:?}"
    );
}

#[test]
fn js_constructor_function_with_prototype_emits_ts2683() {
    // A plain function with `this.prop` assignments and prototype methods is not a
    // constructor in TypeScript 7: the constructor body's `this` is implicitly
    // `any` (TS2683) and `new` reports TS7009.
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
    assert!(
        diags.iter().any(|d| d.0 == 2683),
        "Expected implicit-any `this` (TS2683) in the constructor body, got: {diags:?}"
    );
    assert!(
        diags.iter().any(|d| d.0 == 7009),
        "Expected `new` on a non-constructor function to report TS7009, got: {diags:?}"
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

// =========================================================================
// Mixin / class-inside-function `this` tests (issue #6644)
// =========================================================================

#[test]
fn class_property_initializer_inside_function_no_ts2683() {
    // Class owns `this` in property initializers; outer function is not a receiver.
    let src = r#"
function make() {
    return class {
        name = "hello";
        tag = this.name;
    };
}
"#;
    let diags = get_diagnostics(src);
    assert!(
        !diags.iter().any(|d| d.0 == 2683),
        "Expected class property initializer `this` to suppress TS2683, got: {diags:?}"
    );
}

#[test]
fn mixin_constrained_generic_base_no_ts2683() {
    // Mixin: class extends constrained generic base; `this` is the class instance.
    let src = r#"
type Ctor<T> = new (...args: any[]) => T;
function Tagged<TBase extends Ctor<{ name: string }>>(Base: TBase) {
    return class extends Base {
        tag = `tagged-${this.name}`;
    };
}
"#;
    let diags = get_diagnostics(src);
    assert!(
        !diags.iter().any(|d| d.0 == 2683),
        "Expected mixin class property initializer to suppress TS2683, got: {diags:?}"
    );
}

#[test]
fn mixin_renamed_type_param_no_ts2683() {
    // Same mixin pattern with differently-named type parameter — fix must be structural.
    let src = r#"
type Ctor<K> = new (...args: any[]) => K;
function Stamped<Base extends Ctor<{ id: number }>>(B: Base) {
    return class extends B {
        stamp = this.id;
    };
}
"#;
    let diags = get_diagnostics(src);
    assert!(
        !diags.iter().any(|d| d.0 == 2683),
        "Expected mixin with renamed type param to suppress TS2683, got: {diags:?}"
    );
}

#[test]
fn nested_function_inside_class_inside_function_still_emits_ts2683() {
    // Nested regular function inside a class creates its own `this` binding.
    let src = r#"
function outer() {
    return class {
        method() {
            function inner() {
                return this;
            }
        }
    };
}
"#;
    let diags = get_diagnostics(src);
    assert!(
        diags.iter().any(|d| d.0 == 2683),
        "Expected nested function inside class inside function to emit TS2683, got: {diags:?}"
    );
}

// #16964: a function expression assigned as the RHS of a property/element
// write gets a contextual `this` from the assignment's base object
// expression — matching tsc — instead of being silently `any`.

#[test]
fn property_assignment_this_is_base_object_type_ts2339() {
    // `this` inside the assigned function is the *base object's* type
    // (`{}`, the type of `o`), not `any` — so an absent member access
    // reports TS2339 against that base type instead of staying silent.
    let src = r#"
const o = {};
o.m = function () {
    this.q = 1;
};
"#;
    let diags = get_diagnostics(src);
    assert!(
        diags.iter().any(|d| d.0 == 2339),
        "Expected TS2339 for a missing member on the base object's type, got: {diags:?}"
    );
    assert!(
        !diags.iter().any(|d| d.0 == 2683),
        "This is a resolved contextual `this`, not implicit `any` — TS2683 must not fire, got: {diags:?}"
    );
}

#[test]
fn property_assignment_this_is_nominal_base_type_ts2339() {
    // Same rule against a nominally-typed base: `this` = `Foo`, and `Foo`
    // does not declare `q`, so TS2339 fires against `Foo` — not `bar`'s own
    // (irrelevant) declared type.
    let src = r#"
interface Foo {
    bar: any;
}
declare const foo: Foo;
foo.bar = function () {
    this.q = 1;
};
"#;
    let diags = get_diagnostics(src);
    assert!(
        diags.iter().any(|d| d.0 == 2339),
        "Expected TS2339 against the base object's declared type, got: {diags:?}"
    );
    assert!(
        !diags.iter().any(|d| d.0 == 2683),
        "Contextual `this` resolved from the base object — TS2683 must not fire, got: {diags:?}"
    );
}

#[test]
fn property_assignment_this_member_present_on_base_type_no_error() {
    // Adjacent positive case: the base object's type already declares the
    // member `this` accesses, so neither TS2339 nor TS2683 fires.
    let src = r#"
interface Foo {
    bar: any;
    q: number;
}
declare const foo: Foo;
foo.bar = function () {
    this.q = 1;
};
"#;
    let diags = get_diagnostics(src);
    assert!(
        diags.is_empty(),
        "Expected a clean check when the base type already declares the accessed member, got: {diags:?}"
    );
}

#[test]
fn property_assignment_this_element_access_base_no_error() {
    // Same rule via an element-access assignment target (`x[y] = function`),
    // not just dotted property access.
    let src = r#"
interface Foo {
    q: number;
}
declare const foo: Foo;
declare const key: string;
(foo as any)[key] = function () {
    this.q = 1;
};
"#;
    let diags = get_diagnostics(src);
    assert!(
        diags.is_empty(),
        "Expected a clean check for an `any`-typed element-access base, got: {diags:?}"
    );
}

// #16964 residual: the JS/checkJs expando-receiver path for property-
// assignment `this` (`const o = {}; o.m = function () { this.q = 1; };`)
// stayed silent after #16978 fixed the `.ts` shape, because a JS-only
// blanket "dynamic property write on `this`" suppression in
// `identifier_resolution.rs` predates #16978's assignment-receiver `this`
// mechanism and doesn't know about it — it unconditionally returned `any`
// for any direct `this.prop =` write in a JS file unless the write's RHS
// was `void 0`, even when `this` already had a real, structurally-checked
// receiver type from the property-assignment rule.

#[test]
fn js_expando_property_assignment_this_is_base_object_type_ts2339() {
    let src = r#"
const o = {};
o.m = function () {
    this.q = 1;
};
"#;
    let diags = get_js_diagnostics(src);
    assert!(
        diags.iter().any(|d| d.0 == 2339),
        "Expected TS2339 for a missing member on the base object's type, got: {diags:?}"
    );
    assert!(
        !diags.iter().any(|d| d.0 == 2683),
        "This is a resolved contextual `this`, not implicit `any` — TS2683 must not fire, got: {diags:?}"
    );
}

#[test]
fn js_expando_property_assignment_this_is_nominal_base_type_ts2339() {
    let src = r#"
/** @type {{bar: any}} */
var foo;
foo.bar = function () {
    this.q = 1;
};
"#;
    let diags = get_js_diagnostics(src);
    assert!(
        diags.iter().any(|d| d.0 == 2339),
        "Expected TS2339 against the base object's declared type, got: {diags:?}"
    );
}

#[test]
fn js_expando_property_assignment_this_member_present_no_error() {
    // A NON-EMPTY object-literal initializer is not an expando host (tsc's
    // `getExpandoInitializer` requires an empty literal), so the `o.m` write
    // itself reports TS2339 under `noImplicitAny` (oracle: typescript@7.0.2).
    // The point of this test is unchanged: `this` inside the assigned function
    // resolves to the receiver, whose declared `q` member keeps `this.q` clean
    // and TS2683 must not fire.
    let src = r#"
const o = { q: 0 };
o.m = function () {
    this.q = 1;
};
"#;
    let diags = get_js_diagnostics(src);
    assert_eq!(
        diags.iter().filter(|d| d.0 == 2339).count(),
        1,
        "Expected exactly the TS2339 on the non-expando-host `o.m` write, got: {diags:?}"
    );
    assert!(
        !diags.iter().any(|d| d.0 == 2683),
        "This is a resolved contextual `this`, not implicit `any` — TS2683 must not fire, got: {diags:?}"
    );
}

#[test]
fn js_bare_function_this_property_assignment_still_ts2683() {
    // Adjacent negative case: a plain (not property-assigned) JS function
    // still gets implicit-any `this` — TypeScript 7 no longer synthesizes a
    // constructor-style `this` from `this.prop =` assignments in its own
    // body, so this is unaffected by the property-assignment-receiver fix.
    let src = r#"
function plain() {
    this.q = 1;
}
"#;
    let diags = get_js_diagnostics(src);
    assert!(
        diags.iter().any(|d| d.0 == 2683),
        "Expected TS2683 for a plain JS function's own `this.prop=` body, got: {diags:?}"
    );
}

#[test]
fn bare_identifier_reassignment_still_emits_ts2683() {
    // A bare-identifier reassignment (`f = function () {...}`, no base
    // object) is NOT a property-assignment RHS: tsc still reports plain
    // implicit-any TS2683, matching an unassigned function expression.
    let src = r#"
let f: () => void;
f = function () {
    this.q = 1;
};
"#;
    let diags = get_diagnostics(src);
    assert!(
        diags.iter().any(|d| d.0 == 2683),
        "Expected TS2683 for a bare-identifier reassignment (no base object), got: {diags:?}"
    );
}

// =========================================================================
// JSDoc `@callback` + `@this` tag tests (callbackTag4.ts conformance fixture)
// =========================================================================

#[test]
fn jsdoc_callback_this_tag_suppresses_ts2683() {
    // A `@callback` typedef carrying a standalone `@this {T}` tag must give
    // the assigned function a contextual `this` type, same as a direct
    // `@this` tag on the function itself. Matches
    // `TypeScript/tests/cases/conformance/jsdoc/callbackTag4.ts`.
    let src = r#"
/**
 * @callback C
 * @this {{ a: string, b: number }}
 * @param {string} a
 * @param {number} b
 * @returns {boolean}
 */

/** @type {C} */
const cb = function (a, b) {
    this
    return true
}
"#;
    let diags = get_js_diagnostics(src);
    assert!(
        !diags.iter().any(|d| d.0 == 2683),
        "Expected a `@callback`'s `@this` tag to suppress TS2683, got: {diags:?}"
    );
}

#[test]
fn jsdoc_callback_this_tag_types_this_member_access() {
    // The `@this` type on the callback should also be checked structurally:
    // an absent member on the declared `this` shape should still TS2339.
    let src = r#"
/**
 * @callback C
 * @this {{ a: string }}
 * @param {string} a
 * @returns {boolean}
 */

/** @type {C} */
const cb = function (a) {
    this.missing
    return true
}
"#;
    let diags = get_js_diagnostics(src);
    assert!(
        diags.iter().any(|d| d.0 == 2339),
        "Expected TS2339 for a member absent from the `@this` shape, got: {diags:?}"
    );
    assert!(
        !diags.iter().any(|d| d.0 == 2683),
        "A resolved `@this` type must not also emit TS2683, got: {diags:?}"
    );
}

#[test]
fn jsdoc_callback_without_this_tag_still_emits_ts2683() {
    // Negative case: a `@callback` with no `@this` tag keeps the existing
    // implicit-any behavior — this must not become a blanket suppression.
    let src = r#"
/**
 * @callback C
 * @param {string} a
 * @returns {boolean}
 */

/** @type {C} */
const cb = function (a) {
    this
    return true
}
"#;
    let diags = get_js_diagnostics(src);
    assert!(
        diags.iter().any(|d| d.0 == 2683),
        "Expected TS2683 when the callback has no `@this` tag, got: {diags:?}"
    );
}

#[test]
fn jsdoc_typedef_this_tag_outside_callback_is_ignored() {
    // A plain (non-`@callback`) `@typedef` has no call signature, so an
    // `@this` tag there is meaningless and must not be picked up as a
    // param named "this" on some other shape.
    let src = r#"
/**
 * @typedef {{ this: string }} T
 */

/** @type {T} */
const t = { this: "x" };
"#;
    let diags = get_js_diagnostics(src);
    assert!(
        !diags.iter().any(|d| d.0 == 2683),
        "A non-callback typedef must not be affected by callback `@this` parsing, got: {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// Mechanism 2 of #16964: CommonJS `exports.X = ...` / `module.exports.X = ...`
// property-assigned function expressions. #16978 handled the general
// `o.m = ...` receiver but explicitly deferred the CommonJS bases; these cover
// them. tsc-independent (the tsz-cli lane verifies exact parity vs the oracle).
// ---------------------------------------------------------------------------

#[test]
fn commonjs_exports_property_assigned_function_this_is_implicit_any() {
    // `exports.f = function () { this.q = 1 }` — the bare `exports` identifier is
    // the module's own symbol, which tsc declines to type `this` as, so `this`
    // stays implicitly `any` (TS2683) rather than picking up a receiver.
    let src = "exports.assemble = function () {\n    this.q = 1;\n};\n";
    let diags = get_js_diagnostics(src);
    assert!(
        diags.iter().any(|d| d.0 == 2683),
        "CommonJS `exports.f = function () {{ this }}` should report implicit-any TS2683, got: {diags:?}"
    );
}

#[test]
fn commonjs_module_exports_property_assigned_function_this_is_receiver() {
    // `module.exports.a = function () { this.q }` — tsc types `this` as the
    // module's own exports namespace (`typeof import(self)`), so a missing member
    // is TS2339, not the implicit-any TS2683 the bare `exports` base gets.
    let src = "module.exports.a = function () {\n    this.q;\n};\n";
    let diags = get_js_diagnostics(src);
    assert!(
        diags.iter().any(|d| d.0 == 2339),
        "CommonJS `module.exports.a = function () {{ this.q }}` should report TS2339 on the missing member, got: {diags:?}"
    );
    assert!(
        !diags.iter().any(|d| d.0 == 2683),
        "a `module.exports` receiver `this` must not report implicit-any TS2683, got: {diags:?}"
    );
}
