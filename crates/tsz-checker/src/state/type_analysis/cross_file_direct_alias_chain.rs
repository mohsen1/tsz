use super::source_alias_attribution::record_source_alias_rejection_kinds;

use crate::state::CheckerState;

use tsz_binder::{BinderState, Symbol, SymbolId, symbol_flags};

use tsz_parser::NodeList;

use tsz_parser::parser::node::{NodeAccess, NodeArena, TypeAliasData};

use tsz_parser::parser::{NodeIndex, syntax_kind_ext};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct SourceFileAliasProofKey {
    file_idx: Option<usize>,
    sym_id: SymbolId,
    guarded: bool,
}

type SourceFileImportAliasTarget<'a> =
    dyn Fn(usize, &BinderState, SymbolId) -> Option<SourceFileAliasSymbol<'a>> + 'a;

pub(super) struct SourceFileAliasProofContext<'a> {
    pub(super) current_file_idx: Option<usize>,
    pub(super) global_type_is_lowerable: &'a dyn Fn(&BinderState, &str) -> bool,
    pub(super) global_value_is_lowerable: &'a dyn Fn(&BinderState, &str) -> bool,
    pub(super) import_alias_target: Option<&'a SourceFileImportAliasTarget<'a>>,
}

#[derive(Clone, Copy)]
pub(super) struct SourceFileAliasSymbol<'a> {
    pub(super) arena: &'a NodeArena,
    pub(super) binder: &'a BinderState,
    pub(super) file_idx: Option<usize>,
    pub(super) sym_id: SymbolId,
}

impl<'a> SourceFileAliasProofContext<'a> {
    fn for_file(&self, current_file_idx: Option<usize>) -> SourceFileAliasProofContext<'a> {
        SourceFileAliasProofContext {
            current_file_idx,
            global_type_is_lowerable: self.global_type_is_lowerable,
            global_value_is_lowerable: self.global_value_is_lowerable,
            import_alias_target: self.import_alias_target,
        }
    }
}

include!("cross_file_direct_alias_chain_parts/part1.rs");
include!("cross_file_direct_alias_chain_parts/part2.rs");

include!("cross_file_direct_alias_chain/subtractive_guard_methods.rs");

include!("cross_file_direct_alias_chain/projection_guard_methods.rs");

include!("cross_file_direct_alias_chain/template_literal_guard_methods.rs");

include!("cross_file_direct_alias_chain/type_literal_methods.rs");
