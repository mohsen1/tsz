//! Literal and identifier AST nodes.

use super::base::{NodeBase, NodeIndex, NodeList};
use crate::parser::syntax_kind_ext;
use crate::scanner::SyntaxKind;
use serde::Serialize;

/// An identifier node.
#[derive(Clone, Debug, Serialize)]
pub struct Identifier {
    pub base: NodeBase,
    /// The escaped text of the identifier (with unicode escapes processed)
    pub escaped_text: String,
    /// Original text as it appeared in source (for emit)
    pub original_text: Option<String>,
    /// Type arguments (for JSX intrinsic elements)
    pub type_arguments: Option<NodeList>,
}

impl Identifier {
    pub fn new(escaped_text: String, pos: u32, end: u32) -> Identifier {
        Identifier {
            base: NodeBase::new(SyntaxKind::Identifier, pos, end),
            escaped_text,
            original_text: None,
            type_arguments: None,
        }
    }
}

/// A string literal node.
#[derive(Clone, Debug, Serialize)]
pub struct StringLiteral {
    pub base: NodeBase,
    pub text: String,
    pub is_unterminated: bool,
    pub has_extended_unicode_escape: bool,
}

/// A numeric literal node.
#[derive(Clone, Debug, Serialize)]
pub struct NumericLiteral {
    pub base: NodeBase,
    pub text: String,
    /// The numeric value (parsed from text)
    pub value: f64,
}

/// A BigInt literal node.
#[derive(Clone, Debug, Serialize)]
pub struct BigIntLiteral {
    pub base: NodeBase,
    pub text: String,
}

/// A regular expression literal node.
#[derive(Clone, Debug, Serialize)]
pub struct RegularExpressionLiteral {
    pub base: NodeBase,
    pub text: String,
}

/// A template literal span (part of a template expression).
#[derive(Clone, Debug, Serialize)]
pub struct TemplateSpan {
    pub base: NodeBase,
    pub expression: NodeIndex,
    pub literal: NodeIndex, // TemplateMiddle or TemplateTail
}

/// The root node representing a source file.
#[derive(Clone, Debug, Serialize)]
pub struct SourceFile {
    pub base: NodeBase,
    pub statements: NodeList,
    pub end_of_file_token: NodeIndex,
    pub file_name: String,
    pub text: String,
    pub language_version: u32,
    pub language_variant: u32,
    pub script_kind: u32,
    pub is_declaration_file: bool,
    pub has_no_default_lib: bool,
    /// Identifiers in the file (for binding)
    pub identifiers: Vec<String>,
}

impl SourceFile {
    pub fn new(file_name: String, text: String) -> SourceFile {
        let len = text.len() as u32;
        SourceFile {
            base: NodeBase::new_ext(syntax_kind_ext::SOURCE_FILE, 0, len),
            statements: NodeList::new(),
            end_of_file_token: NodeIndex::NONE,
            file_name,
            text,
            language_version: 0,
            language_variant: 0,
            script_kind: 0,
            is_declaration_file: false,
            has_no_default_lib: false,
            identifiers: Vec::new(),
        }
    }
}
