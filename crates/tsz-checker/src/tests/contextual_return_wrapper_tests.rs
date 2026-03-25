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
