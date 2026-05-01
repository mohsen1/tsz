//! Integration tests for regular-expression emit recovery.

use tsz_emitter::output::printer::PrintOptions;

#[path = "test_support.rs"]
mod test_support;

use test_support::parse_and_print_with_opts;

fn print_es2015(source: &str) -> String {
    parse_and_print_with_opts(source, PrintOptions::es6())
}

/// Source `foo(/notregexp);` (TypeScript test
/// `parserRegularExpressionDivideAmbiguity4.ts`) parses the argument as an
/// unterminated regex. The recovered literal must not also print the call's
/// closing `);`, because the call and expression-statement emitters add those
/// tokens structurally.
#[test]
fn unterminated_regex_call_argument_does_not_duplicate_call_tail() {
    let output = print_es2015("foo(/notregexp);");
    assert!(
        output.contains("foo(/notregexp);"),
        "expected recovered call argument without duplicate tail; output:\n{output}"
    );
    assert!(
        !output.contains("foo(/notregexp););"),
        "recovered regex argument must not include the call tail; output:\n{output}"
    );
}

/// Regex flags are scanned as identifier parts, not only ASCII letters. The
/// source-text preservation path should therefore include non-BMP flag
/// characters already captured by the parser's regex token.
#[test]
fn regex_literal_preserves_non_bmp_flags() {
    let output = print_es2015("const 𝘳𝘦𝘨𝘦𝘹 = /(?𝘴𝘪-𝘮:^𝘧𝘰𝘰.)/𝘨𝘮𝘶;");
    assert!(
        output.contains("/(?𝘴𝘪-𝘮:^𝘧𝘰𝘰.)/𝘨𝘮𝘶"),
        "regex literal should preserve non-BMP flags; output:\n{output}"
    );
}
