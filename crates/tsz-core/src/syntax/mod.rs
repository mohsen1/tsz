//! Per-file syntax pipeline. Parsed trees are immutable after construction.

mod ast;
mod descendant_walk;
mod numeric_literal;
mod parser;
mod regular_expression;
mod scanner;
mod string_literal;
mod template_literal;
mod token;
mod trivia;

pub use ast::*;
pub(crate) use descendant_walk::{
    DescendantAdapter, DescendantContainer, ExpressionEdge, ExpressionRoot, ExpressionTraversal,
    NestedStatement, contains_matching_expression, for_each_statement_in,
    walk_expression_descendants, walk_function_like_descendants, walk_statement_descendants,
    walk_statement_list,
};
pub use numeric_literal::{NumberLiteral, NumericRecoveryLiteral, SeparatedNumberLiteral};
pub(crate) use numeric_literal::{
    NumericRecoveryKind, erased_assertion_expression, erased_expression_separated_number,
    parse_number_literal,
};
pub use parser::{ParseOutput, parse_source};
pub(crate) use regular_expression::RegularExpressionIssue;
pub use regular_expression::RegularExpressionLiteral;
pub use scanner::{ScanOutput, scan_source};
pub use string_literal::{ExtendedUnicodeStringLiteral, StringLiteral, Utf16String};
pub use template_literal::NoSubstitutionTemplateLiteral;
pub(crate) use template_literal::expression_contains_no_substitution_template;
pub use token::{Token, TokenKind};
pub(crate) use trivia::{
    CommentClass, CommentKind, CommentPlacement, CommentSourcePosition, CommentTrivia,
    is_single_line_whitespace, parse_source_check_directive,
};
pub(crate) use trivia::{SourceCheckDirective, SourceCheckDirectiveKind};

pub(crate) const fn keyword_type_text(keyword: KeywordType) -> &'static str {
    match keyword {
        KeywordType::Any => "any",
        KeywordType::Unknown => "unknown",
        KeywordType::Never => "never",
        KeywordType::Void => "void",
        KeywordType::Undefined => "undefined",
        KeywordType::Null => "null",
        KeywordType::Boolean => "boolean",
        KeywordType::Number => "number",
        KeywordType::String => "string",
        KeywordType::BigInt => "bigint",
        KeywordType::Object => "object",
        KeywordType::Symbol => "symbol",
        KeywordType::UniqueSymbol => "unique symbol",
    }
}
