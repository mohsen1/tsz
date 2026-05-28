use tsz_checker::test_utils::check_source_strict_messages_without_missing_libs as check_strict;

fn ts2322_messages(source: &str) -> Vec<String> {
    check_strict(source)
        .into_iter()
        .filter_map(|(code, message)| (code == 2322).then_some(message))
        .collect()
}

#[test]
fn optional_homomorphic_mapped_index_access_displays_source_index() {
    let messages = ts2322_messages(
        r#"
type Maybe<T> = { [Q in keyof T]?: T[Q] };
function f<T, U extends T>(x: T, y: Maybe<U>, k: keyof T) {
    y[k] = x[k];
}
"#,
    );

    assert!(
        messages.iter().any(|message| message
            .contains("Type 'T[keyof T]' is not assignable to type 'U[keyof T] | undefined'.")),
        "expected homomorphic optional mapped index display, got: {messages:#?}"
    );
    assert!(
        messages
            .iter()
            .all(|message| !message.contains("Maybe<U>[keyof T]")),
        "diagnostic should not preserve the alias-index spelling, got: {messages:#?}"
    );
}

#[test]
fn readonly_homomorphic_mapped_index_access_displays_source_index() {
    let messages = ts2322_messages(
        r#"
type Frozen<X> = { readonly [R in keyof X]: X[R] };
function f<T, U extends T>(x: T, y: Frozen<U>, k: keyof T) {
    y[k] = x[k];
}
"#,
    );

    assert!(
        messages
            .iter()
            .any(|message| message
                .contains("Type 'T[keyof T]' is not assignable to type 'U[keyof T")),
        "expected readonly homomorphic mapped index display, got: {messages:#?}"
    );
    assert!(
        messages
            .iter()
            .all(|message| !message.contains("Frozen<U>[keyof T]")),
        "diagnostic should not preserve the alias-index spelling, got: {messages:#?}"
    );
}

#[test]
fn inline_homomorphic_mapped_assignment_reports_generic_value_mismatch() {
    let messages = ts2322_messages(
        r#"
function f<T, U extends T>(x: { [P in keyof T]: T[P] }, y: { [P in keyof T]: U[P] }) {
    y = x;
}
"#,
    );

    assert!(
        messages.iter().any(|message| message.contains(
            "Type '{ [P in keyof T]: T[P]; }' is not assignable to type '{ [P in keyof T]: U[P]; }'."
        )),
        "expected inline homomorphic mapped assignment mismatch, got: {messages:#?}"
    );
}

#[test]
fn constrained_key_homomorphic_mapped_assignment_reports_generic_value_mismatch() {
    let messages = ts2322_messages(
        r#"
function f<T, U extends T, K extends keyof T>(x: { [P in K]: T[P] }, y: { [P in K]: U[P] }) {
    y = x;
}
"#,
    );

    assert!(
        messages.iter().any(|message| message.contains(
            "Type '{ [P in K]: T[P]; }' is not assignable to type '{ [P in K]: U[P]; }'."
        )),
        "expected constrained-key homomorphic mapped assignment mismatch, got: {messages:#?}"
    );
}
