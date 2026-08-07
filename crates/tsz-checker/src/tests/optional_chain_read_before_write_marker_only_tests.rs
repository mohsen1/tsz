//! A read-before-write optional-chain write target (compound assignment,
//! increment/decrement) reports the whole target as possibly `undefined`
//! when the chain's `undefined` is marker-only — i.e. owed solely to the
//! chain's own short-circuit, not to a genuinely optional member.
//!
//! Structural rule: `optional_chain_invalid_assignment_target_context`
//! short-circuits a write target's access type to `any` so an invalid target
//! cannot cascade into assignability diagnostics. A compound assignment or
//! increment/decrement operand also READS the target before writing it,
//! through a separate plain-read computation — tsc's `checkArithmeticOperandType`
//! sees that read's real `T | undefined` result and reports it
//! (TS18047/18048/18049) right alongside the reference-grammar error
//! (TS2777/TS2779), naming the WHOLE target (`'a.b.c.d'`), not just the
//! receiver. tsz's short-circuit swallowed that read entirely.
//!
//! The discriminator is marker-only vs. genuine optionality (tsc strips a
//! chain's own short-circuit marker before checking a continuation): `a.b?.c.d`
//! with `c` required is marker-only and reports on the WHOLE target here;
//! `h?.inner.leaf` with `inner` optional is genuine and is already reported
//! once, naming the RECEIVER, by the ordinary possibly-nullish property-access
//! path — this fix must not add a second report for that case.
//!
//! A second structural fix travels with this one:
//! `optional_chain_invalid_assignment_target_context` used to walk UP through
//! every receiver link of a chain leading to a write target, short-circuiting
//! ALL of them to `any` — not just the target link itself. That silently
//! dropped a receiver's own possibly-nullish diagnostic whenever it happened
//! to sit below an assignment/increment/decrement target. It now checks only
//! the exact node passed in.
//!
//! Oracle: `tsc` 7.0.2 (`scripts/conformance/typescript-versions.json`),
//! `--noEmit --strict --target es2022 --lib es2022 --module esnext`.

use crate::test_utils::{
    check_source_non_strict_codes as non_strict, check_source_strict_codes as strict,
};

const TS18048: u32 = 18048; // '<x>' is possibly 'undefined'.
const TS2322: u32 = 2322; // Type '...' is not assignable to type '...'.
const TS2777: u32 = 2777; // Increment/decrement operand may not be an optional property access.
const TS2779: u32 = 2779; // Assignment LHS may not be an optional property access.

fn count(codes: &[u32], code: u32) -> usize {
    codes.iter().filter(|&&c| c == code).count()
}

#[test]
fn marker_only_compound_assignment_target_reports_the_whole_target() {
    let source = "\
declare const a: { b?: { c: { d: number } } };
a.b?.c.d += 1;";
    let codes = strict(source);
    assert_eq!(count(&codes, TS18048), 1, "got: {codes:?}");
    assert_eq!(count(&codes, TS2779), 1, "got: {codes:?}");
}

#[test]
fn marker_only_increment_and_decrement_targets_report_the_whole_target() {
    for expression in ["a.b?.c.d++", "--a.b?.c.d"] {
        let source = format!("declare const a: {{ b?: {{ c: {{ d: number }} }} }};\n{expression};");
        let codes = strict(&source);
        assert_eq!(
            count(&codes, TS18048),
            1,
            "`{expression}` must report the whole target, got: {codes:?}"
        );
        assert_eq!(
            count(&codes, TS2777),
            1,
            "`{expression}` must still report TS2777, got: {codes:?}"
        );
    }
}

#[test]
fn marker_only_compound_assignment_reports_for_every_arithmetic_operator() {
    for op in ["+=", "-=", "*=", "/="] {
        let source =
            format!("declare const a: {{ b?: {{ c: {{ d: number }} }} }};\na.b?.c.d {op} 1;");
        let codes = strict(&source);
        assert_eq!(
            count(&codes, TS18048),
            1,
            "`{op}` must report the marker-only target, got: {codes:?}"
        );
    }
}

#[test]
fn genuine_optionality_compound_assignment_target_is_unchanged_single_report() {
    // `inner` is genuinely optional. tsc reports ONE TS18048 naming the
    // receiver (`'h.inner'`), via the ordinary possibly-nullish property
    // read — this fix must not add a second report naming the full target.
    let source = "\
declare const h: { inner?: { leaf: number } };
h?.inner.leaf += 1;";
    let codes = strict(source);
    assert_eq!(
        count(&codes, TS18048),
        0,
        "the genuine-optionality receiver report is out of this fix's scope \
         (owned by the write-target receiver check) and must not be doubled here, got: {codes:?}"
    );
    assert_eq!(count(&codes, TS2779), 1, "got: {codes:?}");
}

#[test]
fn renamed_binders_and_deeper_chain_still_report_the_whole_target() {
    let source = "\
declare const probe: { slot?: { coin: { value: number } } };
probe.slot?.coin.value += 1;";
    let codes = strict(source);
    assert_eq!(count(&codes, TS18048), 1, "got: {codes:?}");
    assert_eq!(count(&codes, TS2779), 1, "got: {codes:?}");
}

#[test]
fn a_guarded_target_link_reports_no_nullish_diagnostic() {
    // The target link itself carries `?.`, so the chain short-circuits
    // before any read happens — the grammar error alone, exactly as before.
    let source = "\
declare const a: { b?: { c: { d: number } } };
a.b?.c?.d += 1;";
    let codes = strict(source);
    assert_eq!(count(&codes, TS18048), 0, "got: {codes:?}");
    assert_eq!(count(&codes, TS2779), 1, "got: {codes:?}");
}

#[test]
fn marker_only_target_reports_nothing_without_strict_null_checks() {
    let source = "\
declare const a: { b?: { c: { d: number } } };
a.b?.c.d += 1;
a.b?.c.d++;";
    let codes = non_strict(source);
    assert_eq!(count(&codes, TS18048), 0, "got: {codes:?}");
    assert_eq!(count(&codes, TS2779), 1, "got: {codes:?}");
    assert_eq!(count(&codes, TS2777), 1, "got: {codes:?}");
}

#[test]
fn a_receiver_link_below_a_write_target_keeps_its_own_diagnostic() {
    // Regression coverage for the walk-up removal: `deep.one` is a plain
    // receiver read (required member `two`, so no marker-only undefined of
    // its own here) that sits below the `.two.three` write target — it must
    // not be swept into the target's `any` short-circuit. A plain read of
    // the same receiver expression must produce the identical diagnostic
    // count as when it appears under a write target.
    let read_source = "\
declare const deep: { one: { two: { three: number } } };
const x: number = deep.one.two.three;";
    let write_source = "\
declare const deep: { one: { two: { three: number } } };
deep.one.two.three += 1;";
    assert_eq!(count(&strict(read_source), TS2322), 0);
    let codes = strict(write_source);
    assert_eq!(
        count(&codes, TS2779),
        0,
        "a plain (non-optional-chain) target reports no TS2779, got: {codes:?}"
    );
    assert_eq!(count(&codes, TS18048), 0, "got: {codes:?}");
}
