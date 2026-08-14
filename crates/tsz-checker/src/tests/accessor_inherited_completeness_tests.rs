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

/// Object-literal analogue of the class case, but the fallthrough guard does
/// NOT apply here: object literals don't (yet) wire up their own `TS2366` for
/// this pairing (a separate, pre-existing gap — tracked as follow-up to
/// #16968), so suppressing the compat check's `TS2322` would leave the getter
/// silently unchecked instead of deferring to a completeness check that
/// doesn't exist. `tsc` reports `TS2366` here; tsz keeps reporting `TS2322`
/// (wrong code, but present) until the object-literal completeness gap is
/// closed — a false negative (accepting code tsc rejects) is worse than a
/// wrong-code false positive.
#[test]
fn object_literal_getter_paired_setter_fallthrough_reports_ts2322() {
    assert_codes(
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
        &[2322],
    );
}

/// Object-literal analogue of the no-return-statements class case (above):
/// `tsc` reports `TS2355` + `TS2378`. Object literals still lack their own
/// `TS2355`, but `TS2378` ("a 'get' accessor must return a value") is a
/// separate, already-wired check, and the compat check's `TS2322` still fires
/// alongside it since the fallthrough guard does not apply to object
/// literals — see `object_literal_getter_paired_setter_fallthrough_reports_ts2322`.
#[test]
fn object_literal_getter_paired_setter_no_return_reports_ts2322_and_ts2378() {
    assert_codes(
        r"
        const qb3b = {
            get val() {
            },
            set val(v: number) {}
        };
        ",
        &[2322, 2378],
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

/// A `get`/`set` pair keyed by a **const-enum member** computed name
/// (`[Gate.Open]`) is one property: the unannotated getter inherits the
/// annotated setter's parameter type, so a getter body returning the wrong
/// domain is `TS2322`, exactly as for a plain identifier or string-literal key.
/// The computed key resolves through the qualified-entity-name path, not the
/// identifier-only one — `[Gate.Open]` is a property-access expression, which
/// `binder.resolve_identifier` alone cannot bind, so the pair was previously
/// never matched and the mismatch went undetected. Oracle-verified vs `tsc`.
#[test]
fn const_enum_computed_key_pairs_getter_and_setter_reports_ts2322() {
    assert_codes(
        r"
        const enum Gate { Shut = 0, Open = 1 }
        class Portal {
            get [Gate.Open]() {
                return true;
            }
            set [Gate.Open](swing: number) {}
        }
        ",
        &[2322],
    );
}

/// The same const-enum-keyed pair when the getter body *does* match the
/// setter's parameter domain: pairing must not fabricate a spurious mismatch.
#[test]
fn const_enum_computed_key_pairs_getter_and_setter_compatible_is_clean() {
    assert_codes(
        r"
        const enum Lever { Down = 2, Up = 3 }
        class Switchboard {
            get [Lever.Up]() {
                return 7;
            }
            set [Lever.Up](notch: number) {}
        }
        ",
        &[],
    );
}
