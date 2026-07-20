//! Binder-owned lexical scope checks for selective type-parameter identity.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::{TypeId, TypeParamInfo};

/// Whether an active same-named parameter is a real lexical owner of the
/// current declaration rather than the current binder itself or an unrelated
/// re-entrant scratch-scope entry.
pub(super) fn is_lexical_type_parameter_shadow(
    checker: &CheckerState<'_>,
    active: TypeId,
    active_info: TypeParamInfo,
    name_node: NodeIndex,
) -> bool {
    if !checker
        .ctx
        .binder
        .type_parameter_has_enclosing_same_name_declaration(checker.ctx.arena, name_node)
    {
        return false;
    }

    let belongs_to_current_decl = checker
        .ctx
        .type_param_node_cache
        .get(&(name_node.0, active_info))
        .copied()
        == Some(active)
        || checker.ctx.definition_store.find_type_param_for_decl_node(
            checker.ctx.types.intern_string(&checker.ctx.file_name),
            name_node.0,
            &active_info,
        ) == Some(active);

    !belongs_to_current_decl
}

impl CheckerState<'_> {
    /// Whether this declaration needs exact identity because it shadows an
    /// active same-named parameter from a lexically enclosing generic owner.
    ///
    /// Keep the active-scope check paired with binder ancestry: an unrelated
    /// re-entrant scratch entry must not opt a declaration into the exact
    /// domain, while every scope-reconstruction path must make the same
    /// decision as [`Self::push_type_parameters`].
    pub(crate) fn type_parameter_decl_needs_identity_scope(
        &self,
        name: &str,
        name_node: NodeIndex,
    ) -> bool {
        self.ctx
            .type_parameter_scope
            .get(name)
            .copied()
            .and_then(|active| {
                crate::query_boundaries::common::type_param_info(self.ctx.types, active)
                    .map(|active_info| (active, active_info))
            })
            .is_some_and(|(active, active_info)| {
                is_lexical_type_parameter_shadow(self, active, active_info, name_node)
            })
    }
}
