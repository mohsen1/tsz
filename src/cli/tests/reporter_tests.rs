use super::reporter::Reporter;
use crate::checker::types::diagnostics::{Diagnostic, DiagnosticCategory};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new() -> std::io::Result<Self> {
        let mut path = std::env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        path.push(format!(
            "tsz_cli_reporter_test_{}_{}",
            std::process::id(),
            nanos
        ));
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

// ===========================================================================
// Non-pretty mode tests (--pretty false)
// Format: file(line,col): error TScode: message
// ===========================================================================

#[test]
fn plain_mode_formats_diagnostic_with_location() {
    let temp = TempDir::new().expect("temp dir");
    let file_path = temp.path.join("src/main.ts");
    write_file(&file_path, "let x = 1;\nlet y = 2;\n");

    let diagnostic = Diagnostic {
        file: file_path.to_string_lossy().into_owned(),
        start: 11,
        length: 1,
        message_text: "Cannot find name 'y'.".to_string(),
        category: DiagnosticCategory::Error,
        code: 2304,
        related_information: Vec::new(),
    };

    let mut reporter = Reporter::new(false);
    let output = reporter.render(&[diagnostic.clone()]);

    // Non-pretty: file(line,col): error TScode: message\n (no snippets)
    let expected = format!(
        "{}(2,1): error TS2304: Cannot find name 'y'.\n",
        diagnostic.file
    );
    assert_eq!(output, expected);
}

#[test]
fn plain_mode_omits_code_when_zero() {
    let diagnostic = Diagnostic {
        file: "missing.ts".to_string(),
        start: 0,
        length: 0,
        message_text: "Parse error".to_string(),
        category: DiagnosticCategory::Error,
        code: 0,
        related_information: Vec::new(),
    };

    let mut reporter = Reporter::new(false);
    let output = reporter.render(&[diagnostic]);

    assert!(output.contains(": error: Parse error"), "{output}");
}

#[test]
fn plain_mode_no_source_snippets() {
    let temp = TempDir::new().expect("temp dir");
    let file_path = temp.path.join("test.ts");
    write_file(&file_path, "let x: number = \"string\";\n");

    let diagnostic = Diagnostic {
        file: file_path.to_string_lossy().into_owned(),
        start: 16,
        length: 8,
        message_text: "Type 'string' is not assignable to type 'number'.".to_string(),
        category: DiagnosticCategory::Error,
        code: 2322,
        related_information: Vec::new(),
    };

    let mut reporter = Reporter::new(false);
    let output = reporter.render(&[diagnostic]);

    // Non-pretty mode should NOT contain source snippets
    assert!(
        !output.contains("let x: number"),
        "non-pretty should not include source snippets: {output}"
    );
    assert!(
        !output.contains('~'),
        "non-pretty should not include underline: {output}"
    );
    // Should be single line
    assert_eq!(
        output.lines().count(),
        1,
        "non-pretty should be single line: {output}"
    );
}

#[test]
fn plain_mode_multiple_diagnostics() {
    let temp = TempDir::new().expect("temp dir");
    let file_path = temp.path.join("test.ts");
    write_file(
        &file_path,
        "let x: number = \"string\";\nlet y: string = 42;\n",
    );

    let diagnostics = vec![
        Diagnostic {
            file: file_path.to_string_lossy().into_owned(),
            start: 4,
            length: 1,
            message_text: "Type 'string' is not assignable to type 'number'.".to_string(),
            category: DiagnosticCategory::Error,
            code: 2322,
            related_information: Vec::new(),
        },
        Diagnostic {
            file: file_path.to_string_lossy().into_owned(),
            start: 30,
            length: 1,
            message_text: "Type 'number' is not assignable to type 'string'.".to_string(),
            category: DiagnosticCategory::Error,
            code: 2322,
            related_information: Vec::new(),
        },
    ];

    let mut reporter = Reporter::new(false);
    let output = reporter.render(&diagnostics);

    // Should have exactly 2 lines, no snippets
    assert_eq!(output.lines().count(), 2, "expected 2 lines: {output}");
    for line in output.lines() {
        assert!(!line.contains('~'), "non-pretty should not have underlines");
    }
}

// ===========================================================================
// Pretty mode tests (--pretty true)
// Format: file:line:col - error TScode: message
// ===========================================================================

#[test]
fn pretty_mode_uses_colon_separated_location() {
    let temp = TempDir::new().expect("temp dir");
    let file_path = temp.path.join("test.ts");
    write_file(&file_path, "let x = 1;\nlet y = 2;\n");

    let diagnostic = Diagnostic {
        file: file_path.to_string_lossy().into_owned(),
        start: 11,
        length: 1,
        message_text: "Cannot find name 'y'.".to_string(),
        category: DiagnosticCategory::Error,
        code: 2304,
        related_information: Vec::new(),
    };

    let mut reporter = Reporter::new(false);
    reporter.set_pretty(true);
    let output = reporter.render(&[diagnostic.clone()]);

    // Pretty mode: file:line:col - error TScode: message
    assert!(
        output.contains(&format!("{}:2:1 - error TS2304: ", diagnostic.file)),
        "pretty mode should use colon-separated location: {output}"
    );
}

#[test]
fn pretty_mode_includes_source_snippet() {
    let temp = TempDir::new().expect("temp dir");
    let file_path = temp.path.join("test.ts");
    write_file(&file_path, "let x: number = \"string\";\n");

    let diagnostic = Diagnostic {
        file: file_path.to_string_lossy().into_owned(),
        start: 16,
        length: 8,
        message_text: "Type 'string' is not assignable to type 'number'.".to_string(),
        category: DiagnosticCategory::Error,
        code: 2322,
        related_information: Vec::new(),
    };

    let mut reporter = Reporter::new(false);
    reporter.set_pretty(true);
    let output = reporter.render(&[diagnostic]);

    // Pretty mode should include source line with line number
    assert!(
        output.contains("1 let x: number = \"string\";"),
        "missing source line: {output}"
    );
    // Should include underline
    assert!(output.contains("~~~~~~~~"), "missing underline: {output}");
}

#[test]
fn pretty_mode_snippet_line_number_alignment() {
    let temp = TempDir::new().expect("temp dir");
    let file_path = temp.path.join("test.ts");
    // Create a file with an error on line 10
    let mut source = String::new();
    for i in 1..=9 {
        source.push_str(&format!("let a{} = {};\n", i, i));
    }
    source.push_str("let a10: number = \"string\";\n");
    write_file(&file_path, &source);

    let diagnostic = Diagnostic {
        file: file_path.to_string_lossy().into_owned(),
        start: source.find("\"string\"").unwrap() as u32,
        length: 8,
        message_text: "Type 'string' is not assignable to type 'number'.".to_string(),
        category: DiagnosticCategory::Error,
        code: 2322,
        related_information: Vec::new(),
    };

    let mut reporter = Reporter::new(false);
    reporter.set_pretty(true);
    let output = reporter.render(&[diagnostic]);

    // Line 10 should be formatted as "10 let a10..."
    assert!(
        output.contains("10 let a10: number = \"string\";"),
        "missing line 10 source: {output}"
    );
}

#[test]
fn pretty_mode_summary_single_error_single_file() {
    let temp = TempDir::new().expect("temp dir");
    let file_path = temp.path.join("test.ts");
    write_file(&file_path, "let x = unknownVar;\n");

    let diagnostic = Diagnostic {
        file: file_path.to_string_lossy().into_owned(),
        start: 8,
        length: 10,
        message_text: "Cannot find name 'unknownVar'.".to_string(),
        category: DiagnosticCategory::Error,
        code: 2304,
        related_information: Vec::new(),
    };

    let mut reporter = Reporter::new(false);
    reporter.set_pretty(true);
    let output = reporter.render(&[diagnostic.clone()]);

    // "Found 1 error in test.ts:1" (relative path)
    assert!(
        output.contains("Found 1 error in"),
        "missing summary line: {output}"
    );
}

#[test]
fn pretty_mode_summary_multiple_errors_same_file() {
    let temp = TempDir::new().expect("temp dir");
    let file_path = temp.path.join("test.ts");
    write_file(&file_path, "let x = a;\nlet y = b;\n");

    let diagnostics = vec![
        Diagnostic {
            file: file_path.to_string_lossy().into_owned(),
            start: 8,
            length: 1,
            message_text: "Cannot find name 'a'.".to_string(),
            category: DiagnosticCategory::Error,
            code: 2304,
            related_information: Vec::new(),
        },
        Diagnostic {
            file: file_path.to_string_lossy().into_owned(),
            start: 19,
            length: 1,
            message_text: "Cannot find name 'b'.".to_string(),
            category: DiagnosticCategory::Error,
            code: 2304,
            related_information: Vec::new(),
        },
    ];

    let mut reporter = Reporter::new(false);
    reporter.set_pretty(true);
    let output = reporter.render(&diagnostics);

    // "Found 2 errors in the same file, starting at: test.ts:1"
    assert!(
        output.contains("Found 2 errors in the same file, starting at:"),
        "missing multi-error summary: {output}"
    );
}

#[test]
fn pretty_mode_has_blank_line_between_header_and_snippet() {
    let temp = TempDir::new().expect("temp dir");
    let file_path = temp.path.join("test.ts");
    write_file(&file_path, "let x = unknownVar;\n");

    let diagnostic = Diagnostic {
        file: file_path.to_string_lossy().into_owned(),
        start: 8,
        length: 10,
        message_text: "Cannot find name 'unknownVar'.".to_string(),
        category: DiagnosticCategory::Error,
        code: 2304,
        related_information: Vec::new(),
    };

    let mut reporter = Reporter::new(false);
    reporter.set_pretty(true);
    let output = reporter.render(&[diagnostic]);

    let lines: Vec<&str> = output.lines().collect();
    // Line 0: header
    // Line 1: blank
    // Line 2: source
    // Line 3: underline
    assert!(lines.len() >= 4, "expected at least 4 lines: {output}");
    assert!(
        lines[1].is_empty(),
        "expected blank line between header and source, got: '{}'",
        lines[1]
    );
}
