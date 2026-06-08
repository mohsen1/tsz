//! Regression tests for non-termination on `typeof`-self-referential type
//! aliases (issue #12177 family — recursive evaluation must terminate with the
//! same observable behavior as `tsc`).
//!
//! Structural rule: when a value and a type alias share a name and the alias
//! body references `typeof <name>` (the ubiquitous schema-library shape
//! `const X = ...; type X = Infer<typeof X>`), `typeof X` resolves to the
//! VALUE declaration's type, which is independent of the in-progress alias.
//! tsz previously deferred such a `typeof` to a `TypeQuery` that re-entered the
//! alias being resolved when the `typeof` appeared inside a generic
//! type-argument / indexed-access / tuple position, looping forever where
//! `tsc` terminates. The fix pre-resolves those nested `typeof` references to
//! the value type before evaluation.
//!
//! Each test reaching its assertions is itself the termination proof; the
//! assertions additionally pin the result to `tsc` parity. Binder names are
//! varied across cases so the fix cannot be satisfied by a name-specific path.

use tsz_checker::test_utils::check_source_codes;

/// `type X = F<typeof X>` through a generic alias whose body indexes an
/// intersection. `tsc`: clean (`typeof X` = the value `{ foo: number }`,
/// `F<{foo:number}>` = `1`).
#[test]
fn self_typeof_through_generic_indexed_alias_terminates() {
    let codes = check_source_codes(
        r#"
type Pick1<T> = (T & { p: 1 })["p"];
declare const cfg: { foo: number };
type cfg = Pick1<typeof cfg>;
const value: cfg = 1;
"#,
    );
    assert!(
        codes.is_empty(),
        "self-referential typeof through a generic indexed alias must match tsc (clean). Got: {codes:?}"
    );
}

/// `type X = Box<typeof X>` through an object-wrapping generic alias.
#[test]
fn self_typeof_through_object_wrapper_alias_terminates() {
    let codes = check_source_codes(
        r#"
type Box<E> = { contents: E };
declare const schema: { a: string };
type schema = Box<typeof schema>;
const boxed: schema = { contents: { a: "x" } };
"#,
    );
    assert!(
        codes.is_empty(),
        "self-referential typeof through an object-wrapping alias must match tsc (clean). Got: {codes:?}"
    );
}

/// `type X = Arr<typeof X>` through an array-producing generic alias.
#[test]
fn self_typeof_through_array_alias_terminates() {
    let codes = check_source_codes(
        r#"
type Listing<E> = E[];
declare const node: { n: number };
type node = Listing<typeof node>;
const items: node = [{ n: 1 }];
"#,
    );
    assert!(
        codes.is_empty(),
        "self-referential typeof through an array alias must match tsc (clean). Got: {codes:?}"
    );
}

/// `type X = [typeof X][number]`-style tuple wrapping.
#[test]
fn self_typeof_inside_tuple_argument_terminates() {
    let codes = check_source_codes(
        r#"
type Head<L extends unknown[]> = L[0];
declare const entry: { x: 1 };
type entry = Head<[typeof entry]>;
const first: entry = { x: 1 };
"#,
    );
    assert!(
        codes.is_empty(),
        "self-referential typeof inside a tuple argument must match tsc (clean). Got: {codes:?}"
    );
}

/// Direct indexed access on a self-referential `typeof`.
#[test]
fn self_typeof_direct_indexed_access_terminates() {
    let codes = check_source_codes(
        r#"
declare const record: { foo: number };
type record = (typeof record)["foo"];
const v: record = 5;
"#,
    );
    assert!(
        codes.is_empty(),
        "self-referential typeof in a direct indexed access must match tsc (clean). Got: {codes:?}"
    );
}

/// `keyof typeof X` where the value and alias share a name.
#[test]
fn self_typeof_keyof_operator_terminates() {
    let codes = check_source_codes(
        r#"
declare const flags: { a: 1; b: 2 };
type flags = keyof typeof flags;
const k1: flags = "a";
const k2: flags = "b";
"#,
    );
    assert!(
        codes.is_empty(),
        "keyof of a self-referential typeof must match tsc (clean). Got: {codes:?}"
    );
}

/// The value's type is inferred from a call initializer (no annotation), the
/// shape schema libraries actually emit. `typeof X` must use the inferred
/// value type, not the in-progress alias.
#[test]
fn self_typeof_inferred_initializer_terminates() {
    let codes = check_source_codes(
        r#"
declare function build<T>(shape: T): { wrapped: T };
const model = build({ a: 1 });
type model = (typeof model)["wrapped"];
const m: model = { a: 5 };
"#,
    );
    assert!(
        codes.is_empty(),
        "self-referential typeof over an inferred initializer must match tsc (clean). Got: {codes:?}"
    );
}

/// Control: a DISTINCT-named alias whose body reads `typeof someValue` must keep
/// resolving to the value (no behavior change from the fix).
#[test]
fn distinct_alias_reading_value_typeof_still_resolves() {
    let codes = check_source_codes(
        r#"
type Pick1<T> = (T & { p: 1 })["p"];
declare const source: { foo: number };
type Derived = Pick1<typeof source>;
const value: Derived = 1;
"#,
    );
    assert!(
        codes.is_empty(),
        "distinct-named alias reading a value typeof must remain clean. Got: {codes:?}"
    );
}

/// Schema-library shape: `const X = Factory(...); type X = Static<typeof X>`,
/// with a structural width mismatch between two such models. Must terminate and
/// surface the structural `TS2322` (parity with `tsc`) — never `TS2589` or a
/// hang.
#[test]
fn self_typeof_schema_static_width_mismatch_matches_tsc() {
    let codes = check_source_codes(
        r#"
type Evaluate<T> = T extends infer O ? { [K in keyof O]: O[K] } : never;
interface Schema { params: unknown[]; static: unknown }
interface Str extends Schema { static: string }
type Props = Record<string, Schema>;
type PropsReduce<T extends Props, P extends unknown[]> = Evaluate<{ [K in keyof T]: Static<T[K], P> }>;
interface Obj<T extends Props = Props> extends Schema { static: PropsReduce<T, this["params"]>; properties: T }
type Static<T extends Schema, P extends unknown[] = []> = (T & { params: P })["static"];
declare function obj<T extends Props>(shape: T): Obj<T>;
declare function str(): Str;

const Narrow = obj({ level: obj({ foo: str() }) });
type Narrow = Static<typeof Narrow>;
const Wide = obj({ level: obj({ foo: str(), bar: str() }) });
type Wide = Static<typeof Wide>;

function widen(rows: Narrow[]): Wide[] {
    return rows;
}
"#,
    );
    assert!(
        codes.contains(&2322),
        "schema width mismatch must surface TS2322 like tsc. Got: {codes:?}"
    );
    assert!(
        !codes.contains(&2589),
        "schema self-typeof evaluation must terminate without TS2589. Got: {codes:?}"
    );
}
