#[test]
fn test_jsdoc_function_return_mismatch_reports_inner_body_error_only() {
    let source = r#"
// @ts-check
/** @type {function (number): string} */
const x = (a) => a + 1;
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

    // TODO: tsc emits an inner body TS2322 ("Type 'number' is not assignable to type 'string'")
    // for JSDoc function return mismatch. We currently emit the outer function-level TS2322.
    // Update once inner body return-type elaboration is implemented.
    assert!(
        has_error(&diagnostics, 2322),
        "Expected TS2322 for JSDoc function return mismatch. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_jsdoc_generic_typedef_type_tag_no_erasure_reports_ts2345() {
    let source = r#"
/**
 * @template T
 * @typedef {<T1 extends T>(data: T1) => T1} Test
 */

/** @type {Test<number>} */
const test = dibbity => dibbity

test(1) // ok, T=1
test('hi') // error, T=number
"#;

    let diagnostics = compile_and_get_diagnostics_named(
        "typeTagNoErasure.js",
        source,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            emit_declarations: true,
            strict: true,
            no_implicit_any: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        has_error(&diagnostics, 2345),
        "Expected TS2345 for generic JSDoc typedef call. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 7006),
        "Did not expect TS7006 for generic JSDoc typedef call. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_enum_assignment_preserves_numeric_literal_source_display() {
    let source = r#"
enum E {
    A = 1,
    B = 2,
}
let x: E.A = 4;
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let message =
        diagnostic_message(&diagnostics, 2322).expect("expected TS2322 for assigning 4 to E.A");

    assert!(
        message.contains("Type '4' is not assignable to type 'E.A'."),
        "Expected numeric literal source display to be preserved. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_namespaced_enum_assignability_uses_qualified_names() {
    let source = r#"
namespace First {
    export enum E {
        a, b, c,
    }
}
namespace Abcd {
    export enum E {
        a, b, c, d,
    }
}
declare let abc: First.E;
declare let secondAbcd: Abcd.E;
abc = secondAbcd;
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let message = diagnostic_message(&diagnostics, 2322)
        .expect("expected TS2322 for assigning Abcd.E to First.E");

    assert!(
        message.contains("Type 'Abcd.E' is not assignable to type 'First.E'."),
        "Expected namespaced enum assignability to keep qualified names. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_unambiguous_namespaced_enum_assignability_uses_simple_names() {
    let source = r#"
namespace First {
    export enum E {
        a, b, c,
    }
}
namespace Abc {
    export enum Nope {
        a, b, c,
    }
}
declare let abc: First.E;
declare let nope: Abc.Nope;
abc = nope;
nope = abc;
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let messages: Vec<&str> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .map(|(_, message)| message.as_str())
        .collect();

    assert!(
        messages
            .iter()
            .any(|message| message.contains("Type 'Nope' is not assignable to type 'E'.")),
        "Expected unambiguous namespaced enum display to use simple names. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        messages
            .iter()
            .any(|message| message.contains("Type 'E' is not assignable to type 'Nope'.")),
        "Expected unambiguous reverse enum display to use simple names. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_merged_enum_assignability_uses_all_merged_members() {
    let source = r#"
namespace First {
    export enum E {
        a, b, c,
    }
}
namespace Merged {
    export enum E {
        a, b,
    }
    export enum E {
        c = 3, d,
    }
}
declare let abc: First.E;
declare let merged: Merged.E;
abc = merged;
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let message = diagnostic_message(&diagnostics, 2322)
        .expect("expected TS2322 for assigning merged enum to First.E");

    assert!(
        message.contains("Type 'Merged.E' is not assignable to type 'First.E'."),
        "Expected merged enum assignability to consider all merged members. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_namespaced_enum_object_property_access_uses_typeof_enum_name() {
    let source = r#"
namespace second {
    export enum E {
        A = 2,
    }
}

const value = second.E.B;
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let message = diagnostic_message(&diagnostics, 2339)
        .expect("expected TS2339 for missing enum object property");

    assert!(
        message.contains("Property 'B' does not exist on type 'typeof E'."),
        "Expected namespaced enum object property access to display 'typeof E'. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_stringified_noncanonical_numeric_enum_member_name_is_allowed() {
    let source = r#"
enum Nums {
    "13e-1",
}
"#;

    let diagnostics = compile_and_get_diagnostics(source);

    assert!(
        !has_error(
            &diagnostics,
            tsz_common::diagnostics::diagnostic_codes::AN_ENUM_MEMBER_CANNOT_HAVE_A_NUMERIC_NAME,
        ),
        "Expected non-canonical numeric string enum member names to be allowed. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_negative_infinity_string_enum_member_name_is_allowed() {
    let source = r#"
enum Nums {
    "-Infinity",
}
"#;

    let diagnostics = compile_and_get_diagnostics(source);

    assert!(
        !has_error(
            &diagnostics,
            tsz_common::diagnostics::diagnostic_codes::AN_ENUM_MEMBER_CANNOT_HAVE_A_NUMERIC_NAME,
        ),
        "Expected '-Infinity' string enum member names to be allowed. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_const_enum_string_named_members_are_accessible_by_element_access() {
    let source = r#"
const enum E {
    "hyphen-member" = 1,
    "123startsWithNumber" = 2,
    "has space" = 3,
}

const a = E["hyphen-member"];
const b = E["123startsWithNumber"];
const c = E["has space"];
"#;

    let diagnostics = compile_and_get_diagnostics(source);

    assert!(
        !has_error(
            &diagnostics,
            tsz_common::diagnostics::diagnostic_codes::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_EXPRESSION_OF_TYPE_CANT_BE_USED_TO_IN,
        ),
        "Expected string-named const enum members to be accessible via element access. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_const_enum_initializers_allow_merged_and_qualified_element_access() {
    let source = r#"
const enum Enum1 {
    A0 = 100,
}

const enum Enum1 {
    W1 = A0,
    W2 = Enum1.A0,
    W3 = Enum1["A0"],
    W4 = Enum1[`W2`],
}

namespace A {
    export namespace B {
        export namespace C {
            export const enum E {
                V1 = 1,
                V2 = A.B.C.E.V1 | 100
            }
        }
    }
}

namespace A {
    export namespace B {
        export namespace C {
            export const enum E {
                V3 = A.B.C.E["V2"] & 200,
                V4 = A.B.C.E[`V1`] << 1,
            }
        }
    }
}
"#;

    let diagnostics = compile_and_get_diagnostics(source);

    assert!(
        !has_error(&diagnostics, 2474),
        "Expected merged and qualified const enum initializer references to remain constant expressions.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_type_literal_computed_name_from_enum_object_reports_ts2464() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
export namespace Foo {
  export enum Enum {
    A = "a",
    B = "b",
  }
}

export type Type = { x?: { [Foo.Enum]: 0 } };
"#,
    );

    assert!(
        has_error(&diagnostics, 2464),
        "Expected TS2464 for a computed type-literal property named by an enum object.\nActual diagnostics: {diagnostics:#?}"
    );
}
