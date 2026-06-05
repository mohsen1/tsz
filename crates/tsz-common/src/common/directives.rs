//! ECMAScript directive-prologue recognition.
//!
//! A *Directive Prologue* is the longest prefix of a function, module, or
//! script body consisting of `ExpressionStatement`s whose expression is a
//! string literal. The *Use Strict Directive* is the specific prologue
//! directive `"use strict"`.
//!
//! Per the ECMAScript specification, a Use Strict Directive "may not contain an
//! `EscapeSequence` or `LineContinuation`". In other words, recognition is
//! performed against the *verbatim source text* of the string literal - not its
//! cooked value. So a literal that spells the space (or any character) with an
//! escape sequence is not a Use Strict Directive even though its cooked value is
//! `use strict`. TypeScript follows the same rule: its
//! `isUseStrictPrologueDirective` compares the verbatim node text against
//! `"use strict"` / `'use strict'` rather than the unescaped string value.
//!
//! Recognising the directive against the cooked value would put a file into
//! strict mode that ECMAScript (and `tsc`) leave non-strict, producing spurious
//! strict-mode diagnostics (e.g. a false `TS1101` for a `with` statement that
//! is in fact legal).

/// The verbatim source text of a double-quoted `use strict` directive.
const USE_STRICT_DOUBLE_QUOTED: &str = "\"use strict\"";
/// The verbatim source text of a single-quoted `use strict` directive.
const USE_STRICT_SINGLE_QUOTED: &str = "'use strict'";

/// Returns `true` when `raw_text` is the verbatim source text of a `use strict`
/// directive prologue.
///
/// `raw_text` must be the string literal's source text *including* its
/// surrounding quote characters, e.g. the 12-byte slice `"use strict"`. Forms
/// that share the cooked value `use strict` but differ in source text - escape
/// sequences in place of a character, alternate spacing, or trailing content -
/// are intentionally rejected to match the ECMAScript directive grammar and `tsc`.
#[must_use]
pub fn is_use_strict_directive_raw_text(raw_text: &str) -> bool {
    raw_text == USE_STRICT_DOUBLE_QUOTED || raw_text == USE_STRICT_SINGLE_QUOTED
}

/// Returns `true` when a string-literal directive should be treated as
/// `use strict`, given its optional raw source text and its cooked value.
///
/// The raw source text is authoritative and is used whenever it is available
/// (it always is for literals produced by the parser). The cooked value is a
/// fallback for synthetic literal nodes that carry no recorded source text; in
/// that case the only signal available is the cooked value, so it is compared
/// directly.
#[must_use]
pub fn is_use_strict_directive(raw_text: Option<&str>, cooked_text: &str) -> bool {
    match raw_text {
        Some(raw) => is_use_strict_directive_raw_text(raw),
        None => cooked_text == "use strict",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_double_quoted_is_directive() {
        assert!(is_use_strict_directive_raw_text("\"use strict\""));
    }

    #[test]
    fn plain_single_quoted_is_directive() {
        assert!(is_use_strict_directive_raw_text("'use strict'"));
    }

    #[test]
    fn escaped_form_is_not_directive() {
        // Cooked value is `use strict`, but the escape disqualifies it.
        assert!(!is_use_strict_directive_raw_text("\"use\\u0020strict\""));
        assert!(!is_use_strict_directive_raw_text("'use\\x20strict'"));
        assert!(!is_use_strict_directive_raw_text("\"\\u0075se strict\""));
    }

    #[test]
    fn alternate_spacing_or_content_is_not_directive() {
        assert!(!is_use_strict_directive_raw_text("\"use strict \""));
        assert!(!is_use_strict_directive_raw_text("\" use strict\""));
        assert!(!is_use_strict_directive_raw_text("\"use  strict\""));
        assert!(!is_use_strict_directive_raw_text("`use strict`"));
        assert!(!is_use_strict_directive_raw_text("use strict"));
    }

    #[test]
    fn cooked_fallback_only_applies_without_raw_text() {
        // With raw text present, the escaped form is rejected even though the
        // cooked value matches.
        assert!(!is_use_strict_directive(
            Some("\"use\\u0020strict\""),
            "use strict"
        ));
        // Without raw text, the cooked value is the only available signal.
        assert!(is_use_strict_directive(None, "use strict"));
        assert!(!is_use_strict_directive(None, "use client"));
        // With raw text present, plain forms are accepted.
        assert!(is_use_strict_directive(Some("'use strict'"), "use strict"));
    }
}
