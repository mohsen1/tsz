use crate::test_utils::check_source_diagnostics;

#[test]
fn generic_wrapper_return_context_preserves_outer_type_params() {
    let diags = check_source_diagnostics(
        r#"
declare function wrap<T extends Function>(value: T): T;

function outer<U>(value: U) {
    const fnValue: (arg: U, label: string) => void = wrap((arg, label) => {
        void value;
        void arg;
        void label;
    });
}
"#,
    );

    let ts7006: Vec<_> = diags.iter().filter(|d| d.code == 7006).collect();
    assert_eq!(
        ts7006.len(),
        0,
        "Expected no TS7006 when return-context inference carries outer type parameters, got: {:?}",
        ts7006.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

#[test]
fn generic_wrapper_recheck_clears_stale_implicit_any_on_callback_body_use() {
    let diags = check_source_diagnostics(
        r#"
declare function wrap<T extends Function>(value: T): T;
declare class Proxy<T extends object> {
    constructor(target: T, handler: ProxyHandler<T>);
}
interface ProxyHandler<T extends object> {
    set?: (target: T, property: string | symbol, value: any, receiver: any) => boolean;
}
declare namespace Reflect {
    function set(target: object, property: string | symbol, value: any, receiver: any): boolean;
}

function outer<U extends object>(value: U) {
    return new Proxy(value, {
        set: wrap((target, property, nextValue, receiver) =>
            Reflect.set(target, property, nextValue, receiver)
        ),
    });
}
"#,
    );

    let ts7006: Vec<_> = diags.iter().filter(|d| d.code == 7006).collect();
    assert_eq!(
        ts7006.len(),
        0,
        "Expected no stale TS7006 after contextual re-check, got: {:?}",
        ts7006.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

#[test]
fn generic_wrapper_with_extra_arguments_preserves_contextual_function_type() {
    let diags = check_source_diagnostics(
        r#"
declare function deprecate<T extends Function>(
    fn: T,
    msg: string,
    code?: string,
): T;

function outer<U extends object>(value: U, message: string, code: string): U {
    return new Proxy(value, {
        set: deprecate(
            (target, property, nextValue, receiver) =>
                Reflect.set(target, property, nextValue, receiver),
            message,
            code,
        ),
        defineProperty: deprecate(
            (target, property, descriptor) =>
                Reflect.defineProperty(target, property, descriptor),
            message,
            code,
        ),
        deleteProperty: deprecate(
            (target, property) => Reflect.deleteProperty(target, property),
            message,
            code,
        ),
        setPrototypeOf: deprecate(
            (target, proto) => Reflect.setPrototypeOf(target, proto),
            message,
            code,
        ),
    });
}
"#,
    );

    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    let ts7006: Vec<_> = diags.iter().filter(|d| d.code == 7006).collect();
    assert_eq!(
        ts2322.len(),
        0,
        "Expected no TS2322 when wrapped callbacks are retyped through return-context invalidation, got: {:?}",
        ts2322.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
    assert_eq!(
        ts7006.len(),
        0,
        "Expected no TS7006 in wrapped Proxy handler with extra arguments, got: {:?}",
        ts7006.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// When a generic function's return type is used to infer type parameters
/// from a contextual type in argument position, the inference should flow
/// through to callback parameters even when those callbacks are
/// context-sensitive.
///
/// Pattern: `make<T>(fn: (x: T) => void): { value: T }` with contextual
/// type `{ value: number }`. T should be inferred as `number` from the
/// return context, and `x` should get type `number`.
#[test]
fn return_context_seeds_inference_with_context_sensitive_args() {
    let diags = check_source_diagnostics(
        r#"
declare function make<T>(fn: (x: T) => void): { value: T };
const r: { value: number } = make((x) => {
    const y: string = x; // TS2322: number not assignable to string
});
"#,
    );

    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        !ts2322.is_empty(),
        "Expected TS2322 when contextual return type seeds T=number but callback assigns to string"
    );
}

/// Same pattern but the contextual type comes from being an argument
/// to another function, testing the full inference chain.
#[test]
fn return_context_seeds_inference_through_argument_position() {
    let diags = check_source_diagnostics(
        r#"
declare function make<T>(fn: (x: T) => void): { value: T };
declare function consume(box: { value: number }): void;
consume(make((x) => {
    const y: string = x; // TS2322: number not assignable to string
}));
"#,
    );

    // Check that no TS7006 (implicit any) is emitted for `x` — it should
    // be contextually typed as number from the return-context seeding.
    let ts7006: Vec<_> = diags.iter().filter(|d| d.code == 7006).collect();
    assert_eq!(
        ts7006.len(),
        0,
        "Expected no TS7006 (implicit any) — x should be contextually typed as number, got: {:?}",
        ts7006.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Return-context seeding in `compute_contextual_types` should set
/// `had_return_context_substitution` when the Round 1 substitution
/// already contains the same value, preventing unnecessary retries
/// that could clear valid diagnostics.
#[test]
fn return_context_substitution_match_suppresses_retry() {
    let diags = check_source_diagnostics(
        r#"
interface Action<T extends string> {
    (): void;
    _out?: T;
}
declare function assign<T extends string>(
    fn: (spawn: (actor: T) => void) => {},
): Action<T>;
declare function use(b: Action<"hello">): void;
use(assign((x) => {
    const y: number = x;
    return {};
}));
"#,
    );

    let ts7006: Vec<_> = diags.iter().filter(|d| d.code == 7006).collect();
    assert_eq!(
        ts7006.len(),
        0,
        "Expected no TS7006 — callback parameters should be contextually typed, got: {:?}",
        ts7006.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}
