use super::*;

#[test]
fn split_response_line_simple() {
    assert_eq!(
        split_response_line("--strict --noEmit"),
        vec!["--strict", "--noEmit"]
    );
}

#[test]
fn split_response_line_double_quoted_spaces() {
    assert_eq!(
        split_response_line(r#"--outDir "my output""#),
        vec!["--outDir", "my output"]
    );
}

#[test]
fn split_response_line_single_quoted_spaces() {
    assert_eq!(
        split_response_line("--outDir 'my output'"),
        vec!["--outDir", "my output"]
    );
}

#[test]
fn split_response_line_single_arg() {
    assert_eq!(split_response_line("--strict"), vec!["--strict"]);
}

#[test]
fn split_response_line_empty() {
    let empty: Vec<String> = Vec::new();
    assert_eq!(split_response_line(""), empty);
}

#[test]
fn split_response_line_only_whitespace() {
    let empty: Vec<String> = Vec::new();
    assert_eq!(split_response_line("   "), empty);
}

#[test]
fn split_response_line_quoted_path_with_spaces() {
    assert_eq!(
        split_response_line(r#"--rootDir "C:\Program Files\project""#),
        vec!["--rootDir", r"C:\Program Files\project"]
    );
}

#[test]
fn split_response_line_multiple_quoted_args() {
    assert_eq!(
        split_response_line(r#""file one.ts" "file two.ts""#),
        vec!["file one.ts", "file two.ts"]
    );
}

#[test]
fn split_response_line_adjacent_quotes() {
    // foo"bar"baz should produce foobarbaz (quotes just delimit, no split)
    assert_eq!(split_response_line(r#"foo"bar"baz"#), vec!["foobarbaz"]);
}
