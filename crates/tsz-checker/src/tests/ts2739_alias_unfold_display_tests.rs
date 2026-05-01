//! Regression tests for TS2739 source-display unfolding of single-step
//! type aliases of generic interface applications.
//!
//! Background: when the assignment source is annotated as a non-generic
//! type alias whose body is a generic Application of a *different* named
//! generic type (e.g. `type B = A<X>`), tsc displays the body application
//! form `A<X>` in the "is missing the following properties" message —
//! not the alias name `B`. This contrasts with TS2322 (which keeps the
//! alias name) and with TS2339 (which keeps the receiver alias name).
//!
//! Source: `compiler/objectTypeWithStringAndNumberIndexSignatureToAny.ts`
//! line 91 expects `Type 'NumberTo<number>' is missing the following
//! properties from type 'Obj': hello, world`, where the source identifier
//! is annotated as `nToNumber: NumberToNumber` and
//! `type NumberToNumber = NumberTo<number>`.

use crate::test_utils::check_source_diagnostics;

fn ts2739_source_display(source: &str) -> String {
    let diags = check_source_diagnostics(source);
    let ts2739: Vec<_> = diags.iter().filter(|d| d.code == 2739).collect();
    assert_eq!(
        ts2739.len(),
        1,
        "Expected exactly one TS2739. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
    ts2739[0].message_text.clone()
}

/// Mirrors the failing line in objectTypeWithStringAndNumberIndexSignatureToAny.ts:
/// the annotation alias points to a generic Application of a different generic.
/// Expected: TS2739 source unfolds to the application form.
#[test]
fn ts2739_unfolds_alias_of_generic_interface_application() {
    let msg = ts2739_source_display(
        r#"
interface NumberTo<T> { [x: number]: T }
interface Obj { hello: string; world: number }
type NumberToNumber = NumberTo<number>;
declare const nToNumber: NumberToNumber;
declare let someObj: Obj;
someObj = nToNumber;
"#,
    );
    assert!(
        msg.contains("'NumberTo<number>'"),
        "TS2739 source should unfold 'NumberToNumber' to the application form 'NumberTo<number>'. Got: {msg:?}"
    );
    assert!(
        !msg.contains("'NumberToNumber'"),
        "TS2739 source must not display the wrapper alias 'NumberToNumber'. Got: {msg:?}"
    );
}

/// Anti-hardcoding cover: same structural rule with different identifier names.
/// If the fix relies on a hardcoded user-chosen name, this test breaks.
#[test]
fn ts2739_unfolds_alias_of_generic_interface_application_renamed() {
    let msg = ts2739_source_display(
        r#"
interface Mapper<U> { [k: number]: U }
interface Receiver { foo: string; bar: number }
type Aliased = Mapper<string>;
declare const m: Aliased;
declare let r: Receiver;
r = m;
"#,
    );
    assert!(
        msg.contains("'Mapper<string>'"),
        "Renamed variant: TS2739 should unfold to 'Mapper<string>'. Got: {msg:?}"
    );
    assert!(
        !msg.contains("'Aliased'"),
        "Renamed variant: TS2739 must not display 'Aliased'. Got: {msg:?}"
    );
}

/// Negative cover: when the alias body is *not* a generic Application
/// (e.g. it's a structural object literal), the alias name should still
/// be preserved in the source display. Locks the rule from over-firing.
#[test]
fn ts2739_keeps_alias_when_body_is_structural_object() {
    // Aliasing an object literal directly (no generic application). The
    // alias has no application form to unfold to, so the alias name
    // remains the most informative display.
    let msg = ts2739_source_display(
        r#"
interface Receiver { hello: string; world: number }
type DirectShape = { [x: number]: number };
declare const ds: DirectShape;
declare let r: Receiver;
r = ds;
"#,
    );
    // tsc keeps 'DirectShape' here — direct alias of an object literal.
    assert!(
        msg.contains("'DirectShape'") || msg.contains("'{ [x: number]: number"),
        "TS2739 should keep alias name or its structural body when no application unfold is available. Got: {msg:?}"
    );
}
