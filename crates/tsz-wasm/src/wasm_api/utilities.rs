//! Utility Functions
//!
//! Provides commonly used TypeScript utility functions.

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use tsz::lsp::position::LineMap;

// ============================================================================
// Source File Utilities
// ============================================================================

/// Create a source file (parse only, no binding)
///
/// This is a lightweight way to parse a file without full program creation.
#[wasm_bindgen(js_name = createSourceFile)]
pub fn create_source_file(
    file_name: &str,
    source_text: &str,
    _target: Option<u8>,
) -> super::source_file::TsSourceFile {
    super::source_file::TsSourceFile::new(file_name.to_string(), source_text.to_string())
}

/// Parse a JSON configuration file (tsconfig.json, etc.)
#[wasm_bindgen(js_name = parseConfigFileTextToJson)]
pub fn parse_config_file_text_to_json(file_name: &str, json_text: &str) -> String {
    match serde_json::from_str::<serde_json::Value>(json_text) {
        Ok(value) => serde_json::json!({
            "config": value,
            "error": null
        })
        .to_string(),
        Err(e) => serde_json::json!({
            "config": null,
            "error": {
                "messageText": format!("Failed to parse '{}': {}", file_name, e),
                "category": 1,
                "code": 5083
            }
        })
        .to_string(),
    }
}

/// Parse JSON with comments (JSONC)
#[wasm_bindgen(js_name = parseJsonText)]
pub fn parse_json_text(json_text: &str) -> String {
    // Strip comments for basic JSONC support
    let stripped = strip_json_comments(json_text);

    match serde_json::from_str::<serde_json::Value>(&stripped) {
        Ok(value) => serde_json::to_string(&value).unwrap_or_else(|_| "null".to_string()),
        Err(_) => "null".to_string(),
    }
}

fn strip_json_comments(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_string = false;

    while let Some(c) = chars.next() {
        if in_string {
            result.push(c);
            if c == '"' {
                in_string = false;
            } else if c == '\\' {
                if let Some(&next) = chars.peek() {
                    result.push(next);
                    chars.next();
                }
            }
        } else if c == '"' {
            in_string = true;
            result.push(c);
        } else if c == '/' {
            if let Some(&next) = chars.peek() {
                if next == '/' {
                    // Line comment - skip until newline
                    chars.next();
                    while let Some(&ch) = chars.peek() {
                        if ch == '\n' {
                            result.push('\n');
                            chars.next();
                            break;
                        }
                        chars.next();
                    }
                } else if next == '*' {
                    // Block comment - skip until */
                    chars.next();
                    while let Some(ch) = chars.next() {
                        if ch == '*' {
                            if let Some(&'/') = chars.peek() {
                                chars.next();
                                result.push(' '); // Replace with space
                                break;
                            }
                        }
                    }
                } else {
                    result.push(c);
                }
            } else {
                result.push(c);
            }
        } else {
            result.push(c);
        }
    }

    result
}

// ============================================================================
// Scanner/Tokenizer API
// ============================================================================

/// Token information
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenInfo {
    pub kind: u16,
    pub text: String,
    pub start: u32,
    pub end: u32,
}

/// Scan/tokenize source text using a source file
///
/// Returns JSON array of tokens extracted from the parsed AST
#[wasm_bindgen(js_name = scanTokens)]
pub fn scan_tokens(source_text: &str) -> String {
    // Use the TsSourceFile to parse, which gives us access to the AST
    // For a simple tokenizer, we return basic tokens from the source
    let mut sf =
        super::source_file::TsSourceFile::new("tokens.ts".to_string(), source_text.to_string());

    // Get statement handles to trigger parsing
    let _ = sf.get_statement_handles();

    // For now, return a simple representation
    // A full scanner would need to be exposed from the scanner module
    "[]".to_string()
}

/// Get the name of a SyntaxKind
#[wasm_bindgen(js_name = syntaxKindToName)]
pub fn syntax_kind_to_name(kind: u16) -> String {
    // Map common syntax kinds to names
    match kind {
        0 => "Unknown",
        1 => "EndOfFileToken",
        9 => "NumericLiteral",
        10 => "BigIntLiteral",
        11 => "StringLiteral",
        14 => "RegularExpressionLiteral",
        15 => "NoSubstitutionTemplateLiteral",
        16 => "TemplateHead",
        17 => "TemplateMiddle",
        18 => "TemplateTail",
        19 => "OpenBraceToken",
        20 => "CloseBraceToken",
        21 => "OpenParenToken",
        22 => "CloseParenToken",
        23 => "OpenBracketToken",
        24 => "CloseBracketToken",
        25 => "DotToken",
        26 => "DotDotDotToken",
        27 => "SemicolonToken",
        28 => "CommaToken",
        30 => "LessThanToken",
        32 => "GreaterThanToken",
        63 => "EqualsToken",
        80 => "Identifier",
        83 => "BreakKeyword",
        84 => "CaseKeyword",
        85 => "CatchKeyword",
        86 => "ClassKeyword",
        87 => "ConstKeyword",
        88 => "ContinueKeyword",
        90 => "DefaultKeyword",
        91 => "DeleteKeyword",
        92 => "DoKeyword",
        93 => "ElseKeyword",
        94 => "EnumKeyword",
        95 => "ExportKeyword",
        96 => "ExtendsKeyword",
        97 => "FalseKeyword",
        98 => "FinallyKeyword",
        99 => "ForKeyword",
        100 => "FunctionKeyword",
        101 => "IfKeyword",
        102 => "ImportKeyword",
        103 => "InKeyword",
        104 => "InstanceOfKeyword",
        105 => "NewKeyword",
        106 => "NullKeyword",
        107 => "ReturnKeyword",
        108 => "SuperKeyword",
        109 => "SwitchKeyword",
        110 => "ThisKeyword",
        111 => "ThrowKeyword",
        112 => "TrueKeyword",
        113 => "TryKeyword",
        114 => "TypeOfKeyword",
        115 => "VarKeyword",
        116 => "VoidKeyword",
        117 => "WhileKeyword",
        118 => "WithKeyword",
        120 => "InterfaceKeyword",
        121 => "LetKeyword",
        134 => "AsyncKeyword",
        135 => "AwaitKeyword",
        // AST nodes
        167 => "QualifiedName",
        170 => "Parameter",
        173 => "PropertyDeclaration",
        175 => "MethodDeclaration",
        177 => "Constructor",
        184 => "TypeReference",
        189 => "ArrayType",
        193 => "UnionType",
        194 => "IntersectionType",
        210 => "ArrayLiteralExpression",
        211 => "ObjectLiteralExpression",
        212 => "PropertyAccessExpression",
        213 => "ElementAccessExpression",
        214 => "CallExpression",
        215 => "NewExpression",
        219 => "FunctionExpression",
        220 => "ArrowFunction",
        227 => "BinaryExpression",
        228 => "ConditionalExpression",
        242 => "Block",
        244 => "VariableStatement",
        245 => "ExpressionStatement",
        246 => "IfStatement",
        249 => "ForStatement",
        250 => "ForInStatement",
        251 => "ForOfStatement",
        253 => "ReturnStatement",
        262 => "VariableDeclaration",
        263 => "VariableDeclarationList",
        264 => "FunctionDeclaration",
        265 => "ClassDeclaration",
        266 => "InterfaceDeclaration",
        267 => "TypeAliasDeclaration",
        268 => "EnumDeclaration",
        269 => "ModuleDeclaration",
        275 => "ImportDeclaration",
        281 => "ExportDeclaration",
        311 => "SourceFile",
        _ => "Unknown",
    }
    .to_string()
}

// ============================================================================
// Diagnostic Formatting
// ============================================================================

/// Format a diagnostic to a string
#[wasm_bindgen(js_name = formatDiagnostic)]
pub fn format_diagnostic(diagnostic_json: &str) -> String {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct DiagInput {
        file: Option<String>,
        start: Option<u32>,
        length: Option<u32>,
        message_text: String,
        category: u8,
        code: u32,
    }

    let diag: DiagInput = match serde_json::from_str(diagnostic_json) {
        Ok(d) => d,
        Err(_) => return String::new(),
    };

    let category = match diag.category {
        0 => "warning",
        1 => "error",
        2 => "suggestion",
        3 => "message",
        _ => "error",
    };

    if let Some(file) = diag.file {
        if let Some(start) = diag.start {
            format!(
                "{}({}): {} TS{}: {}",
                file, start, category, diag.code, diag.message_text
            )
        } else {
            format!(
                "{}: {} TS{}: {}",
                file, category, diag.code, diag.message_text
            )
        }
    } else {
        format!("{} TS{}: {}", category, diag.code, diag.message_text)
    }
}

/// Format diagnostics with color and context
#[wasm_bindgen(js_name = formatDiagnosticsWithColorAndContext)]
pub fn format_diagnostics_with_color_and_context(
    diagnostics_json: &str,
    source_text: &str,
) -> String {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct DiagInput {
        file: Option<String>,
        start: Option<u32>,
        length: Option<u32>,
        message_text: String,
        category: u8,
        code: u32,
    }

    let diags: Vec<DiagInput> = match serde_json::from_str(diagnostics_json) {
        Ok(d) => d,
        Err(_) => return String::new(),
    };

    let line_map = LineMap::build(source_text);
    let mut output = String::new();

    for diag in diags {
        let category = match diag.category {
            0 => "\x1b[33mwarning\x1b[0m",
            1 => "\x1b[31merror\x1b[0m",
            2 => "\x1b[36msuggestion\x1b[0m",
            3 => "message",
            _ => "\x1b[31merror\x1b[0m",
        };

        if let Some(ref file) = diag.file {
            if let Some(start) = diag.start {
                let pos = line_map.offset_to_position(start, source_text);
                output.push_str(&format!(
                    "\x1b[36m{}:{}:{}\x1b[0m - {} \x1b[90mTS{}\x1b[0m: {}\n",
                    file,
                    pos.line + 1,
                    pos.character + 1,
                    category,
                    diag.code,
                    diag.message_text
                ));

                // Show context line
                let lines: Vec<&str> = source_text.lines().collect();
                if (pos.line as usize) < lines.len() {
                    let line_text = lines[pos.line as usize];
                    output.push_str(&format!("\n{}\n", line_text));

                    // Show squiggly underline
                    let spaces = " ".repeat(pos.character as usize);
                    let length = diag
                        .length
                        .unwrap_or(1)
                        .min(line_text.len() as u32 - pos.character)
                        as usize;
                    let squiggly = "~".repeat(length.max(1));
                    output.push_str(&format!("{}\x1b[31m{}\x1b[0m\n\n", spaces, squiggly));
                }
            } else {
                output.push_str(&format!(
                    "{}: {} TS{}: {}\n",
                    file, category, diag.code, diag.message_text
                ));
            }
        } else {
            output.push_str(&format!(
                "{} TS{}: {}\n",
                category, diag.code, diag.message_text
            ));
        }
    }

    output
}

// ============================================================================
// Version and Compatibility
// ============================================================================

/// Get the tsz version
#[wasm_bindgen(js_name = getTszVersion)]
pub fn get_tsz_version() -> String {
    "0.1.0".to_string()
}

/// Get the TypeScript version this is compatible with
#[wasm_bindgen(js_name = getTypeScriptVersion)]
pub fn get_typescript_version() -> String {
    "5.3.0".to_string()
}

// ============================================================================
// Node Utilities
// ============================================================================

/// Check if a kind is a keyword
#[wasm_bindgen(js_name = isKeyword)]
pub fn is_keyword(kind: u16) -> bool {
    kind >= 83 && kind <= 165
}

/// Check if a kind is a punctuation token
#[wasm_bindgen(js_name = isPunctuation)]
pub fn is_punctuation(kind: u16) -> bool {
    kind >= 19 && kind <= 79
}

/// Check if a kind is a trivia (whitespace, comment)
#[wasm_bindgen(js_name = isTrivia)]
pub fn is_trivia(kind: u16) -> bool {
    kind >= 2 && kind <= 8
}

/// Check if a kind is a literal expression
#[wasm_bindgen(js_name = isLiteralExpression)]
pub fn is_literal_expression(kind: u16) -> bool {
    kind >= 9 && kind <= 15
}

/// Check if a kind is a template literal token
#[wasm_bindgen(js_name = isTemplateLiteralKind)]
pub fn is_template_literal_kind(kind: u16) -> bool {
    kind >= 15 && kind <= 18
}

/// Get the operator precedence for a binary operator
#[wasm_bindgen(js_name = getOperatorPrecedence)]
pub fn get_operator_precedence(operator_kind: u16) -> u8 {
    match operator_kind {
        28 => 0,                        // CommaToken
        63 => 3,                        // EqualsToken (and other assignments)
        56 => 4,                        // QuestionToken (conditional)
        57 => 5,                        // BarBarToken
        55 => 6,                        // AmpersandAmpersandToken
        52 => 7,                        // BarToken
        51 => 8,                        // CaretToken
        50 => 9,                        // AmpersandToken
        34..=37 => 10,                  // equality
        30 | 32 | 33 | 103 | 104 => 11, // relational
        47..=49 => 12,                  // shift
        39 | 40 => 13,                  // additive
        41..=43 => 14,                  // multiplicative
        _ => 0,
    }
}

/// Get the text of a token kind (for operators, keywords)
#[wasm_bindgen(js_name = tokenToString)]
pub fn token_to_string(kind: u16) -> Option<String> {
    let s = match kind {
        19 => "{",
        20 => "}",
        21 => "(",
        22 => ")",
        23 => "[",
        24 => "]",
        25 => ".",
        26 => "...",
        27 => ";",
        28 => ",",
        30 => "<",
        32 => ">",
        33 => "<=",
        34 => ">=",
        35 => "==",
        36 => "!=",
        37 => "===",
        38 => "!==",
        39 => "+",
        40 => "-",
        41 => "*",
        42 => "/",
        43 => "%",
        44 => "++",
        45 => "--",
        46 => "<<",
        47 => ">>",
        48 => ">>>",
        49 => "&",
        50 => "|",
        51 => "^",
        52 => "!",
        53 => "~",
        54 => "&&",
        55 => "||",
        56 => "?",
        57 => ":",
        58 => "=",
        63 => "=>",
        // Keywords
        83 => "break",
        84 => "case",
        85 => "catch",
        86 => "class",
        87 => "const",
        88 => "continue",
        90 => "default",
        91 => "delete",
        92 => "do",
        93 => "else",
        94 => "enum",
        95 => "export",
        96 => "extends",
        97 => "false",
        98 => "finally",
        99 => "for",
        100 => "function",
        101 => "if",
        102 => "import",
        103 => "in",
        104 => "instanceof",
        105 => "new",
        106 => "null",
        107 => "return",
        108 => "super",
        109 => "switch",
        110 => "this",
        111 => "throw",
        112 => "true",
        113 => "try",
        114 => "typeof",
        115 => "var",
        116 => "void",
        117 => "while",
        118 => "with",
        121 => "let",
        134 => "async",
        135 => "await",
        _ => return None,
    };
    Some(s.to_string())
}
