use tsz_common::diagnostics::diagnostic_codes;

/// Parser state - expression parsing methods
use super::state::{CONTEXT_FLAG_ARROW_PARAMETERS, CONTEXT_FLAG_IN_CONDITIONAL_TRUE, ParserState};

use crate::parser::{
    NodeIndex,
    node::{
        AccessExprData, BinaryExprData, CallExprData, ConditionalExprData, IdentifierData,
        TaggedTemplateData, UnaryExprData, UnaryExprDataEx,
    },
    node_flags, syntax_kind_ext,
};

use tsz_scanner::SyntaxKind;

use tsz_scanner::keyword_text_len;

include!("state_expressions_parts/part1.rs");
include!("state_expressions_parts/part2.rs");
