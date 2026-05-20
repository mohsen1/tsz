use tsz_checker::test_utils::{check_source_strict, diagnostic_codes};

fn assert_no_diagnostic_code(source: &str, code: u32) {
    let diags = check_source_strict(source);
    assert!(
        !diags.iter().any(|diag| diag.code == code),
        "expected no TS{code}, got {:?}",
        diagnostic_codes(&diags)
    );
}

fn assert_has_diagnostic_code(source: &str, code: u32) {
    let diags = check_source_strict(source);
    assert!(
        diags.iter().any(|diag| diag.code == code),
        "expected TS{code}, got {:?}",
        diagnostic_codes(&diags)
    );
}

#[test]
fn observable_input_union_constraint_accepts_subscribable_union_branch() {
    let source = r#"
declare function of<T>(a: T): Observable<T>;
declare function from<O extends ObservableInput<any>>(input: O): Observable<ObservedValueOf<O>>;

type ObservedValueOf<O> = O extends ObservableInput<infer T> ? T : never;

interface Subscribable<T> {
    subscribe(next?: (value: T) => void, error?: (error: any) => void, complete?: () => void): void;
}
type ObservableInput<T> = Subscribable<T> | Subscribable<never>;

interface Observable<T> extends Subscribable<T> {}

function asObservable(input: string | ObservableInput<string>): Observable<string> {
    return typeof input === 'string' ? of(input) : from(input);
}
"#;

    assert_no_diagnostic_code(source, 2345);
}

#[test]
fn observable_input_union_constraint_still_rejects_non_subscribable() {
    let source = r#"
declare function from<O extends ObservableInput<any>>(input: O): Observable<ObservedValueOf<O>>;

type ObservedValueOf<O> = O extends ObservableInput<infer T> ? T : never;

interface Subscribable<T> {
    subscribe(next?: (value: T) => void, error?: (error: any) => void, complete?: () => void): void;
}
type ObservableInput<T> = Subscribable<T> | Subscribable<never>;
interface Observable<T> extends Subscribable<T> {}

declare const input: { unsubscribe(): void };
from(input);
"#;

    assert_has_diagnostic_code(source, 2345);
}

#[test]
fn generic_template_literal_preserves_declared_identifier_spans() {
    let source = r#"
type Registry = {
    a: { a1: {} };
    b: { b1: {} };
};

type Keyof<T> = keyof T & string;

declare function f1<
    Scope extends Keyof<Registry>,
    Event extends Keyof<Registry[Scope]>,
>(eventPath: `${Scope}:${Event}`): void;

function f2<
    Scope extends Keyof<Registry>,
    Event extends Keyof<Registry[Scope]>,
>(scope: Scope, event: Event) {
    f1(`${scope}:${event}`);
}
"#;

    assert_no_diagnostic_code(source, 2345);
}

#[test]
fn generic_template_literal_still_rejects_invalid_path() {
    let source = r#"
type Registry = {
    a: { a1: {} };
    b: { b1: {} };
};

type Keyof<T> = keyof T & string;

declare function f1<
    Scope extends Keyof<Registry>,
    Event extends Keyof<Registry[Scope]>,
>(eventPath: `${Scope}:${Event}`): void;

f1("c:a1");
"#;

    assert_has_diagnostic_code(source, 2345);
}

#[test]
fn union_callee_literal_argument_survives_contextual_collection() {
    let source = r#"
function f5(x: (x: string | undefined) => void, y: (x?: 'hello') => void) {
    let f = !!true ? x : y;
    f();
    f('hello');
}

function f6(x: (x: 'hello' | undefined) => void, y: (x?: string) => void) {
    let f = !!true ? x : y;
    f();
    f('hello');
}
"#;
    let diags = check_source_strict(source);
    let codes = diagnostic_codes(&diags);

    assert!(
        !codes.contains(&2345),
        "literal-preserving union calls must not emit TS2345, got {codes:?}"
    );
    assert!(
        codes.contains(&2554),
        "f6() must still report its missing required argument, got {codes:?}"
    );
}

#[test]
fn union_callee_literal_argument_still_rejects_impossible_literal() {
    let source = r#"
function f5(x: (x: string | undefined) => void, y: (x?: 'hello') => void) {
    let f = !!true ? x : y;
    f('bye');
}
"#;

    assert_has_diagnostic_code(source, 2345);
}

#[test]
fn unannotated_conditional_initializer_keeps_callable_union() {
    let source = r#"
type A = {
    f(): void;
}

type B = {
    f(x?: string): void;
    g(): void;
}

function f11(a: A, b: B) {
    let z = !!true ? a : b;
    z.f();
    z.f('hello');
}
"#;

    assert_no_diagnostic_code(source, 2345);
    assert_no_diagnostic_code(source, 2554);
}

#[test]
fn later_assignment_after_conditional_initializer_still_narrows() {
    let source = r#"
type A = {
    f(): void;
}

type B = {
    f(x?: string): void;
    g(): void;
}

function f11(a: A, b: B) {
    let z = !!true ? a : b;
    z = a;
    z.f('hello');
}
"#;

    assert_has_diagnostic_code(source, 2554);
}

#[test]
fn promise_try_rest_tuple_inference_accepts_undefined_optional_slot() {
    let source = r#"
interface PromiseLike<T> {}
interface PromiseConstructor {
    try<T, Args extends unknown[]>(
        callbackFn: (...args: Args) => T,
        ...args: Args
    ): PromiseLike<T>;
}
declare const Promise: PromiseConstructor;

Promise.try((foo: string, bar?: number) => "Async result", "foo", undefined);
"#;

    assert_no_diagnostic_code(source, 2345);
}

#[test]
fn promise_try_rest_tuple_inference_rejects_undefined_required_slot() {
    let source = r#"
interface PromiseLike<T> {}
interface PromiseConstructor {
    try<T, Args extends unknown[]>(
        callbackFn: (...args: Args) => T,
        ...args: Args
    ): PromiseLike<T>;
}
declare const Promise: PromiseConstructor;

Promise.try((foo: string, bar: number) => "Async result", "foo", undefined);
"#;

    assert_has_diagnostic_code(source, 2345);
}
