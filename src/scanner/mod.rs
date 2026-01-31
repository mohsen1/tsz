//! Scanner types and utilities for TypeScript lexical analysis.
//!
//! This module contains the SyntaxKind enum, scanner implementation,
//! and character code constants for TypeScript lexical analysis.

// Scanner implementation - tokenization logic
pub mod scanner_impl;

// Character code constants used by the scanner
pub mod char_codes;

use serde::Serialize;
use wasm_bindgen::prelude::*;

// =============================================================================
// SyntaxKind Enum - Token Types (Scanner Output)
// =============================================================================

/// Syntax kind enum matching TypeScript's SyntaxKind.
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
    pub const FIRST_TOKEN: SyntaxKind = SyntaxKind::Unknown;
    pub const LAST_TOKEN: SyntaxKind = SyntaxKind::DeferKeyword;
    pub const FIRST_KEYWORD: SyntaxKind = SyntaxKind::BreakKeyword;
    pub const LAST_KEYWORD: SyntaxKind = SyntaxKind::DeferKeyword;
    pub const FIRST_PUNCTUATION: SyntaxKind = SyntaxKind::OpenBraceToken;
    pub const LAST_PUNCTUATION: SyntaxKind = SyntaxKind::CaretEqualsToken;
    pub const FIRST_LITERAL_TOKEN: SyntaxKind = SyntaxKind::NumericLiteral;
    pub const LAST_LITERAL_TOKEN: SyntaxKind = SyntaxKind::NoSubstitutionTemplateLiteral;
    pub const FIRST_TEMPLATE_TOKEN: SyntaxKind = SyntaxKind::NoSubstitutionTemplateLiteral;
    pub const LAST_TEMPLATE_TOKEN: SyntaxKind = SyntaxKind::TemplateTail;
    pub const FIRST_RESERVED_WORD: SyntaxKind = SyntaxKind::BreakKeyword;
    pub const LAST_RESERVED_WORD: SyntaxKind = SyntaxKind::WithKeyword;
    pub const FIRST_FUTURE_RESERVED_WORD: SyntaxKind = SyntaxKind::ImplementsKeyword;
    pub const LAST_FUTURE_RESERVED_WORD: SyntaxKind = SyntaxKind::YieldKeyword;

    /// Safely convert a u16 to SyntaxKind if it's a valid token kind.
    /// Returns None for extended syntax kinds (AST nodes > 166).
    pub fn try_from_u16(value: u16) -> Option<SyntaxKind> {
        // Static assertion: SyntaxKind must be repr(u16) and same size as u16
        const _: () = assert!(
            std::mem::size_of::<SyntaxKind>() == std::mem::size_of::<u16>(),
            "SyntaxKind must be same size as u16 for transmute safety"
        );

        // Valid token range is 0 to LAST_TOKEN (Unknown to DeferKeyword)
        if value <= Self::LAST_TOKEN as u16 {
            // SAFETY: We've verified that:
            // 1. SyntaxKind is #[repr(u16)] (checked at compile time above)
            // 2. The value is in the valid enum range (0..=LAST_TOKEN)
            // 3. SyntaxKind has contiguous values starting from 0
            #[allow(unsafe_code)]
            Some(unsafe { std::mem::transmute::<u16, SyntaxKind>(value) })
        } else {
            None
        }
    }
}

// =============================================================================
// Token Classification Functions
// =============================================================================

/// Check if a token is a keyword.
#[wasm_bindgen(js_name = tokenIsKeyword)]
pub fn token_is_keyword(token: SyntaxKind) -> bool {
    let t = token as u16;
    t >= SyntaxKind::BreakKeyword as u16 && t <= SyntaxKind::DeferKeyword as u16
}

/// Check if a token is an identifier or keyword.
#[wasm_bindgen(js_name = tokenIsIdentifierOrKeyword)]
pub fn token_is_identifier_or_keyword(token: SyntaxKind) -> bool {
    token as u16 >= SyntaxKind::Identifier as u16
}

/// Check if a token is a reserved word (strict reserved words).
#[wasm_bindgen(js_name = tokenIsReservedWord)]
pub fn token_is_reserved_word(token: SyntaxKind) -> bool {
    let t = token as u16;
    t >= SyntaxKind::BreakKeyword as u16 && t <= SyntaxKind::WithKeyword as u16
}

/// Check if a token is a strict mode reserved word.
#[wasm_bindgen(js_name = tokenIsStrictModeReservedWord)]
pub fn token_is_strict_mode_reserved_word(token: SyntaxKind) -> bool {
    let t = token as u16;
    t >= SyntaxKind::ImplementsKeyword as u16 && t <= SyntaxKind::YieldKeyword as u16
}

/// Check if a token is a literal (number, string, etc.).
#[wasm_bindgen(js_name = tokenIsLiteral)]
pub fn token_is_literal(token: SyntaxKind) -> bool {
    let t = token as u16;
    t >= SyntaxKind::NumericLiteral as u16 && t <= SyntaxKind::NoSubstitutionTemplateLiteral as u16
}

/// Check if a token is a template literal token.
#[wasm_bindgen(js_name = tokenIsTemplateLiteral)]
pub fn token_is_template_literal(token: SyntaxKind) -> bool {
    let t = token as u16;
    t >= SyntaxKind::NoSubstitutionTemplateLiteral as u16 && t <= SyntaxKind::TemplateTail as u16
}

/// Check if a token is punctuation.
#[wasm_bindgen(js_name = tokenIsPunctuation)]
pub fn token_is_punctuation(token: SyntaxKind) -> bool {
    let t = token as u16;
    t >= SyntaxKind::OpenBraceToken as u16 && t <= SyntaxKind::CaretEqualsToken as u16
}

/// Check if a token is an assignment operator.
#[wasm_bindgen(js_name = tokenIsAssignmentOperator)]
pub fn token_is_assignment_operator(token: SyntaxKind) -> bool {
    let t = token as u16;
    t >= SyntaxKind::EqualsToken as u16 && t <= SyntaxKind::CaretEqualsToken as u16
}

/// Check if a token is trivia (whitespace, comments).
#[wasm_bindgen(js_name = tokenIsTrivia)]
pub fn token_is_trivia(token: SyntaxKind) -> bool {
    let t = token as u16;
    t >= SyntaxKind::SingleLineCommentTrivia as u16
        && t <= SyntaxKind::NonTextFileMarkerTrivia as u16
}

// =============================================================================
// Keyword Text Mapping
// =============================================================================

/// Internal non-allocating version - returns static str reference.
/// Use this for Rust-internal code to avoid allocations.
pub fn keyword_to_text_static(token: SyntaxKind) -> Option<&'static str> {
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
pub fn keyword_to_text(token: SyntaxKind) -> Option<String> {
    keyword_to_text_static(token).map(|s| s.into())
}

/// Internal non-allocating version - returns static str reference.
/// Use this for Rust-internal code to avoid allocations.
pub fn punctuation_to_text_static(token: SyntaxKind) -> Option<&'static str> {
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
pub fn punctuation_to_text(token: SyntaxKind) -> Option<String> {
    punctuation_to_text_static(token).map(|s| s.into())
}

// =============================================================================
// Text to Keyword Lookup
// =============================================================================

/// Convert a string to its keyword SyntaxKind, if it's a keyword.
/// Returns None if the text is not a keyword.
#[wasm_bindgen(js_name = textToKeyword)]
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
/// Returns Identifier if the text is not a keyword.
#[wasm_bindgen(js_name = stringToToken)]
pub fn string_to_token(text: &str) -> SyntaxKind {
    text_to_keyword(text).unwrap_or(SyntaxKind::Identifier)
}
