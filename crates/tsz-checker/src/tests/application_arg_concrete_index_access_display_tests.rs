//! Regression tests for Application-arg display when the arg is a
//! concrete `IndexAccess`.
//!
//! When a generic application's arg is an `IndexAccess(obj, idx)` whose
//! `obj` and `idx` are both fully concrete (no type parameters, no
//! infer placeholders, idx is a literal), tsc resolves the indexed
//! access in the displayed type — `View<TypeA["bar"]>` is shown as
//! `View<TypeB>`. tsz historically printed the unresolved form because
//! the per-key mapped instantiation left the `IndexAccess` in the
//! Application's arg list and the type printer rendered it verbatim.
//!
//! Source: `compiler/excessPropertyChecksWithNestedIntersections.ts`
//! line 71 expects `View<TypeB>` in the TS2353 message; tsz emits
//! `View<TypeA["bar"]>`.

use crate::test_utils::check_source_diagnostics;

fn first_2353_msg(source: &str) -> String {
    let diags = check_source_diagnostics(source);
    let ts2353 = diags.iter().find(|d| d.code == 2353).unwrap_or_else(|| {
        panic!(
            "Expected TS2353, got: {:?}",
            diags
                .iter()
                .map(|d| (d.code, d.message_text.clone()))
                .collect::<Vec<_>>()
        )
    });
    ts2353.message_text.clone()
}

#[test]
fn application_arg_concrete_index_access_is_resolved_for_excess_property_target() {
    let msg = first_2353_msg(
        r#"
type View<T> = { [K in keyof T]: T[K] extends object ? boolean | View<T[K]> : boolean };

interface Inner { foo: string; bar: string }
interface Outer { foo: string; bar: Inner }

let test: View<Outer>;
test = { foo: true, bar: { foo: true, bar: true, boo: true } };
"#,
    );
    assert!(
        msg.contains("'View<Inner>'"),
        "Application arg View<Outer[\"bar\"]> should resolve to View<Inner>. Got: {msg}"
    );
    assert!(
        !msg.contains("Outer[\""),
        "Concrete IndexAccess should not appear in Application arg display. Got: {msg}"
    );
}

#[test]
fn application_arg_concrete_index_access_is_resolved_renamed() {
    let msg = first_2353_msg(
        r#"
type Wrap<S> = { [Q in keyof S]: S[Q] extends object ? boolean | Wrap<S[Q]> : boolean };

interface Leaf { a: string; b: string }
interface Branch { a: string; b: Leaf }

let target: Wrap<Branch>;
target = { a: true, b: { a: true, b: true, extra: true } };
"#,
    );
    assert!(
        msg.contains("'Wrap<Leaf>'"),
        "Renamed variant: Wrap<Branch[\"b\"]> should resolve to Wrap<Leaf>. Got: {msg}"
    );
}
