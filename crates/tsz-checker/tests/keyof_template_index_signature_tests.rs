use tsz_checker::test_utils::check_source_strict_codes;

fn ts2322_count(source: &str) -> usize {
    check_source_strict_codes(source)
        .into_iter()
        .filter(|code| *code == 2322)
        .count()
}

#[test]
fn keyof_template_index_signature_rejects_non_matching_string_and_number() {
    let source = r#"
type Tmpl = { [key: `prefix_${string}`]: boolean };
type K = keyof Tmpl;

let good: K = "prefix_hello";
let bad: K = "nope";
let num: K = 42;
"#;

    assert_eq!(
        ts2322_count(source),
        2,
        "`keyof` of a template index signature must reject non-matching strings and numbers"
    );
}

#[test]
fn keyof_template_index_signature_pattern_is_not_name_dependent() {
    let source = r#"
type Env = { [prop: `env:${string}`]: string };
type EnvKey = keyof Env;

let good: EnvKey = "env:PATH";
let bad: EnvKey = "prefix_PATH";
"#;

    assert_eq!(
        ts2322_count(source),
        1,
        "`keyof` must preserve the template pattern regardless of the index parameter name"
    );
}

#[test]
fn keyof_plain_string_index_signature_still_accepts_number_keys() {
    let source = r#"
type Dict = { [key: string]: boolean };
type K = keyof Dict;

let goodString: K = "anything";
let goodNumber: K = 42;
"#;

    assert_eq!(
        ts2322_count(source),
        0,
        "plain string index signatures still produce string | number keys"
    );
}
