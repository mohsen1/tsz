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
