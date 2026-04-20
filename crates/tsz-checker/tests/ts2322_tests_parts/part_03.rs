#[test]
fn generic_object_assign_initializer_keeps_outer_ts2322() {
    let source = r#"
type Omit<T, K> = Pick<T, Exclude<keyof T, K>>;
type Assign<T, U> = Omit<T, keyof U> & U;

class Base<T> {
    constructor(public t: T) {}
}

export class Foo<T> extends Base<T> {
    update(): Foo<Assign<T, { x: number }>> {
        const v: Assign<T, { x: number }> = Object.assign(this.t, { x: 1 });
        return new Foo(v);
    }
}
"#;

    let diagnostics = compile_with_libs_for_ts(
        source,
        "test.ts",
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    let codes: Vec<_> = diagnostics.iter().map(|(code, _)| *code).collect();
    assert!(
        codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected outer TS2322 for generic Object.assign initializer, got: {diagnostics:?}"
    );
    assert!(
        codes.contains(&diagnostic_codes::NO_OVERLOAD_MATCHES_THIS_CALL),
        "Expected initializer TS2769 for generic Object.assign initializer, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_string_intrinsic_targets_widen_literal_sources() {
    let source = r#"
let x: Uppercase<string>;
x = "AbC";

let y: Lowercase<string>;
y = "AbC";
"#;

    let diagnostics = diagnostics_for_source(source);
    let messages: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .map(|d| d.message_text.as_str())
        .collect();

    assert!(
        messages.contains(&"Type 'string' is not assignable to type 'Uppercase<string>'."),
        "Expected widened source diagnostic for Uppercase<string>, got: {messages:?}"
    );
    assert!(
        messages.contains(&"Type 'string' is not assignable to type 'Lowercase<string>'."),
        "Expected widened source diagnostic for Lowercase<string>, got: {messages:?}"
    );
    assert!(
        !messages.iter().any(|message| message.contains("\"AbC\"")),
        "String intrinsic diagnostics should widen the source literal, got: {messages:?}"
    );
}

// =============================================================================
// User-Defined Generic Type Application Tests (TS2322 False Positives)
// These test the root cause of 11,000+ extra TS2322 errors
// =============================================================================

#[test]
fn test_ts2322_no_false_positive_simple_generic_identity() {
    // type Id<T> = T; let a: Id<number> = 42;
    let source = r"
        type Id<T> = T;
        let a: Id<number> = 42;
    ";

    let errors = get_all_diagnostics(source);
    let ts2322_errors: Vec<_> = errors
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    assert!(
        ts2322_errors.is_empty(),
        "Expected no TS2322 for Id<number> = 42, got: {ts2322_errors:?}"
    );
}

#[test]
fn test_ts2322_no_false_positive_generic_object_wrapper() {
    // type Box<T> = { value: T }; let b: Box<number> = { value: 42 };
    let source = r"
        type Box<T> = { value: T };
        let b: Box<number> = { value: 42 };
    ";

    let errors = get_all_diagnostics(source);
    let ts2322_errors: Vec<_> = errors
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    assert!(
        ts2322_errors.is_empty(),
        "Expected no TS2322 for Box<number> = {{ value: 42 }}, got: {ts2322_errors:?}"
    );
}

#[test]
fn test_ts2322_no_false_positive_conditional_type_true_branch() {
    // IsStr<string> should evaluate to 'true', and true is assignable to true
    let source = r"
        type IsStr<T> = T extends string ? true : false;
        let a: IsStr<string> = true;
    ";

    let errors = get_all_diagnostics(source);
    let ts2322_errors: Vec<_> = errors
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    assert!(
        ts2322_errors.is_empty(),
        "Expected no TS2322 for IsStr<string> = true, got: {ts2322_errors:?}"
    );
}

#[test]
fn test_ts2322_no_false_positive_conditional_type_false_branch() {
    // IsStr<number> should evaluate to 'false', and false is assignable to false
    let source = r"
        type IsStr<T> = T extends string ? true : false;
        let b: IsStr<number> = false;
    ";

    let errors = get_all_diagnostics(source);
    let ts2322_errors: Vec<_> = errors
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    assert!(
        ts2322_errors.is_empty(),
        "Expected no TS2322 for IsStr<number> = false, got: {ts2322_errors:?}"
    );
}

#[test]
fn test_ts2322_no_false_positive_user_defined_mapped_type() {
    // MyPartial<Cfg> should behave like Partial<Cfg>
    let source = r#"
        type MyPartial<T> = { [K in keyof T]?: T[K] };
        interface Cfg { host: string; port: number }
        let a: MyPartial<Cfg> = {};
        let b: MyPartial<Cfg> = { host: "x" };
    "#;

    let errors = get_all_diagnostics(source);
    let ts2322_errors: Vec<_> = errors
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    assert!(
        ts2322_errors.is_empty(),
        "Expected no TS2322 for MyPartial<Cfg>, got: {ts2322_errors:?}"
    );
}

#[test]
fn test_ts2322_no_false_positive_conditional_infer() {
    // UnpackPromise<Promise<number>> should evaluate to number
    let source = r"
        type UnpackPromise<T> = T extends Promise<infer U> ? U : T;
        let a: UnpackPromise<Promise<number>> = 42;
    ";

    let errors = get_all_diagnostics(source);
    let ts2322_errors: Vec<_> = errors
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    assert!(
        ts2322_errors.is_empty(),
        "Expected no TS2322 for UnpackPromise<Promise<number>> = 42, got: {ts2322_errors:?}"
    );
}

#[test]
fn test_ts2322_conditional_doesnt_leak_uninstantiated_type_parameter() {
    // SyntheticDestination<number, Synthetic<number, number>> should resolve to number, not T
    let source = r#"
        interface Synthetic<A, B extends A> {}
        type SyntheticDestination<T, U> = U extends Synthetic<T, infer V> ? V : never;
        type TestSynthetic = SyntheticDestination<number, Synthetic<number, number>>;
        const y: TestSynthetic = 3;
        const z: TestSynthetic = '3';
    "#;

    let errors = get_all_diagnostics(source);
    // Debug: All diagnostics: {errors:?}
    let _ = &errors;

    // y = 3 should NOT error (number is assignable to number)
    // z = '3' SHOULD error (string is not assignable to number)
    let ts2322_errors: Vec<_> = errors
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    assert_eq!(
        ts2322_errors.len(),
        1,
        "Expected exactly 1 TS2322 for string->number mismatch, got: {ts2322_errors:?}"
    );
    assert!(
        ts2322_errors[0].1.contains("not assignable"),
        "Expected assignability error, got: {:?}",
        ts2322_errors[0].1
    );
}

#[test]
fn test_ts2322_no_false_positive_conditional_expression_with_generics() {
    // Conditional expressions should compute union type first, not check branches individually
    // This tests the fix for premature assignability checking in conditional expressions
    let source = r#"
        interface Shape {
            name: string;
            width: number;
            height: number;
        }

        function getProperty<T, K extends keyof T>(obj: T, key: K): T[K] {
            return obj[key];
        }

        function test(shape: Shape, cond: boolean) {
            // cond ? "width" : "height" should be type "width" | "height"
            // which IS assignable to K extends keyof Shape
            // Should NOT emit TS2322 on individual branches
            let widthOrHeight = getProperty(shape, cond ? "width" : "height");
        }
    "#;

    let errors = get_all_diagnostics(source);
    let ts2322_errors: Vec<_> = errors
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    assert!(
        ts2322_errors.is_empty(),
        "Expected no TS2322 for conditional expression in generic function call, got: {ts2322_errors:?}"
    );
}

