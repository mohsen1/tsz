use crate::state::CheckerState;

use crate::symbols_domain::alias_cycle::AliasCycleTracker;

use rustc_hash::{FxHashMap, FxHashSet};

use tsz_common::text_scan::{JSX_PRAGMA_SCAN_BYTES, leading_window};

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::syntax_kind_ext;

use tsz_scanner::SyntaxKind;

const INTERFACE_MEMBER_KIND_PROPERTY: u8 = 1;

const INTERFACE_MEMBER_KIND_METHOD: u8 = 1 << 1;

const CROSS_FILE_INTERFACE_MEMBER_CONFLICT_LIMIT: usize = 8;

include!("cross_file_conflicts_parts/part1.rs");
include!("cross_file_conflicts_parts/part2.rs");
