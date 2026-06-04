mod interop;

mod reexports;

use crate::module_resolution::module_specifier_candidates;

use crate::state::CheckerState;

use crate::symbol_resolver::TypeSymbolResolution;

use crate::symbols_domain::alias_cycle::AliasCycleTracker;

use rustc_hash::FxHashSet;

use tsz_binder::symbol_flags;

use tsz_parser::parser::{NodeIndex, syntax_kind_ext};

use tsz_scanner::SyntaxKind;

use tsz_solver::TypeId;

include!("module_parts/part1.rs");
include!("module_parts/part2.rs");

fn path_has_node_modules_segment(file_name: &str) -> bool {
    file_name
        .split(['/', '\\'])
        .any(|component| component == "node_modules")
}

#[cfg(test)]
#[path = "module_tests.rs"]
mod tests;
