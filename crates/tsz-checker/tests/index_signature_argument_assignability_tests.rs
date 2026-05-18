//! Locks tsc-parity for TS2345 argument assignability when the source type
//! carries an index signature and the target is a concrete object type.
//!
//! Rule: `{ [k: string]: V }` is NOT assignable to a concrete object type
//! `{ a: A; b: B }` unless `V` is assignable to every required property type
//! AND the concrete type itself has a compatible index signature.
//!
//! Guards the regression class from issue #7638 where PR #7607's bivariant
//! routing change inadvertently suppressed these diagnostics. The bivariant
//! callback sections ensure any future routing refactor is caught immediately.
//!
//! Related: conformance `computedPropertyBindingElementDeclarationNoCrash1.ts`.

use tsz_checker::test_utils::check_source_code_messages as check;
use tsz_common::diagnostics::diagnostic_codes;

fn assert_has(code: u32, source: &str) {
    let diags = check(source);
    assert!(
        diags.iter().any(|(c, _)| *c == code),
        "Expected TS{code}. Got: {diags:?}"
    );
}

fn assert_none(code: u32, source: &str) {
    let diags = check(source);
    assert!(
        diags.iter().all(|(c, _)| *c != code),
        "Expected no TS{code}. Got: {diags:?}"
    );
}

const TS2345: u32 = diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE;
const TS2322: u32 = diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE;

#[test]
fn string_index_unknown_to_concrete_type_is_ts2345() {
    assert_has(
        TS2345,
        r#"
type State = { a: number; b: string };
declare function processState(state: State): void;
declare const data: { [x: string]: unknown };
processState(data);
"#,
    );
}

// Destructuring does not change the source variable's type; TS2345 still fires.
#[test]
fn computed_key_destructuring_then_pass_source_is_ts2345() {
    assert_has(
        TS2345,
        r#"
type State = { a: number; b: string };
declare function processState(state: State): void;
declare const key: string;
declare const data: { [x: string]: unknown };
const { [key]: _val } = data;
processState(data);
"#,
    );
}

#[test]
fn string_literal_computed_key_destructure_then_pass_source_is_ts2345() {
    assert_has(
        TS2345,
        r#"
type State = { a: number; b: string };
declare function processState(state: State): void;
declare const data: { [x: string]: unknown };
const { ['b']: _val } = data;
processState(data);
"#,
    );
}

#[test]
fn string_index_unknown_to_string_index_number_is_ts2345() {
    assert_has(
        TS2345,
        r#"
type NumberMap = { [x: string]: number };
declare function processNumberMap(m: NumberMap): void;
declare const data: { [x: string]: unknown };
processNumberMap(data);
"#,
    );
}

#[test]
fn number_index_incompatible_value_to_string_index_is_ts2345() {
    assert_has(
        TS2345,
        r#"
type StringMap = { [x: string]: string };
declare function processStringMap(m: StringMap): void;
declare const data: { [n: number]: number };
processStringMap(data);
"#,
    );
}

// Guards against the weak_union_violation gate suppressing method errors (#7607).
#[test]
fn bivariant_method_incompatible_param_type_is_ts2322() {
    assert_has(
        TS2322,
        r#"
interface Handler {
    handle(state: { required: number }): void;
}
const bad: Handler = {
    handle(s: { unrelated: string }) {}
};
"#,
    );
}

#[test]
fn bivariant_method_incompatible_renamed_interface_is_ts2322() {
    assert_has(
        TS2322,
        r#"
interface Processor {
    process(input: { value: number }): void;
}
const bad: Processor = {
    process(x: { other: string }) {}
};
"#,
    );
}

#[test]
fn generic_omit_key_function_no_error() {
    assert_none(
        TS2322,
        r#"
type Omit<T, K extends keyof any> = Pick<T, Exclude<keyof T, K>>;
type Pick<T, K extends keyof T> = { [P in K]: T[P]; };
type Exclude<T, U> = T extends U ? never : T;

function omitKey<T extends object, K extends keyof T>(obj: T, key: K): Omit<T, K> {
    const { [key]: _, ...rest } = obj;
    return rest;
}
"#,
    );
}

// Alternate type-parameter names prove the fix is structural (CLAUDE.md §25).
#[test]
fn generic_omit_key_alternate_param_names_no_error() {
    assert_none(
        TS2322,
        r#"
type Omit<T, K extends keyof any> = Pick<T, Exclude<keyof T, K>>;
type Pick<T, K extends keyof T> = { [P in K]: T[P]; };
type Exclude<T, U> = T extends U ? never : T;

function removeKey<Source extends object, Prop extends keyof Source>(
    source: Source,
    prop: Prop,
): Omit<Source, Prop> {
    const { [prop]: _, ...remainder } = source;
    return remainder;
}
"#,
    );
}

#[test]
fn number_index_compatible_value_to_string_index_no_ts2345() {
    assert_none(
        TS2345,
        r#"
type StringMap = { [x: string]: string };
declare function processStringMap(m: StringMap): void;
declare const data: { [n: number]: string };
processStringMap(data);
"#,
    );
}

#[test]
fn compatible_index_signatures_no_ts2345() {
    assert_none(
        TS2345,
        r#"
type NumberMap = { [x: string]: number };
declare function processNumberMap(m: NumberMap): void;
declare const data: { [x: string]: number };
processNumberMap(data);
"#,
    );
}

#[test]
fn any_source_to_concrete_type_no_ts2345() {
    assert_none(
        TS2345,
        r#"
type State = { a: number; b: string };
declare function processState(state: State): void;
declare const data: any;
processState(data);
"#,
    );
}

#[test]
fn concrete_subtypes_no_ts2345() {
    assert_none(
        TS2345,
        r#"
type State = { a: number; b: string };
declare function processState(state: State): void;
declare const data: { a: number; b: string; c: boolean };
processState(data);
"#,
    );
}

#[test]
fn bivariant_method_one_direction_assignable_no_error() {
    assert_none(
        TS2322,
        r#"
interface Handler {
    handle(state: { a: number }): void;
}
const ok: Handler = {
    handle(s: { a: number; b: string }) {}
};
"#,
    );
}
