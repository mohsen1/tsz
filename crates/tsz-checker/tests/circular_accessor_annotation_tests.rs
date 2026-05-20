use crate::context::CheckerOptions;
use crate::test_utils::check_source;

#[test]
fn type_literal_accessor_circular_annotations_report_on_accessor_name() {
    let source = r#"
declare const c1: {
    get foo(): typeof c1.foo;
}

declare const c2: {
    set foo(value: typeof c2.foo);
}

declare const c3: {
    get foo(): string;
    set foo(value: typeof c3.foo);
}

type T1 = {
    get foo(): T1["foo"];
}

type T2 = {
    set foo(value: T2["foo"]);
}

type T3 = {
    get foo(): string;
    set foo(value: T3["foo"]);
}
"#;

    let diagnostics = check_source(
        source,
        "test.ts",
        CheckerOptions {
            emit_declarations: true,
            ..CheckerOptions::default()
        },
    );

    let ts2502: Vec<_> = diagnostics
        .iter()
        .filter(|diag| diag.code == 2502)
        .collect();
    assert_eq!(
        ts2502.len(),
        4,
        "expected 4 TS2502 diagnostics, got: {ts2502:?}"
    );
    assert!(
        ts2502
            .iter()
            .all(|diag| diag.message_text.contains("'foo'")),
        "all TS2502 diagnostics should report on 'foo': {ts2502:?}"
    );
    assert!(
        ts2502.iter().all(|diag| {
            !diag.message_text.contains("'c1'")
                && !diag.message_text.contains("'c2'")
                && !diag.message_text.contains("'c3'")
        }),
        "outer declaration names should not receive TS2502: {ts2502:?}"
    );
}
