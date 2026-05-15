//! TS1207 decorated get/set accessor pair checks.
//!
//! Structural rule: when `experimentalDecorators` is enabled and both
//! accessors of the same static/instance property name have decorators, tsc
//! emits TS1207 at the later accessor's decorator.

use tsz_checker::test_utils::{check_source_codes, check_source_codes_experimental_decorators};

fn check_experimental(source: &str) -> Vec<u32> {
    check_source_codes_experimental_decorators(source)
}

#[test]
fn decorated_get_then_set_same_name_emits_ts1207() {
    let codes = check_experimental(
        "function logged(..._args: any[]): void {}\n\
         class LoggedAccessor {\n\
             @logged\n\
             get value(): number { return 1; }\n\
             @logged\n\
             set value(v: number) {}\n\
         }\n",
    );

    assert!(
        codes.contains(&1207),
        "decorated get/set pair should emit TS1207; got: {codes:?}",
    );
}

#[test]
fn decorated_set_then_get_same_name_emits_ts1207() {
    let codes = check_experimental(
        "function marker(..._args: any[]): void {}\n\
         class ReorderedAccessor {\n\
             @marker\n\
             set renamed(v: number) {}\n\
             @marker\n\
             get renamed(): number { return 1; }\n\
         }\n",
    );

    assert!(
        codes.contains(&1207),
        "decorated set/get pair should emit TS1207 regardless of member order; got: {codes:?}",
    );
}

#[test]
fn decorated_computed_literal_accessors_emit_ts1207() {
    let codes = check_experimental(
        "function observe(..._args: any[]): void {}\n\
         class ComputedAccessor {\n\
             @observe\n\
             get ['computed'](): number { return 1; }\n\
             @observe\n\
             set ['computed'](v: number) {}\n\
         }\n",
    );

    assert!(
        codes.contains(&1207),
        "decorated computed-literal accessors should emit TS1207; got: {codes:?}",
    );
}

#[test]
fn decorated_computed_identifier_accessors_emit_ts1207() {
    let codes = check_experimental(
        "function record(..._args: any[]): void {}\n\
         const key = 'resolved';\n\
         class ResolvedAccessor {\n\
             @record\n\
             get [key](): number { return 1; }\n\
             @record\n\
             set [key](v: number) {}\n\
         }\n",
    );

    assert!(
        codes.contains(&1207),
        "decorated accessors sharing a resolved computed name should emit TS1207; got: {codes:?}",
    );
}

#[test]
fn single_decorated_accessor_pair_does_not_emit_ts1207() {
    let codes = check_experimental(
        "function logged(..._args: any[]): void {}\n\
         class PartiallyDecorated {\n\
             @logged\n\
             get value(): number { return 1; }\n\
             set value(v: number) {}\n\
         }\n",
    );

    assert!(
        !codes.contains(&1207),
        "only one decorated accessor should not emit TS1207; got: {codes:?}",
    );
}

#[test]
fn static_and_instance_decorated_accessors_do_not_share_pair() {
    let codes = check_experimental(
        "function logged(..._args: any[]): void {}\n\
         class SplitAccessor {\n\
             @logged\n\
             static get value(): number { return 1; }\n\
             @logged\n\
             set value(v: number) {}\n\
         }\n",
    );

    assert!(
        !codes.contains(&1207),
        "static and instance accessors should be checked as distinct pairs; got: {codes:?}",
    );
}

#[test]
fn no_ts1207_without_experimental_decorators() {
    let codes = check_source_codes(
        "function logged(..._args: any[]): void {}\n\
         class StageThreeDecorators {\n\
             @logged\n\
             get value(): number { return 1; }\n\
             @logged\n\
             set value(v: number) {}\n\
         }\n",
    );

    assert!(
        !codes.contains(&1207),
        "TS1207 is gated to experimentalDecorators; got: {codes:?}",
    );
}
