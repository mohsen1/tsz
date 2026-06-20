//! Missing-property diagnostics against a nullable-object union target.
//!
//! Structural rule: when an object source is assigned to a union target whose
//! only non-nullish member is a single object-like type (`T | null`,
//! `T | undefined`, `T | null | undefined`), the non-nullish source can never
//! satisfy the nullish members, so `tsc` elaborates the failure against `T`
//! alone. A missing required property therefore surfaces as the top-level
//! `TS2741` (one property) / `TS2739` (multiple) in an assignment/return
//! position — displaying `T`, not the union — exactly as for a bare `T` target.
//! It stays `TS2345` in an argument position. A genuine multi-member union
//! (`A | B`, `T | number`) keeps the generic `TS2322` union mismatch.
//!
//! Before the fix tsz wrapped the failure in a generic `TS2322` with the
//! missing-property message demoted to an elaboration child. The rule is keyed
//! purely on the nullish/object shape, so the tests vary every binder and
//! property name to prove it is not spelling-dependent.

use tsz_checker::test_utils::check_source_code_messages;

fn codes(source: &str) -> Vec<u32> {
    check_source_code_messages(source)
        .into_iter()
        .map(|(code, _)| code)
        .collect()
}

fn has_code_message(source: &str, code: u32, needle: &str) -> bool {
    check_source_code_messages(source)
        .iter()
        .any(|(c, m)| *c == code && m.contains(needle))
}

// ---- single missing property -> top-level TS2741, target shown as `T` ----

#[test]
fn assign_single_missing_to_nullable_union_is_ts2741() {
    let src = r#"
interface Point { a: number; b: string; }
const p: Point | null = { a: 1 };
"#;
    assert_eq!(
        codes(src),
        vec![2741],
        "{:?}",
        check_source_code_messages(src)
    );
    assert!(
        has_code_message(
            src,
            2741,
            "Property 'b' is missing in type '{ a: number; }' but required in type 'Point'.",
        ),
        "target must display the single non-nullish member, not the union: {:?}",
        check_source_code_messages(src)
    );
}

#[test]
fn nullable_union_uses_undefined_member_too() {
    let src = r#"
interface Shape { width: number; height: number; }
const s: Shape | undefined = { width: 3 };
"#;
    assert_eq!(codes(src), vec![2741]);
    assert!(has_code_message(src, 2741, "but required in type 'Shape'.",));
}

#[test]
fn nullable_union_with_both_null_and_undefined() {
    let src = r#"
interface Box { value: number; label: string; }
const b: Box | null | undefined = { value: 7 };
"#;
    assert_eq!(codes(src), vec![2741]);
    assert!(has_code_message(src, 2741, "but required in type 'Box'."));
}

#[test]
fn nullable_union_through_alias() {
    let src = r#"
interface Rec0 { a: number; b: string; }
type MaybeRec = Rec0 | null;
const r: MaybeRec = { a: 1 };
"#;
    // tsc displays the underlying member `Rec0`, not the alias `MaybeRec`.
    assert_eq!(codes(src), vec![2741]);
    assert!(has_code_message(src, 2741, "but required in type 'Rec0'."));
}

#[test]
fn rule_is_property_name_agnostic() {
    // Same shape, different binder + property spellings: behavior must follow
    // the structural nullable-union shape, not the names.
    let src = r#"
interface Widget { renderTarget: number; zIndex: string; }
const w: Widget | null = { renderTarget: 4 };
"#;
    assert_eq!(codes(src), vec![2741]);
    assert!(has_code_message(
        src,
        2741,
        "Property 'zIndex' is missing in type '{ renderTarget: number; }' but required in type 'Widget'.",
    ));
}

// ---- multiple missing properties -> top-level TS2739 ----

#[test]
fn assign_multi_missing_to_nullable_union_is_ts2739() {
    let src = r#"
interface Config { a: number; b: string; c: boolean; }
const cfg: Config | null = { a: 1 };
"#;
    assert_eq!(codes(src), vec![2739]);
    assert!(has_code_message(
        src,
        2739,
        "is missing the following properties from type 'Config': b, c",
    ));
}

// ---- nested property position ----

#[test]
fn nested_property_nullable_union_is_ts2741() {
    let src = r#"
interface Leaf { a: number; b: string; }
interface Tree { leaf: Leaf | null; }
const t: Tree = { leaf: { a: 1 } };
"#;
    assert_eq!(codes(src), vec![2741]);
    assert!(has_code_message(src, 2741, "but required in type 'Leaf'."));
}

#[test]
fn recursive_nullable_union_is_ts2741() {
    let src = r#"
type Node0 = { next: Node0 | null; val: number };
const n: Node0 = { next: { val: 2 }, val: 1 };
"#;
    assert_eq!(codes(src), vec![2741]);
    assert!(has_code_message(src, 2741, "but required in type 'Node0'."));
}

// ---- return position promotes; argument position stays TS2345 ----

#[test]
fn return_position_nullable_union_is_ts2741() {
    let src = r#"
interface Result0 { ok: boolean; data: number; }
function make(): Result0 | undefined { return { ok: true }; }
"#;
    assert_eq!(codes(src), vec![2741]);
    assert!(has_code_message(
        src,
        2741,
        "but required in type 'Result0'."
    ));
}

#[test]
fn argument_position_nullable_union_stays_ts2345() {
    // The argument path keeps TS2345 with the missing-property elaboration —
    // promotion to TS2741 is only for the assignment/return contexts.
    let src = r#"
interface Opts { a: number; b: string; }
declare function run(o: Opts | null): void;
run({ a: 1 });
"#;
    assert_eq!(codes(src), vec![2345]);
    // The argument header stays TS2345 and the parameter is shown as the
    // single non-nullish member `Opts`, not the nullable union.
    assert!(has_code_message(
        src,
        2345,
        "Argument of type '{ a: number; }' is not assignable to parameter of type 'Opts'.",
    ));
}

// ---- non-nullable unions are unchanged (generic TS2322) ----

#[test]
fn two_object_member_union_stays_ts2322() {
    let src = r#"
interface A0 { a: number; b: string; }
interface B0 { a: number; c: boolean; }
const x: A0 | B0 = { a: 1 };
"#;
    assert_eq!(codes(src), vec![2322]);
}

#[test]
fn object_plus_primitive_union_stays_ts2322() {
    let src = r#"
interface P0 { a: number; b: string; }
const x: P0 | number = { a: 1 };
"#;
    assert_eq!(codes(src), vec![2322]);
}

// ---- property-type mismatch (not absent) is not promoted ----

#[test]
fn property_type_mismatch_against_nullable_union_is_not_missing() {
    let src = r#"
interface P1 { a: number; }
const x: P1 | null = { a: "s" };
"#;
    // tsc reports the property-value mismatch at the property, TS2322 — not a
    // missing-property promotion.
    assert_eq!(codes(src), vec![2322]);
    assert!(
        !check_source_code_messages(src)
            .iter()
            .any(|(_, m)| m.contains("is missing in type"))
    );
}
