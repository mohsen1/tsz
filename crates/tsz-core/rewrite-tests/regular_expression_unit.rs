use crate::source::{FileId, Span};

use super::RegularExpressionLiteral;

fn literal(pattern: &str, flags: &str, terminated: bool) -> RegularExpressionLiteral {
    RegularExpressionLiteral {
        raw: format!(
            "/{pattern}/{}{flags}",
            if terminated { "" } else { "missing" }
        ),
        pattern: pattern.to_string(),
        flags: flags.to_string(),
        pattern_span: Span::new(FileId(0), 1, pattern.len() + 1),
        flags_span: Span::new(
            FileId(0),
            pattern.len() + 2,
            pattern.len() + 2 + flags.len(),
        ),
        terminated,
        recovery_at_line_break: false,
    }
}

#[test]
fn validation_gate_admits_the_owned_ascii_and_unicode_family() {
    assert!(literal(r"(#?-?\d*\.\d\w*%?)|(@?#?[\w-?]+%?)", "g", true).validation_supported());
    for pattern in [r"\u{0}", r"\u{110000}", r"\u{-DDDD}", r"\u{r}", r"\u{}"] {
        assert!(
            literal(pattern, "gu", true).validation_supported(),
            "{pattern}"
        );
    }
    assert!(literal("x", "z", true).validation_supported());
    assert!(literal("abc", "", false).validation_supported());
}

#[test]
fn validation_gate_rejects_unowned_flags_escapes_ranges_and_recovery() {
    for flags in ["d", "s", "v", "gv"] {
        assert!(!literal("x", flags, true).validation_supported(), "{flags}");
    }
    for (pattern, flags) in [
        (r"\u{0}", "g"),
        (r"\u{0", "gu"),
        (r"\u{1x2}", "gu"),
        (r"\u{r2}", "gu"),
        (r"\u{\}", "gu"),
        (r"\u{\x}", "gu"),
        (r"\u{(}", "gu"),
        (r"\-", "u"),
        (r"[\B]", "u"),
        (r"[a-z]", "u"),
        (r"\b*", "g"),
        (r"\B+", "g"),
        (r"(?=x)", "g"),
        (r"\p{Letter}", "gu"),
        (r"[z-a]", "g"),
        (r"[[a]]", "g"),
    ] {
        assert!(
            !literal(pattern, flags, true).validation_supported(),
            "{pattern}/{flags}"
        );
    }
    assert!(!literal("[/", "", false).validation_supported());
    assert!(!literal("abc\\", "", false).validation_supported());
    assert!(!literal("abc;", "", false).validation_supported());
    assert!(!literal("abc ", "", false).validation_supported());
}
