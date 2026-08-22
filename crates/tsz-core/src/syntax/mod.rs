//! Per-file syntax pipeline. Parsed trees are immutable after construction.

mod ast;
mod parser;
mod scanner;
mod template_literal;
mod token;

pub use ast::*;
pub use parser::{ParseOutput, parse_source};
pub use scanner::{ScanOutput, scan_source};
pub use template_literal::NoSubstitutionTemplateLiteral;
pub(crate) use template_literal::{
    class_contains_no_substitution_template, expression_contains_no_substitution_template,
    statements_contain_no_substitution_template,
    statements_form_no_substitution_template_expression_file,
    statements_form_no_substitution_template_safe_file,
    statements_form_no_substitution_template_variable_file,
};
pub use token::{Token, TokenKind};
