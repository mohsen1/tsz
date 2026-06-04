//! Class and interface compatibility checking (TS2415, TS2430), member lookup
//! in class chains, and visibility conflict detection.

include!("class_checker_compat_large_methods/check_interface_extension_compatibility_8_0.rs");

use crate::class_checker::MemberVisibility;
use crate::diagnostics::diagnostic_codes;
use crate::query_boundaries::class::{
    should_report_member_type_mismatch, should_report_property_type_mismatch,
};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    __tsz_split_class_checker_compat_check_interface_extension_compatibility_8_0!();
}
