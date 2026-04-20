#[test]
fn mapped_source_generic_call_reports_ts2345() {
    let source = r#"
type A = "number" | "null" | A[];

type F<T> = null extends T
    ? [F<NonNullable<T>>, "null"]
    : T extends number
    ? "number"
    : never;

type G<T> = { [k in keyof T]: F<T[k]> };

interface K {
    b: number | null;
}

const gK: { [key in keyof K]: A } = { b: ["number", "null"] };

function foo<T>(g: G<T>): T {
    return {} as any;
}

foo(gK);
"#;

    assert!(
        has_error_with_code(
            source,
            diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
        ),
        "mapped source generic call should preserve concrete keys and report TS2345"
    );
}

#[test]
fn generic_function_identifier_argument_still_contextually_instantiates() {
    let source = r#"
declare function takesString(fn: (x: string) => string): void;
declare function id<T>(x: T): T;
takesString(id);
"#;

    let diagnostics = get_all_diagnostics(source);
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .collect();

    assert!(
        !relevant.iter().any(|(code, _)| {
            *code == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
        }),
        "generic function identifiers should still use call-argument contextual instantiation, got: {relevant:?}"
    );
}

#[test]
fn test_ts2322_generator_yield_missing_value() {
    let source = r"
        interface IterableIterator<T> {}

        function* g(): IterableIterator<number> {
            yield;
            yield 1;
        }
    ";

    assert!(has_error_with_code(
        source,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
    ));
}

#[test]
fn test_ts2322_generator_yield_wrong_type() {
    let source = r#"
        interface IterableIterator<T> {}

        function* g(): IterableIterator<number> {
            yield "x";
            yield 1;
        }
    "#;

    assert!(has_error_with_code(
        source,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
    ));
}

// =============================================================================
// Variable Declaration Tests (TS2322)
// =============================================================================

#[test]
fn test_ts2322_variable_declaration_wrong_type() {
    let source = r#"
        let x: number = "string";
    "#;

    assert!(has_error_with_code(
        source,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
    ));
}

#[test]
fn test_ts2322_variable_declaration_wrong_object_property() {
    let source = r#"
        let y: { a: number } = { a: "string" };
    "#;

    assert!(has_error_with_code(
        source,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
    ));
}

#[test]
fn test_ts2322_variable_declaration_wrong_array_element() {
    let source = r"
        let z: string[] = [1, 2, 3];
    ";

    assert!(has_error_with_code(
        source,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
    ));
}

#[test]
fn mapped_numeric_handler_context_does_not_falsely_drop_to_implicit_any() {
    let source = r#"
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
"#;

    let diagnostics = compile_with_options(
        source,
        "test.ts",
        CheckerOptions {
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
    );
    let relevant: Vec<_> = diagnostics
        .into_iter()
        .filter(|(code, _)| *code != 2318)
        .collect();

    assert!(
        !relevant
            .iter()
            .any(|(code, _)| { *code == diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE }),
        "mapped handler context should not be misclassified as a primitive-union overload case, got: {relevant:?}"
    );
}

#[test]
fn mapped_type_generic_indexed_access_no_ts2349() {
    // Repro from TypeScript#49338: element access with a generic key on a mapped
    // type should produce a callable result via solver template substitution,
    // not TS2349 "This expression is not callable".
    let source = r#"
type TypesMap = {
    [0]: { foo: 'bar' };
    [1]: { a: 'b' };
};

type P<T extends keyof TypesMap> = { t: T } & TypesMap[T];

type TypeHandlers = {
    [T in keyof TypesMap]?: (p: P<T>) => void;
};

declare const typeHandlers: TypeHandlers;
const onSomeEvent = <T extends keyof TypesMap>(p: P<T>) =>
    typeHandlers[p.t]?.(p);
"#;

    let diagnostics = compile_with_options(
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
        !diagnostics.iter().any(|(code, _)| *code == 2349),
        "generic indexed access into mapped type should be callable, got: {diagnostics:?}"
    );
    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2344),
        "generic indexed access into mapped type should preserve the `keyof TypesMap` constraint, got: {diagnostics:?}"
    );
    assert!(
        !diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "mapped type object literal handlers should contextually type callback params, got: {diagnostics:?}"
    );
}

#[test]
fn mapped_type_generic_indexed_access_class_member() {
    // Repro from TypeScript#49242: accessing a mapped type class member
    // with a generic key derived from the same keyof should work.
    let source = r#"
type Types = {
    first: { a1: true };
    second: { a2: true };
    third: { a3: true };
};

class Test {
    entries: { [T in keyof Types]?: Types[T][] };
    constructor() { this.entries = {}; }
    addEntry<T extends keyof Types>(name: T, entry: Types[T]) {
        if (!this.entries[name]) {
            this.entries[name] = [];
        }
        this.entries[name]?.push(entry);
    }
}
"#;

    let diagnostics = compile_with_options(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
    );

    // Should not emit TS2349 (not callable) for .push() call
    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2349),
        "push on mapped type with generic index should be callable, got: {diagnostics:?}"
    );
}

