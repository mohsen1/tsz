//! Global augmentation property resolution and namespace member
//! visibility helpers.
//!
//! Extracted from `property_access_type.rs` to keep module size manageable.

use super::queries::lib_resolution::{
    augmentation_def_id_from_node, no_value_resolver, resolve_augmentation_node,
};
use crate::state::CheckerState;
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn resolve_array_global_augmentation_property(
        &mut self,
        object_type: TypeId,
        property_name: &str,
    ) -> Option<TypeId> {
        tracing::debug!(
            "resolve_array_global_augmentation_property: property_name = {:?}, object_type = {:?}",
            property_name,
            object_type
        );
        use crate::query_boundaries::common::PropertyAccessResult;
        use rustc_hash::FxHashMap;
        use std::sync::Arc;
        use tsz_lowering::TypeLowering;
        use tsz_parser::parser::NodeArena;

        let base_type =
            crate::query_boundaries::property_access::unwrap_readonly(self.ctx.types, object_type);

        let element_type = if let Some(elem) =
            crate::query_boundaries::property_access::array_element_type(self.ctx.types, base_type)
        {
            Some(elem)
        } else if let Some(union_ty) =
            crate::query_boundaries::property_access::tuple_element_type_union(
                self.ctx.types,
                base_type,
            )
        {
            Some(union_ty)
        } else {
            crate::query_boundaries::property_access::application_first_arg(
                self.ctx.types,
                base_type,
            )
        };
        let element_type = element_type?;

        let augmentation_decls = self.ctx.binder.global_augmentations.get("Array")?;
        if augmentation_decls.is_empty() {
            return None;
        }

        let all_arenas = self.ctx.all_arenas.clone();
        let all_binders = self.ctx.all_binders.clone();
        let lib_contexts = self.ctx.lib_contexts.clone();
        let file_locals_idx = self.ctx.global_file_locals_index.clone();
        let arena_idx = self.ctx.global_arena_index.clone();
        let binder_for_arena = |arena_ref: &NodeArena| -> Option<&tsz_binder::BinderState> {
            let binders = all_binders.as_ref()?;
            let arena_ptr = arena_ref as *const NodeArena as usize;
            // O(1) path via pre-built arena index
            if let Some(idx) = arena_idx.as_ref() {
                let file_idx = *idx.get(&arena_ptr)?;
                return binders.get(file_idx).map(Arc::as_ref);
            }
            // O(N) fallback when index not built
            let arenas = all_arenas.as_ref()?;
            for (idx, arena) in arenas.iter().enumerate() {
                if Arc::as_ptr(arena) as usize == arena_ptr {
                    return binders.get(idx).map(Arc::as_ref);
                }
            }
            None
        };

        let mut cross_file_groups: FxHashMap<usize, (Arc<NodeArena>, Vec<NodeIndex>)> =
            FxHashMap::default();
        for aug in augmentation_decls {
            if let Some(ref arena) = aug.arena {
                let key = Arc::as_ptr(arena) as usize;
                cross_file_groups
                    .entry(key)
                    .or_insert_with(|| (Arc::clone(arena), Vec::new()))
                    .1
                    .push(aug.node);
            } else {
                let key = self.ctx.arena as *const NodeArena as usize;
                cross_file_groups
                    .entry(key)
                    .or_insert_with(|| (Arc::new(self.ctx.arena.clone()), Vec::new()))
                    .1
                    .push(aug.node);
            }
        }

        let mut found_types = Vec::new();
        for (_, (arena, decls)) in cross_file_groups {
            let decl_binder = binder_for_arena(arena.as_ref()).unwrap_or(self.ctx.binder);
            let global_idx = file_locals_idx.as_deref();
            let all_binders_slice = all_binders.as_ref().map(|v| v.as_slice());
            let resolver = |node_idx: NodeIndex| -> Option<u32> {
                resolve_augmentation_node(
                    decl_binder,
                    arena.as_ref(),
                    node_idx,
                    global_idx,
                    all_binders_slice,
                    &lib_contexts,
                )
                .map(|sym_id| sym_id.0)
            };
            let def_id_resolver = |node_idx: NodeIndex| -> Option<tsz_solver::DefId> {
                augmentation_def_id_from_node(
                    &self.ctx,
                    decl_binder,
                    arena.as_ref(),
                    node_idx,
                    global_idx,
                    all_binders_slice,
                    &lib_contexts,
                )
            };

            let decls_with_arenas: Vec<(NodeIndex, &NodeArena)> = decls
                .iter()
                .map(|&decl_idx| (decl_idx, arena.as_ref()))
                .collect();
            let name_resolver = |type_name: &str| -> Option<tsz_solver::DefId> {
                self.resolve_entity_name_text_to_def_id_for_lowering(type_name)
            };
            let lowering = TypeLowering::with_hybrid_resolver(
                arena.as_ref(),
                self.ctx.types,
                &resolver,
                &def_id_resolver,
                &no_value_resolver,
            )
            .with_name_def_id_resolver(&name_resolver);
            let (aug_type, params) =
                lowering.lower_merged_interface_declarations(&decls_with_arenas);
            if aug_type == TypeId::ERROR {
                continue;
            }

            if let PropertyAccessResult::Success { type_id, .. } =
                self.resolve_property_access_with_env(aug_type, property_name)
            {
                found_types.push(type_id);
                continue;
            }

            if !params.is_empty() {
                let mut args = Vec::with_capacity(params.len());
                args.push(element_type);
                for _ in 1..params.len() {
                    args.push(TypeId::ANY);
                }
                let app_type = self.ctx.types.factory().application(aug_type, args);
                if let PropertyAccessResult::Success { type_id, .. } =
                    self.resolve_property_access_with_env(app_type, property_name)
                {
                    found_types.push(type_id);
                }
            }
        }

        if found_types.is_empty() {
            None
        } else {
            Some(tsz_solver::utils::union_or_single(
                self.ctx.types,
                found_types,
            ))
        }
    }

    /// Resolve property from global interface augmentations for primitive wrapper types
    /// and other well-known global interfaces (Boolean, Number, String, `ErrorConstructor`, etc.).
    ///
    /// When a user writes `interface Boolean { doStuff() }` at the top level, this augments
    /// the built-in Boolean interface. Property accesses on `boolean` values should find
    /// these augmented members.
    pub(super) fn resolve_general_global_augmentation_property(
        &mut self,
        object_type: TypeId,
        property_name: &str,
    ) -> Option<TypeId> {
        // Map the object type to potential global interface names
        let interface_names: &[&str] = if crate::query_boundaries::property_access::is_boolean_type(
            self.ctx.types,
            object_type,
        ) {
            &["Boolean"]
        } else if crate::query_boundaries::property_access::is_number_type(
            self.ctx.types,
            object_type,
        ) {
            &["Number"]
        } else if crate::query_boundaries::property_access::is_string_type(
            self.ctx.types,
            object_type,
        ) {
            &["String"]
        } else if crate::query_boundaries::property_access::is_symbol_type(
            self.ctx.types,
            object_type,
        ) {
            &["Symbol"]
        } else if crate::query_boundaries::property_access::is_bigint_type(
            self.ctx.types,
            object_type,
        ) {
            &["BigInt"]
        } else {
            // For object types, try to find the interface name from the symbol
            // that declared the type (handles ErrorConstructor, RegExp, Date, etc.)
            return self.resolve_object_type_global_augmentation(object_type, property_name);
        };

        for &iface_name in interface_names {
            if let Some(result) =
                self.resolve_augmentation_property_by_name(iface_name, property_name)
            {
                return Some(result);
            }
        }
        None
    }

    /// Try to resolve a property from global augmentations for an object type
    /// by looking up its symbol's name in the augmentation map.
    pub(super) fn resolve_object_type_global_augmentation(
        &mut self,
        object_type: TypeId,
        property_name: &str,
    ) -> Option<TypeId> {
        // For object types that come from lib declarations (ErrorConstructor, RegExp, etc.),
        // check if the type's symbol name matches any global augmentation.
        let def_id = crate::query_boundaries::property_access::def_id(self.ctx.types, object_type)?;

        // Look up the symbol for this DefId
        let sym_id = self.ctx.def_to_symbol_id(def_id)?;
        let lib_binders = self.get_lib_binders();
        let symbol = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders)?;
        let name = &symbol.escaped_name;

        if self.ctx.binder.global_augmentations.contains_key(name) {
            return self.resolve_augmentation_property_by_name(name, property_name);
        }
        None
    }

    /// Resolve a property from global augmentation declarations for a specific interface name.
    pub(super) fn resolve_augmentation_property_by_name(
        &mut self,
        interface_name: &str,
        property_name: &str,
    ) -> Option<TypeId> {
        use crate::query_boundaries::common::PropertyAccessResult;
        use rustc_hash::FxHashMap;
        use std::sync::Arc;
        use tsz_lowering::TypeLowering;
        use tsz_parser::parser::NodeArena;

        let augmentation_decls = self.ctx.binder.global_augmentations.get(interface_name)?;
        if augmentation_decls.is_empty() {
            return None;
        }

        let all_arenas = self.ctx.all_arenas.clone();
        let all_binders = self.ctx.all_binders.clone();
        let lib_contexts = self.ctx.lib_contexts.clone();
        let file_locals_idx = self.ctx.global_file_locals_index.clone();
        let arena_idx = self.ctx.global_arena_index.clone();

        let binder_for_arena = |arena_ref: &NodeArena| -> Option<&tsz_binder::BinderState> {
            let binders = all_binders.as_ref()?;
            let arena_ptr = arena_ref as *const NodeArena as usize;
            // O(1) path via pre-built arena index
            if let Some(idx) = arena_idx.as_ref() {
                let file_idx = *idx.get(&arena_ptr)?;
                return binders.get(file_idx).map(Arc::as_ref);
            }
            // O(N) fallback when index not built
            let arenas = all_arenas.as_ref()?;
            for (idx, arena) in arenas.iter().enumerate() {
                if Arc::as_ptr(arena) as usize == arena_ptr {
                    return binders.get(idx).map(Arc::as_ref);
                }
            }
            None
        };

        let mut cross_file_groups: FxHashMap<
            usize,
            (Arc<NodeArena>, Vec<tsz_parser::parser::NodeIndex>),
        > = FxHashMap::default();
        for aug in augmentation_decls {
            if let Some(ref arena) = aug.arena {
                let key = Arc::as_ptr(arena) as usize;
                cross_file_groups
                    .entry(key)
                    .or_insert_with(|| (Arc::clone(arena), Vec::new()))
                    .1
                    .push(aug.node);
            } else {
                let key = self.ctx.arena as *const NodeArena as usize;
                cross_file_groups
                    .entry(key)
                    .or_insert_with(|| (Arc::new(self.ctx.arena.clone()), Vec::new()))
                    .1
                    .push(aug.node);
            }
        }

        let mut found_types = Vec::new();
        for (_, (arena, decls)) in cross_file_groups {
            let decl_binder = binder_for_arena(arena.as_ref()).unwrap_or(self.ctx.binder);
            let global_idx = file_locals_idx.as_deref();
            let all_binders_slice = all_binders.as_ref().map(|v| v.as_slice());
            let resolver = |node_idx: tsz_parser::parser::NodeIndex| -> Option<u32> {
                resolve_augmentation_node(
                    decl_binder,
                    arena.as_ref(),
                    node_idx,
                    global_idx,
                    all_binders_slice,
                    &lib_contexts,
                )
                .map(|sym_id| sym_id.0)
            };
            let def_id_resolver =
                |node_idx: tsz_parser::parser::NodeIndex| -> Option<tsz_solver::DefId> {
                    augmentation_def_id_from_node(
                        &self.ctx,
                        decl_binder,
                        arena.as_ref(),
                        node_idx,
                        global_idx,
                        all_binders_slice,
                        &lib_contexts,
                    )
                };

            let decls_with_arenas: Vec<(tsz_parser::parser::NodeIndex, &NodeArena)> = decls
                .iter()
                .map(|&decl_idx| (decl_idx, arena.as_ref()))
                .collect();
            let name_resolver = |type_name: &str| -> Option<tsz_solver::DefId> {
                self.resolve_entity_name_text_to_def_id_for_lowering(type_name)
            };
            let lowering = TypeLowering::with_hybrid_resolver(
                arena.as_ref(),
                self.ctx.types,
                &resolver,
                &def_id_resolver,
                &no_value_resolver,
            )
            .with_name_def_id_resolver(&name_resolver);
            let (aug_type, _params) =
                lowering.lower_merged_interface_declarations(&decls_with_arenas);
            if aug_type == TypeId::ERROR {
                continue;
            }

            if let PropertyAccessResult::Success { type_id, .. } =
                self.resolve_property_access_with_env(aug_type, property_name)
            {
                found_types.push(type_id);
            }
        }

        if found_types.is_empty() {
            None
        } else {
            Some(tsz_solver::utils::union_or_single(
                self.ctx.types,
                found_types,
            ))
        }
    }

    pub(super) fn qualified_namespace_member_hidden_on_exported_surface(
        &self,
        access_idx: NodeIndex,
        object_expr_idx: NodeIndex,
        _property_name: &str,
    ) -> Option<String> {
        fn rightmost_namespace_name(
            arena: &tsz_parser::parser::node::NodeArena,
            idx: NodeIndex,
        ) -> Option<String> {
            let node = arena.get(idx)?;
            if node.kind == SyntaxKind::Identifier as u16 {
                return arena.get_identifier(node).map(|id| id.escaped_text.clone());
            }
            if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                let access = arena.get_access_expr(node)?;
                let name_node = arena.get(access.name_or_argument)?;
                return arena
                    .get_identifier(name_node)
                    .map(|id| id.escaped_text.clone());
            }
            if node.kind == syntax_kind_ext::QUALIFIED_NAME {
                let name = arena.get_qualified_name(node)?;
                let right = arena.get(name.right)?;
                return arena
                    .get_identifier(right)
                    .map(|id| id.escaped_text.clone());
            }
            None
        }

        fn module_name_matches(
            arena: &tsz_parser::parser::node::NodeArena,
            module_idx: NodeIndex,
            expected_name: &str,
        ) -> bool {
            let Some(node) = arena.get(module_idx) else {
                return false;
            };
            let Some(module) = arena.get_module(node) else {
                return false;
            };
            let Some(name_node) = arena.get(module.name) else {
                return false;
            };
            arena
                .get_identifier(name_node)
                .is_some_and(|ident| ident.escaped_text == expected_name)
        }

        fn module_exports_publicly(
            arena: &tsz_parser::parser::node::NodeArena,
            export_map: &rustc_hash::FxHashMap<u32, bool>,
            module_idx: NodeIndex,
        ) -> bool {
            if export_map.get(&module_idx.0).copied().unwrap_or(false) {
                return true;
            }

            let Some(node) = arena.get(module_idx) else {
                return false;
            };
            let Some(module) = arena.get_module(node) else {
                return false;
            };

            if arena.has_modifier_ref(module.modifiers.as_ref(), SyntaxKind::ExportKeyword)
                || arena.has_modifier_ref(module.modifiers.as_ref(), SyntaxKind::DeclareKeyword)
            {
                return true;
            }

            if let Some(name_node) = arena.get(module.name)
                && (name_node.kind == SyntaxKind::StringLiteral as u16
                    || name_node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16)
            {
                return true;
            }

            let mut current = module_idx;
            while let Some(ext) = arena.get_extended(current) {
                let parent_idx = ext.parent;
                if parent_idx.is_none() {
                    return false;
                }

                let Some(parent_node) = arena.get(parent_idx) else {
                    return false;
                };
                if parent_node.kind == syntax_kind_ext::MODULE_DECLARATION
                    && let Some(parent_module) = arena.get_module(parent_node)
                {
                    if arena.has_modifier_ref(
                        parent_module.modifiers.as_ref(),
                        SyntaxKind::DeclareKeyword,
                    ) {
                        return true;
                    }

                    if let Some(name_node) = arena.get(parent_module.name)
                        && (name_node.kind == SyntaxKind::StringLiteral as u16
                            || name_node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16)
                    {
                        return true;
                    }
                }

                current = parent_idx;
            }

            false
        }

        if self.resolve_identifier_symbol(object_expr_idx).is_some() {
            return None;
        }

        let parent_name = rightmost_namespace_name(self.ctx.arena, object_expr_idx)?;
        let member_id = self.resolve_qualified_symbol(access_idx)?;
        let member_symbol = self
            .get_cross_file_symbol(member_id)
            .or_else(|| self.ctx.binder.get_symbol(member_id))?;

        if (member_symbol.flags & (symbol_flags::VALUE | symbol_flags::EXPORT_VALUE)) == 0
            || member_symbol.is_type_only
        {
            return None;
        }

        let mut saw_matching_namespace_decl = false;
        for &decl_idx in &member_symbol.declarations {
            if decl_idx.is_none() {
                continue;
            }

            let mut current = decl_idx;
            while let Some(ext) = self.ctx.arena.get_extended(current) {
                let parent_idx = ext.parent;
                if parent_idx.is_none() {
                    break;
                }
                let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                    break;
                };
                if parent_node.kind == syntax_kind_ext::MODULE_DECLARATION
                    && module_name_matches(self.ctx.arena, parent_idx, &parent_name)
                {
                    saw_matching_namespace_decl = true;
                    if module_exports_publicly(
                        self.ctx.arena,
                        &self.ctx.binder.module_declaration_exports_publicly,
                        parent_idx,
                    ) {
                        return None;
                    }
                    break;
                }
                current = parent_idx;
            }
        }

        saw_matching_namespace_decl.then(|| format!("typeof {parent_name}"))
    }

    /// Resolve a property from module augmentations for an object type.
    ///
    /// When a property is not found on a class instance type, check if there are
    /// module augmentations (`declare module "X" { interface Y { ... } }`) that add
    /// the property to the class's interface.
    pub(super) fn resolve_module_augmentation_property(
        &mut self,
        object_type: TypeId,
        property_name: &str,
    ) -> Option<TypeId> {
        let base_type =
            crate::query_boundaries::property_access::unwrap_readonly(self.ctx.types, object_type);
        if crate::query_boundaries::common::object_shape_for_type(self.ctx.types, base_type)
            .is_some_and(|shape| {
                shape
                    .flags
                    .contains(tsz_solver::ObjectFlags::NO_MODULE_AUGMENTATION_LOOKUP)
            })
        {
            return None;
        }

        let type_name = self.format_type_for_assignability_message(object_type);
        if type_name.is_empty()
            || type_name == "any"
            || type_name == "unknown"
            || type_name == "never"
        {
            return None;
        }

        let module_specs: Vec<String> =
            if let Some(aug_index) = self.ctx.global_module_augmentations_index.as_ref() {
                aug_index
                    .iter()
                    .filter(|(_, entries)| entries.iter().any(|(_, aug)| aug.name == type_name))
                    .map(|(key, _)| key.clone())
                    .collect()
            } else if let Some(all_binders) = self.ctx.all_binders.as_ref() {
                let mut specs = Vec::new();
                for binder in all_binders.iter() {
                    for (key, augs) in binder.module_augmentations.iter() {
                        if augs.iter().any(|aug| aug.name == type_name) && !specs.contains(key) {
                            specs.push(key.clone());
                        }
                    }
                }
                specs
            } else {
                let mut specs = Vec::new();
                for (key, augs) in self.ctx.binder.module_augmentations.iter() {
                    if augs.iter().any(|aug| aug.name == type_name) {
                        specs.push(key.clone());
                    }
                }
                specs
            };

        if module_specs.is_empty() {
            return None;
        }

        let prop_name_atom = self.ctx.types.intern_string(property_name);
        for module_spec in &module_specs {
            let members = self.get_module_augmentation_members(module_spec, &type_name);
            if let Some(matching_member) = members.iter().find(|m| m.name == prop_name_atom) {
                return Some(matching_member.type_id);
            }
        }

        None
    }
}
