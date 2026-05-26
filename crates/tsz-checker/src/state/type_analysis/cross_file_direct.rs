//! Direct cross-file query fast paths that avoid constructing child checkers.

#[path = "cross_file_direct_paths.rs"]
mod paths;

use super::cross_file_direct_actual_lib::{
    allow_actual_lib_declaration_proof_bypass, allow_generic_actual_lib_direct_fallback,
    is_direct_actual_lib_value_interface_name, iterator_object_has_global_augmentations,
};
use crate::query_boundaries::common;
use crate::state::CheckerState;
pub(crate) use paths::{
    DeclarationFileCacheClass, classify_declaration_file_for_cache,
    is_builtin_lib_declaration_arena, is_builtin_lib_file_name,
    is_direct_actual_lib_declaration_arena, is_dom_builtin_lib_declaration_arena,
    is_external_package_declaration_file_name,
};
use tsz_binder::{BinderState, SymbolId, symbol_flags};
use tsz_common::perf_counters::{
    CrossArenaSymbolMissSource, DirectActualLibAliasBodyOutcome,
    DirectActualLibIntlInterfaceOutcome, DirectCrossFileInterfaceLoweringOutcome,
    DirectSourceFileTypeAliasLoweringOutcome, record_direct_actual_lib_alias_body_outcome,
    record_direct_actual_lib_intl_interface_outcome,
    record_direct_source_file_type_alias_lowering_outcome,
};
use tsz_lowering::TypeLowering;
use tsz_parser::NodeIndex;
use tsz_parser::parser::node::{NodeAccess, NodeArena, TypeAliasData};
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::def::{DefId, DefKind};
use tsz_solver::{TypeId, TypeParamInfo};

struct DirectActualLibAliasBodyProof {
    body: TypeId,
    type_params: Vec<TypeParamInfo>,
    def_id: DefId,
    outcome: DirectActualLibAliasBodyOutcome,
}

fn generic_actual_lib_alias_body_has_direct_shape(
    types: &dyn common::TypeDatabase,
    body: TypeId,
) -> bool {
    common::mapped_type_id(types, body).is_some()
        || common::contains_conditional_type(types, body)
        || common::union_members(types, body).is_some()
        || common::intersection_members(types, body).is_some()
        || common::application_info(types, body).is_some()
        || common::is_string_intrinsic_type(types, body)
}

fn is_direct_lowering_declaration_arena(arena: &NodeArena) -> bool {
    arena.source_files.first().is_some_and(|source_file| {
        source_file.is_declaration_file
            && is_external_package_declaration_file_name(&source_file.file_name)
            && !is_builtin_lib_file_name(&source_file.file_name)
    })
}

pub(super) fn is_direct_lowering_source_file_arena(arena: &NodeArena) -> bool {
    arena
        .source_files
        .first()
        .is_some_and(|source_file| !source_file.is_declaration_file)
}

include!("cross_file_direct/actual_lib_methods.rs");
include!("cross_file_direct/source_shape_methods.rs");
include!("cross_file_direct/source_alias_methods.rs");
include!("cross_file_direct/interface_methods.rs");

#[cfg(test)]
#[path = "cross_file_direct_actual_lib_tests.rs"]
mod actual_lib_tests;

#[cfg(test)]
#[path = "cross_file_direct_tests.rs"]
mod tests;
