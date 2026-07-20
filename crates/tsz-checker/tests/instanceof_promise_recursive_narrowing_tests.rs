//! Regression coverage for Promise `instanceof` narrowing when the non-Promise
//! union member expands through recursive generic result types.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs, load_default_lib_files};
use tsz_common::common::{ModuleKind, ScriptTarget};

fn diagnostics(source: &str) -> Vec<(u32, String)> {
    let libs = load_default_lib_files();
    if libs.is_empty() {
        return Vec::new();
    }
    check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            target: ScriptTarget::ESNext,
            module: ModuleKind::CommonJS,
            strict: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
        &libs,
    )
    .into_iter()
    .map(|diagnostic| (diagnostic.code, diagnostic.message_text))
    .collect()
}

#[test]
fn recursive_iterator_result_union_is_definitely_narrowed_by_promise() {
    let source = r#"
type Result<T, E> = Ok<T, E> | Err<T, E>;
type InferOk<R> = R extends Result<infer T, unknown> ? T : never;
type InferErr<R> = R extends Result<unknown, infer E> ? E : never;

class Ok<T, E> {
  constructor(readonly value: T) {}
  andThen<R extends Result<unknown, unknown>>(f: (value: T) => R): Result<InferOk<R>, InferErr<R> | E>;
  andThen<U, F>(f: (value: T) => Result<U, F>): Result<U, E | F>;
  andThen(f: (value: T) => Result<unknown, unknown>): Result<unknown, unknown> { return f(this.value); }
}

class Err<T, E> {
  constructor(readonly error: E) {}
  andThen<R extends Result<unknown, unknown>>(_f: (value: T) => R): Result<InferOk<R>, InferErr<R> | E>;
  andThen<U, F>(_f: (value: T) => Result<U, F>): Result<U, E | F>;
  andThen(_f: (value: T) => Result<unknown, unknown>): Result<never, E> { return new Err<never, E>(this.error); }
}

function probe<T, E>(
  body: (() => Generator<Err<never, E>, Err<T, E>>) | (() => AsyncGenerator<Err<never, E>, Err<T, E>>),
) {
  const next = body().next();
  if (next instanceof Promise) {
    return next.then((result) => result.value);
  }
  return next.value;
}
"#;

    let diags = diagnostics(source);
    assert!(
        diags.is_empty(),
        "Promise must be the sole true-branch member and the iterator result the sole false-branch member: {diags:#?}",
    );
}

#[test]
fn recursive_promise_narrowing_is_independent_of_user_binder_names() {
    let source = r#"
type Outcome<Value, Fault> = Pass<Value, Fault> | Fail<Value, Fault>;
type Passed<R> = R extends Outcome<infer Value, unknown> ? Value : never;
type Failed<R> = R extends Outcome<unknown, infer Fault> ? Fault : never;

class Pass<Value, Fault> {
  constructor(readonly value: Value) {}
  chain<R extends Outcome<unknown, unknown>>(f: (value: Value) => R): Outcome<Passed<R>, Failed<R> | Fault>;
  chain<Next, Other>(f: (value: Value) => Outcome<Next, Other>): Outcome<Next, Fault | Other>;
  chain(f: (value: Value) => Outcome<unknown, unknown>): Outcome<unknown, unknown> { return f(this.value); }
}

class Fail<Value, Fault> {
  constructor(readonly error: Fault) {}
  chain<R extends Outcome<unknown, unknown>>(_f: (value: Value) => R): Outcome<Passed<R>, Failed<R> | Fault>;
  chain<Next, Other>(_f: (value: Value) => Outcome<Next, Other>): Outcome<Next, Fault | Other>;
  chain(_f: (value: Value) => Outcome<unknown, unknown>): Outcome<never, Fault> { return new Fail<never, Fault>(this.error); }
}

function inspect<Value, Fault>(
  factory: (() => Generator<Fail<never, Fault>, Fail<Value, Fault>>) | (() => AsyncGenerator<Fail<never, Fault>, Fail<Value, Fault>>),
) {
  const step = factory().next();
  if (step instanceof Promise) {
    return step.then((entry) => entry.value);
  }
  return step.value;
}
"#;

    let diags = diagnostics(source);
    assert!(
        diags.is_empty(),
        "renamed classes, methods, values, and generic binders must preserve the structural narrowing rule: {diags:#?}",
    );
}
