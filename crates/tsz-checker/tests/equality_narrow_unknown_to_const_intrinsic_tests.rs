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

// Loose equality (==) must NOT narrow unknown/any to non-nullish literals.
// tsc does not treat `u == 42` or `u == true` as a narrowing guard for unknown.
// Only strict equality (===) narrows; loose equality only narrows for null/undefined.

#[test]
fn loose_equality_does_not_narrow_unknown_to_number_literal() {
    // `u == 42` should NOT narrow u: unknown to 42.
    // Verified against: two names (42/100), both must not narrow.
    let source = r#"
type isUnknown<T> = unknown extends T ? true : false;
type isTrue<T extends true> = T;
declare const u: unknown;
if (u == 42) {
    type A = isTrue<isUnknown<typeof u>>;
}
if (u == 100) {
    type B = isTrue<isUnknown<typeof u>>;
}
"#;
    let codes = diag_codes(source);
    assert!(
        !codes.contains(&2344),
        "Loose == with number literal must not narrow unknown — got TS2344: {codes:?}"
    );
}

#[test]
fn loose_equality_does_not_narrow_unknown_to_boolean_literal() {
    // `u == true` and `u == false` must NOT narrow u: unknown.
    let source = r#"
type isUnknown<T> = unknown extends T ? true : false;
type isTrue<T extends true> = T;
declare const u: unknown;
if (u == true) {
    type A = isTrue<isUnknown<typeof u>>;
}
if (u == false) {
    type B = isTrue<isUnknown<typeof u>>;
}
"#;
    let codes = diag_codes(source);
    assert!(
        !codes.contains(&2344),
        "Loose == with boolean literal must not narrow unknown — got TS2344: {codes:?}"
    );
}

#[test]
fn loose_equality_does_not_narrow_unknown_to_string_literal() {
    // `u == "hello"` must NOT narrow u: unknown to "hello".
    let source = r#"
type isUnknown<T> = unknown extends T ? true : false;
type isTrue<T extends true> = T;
declare const u: unknown;
if (u == "hello") {
    type A = isTrue<isUnknown<typeof u>>;
}
if (u == "world") {
    type B = isTrue<isUnknown<typeof u>>;
}
"#;
    let codes = diag_codes(source);
    assert!(
        !codes.contains(&2344),
        "Loose == with string literal must not narrow unknown — got TS2344: {codes:?}"
    );
}

#[test]
fn strict_equality_still_narrows_unknown_to_literal() {
    // `u === 42` SHOULD narrow u: unknown (strict equality).
    // This ensures we didn't accidentally disable strict narrowing.
    let source = r#"
declare const u: unknown;
if (u === 42) {
    const n: 42 = u;
}
"#;
    let codes = diag_codes(source);
    assert!(
        !codes.contains(&2322),
        "Strict === with number literal MUST narrow unknown — got TS2322: {codes:?}"
    );
}

#[test]
fn loose_equality_still_narrows_unknown_to_null_undefined() {
    // `u == null` SHOULD still narrow u: unknown to null | undefined (NullishEquality).
    let source = r#"
declare const u: unknown;
if (u == null) {
    const x: null | undefined = u;
}
"#;
    let codes = diag_codes(source);
    assert!(
        !codes.contains(&2322),
        "Loose == null must still narrow unknown to null|undefined — got TS2322: {codes:?}"
    );
}

// Switch end-label must not include per-clause flows from fallthrough cases.
// When all switch cases return/throw, post-switch type must remain the declared type.

#[test]
fn switch_post_label_does_not_include_empty_fallthrough_clause_flows() {
    // All cases return or throw, so post-switch is unreachable.
    // The type at the post-switch position must still be `unknown` (declared type),
    // not narrowed to the union of the fallthrough case labels.
    let source = r#"
type isUnknown<T> = unknown extends T ? true : false;
type isTrue<T extends true> = T;
type SomeResponse = 'yes' | 'no' | 'idk';
function switchResponse(x: unknown): SomeResponse {
    switch (x) {
        case 'yes':
        case 'no':
        case 'idk':
            return x;
        default:
            throw new Error('unknown response');
    }
    type End = isTrue<isUnknown<typeof x>>;
}
"#;
    let codes = diag_codes(source);
    assert!(
        !codes.contains(&2344),
        "Post-switch type must be unknown (not narrowed by fallthrough clauses) — got TS2344: {codes:?}"
    );
}

#[test]
fn switch_post_label_does_not_include_empty_fallthrough_clause_flows_renamed_param() {
    // Same test with renamed type parameter to verify structural rule, not spelling.
    let source = r#"
type isUnknownCheck<V> = unknown extends V ? true : false;
type isTrue<T extends true> = T;
type SomeResponse = 'yes' | 'no' | 'idk';
function switchResponse(value: unknown): SomeResponse {
    switch (value) {
        case 'yes':
        case 'no':
        case 'idk':
            return value;
        default:
            throw new Error('unknown response');
    }
    type End = isTrue<isUnknownCheck<typeof value>>;
}
"#;
    let codes = diag_codes(source);
    assert!(
        !codes.contains(&2344),
        "Post-switch type must be unknown (not narrowed by fallthrough clauses) — got TS2344: {codes:?}"
    );
}
