use super::super::class_checker::format_property_name_for_diagnostic;

use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

use crate::query_boundaries::class::{
    should_report_member_type_mismatch, should_report_own_member_type_mismatch,
};

use crate::query_boundaries::common::PropertyAccessResult;

use crate::state::CheckerState;

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::syntax_kind_ext;

use tsz_scanner::SyntaxKind;

use tsz_solver::computation::TypeResolver;

use tsz_solver::{PropertyInfo, TypeId, Visibility};

include!("core_parts/part1.rs");
include!("core_parts/part2.rs");
