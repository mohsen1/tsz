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

#[test]
fn unknown_equality_narrows_to_enum_objects_and_members() {
    let source = r#"
type isUnknown<T> = unknown extends T ? true : false;
type isTrue<T extends true> = T;

const u: unknown = undefined;

enum NumberEnum {
    A,
    B,
    C
}

enum StringEnum {
    A = "A",
    B = "B",
    C = "C"
}

if (u === NumberEnum || u === StringEnum) {
    let enumObj: object = u;
}

if (u === NumberEnum.A) {
    let a: NumberEnum.A = u
}

if (u === StringEnum.B) {
    let b: StringEnum.B = u
}

function switchTestEnum(x: unknown) {
    switch (x) {
        case StringEnum.A:
            const a: StringEnum.A = x;
            break;
        case StringEnum.B:
            const b: StringEnum.B = x;
            break;
        case StringEnum.C:
            const c: StringEnum.C = x;
            break;
    }
    type End = isTrue<isUnknown<typeof x>>
}

function switchTestCollectEnum(x: unknown) {
    switch (x) {
        case StringEnum.A:
            const a: StringEnum.A = x;
        case StringEnum.B:
            const b: StringEnum.A | StringEnum.B = x;
        case StringEnum.C:
            const c: StringEnum.A | StringEnum.B | StringEnum.C = x;
            const all: StringEnum = x;
            return;
    }
    type End = isTrue<isUnknown<typeof x>>
}
"#;
    let diagnostics = diag_messages(source);
    assert!(
        diagnostics.is_empty(),
        "Expected enum object/member equality to narrow unknown without diagnostics, got: {diagnostics:#?}"
    );
}

#[test]
fn unknown_negated_enum_equality_narrows_else_branch() {
    let source = r#"
const u: unknown = undefined;

enum NumberEnum {
    A,
    B,
    C
}

enum StringEnum {
    A = "A",
    B = "B",
    C = "C"
}

function notNotEquals(u: unknown)  {
    if (u !== NumberEnum) { }
    else {
        const o: object = u;
    }

    if (u !== NumberEnum.A) { }
    else {
        const a: NumberEnum.A = u;
    }

    if (u !== NumberEnum.A && u !== NumberEnum.B && u !== StringEnum.A) { }
    else {
        const aOrB: NumberEnum.A | NumberEnum.B | StringEnum.A  = u;
    }

    if (!(u === NumberEnum.A || u === NumberEnum.B || u === StringEnum.A)) { }
    else {
        const aOrB: NumberEnum.A | NumberEnum.B | StringEnum.A  = u;
    }
}
"#;
    let diagnostics = diag_messages(source);
    assert!(
        diagnostics.is_empty(),
        "Expected negated enum equality to narrow unknown in else branches, got: {diagnostics:#?}"
    );
}
