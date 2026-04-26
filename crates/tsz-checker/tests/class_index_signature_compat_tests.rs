//! Focused coverage for class index signature compatibility in extends checks.

use crate::test_utils::check_source_code_messages as compile_and_get_diagnostics;

/// Regression for `numericIndexerConstraint2.ts`: when assigning a plain
/// object to an indexed-access target whose value type has named members the
/// source lacks, the solver returns a nested `MissingProperty` describing
/// that inner mismatch (e.g. `MissingProperty { property_name: "foo",
/// source_type: number, target_type: Foo }`). The renderer must still report
/// the OUTER source/target in the TS2322 message — i.e. show the full index
/// signature `{ [index: string]: Foo; }` as the target, not the inner value
/// type `Foo`. Earlier code mis-classified the assignment as a "primitive
/// source" because `source_type` (the inner property type) was primitive,
/// even though the actual outer source is an object.
#[test]
fn ts2322_index_signature_target_displays_outer_type_when_inner_property_primitive_mismatches() {
    let diags = compile_and_get_diagnostics(
        r#"
class Foo { foo() { } }
declare var x: { [index: string]: Foo; };
var a: { one: number; } = { one: 1 };
x = a;
"#,
    );

    let matching: Vec<_> = diags
        .iter()
        .filter(|(code, msg)| {
            *code == 2322
                && msg.contains("Type '{ one: number; }'")
                && msg.contains("not assignable to type '{ [index: string]: Foo; }'")
        })
        .collect();

    assert!(
        !matching.is_empty(),
        "Expected TS2322 message naming the OUTER index-signature target '{{ [index: string]: Foo; }}', got: {diags:#?}",
    );

    // And we must NOT have leaked the inner property value type `'Foo'` into
    // the top-level message.
    let leaked_inner: Vec<_> = diags
        .iter()
        .filter(|(code, msg)| *code == 2322 && msg.contains("not assignable to type 'Foo'."))
        .collect();
    assert!(
        leaked_inner.is_empty(),
        "TS2322 must not collapse to inner index value type 'Foo'; got: {diags:#?}",
    );
}

#[test]
fn class_extends_reports_incompatible_string_index_signature() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
class Base {
  [key: string]: number;
}

class Derived extends Base {
  [key: string]: string;
}
"#,
    );

    let matching: Vec<_> = diagnostics
        .iter()
        .filter(|(code, msg)| {
            *code == 2415 && msg.contains("'string' index signatures are incompatible")
        })
        .collect();

    assert!(
        !matching.is_empty(),
        "Expected TS2415 with string index signature incompatibility, got: {diagnostics:#?}"
    );
}
