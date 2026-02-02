/// Integration tests that compare tsz output against tsc (TypeScript compiler) output
/// to ensure they match character-by-character.
///
/// These tests require `tsc` to be installed and available in PATH.
/// They compare the diagnostic output format (non-pretty mode) between tsz and tsc
/// to verify that tsz produces identical output to tsc for identical inputs.
///
/// Note: Some tests compare output structure only (ignoring error span positions)
/// because tsz's type checker may report errors on different AST nodes than tsc.
/// Tests that use error codes/types where both compilers agree on spans will
/// verify exact char-by-char matches.
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(name: &str) -> std::io::Result<Self> {
        let mut path = std::env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        path.push(format!("tsz_tsc_compat_{}_{}", name, nanos));
        std::fs::create_dir_all(&path)?;
        Ok(Self { path })
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn write_file(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("failed to create parent directory");
    }
    std::fs::write(path, contents).expect("failed to write file");
}

/// Run tsc and return its stderr output (where diagnostics go) with ANSI codes stripped.
fn run_tsc(cwd: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("tsc")
        .args(args)
        .current_dir(cwd)
        .output()
        .ok()?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    // tsc outputs diagnostics to stdout in non-pretty mode and stderr in some modes
    let combined = if !stdout.is_empty() {
        stdout.into_owned()
    } else {
        stderr.into_owned()
    };
    Some(normalize_output(&combined))
}

/// Run tsz and return its stderr output with ANSI codes stripped.
fn run_tsz(cwd: &Path, args: &[&str]) -> Option<String> {
    let tsz_bin = find_tsz_binary()?;
    let output = Command::new(&tsz_bin)
        .args(args)
        .current_dir(cwd)
        .output()
        .ok()?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    Some(normalize_output(&stderr))
}

/// Find the tsz binary in the target directory.
fn find_tsz_binary() -> Option<PathBuf> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));

    // Try common build output locations
    for profile in &["debug", "release"] {
        for target_dir in &[".target", "target"] {
            let path = manifest_dir.join(target_dir).join(profile).join("tsz");
            if path.exists() {
                return Some(path);
            }
        }
    }
    None
}

/// Normalize output: strip ANSI codes, normalize line endings to \n.
fn normalize_output(s: &str) -> String {
    // Strip ANSI escape codes
    let stripped = strip_ansi(s);
    // Normalize Windows line endings to Unix
    stripped.replace("\r\n", "\n")
}

/// Strip ANSI escape sequences from a string.
fn strip_ansi(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            // Skip escape sequence: ESC [ ... (letter)
            if chars.peek() == Some(&'[') {
                chars.next(); // consume '['
                while let Some(&c) = chars.peek() {
                    chars.next();
                    if c.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            result.push(ch);
        }
    }
    result
}

/// Compare two outputs line by line, returning a detailed diff description.
fn diff_outputs(tsc_output: &str, tsz_output: &str) -> Option<String> {
    let tsc_lines: Vec<&str> = tsc_output.lines().collect();
    let tsz_lines: Vec<&str> = tsz_output.lines().collect();

    let mut diffs = Vec::new();

    let max_lines = tsc_lines.len().max(tsz_lines.len());
    for i in 0..max_lines {
        let tsc_line = tsc_lines.get(i).unwrap_or(&"<missing>");
        let tsz_line = tsz_lines.get(i).unwrap_or(&"<missing>");
        if tsc_line != tsz_line {
            diffs.push(format!(
                "Line {} differs:\n  tsc: {:?}\n  tsz: {:?}",
                i + 1,
                tsc_line,
                tsz_line
            ));
        }
    }

    if tsc_lines.len() != tsz_lines.len() {
        diffs.push(format!(
            "Line count: tsc={}, tsz={}",
            tsc_lines.len(),
            tsz_lines.len()
        ));
    }

    if diffs.is_empty() {
        None
    } else {
        Some(diffs.join("\n"))
    }
}

/// Check that tsc is available on the system.
fn tsc_available() -> bool {
    Command::new("tsc").arg("--version").output().is_ok()
}

// ===========================================================================
// Integration tests: exact match (where checker positions agree)
// ===========================================================================

#[test]
#[ignore = "TODO: tsc compat tests need work"]
fn tsc_compat_cannot_find_name_plain() {
    if !tsc_available() {
        eprintln!("SKIP: tsc not found in PATH");
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
            "tsz output does not match tsc (non-pretty):\n{}\n\ntsc output:\n{}\n\ntsz output:\n{}",
            diff, tsc_out, tsz_out
        );
    }
}

#[test]
#[ignore = "TODO: tsc compat tests need work"]
fn tsc_compat_cannot_find_name_pretty() {
    if !tsc_available() {
        eprintln!("SKIP: tsc not found in PATH");
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
            "tsz output does not match tsc (pretty):\n{}\n\ntsc output:\n{}\n\ntsz output:\n{}",
            diff, tsc_out, tsz_out
        );
    }
}

#[test]
#[ignore = "TODO: tsc compat tests need work"]
fn tsc_compat_multiple_cannot_find_name_plain() {
    if !tsc_available() {
        eprintln!("SKIP: tsc not found in PATH");
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
            "tsz output does not match tsc (non-pretty, multiple errors):\n{}\n\ntsc:\n{}\n\ntsz:\n{}",
            diff, tsc_out, tsz_out
        );
    }
}

#[test]
#[ignore = "TODO: tsc compat tests need work"]
fn tsc_compat_multiple_cannot_find_name_pretty() {
    if !tsc_available() {
        eprintln!("SKIP: tsc not found in PATH");
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
            "tsz output does not match tsc (pretty, multiple errors):\n{}\n\ntsc:\n{}\n\ntsz:\n{}",
            diff, tsc_out, tsz_out
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
            ) {
                if tsc_code_msg != tsz_code_msg {
                    diffs.push(format!(
                        "Line {}: error message differs:\n  tsc: error TS{}\n  tsz: error TS{}",
                        i + 1,
                        tsc_code_msg,
                        tsz_code_msg
                    ));
                }
            }
        }

        // For "Found N errors" lines, should match exactly
        if tsc_line.starts_with("Found ") {
            if tsc_line != tsz_line {
                diffs.push(format!(
                    "Line {}: summary differs:\n  tsc: {}\n  tsz: {}",
                    i + 1,
                    tsc_line,
                    tsz_line
                ));
            }
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
#[ignore = "TODO: tsc compat tests need work"]
fn tsc_compat_structure_type_error_plain() {
    if !tsc_available() {
        eprintln!("SKIP: tsc not found in PATH");
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
        "Different number of errors:\ntsc ({}):\n{}\ntsz ({}):\n{}",
        tsc_count, tsc_out, tsz_count, tsz_out
    );
}

#[test]
#[ignore = "TODO: tsc compat tests need work"]
fn tsc_compat_structure_type_error_pretty() {
    if !tsc_available() {
        eprintln!("SKIP: tsc not found in PATH");
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
        panic!(
            "Output structure mismatch:\n{}\n\ntsc:\n{}\n\ntsz:\n{}",
            diff, tsc_out, tsz_out
        );
    }
}

#[test]
#[ignore = "TODO: tsc compat tests need work"]
fn tsc_compat_no_errors_plain() {
    if !tsc_available() {
        eprintln!("SKIP: tsc not found in PATH");
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
        "tsc should have no errors: {}",
        tsc_out
    );
    assert!(
        tsz_out.trim().is_empty(),
        "tsz should have no errors: {}",
        tsz_out
    );
}

#[test]
#[ignore = "TODO: tsc compat tests need work"]
fn tsc_compat_exit_code_no_errors() {
    if !tsc_available() {
        eprintln!("SKIP: tsc not found in PATH");
        return;
    }

    let temp = TempDir::new("exit_code_ok").expect("temp dir");
    write_file(&temp.path.join("test.ts"), "const x: number = 42;\n");

    let tsz_bin = find_tsz_binary().expect("tsz binary not found");

    let tsc_status = Command::new("tsc")
        .args(["--noEmit", "--pretty", "false", "test.ts"])
        .current_dir(&temp.path)
        .status()
        .expect("tsc failed");

    let tsz_status = Command::new(&tsz_bin)
        .args(["--noEmit", "--pretty", "false", "test.ts"])
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
#[ignore = "TODO: tsc compat tests need work"]
fn tsc_compat_exit_code_with_errors() {
    if !tsc_available() {
        eprintln!("SKIP: tsc not found in PATH");
        return;
    }

    let temp = TempDir::new("exit_code_err").expect("temp dir");
    write_file(&temp.path.join("test.ts"), "const z = unknownVar;\n");

    let tsz_bin = find_tsz_binary().expect("tsz binary not found");

    let tsc_status = Command::new("tsc")
        .args(["--noEmit", "--pretty", "false", "test.ts"])
        .current_dir(&temp.path)
        .status()
        .expect("tsc failed");

    let tsz_status = Command::new(&tsz_bin)
        .args(["--noEmit", "--pretty", "false", "test.ts"])
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
#[ignore = "TODO: tsc compat tests need work"]
fn tsc_compat_line_endings_normalized() {
    if !tsc_available() {
        eprintln!("SKIP: tsc not found in PATH");
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
        "tsc should find TS2304: {}",
        tsc_out
    );
    assert!(
        tsz_out.contains("error TS2304"),
        "tsz should find TS2304: {}",
        tsz_out
    );

    // Exact match for this case (TS2304 spans agree)
    if let Some(diff) = diff_outputs(&tsc_out, &tsz_out) {
        panic!(
            "tsz output does not match tsc (Windows line endings):\n{}\n\ntsc:\n{}\n\ntsz:\n{}",
            diff, tsc_out, tsz_out
        );
    }
}

// ===========================================================================
// Format-specific tests
// ===========================================================================

#[test]
#[ignore = "TODO: tsc compat tests need work"]
fn tsc_compat_plain_format_structure() {
    if !tsc_available() {
        eprintln!("SKIP: tsc not found in PATH");
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
            "Non-pretty line doesn't match format 'file(line,col): error TScode: message': {}",
            line
        );
        // Should contain parenthesized position
        assert!(
            line.contains('(') && line.contains(')'),
            "Non-pretty line missing parenthesized position: {}",
            line
        );
        // Should NOT contain source snippets
        assert!(
            !line.contains('~'),
            "Non-pretty line should not have underline markers: {}",
            line
        );
    }
}

#[test]
#[ignore = "TODO: tsc compat tests need work"]
fn tsc_compat_pretty_format_structure() {
    if !tsc_available() {
        eprintln!("SKIP: tsc not found in PATH");
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
        "Should have 'Found N error(s)' summary:\n{}",
        tsz_out
    );
}

#[test]
#[ignore = "TODO: tsc compat tests need work"]
fn tsc_compat_double_digit_line_number_pretty() {
    if !tsc_available() {
        eprintln!("SKIP: tsc not found in PATH");
        return;
    }

    let temp = TempDir::new("double_digit_line").expect("temp dir");
    let mut source = String::new();
    for i in 1..=9 {
        source.push_str(&format!("const a{} = {};\n", i, i));
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
            "Double-digit line number output mismatch:\n{}\n\ntsc:\n{}\n\ntsz:\n{}",
            diff, tsc_out, tsz_out
        );
    }
}
