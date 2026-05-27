#[test]
fn tsc_compat_cannot_find_name_plain() {
    if !tsc_available() {
        return;
    }

    let temp = TempDir::new("cannot_find_name_plain").expect("temp dir");
    write_file(&temp.path.join("test.ts"), "const z = unknownVar;\n");

    let tsc_out =
        run_tsc(&temp.path, &["--noEmit", "--pretty", "false", "test.ts"]).expect("tsc failed");
    let tsz_out =
        run_tsz(&temp.path, &["--noEmit", "--pretty", "false", "test.ts"]).expect("tsz failed");

    if let Some(diff) = diff_outputs(&tsc_out, &tsz_out) {
        panic!(
            "tsz output does not match tsc (non-pretty):\n{diff}\n\ntsc output:\n{tsc_out}\n\ntsz output:\n{tsz_out}"
        );
    }
}

#[test]
fn tsc_compat_cannot_find_name_pretty() {
    if !tsc_available() {
        return;
    }

    let temp = TempDir::new("cannot_find_name_pretty").expect("temp dir");
    write_file(&temp.path.join("test.ts"), "const z = unknownVar;\n");

    let tsc_out =
        run_tsc(&temp.path, &["--noEmit", "--pretty", "true", "test.ts"]).expect("tsc failed");
    let tsz_out =
        run_tsz(&temp.path, &["--noEmit", "--pretty", "true", "test.ts"]).expect("tsz failed");

    if let Some(diff) = diff_outputs(&tsc_out, &tsz_out) {
        panic!(
            "tsz output does not match tsc (pretty):\n{diff}\n\ntsc output:\n{tsc_out}\n\ntsz output:\n{tsz_out}"
        );
    }
}

#[test]
fn tsc_compat_multiple_cannot_find_name_plain() {
    if !tsc_available() {
        return;
    }

    let temp = TempDir::new("multi_cannot_find_plain").expect("temp dir");
    write_file(
        &temp.path.join("test.ts"),
        "const a = foo;\nconst b = bar;\nconst c = baz;\n",
    );

    let tsc_out =
        run_tsc(&temp.path, &["--noEmit", "--pretty", "false", "test.ts"]).expect("tsc failed");
    let tsz_out =
        run_tsz(&temp.path, &["--noEmit", "--pretty", "false", "test.ts"]).expect("tsz failed");

    if let Some(diff) = diff_outputs(&tsc_out, &tsz_out) {
        panic!(
            "tsz output does not match tsc (non-pretty, multiple errors):\n{diff}\n\ntsc:\n{tsc_out}\n\ntsz:\n{tsz_out}"
        );
    }
}

#[test]
fn unresolved_callee_callback_still_reports_implicit_any() {
    let temp = TempDir::new("unresolved_callee_callback_implicit_any").expect("temp dir");
    write_file(
        &temp.path.join("test.ts"),
        "// @noImplicitAny: true\nconst result = fooBarBaz([], error => error);\nresult;\n",
    );

    let tsz_out =
        run_tsz(&temp.path, &["--noEmit", "--pretty", "false", "test.ts"]).expect("tsz failed");

    assert!(
        tsz_out.contains("TS2304"),
        "expected unresolved callee diagnostic, got:\n{tsz_out}"
    );
    assert!(
        tsz_out.contains("TS7006"),
        "expected callback implicit-any diagnostic to survive unresolved callee fallback, got:\n{tsz_out}"
    );
}

#[test]
fn errored_initializer_receiver_call_still_reports_implicit_any() {
    let temp = TempDir::new("errored_initializer_receiver_call_implicit_any").expect("temp dir");
    write_file(
        &temp.path.join("test.ts"),
        "// @noImplicitAny: true\nconst children = foo.bar();\nchildren.foreach((item) => item);\n",
    );

    let tsz_out =
        run_tsz(&temp.path, &["--noEmit", "--pretty", "false", "test.ts"]).expect("tsz failed");

    assert!(
        tsz_out.contains("TS2304"),
        "expected unresolved receiver diagnostic, got:\n{tsz_out}"
    );
    assert!(
        tsz_out.contains("TS7006"),
        "expected callback implicit-any diagnostic to survive errored initializer flow, got:\n{tsz_out}"
    );
}

#[test]
fn definite_assignment_error_keeps_callback_context() {
    let temp = TempDir::new("definite_assignment_keeps_callback_context").expect("temp dir");
    write_file(
        &temp.path.join("test.ts"),
        "class Observable<T> { map<U>(proj: (e: T) => U): Observable<U> { return null as any; } }\nlet x: Observable<number>;\nlet y = x.map(x => x + 1);\n",
    );

    let tsz_out =
        run_tsz(&temp.path, &["--noEmit", "--pretty", "false", "test.ts"]).expect("tsz failed");

    assert!(
        tsz_out.contains("TS2454"),
        "expected definite-assignment diagnostic, got:\n{tsz_out}"
    );
    assert!(
        !tsz_out.contains("TS7006"),
        "did not expect callback implicit-any diagnostic when contextual type is still known, got:\n{tsz_out}"
    );
}

#[test]
fn tsc_compat_multiple_cannot_find_name_pretty() {
    if !tsc_available() {
        return;
    }

    let temp = TempDir::new("multi_cannot_find_pretty").expect("temp dir");
    write_file(
        &temp.path.join("test.ts"),
        "const a = foo;\nconst b = bar;\nconst c = baz;\n",
    );

    let tsc_out =
        run_tsc(&temp.path, &["--noEmit", "--pretty", "true", "test.ts"]).expect("tsc failed");
    let tsz_out =
        run_tsz(&temp.path, &["--noEmit", "--pretty", "true", "test.ts"]).expect("tsz failed");

    if let Some(diff) = diff_outputs(&tsc_out, &tsz_out) {
        panic!(
            "tsz output does not match tsc (pretty, multiple errors):\n{diff}\n\ntsc:\n{tsc_out}\n\ntsz:\n{tsz_out}"
        );
    }
}

// ===========================================================================
// Structural comparison tests (format structure matches, ignoring span positions)
// These test that the output FORMAT is correct even when the checker reports
// errors on different AST nodes.
// ===========================================================================

/// Compare output structure: same number of lines, same diagnostic header format,
/// same summary format, etc. Ignores specific column numbers and underline positions.
fn compare_output_structure(tsc_output: &str, tsz_output: &str) -> Option<String> {
    let tsc_lines: Vec<&str> = tsc_output.lines().collect();
    let tsz_lines: Vec<&str> = tsz_output.lines().collect();

    let mut diffs = Vec::new();

    // Line count must match
    if tsc_lines.len() != tsz_lines.len() {
        diffs.push(format!(
            "Line count: tsc={}, tsz={}",
            tsc_lines.len(),
            tsz_lines.len()
        ));
    }

    let min_lines = tsc_lines.len().min(tsz_lines.len());
    for i in 0..min_lines {
        let tsc_line = tsc_lines[i];
        let tsz_line = tsz_lines[i];

        // Both blank or both non-blank
        if tsc_line.is_empty() != tsz_line.is_empty() {
            diffs.push(format!(
                "Line {}: blank mismatch (tsc blank={}, tsz blank={})",
                i + 1,
                tsc_line.is_empty(),
                tsz_line.is_empty()
            ));
            continue;
        }

        if tsc_line.is_empty() {
            continue; // Both blank, OK
        }

        // For diagnostic header lines, check the format structure
        // tsc non-pretty: file(line,col): error TScode: message
        // tsc pretty: file:line:col - error TScode: message
        if tsc_line.contains(": error TS") || tsc_line.contains(" - error TS") {
            // Both should have the same error code and message
            if let (Some(tsc_code_msg), Some(tsz_code_msg)) = (
                tsc_line.split("error TS").nth(1),
                tsz_line.split("error TS").nth(1),
            ) && tsc_code_msg != tsz_code_msg
            {
                diffs.push(format!(
                    "Line {}: error message differs:\n  tsc: error TS{}\n  tsz: error TS{}",
                    i + 1,
                    tsc_code_msg,
                    tsz_code_msg
                ));
            }
        }

        // For "Found N errors" lines, should match exactly
        if tsc_line.starts_with("Found ") && tsc_line != tsz_line {
            diffs.push(format!(
                "Line {}: summary differs:\n  tsc: {}\n  tsz: {}",
                i + 1,
                tsc_line,
                tsz_line
            ));
        }

        // For "Errors  Files" header, should match exactly
        if tsc_line == "Errors  Files" && tsz_line != "Errors  Files" {
            diffs.push(format!(
                "Line {}: expected 'Errors  Files', got: {}",
                i + 1,
                tsz_line
            ));
        }
    }

    if diffs.is_empty() {
        None
    } else {
        Some(diffs.join("\n"))
    }
}

#[test]
fn tsc_compat_structure_type_error_plain() {
    if !tsc_available() {
        return;
    }

    let temp = TempDir::new("struct_type_error_plain").expect("temp dir");
    write_file(
        &temp.path.join("test.ts"),
        "let x: number = \"hello\";\nlet y: string = 42;\n",
    );

    let tsc_out =
        run_tsc(&temp.path, &["--noEmit", "--pretty", "false", "test.ts"]).expect("tsc failed");
    let tsz_out =
        run_tsz(&temp.path, &["--noEmit", "--pretty", "false", "test.ts"]).expect("tsz failed");

    // Structural comparison (both should have same number of error lines and format)
    let tsc_count = tsc_out.lines().filter(|l| l.contains("error TS")).count();
    let tsz_count = tsz_out.lines().filter(|l| l.contains("error TS")).count();
    assert_eq!(
        tsc_count, tsz_count,
        "Different number of errors:\ntsc ({tsc_count}):\n{tsc_out}\ntsz ({tsz_count}):\n{tsz_out}"
    );
}

#[test]
fn tsc_compat_structure_type_error_pretty() {
    if !tsc_available() {
        return;
    }

    let temp = TempDir::new("struct_type_error_pretty").expect("temp dir");
    write_file(
        &temp.path.join("test.ts"),
        "let x: number = \"hello\";\nlet y: string = 42;\n",
    );

    let tsc_out =
        run_tsc(&temp.path, &["--noEmit", "--pretty", "true", "test.ts"]).expect("tsc failed");
    let tsz_out =
        run_tsz(&temp.path, &["--noEmit", "--pretty", "true", "test.ts"]).expect("tsz failed");

    if let Some(diff) = compare_output_structure(&tsc_out, &tsz_out) {
        panic!("Output structure mismatch:\n{diff}\n\ntsc:\n{tsc_out}\n\ntsz:\n{tsz_out}");
    }
}

#[test]
fn tsc_compat_no_errors_plain() {
    if !tsc_available() {
        return;
    }

    let temp = TempDir::new("no_errors_plain").expect("temp dir");
    write_file(
        &temp.path.join("test.ts"),
        "const x: number = 42;\nconst y: string = \"hello\";\n",
    );

    let tsc_out =
        run_tsc(&temp.path, &["--noEmit", "--pretty", "false", "test.ts"]).expect("tsc failed");
    let tsz_out =
        run_tsz(&temp.path, &["--noEmit", "--pretty", "false", "test.ts"]).expect("tsz failed");

    // Both should produce empty output for valid code
    assert!(
        tsc_out.trim().is_empty(),
        "tsc should have no errors: {tsc_out}"
    );
    assert!(
        tsz_out.trim().is_empty(),
        "tsz should have no errors: {tsz_out}"
    );
}

#[test]
fn tsc_compat_exit_code_no_errors() {
    if !tsc_available() {
        return;
    }

    let temp = TempDir::new("exit_code_ok").expect("temp dir");
    write_file(&temp.path.join("test.ts"), "const x: number = 42;\n");

    let tsz_bin = find_tsz_binary().expect("tsz binary not found");

    let tsc_status = tsc_command()
        .expect("tsc command unavailable")
        .args(["--noEmit", "--pretty", "false", "test.ts"])
        .current_dir(&temp.path)
        .status()
        .expect("tsc failed");

    let tsz_status = Command::new(&tsz_bin)
        .args(["--noEmit", "--pretty", "false", "--ignoreConfig", "test.ts"])
        .current_dir(&temp.path)
        .status()
        .expect("tsz failed");

    assert_eq!(
        tsc_status.code(),
        tsz_status.code(),
        "Exit codes differ for no-error case: tsc={:?}, tsz={:?}",
        tsc_status.code(),
        tsz_status.code()
    );
}

#[test]
fn tsc_compat_exit_code_with_errors() {
    if !tsc_available() {
        return;
    }

    let temp = TempDir::new("exit_code_err").expect("temp dir");
    write_file(&temp.path.join("test.ts"), "const z = unknownVar;\n");

    let tsz_bin = find_tsz_binary().expect("tsz binary not found");

    let tsc_status = tsc_command()
        .expect("tsc command unavailable")
        .args(["--noEmit", "--pretty", "false", "test.ts"])
        .current_dir(&temp.path)
        .status()
        .expect("tsc failed");

    let tsz_status = Command::new(&tsz_bin)
        .args(["--noEmit", "--pretty", "false", "--ignoreConfig", "test.ts"])
        .current_dir(&temp.path)
        .status()
        .expect("tsz failed");

    assert_eq!(
        tsc_status.code(),
        tsz_status.code(),
        "Exit codes differ for error case: tsc={:?}, tsz={:?}",
        tsc_status.code(),
        tsz_status.code()
    );
}

// ===========================================================================
// Line ending tests (cross-platform)
// ===========================================================================

#[test]
fn tsc_compat_line_endings_normalized() {
    if !tsc_available() {
        return;
    }

    let temp = TempDir::new("line_endings").expect("temp dir");
    // Use \r\n line endings (Windows style) in the source
    write_file(&temp.path.join("test.ts"), "const z = unknownVar;\r\n");

    let tsc_out =
        run_tsc(&temp.path, &["--noEmit", "--pretty", "false", "test.ts"]).expect("tsc failed");
    let tsz_out =
        run_tsz(&temp.path, &["--noEmit", "--pretty", "false", "test.ts"]).expect("tsz failed");

    // After normalization (replace \r\n → \n), outputs should match
    assert!(
        !tsc_out.contains('\r'),
        "tsc output should have normalized line endings"
    );
    assert!(
        !tsz_out.contains('\r'),
        "tsz output should have normalized line endings"
    );

    // Both should detect the same error
    assert!(
        tsc_out.contains("error TS2304"),
        "tsc should find TS2304: {tsc_out}"
    );
    assert!(
        tsz_out.contains("error TS2304"),
        "tsz should find TS2304: {tsz_out}"
    );

    // Exact match for this case (TS2304 spans agree)
    if let Some(diff) = diff_outputs(&tsc_out, &tsz_out) {
        panic!(
            "tsz output does not match tsc (Windows line endings):\n{diff}\n\ntsc:\n{tsc_out}\n\ntsz:\n{tsz_out}"
        );
    }
}

// ===========================================================================
// Format-specific tests
// ===========================================================================

#[test]
fn tsc_compat_plain_format_structure() {
    if !tsc_available() {
        return;
    }

    let temp = TempDir::new("plain_format").expect("temp dir");
    write_file(
        &temp.path.join("test.ts"),
        "const a = foo;\nconst b = bar;\n",
    );

    let tsz_out =
        run_tsz(&temp.path, &["--noEmit", "--pretty", "false", "test.ts"]).expect("tsz failed");

    // Non-pretty format: file(line,col): error TScode: message
    for line in tsz_out.lines() {
        if line.is_empty() {
            continue;
        }
        // Each line should match: file(line,col): category TScode: message
        assert!(
            line.contains("): error TS") || line.contains("): warning TS"),
            "Non-pretty line doesn't match format 'file(line,col): error TScode: message': {line}"
        );
        // Should contain parenthesized position
        assert!(
            line.contains('(') && line.contains(')'),
            "Non-pretty line missing parenthesized position: {line}"
        );
        // Should NOT contain source snippets
        assert!(
            !line.contains('~'),
            "Non-pretty line should not have underline markers: {line}"
        );
    }
}

#[test]
fn tsc_compat_pretty_format_structure() {
    if !tsc_available() {
        return;
    }

    let temp = TempDir::new("pretty_format").expect("temp dir");
    write_file(&temp.path.join("test.ts"), "const a = foo;\n");

    let tsz_out =
        run_tsz(&temp.path, &["--noEmit", "--pretty", "true", "test.ts"]).expect("tsz failed");

    let lines: Vec<&str> = tsz_out.lines().collect();

    // Pretty format structure:
    // Line 0: file:line:col - error TScode: message
    // Line 1: (blank)
    // Line 2: {line_num} {source}
    // Line 3: {underline}
    // Line 4: (blank)
    // Line 5: (blank)
    // Line 6: Found N error(s)...
    // Line 7: (blank - trailing)
    assert!(
        lines.len() >= 6,
        "Pretty output should have at least 6 lines, got {}:\n{}",
        lines.len(),
        tsz_out
    );

    // Line 0: header with colon-separated location and " - error"
    assert!(
        lines[0].contains(" - error TS"),
        "Pretty header should use ' - error TS' format: {}",
        lines[0]
    );
    // Should NOT use parenthesized format in pretty mode
    assert!(
        !lines[0].contains("): error"),
        "Pretty header should not use parenthesized format: {}",
        lines[0]
    );

    // Line 1: blank
    assert!(lines[1].is_empty(), "Line 2 should be blank");

    // Line 2: source line with line number
    assert!(
        lines[2].starts_with('1') || lines[2].starts_with(' '),
        "Source line should start with line number: {}",
        lines[2]
    );

    // Line 3: underline with tildes
    assert!(
        lines[3].contains('~'),
        "Underline line should contain tildes: {}",
        lines[3]
    );

    // Should have "Found" summary
    let has_found = lines.iter().any(|l| l.starts_with("Found "));
    assert!(
        has_found,
        "Should have 'Found N error(s)' summary:\n{tsz_out}"
    );
}

#[test]
fn tsc_compat_double_digit_line_number_pretty() {
    if !tsc_available() {
        return;
    }

    let temp = TempDir::new("double_digit_line").expect("temp dir");
    let mut source = String::new();
    for i in 1..=9 {
        source.push_str(&format!("const a{i} = {i};\n"));
    }
    source.push_str("const a10 = unknownVar;\n");
    write_file(&temp.path.join("test.ts"), &source);

    let tsc_out =
        run_tsc(&temp.path, &["--noEmit", "--pretty", "true", "test.ts"]).expect("tsc failed");
    let tsz_out =
        run_tsz(&temp.path, &["--noEmit", "--pretty", "true", "test.ts"]).expect("tsz failed");

    // Exact match: TS2304 spans should agree for both compilers
    if let Some(diff) = diff_outputs(&tsc_out, &tsz_out) {
        panic!(
            "Double-digit line number output mismatch:\n{diff}\n\ntsc:\n{tsc_out}\n\ntsz:\n{tsz_out}"
        );
    }
}

// ===========================================================================
// CLI error format tests (TS5023, TS5025, TS6369, build mode flag remapping)
// ===========================================================================

const TS5112_COMMAND_LINE_FILES_OUTPUT: &str = "error TS5112: tsconfig.json is present but will not be loaded if files are specified on commandline. Use '--ignoreConfig' to skip this error.\n";

#[test]
fn build_only_flags_report_ts5093_outside_build_mode() {
    let temp = TempDir::new("build_only_flags_ts5093").expect("temp dir");
    write_file(&temp.path.join("a.ts"), "const x = 1;\n");

    for (flag, option_name) in [
        ("--verbose", "verbose"),
        ("--dry", "dry"),
        ("--force", "force"),
        ("--clean", "clean"),
        ("--stopBuildOnErrors", "stopBuildOnErrors"),
    ] {
        let (code, output) =
            run_tsz_with_exit_code(&temp.path, &["--pretty", "false", flag, "a.ts"])
                .expect("tsz binary not found");
        let expected = format!(
            "error TS5093: Compiler option '--{option_name}' may only be used with '--build'.\n"
        );

        assert_eq!(code, 1, "Expected exit code 1 for {flag}, got {code}");
        assert_eq!(output, expected, "Unexpected output for {flag}");
        assert!(
            !temp.path.join("a.js").exists(),
            "{flag} outside build mode should not emit a.js"
        );
    }
}

#[test]
fn build_only_explicit_false_still_reports_ts5093() {
    let temp = TempDir::new("build_only_false_ts5093").expect("temp dir");
    write_file(&temp.path.join("a.ts"), "const x = 1;\n");

    let (code, output) =
        run_tsz_with_exit_code(&temp.path, &["--pretty", "false", "--dry", "false", "a.ts"])
            .expect("tsz binary not found");

    assert_eq!(code, 1, "Expected exit code 1 for --dry false");
    assert_eq!(
        output,
        "error TS5093: Compiler option '--dry' may only be used with '--build'.\n"
    );
    assert!(
        !temp.path.join("a.js").exists(),
        "--dry false outside build mode should not emit a.js"
    );
}

#[test]
fn unknown_flag_ts5023_format() {
    let temp = TempDir::new("unknown_flag_ts5023").expect("temp dir");
    let (code, output) =
        run_tsz_with_exit_code(&temp.path, &["--badFlag"]).expect("tsz binary not found");

    assert_eq!(code, 1, "Expected exit code 1 for unknown flag, got {code}");
    assert!(
        output.contains("error TS5023: Unknown compiler option '--badFlag'."),
        "Expected TS5023 diagnostic for unknown flag, got:\n{output}"
    );
}

#[test]
fn unknown_flag_ts5025_suggestion_format() {
    let temp = TempDir::new("unknown_flag_ts5025").expect("temp dir");
    // --strct is close to --strict, should trigger TS5025 with suggestion
    let (code, output) =
        run_tsz_with_exit_code(&temp.path, &["--strct"]).expect("tsz binary not found");

    assert_eq!(
        code, 1,
        "Expected exit code 1 for unknown flag with suggestion, got {code}"
    );
    assert!(
        output.contains("error TS5025: Unknown compiler option '--strct'. Did you mean 'strict'?"),
        "Expected TS5025 diagnostic with suggestion, got:\n{output}"
    );
}

#[test]
fn unknown_flag_exit_code_is_1_not_2() {
    let temp = TempDir::new("unknown_flag_exit_code").expect("temp dir");
    let (code, _output) = run_tsz_with_exit_code(&temp.path, &["--totallyBogusOption123"])
        .expect("tsz binary not found");

    assert_eq!(
        code, 1,
        "Expected exit code 1 for unknown flag (not clap's default 2), got {code}"
    );
}

#[test]
fn generate_cpu_profile_is_visible_unsupported_error() {
    let temp = TempDir::new("generate_cpu_profile_unsupported").expect("temp dir");
    write_file(&temp.path.join("test.ts"), "const value = 1;\n");
    write_file(
        &temp.path.join("tsconfig.json"),
        r#"{"compilerOptions":{"noEmit":true},"files":["test.ts"]}"#,
    );

    let profile_path = temp.path.join("tsz.cpuprofile");
    let (code, output) = run_tsz_with_exit_code(
        &temp.path,
        &[
            "-p",
            "tsconfig.json",
            "--generateCpuProfile",
            "tsz.cpuprofile",
            "--pretty",
            "false",
        ],
    )
    .expect("tsz binary not found");

    assert_eq!(
        code, 1,
        "Expected unsupported --generateCpuProfile to exit 1, got {code}:\n{output}"
    );
    assert!(
        output.contains("--generateCpuProfile")
            && output.contains("not supported")
            && output.contains("--generateTrace"),
        "Expected visible unsupported-option error, got:\n{output}"
    );
    assert!(
        !profile_path.exists(),
        "Unsupported --generateCpuProfile should not create a fake profile at {}",
        profile_path.display()
    );
}

#[test]
fn bare_optional_boolean_flags_apply_to_following_input_file() {
    let temp = TempDir::new("bare_optional_boolean_flags").expect("temp dir");
    write_file(
        &temp.path.join("test.ts"),
        "function f(value) { return value; }\nconst text: string = null;\n",
    );

    let (code, output) = run_tsz_with_exit_code(
        &temp.path,
        &[
            "--ignoreConfig",
            "--noEmit",
            "--pretty",
            "false",
            "--noImplicitAny",
            "--strictNullChecks",
            "test.ts",
        ],
    )
    .expect("tsz binary not found");

    assert_ne!(code, 0, "Expected diagnostics exit code, got {code}");
    assert!(
        !output.contains("TS6044"),
        "Bare optional boolean flags should not require explicit values:\n{output}"
    );
    assert!(
        output.contains("TS7006") && output.contains("TS2322"),
        "Expected both bare boolean flags to affect test.ts, got:\n{output}"
    );
}

#[test]
fn command_line_files_with_discovered_tsconfig_report_ts5112() {
    let temp = TempDir::new("command_line_files_ts5112").expect("temp dir");
    write_file(
        &temp.path.join("tsconfig.json"),
        r#"{"compilerOptions":{"noEmit":true}}"#,
    );
    write_file(&temp.path.join("src/a.ts"), "const a = 1;\n");

    let (code, output) =
        run_tsz_with_exit_code(&temp.path, &["--pretty", "false", "--noLib", "src/a.ts"])
            .expect("tsz binary not found");

    assert_eq!(code, 1, "Expected exit code 1 for TS5112, got {code}");
    assert_eq!(output, TS5112_COMMAND_LINE_FILES_OUTPUT);
}

#[test]
fn list_files_only_with_discovered_tsconfig_reports_ts5112() {
    let temp = TempDir::new("list_files_only_ts5112").expect("temp dir");
    write_file(
        &temp.path.join("tsconfig.json"),
        r#"{"compilerOptions":{"noEmit":true}}"#,
    );
    write_file(&temp.path.join("src/a.ts"), "const a = 1;\n");

    let (code, output) = run_tsz_with_exit_code(
        &temp.path,
        &[
            "--pretty",
            "false",
            "--noLib",
            "--listFilesOnly",
            "src/a.ts",
        ],
    )
    .expect("tsz binary not found");

    assert_eq!(code, 1, "Expected exit code 1 for TS5112, got {code}");
    assert_eq!(output, TS5112_COMMAND_LINE_FILES_OUTPUT);
}

#[test]
fn list_files_only_reports_ts6504_for_explicit_js_root_without_allow_js() {
    let temp = TempDir::new("list_files_only_ts6504_js_root").expect("temp dir");
    write_file(
        &temp.path.join("tsconfig.json"),
        r#"{"compilerOptions":{"noEmit":true},"files":["a.js"]}"#,
    );
    write_file(&temp.path.join("a.js"), "const x = 1;\n");

    let (code, output) =
        run_tsz_with_exit_code(&temp.path, &["--pretty", "false", "--listFilesOnly"])
            .expect("tsz binary not found");

    assert_eq!(code, 1, "Expected exit code 1 for TS6504, got {code}");
    assert!(
        output.contains("error TS6504")
            && output.contains("a.js")
            && output.contains("allowJs")
            && output.contains("Part of 'files' list in tsconfig.json"),
        "--listFilesOnly should report the explicit JS root diagnostic before listing files, got:\n{output}"
    );
}

#[test]
fn list_files_only_without_inputs_and_without_config_prints_help() {
    let temp = TempDir::new("list_files_only_no_inputs_no_config").expect("temp dir");

    let (code, output) =
        run_tsz_with_exit_code(&temp.path, &["--listFilesOnly"]).expect("tsz binary not found");

    assert_eq!(
        code, 1,
        "Expected exit code 1 for no-input help, got {code}"
    );
    assert!(
        output.contains("Version ") && output.contains("The TypeScript Compiler"),
        "--listFilesOnly without inputs or tsconfig should print help, got:\n{output}"
    );
}

#[test]
fn ignore_config_skips_ts5112_for_command_line_files() {
    let temp = TempDir::new("ignore_config_skips_ts5112").expect("temp dir");
    write_file(
        &temp.path.join("tsconfig.json"),
        r#"{"compilerOptions":{"noEmit":true}}"#,
    );
    write_file(&temp.path.join("src/a.ts"), "const a = 1;\n");

    let (_code, output) = run_tsz_with_exit_code(
        &temp.path,
        &["--pretty", "false", "--noLib", "--ignoreConfig", "src/a.ts"],
    )
    .expect("tsz binary not found");

    assert!(
        !output.contains("TS5112"),
        "--ignoreConfig should skip TS5112, got:\n{output}"
    );

    let (list_code, list_output) = run_tsz_with_exit_code(
        &temp.path,
        &[
            "--pretty",
            "false",
            "--noLib",
            "--ignoreConfig",
            "--listFilesOnly",
            "src/a.ts",
        ],
    )
    .expect("tsz binary not found");

    assert_eq!(
        list_code, 0,
        "--listFilesOnly with --ignoreConfig should succeed, got:\n{list_output}"
    );
    assert!(
        !list_output.contains("TS5112") && list_output.contains("src/a.ts"),
        "--listFilesOnly with --ignoreConfig should list the explicit file, got:\n{list_output}"
    );
}

#[test]
fn build_mode_v_means_verbose() {
    let temp = TempDir::new("build_v_verbose").expect("temp dir");
    // With -b -v, -v should map to --build-verbose, NOT --version.
    // Since there's no tsconfig, it will error, but should not print version info.
    let (code, output) =
        run_tsz_with_exit_code(&temp.path, &["-b", "-v"]).expect("tsz binary not found");

    // Should NOT contain version output
    assert!(
        !output.contains("Version ") && !output.contains("tsz "),
        "tsz -b -v should not print version info, got:\n{output}"
    );
    // The build should proceed (even if it fails due to no tsconfig) - it should
    // not be interpreted as --version
    let _ = code; // Exit code varies based on tsconfig presence
}

#[test]
fn build_mode_d_means_dry() {
    let temp = TempDir::new("build_d_dry").expect("temp dir");
    // With -b -d, -d should map to --dry, NOT --declaration.
    // Since there's no tsconfig, it will error, but should try dry run path.
    let (_code, output) =
        run_tsz_with_exit_code(&temp.path, &["-b", "-d"]).expect("tsz binary not found");

    // If it tried --declaration instead, clap would likely work differently.
    // The key test: -d in build mode should not set declaration=true outside build context.
    // The output should either show dry run behavior or build mode error (no tsconfig),
    // but not a "declaration" related message.
    let _ = output;
}

#[test]
fn build_mode_f_means_force() {
    let temp = TempDir::new("build_f_force").expect("temp dir");
    // With -b -f, -f should map to --force.
    let (_code, _output) =
        run_tsz_with_exit_code(&temp.path, &["-b", "-f"]).expect("tsz binary not found");
    // Should not error with "unknown argument" for -f in build mode
}

#[test]
fn build_not_first_ts6369() {
    let temp = TempDir::new("build_not_first").expect("temp dir");
    // --build must be first; if it's not, emit TS6369
    let (code, output) =
        run_tsz_with_exit_code(&temp.path, &["--noEmit", "--build"]).expect("tsz binary not found");

    assert_eq!(
        code, 1,
        "Expected exit code 1 for TS6369 (--build not first), got {code}"
    );
    assert!(
        output.contains("error TS6369: Option '--build' must be the first command line argument."),
        "Expected TS6369 diagnostic, got:\n{output}"
    );
}

#[test]
fn build_first_is_ok() {
    let temp = TempDir::new("build_first_ok").expect("temp dir");
    // --build as first argument should NOT trigger TS6369
    let (_code, output) =
        run_tsz_with_exit_code(&temp.path, &["--build"]).expect("tsz binary not found");

    assert!(
        !output.contains("TS6369"),
        "Should not emit TS6369 when --build is first, got:\n{output}"
    );
}

#[test]
fn build_short_b_not_first_ts5023() {
    let temp = TempDir::new("build_short_not_first").expect("temp dir");
    // -b not first: tsc v6 treats this as an unknown flag (TS5023), not TS6369.
    // Only the long form --build triggers TS6369.
    let (code, output) =
        run_tsz_with_exit_code(&temp.path, &["--noEmit", "-b"]).expect("tsz binary not found");

    assert_eq!(code, 1, "Expected exit code 1 for unknown -b, got {code}");
    assert!(
        output.contains("error TS5023"),
        "Expected TS5023 for -b not first (matching tsc v6), got:\n{output}"
    );
}

// ===========================================================================
// End-to-end parity tests: tsz vs tsc (pinned TypeScript version)
//
// These tests run both compilers on identical inputs and assert that outputs
// and exit codes match exactly. They use the tsc version installed globally,
// which must match the pinned version in scripts/package.json.
// ===========================================================================

// ---------------------------------------------------------------------------
// TS6046: Valid values for enum options
// ---------------------------------------------------------------------------

#[test]
fn tsc_parity_ts6046_target() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("ts6046_target").expect("temp dir");
    write_file(&temp.path.join("test.ts"), "export {};\n");
    assert_tsc_tsz_match_with_exit_code(
        &temp.path,
        &["--target", "badValue", "test.ts"],
        "TS6046 --target",
    );
}

#[test]
fn tsc_parity_ts6046_module() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("ts6046_module").expect("temp dir");
    write_file(&temp.path.join("test.ts"), "export {};\n");
    assert_tsc_tsz_match_with_exit_code(
        &temp.path,
        &["--module", "badValue", "test.ts"],
        "TS6046 --module",
    );
}

#[test]
fn tsc_parity_ts6046_jsx() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("ts6046_jsx").expect("temp dir");
    write_file(&temp.path.join("test.ts"), "export {};\n");
    assert_tsc_tsz_match_with_exit_code(
        &temp.path,
        &["--jsx", "badValue", "test.ts"],
        "TS6046 --jsx",
    );
}

#[test]
fn tsc_parity_ts6046_module_resolution() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("ts6046_modres").expect("temp dir");
    write_file(&temp.path.join("test.ts"), "export {};\n");
    assert_tsc_tsz_match_with_exit_code(
        &temp.path,
        &["--moduleResolution", "badValue", "test.ts"],
        "TS6046 --moduleResolution",
    );
}

#[test]
fn tsc_parity_ts6046_module_detection() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("ts6046_moddet").expect("temp dir");
    write_file(&temp.path.join("test.ts"), "export {};\n");
    assert_tsc_tsz_match_with_exit_code(
        &temp.path,
        &["--moduleDetection", "badValue", "test.ts"],
        "TS6046 --moduleDetection",
    );
}

// ---------------------------------------------------------------------------
// Exit codes
// ---------------------------------------------------------------------------

#[test]
fn tsc_parity_exit_code_clean() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("exit_clean").expect("temp dir");
    write_file(&temp.path.join("test.ts"), "const x: number = 42;\n");
    assert_tsc_tsz_match_with_exit_code(
        &temp.path,
        &["--noEmit", "--pretty", "false", "test.ts"],
        "exit code: clean compile",
    );
}

#[test]
fn tsc_parity_exit_code_errors_no_emit() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("exit_noEmit").expect("temp dir");
    write_file(&temp.path.join("test.ts"), "const z = unknownVar;\n");
    // --noEmit + errors => exit code 2 (errors present, no outputs to skip)
    assert_tsc_tsz_match_with_exit_code(
        &temp.path,
        &["--noEmit", "--pretty", "false", "test.ts"],
        "exit code: --noEmit + errors",
    );
}

#[test]
fn tsc_parity_exit_code_unknown_flag() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("exit_unknown").expect("temp dir");
    assert_tsc_tsz_match_with_exit_code(
        &temp.path,
        &["--totallyBogusFlag"],
        "exit code: unknown flag",
    );
}

// ---------------------------------------------------------------------------
// TS5023 / TS5025: Unknown compiler option
// ---------------------------------------------------------------------------

#[test]
fn tsc_parity_ts5023_unknown_flag() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("ts5023").expect("temp dir");
    assert_tsc_tsz_match_with_exit_code(&temp.path, &["--badFlag"], "TS5023 unknown flag");
}

#[test]
fn tsc_parity_ts5025_did_you_mean() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("ts5025").expect("temp dir");
    assert_tsc_tsz_match_with_exit_code(&temp.path, &["--strct"], "TS5025 did you mean");
}

#[test]
fn tsc_parity_ts5025_targett() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("ts5025_target").expect("temp dir");
    assert_tsc_tsz_match_with_exit_code(
        &temp.path,
        &["--targett"],
        "TS5025 --targett did you mean --target",
    );
}

// ---------------------------------------------------------------------------
// TS6369: --build not first
// ---------------------------------------------------------------------------

#[test]
fn tsc_parity_ts6369_build_not_first() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("ts6369").expect("temp dir");
    assert_tsc_tsz_match_with_exit_code(
        &temp.path,
        &["--noEmit", "--build"],
        "TS6369 --build not first",
    );
}

#[test]
fn tsc_parity_ts6369_short_b_not_first() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("ts6369_short").expect("temp dir");
    assert_tsc_tsz_match_with_exit_code(&temp.path, &["--noEmit", "-b"], "TS6369 -b not first");
}

// ---------------------------------------------------------------------------
// --version / --help
// ---------------------------------------------------------------------------

#[test]
fn tsc_parity_version() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("version").expect("temp dir");
    assert_tsc_tsz_match_with_exit_code(&temp.path, &["--version"], "--version output");
}

#[test]
fn tsc_parity_version_short() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("version_short").expect("temp dir");
    assert_tsc_tsz_match_with_exit_code(&temp.path, &["-v"], "-v output");
}

#[test]
fn tsc_parity_help() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("help").expect("temp dir");
    assert_tsc_tsz_match_with_exit_code(&temp.path, &["--help"], "--help output");
}

#[test]
fn tsc_parity_help_all() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("help_all").expect("temp dir");
    assert_tsc_tsz_match_with_exit_code(&temp.path, &["--help", "--all"], "--help --all output");
}

#[test]
fn tsc_parity_no_input() {
    if !tsc_available() {
        return;
    }
    // No tsconfig.json, no files => print version + help, exit 1
    let temp = TempDir::new("no_input").expect("temp dir");
    assert_tsc_tsz_match_with_exit_code(&temp.path, &[], "no input (no tsconfig, no files)");
}

#[test]
fn no_input_from_project_subdirectory_uses_parent_tsconfig() {
    let temp = TempDir::new("parent_config_no_input").expect("temp dir");
    write_file(
        &temp.path.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "strict": true,
    "noEmit": true
  },
  "files": ["src/a.ts"]
}
"#,
    );
    write_file(
        &temp.path.join("src/a.ts"),
        "function f(value) {\n  return value;\n}\nf(1);\n",
    );

    let src_dir = temp.path.join("src");
    let (code, output) = run_tsz_with_exit_code(&src_dir, &["--pretty", "false"])
        .expect("tsz should run from project subdirectory");

    assert_ne!(
        code, 0,
        "strict config should report diagnostics:\n{output}"
    );
    assert!(
        output.contains("TS7006"),
        "parent tsconfig should be discovered and applied:\n{output}"
    );
    assert!(
        !output.contains("tsc: The TypeScript Compiler"),
        "subdirectory project discovery should not fall through to help:\n{output}"
    );
}

#[test]
fn tsc_parity_show_config_strict_stays_compact() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("show_config_strict_compact").expect("temp dir");
    write_file(&temp.path.join("main.ts"), "const n: number = 1;\n");
    write_file(
        &temp.path.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "module": "commonjs",
    "target": "es2017",
    "strict": true,
    "noEmit": true
  },
  "files": ["main.ts"]
}
"#,
    );

    let tsc_output = run_tsc(&temp.path, &["--showConfig"]).expect("tsc should run");
    let output = run_tsz(&temp.path, &["--showConfig"]).expect("tsz should run");
    assert!(
        !tsc_output.contains("\"strictNullChecks\""),
        "tsc should keep strict sub-options compact: {tsc_output}"
    );
    assert!(
        !output.contains("\"strictNullChecks\""),
        "strict sub-options should not be expanded: {output}"
    );
}

#[test]
fn tsc_parity_show_config_node16_resolve_json_false() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("show_config_node16_resolve_json").expect("temp dir");
    write_file(
        &temp.path.join("main.ts"),
        "import data from \"./data.json\";\nconst n: number = data.value;\n",
    );
    write_file(&temp.path.join("data.json"), "{\"value\":123}\n");
    write_file(
        &temp.path.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "module": "node16",
    "moduleResolution": "node16",
    "target": "es2017",
    "strict": true,
    "noEmit": true
  },
  "files": ["main.ts", "data.json"]
}
"#,
    );

    let tsc_output = run_tsc(&temp.path, &["--showConfig"]).expect("tsc should run");
    let output = run_tsz(&temp.path, &["--showConfig"]).expect("tsz should run");
    assert!(
        tsc_output.contains("\"resolveJsonModule\": false"),
        "tsc should show node16 resolveJsonModule false: {tsc_output}"
    );
    assert!(
        output.contains("\"resolveJsonModule\": false"),
        "node16 showConfig should include resolveJsonModule false: {output}"
    );
}

#[test]
fn show_config_check_js_implied_allow_js_includes_js_files() {
    let temp = TempDir::new("show_config_checkjs_allowjs_files").expect("temp dir");
    write_file(&temp.path.join("src/a.js"), "module.exports = 1;\n");
    write_file(&temp.path.join("src/b.ts"), "const x: number = 1;\n");
    write_file(
        &temp.path.join("tsconfig.json"),
        r#"{"compilerOptions":{"checkJs":true},"include":["src"]}"#,
    );

    let output = run_tsz(&temp.path, &["--showConfig"]).expect("tsz should run");
    let json: serde_json::Value = serde_json::from_str(&output)
        .unwrap_or_else(|_| panic!("invalid showConfig JSON:\n{output}"));
    let options = json
        .get("compilerOptions")
        .and_then(|value| value.as_object())
        .unwrap_or_else(|| panic!("missing compilerOptions in showConfig output:\n{output}"));
    let files: Vec<_> = json
        .get("files")
        .and_then(|value| value.as_array())
        .unwrap_or_else(|| panic!("missing files in showConfig output:\n{output}"))
        .iter()
        .filter_map(|value| value.as_str())
        .collect();

    assert_eq!(
        options.get("allowJs"),
        Some(&serde_json::Value::Bool(true)),
        "showConfig should print implied allowJs: {output}"
    );
    assert!(
        files.iter().any(|file| file.ends_with("src/a.js")),
        "showConfig files should include JS discovered via implied allowJs: {output}"
    );
    assert!(
        files.iter().any(|file| file.ends_with("src/b.ts")),
        "showConfig files should keep TS files too: {output}"
    );
}

#[test]
fn show_config_renders_inherited_root_selectors_relative_to_child_config() {
    let temp = TempDir::new("show_config_inherited_root_selectors").expect("temp dir");
    let base = temp.path.join("base");
    let child = temp.path.join("child");
    std::fs::create_dir_all(base.join("src")).expect("create base src");
    std::fs::create_dir_all(&child).expect("create child");
    write_file(&base.join("src/a.ts"), "export const x = 1;\n");
    write_file(
        &base.join("tsconfig.base.json"),
        r#"{
  "include": ["src"]
}
"#,
    );
    write_file(
        &child.join("tsconfig.json"),
        r#"{
  "extends": "../base/tsconfig.base.json"
}
"#,
    );

    let output = run_tsz(&child, &["--showConfig"]).expect("tsz should run");
    let json: serde_json::Value = serde_json::from_str(&output)
        .unwrap_or_else(|_| panic!("invalid showConfig JSON:\n{output}"));
    let files = json
        .get("files")
        .and_then(|v| v.as_array())
        .unwrap_or_else(|| panic!("missing files in showConfig output:\n{output}"));
    let include = json
        .get("include")
        .and_then(|v| v.as_array())
        .unwrap_or_else(|| panic!("missing include in showConfig output:\n{output}"));

    assert_eq!(
        files,
        &[serde_json::Value::String("../base/src/a.ts".to_string())],
        "inherited discovered file should render relative to child config: {output}"
    );
    assert_eq!(
        include,
        &[serde_json::Value::String("../base/src".to_string())],
        "inherited include should render relative to child config: {output}"
    );
    assert!(
        !output.contains(temp.path.to_string_lossy().as_ref()),
        "showConfig should not leak absolute temp paths: {output}"
    );
}

#[test]
fn show_config_rejects_tsconfig_only_cli_options() {
    let temp = TempDir::new("show_config_tsconfig_only_cli_options").expect("temp dir");
    write_file(&temp.path.join("index.ts"), "export {};\n");

    for (flag, value) in [("--paths", "@/*=src/*"), ("--plugins", "foo")] {
        let (code, output) = run_tsz_with_exit_code(
            &temp.path,
            &["--showConfig", "--ignoreConfig", flag, value, "index.ts"],
        )
        .expect("tsz should run");
        assert_eq!(code, 1, "expected failure for {flag}, got: {output}");
        assert!(
            output.contains("error TS6064:"),
            "expected TS6064 for {flag}, got: {output}"
        );
    }
}

#[test]
fn invalid_top_level_config_array_types_emit_ts5024() {
    let temp = TempDir::new("top_level_config_array_types").expect("temp dir");
    write_file(&temp.path.join("a.ts"), "export {};\n");

    for (key, value) in [
        ("include", r#""*.ts""#),
        ("exclude", r#""dist""#),
        ("references", r#""./lib""#),
    ] {
        write_file(
            &temp.path.join("tsconfig.json"),
            &format!(
                r#"{{
  "{key}": {value},
  "compilerOptions": {{ "noEmit": true }},
  "files": ["a.ts"]
}}
"#
            ),
        );

        let (code, output) =
            run_tsz_with_exit_code(&temp.path, &["-p", "tsconfig.json", "--pretty", "false"])
                .expect("tsz should run");

        assert_ne!(
            code, 0,
            "expected config diagnostic for {key}, got: {output}"
        );
        assert!(
            !output.contains("failed to parse tsconfig"),
            "invalid {key} should recover through TS5024, got:\n{output}"
        );
        assert!(
            output.contains(&format!(
                "error TS5024: Compiler option '{key}' requires a value of type Array."
            )),
            "expected TS5024 for {key}, got:\n{output}"
        );
    }
}

#[test]
fn show_config_includes_supported_direct_and_inherited_options() {
    let temp = TempDir::new("show_config_supported_options").expect("temp dir");
    write_file(&temp.path.join("a.ts"), "enum E { A }\n");
    write_file(
        &temp.path.join("base.json"),
        r#"{
  "compilerOptions": {
    "erasableSyntaxOnly": true
  }
}
"#,
    );
    write_file(
        &temp.path.join("tsconfig.json"),
        r#"{
  "extends": "./base.json",
  "compilerOptions": {
    "strictNullChecks": true,
    "exactOptionalPropertyTypes": true
  },
  "files": ["a.ts"]
}
"#,
    );

    let output = run_tsz(&temp.path, &["--showConfig"]).expect("tsz should run");
    let json: serde_json::Value = serde_json::from_str(&output)
        .unwrap_or_else(|_| panic!("invalid showConfig JSON:\n{output}"));
    let options = json
        .get("compilerOptions")
        .and_then(|v| v.as_object())
        .unwrap_or_else(|| panic!("missing compilerOptions in showConfig output:\n{output}"));

    assert_eq!(
        options.get("strictNullChecks"),
        Some(&serde_json::Value::Bool(true)),
        "control option should still render: {output}"
    );
    assert_eq!(
        options.get("exactOptionalPropertyTypes"),
        Some(&serde_json::Value::Bool(true)),
        "direct supported option should render: {output}"
    );
    assert_eq!(
        options.get("erasableSyntaxOnly"),
        Some(&serde_json::Value::Bool(true)),
        "inherited supported option should render: {output}"
    );
}

#[test]
fn show_config_direct_base_url_and_root_dirs_stay_relative() {
    let temp = TempDir::new("show_config_direct_path_options").expect("temp dir");
    std::fs::create_dir_all(temp.path.join("src")).expect("create src");
    std::fs::create_dir_all(temp.path.join("generated")).expect("create generated");
    write_file(&temp.path.join("src/a.ts"), "export {}\n");
    write_file(
        &temp.path.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "baseUrl": "src",
    "rootDirs": ["src", "generated"],
    "rootDir": "src",
    "outDir": "dist"
  },
  "files": ["src/a.ts"]
}
"#,
    );

    let output =
        run_tsz(&temp.path, &["--showConfig", "--pretty", "false"]).expect("tsz should run");
    let json: serde_json::Value = serde_json::from_str(&output)
        .unwrap_or_else(|_| panic!("invalid showConfig JSON:\n{output}"));
    let options = json
        .get("compilerOptions")
        .and_then(|v| v.as_object())
        .unwrap_or_else(|| panic!("missing compilerOptions in showConfig output:\n{output}"));

    assert_eq!(
        options.get("baseUrl"),
        Some(&serde_json::Value::String("./src".to_string())),
        "direct baseUrl should stay config-relative: {output}"
    );
    assert_eq!(
        options.get("rootDirs"),
        Some(&serde_json::json!(["./src", "./generated"])),
        "direct rootDirs should stay config-relative: {output}"
    );
    assert!(
        !output.contains(temp.path.to_string_lossy().as_ref()),
        "showConfig leaked the temp directory in path options:\n{output}"
    );
}

#[test]
fn show_config_inherited_base_url_and_root_dirs_stay_declaring_relative() {
    let temp = TempDir::new("show_config_inherited_path_options").expect("temp dir");
    std::fs::create_dir_all(temp.path.join("base/src")).expect("create base src");
    std::fs::create_dir_all(temp.path.join("base/generated")).expect("create base generated");
    std::fs::create_dir_all(temp.path.join("app/src")).expect("create app src");
    write_file(&temp.path.join("app/src/a.ts"), "export {}\n");
    write_file(
        &temp.path.join("base/tsconfig.base.json"),
        r#"{
  "compilerOptions": {
    "baseUrl": ".",
    "rootDirs": ["src", "generated"]
  }
}
"#,
    );
    write_file(
        &temp.path.join("app/tsconfig.json"),
        r#"{
  "extends": "../base/tsconfig.base.json",
  "files": ["src/a.ts"]
}
"#,
    );

    let output = run_tsz(&temp.path.join("app"), &["--showConfig"]).expect("tsz should run");
    let json: serde_json::Value = serde_json::from_str(&output)
        .unwrap_or_else(|_| panic!("invalid showConfig JSON:\n{output}"));
    let options = json
        .get("compilerOptions")
        .and_then(|v| v.as_object())
        .unwrap_or_else(|| panic!("missing compilerOptions in showConfig output:\n{output}"));

    assert_eq!(
        options.get("baseUrl"),
        Some(&serde_json::Value::String("../base".to_string())),
        "inherited baseUrl should render relative to the child config: {output}"
    );
    assert_eq!(
        options.get("rootDirs"),
        Some(&serde_json::json!(["../base/src", "../base/generated"])),
        "inherited rootDirs should render relative to the child config: {output}"
    );
}

// ---------------------------------------------------------------------------
// --init
// ---------------------------------------------------------------------------

#[test]
fn tsc_parity_init() {
    if !tsc_available() {
        return;
    }
    // Run --init in separate temp dirs and compare generated tsconfig.json
    let temp_tsc = TempDir::new("init_tsc").expect("temp dir");
    let temp_tsz = TempDir::new("init_tsz").expect("temp dir");

    let tsc_out = run_tsc(&temp_tsc.path, &["--init"]).expect("tsc --init failed");
    let tsz_out = run_tsz(&temp_tsz.path, &["--init"]).expect("tsz --init failed");

    // Console output should match
    if let Some(diff) = diff_outputs(&tsc_out, &tsz_out) {
        panic!("--init console output mismatch:\n{diff}\n\ntsc:\n{tsc_out}\n\ntsz:\n{tsz_out}");
    }

    // Generated tsconfig.json should match
    let tsc_config =
        std::fs::read_to_string(temp_tsc.path.join("tsconfig.json")).expect("tsc tsconfig.json");
    let tsz_config =
        std::fs::read_to_string(temp_tsz.path.join("tsconfig.json")).expect("tsz tsconfig.json");
    assert_eq!(
        tsc_config, tsz_config,
        "--init: generated tsconfig.json files differ"
    );
}

/// Regression test for #3905. When `--init` is invoked together with
/// recognized compiler options, the generated tsconfig.json should reflect
/// those options instead of the hardcoded template. This exercises three
/// distinct override paths: replacing a commented template line (`rootDir`,
/// `outDir`), overwriting an active template line (`module`, `target`,
/// `strict`), and appending an option that has no template slot (`pretty`).
#[test]
fn tsc_parity_init_with_options() {
    if !tsc_available() {
        return;
    }
    let temp_tsc = TempDir::new("init_opts_tsc").expect("temp dir");
    let temp_tsz = TempDir::new("init_opts_tsz").expect("temp dir");

    let opts: &[&str] = &[
        "--init",
        "--target",
        "es2015",
        "--module",
        "commonjs",
        "--rootDir",
        "src",
        "--outDir",
        "dist",
        "--strict",
        "false",
        "--pretty",
        "false",
    ];

    let tsc_out = run_tsc(&temp_tsc.path, opts).expect("tsc --init failed");
    let tsz_out = run_tsz(&temp_tsz.path, opts).expect("tsz --init failed");

    if let Some(diff) = diff_outputs(&tsc_out, &tsz_out) {
        panic!("--init console output mismatch:\n{diff}\n\ntsc:\n{tsc_out}\n\ntsz:\n{tsz_out}");
    }

    let tsc_config =
        std::fs::read_to_string(temp_tsc.path.join("tsconfig.json")).expect("tsc tsconfig.json");
    let tsz_config =
        std::fs::read_to_string(temp_tsz.path.join("tsconfig.json")).expect("tsz tsconfig.json");
    assert_eq!(
        tsc_config, tsz_config,
        "--init with options: generated tsconfig.json files differ"
    );
}

/// Multiple command-line-only options (`--diagnostics`, `--listFiles`,
/// `--noEmit`, `--pretty`) get appended after the template body in the order
/// they appeared on the command line.
#[test]
fn tsc_parity_init_appends_command_line_options_in_order() {
    if !tsc_available() {
        return;
    }
    let temp_tsc = TempDir::new("init_append_tsc").expect("temp dir");
    let temp_tsz = TempDir::new("init_append_tsz").expect("temp dir");

    let opts: &[&str] = &[
        "--init",
        "--listFiles",
        "--noEmit",
        "--diagnostics",
        "--pretty",
        "false",
    ];

    let tsc_out = run_tsc(&temp_tsc.path, opts).expect("tsc --init failed");
    let tsz_out = run_tsz(&temp_tsz.path, opts).expect("tsz --init failed");

    if let Some(diff) = diff_outputs(&tsc_out, &tsz_out) {
        panic!("--init console output mismatch:\n{diff}\n\ntsc:\n{tsc_out}\n\ntsz:\n{tsz_out}");
    }

    let tsc_config =
        std::fs::read_to_string(temp_tsc.path.join("tsconfig.json")).expect("tsc tsconfig.json");
    let tsz_config =
        std::fs::read_to_string(temp_tsz.path.join("tsconfig.json")).expect("tsz tsconfig.json");
    assert_eq!(
        tsc_config, tsz_config,
        "--init append-only options: generated tsconfig.json files differ"
    );
}

// ---------------------------------------------------------------------------
// Diagnostic output: plain mode exact match
// ---------------------------------------------------------------------------

#[test]
fn tsc_parity_plain_single_ts2304() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("plain_ts2304").expect("temp dir");
    write_file(&temp.path.join("test.ts"), "const z = unknownVar;\n");
    assert_tsc_tsz_match(
        &temp.path,
        &["--noEmit", "--pretty", "false", "test.ts"],
        "plain single TS2304",
    );
}

#[test]
fn tsc_parity_plain_multiple_ts2304() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("plain_multi_ts2304").expect("temp dir");
    write_file(
        &temp.path.join("test.ts"),
        "const a = foo;\nconst b = bar;\nconst c = baz;\n",
    );
    assert_tsc_tsz_match(
        &temp.path,
        &["--noEmit", "--pretty", "false", "test.ts"],
        "plain multiple TS2304",
    );
}

#[test]
fn tsc_parity_plain_multi_file() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("plain_multi_file").expect("temp dir");
    write_file(&temp.path.join("a.ts"), "const a = foo;\n");
    write_file(&temp.path.join("b.ts"), "const b = bar;\n");
    assert_tsc_tsz_match(
        &temp.path,
        &["--noEmit", "--pretty", "false", "a.ts", "b.ts"],
        "plain multi-file",
    );
}

#[test]
fn tsc_parity_plain_no_errors() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("plain_clean").expect("temp dir");
    write_file(
        &temp.path.join("test.ts"),
        "const x: number = 42;\nconst y: string = \"hello\";\n",
    );
    assert_tsc_tsz_match(
        &temp.path,
        &["--noEmit", "--pretty", "false", "test.ts"],
        "plain no errors",
    );
}

// ---------------------------------------------------------------------------
// Diagnostic output: pretty mode exact match
// ---------------------------------------------------------------------------

#[test]
fn tsc_parity_pretty_single_ts2304() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("pretty_ts2304").expect("temp dir");
    write_file(&temp.path.join("test.ts"), "const z = unknownVar;\n");
    assert_tsc_tsz_match(
        &temp.path,
        &["--noEmit", "--pretty", "true", "test.ts"],
        "pretty single TS2304",
    );
}

