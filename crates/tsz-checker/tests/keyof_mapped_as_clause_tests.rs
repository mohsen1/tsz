//! Tests for keyof on mapped types with as-clauses.
//!
//! When a mapped type uses key remapping (`as` clause), the checker's keyof
//! pre-check in `assignability_checker.rs` must not emit a false TS2322.
//! The pre-check extracts allowed keys from a keyof type, but for complex
//! inner types (Application, Mapped with as-clause, Lazy), it returns an
//! empty set. The empty-set guard ensures we fall through to the solver's
//! full evaluation pipeline rather than treating every string literal as
//! not-in-keys.

use tsz_checker::test_utils::{
    check_source_codes as check_and_get_codes, check_source_diagnostics, diagnostic_count,
};

#[test]
fn keyof_mapped_type_with_as_clause_no_false_ts2322() {
    // PickByValueType filters keys via `as T[K] extends U ? K : never`.
    // `keyof PickByValueType<Example, string>` should resolve to "foo".
    // Assigning "foo" to that keyof type must NOT produce TS2322.
    let code = r#"
type Example = { foo: string; bar: number };
type PickByValueType<T, U> = {
  [K in keyof T as T[K] extends U ? K : never]: T[K]
};
type T1 = PickByValueType<Example, string>;
type T2 = keyof T1;
const e2: T2 = "foo";
    "#;

    let codes = check_and_get_codes(code);
    let ts2322_count = codes.iter().filter(|&&c| c == 2322).count();
    assert_eq!(
        ts2322_count, 0,
        "Expected no TS2322 for valid keyof mapped-type-with-as-clause assignment, got codes: {codes:?}"
    );
}

#[test]
fn keyof_mapped_type_with_as_clause_invalid_key_still_errors() {
    // "bar" is filtered OUT by the as-clause (bar: number, not string).
    // Assigning "bar" to keyof PickByValueType<Example, string> should error.
    let code = r#"
type Example = { foo: string; bar: number };
type PickByValueType<T, U> = {
  [K in keyof T as T[K] extends U ? K : never]: T[K]
};
type T1 = PickByValueType<Example, string>;
type T2 = keyof T1;
const e2: T2 = "bar";
    "#;

    let codes = check_and_get_codes(code);
    let ts2322_count = codes.iter().filter(|&&c| c == 2322).count();
    assert_eq!(
        ts2322_count, 1,
        "Expected TS2322 for invalid key 'bar' not in keyof mapped type, got codes: {codes:?}"
    );
}

#[test]
fn keyof_simple_object_pre_check_still_works() {
    // For a plain object type, the pre-check should still catch invalid keys
    // without needing to fall through to the solver.
    let code = r#"
type Obj = { a: number; b: string };
type K = keyof Obj;
const x: K = "c";
    "#;

    let codes = check_and_get_codes(code);
    let ts2322_count = codes.iter().filter(|&&c| c == 2322).count();
    assert_eq!(
        ts2322_count, 1,
        "Expected TS2322 for 'c' not in keyof simple object, got codes: {codes:?}"
    );
}

#[test]
fn key_remapped_object_literal_reports_all_property_mismatches() {
    let source = r#"
type ObjectFromEntries<T extends readonly [string, any][]> = {
  [K in T[number] as K[0]]: K[1]
};

type Entries = [
  ["name", string],
  ["age", number],
  ["active", boolean]
];

type Obj = ObjectFromEntries<Entries>;
const wrongObj: Obj = { name: 123, age: "wrong", active: "yes" };
"#;

    let diagnostics = check_source_diagnostics(source);
    let ts2322_count = diagnostic_count(&diagnostics, 2322);
    assert_eq!(
        ts2322_count, 3,
        "Expected one TS2322 per mismatching key-remapped property, got: {diagnostics:#?}"
    );

    for expected in [
        "Type 'number' is not assignable to type 'string'.",
        "Type 'string' is not assignable to type 'number'.",
        "Type 'string' is not assignable to type 'boolean'.",
    ] {
        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.code == 2322 && diag.message_text.contains(expected)),
            "Expected diagnostic containing {expected:?}, got: {diagnostics:#?}"
        );
    }
}

#[test]
fn non_homomorphic_mapped_type_solver_delegation() {
    // Non-homomorphic mapped types (constraint is a literal union, not keyof T)
    // should be evaluated by the solver's evaluator via evaluate_type_with_env,
    // not by the checker's manual property expansion loop. This test verifies
    // that the solver-first delegation path produces correct results.
    let code = r#"
type Keys = "a" | "b";
type MyRecord = { [K in Keys]: number };
const r: MyRecord = { a: 1, b: 2 };
const x: number = r.a;
const y: number = r.b;
    "#;

    let codes = check_and_get_codes(code);
    assert!(
        !codes.contains(&2339),
        "Expected no TS2339 for property access on non-homomorphic mapped type, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2322),
        "Expected no TS2322 for valid assignment to non-homomorphic mapped type, got: {codes:?}"
    );
}

#[test]
fn non_homomorphic_mapped_type_with_template_transform() {
    // Non-homomorphic mapped type with a non-trivial template.
    // The solver's evaluator should correctly expand { [K in "x" | "y"]: Box<K> }
    // to { x: Box<"x">, y: Box<"y"> } (or the evaluated form).
    let code = r#"
type Box<T> = { value: T };
type MyMap = { [K in "x" | "y"]: Box<K> };
const m: MyMap = { x: { value: "x" }, y: { value: "y" } };
    "#;

    let codes = check_and_get_codes(code);
    assert!(
        !codes.contains(&2322),
        "Expected no TS2322 for non-homomorphic mapped type with template, got: {codes:?}"
    );
}

#[test]
fn mapped_type_as_clause_over_object_union_constraint() {
    // Mapped type with `as` clause where the constraint is a union of objects
    // (not string literals). The solver must iterate over the object members,
    // evaluate the `as` clause for each, and produce a concrete object type.
    //
    // This fixes a false TS2536 in patterns like:
    //   type Baz = { [K in keyof Lookup]: Lookup[K]['name'] }
    // where Lookup is a mapped type with key remapping over an object union.
    let code = r#"
type Lookup = { [Item in ({readonly name: "a"} | {readonly name: "b"}) as Item['name']]: Item };
type Baz = { [K in keyof Lookup]: Lookup[K]['name'] };
    "#;

    let codes = check_and_get_codes(code);
    let ts2536_count = codes.iter().filter(|&&c| c == 2536).count();
    assert_eq!(
        ts2536_count, 0,
        "Expected no TS2536 for indexing mapped type with as-clause over object union, got codes: {codes:?}"
    );
}

#[test]
fn mapped_type_as_clause_over_object_union_produces_concrete_type() {
    // Verify that the mapped type with `as` clause over an object union produces
    // a concrete type where property access works correctly.
    let code = r#"
type Lookup = { [Item in ({name: "a", value: 1} | {name: "b", value: 2}) as Item['name']]: Item };
const x: Lookup = { a: { name: "a", value: 1 }, b: { name: "b", value: 2 } };
const y: { name: "a", value: 1 } = x.a;
    "#;

    let codes = check_and_get_codes(code);
    assert!(
        !codes.contains(&2322),
        "Expected no TS2322 for accessing mapped type with as-clause over object union, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2339),
        "Expected no TS2339 for property access on mapped type with as-clause, got: {codes:?}"
    );
}

#[test]
fn mapped_type_as_clause_never_filter_over_objects() {
    // When the `as` clause evaluates to `never` for some members, those members
    // should be filtered out (not produce properties).
    let code = r#"
type OnlyA = { [Item in ({name: "a"} | {name: "b"}) as Item extends {name: "a"} ? Item['name'] : never]: Item };
const x: OnlyA = { a: { name: "a" } };
    "#;

    let codes = check_and_get_codes(code);
    assert!(
        !codes.contains(&2322),
        "Expected no TS2322 for filtered mapped type with as-clause, got: {codes:?}"
    );
}

// =============================================================================
// `keyof T` over a source with both literal properties AND an index signature
// (issue #6814). `interner.union` collapses `"foo" | string | number` into
// `string | number` for subtype reasons, but mapped iteration must enumerate
// each named property and each index signature separately so per-key
// as-clause filters drop the index step without dropping named properties.
// =============================================================================

#[test]
fn remove_index_signature_preserves_named_property() {
    // Canonical RemoveIndexSignature pattern: filter out `string`/`number`
    // index keys via `string extends K ? never`. The named property `foo`
    // must survive because `string extends "foo"` is `false`.
    let code = r#"
type RemoveIndexSignature<T> = {
  [K in keyof T as string extends K
    ? never
    : number extends K
      ? never
      : K]: T[K]
};
interface Foo {
  [key: string]: any;
  foo(): void;
}
type Cleaned = RemoveIndexSignature<Foo>;
declare const cleaned: Cleaned;
cleaned.foo();
    "#;
    let codes = check_and_get_codes(code);
    assert!(
        !codes.contains(&2339),
        "Expected named property `foo` to survive index-signature filter, got: {codes:?}"
    );
}

#[test]
fn remove_index_signature_different_type_param_names() {
    // Renaming the iteration variable (`P` instead of `K`) and the source
    // type parameter (`X` instead of `T`) must not change the result.
    // Proves the fix is structural, not name-dependent.
    let code = r#"
type Strip<X> = {
  [P in keyof X as string extends P ? never : P]: X[P]
};
interface Bar {
  [k: string]: unknown;
  bar(): number;
  baz: string;
}
declare const stripped: Strip<Bar>;
stripped.bar();
const s: string = stripped.baz;
    "#;
    let codes = check_and_get_codes(code);
    assert!(
        !codes.contains(&2339),
        "Expected named props bar/baz to survive (different type-param names), got: {codes:?}"
    );
    assert!(
        !codes.contains(&2322),
        "Expected baz to keep its `string` type, got: {codes:?}"
    );
}

#[test]
fn strip_number_index_signature_preserves_named() {
    // Same pattern with the numeric index signature instead of string.
    let code = r#"
type StripNum<T> = {
  [K in keyof T as number extends K ? never : K]: T[K]
};
interface Baz {
  [n: number]: any;
  named: string;
}
declare const sb: StripNum<Baz>;
const v: string = sb.named;
    "#;
    let codes = check_and_get_codes(code);
    assert!(
        !codes.contains(&2339),
        "Expected named property to survive number-index filter, got: {codes:?}"
    );
}

#[test]
fn key_remap_with_template_literal_keeps_named_keys() {
    // Renaming via template literal (`as `${P}_${K & string}``) requires the
    // literal key `alpha` to be visible to the substitution; if the literal is
    // collapsed into `string`, the resulting prefixed key would be `${P}_string`
    // (deferred) instead of the concrete `x_alpha`.
    let code = r#"
type Prefix<T, P extends string> = {
  [K in keyof T as string extends K ? never : `${P}_${K & string}`]: T[K]
};
interface Q {
  [k: string]: any;
  alpha: number;
  beta: boolean;
}
declare const px: Prefix<Q, "x">;
const a: number = px.x_alpha;
const b: boolean = px.x_beta;
    "#;
    let codes = check_and_get_codes(code);
    assert!(
        !codes.contains(&2339),
        "Expected x_alpha / x_beta to be present after prefix-rename, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2322),
        "Expected x_alpha / x_beta value types to match, got: {codes:?}"
    );
}

#[test]
fn key_remap_with_required_modifier_over_indexed_source() {
    // Combine the as-clause filter with the `-?` (required) modifier. The
    // named property must be present and required.
    let code = r#"
interface Src {
  [k: string]: unknown;
  named?: string;
}
type ReqStrip<T> = {
  [K in keyof T as string extends K ? never : K]-?: T[K]
};
declare const rs: ReqStrip<Src>;
const v: unknown = rs.named;
    "#;
    let codes = check_and_get_codes(code);
    assert!(
        !codes.contains(&2339),
        "Expected `named` to survive strip + required, got: {codes:?}"
    );
}

#[test]
fn pure_literal_source_unchanged_no_regression() {
    // Source has NO index signature: existing path must remain untouched.
    // The fix is gated on `literal keys + index signature` so this should
    // exercise the unchanged behavior.
    let code = r#"
type Strip<T> = {
  [K in keyof T as string extends K ? never : K]: T[K]
};
interface NoIdx { foo: number; bar: string; }
declare const s: Strip<NoIdx>;
const f: number = s.foo;
const b: string = s.bar;
    "#;
    let codes = check_and_get_codes(code);
    assert!(
        !codes.contains(&2339),
        "Expected pure-literal source to keep its props, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2322),
        "Expected pure-literal source to keep its types, got: {codes:?}"
    );
}

#[test]
fn pure_index_source_collapses_to_empty() {
    // Source has only an index signature, no named keys. After filtering the
    // index out, the resulting type has no properties — accessing one is an
    // error. Verifies the negative case.
    let code = r#"
type Strip<T> = {
  [K in keyof T as string extends K ? never : K]: T[K]
};
interface OnlyIdx { [k: string]: any; }
declare const oi: Strip<OnlyIdx>;
const v = oi.anything;
    "#;
    let codes = check_and_get_codes(code);
    assert!(
        codes.contains(&2339),
        "Expected TS2339 when accessing a property on a stripped index-only object, got: {codes:?}"
    );
}

#[test]
fn identity_mapped_over_indexed_source_keeps_named_and_index() {
    // No `as` clause: both the named property AND arbitrary index access
    // must work (identity mapped should round-trip the source shape).
    let code = r#"
type Id<T> = { [K in keyof T]: T[K] };
interface IxSrc {
  [k: string]: any;
  named: string;
}
declare const ix: Id<IxSrc>;
const n: string = ix.named;
const a: any = ix["arbitrary"];
    "#;
    let codes = check_and_get_codes(code);
    assert!(
        !codes.contains(&2339),
        "Expected identity mapped to preserve named + index, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2322),
        "Expected identity mapped to keep value types, got: {codes:?}"
    );
}
