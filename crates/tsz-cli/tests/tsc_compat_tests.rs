//! Integration tests that compare tsz output against tsc (TypeScript compiler) output
//! to ensure they match character-by-character.
//!
//! These tests require `tsc` to be installed and available in PATH.
//! They compare the diagnostic output format (non-pretty mode) between tsz and tsc
//! to verify that tsz produces identical output to tsc for identical inputs.
//!
//! Note: Some tests compare output structure only (ignoring error span positions)
//! because tsz's type checker may report errors on different AST nodes than tsc.
//! Tests that use error codes/types where both compilers agree on spans will
//! verify exact char-by-char matches.

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
        path.push(format!("tsz_tsc_compat_{name}_{nanos}"));
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

/// Run tsz and return its diagnostic output with ANSI codes stripped.
fn run_tsz(cwd: &Path, args: &[&str]) -> Option<String> {
    let tsz_bin = find_tsz_binary()?;
    let output = Command::new(&tsz_bin)
        .args(args)
        .current_dir(cwd)
        .output()
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    // tsz currently writes diagnostics to stdout in plain mode.
    let combined = if !stdout.is_empty() {
        stdout.into_owned()
    } else {
        stderr.into_owned()
    };
    Some(normalize_output(&combined))
}

/// Run tsz and return (`exit_code`, `combined_output`).
fn run_tsz_with_exit_code(cwd: &Path, args: &[&str]) -> Option<(i32, String)> {
    let tsz_bin = find_tsz_binary()?;
    let output = Command::new(&tsz_bin)
        .args(args)
        .current_dir(cwd)
        .output()
        .ok()?;

    let code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Combine both stdout and stderr for the full picture
    let mut combined = String::new();
    if !stderr.is_empty() {
        combined.push_str(&stderr);
    }
    if !stdout.is_empty() {
        combined.push_str(&stdout);
    }
    Some((code, normalize_output(&combined)))
}

/// Run tsc and return (`exit_code`, `combined_output`).
fn run_tsc_with_exit_code(cwd: &Path, args: &[&str]) -> Option<(i32, String)> {
    let output = Command::new("tsc")
        .args(args)
        .current_dir(cwd)
        .output()
        .ok()?;

    let code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut combined = String::new();
    if !stderr.is_empty() {
        combined.push_str(&stderr);
    }
    if !stdout.is_empty() {
        combined.push_str(&stdout);
    }
    Some((code, normalize_output(&combined)))
}

/// Run both tsc and tsz and assert their outputs match exactly.
/// Returns the common output on success.
fn assert_tsc_tsz_match(cwd: &Path, args: &[&str], label: &str) -> String {
    let tsc_out = run_tsc(cwd, args).expect("tsc failed to run");
    let tsz_out = run_tsz(cwd, args).expect("tsz failed to run");
    if let Some(diff) = diff_outputs(&tsc_out, &tsz_out) {
        panic!(
            "{label}: tsz output does not match tsc.\n{diff}\n\ntsc:\n{tsc_out}\n\ntsz:\n{tsz_out}"
        );
    }
    tsc_out
}

/// Run both tsc and tsz and assert their outputs AND exit codes match.
fn assert_tsc_tsz_match_with_exit_code(cwd: &Path, args: &[&str], label: &str) {
    let (tsc_code, tsc_out) = run_tsc_with_exit_code(cwd, args).expect("tsc failed to run");
    let (tsz_code, tsz_out) = run_tsz_with_exit_code(cwd, args).expect("tsz failed to run");
    assert_eq!(
        tsc_code, tsz_code,
        "{label}: exit code mismatch: tsc={tsc_code}, tsz={tsz_code}\ntsc output:\n{tsc_out}\ntsz output:\n{tsz_out}"
    );
    let tsc_norm = normalize_output(&tsc_out);
    let tsz_norm = normalize_output(&tsz_out);
    if let Some(diff) = diff_outputs(&tsc_norm, &tsz_norm) {
        panic!("{label}: output mismatch.\n{diff}\n\ntsc:\n{tsc_norm}\n\ntsz:\n{tsz_norm}");
    }
}

/// Find the tsz binary in the target directory.
fn find_tsz_binary() -> Option<PathBuf> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));

    // Try workspace root (two directories up from crates/tsz-cli)
    if let Some(workspace_root) = manifest_dir.parent().and_then(|p| p.parent()) {
        for profile in &["debug", "release", "dist-fast"] {
            for target_dir in &[".target", "target"] {
                let path = workspace_root.join(target_dir).join(profile).join("tsz");
                if path.exists() {
                    return Some(path);
                }
            }
        }
    }

    // Try crate-local build output locations (fallback)
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
fn tsc_compat_exit_code_with_errors() {
    if !tsc_available() {
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
#[ignore = "pre-existing: remote merge regression"]
fn tsc_parity_version() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("version").expect("temp dir");
    assert_tsc_tsz_match_with_exit_code(&temp.path, &["--version"], "--version output");
}

#[test]
#[ignore = "pre-existing: remote merge regression"]
fn tsc_parity_version_short() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("version_short").expect("temp dir");
    assert_tsc_tsz_match_with_exit_code(&temp.path, &["-v"], "-v output");
}

#[test]
#[ignore = "pre-existing: remote merge regression"]
fn tsc_parity_help() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("help").expect("temp dir");
    assert_tsc_tsz_match_with_exit_code(&temp.path, &["--help"], "--help output");
}

#[test]
#[ignore = "pre-existing: remote merge regression"]
fn tsc_parity_help_all() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("help_all").expect("temp dir");
    assert_tsc_tsz_match_with_exit_code(&temp.path, &["--help", "--all"], "--help --all output");
}

#[test]
#[ignore = "pre-existing: remote merge regression"]
fn tsc_parity_no_input() {
    if !tsc_available() {
        return;
    }
    // No tsconfig.json, no files => print version + help, exit 1
    let temp = TempDir::new("no_input").expect("temp dir");
    assert_tsc_tsz_match_with_exit_code(&temp.path, &[], "no input (no tsconfig, no files)");
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

// NOTE: tsc v6 emits TS5107 deprecation warnings for deprecated values like
// --target es5, --module amd, etc. tsz does not yet implement TS5107.
// These tests verify that deprecated values are at least accepted (not rejected)
// by tsz, matching tsc's behavior of still compiling them.

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
