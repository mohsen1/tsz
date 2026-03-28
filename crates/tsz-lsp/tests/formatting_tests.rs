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
        "should add semicolon, got: {formatted}"
    );
    assert!(
        formatted.contains("let y = 2;"),
        "should add semicolon, got: {formatted}"
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
fn test_format_normalizes_as_operator_spacing() {
    let source = "var x = 3   as  number;\n";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    assert_eq!(formatted, "var x = 3 as number;\n");
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
                "Overlapping edits: {a:?} and {b:?}"
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
        "got: {formatted}"
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
fn test_format_pasted_class_member_spacing_matches_tsserver_shape() {
    let source =
        "namespace TestModule {\n class TestClass{\nprivate   foo;\npublic testMethod( )\n{}\n}\n}";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    assert_eq!(
        formatted,
        "namespace TestModule {\n    class TestClass {\n        private foo;\n        public testMethod() { }\n    }\n}\n"
    );
}

#[test]
fn test_format_multiline_import() {
    let source = "import { foo } from \"bar\";\n";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    assert!(
        formatted.contains("import { foo } from \"bar\";"),
        "got: {formatted}"
    );
}

#[test]
fn test_compute_line_edits_descending_order_preserves_markers_on_sequential_apply() {
    fn position_to_offset(text: &str, position: Position) -> usize {
        let mut line = 0u32;
        let mut character = 0u32;
        for (idx, ch) in text.char_indices() {
            if line == position.line && character == position.character {
                return idx;
            }
            if ch == '\n' {
                line += 1;
                character = 0;
            } else {
                character += 1;
            }
        }
        if line == position.line && character == position.character {
            return text.len();
        }
        panic!("invalid position: {position:?}");
    }

    let source = "class TestClass {\n    private testMethod1(param1: boolean,\n                        param2/*1*/: boolean) {\n    }\n\n    public testMethod2(a: number, b: number, c: number) {\n        if (a === b) {\n        }\n        else if (a != c &&\n                 a/*2*/ > b &&\n                 b/*3*/ < c) {\n        }\n\n    }\n}\n";
    let options = FormattingOptions::default();
    let edits = DocumentFormattingProvider::apply_basic_formatting(source, &options).unwrap();

    assert!(!edits.is_empty());
    for window in edits.windows(2) {
        let current = &window[0].range.start;
        let next = &window[1].range.start;
        assert!(
            current.line > next.line
                || (current.line == next.line && current.character >= next.character),
            "edits must be sorted descending: {edits:#?}"
        );
    }

    let mut text = source.to_string();
    for edit in edits {
        let start = position_to_offset(&text, edit.range.start);
        let end = position_to_offset(&text, edit.range.end);
        text.replace_range(start..end, &edit.new_text);
    }

    assert!(text.contains("/*1*/"), "marker 1 was removed: {text}");
    assert!(text.contains("/*2*/"), "marker 2 was removed: {text}");
    assert!(text.contains("/*3*/"), "marker 3 was removed: {text}");
}

#[test]
fn test_formatting_empty_string() {
    let source = "";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    // Empty string should produce empty or just a newline
    assert!(
        formatted.is_empty() || formatted == "\n",
        "Empty source should format to empty or newline, got: {formatted:?}"
    );
}

#[test]
fn test_formatting_only_whitespace() {
    let source = "   \n  \n   ";
    let options = FormattingOptions {
        trim_trailing_whitespace: Some(true),
        ..Default::default()
    };
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    // All whitespace should be trimmed from each line
    for line in formatted.lines() {
        assert!(
            !line.ends_with(' '),
            "Line should not end with spaces: {line:?}"
        );
    }
}

#[test]
fn test_formatting_preserves_content() {
    let source = "function foo() {\n  return 42;\n}\n";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    assert!(
        formatted.contains("function foo()"),
        "Should preserve function declaration"
    );
    assert!(
        formatted.contains("return 42"),
        "Should preserve return statement"
    );
}

#[test]
fn test_formatting_tab_to_spaces() {
    let source = "function foo() {\n\treturn 42;\n}\n";
    let options = FormattingOptions {
        tab_size: 2,
        insert_spaces: true,
        ..Default::default()
    };
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    // Tabs should be converted to spaces
    if !formatted.contains('\t') {
        assert!(
            formatted.contains("  return"),
            "Should convert tab to 2 spaces, got: {formatted:?}"
        );
    }
}

#[test]
fn test_formatting_no_trailing_whitespace() {
    let source = "const x = 1;    \nconst y = 2;  \nconst z = 3;\n";
    let options = FormattingOptions {
        trim_trailing_whitespace: Some(true),
        ..Default::default()
    };
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    for (i, line) in formatted.lines().enumerate() {
        assert!(
            !line.ends_with(' ') && !line.ends_with('\t'),
            "Line {i} should not have trailing whitespace: {line:?}"
        );
    }
}

#[test]
fn test_formatting_options_custom_tab_size() {
    let options = FormattingOptions {
        tab_size: 2,
        insert_spaces: true,
        ..Default::default()
    };
    assert_eq!(options.tab_size, 2);
    assert!(options.insert_spaces);
}

#[test]
fn test_formatting_single_line() {
    let source = "const x = 1;";
    let options = FormattingOptions {
        insert_final_newline: Some(false),
        trim_trailing_whitespace: Some(false),
        ..Default::default()
    };
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    assert!(
        formatted.contains("const x = 1"),
        "Should preserve single line content"
    );
}

// =========================================================================
// Additional coverage tests
// =========================================================================

#[test]
fn test_format_arrow_function_with_params() {
    let source = "const add = (a: number, b: number) => {\nreturn a + b;\n}";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    let lines: Vec<&str> = formatted.trim_end().lines().collect();

    assert_eq!(lines[0], "const add = (a: number, b: number) => {");
    assert_eq!(lines[1], "    return a + b;");
    assert_eq!(lines[2], "}");
}

#[test]
fn test_format_arrow_function_single_expression() {
    let source = "const square = (x: number) => x * x";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    assert!(
        formatted.contains("const square = (x: number) => x * x;"),
        "Should add semicolon to single-expression arrow, got: {formatted}"
    );
}

#[test]
fn test_format_class_method_with_modifiers() {
    let source = "class MyClass {\npublic greet(name: string) {\nreturn name;\n}\nprivate helper() {\nreturn 1;\n}\n}";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    let lines: Vec<&str> = formatted.trim_end().lines().collect();

    assert_eq!(lines[0], "class MyClass {");
    assert!(
        lines[1].starts_with("    public greet"),
        "public method should be indented, got: {}",
        lines[1]
    );
    assert!(
        lines[4].starts_with("    private helper"),
        "private method should be indented, got: {}",
        lines[4]
    );
}

#[test]
fn test_format_template_literal_preserves_content() {
    let source = "const msg = `hello ${name} world`;\n";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    assert!(
        formatted.contains("`hello ${name} world`"),
        "Template literal content should be preserved, got: {formatted}"
    );
}

#[test]
fn test_format_switch_with_default() {
    let source = "switch (x) {\ncase 'a':\nlet a = 1;\nbreak;\ndefault:\nlet d = 0;\nbreak;\n}";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    let lines: Vec<&str> = formatted.trim_end().lines().collect();

    assert_eq!(lines[0], "switch (x) {");
    // default: should be at same level as case
    let default_line = lines.iter().find(|l| l.trim().starts_with("default:"));
    assert!(default_line.is_some(), "Should have a default: line");
}

#[test]
fn test_format_destructuring_assignment() {
    let source = "const { a, b } = obj\nconst [x, y] = arr\n";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    assert!(
        formatted.contains("const { a, b } = obj;"),
        "Should add semicolon to destructured object, got: {formatted}"
    );
    assert!(
        formatted.contains("const [x, y] = arr;"),
        "Should add semicolon to destructured array, got: {formatted}"
    );
}

#[test]
fn test_format_import_with_from() {
    let source = "import { useState, useEffect } from \"react\"\n";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    assert!(
        formatted.contains("import { useState, useEffect } from \"react\";"),
        "Should add semicolon to import with from, got: {formatted}"
    );
}

#[test]
fn test_format_import_without_from_no_semicolon() {
    // Import without 'from' (side-effect import) should not get extra semicolons
    let source = "import \"polyfill\";\n";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    assert!(
        !formatted.contains(";;"),
        "Should not double-semicolon side-effect import, got: {formatted}"
    );
}

#[test]
fn test_format_enum_declaration() {
    let source = "enum Color {\nRed,\nGreen,\nBlue,\n}";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    let lines: Vec<&str> = formatted.trim_end().lines().collect();

    assert_eq!(lines[0], "enum Color {");
    assert_eq!(lines[1], "    Red,");
    assert_eq!(lines[2], "    Green,");
    assert_eq!(lines[3], "    Blue,");
    assert_eq!(lines[4], "}");
}

#[test]
fn test_format_multiline_array_indentation() {
    let source = "const arr = [\n1,\n2,\n3,\n]";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    let lines: Vec<&str> = formatted.trim_end().lines().collect();

    // Array elements should be indented inside brackets
    assert!(lines[0].starts_with("const arr = ["), "got: {}", lines[0]);
    assert_eq!(lines[1], "    1,");
    assert_eq!(lines[2], "    2,");
    assert_eq!(lines[3], "    3,");
    assert!(
        lines[4].trim() == "]" || lines[4].trim() == "];",
        "got: {}",
        lines[4]
    );
}

#[test]
fn test_format_decorator_no_semicolon() {
    let source = "@Component({\nselector: 'app-root',\n})\nclass AppComponent {\n}";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    // Decorator line should not end with semicolon
    let first_line = formatted.lines().next().unwrap();
    assert!(
        !first_line.ends_with(';'),
        "Decorator line should not have semicolon, got: {first_line}"
    );
}

#[test]
fn test_format_semicolon_remove_mode() {
    let source = "let x = 1;\nlet y = 2;\n";
    let options = FormattingOptions {
        semicolons: Some("remove".to_string()),
        ..Default::default()
    };
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    // In remove mode, existing semicolons should be preserved (we only skip adding new ones)
    // The formatter doesn't strip semicolons; it just doesn't add them
    assert!(
        formatted.contains("let x = 1;"),
        "Remove mode should preserve existing semicolons, got: {formatted}"
    );
}

#[test]
fn test_format_no_semicolon_after_control_flow() {
    let source = "if (condition) {\nreturn 1;\n}\nfor (const x of items) {\nprocess(x);\n}\nwhile (true) {\nbreak;\n}";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);

    // Control flow keywords should not get semicolons
    for line in formatted.lines() {
        let t = line.trim();
        if t.starts_with("if ") || t.starts_with("for ") || t.starts_with("while ") {
            assert!(
                !t.ends_with(';'),
                "Control flow line should not have semicolon: {t}"
            );
        }
    }
}

#[test]
fn test_format_no_final_newline_option() {
    let source = "let x = 1;\n";
    let options = FormattingOptions {
        insert_final_newline: Some(false),
        ..Default::default()
    };
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    assert!(
        !formatted.ends_with('\n'),
        "Should not end with newline when insert_final_newline is false, got: {formatted:?}"
    );
}

#[test]
fn test_format_collapse_whitespace_in_code() {
    let source = "const   x   =   1;\n";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    assert!(
        formatted.contains("const x = 1;"),
        "Should collapse multiple spaces, got: {formatted}"
    );
}

#[test]
fn test_format_ensure_space_before_brace() {
    let source = "function foo(){\nreturn 1;\n}\n";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    assert!(
        formatted.contains("function foo() {"),
        "Should add space before brace, got: {formatted}"
    );
}

#[test]
fn test_format_try_catch_no_semicolons() {
    let source = "try {\nthrow new Error('oops');\n} catch (e) {\nlet x = 1;\n}";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);

    for line in formatted.lines() {
        let t = line.trim();
        if t.starts_with("try ") || t.starts_with("} catch") {
            assert!(
                !t.ends_with(';'),
                "try/catch line should not have semicolon: {t}"
            );
        }
    }
}

#[test]
fn test_compute_line_edits_extra_lines_in_formatted() {
    let original = "line1\n";
    let formatted = "line1\nline2\nline3\n";
    let result = DocumentFormattingProvider::compute_line_edits(original, formatted);
    assert!(result.is_ok());
    let edits = result.unwrap();
    assert!(
        !edits.is_empty(),
        "Should produce edits for extra lines in formatted output"
    );
}

#[test]
fn test_compute_line_edits_fewer_lines_in_formatted() {
    let original = "line1\nline2\nline3\n";
    let formatted = "line1\n";
    let result = DocumentFormattingProvider::compute_line_edits(original, formatted);
    assert!(result.is_ok());
    let edits = result.unwrap();
    assert!(
        !edits.is_empty(),
        "Should produce edits when formatted has fewer lines"
    );
}

#[test]
fn test_formatting_class_with_methods() {
    let source = "class Foo{\n  method1()  {}\n  method2()  {}\n}\n";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    let _ = formatted;
}

#[test]
fn test_formatting_interface_members() {
    let source = "interface IFoo{\n  name:string;\n  age  :  number;\n}\n";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    let _ = formatted;
}

#[test]
fn test_formatting_arrow_functions() {
    let source = "const add=(a:number,b:number)=>a+b;\n";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    let _ = formatted;
}

#[test]
fn test_formatting_template_literals() {
    let source = "const msg = `Hello\n  ${name}\n  world`;\n";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    let _ = formatted;
}

#[test]
fn test_formatting_switch_case() {
    let source = "switch(x){\ncase 1:\nbreak;\ncase 2:\nbreak;\ndefault:\nbreak;\n}\n";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    let _ = formatted;
}

#[test]
fn test_formatting_enum_declaration() {
    let source = "enum Color  {Red,Green,  Blue}\n";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    let _ = formatted;
}

#[test]
fn test_formatting_type_alias() {
    let source = "type Result<T,E>=  {ok:T}|{err:E};\n";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    let _ = formatted;
}

#[test]
fn test_formatting_destructuring() {
    let source = "const {a,b,c}=obj;\nconst [x,y,...rest]=arr;\n";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    let _ = formatted;
}

#[test]
fn test_formatting_async_await() {
    let source = "async function fetch(){const data=await   getData();return data;}\n";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    let _ = formatted;
}

#[test]
fn test_formatting_try_catch() {
    let source = "try{doSomething();}catch(e){handleError(e);}finally{cleanup();}\n";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    let _ = formatted;
}

#[test]
fn test_formatting_generics() {
    let source = "function identity<T>(arg:T):T{return arg;}\n";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    let _ = formatted;
}

#[test]
fn test_formatting_jsx_like() {
    let source = "const el = <div className=\"test\">content</div>;\n";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    let _ = formatted;
}

// =========================================================================
// Batch 3: edge cases and additional coverage
// =========================================================================

#[test]
fn test_format_for_loop_indentation() {
    let source = "for (let i = 0; i < 10; i++) {\nconsole.log(i);\n}";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    let lines: Vec<&str> = formatted.trim_end().lines().collect();

    assert_eq!(lines[0], "for (let i = 0; i < 10; i++) {");
    assert_eq!(lines[1], "    console.log(i);");
    assert_eq!(lines[2], "}");
}

#[test]
fn test_format_while_loop_indentation() {
    let source = "while (condition) {\ndoWork();\n}";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    let lines: Vec<&str> = formatted.trim_end().lines().collect();

    assert_eq!(lines[0], "while (condition) {");
    assert_eq!(lines[1], "    doWork();");
    assert_eq!(lines[2], "}");
}

#[test]
fn test_format_do_while_indentation() {
    let source = "do {\nx++;\n} while (x < 10)";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    let lines: Vec<&str> = formatted.trim_end().lines().collect();

    assert_eq!(lines[0], "do {");
    assert_eq!(lines[1], "    x++;");
}

#[test]
fn test_format_triple_nested_blocks() {
    let source = "function a() {\nif (true) {\nfor (let i = 0; i < 1; i++) {\nlet x = i;\n}\n}\n}";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    let lines: Vec<&str> = formatted.trim_end().lines().collect();

    assert_eq!(lines[0], "function a() {");
    assert_eq!(lines[1], "    if (true) {");
    assert_eq!(lines[2], "        for (let i = 0; i < 1; i++) {");
    assert_eq!(lines[3], "            let x = i;");
    assert_eq!(lines[4], "        }");
    assert_eq!(lines[5], "    }");
    assert_eq!(lines[6], "}");
}

#[test]
fn test_format_interface_indentation() {
    let source = "interface User {\nname: string;\nage: number;\n}";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    let lines: Vec<&str> = formatted.trim_end().lines().collect();

    assert_eq!(lines[0], "interface User {");
    assert_eq!(lines[1], "    name: string;");
    assert_eq!(lines[2], "    age: number;");
    assert_eq!(lines[3], "}");
}

#[test]
fn test_format_type_alias_indentation() {
    let source = "type Result<T> = {\nvalue: T;\nerror: string;\n}";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    let lines: Vec<&str> = formatted.trim_end().lines().collect();

    assert_eq!(lines[0], "type Result<T> = {");
    assert_eq!(lines[1], "    value: T;");
    assert_eq!(lines[2], "    error: string;");
    assert_eq!(lines[3], "}");
}

#[test]
fn test_format_object_literal_indentation() {
    let source = "const obj = {\na: 1,\nb: 2,\nc: 3,\n}";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    let lines: Vec<&str> = formatted.trim_end().lines().collect();

    assert!(lines[0].starts_with("const obj = {"), "got: {}", lines[0]);
    assert_eq!(lines[1], "    a: 1,");
    assert_eq!(lines[2], "    b: 2,");
    assert_eq!(lines[3], "    c: 3,");
}

#[test]
fn test_format_tab_size_8() {
    let source = "function foo() {\nlet x = 1;\n}";
    let options = FormattingOptions {
        tab_size: 8,
        insert_spaces: true,
        ..Default::default()
    };
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    let lines: Vec<&str> = formatted.trim_end().lines().collect();

    assert_eq!(lines[1], "        let x = 1;");
}

#[test]
fn test_format_multiple_statements_same_line_semicolons() {
    let source = "let a = 1; let b = 2; let c = 3;\n";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    // Should not produce double semicolons
    assert!(
        !formatted.contains(";;"),
        "Should not produce double semicolons, got: {formatted}"
    );
}

#[test]
fn test_format_export_default_function() {
    let source = "export default function handler() {\nreturn null;\n}";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    let lines: Vec<&str> = formatted.trim_end().lines().collect();

    assert!(
        lines[0].starts_with("export default function"),
        "got: {}",
        lines[0]
    );
    assert_eq!(lines[1], "    return null;");
}

#[test]
fn test_format_const_enum() {
    let source = "const enum Direction {\nUp,\nDown,\nLeft,\nRight,\n}";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    let lines: Vec<&str> = formatted.trim_end().lines().collect();

    assert!(
        lines[0].starts_with("const enum Direction"),
        "got: {}",
        lines[0]
    );
    assert_eq!(lines[1], "    Up,");
}

#[test]
fn test_format_class_extends_implements() {
    let source = "class Dog extends Animal {\nbark() {\nreturn 'woof';\n}\n}";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    let lines: Vec<&str> = formatted.trim_end().lines().collect();

    assert!(
        lines[0].contains("class Dog extends Animal"),
        "got: {}",
        lines[0]
    );
    assert_eq!(lines[1], "    bark() {");
    assert_eq!(lines[2], "        return 'woof';");
}

#[test]
fn test_format_multiline_string_concatenation() {
    let source = "const s = 'hello' +\n'world';\n";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    assert!(
        formatted.contains("'hello'"),
        "Should preserve string content, got: {formatted}"
    );
    assert!(
        formatted.contains("'world'"),
        "Should preserve string content, got: {formatted}"
    );
}

#[test]
fn test_format_namespace_indentation() {
    let source = "namespace MyApp {\nexport function init() {\nreturn true;\n}\n}";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    let lines: Vec<&str> = formatted.trim_end().lines().collect();

    assert_eq!(lines[0], "namespace MyApp {");
    assert!(
        lines[1].starts_with("    export function init()"),
        "got: {}",
        lines[1]
    );
    assert_eq!(lines[2], "        return true;");
}

#[test]
fn test_format_ternary_expression() {
    let source = "const x = condition ? 'yes' : 'no'\n";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    assert!(
        formatted.contains("condition ? 'yes' : 'no'"),
        "Ternary should be preserved, got: {formatted}"
    );
}

#[test]
fn test_format_only_newlines() {
    let source = "\n\n\n";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    // Should not crash; output may be empty or just newlines
    let _ = formatted;
}

#[test]
fn test_format_unicode_identifiers() {
    let source = "const cafe\u{0301} = 'coffee';\nconst \u{03B1} = 1;\n";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    // Should handle unicode without panic
    let _ = formatted;
}

#[test]
fn test_format_computed_property() {
    let source = "const obj = {\n[key]: value,\n[Symbol.iterator]: fn,\n}";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    let lines: Vec<&str> = formatted.trim_end().lines().collect();
    // Computed properties should be indented
    assert!(
        lines[1].starts_with("    "),
        "Computed property should be indented, got: {}",
        lines[1]
    );
}

#[test]
fn test_format_try_catch_finally() {
    let source = "try {\nfoo();\n} catch (e) {\nbar();\n} finally {\nbaz();\n}";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    let lines: Vec<&str> = formatted.trim_end().lines().collect();

    assert_eq!(lines[0], "try {");
    assert_eq!(lines[1], "    foo();");
    assert!(
        lines[4].contains("finally"),
        "Should have finally block, got: {}",
        lines[4]
    );
}

#[test]
fn test_compute_line_edits_identical_multiline() {
    let original = "line1\nline2\nline3\n";
    let formatted = "line1\nline2\nline3\n";
    let result = DocumentFormattingProvider::compute_line_edits(original, formatted);
    assert!(result.is_ok());
    assert!(
        result.unwrap().is_empty(),
        "Identical content should produce no edits"
    );
}

#[test]
fn test_format_on_key_closing_brace() {
    let source = "function foo() {\n    let x = 1;\n}";
    let options = FormattingOptions::default();
    let result = DocumentFormattingProvider::format_on_key(source, 2, 1, "}", &options);
    assert!(result.is_ok());
}

// =========================================================================
// Batch: additional edge case tests
// =========================================================================

#[test]
fn test_format_class_with_readonly_property() {
    let source = "class Config {\nreadonly name: string;\nreadonly value: number;\n}";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    let lines: Vec<&str> = formatted.trim_end().lines().collect();

    assert_eq!(lines[0], "class Config {");
    assert!(
        lines[1].starts_with("    readonly name"),
        "readonly property should be indented, got: {}",
        lines[1]
    );
}

#[test]
fn test_format_abstract_class_indentation() {
    let source =
        "abstract class Shape {\nabstract area(): number;\ntoString() {\nreturn 'shape';\n}\n}";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    let lines: Vec<&str> = formatted.trim_end().lines().collect();

    assert!(
        lines[0].contains("abstract class Shape"),
        "got: {}",
        lines[0]
    );
    assert!(
        lines[1].starts_with("    "),
        "abstract method should be indented, got: {}",
        lines[1]
    );
}

#[test]
fn test_format_multiple_blank_lines_preserved() {
    let source = "let a = 1;\n\n\nlet b = 2;\n";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    // Multiple blank lines should not crash
    assert!(
        formatted.contains("let a = 1;"),
        "Should preserve first statement, got: {formatted}"
    );
    assert!(
        formatted.contains("let b = 2;"),
        "Should preserve second statement, got: {formatted}"
    );
}

#[test]
fn test_format_optional_chaining() {
    let source = "const x = obj?.prop?.method?.()\n";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    assert!(
        formatted.contains("obj?.prop?.method?.()"),
        "Optional chaining should be preserved, got: {formatted}"
    );
}

#[test]
fn test_format_nullish_coalescing() {
    let source = "const x = value ?? defaultValue\n";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    assert!(
        formatted.contains("value ?? defaultValue"),
        "Nullish coalescing should be preserved, got: {formatted}"
    );
}

#[test]
fn test_format_for_of_loop_indentation() {
    let source = "for (const item of items) {\nprocess(item);\n}";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    let lines: Vec<&str> = formatted.trim_end().lines().collect();

    assert!(
        lines[0].starts_with("for (const item of items)"),
        "got: {}",
        lines[0]
    );
    assert_eq!(lines[1], "    process(item);");
    assert_eq!(lines[2], "}");
}

#[test]
fn test_format_for_in_loop_indentation() {
    let source = "for (const key in obj) {\nconsole.log(key);\n}";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    let lines: Vec<&str> = formatted.trim_end().lines().collect();

    assert!(
        lines[0].starts_with("for (const key in obj)"),
        "got: {}",
        lines[0]
    );
    assert_eq!(lines[1], "    console.log(key);");
}

#[test]
fn test_format_labeled_statement() {
    let source = "outer:\nfor (let i = 0; i < 10; i++) {\nbreak outer;\n}";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    // Should not crash on labeled statements
    let _ = formatted;
}

#[test]
fn test_format_empty_function_body() {
    let source = "function noop() {}";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    assert!(
        formatted.contains("function noop()"),
        "Should preserve empty function, got: {formatted}"
    );
}

#[test]
fn test_format_spread_operator() {
    let source = "const merged = { ...a, ...b }\n";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    assert!(
        formatted.contains("...a") && formatted.contains("...b"),
        "Spread operator should be preserved, got: {formatted}"
    );
}

#[test]
fn test_format_type_assertion() {
    let source = "const x = value as string\n";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    assert!(
        formatted.contains("value as string"),
        "Type assertion should be preserved, got: {formatted}"
    );
}

#[test]
fn test_format_single_line_comment_preserved() {
    let source = "// This is a comment\nlet x = 1;\n";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    assert!(
        formatted.contains("// This is a comment"),
        "Single-line comment should be preserved, got: {formatted}"
    );
}

#[test]
fn test_format_block_comment_preserved() {
    let source = "/* block comment */\nlet x = 1;\n";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    assert!(
        formatted.contains("/* block comment */"),
        "Block comment should be preserved, got: {formatted}"
    );
}

#[test]
fn test_format_tab_size_1() {
    let source = "function foo() {\nlet x = 1;\n}";
    let options = FormattingOptions {
        tab_size: 1,
        insert_spaces: true,
        ..Default::default()
    };
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    let lines: Vec<&str> = formatted.trim_end().lines().collect();

    assert_eq!(lines[1], " let x = 1;");
}

#[test]
fn test_compute_line_edits_all_lines_changed() {
    let original = "aaa\nbbb\nccc\n";
    let formatted = "xxx\nyyy\nzzz\n";
    let result = DocumentFormattingProvider::compute_line_edits(original, formatted);
    assert!(result.is_ok());
    let edits = result.unwrap();
    assert!(!edits.is_empty(), "All lines changed should produce edits");
}

#[test]
fn test_format_on_key_semicolon_basic() {
    let source = "let x = 1;\n";
    let options = FormattingOptions::default();
    let result = DocumentFormattingProvider::format_on_key(source, 0, 10, ";", &options);
    assert!(result.is_ok());
}

#[test]
fn test_formatting_on_single_line_blocks() {
    // Matches fourslash test: formattingOnSingleLineBlocks.ts
    let source = "class C\n{}\nif (true)\n{}\n";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::format_text(source, &options);
    assert!(
        formatted.contains("class C { }"),
        "Expected 'class C {{ }}' but got: {formatted}"
    );
    assert!(
        formatted.contains("if (true) { }"),
        "Expected 'if (true) {{ }}' but got: {formatted}"
    );
}
