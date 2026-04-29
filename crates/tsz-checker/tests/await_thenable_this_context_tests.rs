use tsz_checker::test_utils::check_source_code_messages as diagnostics;

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
