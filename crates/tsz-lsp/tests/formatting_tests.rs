use super::*;

// =========================================================================
// Options + TextEdit basics
// =========================================================================

#[test]
fn test_formatting_options_default() {
    let options = FormattingOptions::default();
    assert_eq!(options.tab_size, 4);
    assert!(options.insert_spaces);
    assert_eq!(options.trim_trailing_whitespace, Some(true));
    assert_eq!(options.insert_final_newline, Some(true));
    assert_eq!(options.trim_final_newlines, Some(true));
    assert_eq!(options.semicolons, None);
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
fn test_formatting_options_custom_tab_size() {
    let options = FormattingOptions {
        tab_size: 2,
        insert_spaces: true,
        ..Default::default()
    };
    assert_eq!(options.tab_size, 2);
    assert!(options.insert_spaces);
}

// =========================================================================
// FallbackFormattingMode capability boundary
// =========================================================================

#[test]
fn test_fallback_mode_variants_are_distinct() {
    // The enum is the documented capability boundary between safe whitespace
    // cleanup and "needs a real parser". Guard against a future merge that
    // silently collapses the two variants.
    assert_ne!(
        FallbackFormattingMode::WhitespaceOnly,
        FallbackFormattingMode::UnsupportedForStructuralFormatting
    );
}

// =========================================================================
// Conservative fallback: whitespace-only operations
// =========================================================================

#[test]
fn test_fallback_trims_trailing_whitespace_only() {
    let source = "let x = 1;   \nlet y = 2;\n";
    let options = FormattingOptions {
        trim_trailing_whitespace: Some(true),
        ..Default::default()
    };
    let formatted = DocumentFormattingProvider::safe_whitespace_text(source, &options);
    assert_eq!(formatted, "let x = 1;\nlet y = 2;\n");
}

#[test]
fn test_fallback_respects_trim_trailing_whitespace_disabled() {
    let source = "let x = 1;   \nlet y = 2;\n";
    let options = FormattingOptions {
        trim_trailing_whitespace: Some(false),
        ..Default::default()
    };
    let formatted = DocumentFormattingProvider::safe_whitespace_text(source, &options);
    assert_eq!(formatted, "let x = 1;   \nlet y = 2;\n");
}

#[test]
fn test_fallback_adds_final_newline_when_missing() {
    let source = "let x = 1;";
    let options = FormattingOptions {
        insert_final_newline: Some(true),
        ..Default::default()
    };
    let formatted = DocumentFormattingProvider::safe_whitespace_text(source, &options);
    assert_eq!(formatted, "let x = 1;\n");
}

#[test]
fn test_fallback_does_not_add_final_newline_when_disabled() {
    let source = "let x = 1;";
    let options = FormattingOptions {
        insert_final_newline: Some(false),
        trim_final_newlines: Some(false),
        ..Default::default()
    };
    let formatted = DocumentFormattingProvider::safe_whitespace_text(source, &options);
    assert_eq!(formatted, "let x = 1;");
}

#[test]
fn test_fallback_empty_source_stays_empty() {
    let source = "";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::safe_whitespace_text(source, &options);
    assert_eq!(
        formatted, "",
        "empty input must not become a newline-only file"
    );
}

#[test]
fn test_fallback_trims_final_blank_lines_when_enabled() {
    let source = "let x = 1;\n\n\n\n";
    let options = FormattingOptions {
        trim_final_newlines: Some(true),
        insert_final_newline: Some(true),
        ..Default::default()
    };
    let formatted = DocumentFormattingProvider::safe_whitespace_text(source, &options);
    assert_eq!(formatted, "let x = 1;\n");
}

// =========================================================================
// Fallback must NOT perform structural rewrites
// =========================================================================

#[test]
fn test_fallback_does_not_insert_semicolons() {
    // Previously, the heuristic formatter would append `;` to statement lines.
    // The conservative fallback must not.
    let source = "let x = 1\nlet y = 2\n";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::safe_whitespace_text(source, &options);
    assert_eq!(
        formatted, source,
        "fallback must not insert semicolons, got: {formatted:?}"
    );
}

#[test]
fn test_fallback_does_not_remove_semicolons() {
    let source = "let x = 1;\nlet y = 2;\n";
    let options = FormattingOptions {
        semicolons: Some("remove".to_string()),
        ..Default::default()
    };
    let formatted = DocumentFormattingProvider::safe_whitespace_text(source, &options);
    assert_eq!(
        formatted, source,
        "fallback must not remove semicolons even when option asks for it"
    );
}

#[test]
fn test_fallback_does_not_rewrite_brace_spacing() {
    let source = "function foo(){\nreturn 1;\n}\n";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::safe_whitespace_text(source, &options);
    assert_eq!(
        formatted, source,
        "fallback must not insert a space before '{{', got: {formatted:?}"
    );
}

#[test]
fn test_fallback_does_not_collapse_empty_block() {
    // Previously `... ()\n{}` was heuristically merged into `...() { }`.
    let source = "class C\n{}\nif (true)\n{}\n";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::safe_whitespace_text(source, &options);
    assert_eq!(
        formatted, source,
        "fallback must not merge single-line blocks"
    );
}

#[test]
fn test_fallback_does_not_reindent() {
    // The input deliberately has no leading indentation inside the block.
    // A syntax-aware formatter would indent; the conservative fallback must not.
    let source = "function foo() {\nlet x = 1;\n}\n";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::safe_whitespace_text(source, &options);
    assert_eq!(
        formatted, source,
        "fallback must not infer indentation from braces"
    );
}

#[test]
fn test_fallback_does_not_normalize_as_operator() {
    let source = "var x = 3   as  number;\n";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::safe_whitespace_text(source, &options);
    assert_eq!(formatted, source, "fallback must not rewrite `as` spacing");
}

#[test]
fn test_fallback_does_not_collapse_internal_whitespace() {
    let source = "const   x   =   1;\n";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::safe_whitespace_text(source, &options);
    assert_eq!(
        formatted, source,
        "fallback must not collapse internal whitespace in code"
    );
}

#[test]
fn test_fallback_does_not_touch_tabs_as_indentation() {
    // Converting tab → spaces inside code is syntax-sensitive (the tab may be
    // inside a string or regex literal), so the fallback must leave it alone.
    let source = "function foo() {\n\treturn 42;\n}\n";
    let options = FormattingOptions {
        tab_size: 2,
        insert_spaces: true,
        ..Default::default()
    };
    let formatted = DocumentFormattingProvider::safe_whitespace_text(source, &options);
    assert_eq!(
        formatted, source,
        "fallback must not convert tabs to spaces"
    );
}

// =========================================================================
// Preservation of syntax-sensitive constructs
// =========================================================================

#[test]
fn test_fallback_preserves_template_literals() {
    let source = "const msg = `hello ${name}\n  world`;\n";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::safe_whitespace_text(source, &options);
    assert_eq!(
        formatted, source,
        "template literal must be preserved exactly"
    );
}

#[test]
fn test_fallback_preserves_regex_literals() {
    // A regex containing `/`, `{`, and `}` characters could easily confuse
    // naive text heuristics. The fallback must pass it through untouched.
    let source = "const re = /\\/foo\\{bar\\}baz/g;\nconst s = 'a/b/c';\n";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::safe_whitespace_text(source, &options);
    assert_eq!(formatted, source);
}

#[test]
fn test_fallback_preserves_multiline_generics_and_conditionals() {
    let source = "type Unwrap<T> = T extends Promise<\n    infer U\n> ? U : T;\n";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::safe_whitespace_text(source, &options);
    assert_eq!(formatted, source);
}

#[test]
fn test_fallback_preserves_decorators() {
    let source = "@Component({\n    selector: 'app-root',\n})\nclass AppComponent {}\n";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::safe_whitespace_text(source, &options);
    assert_eq!(formatted, source);
}

#[test]
fn test_fallback_preserves_tsx_like_syntax() {
    let source = "const el = <div className=\"t\">{value}</div>;\n";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::safe_whitespace_text(source, &options);
    assert_eq!(formatted, source);
}

#[test]
fn test_fallback_preserves_string_literals_with_whitespace() {
    // Whitespace inside string literals must never be touched, even with
    // trailing-whitespace trimming on.
    let source = "const s = \"hello   world\";\n";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::safe_whitespace_text(source, &options);
    assert_eq!(formatted, source);
}

#[test]
fn test_fallback_preserves_complex_typescript_input() {
    // Larger integration-shaped fixture. The fallback may only adjust EOF
    // newline / trailing whitespace. Non-whitespace content must be identical.
    let source = "\
import { foo } from \"bar\";

@decorator({ option: true })
class Complex<T extends Record<string, unknown>> {
    private readonly items: Array<T> = [];

    async process(input: string): Promise<T | null> {
        const pattern = /^[a-z]+/i;
        const label = `<${input}>`;
        return input.match(pattern) ? null : (label as unknown as T);
    }
}
";
    let options = FormattingOptions::default();
    let formatted = DocumentFormattingProvider::safe_whitespace_text(source, &options);
    assert_eq!(
        formatted, source,
        "fallback must not touch complex TS content"
    );
}

// =========================================================================
// apply_safe_whitespace_formatting produces edits consistent with text form
// =========================================================================

#[test]
fn test_apply_safe_whitespace_formatting_trailing_whitespace_edits() {
    let source = "let x = 1;   \nlet y = 2;\n";
    let options = FormattingOptions::default();
    let edits =
        DocumentFormattingProvider::apply_safe_whitespace_formatting(source, &options).unwrap();
    assert!(
        !edits.is_empty(),
        "trailing whitespace should produce at least one edit"
    );
    // Applying the edit must yield the whitespace-only normalized form.
    let expected = DocumentFormattingProvider::safe_whitespace_text(source, &options);
    let applied = apply_text_edits(source, &edits);
    assert_eq!(applied, expected);
}

#[test]
fn test_apply_safe_whitespace_formatting_noop_when_clean() {
    let source = "let x = 1;\nlet y = 2;\n";
    let options = FormattingOptions::default();
    let edits =
        DocumentFormattingProvider::apply_safe_whitespace_formatting(source, &options).unwrap();
    assert!(edits.is_empty(), "clean input must produce no edits");
}

#[test]
fn test_apply_safe_whitespace_formatting_empty_source() {
    let source = "";
    let options = FormattingOptions::default();
    let edits =
        DocumentFormattingProvider::apply_safe_whitespace_formatting(source, &options).unwrap();
    assert!(edits.is_empty(), "empty source must not synthesize edits");
}

// =========================================================================
// compute_line_edits
// =========================================================================

#[test]
fn test_compute_line_edits_no_change() {
    let edits = DocumentFormattingProvider::compute_line_edits("hello\n", "hello\n").unwrap();
    assert!(edits.is_empty());
}

#[test]
fn test_compute_line_edits_single_line_change_emits_one_line_edit() {
    let edits = DocumentFormattingProvider::compute_line_edits("hello  \n", "hello\n").unwrap();
    assert_eq!(edits.len(), 1);
    let edit = &edits[0];
    assert_eq!(edit.range.start.line, 0);
    assert_eq!(edit.range.start.character, 0);
    assert_eq!(edit.range.end.line, 0);
    assert_eq!(edit.new_text, "hello");
}

#[test]
fn test_compute_line_edits_same_line_count_emits_per_line_edits() {
    let original = "a  \nb\nc  \n";
    let formatted = "a\nb\nc\n";
    let edits = DocumentFormattingProvider::compute_line_edits(original, formatted).unwrap();
    assert_eq!(edits.len(), 2);
    // Edits must be descending (bottom-to-top) so that sequential application
    // does not invalidate later ranges.
    assert!(edits[0].range.start.line >= edits[1].range.start.line);
    let applied = apply_text_edits(original, &edits);
    assert_eq!(applied, formatted);
}

#[test]
fn test_compute_line_edits_different_line_count_emits_whole_document_edit() {
    let original = "line1\n";
    let formatted = "line1\nline2\nline3\n";
    let edits = DocumentFormattingProvider::compute_line_edits(original, formatted).unwrap();
    assert_eq!(
        edits.len(),
        1,
        "differing line counts must produce a single whole-document edit"
    );
    assert_eq!(edits[0].range.start, Position::new(0, 0));
    let applied = apply_text_edits(original, &edits);
    assert_eq!(applied, formatted);
}

#[test]
fn test_compute_line_edits_fewer_lines_in_formatted() {
    let original = "line1\nline2\nline3\n";
    let formatted = "line1\n";
    let edits = DocumentFormattingProvider::compute_line_edits(original, formatted).unwrap();
    assert_eq!(edits.len(), 1, "should collapse to one whole-document edit");
    let applied = apply_text_edits(original, &edits);
    assert_eq!(applied, formatted);
}

#[test]
fn test_compute_line_edits_trailing_newline_change_emits_whole_document_edit() {
    let original = "a\nb\n";
    let formatted = "a\nb";
    let edits = DocumentFormattingProvider::compute_line_edits(original, formatted).unwrap();
    // Trailing-newline state differs — treat as a whole-document change.
    assert_eq!(edits.len(), 1);
    let applied = apply_text_edits(original, &edits);
    assert_eq!(applied, formatted);
}

#[test]
fn test_compute_line_edits_descending_order_preserves_sequential_apply() {
    // Multiple per-line edits must be sorted bottom-to-top.
    let original = "a  \nb  \nc  \n";
    let formatted = "a\nb\nc\n";
    let edits = DocumentFormattingProvider::compute_line_edits(original, formatted).unwrap();
    assert!(!edits.is_empty());
    for window in edits.windows(2) {
        let curr = &window[0].range.start;
        let next = &window[1].range.start;
        assert!(
            curr.line > next.line || (curr.line == next.line && curr.character >= next.character),
            "edits must be sorted descending: {edits:#?}"
        );
    }
    let applied = apply_text_edits(original, &edits);
    assert_eq!(applied, formatted);
}

#[test]
fn test_compute_line_edits_no_overlapping_ranges() {
    let original = "line1\nline2  \nline3\n";
    let formatted = "line1\nline2\nline3\n";
    let edits = DocumentFormattingProvider::compute_line_edits(original, formatted).unwrap();

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

// =========================================================================
// format_on_key: fallback is whitespace-safe only
// =========================================================================

#[test]
fn test_format_on_semicolon_trims_trailing_whitespace_only() {
    let source = "let x = 1;   \n";
    let options = FormattingOptions::default();
    let edits = DocumentFormattingProvider::format_on_key(source, 0, 11, ";", &options).unwrap();
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].new_text, "");
    assert_eq!(edits[0].range.start.line, 0);
    // The trim range must start exactly after the last non-whitespace char.
    assert_eq!(edits[0].range.start.character, "let x = 1;".len() as u32);
}

#[test]
fn test_format_on_semicolon_does_not_remove_double_semicolons() {
    let source = "let x = 1;;\n";
    let options = FormattingOptions::default();
    let edits = DocumentFormattingProvider::format_on_key(source, 0, 11, ";", &options).unwrap();
    // No trailing whitespace to trim; fallback is not allowed to remove
    // a second semicolon by guess — that requires knowing the line is a
    // real statement.
    assert!(edits.is_empty(), "fallback must not rewrite `;;`");
}

#[test]
fn test_format_on_enter_trims_previous_line_trailing_whitespace() {
    let source = "let x = 1;   \nlet y = 2;\n";
    let options = FormattingOptions::default();
    let edits = DocumentFormattingProvider::format_on_key(source, 1, 0, "\n", &options).unwrap();
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].range.start.line, 0);
    assert_eq!(edits[0].new_text, "");
}

#[test]
fn test_format_on_enter_does_not_indent_new_line() {
    let source = "function foo() {\n\n";
    let options = FormattingOptions::default();
    let edits = DocumentFormattingProvider::format_on_key(source, 1, 0, "\n", &options).unwrap();
    // Fallback must not invent indentation for the blank new line.
    for edit in &edits {
        assert!(
            edit.new_text.chars().all(|c| c == ' ' || c == '\t') || edit.new_text.is_empty(),
            "fallback must not insert non-whitespace content, got: {:?}",
            edit.new_text
        );
    }
}

#[test]
fn test_format_on_closing_brace_is_noop_in_fallback() {
    let source = "function foo() {\n    let x = 1;\n}";
    let options = FormattingOptions::default();
    let edits = DocumentFormattingProvider::format_on_key(source, 2, 1, "}", &options).unwrap();
    assert!(
        edits.is_empty(),
        "close-brace formatting requires a parser; fallback returns no edits, got: {edits:?}"
    );
}

#[test]
fn test_format_on_key_unknown_key() {
    let source = "let x = 1;\n";
    let options = FormattingOptions::default();
    let edits = DocumentFormattingProvider::format_on_key(source, 0, 5, "a", &options).unwrap();
    assert!(edits.is_empty());
}

#[test]
fn test_format_on_key_respects_trim_trailing_whitespace_disabled() {
    let source = "let x = 1;   \n";
    let options = FormattingOptions {
        trim_trailing_whitespace: Some(false),
        ..Default::default()
    };
    let edits = DocumentFormattingProvider::format_on_key(source, 0, 11, ";", &options).unwrap();
    assert!(
        edits.is_empty(),
        "disabling trim_trailing_whitespace must silence format-on-key"
    );
}

// =========================================================================
// Range formatting
// =========================================================================

#[test]
fn test_format_range_limits_edits_to_selected_lines() {
    let source = "let a = 1;   \nlet b = 2;   \nlet c = 3;\n";
    let options = FormattingOptions::default();
    let range = Range::new(Position::new(0, 0), Position::new(0, 0));
    let edits = DocumentFormattingProvider::format_range(source, range, &options).unwrap();
    for edit in &edits {
        assert_eq!(
            edit.range.start.line, 0,
            "edits must be confined to the requested line 0: {edit:?}"
        );
    }
}

#[test]
fn test_format_range_empty_for_out_of_bounds() {
    let source = "let a = 1;\n";
    let options = FormattingOptions::default();
    let range = Range::new(Position::new(50, 0), Position::new(60, 0));
    let edits = DocumentFormattingProvider::format_range(source, range, &options).unwrap();
    assert!(edits.is_empty());
}

// =========================================================================
// External formatter flow (parse_eslint_fix_output)
// =========================================================================

#[test]
fn test_parse_eslint_fix_output_empty_stdout() {
    // ESLint can exit with no stdout at all (e.g. non-applicable file).
    let result = DocumentFormattingProvider::parse_eslint_fix_output("").unwrap();
    assert!(result.is_none());

    let result = DocumentFormattingProvider::parse_eslint_fix_output("   \n   ").unwrap();
    assert!(result.is_none());
}

#[test]
fn test_parse_eslint_fix_output_no_fixes() {
    let json = r#"[{
        "filePath": "/tmp/foo.ts",
        "messages": [],
        "errorCount": 0,
        "warningCount": 0,
        "fixableErrorCount": 0,
        "fixableWarningCount": 0,
        "source": "let x = 1;\n"
    }]"#;
    let result = DocumentFormattingProvider::parse_eslint_fix_output(json).unwrap();
    assert!(result.is_none());
}

#[test]
fn test_parse_eslint_fix_output_with_fixes() {
    let json = r#"[{
        "filePath": "/tmp/foo.ts",
        "messages": [],
        "errorCount": 0,
        "warningCount": 0,
        "fixableErrorCount": 0,
        "fixableWarningCount": 0,
        "output": "let x = 1;\n"
    }]"#;
    let result = DocumentFormattingProvider::parse_eslint_fix_output(json).unwrap();
    assert_eq!(result.as_deref(), Some("let x = 1;\n"));
}

#[test]
fn test_parse_eslint_fix_output_invalid_json() {
    let result = DocumentFormattingProvider::parse_eslint_fix_output("not json at all");
    assert!(result.is_err());
}

#[test]
fn test_parse_eslint_fix_output_non_array_root() {
    let result = DocumentFormattingProvider::parse_eslint_fix_output(r#"{"foo":"bar"}"#).unwrap();
    assert!(result.is_none());
}

#[test]
fn test_parse_eslint_fix_output_empty_array() {
    let result = DocumentFormattingProvider::parse_eslint_fix_output("[]").unwrap();
    assert!(result.is_none());
}

// =========================================================================
// Test helpers
// =========================================================================

/// Apply `compute_line_edits`-shaped edits (sorted bottom-to-top) to `source`.
fn apply_text_edits(source: &str, edits: &[TextEdit]) -> String {
    let mut text = source.to_string();
    for edit in edits {
        let start = position_to_offset(&text, edit.range.start);
        let end = position_to_offset(&text, edit.range.end);
        text.replace_range(start..end, &edit.new_text);
    }
    text
}

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
    panic!(
        "invalid position: {position:?} for text of length {}",
        text.len()
    );
}
