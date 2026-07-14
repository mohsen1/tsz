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

/// Real-lib variant: well-known-symbol recognition keys on the BUILTIN lib
/// `Symbol` identity, so the file-local prelude stub cannot provide the
/// `[Symbol.asyncIterator]` protocol member. Tests whose fixture depends on
/// that recognition (for-await over an `AsyncIterableIterator` constraint)
/// must run against the real lib set, mirroring the CLI.
fn real_lib_strict_codes(source: &str) -> Vec<u32> {
    use tsz_checker::context::{CheckerOptions, ScriptTarget};
    let libs = tsz_checker::test_utils::load_default_lib_files();
    tsz_checker::test_utils::check_multi_file_with_libs(
        &[("test.ts", source)],
        "test.ts",
        CheckerOptions {
            target: ScriptTarget::ES2022,
            strict: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
        &libs,
    )
    .iter()
    .map(|d| d.code)
    .collect()
}

#[test]
fn for_await_type_parameter_constraint_provides_element_type() {
    let codes = real_lib_strict_codes(
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
    let codes = real_lib_strict_codes(
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

#[test]
fn circular_type_parameter_constraint_does_not_recurse_forever() {
    let codes = strict_codes(
        r#"
async function h<Outer extends Inner, Inner extends Outer>(source: Outer) {
  for await (const item of source) {
    const bad: string = item;
  }
}
"#,
    );
    assert!(
        !codes.is_empty(),
        "circular constraints should still produce diagnostics or a safe fallback, got {codes:?}",
    );
}
