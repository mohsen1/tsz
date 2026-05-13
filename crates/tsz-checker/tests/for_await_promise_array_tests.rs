use tsz_checker::test_utils::check_source_code_messages as diagnostics;

#[test]
fn for_await_of_promise_array_element_type_is_awaited() {
    let source = r#"
interface PromiseLike<T> {
    then<TResult1 = T, TResult2 = never>(
        onfulfilled?: ((value: T) => TResult1 | PromiseLike<TResult1>) | null,
        onrejected?: ((reason: any) => TResult2 | PromiseLike<TResult2>) | null
    ): PromiseLike<TResult1 | TResult2>;
}
interface Promise<T> extends PromiseLike<T> {
    then<TResult1 = T, TResult2 = never>(
        onfulfilled?: ((value: T) => TResult1 | PromiseLike<TResult1>) | null,
        onrejected?: ((reason: any) => TResult2 | PromiseLike<TResult2>) | null
    ): Promise<TResult1 | TResult2>;
}
interface Array<T> {
    [n: number]: T;
    length: number;
    [Symbol.iterator](): Iterator<T>;
}
interface Iterator<T> {
    next(): IteratorResult<T>;
}
interface IteratorResult<T> {
    done?: boolean;
    value: T;
}
async function processItems(items: Promise<number>[]) {
    for await (const item of items) {
        const n: number = item;
    }
}
"#;

    let diags = diagnostics(source);
    let ts2322 = diags
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect::<Vec<_>>();
    assert!(
        ts2322.is_empty(),
        "for-await-of over Promise<number>[] should type item as number, not Promise<number>. Got: {ts2322:#?}"
    );
}

#[test]
fn for_await_of_non_promise_array_element_type_unchanged() {
    let source = r#"
interface Array<T> {
    [n: number]: T;
    length: number;
    [Symbol.iterator](): Iterator<T>;
}
interface Iterator<T> {
    next(): IteratorResult<T>;
}
interface IteratorResult<T> {
    done?: boolean;
    value: T;
}
async function processNumbers(items: number[]) {
    for await (const item of items) {
        const n: number = item;
    }
}
"#;

    let diags = diagnostics(source);
    let ts2322 = diags
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect::<Vec<_>>();
    assert!(
        ts2322.is_empty(),
        "for-await-of over number[] should type item as number. Got: {ts2322:#?}"
    );
}
