//! Scanner types and utilities for TypeScript lexical analysis.
//!
//! This module contains the `SyntaxKind` enum, scanner implementation,
//! and character code constants for TypeScript lexical analysis.
// Scanner implementation - tokenization logic
pub mod scanner_impl;

// Character code constants used by the scanner
pub mod char_codes;

use serde::Serialize;
use wasm_bindgen::prelude::wasm_bindgen;

// =============================================================================
// SyntaxKind Enum - Token Types (Scanner Output)
// =============================================================================

/// Syntax kind enum matching TypeScript's `SyntaxKind`.
/// This enum contains only the token types produced by the scanner (0-186).
/// AST node types are not included here.
#[wasm_bindgen]
#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Hash, Serialize)]
pub enum SyntaxKind {
    Unknown = 0,
    EndOfFileToken = 1,
    SingleLineCommentTrivia = 2,
    MultiLineCommentTrivia = 3,
    NewLineTrivia = 4,
    WhitespaceTrivia = 5,
    ShebangTrivia = 6,
    ConflictMarkerTrivia = 7,
    NonTextFileMarkerTrivia = 8,
    // Literals
    NumericLiteral = 9,
    BigIntLiteral = 10,
    StringLiteral = 11,
    JsxText = 12,
    JsxTextAllWhiteSpaces = 13,
    RegularExpressionLiteral = 14,
    NoSubstitutionTemplateLiteral = 15,
    // Pseudo-literals
    TemplateHead = 16,
    TemplateMiddle = 17,
    TemplateTail = 18,
    // Punctuation
    OpenBraceToken = 19,
    CloseBraceToken = 20,
    OpenParenToken = 21,
    CloseParenToken = 22,
    OpenBracketToken = 23,
    CloseBracketToken = 24,
    DotToken = 25,
    DotDotDotToken = 26,
    SemicolonToken = 27,
    CommaToken = 28,
    QuestionDotToken = 29,
    LessThanToken = 30,
    LessThanSlashToken = 31,
    GreaterThanToken = 32,
    LessThanEqualsToken = 33,
    GreaterThanEqualsToken = 34,
    EqualsEqualsToken = 35,
    ExclamationEqualsToken = 36,
    EqualsEqualsEqualsToken = 37,
    ExclamationEqualsEqualsToken = 38,
    EqualsGreaterThanToken = 39,
    PlusToken = 40,
    MinusToken = 41,
    AsteriskToken = 42,
    AsteriskAsteriskToken = 43,
    SlashToken = 44,
    PercentToken = 45,
    PlusPlusToken = 46,
    MinusMinusToken = 47,
    LessThanLessThanToken = 48,
    GreaterThanGreaterThanToken = 49,
    GreaterThanGreaterThanGreaterThanToken = 50,
    AmpersandToken = 51,
    BarToken = 52,
    CaretToken = 53,
    ExclamationToken = 54,
    TildeToken = 55,
    AmpersandAmpersandToken = 56,
    BarBarToken = 57,
    QuestionToken = 58,
    ColonToken = 59,
    AtToken = 60,
    QuestionQuestionToken = 61,
    BacktickToken = 62,
    HashToken = 63,
    // Assignments
    EqualsToken = 64,
    PlusEqualsToken = 65,
    MinusEqualsToken = 66,
    AsteriskEqualsToken = 67,
    AsteriskAsteriskEqualsToken = 68,
    SlashEqualsToken = 69,
    PercentEqualsToken = 70,
    LessThanLessThanEqualsToken = 71,
    GreaterThanGreaterThanEqualsToken = 72,
    GreaterThanGreaterThanGreaterThanEqualsToken = 73,
    AmpersandEqualsToken = 74,
    BarEqualsToken = 75,
    BarBarEqualsToken = 76,
    AmpersandAmpersandEqualsToken = 77,
    QuestionQuestionEqualsToken = 78,
    CaretEqualsToken = 79,
    // Identifiers
    Identifier = 80,
    PrivateIdentifier = 81,
    JSDocCommentTextToken = 82,
    // Reserved words (keywords)
    BreakKeyword = 83,
    CaseKeyword = 84,
    CatchKeyword = 85,
    ClassKeyword = 86,
    ConstKeyword = 87,
    ContinueKeyword = 88,
    DebuggerKeyword = 89,
    DefaultKeyword = 90,
    DeleteKeyword = 91,
    DoKeyword = 92,
    ElseKeyword = 93,
    EnumKeyword = 94,
    ExportKeyword = 95,
    ExtendsKeyword = 96,
    FalseKeyword = 97,
    FinallyKeyword = 98,
    ForKeyword = 99,
    FunctionKeyword = 100,
    IfKeyword = 101,
    ImportKeyword = 102,
    InKeyword = 103,
    InstanceOfKeyword = 104,
    NewKeyword = 105,
    NullKeyword = 106,
    ReturnKeyword = 107,
    SuperKeyword = 108,
    SwitchKeyword = 109,
    ThisKeyword = 110,
    ThrowKeyword = 111,
    TrueKeyword = 112,
    TryKeyword = 113,
    TypeOfKeyword = 114,
    VarKeyword = 115,
    VoidKeyword = 116,
    WhileKeyword = 117,
    WithKeyword = 118,
    // Strict mode reserved words
    ImplementsKeyword = 119,
    InterfaceKeyword = 120,
    LetKeyword = 121,
    PackageKeyword = 122,
    PrivateKeyword = 123,
    ProtectedKeyword = 124,
    PublicKeyword = 125,
    StaticKeyword = 126,
    YieldKeyword = 127,
    // Contextual keywords
    AbstractKeyword = 128,
    AccessorKeyword = 129,
    AsKeyword = 130,
    AssertsKeyword = 131,
    AssertKeyword = 132,
    AnyKeyword = 133,
    AsyncKeyword = 134,
    AwaitKeyword = 135,
    BooleanKeyword = 136,
    ConstructorKeyword = 137,
    DeclareKeyword = 138,
    GetKeyword = 139,
    InferKeyword = 140,
    IntrinsicKeyword = 141,
    IsKeyword = 142,
    KeyOfKeyword = 143,
    ModuleKeyword = 144,
    NamespaceKeyword = 145,
    NeverKeyword = 146,
    OutKeyword = 147,
    ReadonlyKeyword = 148,
    RequireKeyword = 149,
    NumberKeyword = 150,
    ObjectKeyword = 151,
    SatisfiesKeyword = 152,
    SetKeyword = 153,
    StringKeyword = 154,
    SymbolKeyword = 155,
    TypeKeyword = 156,
    UndefinedKeyword = 157,
    UniqueKeyword = 158,
    UnknownKeyword = 159,
    UsingKeyword = 160,
    FromKeyword = 161,
    GlobalKeyword = 162,
    BigIntKeyword = 163,
    OverrideKeyword = 164,
    OfKeyword = 165,
    DeferKeyword = 166, // LastKeyword and LastToken
}

// =============================================================================
// SyntaxKind Constants
// =============================================================================

impl SyntaxKind {
    pub const FIRST_TOKEN: Self = Self::Unknown;
    pub const LAST_TOKEN: Self = Self::DeferKeyword;
    pub const FIRST_KEYWORD: Self = Self::BreakKeyword;
    pub const LAST_KEYWORD: Self = Self::DeferKeyword;
    pub const FIRST_PUNCTUATION: Self = Self::OpenBraceToken;
    pub const LAST_PUNCTUATION: Self = Self::CaretEqualsToken;
    pub const FIRST_LITERAL_TOKEN: Self = Self::NumericLiteral;
    pub const LAST_LITERAL_TOKEN: Self = Self::NoSubstitutionTemplateLiteral;
    pub const FIRST_TEMPLATE_TOKEN: Self = Self::NoSubstitutionTemplateLiteral;
    pub const LAST_TEMPLATE_TOKEN: Self = Self::TemplateTail;
    pub const FIRST_RESERVED_WORD: Self = Self::BreakKeyword;
    pub const LAST_RESERVED_WORD: Self = Self::WithKeyword;
    pub const FIRST_FUTURE_RESERVED_WORD: Self = Self::ImplementsKeyword;
    pub const LAST_FUTURE_RESERVED_WORD: Self = Self::YieldKeyword;

    /// Safely convert a u16 to `SyntaxKind` if it's a valid token kind.
    /// Returns None for extended syntax kinds (AST nodes > 166).
    #[must_use]
    pub fn try_from_u16(value: u16) -> Option<Self> {
        // Static assertion: SyntaxKind must be repr(u16) and same size as u16
        const _: () = assert!(
            std::mem::size_of::<SyntaxKind>() == std::mem::size_of::<u16>(),
            "SyntaxKind must be same size as u16 for safe conversion"
        );
        // Valid token range is 0 to LAST_TOKEN (Unknown to DeferKeyword).
        if value <= Self::LAST_TOKEN as u16 {
            KIND_BY_VALUE.get(value as usize).copied()
        } else {
            None
        }
    }
}

const KIND_BY_VALUE: [SyntaxKind; 167] = [
    SyntaxKind::Unknown,
    SyntaxKind::EndOfFileToken,
    SyntaxKind::SingleLineCommentTrivia,
    SyntaxKind::MultiLineCommentTrivia,
    SyntaxKind::NewLineTrivia,
    SyntaxKind::WhitespaceTrivia,
    SyntaxKind::ShebangTrivia,
    SyntaxKind::ConflictMarkerTrivia,
    SyntaxKind::NonTextFileMarkerTrivia,
    SyntaxKind::NumericLiteral,
    SyntaxKind::BigIntLiteral,
    SyntaxKind::StringLiteral,
    SyntaxKind::JsxText,
    SyntaxKind::JsxTextAllWhiteSpaces,
    SyntaxKind::RegularExpressionLiteral,
    SyntaxKind::NoSubstitutionTemplateLiteral,
    SyntaxKind::TemplateHead,
    SyntaxKind::TemplateMiddle,
    SyntaxKind::TemplateTail,
    SyntaxKind::OpenBraceToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::OpenParenToken,
    SyntaxKind::CloseParenToken,
    SyntaxKind::OpenBracketToken,
    SyntaxKind::CloseBracketToken,
    SyntaxKind::DotToken,
    SyntaxKind::DotDotDotToken,
    SyntaxKind::SemicolonToken,
    SyntaxKind::CommaToken,
    SyntaxKind::QuestionDotToken,
    SyntaxKind::LessThanToken,
    SyntaxKind::LessThanSlashToken,
    SyntaxKind::GreaterThanToken,
    SyntaxKind::LessThanEqualsToken,
    SyntaxKind::GreaterThanEqualsToken,
    SyntaxKind::EqualsEqualsToken,
    SyntaxKind::ExclamationEqualsToken,
    SyntaxKind::EqualsEqualsEqualsToken,
    SyntaxKind::ExclamationEqualsEqualsToken,
    SyntaxKind::EqualsGreaterThanToken,
    SyntaxKind::PlusToken,
    SyntaxKind::MinusToken,
    SyntaxKind::AsteriskToken,
    SyntaxKind::AsteriskAsteriskToken,
    SyntaxKind::SlashToken,
    SyntaxKind::PercentToken,
    SyntaxKind::PlusPlusToken,
    SyntaxKind::MinusMinusToken,
    SyntaxKind::LessThanLessThanToken,
    SyntaxKind::GreaterThanGreaterThanToken,
    SyntaxKind::GreaterThanGreaterThanGreaterThanToken,
    SyntaxKind::AmpersandToken,
    SyntaxKind::BarToken,
    SyntaxKind::CaretToken,
    SyntaxKind::ExclamationToken,
    SyntaxKind::TildeToken,
    SyntaxKind::AmpersandAmpersandToken,
    SyntaxKind::BarBarToken,
    SyntaxKind::QuestionToken,
    SyntaxKind::ColonToken,
    SyntaxKind::AtToken,
    SyntaxKind::QuestionQuestionToken,
    SyntaxKind::BacktickToken,
    SyntaxKind::HashToken,
    SyntaxKind::EqualsToken,
    SyntaxKind::PlusEqualsToken,
    SyntaxKind::MinusEqualsToken,
    SyntaxKind::AsteriskEqualsToken,
    SyntaxKind::AsteriskAsteriskEqualsToken,
    SyntaxKind::SlashEqualsToken,
    SyntaxKind::PercentEqualsToken,
    SyntaxKind::LessThanLessThanEqualsToken,
    SyntaxKind::GreaterThanGreaterThanEqualsToken,
    SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken,
    SyntaxKind::AmpersandEqualsToken,
    SyntaxKind::BarEqualsToken,
    SyntaxKind::BarBarEqualsToken,
    SyntaxKind::AmpersandAmpersandEqualsToken,
    SyntaxKind::QuestionQuestionEqualsToken,
    SyntaxKind::CaretEqualsToken,
    SyntaxKind::Identifier,
    SyntaxKind::PrivateIdentifier,
    SyntaxKind::JSDocCommentTextToken,
    SyntaxKind::BreakKeyword,
    SyntaxKind::CaseKeyword,
    SyntaxKind::CatchKeyword,
    SyntaxKind::ClassKeyword,
    SyntaxKind::ConstKeyword,
    SyntaxKind::ContinueKeyword,
    SyntaxKind::DebuggerKeyword,
    SyntaxKind::DefaultKeyword,
    SyntaxKind::DeleteKeyword,
    SyntaxKind::DoKeyword,
    SyntaxKind::ElseKeyword,
    SyntaxKind::EnumKeyword,
    SyntaxKind::ExportKeyword,
    SyntaxKind::ExtendsKeyword,
    SyntaxKind::FalseKeyword,
    SyntaxKind::FinallyKeyword,
    SyntaxKind::ForKeyword,
    SyntaxKind::FunctionKeyword,
    SyntaxKind::IfKeyword,
    SyntaxKind::ImportKeyword,
    SyntaxKind::InKeyword,
    SyntaxKind::InstanceOfKeyword,
    SyntaxKind::NewKeyword,
    SyntaxKind::NullKeyword,
    SyntaxKind::ReturnKeyword,
    SyntaxKind::SuperKeyword,
    SyntaxKind::SwitchKeyword,
    SyntaxKind::ThisKeyword,
    SyntaxKind::ThrowKeyword,
    SyntaxKind::TrueKeyword,
    SyntaxKind::TryKeyword,
    SyntaxKind::TypeOfKeyword,
    SyntaxKind::VarKeyword,
    SyntaxKind::VoidKeyword,
    SyntaxKind::WhileKeyword,
    SyntaxKind::WithKeyword,
    SyntaxKind::ImplementsKeyword,
    SyntaxKind::InterfaceKeyword,
    SyntaxKind::LetKeyword,
    SyntaxKind::PackageKeyword,
    SyntaxKind::PrivateKeyword,
    SyntaxKind::ProtectedKeyword,
    SyntaxKind::PublicKeyword,
    SyntaxKind::StaticKeyword,
    SyntaxKind::YieldKeyword,
    SyntaxKind::AbstractKeyword,
    SyntaxKind::AccessorKeyword,
    SyntaxKind::AsKeyword,
    SyntaxKind::AssertsKeyword,
    SyntaxKind::AssertKeyword,
    SyntaxKind::AnyKeyword,
    SyntaxKind::AsyncKeyword,
    SyntaxKind::AwaitKeyword,
    SyntaxKind::BooleanKeyword,
    SyntaxKind::ConstructorKeyword,
    SyntaxKind::DeclareKeyword,
    SyntaxKind::GetKeyword,
    SyntaxKind::InferKeyword,
    SyntaxKind::IntrinsicKeyword,
    SyntaxKind::IsKeyword,
    SyntaxKind::KeyOfKeyword,
    SyntaxKind::ModuleKeyword,
    SyntaxKind::NamespaceKeyword,
    SyntaxKind::NeverKeyword,
    SyntaxKind::OutKeyword,
    SyntaxKind::ReadonlyKeyword,
    SyntaxKind::RequireKeyword,
    SyntaxKind::NumberKeyword,
    SyntaxKind::ObjectKeyword,
    SyntaxKind::SatisfiesKeyword,
    SyntaxKind::SetKeyword,
    SyntaxKind::StringKeyword,
    SyntaxKind::SymbolKeyword,
    SyntaxKind::TypeKeyword,
    SyntaxKind::UndefinedKeyword,
    SyntaxKind::UniqueKeyword,
    SyntaxKind::UnknownKeyword,
    SyntaxKind::UsingKeyword,
    SyntaxKind::FromKeyword,
    SyntaxKind::GlobalKeyword,
    SyntaxKind::BigIntKeyword,
    SyntaxKind::OverrideKeyword,
    SyntaxKind::OfKeyword,
    SyntaxKind::DeferKeyword,
];

// =============================================================================
// Token Classification Functions
// =============================================================================

/// Check if a token is a keyword.
const fn token_is_keyword_inner(token: SyntaxKind) -> bool {
    let t = token as u16;
    t >= SyntaxKind::BreakKeyword as u16 && t <= SyntaxKind::DeferKeyword as u16
}

#[must_use]
#[cfg(not(target_arch = "wasm32"))]
pub const fn token_is_keyword(token: SyntaxKind) -> bool {
    token_is_keyword_inner(token)
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(js_name = tokenIsKeyword)]
#[must_use]
pub fn token_is_keyword(token: SyntaxKind) -> bool {
    token_is_keyword_inner(token)
}

/// Check if a token is an identifier or keyword.
const fn token_is_identifier_or_keyword_inner(token: SyntaxKind) -> bool {
    token as u16 >= SyntaxKind::Identifier as u16
}

#[must_use]
#[cfg(not(target_arch = "wasm32"))]
pub const fn token_is_identifier_or_keyword(token: SyntaxKind) -> bool {
    token_is_identifier_or_keyword_inner(token)
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(js_name = tokenIsIdentifierOrKeyword)]
#[must_use]
pub fn token_is_identifier_or_keyword(token: SyntaxKind) -> bool {
    token_is_identifier_or_keyword_inner(token)
}

/// Check if a token is a reserved word (strict reserved words).
const fn token_is_reserved_word_inner(token: SyntaxKind) -> bool {
    let t = token as u16;
    t >= SyntaxKind::BreakKeyword as u16 && t <= SyntaxKind::WithKeyword as u16
}

#[must_use]
#[cfg(not(target_arch = "wasm32"))]
pub const fn token_is_reserved_word(token: SyntaxKind) -> bool {
    token_is_reserved_word_inner(token)
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(js_name = tokenIsReservedWord)]
#[must_use]
pub fn token_is_reserved_word(token: SyntaxKind) -> bool {
    token_is_reserved_word_inner(token)
}

/// Check if a token is a strict mode reserved word.
const fn token_is_strict_mode_reserved_word_inner(token: SyntaxKind) -> bool {
    let t = token as u16;
    t >= SyntaxKind::ImplementsKeyword as u16 && t <= SyntaxKind::YieldKeyword as u16
}

#[must_use]
#[cfg(not(target_arch = "wasm32"))]
pub const fn token_is_strict_mode_reserved_word(token: SyntaxKind) -> bool {
    token_is_strict_mode_reserved_word_inner(token)
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(js_name = tokenIsStrictModeReservedWord)]
#[must_use]
pub fn token_is_strict_mode_reserved_word(token: SyntaxKind) -> bool {
    token_is_strict_mode_reserved_word_inner(token)
}

/// Check if a token is a literal (number, string, etc.).
const fn token_is_literal_inner(token: SyntaxKind) -> bool {
    let t = token as u16;
    t >= SyntaxKind::NumericLiteral as u16 && t <= SyntaxKind::NoSubstitutionTemplateLiteral as u16
}

#[must_use]
#[cfg(not(target_arch = "wasm32"))]
pub const fn token_is_literal(token: SyntaxKind) -> bool {
    token_is_literal_inner(token)
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(js_name = tokenIsLiteral)]
#[must_use]
pub fn token_is_literal(token: SyntaxKind) -> bool {
    token_is_literal_inner(token)
}

/// Check if a token is a template literal token.
const fn token_is_template_literal_inner(token: SyntaxKind) -> bool {
    let t = token as u16;
    t >= SyntaxKind::NoSubstitutionTemplateLiteral as u16 && t <= SyntaxKind::TemplateTail as u16
}

#[must_use]
#[cfg(not(target_arch = "wasm32"))]
pub const fn token_is_template_literal(token: SyntaxKind) -> bool {
    token_is_template_literal_inner(token)
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(js_name = tokenIsTemplateLiteral)]
#[must_use]
pub fn token_is_template_literal(token: SyntaxKind) -> bool {
    token_is_template_literal_inner(token)
}

/// Check if a token is punctuation.
const fn token_is_punctuation_inner(token: SyntaxKind) -> bool {
    let t = token as u16;
    t >= SyntaxKind::OpenBraceToken as u16 && t <= SyntaxKind::CaretEqualsToken as u16
}

#[must_use]
#[cfg(not(target_arch = "wasm32"))]
pub const fn token_is_punctuation(token: SyntaxKind) -> bool {
    token_is_punctuation_inner(token)
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(js_name = tokenIsPunctuation)]
#[must_use]
pub fn token_is_punctuation(token: SyntaxKind) -> bool {
    token_is_punctuation_inner(token)
}

/// Check if a token is an assignment operator.
const fn token_is_assignment_operator_inner(token: SyntaxKind) -> bool {
    let t = token as u16;
    t >= SyntaxKind::EqualsToken as u16 && t <= SyntaxKind::CaretEqualsToken as u16
}

#[must_use]
#[cfg(not(target_arch = "wasm32"))]
pub const fn token_is_assignment_operator(token: SyntaxKind) -> bool {
    token_is_assignment_operator_inner(token)
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(js_name = tokenIsAssignmentOperator)]
#[must_use]
pub fn token_is_assignment_operator(token: SyntaxKind) -> bool {
    token_is_assignment_operator_inner(token)
}

/// Check if a token is trivia (whitespace, comments).
const fn token_is_trivia_inner(token: SyntaxKind) -> bool {
    let t = token as u16;
    t >= SyntaxKind::SingleLineCommentTrivia as u16
        && t <= SyntaxKind::NonTextFileMarkerTrivia as u16
}

#[must_use]
#[cfg(not(target_arch = "wasm32"))]
pub const fn token_is_trivia(token: SyntaxKind) -> bool {
    token_is_trivia_inner(token)
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(js_name = tokenIsTrivia)]
#[must_use]
pub fn token_is_trivia(token: SyntaxKind) -> bool {
    token_is_trivia_inner(token)
}

// =============================================================================
// Keyword Text Mapping
// =============================================================================

/// Internal non-allocating version - returns static str reference.
/// Use this for Rust-internal code to avoid allocations.
#[must_use]
pub const fn keyword_to_text_static(token: SyntaxKind) -> Option<&'static str> {
    match token {
        SyntaxKind::BreakKeyword => Some("break"),
        SyntaxKind::CaseKeyword => Some("case"),
        SyntaxKind::CatchKeyword => Some("catch"),
        SyntaxKind::ClassKeyword => Some("class"),
        SyntaxKind::ConstKeyword => Some("const"),
        SyntaxKind::ContinueKeyword => Some("continue"),
        SyntaxKind::DebuggerKeyword => Some("debugger"),
        SyntaxKind::DefaultKeyword => Some("default"),
        SyntaxKind::DeleteKeyword => Some("delete"),
        SyntaxKind::DoKeyword => Some("do"),
        SyntaxKind::ElseKeyword => Some("else"),
        SyntaxKind::EnumKeyword => Some("enum"),
        SyntaxKind::ExportKeyword => Some("export"),
        SyntaxKind::ExtendsKeyword => Some("extends"),
        SyntaxKind::FalseKeyword => Some("false"),
        SyntaxKind::FinallyKeyword => Some("finally"),
        SyntaxKind::ForKeyword => Some("for"),
        SyntaxKind::FunctionKeyword => Some("function"),
        SyntaxKind::IfKeyword => Some("if"),
        SyntaxKind::ImportKeyword => Some("import"),
        SyntaxKind::InKeyword => Some("in"),
        SyntaxKind::InstanceOfKeyword => Some("instanceof"),
        SyntaxKind::NewKeyword => Some("new"),
        SyntaxKind::NullKeyword => Some("null"),
        SyntaxKind::ReturnKeyword => Some("return"),
        SyntaxKind::SuperKeyword => Some("super"),
        SyntaxKind::SwitchKeyword => Some("switch"),
        SyntaxKind::ThisKeyword => Some("this"),
        SyntaxKind::ThrowKeyword => Some("throw"),
        SyntaxKind::TrueKeyword => Some("true"),
        SyntaxKind::TryKeyword => Some("try"),
        SyntaxKind::TypeOfKeyword => Some("typeof"),
        SyntaxKind::VarKeyword => Some("var"),
        SyntaxKind::VoidKeyword => Some("void"),
        SyntaxKind::WhileKeyword => Some("while"),
        SyntaxKind::WithKeyword => Some("with"),
        // Strict mode reserved words
        SyntaxKind::ImplementsKeyword => Some("implements"),
        SyntaxKind::InterfaceKeyword => Some("interface"),
        SyntaxKind::LetKeyword => Some("let"),
        SyntaxKind::PackageKeyword => Some("package"),
        SyntaxKind::PrivateKeyword => Some("private"),
        SyntaxKind::ProtectedKeyword => Some("protected"),
        SyntaxKind::PublicKeyword => Some("public"),
        SyntaxKind::StaticKeyword => Some("static"),
        SyntaxKind::YieldKeyword => Some("yield"),
        // Contextual keywords
        SyntaxKind::AbstractKeyword => Some("abstract"),
        SyntaxKind::AccessorKeyword => Some("accessor"),
        SyntaxKind::AsKeyword => Some("as"),
        SyntaxKind::AssertsKeyword => Some("asserts"),
        SyntaxKind::AssertKeyword => Some("assert"),
        SyntaxKind::AnyKeyword => Some("any"),
        SyntaxKind::AsyncKeyword => Some("async"),
        SyntaxKind::AwaitKeyword => Some("await"),
        SyntaxKind::BooleanKeyword => Some("boolean"),
        SyntaxKind::ConstructorKeyword => Some("constructor"),
        SyntaxKind::DeclareKeyword => Some("declare"),
        SyntaxKind::GetKeyword => Some("get"),
        SyntaxKind::InferKeyword => Some("infer"),
        SyntaxKind::IntrinsicKeyword => Some("intrinsic"),
        SyntaxKind::IsKeyword => Some("is"),
        SyntaxKind::KeyOfKeyword => Some("keyof"),
        SyntaxKind::ModuleKeyword => Some("module"),
        SyntaxKind::NamespaceKeyword => Some("namespace"),
        SyntaxKind::NeverKeyword => Some("never"),
        SyntaxKind::OutKeyword => Some("out"),
        SyntaxKind::ReadonlyKeyword => Some("readonly"),
        SyntaxKind::RequireKeyword => Some("require"),
        SyntaxKind::NumberKeyword => Some("number"),
        SyntaxKind::ObjectKeyword => Some("object"),
        SyntaxKind::SatisfiesKeyword => Some("satisfies"),
        SyntaxKind::SetKeyword => Some("set"),
        SyntaxKind::StringKeyword => Some("string"),
        SyntaxKind::SymbolKeyword => Some("symbol"),
        SyntaxKind::TypeKeyword => Some("type"),
        SyntaxKind::UndefinedKeyword => Some("undefined"),
        SyntaxKind::UniqueKeyword => Some("unique"),
        SyntaxKind::UnknownKeyword => Some("unknown"),
        SyntaxKind::UsingKeyword => Some("using"),
        SyntaxKind::FromKeyword => Some("from"),
        SyntaxKind::GlobalKeyword => Some("global"),
        SyntaxKind::BigIntKeyword => Some("bigint"),
        SyntaxKind::OverrideKeyword => Some("override"),
        SyntaxKind::OfKeyword => Some("of"),
        SyntaxKind::DeferKeyword => Some("defer"),
        _ => None,
    }
}

/// Get the text representation of a keyword token.
/// WASM-exported version that allocates a String for JS compatibility.
#[wasm_bindgen(js_name = keywordToText)]
#[must_use]
pub fn keyword_to_text(token: SyntaxKind) -> Option<String> {
    keyword_to_text_static(token).map(std::convert::Into::into)
}

/// Byte length of a keyword's source text, as a `u32` for `parse_error_at`
/// span arguments. Returns `0` for non-keyword tokens.
///
/// All keyword texts in `keyword_to_text_static` are pure ASCII, so byte length
/// equals character length. Using this avoids hardcoding `6 // length of "export"`
/// at every parser-recovery diagnostic site.
#[must_use]
pub const fn keyword_text_len(token: SyntaxKind) -> u32 {
    match keyword_to_text_static(token) {
        Some(text) => text.len() as u32,
        None => 0,
    }
}

/// Internal non-allocating version - returns static str reference.
/// Use this for Rust-internal code to avoid allocations.
#[must_use]
pub const fn punctuation_to_text_static(token: SyntaxKind) -> Option<&'static str> {
    match token {
        SyntaxKind::OpenBraceToken => Some("{"),
        SyntaxKind::CloseBraceToken => Some("}"),
        SyntaxKind::OpenParenToken => Some("("),
        SyntaxKind::CloseParenToken => Some(")"),
        SyntaxKind::OpenBracketToken => Some("["),
        SyntaxKind::CloseBracketToken => Some("]"),
        SyntaxKind::DotToken => Some("."),
        SyntaxKind::DotDotDotToken => Some("..."),
        SyntaxKind::SemicolonToken => Some(";"),
        SyntaxKind::CommaToken => Some(","),
        SyntaxKind::QuestionDotToken => Some("?."),
        SyntaxKind::LessThanToken => Some("<"),
        SyntaxKind::LessThanSlashToken => Some("</"),
        SyntaxKind::GreaterThanToken => Some(">"),
        SyntaxKind::LessThanEqualsToken => Some("<="),
        SyntaxKind::GreaterThanEqualsToken => Some(">="),
        SyntaxKind::EqualsEqualsToken => Some("=="),
        SyntaxKind::ExclamationEqualsToken => Some("!="),
        SyntaxKind::EqualsEqualsEqualsToken => Some("==="),
        SyntaxKind::ExclamationEqualsEqualsToken => Some("!=="),
        SyntaxKind::EqualsGreaterThanToken => Some("=>"),
        SyntaxKind::PlusToken => Some("+"),
        SyntaxKind::MinusToken => Some("-"),
        SyntaxKind::AsteriskToken => Some("*"),
        SyntaxKind::AsteriskAsteriskToken => Some("**"),
        SyntaxKind::SlashToken => Some("/"),
        SyntaxKind::PercentToken => Some("%"),
        SyntaxKind::PlusPlusToken => Some("++"),
        SyntaxKind::MinusMinusToken => Some("--"),
        SyntaxKind::LessThanLessThanToken => Some("<<"),
        SyntaxKind::GreaterThanGreaterThanToken => Some(">>"),
        SyntaxKind::GreaterThanGreaterThanGreaterThanToken => Some(">>>"),
        SyntaxKind::AmpersandToken => Some("&"),
        SyntaxKind::BarToken => Some("|"),
        SyntaxKind::CaretToken => Some("^"),
        SyntaxKind::ExclamationToken => Some("!"),
        SyntaxKind::TildeToken => Some("~"),
        SyntaxKind::AmpersandAmpersandToken => Some("&&"),
        SyntaxKind::BarBarToken => Some("||"),
        SyntaxKind::QuestionToken => Some("?"),
        SyntaxKind::ColonToken => Some(":"),
        SyntaxKind::AtToken => Some("@"),
        SyntaxKind::QuestionQuestionToken => Some("??"),
        SyntaxKind::BacktickToken => Some("`"),
        SyntaxKind::HashToken => Some("#"),
        // Assignment operators
        SyntaxKind::EqualsToken => Some("="),
        SyntaxKind::PlusEqualsToken => Some("+="),
        SyntaxKind::MinusEqualsToken => Some("-="),
        SyntaxKind::AsteriskEqualsToken => Some("*="),
        SyntaxKind::AsteriskAsteriskEqualsToken => Some("**="),
        SyntaxKind::SlashEqualsToken => Some("/="),
        SyntaxKind::PercentEqualsToken => Some("%="),
        SyntaxKind::LessThanLessThanEqualsToken => Some("<<="),
        SyntaxKind::GreaterThanGreaterThanEqualsToken => Some(">>="),
        SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken => Some(">>>="),
        SyntaxKind::AmpersandEqualsToken => Some("&="),
        SyntaxKind::BarEqualsToken => Some("|="),
        SyntaxKind::BarBarEqualsToken => Some("||="),
        SyntaxKind::AmpersandAmpersandEqualsToken => Some("&&="),
        SyntaxKind::QuestionQuestionEqualsToken => Some("??="),
        SyntaxKind::CaretEqualsToken => Some("^="),
        _ => None,
    }
}

/// Get the text representation of a punctuation token.
/// WASM-exported version that allocates a String for JS compatibility.
#[wasm_bindgen(js_name = punctuationToText)]
#[must_use]
pub fn punctuation_to_text(token: SyntaxKind) -> Option<String> {
    punctuation_to_text_static(token).map(std::convert::Into::into)
}

// =============================================================================
// Text to Keyword Lookup
// =============================================================================

/// Convert a string to its keyword `SyntaxKind`, if it's a keyword.
/// Returns None if the text is not a keyword.
#[wasm_bindgen(js_name = textToKeyword)]
#[must_use]
pub fn text_to_keyword(text: &str) -> Option<SyntaxKind> {
    match text {
        // Reserved words
        "break" => Some(SyntaxKind::BreakKeyword),
        "case" => Some(SyntaxKind::CaseKeyword),
        "catch" => Some(SyntaxKind::CatchKeyword),
        "class" => Some(SyntaxKind::ClassKeyword),
        "const" => Some(SyntaxKind::ConstKeyword),
        "continue" => Some(SyntaxKind::ContinueKeyword),
        "debugger" => Some(SyntaxKind::DebuggerKeyword),
        "default" => Some(SyntaxKind::DefaultKeyword),
        "delete" => Some(SyntaxKind::DeleteKeyword),
        "do" => Some(SyntaxKind::DoKeyword),
        "else" => Some(SyntaxKind::ElseKeyword),
        "enum" => Some(SyntaxKind::EnumKeyword),
        "export" => Some(SyntaxKind::ExportKeyword),
        "extends" => Some(SyntaxKind::ExtendsKeyword),
        "false" => Some(SyntaxKind::FalseKeyword),
        "finally" => Some(SyntaxKind::FinallyKeyword),
        "for" => Some(SyntaxKind::ForKeyword),
        "function" => Some(SyntaxKind::FunctionKeyword),
        "if" => Some(SyntaxKind::IfKeyword),
        "import" => Some(SyntaxKind::ImportKeyword),
        "in" => Some(SyntaxKind::InKeyword),
        "instanceof" => Some(SyntaxKind::InstanceOfKeyword),
        "new" => Some(SyntaxKind::NewKeyword),
        "null" => Some(SyntaxKind::NullKeyword),
        "return" => Some(SyntaxKind::ReturnKeyword),
        "super" => Some(SyntaxKind::SuperKeyword),
        "switch" => Some(SyntaxKind::SwitchKeyword),
        "this" => Some(SyntaxKind::ThisKeyword),
        "throw" => Some(SyntaxKind::ThrowKeyword),
        "true" => Some(SyntaxKind::TrueKeyword),
        "try" => Some(SyntaxKind::TryKeyword),
        "typeof" => Some(SyntaxKind::TypeOfKeyword),
        "var" => Some(SyntaxKind::VarKeyword),
        "void" => Some(SyntaxKind::VoidKeyword),
        "while" => Some(SyntaxKind::WhileKeyword),
        "with" => Some(SyntaxKind::WithKeyword),
        // Strict mode reserved words
        "implements" => Some(SyntaxKind::ImplementsKeyword),
        "interface" => Some(SyntaxKind::InterfaceKeyword),
        "let" => Some(SyntaxKind::LetKeyword),
        "package" => Some(SyntaxKind::PackageKeyword),
        "private" => Some(SyntaxKind::PrivateKeyword),
        "protected" => Some(SyntaxKind::ProtectedKeyword),
        "public" => Some(SyntaxKind::PublicKeyword),
        "static" => Some(SyntaxKind::StaticKeyword),
        "yield" => Some(SyntaxKind::YieldKeyword),
        // Contextual keywords
        "abstract" => Some(SyntaxKind::AbstractKeyword),
        "accessor" => Some(SyntaxKind::AccessorKeyword),
        "as" => Some(SyntaxKind::AsKeyword),
        "asserts" => Some(SyntaxKind::AssertsKeyword),
        "assert" => Some(SyntaxKind::AssertKeyword),
        "any" => Some(SyntaxKind::AnyKeyword),
        "async" => Some(SyntaxKind::AsyncKeyword),
        "await" => Some(SyntaxKind::AwaitKeyword),
        "boolean" => Some(SyntaxKind::BooleanKeyword),
        "constructor" => Some(SyntaxKind::ConstructorKeyword),
        "declare" => Some(SyntaxKind::DeclareKeyword),
        "get" => Some(SyntaxKind::GetKeyword),
        "infer" => Some(SyntaxKind::InferKeyword),
        "intrinsic" => Some(SyntaxKind::IntrinsicKeyword),
        "is" => Some(SyntaxKind::IsKeyword),
        "keyof" => Some(SyntaxKind::KeyOfKeyword),
        "module" => Some(SyntaxKind::ModuleKeyword),
        "namespace" => Some(SyntaxKind::NamespaceKeyword),
        "never" => Some(SyntaxKind::NeverKeyword),
        "out" => Some(SyntaxKind::OutKeyword),
        "readonly" => Some(SyntaxKind::ReadonlyKeyword),
        "require" => Some(SyntaxKind::RequireKeyword),
        "number" => Some(SyntaxKind::NumberKeyword),
        "object" => Some(SyntaxKind::ObjectKeyword),
        "satisfies" => Some(SyntaxKind::SatisfiesKeyword),
        "set" => Some(SyntaxKind::SetKeyword),
        "string" => Some(SyntaxKind::StringKeyword),
        "symbol" => Some(SyntaxKind::SymbolKeyword),
        "type" => Some(SyntaxKind::TypeKeyword),
        "undefined" => Some(SyntaxKind::UndefinedKeyword),
        "unique" => Some(SyntaxKind::UniqueKeyword),
        "unknown" => Some(SyntaxKind::UnknownKeyword),
        "using" => Some(SyntaxKind::UsingKeyword),
        "from" => Some(SyntaxKind::FromKeyword),
        "global" => Some(SyntaxKind::GlobalKeyword),
        "bigint" => Some(SyntaxKind::BigIntKeyword),
        "override" => Some(SyntaxKind::OverrideKeyword),
        "of" => Some(SyntaxKind::OfKeyword),
        "defer" => Some(SyntaxKind::DeferKeyword),
        _ => None,
    }
}

/// Get the token kind for a given text, including identifiers and keywords.
/// Returns `Identifier` if the text is not a keyword.
#[wasm_bindgen(js_name = stringToToken)]
#[must_use]
pub fn string_to_token(text: &str) -> SyntaxKind {
    text_to_keyword(text).unwrap_or(SyntaxKind::Identifier)
}

// =============================================================================
// Unit Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ── SyntaxKind::try_from_u16 ──────────────────────────────────────

    #[test]
    fn try_from_u16_valid_range() {
        assert_eq!(SyntaxKind::try_from_u16(0), Some(SyntaxKind::Unknown));
        assert_eq!(
            SyntaxKind::try_from_u16(1),
            Some(SyntaxKind::EndOfFileToken)
        );
        assert_eq!(
            SyntaxKind::try_from_u16(9),
            Some(SyntaxKind::NumericLiteral)
        );
        assert_eq!(SyntaxKind::try_from_u16(80), Some(SyntaxKind::Identifier));
        assert_eq!(
            SyntaxKind::try_from_u16(166),
            Some(SyntaxKind::DeferKeyword)
        );
    }

    #[test]
    fn try_from_u16_out_of_range() {
        assert_eq!(SyntaxKind::try_from_u16(167), None);
        assert_eq!(SyntaxKind::try_from_u16(200), None);
        assert_eq!(SyntaxKind::try_from_u16(u16::MAX), None);
    }

    // ── token_is_keyword ──────────────────────────────────────────────

    #[test]
    fn keyword_classification() {
        assert!(token_is_keyword(SyntaxKind::BreakKeyword));
        assert!(token_is_keyword(SyntaxKind::IfKeyword));
        assert!(token_is_keyword(SyntaxKind::ClassKeyword));
        assert!(token_is_keyword(SyntaxKind::DeferKeyword)); // last keyword
        assert!(token_is_keyword(SyntaxKind::AsyncKeyword));
        assert!(token_is_keyword(SyntaxKind::LetKeyword));
        assert!(token_is_keyword(SyntaxKind::YieldKeyword));

        assert!(!token_is_keyword(SyntaxKind::Identifier));
        assert!(!token_is_keyword(SyntaxKind::NumericLiteral));
        assert!(!token_is_keyword(SyntaxKind::OpenBraceToken));
        assert!(!token_is_keyword(SyntaxKind::EndOfFileToken));
    }

    // ── token_is_reserved_word ────────────────────────────────────────

    #[test]
    fn reserved_word_classification() {
        // Reserved words: break..with
        assert!(token_is_reserved_word(SyntaxKind::BreakKeyword));
        assert!(token_is_reserved_word(SyntaxKind::WithKeyword));
        assert!(token_is_reserved_word(SyntaxKind::ReturnKeyword));
        assert!(token_is_reserved_word(SyntaxKind::ClassKeyword));

        // Strict mode reserved words are NOT reserved words
        assert!(!token_is_reserved_word(SyntaxKind::ImplementsKeyword));
        assert!(!token_is_reserved_word(SyntaxKind::YieldKeyword));
        // Contextual keywords are NOT reserved words
        assert!(!token_is_reserved_word(SyntaxKind::AsyncKeyword));
        assert!(!token_is_reserved_word(SyntaxKind::TypeKeyword));
        assert!(!token_is_reserved_word(SyntaxKind::Identifier));
    }

    // ── token_is_strict_mode_reserved_word ────────────────────────────

    #[test]
    fn strict_mode_reserved_word_classification() {
        assert!(token_is_strict_mode_reserved_word(
            SyntaxKind::ImplementsKeyword
        ));
        assert!(token_is_strict_mode_reserved_word(
            SyntaxKind::InterfaceKeyword
        ));
        assert!(token_is_strict_mode_reserved_word(SyntaxKind::LetKeyword));
        assert!(token_is_strict_mode_reserved_word(SyntaxKind::YieldKeyword));

        assert!(!token_is_strict_mode_reserved_word(
            SyntaxKind::BreakKeyword
        ));
        assert!(!token_is_strict_mode_reserved_word(
            SyntaxKind::AsyncKeyword
        ));
        assert!(!token_is_strict_mode_reserved_word(SyntaxKind::Identifier));
    }

    // ── token_is_literal ──────────────────────────────────────────────

    #[test]
    fn literal_classification() {
        assert!(token_is_literal(SyntaxKind::NumericLiteral));
        assert!(token_is_literal(SyntaxKind::BigIntLiteral));
        assert!(token_is_literal(SyntaxKind::StringLiteral));
        assert!(token_is_literal(SyntaxKind::RegularExpressionLiteral));
        assert!(token_is_literal(SyntaxKind::NoSubstitutionTemplateLiteral));

        assert!(!token_is_literal(SyntaxKind::TemplateHead));
        assert!(!token_is_literal(SyntaxKind::Identifier));
        assert!(!token_is_literal(SyntaxKind::BreakKeyword));
    }

    // ── token_is_template_literal ─────────────────────────────────────

    #[test]
    fn template_literal_classification() {
        assert!(token_is_template_literal(
            SyntaxKind::NoSubstitutionTemplateLiteral
        ));
        assert!(token_is_template_literal(SyntaxKind::TemplateHead));
        assert!(token_is_template_literal(SyntaxKind::TemplateMiddle));
        assert!(token_is_template_literal(SyntaxKind::TemplateTail));

        assert!(!token_is_template_literal(SyntaxKind::StringLiteral));
        assert!(!token_is_template_literal(SyntaxKind::NumericLiteral));
    }

    // ── token_is_punctuation ──────────────────────────────────────────

    #[test]
    fn punctuation_classification() {
        assert!(token_is_punctuation(SyntaxKind::OpenBraceToken));
        assert!(token_is_punctuation(SyntaxKind::SemicolonToken));
        assert!(token_is_punctuation(SyntaxKind::PlusToken));
        assert!(token_is_punctuation(SyntaxKind::EqualsToken));
        assert!(token_is_punctuation(SyntaxKind::CaretEqualsToken)); // last punctuation

        assert!(!token_is_punctuation(SyntaxKind::Identifier));
        assert!(!token_is_punctuation(SyntaxKind::NumericLiteral));
        assert!(!token_is_punctuation(SyntaxKind::BreakKeyword));
    }

    // ── token_is_assignment_operator ──────────────────────────────────

    #[test]
    fn assignment_operator_classification() {
        assert!(token_is_assignment_operator(SyntaxKind::EqualsToken));
        assert!(token_is_assignment_operator(SyntaxKind::PlusEqualsToken));
        assert!(token_is_assignment_operator(
            SyntaxKind::AsteriskAsteriskEqualsToken
        ));
        assert!(token_is_assignment_operator(SyntaxKind::BarBarEqualsToken));
        assert!(token_is_assignment_operator(
            SyntaxKind::QuestionQuestionEqualsToken
        ));
        assert!(token_is_assignment_operator(SyntaxKind::CaretEqualsToken));

        assert!(!token_is_assignment_operator(SyntaxKind::PlusToken));
        assert!(!token_is_assignment_operator(SyntaxKind::EqualsEqualsToken));
        assert!(!token_is_assignment_operator(SyntaxKind::Identifier));
    }

    // ── token_is_trivia ──────────────────────────────────────────────

    #[test]
    fn trivia_classification() {
        assert!(token_is_trivia(SyntaxKind::SingleLineCommentTrivia));
        assert!(token_is_trivia(SyntaxKind::MultiLineCommentTrivia));
        assert!(token_is_trivia(SyntaxKind::NewLineTrivia));
        assert!(token_is_trivia(SyntaxKind::WhitespaceTrivia));
        assert!(token_is_trivia(SyntaxKind::ShebangTrivia));
        assert!(token_is_trivia(SyntaxKind::ConflictMarkerTrivia));
        assert!(token_is_trivia(SyntaxKind::NonTextFileMarkerTrivia));

        assert!(!token_is_trivia(SyntaxKind::Unknown));
        assert!(!token_is_trivia(SyntaxKind::EndOfFileToken));
        assert!(!token_is_trivia(SyntaxKind::NumericLiteral));
    }

    // ── token_is_identifier_or_keyword ────────────────────────────────

    #[test]
    fn identifier_or_keyword_classification() {
        assert!(token_is_identifier_or_keyword(SyntaxKind::Identifier));
        assert!(token_is_identifier_or_keyword(SyntaxKind::BreakKeyword));
        assert!(token_is_identifier_or_keyword(SyntaxKind::AsyncKeyword));
        assert!(token_is_identifier_or_keyword(SyntaxKind::DeferKeyword));
        // PrivateIdentifier is between Identifier and keywords
        assert!(token_is_identifier_or_keyword(
            SyntaxKind::PrivateIdentifier
        ));

        assert!(!token_is_identifier_or_keyword(SyntaxKind::NumericLiteral));
        assert!(!token_is_identifier_or_keyword(SyntaxKind::OpenBraceToken));
        assert!(!token_is_identifier_or_keyword(SyntaxKind::EndOfFileToken));
    }

    // ── text_to_keyword / keyword_to_text roundtrip ───────────────────

    #[test]
    fn text_to_keyword_all_reserved_words() {
        let cases = [
            ("break", SyntaxKind::BreakKeyword),
            ("case", SyntaxKind::CaseKeyword),
            ("class", SyntaxKind::ClassKeyword),
            ("const", SyntaxKind::ConstKeyword),
            ("function", SyntaxKind::FunctionKeyword),
            ("if", SyntaxKind::IfKeyword),
            ("return", SyntaxKind::ReturnKeyword),
            ("this", SyntaxKind::ThisKeyword),
            ("typeof", SyntaxKind::TypeOfKeyword),
            ("var", SyntaxKind::VarKeyword),
            ("void", SyntaxKind::VoidKeyword),
            ("while", SyntaxKind::WhileKeyword),
            ("with", SyntaxKind::WithKeyword),
        ];
        for (text, expected) in cases {
            assert_eq!(
                text_to_keyword(text),
                Some(expected),
                "text_to_keyword({text:?})"
            );
        }
    }

    #[test]
    fn text_to_keyword_contextual_keywords() {
        let cases = [
            ("async", SyntaxKind::AsyncKeyword),
            ("await", SyntaxKind::AwaitKeyword),
            ("type", SyntaxKind::TypeKeyword),
            ("declare", SyntaxKind::DeclareKeyword),
            ("abstract", SyntaxKind::AbstractKeyword),
            ("as", SyntaxKind::AsKeyword),
            ("satisfies", SyntaxKind::SatisfiesKeyword),
            ("keyof", SyntaxKind::KeyOfKeyword),
            ("infer", SyntaxKind::InferKeyword),
            ("readonly", SyntaxKind::ReadonlyKeyword),
            ("override", SyntaxKind::OverrideKeyword),
            ("defer", SyntaxKind::DeferKeyword),
        ];
        for (text, expected) in cases {
            assert_eq!(
                text_to_keyword(text),
                Some(expected),
                "text_to_keyword({text:?})"
            );
        }
    }

    #[test]
    fn text_to_keyword_non_keywords() {
        assert_eq!(text_to_keyword("foo"), None);
        assert_eq!(text_to_keyword("bar"), None);
        assert_eq!(text_to_keyword(""), None);
        assert_eq!(text_to_keyword("IF"), None); // case sensitive
        assert_eq!(text_to_keyword("Class"), None); // case sensitive
    }

    #[test]
    fn keyword_to_text_roundtrip() {
        // Every keyword should roundtrip: text_to_keyword(keyword_to_text(k)) == Some(k)
        let keywords = [
            SyntaxKind::BreakKeyword,
            SyntaxKind::CaseKeyword,
            SyntaxKind::CatchKeyword,
            SyntaxKind::ClassKeyword,
            SyntaxKind::ConstKeyword,
            SyntaxKind::IfKeyword,
            SyntaxKind::ReturnKeyword,
            SyntaxKind::AsyncKeyword,
            SyntaxKind::AwaitKeyword,
            SyntaxKind::TypeKeyword,
            SyntaxKind::LetKeyword,
            SyntaxKind::YieldKeyword,
            SyntaxKind::DeferKeyword,
            SyntaxKind::SatisfiesKeyword,
        ];
        for kw in keywords {
            let text = keyword_to_text_static(kw).expect("keyword should have text");
            let roundtripped = text_to_keyword(text);
            assert_eq!(
                roundtripped,
                Some(kw),
                "roundtrip failed for {kw:?} -> {text:?}"
            );
        }
    }

    #[test]
    fn keyword_to_text_non_keywords() {
        assert_eq!(keyword_to_text_static(SyntaxKind::Identifier), None);
        assert_eq!(keyword_to_text_static(SyntaxKind::NumericLiteral), None);
        assert_eq!(keyword_to_text_static(SyntaxKind::OpenBraceToken), None);
    }

    // ── punctuation_to_text_static ────────────────────────────────────

    #[test]
    fn punctuation_to_text_basics() {
        assert_eq!(
            punctuation_to_text_static(SyntaxKind::OpenBraceToken),
            Some("{")
        );
        assert_eq!(
            punctuation_to_text_static(SyntaxKind::CloseBraceToken),
            Some("}")
        );
        assert_eq!(
            punctuation_to_text_static(SyntaxKind::SemicolonToken),
            Some(";")
        );
        assert_eq!(
            punctuation_to_text_static(SyntaxKind::DotDotDotToken),
            Some("...")
        );
        assert_eq!(
            punctuation_to_text_static(SyntaxKind::EqualsGreaterThanToken),
            Some("=>")
        );
        assert_eq!(
            punctuation_to_text_static(SyntaxKind::QuestionQuestionToken),
            Some("??")
        );
        assert_eq!(
            punctuation_to_text_static(SyntaxKind::AsteriskAsteriskToken),
            Some("**")
        );
        assert_eq!(
            punctuation_to_text_static(SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken),
            Some(">>>=")
        );
    }

    #[test]
    fn punctuation_to_text_non_punctuation() {
        assert_eq!(punctuation_to_text_static(SyntaxKind::Identifier), None);
        assert_eq!(punctuation_to_text_static(SyntaxKind::BreakKeyword), None);
    }

    // ── string_to_token ───────────────────────────────────────────────

    #[test]
    fn string_to_token_keywords_and_identifiers() {
        assert_eq!(string_to_token("if"), SyntaxKind::IfKeyword);
        assert_eq!(string_to_token("class"), SyntaxKind::ClassKeyword);
        assert_eq!(string_to_token("async"), SyntaxKind::AsyncKeyword);
        assert_eq!(string_to_token("myVariable"), SyntaxKind::Identifier);
        assert_eq!(string_to_token("_foo"), SyntaxKind::Identifier);
        assert_eq!(string_to_token("$bar"), SyntaxKind::Identifier);
    }

    // ── SyntaxKind constants ──────────────────────────────────────────

    #[test]
    fn syntax_kind_constants_are_consistent() {
        assert!(SyntaxKind::FIRST_KEYWORD as u16 <= SyntaxKind::LAST_KEYWORD as u16);
        assert!(SyntaxKind::FIRST_PUNCTUATION as u16 <= SyntaxKind::LAST_PUNCTUATION as u16);
        assert!(SyntaxKind::FIRST_LITERAL_TOKEN as u16 <= SyntaxKind::LAST_LITERAL_TOKEN as u16);
        assert!(SyntaxKind::FIRST_TEMPLATE_TOKEN as u16 <= SyntaxKind::LAST_TEMPLATE_TOKEN as u16);
        assert!(SyntaxKind::FIRST_RESERVED_WORD as u16 <= SyntaxKind::LAST_RESERVED_WORD as u16);

        // Verify boundary relationships
        assert_eq!(SyntaxKind::FIRST_TOKEN, SyntaxKind::Unknown);
        assert_eq!(SyntaxKind::LAST_TOKEN, SyntaxKind::DeferKeyword);
        assert_eq!(SyntaxKind::FIRST_KEYWORD, SyntaxKind::BreakKeyword);
        assert_eq!(SyntaxKind::LAST_KEYWORD, SyntaxKind::DeferKeyword);
    }

    // ── KIND_BY_VALUE table consistency ───────────────────────────────

    #[test]
    fn kind_by_value_table_is_identity() {
        // Every entry in KIND_BY_VALUE should match its index
        for (i, &kind) in KIND_BY_VALUE.iter().enumerate() {
            assert_eq!(
                kind as u16, i as u16,
                "KIND_BY_VALUE[{i}] = {:?} (value {}), expected value {i}",
                kind, kind as u16
            );
        }
    }
}
