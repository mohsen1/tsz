impl<'a> CheckerState<'a> {
    pub(in crate::jsdoc) fn resolve_jsdoc_symbol_type(
        &mut self,
        sym_id: tsz_binder::SymbolId,
    ) -> TypeId {
        let Some(symbol) = self
            .get_cross_file_symbol(sym_id)
            .or_else(|| self.ctx.binder.get_symbol(sym_id))
            .cloned()
        else {
            return TypeId::ERROR;
        };

        if symbol.has_any_flags(symbol_flags::ALIAS) {
            let mut visited_aliases = AliasCycleTracker::new();
            if let Some(target) = self.resolve_alias_symbol(sym_id, &mut visited_aliases) {
                if target == sym_id {
                    // Some unresolved aliases (notably synthetic JSDoc @import aliases)
                    // can legitimately resolve to themselves. Re-entering with the same
                    // symbol would recurse forever and overflow the stack.
                    return TypeId::ERROR;
                }
                return self.resolve_jsdoc_symbol_type(target);
            }
        }

        if symbol.has_any_flags(symbol_flags::TYPE_PARAMETER) {
            return self.type_reference_symbol_type(sym_id);
        }

        if symbol.has_any_flags(
            symbol_flags::TYPE_ALIAS
                | symbol_flags::CLASS
                | symbol_flags::INTERFACE
                | symbol_flags::ENUM,
        ) {
            return self.type_reference_symbol_type(sym_id);
        }

        if symbol.has_any_flags(symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE) {
            let namespace_type = self.get_type_of_symbol(sym_id);
            if namespace_type != TypeId::ERROR && namespace_type != TypeId::UNKNOWN {
                return namespace_type;
            }
        }

        if symbol.has_any_flags(symbol_flags::FUNCTION) && symbol.value_declaration.is_some() {
            let constructor_type = self.get_type_of_symbol(sym_id);
            if !self.ctx.class_instance_resolution_set.insert(sym_id) {
                let def_id = self.ctx.get_or_create_def_id(sym_id);
                return self.ctx.types.factory().lazy(def_id);
            }
            let instance_type = self.synthesize_js_constructor_instance_type(
                symbol.value_declaration,
                constructor_type,
                &[],
            );
            self.ctx.class_instance_resolution_set.remove(&sym_id);
            if let Some(instance_type) = instance_type {
                return instance_type;
            }
        }

        if symbol.has_any_flags(
            symbol_flags::FUNCTION_SCOPED_VARIABLE | symbol_flags::BLOCK_SCOPED_VARIABLE,
        ) {
            if let Some(enum_type) = symbol
                .declarations
                .iter()
                .copied()
                .filter(|decl| decl.is_some())
                .find_map(|decl| self.jsdoc_enum_annotation_type_for_symbol_decl(sym_id, decl))
            {
                return enum_type;
            }
            if symbol.value_declaration.is_some()
                && let Some(instance_type) = self.resolve_jsdoc_commonjs_binding_element_type(
                    symbol.value_declaration,
                    symbol.escaped_name.as_str(),
                )
            {
                return instance_type;
            }
            let value_type = self.get_type_of_symbol(sym_id);
            let prefer_value_type = symbol.value_declaration.is_some()
                && self.jsdoc_declared_value_symbol_prefers_value_type(
                    sym_id,
                    symbol.value_declaration,
                );
            if !prefer_value_type
                && let Some(instance_type) = self.instance_type_from_constructor_type(value_type)
            {
                return instance_type;
            }
            // Fall back to the raw value type for non-constructor variables.
            if value_type != TypeId::ERROR && value_type != TypeId::UNKNOWN {
                return value_type;
            }
        }

        TypeId::ERROR
    }
}
