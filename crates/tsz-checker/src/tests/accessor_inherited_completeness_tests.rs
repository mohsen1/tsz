//! Regression tests for `TS2366`/`TS2355` control-flow completeness on a
//! `get` accessor whose return type is *inherited* from a paired `set`
//! accessor's annotated parameter type (`tsc`'s
//! `isGetAccessorWithAnnotatedSetAccessor` -> `getContextualReturnType`).
//!
//! `tsc` treats that inherited type as the getter's effective declared type
//! for every purpose, including the code-path completeness check: a getter
//! whose body does not return on every path reports the same `TS2366`/`TS2355`
//! an explicitly-annotated getter would. tsz's completeness check only fired
//! for a getter's *own* annotation, so an unannotated getter falling through
//! silently skipped `TS2366` there and instead ran a separate getter/setter
//! `TS2322` compatibility check with the implicit-fallthrough-`undefined`
//! folded into the inferred type — reporting a spurious `TS2322` (e.g.
//! `number | undefined` is not assignable to `number`) tsc never emits.
//!
//! Expected diagnostics are oracle-verified against pinned `tsc` (7.0.2).
//!
//! Every binder name is distinct per test so nothing can key on an
//! identifier string.

use crate::test_utils::check_source_strict_messages;

fn codes(source: &str) -> Vec<u32> {
    let mut found: Vec<u32> = check_source_strict_messages(source)
        .into_iter()
        .map(|(code, _)| code)
        .collect();
    found.sort_unstable();
    found
}

#[track_caller]
fn assert_codes(source: &str, expected: &[u32]) {
    let found = codes(source);
    let mut want = expected.to_vec();
    want.sort_unstable();
    assert_eq!(found, want, "diagnostics for source:\n{source}");
}

/// An unannotated getter paired with an annotated setter, falling through one
/// branch: tsc reports `TS2366` (inherited type does not include `undefined`),
/// not a `TS2322` mismatch against the setter's type.
#[test]
fn unannotated_getter_paired_setter_fallthrough_reports_ts2366() {
    assert_codes(
        r"
        declare const flagQb1: boolean;
        class Qb1 {
            get val() {
                if (flagQb1) {
                    return 0;
                }
            }
            set val(v: number) {}
        }
        ",
        &[2366],
    );
}

/// Same shape, but the getter has NO return statements at all: tsc reports
/// both `TS2355` (declared type not `undefined`/`void`/`any` but never
/// returns, anchored via the inherited setter type) AND `TS2378` ("a 'get'
/// accessor must return a value") — the two checks are independent passes in
/// tsc and are not mutually exclusive.
#[test]
fn unannotated_getter_paired_setter_no_return_reports_ts2355_and_ts2378() {
    assert_codes(
        r"
        class Qb2 {
            get val() {
            }
            set val(v: number) {}
        }
        ",
        &[2355, 2378],
    );
}

/// Object-literal analogue of the class case: the compat check must not
/// double-report `TS2322` once it defers a falling-through getter body to
/// the completeness family — even though object literals do not (yet) wire
/// up their own `TS2366` for this pairing (a separate, pre-existing gap).
/// The point under test is the absence of the spurious `TS2322`.
#[test]
fn object_literal_getter_paired_setter_fallthrough_reports_no_ts2322() {
    let found = codes(
        r"
        declare const flagQb3: boolean;
        const qb3 = {
            get val() {
                if (flagQb3) {
                    return 0;
                }
            },
            set val(v: number) {}
        };
        ",
    );
    assert!(
        !found.contains(&2322),
        "expected no spurious TS2322, found: {found:?}"
    );
}

/// A getter that always returns (no fallthrough) still gets the real
/// `TS2322` mismatch against the setter's inherited type — the fallthrough
/// guard must not swallow a genuine type mismatch.
#[test]
fn unannotated_getter_paired_setter_no_fallthrough_still_reports_ts2322() {
    assert_codes(
        r#"
        class Qb4 {
            get val() {
                return "not a number";
            }
            set val(v: number) {}
        }
        "#,
        &[2322],
    );
}

/// Both accessors annotated: TS 5.1 allows unrelated getter/setter types, so
/// neither `TS2366` nor `TS2322` fire even with a falling-through getter body
/// (the getter's own annotation, not the setter's, governs completeness).
#[test]
fn both_annotated_fallthrough_uses_own_annotation_not_setter() {
    assert_codes(
        r"
        declare const flagQb5: boolean;
        class Qb5 {
            get val(): number | undefined {
                if (flagQb5) {
                    return 0;
                }
            }
            set val(v: number) {}
        }
        ",
        &[],
    );
}
