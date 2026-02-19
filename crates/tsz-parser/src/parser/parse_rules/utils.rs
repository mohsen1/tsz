//! Shared parsing helpers used by parser modules.
//! Keep this module focused on reusable token lookahead and token classification
//! logic without pulling in parser state behavior.

use tsz_scanner::SyntaxKind;
use tsz_scanner::scanner_impl::ScannerState;

/// Check if a token is an identifier or any keyword token.
pub fn is_identifier_or_keyword(token: SyntaxKind) -> bool {
    token == SyntaxKind::Identifier || tsz_scanner::token_is_keyword(token)
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

/// Look ahead to check if `namespace`/`module` is followed by a declaration name.
pub fn look_ahead_is_module_declaration(
    scanner: &mut ScannerState,
    current_token: SyntaxKind,
) -> bool {
    look_ahead_is(scanner, current_token, |token| {
        matches!(
            token,
            SyntaxKind::StringLiteral | SyntaxKind::OpenBraceToken
        ) || (token == SyntaxKind::Identifier
            || (tsz_scanner::token_is_keyword(token)
                && !tsz_scanner::token_is_reserved_word(token)))
    })
}

/// Look ahead to check if `type` begins a type alias declaration.
pub fn look_ahead_is_type_alias_declaration(
    scanner: &mut ScannerState,
    current_token: SyntaxKind,
) -> bool {
    look_ahead_is(scanner, current_token, is_identifier_or_keyword)
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
    if !is_identifier_fn(next1) {
        scanner.restore_state(snapshot);
        return false;
    }

    let next2 = scanner.scan();
    if next2 == SyntaxKind::EqualsToken {
        scanner.restore_state(snapshot);
        return true;
    }

    if next1 == SyntaxKind::TypeKeyword && is_identifier_fn(next2) {
        let next3 = scanner.scan();
        if next3 == SyntaxKind::EqualsToken {
            scanner.restore_state(snapshot);
            return true;
        }
    }

    scanner.restore_state(snapshot);
    false
}

/// Look ahead to check if we have `import (`/`import.` (dynamic import forms).
pub fn look_ahead_is_import_call(scanner: &mut ScannerState, current_token: SyntaxKind) -> bool {
    look_ahead_is(scanner, current_token, |token| {
        matches!(token, SyntaxKind::OpenParenToken | SyntaxKind::DotToken)
    })
}
