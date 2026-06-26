//! TS2312 on an interface that extends a *generic deferred* alias base.
//!
//! Structural rule: an interface heritage base must resolve to an object type
//! (or an intersection of object types) with statically known members. When an
//! interface extends a generic type alias whose body is a deferred non-object
//! type — a generic conditional (`T extends X ? A : B`), an indexed access
//! (`T[keyof T]`), `keyof`, or a union — applied to the interface's own type
//! parameters, the base stays deferred and `tsc` reports TS2312
//! ("An interface can only extend an object type or intersection of object
//! types with statically known members").
//!
//! tsz previously accepted these silently: the heritage validation classified
//! a *generic mapped* alias body but not a generic conditional / indexed-access
//! / `keyof` / union body, and the deferred base erased to `Error`, so the
//! whole heritage clause was skipped. The check now classifies any
//! non-object alias body via the shared `is_valid_interface_base_type`
//! gateway, not just the mapped case.
//!
//! The matrix varies the alias kind, binder names, and the
//! generic-vs-concrete distinction so the rule is exercised structurally
//! rather than for one fixture.

use tsz_checker::test_utils::check_source_code_messages as diagnostics;

const TS2312: u32 = 2312;

fn ts2312_count(source: &str) -> usize {
    diagnostics(source)
        .into_iter()
        .filter(|(code, _)| *code == TS2312)
        .count()
}

// ───────────────────────── positive (must emit TS2312) ─────────────────────

#[test]
fn extends_generic_conditional_alias() {
    let source = r#"
type Cond<T> = T extends string ? { a: 1 } : { b: 2 };
interface I<T> extends Cond<T> {}
"#;
    assert_eq!(
        ts2312_count(source),
        1,
        "extends a generic conditional alias must emit TS2312"
    );
}

#[test]
fn extends_generic_conditional_alias_renamed_binders() {
    // Same shape as above with different binder names — the rule is structural,
    // not keyed on any identifier.
    let source = r#"
type Pick2<Elem> = Elem extends number ? { hit: 1 } : { miss: 0 };
interface Box<Elem> extends Pick2<Elem> {}
"#;
    assert_eq!(
        ts2312_count(source),
        1,
        "renamed binders must not change the TS2312 outcome"
    );
}

#[test]
fn extends_generic_indexed_access_alias() {
    let source = r#"
type Values<T> = T[keyof T];
interface I<T> extends Values<T> {}
"#;
    assert_eq!(
        ts2312_count(source),
        1,
        "extends a generic indexed-access alias must emit TS2312"
    );
}

#[test]
fn extends_generic_keyof_alias() {
    let source = r#"
type Keys<T> = keyof T;
interface I<T> extends Keys<T> {}
"#;
    assert_eq!(
        ts2312_count(source),
        1,
        "extends a generic keyof alias must emit TS2312"
    );
}

#[test]
fn extends_generic_union_alias() {
    let source = r#"
type Eith<T> = { a: T } | { b: T };
interface I<T> extends Eith<T> {}
"#;
    assert_eq!(
        ts2312_count(source),
        1,
        "extends a generic union alias must emit TS2312"
    );
}

#[test]
fn extends_generic_conditional_with_infer() {
    let source = r#"
type Elem<T> = T extends (infer U)[] ? { u: U } : {};
interface I<T> extends Elem<T> {}
"#;
    assert_eq!(
        ts2312_count(source),
        1,
        "extends a generic conditional with infer must emit TS2312"
    );
}

// ───────────────────────── negative (must stay clean) ──────────────────────

#[test]
fn extends_concrete_conditional_resolving_to_object() {
    // A concrete argument reduces the conditional to an object type, which is a
    // valid base — no TS2312.
    let source = r#"
type Cond<T> = T extends string ? { a: 1 } : { b: 2 };
interface I extends Cond<number> {}
"#;
    assert_eq!(
        ts2312_count(source),
        0,
        "a concrete conditional resolving to an object is a valid base"
    );
}

#[test]
fn extends_generic_object_alias() {
    let source = r#"
type Obj<T> = { x: T };
interface I<T> extends Obj<T> {}
"#;
    assert_eq!(
        ts2312_count(source),
        0,
        "a generic object-shaped alias is a valid base"
    );
}

#[test]
fn extends_generic_array_alias() {
    let source = r#"
type Arr<T> = T[];
interface I<T> extends Arr<T> {}
"#;
    assert_eq!(
        ts2312_count(source),
        0,
        "a generic array alias is a valid (object) base"
    );
}

#[test]
fn extends_generic_interface_application() {
    let source = r#"
interface Base<T> { x: T }
interface I<T> extends Base<T> {}
"#;
    assert_eq!(
        ts2312_count(source),
        0,
        "a generic interface application is a valid base"
    );
}

#[test]
fn extends_mapped_over_concrete_object() {
    // `Partial<{ x: T }>` is a homomorphic mapped type over a concrete object,
    // which resolves to an object type with statically known members.
    let source = r#"
type P<T> = Partial<{ x: T }>;
interface I<T> extends P<T> {}
"#;
    assert_eq!(
        ts2312_count(source),
        0,
        "a mapped type over a concrete object is a valid base"
    );
}
