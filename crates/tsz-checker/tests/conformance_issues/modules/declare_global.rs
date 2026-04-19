use crate::core::*;

#[test]
fn test_tagged_template_generic_contextual_typing() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
function someGenerics6<A>(strs: TemplateStringsArray, a: (a: A) => A, b: (b: A) => A, c: (c: A) => A) { }
someGenerics6 `${ (n: number) => n }${ n => n }${ n => n }`;
        ",
        opts,
    );
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();
    assert!(
        !has_error(&relevant, 7006),
        "Should NOT emit TS7006 - 'n' should be inferred as number from generic context.\nActual errors: {relevant:#?}"
    );
}

/// Test that write-only parameters are correctly flagged as unused (TS6133).
///
/// When a parameter is assigned to (`person2 = "dummy"`) but never read,
/// TS6133 should still fire. Previously, `check_const_assignment` used the
/// tracking `resolve_identifier_symbol` to look up the symbol, which added
/// the assignment target to `referenced_symbols`. This suppressed the TS6133
/// diagnostic because the unused-checker's early skip treated the symbol as
/// "used".
///
/// Fix: `get_const_variable_name` now uses the binder-level `resolve_identifier`
/// (no tracking side-effect) so assignment targets stay in `written_symbols`
/// only.
#[test]
fn test_ts6133_write_only_parameter_still_flagged() {
    let opts = CheckerOptions {
        no_unused_locals: true,
        no_unused_parameters: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
function greeter(person: string, person2: string) {
    var unused = 20;
    person2 = "dummy value";
}
        "#,
        opts,
    );

    let ts6133_names: Vec<&str> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 6133)
        .map(|(_, msg)| {
            // Extract name from "'X' is declared but its value is never read."
            msg.split('\'').nth(1).unwrap_or("?")
        })
        .collect();

    assert!(
        ts6133_names.contains(&"person"),
        "Should flag 'person' as unused. Got: {ts6133_names:?}"
    );
    assert!(
        ts6133_names.contains(&"person2"),
        "Should flag 'person2' as unused (write-only). Got: {ts6133_names:?}"
    );
    assert!(
        ts6133_names.contains(&"unused"),
        "Should flag 'unused' as unused. Got: {ts6133_names:?}"
    );
}

/// Test that const assignment detection (TS2588) still works after the
/// `resolve_identifier_symbol` → `binder.resolve_identifier` change.
#[test]
fn test_ts2588_const_assignment_still_detected() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
const x = 5;
x = 10;
        "#,
    );
    assert!(
        has_error(&diagnostics, 2588),
        "Should emit TS2588 for assignment to const. Got: {diagnostics:#?}"
    );
}

/// Test that write-only parameters with multiple params all get flagged.
#[test]
fn test_ts6133_write_only_middle_parameter() {
    let opts = CheckerOptions {
        no_unused_locals: true,
        no_unused_parameters: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
function greeter(person: string, person2: string, person3: string) {
    var unused = 20;
    person2 = "dummy value";
}
        "#,
        opts,
    );

    let ts6133_names: Vec<&str> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 6133)
        .map(|(_, msg)| msg.split('\'').nth(1).unwrap_or("?"))
        .collect();

    assert!(
        ts6133_names.contains(&"person"),
        "Should flag 'person'. Got: {ts6133_names:?}"
    );
    assert!(
        ts6133_names.contains(&"person2"),
        "Should flag 'person2' (write-only). Got: {ts6133_names:?}"
    );
    assert!(
        ts6133_names.contains(&"person3"),
        "Should flag 'person3'. Got: {ts6133_names:?}"
    );
    assert!(
        ts6133_names.contains(&"unused"),
        "Should flag 'unused'. Got: {ts6133_names:?}"
    );
}

/// Test that underscore-prefixed binding elements in destructuring are suppressed
/// but regular underscore-prefixed declarations are NOT suppressed.
/// TSC only suppresses `_`-prefixed names in destructuring patterns, not in
/// regular `let`/`const`/`var` declarations.
#[test]
fn test_ts6133_underscore_regular_declarations_still_flagged() {
    let opts = CheckerOptions {
        no_unused_locals: true,
        no_unused_parameters: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
function f() {
    let _a = 1;
    let _b = "hello";
    let notUsed = 99;
    console.log("ok");
}
        "#,
        opts,
    );

    let ts6133_names: Vec<&str> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 6133)
        .map(|(_, msg)| msg.split('\'').nth(1).unwrap_or("?"))
        .collect();

    // TSC flags regular `let _a = 1` declarations — underscore suppression
    // only applies to destructuring binding elements, not regular declarations.
    assert!(
        ts6133_names.contains(&"_a"),
        "Should flag '_a' (regular declaration, not destructuring). Got: {ts6133_names:?}"
    );
    assert!(
        ts6133_names.contains(&"_b"),
        "Should flag '_b' (regular declaration, not destructuring). Got: {ts6133_names:?}"
    );
    assert!(
        ts6133_names.contains(&"notUsed"),
        "Should flag 'notUsed'. Got: {ts6133_names:?}"
    );
}

/// Test that underscore-prefixed binding elements in destructuring are suppressed.
/// This is the main pattern seen in failing conformance tests like
/// `unusedVariablesWithUnderscoreInBindingElement.ts`.
#[test]
fn test_ts6133_underscore_destructuring_suppressed() {
    let opts = CheckerOptions {
        no_unused_locals: true,
        no_unused_parameters: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
function f() {
    const [_a, b] = [1, 2];
    console.log(b);
}
        "#,
        opts,
    );

    let ts6133_names: Vec<&str> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 6133)
        .map(|(_, msg)| msg.split('\'').nth(1).unwrap_or("?"))
        .collect();

    assert!(
        !ts6133_names.contains(&"_a"),
        "Should NOT flag '_a' in array destructuring (underscore-prefixed). Got: {ts6133_names:?}"
    );
    // `b` is used via console.log, so it shouldn't be flagged either
    assert!(
        ts6133_names.is_empty(),
        "Should have no TS6133. Got: {ts6133_names:?}"
    );
}

/// Test object destructuring with underscore-prefixed binding element.
#[test]
fn test_ts6133_underscore_object_destructuring_suppressed() {
    let opts = CheckerOptions {
        no_unused_locals: true,
        no_unused_parameters: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
function f() {
    const obj = { a: 1, b: 2 };
    const { a: _a, b } = obj;
    console.log(b);
}
        "#,
        opts,
    );

    let ts6133_names: Vec<&str> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 6133)
        .map(|(_, msg)| msg.split('\'').nth(1).unwrap_or("?"))
        .collect();

    assert!(
        !ts6133_names.contains(&"_a"),
        "Should NOT flag '_a' in object destructuring. Got: {ts6133_names:?}"
    );
}

#[test]
fn test_ts6198_object_destructuring_ignores_explicit_underscore_aliases() {
    let opts = CheckerOptions {
        no_unused_locals: true,
        no_unused_parameters: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
function f() {
    const { a1: _a1, b1 } = { a1: 1, b1: 1 };
    const { a2, b2: _b2 } = { a2: 1, b2: 1 };
    const { a3: _a3, b3: _b3 } = { a3: 1, b3: 1 };
    const { _a4, _b4 } = { _a4: 1, _b4: 1 };
}
        "#,
        opts,
    );

    let ts6133_names: Vec<&str> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 6133)
        .map(|(_, msg)| msg.split('\'').nth(1).unwrap_or("?"))
        .collect();
    let ts6198_count = diagnostics.iter().filter(|(code, _)| *code == 6198).count();

    assert!(
        ts6133_names.contains(&"b1"),
        "Should flag 'b1' instead of collapsing to TS6198. Got: {diagnostics:?}"
    );
    assert!(
        ts6133_names.contains(&"a2"),
        "Should flag 'a2' instead of collapsing to TS6198. Got: {diagnostics:?}"
    );
    assert!(
        !ts6133_names.contains(&"_a1")
            && !ts6133_names.contains(&"_b2")
            && !ts6133_names.contains(&"_a3")
            && !ts6133_names.contains(&"_b3"),
        "Explicit underscore aliases should stay suppressed. Got: {diagnostics:?}"
    );
    assert_eq!(
        ts6198_count, 1,
        "Only the shorthand underscore object pattern should emit TS6198. Got: {diagnostics:?}"
    );
}

#[test]
fn test_ts6198_nested_object_destructuring_only_reports_inner_pattern() {
    let opts = CheckerOptions {
        no_unused_locals: true,
        no_unused_parameters: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
function f() {
    const {
        a3,
        b3: {
            b31: {
                b311, b312
            }
        },
        c3,
        d3
    } = { a3: 1, b3: { b31: { b311: 1, b312: 1 } }, c3: 1, d3: 1 };
}
        "#,
        opts,
    );

    let ts6133_names: Vec<&str> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 6133)
        .map(|(_, msg)| msg.split('\'').nth(1).unwrap_or("?"))
        .collect();
    let ts6198_count = diagnostics.iter().filter(|(code, _)| *code == 6198).count();

    assert!(
        ts6133_names.contains(&"a3")
            && ts6133_names.contains(&"c3")
            && ts6133_names.contains(&"d3"),
        "Outer direct bindings should still get TS6133. Got: {diagnostics:?}"
    );
    assert_eq!(
        ts6198_count, 1,
        "Only the nested object pattern should emit TS6198. Got: {diagnostics:?}"
    );
}

/// Test that underscore-prefixed parameters still work (regression guard).
#[test]
fn test_ts6133_underscore_params_still_suppressed() {
    let opts = CheckerOptions {
        no_unused_locals: true,
        no_unused_parameters: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
function f(_unused: string, used: string) {
    console.log(used);
}
        "#,
        opts,
    );

    let ts6133_names: Vec<&str> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 6133)
        .map(|(_, msg)| msg.split('\'').nth(1).unwrap_or("?"))
        .collect();

    assert!(
        !ts6133_names.contains(&"_unused"),
        "Should NOT flag '_unused' parameter. Got: {ts6133_names:?}"
    );
    assert!(
        ts6133_names.is_empty(),
        "Should have no TS6133 diagnostics at all. Got: {ts6133_names:?}"
    );
}

/// Test that TS2305 diagnostic includes quoted module name matching tsc format.
/// TSC emits: Module '"./foo"' has no exported member 'Bar'.
/// (outer ' from the message template, inner " from source-level quotes)
#[test]
fn test_ts2305_module_name_includes_quotes() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
export function foo() {}
import { nonExistent } from "./thisModule";
        "#,
    );

    let ts2305_msgs: Vec<&str> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2305 || *code == 2307)
        .map(|(_, msg)| msg.as_str())
        .collect();

    // If TS2305 is emitted, verify it includes quoted module name
    for msg in &ts2305_msgs {
        if msg.contains("has no exported member") {
            assert!(
                msg.contains("\"./thisModule\""),
                "TS2305 should include quoted module name. Got: {msg}"
            );
        }
    }
}

/// TS2451 vs TS2300: when `let` appears before `var` for the same name, tsc emits TS2451
/// ("Cannot redeclare block-scoped variable") rather than TS2300 ("Duplicate identifier").
/// The distinction depends on which declaration appears first in source order.
///
/// Regression test: the binder's declaration vector can be reordered by var hoisting,
/// so we must use source position to determine the first declaration.
#[test]
fn test_ts2451_let_before_var_emits_block_scoped_error() {
    let diagnostics = compile_and_get_diagnostics(
        r"
let x = 1;
var x = 2;
",
    );

    // Filter to only duplicate-identifier-family codes (ignore TS2318 from missing libs)
    let codes: Vec<u32> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2451 || *code == 2300)
        .map(|(code, _)| *code)
        .collect();
    // Both declarations should get TS2451 (block-scoped redeclaration)
    assert!(
        codes.iter().all(|&c| c == 2451),
        "Expected all TS2451, got codes: {codes:?}"
    );
    assert!(
        codes.len() == 2,
        "Expected 2 diagnostics (one per declaration), got {}",
        codes.len()
    );
}

/// When `var` appears before `let` for the same name, tsc emits TS2300
/// ("Duplicate identifier") because the first declaration is non-block-scoped.
/// When `let` appears before `var`, tsc emits TS2451 instead.
#[test]
fn test_ts2300_var_before_let_emits_duplicate_identifier() {
    let diagnostics = compile_and_get_diagnostics(
        r"
var x = 1;
let x = 2;
",
    );

    // Filter to only duplicate-identifier-family codes (ignore TS2318 from missing libs)
    let codes: Vec<u32> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2451 || *code == 2300)
        .map(|(code, _)| *code)
        .collect();
    // tsc uses TS2300 when the first declaration is non-block-scoped (var).
    assert!(
        codes.iter().all(|&c| c == 2300),
        "Expected all TS2300 (var-first + let conflict), got codes: {codes:?}"
    );
    assert!(
        codes.len() == 2,
        "Expected 2 diagnostics (one per declaration), got {}",
        codes.len()
    );
}

#[test]
fn test_block_scoped_function_duplicate_identifier_matches_catch_block_baseline() {
    let source = "\
var v;
try { } catch (e) {
    function v() { }
}

function w() { }
try { } catch (e) {
    var w;
}

try { } catch (e) {
    var x;
    function x() { }
    function e() { }
    var p: string;
    var p: number;
}
";

    let diagnostics = compile_and_get_raw_diagnostics_named(
        "test.ts",
        source,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    let expected_ts2300_starts: FxHashSet<u32> = FxHashSet::from_iter([
        u32::try_from(source.find("function w()").unwrap() + 9).unwrap(),
        u32::try_from(source.find("var w;").unwrap() + 4).unwrap(),
    ]);
    let actual_ts2300_starts: FxHashSet<u32> = diagnostics
        .iter()
        .filter(|d| d.code == 2300)
        .map(|d| d.start)
        .collect();

    assert_eq!(
        actual_ts2300_starts, expected_ts2300_starts,
        "Expected only the outer `function w` and inner `var w` TS2300 anchors from the catch-block baseline.\nActual diagnostics: {diagnostics:#?}"
    );
    assert!(
        diagnostics.iter().any(|d| {
            d.code == 2403
                && d.start == u32::try_from(source.rfind("var p: number;").unwrap() + 4).unwrap()
        }),
        "Expected TS2403 on the second `p` declaration.\nActual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !diagnostics.iter().any(|d| {
            d.code == 2300
                && d.start == u32::try_from(source.find("function e()").unwrap() + 9).unwrap()
        }),
        "Catch parameter shadowing should not produce TS2300 for `function e()`.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_block_scoped_function_skips_catch_parameter_and_outer_var_in_es2015() {
    let source = "\
var e;
try {} catch (e) { if (true) { function e() {} } }
";

    let diagnostics = compile_and_get_raw_diagnostics_named(
        "test.ts",
        source,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        diagnostics.is_empty(),
        "Expected the nested ES2015 block function to ignore both the catch parameter and the outer `var e`.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_function_arg_shadowing_preserves_parameter_surface_and_ts2403() {
    let source = r#"
class A { foo() { } }
class B { bar() { } }
function foo(x: A) {
   var x: B = new B();
     x.bar();
}
"#;

    let diagnostics = compile_and_get_raw_diagnostics_named(
        "test.ts",
        source,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        diagnostics.iter().any(|d| d.code == 2403),
        "Expected TS2403 for the var/parameter redeclaration.\nActual diagnostics: {diagnostics:#?}"
    );
    assert!(
        diagnostics.iter().any(|d| {
            d.code == 2339
                && d.message_text
                    .contains("Property 'bar' does not exist on type 'A'")
        }),
        "Expected x.bar() to keep the original parameter type surface and emit TS2339.\nActual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !diagnostics.iter().any(|d| d.code == 2322),
        "Did not expect a false TS2322 on the redeclaration initializer.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_property_access_widening_element_write_reports_fresh_empty_branch() {
    let source = r#"
function foo(options?: { a: string, b: number }) {
    (options || {})["a"] = 1;
}
"#;

    let diagnostics = compile_and_get_raw_diagnostics_named(
        "test.ts",
        source,
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    let ts7053 = diagnostics
        .iter()
        .find(|d| d.code == 7053)
        .expect("expected TS7053 for the element write");

    assert!(
        ts7053.message_text.contains("type '{}'."),
        "Expected TS7053 to report the fresh empty-object branch, got: {ts7053:#?}"
    );
}

#[test]
fn test_module_exports_define_property_does_not_fall_back_to_lib_signature() {
    let diagnostics = compile_and_get_diagnostics_named_with_lib_and_options(
        "mod2.js",
        r#"
Object.defineProperty(module.exports, "thing", { value: "yes", writable: true });
Object.defineProperty(module.exports, "readonlyProp", { value: "Smith", writable: false });
Object.defineProperty(module.exports, "rwAccessors", { get() { return 98122 }, set(_) { /*ignore*/ } });
Object.defineProperty(module.exports, "readonlyAccessor", { get() { return 21.75 } });
Object.defineProperty(module.exports, "setonlyAccessor", {
    /** @param {string} str */
    set(str) {
        this.rwAccessors = Number(str)
    }
});
"#,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            strict: true,
            target: ScriptTarget::ES2015,
            module: ModuleKind::CommonJS,
            ..CheckerOptions::default()
        },
    );

    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| matches!(*code, 2345 | 7006))
        .collect();

    assert!(
        relevant.is_empty(),
        "Did not expect Object.defineProperty(module.exports, ...) to fall back to lib-call TS2345/TS7006 diagnostics. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_jsdoc_exports_property_assignment_contextually_types_object_literal_methods() {
    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &[(
            "test.js",
            r#"
/** @typedef {{
    status: 'done'
    m(n: number): void
}} DoneStatus */

/** @type {DoneStatus} */
exports.x = {
    status: 'done',
    m(n) { }
}
exports.x
"#,
        )],
        "test.js",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            strict: true,
            target: ScriptTarget::ES2015,
            module: ModuleKind::CommonJS,
            ..CheckerOptions::default()
        },
    );

    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| matches!(*code, 2339 | 7006))
        .collect();
    assert!(
        relevant.is_empty(),
        "Expected JSDoc `exports.x` assignment to preserve contextual typing. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_jsdoc_module_exports_property_assignment_contextually_types_object_literal_methods() {
    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &[(
            "test.js",
            r#"
/** @typedef {{
    status: 'done'
    m(n: number): void
}} DoneStatus */

/** @type {DoneStatus} */
module.exports.y = {
    status: 'done',
    m(n) { }
}
module.exports.y
"#,
        )],
        "test.js",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            strict: true,
            target: ScriptTarget::ES2015,
            module: ModuleKind::CommonJS,
            ..CheckerOptions::default()
        },
    );

    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| matches!(*code, 2339 | 7006))
        .collect();
    assert!(
        relevant.is_empty(),
        "Expected JSDoc `module.exports.y` assignment to preserve contextual typing. Actual diagnostics: {diagnostics:#?}"
    );
}
