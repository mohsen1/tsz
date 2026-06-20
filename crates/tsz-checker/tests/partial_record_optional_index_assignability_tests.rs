//! Assignability against an *optional* index signature (`[k: K]?: V`), as
//! produced by `Partial<Record<K, V>>`.
//!
//! Regression for the remeda `omitBy` false positive (issue #14158): an object
//! spread of a generic `T extends object` (typed as `T` itself) — and, more
//! broadly, any `object`-constrained / property-less source — is assignable to
//! `Partial<Record<string, unknown>>` even though it is correctly rejected by a
//! *required* `Record<string, unknown>`. tsc accepts the optional form because
//! the `?` modifier makes the index signature impose no requirement on a source
//! that declares no own properties.
//!
//! The structural rule (matched against `tsc` 6.x): a source `S` satisfies a
//! target's string/number index signature when `S` provides a compatible index
//! signature, OR `S`'s own properties are individually compatible (with `S`
//! inferable / an explicit index), OR — and this is the optional-only relaxation
//! — the index signature is OPTIONAL and `S` has no own properties.
//!
//! Binder and type-parameter names are varied across cases so no fix can key on
//! an identifier.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs_code_messages, load_lib_files};

/// Type-check `source` in strict mode with the es5 lib loaded (so `Record` and
/// `Partial` resolve) and count the TS2322 assignability errors.
fn ts2322_count(source: &str) -> usize {
    let lib_files = load_lib_files(&["es5.d.ts"]);
    check_source_with_libs_code_messages(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..Default::default()
        },
        &lib_files,
    )
    .iter()
    .filter(|(code, _)| *code == 2322)
    .count()
}

// ---------------------------------------------------------------------------
// Positive cases: must be CLEAN (no TS2322), matching tsc.
// ---------------------------------------------------------------------------

#[test]
fn object_spread_of_generic_into_partial_record_is_clean() {
    // The exact issue witness (renamed binders): `{ ...data }` is typed as the
    // spread source `T`, which must still satisfy the optional index signature.
    assert_eq!(
        ts2322_count(
            r#"
export function omitByImpl<Elem extends object>(payload: Elem): Record<string, unknown> {
  const out: Partial<Record<string, unknown>> = { ...payload };
  return out;
}
"#,
        ),
        0,
        "object spread of a generic must be assignable to Partial<Record<string, unknown>>",
    );
}

#[test]
fn bare_object_constrained_generic_into_partial_record_is_clean() {
    assert_eq!(
        ts2322_count(
            r#"
function pass<U extends object>(value: U) {
  const sink: Partial<Record<string, unknown>> = value;
  return sink;
}
"#,
        ),
        0,
        "a bare object-constrained generic must be assignable to an optional index signature",
    );
}

#[test]
fn record_constrained_generic_into_partial_record_is_clean() {
    assert_eq!(
        ts2322_count(
            r#"
function pass<R extends Record<string, number>>(rec: R) {
  const sink: Partial<Record<string, unknown>> = { ...rec };
  return sink;
}
"#,
        ),
        0,
        "a Record-constrained generic spread must be assignable to an optional index signature",
    );
}

#[test]
fn object_keyword_into_partial_record_is_clean() {
    assert_eq!(
        ts2322_count(
            r#"
declare const anything: object;
const stringIndexed: Partial<Record<string, unknown>> = anything;
const numberIndexed: Partial<Record<number, unknown>> = anything;
"#,
        ),
        0,
        "the `object` keyword must be assignable to an optional string/number index signature",
    );
}

#[test]
fn empty_interface_into_partial_record_is_clean() {
    assert_eq!(
        ts2322_count(
            r#"
interface Empty {}
declare const blank: Empty;
const sink: Partial<Record<string, unknown>> = blank;
"#,
        ),
        0,
        "a property-less named interface must be assignable to an optional index signature",
    );
}

#[test]
fn type_literal_with_props_into_partial_record_is_clean() {
    // A type literal is an inferable-index source; its members are checked
    // against the index value type (`number <: unknown`), so it is accepted.
    assert_eq!(
        ts2322_count(
            r#"
type Lit = { width: number };
declare const lit: Lit;
const sink: Partial<Record<string, unknown>> = lit;
"#,
        ),
        0,
        "a type literal with index-compatible members satisfies the optional index signature",
    );
}

#[test]
fn spread_with_extra_member_into_partial_record_is_clean() {
    assert_eq!(
        ts2322_count(
            r#"
function withExtra<T extends object>(data: T) {
  const out: Partial<Record<string, unknown>> = { ...data, extra: 1 };
  return out;
}
"#,
        ),
        0,
        "a spread combined with explicit members must satisfy the optional index signature",
    );
}

// ---------------------------------------------------------------------------
// Negative controls: must STILL error (TS2322), matching tsc. These prove the
// optional relaxation did not loosen the *required* index relation, nor the
// value-type check.
// ---------------------------------------------------------------------------

#[test]
fn generic_into_required_record_still_errors() {
    // A *required* index signature still rejects a property-less source: the
    // optionality relaxation must not leak here.
    assert!(
        ts2322_count(
            r#"
function omit<T extends object>(data: T) {
  const out: Record<string, unknown> = { ...data };
  return out;
}
"#,
        ) >= 1,
        "object spread of a generic must NOT satisfy a required Record<string, unknown>",
    );
}

#[test]
fn object_keyword_into_required_record_still_errors() {
    assert!(
        ts2322_count(
            r#"
declare const anything: object;
const sink: Record<string, unknown> = anything;
"#,
        ) >= 1,
        "the `object` keyword must NOT satisfy a required index signature",
    );
}

#[test]
fn named_interface_with_props_into_partial_record_still_errors() {
    // A named interface/class with own properties is NOT an inferable-index
    // source, so it stays rejected even against the optional index signature.
    assert!(
        ts2322_count(
            r#"
interface Point { x: number }
declare const point: Point;
const sink: Partial<Record<string, unknown>> = point;
"#,
        ) >= 1,
        "a named interface with properties must NOT satisfy the optional index signature",
    );
}

#[test]
fn class_instance_with_props_into_partial_record_still_errors() {
    assert!(
        ts2322_count(
            r#"
class Widget { id = 1 }
declare const widget: Widget;
const sink: Partial<Record<string, unknown>> = widget;
"#,
        ) >= 1,
        "a class instance with properties must NOT satisfy the optional index signature",
    );
}

#[test]
fn spread_into_partial_record_with_incompatible_value_still_errors() {
    // The optional relaxation only covers property-less sources. A source with a
    // property whose value is incompatible with the index value type must error.
    assert!(
        ts2322_count(
            r#"
function bad(data: { label: string }) {
  const out: Partial<Record<string, number>> = { ...data };
  return out;
}
"#,
        ) >= 1,
        "an incompatible property value must NOT satisfy an optional numeric-valued index signature",
    );
}
