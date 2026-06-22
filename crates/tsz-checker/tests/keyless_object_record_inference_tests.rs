//! Regression coverage for inferring a generic `Record<K, T>` / mapped-type
//! parameter from a **keyless object source** — the intrinsic `object` type and
//! the empty object literal `{}`.
//!
//! Structural rule (matches TypeScript 6.0.x `inferToMappedType`): a non-
//! homomorphic mapped type `{ [P in K]: T }` (the body of `Record<K, T>`) is
//! inferred from a source by its enumerable keys. A keyless source has
//! `keyof source === never`, so the key space — and, because there are no
//! values, the template space — collapse to `never`: `K = never`, `T = never`.
//! `Record<never, never>` is `{}`, which the source satisfies, so the call is
//! accepted.
//!
//! Previously tsz produced no inference candidates for the intrinsic `object`
//! against the mapped target, so `K` fell back to its constraint (`PropertyKey`)
//! and `T` to `unknown`/`object`. The resulting `Record<PropertyKey, …>` has a
//! required index signature that the bare `object` does not satisfy, yielding a
//! false `TS2345`/`TS2322`. tsc accepts every positive case below.
//!
//! Binder names (type-parameter and value identifiers) are varied per case so
//! the coverage is structural, not keyed to a particular name.

use tsz_checker::test_utils::{
    check_source_with_libs_code_messages, load_default_lib_files, strict_checker_options,
};

const TS2322: u32 = 2322; // Type not assignable
const TS2345: u32 = 2345; // Argument not assignable to parameter

fn codes(source: &str) -> Vec<u32> {
    // The default libs supply `Record`, `PropertyKey`, and `Array.isArray`, and
    // narrow `unknown` the way the real compiler does.
    let libs = load_default_lib_files();
    check_source_with_libs_code_messages(source, "test.ts", strict_checker_options(), &libs)
        .into_iter()
        .map(|(code, _)| code)
        .collect()
}

// ---------------------------------------------------------------------------
// Positive cases — tsc is clean on all of these.
// ---------------------------------------------------------------------------

#[test]
fn record_param_inferred_from_intrinsic_object_argument() {
    // The ts-pattern `recordEvery` witness, reduced: a `Record<K, T>` parameter
    // called with an `object`-typed argument. K = never, T = never.
    let diags = codes(
        r#"
declare const everyEntry: <K extends PropertyKey, V>(
  table: Record<K, V>,
  predicate: (key: K, value: V) => boolean
) => boolean;

function run(value: object): void {
  everyEntry(value, (k, v) => true);
}
"#,
    );
    assert!(
        !diags.contains(&TS2345),
        "`Record<K, V>` param inferred from `object` must accept the argument, got {diags:?}"
    );
}

#[test]
fn record_param_inferred_from_empty_object_literal_type() {
    // The empty object literal type is also keyless.
    let diags = codes(
        r#"
declare const forEachKey: <Key extends PropertyKey, Val>(
  dict: Record<Key, Val>
) => void;

function go(empty: {}): void {
  forEachKey(empty);
}
"#,
    );
    assert!(
        !diags.contains(&TS2345),
        "empty object literal type must infer `Record<never, never>`, got {diags:?}"
    );
}

#[test]
fn mapped_type_alias_param_inferred_from_object() {
    // A user-defined non-homomorphic mapped-type alias behaves like `Record`.
    let diags = codes(
        r#"
type Dictionary<Key extends PropertyKey, Value> = { [Entry in Key]: Value };
declare const sizeOf: <Key extends PropertyKey, Value>(
  source: Dictionary<Key, Value>
) => number;

function measure(thing: object): number {
  return sizeOf(thing);
}
"#,
    );
    assert!(
        !diags.contains(&TS2345) && !diags.contains(&TS2322),
        "mapped-type alias param inferred from `object` must be accepted, got {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// Negative control — the assignability rule itself is unchanged. With explicit
// type arguments (no inference), `object` is NOT assignable to a populated
// `Record`, exactly as in tsc.
// ---------------------------------------------------------------------------

#[test]
fn explicit_record_type_args_still_reject_object() {
    // `object` is not assignable to `Record<PropertyKey, unknown>`; supplying
    // the type arguments explicitly must still surface the mismatch (tsc emits
    // TS2345 here too). The inference fix must not weaken this.
    let diags = codes(
        r#"
declare const everyEntry: <K extends PropertyKey, V>(
  table: Record<K, V>,
  predicate: (key: K, value: V) => boolean
) => boolean;

function run(value: object): void {
  everyEntry<PropertyKey, unknown>(value, (k, v) => true);
}
"#,
    );
    assert!(
        diags.contains(&TS2345),
        "explicit `Record<PropertyKey, unknown>` must still reject `object`, got {diags:?}"
    );
}
