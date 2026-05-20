//! When a generic interface has a default type argument that is itself an
//! Application type (e.g. `V = Map<K, unknown>`), assigning an object literal
//! with a `new Map()` value to the default-instantiated type must NOT produce
//! a false TS2322. The underlying rule:
//!
//! When a generic body is a plain object type (no `Conditional`, `IndexAccess`,
//! `KeyOf`, `Mapped`, or `TemplateLiteral` operations), Application-form type
//! arguments must be preserved during instantiation so the solver's variance
//! fast path can fire during compatibility checks. Eagerly expanding
//! `Map<string, unknown>` to its structural object form breaks the App-vs-App
//! fast path and causes structural expansion to fail (or be order-dependent on
//! prior cached checks).
//!
//! Adjacent cases covered here prove the rule generalizes beyond the reported
//! `Simple2` shape:
//! - Single-param default (`V = Map<string, unknown>`)
//! - Multi-param default where V depends on K (`V = Map<K, unknown>`)
//! - Different type-parameter name spellings (`Item`, `Value`)
//! - Mapped/conditional body generics still expand args (regression guard)
//! - Incorrect types still emit TS2322 (false-negative guard)

use crate::test_utils::has_diagnostic_code;
use tsz_common::options::checker::CheckerOptions;

fn diags_strict(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    let opts = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    crate::test_utils::check_source(source, "test.ts", opts)
}

// ---------------------------------------------------------------------------
// Positive cases: default-applied generics must accept compatible assignments
// ---------------------------------------------------------------------------

#[test]
fn single_param_map_default_accepts_new_map() {
    // `V = Map<string, unknown>` — body is a plain object; no structural ops.
    let diags = diags_strict(
        r#"
interface Store<V = Map<string, unknown>> { store: V; }
const s: Store = { store: new Map() };
"#,
    );
    assert!(
        !has_diagnostic_code(&diags, 2322),
        "Expected no TS2322 for `new Map()` assigned to `Store` with Map default; got: {diags:?}"
    );
}

#[test]
fn two_param_map_default_where_v_depends_on_k_accepts_new_map() {
    // `V = Map<K, unknown>` — K defaults to `string`; body is plain object.
    let diags = diags_strict(
        r#"
interface Keyed<K extends string = string, V = Map<K, unknown>> {
    key: K;
    store: V;
}
const k: Keyed = { key: "hi", store: new Map() };
"#,
    );
    assert!(
        !has_diagnostic_code(&diags, 2322),
        "Expected no TS2322 for Keyed default; got: {diags:?}"
    );
}

#[test]
fn alternate_param_names_still_accepted() {
    // Same rule under different type-parameter name spellings.
    let diags = diags_strict(
        r#"
interface Container<Item = Map<string, unknown>> { value: Item; }
const c: Container = { value: new Map() };

interface Wrapper<Value = Map<string, unknown>> { inner: Value; }
const w: Wrapper = { inner: new Map() };
"#,
    );
    assert!(
        !has_diagnostic_code(&diags, 2322),
        "Expected no TS2322 with alternate param names; got: {diags:?}"
    );
}

#[test]
fn explicit_application_arg_matches_default_accepted() {
    // Explicit `Store<Map<string, unknown>>` — the same as the default.
    let diags = diags_strict(
        r#"
interface Store<V = Map<string, unknown>> { store: V; }
const s: Store<Map<string, unknown>> = { store: new Map() };
"#,
    );
    assert!(
        !has_diagnostic_code(&diags, 2322),
        "Expected no TS2322 for explicit Map<string,unknown> arg; got: {diags:?}"
    );
}

#[test]
fn multi_layer_nesting_does_not_false_positive() {
    // Wrapper around Store; still a plain-object body at each level.
    let diags = diags_strict(
        r#"
interface Store<V = Map<string, unknown>> { store: V; }
interface Repo<S = Store> { repo: S; }
const r: Repo = { repo: { store: new Map() } };
"#,
    );
    assert!(
        !has_diagnostic_code(&diags, 2322),
        "Expected no TS2322 for nested default; got: {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// Negative cases: wrong types must still emit TS2322
// ---------------------------------------------------------------------------

#[test]
fn number_not_assignable_to_map_default_emits_ts2322() {
    let diags = diags_strict(
        r#"
interface Store<V = Map<string, unknown>> { store: V; }
const s: Store = { store: 42 };
"#,
    );
    assert!(
        has_diagnostic_code(&diags, 2322),
        "Expected TS2322 for number assigned to Map default; got: {diags:?}"
    );
}

#[test]
fn string_not_assignable_to_map_default_emits_ts2322() {
    let diags = diags_strict(
        r#"
interface Store<V = Map<string, unknown>> { store: V; }
const s: Store = { store: "hello" };
"#,
    );
    assert!(
        has_diagnostic_code(&diags, 2322),
        "Expected TS2322 for string assigned to Map default; got: {diags:?}"
    );
}

#[test]
fn wrong_key_type_emits_ts2322() {
    // Fake<number, unknown> is not assignable to Fake<string, unknown> (invariant key param).
    let diags = diags_strict(
        r#"
declare class Fake<K, V> { key: K; value: V; }
interface Store<V = Fake<string, unknown>> { store: V; }
declare const m: Fake<number, unknown>;
const s: Store = { store: m };
"#,
    );
    assert!(
        has_diagnostic_code(&diags, 2322),
        "Expected TS2322 for Fake<number, unknown> assigned to Fake<string, unknown> default; got: {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// Regression guards: generics with structural operations must still expand args
// ---------------------------------------------------------------------------

#[test]
fn partial_like_mapped_body_still_type_checks() {
    // `Partial<T>` body is a Mapped type — requires concrete arg expansion.
    // If we preserve the Application arg incorrectly here, Partial<{a: string}>
    // would not have property `a` visible and errors would be wrong.
    let diags = diags_strict(
        r#"
type MyPartial<T> = { [P in keyof T]?: T[P] };
interface Obj { a: string; b: number; }
const p: MyPartial<Obj> = { a: "hi" };
"#,
    );
    assert!(
        !has_diagnostic_code(&diags, 2322),
        "Expected no TS2322 for valid partial assignment; got: {diags:?}"
    );
}

#[test]
fn conditional_body_generic_still_checks_correctly() {
    // Conditional type body requires concrete args — wrong type must still error.
    let diags = diags_strict(
        r#"
type Boxed<T> = T extends string ? { kind: "str"; val: T } : { kind: "other" };
const b: Boxed<string> = { kind: "str", val: "hi" };
"#,
    );
    assert!(
        !has_diagnostic_code(&diags, 2322),
        "Expected no TS2322 for valid conditional type assignment; got: {diags:?}"
    );
}

#[test]
fn explicit_different_generic_arg_does_not_match_default() {
    // Explicit `Fake<number, unknown>` is different from the `Fake<string, unknown>` default.
    // A source typed as `Fake<string, unknown>` should NOT be assignable to `Store<Fake<number, unknown>>`.
    let diags = diags_strict(
        r#"
declare class Fake<K, V> { key: K; value: V; }
interface Store<V = Fake<string, unknown>> { store: V; }
declare const strFake: Fake<string, unknown>;
const s: Store<Fake<number, unknown>> = { store: strFake };
"#,
    );
    assert!(
        has_diagnostic_code(&diags, 2322),
        "Expected TS2322 for Fake<string,unknown> source with Fake<number,unknown> explicit target; got: {diags:?}"
    );
}
