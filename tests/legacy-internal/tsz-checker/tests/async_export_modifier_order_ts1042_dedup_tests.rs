//! `async export class C {}` / `async export interface I {}` /
//! `async export enum E {}` / `async export namespace N {}` reported TS1029
//! ("'export' modifier must precede 'async' modifier.") *and* TS1042
//! ("'async' modifier cannot be used here."), where tsc reports TS1029
//! alone.
//!
//! Same mechanism as `declare_export_default_modifier_order_ts1319_dedup_tests`:
//! TS1029 is a grammar-only diagnostic (`is_parser_grammar_code`, not
//! `is_real_syntax_error`, in `tsz-cli`'s `check_utils.rs`), so it never
//! flips the whole-file `has_parse_errors` gate. tsc still suppresses TS1042
//! here because its `checkGrammarModifiers` already reported an error on
//! this exact modifier list and returns early, skipping the sibling check
//! that would otherwise ask whether `async` is legal on a class/interface/
//! enum/namespace declaration at all. tsz's parser emits TS1029 eagerly
//! during parsing (`look_ahead_async_before_export_target`,
//! `parse_statement_async_declaration_or_expression`, #16403 slice 3) —
//! before the declaration's own modifier list is checked as a unit — so
//! `check_async_modifier_on_declaration`
//! (`state/state_checking_members/member_declaration_checks.rs`) re-derives
//! "did it already fire" from `all_parse_error_positions` instead of an AST
//! field.
//!
//! Uses [`check_source_codes_with_grammar_only_parse_health`], not
//! [`crate::test_utils::check_source_codes_with_parse_health`]: the coarse
//! helper sets `has_parse_errors = true` for ANY parser diagnostic,
//! including TS1029, which would trip the pre-existing whole-file gate and
//! hide this bug behind a false negative.
//!
//! All expectations measured directly against the pinned `typescript@7.0.2`
//! oracle (`scripts/conformance/typescript-versions.json`),
//! `--noEmit --strict --pretty false --target es2022 --module es2022`.

use crate::test_utils::check_source_codes_with_grammar_only_parse_health;

const MODIFIER_MUST_PRECEDE_MODIFIER: u32 = 1029;
const ASYNC_MODIFIER_CANNOT_BE_USED_HERE: u32 = 1042;

/// oracle: TS1029 alone.
#[test]
fn async_export_class_reports_only_ts1029() {
    let codes = check_source_codes_with_grammar_only_parse_health("async export class C {}");
    assert!(
        codes.contains(&MODIFIER_MUST_PRECEDE_MODIFIER),
        "expected TS1029 for the misordered `async export`; got {codes:?}"
    );
    assert!(
        !codes.contains(&ASYNC_MODIFIER_CANNOT_BE_USED_HERE),
        "tsc's checkGrammarModifiers already reported on this node and returns \
         early, so TS1042 must not additionally fire; got {codes:?}"
    );
}

/// oracle: TS1029 alone.
#[test]
fn async_export_interface_reports_only_ts1029() {
    let codes = check_source_codes_with_grammar_only_parse_health("async export interface I {}");
    assert!(codes.contains(&MODIFIER_MUST_PRECEDE_MODIFIER));
    assert!(!codes.contains(&ASYNC_MODIFIER_CANNOT_BE_USED_HERE));
}

/// oracle: TS1029 alone.
#[test]
fn async_export_enum_reports_only_ts1029() {
    let codes = check_source_codes_with_grammar_only_parse_health("async export enum E { A }");
    assert!(codes.contains(&MODIFIER_MUST_PRECEDE_MODIFIER));
    assert!(!codes.contains(&ASYNC_MODIFIER_CANNOT_BE_USED_HERE));
}

/// oracle: TS1029 alone.
#[test]
fn async_export_namespace_reports_only_ts1029() {
    let codes = check_source_codes_with_grammar_only_parse_health("async export namespace N {}");
    assert!(codes.contains(&MODIFIER_MUST_PRECEDE_MODIFIER));
    assert!(!codes.contains(&ASYNC_MODIFIER_CANNOT_BE_USED_HERE));
}

/// A nested container — the position-range check must not depend on nesting
/// depth or on the enclosing container kind.
#[test]
fn async_export_class_in_namespace_body_reports_only_ts1029() {
    let codes = check_source_codes_with_grammar_only_parse_health(
        "namespace N { async export class C {} }",
    );
    assert!(codes.contains(&MODIFIER_MUST_PRECEDE_MODIFIER));
    assert!(!codes.contains(&ASYNC_MODIFIER_CANNOT_BE_USED_HERE));
}

/// Negative control: a bare `async class C {}` (no `export`, so no
/// modifier-order violation exists to deduplicate against) must still report
/// TS1042 alone — the new suppression must not fire when nothing preceded
/// it.
#[test]
fn plain_async_class_still_reports_ts1042() {
    let codes = check_source_codes_with_grammar_only_parse_health("async class C {}");
    assert!(
        codes.contains(&ASYNC_MODIFIER_CANNOT_BE_USED_HERE),
        "no modifier-order diagnostic exists here, so TS1042 must still fire; got {codes:?}"
    );
    assert!(!codes.contains(&MODIFIER_MUST_PRECEDE_MODIFIER));
}

/// Negative control: a bare `async enum E {}`.
#[test]
fn plain_async_enum_still_reports_ts1042() {
    let codes = check_source_codes_with_grammar_only_parse_health("async enum E { A }");
    assert!(codes.contains(&ASYNC_MODIFIER_CANNOT_BE_USED_HERE));
    assert!(!codes.contains(&MODIFIER_MUST_PRECEDE_MODIFIER));
}

/// Negative control: `async function f() {}` is unaffected — `async` is
/// legal on a function declaration, so `check_async_modifier_on_declaration`
/// (which only ever runs for class/interface/enum/module declarations) is
/// not in the picture at all, and this must stay diagnostic-free.
#[test]
fn plain_async_function_reports_no_modifier_diagnostic() {
    let codes = check_source_codes_with_grammar_only_parse_health("async function f() {}");
    assert!(!codes.contains(&ASYNC_MODIFIER_CANNOT_BE_USED_HERE));
    assert!(!codes.contains(&MODIFIER_MUST_PRECEDE_MODIFIER));
}
