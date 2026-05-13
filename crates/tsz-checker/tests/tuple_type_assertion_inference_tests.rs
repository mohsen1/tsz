use tsz_checker::test_utils::check_source_code_messages;

#[test]
fn tuple_type_assertion_preserves_literal_array_element_inference() {
    let source = r#"
declare function f1<T1, T2>(values: [T1[], T2[]]): T1;
declare function f2<T1, T2>(values: readonly [T1[], T2[]]): T1;

let expected: "a";
expected = f1(undefined as ["a"[], "b"[]]);
expected = f2(undefined as ["a"[], "b"[]]);
"#;

    let diagnostics = check_source_code_messages(source);
    assert!(
        diagnostics.iter().all(|(code, message)| {
            *code != 2322 || !message.contains("Type 'string' is not assignable to type '\"a\"'")
        }),
        "asserted tuple source should infer T1 as literal \"a\", got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn individually_const_asserted_array_elements_preserve_literal_union() {
    let source = r#"
const arr = [
  "a" as const,
  "b" as const,
  "c" as const,
];

const elem: "a" | "b" | "c" = arr[0];
"#;

    let diagnostics = check_source_code_messages(source);
    assert!(
        diagnostics.iter().all(|(code, message)| {
            *code != 2322 || !message.contains("Type 'string' is not assignable")
        }),
        "const-asserted array elements should infer a literal union, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn mixed_const_asserted_array_element_still_widens_unasserted_element() {
    let source = r#"
const arr = ["a" as const, "b"];
const elem: "a" | "b" = arr[0];
"#;

    let diagnostics = check_source_code_messages(source);
    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2322 && message.contains("Type 'string' is not assignable")
        }),
        "mixed const and unasserted elements should still widen to string[], got diagnostics: {diagnostics:?}"
    );
}
