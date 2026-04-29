use tsz_checker::test_utils::check_source_diagnostics;

#[test]
fn mapped_indexed_access_discriminated_union_reports_outer_assignment() {
    let source = r#"
type Pairs<T> = {
    [TKey in keyof T]: {
        key: TKey;
        value: T[TKey];
    };
};

type Pair<T> = Pairs<T>[keyof T];

type FooBar = {
    foo: string;
    bar: number;
};

let pair1: Pair<FooBar> = {
    key: "foo",
    value: 3
};

let pair2: Pairs<FooBar>[keyof FooBar] = {
    key: "foo",
    value: 3
};
"#;

    let diagnostics = check_source_diagnostics(source);
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|diag| diag.code == 2322)
        .collect();

    assert_eq!(
        ts2322.len(),
        2,
        "expected one TS2322 per invalid assignment, got: {diagnostics:#?}"
    );
    assert!(
        ts2322
            .iter()
            .any(|diag| diag.message_text.contains("Pair<FooBar>")),
        "alias target should stay on the outer assignment diagnostic: {ts2322:#?}"
    );
    assert!(
        ts2322.iter().any(|diag| diag
            .message_text
            .contains("{ key: \"foo\"; value: string; } | { key: \"bar\"; value: number; }")),
        "indexed-access target should display its evaluated union on the outer assignment: {ts2322:#?}"
    );
    assert!(
        ts2322.iter().all(|diag| !diag
            .message_text
            .contains("Type 'number' is not assignable to type 'string'")),
        "mapped indexed access assignments should not elaborate into the selected union member's property: {ts2322:#?}"
    );
}
