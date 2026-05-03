mod emit;
mod transform;

use tsz_parser::parser::NodeIndex;

// =============================================================================
// Internal Data Types
// =============================================================================

pub(super) struct JsxUsage {
    pub(super) needs_jsx: bool,
    pub(super) needs_jsxs: bool,
    pub(super) needs_fragment: bool,
    pub(super) needs_create_element: bool,
}

#[derive(Clone)]
/// Separator style for JSX child emission.
pub(super) enum JsxChildSep {
    /// `, ` before every child (classic createElement separate args)
    CommaSpace,
    /// newline before every child, `,` after all but last (multiline classic)
    CommaNewline,
    /// `, ` only between children (automatic `children: [a, b]`)
    CommaBetween,
    /// No separator (single-child automatic)
    None,
}

#[derive(Clone)]
pub(super) enum JsxAttrInfo {
    Named { name: String, value: JsxAttrValue },
    Spread { expr: NodeIndex },
}

#[derive(Clone)]
pub(super) enum JsxAttrValue {
    /// String literal attribute -- carries the node index for quote-preserving emission
    StringNode(NodeIndex),
    Bool(bool),
    Expr(NodeIndex),
}

pub(super) struct JsxAttrsInfo {
    pub(super) attrs: Vec<JsxAttrInfo>,
    pub(super) has_spread: bool,
}

pub(super) enum AttrGroup {
    Named(Vec<JsxAttrInfo>),
    Spread(NodeIndex),
    /// An object literal from a spread that can be safely inlined.
    InlinedObjectLiteral(NodeIndex),
}

/// Group consecutive named attributes together, with spreads as separators.
pub(super) fn group_jsx_attrs(attrs: &[JsxAttrInfo]) -> Vec<AttrGroup> {
    let mut groups: Vec<AttrGroup> = Vec::new();
    let mut current_named: Vec<JsxAttrInfo> = Vec::new();

    for attr in attrs {
        match attr {
            JsxAttrInfo::Spread { expr } => {
                if !current_named.is_empty() {
                    groups.push(AttrGroup::Named(std::mem::take(&mut current_named)));
                }
                groups.push(AttrGroup::Spread(*expr));
            }
            named => {
                current_named.push(named.clone());
            }
        }
    }

    if !current_named.is_empty() {
        groups.push(AttrGroup::Named(current_named));
    }

    // If the first element is a spread, prepend an empty object for Object.assign
    if !groups.is_empty() && matches!(groups[0], AttrGroup::Spread(_)) {
        groups.insert(0, AttrGroup::Named(Vec::new()));
    }

    groups
}

// =============================================================================
// Pragma extraction
// =============================================================================

/// Extract `@jsxImportSource <package>` from leading block comments.
/// Mirrors tsc behavior: only block comments before any code are scanned.
pub(super) fn extract_jsx_import_source(source: &str) -> Option<String> {
    let scan_limit = source.len().min(4096);
    let text = &source[..scan_limit];
    let bytes = text.as_bytes();
    let mut pos = 0;
    while pos < bytes.len() {
        if bytes[pos].is_ascii_whitespace() {
            pos += 1;
            continue;
        }
        if pos + 1 < bytes.len() && bytes[pos] == b'/' && bytes[pos + 1] == b'*' {
            let comment_start = pos + 2;
            if let Some(end_offset) = text[comment_start..].find("*/") {
                let comment_body = &text[comment_start..comment_start + end_offset];
                if let Some(idx) = comment_body.find("@jsxImportSource") {
                    let after = &comment_body[idx + "@jsxImportSource".len()..];
                    let pkg: String = after
                        .trim_start()
                        .chars()
                        .take_while(|c| {
                            c.is_alphanumeric()
                                || *c == '_'
                                || *c == '-'
                                || *c == '/'
                                || *c == '@'
                                || *c == '.'
                        })
                        .collect();
                    if !pkg.is_empty() {
                        return Some(pkg);
                    }
                }
                pos = comment_start + end_offset + 2;
            } else {
                break;
            }
            continue;
        }
        if pos + 1 < bytes.len() && bytes[pos] == b'/' && bytes[pos + 1] == b'/' {
            if let Some(nl) = text[pos..].find('\n') {
                pos += nl + 1;
            } else {
                break;
            }
            continue;
        }
        break;
    }
    None
}

// =============================================================================
// JSX Text Processing (matches tsc behavior)
// =============================================================================

/// Process JSX text content matching tsc's `getTransformedJsxText` algorithm:
///
/// - If the text has no newlines, return it as-is (preserving whitespace).
/// - If multi-line, trim each line's leading/trailing whitespace, skip empty
///   lines, and join with a single space.
pub(super) fn process_jsx_text(text: &str) -> String {
    // No newlines at all -> return as-is (even if whitespace-only)
    if !text.contains('\n') {
        return text.to_string();
    }

    // Multi-line processing (matches tsc's algorithm)
    let lines: Vec<&str> = text.split('\n').collect();
    let mut parts: Vec<String> = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        let trimmed = if i == 0 {
            // First line: trim end only
            line.trim_end()
        } else if i == lines.len() - 1 {
            // Last line: trim start only
            line.trim_start()
        } else {
            // Middle lines: trim both
            line.trim()
        };

        if trimmed.is_empty() {
            continue;
        }
        parts.push(trimmed.to_string());
    }

    parts.join(" ")
}

/// Escape a string for JS string literal context with entity decoding and Unicode escaping.
/// `quote` is the surrounding quote char (' or ") so we know which to escape.
pub(super) fn escape_jsx_text_for_js_with_quote(s: &str, quote: char) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            c if c == quote => {
                result.push('\\');
                result.push(c);
            }
            // Non-ASCII chars get \uXXXX escaping (or surrogate pairs for > U+FFFF)
            c if c as u32 > 0x7E => {
                let cp = c as u32;
                if cp > 0xFFFF {
                    // UTF-16 surrogate pair
                    let hi = 0xD800 + ((cp - 0x10000) >> 10);
                    let lo = 0xDC00 + ((cp - 0x10000) & 0x3FF);
                    result.push_str(&format!("\\u{hi:04X}\\u{lo:04X}"));
                } else {
                    result.push_str(&format!("\\u{cp:04X}"));
                }
            }
            _ => result.push(c),
        }
    }
    result
}

/// Decode HTML/XML entities in JSX text.
/// Handles named entities (&amp; &lt; &gt; &quot; &middot; &hellip; etc.),
/// numeric decimal (&#123;), and hex (&#x7D;) references.
/// Unknown named entities are left as-is (e.g. &notAnEntity;).
pub(super) fn decode_jsx_entities(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '&' {
            // Collect entity body until ';' or non-entity char
            let mut body = String::new();
            let mut found_semi = false;
            while let Some(&next) = chars.peek() {
                if next == ';' {
                    chars.next();
                    found_semi = true;
                    break;
                }
                if next.is_alphanumeric() || next == '#' {
                    body.push(next);
                    chars.next();
                } else {
                    break;
                }
            }
            if found_semi {
                if let Some(decoded) = resolve_entity(&body) {
                    result.push_str(&decoded);
                } else {
                    // Unknown entity -- leave as-is
                    result.push('&');
                    result.push_str(&body);
                    result.push(';');
                }
            } else {
                result.push('&');
                result.push_str(&body);
            }
        } else {
            result.push(c);
        }
    }
    result
}

/// Resolve a single HTML entity body (without & and ;) to its character(s).
fn resolve_entity(body: &str) -> Option<String> {
    // Numeric: &#123; or &#x7D;
    if let Some(num_part) = body.strip_prefix('#') {
        let cp = if num_part.starts_with('x') || num_part.starts_with('X') {
            u32::from_str_radix(&num_part[1..], 16).ok()?
        } else {
            num_part.parse::<u32>().ok()?
        };
        return char::from_u32(cp).map(|c| c.to_string());
    }
    // Named entities
    let c = match body {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => '\u{00A0}',
        "middot" => '\u{00B7}',
        "mdash" => '\u{2014}',
        "ndash" => '\u{2013}',
        "hellip" => '\u{2026}',
        "laquo" => '\u{00AB}',
        "raquo" => '\u{00BB}',
        "bull" => '\u{2022}',
        "copy" => '\u{00A9}',
        "reg" => '\u{00AE}',
        "trade" => '\u{2122}',
        "hearts" => '\u{2665}',
        "larr" => '\u{2190}',
        "rarr" => '\u{2192}',
        "uarr" => '\u{2191}',
        "darr" => '\u{2193}',
        _ => return None,
    };
    Some(c.to_string())
}

/// Check if a property name needs quoting in an object literal.
pub(super) fn needs_quoting(name: &str) -> bool {
    if name.is_empty() {
        return true;
    }
    // Names with colons (namespaced), hyphens, or starting with digits need quoting
    name.contains(':') || name.contains('-') || name.starts_with(|c: char| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use crate::output::printer::{PrintOptions, Printer};
    use tsz_parser::ParserState;

    fn emit_jsx(source: &str) -> String {
        let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        printer.finish().code
    }

    #[test]
    fn self_closing_no_attributes_has_space_before_slash() {
        let output = emit_jsx("const x = <Tag />;");
        assert!(
            output.contains("<Tag />"),
            "Self-closing element without attributes should have space before />.\nOutput: {output}"
        );
    }

    #[test]
    fn self_closing_with_attributes_has_no_space_before_slash() {
        let output = emit_jsx("const x = <Tag foo=\"bar\"/>;");
        assert!(
            output.contains("<Tag foo=\"bar\"/>"),
            "Self-closing element with attributes should NOT have extra space before />.\nOutput: {output}"
        );
    }

    #[test]
    fn self_closing_with_expression_attribute_no_extra_space() {
        let output = emit_jsx("const x = <Tag value={42}/>;");
        assert!(
            output.contains("<Tag value={42}/>"),
            "Self-closing element with expression attribute should NOT have extra space before />.\nOutput: {output}"
        );
    }

    #[test]
    fn conflict_marker_unclosed_jsx_emits_empty_synthesized_close() {
        let output = emit_jsx("const x = <div>\n<<<<<<< HEAD");
        assert!(
            output.contains("const x = <div></>;"),
            "Conflict-marker JSX recovery should emit an empty synthesized close.\nOutput: {output}"
        );
        assert!(
            !output.contains("</div>"),
            "Conflict-marker JSX recovery should not mirror the opener tag.\nOutput: {output}"
        );
    }

    #[test]
    fn jsx_text_multiline_content_preserves_whitespace() {
        // tsc preserves JSX text content including leading/trailing whitespace and newlines.
        // The scanner's re_scan_jsx_token must reset to full_start_pos (before trivia)
        // so the text node captures the complete whitespace content.
        let source = "let k1 = <Comp a={10} b=\"hi\">\n        hi hi hi!\n    </Comp>;";
        let output = emit_jsx(source);
        assert!(
            output.contains("\n        hi hi hi!\n    "),
            "JSX text should preserve leading/trailing whitespace and newlines.\nOutput: {output}"
        );
    }

    #[test]
    fn jsx_text_single_line_content() {
        let output = emit_jsx("let x = <div>hello world</div>;");
        assert!(
            output.contains(">hello world</"),
            "JSX text on single line should be preserved.\nOutput: {output}"
        );
    }

    #[test]
    fn jsx_text_with_nested_elements() {
        let source = "let x = <Comp>\n        <div>inner</div>\n    </Comp>;";
        let output = emit_jsx(source);
        assert!(
            output.contains("\n        <div>inner</div>\n    "),
            "JSX text whitespace around nested elements should be preserved.\nOutput: {output}"
        );
    }

    #[test]
    fn jsx_text_whitespace_only_between_elements() {
        // Whitespace-only text nodes between JSX elements should be preserved
        let source = "let x = <div>\n    <span>a</span>\n    <span>b</span>\n</div>;";
        let output = emit_jsx(source);
        assert!(
            output.contains("<span>a</span>\n    <span>b</span>"),
            "Whitespace between JSX children should be preserved.\nOutput: {output}"
        );
    }

    #[test]
    fn jsx_expression_with_trailing_comment_in_expression_is_preserved() {
        let source = "let x = <div>{null/* preserved */}</div>;";
        let output = emit_jsx(source);
        assert!(
            output.contains("/* preserved */"),
            "Trailing comment inside JSX expression should be preserved.\nOutput: {output}"
        );
        assert!(
            !output.contains("{null}"),
            "Trailing comment should not be dropped from JSX expression.\nOutput: {output}"
        );
    }

    #[test]
    fn jsx_expression_without_expression_preserves_inner_comments() {
        let source = "let x = <div>{\n    // ???\n}</div>;";
        let output = emit_jsx(source);
        assert!(
            output.contains("// ???"),
            "Line comment inside a comment-only JSX expression should be preserved.\nOutput: {output}"
        );
        // The comment should appear after `{` on a new line, and the closing `}`
        // should align with the comment (both at the increased indent level).
        assert!(
            output.contains("{") && output.contains("// ???") && output.contains("}"),
            "Comment should remain inside JSX expression braces.\nOutput: {output}"
        );
        // Closing `}` should be on its own line after the comment (not on the
        // same line), matching tsc's output for JSX expression comments.
        let comment_idx = output.find("// ???").unwrap();
        let after_comment = &output[comment_idx..];
        assert!(
            after_comment.contains('\n'),
            "There should be a newline after the comment before the closing brace.\nOutput: {output}"
        );
        let closing_brace = after_comment.find('}').unwrap();
        let between = &after_comment[..closing_brace];
        assert!(
            between.contains('\n'),
            "Closing brace should be on a separate line from the comment.\nOutput: {output}"
        );
    }

    #[test]
    fn jsx_expression_without_expression_normalizes_multiline_leading_comment_indentation() {
        let source = "let x = <div>{\n    // ??? 1\n            // ??? 2\n}</div>;";
        let output = emit_jsx(source);
        // Both comments should appear in the output and the closing `}` should
        // follow on its own line.
        assert!(
            output.contains("// ??? 1") && output.contains("// ??? 2"),
            "Both comment lines should be preserved.\nOutput: {output}"
        );
        // The two comments should be on separate lines with uniform indentation
        let idx1 = output.find("// ??? 1").unwrap();
        let idx2 = output.find("// ??? 2").unwrap();
        assert!(
            output[idx1..idx2].contains('\n'),
            "Comment-only JSX expression lines should be on separate lines.\nOutput: {output}"
        );
    }

    #[test]
    fn jsx_expression_inline_block_comment_keeps_spacing() {
        let source = "let x = <div>{\n    // ???\n/* ??? */}</div>;";
        let output = emit_jsx(source);
        assert!(
            output.contains("/* ??? */ }"),
            "Trailing inline block comment inside JSX expression should keep leading space before closing brace.\nOutput: {output}"
        );
    }
}
