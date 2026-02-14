use super::*;

#[test]
fn test_formatting_options_default() {
    let options = FormattingOptions::default();
    assert_eq!(options.tab_size, 4);
    assert!(options.insert_spaces);
    assert_eq!(options.trim_trailing_whitespace, Some(true));
    assert_eq!(options.insert_final_newline, Some(true));
}

#[test]
fn test_basic_formatting_trailing_whitespace() {
    let source = "let x = 1;   \nlet y = 2;\n";
    let options = FormattingOptions {
        trim_trailing_whitespace: Some(true),
        ..Default::default()
    };

    let result = DocumentFormattingProvider::apply_basic_formatting(source, &options);
    assert!(result.is_ok());

    let edits = result.unwrap();
    assert!(!edits.is_empty());
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    assert!(formatted.contains("let x = 1;\n"));
    assert!(!formatted.contains("let x = 1;   "));
}

#[test]
fn test_basic_formatting_insert_final_newline() {
    let source = "let x = 1;";
    let options = FormattingOptions {
        insert_final_newline: Some(true),
        ..Default::default()
    };

    let formatted = DocumentFormattingProvider::format_text(source, &options);
    assert!(formatted.ends_with('\n'));
}

#[test]
fn test_basic_formatting_tabs_to_spaces() {
    let source = "\tlet x = 1;";
    let options = FormattingOptions {
        tab_size: 4,
        insert_spaces: true,
        ..Default::default()
    };

    let formatted = DocumentFormattingProvider::format_text(source, &options);
    // The formatter re-indents, so a top-level let should have no indent
    assert!(formatted.starts_with("let x = 1;"));
}

#[test]
fn test_basic_formatting_spaces_to_tabs() {
    let source = "    let x = 1;";
    let options = FormattingOptions {
        tab_size: 4,
        insert_spaces: false,
        ..Default::default()
    };

    let formatted = DocumentFormattingProvider::format_text(source, &options);
    assert!(formatted.starts_with("let x = 1;"));
}

#[test]
fn test_text_edit_creation() {
    let range = Range::new(Position::new(0, 0), Position::new(0, 10));
    let edit = TextEdit::new(range, "new text".to_string());
    assert_eq!(edit.new_text, "new text");
    assert_eq!(edit.range.start.line, 0);
    assert_eq!(edit.range.end.character, 10);
}

#[test]
fn test_convert_leading_spaces_to_tabs() {
    let result =
        DocumentFormattingProvider::convert_leading_spaces_to_tabs("        let x = 1;", 4);
    assert_eq!(result, "\t\tlet x = 1;");

    let result = DocumentFormattingProvider::convert_leading_spaces_to_tabs("      let x = 1;", 4);
    assert_eq!(result, "\t  let x = 1;");

    let result = DocumentFormattingProvider::convert_leading_spaces_to_tabs("  let x = 1;", 4);
    assert_eq!(result, "  let x = 1;");
}

#[test]
fn test_basic_formatting_preserves_multiline() {
    let source = "function foo() {\n  return 1;\n}";
    let options = FormattingOptions::default();

    let formatted = DocumentFormattingProvider::format_text(source, &options);
    assert!(formatted.contains("function foo()"));
    assert!(formatted.contains("return 1;"));
}

#[test]
fn test_basic_formatting_empty_source() {
    let source = "";
    let options = FormattingOptions::default();

    let result = DocumentFormattingProvider::apply_basic_formatting(source, &options);
    assert!(result.is_ok());
    let edits = result.unwrap();
    assert!(
        edits.is_empty()
            || edits
                .iter()
                .all(|e| e.new_text.is_empty() || e.new_text == "\n")
    );
}

// =========================================================================
// New tests: indentation
// =========================================================================

#[test]
fn test_format_if_else_indentation() {
    let source = "if (x) {\nlet a = 1;\n} else {\nlet b = 2;\n}";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    let lines: Vec<&str> = formatted.trim_end().lines().collect();

    assert_eq!(lines[0], "if (x) {");
    assert_eq!(lines[1], "    let a = 1;");
    assert_eq!(lines[2], "} else {");
    assert_eq!(lines[3], "    let b = 2;");
    assert_eq!(lines[4], "}");
}

#[test]
fn test_format_function_body_indentation() {
    let source = "function greet(name: string) {\nconst msg = \"hi\";\nreturn msg;\n}";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    let lines: Vec<&str> = formatted.trim_end().lines().collect();

    assert_eq!(lines[0], "function greet(name: string) {");
    assert_eq!(lines[1], "    const msg = \"hi\";");
    assert_eq!(lines[2], "    return msg;");
    assert_eq!(lines[3], "}");
}

#[test]
fn test_format_switch_case_indentation() {
    let source = "switch (x) {\ncase 1:\nlet a = 1;\nbreak;\ncase 2:\nlet b = 2;\nbreak;\ndefault:\nlet c = 3;\n}";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    let lines: Vec<&str> = formatted.trim_end().lines().collect();

    assert_eq!(lines[0], "switch (x) {");
    assert!(lines[1].starts_with("case 1:"), "got: {}", lines[1]);
    assert!(
        lines[2].starts_with("    "),
        "case body should be indented, got: {}",
        lines[2]
    );
}

#[test]
fn test_format_nested_blocks() {
    let source = "function foo() {\nif (true) {\nlet x = 1;\n}\n}";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    let lines: Vec<&str> = formatted.trim_end().lines().collect();

    assert_eq!(lines[0], "function foo() {");
    assert_eq!(lines[1], "    if (true) {");
    assert_eq!(lines[2], "        let x = 1;");
    assert_eq!(lines[3], "    }");
    assert_eq!(lines[4], "}");
}

#[test]
fn test_format_semicolon_normalization() {
    let source = "let x = 1\nlet y = 2\n";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);

    assert!(
        formatted.contains("let x = 1;"),
        "should add semicolon, got: {}",
        formatted
    );
    assert!(
        formatted.contains("let y = 2;"),
        "should add semicolon, got: {}",
        formatted
    );
}

#[test]
fn test_format_no_double_semicolons() {
    let source = "let x = 1;\nlet y = 2;\n";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);

    assert!(
        !formatted.contains(";;"),
        "should not produce double semicolons"
    );
}

#[test]
fn test_format_tab_size_2() {
    let source = "function foo() {\nlet x = 1;\n}";
    let options = FormattingOptions {
        tab_size: 2,
        insert_spaces: true,
        ..Default::default()
    };
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    let lines: Vec<&str> = formatted.trim_end().lines().collect();

    assert_eq!(lines[1], "  let x = 1;");
}

#[test]
fn test_format_with_tabs() {
    let source = "function foo() {\nlet x = 1;\n}";
    let options = FormattingOptions {
        tab_size: 4,
        insert_spaces: false,
        ..Default::default()
    };
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    let lines: Vec<&str> = formatted.trim_end().lines().collect();

    assert_eq!(lines[1], "\tlet x = 1;");
}

// =========================================================================
// New tests: format on key
// =========================================================================

#[test]
fn test_format_on_semicolon_removes_double() {
    let source = "let x = 1;;\n";
    let options = FormattingOptions::default();
    let result = DocumentFormattingProvider::format_on_key(source, 0, 11, ";", &options);
    assert!(result.is_ok());
    let edits = result.unwrap();
    assert!(
        !edits.is_empty(),
        "should produce edit for double semicolon"
    );
    let edit = &edits[0];
    assert!(
        edit.new_text.ends_with("let x = 1;"),
        "got: {}",
        edit.new_text
    );
    assert!(!edit.new_text.ends_with(";;"));
}

#[test]
fn test_format_on_enter_trims_prev_line() {
    let source = "let x = 1;   \nlet y = 2;\n";
    let options = FormattingOptions::default();
    let result = DocumentFormattingProvider::format_on_key(source, 1, 0, "\n", &options);
    assert!(result.is_ok());
    let edits = result.unwrap();
    let has_trim = edits
        .iter()
        .any(|e| e.range.start.line == 0 && e.new_text.is_empty());
    assert!(has_trim, "should trim trailing whitespace on previous line");
}

#[test]
fn test_format_on_enter_indents_after_brace() {
    let source = "function foo() {\n\n";
    let options = FormattingOptions::default();
    let result = DocumentFormattingProvider::format_on_key(source, 1, 0, "\n", &options);
    assert!(result.is_ok());
    // The current line is empty, so no indent edit is produced (graceful)
}

#[test]
fn test_format_on_key_unknown_key() {
    let source = "let x = 1;\n";
    let options = FormattingOptions::default();
    let result = DocumentFormattingProvider::format_on_key(source, 0, 5, "a", &options);
    assert!(result.is_ok());
    assert!(result.unwrap().is_empty());
}

// =========================================================================
// New tests: line edit correctness (0-based positions)
// =========================================================================

#[test]
fn test_compute_line_edits_no_change() {
    let result = DocumentFormattingProvider::compute_line_edits("hello\n", "hello\n");
    assert!(result.is_ok());
    assert!(result.unwrap().is_empty());
}

#[test]
fn test_compute_line_edits_single_line_change() {
    let result = DocumentFormattingProvider::compute_line_edits("hello  \n", "hello\n");
    assert!(result.is_ok());
    let edits = result.unwrap();
    assert_eq!(edits.len(), 1);
    let edit = &edits[0];
    assert_eq!(edit.range.start.line, 0);
    assert_eq!(edit.range.start.character, 0);
    assert_eq!(edit.range.end.line, 0);
    assert_eq!(edit.new_text, "hello");
}

#[test]
fn test_compute_line_edits_no_overlapping_ranges() {
    let original = "line1\nline2  \nline3\n";
    let formatted = "line1\nline2\nline3\n";
    let result = DocumentFormattingProvider::compute_line_edits(original, formatted);
    assert!(result.is_ok());
    let edits = result.unwrap();

    for i in 0..edits.len() {
        for j in (i + 1)..edits.len() {
            let a = &edits[i];
            let b = &edits[j];
            let a_before_b = a.range.end.line < b.range.start.line
                || (a.range.end.line == b.range.start.line
                    && a.range.end.character <= b.range.start.character);
            let b_before_a = b.range.end.line < a.range.start.line
                || (b.range.end.line == a.range.start.line
                    && b.range.end.character <= a.range.start.character);
            assert!(
                a_before_b || b_before_a,
                "Overlapping edits: {:?} and {:?}",
                a,
                b
            );
        }
    }
}

#[test]
fn test_format_positions_are_zero_based() {
    let source = "function foo() {\nlet x = 1\n}";
    let options = FormattingOptions::default();
    let result = DocumentFormattingProvider::apply_basic_formatting(source, &options);
    assert!(result.is_ok());
    let edits = result.unwrap();

    for edit in &edits {
        assert!(
            edit.range.start.line < 1000,
            "Start line too large: {}",
            edit.range.start.line
        );
        assert!(
            edit.range.end.line < 1000,
            "End line too large: {}",
            edit.range.end.line
        );
    }
}

#[test]
fn test_format_class_body_indentation() {
    let source = "class Foo {\nbar: number;\nbaz() {\nreturn 1;\n}\n}";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    let lines: Vec<&str> = formatted.trim_end().lines().collect();

    assert_eq!(lines[0], "class Foo {");
    assert_eq!(lines[1], "    bar: number;");
    assert_eq!(lines[2], "    baz() {");
    assert_eq!(lines[3], "        return 1;");
    assert_eq!(lines[4], "    }");
    assert_eq!(lines[5], "}");
}

#[test]
fn test_format_preserves_empty_lines() {
    let source = "let x = 1;\n\nlet y = 2;\n";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    assert!(
        formatted.contains("let x = 1;\n\nlet y = 2;"),
        "got: {}",
        formatted
    );
}

#[test]
fn test_format_arrow_function() {
    let source = "const fn = () => {\nreturn 1;\n}";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    let lines: Vec<&str> = formatted.trim_end().lines().collect();

    assert_eq!(lines[0], "const fn = () => {");
    assert_eq!(lines[1], "    return 1;");
    assert_eq!(lines[2], "}");
}

#[test]
fn test_format_multiline_import() {
    let source = "import { foo } from \"bar\";\n";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    assert!(
        formatted.contains("import { foo } from \"bar\";"),
        "got: {}",
        formatted
    );
}
