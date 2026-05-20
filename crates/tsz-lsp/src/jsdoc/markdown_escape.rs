//! Markdown escaping helpers for `JSDoc`-derived hover content.
//!
//! LSP hover serializes `JSDoc` text into Markdown. User-supplied text can
//! contain Markdown delimiter characters (`[`, `]`, `(`, `)`, `` ` ``, `*`,
//! `_`, `<`, `>`, …) that would otherwise be interpreted as link, emphasis,
//! inline-code, or HTML markup by the editor's Markdown renderer.
//!
//! Two helpers cover the two rendering voices:
//!
//! - [`escape_markdown_label`] escapes inline Markdown delimiters inside prose
//!   and inside the label portion of Markdown links (`[label](uri)`). It uses
//!   backslash escapes per `CommonMark` §2.4 so the rendered text reads exactly
//!   as the user wrote it.
//! - [`format_inline_code`] wraps content in a backtick fence whose length is
//!   chosen so the longest backtick run inside the content cannot terminate
//!   the span (`CommonMark` §6.1). When the content starts or ends with a
//!   backtick a single padding space is added so the fence is unambiguous.
//!
//! Both helpers are name-independent: they react only to the structural
//! presence of delimiter characters, not to specific identifiers, file names,
//! or rendered diagnostic shapes.

/// Escape inline Markdown delimiters in user-supplied prose.
///
/// The escape set is the subset of `CommonMark` §2.4 characters that can change
/// the meaning of inline text in a typical renderer: link/code/emphasis
/// markers, HTML angle brackets, the backslash itself, and pipes (so the text
/// stays inert inside Markdown tables).
///
/// Returns the input unchanged when no escape characters are present, so the
/// common alphanumeric path avoids allocation.
pub fn escape_markdown_label(text: &str) -> String {
    if !text.contains(is_escapable_inline) {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len() + 4);
    for ch in text.chars() {
        if is_escapable_inline(ch) {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

const fn is_escapable_inline(ch: char) -> bool {
    matches!(
        ch,
        '\\' | '`'
            | '*'
            | '_'
            | '{'
            | '}'
            | '['
            | ']'
            | '('
            | ')'
            | '<'
            | '>'
            | '#'
            | '!'
            | '|'
            | '~'
    )
}

/// Render `content` as a Markdown inline code span (`` `…` ``), choosing a
/// fence length that cannot be terminated by any backtick run inside the
/// content. Returns the empty string when `content` is empty.
pub fn format_inline_code(content: &str) -> String {
    if content.is_empty() {
        return String::new();
    }
    let fence_len = longest_backtick_run(content) + 1;
    let fence: String = "`".repeat(fence_len);
    let needs_pad = content.starts_with('`') || content.ends_with('`');
    let mut out =
        String::with_capacity(content.len() + fence_len * 2 + if needs_pad { 2 } else { 0 });
    out.push_str(&fence);
    if needs_pad {
        out.push(' ');
    }
    out.push_str(content);
    if needs_pad {
        out.push(' ');
    }
    out.push_str(&fence);
    out
}

fn longest_backtick_run(text: &str) -> usize {
    let mut longest = 0;
    let mut current = 0;
    for ch in text.chars() {
        if ch == '`' {
            current += 1;
            if current > longest {
                longest = current;
            }
        } else {
            current = 0;
        }
    }
    longest
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_label_passes_alphanumeric_unchanged() {
        assert_eq!(
            escape_markdown_label("Adds two numbers"),
            "Adds two numbers"
        );
        assert_eq!(escape_markdown_label(""), "");
        assert_eq!(escape_markdown_label("123 abc"), "123 abc");
    }

    #[test]
    fn escape_label_escapes_link_brackets_and_parens() {
        assert_eq!(escape_markdown_label("foo[0]"), "foo\\[0\\]");
        assert_eq!(escape_markdown_label("see (note)"), "see \\(note\\)");
        assert_eq!(escape_markdown_label("[a](b)"), "\\[a\\]\\(b\\)",);
    }

    #[test]
    fn escape_label_escapes_emphasis_and_code_markers() {
        assert_eq!(escape_markdown_label("*hi*"), "\\*hi\\*");
        assert_eq!(escape_markdown_label("__bold__"), "\\_\\_bold\\_\\_");
        assert_eq!(escape_markdown_label("`code`"), "\\`code\\`");
    }

    #[test]
    fn escape_label_escapes_html_angles_and_backslash() {
        assert_eq!(escape_markdown_label("<b>"), "\\<b\\>");
        assert_eq!(escape_markdown_label("a\\b"), "a\\\\b");
    }

    #[test]
    fn escape_label_is_name_independent() {
        // The function must react to delimiter characters, not to specific
        // identifier spellings. Renaming alphanumeric content must not change
        // the escape pattern.
        let a = escape_markdown_label("Alpha[Beta]");
        let b = escape_markdown_label("Foo[Bar]");
        let c = escape_markdown_label("X[Y]");
        assert!(a.contains("\\[") && a.contains("\\]"));
        assert!(b.contains("\\[") && b.contains("\\]"));
        assert!(c.contains("\\[") && c.contains("\\]"));
    }

    #[test]
    fn inline_code_alphanumeric_uses_single_fence() {
        assert_eq!(format_inline_code("name"), "`name`");
        assert_eq!(format_inline_code("a b c"), "`a b c`");
    }

    #[test]
    fn inline_code_empty_returns_empty() {
        assert_eq!(format_inline_code(""), "");
    }

    #[test]
    fn inline_code_with_single_backtick_uses_double_fence() {
        // Content `foo`bar should render as ``foo`bar`` so the inner ` does
        // not close the span.
        assert_eq!(format_inline_code("foo`bar"), "``foo`bar``");
    }

    #[test]
    fn inline_code_with_double_backtick_uses_triple_fence() {
        assert_eq!(format_inline_code("a``b"), "```a``b```");
    }

    #[test]
    fn inline_code_with_leading_backtick_pads_with_space() {
        // `CommonMark` §6.1: when the content starts or ends with a backtick,
        // a single space pad makes the fence unambiguous; both pads are
        // stripped by the renderer.
        assert_eq!(format_inline_code("`leading"), "`` `leading ``");
        assert_eq!(format_inline_code("trailing`"), "`` trailing` ``");
    }

    #[test]
    fn inline_code_with_only_backticks_picks_longer_fence_and_pads() {
        assert_eq!(format_inline_code("`"), "`` ` ``");
        assert_eq!(format_inline_code("``"), "``` `` ```");
    }
}
