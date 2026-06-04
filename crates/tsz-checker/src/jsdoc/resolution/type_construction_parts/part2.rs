impl<'a> CheckerState<'a> {
    /// Build the type for a JSDoc `@typedef`. When `recursive_alias_name` is
    /// `Some` and the typedef is generic, the alias is registered as a lazy
    /// `DefId` *before* the body is constructed and recorded in the
    /// generic-typedef re-entrancy guard, so that a self-recursive generic
    /// application inside the body defers to `Application(Lazy(DefId), args)`
    /// instead of re-expanding the body until the stack overflows.
    pub(in crate::jsdoc::resolution) fn type_from_jsdoc_typedef_inner(
        &mut self,
        info: JsdocTypedefInfo,
        recursive_alias_name: Option<&str>,
    ) -> Option<(TypeId, Vec<tsz_solver::TypeParamInfo>)> {
        let factory = self.ctx.types.factory();
        let import_alias_body = info
            .base_type
            .as_deref()
            .is_some_and(|expr| expr.trim_start().starts_with("import("));
        let mut type_param_infos = Vec::with_capacity(info.template_params.len());
        let mut scope_updates = Vec::with_capacity(info.template_params.len());
        for template in &info.template_params {
            let constraint = template
                .constraint
                .as_deref()
                .and_then(|expr| self.resolve_jsdoc_type_str(expr));
            let atom = self.ctx.types.intern_string(&template.name);
            let param = tsz_solver::TypeParamInfo {
                name: atom,
                constraint,
                default: None,
                is_const: false,
            };
            let type_id = factory.type_param(param);
            let previous = self
                .ctx
                .type_parameter_scope
                .insert(template.name.clone(), type_id);
            type_param_infos.push(param);
            scope_updates.push((template.name.clone(), previous));
        }

        // Arm the recursive-generic-typedef guard before constructing the body.
        // Only generic typedefs need this: a non-generic recursive alias already
        // defers through `jsdoc_typedef_resolving` + the file-local Lazy lookup.
        let recursive_def = match recursive_alias_name {
            Some(name) if !type_param_infos.is_empty() => {
                let def_id = self.ensure_recursive_jsdoc_typedef_def(name, &type_param_infos);
                self.ctx
                    .jsdoc_generic_typedef_resolving
                    .borrow_mut()
                    .insert(name.to_owned(), def_id);
                Some(def_id)
            }
            _ => None,
        };

        let result = if let Some(cb) = info.callback {
            self.type_from_jsdoc_callback(cb)
        } else {
            self.type_from_jsdoc_object_typedef(info)
        };

        for (name, previous) in scope_updates.into_iter().rev() {
            if let Some(previous) = previous {
                self.ctx.type_parameter_scope.insert(name, previous);
            } else {
                self.ctx.type_parameter_scope.remove(&name);
            }
        }

        if let (Some(def_id), Some(name)) = (recursive_def, recursive_alias_name) {
            self.ctx
                .jsdoc_generic_typedef_resolving
                .borrow_mut()
                .remove(name);
            // Record the (uninstantiated) generic body so the solver can resolve
            // the deferred `Lazy(DefId)` self-references coinductively. Only
            // overwrite when the body actually resolved — never clobber a
            // previously-resolved alias body with the `ANY` placeholder.
            if let Some(body) = result {
                self.ctx.definition_store.set_body(def_id, body);
            }
        }

        if result.is_none() && import_alias_body {
            return None;
        }
        Some((result.unwrap_or(TypeId::ANY), type_param_infos))
    }

    fn type_from_jsdoc_callback(&mut self, cb: JsdocCallbackInfo) -> Option<TypeId> {
        let factory = self.ctx.types.factory();
        let mut params = Vec::new();
        let mut this_type = None;
        let nested_entries: Vec<(String, String, bool)> = cb
            .params
            .iter()
            .filter_map(|param| {
                (param.name.contains('.') || param.name.contains("[]")).then_some((
                    param.name.clone(),
                    param.type_expr.clone().unwrap_or_else(|| "any".to_string()),
                    param.optional,
                ))
            })
            .collect();

        for param in &cb.params {
            if param.name.contains('.') || param.name.contains("[]") {
                continue;
            }

            let raw_type_expr = param.type_expr.clone().unwrap_or_else(|| "any".to_string());
            let effective_expr = raw_type_expr.trim_end_matches('=').trim();
            let effective_expr = if param.rest {
                effective_expr.trim_start_matches("...").trim()
            } else {
                effective_expr
            };

            let is_object_base = effective_expr == "Object" || effective_expr == "object";
            let is_array_object_base = effective_expr == "Object[]"
                || effective_expr == "object[]"
                || effective_expr == "Array.<Object>"
                || effective_expr == "Array.<object>"
                || effective_expr == "Array<Object>"
                || effective_expr == "Array<object>";

            let mut type_id =
                if (is_object_base || is_array_object_base) && !nested_entries.is_empty() {
                    self.build_nested_param_object_type_from_entries(
                        &nested_entries,
                        &param.name,
                        is_array_object_base,
                    )
                    .or_else(|| self.jsdoc_type_from_expression(effective_expr))
                    .unwrap_or(TypeId::ANY)
                } else {
                    self.jsdoc_type_from_expression(effective_expr)
                        .unwrap_or(TypeId::ANY)
                };

            if param.rest {
                type_id = factory.array(type_id);
            }

            if param.name == "this" {
                this_type = Some(type_id);
                continue;
            }

            let name_atom = self.ctx.types.intern_string(&param.name);
            params.push(ParamInfo {
                name: Some(name_atom),
                type_id,
                optional: param.optional,
                rest: param.rest,
            });
        }

        let mut type_predicate = None;
        let return_type = if let Some((is_asserts, param_name, type_str)) = cb.predicate {
            let pred_type = type_str
                .as_deref()
                .and_then(|s| self.jsdoc_type_from_expression(s));
            let target = if param_name == "this" {
                TypePredicateTarget::This
            } else {
                let atom = self.ctx.types.intern_string(&param_name);
                TypePredicateTarget::Identifier(atom)
            };
            let parameter_index = if param_name != "this" {
                params.iter().position(|param| {
                    param
                        .name
                        .is_some_and(|name| name == self.ctx.types.intern_string(&param_name))
                })
            } else {
                None
            };
            type_predicate = Some(TypePredicate {
                asserts: is_asserts,
                target,
                type_id: pred_type,
                parameter_index,
            });
            if is_asserts {
                TypeId::VOID
            } else {
                TypeId::BOOLEAN
            }
        } else if let Some(ref ret_expr) = cb.return_type {
            let ret_expr = ret_expr.trim();
            self.jsdoc_type_from_expression(ret_expr)
                .or_else(|| {
                    if ret_expr.starts_with('{') && ret_expr.ends_with('}') {
                        self.parse_jsdoc_object_literal_type(ret_expr)
                    } else {
                        None
                    }
                })
                .unwrap_or(TypeId::ANY)
        } else {
            TypeId::VOID
        };

        // `@template` on a JSDoc callback typedef belongs to the typedef alias:
        // `type B<T> = () => T`, not `<T>() => T`. Keeping those parameters on
        // the function body makes alias instantiation shadow them, so `B<string>`
        // still formats and behaves like the uninstantiated `B`.
        Some(factory.function(FunctionShape {
            type_params: Vec::new(),
            params,
            this_type,
            return_type,
            type_predicate,
            is_constructor: false,
            is_method: false,
        }))
    }

    fn type_from_jsdoc_object_typedef(&mut self, info: JsdocTypedefInfo) -> Option<TypeId> {
        let factory = self.ctx.types.factory();
        let base_type = if let Some(base_type_expr) = &info.base_type {
            let expr = base_type_expr.trim();
            if expr != "Object" && expr != "object" {
                return self.resolve_jsdoc_type_str(expr);
            }
            None
        } else {
            None
        };
        let mut top_level = Vec::new();
        let mut nested_entries = Vec::new();
        for prop in info.properties {
            if prop.name.contains('.') {
                nested_entries.push((prop.name, prop.type_expr, prop.optional));
            } else {
                top_level.push(prop);
            }
        }
        let mut prop_infos = Vec::with_capacity(top_level.len());
        for prop in top_level {
            let mut prop_type = if prop.type_expr.trim().is_empty() {
                TypeId::ANY
            } else {
                self.jsdoc_type_from_expression(&prop.type_expr)
                    .unwrap_or(TypeId::ANY)
            };
            let effective_expr = prop.type_expr.trim_end_matches('=').trim();
            let is_array_object_base = effective_expr == "Object[]"
                || effective_expr == "object[]"
                || effective_expr == "Array.<Object>"
                || effective_expr == "Array.<object>"
                || effective_expr == "Array<Object>"
                || effective_expr == "Array<object>";
            if let Some(built) = self.build_nested_param_object_type_from_entries(
                &nested_entries,
                &prop.name,
                is_array_object_base,
            ) {
                prop_type = built;
            }
            if prop.optional
                && self.ctx.strict_null_checks()
                && !self.ctx.exact_optional_property_types()
                && prop_type != TypeId::ANY
                && prop_type != TypeId::UNDEFINED
            {
                prop_type = factory.union2(prop_type, TypeId::UNDEFINED);
            }
            let name_atom = self.ctx.types.intern_string(&prop.name);
            prop_infos.push(PropertyInfo {
                name: name_atom,
                type_id: prop_type,
                write_type: prop_type,
                optional: prop.optional,
                readonly: false,
                is_method: false,
                is_class_prototype: false,
                visibility: Visibility::Public,
                parent_id: None,
                declaration_order: 0,
                is_string_named: false,
                is_symbol_named: false,
                single_quoted_name: false,
            });
        }
        let object_type = if !prop_infos.is_empty() {
            Some(factory.object(prop_infos))
        } else {
            None
        };
        match (object_type, base_type) {
            (Some(obj), Some(base)) => Some(factory.intersection2(obj, base)),
            (Some(obj), None) => Some(obj),
            (None, Some(base)) => Some(base),
            (None, None) => None,
        }
    }

    // NOTE: jsdoc_has_readonly_tag, jsdoc_access_level, find_orphaned_extends_tags_for_statements,
    // is_in_different_function_scope, find_function_body_end are in lookup.rs
    // NOTE: resolve_jsdoc_generic_typedef_type + ensure_recursive_jsdoc_typedef_def
    // live in generic_typedef.rs.
}
