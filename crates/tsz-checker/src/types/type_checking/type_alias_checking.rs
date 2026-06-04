use super::alias_defid_visited_pool::with_alias_defid_visited;

use crate::state::CheckerState;

use tsz_parser::parser::node::NodeAccess;

use tsz_parser::parser::syntax_kind_ext;

use tsz_parser::parser::{NodeIndex, NodeList};

use tsz_scanner::SyntaxKind;

use tsz_solver::TypeId;

#[inline]
fn record_type_alias_phase_timing(
    file: &str,
    name: Option<&str>,
    phase: &'static str,
    pos: u32,
    end: u32,
    start: Option<web_time::Instant>,
) {
    if let Some(start) = start {
        tsz_common::perf_counters::record_slow_type_alias_check_timing(
            file,
            name,
            phase,
            pos,
            end,
            start.elapsed().as_nanos() as u64,
        );
    }
}

include!("type_alias_checking_parts/part1.rs");
include!("type_alias_checking_parts/part2.rs");
