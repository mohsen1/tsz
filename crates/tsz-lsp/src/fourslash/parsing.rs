use super::Marker;

/// Parse markers from source text and return (`cleaned_source`, markers).
///
/// Markers have the format `/*name*/` where `name` is the marker identifier.
/// The anonymous marker `/**/` gets the name `""`.
pub(super) fn parse_markers(file: &str, source: &str) -> (String, Vec<Marker>) {
    let mut cleaned = String::with_capacity(source.len());
    // (name, byte offset in cleaned text); line/character are resolved once
    // the cleaned text is complete.
    let mut pending: Vec<(String, u32)> = Vec::new();
    let mut i = 0;
    let bytes = source.as_bytes();

    while i < bytes.len() {
        if i + 3 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            // Check for marker pattern: /*name*/
            if let Some(end) = find_marker_end(&bytes[i + 2..]) {
                let name = String::from_utf8_lossy(&bytes[i + 2..i + 2 + end]).to_string();
                pending.push((name, cleaned.len() as u32));
                i += 2 + end + 2; // skip /*name*/
                continue;
            }
        }
        // Copy the full UTF-8 character so multi-byte content survives intact.
        let ch_len = source[i..].chars().next().map_or(1, char::len_utf8);
        cleaned.push_str(&source[i..i + ch_len]);
        i += ch_len;
    }

    let line_map = tsz_common::position::LineMap::build(&cleaned);
    let markers = pending
        .into_iter()
        .map(|(name, offset)| {
            let position = line_map.offset_to_position(offset, &cleaned);
            Marker {
                name,
                file: file.to_string(),
                line: position.line,
                character: position.character,
                offset,
            }
        })
        .collect();

    (cleaned, markers)
}

/// Find the end of a marker name (position of `*/` relative to start).
pub(crate) fn find_marker_end(bytes: &[u8]) -> Option<usize> {
    for i in 0..bytes.len().saturating_sub(1) {
        if bytes[i] == b'*' && bytes[i + 1] == b'/' {
            // Only match if marker name is "valid" (no spaces, not a multi-line comment)
            let name = &bytes[..i];
            if name.iter().any(|&b| b == b'\n' || b == b'\r') {
                return None;
            }
            return Some(i);
        }
    }
    None
}

/// Parse multi-file test content.
///
/// Multi-file tests use `// @filename: path.ts` directives to separate files:
/// ```text
/// // @filename: a.ts
/// export const x = 1;
/// // @filename: b.ts
/// import { x } from "./a";
/// /*ref*/x;
/// ```
/// If `trimmed_line` begins with a `// @filename:` directive (in either
/// space-after-slashes spelling), return `(prefix, suffix)` where `prefix`
/// is the literal directive token and `suffix` is everything after it.
/// Otherwise return `None`. Shared by `parse_multi_file` and the variant
/// generator so the two recognizers always agree.
pub(crate) fn strip_filename_directive(trimmed_line: &str) -> Option<(&'static str, &str)> {
    if let Some(rest) = trimmed_line.strip_prefix("// @filename:") {
        Some(("// @filename:", rest))
    } else {
        trimmed_line
            .strip_prefix("//@filename:")
            .map(|rest| ("//@filename:", rest))
    }
}

pub(super) fn parse_multi_file(content: &str) -> Vec<(String, String)> {
    let mut files: Vec<(String, String)> = Vec::new();
    let mut current_file = String::new();
    let mut current_content = String::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if let Some((_, filename)) = strip_filename_directive(trimmed) {
            if !current_file.is_empty() {
                files.push((current_file, current_content));
                current_content = String::new();
            }
            current_file = filename.trim().to_string();
        } else {
            if !current_content.is_empty() {
                current_content.push('\n');
            }
            current_content.push_str(line);
        }
    }

    if !current_file.is_empty() {
        files.push((current_file, current_content));
    }

    // If no @filename directives were found, treat the whole thing as a single file
    if files.is_empty() {
        files.push(("test.ts".to_string(), content.to_string()));
    }

    files
}

/// Remove common leading whitespace from a multi-line string.
///
/// This allows tests to be written with natural indentation:
/// ```ignore
/// let t = FourslashTest::new("
///     const x = 1;
///     x + 1;
/// ");
/// ```
pub(super) fn dedent(s: &str) -> String {
    let lines: Vec<&str> = s.lines().collect();

    // Find minimum indentation (ignoring empty lines and the first/last if empty)
    let min_indent = lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start().len())
        .min()
        .unwrap_or(0);

    let result: Vec<&str> = lines
        .iter()
        .map(|line| {
            if line.len() >= min_indent {
                &line[min_indent..]
            } else {
                line.trim()
            }
        })
        .collect();

    // Trim leading and trailing empty lines
    let start = result.iter().position(|l| !l.is_empty()).unwrap_or(0);
    let end = result
        .iter()
        .rposition(|l| !l.is_empty())
        .map(|i| i + 1)
        .unwrap_or(0);

    if start >= end {
        return String::new();
    }

    result[start..end].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fourslash::FourslashTest;

    #[test]
    fn test_parse_markers_simple() {
        let (cleaned, markers) = parse_markers("test.ts", "const /*def*/x = 42;");
        assert_eq!(cleaned, "const x = 42;");
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].name, "def");
        assert_eq!(markers[0].character, 6); // position of 'x'
    }

    #[test]
    fn test_parse_markers_anonymous() {
        let (cleaned, markers) = parse_markers("test.ts", "foo(/**/);");
        assert_eq!(cleaned, "foo();");
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].name, "");
    }

    #[test]
    fn test_parse_markers_multiple() {
        let (cleaned, markers) = parse_markers("test.ts", "/*a*/x + /*b*/y");
        assert_eq!(cleaned, "x + y");
        assert_eq!(markers.len(), 2);
        assert_eq!(markers[0].name, "a");
        assert_eq!(markers[1].name, "b");
    }

    #[test]
    fn test_parse_markers_multiline() {
        let (cleaned, markers) = parse_markers("test.ts", "const /*def*/x = 1;\n/*ref*/x;");
        assert_eq!(cleaned, "const x = 1;\nx;");
        assert_eq!(markers.len(), 2);
        assert_eq!(markers[0].name, "def");
        assert_eq!(markers[0].line, 0);
        assert_eq!(markers[0].character, 6);
        assert_eq!(markers[1].name, "ref");
        assert_eq!(markers[1].line, 1);
        assert_eq!(markers[1].character, 0);
    }

    #[test]
    fn test_parse_markers_preserves_non_ascii_and_reports_utf16_columns() {
        // "héllo" contains a 2-byte é; 😀 is 4 bytes / 2 UTF-16 units.
        let (cleaned, markers) = parse_markers(
            "test.ts",
            "const h\u{00E9}llo = \"\u{1F600}\";\n/*m*/h\u{00E9}llo;",
        );
        assert_eq!(
            cleaned,
            "const h\u{00E9}llo = \"\u{1F600}\";\nh\u{00E9}llo;"
        );
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].line, 1);
        assert_eq!(markers[0].character, 0);
        // Byte offset into the cleaned text (line 1 starts after the 22-byte
        // first line + newline).
        assert_eq!(markers[0].offset, 23);
        assert_eq!(&cleaned[markers[0].offset as usize..], "h\u{00E9}llo;");
    }

    #[test]
    fn test_parse_markers_after_non_ascii_on_same_line_counts_utf16() {
        let (cleaned, markers) = parse_markers("test.ts", "\u{1F600} + /*m*/x");
        assert_eq!(cleaned, "\u{1F600} + x");
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].line, 0);
        // Emoji = 2 UTF-16 units, then " + " = 3.
        assert_eq!(markers[0].character, 5);
        // Byte offset: emoji = 4 bytes, " + " = 3.
        assert_eq!(markers[0].offset, 7);
    }

    #[test]
    fn test_parse_multi_file() {
        let content =
            "// @filename: a.ts\nexport const x = 1;\n// @filename: b.ts\nimport { x } from './a';";
        let files = parse_multi_file(content);
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].0, "a.ts");
        assert_eq!(files[0].1, "export const x = 1;");
        assert_eq!(files[1].0, "b.ts");
        assert_eq!(files[1].1, "import { x } from './a';");
    }

    #[test]
    fn test_dedent() {
        let input = "
            const x = 1;
            x + 1;
        ";
        let result = dedent(input);
        assert_eq!(result, "const x = 1;\nx + 1;");
    }

    #[test]
    fn test_fourslash_go_to_definition() {
        let mut t = FourslashTest::new(
            "
            const /*def*/x = 1;
            /*ref*/x + 1;
        ",
        );
        t.go_to_definition("ref").expect_at_marker("def");
    }

    #[test]
    fn test_fourslash_hover() {
        let mut t = FourslashTest::new(
            "
            const /*x*/x = 42;
        ",
        );
        t.hover("x").expect_found();
    }

    #[test]
    fn test_fourslash_references() {
        let mut t = FourslashTest::new(
            "
            const /*def*/x = 1;
            /*ref1*/x + /*ref2*/x;
        ",
        );
        // Should find references (the definition + usages)
        t.references("def").expect_found();
    }

    #[test]
    fn test_fourslash_multi_file() {
        let mut t = FourslashTest::multi_file(&[
            ("a.ts", "export const x = 1;"),
            ("b.ts", "const /*def*/y = 2;\n/*ref*/y;"),
        ]);
        // Definition within same file should work
        t.go_to_definition("ref").expect_at_marker("def");
    }

    #[test]
    fn test_fourslash_document_symbols() {
        let mut t = FourslashTest::new(
            "
            function foo() {}
            class Bar {}
            const baz = 1;
        ",
        );
        t.document_symbols("test.ts")
            .expect_found()
            .expect_symbol("foo")
            .expect_symbol("Bar")
            .expect_symbol("baz");
    }

    #[test]
    fn test_fourslash_completions() {
        let mut t = FourslashTest::new(
            "
            const myVariable = 42;
            /**/my
        ",
        );
        // At the marker position, we should get completions including our variable
        let result = t.completions("");
        // Completions may or may not include myVariable depending on implementation
        // This just verifies the framework works
        // Framework test - completions query should work without panic
        let _ = result.items.len();
    }

    #[test]
    fn test_fourslash_rename() {
        let mut t = FourslashTest::new(
            "
            const /*x*/x = 1;
            x + x;
        ",
        );
        t.rename("x", "y")
            .expect_success()
            .expect_edits_in_file("test.ts");
    }

    #[test]
    fn test_fourslash_at_filename_parsing() {
        let t = FourslashTest::from_content(
            "// @filename: utils.ts\nexport function /*def*/helper() {}\n// @filename: main.ts\nimport { /*ref*/helper } from './utils';\nhelper();",
        );
        // Verify markers were parsed in correct files
        assert_eq!(t.marker_file("def"), "utils.ts");
        assert_eq!(t.marker_file("ref"), "main.ts");
    }
}
