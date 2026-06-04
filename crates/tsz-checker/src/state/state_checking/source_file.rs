use crate::context::{TypingRequest, is_declaration_file_name};

use crate::state::CheckerState;

use crate::statements::StatementChecker;

use rustc_hash::FxHashSet;

use tracing::{Level, span};

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::syntax_kind_ext;

use tsz_scanner::SyntaxKind;

include!("source_file_parts/part1.rs");
include!("source_file_parts/part2.rs");

fn is_same_display_assignability_message(message: &str) -> bool {
    let Some(source_rest) = message.strip_prefix("Type '") else {
        return false;
    };
    let Some(source_end) = source_rest.find('\'') else {
        return false;
    };
    let source = &source_rest[..source_end];
    let Some(target_start) = message.find("' is not assignable to type '") else {
        return false;
    };
    let target_rest = &message[target_start + "' is not assignable to type '".len()..];
    let Some(target_end) = target_rest.find('\'') else {
        return false;
    };
    let target = &target_rest[..target_end];

    source == target
}
