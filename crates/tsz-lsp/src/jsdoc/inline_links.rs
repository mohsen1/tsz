//! Parsing and expansion of inline JSDoc link constructs (`{@link}`,
//! `{@linkcode}`, `{@linkplain}`) for hover Markdown, plain-text documentation
//! strings, and tsserver display-part arrays.

// tsserver protocol display-part kind strings for inline JSDoc links.
const KIND_TEXT: &str = "text";
const KIND_LINK: &str = "link";
const KIND_LINK_NAME: &str = "linkName";
const KIND_LINK_TEXT: &str = "linkText";

/// Which variant of the inline link tag was used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkVariant {
    /// `{@link ...}` – standard link
    Link,
    /// `{@linkcode ...}` – link rendered in monospace/code font
    Linkcode,
    /// `{@linkplain ...}` – link rendered as plain text (no code font)
    Linkplain,
}

impl LinkVariant {
    const fn tag_text(&self) -> &'static str {
        match self {
            LinkVariant::Link => "@link",
            LinkVariant::Linkcode => "@linkcode",
            LinkVariant::Linkplain => "@linkplain",
        }
    }
}

/// A single `{@link …}` / `{@linkcode …}` / `{@linkplain …}` span inside
/// a larger documentation string.
#[derive(Debug, Clone)]
pub struct InlineLinkSpan {
    /// Byte offset of the opening `{` in the source string.
    pub start: usize,
    /// Byte offset one past the closing `}`.
    pub end: usize,
    /// Which link variant was written.
    pub variant: LinkVariant,
    /// The URL or qualified symbol name (first whitespace-delimited token
    /// after the tag keyword).
    pub target: String,
    pub display: Option<String>,
}

impl InlineLinkSpan {
    pub fn display_text(&self) -> &str {
        self.display.as_deref().unwrap_or(&self.target)
    }

    pub fn is_url(&self) -> bool {
        self.target.starts_with("http://") || self.target.starts_with("https://")
    }
}

/// Parse all inline link spans from `text`.
///
/// Handles `{@link}`, `{@linkcode}`, and `{@linkplain}` constructs. Unknown
/// or malformed constructs (missing `}`, empty target) are silently skipped,
/// so callers receive only well-formed spans and the surrounding text is left
/// unchanged for those positions.
pub fn parse_link_spans(text: &str) -> Vec<InlineLinkSpan> {
    let mut spans = Vec::new();
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i + 1 < len {
        if bytes[i] != b'{' || bytes[i + 1] != b'@' {
            i += 1;
            continue;
        }

        let tag_start = i;
        let rest = &text[i + 1..];

        let (variant, tag_end) = if rest.starts_with("@linkcode") {
            let end = i + 1 + 9; // `{` + `@linkcode`
            // Ensure not followed by another identifier character (@linkcodex…)
            if end < len && is_ident_continue(bytes[end]) {
                i += 1;
                continue;
            }
            (LinkVariant::Linkcode, end)
        } else if rest.starts_with("@linkplain") {
            let end = i + 1 + 10;
            if end < len && is_ident_continue(bytes[end]) {
                i += 1;
                continue;
            }
            (LinkVariant::Linkplain, end)
        } else if rest.starts_with("@link") {
            let end = i + 1 + 5;
            if end < len && is_ident_continue(bytes[end]) {
                i += 1;
                continue;
            }
            (LinkVariant::Link, end)
        } else {
            i += 1;
            continue;
        };

        let mut content_start = tag_end;
        while content_start < len && bytes[content_start].is_ascii_whitespace() {
            content_start += 1;
        }

        let Some(close_offset) = text[tag_end..].find('}') else {
            i += 1;
            continue;
        };
        let close_pos = tag_end + close_offset;

        let content = text[content_start..close_pos].trim();

        if content.is_empty() {
            i = close_pos + 1;
            continue;
        }

        let (target, display) = match content
            .as_bytes()
            .iter()
            .position(|b| b.is_ascii_whitespace())
        {
            Some(sp) => {
                let t = content[..sp].to_string();
                let d = content[sp..].trim().to_string();
                (t, if d.is_empty() { None } else { Some(d) })
            }
            None => (content.to_string(), None),
        };

        if target.is_empty() {
            i = close_pos + 1;
            continue;
        }

        spans.push(InlineLinkSpan {
            start: tag_start,
            end: close_pos + 1,
            variant,
            target,
            display,
        });

        i = close_pos + 1;
    }

    spans
}

/// Expand inline link constructs to plain text.
///
/// - `{@link X}` → `X`
/// - `{@link X Display Text}` → `Display Text`
/// - `{@linkcode X}` → `X`
/// - `{@linkplain X}` → `X`
///
/// Surrounding prose is reproduced verbatim without any escaping.
pub fn expand_to_plain_text(text: &str) -> String {
    let spans = parse_link_spans(text);
    if spans.is_empty() {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut last_end = 0;
    for span in &spans {
        out.push_str(&text[last_end..span.start]);
        out.push_str(span.display_text());
        last_end = span.end;
    }
    out.push_str(&text[last_end..]);
    out
}

/// Expand inline link constructs to Markdown, escaping surrounding prose.
///
/// This is the correct function to use when building Markdown hover content.
/// It applies `CommonMark` escaping to prose segments so that user-supplied
/// text containing `[`, `]`, `*`, etc. does not break the rendered output,
/// while inline link spans are turned into Markdown structures that do not
/// need further escaping:
///
/// - URL targets → `[display](url)` Markdown hyperlinks
/// - Symbol targets with `{@link …}` → `` `display` `` inline-code span
/// - Symbol targets with `{@linkcode …}` → `` `display` `` inline-code span
/// - Symbol targets with `{@linkplain …}` → prose (escaped)
pub fn expand_to_markdown_escaped(text: &str) -> String {
    expand_to_markdown_with_resolver(text, |_| None)
}

/// Like [`expand_to_markdown_escaped`] but accepts a resolver that maps a
/// symbol name to a Markdown URI string.  When the resolver returns `Some(uri)`,
/// the link becomes `[display](uri)` instead of an inline code span.
pub fn expand_to_markdown_with_resolver<F>(text: &str, resolve: F) -> String
where
    F: Fn(&str) -> Option<String>,
{
    let spans = parse_link_spans(text);
    if spans.is_empty() {
        return super::markdown_escape::escape_markdown_label(text);
    }

    let mut out = String::with_capacity(text.len() + text.len() / 4 + 32);
    let mut last_end = 0;

    for span in &spans {
        out.push_str(&super::markdown_escape::escape_markdown_label(
            &text[last_end..span.start],
        ));

        let display = span.display_text();

        match span.variant {
            LinkVariant::Linkplain => {
                out.push_str(&super::markdown_escape::escape_markdown_label(display));
            }
            LinkVariant::Linkcode => {
                out.push_str(&super::markdown_escape::format_inline_code(display));
            }
            LinkVariant::Link => {
                if span.is_url() {
                    push_markdown_hyperlink(&mut out, display, &span.target);
                } else if let Some(uri) = resolve(&span.target) {
                    push_markdown_hyperlink(&mut out, display, &uri);
                } else {
                    out.push_str(&super::markdown_escape::format_inline_code(display));
                }
            }
        }

        last_end = span.end;
    }

    out.push_str(&super::markdown_escape::escape_markdown_label(
        &text[last_end..],
    ));
    out
}

/// Build a JSON display-parts array from `text`, expanding inline link
/// constructs into structured parts that match the tsserver protocol.
///
/// For `{@link Foo}` (symbol, no display text):
/// ```json
/// [{"text":"{@link ","kind":"link"}, {"text":"Foo","kind":"linkName"}, {"text":"}","kind":"link"}]
/// ```
///
/// For `{@link Foo label}` (symbol with display text):
/// ```json
/// [{"text":"{@link ","kind":"link"}, {"text":"Foo","kind":"linkName","target":...},
///  {"text":"label","kind":"linkText"}, {"text":"}","kind":"link"}]
/// ```
///
/// For `{@link https://example.com Click here}` (URL):
/// ```json
/// [{"text":"{@link ","kind":"link"}, {"text":"https://example.com Click here","kind":"linkText"},
///  {"text":"}","kind":"link"}]
/// ```
pub fn build_doc_display_parts(text: &str) -> serde_json::Value {
    build_doc_display_parts_with_resolver(text, |_| None)
}

/// Like [`build_doc_display_parts`] but adds a `target` property on the
/// `linkName` part when the resolver returns a value for the link's target name.
pub fn build_doc_display_parts_with_resolver<F>(text: &str, resolve: F) -> serde_json::Value
where
    F: Fn(&str) -> Option<serde_json::Value>,
{
    let spans = parse_link_spans(text);
    if spans.is_empty() {
        return serde_json::json!([{"text": text, "kind": KIND_TEXT}]);
    }

    let mut parts: Vec<serde_json::Value> = Vec::new();
    let mut last_end = 0;

    for span in &spans {
        let prefix = &text[last_end..span.start];
        if !prefix.is_empty() {
            parts.push(serde_json::json!({"text": prefix, "kind": KIND_TEXT}));
        }

        // Opening "link" part contains the full tag prefix, e.g. "{@link ".
        let open_text = format!("{{{} ", span.variant.tag_text());
        parts.push(serde_json::json!({"text": open_text, "kind": KIND_LINK}));

        if span.is_url() {
            // URLs: full content (url + optional display text) as a single linkText part.
            let full_content = if let Some(ref d) = span.display {
                format!("{} {}", span.target, d)
            } else {
                span.target.clone()
            };
            parts.push(serde_json::json!({"text": full_content, "kind": KIND_LINK_TEXT}));
        } else {
            // Symbol refs: target as linkName (with optional target property from resolver),
            // then display text as linkText only if it differs from the target.
            let link_name_part = if let Some(target_val) = resolve(&span.target) {
                serde_json::json!({
                    "text": &span.target,
                    "kind": KIND_LINK_NAME,
                    "target": target_val
                })
            } else {
                serde_json::json!({"text": &span.target, "kind": KIND_LINK_NAME})
            };
            parts.push(link_name_part);

            if let Some(ref display) = span.display {
                parts.push(serde_json::json!({"text": display, "kind": KIND_LINK_TEXT}));
            }
        }

        parts.push(serde_json::json!({"text": "}", "kind": KIND_LINK}));

        last_end = span.end;
    }

    let suffix = &text[last_end..];
    if !suffix.is_empty() {
        parts.push(serde_json::json!({"text": suffix, "kind": KIND_TEXT}));
    }

    serde_json::Value::Array(parts)
}

const fn is_ident_continue(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn push_markdown_hyperlink(out: &mut String, display: &str, uri: &str) {
    out.push('[');
    out.push_str(&super::markdown_escape::escape_markdown_label(display));
    out.push_str("](");
    out.push_str(uri);
    out.push(')');
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_link_spans ───────────────────────────────────────────────────

    #[test]
    fn parse_simple_link() {
        let spans = parse_link_spans("{@link Foo}");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].variant, LinkVariant::Link);
        assert_eq!(spans[0].target, "Foo");
        assert!(spans[0].display.is_none());
        assert_eq!(spans[0].start, 0);
        assert_eq!(spans[0].end, 11);
    }

    #[test]
    fn parse_link_with_display_text() {
        let spans = parse_link_spans("{@link Foo the display}");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].target, "Foo");
        assert_eq!(spans[0].display.as_deref(), Some("the display"));
    }

    #[test]
    fn parse_linkcode_variant() {
        let spans = parse_link_spans("{@linkcode myFunc}");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].variant, LinkVariant::Linkcode);
        assert_eq!(spans[0].target, "myFunc");
    }

    #[test]
    fn parse_linkplain_variant() {
        let spans = parse_link_spans("{@linkplain SomeType}");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].variant, LinkVariant::Linkplain);
        assert_eq!(spans[0].target, "SomeType");
    }

    #[test]
    fn parse_url_target() {
        let spans = parse_link_spans("{@link https://example.com}");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].target, "https://example.com");
        assert!(spans[0].is_url());
    }

    #[test]
    fn parse_dotted_symbol_name() {
        let spans = parse_link_spans("{@link NS.R}");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].target, "NS.R");
    }

    #[test]
    fn parse_multiple_links() {
        let text = "See {@link A} and {@link B Display}.";
        let spans = parse_link_spans(text);
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].target, "A");
        assert_eq!(spans[1].target, "B");
        assert_eq!(spans[1].display.as_deref(), Some("Display"));
    }

    #[test]
    fn parse_empty_link_is_skipped() {
        let spans = parse_link_spans("{@link}");
        assert!(spans.is_empty(), "empty link should be skipped");
    }

    #[test]
    fn parse_unclosed_link_is_skipped() {
        let spans = parse_link_spans("{@link Foo");
        assert!(spans.is_empty(), "unclosed link should be skipped");
    }

    #[test]
    fn parse_no_links_in_plain_text() {
        let spans = parse_link_spans("nothing special here");
        assert!(spans.is_empty());
    }

    // Renamed variable: same structural position, different identifier name.
    #[test]
    fn parse_any_identifier_name_works() {
        for name in &["K", "MyClass", "X", "some_fn", "NS.Method"] {
            let text = format!("{{@link {name}}}");
            let spans = parse_link_spans(&text);
            assert_eq!(spans.len(), 1, "should parse link for identifier {name}");
            assert_eq!(spans[0].target, *name);
        }
    }

    // ── expand_to_plain_text ───────────────────────────────────────────────

    #[test]
    fn plain_text_simple_link() {
        assert_eq!(expand_to_plain_text("{@link Foo}"), "Foo");
    }

    #[test]
    fn plain_text_link_with_display() {
        assert_eq!(
            expand_to_plain_text("{@link Foo the display}"),
            "the display"
        );
    }

    #[test]
    fn plain_text_in_sentence() {
        assert_eq!(
            expand_to_plain_text("Use {@link SomeClass} for details."),
            "Use SomeClass for details."
        );
    }

    #[test]
    fn plain_text_linkcode() {
        assert_eq!(expand_to_plain_text("{@linkcode myFunc}"), "myFunc");
    }

    #[test]
    fn plain_text_linkplain() {
        assert_eq!(expand_to_plain_text("{@linkplain SomeType}"), "SomeType");
    }

    #[test]
    fn plain_text_multiple_links() {
        assert_eq!(
            expand_to_plain_text("See {@link A} and {@link B DisplayB}."),
            "See A and DisplayB."
        );
    }

    #[test]
    fn plain_text_no_links_unchanged() {
        let input = "No inline tags here.";
        assert_eq!(expand_to_plain_text(input), input);
    }

    #[test]
    fn plain_text_url_link() {
        assert_eq!(
            expand_to_plain_text("{@link https://example.com}"),
            "https://example.com"
        );
    }

    // ── expand_to_markdown_escaped ─────────────────────────────────────────

    #[test]
    fn markdown_simple_link_becomes_inline_code() {
        let out = expand_to_markdown_escaped("{@link Foo}");
        assert_eq!(out, "`Foo`");
    }

    #[test]
    fn markdown_link_in_sentence_escapes_prose() {
        let out = expand_to_markdown_escaped("Use {@link SomeClass} for details.");
        assert_eq!(out, "Use `SomeClass` for details.");
    }

    #[test]
    fn markdown_link_display_text_used() {
        let out = expand_to_markdown_escaped("{@link Foo the label}");
        assert_eq!(out, "`the label`");
    }

    #[test]
    fn markdown_linkcode_becomes_inline_code() {
        let out = expand_to_markdown_escaped("{@linkcode myFunc}");
        assert_eq!(out, "`myFunc`");
    }

    #[test]
    fn markdown_linkplain_becomes_plain() {
        let out = expand_to_markdown_escaped("{@linkplain SomeType}");
        assert_eq!(out, "SomeType");
    }

    #[test]
    fn markdown_url_becomes_hyperlink() {
        let out = expand_to_markdown_escaped("{@link https://example.com}");
        assert_eq!(out, "[https://example.com](https://example.com)");
    }

    #[test]
    fn markdown_url_with_display_text() {
        let out = expand_to_markdown_escaped("{@link https://example.com Click here}");
        assert_eq!(out, "[Click here](https://example.com)");
    }

    #[test]
    fn markdown_prose_with_special_chars_is_escaped() {
        let out = expand_to_markdown_escaped("[brackets] before {@link Foo}.");
        assert!(out.contains("\\[brackets\\]"), "got: {out}");
        assert!(out.contains("`Foo`"), "got: {out}");
    }

    #[test]
    fn markdown_no_links_still_escapes() {
        let out = expand_to_markdown_escaped("see [here](there)");
        assert!(out.contains("\\[here\\]"), "got: {out}");
    }

    #[test]
    fn markdown_with_resolver() {
        let out = expand_to_markdown_with_resolver("{@link Foo}", |name| {
            if name == "Foo" {
                Some("file:///test.ts#L1".to_string())
            } else {
                None
            }
        });
        assert_eq!(out, "[Foo](file:///test.ts#L1)");
    }

    // ── build_doc_display_parts ────────────────────────────────────────────

    #[test]
    fn display_parts_simple_link() {
        // {open-link, linkName(Foo), close-link} = 3 parts
        let parts = build_doc_display_parts("{@link Foo}");
        let arr = parts.as_array().unwrap();
        assert_eq!(arr.len(), 3);
        assert_eq!(arr[0]["kind"], KIND_LINK);
        assert_eq!(arr[0]["text"], "{@link ");
        assert_eq!(arr[1]["kind"], KIND_LINK_NAME);
        assert_eq!(arr[1]["text"], "Foo");
        assert_eq!(arr[2]["kind"], KIND_LINK);
        assert_eq!(arr[2]["text"], "}");
    }

    #[test]
    fn display_parts_in_sentence() {
        // text("Use "), link("{@link "), linkName("Foo"), link("}"), text(" for details.")
        let parts = build_doc_display_parts("Use {@link Foo} for details.");
        let arr = parts.as_array().unwrap();
        assert_eq!(arr.len(), 5);
        assert_eq!(arr[0]["kind"], KIND_TEXT);
        assert_eq!(arr[0]["text"], "Use ");
        assert_eq!(arr[1]["kind"], KIND_LINK);
        assert_eq!(arr[1]["text"], "{@link ");
        assert_eq!(arr[2]["kind"], KIND_LINK_NAME);
        assert_eq!(arr[2]["text"], "Foo");
        assert_eq!(arr[3]["kind"], KIND_LINK);
        assert_eq!(arr[3]["text"], "}");
        assert_eq!(arr[4]["kind"], KIND_TEXT);
        assert_eq!(arr[4]["text"], " for details.");
    }

    #[test]
    fn display_parts_no_links_single_text_part() {
        let parts = build_doc_display_parts("plain text");
        let arr = parts.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["kind"], KIND_TEXT);
        assert_eq!(arr[0]["text"], "plain text");
    }

    #[test]
    fn display_parts_linkcode_tag_name() {
        // Opening link part contains the full tag prefix.
        let parts = build_doc_display_parts("{@linkcode myFunc}");
        let arr = parts.as_array().unwrap();
        assert_eq!(arr[0]["kind"], KIND_LINK);
        assert_eq!(arr[0]["text"], "{@linkcode ");
        assert_eq!(arr[1]["kind"], KIND_LINK_NAME);
        assert_eq!(arr[1]["text"], "myFunc");
    }

    #[test]
    fn display_parts_link_with_display_text() {
        // {open-link, linkName(Foo), linkText(the label), close-link} = 4 parts
        let parts = build_doc_display_parts("{@link Foo the label}");
        let arr = parts.as_array().unwrap();
        assert_eq!(arr.len(), 4);
        assert_eq!(arr[0]["kind"], KIND_LINK);
        assert_eq!(arr[0]["text"], "{@link ");
        assert_eq!(arr[1]["kind"], KIND_LINK_NAME);
        assert_eq!(arr[1]["text"], "Foo");
        assert_eq!(arr[2]["kind"], KIND_LINK_TEXT);
        assert_eq!(arr[2]["text"], "the label");
        assert_eq!(arr[3]["kind"], KIND_LINK);
        assert_eq!(arr[3]["text"], "}");
    }

    #[test]
    fn display_parts_url_with_display_text() {
        // URL: full "https://... Click here" as a single linkText part.
        let parts = build_doc_display_parts("{@link https://example.com Click here}");
        let arr = parts.as_array().unwrap();
        assert_eq!(arr.len(), 3);
        assert_eq!(arr[0]["kind"], KIND_LINK);
        assert_eq!(arr[0]["text"], "{@link ");
        assert_eq!(arr[1]["kind"], KIND_LINK_TEXT);
        assert_eq!(arr[1]["text"], "https://example.com Click here");
        assert_eq!(arr[2]["kind"], KIND_LINK);
        assert_eq!(arr[2]["text"], "}");
    }

    #[test]
    fn display_parts_url_no_display() {
        let parts = build_doc_display_parts("{@link https://example.com}");
        let arr = parts.as_array().unwrap();
        assert_eq!(arr.len(), 3);
        assert_eq!(arr[1]["kind"], KIND_LINK_TEXT);
        assert_eq!(arr[1]["text"], "https://example.com");
    }

    #[test]
    fn display_parts_resolver_sets_target_on_link_name() {
        let parts = build_doc_display_parts_with_resolver("{@link Foo}", |name| {
            if name == "Foo" {
                Some(serde_json::json!({"fileName": "test.ts", "textSpan": {"start": 0}}))
            } else {
                None
            }
        });
        let arr = parts.as_array().unwrap();
        // 3 parts: open-link, linkName(Foo)+target, close-link
        assert_eq!(arr.len(), 3);
        assert_eq!(arr[1]["kind"], KIND_LINK_NAME);
        assert_eq!(arr[1]["text"], "Foo");
        assert!(
            arr[1].get("target").is_some(),
            "linkName should have target field"
        );
    }
}
