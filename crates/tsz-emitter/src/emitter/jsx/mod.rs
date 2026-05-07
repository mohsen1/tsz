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
    EmptyExpression,
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
/// Mirrors tsc behavior: only block comments before any code are scanned, and
/// the `@jsxImportSource` tag must be followed by a pragma boundary
/// (whitespace or end-of-comment) — this rejects fake tags like
/// `@jsxImportSourcex preact` that would otherwise be misparsed as a real
/// pragma with package `x`.
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
                let mut start = 0usize;
                let mut after_idx: Option<usize> = None;
                while let Some(rel) = comment_body[start..].find("@jsxImportSource") {
                    let abs = start + rel;
                    let after = abs + "@jsxImportSource".len();
                    let body_bytes = comment_body.as_bytes();
                    if after >= body_bytes.len()
                        || (body_bytes[after] as char).is_ascii_whitespace()
                    {
                        after_idx = Some(after);
                        break;
                    }
                    start = after;
                    if start >= comment_body.len() {
                        break;
                    }
                }
                if let Some(after) = after_idx {
                    let pkg: String = comment_body[after..]
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
/// - If the text has no line breaks, return it as-is (preserving whitespace).
/// - If multi-line, trim each line's leading/trailing whitespace, skip empty
///   lines, and join with a single space.
///
/// All three JS line terminator forms (`\r\n`, `\n`, and bare `\r`) act as
/// line breaks. tsc treats CR-only line breaks the same as LF — `isLineBreak`
/// in `compiler/scanner.ts` accepts CR, LF, LS, and PS.
pub(super) fn process_jsx_text(text: &str) -> String {
    // No line breaks at all -> return as-is (even if whitespace-only)
    if !text.contains('\n') && !text.contains('\r') {
        return text.to_string();
    }

    // Normalize CRLF and bare CR to LF, then split on LF. Without this, a
    // CR-only line break (`a\rb`) would not split into separate lines and the
    // whitespace coalescing would preserve the `\r` byte in the emitted string.
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");

    // Multi-line processing (matches tsc's algorithm)
    let lines: Vec<&str> = normalized.split('\n').collect();
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
    // Named entities. Table mirrors the HTML named entity set TypeScript ships
    // in `src/compiler/transformers/jsx.ts`, so JSX text and string attribute
    // values decode to the same runtime characters tsc produces.
    let cp = named_entity_codepoint(body)?;
    char::from_u32(cp).map(|c| c.to_string())
}

fn named_entity_codepoint(name: &str) -> Option<u32> {
    let cp: u32 = match name {
        "quot" => 0x0022,
        "amp" => 0x0026,
        "apos" => 0x0027,
        "lt" => 0x003C,
        "gt" => 0x003E,
        "nbsp" => 0x00A0,
        "iexcl" => 0x00A1,
        "cent" => 0x00A2,
        "pound" => 0x00A3,
        "curren" => 0x00A4,
        "yen" => 0x00A5,
        "brvbar" => 0x00A6,
        "sect" => 0x00A7,
        "uml" => 0x00A8,
        "copy" => 0x00A9,
        "ordf" => 0x00AA,
        "laquo" => 0x00AB,
        "not" => 0x00AC,
        "shy" => 0x00AD,
        "reg" => 0x00AE,
        "macr" => 0x00AF,
        "deg" => 0x00B0,
        "plusmn" => 0x00B1,
        "sup2" => 0x00B2,
        "sup3" => 0x00B3,
        "acute" => 0x00B4,
        "micro" => 0x00B5,
        "para" => 0x00B6,
        "middot" => 0x00B7,
        "cedil" => 0x00B8,
        "sup1" => 0x00B9,
        "ordm" => 0x00BA,
        "raquo" => 0x00BB,
        "frac14" => 0x00BC,
        "frac12" => 0x00BD,
        "frac34" => 0x00BE,
        "iquest" => 0x00BF,
        "Agrave" => 0x00C0,
        "Aacute" => 0x00C1,
        "Acirc" => 0x00C2,
        "Atilde" => 0x00C3,
        "Auml" => 0x00C4,
        "Aring" => 0x00C5,
        "AElig" => 0x00C6,
        "Ccedil" => 0x00C7,
        "Egrave" => 0x00C8,
        "Eacute" => 0x00C9,
        "Ecirc" => 0x00CA,
        "Euml" => 0x00CB,
        "Igrave" => 0x00CC,
        "Iacute" => 0x00CD,
        "Icirc" => 0x00CE,
        "Iuml" => 0x00CF,
        "ETH" => 0x00D0,
        "Ntilde" => 0x00D1,
        "Ograve" => 0x00D2,
        "Oacute" => 0x00D3,
        "Ocirc" => 0x00D4,
        "Otilde" => 0x00D5,
        "Ouml" => 0x00D6,
        "times" => 0x00D7,
        "Oslash" => 0x00D8,
        "Ugrave" => 0x00D9,
        "Uacute" => 0x00DA,
        "Ucirc" => 0x00DB,
        "Uuml" => 0x00DC,
        "Yacute" => 0x00DD,
        "THORN" => 0x00DE,
        "szlig" => 0x00DF,
        "agrave" => 0x00E0,
        "aacute" => 0x00E1,
        "acirc" => 0x00E2,
        "atilde" => 0x00E3,
        "auml" => 0x00E4,
        "aring" => 0x00E5,
        "aelig" => 0x00E6,
        "ccedil" => 0x00E7,
        "egrave" => 0x00E8,
        "eacute" => 0x00E9,
        "ecirc" => 0x00EA,
        "euml" => 0x00EB,
        "igrave" => 0x00EC,
        "iacute" => 0x00ED,
        "icirc" => 0x00EE,
        "iuml" => 0x00EF,
        "eth" => 0x00F0,
        "ntilde" => 0x00F1,
        "ograve" => 0x00F2,
        "oacute" => 0x00F3,
        "ocirc" => 0x00F4,
        "otilde" => 0x00F5,
        "ouml" => 0x00F6,
        "divide" => 0x00F7,
        "oslash" => 0x00F8,
        "ugrave" => 0x00F9,
        "uacute" => 0x00FA,
        "ucirc" => 0x00FB,
        "uuml" => 0x00FC,
        "yacute" => 0x00FD,
        "thorn" => 0x00FE,
        "yuml" => 0x00FF,
        "OElig" => 0x0152,
        "oelig" => 0x0153,
        "Scaron" => 0x0160,
        "scaron" => 0x0161,
        "Yuml" => 0x0178,
        "fnof" => 0x0192,
        "circ" => 0x02C6,
        "tilde" => 0x02DC,
        "Alpha" => 0x0391,
        "Beta" => 0x0392,
        "Gamma" => 0x0393,
        "Delta" => 0x0394,
        "Epsilon" => 0x0395,
        "Zeta" => 0x0396,
        "Eta" => 0x0397,
        "Theta" => 0x0398,
        "Iota" => 0x0399,
        "Kappa" => 0x039A,
        "Lambda" => 0x039B,
        "Mu" => 0x039C,
        "Nu" => 0x039D,
        "Xi" => 0x039E,
        "Omicron" => 0x039F,
        "Pi" => 0x03A0,
        "Rho" => 0x03A1,
        "Sigma" => 0x03A3,
        "Tau" => 0x03A4,
        "Upsilon" => 0x03A5,
        "Phi" => 0x03A6,
        "Chi" => 0x03A7,
        "Psi" => 0x03A8,
        "Omega" => 0x03A9,
        "alpha" => 0x03B1,
        "beta" => 0x03B2,
        "gamma" => 0x03B3,
        "delta" => 0x03B4,
        "epsilon" => 0x03B5,
        "zeta" => 0x03B6,
        "eta" => 0x03B7,
        "theta" => 0x03B8,
        "iota" => 0x03B9,
        "kappa" => 0x03BA,
        "lambda" => 0x03BB,
        "mu" => 0x03BC,
        "nu" => 0x03BD,
        "xi" => 0x03BE,
        "omicron" => 0x03BF,
        "pi" => 0x03C0,
        "rho" => 0x03C1,
        "sigmaf" => 0x03C2,
        "sigma" => 0x03C3,
        "tau" => 0x03C4,
        "upsilon" => 0x03C5,
        "phi" => 0x03C6,
        "chi" => 0x03C7,
        "psi" => 0x03C8,
        "omega" => 0x03C9,
        "thetasym" => 0x03D1,
        "upsih" => 0x03D2,
        "piv" => 0x03D6,
        "ensp" => 0x2002,
        "emsp" => 0x2003,
        "thinsp" => 0x2009,
        "zwnj" => 0x200C,
        "zwj" => 0x200D,
        "lrm" => 0x200E,
        "rlm" => 0x200F,
        "ndash" => 0x2013,
        "mdash" => 0x2014,
        "lsquo" => 0x2018,
        "rsquo" => 0x2019,
        "sbquo" => 0x201A,
        "ldquo" => 0x201C,
        "rdquo" => 0x201D,
        "bdquo" => 0x201E,
        "dagger" => 0x2020,
        "Dagger" => 0x2021,
        "bull" => 0x2022,
        "hellip" => 0x2026,
        "permil" => 0x2030,
        "prime" => 0x2032,
        "Prime" => 0x2033,
        "lsaquo" => 0x2039,
        "rsaquo" => 0x203A,
        "oline" => 0x203E,
        "frasl" => 0x2044,
        "euro" => 0x20AC,
        "image" => 0x2111,
        "weierp" => 0x2118,
        "real" => 0x211C,
        "trade" => 0x2122,
        "alefsym" => 0x2135,
        "larr" => 0x2190,
        "uarr" => 0x2191,
        "rarr" => 0x2192,
        "darr" => 0x2193,
        "harr" => 0x2194,
        "crarr" => 0x21B5,
        "lArr" => 0x21D0,
        "uArr" => 0x21D1,
        "rArr" => 0x21D2,
        "dArr" => 0x21D3,
        "hArr" => 0x21D4,
        "forall" => 0x2200,
        "part" => 0x2202,
        "exist" => 0x2203,
        "empty" => 0x2205,
        "nabla" => 0x2207,
        "isin" => 0x2208,
        "notin" => 0x2209,
        "ni" => 0x220B,
        "prod" => 0x220F,
        "sum" => 0x2211,
        "minus" => 0x2212,
        "lowast" => 0x2217,
        "radic" => 0x221A,
        "prop" => 0x221D,
        "infin" => 0x221E,
        "ang" => 0x2220,
        "and" => 0x2227,
        "or" => 0x2228,
        "cap" => 0x2229,
        "cup" => 0x222A,
        "int" => 0x222B,
        "there4" => 0x2234,
        "sim" => 0x223C,
        "cong" => 0x2245,
        "asymp" => 0x2248,
        "ne" => 0x2260,
        "equiv" => 0x2261,
        "le" => 0x2264,
        "ge" => 0x2265,
        "sub" => 0x2282,
        "sup" => 0x2283,
        "nsub" => 0x2284,
        "sube" => 0x2286,
        "supe" => 0x2287,
        "oplus" => 0x2295,
        "otimes" => 0x2297,
        "perp" => 0x22A5,
        "sdot" => 0x22C5,
        "lceil" => 0x2308,
        "rceil" => 0x2309,
        "lfloor" => 0x230A,
        "rfloor" => 0x230B,
        "lang" => 0x2329,
        "rang" => 0x232A,
        "loz" => 0x25CA,
        "spades" => 0x2660,
        "clubs" => 0x2663,
        "hearts" => 0x2665,
        "diams" => 0x2666,
        _ => return None,
    };
    Some(cp)
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
    use super::process_jsx_text;
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
    fn process_jsx_text_normalizes_cr_only_line_break() {
        // Issue #3903: JSX text with a CR-only line break (e.g. "a\rb") was
        // emitted unchanged because the multiline path only fired on '\n'.
        // tsc collapses CR-only breaks the same as LF, joining trimmed lines
        // with a single space.
        assert_eq!(process_jsx_text("a\rb"), "a b");
    }

    #[test]
    fn process_jsx_text_normalizes_crlf_line_break() {
        assert_eq!(process_jsx_text("a\r\nb"), "a b");
    }

    #[test]
    fn process_jsx_text_normalizes_mixed_cr_and_lf() {
        assert_eq!(process_jsx_text("a\rb\nc"), "a b c");
    }

    #[test]
    fn process_jsx_text_preserves_text_without_line_breaks() {
        // Non-multiline text must round-trip verbatim, including significant
        // whitespace, so the rest of the emitter can decide how to escape it.
        assert_eq!(process_jsx_text("hello world"), "hello world");
        assert_eq!(process_jsx_text("  spaced  "), "  spaced  ");
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

    #[test]
    fn decode_jsx_entities_decodes_extended_named_entities() {
        // Latin-1 supplement and other letters tsc decodes but the previous
        // hardcoded list omitted.
        assert_eq!(super::decode_jsx_entities("&eacute;"), "\u{00E9}");
        assert_eq!(super::decode_jsx_entities("&Aacute;"), "\u{00C1}");
        assert_eq!(super::decode_jsx_entities("&iquest;"), "\u{00BF}");
        assert_eq!(super::decode_jsx_entities("&Eacute;"), "\u{00C9}");
        assert_eq!(super::decode_jsx_entities("&szlig;"), "\u{00DF}");
        // Greek letters
        assert_eq!(super::decode_jsx_entities("&alpha;"), "\u{03B1}");
        assert_eq!(super::decode_jsx_entities("&Omega;"), "\u{03A9}");
        assert_eq!(super::decode_jsx_entities("&Delta;"), "\u{0394}");
        // Math symbols
        assert_eq!(super::decode_jsx_entities("&sum;"), "\u{2211}");
        assert_eq!(super::decode_jsx_entities("&infin;"), "\u{221E}");
        // Currency
        assert_eq!(super::decode_jsx_entities("&euro;"), "\u{20AC}");
    }

    #[test]
    fn decode_jsx_entities_preserves_previously_supported_entities() {
        // Regression net for entities the old short table covered.
        assert_eq!(super::decode_jsx_entities("&amp;"), "&");
        assert_eq!(super::decode_jsx_entities("&lt;"), "<");
        assert_eq!(super::decode_jsx_entities("&gt;"), ">");
        assert_eq!(super::decode_jsx_entities("&quot;"), "\"");
        assert_eq!(super::decode_jsx_entities("&apos;"), "'");
        assert_eq!(super::decode_jsx_entities("&nbsp;"), "\u{00A0}");
        assert_eq!(super::decode_jsx_entities("&middot;"), "\u{00B7}");
        assert_eq!(super::decode_jsx_entities("&hellip;"), "\u{2026}");
        assert_eq!(super::decode_jsx_entities("&copy;"), "\u{00A9}");
        assert_eq!(super::decode_jsx_entities("&trade;"), "\u{2122}");
        assert_eq!(super::decode_jsx_entities("&hearts;"), "\u{2665}");
        assert_eq!(super::decode_jsx_entities("&rarr;"), "\u{2192}");
    }

    #[test]
    fn decode_jsx_entities_mixes_text_and_entities() {
        assert_eq!(
            super::decode_jsx_entities("caf&eacute; &amp; the&aacute;tre"),
            "caf\u{00E9} & the\u{00E1}tre",
        );
    }

    #[test]
    fn decode_jsx_entities_leaves_unknown_named_entity_alone() {
        // Truly unknown names round-trip verbatim, including the trailing semi.
        assert_eq!(
            super::decode_jsx_entities("&notARealEntity;"),
            "&notARealEntity;",
        );
    }

    #[test]
    fn decode_jsx_entities_decodes_numeric_entities() {
        // Existing behavior we must keep.
        assert_eq!(super::decode_jsx_entities("&#233;"), "\u{00E9}");
        assert_eq!(super::decode_jsx_entities("&#xE9;"), "\u{00E9}");
        assert_eq!(super::decode_jsx_entities("&#x2026;"), "\u{2026}");
    }
}
