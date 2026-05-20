use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

#[test]
fn bare_alias_to_generic_class_default_keeps_alias_display() {
    let source = r#"
declare class TableClass<S = any> {
    _field: S;
}

type Table = TableClass;

declare const o: Table;
let value: boolean = o;
"#;

    let diagnostics = check_source(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            strict_function_types: true,
            ..CheckerOptions::default()
        },
    );
    let diag = diagnostics
        .iter()
        .find(|diag| diag.code == 2322)
        .expect("expected TS2322 for assigning Table to boolean");
    assert!(
        diag.message_text
            .contains("Type 'Table' is not assignable to type 'boolean'."),
        "alias declaration diagnostic should keep the source alias, got: {diag:?}"
    );
    assert!(
        !diag.message_text.contains("TableClass<any>"),
        "alias declaration diagnostic should not expand to the generic class default, got: {diag:?}"
    );
}

#[test]
fn anonymous_empty_object_target_is_not_repainted_as_mapped_alias_reduction() {
    let source = r#"
type T50<T> = { [P in keyof T]: number };
type T52 = T50<unknown>;

function f22(x: unknown) {
    let v: {} = x;
}

function f30<T, U extends unknown>(t: T, u: U) {
    let x: {} = t;
    let y: {} = u;
}

function oops<T extends unknown>(arg: T): {} {
    return arg;
}
"#;

    let diagnostics = check_source(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            strict_function_types: true,
            ..CheckerOptions::default()
        },
    );
    let ts2322_messages: Vec<_> = diagnostics
        .iter()
        .filter(|diag| diag.code == 2322)
        .map(|diag| diag.message_text.as_str())
        .collect();
    assert_eq!(
        ts2322_messages.len(),
        4,
        "expected four TS2322 diagnostics for unknown/type-param to {{}}, got: {diagnostics:#?}"
    );
    assert!(
        ts2322_messages
            .iter()
            .all(|message| message.contains("not assignable to type '{}'.")
                && !message.contains("T50<unknown>")),
        "anonymous {{}} targets must not inherit the display alias from T50<unknown>: {ts2322_messages:#?}"
    );
}
