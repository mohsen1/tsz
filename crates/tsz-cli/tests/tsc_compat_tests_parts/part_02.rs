#[test]
fn tsc_parity_pretty_multiple_ts2304() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("pretty_multi_ts2304").expect("temp dir");
    write_file(
        &temp.path.join("test.ts"),
        "const a = foo;\nconst b = bar;\nconst c = baz;\n",
    );
    assert_tsc_tsz_match(
        &temp.path,
        &["--noEmit", "--pretty", "true", "test.ts"],
        "pretty multiple TS2304",
    );
}

#[test]
fn tsc_parity_pretty_multi_file_summary() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("pretty_multi_file_summary").expect("temp dir");
    write_file(
        &temp.path.join("a.ts"),
        "const a1 = foo;\nconst a2 = bar;\n",
    );
    write_file(&temp.path.join("b.ts"), "const b1 = baz;\n");
    let tsc_out = run_tsc(
        &temp.path,
        &["--noEmit", "--pretty", "true", "a.ts", "b.ts"],
    )
    .expect("tsc failed");
    let tsz_out = run_tsz(
        &temp.path,
        &["--noEmit", "--pretty", "true", "a.ts", "b.ts"],
    )
    .expect("tsz failed");

    // Check the summary table structure matches
    if let Some(diff) = compare_output_structure(&tsc_out, &tsz_out) {
        panic!(
            "pretty multi-file summary structure mismatch:\n{diff}\n\ntsc:\n{tsc_out}\n\ntsz:\n{tsz_out}"
        );
    }

    // Verify "Found N errors in M files" summary text
    let tsc_summary: Vec<&str> = tsc_out
        .lines()
        .filter(|l| l.starts_with("Found "))
        .collect();
    let tsz_summary: Vec<&str> = tsz_out
        .lines()
        .filter(|l| l.starts_with("Found "))
        .collect();
    assert_eq!(
        tsc_summary, tsz_summary,
        "Found summary mismatch:\ntsc: {tsc_summary:?}\ntsz: {tsz_summary:?}"
    );

    // Verify "Errors  Files" table exists in both
    assert!(
        tsc_out.contains("Errors  Files"),
        "tsc missing 'Errors  Files' table"
    );
    assert!(
        tsz_out.contains("Errors  Files"),
        "tsz missing 'Errors  Files' table"
    );
}

#[test]
fn tsc_parity_pretty_double_digit_line() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("pretty_double_digit").expect("temp dir");
    let mut source = String::new();
    for i in 1..=9 {
        source.push_str(&format!("const a{i} = {i};\n"));
    }
    source.push_str("const a10 = unknownVar;\n");
    write_file(&temp.path.join("test.ts"), &source);
    assert_tsc_tsz_match(
        &temp.path,
        &["--noEmit", "--pretty", "true", "test.ts"],
        "pretty double-digit line number",
    );
}

#[test]
fn tsc_parity_pretty_triple_digit_line() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("pretty_triple_digit").expect("temp dir");
    let mut source = String::new();
    for i in 1..=99 {
        source.push_str(&format!("const v{i} = {i};\n"));
    }
    source.push_str("const v100 = unknownVar;\n");
    write_file(&temp.path.join("test.ts"), &source);
    assert_tsc_tsz_match(
        &temp.path,
        &["--noEmit", "--pretty", "true", "test.ts"],
        "pretty triple-digit line number",
    );
}

// ---------------------------------------------------------------------------
// TS2304 with various identifier patterns
// ---------------------------------------------------------------------------

#[test]
fn tsc_parity_ts2304_unicode_identifier() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("ts2304_unicode").expect("temp dir");
    write_file(&temp.path.join("test.ts"), "const x = café;\n");
    assert_tsc_tsz_match(
        &temp.path,
        &["--noEmit", "--pretty", "false", "test.ts"],
        "TS2304 unicode identifier (plain)",
    );
}

#[test]
fn tsc_parity_ts2304_long_identifier() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("ts2304_long_id").expect("temp dir");
    write_file(
        &temp.path.join("test.ts"),
        "const x = thisIsAVeryLongIdentifierNameThatDoesNotExistAnywhere;\n",
    );
    assert_tsc_tsz_match(
        &temp.path,
        &["--noEmit", "--pretty", "false", "test.ts"],
        "TS2304 long identifier (plain)",
    );
}

// ---------------------------------------------------------------------------
// TS2322: type mismatch (plain mode - exact match for error text)
// ---------------------------------------------------------------------------

#[test]
fn tsc_parity_ts2322_plain() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("ts2322_plain").expect("temp dir");
    write_file(
        &temp.path.join("test.ts"),
        "let x: number = \"hello\";\nlet y: string = 42;\n",
    );
    assert_tsc_tsz_match(
        &temp.path,
        &["--noEmit", "--pretty", "false", "test.ts"],
        "TS2322 type mismatch (plain)",
    );
}

// ---------------------------------------------------------------------------
// TS8020: JSDoc types in TypeScript source
// ---------------------------------------------------------------------------

#[test]
fn tsc_parity_jsdoc_constructor_function_suffix() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("ts8020_jsdoc_constructor_suffix").expect("temp dir");
    write_file(
        &temp.path.join("main.ts"),
        "var c: function(new: number): string;\n",
    );
    assert_tsc_tsz_match_with_exit_code(
        &temp.path,
        &["--noEmit", "--pretty", "false", "main.ts"],
        "JSDoc constructor function suffix recovery",
    );
}

// ---------------------------------------------------------------------------
// TS1005: Syntax errors
// ---------------------------------------------------------------------------

#[test]
fn tsc_parity_ts1005_missing_semicolon_plain() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("ts1005_semi").expect("temp dir");
    write_file(&temp.path.join("test.ts"), "const x = 1\nconst y = 2\n");
    assert_tsc_tsz_match(
        &temp.path,
        &["--noEmit", "--pretty", "false", "test.ts"],
        "TS1005 missing semicolon (plain)",
    );
}

// ---------------------------------------------------------------------------
// --build mode: TS5083 missing tsconfig
// ---------------------------------------------------------------------------

#[test]
fn tsc_parity_build_missing_tsconfig() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("build_no_tsconfig").expect("temp dir");
    // --build with a path that doesn't exist
    let (tsc_code, tsc_out) =
        run_tsc_with_exit_code(&temp.path, &["--build", "nonexistent/tsconfig.json"])
            .expect("tsc failed");
    let (tsz_code, tsz_out) =
        run_tsz_with_exit_code(&temp.path, &["--build", "nonexistent/tsconfig.json"])
            .expect("tsz failed");

    assert_eq!(
        tsc_code, tsz_code,
        "build missing tsconfig exit code: tsc={tsc_code}, tsz={tsz_code}"
    );
    // Both should mention TS5083
    assert!(
        tsc_out.contains("TS5083") || tsc_out.contains("Cannot read file"),
        "tsc should report missing file: {tsc_out}"
    );
    assert!(
        tsz_out.contains("TS5083") || tsz_out.contains("Cannot read file"),
        "tsz should report missing file: {tsz_out}"
    );
}

// ---------------------------------------------------------------------------
// Line endings: Windows-style source
// ---------------------------------------------------------------------------

#[test]
fn tsc_parity_windows_line_endings() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("windows_crlf").expect("temp dir");
    write_file(&temp.path.join("test.ts"), "const z = unknownVar;\r\n");
    assert_tsc_tsz_match(
        &temp.path,
        &["--noEmit", "--pretty", "false", "test.ts"],
        "Windows CRLF line endings",
    );
}

// ---------------------------------------------------------------------------
// Multiple error codes in same file
// ---------------------------------------------------------------------------

#[test]
fn tsc_parity_mixed_error_codes_plain() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("mixed_codes").expect("temp dir");
    // TS2304 (undefined name) + TS2322 (type mismatch) in same file
    write_file(
        &temp.path.join("test.ts"),
        "const a = unknownName;\nlet b: number = \"hello\";\n",
    );
    assert_tsc_tsz_match(
        &temp.path,
        &["--noEmit", "--pretty", "false", "test.ts"],
        "mixed error codes (plain)",
    );
}

// ---------------------------------------------------------------------------
// Summary: "Found 1 error" vs "Found N errors"
// ---------------------------------------------------------------------------

#[test]
fn tsc_parity_found_1_error_pretty() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("found_1_error").expect("temp dir");
    write_file(&temp.path.join("test.ts"), "const z = unknownVar;\n");
    let output = assert_tsc_tsz_match(
        &temp.path,
        &["--noEmit", "--pretty", "true", "test.ts"],
        "Found 1 error summary",
    );
    assert!(
        output.contains("Found 1 error"),
        "Should contain 'Found 1 error': {output}"
    );
}

#[test]
fn tsc_parity_found_n_errors_same_file_pretty() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("found_n_errors_same").expect("temp dir");
    write_file(
        &temp.path.join("test.ts"),
        "const a = foo;\nconst b = bar;\n",
    );
    let output = assert_tsc_tsz_match(
        &temp.path,
        &["--noEmit", "--pretty", "true", "test.ts"],
        "Found N errors same file summary",
    );
    assert!(
        output.contains("Found 2 errors in the same file"),
        "Should contain 'Found 2 errors in the same file': {output}"
    );
}

// ---------------------------------------------------------------------------
// Deprecated option values: should still be accepted as input
// ---------------------------------------------------------------------------

// Deprecated values can emit TS5107, but they should still be accepted as
// option values rather than rejected with TS6046.

#[test]
fn deprecated_target_es5_accepted() {
    let temp = TempDir::new("deprecated_es5").expect("temp dir");
    write_file(&temp.path.join("test.ts"), "const x = 1;\n");
    let (_code, output) = run_tsz_with_exit_code(
        &temp.path,
        &[
            "--noEmit", "--pretty", "false", "--target", "es5", "test.ts",
        ],
    )
    .expect("tsz binary not found");
    assert!(
        !output.contains("TS6046"),
        "Deprecated --target es5 should not produce TS6046: {output}"
    );
}

#[test]
fn removed_target_es3_reports_ts5108() {
    let temp = TempDir::new("removed_es3").expect("temp dir");
    write_file(&temp.path.join("test.ts"), "let x: string = 1;\n");
    let (_code, output) = run_tsz_with_exit_code(
        &temp.path,
        &[
            "--noEmit", "--pretty", "false", "--target", "ES3", "test.ts",
        ],
    )
    .expect("tsz binary not found");
    assert!(
        output.contains("TS5108"),
        "Removed --target ES3 should produce TS5108: {output}"
    );
    assert!(
        output.contains("Option 'target=ES3' has been removed"),
        "Removed --target ES3 should use the removed-value diagnostic: {output}"
    );
    assert!(
        !output.contains("TS6046"),
        "Removed --target ES3 should not be rejected as an invalid enum value: {output}"
    );
}

#[test]
fn deprecated_module_amd_accepted() {
    let temp = TempDir::new("deprecated_amd").expect("temp dir");
    write_file(&temp.path.join("test.ts"), "export const x = 1;\n");
    let (_code, output) = run_tsz_with_exit_code(
        &temp.path,
        &[
            "--noEmit", "--pretty", "false", "--module", "amd", "test.ts",
        ],
    )
    .expect("tsz binary not found");
    assert!(
        !output.contains("TS6046"),
        "Deprecated --module amd should not produce TS6046: {output}"
    );
}

#[test]
fn dom_deprecated_tag_name_map_keeps_element_constraint_under_node_merge() {
    let Some(_) = find_tsz_binary() else {
        println!("skipping: tsz binary not found");
        return;
    };
    let temp = TempDir::new("dom_deprecated_tag_name_map").expect("temp dir");
    write_file(
        &temp.path.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "strict": true,
    "noEmit": true,
    "pretty": false,
    "noLib": true
  },
  "files": ["lib.d.ts", "test.ts"]
}
"#,
    );
    write_file(
        &temp.path.join("lib.d.ts"),
        r#"
declare const enum SyntaxKind {
    Modifier,
    Decorator,
}

interface Node {
    kind: SyntaxKind;
}

interface Modifier extends Node { kind: SyntaxKind.Modifier; }
interface Decorator extends Node { kind: SyntaxKind.Decorator; }

interface Element extends Node { tagName: string; }
interface HTMLElement extends Element { id: string; }
interface HTMLUnknownElement extends HTMLElement { unknown: string; }
interface HTMLTrackElement extends HTMLElement { kind: string; }

interface HTMLElementTagNameMap {
    div: HTMLElement;
    track: HTMLTrackElement;
}

interface HTMLElementDeprecatedTagNameMap {
    acronym: HTMLElement;
    applet: HTMLUnknownElement;
}

interface HTMLCollectionOf<T extends Element> {
    item(index: number): T;
}

interface QueryRoot {
    getElementsByTagName<K extends keyof HTMLElementTagNameMap>(
        qualifiedName: K
    ): HTMLCollectionOf<HTMLElementTagNameMap[K]>;
    getElementsByDeprecatedTagName<K extends keyof HTMLElementDeprecatedTagNameMap>(
        qualifiedName: K
    ): HTMLCollectionOf<HTMLElementDeprecatedTagNameMap[K]>;
}
"#,
    );
    write_file(
        &temp.path.join("test.ts"),
        r#"
interface Modifier extends Node { kind: SyntaxKind.Modifier; }
interface Decorator extends Node { kind: SyntaxKind.Decorator; }
"#,
    );

    let (_code, output) = run_tsz_with_exit_code(
        &temp.path,
        &["--project", ".", "--noEmit", "--pretty", "false"],
    )
    .expect("tsz binary not found");
    assert!(
        output.contains("HTMLElementTagNameMap[K]"),
        "regular tag map should still fail because HTMLTrackElement.kind conflicts with merged Node.kind: {output}"
    );
    assert!(
        !output.contains("HTMLElementDeprecatedTagNameMap[K]"),
        "deprecated tag map entries all satisfy Element and should not produce TS2344: {output}"
    );
}

// ---------------------------------------------------------------------------
// TS2427: Interface name reserved-word handling.
//
// tsc only emits ONE TS2427 (for the hard-keyword interface name `void` or
// `null`) when such an interface declaration is present in a file. Other
// reserved-name interfaces (`any`, `number`, etc.) in the SAME file have
// their TS2427 suppressed because tsc's parser produces a parse error for
// the hard-keyword name, which cascade-suppresses the lazy diagnostics for
// the other interface declarations.
// Regression test for the conformance failure on
// `interfacesWithPredefinedTypesAsNames.ts`.
// ---------------------------------------------------------------------------

#[test]
fn tsc_parity_ts2427_void_suppresses_other_predefined_names() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("ts2427_void_suppresses").expect("temp dir");
    write_file(
        &temp.path.join("test.ts"),
        "interface any { }\n\
         interface number { }\n\
         interface string { }\n\
         interface boolean { }\n\
         interface void {}\n\
         interface unknown {}\n\
         interface never {}\n",
    );
    assert_tsc_tsz_match(
        &temp.path,
        &[
            "--target", "es2015", "--noEmit", "--pretty", "false", "test.ts",
        ],
        "TS2427 void hard-keyword suppresses other predefined-name TS2427s",
    );
}

#[test]
fn tsc_parity_ts2427_null_suppresses_other_predefined_names() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("ts2427_null_suppresses").expect("temp dir");
    write_file(
        &temp.path.join("test.ts"),
        "interface any { }\n\
         interface null {}\n",
    );
    assert_tsc_tsz_match(
        &temp.path,
        &[
            "--target", "es2015", "--noEmit", "--pretty", "false", "test.ts",
        ],
        "TS2427 null keeps parser recovery TS1005 while any is suppressed",
    );
}

#[test]
fn tsc_parity_ts2427_any_alone_still_reported() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("ts2427_any_only").expect("temp dir");
    write_file(
        &temp.path.join("test.ts"),
        "interface any { }\n\
         interface number { }\n",
    );
    // Without `void`/`null`, tsc reports TS2427 for both interfaces. This
    // test pins that the suppression only kicks in when a hard-keyword
    // interface name is present.
    assert_tsc_tsz_match(
        &temp.path,
        &[
            "--target", "es2015", "--noEmit", "--pretty", "false", "test.ts",
        ],
        "TS2427 still reported for predefined names when no hard keyword present",
    );
}

/// Regression for #3908: when `noEmit` comes from tsconfig.json (not the
/// CLI flag), tsz must exit with `DiagnosticsPresent_OutputsGenerated` (2),
/// matching tsc. Previously the exit-code branch only consulted the CLI
/// arg, so config-only `noEmit` fell through to the outputs-skipped path
/// (1).
#[test]
fn tsconfig_no_emit_with_errors_exits_outputs_generated() {
    let Some(_) = find_tsz_binary() else {
        println!("skipping: tsz binary not found");
        return;
    };
    let temp = TempDir::new("tsconfig_no_emit_exit_code").expect("temp dir");
    write_file(&temp.path.join("a.ts"), "let x: string = 1;\n");
    write_file(
        &temp.path.join("tsconfig.json"),
        r#"{"compilerOptions":{"noEmit":true},"files":["a.ts"]}"#,
    );

    let (code, output) =
        run_tsz_with_exit_code(&temp.path, &["-p", "tsconfig.json", "--pretty", "false"])
            .expect("tsz should run");
    assert!(
        output.contains("TS2322"),
        "expected TS2322 diagnostic, got:\n{output}"
    );
    assert_eq!(
        code, 2,
        "tsconfig noEmit with errors should exit 2 (DiagnosticsPresent_OutputsGenerated), got {code}\n{output}"
    );
}

/// Companion to the test above: the same program with `--noEmit` on the
/// command line must produce the same exit code. This locks the parity
/// between CLI-driven and tsconfig-driven `noEmit`.
#[test]
fn cli_no_emit_with_errors_exits_outputs_generated() {
    let Some(_) = find_tsz_binary() else {
        println!("skipping: tsz binary not found");
        return;
    };
    let temp = TempDir::new("cli_no_emit_exit_code").expect("temp dir");
    write_file(&temp.path.join("a.ts"), "let x: string = 1;\n");
    write_file(
        &temp.path.join("tsconfig.json"),
        r#"{"compilerOptions":{},"files":["a.ts"]}"#,
    );

    let (code, output) = run_tsz_with_exit_code(
        &temp.path,
        &["-p", "tsconfig.json", "--noEmit", "--pretty", "false"],
    )
    .expect("tsz should run");
    assert!(
        output.contains("TS2322"),
        "expected TS2322 diagnostic, got:\n{output}"
    );
    assert_eq!(
        code, 2,
        "CLI --noEmit with errors should exit 2 (DiagnosticsPresent_OutputsGenerated), got {code}\n{output}"
    );
}

// --- Regression tests for issue #3919 ---
//
// `tsz --showConfig` must print the resolved config without validating root
// files. tsc preserves explicit `files` entries that have unsupported
// extensions or that point at missing paths; tsz used to convert both into
// TS18003 because `discover_ts_files` filtered/rejected them and the empty
// result triggered the "no inputs found" error.

#[test]
fn show_config_preserves_unsupported_extension_in_files() {
    let temp = TempDir::new("show_config_unsupported_extension").expect("temp dir");
    write_file(
        &temp.path.join("tsconfig.json"),
        r#"{"files":["style.css"],"compilerOptions":{"noEmit":true}}"#,
    );
    write_file(&temp.path.join("style.css"), "body{}\n");

    let (code, output) =
        run_tsz_with_exit_code(&temp.path, &["--showConfig"]).expect("tsz should run");
    assert_eq!(
        code, 0,
        "--showConfig must exit 0 with an unsupported-extension files entry, got: {output}"
    );
    assert!(
        !output.contains("error TS18003"),
        "--showConfig must not emit TS18003: {output}"
    );
    assert!(
        !output.contains("error TS6054"),
        "--showConfig must not emit TS6054 (unsupported extension): {output}"
    );
    assert!(
        output.contains("\"./style.css\""),
        "--showConfig must preserve the unsupported file entry verbatim: {output}"
    );
}

#[test]
fn show_config_preserves_missing_file_in_files() {
    let temp = TempDir::new("show_config_missing_file").expect("temp dir");
    write_file(
        &temp.path.join("tsconfig.json"),
        r#"{"files":["missing.ts"],"compilerOptions":{"noEmit":true}}"#,
    );

    let (code, output) =
        run_tsz_with_exit_code(&temp.path, &["--showConfig"]).expect("tsz should run");
    assert_eq!(
        code, 0,
        "--showConfig must exit 0 even when an explicit file is missing, got: {output}"
    );
    assert!(
        !output.contains("error TS18003"),
        "--showConfig must not emit TS18003: {output}"
    );
    assert!(
        !output.contains("error TS6053"),
        "--showConfig must not emit TS6053 (file not found): {output}"
    );
    assert!(
        output.contains("\"./missing.ts\""),
        "--showConfig must preserve the missing file entry verbatim: {output}"
    );
}

#[test]
fn show_config_preserves_files_when_only_unsupported_entries() {
    let temp = TempDir::new("show_config_only_unsupported").expect("temp dir");
    write_file(
        &temp.path.join("tsconfig.json"),
        r#"{"files":["a.css","b.scss"],"compilerOptions":{"noEmit":true}}"#,
    );
    write_file(&temp.path.join("a.css"), "/*a*/\n");
    write_file(&temp.path.join("b.scss"), "/*b*/\n");

    let (code, output) =
        run_tsz_with_exit_code(&temp.path, &["--showConfig"]).expect("tsz should run");
    assert_eq!(
        code, 0,
        "--showConfig must exit 0 when every explicit file has an unsupported extension, got: {output}"
    );
    assert!(
        output.contains("\"./a.css\"") && output.contains("\"./b.scss\""),
        "--showConfig must preserve every explicit entry verbatim: {output}"
    );
}

#[test]
fn show_config_normalizes_already_relative_files_entry() {
    // A `./`-prefixed path in tsconfig must round-trip unchanged (no `./././`).
    let temp = TempDir::new("show_config_already_relative").expect("temp dir");
    write_file(
        &temp.path.join("tsconfig.json"),
        r#"{"files":["./main.ts"],"compilerOptions":{"noEmit":true}}"#,
    );
    write_file(&temp.path.join("main.ts"), "export {};\n");

    let (code, output) =
        run_tsz_with_exit_code(&temp.path, &["--showConfig"]).expect("tsz should run");
    assert_eq!(code, 0, "--showConfig must exit 0, got: {output}");
    assert!(
        output.contains("\"./main.ts\""),
        "expected \"./main.ts\" entry: {output}"
    );
    assert!(
        !output.contains("\"././main.ts\""),
        "must not double-prefix already-relative paths: {output}"
    );
}

#[test]
fn tsc_parity_show_config_unsupported_extension_files_entry() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("show_config_parity_unsupported").expect("temp dir");
    write_file(
        &temp.path.join("tsconfig.json"),
        r#"{"files":["style.css"],"compilerOptions":{"noEmit":true}}"#,
    );
    write_file(&temp.path.join("style.css"), "body{}\n");

    assert_tsc_tsz_match_with_exit_code(
        &temp.path,
        &["--showConfig"],
        "tsz --showConfig must match tsc when files lists an unsupported extension",
    );
}

#[test]
fn this_type_predicate_narrows_receiver_property() {
    let temp = TempDir::new("this_predicate_receiver_property").expect("temp dir");
    write_file(
        &temp.path.join("main.ts"),
        r#"
class Container<T> {
  value: T | null = null;

  hasValue(): this is Container<T> & { value: T } {
    return this.value !== null;
  }
}

const container = new Container<number>();

if (container.hasValue()) {
  const value: number = container.value;
}
"#,
    );

    let (code, output) = run_tsz_with_exit_code(
        &temp.path,
        &["--noEmit", "--strict", "--pretty", "false", "main.ts"],
    )
    .expect("tsz should run");

    assert_eq!(
        code, 0,
        "`this is ...` predicates must narrow receiver properties, got: {output}"
    );
}
