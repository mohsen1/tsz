use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};

/// Parser state - export declarations and control flow statement parsing
//
//
/// switch/try/do statements, string literals, and expression statements.
use super::state::{CONTEXT_FLAG_DISALLOW_IN, ParserState};

use crate::parser::parse_rules::look_ahead_is;

use crate::parser::{
    NodeIndex,
    node::{
        BlockData, ExportAssignmentData, ExportDeclData, IfStatementData, LiteralData, LoopData,
        NamedImportsData, ReturnData, SwitchData, VariableData, VariableDeclarationData,
    },
    syntax_kind_ext,
};

use tsz_scanner::SyntaxKind;

use tsz_scanner::keyword_text_len;

use tsz_scanner::scanner_impl::TokenFlags;

include!("state_declarations_exports_parts/part1.rs");
include!("state_declarations_exports_parts/part2.rs");
