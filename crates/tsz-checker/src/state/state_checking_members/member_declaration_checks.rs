use crate::context::TypingRequest;

use crate::state::{CheckerState, MemberAccessLevel, MemberLookup};

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::syntax_kind_ext;

use tsz_scanner::SyntaxKind;

use tsz_solver::TypeId;

include!("member_declaration_checks_parts/part1.rs");
include!("member_declaration_checks_parts/part2.rs");
