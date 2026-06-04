use super::state::{
    CONTEXT_FLAG_ASYNC, CONTEXT_FLAG_DISALLOW_IN, CONTEXT_FLAG_FUNCTION_BODY,
    CONTEXT_FLAG_GENERATOR, CONTEXT_FLAG_IN_PARENTHESIZED_EXPRESSION, CONTEXT_FLAG_STATIC_BLOCK,
    ParserState,
};

use crate::parser::{
    NodeIndex, NodeList,
    node::{
        AccessExprData, CallExprData, IdentifierData, LiteralData, LiteralExprData,
        ParenthesizedData, TaggedTemplateData, TemplateExprData, TemplateSpanData,
    },
    syntax_kind_ext,
};

use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

use tsz_common::interner::Atom;

use tsz_scanner::SyntaxKind;

use tsz_scanner::keyword_text_len;

use tsz_scanner::scanner_impl::TokenFlags;

include!("state_expressions_literals_parts/part1.rs");
include!("state_expressions_literals_parts/part2.rs");
include!("state_expressions_literals_parts/part3.rs");
