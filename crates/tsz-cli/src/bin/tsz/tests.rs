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

// ==================== Case-insensitive flag normalization ====================

#[test]
fn preprocess_case_insensitive_noemit() {
    let args = vec![
        OsString::from("tsz"),
        OsString::from("--NoEmit"),
        OsString::from("file.ts"),
    ];
    let result = preprocess_args(args);
    assert!(result.iter().any(|a| a == "--noEmit"));
}

#[test]
fn preprocess_case_insensitive_all_caps() {
    let args = vec![
        OsString::from("tsz"),
        OsString::from("--STRICT"),
        OsString::from("file.ts"),
    ];
    let result = preprocess_args(args);
    assert!(result.iter().any(|a| a == "--strict"));
}

#[test]
fn preprocess_case_insensitive_with_value() {
    let args = vec![
        OsString::from("tsz"),
        OsString::from("--Target"),
        OsString::from("ES2020"),
        OsString::from("file.ts"),
    ];
    let result = preprocess_args(args);
    assert!(result.iter().any(|a| a == "--target"));
    // Value should be preserved as-is
    assert!(result.iter().any(|a| a == "ES2020"));
}

#[test]
fn preprocess_case_insensitive_equals_form() {
    let args = vec![
        OsString::from("tsz"),
        OsString::from("--Target=ES2020"),
        OsString::from("file.ts"),
    ];
    let result = preprocess_args(args);
    assert!(result.iter().any(|a| a == "--target=ES2020"));
}

#[test]
fn preprocess_file_paths_not_lowercased() {
    let args = vec![
        OsString::from("tsz"),
        OsString::from("--noEmit"),
        OsString::from("MyFile.ts"),
    ];
    let result = preprocess_args(args);
    assert!(result.iter().any(|a| a == "MyFile.ts"));
}

// ==================== Duplicate flag handling ====================

#[test]
fn preprocess_duplicate_boolean_flags() {
    let args = vec![
        OsString::from("tsz"),
        OsString::from("--strict"),
        OsString::from("--strict"),
        OsString::from("file.ts"),
    ];
    let result = preprocess_args(args);
    let strict_count = result.iter().filter(|a| *a == "--strict").count();
    assert_eq!(strict_count, 1, "duplicate --strict should be deduplicated");
}

#[test]
fn preprocess_duplicate_valued_flags_last_wins() {
    let args = vec![
        OsString::from("tsz"),
        OsString::from("--target"),
        OsString::from("ES2020"),
        OsString::from("--target"),
        OsString::from("ES2022"),
        OsString::from("file.ts"),
    ];
    let result = preprocess_args(args);
    let target_count = result.iter().filter(|a| *a == "--target").count();
    assert_eq!(target_count, 1, "duplicate --target should be deduplicated");
    // Last value wins
    assert!(result.iter().any(|a| a == "ES2022"));
    assert!(!result.iter().any(|a| a == "ES2020"));
}

// ==================== Boolean true/false value handling ====================

#[test]
fn preprocess_strict_false_removes_flag() {
    let args = vec![
        OsString::from("tsz"),
        OsString::from("--strict"),
        OsString::from("false"),
        OsString::from("file.ts"),
    ];
    let result = preprocess_args(args);
    assert!(
        !result.iter().any(|a| a == "--strict"),
        "--strict false should remove the flag"
    );
    // "false" should NOT appear as a file path
    assert!(
        !result.iter().any(|a| a == "false"),
        "'false' should not be a positional arg"
    );
    // file.ts should still be there
    assert!(result.iter().any(|a| a == "file.ts"));
}

#[test]
fn preprocess_strict_true_keeps_flag() {
    let args = vec![
        OsString::from("tsz"),
        OsString::from("--strict"),
        OsString::from("true"),
        OsString::from("file.ts"),
    ];
    let result = preprocess_args(args);
    assert!(
        result.iter().any(|a| a == "--strict"),
        "--strict true should keep the flag"
    );
    // "true" should NOT appear as a file path
    assert!(
        !result.iter().any(|a| a == "true"),
        "'true' should not be a positional arg"
    );
}

#[test]
fn preprocess_noemit_false_removes_flag() {
    let args = vec![
        OsString::from("tsz"),
        OsString::from("--noEmit"),
        OsString::from("false"),
        OsString::from("file.ts"),
    ];
    let result = preprocess_args(args);
    assert!(
        !result.iter().any(|a| a == "--noEmit"),
        "--noEmit false should remove the flag"
    );
}

#[test]
fn preprocess_non_boolean_false_not_consumed() {
    // --target is not a boolean flag, so "false" should not be consumed
    let args = vec![
        OsString::from("tsz"),
        OsString::from("--outDir"),
        OsString::from("false"),
        OsString::from("file.ts"),
    ];
    let result = preprocess_args(args);
    assert!(result.iter().any(|a| a == "--outDir"));
    assert!(result.iter().any(|a| a == "false"));
}

// ==================== handle_build_clean respects outDir ====================

#[test]
fn build_clean_removes_buildinfo_under_out_dir() {
    use std::fs;
    use tsz_cli::project_refs::ProjectReferenceGraph;

    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();

    let tsconfig_path = root.join("tsconfig.json");
    fs::write(
        &tsconfig_path,
        r#"{"compilerOptions":{"composite":true,"outDir":"dist","declaration":true}}"#,
    )
    .expect("write tsconfig");

    let src_dir = root.join("src");
    fs::create_dir_all(&src_dir).expect("mkdir src");
    fs::write(src_dir.join("index.ts"), "export const x = 1;\n").expect("write entry");

    let dist_dir = root.join("dist");
    fs::create_dir_all(&dist_dir).expect("mkdir dist");
    let buildinfo = dist_dir.join("tsconfig.tsbuildinfo");
    fs::write(&buildinfo, "{}").expect("write buildinfo");

    // Also drop a stray .tsbuildinfo next to the tsconfig so we verify the
    // fix is deleting the correct file (the one under outDir) and not the
    // legacy sibling location.
    let sibling_buildinfo = root.join("tsconfig.tsbuildinfo");
    fs::write(&sibling_buildinfo, "{}").expect("write sibling buildinfo");

    let graph = ProjectReferenceGraph::load(&tsconfig_path).expect("load graph");
    handle_build_clean(&graph, false).expect("clean");

    assert!(
        !buildinfo.exists(),
        "dist/tsconfig.tsbuildinfo should have been deleted"
    );
    assert!(
        !dist_dir.exists(),
        "dist/ directory should have been deleted"
    );
    // The sibling file lives at the legacy location; it is not the build
    // output, so leave it untouched.
    assert!(
        sibling_buildinfo.exists(),
        "sibling tsconfig.tsbuildinfo at project root should be left alone"
    );
}

#[test]
fn build_clean_removes_buildinfo_next_to_tsconfig_when_no_out_dir() {
    use std::fs;
    use tsz_cli::project_refs::ProjectReferenceGraph;

    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();

    let tsconfig_path = root.join("tsconfig.json");
    fs::write(&tsconfig_path, r#"{"compilerOptions":{"composite":true}}"#).expect("write tsconfig");

    let src_dir = root.join("src");
    fs::create_dir_all(&src_dir).expect("mkdir src");
    fs::write(src_dir.join("index.ts"), "export const x = 1;\n").expect("write entry");

    let buildinfo = root.join("tsconfig.tsbuildinfo");
    fs::write(&buildinfo, "{}").expect("write buildinfo");

    let graph = ProjectReferenceGraph::load(&tsconfig_path).expect("load graph");
    handle_build_clean(&graph, false).expect("clean");

    assert!(
        !buildinfo.exists(),
        "tsconfig.tsbuildinfo next to tsconfig should be deleted when no outDir is set"
    );
}
