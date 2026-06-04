use crate::context::{is_declaration_file_name, is_js_file_name};

use crate::state::CheckerState;

use crate::symbols_domain::alias_cycle::AliasCycleTracker;

use tsz_binder::symbol_flags;

use tsz_common::numeric::parse_numeric_literal_value;

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::syntax_kind_ext;

use tsz_scanner::SyntaxKind;

use tsz_solver::{IndexSignature, ObjectShape, TypeId, TypePredicate};

/// Strip a leading and matching trailing `"` or `'` from `s` if both are
/// present. Returns the bare inner string when stripped, otherwise `None`.
fn strip_quoted_string(s: &str) -> Option<&str> {
    s.strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .or_else(|| {
            s.strip_prefix('\'')
                .and_then(|rest| rest.strip_suffix('\''))
        })
}

include!("name_resolution_parts/part1.rs");
include!("name_resolution_parts/part2.rs");
