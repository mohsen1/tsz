//! Heritage-member extraction for module augmentations.
//!
//! Augmentation interfaces can inherit construction through `extends`, so
//! direct member lowering is only the first half of extraction. Heritage must
//! be resolved in the declaration's own arena and binder before reading
//! construct candidates or inherited properties.

use crate::query_boundaries::construct_signatures::{
    construct_signature_origin, construct_signatures_for_type,
};
use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_binder::ModuleAugmentation;
use tsz_common::perf_counters::CheckerCreationReason;
use tsz_parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext::{METHOD_SIGNATURE, PROPERTY_SIGNATURE};
use tsz_solver::{CallSignature, IndexSignature, PropertyInfo, TypeId};

#[derive(Default)]
pub(crate) struct ModuleAugmentationInterfaceSurface {
    pub(crate) properties: Vec<PropertyInfo>,
    /// Namespace/enum declarations merged with the augmented symbol. These
    /// belong to a class/function value, never to a class instance prototype.
    pub(crate) value_properties: Vec<PropertyInfo>,
    pub(crate) call_signatures: Vec<CallSignature>,
    pub(crate) string_index: Option<IndexSignature>,
    pub(crate) number_index: Option<IndexSignature>,
    pub(crate) symbol_index: Option<IndexSignature>,
}

impl CheckerState<'_> {
    pub(crate) fn get_module_augmentation_members(
        &mut self,
        module_spec: &str,
        interface_name: &str,
    ) -> Vec<PropertyInfo> {
        self.get_module_augmentation_members_inner(module_spec, interface_name, None, None)
            .properties
    }

    pub(crate) fn get_module_augmentation_members_instantiated(
        &mut self,
        module_spec: &str,
        interface_name: &str,
        type_args: &[TypeId],
    ) -> Vec<PropertyInfo> {
        self.get_module_augmentation_members_inner(
            module_spec,
            interface_name,
            Some(type_args),
            None,
        )
        .properties
    }

    /// Resolve an interface declared inside the current string-literal module
    /// augmentation through its owning arena, rather than through a
    /// program-global raw `SymbolId` registration that may belong to another
    /// binder.
    pub(crate) fn local_module_augmentation_interface_type(
        &mut self,
        sym_id: tsz_binder::SymbolId,
    ) -> Option<tsz_solver::TypeId> {
        let (name, declarations) = self
            .local_module_augmentation_symbol(sym_id)
            .map(|symbol| (symbol.escaped_name.clone(), symbol.declarations.clone()))?;
        self.module_augmentation_interface_type_from_local_symbol(name, declarations)
    }

    pub(crate) fn current_module_augmentation_interface_type(
        &mut self,
        sym_id: tsz_binder::SymbolId,
    ) -> Option<tsz_solver::TypeId> {
        // `current_module_augmentation_target_symbol` selects from the
        // current file's binder when the program-wide augmentation index is
        // installed, and from `ctx.binder` only in the single-binder path.
        // Preserve that owner pairing here: `SymbolId` is binder-relative, so
        // re-reading an indexed result through `ctx.binder` can select a
        // same-number foreign symbol or miss the exact local declaration.
        let owner_binder = if self.ctx.global_augmentation_targets_index.is_some() {
            self.ctx.get_binder_for_file(self.ctx.current_file_idx)?
        } else {
            self.ctx.binder
        };
        let (name, declarations) = owner_binder
            .augmentation_target_modules
            .contains_key(&sym_id)
            .then(|| owner_binder.get_symbol(sym_id))
            .flatten()
            .map(|symbol| (symbol.escaped_name.clone(), symbol.declarations.clone()))?;
        self.module_augmentation_interface_type_from_local_symbol(name, declarations)
    }

    fn module_augmentation_interface_type_from_local_symbol(
        &mut self,
        name: String,
        declarations: Vec<NodeIndex>,
    ) -> Option<tsz_solver::TypeId> {
        let mut merged = None;
        for declaration in declarations {
            let is_matching_interface = self
                .ctx
                .arena
                .get(declaration)
                .and_then(|node| self.ctx.arena.get_interface(node))
                .and_then(|interface| self.ctx.arena.get_identifier_text(interface.name))
                .is_some_and(|declaration_name| declaration_name == name);
            if !is_matching_interface {
                continue;
            }
            let declaration_type = self.get_type_of_interface(declaration);
            merged = Some(merged.map_or(declaration_type, |previous| {
                self.merge_interface_types(previous, declaration_type)
            }));
        }
        merged
    }

    pub(crate) fn get_module_augmentation_construct_signatures(
        &mut self,
        module_spec: &str,
        interface_name: &str,
        owner_def_id: Option<tsz_solver::DefId>,
    ) -> Vec<tsz_solver::CallSignature> {
        let declarations = self.get_module_augmentation_declarations(module_spec, interface_name);
        self.get_module_augmentation_construct_signatures_from_declarations(
            &declarations,
            interface_name,
            owner_def_id,
        )
    }

    pub(crate) fn get_module_augmentation_construct_signatures_from_declarations(
        &mut self,
        declarations: &[ModuleAugmentation],
        interface_name: &str,
        owner_def_id: Option<tsz_solver::DefId>,
    ) -> Vec<tsz_solver::CallSignature> {
        let mut signatures = Vec::new();

        for declaration in declarations {
            let arena = declaration.arena.as_deref().unwrap_or(self.ctx.arena);
            let mut group = if arena.shares_node_storage_with(self.ctx.arena) {
                self.module_augmentation_construct_signatures_local(
                    declaration.node,
                    interface_name,
                    owner_def_id,
                )
            } else {
                self.delegate_module_augmentation_construct_signatures(
                    declaration.node,
                    arena,
                    interface_name,
                    owner_def_id,
                )
            };
            signatures.append(&mut group);
        }

        signatures
    }

    pub(crate) fn get_module_augmentation_interface_surface(
        &mut self,
        module_spec: &str,
        interface_name: &str,
        type_args: Option<&[TypeId]>,
    ) -> ModuleAugmentationInterfaceSurface {
        let declarations = self.get_module_augmentation_declarations(module_spec, interface_name);
        self.get_module_augmentation_interface_surface_from_declarations(&declarations, type_args)
    }

    pub(crate) fn get_module_augmentation_interface_surface_from_declarations(
        &mut self,
        declarations: &[ModuleAugmentation],
        type_args: Option<&[TypeId]>,
    ) -> ModuleAugmentationInterfaceSurface {
        let mut surface = ModuleAugmentationInterfaceSurface::default();

        for declaration in declarations {
            let arena = declaration.arena.as_deref().unwrap_or(self.ctx.arena);
            let mut group = if arena.shares_node_storage_with(self.ctx.arena) {
                self.module_augmentation_interface_surface_local(declaration.node, type_args)
            } else {
                self.delegate_module_augmentation_interface_surface(
                    declaration.node,
                    arena,
                    type_args,
                )
            };
            surface.properties.append(&mut group.properties);
            surface.value_properties.append(&mut group.value_properties);
            surface.call_signatures.append(&mut group.call_signatures);
            surface.string_index =
                crate::query_boundaries::intersection_display::merge_index_signature_infos(
                    self.ctx.types,
                    surface.string_index,
                    group.string_index,
                );
            surface.number_index =
                crate::query_boundaries::intersection_display::merge_index_signature_infos(
                    self.ctx.types,
                    surface.number_index,
                    group.number_index,
                );
            surface.symbol_index =
                crate::query_boundaries::intersection_display::merge_index_signature_infos(
                    self.ctx.types,
                    surface.symbol_index,
                    group.symbol_index,
                );
        }

        surface.properties =
            self.normalize_module_augmentation_inherited_properties(surface.properties);
        crate::types_domain::interface_signature_merge::dedup_call_signatures_keep_last(
            &mut surface.call_signatures,
        );
        surface
    }

    fn normalize_module_augmentation_inherited_properties(
        &self,
        properties: Vec<PropertyInfo>,
    ) -> Vec<PropertyInfo> {
        // Multiple augmentation declarations can inherit the same member
        // (including diamonds). Normalize at the solver boundary before any
        // ObjectShape is interned; first declaration order remains stable.
        crate::query_boundaries::intersection_display::normalize_property_infos(
            self.ctx.types,
            properties,
        )
    }

    fn module_augmentation_interface_surface_local(
        &mut self,
        declaration: NodeIndex,
        type_args: Option<&[TypeId]>,
    ) -> ModuleAugmentationInterfaceSurface {
        let Some((type_parameters, member_indices)) = self
            .ctx
            .arena
            .get(declaration)
            .and_then(|node| self.ctx.arena.get_interface(node))
            .map(|interface| {
                (
                    interface.type_parameters.clone(),
                    interface.members.nodes.clone(),
                )
            })
        else {
            return ModuleAugmentationInterfaceSurface::default();
        };
        let mut direct_names = FxHashSet::default();
        for member_idx in member_indices {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if !matches!(member_node.kind, PROPERTY_SIGNATURE | METHOD_SIGNATURE) {
                continue;
            }
            let Some(signature) = self.ctx.arena.get_signature(member_node) else {
                continue;
            };
            if let Some(name) = self.augmentation_member_key_name(self.ctx.arena, signature.name) {
                direct_names.insert(self.ctx.types.intern_string(&name));
            }
        }

        let mut fully_typed = self.get_type_of_interface(declaration);
        if let Some(type_args) = type_args {
            let (type_params, updates) = self.push_type_parameters(&type_parameters);
            if type_params.len() == type_args.len() && !type_params.is_empty() {
                let substitution = crate::query_boundaries::common::TypeSubstitution::from_args(
                    self.ctx.types,
                    &type_params,
                    type_args,
                );
                fully_typed = crate::query_boundaries::common::instantiate_type(
                    self.ctx.types,
                    fully_typed,
                    &substitution,
                );
            }
            self.pop_type_parameters(updates);
        }

        let (
            mut properties,
            string_index,
            number_index,
            symbol_index,
        ) = match crate::query_boundaries::intersection_display::
            collect_properties_in_declaration_order(fully_typed, self.ctx.types, &self.ctx)
        {
            crate::query_boundaries::intersection_display::PropertyCollectionResult::Properties {
                properties,
                string_index,
                number_index,
                symbol_index,
            } => (properties, string_index, number_index, symbol_index),
            _ => (Vec::new(), None, None, None),
        };
        properties.retain(|property| !direct_names.contains(&property.name));

        ModuleAugmentationInterfaceSurface {
            properties,
            value_properties: Vec::new(),
            call_signatures: crate::query_boundaries::common::call_signatures_for_type(
                self.ctx.types,
                fully_typed,
            )
            .unwrap_or_default(),
            string_index,
            number_index,
            symbol_index,
        }
    }

    fn module_augmentation_construct_signatures_local(
        &mut self,
        declaration: NodeIndex,
        interface_name: &str,
        owner_def_id: Option<tsz_solver::DefId>,
    ) -> Vec<tsz_solver::CallSignature> {
        let directly_lowered = self.lower_augmentation_for_arena(
            self.ctx.arena,
            std::slice::from_ref(&declaration),
            &self.ctx.lib_contexts,
            interface_name,
            owner_def_id,
        );
        let mut signatures =
            construct_signatures_for_type(self.ctx.types, directly_lowered).unwrap_or_default();

        // Keep direct signatures from augmentation lowering: it assigns their
        // declaration-group provenance against the augmented home definition,
        // which drives tsc's specialized-first candidate ordering. Then append
        // only the inherited tail from canonical interface typing. Canonical
        // typing owns heritage name resolution and the declaration's generic
        // scope, including bases declared inside the same module block.
        let fully_typed = self.get_type_of_interface(declaration);
        let fully_typed_signatures =
            construct_signatures_for_type(self.ctx.types, fully_typed).unwrap_or_default();
        let canonical_direct_origin = self.ctx.arena.get(declaration).map(|node| {
            construct_signature_origin(
                self.ctx
                    .binder
                    .get_node_symbol(declaration)
                    .map(|symbol_id| self.ctx.get_or_create_def_id(symbol_id)),
                self.ctx.types.intern_string(&self.ctx.file_name),
                node.pos,
                node.end,
            )
        });
        for signature in fully_typed_signatures {
            let is_direct_declaration = signature
                .construct_origin
                .zip(canonical_direct_origin)
                .is_some_and(|(candidate, direct)| {
                    candidate.declaration_file == direct.declaration_file
                        && candidate.declaration_pos == direct.declaration_pos
                        && candidate.declaration_end == direct.declaration_end
                });
            if !is_direct_declaration {
                signatures.push(signature);
            }
        }
        signatures
    }

    fn delegate_module_augmentation_construct_signatures(
        &mut self,
        declaration: NodeIndex,
        arena: &tsz_parser::parser::NodeArena,
        interface_name: &str,
        owner_def_id: Option<tsz_solver::DefId>,
    ) -> Vec<tsz_solver::CallSignature> {
        let delegate_file_idx = self.ctx.get_file_idx_for_arena(arena);
        let delegate_binder = delegate_file_idx
            .and_then(|file_idx| self.ctx.all_binders.as_ref()?.get(file_idx).cloned());
        let Some(delegate_binder) = delegate_binder else {
            return Vec::new();
        };
        let Some(_cross_arena_guard) = Self::enter_cross_arena_delegation() else {
            return Vec::new();
        };
        if !self.ctx.enter_recursion() {
            Self::mark_cross_arena_bailout();
            return Vec::new();
        }

        let delegate_file_name = arena.source_files.first().map_or_else(
            || self.ctx.file_name.clone(),
            |source| source.file_name.clone(),
        );
        tsz_common::perf_counters::record_delegate_cross_arena_miss();
        let _delegate_depth_guard = tsz_common::perf_counters::enter_delegate();
        let mut checker = CheckerState::delegate_for_arena(
            arena,
            delegate_binder.as_ref(),
            delegate_file_name,
            self,
            CheckerCreationReason::DelegateCrossArenaOther,
        );
        let preserve_symbol = delegate_binder
            .get_node_symbol(declaration)
            .unwrap_or(tsz_binder::SymbolId(u32::MAX));
        self.clear_delegated_symbol_cache_collisions(
            &mut checker,
            delegate_binder.as_ref(),
            preserve_symbol,
        );
        checker.ctx.current_file_idx = delegate_file_idx.unwrap_or(self.ctx.current_file_idx);
        let signatures = checker.module_augmentation_construct_signatures_local(
            declaration,
            interface_name,
            owner_def_id,
        );

        self.ctx.leave_recursion();
        signatures
    }

    fn delegate_module_augmentation_interface_surface(
        &mut self,
        declaration: NodeIndex,
        arena: &tsz_parser::parser::NodeArena,
        type_args: Option<&[TypeId]>,
    ) -> ModuleAugmentationInterfaceSurface {
        let delegate_file_idx = self.ctx.get_file_idx_for_arena(arena);
        let delegate_binder = delegate_file_idx
            .and_then(|file_idx| self.ctx.all_binders.as_ref()?.get(file_idx).cloned());
        let Some(delegate_binder) = delegate_binder else {
            return ModuleAugmentationInterfaceSurface::default();
        };
        let Some(_cross_arena_guard) = Self::enter_cross_arena_delegation() else {
            return ModuleAugmentationInterfaceSurface::default();
        };
        if !self.ctx.enter_recursion() {
            Self::mark_cross_arena_bailout();
            return ModuleAugmentationInterfaceSurface::default();
        }

        let delegate_file_name = arena.source_files.first().map_or_else(
            || self.ctx.file_name.clone(),
            |source| source.file_name.clone(),
        );
        tsz_common::perf_counters::record_delegate_cross_arena_miss();
        let _delegate_depth_guard = tsz_common::perf_counters::enter_delegate();
        let mut checker = CheckerState::delegate_for_arena(
            arena,
            delegate_binder.as_ref(),
            delegate_file_name,
            self,
            CheckerCreationReason::DelegateCrossArenaOther,
        );
        let preserve_symbol = delegate_binder
            .get_node_symbol(declaration)
            .unwrap_or(tsz_binder::SymbolId(u32::MAX));
        self.clear_delegated_symbol_cache_collisions(
            &mut checker,
            delegate_binder.as_ref(),
            preserve_symbol,
        );
        checker.ctx.current_file_idx = delegate_file_idx.unwrap_or(self.ctx.current_file_idx);
        let surface = checker.module_augmentation_interface_surface_local(declaration, type_args);

        self.ctx.leave_recursion();
        surface
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query_boundaries::common::TypeInterner;
    use tsz_binder::BinderState;
    use tsz_parser::parser::NodeArena;

    #[test]
    fn inherited_property_normalization_merges_duplicate_names() {
        let arena = NodeArena::new();
        let binder = BinderState::new();
        let types = TypeInterner::new();
        let checker = CheckerState::new(
            &arena,
            &binder,
            &types,
            "test.ts".to_string(),
            Default::default(),
        );
        let name = types.intern_string("shared");
        let properties = checker.normalize_module_augmentation_inherited_properties(vec![
            PropertyInfo::new(name, TypeId::STRING),
            PropertyInfo::new(name, TypeId::NUMBER),
        ]);

        assert_eq!(
            properties.len(),
            1,
            "diamond/repeated heritage must expose one canonical property per name"
        );
        assert_eq!(
            properties[0].type_id,
            TypeId::NEVER,
            "conflicting inherited property types should use canonical intersection semantics"
        );
    }
}
