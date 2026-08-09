//! Regression tests for #16273: an accessor in a **type-member list** (interface or
//! type literal) parsed through a stripped-down clone of the accessor-declaration
//! path. `parse_get_accessor_signature` asserted `()` instead of parsing a parameter
//! list, and neither signature arm parsed a type-parameter list or carried any of the
//! accessor grammar.
//!
//! tsc keys its accessor grammar on the accessor node, not on its container, so an
//! accessor signature accepts exactly the same parameter list as a class or
//! object-literal accessor and reports the same errors on it.
//!
//! The failure mode was worse than the missing diagnostics: the first parameter token
//! terminated the member, so `get g(this: I): number` became a `TS1005`/`TS1131`
//! cascade — and a parse error of that shape suppresses checker output for the whole
//! file, so one mistyped accessor signature silently disabled type checking for every
//! unrelated construct in its file.
//!
//! Every expectation below was recorded from the pinned `typescript@7.0.2` oracle
//! (`--noEmit --strict --pretty false`), not derived from the rule. Binder names vary
//! per row so no check can key on an identifier string.

use crate::parser::test_fixture::parse_source;

/// Codes emitted by the parser for `source`, in emission order.
fn codes(source: &str) -> Vec<u32> {
    let (parser, _) = parse_source(source);
    parser.get_diagnostics().iter().map(|d| d.code).collect()
}

/// Assert the parser emits exactly `expected` for `source`.
fn assert_codes(source: &str, expected: &[u32], label: &str) {
    let (parser, _) = parse_source(source);
    let actual: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert_eq!(
        actual,
        expected,
        "{label}: parser diagnostics mismatch for {source:?}\n  got: {:?}",
        parser.get_diagnostics()
    );
}

const TS1049_SET_MUST_HAVE_EXACTLY_ONE_PARAMETER: u32 = 1049;
const TS1051_SET_CANNOT_HAVE_OPTIONAL_PARAMETER: u32 = 1051;
const TS1054_GET_CANNOT_HAVE_PARAMETERS: u32 = 1054;
const TS1094_ACCESSOR_CANNOT_HAVE_TYPE_PARAMETERS: u32 = 1094;
const TS1095_SET_CANNOT_HAVE_RETURN_TYPE: u32 = 1095;

// ---------------------------------------------------------------------------
// The reported witness: a `this` parameter no longer breaks the parse.
//
// tsc reports TS2784 on each of these and nothing from the parser. TS2784 is a
// checker diagnostic, so the parser's contract here is to emit *nothing* and
// produce a well-formed accessor node.
// ---------------------------------------------------------------------------

#[test]
fn interface_get_accessor_signature_accepts_a_this_parameter() {
    assert_codes(
        "interface Ia10 { get ga10(this: Ia10): number; }",
        &[],
        "interface getter with a lone `this` parameter",
    );
}

#[test]
fn interface_set_accessor_signature_accepts_a_this_parameter() {
    assert_codes(
        "interface Ib11 { set sb11(this: Ib11, vb11: number); }",
        &[],
        "interface setter with a leading `this` parameter",
    );
}

#[test]
fn type_literal_get_accessor_signature_accepts_a_this_parameter() {
    assert_codes(
        "type Tc12 = { get gc12(this: Tc12): number };",
        &[],
        "type-literal getter with a lone `this` parameter",
    );
}

#[test]
fn type_literal_set_accessor_signature_accepts_a_this_parameter() {
    assert_codes(
        "type Td13 = { set sd13(this: Td13, vd13: string) };",
        &[],
        "type-literal setter with a leading `this` parameter",
    );
}

#[test]
fn ambient_declaration_accessor_signature_accepts_a_this_parameter() {
    assert_codes(
        "declare const de14: { get ge14(this: unknown): number };",
        &[],
        "accessor signature inside a `declare const` type literal",
    );
}

/// The whole point of the bug: one bad accessor signature used to suppress
/// checking for its entire file by leaving a `TS1005`/`TS1131` cascade behind.
/// Unrelated members after it must parse cleanly.
#[test]
fn a_this_parameter_accessor_does_not_wreck_the_rest_of_the_member_list() {
    assert_codes(
        "interface If15 {\n  get gf15(this: If15): number;\n  mg15(xh15: number): void;\n  pi15: string;\n}",
        &[],
        "members following an accessor signature with a `this` parameter",
    );
}

// ---------------------------------------------------------------------------
// TS1054 — a `get` accessor cannot have parameters.
//
// A `this` parameter is not a value parameter, so a getter whose *only*
// parameter is `this` has correct arity and draws no TS1054 (rows above);
// `this` plus a real parameter draws it (row below), exactly as tsc does.
// ---------------------------------------------------------------------------

#[test]
fn interface_get_accessor_signature_with_a_value_parameter_reports_ts1054() {
    assert_codes(
        "interface Ij20 { get gj20(xk20: number): number; }",
        &[TS1054_GET_CANNOT_HAVE_PARAMETERS],
        "interface getter with one value parameter",
    );
}

#[test]
fn interface_get_accessor_signature_with_two_value_parameters_reports_ts1054_once() {
    assert_codes(
        "interface Il21 { get gl21(xm21: number, yn21: string): number; }",
        &[TS1054_GET_CANNOT_HAVE_PARAMETERS],
        "interface getter with two value parameters",
    );
}

#[test]
fn get_accessor_signature_with_this_plus_a_value_parameter_reports_ts1054() {
    // tsc reports TS1054 here *and* TS2784 on the `this` parameter: the `this`
    // parameter is discounted for arity but is still illegal on an accessor.
    assert_codes(
        "interface Io22 { get go22(this: Io22, xp22: number): number; }",
        &[TS1054_GET_CANNOT_HAVE_PARAMETERS],
        "interface getter with `this` plus a value parameter",
    );
}

#[test]
fn type_literal_get_accessor_signature_with_a_value_parameter_reports_ts1054() {
    assert_codes(
        "type Tq23 = { get gq23(xr23: string): string };",
        &[TS1054_GET_CANNOT_HAVE_PARAMETERS],
        "type-literal getter with one value parameter",
    );
}

/// Returns the `(start, length)` of the first diagnostic with `code`.
fn span_of(source: &str, code: u32) -> (u32, u32) {
    let (parser, _) = parse_source(source);
    let diagnostic = parser
        .get_diagnostics()
        .iter()
        .find(|d| d.code == code)
        .unwrap_or_else(|| panic!("expected code {code} for {source:?}"));
    (diagnostic.start, diagnostic.length)
}

#[test]
fn get_accessor_signature_reports_ts1054_at_the_accessor_name() {
    // tsc's `grammarErrorOnNode(accessor.name, …)` anchors at the name, not at
    // the `get` keyword and not at the offending parameter. The reported column
    // is what the anchor controls, so `start` is the assertion that matters.
    let source = "interface Is24 { get gs24(xt24: number): number; }";
    let (start, _) = span_of(source, TS1054_GET_CANNOT_HAVE_PARAMETERS);
    assert_eq!(
        start,
        source.find("gs24").expect("name present") as u32,
        "TS1054 must anchor at the accessor name in {source:?}"
    );
}

/// The signature arm and the class arm must produce the *same shape* of span for
/// TS1054, so the two cannot drift. Both read `name.end - name.pos`; the length is
/// now exactly the accessor name's width (`gy27`, 4 chars), matching tsc's
/// `grammarErrorOnNode(accessor.name, …)`. This span used to overshoot the
/// identifier by the width of the following token (`(`) because
/// `parse_property_name_impl` read `token_end()` *after* advancing past the name —
/// which not only mis-underlined but, since `compareDiagnostics` breaks ties on
/// length before code, sorted TS1054 *after* a same-position checker diagnostic
/// (e.g. TS2378) instead of before it. Pinning the exact width guards the fix.
#[test]
fn get_accessor_ts1054_span_agrees_between_the_signature_and_class_arms() {
    let (sig_start, sig_len) = span_of(
        "interface Iy27 { get gy27(xz27: number): number; }",
        TS1054_GET_CANNOT_HAVE_PARAMETERS,
    );
    let (class_start, class_len) = span_of(
        "class Ky27 { get gy27(xz27: number) { return 1 } }",
        TS1054_GET_CANNOT_HAVE_PARAMETERS,
    );
    assert_eq!(
        sig_len, class_len,
        "TS1054 span length must match between the accessor-signature and class arms"
    );
    assert_eq!(
        sig_len,
        "gy27".len() as u32,
        "TS1054 must span exactly the accessor name, matching tsc"
    );
    assert_eq!(
        sig_start - "interface Iy27 { get ".len() as u32,
        class_start - "class Ky27 { get ".len() as u32,
        "TS1054 must anchor at the same offset within the member in both arms"
    );
}

#[test]
fn empty_get_accessor_signature_is_clean() {
    assert_codes(
        "interface Iu25 { get gu25(): number; }",
        &[],
        "getter signature with an empty parameter list",
    );
}

/// `get x(,)` is a parameter-declaration error, not an arity error: tsc emits
/// TS1138 for the empty slot and no TS1054, because the parsed list is empty.
/// Reporting TS1054 *after* parsing the list rather than before is what makes
/// this row come out right.
#[test]
fn get_accessor_signature_with_an_empty_parameter_slot_does_not_report_ts1054() {
    assert!(
        !codes("interface Iv26 { get gv26(,): number; }")
            .contains(&TS1054_GET_CANNOT_HAVE_PARAMETERS),
        "an empty parameter slot is TS1138, not TS1054"
    );
}

// ---------------------------------------------------------------------------
// TS1094 — an accessor cannot have type parameters.
//
// Neither signature arm parsed a type-parameter list at all, so `get g<T>()`
// produced `TS1005: '(' expected.` and took the rest of the member list with it.
// ---------------------------------------------------------------------------

#[test]
fn get_accessor_signature_with_type_parameters_reports_ts1094() {
    assert_codes(
        "interface Iw30 { get gw30<Tx30>(): number; }",
        &[TS1094_ACCESSOR_CANNOT_HAVE_TYPE_PARAMETERS],
        "interface getter with a type parameter list",
    );
}

#[test]
fn set_accessor_signature_with_type_parameters_reports_ts1094() {
    assert_codes(
        "interface Iy31 { set sy31<Tz31>(va31: number); }",
        &[TS1094_ACCESSOR_CANNOT_HAVE_TYPE_PARAMETERS],
        "interface setter with a type parameter list",
    );
}

#[test]
fn type_literal_get_accessor_signature_with_type_parameters_reports_ts1094() {
    assert_codes(
        "type Tb32 = { get gb32<Tc32>(): number };",
        &[TS1094_ACCESSOR_CANNOT_HAVE_TYPE_PARAMETERS],
        "type-literal getter with a type parameter list",
    );
}

#[test]
fn type_parameters_on_an_accessor_signature_do_not_wreck_the_member_list() {
    assert_codes(
        "interface Id33 {\n  get gd33<Te33>(): number;\n  mf33(): void;\n}",
        &[TS1094_ACCESSOR_CANNOT_HAVE_TYPE_PARAMETERS],
        "member following a generic accessor signature",
    );
}

// ---------------------------------------------------------------------------
// TS1049 / TS1051 / TS1095 — the `set` grammar, which the signature arm carried
// none of. The class-body arm already had all three; these rows pin that the
// signature arm now agrees with it.
// ---------------------------------------------------------------------------

#[test]
fn set_accessor_signature_with_two_value_parameters_reports_ts1049() {
    assert_codes(
        "interface Ig40 { set sg40(vh40: number, wi40: number); }",
        &[TS1049_SET_MUST_HAVE_EXACTLY_ONE_PARAMETER],
        "interface setter with two value parameters",
    );
}

#[test]
fn set_accessor_signature_with_no_parameters_reports_ts1049() {
    assert_codes(
        "interface Ij41 { set sj41(); }",
        &[TS1049_SET_MUST_HAVE_EXACTLY_ONE_PARAMETER],
        "interface setter with an empty parameter list",
    );
}

#[test]
fn set_accessor_signature_with_this_plus_one_value_parameter_is_correct_arity() {
    // `this` is discounted, so this is a one-value-parameter setter: no TS1049.
    // tsc reports only TS2784 here.
    assert_codes(
        "interface Ik42 { set sk42(this: Ik42, vl42: number); }",
        &[],
        "interface setter with `this` plus one value parameter",
    );
}

#[test]
fn set_accessor_signature_with_only_a_this_parameter_is_correct_arity() {
    // tsc counts `parameters.length`, which is 1 here, so TS1049 does not fire.
    assert_codes(
        "interface Im43 { set sm43(this: Im43); }",
        &[],
        "interface setter whose only parameter is `this`",
    );
}

#[test]
fn set_accessor_signature_with_an_optional_parameter_reports_ts1051() {
    assert_codes(
        "interface In44 { set sn44(vo44?: number); }",
        &[TS1051_SET_CANNOT_HAVE_OPTIONAL_PARAMETER],
        "interface setter with an optional parameter",
    );
}

#[test]
fn set_accessor_signature_with_a_return_type_reports_ts1095() {
    assert_codes(
        "interface Ip45 { set sp45(vq45: number): void; }",
        &[TS1095_SET_CANNOT_HAVE_RETURN_TYPE],
        "interface setter with a return type annotation",
    );
}

#[test]
fn set_accessor_signature_return_type_is_suppressed_by_a_count_error() {
    // tsc's `checkGrammarAccessor` returns at the first error, so a wrong
    // parameter count suppresses the return-type and optional-parameter checks.
    assert_codes(
        "interface Ir46 { set sr46(vs46: number, wt46: number): void; }",
        &[TS1049_SET_MUST_HAVE_EXACTLY_ONE_PARAMETER],
        "count error suppresses TS1095",
    );
}

#[test]
fn set_accessor_signature_optional_parameter_is_suppressed_by_a_count_error() {
    assert_codes(
        "interface Iu47 { set su47(vv47?: number, ww47?: number); }",
        &[TS1049_SET_MUST_HAVE_EXACTLY_ONE_PARAMETER],
        "count error suppresses TS1051",
    );
}

#[test]
fn type_literal_set_accessor_signature_with_two_parameters_reports_ts1049() {
    assert_codes(
        "type Tx48 = { set sx48(vy48: string, wz48: string) };",
        &[TS1049_SET_MUST_HAVE_EXACTLY_ONE_PARAMETER],
        "type-literal setter with two value parameters",
    );
}

#[test]
fn well_formed_set_accessor_signature_is_clean() {
    assert_codes(
        "interface Ia49 { set sa49(vb49: number); }",
        &[],
        "setter signature with exactly one value parameter",
    );
}

// ---------------------------------------------------------------------------
// The sibling type-member kinds already accepted `this` and must keep doing so.
// These are the negatives that distinguish "the accessor arm was broken" from
// "the member list was broken".
// ---------------------------------------------------------------------------

#[test]
fn method_call_and_construct_signatures_still_accept_a_this_parameter() {
    assert_codes(
        "interface Ic50 {\n  mc50(this: Ic50): void;\n  (this: Ic50): void;\n  new (this: Ic50): Ic50;\n}",
        &[],
        "method, call and construct signatures with a `this` parameter",
    );
}

#[test]
fn a_property_named_get_is_still_a_property() {
    // `get` and `set` are not reserved: a member literally named `get` must not
    // route into the accessor arm.
    assert_codes(
        "interface Id51 { get: number; set: string; }",
        &[],
        "members named `get` and `set`",
    );
}

#[test]
fn a_method_named_get_is_still_a_method() {
    assert_codes(
        "interface Ie52 { get(xf52: number): void; set(yg52: string): void; }",
        &[],
        "methods named `get` and `set`",
    );
}

// ---------------------------------------------------------------------------
// Class and object-literal accessors are the paths this change deliberately
// left alone apart from routing them through the shared helpers. Pinned here so
// the extraction cannot silently change them.
// ---------------------------------------------------------------------------

#[test]
fn class_set_accessor_grammar_is_unchanged_by_the_shared_helpers() {
    assert_codes(
        "class Kh60 { set sh60(vi60: number, wj60: number) {} }",
        &[TS1049_SET_MUST_HAVE_EXACTLY_ONE_PARAMETER],
        "class setter count error",
    );
    assert_codes(
        "class Kk61 { set sk61(vl61?: number) {} }",
        &[TS1051_SET_CANNOT_HAVE_OPTIONAL_PARAMETER],
        "class setter optional parameter",
    );
    assert_codes(
        "class Km62 { set sm62(vn62: number): void {} }",
        &[TS1095_SET_CANNOT_HAVE_RETURN_TYPE],
        "class setter return type",
    );
    assert_codes(
        "class Ko63 { set so63(this: Ko63, vp63: number) {} }",
        &[],
        "class setter with `this` plus one value parameter",
    );
}

#[test]
fn object_literal_set_accessor_grammar_is_unchanged_by_the_shared_helpers() {
    assert_codes(
        "const oq64 = { set sq64(vr64: number, ws64: number) {} };",
        &[TS1049_SET_MUST_HAVE_EXACTLY_ONE_PARAMETER],
        "object-literal setter count error",
    );
    assert_codes(
        "const ot65 = { set st65(vu65?: number) {} };",
        &[TS1051_SET_CANNOT_HAVE_OPTIONAL_PARAMETER],
        "object-literal setter optional parameter",
    );
}
