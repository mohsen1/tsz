//! TS7053 — indexing a type parameter whose constraint resolves (via its
//! apparent type) to a shape with a string index signature.
//!
//! Structural rule: when the indexed object is a type parameter, indexability is
//! governed by the *apparent type* of its base constraint. A constraint written
//! as an unevaluated application / alias / mapped type (e.g. `Record<string, V>`,
//! `Partial<Record<…>>`, `Readonly<Record<…>>`, a user mapped alias) — or an
//! intersection containing one — must be reduced to its apparent type before the
//! implicit-any-index check decides the key is unusable. tsc allows the access
//! because the resolved constraint carries a string index signature; tsz must
//! match. Constraints that genuinely lack a matching index signature still
//! report TS7053.
//!
//! Issue: <https://github.com/mohsen1/tsz/issues/10674>
use tsz_checker::context::{CheckerOptions, ScriptTarget};
use tsz_checker::test_utils::check_source;

fn codes(source: &str) -> Vec<u32> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2022,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|d| d.code)
    .collect()
}

// Reported repro: a `Record<string, V>` application constraint indexed by a
// `string` key. tsc clean; previously tsz emitted a false TS7053.
#[test]
fn record_string_constraint_no_ts7053() {
    let result =
        codes(r#"function f<T extends Record<string, unknown>>(o: T, k: string) { return o[k]; }"#);
    assert!(
        !result.contains(&7053),
        "expected no TS7053 for o[k] on T extends Record<string, unknown>, got: {result:?}"
    );
    let numeric =
        codes(r#"function g<T extends Record<string, number>>(o: T, k: string) { return o[k]; }"#);
    assert!(
        !numeric.contains(&7053),
        "expected no TS7053 for Record<string, number> constraint, got: {numeric:?}"
    );
}

// Name independence: renaming the type parameter must not change the outcome.
#[test]
fn record_constraint_is_name_independent() {
    let result = codes(
        r#"function f<XYZ extends Record<string, unknown>>(o: XYZ, key: string) { return o[key]; }"#,
    );
    assert!(
        !result.contains(&7053),
        "expected no TS7053 with renamed type parameter, got: {result:?}"
    );
}

// Wrapper applications over `Record` (`Partial`, `Readonly`) still expose the
// string index signature on the apparent type.
#[test]
fn wrapped_record_constraint_no_ts7053() {
    let partial = codes(
        r#"function f<T extends Partial<Record<string, number>>>(o: T, k: string) { return o[k]; }"#,
    );
    assert!(
        !partial.contains(&7053),
        "expected no TS7053 for Partial<Record<…>> constraint, got: {partial:?}"
    );
    let readonly = codes(
        r#"function f<T extends Readonly<Record<string, number>>>(o: T, k: string) { return o[k]; }"#,
    );
    assert!(
        !readonly.contains(&7053),
        "expected no TS7053 for Readonly<Record<…>> constraint, got: {readonly:?}"
    );
}

// A user-defined mapped-type alias behaves like `Record`: name-independent, so a
// differently-named iteration variable must also resolve.
#[test]
fn user_mapped_alias_constraint_no_ts7053() {
    let result = codes(
        r#"
type MyRec<V> = { [K in string]: V };
function f<T extends MyRec<number>>(o: T, k: string) { return o[k]; }
"#,
    );
    assert!(
        !result.contains(&7053),
        "expected no TS7053 for a user mapped-alias constraint, got: {result:?}"
    );
    // Different iteration-variable spelling — proves the fix is structural.
    let renamed = codes(
        r#"
type OtherRec<V> = { [Prop in string]: V };
function f<T extends OtherRec<number>>(o: T, k: string) { return o[k]; }
"#,
    );
    assert!(
        !renamed.contains(&7053),
        "expected no TS7053 with renamed mapped iteration variable, got: {renamed:?}"
    );
}

// Intersection constraint where one member carries the string index signature.
#[test]
fn intersection_with_record_member_no_ts7053() {
    let result = codes(
        r#"function f<T extends Record<string, number> & { a: number }>(o: T, k: string) { return o[k]; }"#,
    );
    assert!(
        !result.contains(&7053),
        "expected no TS7053 for `Record<…> & {{ a: number }}` constraint, got: {result:?}"
    );
}

// Negative control: a constraint that genuinely lacks a string index signature
// must still report TS7053.
#[test]
fn object_constraint_without_index_still_emits_ts7053() {
    let result = codes(r#"function f<T extends { a: number }>(o: T, k: string) { return o[k]; }"#);
    assert!(
        result.contains(&7053),
        "expected TS7053 for a constraint without a string index signature, got: {result:?}"
    );
}

// Negative control: a constraint that resolves (through an alias) to a plain
// object without an index signature must still report TS7053 — the apparent-type
// resolution must not over-suppress.
#[test]
fn alias_to_plain_object_constraint_still_emits_ts7053() {
    let result = codes(
        r#"
type Plain = { a: number };
function f<T extends Plain>(o: T, k: string) { return o[k]; }
"#,
    );
    assert!(
        result.contains(&7053),
        "expected TS7053 for an alias-to-plain-object constraint, got: {result:?}"
    );
}
