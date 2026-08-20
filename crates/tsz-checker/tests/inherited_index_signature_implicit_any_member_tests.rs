//! A property signature with no type annotation (`interface I { a; }`) is
//! *legally* implicit `any`, not a missing-annotation bug. tsz previously
//! lowered it to the `error` sentinel, which poisoned every structural query
//! over the containing object type — most visibly dropping an inherited index
//! signature and silencing TS2413. The rationale and the exact failure mode are
//! documented on `TypeLowering::lower_property_signature_type`; these tests pin
//! the observable behavior.
//!
//! Binder names are varied per case so nothing keys on a specific identifier.

use tsz_checker::test_utils::{
    check_source_code_messages, check_source_codes, check_source_non_strict_codes,
};

const TS2413: u32 = 2413; // '<x>' index type '<a>' is not assignable to '<y>' index type '<b>'.
const TS7008: u32 = 7008; // Member '<x>' implicitly has an '<any>' type.

/// The rendered TS2413 message must name the member as `any` (proving the member
/// is typed implicit `any`, not the `error` sentinel that would drop the string
/// index). This mirrors the conformance oracle's message key for
/// `inheritedStringIndexersFromDifferentBaseTypes2.ts`:
/// `'number' index type '{}' is not assignable to 'string' index type '{ a: any; }'.`
#[test]
fn ts2413_message_renders_implicit_any_member_as_any() {
    let diags = check_source_code_messages(
        r#"
interface Ba { [s: string]: { b; }; }
interface Bd { [s: number]: {}; }
interface Be extends Ba, Bd {}
"#,
    );
    let has_2413_any = diags
        .iter()
        .any(|(code, msg)| *code == TS2413 && msg.contains("{ b: any; }"));
    assert!(
        has_2413_any,
        "TS2413 message must render the implicit-any member as 'any' (not drop the index), got: {diags:#?}",
    );
}

/// The `inheritedStringIndexersFromDifferentBaseTypes2` conformance witness:
/// an interface inherits a `string` index whose value carries an implicitly-`any`
/// member and a `number` index whose value is empty, so `number`-index value `{}`
/// is not assignable to `string`-index value `{ c: any }` and tsc reports TS2413.
/// Before the fix the implicit-any member poisoned the `string` value type and the
/// index was dropped, silencing the diagnostic. An explicit `any` member and an
/// implicit-any member (`{ c; }`) must behave identically here.
#[test]
fn implicit_any_member_matches_explicit_any_member() {
    let implicit = check_source_non_strict_codes(
        r#"
interface Ca { [s: string]: { c; }; }
interface Cd { [s: number]: {}; }
interface Ce extends Ca, Cd {}
"#,
    );
    let explicit = check_source_non_strict_codes(
        r#"
interface Ca { [s: string]: { c: any }; }
interface Cd { [s: number]: {}; }
interface Ce extends Ca, Cd {}
"#,
    );
    assert!(
        implicit.contains(&TS2413),
        "implicit-any: expected TS2413, got: {implicit:?}"
    );
    assert!(
        explicit.contains(&TS2413),
        "explicit-any: expected TS2413, got: {explicit:?}"
    );
    assert_eq!(
        implicit, explicit,
        "implicit-any member must produce the same diagnostics as an explicit `any` member",
    );
}

/// The fix must not over-report: when the `number`-index value is assignable to
/// the `string`-index value (both `{ d: any }`), there is no TS2413. Before the
/// fix this was clean only because the string index was dropped entirely; now it
/// is clean for the right reason — the value types are compatible.
#[test]
fn compatible_index_values_with_implicit_any_member_stays_clean() {
    let codes = check_source_non_strict_codes(
        r#"
interface Na { [s: string]: { d; }; }
interface Nd { [s: number]: { d; }; }
interface Ne extends Na, Nd {}
"#,
    );
    assert!(
        !codes.contains(&TS2413),
        "compatible index values must stay clean; got unexpected TS2413: {codes:?}",
    );
}

/// The direct own-declaration conflict path (member-scan, which recomputes the
/// value type) already reported TS2413 and must stay unchanged.
#[test]
fn direct_own_declaration_index_conflict_reports_ts2413() {
    let codes = check_source_non_strict_codes(
        r#"
interface Oa {
  [s: string]: { e; };
  [n: number]: {};
}
"#,
    );
    assert!(
        codes.contains(&TS2413),
        "own-declaration index conflict must report TS2413, got: {codes:?}",
    );
}

/// An implicit-any property is `any`, not `error`: a concrete value assigns to
/// it cleanly and no spurious diagnostics are introduced.
#[test]
fn implicit_any_property_accepts_concrete_value_stays_clean() {
    let codes = check_source_non_strict_codes(
        r#"
interface Pa { f; }
const pv: Pa = { f: 123 };
const pn: number = pv.f;
"#,
    );
    assert!(
        !codes.contains(&TS2413),
        "no index diagnostic expected here, got: {codes:?}",
    );
    assert!(
        codes.is_empty(),
        "implicit-any property must accept a concrete value without diagnostics, got: {codes:?}",
    );
}

/// Under `noImplicitAny` (default options) a property signature with no
/// annotation still reports TS7008 — the fix only changes the lowered *type*,
/// not the missing-annotation diagnostic.
#[test]
fn implicit_any_property_still_reports_ts7008_under_no_implicit_any() {
    let codes = check_source_codes("interface Ga { g; }");
    assert!(
        codes.contains(&TS7008),
        "missing annotation must report TS7008 under noImplicitAny, got: {codes:?}",
    );
}
