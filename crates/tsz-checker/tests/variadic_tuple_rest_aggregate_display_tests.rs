use tsz_checker::context::CheckerOptions;

fn messages(source: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_source(source, "test.ts", CheckerOptions::default())
        .into_iter()
        .filter(|diag| diag.code != 2318)
        .map(|diag| (diag.code, diag.message_text))
        .collect()
}

#[test]
fn underfilled_variadic_rest_displays_empty_aggregate_not_alias() {
    let cases = [
        (
            "TailAlias",
            r#"
type DropOne<T extends unknown[]> = T extends [unknown, ...infer Rest] ? Rest : never;
type TailAlias = DropOne<[string]>;
declare function foo3<T extends unknown[]>(head: number, ...tail: [...T, number]): void;
foo3(1);
"#,
        ),
        (
            "RestAlias",
            r#"
type RemoveHead<Items extends unknown[]> = Items extends [unknown, ...infer Remaining] ? Remaining : never;
type RestAlias = RemoveHead<[boolean]>;
declare function invoke<Parts extends unknown[]>(head: number, ...tail: [...Parts, number]): void;
invoke(1);
"#,
        ),
    ];

    for (alias, source) in cases {
        let diags = messages(source);
        let ts2345: Vec<_> = diags
            .iter()
            .filter(|(code, _)| *code == 2345)
            .map(|(_, message)| message.as_str())
            .collect();

        assert_eq!(ts2345.len(), 1, "expected one TS2345, got {diags:?}");
        assert!(
            ts2345[0].contains(
                "Argument of type '[]' is not assignable to parameter of type '[...unknown[], number]'."
            ),
            "rest aggregate diagnostic should display structural tuple; got {ts2345:?}"
        );
        assert!(
            !ts2345[0].contains(alias),
            "rest aggregate diagnostic must not leak alias {alias}; got {ts2345:?}"
        );
    }
}

#[test]
fn unbounded_variadic_tuple_assignment_displays_structural_target() {
    let source = r#"
type Values = number[];
type Unbounded = [...Values, boolean];
const data: Unbounded = [false, false];
"#;

    let diags = messages(source);
    let ts2322: Vec<_> = diags
        .iter()
        .filter(|(code, _)| *code == 2322)
        .map(|(_, message)| message.as_str())
        .collect();

    assert_eq!(ts2322.len(), 1, "expected one TS2322, got {diags:?}");
    assert!(
        ts2322[0].contains(
            "Type '[boolean, false]' is not assignable to type '[...number[], boolean]'."
        ),
        "unbounded tuple target should display structurally; got {ts2322:?}"
    );
    assert!(
        !ts2322[0].contains("Unbounded"),
        "unbounded tuple target diagnostic must not leak alias; got {ts2322:?}"
    );
}

#[test]
fn underfilled_variadic_tuple_assignment_preserves_alias_target() {
    let source = r#"
type Handler = (arg: number) => void;
type Funcs = [...Handler[], (arg: string) => void];
const data: Funcs = [];
"#;

    let diags = messages(source);
    let ts2322: Vec<_> = diags
        .iter()
        .filter(|(code, _)| *code == 2322)
        .map(|(_, message)| message.as_str())
        .collect();

    assert_eq!(ts2322.len(), 1, "expected one TS2322, got {diags:?}");
    assert!(
        ts2322[0].contains("Type '[]' is not assignable to type 'Funcs'."),
        "arity-only variadic tuple target should preserve alias; got {ts2322:?}"
    );
}
