//! Duplicate parameter names (`TS2300`) for every function-like signature
//! written in *type* position.
//!
//! Structural rule, one sentence: a repeated parameter name is a duplicate
//! identifier in *any* function-like signature — `tsc` runs the same
//! `checkGrammarParameterList` for a function/constructor type and for every
//! call, construct, and method signature of an object type literal as it does
//! for a function declaration or an interface member, and blames `TS2300` on
//! **every** occurrence of the name.
//!
//! Two pre-existing gaps in the type-position paths are closed here (both were
//! called out as follow-ups in the header of
//! `duplicate_parameter_names_function_expression_forms_tests.rs`):
//!
//! 1. **Object type-literal signatures did not run the check at all.** A
//!    `type T = { m(a, a): void }` (and the call/construct signature forms)
//!    routed through `get_type_from_type_literal`, which never consulted the
//!    parameter list for duplicates, so tsz was silent where tsc reports two
//!    diagnostics. The check now runs once per written type-literal node —
//!    alias body, inline annotation, nested — inside that construction path,
//!    which is the position-complete home (`types/type_literal_checker.rs`).
//! 2. **Function/constructor types reported one occurrence instead of two.**
//!    `check_duplicate_parameters_in_type` (`types/type_node_helpers.rs`) blamed
//!    only the *second* occurrence; it now retroactively blames the first, the
//!    same way `CheckerState::check_duplicate_parameters` does for the
//!    declaration forms, so a two-parameter clash is two diagnostics and a
//!    three-parameter clash is three.
//!
//! Every expectation below is pinned against `typescript@7.0.2`
//! (`scripts/conformance/oracle.sh <file> --strict`), including the anchor
//! offset: `TS2300` sits on each occurrence of the repeated name. Binder names
//! are distinct in every row so nothing can key on a particular identifier
//! string (the shape, not the name, drives the rule), and each row's negative
//! sibling uses the same container and binder shape with *distinct* names to
//! prove the check keys on the duplication and not on the container.

use tsz_checker::test_utils::check_source_strict;

/// `(code, 0-based start)` for every diagnostic, sorted — the shape the oracle
/// rows were recorded in. An exact assertion also proves no *other* diagnostic
/// fires on these signatures.
fn sites(source: &str) -> Vec<(u32, u32)> {
    let mut out: Vec<(u32, u32)> = check_source_strict(source)
        .iter()
        .map(|d| (d.code, d.start))
        .collect();
    out.sort_unstable();
    out
}

#[track_caller]
fn assert_sites(source: &str, expected: &[(u32, u32)]) {
    assert_eq!(sites(source), expected.to_vec(), "source: {source}");
}

// ---------------------------------------------------------------------------
// Function and constructor types. Previously reported one occurrence; now two.
// ---------------------------------------------------------------------------

#[test]
fn function_type_duplicate_parameter_reports_both_occurrences() {
    assert_sites(
        "type Fn1 = (dup1: number, dup1: string) => void;",
        &[(2300, 12), (2300, 26)],
    );
}

#[test]
fn constructor_type_duplicate_parameter_reports_both_occurrences() {
    assert_sites(
        "type Ct1 = new (dup2: number, dup2: string) => void;",
        &[(2300, 16), (2300, 30)],
    );
}

#[test]
fn function_type_binding_pattern_duplicate_reports_both_occurrences() {
    assert_sites(
        "type Bp2 = ({ dup7, dup7 }: any) => void;",
        &[(2300, 14), (2300, 20)],
    );
}

// ---------------------------------------------------------------------------
// Object type-literal signatures (alias body). Previously silent on all arms.
// ---------------------------------------------------------------------------

#[test]
fn type_literal_call_signature_duplicate_reports_both_occurrences() {
    assert_sites(
        "type Cl1 = { (dup3: number, dup3: string): void };",
        &[(2300, 14), (2300, 28)],
    );
}

#[test]
fn type_literal_construct_signature_duplicate_reports_both_occurrences() {
    assert_sites(
        "type Cs1 = { new (dup4: number, dup4: string): void };",
        &[(2300, 18), (2300, 32)],
    );
}

#[test]
fn type_literal_method_signature_duplicate_reports_both_occurrences() {
    assert_sites(
        "type Me1 = { run(dup5: number, dup5: string): void };",
        &[(2300, 17), (2300, 31)],
    );
}

#[test]
fn type_literal_method_binding_pattern_duplicate_reports_both_occurrences() {
    assert_sites(
        "type Bp1 = { run({ dup6, dup6 }: any): void };",
        &[(2300, 19), (2300, 25)],
    );
}

// ---------------------------------------------------------------------------
// Inline annotation positions (not just the alias body). The construction path
// is reached once per written node, so each position reports exactly twice.
// ---------------------------------------------------------------------------

#[test]
fn variable_annotation_type_literal_duplicate_reports_both_occurrences() {
    assert_sites(
        "let vv: { run(dup8: number, dup8: string): void };",
        &[(2300, 14), (2300, 28)],
    );
}

#[test]
fn parameter_annotation_type_literal_duplicate_reports_both_occurrences() {
    assert_sites(
        "function fn9(pp: { run(dup9: number, dup9: string): void }) {}",
        &[(2300, 23), (2300, 37)],
    );
}

// ---------------------------------------------------------------------------
// More than two occurrences: every occurrence is blamed, not just the extras.
// ---------------------------------------------------------------------------

#[test]
fn function_type_three_occurrences_reports_three_diagnostics() {
    assert_sites(
        "type Th1 = (t3: number, t3: number, t3: number) => void;",
        &[(2300, 12), (2300, 24), (2300, 36)],
    );
}

// ---------------------------------------------------------------------------
// Negative controls: same container and binder shape, distinct names -> clean.
// Proves the rule keys on the duplication, not on the container.
// ---------------------------------------------------------------------------

#[test]
fn function_type_distinct_parameters_is_clean() {
    assert_sites("type Ng1 = (ok1: number, ok2: string) => void;", &[]);
}

#[test]
fn type_literal_method_distinct_parameters_is_clean() {
    assert_sites("type Ng2 = { run(ok3: number, ok4: string): void };", &[]);
}

// ---------------------------------------------------------------------------
// Double-emission guards: the check must fire once per *written* signature, not
// once per instantiation, and independently per overload.
// ---------------------------------------------------------------------------

#[test]
fn generic_type_literal_alias_instantiated_twice_reports_once() {
    // The alias body is walked once when `T`'s declared type is computed; the
    // two instantiations reuse the cached type, so the clash is blamed twice,
    // not four times.
    assert_sites(
        "type T<X> = { m(dupG: X, dupG: X): void };\nlet p: T<number>;\nlet q: T<string>;\n",
        &[(2300, 16), (2300, 25)],
    );
}

#[test]
fn type_literal_method_overloads_each_report_their_own_duplicate() {
    assert_sites(
        "type O = { run(dupA: number, dupA: number): void; run(dupB: string, dupB: string): void };",
        &[(2300, 15), (2300, 29), (2300, 54), (2300, 68)],
    );
}

// ---------------------------------------------------------------------------
// Declaration-form baselines already reported both occurrences; pin them so a
// future refactor of the shared reporting cannot silently regress them.
// ---------------------------------------------------------------------------

#[test]
fn interface_method_duplicate_still_reports_both_occurrences() {
    assert_sites(
        "interface If1 { run(dup10: number, dup10: string): void }",
        &[(2300, 20), (2300, 35)],
    );
}
