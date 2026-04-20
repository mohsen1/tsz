#[test]
fn mapped_type_generic_indexed_access_full_file_has_no_ts2344_or_ts7006() {
    let source = r#"
type Types = {
    first: { a1: true };
    second: { a2: true };
    third: { a3: true };
};

class Test {
    entries: { [T in keyof Types]?: Types[T][] };

    constructor() {
        this.entries = {};
    }

    addEntry<T extends keyof Types>(name: T, entry: Types[T]) {
        if (!this.entries[name]) {
            this.entries[name] = [];
        }
        this.entries[name]?.push(entry);
    }
}

type TypesMap = {
    [0]: { foo: 'bar' };
    [1]: { a: 'b' };
};

type P<T extends keyof TypesMap> = { t: T } & TypesMap[T];

type TypeHandlers = {
    [T in keyof TypesMap]?: (p: P<T>) => void;
};

const typeHandlers: TypeHandlers = {
    [0]: (p) => p.foo,
    [1]: (p) => p.a,
};

const onSomeEvent = <T extends keyof TypesMap>(p: P<T>) =>
    typeHandlers[p.t]?.(p);
"#;

    let diagnostics = compile_with_libs_for_ts(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            no_implicit_any: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2344),
        "full mapped-type generic indexed-access repro should not emit TS2344, got: {diagnostics:?}"
    );
    assert!(
        !diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "full mapped-type generic indexed-access repro should not emit TS7006, got: {diagnostics:?}"
    );
}

#[test]
fn mapped_type_recursive_inference_generic_call_preserves_nested_callback_context() {
    let source = r#"
type MorphTuple = [string, "|>", any];

type validateMorph<def extends MorphTuple> = def[1] extends "|>"
    ? [validateDefinition<def[0]>, "|>", (In: def[0]) => unknown]
    : def;

type validateDefinition<def> = def extends MorphTuple
    ? validateMorph<def>
    : {
          [k in keyof def]: validateDefinition<def[k]>
      };

declare function type<def>(def: validateDefinition<def>): def;

const shallow = type(["ark", "|>", (x) => x.length]);
const objectLiteral = type({ a: ["ark", "|>", (x) => x.length] });
const nestedTuple = type([["ark", "|>", (x) => x.length]]);
"#;

    let diagnostics = compile_with_libs_for_ts(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            no_implicit_any: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "recursive mapped/conditional generic call should contextually type nested callbacks, got: {diagnostics:?}"
    );
}

#[test]
fn union_of_overloaded_array_method_aliases_preserves_callback_context() {
    let source = r#"
interface Fizz { id: number; fizz: string }
interface Buzz { id: number; buzz: string }
interface Arr<T> {
  filter<S extends T>(pred: (value: T) => value is S): S[];
  filter(pred: (value: T) => unknown): T[];
}
declare const m: Arr<Fizz>["filter"] | Arr<Buzz>["filter"];
m(item => item.id < 5);
"#;

    let diagnostics = compile_with_libs_for_ts(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            no_implicit_any: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "union of overloaded array method aliases should contextually type callback params, got: {diagnostics:?}"
    );
}

#[test]
fn union_of_builtin_array_methods_preserves_callback_context() {
    let source = r#"
interface Fizz { id: number; fizz: string }
interface Buzz { id: number; buzz: string }

([] as Fizz[] | Buzz[]).filter(item => item.id < 5);
([] as Fizz[] | readonly Buzz[]).filter(item => item.id < 5);
([] as Fizz[] | Buzz[]).find(item => item);
([] as Fizz[] | Buzz[]).every(item => item.id < 5);
"#;

    let diagnostics = compile_with_libs_for_ts(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            no_implicit_any: true,
            strict_null_checks: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "union of built-in array methods should contextually type callback params, got: {diagnostics:?}"
    );
}
// =============================================================================
// Assignment Expression Tests (TS2322)
// =============================================================================

#[test]
fn test_ts2322_assignment_wrong_primitive() {
    let source = r#"
        let a: number;
        a = "string";
    "#;

    assert!(has_error_with_code(
        source,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
    ));
}

#[test]
fn test_ts2322_assignment_wrong_object_property() {
    let source = r#"
        let obj: { a: number };
        obj = { a: "string" };
    "#;

    assert!(has_error_with_code(
        source,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
    ));
}

// =============================================================================
// Multiple TS2322 Errors
// =============================================================================

#[test]
fn test_ts2322_multiple_errors() {
    let source = r#"
        function f1(): number {
            return "string";
        }
        function f2(): string {
            return 42;
        }
        let x: number = "x";
        let y: string = 123;
    "#;

    let count = count_errors_with_code(source, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert!(count >= 4, "Expected at least 4 TS2322 errors, got {count}");
}

#[test]
fn test_ts2322_distinct_type_parameters_are_not_suppressed() {
    let source = r#"
        function unconstrained<T, U>(t: T, u: U) {
            t = u;
            u = t;
        }

        function constrained<T extends { foo: string }, U extends { foo: string }>(t: T, u: U) {
            t = u;
            u = t;
        }

        class Box<T extends { foo: string }, U extends { foo: string }> {
            t!: T;
            u!: U;

            assign() {
                this.t = this.u;
                this.u = this.t;
            }
        }
    "#;

    let count = count_errors_with_code(source, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert_eq!(
        count, 6,
        "Expected TS2322 for each distinct type-parameter assignment, got {count}"
    );
}

// =============================================================================
// No Error Tests (Verify we don't emit false positives)
// =============================================================================

#[test]
fn test_ts2322_no_error_correct_types() {
    let source = r#"
        function returnNumber(): number {
            return 42;
        }
        let x: number = 42;
        let y: { a: number } = { a: 42 };
        let z: string[] = ["a", "b"];
        let a: number;
        a = 42;
    "#;

    assert!(!has_error_with_code(
        source,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
    ));
}

#[test]
fn test_ts2322_generic_object_literal_call_property_anchor_and_message() {
    let source = r#"
function foo<T>(x: { bar: T; baz: T }) {
    return x;
}
var r = foo<number>({ bar: 1, baz: '' });
"#;

    let diagnostics = diagnostics_for_source(source);
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    let has_ts2345 = diagnostics.iter().any(|d| {
        d.code == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
    });

    assert_eq!(
        errors.len(),
        1,
        "Expected exactly one TS2322 diagnostic, got: {errors:?}"
    );
    let diag = errors[0];
    let expected_messages = [
        "Type 'string' is not assignable to type 'number'.",
        "Type 'number' is not assignable to type 'string'.",
    ];
    assert!(
        expected_messages.contains(&diag.message_text.as_str()),
        "Unexpected TS2322 message: {}",
        diag.message_text
    );
    assert!(
        !has_ts2345,
        "Did not expect outer TS2345 once property-level TS2322 elaboration applies, got: {diagnostics:?}"
    );

    let expected_baz_start = source
        .find("baz: ''")
        .expect("expected test snippet to contain baz property");
    let expected_bar_start = source
        .find("bar: 1")
        .expect("expected test snippet to contain bar property");
    let expected_object_start = source
        .find("{ bar: 1, baz: '' }")
        .expect("expected test snippet to contain object literal");
    assert!(
        diag.start == expected_baz_start as u32
            || diag.start == expected_bar_start as u32
            || diag.start == expected_object_start as u32,
        "Expected TS2322 on baz/bar/object literal node, got start {}",
        diag.start
    );
}

