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
    DescendantAdapter, DescendantContainer, ExpressionRoot, ExpressionTraversal, NestedStatement,
    contains_matching_expression, for_each_statement_in, walk_expression_descendants,
    walk_function_like_descendants, walk_statement_descendants,
};
pub use numeric_literal::{NumberLiteral, NumericRecoveryLiteral, SeparatedNumberLiteral};
pub(crate) use numeric_literal::{
    NumericRecoveryKind, erased_assertion_expression, erased_expression_separated_number,
    numeric_recovery_family, parse_number_literal, statements_form_numeric_recovery_safe_file,
};
pub use parser::{ParseOutput, parse_source};
pub use regular_expression::RegularExpressionLiteral;
pub(crate) use regular_expression::{
    comments_form_regular_expression_safe_file, statements_form_regular_expression_expression_file,
    statements_form_regular_expression_safe_file, statements_form_regular_expression_variable_file,
};
pub use scanner::{ScanOutput, scan_source};
pub use string_literal::{ExtendedUnicodeStringLiteral, StringLiteral, Utf16String};
pub(crate) use string_literal::{
    comments_form_extended_unicode_string_safe_file,
    statements_form_extended_unicode_string_safe_file,
    statements_form_extended_unicode_string_variable_file,
};
pub use template_literal::NoSubstitutionTemplateLiteral;
pub(crate) use template_literal::{
    class_contains_no_substitution_template, expression_contains_no_substitution_template,
    statements_contain_no_substitution_template,
    statements_form_no_substitution_template_expression_file,
    statements_form_no_substitution_template_safe_file,
    statements_form_no_substitution_template_variable_file,
};
pub use token::{Token, TokenKind};
pub(crate) use trivia::{
    CommentKind, CommentPlacement, CommentSourcePosition, CommentTrivia,
    comments_form_contiguous_plain_leading_run,
    comments_form_no_substitution_template_expression_file, is_single_line_whitespace,
    parse_source_check_directive, source_is_ascii_outside_comments,
    source_uses_supported_line_breaks, statement_starts_at_supported_column,
};
pub(crate) use trivia::{SourceCheckDirective, SourceCheckDirectiveKind};
