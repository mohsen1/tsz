use tsz_checker::test_utils::check_source_strict_codes;

const ASYNC_ITERABLE_PRELUDE: &str = r#"
declare const Symbol: { readonly asyncIterator: unique symbol };

interface PromiseLike<T> {
  then<TResult>(onfulfilled: (value: T) => TResult | PromiseLike<TResult>): PromiseLike<TResult>;
}

interface Promise<T> extends PromiseLike<T> {}

interface IteratorYieldResult<TYield> {
  done?: false;
  value: TYield;
}

interface IteratorReturnResult<TReturn> {
  done: true;
  value: TReturn;
}

type IteratorResult<TYield, TReturn = any> =
  IteratorYieldResult<TYield> | IteratorReturnResult<TReturn>;

interface AsyncIterator<T, TReturn = any, TNext = unknown> {
  next(...args: [] | [TNext]): Promise<IteratorResult<T, TReturn>>;
}

interface AsyncIterable<T> {
  [Symbol.asyncIterator](): AsyncIterator<T>;
}

interface AsyncIterableIterator<T> extends AsyncIterator<T> {
  [Symbol.asyncIterator](): AsyncIterableIterator<T>;
}
"#;

fn strict_codes(source: &str) -> Vec<u32> {
    check_source_strict_codes(&format!("{ASYNC_ITERABLE_PRELUDE}\n{source}"))
}

#[test]
fn for_await_type_parameter_constraint_provides_element_type() {
    let codes = strict_codes(
        r#"
async function f<T extends AsyncIterableIterator<number>>(iter: T) {
  for await (const value of iter) {
    const ok: number = value;
    const bad: string = value;
  }
}
"#,
    );
    assert!(
        !codes.contains(&2504),
        "constrained async-iterable type parameter must not emit TS2504, got {codes:?}",
    );
    assert!(
        codes.contains(&2322),
        "for-await element type must be number rather than any, got {codes:?}",
    );
}

#[test]
fn for_await_renamed_type_parameter_constraint_provides_element_type() {
    let codes = strict_codes(
        r#"
async function g<Stream extends AsyncIterableIterator<boolean>>(source: Stream) {
  for await (const item of source) {
    const ok: boolean = item;
    const bad: number = item;
  }
}
"#,
    );
    assert!(
        !codes.contains(&2504),
        "renamed constrained async-iterable parameter must not emit TS2504, got {codes:?}",
    );
    assert!(
        codes.contains(&2322),
        "for-await element type must follow the constraint yield type, got {codes:?}",
    );
}
