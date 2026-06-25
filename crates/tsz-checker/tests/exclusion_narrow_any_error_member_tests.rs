//! Tests for `x !== undefined` exclusion narrowing of `any` / error-typed
//! members.
//!
//! Structural rule: when narrowing a member by excluding `undefined` (the true
//! branch of `obj.prop !== undefined`, or the symmetric `=== undefined` false
//! branch), a member whose type is `any` — including an alias that resolves to
//! `any`, or an `any` written inline — or an *error* type (e.g. an unresolved
//! type name from a failed import / a typo) is INERT: tsc keeps it unchanged
//! (`any - X = any`; an error type stays the error type). tsz previously ran the
//! exclusion through `is_assignable_to(member, undefined)`, which is `true` for
//! `any` (assignable to everything) and short-circuits `true` for an error type,
//! so the member was dropped and the property collapsed to `never`. Re-reading
//! the guarded property then drew a spurious TS2339 ("property does not exist on
//! never") / TS18048. Owner layer: solver narrowing `member_excluded_by`.
//!
//! The negative controls prove the fix does not blanket-suppress real
//! exclusion: a genuine optional `string` property still narrows to `string`
//! (and excess `undefined` is still removed), and a local variable of union type
//! still narrows correctly. Binder names are varied across the tests so the
//! behavior cannot depend on a particular identifier (anti-hardcoding).

use tsz_checker::test_utils::check_source_strict_codes;

// ---------------------------------------------------------------------------
// Positive cases: `any` / error members must stay INERT (no spurious error).
// ---------------------------------------------------------------------------

/// Reported repro: an optional member whose type is an *unresolved* name
/// (`DoesNotExist`) — an error type. After `o.validate !== undefined` the member
/// must remain readable, not collapse to `never`.
#[test]
fn error_typed_member_inert_under_undefined_exclusion() {
    let codes = check_source_strict_codes(
        r#"
declare const o: { validate?: DoesNotExist };
if (o.validate !== undefined) {
  const x = o.validate;
}
"#,
    );
    // The only legitimate diagnostic is TS2304 for the unresolved `DoesNotExist`
    // name itself. There must be NO TS2339 (property on `never`) or TS18048.
    assert!(
        !codes.contains(&2339) && !codes.contains(&18048),
        "error-typed member must stay inert under `!== undefined`, got: {codes:?}"
    );
}

/// Equivalent shape: an unresolved name reached through a *failed import*. Same
/// error-type path, different binder spelling.
#[test]
fn imported_unresolved_member_inert_under_undefined_exclusion() {
    let codes = check_source_strict_codes(
        r#"
import { Missing } from "./nowhere";
declare const config: { handler?: Missing };
if (config.handler !== undefined) {
  const ref = config.handler;
  void ref;
}
"#,
    );
    assert!(
        !codes.contains(&2339) && !codes.contains(&18048),
        "member typed by a failed import must stay inert under `!== undefined`, got: {codes:?}"
    );
}

/// Inline `any` member: `any - undefined = any`. Reading the guarded property
/// must be fine.
#[test]
fn inline_any_member_inert_under_undefined_exclusion() {
    let codes = check_source_strict_codes(
        r#"
declare const bag: { payload?: any };
if (bag.payload !== undefined) {
  const taken = bag.payload;
  void taken;
}
"#,
    );
    assert!(
        !codes.contains(&2339) && !codes.contains(&18048),
        "inline-any member must stay `any` under `!== undefined`, got: {codes:?}"
    );
}

/// Alias-`any` member: a type alias that resolves to `any`. Must be caught by
/// resolving the member before classifying it (not just literal `TypeId::ANY`).
/// Different identifier spellings again.
#[test]
fn alias_any_member_inert_under_undefined_exclusion() {
    let codes = check_source_strict_codes(
        r#"
type Loose = any;
declare const container: { slot?: Loose };
if (container.slot !== undefined) {
  const grabbed = container.slot;
  void grabbed;
}
"#,
    );
    assert!(
        !codes.contains(&2339) && !codes.contains(&18048),
        "alias-any member must stay inert under `!== undefined`, got: {codes:?}"
    );
}

/// Symmetric form: the `=== undefined` false branch performs the same exclusion.
#[test]
fn any_member_inert_under_equality_false_branch() {
    let codes = check_source_strict_codes(
        r#"
declare const record: { entry?: any };
if (record.entry === undefined) {
} else {
  const used = record.entry;
  void used;
}
"#,
    );
    assert!(
        !codes.contains(&2339) && !codes.contains(&18048),
        "any member must stay inert in the `=== undefined` else branch, got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Negative controls: real exclusion narrowing MUST still happen.
// ---------------------------------------------------------------------------

/// A genuine optional `string` property still narrows: after `v !== undefined`
/// the member is `string`, so `.toUpperCase()` is fine and there is NO TS18048.
/// This proves excess-`undefined` removal still works on real members.
#[test]
fn optional_string_member_still_narrows() {
    let codes = check_source_strict_codes(
        r#"
declare const settings: { label?: string };
if (settings.label !== undefined) {
  const upper = settings.label.toUpperCase();
  void upper;
}
"#,
    );
    assert!(
        !codes.contains(&18048) && !codes.contains(&2339),
        "optional string member must narrow to string (no diagnostic), got: {codes:?}"
    );
}

/// Negative guard for the OTHER direction: without the guard, the optional
/// `string` member is possibly `undefined`, so reading `.toUpperCase()`
/// unguarded MUST still report TS18048. The fix must not suppress real
/// undefined-handling.
#[test]
fn optional_string_member_unguarded_still_reports() {
    let codes = check_source_strict_codes(
        r#"
declare const settings: { title?: string };
const upper = settings.title.toUpperCase();
void upper;
"#,
    );
    assert!(
        codes.contains(&18048),
        "unguarded possibly-undefined string member must still report TS18048, got: {codes:?}"
    );
}

/// A local variable of a union type still narrows correctly through the
/// exclusion path (the literal-`any` local guard must not be the only thing
/// keeping locals working). After `n !== undefined`, `n` is `number`.
#[test]
fn local_union_variable_still_narrows() {
    let codes = check_source_strict_codes(
        r#"
declare const maybeNum: number | undefined;
const n = maybeNum;
if (n !== undefined) {
  const fixed = n.toFixed(2);
  void fixed;
}
"#,
    );
    assert!(
        !codes.contains(&18048) && !codes.contains(&2339),
        "local union variable must narrow to number after `!== undefined`, got: {codes:?}"
    );
}

/// A `void` member is still excluded by `!== undefined` (the pre-existing
/// `void`-vs-`undefined` special case must survive the new guard): after the
/// guard the union collapses to `boolean`, assignable to a `boolean` target with
/// no TS2322.
#[test]
fn void_member_still_excluded_by_undefined() {
    let codes = check_source_strict_codes(
        r#"
declare const flag: boolean | void;
let target: boolean;
if (flag !== undefined) {
  target = flag;
}
void target;
"#,
    );
    assert!(
        !codes.contains(&2322),
        "`void` member must still be excluded by `!== undefined`, got: {codes:?}"
    );
}
