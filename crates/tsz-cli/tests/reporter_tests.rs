use super::reporter::Reporter;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tsz_checker::diagnostics::{Diagnostic, DiagnosticCategory};

static TEMP_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

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
        let unique = TEMP_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
        path.push(format!(
            "tsz_cli_reporter_test_{}_{}_{}",
            std::process::id(),
            nanos,
            unique
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

    let diagnostic = Diagnostic::error(
        file_path.to_string_lossy().into_owned(),
        11,
        1,
        "Cannot find name 'y'.".to_string(),
        2304,
    );

    let mut reporter = Reporter::new(false);
    let output = reporter.render(std::slice::from_ref(&diagnostic));

    // Non-pretty: relative_path(line,col): error TScode: message\n (no snippets)
    // The reporter now shows relative paths from cwd, so check structure
    assert!(
        output.ends_with("(2,1): error TS2304: Cannot find name 'y'.\n"),
        "unexpected format: {output}"
    );
    assert!(
        output.contains("main.ts"),
        "should contain file name: {output}"
    );
}

#[test]
fn plain_mode_omits_code_when_zero() {
    let diagnostic =
        Diagnostic::error("missing.ts".to_string(), 0, 0, "Parse error".to_string(), 0);

    let mut reporter = Reporter::new(false);
    let output = reporter.render(&[diagnostic]);

    assert!(output.contains(": error: Parse error"), "{output}");
}

#[test]
fn plain_mode_no_source_snippets() {
    let temp = TempDir::new().expect("temp dir");
    let file_path = temp.path.join("test.ts");
    write_file(&file_path, "let x: number = \"string\";\n");

    let diagnostic = Diagnostic::error(
        file_path.to_string_lossy().into_owned(),
        16,
        8,
        "Type 'string' is not assignable to type 'number'.".to_string(),
        2322,
    );

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
        Diagnostic::error(
            file_path.to_string_lossy().into_owned(),
            4,
            1,
            "Type 'string' is not assignable to type 'number'.".to_string(),
            2322,
        ),
        Diagnostic::error(
            file_path.to_string_lossy().into_owned(),
            30,
            1,
            "Type 'number' is not assignable to type 'string'.".to_string(),
            2322,
        ),
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

    let diagnostic = Diagnostic::error(
        file_path.to_string_lossy().into_owned(),
        11,
        1,
        "Cannot find name 'y'.".to_string(),
        2304,
    );

    let mut reporter = Reporter::new(false);
    reporter.set_pretty(true);
    let output = reporter.render(std::slice::from_ref(&diagnostic));

    // Pretty mode: relative_path:line:col - error TScode: message
    assert!(
        output.contains("test.ts:2:1 - error TS2304: "),
        "pretty mode should use colon-separated location: {output}"
    );
}

#[test]
fn pretty_mode_includes_source_snippet() {
    let temp = TempDir::new().expect("temp dir");
    let file_path = temp.path.join("test.ts");
    write_file(&file_path, "let x: number = \"string\";\n");

    let diagnostic = Diagnostic::error(
        file_path.to_string_lossy().into_owned(),
        16,
        8,
        "Type 'string' is not assignable to type 'number'.".to_string(),
        2322,
    );

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
        source.push_str(&format!("let a{i} = {i};\n"));
    }
    source.push_str("let a10: number = \"string\";\n");
    write_file(&file_path, &source);

    let diagnostic = Diagnostic::error(
        file_path.to_string_lossy().into_owned(),
        source.find("\"string\"").unwrap() as u32,
        8,
        "Type 'string' is not assignable to type 'number'.".to_string(),
        2322,
    );

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

    let diagnostic = Diagnostic::error(
        file_path.to_string_lossy().into_owned(),
        8,
        10,
        "Cannot find name 'unknownVar'.".to_string(),
        2304,
    );

    let mut reporter = Reporter::new(false);
    reporter.set_pretty(true);
    let output = reporter.render(std::slice::from_ref(&diagnostic));

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
        Diagnostic::error(
            file_path.to_string_lossy().into_owned(),
            8,
            1,
            "Cannot find name 'a'.".to_string(),
            2304,
        ),
        Diagnostic::error(
            file_path.to_string_lossy().into_owned(),
            19,
            1,
            "Cannot find name 'b'.".to_string(),
            2304,
        ),
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

    let diagnostic = Diagnostic::error(
        file_path.to_string_lossy().into_owned(),
        8,
        10,
        "Cannot find name 'unknownVar'.".to_string(),
        2304,
    );

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

#[test]
fn plain_mode_renders_related_information_as_indented_message_only() {
    let temp = TempDir::new().expect("temp dir");
    let file_path = temp.path.join("test.ts");
    write_file(&file_path, "foo();\n");

    let diagnostic = Diagnostic::error(
        file_path.to_string_lossy().into_owned(),
        0,
        3,
        "Cannot find name 'foo'.".to_string(),
        2304,
    )
    .with_related(
        file_path.to_string_lossy().into_owned(),
        0,
        3,
        "The missing symbol is referenced here.",
    );

    let mut reporter = Reporter::new(false);
    let output = reporter.render(&[diagnostic]);
    let lines: Vec<&str> = output.lines().collect();

    assert_eq!(lines.len(), 2, "expected primary + related line: {output}");
    assert_eq!(lines[1], "  The missing symbol is referenced here.");
    assert!(
        !lines[1].contains("test.ts"),
        "plain related info should not repeat locations: {output}"
    );
    assert!(
        !lines[1].contains("TS"),
        "plain related info should not show diagnostic codes: {output}"
    );
}

#[test]
fn plain_mode_uses_parent_segments_for_files_outside_cwd() {
    let temp = TempDir::new().expect("temp dir");
    let cwd = temp.path.join("workspace/src");
    let file_path = temp.path.join("workspace/shared/test.ts");
    write_file(&file_path, "value;\n");

    let diagnostic = Diagnostic::error(
        file_path.to_string_lossy().into_owned(),
        0,
        5,
        "Cannot find name 'value'.".to_string(),
        2304,
    );

    let mut reporter = Reporter::new(false);
    reporter.set_cwd(&cwd);
    let output = reporter.render(&[diagnostic]);

    assert!(
        output.starts_with("../shared/test.ts(1,1): error TS2304:"),
        "expected relative parent path, got: {output}"
    );
}

#[test]
fn pretty_mode_renders_related_location_snippet_and_message() {
    let temp = TempDir::new().expect("temp dir");
    let main_path = temp.path.join("main.ts");
    let decl_path = temp.path.join("decl.ts");
    write_file(&main_path, "foo();\n");
    write_file(&decl_path, "declare function foo(): void;\n");

    let related_start = "declare function ".len() as u32;
    let diagnostic = Diagnostic::error(
        main_path.to_string_lossy().into_owned(),
        0,
        3,
        "Cannot find name 'foo'.".to_string(),
        2304,
    )
    .with_related(
        decl_path.to_string_lossy().into_owned(),
        related_start,
        3,
        "The symbol is declared here.",
    );

    let mut reporter = Reporter::new(false);
    reporter.set_pretty(true);
    reporter.set_cwd(&temp.path);
    let output = reporter.render(&[diagnostic]);

    assert!(
        output.contains("decl.ts:1:18"),
        "missing related location: {output}"
    );
    assert!(
        output.contains("1 declare function foo(): void;"),
        "missing related source snippet: {output}"
    );
    assert!(
        output.contains("~~~"),
        "missing related underline: {output}"
    );
    assert!(
        output.contains("    The symbol is declared here."),
        "missing related message: {output}"
    );
}

#[test]
fn pretty_mode_summary_multiple_files_groups_only_errors() {
    let temp = TempDir::new().expect("temp dir");
    let alpha_path = temp.path.join("alpha.ts");
    let beta_path = temp.path.join("beta.ts");
    write_file(&alpha_path, "a();\nb();\n");
    write_file(&beta_path, "c();\n");

    let diagnostics = vec![
        Diagnostic::error(
            alpha_path.to_string_lossy().into_owned(),
            0,
            1,
            "Cannot find name 'a'.".to_string(),
            2304,
        ),
        Diagnostic::error(
            alpha_path.to_string_lossy().into_owned(),
            5,
            1,
            "Cannot find name 'b'.".to_string(),
            2304,
        ),
        Diagnostic::error(
            beta_path.to_string_lossy().into_owned(),
            0,
            1,
            "Cannot find name 'c'.".to_string(),
            2304,
        ),
        Diagnostic {
            category: DiagnosticCategory::Warning,
            code: 9999,
            file: beta_path.to_string_lossy().into_owned(),
            start: 0,
            length: 1,
            message_text: "This warning should not affect the summary.".to_string(),
            related_information: Vec::new(),
        },
    ];

    let mut reporter = Reporter::new(false);
    reporter.set_pretty(true);
    reporter.set_cwd(&temp.path);
    let output = reporter.render(&diagnostics);

    assert!(
        output.contains("Found 3 errors in 2 files."),
        "missing multi-file summary: {output}"
    );
    assert!(
        output.contains("Errors  Files"),
        "missing summary table: {output}"
    );
    assert!(
        output.contains("alpha.ts:1"),
        "missing alpha entry: {output}"
    );
    assert!(output.contains("beta.ts:1"), "missing beta entry: {output}");
}

#[test]
fn pretty_mode_omits_summary_when_only_warnings_are_present() {
    let temp = TempDir::new().expect("temp dir");
    let file_path = temp.path.join("warn.ts");
    write_file(&file_path, "warn();\n");

    let diagnostic = Diagnostic {
        category: DiagnosticCategory::Warning,
        code: 9999,
        file: file_path.to_string_lossy().into_owned(),
        start: 0,
        length: 4,
        message_text: "A warning message.".to_string(),
        related_information: Vec::new(),
    };

    let mut reporter = Reporter::new(false);
    reporter.set_pretty(true);
    let output = reporter.render(&[diagnostic]);

    assert!(
        !output.contains("Found "),
        "warnings should not add a summary: {output}"
    );
    assert!(
        !output.contains("Errors  Files"),
        "warnings should not add an error table: {output}"
    );
}
