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
        is_identifier_or_keyword(token) || token == SyntaxKind::NumericLiteral
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
