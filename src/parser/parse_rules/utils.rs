//! Common parsing utilities
//!
//! This module contains shared parsing utilities that eliminate code duplication
//! across the parser. It consolidates repeated patterns like look-ahead functions
//! and modifier parsing.

use crate::scanner::SyntaxKind;
use crate::scanner_impl::ScannerState;

// Re-export token validation functions for use in look_ahead helpers
pub use self::token_validation::*;

// =============================================================================
// Look-Ahead Utilities (Consolidates 24 look_ahead functions)
// =============================================================================

/// Look ahead to check if current token is followed by a specific token.
///
/// This consolidates the repeated pattern:
/// ```rust
/// let snapshot = self.scanner.save_state();
/// let current = self.current_token;
/// self.next_token();
/// let result = /* check condition */;
/// self.scanner.restore_state(snapshot);
/// self.current_token = current;
/// result
/// ```
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

/// Look ahead to check if current token is followed by one of multiple tokens.
pub fn look_ahead_is_any_of(
    scanner: &mut ScannerState,
    current_token: SyntaxKind,
    kinds: &[SyntaxKind],
) -> bool {
    look_ahead_is(scanner, current_token, |token| kinds.contains(&token))
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

/// Look ahead to check if "accessor" is followed by a declaration keyword.
pub fn look_ahead_is_accessor_keyword(
    scanner: &mut ScannerState,
    current_token: SyntaxKind,
) -> bool {
    look_ahead_is(scanner, current_token, |token| {
        matches!(
            token,
            SyntaxKind::ClassKeyword | SyntaxKind::InterfaceKeyword | SyntaxKind::EnumKeyword
        )
    })
}

// =============================================================================
// Modifier Parsing Utilities
// =============================================================================

/// A modifier that was parsed.
#[derive(Debug, Clone, Copy)]
pub struct ParsedModifier {
    pub kind: SyntaxKind,
    pub start_pos: u32,
}

/// Parse a single modifier token.
///
/// Returns None if the current token is not a modifier.
/// Returns Some(ParsedModifier) with the modifier kind and position.
///
/// This consolidates the repeated pattern:
/// ```rust
/// SyntaxKind::StaticKeyword => {
///     self.next_token();
///     self.arena.create_modifier(SyntaxKind::StaticKeyword, start_pos)
/// }
/// ```
pub fn parse_modifier_token(token: SyntaxKind, start_pos: u32) -> Option<ParsedModifier> {
    match token {
        SyntaxKind::StaticKeyword
        | SyntaxKind::PublicKeyword
        | SyntaxKind::PrivateKeyword
        | SyntaxKind::ProtectedKeyword
        | SyntaxKind::ReadonlyKeyword
        | SyntaxKind::AbstractKeyword
        | SyntaxKind::OverrideKeyword
        | SyntaxKind::AsyncKeyword
        | SyntaxKind::DeclareKeyword
        | SyntaxKind::AccessorKeyword
        | SyntaxKind::ConstKeyword
        | SyntaxKind::ExportKeyword
        | SyntaxKind::DefaultKeyword
        | SyntaxKind::InKeyword => Some(ParsedModifier {
            kind: token,
            start_pos,
        }),
        _ => None,
    }
}

/// Check if a token is a valid class member modifier.
pub fn is_class_member_modifier(token: SyntaxKind) -> bool {
    matches!(
        token,
        SyntaxKind::StaticKeyword
            | SyntaxKind::PublicKeyword
            | SyntaxKind::PrivateKeyword
            | SyntaxKind::ProtectedKeyword
            | SyntaxKind::ReadonlyKeyword
            | SyntaxKind::AbstractKeyword
            | SyntaxKind::OverrideKeyword
            | SyntaxKind::AsyncKeyword
            | SyntaxKind::DeclareKeyword
            | SyntaxKind::AccessorKeyword
            | SyntaxKind::ConstKeyword
            | SyntaxKind::ExportKeyword
    )
}

/// Check if a token is a valid declaration modifier.
pub fn is_declaration_modifier(token: SyntaxKind) -> bool {
    matches!(
        token,
        SyntaxKind::ExportKeyword
            | SyntaxKind::DefaultKeyword
            | SyntaxKind::AsyncKeyword
            | SyntaxKind::DeclareKeyword
            | SyntaxKind::ConstKeyword
            | SyntaxKind::AbstractKeyword
            | SyntaxKind::AccessorKeyword
            | SyntaxKind::InKeyword
    )
}

/// Check if a token is a valid parameter modifier.
pub fn is_parameter_modifier(token: SyntaxKind) -> bool {
    matches!(
        token,
        SyntaxKind::PublicKeyword
            | SyntaxKind::PrivateKeyword
            | SyntaxKind::ProtectedKeyword
            | SyntaxKind::ReadonlyKeyword
            | SyntaxKind::AsyncKeyword
            | SyntaxKind::AccessorKeyword
    )
}

// =============================================================================
// Token Validation Utilities
// =============================================================================

mod token_validation {
    use super::*;

    /// Check if a token can start a type.
    pub fn can_token_start_type(token: SyntaxKind) -> bool {
        matches!(
            token,
            SyntaxKind::VoidKeyword
                | SyntaxKind::AnyKeyword
                | SyntaxKind::UnknownKeyword
                | SyntaxKind::NumberKeyword
                | SyntaxKind::BigIntKeyword
                | SyntaxKind::StringKeyword
                | SyntaxKind::BooleanKeyword
                | SyntaxKind::SymbolKeyword
                | SyntaxKind::NeverKeyword
                | SyntaxKind::ObjectKeyword
                | SyntaxKind::TrueKeyword
                | SyntaxKind::FalseKeyword
                | SyntaxKind::NullKeyword
                | SyntaxKind::UndefinedKeyword
                | SyntaxKind::ThisKeyword
                | SyntaxKind::OpenParenToken
                | SyntaxKind::OpenBracketToken
                | SyntaxKind::LessThanToken
                | SyntaxKind::StringLiteral
                | SyntaxKind::NoSubstitutionTemplateLiteral
                | SyntaxKind::TemplateHead
                | SyntaxKind::Identifier
        )
    }

    /// Check if a token is an identifier or keyword (can be used as identifier).
    pub fn is_identifier_or_keyword(token: SyntaxKind) -> bool {
        // Match TypeScript's isIdentifierOrKeyword: Identifier or any keyword
        token == SyntaxKind::Identifier || crate::scanner::token_is_keyword(token)
    }

    /// Check if a token is a valid property name.
    pub fn is_property_name(token: SyntaxKind) -> bool {
        matches!(
            token,
            SyntaxKind::Identifier
                | SyntaxKind::StringLiteral
                | SyntaxKind::NumericLiteral
                | SyntaxKind::BigIntLiteral
                | SyntaxKind::OpenBracketToken
                | SyntaxKind::BreakKeyword
                | SyntaxKind::CaseKeyword
                | SyntaxKind::CatchKeyword
                | SyntaxKind::ClassKeyword
                | SyntaxKind::ConstKeyword
                | SyntaxKind::ContinueKeyword
                | SyntaxKind::DebuggerKeyword
                | SyntaxKind::DefaultKeyword
                | SyntaxKind::DeleteKeyword
                | SyntaxKind::DoKeyword
                | SyntaxKind::ElseKeyword
                | SyntaxKind::EnumKeyword
                | SyntaxKind::ExportKeyword
                | SyntaxKind::ExtendsKeyword
                | SyntaxKind::FalseKeyword
                | SyntaxKind::FinallyKeyword
                | SyntaxKind::ForKeyword
                | SyntaxKind::FunctionKeyword
                | SyntaxKind::IfKeyword
                | SyntaxKind::ImportKeyword
                | SyntaxKind::InKeyword
                | SyntaxKind::InstanceOfKeyword
                | SyntaxKind::NewKeyword
                | SyntaxKind::NullKeyword
                | SyntaxKind::ReturnKeyword
                | SyntaxKind::SuperKeyword
                | SyntaxKind::SwitchKeyword
                | SyntaxKind::ThisKeyword
                | SyntaxKind::ThrowKeyword
                | SyntaxKind::TrueKeyword
                | SyntaxKind::TryKeyword
                | SyntaxKind::TypeOfKeyword
                | SyntaxKind::VarKeyword
                | SyntaxKind::VoidKeyword
                | SyntaxKind::WhileKeyword
                | SyntaxKind::WithKeyword
                | SyntaxKind::ConstructorKeyword
                | SyntaxKind::InterfaceKeyword
                | SyntaxKind::ReadonlyKeyword
                | SyntaxKind::TypeKeyword
                | SyntaxKind::AbstractKeyword
                | SyntaxKind::AccessorKeyword
                | SyntaxKind::AsyncKeyword
                | SyntaxKind::AwaitKeyword
                | SyntaxKind::DeclareKeyword
                | SyntaxKind::InferKeyword
                | SyntaxKind::IsKeyword
                | SyntaxKind::KeyOfKeyword
                | SyntaxKind::ModuleKeyword
                | SyntaxKind::NamespaceKeyword
                | SyntaxKind::NeverKeyword
                | SyntaxKind::OutKeyword
                | SyntaxKind::ProtectedKeyword
                | SyntaxKind::PublicKeyword
                | SyntaxKind::PrivateKeyword
                | SyntaxKind::OverrideKeyword
                | SyntaxKind::StaticKeyword
                | SyntaxKind::FromKeyword
                | SyntaxKind::AsKeyword
                | SyntaxKind::UsingKeyword
                | SyntaxKind::GetKeyword
                | SyntaxKind::SetKeyword
                | SyntaxKind::AssertsKeyword
                | SyntaxKind::AssertKeyword
                | SyntaxKind::GlobalKeyword
                | SyntaxKind::RequireKeyword
                | SyntaxKind::SatisfiesKeyword
                | SyntaxKind::IntrinsicKeyword
                | SyntaxKind::DeferKeyword
        )
    }
}

/// Check if a token is a literal.
pub fn is_literal(token: SyntaxKind) -> bool {
    matches!(
        token,
        SyntaxKind::NullKeyword
            | SyntaxKind::TrueKeyword
            | SyntaxKind::FalseKeyword
            | SyntaxKind::NumericLiteral
            | SyntaxKind::BigIntLiteral
            | SyntaxKind::StringLiteral
            | SyntaxKind::NoSubstitutionTemplateLiteral
    )
}

/// Look ahead to check if "namespace"/"module" starts a declaration.
pub fn look_ahead_is_module_declaration(
    scanner: &mut ScannerState,
    current_token: SyntaxKind,
) -> bool {
    look_ahead_is(scanner, current_token, |token| {
        matches!(
            token,
            SyntaxKind::Identifier | SyntaxKind::StringLiteral | SyntaxKind::OpenBraceToken
        )
    })
}

/// Look ahead to check if "type" starts a type alias declaration.
pub fn look_ahead_is_type_alias_declaration(
    scanner: &mut ScannerState,
    current_token: SyntaxKind,
) -> bool {
    look_ahead_is(scanner, current_token, |token| {
        token == SyntaxKind::Identifier
    })
}

/// Look ahead to check if we have "const enum".
pub fn look_ahead_is_const_enum(scanner: &mut ScannerState, current_token: SyntaxKind) -> bool {
    look_ahead_is(scanner, current_token, |token| {
        token == SyntaxKind::EnumKeyword
    })
}

/// Look ahead to check if we have "import identifier =" (import equals).
///
/// This is a two-token look-ahead: skip 'import', check for identifier, then check for '='.
pub fn look_ahead_is_import_equals(
    scanner: &mut ScannerState,
    _current_token: SyntaxKind,
    is_identifier_fn: impl FnOnce(SyntaxKind) -> bool,
) -> bool {
    let snapshot = scanner.save_state();

    // Skip 'import'
    let next1 = scanner.scan();

    // Check for identifier or keyword that can be used as identifier
    if !is_identifier_fn(next1) {
        scanner.restore_state(snapshot);
        return false;
    }

    // Skip identifier, check for '='
    let next2 = scanner.scan();
    let is_equals = next2 == SyntaxKind::EqualsToken;

    scanner.restore_state(snapshot);
    is_equals
}

/// Look ahead to check if we have "import (" (dynamic import).
pub fn look_ahead_is_import_call(scanner: &mut ScannerState, current_token: SyntaxKind) -> bool {
    look_ahead_is(scanner, current_token, |token| {
        matches!(token, SyntaxKind::OpenParenToken | SyntaxKind::DotToken)
    })
}
