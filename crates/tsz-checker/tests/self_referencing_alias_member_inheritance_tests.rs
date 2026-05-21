//! Regression coverage for the `TypeId::ERROR` leak boundary fix in
//! `base_instance_type_from_expression`. See issue #7688.
//!
//! Assertions: each test pins the expected diagnostic by `(code, line,
//! column)` plus a structural message fragment of the TS2416 template, not
//! just `codes.contains(&2416)`. The fragments use template wording
//! (e.g. `" is not assignable to the same property in base type "`) rather
//! than user-chosen identifier names, alias names, or type-parameter names
//! so the assertions remain rename-agnostic per CLAUDE.md §25 (see the
//! `*_is_name_agnostic` test for the explicit rename pin). See
//! issue #8488 for the rationale behind upgrading from code-only asserts.

use tsz_checker::test_utils::{DiagnosticShape, assert_diagnostic_shape, check_source_diagnostics};

/// Structural fragment of the TS2416 message template
/// `"Property '{0}' in type '{1}' is not assignable to the same property in base type '{2}'."`
/// — chosen so the assertion does not lock onto a particular property name,
/// type alias, or type-parameter spelling.
const TS2416_TEMPLATE_FRAGMENT: &str = " is not assignable to the same property in base type ";

#[test]
fn ts2416_fires_for_generic_intersection_override() {
    let source = r#"
declare class Base<P> {
    foo: P & { extra?: number };
}
class Derived<U> extends Base<U> {
    foo: U = undefined as any;
}
"#;
    let diags = check_source_diagnostics(source);
    assert_diagnostic_shape(
        source,
        &diags,
        &DiagnosticShape::code(2416)
            .at(6, 5)
            .with_message_fragment(TS2416_TEMPLATE_FRAGMENT),
    );
}

#[test]
fn self_referencing_alias_member_does_not_suppress_sibling_ts2416() {
    let source = r#"
type AliasD = ClassD<any>;
declare class ClassD<P> {
    foo: P & { extra?: number };
    bar: AliasD;
    x: string;
}
class DerivedD<U> extends ClassD<U> {
    x: number = 0;
}
"#;
    let diags = check_source_diagnostics(source);
    assert_diagnostic_shape(
        source,
        &diags,
        &DiagnosticShape::code(2416)
            .at(9, 5)
            .with_message_fragment(TS2416_TEMPLATE_FRAGMENT),
    );
}

#[test]
fn self_referencing_alias_index_signature_does_not_suppress_sibling_ts2416() {
    // Matches the `tsxGenericAttributesType6.tsx` shape:
    // `ReactInstance = Component<any, any> | Element` used as a `[key: string]: Inst`
    // value type on the base class.
    let source = r#"
type Inst = ClassU<any> | { __el: true };
declare class ClassU<P> {
    foo: P & { extra?: number };
    refs: { [key: string]: Inst };
    x: string;
}
class DerivedU<U> extends ClassU<U> {
    x: number = 0;
}
"#;
    let diags = check_source_diagnostics(source);
    assert_diagnostic_shape(
        source,
        &diags,
        &DiagnosticShape::code(2416)
            .at(9, 5)
            .with_message_fragment(TS2416_TEMPLATE_FRAGMENT),
    );
}

#[test]
fn non_self_alias_member_does_not_inhibit_ts2416() {
    let source = r#"
type Plain = { __el: true };
declare class ClassN<P> {
    foo: P & { extra?: number };
    bar: Plain;
}
class DerivedN<U> extends ClassN<U> {
    foo: U = undefined as any;
}
"#;
    let diags = check_source_diagnostics(source);
    assert_diagnostic_shape(
        source,
        &diags,
        &DiagnosticShape::code(2416)
            .at(8, 5)
            .with_message_fragment(TS2416_TEMPLATE_FRAGMENT),
    );
}

#[test]
fn ts2416_with_self_alias_member_is_name_agnostic() {
    // §25 anti-hardcoding: renaming bound type parameters and the alias /
    // class identifiers must not change behaviour. The structural template
    // fragment intentionally does not name `P`, `U`, `K`, `V`, `Holder`,
    // `Aliased`, or `pinned` — so renaming them all in the fixture must
    // not affect the assertion.
    let source = r#"
type Aliased = Holder<any>;
declare class Holder<K> {
    pinned: K & { extra?: number };
    self: Aliased;
    y: string;
}
class Sub<V> extends Holder<V> {
    y: number = 0;
}
"#;
    let diags = check_source_diagnostics(source);
    assert_diagnostic_shape(
        source,
        &diags,
        &DiagnosticShape::code(2416)
            .at(9, 5)
            .with_message_fragment(TS2416_TEMPLATE_FRAGMENT),
    );
}
