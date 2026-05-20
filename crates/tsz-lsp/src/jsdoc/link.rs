//! Parsing and rendering of inline JSDoc link tags.
//!
//! Recognizes the three flavors defined by the TypeScript handbook:
//!
//! - `{@link Target}`            — plain prose link
//! - `{@linkcode Target}`        — link rendered in code voice
//! - `{@linkplain Target}`       — explicit plain voice
//!
//! Each may be followed by an optional display string, separated by either
//! whitespace or a `|` (e.g. `{@link Foo Bar baz}` / `{@link Foo|Bar baz}`).
//! The display string ends at the closing `}` and any leading whitespace or
//! `|` between the target and the display is consumed.
//!
//! This module owns *parsing* of the textual form and *rendering* of the
//! replacement Markdown or plain text. Resolution of the target to a symbol
//! lives outside (`LinkUriResolver`) so the parser stays decoupled from any
//! one symbol table or LSP feature.

/// Visual style requested by a link tag.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum LinkStyle {
    /// Default. Display text rendered as ordinary prose.
    Plain,
    /// Display text wrapped in backticks (Markdown inline code).
    Code,
}

/// One inline link occurrence inside a JSDoc comment body.
#[derive(Debug, Clone)]
pub struct InlineLink<'a> {
    /// Byte range of the full `{@link ...}` token, including braces.
    pub span: std::ops::Range<usize>,
    /// The textual target reference (e.g. `Foo`, `Foo.bar`, `module#Foo`).
    pub target: &'a str,
    /// Optional display text following the target. `None` means the link
    /// should render its target as the visible label.
    pub display: Option<&'a str>,
    /// Plain prose / code voice toggle.
    pub style: LinkStyle,
}

/// Find all `{@link …}` / `{@linkcode …}` / `{@linkplain …}` tokens in `text`.
///
/// Detection is name-agnostic: any identifier of the user's choice may appear
/// as the target; only the tag keyword itself is fixed (§25).
pub fn iter_inline_links(text: &str) -> Vec<InlineLink<'_>> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(rel) = text[i..].find("{@") {
        let start = i + rel;
        let after_at = start + 2;
        let Some((style, tag_end)) = match_tag_keyword(&bytes[after_at..]) else {
            i = start + 2;
            continue;
        };
        let body_start = after_at + tag_end;
        let Some(close_rel) = text[body_start..].find('}') else {
            // No matching `}` — bail out; remaining text cannot contain
            // further well-formed links because they all need a `}`.
            break;
        };
        let body = &text[body_start..body_start + close_rel];
        let (target, display) = split_target_and_display(body);
        if target.is_empty() {
            i = body_start + close_rel + 1;
            continue;
        }
        out.push(InlineLink {
            span: start..body_start + close_rel + 1,
            target,
            display,
            style,
        });
        i = body_start + close_rel + 1;
    }
    out
}

/// Recognize one of `link`, `linkcode`, `linkplain` immediately following
/// `{@`. Returns the link style and the byte length of the keyword followed
/// by exactly one separator (whitespace). The keyword must be terminated by
/// whitespace or `}`; `{@linkable}` is not a link tag.
fn match_tag_keyword(after_at: &[u8]) -> Option<(LinkStyle, usize)> {
    const CANDIDATES: &[(&[u8], LinkStyle)] = &[
        (b"linkcode", LinkStyle::Code),
        (b"linkplain", LinkStyle::Plain),
        (b"link", LinkStyle::Plain),
    ];
    for (kw, style) in CANDIDATES {
        if after_at.len() >= kw.len() && after_at.starts_with(kw) {
            let next = after_at.get(kw.len()).copied();
            match next {
                Some(b'}') => return Some((*style, kw.len())),
                Some(b) if b.is_ascii_whitespace() => return Some((*style, kw.len() + 1)),
                _ => continue,
            }
        }
    }
    None
}

/// Split the body inside `{@link …}` into `(target, display)`.
fn split_target_and_display(body: &str) -> (&str, Option<&str>) {
    let trimmed = body.trim();
    let (target, rest) = match trimmed.find(|c: char| c.is_whitespace() || c == '|') {
        Some(sep) => (&trimmed[..sep], &trimmed[sep..]),
        None => (trimmed, ""),
    };
    let display = rest
        .trim_start_matches(|c: char| c.is_whitespace() || c == '|')
        .trim_end();
    (target, (!display.is_empty()).then_some(display))
}

/// Side-band resolver from a target reference to an LSP URI.
///
/// Implementations resolve `target` (e.g. `Foo`, `Foo.bar`) to a URI that the
/// editor can navigate to, or return `None` to indicate the link is broken;
/// broken links must render as plain text so hover never crashes.
pub trait LinkUriResolver {
    fn resolve_link_uri(&mut self, target: &str) -> Option<String>;
}

impl<F> LinkUriResolver for F
where
    F: FnMut(&str) -> Option<String>,
{
    fn resolve_link_uri(&mut self, target: &str) -> Option<String> {
        (self)(target)
    }
}

/// Rewrite every inline link token in `text` into Markdown.
///
/// Resolved links render as `[label](uri)` (or `` [`label`](uri) `` for
/// `{@linkcode}`). Unresolved links degrade to plain text so editors that
/// don't process Markdown links still see the human-readable form, and so a
/// broken target never crashes hover (acceptance criterion).
pub fn expand_links_to_markdown(text: &str, resolver: &mut impl LinkUriResolver) -> String {
    rewrite_links(text, resolver, RenderMode::Markdown)
}

/// Rewrite inline link tokens in `text` into plain text for hovers that do
/// not surface Markdown (e.g. tsserver-style `documentation` field).
pub fn expand_links_to_plain(text: &str, resolver: &mut impl LinkUriResolver) -> String {
    rewrite_links(text, resolver, RenderMode::Plain)
}

#[derive(Copy, Clone)]
enum RenderMode {
    Markdown,
    Plain,
}

fn rewrite_links(text: &str, resolver: &mut impl LinkUriResolver, mode: RenderMode) -> String {
    let links = iter_inline_links(text);
    if links.is_empty() {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    for link in links {
        out.push_str(&text[cursor..link.span.start]);
        let label = link.display.unwrap_or(link.target);
        let uri = match mode {
            RenderMode::Markdown => resolver.resolve_link_uri(link.target),
            RenderMode::Plain => None,
        };
        let code_voice =
            matches!(link.style, LinkStyle::Code) && matches!(mode, RenderMode::Markdown);
        if let Some(uri) = uri {
            out.push('[');
            if code_voice {
                out.push('`');
            }
            out.push_str(label);
            if code_voice {
                out.push('`');
            }
            out.push_str("](");
            out.push_str(&uri);
            out.push(')');
        } else if code_voice {
            out.push('`');
            out.push_str(label);
            out.push('`');
        } else {
            out.push_str(label);
        }
        cursor = link.span.end;
    }
    out.push_str(&text[cursor..]);
    out
}

#[cfg(test)]
#[path = "../../tests/jsdoc_link_tests.rs"]
mod jsdoc_link_tests;
