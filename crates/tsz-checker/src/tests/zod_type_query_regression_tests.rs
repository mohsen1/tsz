use crate::test_utils::check_source_diagnostics;

#[test]
fn generic_promise_then_flattens_promise_return_from_callback() {
    let diags = check_source_diagnostics(
        r#"
interface PromiseLike<T> {
    then<TResult1 = T>(
        onfulfilled?: ((value: T) => TResult1 | PromiseLike<TResult1>) | undefined | null
    ): PromiseLike<TResult1>;
}

interface Promise<T> {
    then<TResult1 = T>(
        onfulfilled?: ((value: T) => TResult1 | PromiseLike<TResult1>) | undefined | null
    ): Promise<TResult1>;
}

interface Response {
    json(): Promise<{ entries: string[] }>;
}

declare function fetch(url: string): Promise<Response>;

fetch("/entries")
    .then(res => res.json())
    .then(data => {
        const entries: string[] = data.entries;
        return entries;
    });
"#,
    );

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2339 || d.code == 2345)
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected Promise.then callback returning Promise<T> to infer T, got: {:?}",
        errors.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

#[test]
fn object_literal_function_property_can_read_earlier_self_property() {
    let diags = check_source_diagnostics(
        r#"
const entryKeys = {
    all: ['entries'] as const,
    list: () => [...entryKeys.all, 'list'] as const
};

declare function takesKey(key: readonly ['entries', 'list']): void;
takesKey(entryKeys.list());
"#,
    );

    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 2339 | 2345 | 7022 | 7023))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected self-referencing object literal property to keep earlier property type, got: {:?}",
        relevant
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn conditional_with_failed_infer_uses_false_branch() {
    let diags = check_source_diagnostics(
        r#"
interface Register {}
interface Error { message: string }
type DefaultError = Register extends { defaultError: infer TError } ? TError : Error;
declare const err: DefaultError;
declare function takesError(err: Error): void;
takesError(err);
declare function takesString(value: string): void;
takesString(err);
"#,
    );

    let relevant: Vec<_> = diags.iter().filter(|d| d.code == 2345).collect();
    assert_eq!(
        relevant.len(),
        1,
        "Expected conditional infer miss to resolve false branch and reject string, got: {:?}",
        relevant
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}
