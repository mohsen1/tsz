//! Apparent type of `unknown` under `strictNullChecks: false`.
//!
//! `tsc`'s `getApparentType` ends with
//! `t.flags & TypeFlags.Unknown && !strictNullChecks ? emptyObjectType : t`, so
//! with the flag off an `unknown` receiver looks its members up on the empty
//! object type and therefore finds the global `Object` members (`toString`,
//! `valueOf`, `hasOwnProperty`, ...). An *unconstrained* type parameter reaches
//! the same rule: `getBaseConstraintOfType` yields no constraint, so its
//! apparent type is `unknown`.
//!
//! tsz reported TS2339 for every such access regardless of the flag, producing
//! false positives on `// @strict: false` corpus rows —
//! `propertyAccessOnTypeParameterWithoutConstraints.ts` expects zero
//! diagnostics but got three.
//!
//! The negative half matters just as much: a member that is *not* on `Object`
//! must still report TS2339, and must report it against the original receiver
//! (`T` / `unknown`), never against the substituted apparent type. Under
//! `strictNullChecks: true` nothing changes — `unknown` keeps no members.

use tsz_checker::test_utils::{
    check_source_non_strict_codes, check_source_strict_codes, check_with_options_code_messages,
    non_strict_checker_options,
};

/// TS2339 count, so a test can assert "no false positive" and "still reported"
/// without depending on unrelated diagnostics in the same source.
fn ts2339_count(codes: &[u32]) -> usize {
    codes.iter().filter(|&&code| code == 2339).count()
}

#[test]
fn object_members_resolve_on_unconstrained_type_parameter_when_not_strict() {
    let source = r"
function f<T>(x: T) {
    return x.toString() + x.valueOf() + x.hasOwnProperty('a') + x.isPrototypeOf(x);
}
";
    let codes = check_source_non_strict_codes(source);
    assert_eq!(
        ts2339_count(&codes),
        0,
        "Object members on a bare `T` resolve through the `{{}}` apparent type when strictNullChecks is off; got: {codes:?}"
    );
}

#[test]
fn object_members_still_error_on_unconstrained_type_parameter_when_strict() {
    let source = r"
function f<T>(x: T) {
    return x.toString();
}
";
    let codes = check_source_strict_codes(source);
    assert_eq!(
        ts2339_count(&codes),
        1,
        "under strictNullChecks the apparent type of a bare `T` is `unknown`, which has no members; got: {codes:?}"
    );
}

#[test]
fn absent_member_on_unconstrained_type_parameter_still_errors_when_not_strict() {
    let source = r"
function f<T>(x: T) {
    return x.notAnObjectMember();
}
";
    let codes = check_source_non_strict_codes(source);
    assert_eq!(
        ts2339_count(&codes),
        1,
        "a member that is not on the global `Object` must still be reported; got: {codes:?}"
    );
}

#[test]
fn absent_member_reports_against_the_type_parameter_not_the_apparent_type() {
    let source = r"
function f<TElem>(x: TElem) {
    return x.notAnObjectMember();
}
";
    let messages = check_with_options_code_messages(source, non_strict_checker_options());
    let text = messages
        .iter()
        .find(|(code, _)| *code == 2339)
        .map(|(_, text)| text.clone())
        .unwrap_or_else(|| panic!("expected TS2339, got: {messages:?}"));
    assert!(
        text.contains("'TElem'"),
        "TS2339 must name the receiver the user wrote, not the substituted apparent type; got: {text}"
    );
}

#[test]
fn object_members_resolve_on_unknown_receiver_when_not_strict() {
    let source = r"
declare const u: unknown;
const a = u.toString();
const b = u.hasOwnProperty('k');
";
    let codes = check_source_non_strict_codes(source);
    assert_eq!(
        ts2339_count(&codes),
        0,
        "an `unknown` receiver has the `{{}}` apparent type when strictNullChecks is off; got: {codes:?}"
    );
}

#[test]
fn absent_member_on_unknown_receiver_still_errors_when_not_strict() {
    let source = r"
declare const u: unknown;
const a = u.notAnObjectMember;
";
    let codes = check_source_non_strict_codes(source);
    assert_eq!(
        ts2339_count(&codes),
        1,
        "`unknown` gains only the empty object type's members, not arbitrary ones; got: {codes:?}"
    );
}

#[test]
fn alias_and_wrapper_forms_of_unknown_follow_the_same_rule() {
    let source = r"
type Alias = unknown;
type Wrap<T> = T;
declare const al: Alias;
declare const w: Wrap<unknown>;
const a = al.toString();
const b = w.hasOwnProperty('k');
const c = al.absentOne;
const d = w.absentTwo;
";
    let codes = check_source_non_strict_codes(source);
    assert_eq!(
        ts2339_count(&codes),
        2,
        "aliased and wrapper-instantiated `unknown` behave exactly like the bare form: two hits, two misses; got: {codes:?}"
    );
}

#[test]
fn constrained_type_parameter_is_unaffected() {
    let source = r"
interface Stamped { at: number; }
function f<TItem extends Stamped>(x: TItem) {
    return x.at;
}
function g<TItem extends Stamped>(x: TItem) {
    return x.notOnStamped;
}
";
    let codes = check_source_non_strict_codes(source);
    assert_eq!(
        ts2339_count(&codes),
        1,
        "a constrained parameter resolves through its constraint, so only the absent member errors; got: {codes:?}"
    );
}

#[test]
fn generic_class_member_access_follows_the_receiver_binder() {
    let source = r"
class Holder<TValue> {
    constructor(private value: TValue) {}
    describe() { return this.value.toLocaleString(); }
    broken() { return this.value.noSuchMember(); }
}
const h = new Holder<number>(1);
const described = h.describe();
";
    let codes = check_source_non_strict_codes(source);
    assert_eq!(
        ts2339_count(&codes),
        1,
        "the rule applies to a type parameter reached through `this`, and the concrete instantiation still checks; got: {codes:?}"
    );
}

#[test]
fn inferred_unconstrained_call_return_follows_the_same_rule() {
    let source = r"
declare function mk<T>(): T;
const a = mk().propertyIsEnumerable('x');
const b = mk().nothingHere;
";
    let codes = check_source_non_strict_codes(source);
    assert_eq!(
        ts2339_count(&codes),
        1,
        "an uninferable `T` return widens to `unknown` and follows the same apparent-type rule; got: {codes:?}"
    );
}
