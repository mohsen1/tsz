//! Regression tests for get/set accessor pairing when the `set` accessor
//! declares an explicit (illegal, `TS2784`) `this` parameter.
//!
//! `tsc`'s `getSetAccessorValueParameter` reads a setter's *value* parameter
//! — skipping a leading `this` parameter — everywhere it contextually types
//! the paired getter's return, the setter's own unannotated parameter, or
//! compares get/set types for `TS2322`. tsz's equivalents
//! (`contextual_getter_return_type_in_members`,
//! `contextual_setter_parameter_types_in_members`, and the object-literal /
//! class accessor type-compatibility checks) instead read
//! `parameters.nodes.first()` unconditionally, so a setter with an explicit
//! `this` parameter had that `this` parameter's type mistaken for the value
//! parameter's type. On top of the correct `TS2784` (`this` parameters are
//! blanket-illegal on accessors as of TypeScript 7), this produced spurious
//! extra `TS2322`/`TS2339` diagnostics that `tsc` never reports.
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

/// An unannotated getter paired with a setter that has both an explicit
/// (illegal) `this` parameter and its own annotated value parameter: the
/// getter's contextual return type must come from the setter's *value*
/// parameter (`number`), not its `this` parameter (`Wa1`) — so the getter's
/// `return this.n` body must not draw a spurious `TS2322`.
#[test]
fn setter_this_param_does_not_leak_into_getter_contextual_return_type() {
    assert_codes(
        r"
        interface Wa1 { n: number; x: number; }
        const oa1 = {
            n: 1,
            get x() { return this.n; },
            set x(this: Wa1, n: number) { this.n = n; }
        };
        ",
        &[2784],
    );
}

/// Both accessors declare an explicit (illegal) `this` parameter of
/// *different* types (`Wa2` vs `Wb2`). Only the two `TS2784`s fire — tsc
/// does not additionally compare the mismatched `this` types via `TS2322`
/// once the parameter itself is illegal.
#[test]
fn mismatched_illegal_this_params_report_only_ts2784() {
    assert_codes(
        r"
        interface Wa2 { n: number; }
        interface Wb2 { wrong: string; }
        const oa2 = {
            n: 1,
            get x(this: Wa2) { return this.n; },
            set x(this: Wb2, n) { }
        };
        ",
        &[2784, 2784],
    );
}

/// The setter's own leading `this` parameter has no type annotation either
/// (`set x(this, n)`); the paired getter's inferred return type must not be
/// cached onto that `this` slot, or `this` inside the setter body is
/// mistyped as the getter's return type, drawing a spurious `TS2339`.
#[test]
fn setter_this_param_without_annotation_does_not_receive_getter_return_type() {
    assert_codes(
        r"
        interface Wa3 { n: number; x: number; }
        const oa3 = {
            n: 1,
            get x(this: Wa3) { return this.n; },
            set x(this, n) { this.n = n; }
        };
        ",
        &[2784, 2784],
    );
}

/// Control: with no `this` parameter on either accessor, a genuine
/// getter/setter type mismatch must still report `TS2322` — the fix must
/// not suppress the real diagnostic, only the `this`-parameter misread.
#[test]
fn genuine_getter_setter_type_mismatch_without_this_param_still_reports() {
    assert_codes(
        r"
        interface Wa4 { wrong: string; }
        const oa4 = {
            get x() { return 1; },
            set x(v: Wa4) { }
        };
        ",
        &[2322],
    );
}

/// Control: a setter whose own value parameter is annotated and a plain
/// (no `this`) unannotated getter must still contextually type the getter's
/// return from that annotation, unaffected by the `this`-skip change.
#[test]
fn unannotated_getter_still_takes_setter_value_param_type_without_this() {
    assert_codes(
        r"
        const oa5 = {
            get x() { return 1; },
            set x(v: string) { }
        };
        ",
        &[2322],
    );
}

/// Class-accessor mirror of the object-literal case: an explicit `this`
/// parameter on a class setter must not leak into the paired getter's
/// contextual return type or the type-compatibility check.
#[test]
fn class_setter_this_param_does_not_leak_into_paired_getter() {
    assert_codes(
        r"
        class Ca6 {
            n: number = 1;
            get x() { return this.n; }
            set x(this: Ca6, n: number) { this.n = n; }
        }
        ",
        &[2784],
    );
}
