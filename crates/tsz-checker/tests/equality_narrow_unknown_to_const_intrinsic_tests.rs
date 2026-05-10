//! Equality narrowing of `unknown` against a const variable typed as a
//! primitive intrinsic (e.g. `declare const aString: string`).
//!
//! tsc treats `if (u === aString)` as a guard that narrows `u: unknown` to
//! the right operand's declared primitive type. The const's annotation
//! resolves to a `TypeId::STRING` etc. intrinsic, which `is_narrowing_literal`
//! must accept as a valid comparand.

use tsz_checker::context::CheckerOptions;
use tsz_common::checker_options::JsxMode;

fn diag_codes(source: &str) -> Vec<u32> {
    let opts = CheckerOptions {
        jsx_mode: JsxMode::Preserve,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    tsz_checker::test_utils::check_source(source, "test.ts", opts)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn diag_messages(source: &str) -> Vec<(u32, String)> {
    let opts = CheckerOptions {
        jsx_mode: JsxMode::Preserve,
        strict: true,
        ..CheckerOptions::default()
    }
    .apply_strict_defaults();
    tsz_checker::test_utils::check_source(source, "test.ts", opts)
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

#[test]
fn unknown_equality_narrows_to_const_string_annotation() {
    let source = r#"
declare const u: unknown;
declare const aString: string;
if (u === aString) {
    let s: string = u;
}
"#;
    let codes = diag_codes(source);
    assert!(
        !codes.contains(&2322),
        "Expected no TS2322 — narrowing should produce string, got: {codes:?}"
    );
}

#[test]
fn unknown_equality_narrows_to_const_number_annotation() {
    let source = r#"
declare const u: unknown;
declare const aNumber: number;
if (u === aNumber) {
    let n: number = u;
}
"#;
    let codes = diag_codes(source);
    assert!(
        !codes.contains(&2322),
        "Expected no TS2322 for number-annotated const equality narrowing, got: {codes:?}"
    );
}

#[test]
fn unknown_equality_narrows_to_const_boolean_annotation() {
    let source = r#"
declare const u: unknown;
declare const aBoolean: boolean;
if (u === aBoolean) {
    let b: boolean = u;
}
"#;
    let codes = diag_codes(source);
    assert!(
        !codes.contains(&2322),
        "Expected no TS2322 for boolean-annotated const equality narrowing, got: {codes:?}"
    );
}

#[test]
fn unknown_equality_param_name_independent() {
    // Locks the rule is purely structural — using a different const name
    // keeps the same narrowing behaviour.
    let source = r#"
declare const u: unknown;
declare const aDifferentName: string;
if (u === aDifferentName) {
    let s: string = u;
}
"#;
    let codes = diag_codes(source);
    assert!(
        !codes.contains(&2322),
        "Expected no TS2322 regardless of const name choice, got: {codes:?}"
    );
}

/// Regression: primitive-intrinsic comparands MUST NOT narrow union sources
/// in the false branch. Mirrors tsc behaviour — `string` is not a unit type,
/// so `x !== y` (where `y: string`) leaves `x: string | number` unchanged.
#[test]
fn primitive_const_inequality_does_not_exclude_from_union_false_branch() {
    let source = r#"
declare const y: string;
function f(x: string | number) {
    if (x !== y) {
        let n: number = x;
    }
}
"#;
    let codes = diag_codes(source);
    assert!(
        codes.contains(&2322),
        "Expected TS2322 — `x !== y` must not narrow `x: string | number` to `number` in else branch (string is not a unit type), got: {codes:?}"
    );
}

#[test]
fn union_equality_with_number_const_does_not_narrow_false_branch() {
    let source = r#"
declare const n: number;
function f(x: string | number) {
    if (x === n) {
        // true branch: narrows to number — OK
    } else {
        let s: string = x;
    }
}
"#;
    let codes = diag_codes(source);
    assert!(
        codes.contains(&2322),
        "Expected TS2322 — false branch of `x === n` (n: number) must leave `x: string | number` unchanged, got: {codes:?}"
    );
}

#[test]
fn explicit_unknown_flow_cache_does_not_poison_primitive_const_narrowing() {
    let source = r#"
declare const u: unknown;
declare const aString: string;
declare const aUnion: { x: number } | { y: string };

if (u === aString) {
    let s: string = u;
}

if (u === aUnion) {
    type StillUnknown = unknown extends typeof u ? true : false;
}
"#;
    let codes = diag_codes(source);
    assert!(
        !codes.contains(&2322),
        "Expected no TS2322 - later broad unknown equality must not poison earlier primitive narrowing, got: {codes:?}"
    );
}

#[test]
fn unknown_equality_narrows_to_object_literal_annotation_with_later_union_comparand() {
    let source = r#"
declare const u: unknown;
declare const anObjectLiteral: { x: number };
declare const aUnion: { x: number } | { y: string };

if (u === anObjectLiteral) {
    let o: object = u;
}

if (u === aUnion) {
    type StillUnknown = unknown extends typeof u ? true : false;
}
"#;
    let codes = diag_codes(source);
    assert!(
        !codes.contains(&2322),
        "Expected no TS2322 - object-literal annotation should narrow unknown to object even with a later union comparand, got: {codes:?}"
    );
}

#[test]
fn unknown_type2_object_literal_repro_with_initialized_unknown() {
    let source = r#"
const u: unknown = undefined;

declare const anObjectLiteral: { x: number };
declare const aUnion: { x: number } | { y: string };

if (u === anObjectLiteral) {
    let uObjectLiteral: object = u;
}

if (u === aUnion) {
    type StillUnknown = unknown extends typeof u ? true : false;
}
"#;
    let codes = diag_codes(source);
    assert!(
        !codes.contains(&2322),
        "Expected no TS2322 for the unknownType2 object-literal repro, got: {codes:?}"
    );
}

#[test]
fn unknown_switch_cases_narrow_symbol_and_objectish_comparisons() {
    let source = r#"
declare const symb: unique symbol;

function symbolCase(x: unknown) {
    switch (x) {
        case symb:
            const s: typeof symb = x;
            break;
    }
}

function objectCases(x: unknown, fn: () => void, obj: { prop: number }) {
    switch (x) {
        case fn:
            const f: object = x;
            break;
        case obj:
            const o: object = x;
            break;
    }
}
"#;
    let codes = diag_codes(source);
    assert!(
        !codes.contains(&2322),
        "Expected no TS2322 - switch case identifiers should narrow unknown to symbol/objectish comparands, got: {codes:?}"
    );
}

#[test]
fn unknown_switch_return_mismatch_displays_narrowed_case_union() {
    let source = r#"
type SomeResponse = 'yes' | 'no' | 'idk';

function switchResponseWrong(x: unknown): SomeResponse {
    switch (x) {
        case 'yes':
        case 'no':
        case 'maybe':
            return x;
        default:
            throw '';
    }
}
"#;
    let diagnostics = diag_messages(source);
    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2322
                && message
                    == "Type '\"maybe\" | \"yes\" | \"no\"' is not assignable to type 'SomeResponse'."
        }),
        "Expected TS2322 to display the narrowed switch-case union, got: {diagnostics:#?}"
    );
}
