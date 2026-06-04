#[test]
fn test_js_constructor_instance_missing_property_does_not_use_variable_typeof_display() {
    let diagnostics = compile_and_get_diagnostics_named(
        "a.js",
        r#"
function C() {
    this.p = 1;
    this.q = void 0;
}
var c = new C();
c.p + c.q;
"#,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            target: ScriptTarget::ES2015,
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
    );

    let ts2339 = diagnostic_message(&diagnostics, 2339)
        .expect("expected TS2339 for missing constructor property");

    assert!(
        ts2339.contains("Property 'q' does not exist on type 'C'."),
        "Expected constructor instance missing-property display to use C. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !ts2339.contains("typeof c"),
        "Did not expect constructor instance missing-property display to use typeof c. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_merged_declarations_non_exported_namespace_members_stay_hidden() {
    let source = r#"
namespace M {
 export enum Color {
   Red, Green
 }
}
namespace M {
 export namespace Color {
   export var Blue = 4;
  }
}
var p = M.Color.Blue;

namespace M {
    export function foo() {
    }
}

namespace M {
    namespace foo {
        export var x = 1;
    }
}

namespace M {
    export namespace foo {
        export var y = 2
    }
}

namespace M {
    namespace foo {
        export var z = 1;
    }
}

M.foo()
M.foo.x
M.foo.y
M.foo.z
"#;

    let diagnostics = compile_and_get_diagnostics_named(
        "mergedDeclarations3.ts",
        source,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    let relevant: Vec<(u32, String)> = diagnostics
        .into_iter()
        .filter(|(code, _)| *code != 2318)
        .collect();
    let ts2339: Vec<&str> = relevant
        .iter()
        .filter(|(code, _)| *code == 2339)
        .map(|(_, message)| message.as_str())
        .collect();

    assert_eq!(
        ts2339.len(),
        2,
        "Expected exactly 2 TS2339 errors. Actual diagnostics: {relevant:#?}"
    );
    assert!(
        ts2339
            .iter()
            .any(|message| message.contains("Property 'x' does not exist on type")),
        "Expected TS2339 for M.foo.x. Actual diagnostics: {relevant:#?}"
    );
    assert!(
        ts2339
            .iter()
            .any(|message| message.contains("Property 'z' does not exist on type")),
        "Expected TS2339 for M.foo.z. Actual diagnostics: {relevant:#?}"
    );
    assert!(
        !ts2339
            .iter()
            .any(|message| message.contains("Property 'y'")),
        "Did not expect TS2339 for M.foo.y. Actual diagnostics: {relevant:#?}"
    );
}

#[test]
fn test_jsdoc_callback_typedef_contextually_types_closure_parameters() {
    let source = r#"
/** @callback Sid
 * @param {string} s
 * @returns {string}
 */
var x = 1;

/** @type {Sid} */
var sid = s => s + "!";
"#;

    let diagnostics = compile_and_get_diagnostics_named(
        "test.js",
        source,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            strict: true,
            no_implicit_any: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 7006),
        "Did not expect TS7006 for closure parameter contextually typed from JSDoc callback typedef. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_jsdoc_callback_typedef_on_constructor_scope_suppresses_ts7006() {
    let source = r#"
export class Preferences {
  assignability = "no";
  /**
   * @callback ValueGetter_2
   * @param {string} name
   * @returns {boolean|number|string|undefined}
   */
  constructor() {}
}

/** @type {ValueGetter_2} */
var ooscope2 = s => s.length > 0;
"#;

    let diagnostics = compile_and_get_diagnostics_named(
        "test.js",
        source,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            strict: true,
            no_implicit_any: true,
            target: ScriptTarget::ES2015,
            module: tsz_common::common::ModuleKind::ESNext,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 7006),
        "Did not expect TS7006 for closure typed from constructor-scoped JSDoc callback typedef. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_jsdoc_callback_typedef_contextually_types_function_declaration_parameters() {
    let source = r#"
/**
 * @callback Cb
 * @param {unknown} x
 * @return {x is number}
 */

/** @type {Cb} */
function isNumber(x) { return typeof x === "number"; }

/** @param {unknown} x */
function g(x) {
    if (isNumber(x)) {
        x * 2;
    }
}
"#;

    let diagnostics = compile_and_get_diagnostics_named(
        "test.js",
        source,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            strict: true,
            no_implicit_any: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 7006),
        "Did not expect TS7006 for function declaration typed from JSDoc callback typedef. Actual diagnostics: {diagnostics:#?}"
    );
}
