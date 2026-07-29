use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_solver::{PropertyInfo, TypeId};

impl<'a> CheckerState<'a> {
    /// Resolve one runtime property of an imported module namespace.
    ///
    /// Module augmentations can introduce a type-space symbol without a value
    /// companion. Returning `None` preserves that absence instead of
    /// materializing an empty namespace object at a value-use site. Native
    /// values are returned with owner-qualified value augmentations already
    /// applied; no-native targets use the exact runtime-export resolver.
    pub(crate) fn namespace_import_export_property_type(
        &mut self,
        module_name: &str,
        export_sym_id: SymbolId,
        export_name: &str,
    ) -> Option<TypeId> {
        let target_lacks_native_value = self
            .module_augmentation_target_native_spaces(module_name, export_name)
            .is_some_and(|(_, has_value)| !has_value);
        if target_lacks_native_value || self.export_symbol_has_no_value(export_sym_id) {
            return self.module_augmentation_runtime_export_type(module_name, export_name);
        }

        if let Some(function_value_type) =
            self.namespace_function_interface_value_type(module_name, export_name)
        {
            return Some(function_value_type);
        }

        let symbol_flags_opt = self
            .get_cross_file_symbol(export_sym_id)
            .or_else(|| self.get_symbol_globally(export_sym_id))
            .map(|symbol| symbol.flags);
        let is_pure_namespace = symbol_flags_opt.is_some_and(|flags| {
            (flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE)) != 0
                && (flags & (symbol_flags::CLASS | symbol_flags::FUNCTION)) == 0
        });
        if is_pure_namespace {
            let prop_type = self.build_namespace_object_type(export_sym_id);
            self.ctx.namespace_module_names.insert(
                prop_type,
                self.imported_namespace_display_module_name(module_name),
            );
            return Some(self.apply_module_value_augmentations(
                module_name,
                export_name,
                prop_type,
            ));
        }

        if let Some(alias_prop_type) =
            self.named_import_alias_namespace_property_type(export_sym_id, export_name)
        {
            return Some(self.apply_module_value_augmentations(
                module_name,
                export_name,
                alias_prop_type,
            ));
        }

        if let Some(value_type) = self.get_validated_member_type(export_sym_id, export_name) {
            return Some(self.apply_module_value_augmentations(
                module_name,
                export_name,
                value_type,
            ));
        }

        let should_delegate = self
            .ctx
            .resolve_symbol_file_index(export_sym_id)
            .is_some_and(|file_idx| file_idx != self.ctx.current_file_idx)
            || self
                .get_cross_file_symbol(export_sym_id)
                .is_some_and(|symbol| {
                    symbol.decl_file_idx != u32::MAX
                        && symbol.decl_file_idx as usize != self.ctx.current_file_idx
                });
        let mut prop_type = if should_delegate {
            self.delegate_cross_arena_symbol_resolution(export_sym_id)
                .map(|(type_id, _)| type_id)
                .unwrap_or_else(|| self.get_type_of_symbol(export_sym_id))
        } else {
            self.get_type_of_symbol(export_sym_id)
        };
        if symbol_flags_opt.is_some_and(|flags| {
            (flags & symbol_flags::ENUM) != 0 && (flags & symbol_flags::ENUM_MEMBER) == 0
        }) {
            prop_type = self.get_enum_namespace_type_for_value(prop_type);
        }
        Some(self.apply_module_value_augmentations(module_name, export_name, prop_type))
    }

    /// Resolve the runtime declaration group of a function whose exported name
    /// is also merged with an interface.
    ///
    /// Namespace and `require` properties carry a binder-local export
    /// `SymbolId`. Looking that raw id up through the consumer can select the
    /// interface side (or a same-numbered local symbol) instead of the
    /// declaring file's function group. Re-resolve the module/export pair to
    /// its owning file, then lower only that owner's function declarations.
    pub(crate) fn namespace_function_interface_value_type(
        &mut self,
        module_name: &str,
        export_name: &str,
    ) -> Option<TypeId> {
        let normalized_module_name = module_name.trim().trim_matches('"').trim_matches('\'');
        let target_file_idx = normalized_module_name
            .strip_prefix("file_idx:")
            .and_then(|file_idx| file_idx.parse::<usize>().ok())
            .or_else(|| {
                self.ctx.resolve_import_target_from_file(
                    self.ctx.current_file_idx,
                    normalized_module_name,
                )
            })
            .or_else(|| self.ctx.resolve_import_target(normalized_module_name))?;
        let mut visited = rustc_hash::FxHashSet::default();
        let (target_sym_id, owner_file_idx) =
            self.resolve_export_in_file(target_file_idx, export_name, &mut visited)?;
        let (target_flags, value_declaration) = {
            let owner_binder = self.ctx.get_binder_for_file(owner_file_idx)?;
            let target_symbol = owner_binder.get_symbol(target_sym_id)?;
            (target_symbol.flags, target_symbol.value_declaration)
        };
        let is_function_interface = (target_flags & symbol_flags::FUNCTION) != 0
            && (target_flags & symbol_flags::INTERFACE) != 0
            && (target_flags & symbol_flags::VALUE) != 0
            && (target_flags & symbol_flags::ALIAS) == 0;
        if !is_function_interface || value_declaration.is_none() {
            return None;
        }

        self.ctx
            .register_symbol_file_target(target_sym_id, owner_file_idx);
        let function_value_type = self.type_of_function_group_for_cross_file_symbol(
            target_sym_id,
            value_declaration,
            owner_file_idx,
        );
        Some(self.apply_module_value_augmentations_to_direct_value(
            module_name,
            export_name,
            function_value_type,
        ))
    }

    /// Append exact runtime exports introduced only by module augmentations.
    ///
    /// The collector can discover broader type/barrel candidates, while the
    /// exact resolver owner-qualifies each name and omits declarations that
    /// contribute no JavaScript value.
    pub(crate) fn append_module_augmentation_runtime_export_properties(
        &mut self,
        module_name: &str,
        properties: &mut Vec<PropertyInfo>,
    ) {
        for augmentation_name in self.collect_module_augmentation_names(module_name) {
            let name = self.ctx.types.intern_string(&augmentation_name);
            if properties.iter().any(|property| property.name == name) {
                continue;
            }
            let Some(prop_type) =
                self.module_augmentation_runtime_export_type(module_name, &augmentation_name)
            else {
                continue;
            };
            properties.push(
                crate::query_boundaries::state::type_analysis::namespace_export_property(
                    name, prop_type, 0,
                ),
            );
        }
    }
}
