use crate::import::core::ModuleNotFoundSite;

use crate::state::CheckerState;

use crate::symbols_domain::alias_cycle::AliasCycleTracker;

use tsz_common::ModuleKind;

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::node::NodeAccess;

use tsz_parser::parser::syntax_kind_ext;

use tsz_scanner::SyntaxKind;

/// Whether a type-only reference came from `import type` or `export type`.
#[derive(Debug)]
enum TypeOnlyKind {
    ImportType,
    ExportType,
}

include!("equals_parts/part1.rs");
include!("equals_parts/part2.rs");
