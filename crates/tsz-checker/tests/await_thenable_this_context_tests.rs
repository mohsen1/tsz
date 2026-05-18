use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{
    check_source_code_messages as diagnostics, check_source_with_libs_code_messages,
    load_default_lib_files,
};
use tsz_common::common::{ModuleKind, ScriptTarget};

fn diagnostic_codes(source: &str, module: ModuleKind, target: ScriptTarget) -> Vec<u32> {
    tsz_checker::test_utils::check_source(
        source,
        "test.ts",
        CheckerOptions {
            module,
            target,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|diagnostic| diagnostic.code)
    .collect()
}

#[test]
fn await_direct_standard_lib_promise_resolve_skips_invalid_thenable_this_diagnostic() {
    let libs = load_default_lib_files();
    assert!(!libs.is_empty(), "default lib files must be available");

    let diags = check_source_with_libs_code_messages(
        r#"
async function process<T extends Record<string, unknown>>(input: T): Promise<T> {
    const result = await Promise.resolve(input);
    return result;
}
"#,
        "test.ts",
        CheckerOptions::default(),
        &libs,
    );
    assert!(
        !diags.iter().any(|(code, _)| *code == 1320),
        "Direct standard-library Promise applications cannot fail TS1320 thenable-this validation. Got: {diags:#?}"
    );
}

#[test]
fn await_local_promise_like_named_type_still_validates_then_this_type() {
    let diags = diagnostics(
        r#"
interface PromiseLike<T> {
    then(this: { required: string }, onfulfilled?: ((value: T) => void) | null): void;
}
declare const bad: PromiseLike<string>;
async function test() {
    await bad;
}
"#,
    );
    assert!(
        diags.iter().any(|(code, _)| *code == 1320),
        "Local PromiseLike applications must still run structural TS1320 validation. Got: {diags:#?}"
    );
}

#[test]
fn await_standard_lib_promise_shell_still_validates_bad_thenable_payload() {
    let libs = load_default_lib_files();
    assert!(!libs.is_empty(), "default lib files must be available");

    let diags = check_source_with_libs_code_messages(
        r#"
interface BadThenable {
    then(this: { required: string }, onfulfilled?: ((value: number) => void) | null): void;
}
declare const nestedPromise: Promise<BadThenable>;
declare const nestedPromiseLike: PromiseLike<BadThenable>;
declare const normalizedPayload: Promise<Awaited<BadThenable>>;
async function test() {
    await nestedPromise;
    await nestedPromiseLike;
    await normalizedPayload;
}
"#,
        "test.ts",
        CheckerOptions::default(),
        &libs,
    );
    let ts1320_count = diags.iter().filter(|(code, _)| *code == 1320).count();
    assert_eq!(
        ts1320_count, 2,
        "Standard-library Promise shells must skip only their own then check and still validate the awaited payload. Got: {diags:#?}"
    );
}

#[test]
fn await_thenable_accepts_then_signature_with_specialized_this_type() {
    let source = r#"
interface PromiseLike<T> {
    then<TResult1 = T, TResult2 = never>(
        onfulfilled?: ((value: T) => TResult1 | PromiseLike<TResult1>) | null,
        onrejected?: ((reason: any) => TResult2 | PromiseLike<TResult2>) | null
    ): PromiseLike<TResult1 | TResult2>;
}
interface Promise<T> extends PromiseLike<T> {}
class EPromise<E, A> {
    then<B = A>(
        this: EPromise<never, A>,
        onfulfilled?: ((value: A) => B | PromiseLike<B>) | null | undefined
    ): PromiseLike<B> {
        return null as any;
    }
}
declare const withTypedFailure: EPromise<number, string>;
async function test() {
    await withTypedFailure;
}
"#;

    let diags = diagnostics(source);
    assert!(
        !diags.iter().any(|(code, _)| *code == 1320),
        "Did not expect TS1320 for await operand with specialized then this type. Got: {diags:#?}"
    );
}

#[test]
fn phantom_type_parameters_do_not_block_recursive_this_assignability() {
    let source = r#"
interface Box<T> {}
class ThenLike<E, A> {
    then<B = A>(
        this: ThenLike<never, A>,
        onfulfilled?: ((value: A) => B | Box<B>) | null | undefined
    ): Box<B> {
        return null as any;
    }
}
declare const numberString: ThenLike<number, string>;
const neverString: ThenLike<never, string> = numberString;
numberString.then(value => value);
"#;

    let diags = diagnostics(source);
    assert!(
        !diags.iter().any(|(code, _)| *code == 2322 || *code == 2684),
        "Did not expect TS2322/TS2684 when only a phantom type argument differs. Got: {diags:#?}"
    );
}

#[test]
fn await_thenable_accepts_then_signature_with_satisfied_this_type() {
    let source = r#"
interface PromiseLike<T> {
    then<TResult1 = T, TResult2 = never>(
        onfulfilled?: ((value: T) => TResult1 | PromiseLike<TResult1>) | null,
        onrejected?: ((reason: any) => TResult2 | PromiseLike<TResult2>) | null
    ): PromiseLike<TResult1 | TResult2>;
}
interface Promise<T> extends PromiseLike<T> {}
class EPromise<E, A> {
    then<B = A>(
        this: EPromise<never, A>,
        onfulfilled?: ((value: A) => B | PromiseLike<B>) | null | undefined
    ): PromiseLike<B> {
        return null as any;
    }
}
declare const ok: EPromise<never, string>;
async function test() {
    await ok;
}
"#;

    let diags = diagnostics(source);
    assert!(
        !diags.iter().any(|(code, _)| *code == 1320),
        "Did not expect TS1320 when the await operand satisfies then's this type. Got: {diags:#?}"
    );
}

#[test]
fn contextual_constructor_return_infers_class_type_parameters() {
    let source = r#"
class Box<T> {
    value!: T;
    constructor() {}
}
function makeBox(): Box<string> {
    return new Box();
}
"#;

    let diags = diagnostics(source);
    assert!(
        diags.is_empty(),
        "Expected contextual constructor return type to infer Box<string>, got: {diags:#?}"
    );
}

#[test]
fn epromise_static_constructors_use_contextual_return_type() {
    let source = r#"
type Either<E, A> = Left<E> | Right<A>;
type Left<E> = { tag: 'Left', e: E };
type Right<A> = { tag: 'Right', a: A };

const mkLeft = <E>(e: E): Either<E, never> => ({ tag: 'Left', e });
const mkRight = <A>(a: A): Either<never, A> => ({ tag: 'Right', a });

interface PromiseLike<T> {
    then<TResult1 = T, TResult2 = never>(
        onfulfilled?: ((value: T) => TResult1 | PromiseLike<TResult1>) | null,
        onrejected?: ((reason: any) => TResult2 | PromiseLike<TResult2>) | null
    ): PromiseLike<TResult1 | TResult2>;
}
interface Promise<T> extends PromiseLike<T> {}
declare const Promise: {
    resolve<T>(value: T): Promise<T>;
};

class EPromise<E, A> implements PromiseLike<A> {
    static succeed<A>(a: A): EPromise<never, A> {
        return new EPromise(Promise.resolve(mkRight(a)));
    }

    static fail<E>(e: E): EPromise<E, never> {
        return new EPromise(Promise.resolve(mkLeft(e)));
    }

    constructor(readonly p: PromiseLike<Either<E, A>>) { }

    then<B = A, B1 = never>(
        this: EPromise<never, A>,
        onfulfilled?: ((value: A) => B | PromiseLike<B>) | null | undefined,
        onrejected?: ((reason: any) => B1 | PromiseLike<B1>) | null | undefined
    ): PromiseLike<B | B1> {
        return this.p.then(
            either => onfulfilled?.((either as Right<A>).a) ?? (either as Right<A>).a as unknown as B,
            onrejected
        )
    }
}
"#;

    let diags = diagnostics(source);
    assert!(
        !diags.iter().any(|(code, message)| {
            *code == 2322 && message.contains("EPromise<unknown, unknown>")
        }),
        "Did not expect EPromise constructor return to infer unknowns. Got: {diags:#?}"
    );
    assert!(
        !diags.iter().any(|(code, message)| {
            *code == 2322 && message.contains("PromiseLike<PromiseLike")
        }),
        "Did not expect nested PromiseLike inference in then return. Got: {diags:#?}"
    );
}

#[test]
fn variable_initializer_top_level_await_requires_supported_module_kind() {
    let codes = diagnostic_codes(
        "const data = await 1;\nexport {};\n",
        ModuleKind::CommonJS,
        ScriptTarget::ESNext,
    );

    assert!(
        codes.contains(&1378),
        "top-level await in a variable initializer must emit TS1378 for CommonJS modules; got {codes:?}",
    );
}

#[test]
fn nested_initializer_top_level_await_requires_supported_target() {
    let codes = diagnostic_codes(
        "const data = choose(await 1 ? 1 : 2);\ndeclare function choose(value: number): number;\nexport {};\n",
        ModuleKind::ES2022,
        ScriptTarget::ES5,
    );

    assert!(
        codes.contains(&1378),
        "top-level await nested inside an initializer must emit TS1378 below ES2017; got {codes:?}",
    );
}

#[test]
fn top_level_await_supported_module_and_target_is_allowed() {
    let codes = diagnostic_codes(
        "const data = await 1;\nexport {};\n",
        ModuleKind::ES2022,
        ScriptTarget::ES2017,
    );

    assert!(
        !codes.contains(&1378),
        "ES2022 modules targeting ES2017 should allow top-level await; got {codes:?}",
    );
}

#[test]
fn await_inside_nested_async_function_initializer_is_not_top_level() {
    let codes = diagnostic_codes(
        "const run = async () => await 1;\nexport {};\n",
        ModuleKind::CommonJS,
        ScriptTarget::ESNext,
    );

    assert!(
        !codes.contains(&1378),
        "await inside a nested async function body must not use the source-file top-level gate; got {codes:?}",
    );
}
