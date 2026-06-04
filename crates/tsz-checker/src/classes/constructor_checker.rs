use crate::query_boundaries::{
    checkers::constructor::{
        AbstractConstructorAnchor, ConstructorAccessKind, ConstructorReturnMergeKind,
        InstanceTypeKind, classify_for_constructor_access, classify_for_constructor_return_merge,
        classify_for_instance_type, construct_return_type_for_display, has_construct_signatures,
        resolve_abstract_constructor_anchor,
    },
    common,
};

use crate::state::{CheckerState, MAX_TREE_WALK_ITERATIONS, MemberAccessLevel};

use rustc_hash::FxHashSet;

use tsz_binder::{SymbolId, symbol_flags};

use tsz_common::interner::Atom;

use tsz_parser::parser::NodeIndex;

use tsz_scanner::SyntaxKind;

use tsz_solver::TypeId;

use tsz_solver::computation::TypeEnvironment;

include!("constructor_checker_parts/part1.rs");
include!("constructor_checker_parts/part2.rs");
