use super::*;
use crate::source::FileId;

fn cooked(raw: &str) -> Option<String> {
    ScannedTemplateLiteral::terminated(Span::new(FileId(0), 0, raw.len()), raw)
        .syntax_literal()
        .map(|literal| literal.cooked)
}

#[test]
fn cooks_control_unicode_identity_and_line_continuation_escapes() {
    assert_eq!(
        cooked(r"`\0\x19\u001f\u{20}\t\v\f\b\r\n\world\``"),
        Some("\0\u{19}\u{1f} \t\u{b}\u{c}\u{8}\r\nworld`".to_string())
    );
    assert_eq!(cooked("`a\\\r\nb\\\nc`").as_deref(), Some("abc"));
    assert_eq!(cooked("`a\rb\r\nc`").as_deref(), Some("a\nb\nc"));
}

#[test]
fn rejects_legacy_octal_malformed_unicode_and_unrepresentable_surrogates() {
    for raw in [
        r"`\00`",
        r"`\8`",
        r"`\x0`",
        r"`\u123`",
        r"`\u{}`",
        r"`\u{110000}`",
        r"`\uD800`",
    ] {
        assert_eq!(cooked(raw), None, "{raw}");
    }
}
