//! Shared parsing helpers used by parser modules.
//! Keep this module focused on reusable token lookahead and token classification
//! logic without pulling in parser state behavior.

use tsz_scanner::SyntaxKind;
use tsz_scanner::scanner_impl::ScannerState;

/// Check if a token is an identifier or any keyword token.
pub fn is_identifier_or_keyword(token: SyntaxKind) -> bool {
    token == SyntaxKind::Identifier || tsz_scanner::token_is_keyword(token)
}

/// Check if a token is an identifier or contextual keyword (but NOT a reserved word).
/// Matches tsc's `isIdentifier()` behavior — contextual keywords like `type`, `async`,
/// `of` etc. can be binding names, but reserved words like `import`, `class`, `for` cannot.
pub fn is_identifier_or_contextual_keyword(token: SyntaxKind) -> bool {
    token == SyntaxKind::Identifier
        || (tsz_scanner::token_is_keyword(token) && !tsz_scanner::token_is_reserved_word(token))
}

/// Look ahead to check if current token is followed by a token matching `check`.
pub fn look_ahead_is<F>(scanner: &mut ScannerState, _current_token: SyntaxKind, check: F) -> bool
where
    F: FnOnce(SyntaxKind) -> bool,
{
    let snapshot = scanner.save_state();
    let next = scanner.scan();

    let result = check(next);

    scanner.restore_state(snapshot);
    result
}

/// Look ahead to check if current token is followed on the **same line** by a token matching `check`.
/// Returns false if the next token has a preceding line break (ASI would apply).
pub fn look_ahead_is_on_same_line<F>(
    scanner: &mut ScannerState,
    _current_token: SyntaxKind,
    check: F,
) -> bool
where
    F: FnOnce(SyntaxKind) -> bool,
{
    let snapshot = scanner.save_state();
    let next = scanner.scan();
    let result = !scanner.has_preceding_line_break() && check(next);
    scanner.restore_state(snapshot);
    result
}

/// Look ahead to check if "async" is followed by a declaration keyword.
pub fn look_ahead_is_async_declaration(
    scanner: &mut ScannerState,
    current_token: SyntaxKind,
) -> bool {
    look_ahead_is(scanner, current_token, |token| {
        matches!(
            token,
            SyntaxKind::ClassKeyword
                | SyntaxKind::FunctionKeyword
                | SyntaxKind::InterfaceKeyword
                | SyntaxKind::EnumKeyword
                | SyntaxKind::NamespaceKeyword
                | SyntaxKind::ModuleKeyword
        )
    })
}

/// Look ahead to check if "abstract" is followed by a declaration keyword.
pub fn look_ahead_is_abstract_declaration(
    scanner: &mut ScannerState,
    current_token: SyntaxKind,
) -> bool {
    look_ahead_is(scanner, current_token, |token| {
        matches!(
            token,
            SyntaxKind::ClassKeyword
                | SyntaxKind::InterfaceKeyword
                | SyntaxKind::EnumKeyword
                | SyntaxKind::NamespaceKeyword
                | SyntaxKind::ModuleKeyword
        )
    })
}

/// Look ahead to check if `namespace`/`module` is followed by a declaration name on the same line.
/// ASI prevents treating `namespace\nfoo` as a namespace declaration.
///
/// The next token must be a valid namespace name: an identifier, a keyword (except
/// for binary operators `in`/`instanceof`), a string literal, `{`, or a numeric literal.
/// Binary operators like `in` would parse as an expression instead (e.g., `module in {}`).
/// Other reserved keywords like `void`, `return`, `class` are accepted as namespace
/// names here, but the parser will emit TS2819 (Namespace name cannot be 'X').
pub fn look_ahead_is_module_declaration(
    scanner: &mut ScannerState,
    current_token: SyntaxKind,
) -> bool {
    look_ahead_is_on_same_line(scanner, current_token, |token| {
        matches!(
            token,
            SyntaxKind::StringLiteral | SyntaxKind::OpenBraceToken | SyntaxKind::NumericLiteral
        ) || token == SyntaxKind::Identifier
            || (tsz_scanner::token_is_keyword(token)
                && !matches!(token, SyntaxKind::InKeyword | SyntaxKind::InstanceOfKeyword))
    })
}

/// Look ahead to check if `type` begins a type alias declaration.
/// ASI prevents treating `type\nFoo = ...` as a type alias declaration.
pub fn look_ahead_is_type_alias_declaration(
    scanner: &mut ScannerState,
    current_token: SyntaxKind,
) -> bool {
    look_ahead_is_on_same_line(scanner, current_token, |token| {
        (is_identifier_or_keyword(token) && token != SyntaxKind::VoidKeyword)
            || token == SyntaxKind::NumericLiteral
    })
}

/// Look ahead to check if we have `const enum`.
pub fn look_ahead_is_const_enum(scanner: &mut ScannerState, current_token: SyntaxKind) -> bool {
    look_ahead_is(scanner, current_token, |token| {
        token == SyntaxKind::EnumKeyword
    })
}

/// Look ahead to check if current token begins an import equals declaration.
pub fn look_ahead_is_import_equals(
    scanner: &mut ScannerState,
    _current_token: SyntaxKind,
    is_identifier_fn: impl Fn(SyntaxKind) -> bool,
) -> bool {
    let snapshot = scanner.save_state();

    let next1 = scanner.scan();
    if tsz_scanner::token_is_reserved_word(next1) {
        let next2 = scanner.scan();
        scanner.restore_state(snapshot);
        return next2 == SyntaxKind::EqualsToken;
    }
    if !is_identifier_fn(next1) {
        scanner.restore_state(snapshot);
        return false;
    }

    let next2 = scanner.scan();
    if next2 == SyntaxKind::EqualsToken {
        scanner.restore_state(snapshot);
        return true;
    }

    // Handle `import type ...` and `import defer ...` — these contextual keywords
    // can be either modifiers or the import name itself.
    // tsc uses `tokenAfterImportDefinitelyProducesImportDeclaration` which checks
    // for `{`, `*`, `from` to decide if the keyword is a modifier. If it IS a
    // modifier and a real identifier follows, then the standard `,`/`from` check
    // decides import-declaration vs import-equals.
    if next1 == SyntaxKind::TypeKeyword || next1 == SyntaxKind::DeferKeyword {
        // `import type/defer {` or `import type/defer *` → modifier, import-declaration
        if next2 == SyntaxKind::OpenBraceToken || next2 == SyntaxKind::AsteriskToken {
            scanner.restore_state(snapshot);
            return false;
        }
        // `import type/defer from ...` — ambiguous: `from` could be the keyword or
        // the binding name. In tsc, when `type` is followed by an identifier-like
        // token (including `from`), tsc sets isTypeOnly=true, parses `from` as the
        // binding name, then checks if the NEXT token produces an import-declaration
        // (`,` or `from` keyword) or falls through to import-equals.
        // So `import type from "mod"` → import-declaration (type is modifier, `from`
        // is name, then `from` keyword + string).
        // But `import type from = require(...)` → import-equals (type is modifier,
        // `from` is name, `=` is not `,`/`from`).
        if next2 == SyntaxKind::FromKeyword {
            let next3 = scanner.scan();
            scanner.restore_state(snapshot);
            if next3 == SyntaxKind::EqualsToken {
                // `import type from =` → type-only import-equals with `from` as name
                return true;
            }
            // `import type from "mod"` or `import type from , ...` → import-declaration
            return false;
        }
        // `import type/defer <identifier> ...` where identifier is not `from`
        if is_identifier_fn(next2) {
            // `import defer type ...` is a modifier conflict that tsc parses as
            // an import-declaration (with `defer` as modifier, `type` as the
            // default-import name, then expects `from` at the next significant
            // token). Without this guard, `defer type *` erroneously routes to
            // the import-equals path and emits `'=' expected` at the `type`
            // keyword instead of `'from' expected` at the `*`. The reverse
            // ordering `import type defer *` is intentionally NOT short-
            // circuited — tsc treats `type` as a modifier and `defer` as the
            // import name, then routes through the import-equals lookahead.
            if next1 == SyntaxKind::DeferKeyword && next2 == SyntaxKind::TypeKeyword {
                scanner.restore_state(snapshot);
                return false;
            }
            let next3 = scanner.scan();
            scanner.restore_state(snapshot);
            if next3 == SyntaxKind::EqualsToken {
                return true;
            }
            // Match tsc: after `import type <id>`, if the next token is NOT `,` or
            // `from`, the identifier doesn't definitely produce an import declaration,
            // so tsc falls through to parseImportEqualsDeclaration.
            if next3 != SyntaxKind::CommaToken && next3 != SyntaxKind::FromKeyword {
                return true;
            }
            return false;
        }
        // `import type/defer <other>` — keyword is the import name (falls through below)
    }

    scanner.restore_state(snapshot);

    // Match tsc: after `import <identifier>`, if the next token is NOT `,` or
    // `from` keyword, tsc's `tokenAfterImportedIdentifierDefinitelyProducesImportDeclaration`
    // returns false and tsc falls through to parseImportEqualsDeclaration.
    // This handles cases like `import Foo From './Foo'` (capital F — not the `from` keyword)
    // where tsc emits "'=' expected." instead of "'from' expected.".
    if next2 != SyntaxKind::CommaToken && next2 != SyntaxKind::FromKeyword {
        return true;
    }

    false
}

/// Look ahead to check if we have `import (`/`import.`/`import<` (dynamic import forms).
/// `import<` is included so it routes through the expression parser which emits TS1326.
pub fn look_ahead_is_import_call(scanner: &mut ScannerState, current_token: SyntaxKind) -> bool {
    look_ahead_is(scanner, current_token, |token| {
        matches!(
            token,
            SyntaxKind::OpenParenToken | SyntaxKind::DotToken | SyntaxKind::LessThanToken
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Make a scanner positioned at the **first** token of `source`.
    /// The look-ahead helpers expect to be called when the scanner has
    /// just consumed `current_token`; they then peek at the next token.
    /// Returns `(scanner, current_token)`.
    fn scanner_after_first(source: &str) -> (ScannerState, SyntaxKind) {
        let mut scanner = ScannerState::new(source.to_string(), true);
        let first = scanner.scan();
        (scanner, first)
    }

    // ---------- is_identifier_or_keyword ----------------------------------

    #[test]
    fn is_identifier_or_keyword_accepts_identifier() {
        assert!(is_identifier_or_keyword(SyntaxKind::Identifier));
    }

    #[test]
    fn is_identifier_or_keyword_accepts_reserved_keyword() {
        assert!(is_identifier_or_keyword(SyntaxKind::ClassKeyword));
        assert!(is_identifier_or_keyword(SyntaxKind::ImportKeyword));
        assert!(is_identifier_or_keyword(SyntaxKind::ReturnKeyword));
    }

    #[test]
    fn is_identifier_or_keyword_accepts_contextual_keyword() {
        assert!(is_identifier_or_keyword(SyntaxKind::TypeKeyword));
        assert!(is_identifier_or_keyword(SyntaxKind::AsyncKeyword));
        assert!(is_identifier_or_keyword(SyntaxKind::OfKeyword));
    }

    #[test]
    fn is_identifier_or_keyword_rejects_punctuation_and_literals() {
        assert!(!is_identifier_or_keyword(SyntaxKind::OpenBraceToken));
        assert!(!is_identifier_or_keyword(SyntaxKind::EqualsToken));
        assert!(!is_identifier_or_keyword(SyntaxKind::StringLiteral));
        assert!(!is_identifier_or_keyword(SyntaxKind::NumericLiteral));
    }

    // ---------- is_identifier_or_contextual_keyword ------------------------

    #[test]
    fn contextual_only_accepts_identifier_and_non_reserved_keywords() {
        assert!(is_identifier_or_contextual_keyword(SyntaxKind::Identifier));
        assert!(is_identifier_or_contextual_keyword(SyntaxKind::TypeKeyword));
        assert!(is_identifier_or_contextual_keyword(SyntaxKind::OfKeyword));
        assert!(is_identifier_or_contextual_keyword(
            SyntaxKind::AsyncKeyword
        ));
    }

    #[test]
    fn contextual_only_rejects_reserved_words() {
        assert!(!is_identifier_or_contextual_keyword(
            SyntaxKind::ClassKeyword
        ));
        assert!(!is_identifier_or_contextual_keyword(
            SyntaxKind::ImportKeyword
        ));
        assert!(!is_identifier_or_contextual_keyword(SyntaxKind::ForKeyword));
    }

    #[test]
    fn contextual_only_rejects_punctuation_and_literals() {
        assert!(!is_identifier_or_contextual_keyword(
            SyntaxKind::OpenBraceToken
        ));
        assert!(!is_identifier_or_contextual_keyword(
            SyntaxKind::StringLiteral
        ));
    }

    // ---------- look_ahead_is restores scanner state -----------------------

    #[test]
    fn look_ahead_is_does_not_advance_scanner() {
        let (mut scanner, current) = scanner_after_first("foo bar");
        assert_eq!(current, SyntaxKind::Identifier);
        let result = look_ahead_is(&mut scanner, current, |t| t == SyntaxKind::Identifier);
        assert!(result, "expected `bar` to be classified as an identifier");
        // After the look-ahead, scanning again must see `bar`, proving the
        // snapshot was restored.
        let after = scanner.scan();
        assert_eq!(after, SyntaxKind::Identifier);
        assert_eq!(scanner.get_token_value(), "bar");
    }

    #[test]
    fn look_ahead_is_returns_false_when_check_fails() {
        let (mut scanner, current) = scanner_after_first("foo;");
        let result = look_ahead_is(&mut scanner, current, |t| t == SyntaxKind::OpenBraceToken);
        assert!(!result);
    }

    // ---------- look_ahead_is_on_same_line ---------------------------------

    #[test]
    fn look_ahead_is_on_same_line_true_without_line_break() {
        let (mut scanner, current) = scanner_after_first("foo bar");
        let result =
            look_ahead_is_on_same_line(&mut scanner, current, |t| t == SyntaxKind::Identifier);
        assert!(result);
    }

    #[test]
    fn look_ahead_is_on_same_line_false_with_line_break() {
        let (mut scanner, current) = scanner_after_first("foo\nbar");
        let result =
            look_ahead_is_on_same_line(&mut scanner, current, |t| t == SyntaxKind::Identifier);
        assert!(
            !result,
            "ASI: a line break before `bar` must make `look_ahead_is_on_same_line` return false"
        );
    }

    // ---------- look_ahead_is_async_declaration ----------------------------

    #[test]
    fn look_ahead_is_async_declaration_accepts_function_class_interface_etc() {
        for next in &["function f(){}", "class C{}", "interface I{}", "enum E{}"] {
            let source = format!("async {next}");
            let (mut scanner, current) = scanner_after_first(&source);
            assert_eq!(current, SyntaxKind::AsyncKeyword);
            assert!(
                look_ahead_is_async_declaration(&mut scanner, current),
                "expected `async {next}` to be classified as an async declaration"
            );
        }
    }

    #[test]
    fn look_ahead_is_async_declaration_rejects_arrow() {
        let (mut scanner, current) = scanner_after_first("async () => 1");
        assert!(!look_ahead_is_async_declaration(&mut scanner, current));
    }

    // ---------- look_ahead_is_abstract_declaration -------------------------

    #[test]
    fn look_ahead_is_abstract_declaration_accepts_class() {
        let (mut scanner, current) = scanner_after_first("abstract class C {}");
        assert_eq!(current, SyntaxKind::AbstractKeyword);
        assert!(look_ahead_is_abstract_declaration(&mut scanner, current));
    }

    #[test]
    fn look_ahead_is_abstract_declaration_rejects_identifier_after() {
        let (mut scanner, current) = scanner_after_first("abstract foo");
        assert!(!look_ahead_is_abstract_declaration(&mut scanner, current));
    }

    // ---------- look_ahead_is_module_declaration ---------------------------

    #[test]
    fn look_ahead_is_module_declaration_accepts_string_literal_name() {
        let (mut scanner, current) = scanner_after_first(r#"module "external" {}"#);
        assert_eq!(current, SyntaxKind::ModuleKeyword);
        assert!(look_ahead_is_module_declaration(&mut scanner, current));
    }

    #[test]
    fn look_ahead_is_module_declaration_accepts_identifier_name() {
        let (mut scanner, current) = scanner_after_first("namespace Foo {}");
        assert_eq!(current, SyntaxKind::NamespaceKeyword);
        assert!(look_ahead_is_module_declaration(&mut scanner, current));
    }

    #[test]
    fn look_ahead_is_module_declaration_rejects_in_keyword() {
        // Binary `in` / `instanceof` are intentionally rejected so that
        // `module in obj` parses as an expression, not as a namespace decl.
        let (mut scanner, current) = scanner_after_first("module in obj");
        assert!(!look_ahead_is_module_declaration(&mut scanner, current));
    }

    #[test]
    fn look_ahead_is_module_declaration_false_after_line_break() {
        // ASI: `namespace\nFoo` must NOT parse as a namespace decl.
        let (mut scanner, current) = scanner_after_first("namespace\nFoo {}");
        assert!(!look_ahead_is_module_declaration(&mut scanner, current));
    }

    // ---------- look_ahead_is_type_alias_declaration -----------------------

    #[test]
    fn look_ahead_is_type_alias_declaration_accepts_identifier() {
        let (mut scanner, current) = scanner_after_first("type Foo = number");
        assert_eq!(current, SyntaxKind::TypeKeyword);
        assert!(look_ahead_is_type_alias_declaration(&mut scanner, current));
    }

    #[test]
    fn look_ahead_is_type_alias_declaration_false_after_line_break() {
        // ASI: `type\nFoo = ...` must not parse as a type alias decl.
        let (mut scanner, current) = scanner_after_first("type\nFoo = number");
        assert!(!look_ahead_is_type_alias_declaration(&mut scanner, current));
    }

    #[test]
    fn look_ahead_is_type_alias_declaration_rejects_void_keyword() {
        let (mut scanner, current) = scanner_after_first("type void = T");
        assert_eq!(current, SyntaxKind::TypeKeyword);
        assert!(!look_ahead_is_type_alias_declaration(&mut scanner, current));
    }

    // ---------- look_ahead_is_const_enum -----------------------------------

    #[test]
    fn look_ahead_is_const_enum_true_for_const_enum() {
        let (mut scanner, current) = scanner_after_first("const enum E {}");
        assert_eq!(current, SyntaxKind::ConstKeyword);
        assert!(look_ahead_is_const_enum(&mut scanner, current));
    }

    #[test]
    fn look_ahead_is_const_enum_false_for_const_var() {
        let (mut scanner, current) = scanner_after_first("const x = 1");
        assert!(!look_ahead_is_const_enum(&mut scanner, current));
    }

    // ---------- look_ahead_is_import_call ----------------------------------

    #[test]
    fn look_ahead_is_import_call_accepts_open_paren() {
        let (mut scanner, current) = scanner_after_first(r#"import("./mod")"#);
        assert_eq!(current, SyntaxKind::ImportKeyword);
        assert!(look_ahead_is_import_call(&mut scanner, current));
    }

    #[test]
    fn look_ahead_is_import_call_accepts_dot_for_meta() {
        let (mut scanner, current) = scanner_after_first("import.meta");
        assert!(look_ahead_is_import_call(&mut scanner, current));
    }

    #[test]
    fn look_ahead_is_import_call_accepts_less_than_for_generic() {
        // `import<` is intentionally captured so the expression parser can
        // emit TS1326 instead of routing into the import-decl path.
        let (mut scanner, current) = scanner_after_first("import<T>");
        assert!(look_ahead_is_import_call(&mut scanner, current));
    }

    #[test]
    fn look_ahead_is_import_call_rejects_identifier() {
        let (mut scanner, current) = scanner_after_first(r#"import foo from "m""#);
        assert!(!look_ahead_is_import_call(&mut scanner, current));
    }
}
