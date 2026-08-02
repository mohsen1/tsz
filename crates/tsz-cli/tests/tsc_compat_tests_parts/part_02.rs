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

/// TS 7.0.2 dropped JS constructor-function inference: a bare JSDoc type
/// reference to a function-valued binding (function declaration, function
/// expression in a var, arrow function, require-destructured function
/// export) or to a class-EXPRESSION variable is TS2749 value-used-as-type,
/// while class declarations, require-destructured class exports,
/// whole-module require variables, and `typeof` queries keep resolving.
/// Differential against the pinned tsc; binder names vary per case.
#[test]
fn tsc_parity_jsdoc_ctor_fn_value_as_type_matrix() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("jsdoc_ctor_fn_matrix").expect("temp dir");
    write_file(
        &temp.path.join("fndecl.js"),
        "function Gadget() { this.x = 1; }\n/** @param {Gadget} p */\nfunction useG(p) { p.x; }\n",
    );
    write_file(
        &temp.path.join("varfn.js"),
        "var Fixture = function () { this.y = 1; };\n/** @param {Fixture} p */\nfunction useF(p) { p.y; }\n",
    );
    write_file(
        &temp.path.join("arrowfn.js"),
        "var makeIt = (n) => ({ v: n });\n/** @param {makeIt} p */\nfunction useA(p) { p.v; }\n",
    );
    write_file(
        &temp.path.join("classexpr.js"),
        "const Crate = class { constructor() { this.w = 1; } };\n/** @param {Crate} p */\nfunction useC(p) { p.w; }\n",
    );
    write_file(
        &temp.path.join("classdecl.js"),
        "class Hull { constructor() { this.k = 1; } }\n/** @param {Hull} p */\nfunction useH(p) { p.k; }\n",
    );
    write_file(
        &temp.path.join("typeofok.js"),
        "var build = function () { return 1; };\n/** @param {typeof build} p */\nfunction useT(p) { var n = p(); }\n",
    );
    write_file(
        &temp.path.join("dep-fn.js"),
        "exports.assemble = function () {\n    this.q = 1;\n};\n",
    );
    write_file(
        &temp.path.join("usedep-fn.js"),
        "const { assemble } = require(\"./dep-fn\");\n/** @param {assemble} p */\nfunction useD(p) { p.q; }\n",
    );
    write_file(
        &temp.path.join("dep-class.js"),
        "exports.Rig = class {\n    constructor() {\n        this.r = 1;\n    }\n};\n",
    );
    write_file(
        &temp.path.join("usedep-class.js"),
        "const { Rig } = require(\"./dep-class\");\n/** @param {Rig} p */\nfunction useR(p) { p.r; }\n",
    );
    write_file(
        &temp.path.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "allowJs": true,
    "checkJs": true,
    "noEmit": true,
    "module": "commonjs",
    "strict": true
  },
  "include": ["*.js"]
}"#,
    );
    assert_tsc_tsz_match(
        &temp.path,
        &["-p", "tsconfig.json", "--pretty", "false"],
        "JSDoc ctor-fn value-as-type matrix",
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
    // tsc 7.0.2 reports the missing build tsconfig as TS6053 "File ... not
    // found." (the 6.x TS5083 "Cannot read file" is gone) and exits 0.
    assert!(
        tsc_out.contains("TS6053") || tsc_out.contains("not found"),
        "tsc should report missing file: {tsc_out}"
    );
    assert!(
        tsz_out.contains("TS6053") || tsz_out.contains("not found"),
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
fn target_es3_reports_ts6046() {
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
        output.contains("TS6046"),
        "--target ES3 should produce TS6046: {output}"
    );
    assert!(
        output.contains("Argument for '--target' option must be"),
        "--target ES3 should use the invalid-value diagnostic: {output}"
    );
    assert!(
        !output.contains("TS5108"),
        "--target ES3 must not use the removed-value diagnostic: {output}"
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
fn ts6046_module_resolution_hint_omits_deprecated_modes() {
    // tsc 6.x lists only the non-deprecated modes in the TS6046 hint. This runs tsz
    // standalone (no tsc required), so it guards the exact hint text in CI regardless of
    // whether a PATH `tsc` is present.
    let temp = TempDir::new("ts6046_modres_hint").expect("temp dir");
    write_file(&temp.path.join("test.ts"), "export {};\n");
    let (code, output) =
        run_tsz_with_exit_code(&temp.path, &["--moduleResolution", "badValue", "test.ts"])
            .expect("tsz binary not found");
    assert_ne!(
        code, 0,
        "invalid --moduleResolution should be a non-zero exit: {output}"
    );
    assert!(
        output.contains(
            "error TS6046: Argument for '--moduleResolution' option must be: 'node16', 'nodenext', 'bundler'."
        ),
        "TS6046 hint should list only the non-deprecated modes: {output}"
    );
    assert!(
        !output.contains("'node10'") && !output.contains("'classic'"),
        "TS6046 hint must not list the deprecated node10/classic modes: {output}"
    );
}

#[test]
fn deprecated_module_resolution_node10_accepted() {
    // node10/classic are dropped from the TS6046 hint but remain accepted as input
    // (tsc keeps them in the value set, warning via TS5107). tsz must not reject them.
    let temp = TempDir::new("deprecated_modres_node10").expect("temp dir");
    write_file(&temp.path.join("test.ts"), "export const x = 1;\n");
    let (_code, output) = run_tsz_with_exit_code(
        &temp.path,
        &[
            "--noEmit",
            "--pretty",
            "false",
            "--moduleResolution",
            "node10",
            "test.ts",
        ],
    )
    .expect("tsz binary not found");
    assert!(
        !output.contains("TS6046"),
        "Deprecated --moduleResolution node10 should not produce TS6046: {output}"
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

/// tsc 7.0.2 exits `DiagnosticsPresent_OutputsSkipped` (1) when `noEmit`
/// comes from tsconfig.json and the program has errors: nothing was
/// written, so the outputs-generated code (2) of the 6.x era no longer
/// applies. CLI-driven and config-driven `noEmit` agree (companion below).
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
        code, 1,
        "tsconfig noEmit with errors should exit 1 (DiagnosticsPresent_OutputsSkipped), got {code}\n{output}"
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
        code, 1,
        "CLI --noEmit with errors should exit 1 (DiagnosticsPresent_OutputsSkipped), got {code}\n{output}"
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

#[test]
fn exported_value_only_lib_type_shadow_in_mapped_reducer_has_no_ts2749() {
    let temp = TempDir::new("exported_value_only_lib_type_shadow").expect("temp dir");
    write_file(
        &temp.path.join("test.ts"),
        r#"
export declare const Readonly: unique symbol;
export declare const Optional: unique symbol;
export interface TSchema {
  [Readonly]?: string
  [Optional]?: string
  params: unknown[]
  static: unknown
}
export type TReadonly<T extends TSchema> = T & { [Readonly]: 'Readonly' }
export type TOptional<T extends TSchema> = T & { [Optional]: 'Optional' }
export type TPropertyKey = string | number
export type TProperties = Record<TPropertyKey, TSchema>
export type ReadonlyPropertyKeys<T extends TProperties> = {
  [K in keyof T]: T[K] extends TReadonly<TSchema>
    ? (T[K] extends TOptional<T[K]> ? never : K)
    : never
}[keyof T]
export type OptionalPropertyKeys<T extends TProperties> = {
  [K in keyof T]: T[K] extends TOptional<TSchema>
    ? (T[K] extends TReadonly<T[K]> ? never : K)
    : never
}[keyof T]
export type PropertiesReducer<T extends TProperties, R extends Record<keyof any, unknown>> =
  Readonly<Partial<Pick<R, ReadonlyPropertyKeys<T>>>> &
  Partial<Pick<R, OptionalPropertyKeys<T>>>
export type PropertiesReduce<T extends TProperties> = PropertiesReducer<T, {
  [K in keyof T]: T[K]['static']
}>
"#,
    );

    let (tsz_code, tsz_output) = run_tsz_with_exit_code(
        &temp.path,
        &["--noEmit", "--strict", "--target", "es2015", "test.ts"],
    )
    .expect("tsz should run");

    assert_eq!(
        tsz_code, 0,
        "tsz must not emit TS2749 for VALUE-only exported Readonly shadowing lib type alias\n\
         tsz output:\n{tsz_output}"
    );
}

#[test]
fn imported_conditional_select_object_map_satisfies_string_constraint() {
    let temp = TempDir::new("imported_conditional_select_object_map").expect("temp dir");
    write_file(
        &temp.path.join("Any/Key.ts"),
        "export type Key = string | number | symbol\n",
    );
    write_file(
        &temp.path.join("Any/_Internal.ts"),
        "export type Match = 'default' | 'contains->' | 'extends->' | '<-contains' | '<-extends' | 'equals'\n",
    );
    write_file(
        &temp.path.join("Any/Extends.ts"),
        r#"
export type Extends<A1 extends any, A2 extends any> =
  [A1] extends [never] ? 0 : A1 extends A2 ? 1 : 0
"#,
    );
    write_file(
        &temp.path.join("Any/Contains.ts"),
        r#"
import {Extends} from './Extends'
export type Contains<A1 extends any, A2 extends any> =
  Extends<A1, A2> extends 1 ? 1 : 0
"#,
    );
    write_file(
        &temp.path.join("Any/Equals.ts"),
        r#"
export type Equals<A1 extends any, A2 extends any> =
  (<A>() => A extends A2 ? 1 : 0) extends (<A>() => A extends A1 ? 1 : 0)
    ? 1
    : 0
"#,
    );
    write_file(
        &temp.path.join("Any/Is.ts"),
        r#"
import {Match} from './_Internal'
import {Extends} from './Extends'
import {Equals} from './Equals'
import {Contains} from './Contains'

export type Is<A extends any, A1 extends any, match extends Match = 'default'> = {
  'default': Extends<A, A1>
  'contains->': Contains<A, A1>
  'extends->': Extends<A, A1>
  '<-contains': Contains<A1, A>
  '<-extends': Extends<A1, A>
  'equals': Equals<A1, A>
}[match]
"#,
    );
    write_file(
        &temp.path.join("List/List.ts"),
        "export type List<A = any> = ReadonlyArray<A>\n",
    );
    write_file(
        &temp.path.join("List/Head.ts"),
        r#"
import {List} from './List'
export type Head<L extends List> = L extends readonly [] ? never : L[0]
"#,
    );
    write_file(
        &temp.path.join("List/Tail.ts"),
        r#"
import {List} from './List'
export type Tail<L extends List> =
  L extends readonly [] ? L : L extends readonly [any?, ...infer LTail] ? LTail : L
"#,
    );
    write_file(
        &temp.path.join("List/Pop.ts"),
        r#"
import {List} from './List'
export type Pop<L extends List> =
  L extends (readonly [...infer LBody, any] | readonly [...infer LBody, any?]) ? LBody : L
"#,
    );
    write_file(
        &temp.path.join("Object/Path.ts"),
        r#"
import {Key} from '../Any/Key'
import {List} from '../List/List'

export type Path<O, P extends List<Key>> =
  P extends readonly []
    ? O
    : P extends readonly [infer K, ...infer R]
      ? K extends keyof O ? Path<O[K], Extract<R, List<Key>>> : never
      : O
"#,
    );
    write_file(
        &temp.path.join("Object/UnionOf.ts"),
        r#"
export type UnionOf<O extends object> =
  O extends unknown ? O[keyof O] : never
"#,
    );
    write_file(
        &temp.path.join("Union/Select.ts"),
        r#"
import {Is} from '../Any/Is'
import {Match} from '../Any/_Internal'

export type Select<U extends any, M extends any, match extends Match = 'default'> =
  U extends unknown ? {1: U & M, 0: never}[Is<U, M, match>] : never
"#,
    );
    write_file(
        &temp.path.join("String/Join.ts"),
        r#"
import {List} from '../List/List'
export type Join<T extends List, D extends string = ''> = string
"#,
    );
    write_file(
        &temp.path.join("String/Split.ts"),
        "export type Split<S extends string, D extends string = ''> = string[]\n",
    );
    write_file(
        &temp.path.join("Function/AutoPath.ts"),
        r#"
import {Key} from '../Any/Key'
import {Head} from '../List/Head'
import {List} from '../List/List'
import {Pop} from '../List/Pop'
import {Tail} from '../List/Tail'
import {Path} from '../Object/Path'
import {UnionOf} from '../Object/UnionOf'
import {Select} from '../Union/Select'
import {Join} from '../String/Join'
import {Split} from '../String/Split'

type Index = number | string;
type KeyToIndex<K extends Key, SP extends List<Index>> =
  number extends K ? Head<SP> : K & Index;
type MetaPath<O, D extends string, SP extends List<Index> = [], P extends List<Index> = []> = {
  [K in keyof O]:
    | MetaPath<O[K], D, Tail<SP>, [...P, KeyToIndex<K, SP>]>
    | Join<[...P, KeyToIndex<K, SP>], D>;
};
type NextPath<OP> = Select<UnionOf<Exclude<OP, string> & {}>, string>;
type ExecPath<A, SP extends List<Index>, Delimiter extends string> =
  NextPath<Path<MetaPath<A, Delimiter, SP>, SP>>;
type HintPath<A, P extends string, SP extends List<Index>, Exec extends string, D extends string> =
  [Exec] extends [never] ? ExecPath<A, Pop<SP>, D> : Exec | P;
type _AutoPath<A, P extends string, D extends string, SP extends List<Index> = Split<P, D>> =
  HintPath<A, P, SP, ExecPath<A, SP, D>, D>;
export type AutoPath<O extends any, P extends string, D extends string = '.'> =
  _AutoPath<O, P, D>;
"#,
    );

    let (tsc_code, tsc_output) = run_tsc_with_exit_code(
        &temp.path,
        &[
            "--noEmit",
            "--strict",
            "--target",
            "es2022",
            "Function/AutoPath.ts",
        ],
    )
    .expect("tsc should run");
    assert_eq!(
        tsc_code, 0,
        "tsc accepted the imported Select shape: {tsc_output}"
    );

    let (tsz_code, tsz_output) = run_tsz_with_exit_code(
        &temp.path,
        &[
            "--noEmit",
            "--strict",
            "--target",
            "es2022",
            "Function/AutoPath.ts",
        ],
    )
    .expect("tsz should run");
    assert_eq!(
        tsz_code, 0,
        "Conditional object-map filters must satisfy the string constraint like tsc.\n\
         tsz output:\n{tsz_output}"
    );
}

#[test]
fn imported_recursive_iteration_map_satisfies_iteration_constraint() {
    let temp = TempDir::new("imported_recursive_iteration_map").expect("temp dir");
    write_file(
        &temp.path.join("Iteration/Iteration.ts"),
        r#"
export type Iteration = [
  value: number,
  sign: '-' | '0' | '+',
  prev: keyof IterationMap,
  next: keyof IterationMap,
  oppo: keyof IterationMap,
];
export type IterationMap = {
  '__': [number, '-' | '0' | '+', '__', '__', '__'],
  '-1': [-1, '-', '__', '0', '1'],
  '0': [0, '0', '-1', '1', '0'],
  '1': [1, '+', '0', '__', '-1'],
};
"#,
    );
    write_file(
        &temp.path.join("Iteration/IterationOf.ts"),
        r#"
import {IterationMap} from './Iteration'
export type IterationOf<N extends number> =
  `${N}` extends keyof IterationMap ? IterationMap[`${N}`] : IterationMap['__']
"#,
    );
    write_file(
        &temp.path.join("Iteration/Prev.ts"),
        r#"
import {Iteration, IterationMap} from './Iteration'
export type Prev<I extends Iteration> = IterationMap[I[2]]
"#,
    );
    write_file(
        &temp.path.join("Iteration/Next.ts"),
        r#"
import {Iteration, IterationMap} from './Iteration'
export type Next<I extends Iteration> = IterationMap[I[3]]
"#,
    );
    write_file(
        &temp.path.join("Iteration/Pos.ts"),
        r#"
import {Iteration} from './Iteration'
export type Pos<I extends Iteration> = I[0]
"#,
    );
    write_file(
        &temp.path.join("Any/Cast.ts"),
        r#"
export type Cast<A1 extends any, A2 extends any> =
  A1 extends A2 ? A1 : A2
"#,
    );
    write_file(
        &temp.path.join("Number/IsNegative.ts"),
        r#"
import {IterationOf} from '../Iteration/IterationOf'
import {Iteration} from '../Iteration/Iteration'

export type _IsNegative<N extends Iteration> = {
  '-': 1
  '+': 0
  '0': 0
}[N[1]]
export type IsNegative<N extends number> = _IsNegative<IterationOf<N>>
"#,
    );
    write_file(
        &temp.path.join("Number/IsPositive.ts"),
        r#"
import {_IsNegative} from './IsNegative'
import {IterationOf} from '../Iteration/IterationOf'
import {Iteration} from '../Iteration/Iteration'

export type _IsPositive<N extends Iteration> = {
  '-': 0
  '+': 1
  '0': 0
}[N[1]]
export type IsPositive<N extends number> = _IsPositive<IterationOf<N>>
"#,
    );
    write_file(
        &temp.path.join("Number/Sub.ts"),
        r#"
import {Iteration} from '../Iteration/Iteration'
import {Pos} from '../Iteration/Pos'
import {Prev} from '../Iteration/Prev'
import {Next} from '../Iteration/Next'
import {_IsNegative} from './IsNegative'
import {Cast} from '../Any/Cast'

type SubPositive<N1 extends Iteration, N2 extends Iteration> = {
  0: SubPositive<Prev<N1>, Prev<N2>>
  1: N1
  2: number
}[Pos<N2> extends 0 ? 1 : number extends Pos<N2> ? 2 : 0] extends infer X
  ? Cast<X, Iteration>
  : never

type SubNegative<N1 extends Iteration, N2 extends Iteration> = {
  0: SubNegative<Next<N1>, Next<N2>>
  1: N1
  2: number
}[Pos<N2> extends 0 ? 1 : number extends Pos<N2> ? 2 : 0] extends infer X
  ? Cast<X, Iteration>
  : never

export type _Sub<N1 extends Iteration, N2 extends Iteration> = {
  0: SubPositive<N1, N2>
  1: SubNegative<N1, N2>
}[_IsNegative<N2>]
"#,
    );
    write_file(
        &temp.path.join("Number/Greater.ts"),
        r#"
import {_Sub} from './Sub'
import {_IsPositive} from './IsPositive'
import {IterationOf} from '../Iteration/IterationOf'
import {Iteration} from '../Iteration/Iteration'

export type _Greater<N1 extends Iteration, N2 extends Iteration> =
  _IsPositive<_Sub<N1, N2>>
export type Greater<N1 extends number, N2 extends number> =
  N1 extends unknown
    ? N2 extends unknown
      ? _Greater<IterationOf<N1>, IterationOf<N2>>
      : never
    : never
"#,
    );

    let (tsc_code, tsc_output) = run_tsc_with_exit_code(
        &temp.path,
        &[
            "--noEmit",
            "--strict",
            "--target",
            "es2022",
            "Number/Greater.ts",
        ],
    )
    .expect("tsc should run");
    assert_eq!(
        tsc_code, 0,
        "tsc accepted the imported iteration map: {tsc_output}"
    );

    let (tsz_code, tsz_output) = run_tsz_with_exit_code(
        &temp.path,
        &[
            "--noEmit",
            "--strict",
            "--target",
            "es2022",
            "Number/Greater.ts",
        ],
    )
    .expect("tsz should run");
    assert_eq!(
        tsz_code, 0,
        "Imported recursive object-map aliases should satisfy the Iteration constraint like tsc.\n\
         tsz output:\n{tsz_output}"
    );
}

/// Build a project whose `node_modules/<pkg>` has a JS entry point and no
/// declaration file, so the specifier resolves to an *untyped* module.
///
/// Binder name (`pkg`) and augmented member name are parameters rather than
/// literals so the adjacent cases below vary them: the augmentation rule is
/// structural (resolution extension), never name-keyed.
fn write_untyped_package_project(root: &std::path::Path, pkg: &str, source: &str) {
    write_file(
        &root.join(format!("node_modules/{pkg}/index.js")),
        "module.exports = {};\n",
    );
    write_file(
        &root.join(format!("node_modules/{pkg}/package.json")),
        &format!("{{ \"name\": \"{pkg}\", \"version\": \"1.0.0\", \"main\": \"index.js\" }}\n"),
    );
    write_file(&root.join("a.ts"), source);
    write_file(
        &root.join("tsconfig.json"),
        "{ \"compilerOptions\": { \"module\": \"commonjs\", \"strict\": true, \"types\": [] }, \"files\": [\"a.ts\"] }\n",
    );
}

#[test]
fn augmenting_untyped_node_modules_package_reports_ts2665_not_ts2664() {
    let temp = TempDir::new("augment_untyped_module_ts2665").expect("temp dir");
    write_untyped_package_project(
        &temp.path,
        "widgetlib",
        "declare module \"widgetlib\" { export const x: number; }\nimport { x } from \"widgetlib\";\nx;\n",
    );

    let Some((code, output)) = run_tsz_with_exit_code(
        &temp.path,
        &["-p", ".", "--noEmit", "--pretty", "false"],
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert_ne!(code, 0, "augmenting an untyped module should fail:\n{output}");
    assert!(
        output.contains("error TS2665: Invalid module name in augmentation. Module 'widgetlib' resolves to an untyped module at "),
        "expected TS2665 for an untyped augmentation target, got:\n{output}"
    );
    assert!(
        output.contains("node_modules/widgetlib/index.js', which cannot be augmented."),
        "TS2665 must name the resolved JS file, got:\n{output}"
    );
    // The pre-fix behaviour: `noImplicitAny` makes the driver record a TS7016
    // resolution error, the specifier never enters `resolved_module_specifiers`,
    // and the augmentation drew a *wrong* TS2664. tsc reports TS2665 only.
    assert!(
        !output.contains("error TS2664"),
        "TS2664 and TS2665 are mutually exclusive for this shape, got:\n{output}"
    );
    // TS7016 at the import site is independent and must survive.
    assert!(
        output.contains("error TS7016"),
        "the import site keeps its own TS7016 under noImplicitAny, got:\n{output}"
    );
}

#[test]
fn augmenting_untyped_node_modules_subpath_reports_ts2665() {
    let temp = TempDir::new("augment_untyped_subpath_ts2665").expect("temp dir");
    // Renamed binder plus a nested subpath target: the resolved file is not the
    // package entry point, so the message must carry the subpath's own path.
    write_file(
        &temp.path.join("node_modules/toolkit/lib/inner.js"),
        "module.exports = {};\n",
    );
    write_file(
        &temp.path.join("node_modules/toolkit/package.json"),
        "{ \"name\": \"toolkit\", \"version\": \"1.0.0\", \"main\": \"index.js\" }\n",
    );
    write_file(
        &temp.path.join("a.ts"),
        "declare module \"toolkit/lib/inner\" { export const y: string; }\nexport {};\n",
    );
    write_file(
        &temp.path.join("tsconfig.json"),
        "{ \"compilerOptions\": { \"module\": \"commonjs\", \"strict\": false, \"types\": [] }, \"files\": [\"a.ts\"] }\n",
    );

    let Some((code, output)) = run_tsz_with_exit_code(
        &temp.path,
        &["-p", ".", "--noEmit", "--pretty", "false"],
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert_ne!(code, 0, "augmenting an untyped subpath should fail:\n{output}");
    assert!(
        output.contains("error TS2665: Invalid module name in augmentation. Module 'toolkit/lib/inner' resolves to an untyped module at "),
        "expected TS2665 naming the subpath specifier, got:\n{output}"
    );
    assert!(
        output.contains("node_modules/toolkit/lib/inner.js', which cannot be augmented."),
        "TS2665 must name the resolved subpath file, not the package entry, got:\n{output}"
    );
}

#[test]
fn augmenting_typed_node_modules_package_reports_nothing() {
    // Negative control: the same project shape with a declaration file present.
    // Augmenting a typed module is legal, so neither TS2665 nor TS2664 fires.
    let temp = TempDir::new("augment_typed_module_clean").expect("temp dir");
    write_untyped_package_project(
        &temp.path,
        "widgetlib",
        "declare module \"widgetlib\" { export const x: number; }\nimport { x } from \"widgetlib\";\nx;\n",
    );
    write_file(
        &temp.path.join("node_modules/widgetlib/index.d.ts"),
        "export declare const z: string;\n",
    );
    write_file(
        &temp.path.join("node_modules/widgetlib/package.json"),
        "{ \"name\": \"widgetlib\", \"version\": \"1.0.0\", \"main\": \"index.js\", \"types\": \"index.d.ts\" }\n",
    );

    let Some((code, output)) = run_tsz_with_exit_code(
        &temp.path,
        &["-p", ".", "--noEmit", "--pretty", "false"],
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert_eq!(
        code, 0,
        "augmenting a typed package is legal and must stay clean:\n{output}"
    );
    assert!(
        !output.contains("TS2665") && !output.contains("TS2664"),
        "no augmentation diagnostic expected for a typed target, got:\n{output}"
    );
}

#[test]
fn ambient_module_declaration_in_script_file_is_not_an_augmentation() {
    // Negative control on the `is_external_module()` gate: with no top-level
    // import/export the file is a script, so `declare module "..."` *declares*
    // an ambient external module rather than augmenting the on-disk package.
    // tsc stays silent even though `node_modules/widgetlib` is untyped.
    let temp = TempDir::new("ambient_module_script_file_clean").expect("temp dir");
    write_untyped_package_project(
        &temp.path,
        "widgetlib",
        "declare module \"widgetlib\" { export const x: number; }\n",
    );

    let Some((code, output)) = run_tsz_with_exit_code(
        &temp.path,
        &["-p", ".", "--noEmit", "--pretty", "false"],
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert_eq!(
        code, 0,
        "an ambient module declaration in a script file must stay clean:\n{output}"
    );
    assert!(
        !output.contains("TS2665"),
        "script-file ambient declaration is not an augmentation, got:\n{output}"
    );
}
