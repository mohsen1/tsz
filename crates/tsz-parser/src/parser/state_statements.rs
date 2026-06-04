use super::state::{CONTEXT_FLAG_IN_BLOCK, IncrementalParseResult, ParserState};

use crate::parser::{
    NodeIndex, NodeList,
    node::{BlockData, QualifiedNameData, SourceFileData, VariableData, VariableDeclarationData},
    syntax_kind_ext,
};

use tsz_common::diagnostics::diagnostic_codes;

use tsz_scanner::{SyntaxKind, token_is_keyword};

include!("state_statements_parts/part1.rs");
include!("state_statements_parts/part2.rs");
