//! TS2386 optionality agreement on method overloads declared in an inline type
//! literal (`{ ... }`), as opposed to interface/class bodies.
//!
//! These exercise `check_type_literal_overload_optionality`, which runs on every
//! type literal in the program. The matrix covers the empty/single-member fast
//! paths, matched and mismatched overload pairs, a renamed-binder independence
//! case, a non-method mix that must be ignored, and multiple groups where only
//! the conflicting one reports — so the allocation-light grouping cannot quietly
//! change observable behavior.

use tsz_checker::test_utils::check_source_codes as get_codes;

fn count_2386(source: &str) -> usize {
    get_codes(source).iter().filter(|&&c| c == 2386).count()
}

#[test]
fn empty_type_literal_no_error() {
    let source = "type T = {};\nlet x: T;\n";
    assert_eq!(count_2386(source), 0);
}

#[test]
fn single_method_type_literal_no_error() {
    let source = "let x: { foo(): void };\n";
    assert_eq!(count_2386(source), 0);
}

#[test]
fn matched_required_overloads_no_error() {
    let source = "let x: { foo(): void; foo(s: string): void };\n";
    assert_eq!(count_2386(source), 0);
}

#[test]
fn matched_optional_overloads_no_error() {
    let source = "let x: { foo?(): void; foo?(s: string): void };\n";
    assert_eq!(count_2386(source), 0);
}

#[test]
fn mismatched_optional_then_required_errors() {
    let source = "let x: { foo?(): void; foo(s: string): void };\n";
    assert_eq!(count_2386(source), 1);
}

#[test]
fn mismatched_required_then_optional_errors() {
    let source = "let x: { foo(): void; foo?(s: string): void };\n";
    assert_eq!(count_2386(source), 1);
}

#[test]
fn three_overloads_one_mismatch_reports_once() {
    // First two agree (required); the third disagrees (optional) and is the only
    // one reported, anchored against the first declaration.
    let source = "let x: { foo(): void; foo(s: string): void; foo?(n: number): void };\n";
    assert_eq!(count_2386(source), 1);
}

#[test]
fn three_overloads_two_mismatch_reports_twice() {
    // First is required; the next two are optional and each disagrees with it.
    let source = "let x: { foo(): void; foo?(s: string): void; foo?(n: number): void };\n";
    assert_eq!(count_2386(source), 2);
}

#[test]
fn distinct_method_names_independent_no_error() {
    // Different names never form one overload group, so differing optionality is
    // fine. Proves the grouping keys on the actual member name.
    let source = "let x: { foo?(): void; bar(s: string): void };\n";
    assert_eq!(count_2386(source), 0);
}

#[test]
fn renamed_binder_independence() {
    // Same structural conflict under a different identifier still reports exactly
    // once — the check is not tied to a particular spelling.
    let a = "let x: { alpha?(): void; alpha(s: string): void };\n";
    let b = "let x: { omega?(): void; omega(s: string): void };\n";
    assert_eq!(count_2386(a), 1);
    assert_eq!(count_2386(b), 1);
}

#[test]
fn property_with_same_name_as_method_ignored() {
    // A property signature is not a method overload; mixing one in must not turn
    // a lone method into a reported conflict.
    let source = "let x: { foo: number; bar(): void };\n";
    assert_eq!(count_2386(source), 0);
}

#[test]
fn two_groups_only_conflicting_one_reports() {
    // `foo` overloads agree; `bar` overloads disagree. Exactly one TS2386.
    let source =
        "let x: { foo(): void; foo(s: string): void; bar(): void; bar?(n: number): void };\n";
    assert_eq!(count_2386(source), 1);
}

#[test]
fn string_literal_named_method_overloads_conflict() {
    // Computed/string-literal member names group the same way as identifiers.
    let source = "let x: { \"go\"?(): void; \"go\"(s: string): void };\n";
    assert_eq!(count_2386(source), 1);
}
