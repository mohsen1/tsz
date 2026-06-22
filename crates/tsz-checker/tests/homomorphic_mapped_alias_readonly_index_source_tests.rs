//! Regression coverage for #14509: a homomorphic mapped-type **alias**
//! `type M<T> = { [K in keyof T]: T[K] }` applied to a source carrying a
//! **readonly numeric (or string) index signature** must evaluate like the
//! inline mapped form, not collapse into a `readonly V[]` array shape.
//!
//! Root cause: the mapped-type instantiator's `extract_array_element` (and the
//! sibling array-shortcut paths) treated *any* object with a readonly numeric
//! index signature as a `ReadonlyArray`. A plain `{ readonly [k: number]: V }`
//! object — including a `typeof enum`, which is exactly a numeric index
//! signature plus named members — has such an index but is **not** an array, so
//! reshaping it into an array left the aliased application unevaluated and drew
//! a spurious TS2322 (and TS2339 on member access). The fix gates the
//! readonly-numeric-index → array shortcut on the array marker methods
//! (`slice`/`concat`), the same structural signal the evaluator and the
//! conditional `infer` array path already use.

use tsz_checker::context::CheckerOptions;

fn strict_with_libs(source: &str) -> Vec<(u32, String)> {
    let libs = tsz_checker::test_utils::load_default_lib_files();
    tsz_checker::test_utils::check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
        &libs,
    )
    .into_iter()
    .map(|d| (d.code, d.message_text))
    .collect()
}

fn relation_codes(diags: &[(u32, String)]) -> Vec<u32> {
    diags
        .iter()
        .filter(|(code, _)| matches!(*code, 2322 | 2339 | 2353))
        .map(|(code, _)| *code)
        .collect()
}

#[test]
fn aliased_homomorphic_mapped_over_readonly_numeric_index_plus_named_evaluates() {
    let diags = strict_with_libs(
        r#"
type Source = { readonly [x: number]: string; readonly A: 1; readonly B: 2 };
type Identity<T> = { [K in keyof T]: T[K] };
type Result = Identity<Source>;
const value: Result = { A: 1, B: 2 };
"#,
    );
    assert!(
        relation_codes(&diags).is_empty(),
        "aliased homomorphic mapped over readonly-numeric-index+named source should evaluate cleanly, got: {diags:#?}"
    );
}

#[test]
fn aliased_homomorphic_mapped_member_access_is_not_ts2339() {
    let diags = strict_with_libs(
        r#"
type Source = { readonly [x: number]: string; readonly A: 1; readonly B: 2 };
type Identity<T> = { [K in keyof T]: T[K] };
type Result = Identity<Source>;
declare const r: Result;
const a: 1 = r.A;
const b: 2 = r.B;
const n: string = r[5];
"#,
    );
    assert!(
        relation_codes(&diags).is_empty(),
        "members of the evaluated alias must be accessible, got: {diags:#?}"
    );
}

#[test]
fn aliased_homomorphic_mapped_over_typeof_enum_evaluates() {
    // The original witness: `typeof E` is a numeric index signature plus named
    // members, so it exercises the exact readonly-numeric-index shape.
    let diags = strict_with_libs(
        r#"
enum E { A, B }
type Identity<W> = { [Q in keyof W]: W[Q] };
type Result = Identity<typeof E>;
const value: Result = { A: E.A, B: E.B };
"#,
    );
    assert!(
        relation_codes(&diags).is_empty(),
        "aliased homomorphic mapped over `typeof enum` should evaluate cleanly, got: {diags:#?}"
    );
}

#[test]
fn aliased_homomorphic_mapped_over_readonly_string_index_plus_named_evaluates() {
    let diags = strict_with_libs(
        r#"
type Source = { readonly [k: string]: number; readonly x: 1 };
type Clone<Elem> = { [Key in keyof Elem]: Elem[Key] };
type Result = Clone<Source>;
const value: Result = { x: 1, y: 2 };
"#,
    );
    assert!(
        relation_codes(&diags).is_empty(),
        "aliased homomorphic mapped over readonly-string-index+named source should evaluate cleanly, got: {diags:#?}"
    );
}

#[test]
fn aliased_homomorphic_mapped_over_readonly_numeric_index_only_evaluates() {
    // The index-signature-only shape (no named members) must also evaluate.
    let diags = strict_with_libs(
        r#"
type Source = { readonly [x: number]: string };
type Mirror<Box> = { [Slot in keyof Box]: Box[Slot] };
type Result = Mirror<Source>;
const value: Result = { 0: "x" };
"#,
    );
    assert!(
        relation_codes(&diags).is_empty(),
        "aliased homomorphic mapped over readonly-numeric-index-only source should evaluate cleanly, got: {diags:#?}"
    );
}

#[test]
fn aliased_homomorphic_mapped_over_readonly_array_still_maps_to_array() {
    // A genuine `ReadonlyArray<V>` (which carries `slice`/`concat`) must still
    // map to an array shape — the fix must not over-broaden and break this.
    let diags = strict_with_libs(
        r#"
type Mirror<Box> = { [Slot in keyof Box]: Box[Slot] };
type Result = Mirror<ReadonlyArray<number>>;
const value: Result = [1, 2, 3];
const len: number = value.length;
const sliced: readonly number[] = value.slice();
"#,
    );
    assert!(
        relation_codes(&diags).is_empty(),
        "genuine ReadonlyArray must still map to an array with its methods, got: {diags:#?}"
    );
}

#[test]
fn aliased_homomorphic_mapped_over_mutable_array_still_maps_to_array() {
    let diags = strict_with_libs(
        r#"
type Mirror<Box> = { [Slot in keyof Box]: Box[Slot] };
type Result = Mirror<number[]>;
const value: Result = [1, 2];
value.push(3);
"#,
    );
    assert!(
        relation_codes(&diags).is_empty(),
        "mutable Array must still map to a mutable array, got: {diags:#?}"
    );
}

#[test]
fn aliased_homomorphic_mapped_still_rejects_excess_property() {
    // Negative control: the result is a real object with a readonly numeric
    // index signature, so a string-named excess property is still rejected —
    // proving the type was evaluated rather than blanket-accepted.
    let diags = strict_with_libs(
        r#"
type Source = { readonly [x: number]: string; readonly A: 1; readonly B: 2 };
type Identity<T> = { [K in keyof T]: T[K] };
type Result = Identity<Source>;
const value: Result = { A: 1, B: 2, C: 3 };
"#,
    );
    assert!(
        relation_codes(&diags).iter().any(|&c| c == 2353),
        "an excess string-named property must still error (TS2353), got: {diags:#?}"
    );
}
