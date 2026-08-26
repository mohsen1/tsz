use super::*;

fn cooked(raw: &str) -> Option<String> {
    scan_no_substitution_template(raw)
        .ok()
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

#[test]
fn shared_decoder_keeps_template_specific_diagnostic_spans_and_messages() {
    let hexadecimal = "Hexadecimal digit expected.";
    let range = "An extended Unicode escape value must be between 0x0 and 0x10FFFF inclusive.";
    let cases = [
        (r"`\x`", 1125, 3, 1, hexadecimal),
        (r"`\x0`", 1125, 4, 1, hexadecimal),
        (r"`\xG0`", 1125, 3, 1, hexadecimal),
        (r"`\x0G`", 1125, 4, 1, hexadecimal),
        (r"`\u`", 1125, 3, 1, hexadecimal),
        (r"`\u000`", 1125, 6, 1, hexadecimal),
        (r"`\u00G0`", 1125, 5, 1, hexadecimal),
        (r"`\u{}`", 1125, 4, 1, hexadecimal),
        (r"`\u{G}`", 1125, 4, 1, hexadecimal),
        (r"`\u{110000}`", 1198, 4, 6, range),
        (
            r"`\u{10FFFF`",
            1199,
            10,
            1,
            "Unterminated Unicode escape sequence.",
        ),
        (r"`\8`", 1488, 1, 2, "Escape sequence '\\8' is not allowed."),
        (r"`\9`", 1488, 1, 2, "Escape sequence '\\9' is not allowed."),
        (
            r"`\1`",
            1487,
            1,
            2,
            "Octal escape sequences are not allowed. Use the syntax '\\x01'.",
        ),
        (
            r"`\08`",
            1487,
            1,
            2,
            "Octal escape sequences are not allowed. Use the syntax '\\x00'.",
        ),
        (
            r"`\170`",
            1487,
            1,
            4,
            "Octal escape sequences are not allowed. Use the syntax '\\x78'.",
        ),
    ];

    for (raw, code, start, length, message) in cases {
        let Err(CookError::Diagnostic(diagnostic)) = scan_no_substitution_template(raw) else {
            panic!("expected an escape diagnostic for {raw}");
        };
        assert_eq!(
            (
                diagnostic.code,
                diagnostic.relative_start,
                diagnostic.length,
                diagnostic.message.as_str(),
            ),
            (code, start, length, message),
            "{raw}"
        );
    }
}
