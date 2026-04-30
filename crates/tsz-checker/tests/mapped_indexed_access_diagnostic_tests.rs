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

/// tsc loses the outer alias when a non-generic `type Foo = X[K]` reduces to
/// a single concrete type; the parameter shows the resolved form, not the
/// alias name. This mirrors the lib's
/// `type WeakKey = WeakKeyTypes[keyof WeakKeyTypes]` (where `WeakKeyTypes`
/// has only `object: object` in es2022) which displays as `object`.
///
/// Repro:
/// ```ts
/// interface MyKeyTypes { object: object; }
/// type MyKey = MyKeyTypes[keyof MyKeyTypes];
/// interface MockRegistry<T> { register(target: MyKey, heldValue: T): void; }
/// declare const f: MockRegistry<unknown>;
/// const s: symbol = Symbol("s");
/// f.register(s, null);
/// // tsc: Argument of type 'symbol' is not assignable to parameter of type 'object'.
/// ```
#[test]
fn indexed_access_alias_displays_resolved_form_in_call_parameter_diagnostic() {
    let source = r#"
interface MyKeyTypes { object: object; }
type MyKey = MyKeyTypes[keyof MyKeyTypes];

interface MockRegistry<T> {
    register(target: MyKey, heldValue: T): void;
}
declare const f: MockRegistry<unknown>;
const s: symbol = Symbol("s");
f.register(s, null);
"#;

    let diagnostics = check_source_diagnostics(source);
    let ts2345: Vec<_> = diagnostics.iter().filter(|d| d.code == 2345).collect();
    assert_eq!(
        ts2345.len(),
        1,
        "expected one TS2345 for the symbol argument, got: {diagnostics:#?}"
    );
    assert!(
        ts2345[0].message_text.contains("'object'"),
        "parameter type should display as 'object' (the resolved form), not the alias name: {:?}",
        ts2345[0].message_text
    );
    assert!(
        !ts2345[0].message_text.contains("'MyKey'"),
        "outer alias should be lost in the indexed-access reduction: {:?}",
        ts2345[0].message_text
    );
}
