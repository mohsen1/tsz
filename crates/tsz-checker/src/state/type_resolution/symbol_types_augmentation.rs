//! Local symbol selection for type references inside module augmentations.
//!
//! Raw `SymbolId`s are binder-local. When a module augmentation owns the
//! matching target symbol, that local symbol must win over a registered or
//! cross-file symbol with the same numeric ID.

use crate::state::CheckerState;
use crate::types_domain::module_augmentation::ModuleAugmentationSpace;
use tsz_binder::{Symbol, SymbolId, symbol_flags};
use tsz_parser::NodeIndex;
use tsz_solver::TypeId;

impl CheckerState<'_> {
    /// Get a type reference while keeping binder-local augmentation ownership
    /// scoped to this exact syntax node.
    pub(crate) fn get_type_from_type_reference(&mut self, idx: NodeIndex) -> TypeId {
        let type_reference = self
            .ctx
            .arena
            .get(idx)
            .and_then(|node| self.ctx.arena.get_type_ref(node))
            .map(|reference| (reference.type_name, reference.type_arguments.clone()));
        let Some((type_name_idx, type_arguments)) = type_reference else {
            return self.get_type_from_type_reference_inner(idx);
        };
        let has_type_arguments = type_arguments
            .as_ref()
            .is_some_and(|arguments| !arguments.nodes.is_empty());

        if let Some(interface_type) =
            self.exact_namespace_import_interface_type(idx, type_name_idx, type_arguments.as_ref())
        {
            return interface_type;
        }

        let exact_target = self.exact_namespace_import_augmentation_target(type_name_idx);
        if !has_type_arguments
            && let Some((module_specifier, member_name)) = exact_target.as_ref()
            && let Some(interface_type) =
                self.exact_augmentation_only_interface_type(module_specifier, member_name)
        {
            return interface_type;
        }

        let exact_owner = exact_target
            .as_ref()
            .and_then(|(module_specifier, member_name)| {
                self.exact_module_augmentation_declaration_owner(module_specifier, member_name)
            });
        if !has_type_arguments && let Some((augmentation, _)) = exact_owner.as_ref() {
            let arena = augmentation.arena.as_deref().unwrap_or(self.ctx.arena);
            if let Some(enum_type) =
                self.augmentation_enum_declaration_type(augmentation.node, arena)
            {
                return enum_type;
            }
        }

        let owned_augmentation_symbol = exact_owner
            .and_then(|(augmentation, owner_file_idx)| {
                self.ctx
                    .get_binder_for_file(owner_file_idx)?
                    .get_node_symbol(augmentation.node)
                    .map(|symbol_id| (symbol_id, owner_file_idx))
            })
            .or_else(|| {
                self.current_module_augmentation_type_position_symbol(type_name_idx)
                    .map(|symbol_id| (symbol_id, self.ctx.current_file_idx))
            });
        let Some((symbol_id, owner_file_idx)) = owned_augmentation_symbol else {
            return self.get_type_from_type_reference_inner(idx);
        };

        // Raw `SymbolId`s are binder-local. Temporarily attribute this exact
        // syntactic augmentation reference to its current-file owner, then
        // restore the prior overlay entry so a same-number foreign symbol is
        // unaffected by query order.
        let previous_owner = self.ctx.local_symbol_file_target_override(symbol_id);
        self.ctx
            .register_symbol_file_target(symbol_id, owner_file_idx);
        let result = self.get_type_from_type_reference_inner(idx);
        self.ctx
            .restore_local_symbol_file_target_override(symbol_id, previous_owner);
        result
    }

    fn exact_namespace_import_augmentation_target(
        &self,
        type_name_idx: NodeIndex,
    ) -> Option<(String, String)> {
        let (module_specifier, mut members) =
            self.exact_namespace_import_type_path(type_name_idx)?;
        if members.len() != 1 {
            return None;
        }
        Some((module_specifier, members.pop()?))
    }

    pub(crate) fn exact_augmentation_only_interface_type(
        &mut self,
        module_specifier: &str,
        member_name: &str,
    ) -> Option<TypeId> {
        let (target_has_native_type, _) =
            self.module_augmentation_target_native_spaces(module_specifier, member_name)?;
        if target_has_native_type {
            return None;
        }

        let declarations =
            self.exact_module_augmentation_declarations(module_specifier, member_name);
        let mut has_interface = false;
        for declaration in &declarations {
            let arena = declaration.arena.as_deref().unwrap_or(self.ctx.arena);
            let node = arena.get(declaration.node)?;
            if let Some(interface) = arena.get_interface(node) {
                if interface
                    .type_parameters
                    .as_ref()
                    .is_some_and(|parameters| !parameters.nodes.is_empty())
                {
                    return None;
                }
                has_interface = true;
                continue;
            }
            if matches!(
                node.kind,
                tsz_parser::parser::syntax_kind_ext::CLASS_DECLARATION
                    | tsz_parser::parser::syntax_kind_ext::ENUM_DECLARATION
                    | tsz_parser::parser::syntax_kind_ext::TYPE_ALIAS_DECLARATION
            ) {
                return None;
            }
        }
        if !has_interface {
            return None;
        }

        let empty = crate::query_boundaries::module_augmentation::empty_declaration_space_type(
            self.ctx.types,
        );
        let merged = self.apply_module_augmentations_in_space(
            module_specifier,
            member_name,
            empty,
            ModuleAugmentationSpace::Type,
            false,
            Some(&declarations),
        );
        if matches!(merged, TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN) {
            return None;
        }
        let (augmentation, owner_file_idx) =
            self.exact_module_augmentation_declaration_owner(module_specifier, member_name)?;
        let owner_binder = self.ctx.get_binder_for_file(owner_file_idx)?;
        let symbol_id = owner_binder.get_node_symbol(augmentation.node)?;
        let definition =
            self.ctx
                .def_id_for_declaration_in_file(symbol_id, owner_file_idx, member_name)?;
        self.ctx
            .register_augmented_def_in_envs(definition, merged, false);
        Some(
            crate::query_boundaries::module_augmentation::declaration_space_lazy_type(
                self.ctx.types,
                definition,
            ),
        )
    }

    pub(crate) fn local_type_reference_symbol(&self, sym_id: SymbolId) -> Option<&Symbol> {
        let local_alias = self
            .ctx
            .resolve_dynamic_symbol_file_index(sym_id)
            .is_none()
            .then(|| {
                self.ctx
                    .binder
                    .get_symbol(sym_id)
                    .filter(|symbol| symbol.has_any_flags(symbol_flags::ALIAS))
            })
            .flatten();
        local_alias.or_else(|| self.local_module_augmentation_symbol(sym_id))
    }

    pub(crate) fn local_type_reference_with_params_symbol(
        &self,
        sym_id: SymbolId,
    ) -> Option<&Symbol> {
        self.ctx
            .binder
            .get_symbol(sym_id)
            .filter(|symbol| symbol.has_any_flags(symbol_flags::ALIAS))
            .or_else(|| self.local_module_augmentation_symbol(sym_id))
    }

    pub(crate) fn local_module_augmentation_symbol(&self, sym_id: SymbolId) -> Option<&Symbol> {
        // A cross-file lookup records the owning file before returning its
        // binder-local raw id. Respect that explicit owner: the same numeric id
        // may name an unrelated augmentation interface in this binder.
        //
        // Exact local augmentation lookup sites register `current_file_idx`
        // before handing the id to the shared type-reference pipeline, so this
        // still lets the declaration-local symbol win over a stale global
        // owner-index entry without stealing a freshly resolved foreign symbol.
        if self
            .ctx
            .resolve_dynamic_symbol_file_index(sym_id)
            .is_some_and(|file_idx| file_idx != self.ctx.current_file_idx)
        {
            return None;
        }
        self.ctx
            .binder
            .augmentation_target_modules
            .contains_key(&sym_id)
            .then(|| self.ctx.binder.get_symbol(sym_id))
            .flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::CheckerOptions;
    use crate::query_boundaries::common::TypeInterner;
    use std::sync::Arc;
    use tsz_binder::BinderState;
    use tsz_parser::parser::ParserState;

    #[test]
    fn explicit_foreign_owner_blocks_same_number_local_augmentation_symbol() {
        let source = r#"
export {};
declare module "./home" {
    interface LocalCollision { local: true }
}
"#;
        let mut parser = ParserState::new("augmentation.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = Arc::new(parser.into_arena());
        let mut binder = BinderState::new();
        binder.bind_source_file(arena.as_ref(), root);
        let sym_id = binder
            .augmentation_target_modules
            .keys()
            .copied()
            .find(|&candidate| {
                binder
                    .get_symbol(candidate)
                    .is_some_and(|symbol| symbol.escaped_name == "LocalCollision")
            })
            .expect("augmentation interface symbol");
        let type_reference = binder
            .get_symbol(sym_id)
            .and_then(|symbol| symbol.declarations.first().copied())
            .and_then(|declaration| arena.get(declaration))
            .and_then(|node| arena.get_interface(node))
            .and_then(|interface| interface.members.nodes.first().copied())
            .and_then(|member| arena.get(member))
            .and_then(|node| arena.get_signature(node))
            .map(|signature| signature.type_annotation)
            .expect("self property type reference");
        let types = TypeInterner::new();
        let mut checker = CheckerState::new(
            arena.as_ref(),
            &binder,
            &types,
            "augmentation.ts".to_string(),
            CheckerOptions::default(),
        );
        checker.ctx.set_current_file_idx(1);

        assert!(
            checker.local_module_augmentation_symbol(sym_id).is_some(),
            "an unqualified local augmentation id should resolve locally"
        );
        checker.ctx.register_symbol_file_target(sym_id, 0);
        assert!(
            checker.local_module_augmentation_symbol(sym_id).is_none(),
            "an explicitly owned foreign id must not be stolen by a same-number local augmentation"
        );
        let _ = checker.get_type_from_type_reference(type_reference);
        assert_eq!(
            checker.ctx.resolve_dynamic_symbol_file_index(sym_id),
            Some(0),
            "an exact local query must restore the prior foreign owner instead of making later resolution query-order dependent"
        );
        checker.ctx.register_symbol_file_target(sym_id, 1);
        assert!(
            checker.local_module_augmentation_symbol(sym_id).is_some(),
            "an exact local lookup can restore the current owner before shared raw-id resolution"
        );
    }
}
