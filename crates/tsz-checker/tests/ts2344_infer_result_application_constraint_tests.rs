//! TS2344 checks for alias applications that produce conditional `infer` results.
//!
//! Structural rule: when a generic type argument is an alias application whose
//! instantiated body is `T extends Pattern<infer U> ? U : never`, constraint
//! validation should classify that shape without fully evaluating recursive
//! helper applications. The actual constraint decision still comes from the
//! inferred result's source constraints.

fn diagnostics_for(source: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_source_code_messages(source)
}

fn ts2344_messages(diagnostics: &[(u32, String)]) -> Vec<&str> {
    diagnostics
        .iter()
        .filter_map(|(code, message)| (*code == 2344).then_some(message.as_str()))
        .collect()
}

#[test]
fn infer_result_alias_application_uses_source_constraint() {
    let diagnostics = diagnostics_for(
        r#"
type Box<T> = { value: T };
type ElementOf<T> = T extends Box<infer U> ? U : never;
type NeedString<T extends string> = T;

type Use<T extends Box<string>> = NeedString<ElementOf<T>>;
"#,
    );

    let ts2344 = ts2344_messages(&diagnostics);
    assert!(
        ts2344.is_empty(),
        "ElementOf<T> should satisfy string through T's Box<string> constraint. Got: {diagnostics:#?}"
    );
}

#[test]
fn infer_result_alias_application_renamed_params() {
    let diagnostics = diagnostics_for(
        r#"
type Wrapper<X> = { item: X };
type Inner<X> = X extends Wrapper<infer Y> ? Y : never;
type NeedNumber<X extends number> = X;

type Use<X extends Wrapper<number>> = NeedNumber<Inner<X>>;
"#,
    );

    let ts2344 = ts2344_messages(&diagnostics);
    assert!(
        ts2344.is_empty(),
        "Inner<X> should satisfy number through X's Wrapper<number> constraint. Got: {diagnostics:#?}"
    );
}

#[test]
fn infer_result_alias_application_still_rejects_mismatched_constraint() {
    let diagnostics = diagnostics_for(
        r#"
type Box<T> = { value: T };
type ElementOf<T> = T extends Box<infer U> ? U : never;
type NeedString<T extends string> = T;

type Bad<T extends Box<number>> = NeedString<ElementOf<T>>;
"#,
    );

    let ts2344 = ts2344_messages(&diagnostics);
    assert_eq!(
        ts2344.len(),
        1,
        "ElementOf<T> from Box<number> should still fail a string constraint. Got: {diagnostics:#?}"
    );
}
