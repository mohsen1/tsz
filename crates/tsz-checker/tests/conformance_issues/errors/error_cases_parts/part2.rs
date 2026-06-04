#[test]
fn test_indexed_access_string_number_union_reports_both_index_signatures() {
    let source = r#"
class Shape {
    name: string;
}

type T = Shape[string | number];
"#;
    let diagnostics =
        compile_and_get_raw_diagnostics_named("test.ts", source, CheckerOptions::default());

    let ts2537: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == 2537)
        .collect();
    assert!(
        ts2537.iter().any(|diagnostic| {
            diagnostic
                .message_text
                .contains("Type 'Shape' has no matching index signature for type 'string'")
        }),
        "Expected TS2537 for the string member.\nActual diagnostics: {diagnostics:#?}"
    );
    assert!(
        ts2537.iter().any(|diagnostic| {
            diagnostic
                .message_text
                .contains("Type 'Shape' has no matching index signature for type 'number'")
        }),
        "Expected TS2537 for the number member.\nActual diagnostics: {diagnostics:#?}"
    );

    let expected_start = source.find("string | number").unwrap() as u32;
    let expected_len = "string | number".len() as u32;
    assert!(
        ts2537.iter().all(|diagnostic| {
            diagnostic.start == expected_start && diagnostic.length == expected_len
        }),
        "Expected TS2537 diagnostics to anchor at the full index type.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_indexed_access_type_reports_ts2537_for_array_string_index() {
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_and_get_diagnostics_with_lib(
        r#"
type T = string[][string];
"#,
    );

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2537
                && message
                    .contains("Type 'string[]' has no matching index signature for type 'string'")
        }),
        "Expected TS2537 for `string[][string]`.\nActual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2536),
        "Did not expect TS2536 for `string[][string]` once concrete classifier applies.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_nested_indexed_access_diagnostic_uses_last_bracket_span() {
    if !lib_files_available() {
        return;
    }

    let source = r#"
type T = string[][boolean];
"#;
    let diagnostics = compile_and_get_raw_diagnostics_named_with_lib_and_options(
        "test.ts",
        source,
        CheckerOptions::default(),
    );

    let ts2538 = diagnostics
        .iter()
        .find(|diagnostic| {
            diagnostic.code == 2538
                && diagnostic
                    .message_text
                    .contains("Type 'boolean' cannot be used as an index type")
        })
        .expect("expected TS2538 for `string[][boolean]`");
    assert_eq!(ts2538.start, source.rfind("boolean").unwrap() as u32);
    assert_eq!(ts2538.length, "boolean".len() as u32);
}

#[test]
fn test_contextual_intersection_callback_return_preserves_object_literal_members() {
    let diagnostics = compile_and_get_diagnostics_named(
        "test.ts",
        r#"
declare function test4(
  arg: { a: () => { prop: "foo" } } & {
    [k: string]: () => { prop: any };
  },
): unknown;

test4({
  a: () => ({ prop: "foo" }),
  b: () => ({ prop: "bar" }),
});

test4({
  a: () => ({ prop: "bar" }),
});
"#,
        CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2015,
            strict_null_checks: true,
            ..Default::default()
        },
    );

    let bar_errors = diagnostics
        .iter()
        .filter(|(code, message)| {
            *code == 2322 && message.contains("Type '\"bar\"' is not assignable to type '\"foo\"'")
        })
        .count();

    assert_eq!(
        bar_errors, 1,
        "Expected exactly the single invalid callback-return literal mismatch from test4, matching the TypeScript baseline.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_excess_property_display_widens_mapped_callback_value_param() {
    let diagnostics = compile_and_get_diagnostics_named(
        "test.ts",
        r#"
declare function f2<T extends object>(
  data: T,
  handlers: { [P in keyof T as T[P] extends string ? P : never]: (value: T[P], prop: P) => void },
): void;

f2(
  {
    foo: 0,
    bar: "",
  },
  {
    foo: (value, key) => {},
  },
);
"#,
        CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2015,
            strict_null_checks: true,
            ..Default::default()
        },
    );

    assert!(
        diagnostics
            .iter()
            .any(|(_, message)| message.contains("(value: string, prop: \"bar\") => void")),
        "Expected excess-property target display to widen callback value parameter to string.\nActual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !diagnostics
            .iter()
            .any(|(_, message)| message.contains("(value: \"\", prop: \"bar\") => void")),
        "Did not expect literal empty-string callback parameter in excess-property target display.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_async_generator_type_references_preserve_all_type_params() {
    if !lib_files_available() {
        return;
    }

    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        r#"
interface Result<T, E> {
    [Symbol.iterator](): Generator<E, T, unknown>
}

type Book = { id: string; title: string; authorId: string };
type Author = { id: string; name: string };
type BookWithAuthor = Book & { author: Author };

declare const authorPromise: Promise<Result<Author, "NOT_FOUND_AUTHOR">>;
declare const mapper: <T>(result: Result<T, "NOT_FOUND_AUTHOR">) => Result<T, "NOT_FOUND_AUTHOR">;
type T = AsyncGenerator<string, number, unknown>;
declare const g: <T, U, V>() => AsyncGenerator<T, U, V>;
async function* f(): AsyncGenerator<"NOT_FOUND_AUTHOR" | "NOT_FOUND_BOOK", BookWithAuthor, unknown> {
    const test1 = await authorPromise.then(mapper);
    const test2 = yield* await authorPromise.then(mapper);
    const x1 = yield* g();
    const x2: number = yield* g();
    return null! as BookWithAuthor;
}
"#,
        CheckerOptions {
            target: ScriptTarget::ESNext,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2314),
        "AsyncGenerator should retain its 3-parameter lib arity.\nActual diagnostics: {diagnostics:#?}"
    );
    assert_eq!(
        diagnostics.iter().filter(|(code, _)| *code == 2322).count(),
        0,
        "AsyncGenerator yield* contextual typing should preserve delegated return context.\nActual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !diagnostics
            .iter()
            .any(|(code, _)| matches!(*code, 2504 | 2769)),
        "Optional callback unions should preserve contextual signatures for generic mappers.\nActual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2345),
        "Delegated `yield* await promise.then(mapper)` should not over-constrain the generic mapper callback.\nActual diagnostics: {diagnostics:#?}"
    );
}
