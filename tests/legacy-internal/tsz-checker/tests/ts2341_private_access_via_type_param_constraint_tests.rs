//! Regression tests for TS2341/TS2445 enforcement when `this` is typed via a
//! type parameter constrained to a class.
//!
//! Source: `conformance/types/thisType/thisTypeAccessibility.ts`. When a free
//! function or prototype-assigned method takes `this: T` where `T extends Foo`,
//! tsc enforces private/protected accessibility rules through the constraint
//! chain. Previously tsz's `resolve_class_for_access` returned `None` for
//! type-parameter receivers, which silently accepted private member access.
//! See `crates/tsz-checker/src/symbols/symbol_resolver_utils.rs` —
//! `resolve_class_for_access` now walks `type_parameter_constraint`, then
//! tries `get_class_decl_from_type` (covers brand-bearing instance types) and
//! falls back to the constraint's lazy-symbol class declaration (covers
//! merged interface+class where the constraint resolves to the interface).

use crate::test_utils::check_source_diagnostics;

fn diag_codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

/// Plain class (no merge): private access through `this: T extends Foo`
/// inside a free function emits TS2341.
#[test]
fn ts2341_fires_on_priv_access_via_t_extends_class() {
    let source = "\
class Foo {
    private p: number = 1;
}
function ext<T extends Foo>(this: T, n: number) {
    this.p = n;
}
";
    let codes = diag_codes(source);
    assert!(
        codes.contains(&2341),
        "Expected TS2341 for private access via type-param constraint. Got: {codes:?}"
    );
}

/// Anti-hardcoding cover: same shape with a different class and member name.
#[test]
fn ts2341_fires_on_priv_access_via_t_extends_class_renamed() {
    let source = "\
class Widget {
    private secret: string = \"hi\";
}
function tweak<U extends Widget>(this: U) {
    this.secret = \"bye\";
}
";
    let codes = diag_codes(source);
    assert!(
        codes.contains(&2341),
        "Renamed variant: TS2341 must fire. Got: {codes:?}"
    );
}

/// Class+interface declaration merge: tsc still enforces TS2341 because
/// the constraint's class brand is reachable via the merged symbol.
#[test]
fn ts2341_fires_on_priv_access_via_t_extends_merged_class_interface() {
    let source = "\
class Box {
    private payload: number = 0;
}
interface Box {
    helper(): void;
}
function setIt<T extends Box>(this: T, v: number) {
    this.payload = v;
}
";
    let codes = diag_codes(source);
    assert!(
        codes.contains(&2341),
        "Merged interface+class: TS2341 must still fire. Got: {codes:?}"
    );
}

/// Public access through `this: T extends Foo` is allowed — no error.
#[test]
fn no_error_on_public_access_via_t_extends_class() {
    let source = "\
class Foo {
    pub: number = 1;
}
function ext<T extends Foo>(this: T, n: number) {
    this.pub = n;
}
";
    let codes = diag_codes(source);
    assert!(
        !codes.contains(&2341),
        "Public access must not fire TS2341. Got: {codes:?}"
    );
    assert!(
        !codes.contains(&2445),
        "Public access must not fire TS2445. Got: {codes:?}"
    );
}
