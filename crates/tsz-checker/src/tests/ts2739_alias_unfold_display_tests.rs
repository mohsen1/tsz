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

/// Regression: TS2739's missing-property list must follow the target's
/// source-declaration order, not the global type interner's name-sorted
/// shape position. The interner sorts properties alphabetically for stable
/// shape hashing; if the diagnostic emit code uses that incidental shape
/// position as the primary sort key, the missing-property list comes out
/// in alphabetic-shuffled order. tsc lists them in declaration order.
#[test]
fn ts2739_lists_missing_properties_in_declaration_order_alpha_choice() {
    let msg = ts2739_source_display(
        r#"
class Receiver { id: number = 0; }
class Owner<T> {
    source: T;
    recurse: Owner<T>;
    wrapped: Owner<Owner<T>>;
}
const target: Owner<string> = new Receiver();
"#,
    );
    let source_idx = msg.find("source").expect("expected 'source' in message");
    let recurse_idx = msg.find("recurse").expect("expected 'recurse' in message");
    let wrapped_idx = msg.find("wrapped").expect("expected 'wrapped' in message");
    assert!(
        source_idx < recurse_idx && recurse_idx < wrapped_idx,
        "TS2739 missing properties must be in declaration order (source, recurse, wrapped). Got: {msg:?}"
    );
}

/// Anti-hardcoding cover: same structural rule with different property
/// names. The fix is about source-declaration order, not the specific
/// names of the missing properties — renaming `source`/`recurse`/`wrapped`
/// to `alpha`/`beta`/`gamma` (where alphabetic order would also place
/// `alpha` first) still requires declaration order to be honored, with
/// `gamma` coming last even though it is alphabetically last.
#[test]
fn ts2739_lists_missing_properties_in_declaration_order_renamed_props() {
    let msg = ts2739_source_display(
        r#"
class Sink { id: number = 0; }
class Producer<U> {
    zeta: U;
    alpha: Producer<U>;
    middle: Producer<Producer<U>>;
}
const out: Producer<string> = new Sink();
"#,
    );
    let zeta_idx = msg.find("zeta").expect("expected 'zeta' in message");
    let alpha_idx = msg.find("alpha").expect("expected 'alpha' in message");
    let middle_idx = msg.find("middle").expect("expected 'middle' in message");
    assert!(
        zeta_idx < alpha_idx && alpha_idx < middle_idx,
        "TS2739 missing properties must follow declaration order (zeta, alpha, middle), not alphabetic. Got: {msg:?}"
    );
}

/// Constructor parameter properties share the constructor's class-member
/// position, so they need their own stable sub-order before entering the
/// class property map.
#[test]
fn ts2739_lists_constructor_parameter_properties_in_source_order() {
    let msg = ts2739_source_display(
        r#"
class Empty {}
class Target {
    constructor(public zeta: string, public alpha: number) {}
    middle: boolean = false;
}
const out: Target = new Empty();
"#,
    );
    let zeta_idx = msg.find("zeta").expect("expected 'zeta' in message");
    let alpha_idx = msg.find("alpha").expect("expected 'alpha' in message");
    let middle_idx = msg.find("middle").expect("expected 'middle' in message");
    assert!(
        zeta_idx < alpha_idx && alpha_idx < middle_idx,
        "TS2739 missing properties must follow constructor parameter and class member source order. Got: {msg:?}"
    );
}
