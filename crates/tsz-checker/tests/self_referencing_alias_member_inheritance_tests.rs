//! Regression coverage for the `TypeId::ERROR` leak boundary fix in
//! `base_instance_type_from_expression`. See issue #7688.

use tsz_checker::test_utils::check_source_codes;

#[test]
fn ts2416_fires_for_generic_intersection_override() {
    let codes = check_source_codes(
        r#"
declare class Base<P> {
    foo: P & { extra?: number };
}
class Derived<U> extends Base<U> {
    foo: U = undefined as any;
}
"#,
    );
    assert!(
        codes.contains(&2416),
        "Expected TS2416 for derived `foo: U` overriding base `foo: P & {{ extra? }}`; got {codes:?}"
    );
}

#[test]
fn self_referencing_alias_member_does_not_suppress_sibling_ts2416() {
    let codes = check_source_codes(
        r#"
type AliasD = ClassD<any>;
declare class ClassD<P> {
    foo: P & { extra?: number };
    bar: AliasD;
    x: string;
}
class DerivedD<U> extends ClassD<U> {
    x: number = 0;
}
"#,
    );
    assert!(
        codes.contains(&2416),
        "Expected TS2416 for DerivedD.x override even with self-referencing aliased member on base; got {codes:?}"
    );
}

#[test]
fn self_referencing_alias_index_signature_does_not_suppress_sibling_ts2416() {
    // Matches the `tsxGenericAttributesType6.tsx` shape:
    // `ReactInstance = Component<any, any> | Element` used as a `[key: string]: Inst`
    // value type on the base class.
    let codes = check_source_codes(
        r#"
type Inst = ClassU<any> | { __el: true };
declare class ClassU<P> {
    foo: P & { extra?: number };
    refs: { [key: string]: Inst };
    x: string;
}
class DerivedU<U> extends ClassU<U> {
    x: number = 0;
}
"#,
    );
    assert!(
        codes.contains(&2416),
        "Expected TS2416 for DerivedU.x override with self-referencing aliased index-signature member; got {codes:?}"
    );
}

#[test]
fn non_self_alias_member_does_not_inhibit_ts2416() {
    let codes = check_source_codes(
        r#"
type Plain = { __el: true };
declare class ClassN<P> {
    foo: P & { extra?: number };
    bar: Plain;
}
class DerivedN<U> extends ClassN<U> {
    foo: U = undefined as any;
}
"#,
    );
    assert!(
        codes.contains(&2416),
        "Expected TS2416 for DerivedN.foo override when base alias member is not self-referencing; got {codes:?}"
    );
}

#[test]
fn ts2416_with_self_alias_member_is_name_agnostic() {
    // §25 anti-hardcoding: renaming bound type parameters must not change behaviour.
    let codes = check_source_codes(
        r#"
type Aliased = Holder<any>;
declare class Holder<K> {
    pinned: K & { extra?: number };
    self: Aliased;
    y: string;
}
class Sub<V> extends Holder<V> {
    y: number = 0;
}
"#,
    );
    assert!(
        codes.contains(&2416),
        "Expected TS2416 for Sub.y override with self-aliased Holder; got {codes:?}"
    );
}
