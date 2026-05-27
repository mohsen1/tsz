use super::{TypeId, TypePrinter, visitor};
use tsz_binder::{SymbolId, symbol_flags};

impl<'a> TypePrinter<'a> {
    pub(crate) fn format_type_literal_parts(&self, parts: &[String]) -> String {
        if let Some(indent) = self.indent_level {
            let member_indent = "    ".repeat((indent + 1) as usize);
            let closing_indent = "    ".repeat(indent as usize);
            let lines: Vec<String> = parts
                .iter()
                .map(|p| format!("{member_indent}{p};"))
                .collect();
            format!("{{\n{}\n{}}}", lines.join("\n"), closing_indent)
        } else {
            format!("{{ {} }}", parts.join("; "))
        }
    }

    pub(crate) fn print_call_signature(
        &self,
        sig: &tsz_solver::types::CallSignature,
        is_construct: bool,
        is_abstract: bool,
    ) -> String {
        let prefix = if is_construct && is_abstract {
            "abstract new "
        } else if is_construct {
            "new "
        } else {
            ""
        };

        let scoped = self.with_type_param_scope(&sig.type_params);
        let type_params_str = if !sig.type_params.is_empty() {
            let params: Vec<String> = sig
                .type_params
                .iter()
                .map(|tp| scoped.print_type_parameter_decl(tp))
                .collect();
            format!("<{}>", params.join(", "))
        } else {
            String::new()
        };

        let mut params = Vec::new();
        if let Some(this_type) = sig.this_type {
            params.push(format!("this: {}", scoped.print_type(this_type)));
        }
        for param in &sig.params {
            let mut param_str = String::new();
            if param.rest {
                param_str.push_str("...");
            }
            if let Some(name) = param.name {
                param_str.push_str(&scoped.resolve_atom(name));
                if param.optional {
                    param_str.push('?');
                }
                param_str.push_str(": ");
            }
            if param.optional {
                param_str.push_str(&scoped.print_optional_param_type(param.type_id));
            } else {
                param_str.push_str(&scoped.print_type(param.type_id));
            }
            params.push(param_str);
        }

        let mut nested = scoped.clone();
        if let Some(indent) = nested.indent_level {
            nested.indent_level = Some(indent + 1);
        }
        let return_str = if let Some(ref pred) = sig.type_predicate {
            nested.print_type_predicate(pred)
        } else {
            nested.print_type(sig.return_type)
        };
        format!(
            "{}{}({}): {}",
            prefix,
            type_params_str,
            params.join(", "),
            return_str
        )
    }

    /// Print a call signature in arrow function syntax: (params) => `ReturnType`
    pub(crate) fn print_call_signature_arrow(
        &self,
        sig: &tsz_solver::types::CallSignature,
    ) -> String {
        let scoped = self.with_type_param_scope(&sig.type_params);
        let type_params_str = if !sig.type_params.is_empty() {
            let params: Vec<String> = sig
                .type_params
                .iter()
                .map(|tp| scoped.print_type_parameter_decl(tp))
                .collect();
            format!("<{}>", params.join(", "))
        } else {
            String::new()
        };

        let mut params = Vec::new();
        if let Some(this_type) = sig.this_type {
            params.push(format!("this: {}", scoped.print_type(this_type)));
        }
        for param in &sig.params {
            let mut param_str = String::new();
            if param.rest {
                param_str.push_str("...");
            }
            if let Some(name) = param.name {
                param_str.push_str(&scoped.resolve_atom(name));
                if param.optional {
                    param_str.push('?');
                }
                param_str.push_str(": ");
            }
            if param.optional {
                param_str.push_str(&scoped.print_optional_param_type(param.type_id));
            } else {
                param_str.push_str(&scoped.print_type(param.type_id));
            }
            params.push(param_str);
        }

        let mut nested = scoped.clone();
        if let Some(indent) = nested.indent_level {
            nested.indent_level = Some(indent + 1);
        }
        let return_str = if let Some(ref pred) = sig.type_predicate {
            nested.print_type_predicate(pred)
        } else {
            let inner = nested.print_type(sig.return_type);
            // tsc parenthesises a conditional return when emitting an
            // arrow-form callable so the printed text round-trips
            // unambiguously through the parser even when nested inside
            // a larger conditional or extends position (the outer
            // conditional's `? : ` would otherwise capture the inner's
            // `? :`).  Mirror that here.
            if tsz_solver::is_conditional_type(self.interner, sig.return_type) {
                format!("({inner})")
            } else {
                inner
            }
        };
        format!(
            "{}({}) => {}",
            type_params_str,
            params.join(", "),
            return_str
        )
    }

    pub(crate) fn print_construct_signature_arrow(
        &self,
        sig: &tsz_solver::types::CallSignature,
        is_abstract: bool,
    ) -> String {
        let scoped = self.with_type_param_scope(&sig.type_params);
        let type_params_str = if !sig.type_params.is_empty() {
            let params: Vec<String> = sig
                .type_params
                .iter()
                .map(|tp| scoped.print_type_parameter_decl(tp))
                .collect();
            format!("<{}>", params.join(", "))
        } else {
            String::new()
        };

        let mut params = Vec::new();
        for param in &sig.params {
            let mut param_str = String::new();
            if param.rest {
                param_str.push_str("...");
            }
            if let Some(name) = param.name {
                param_str.push_str(&scoped.resolve_atom(name));
                if param.optional {
                    param_str.push('?');
                }
                param_str.push_str(": ");
            }
            if param.optional {
                param_str.push_str(&scoped.print_optional_param_type(param.type_id));
            } else {
                param_str.push_str(&scoped.print_type(param.type_id));
            }
            params.push(param_str);
        }

        let mut nested = scoped.clone();
        if let Some(indent) = nested.indent_level {
            nested.indent_level = Some(indent.saturating_sub(2));
        }
        let return_str = if let Some(ref pred) = sig.type_predicate {
            nested.print_type_predicate(pred)
        } else {
            nested.print_type(sig.return_type)
        };

        let prefix = if is_abstract { "abstract new " } else { "new " };
        format!(
            "{prefix}{}({}) => {}",
            type_params_str,
            params.join(", "),
            return_str
        )
    }

    pub(crate) fn type_needs_parentheses_in_composition(&self, type_id: TypeId) -> bool {
        if visitor::function_shape_id(self.interner, type_id).is_some() {
            return true;
        }

        let Some(callable_id) = visitor::callable_shape_id(self.interner, type_id) else {
            return false;
        };
        let callable = self.interner.callable_shape(callable_id);
        let has_properties = callable
            .properties
            .iter()
            .any(|property| !self.property_is_hidden_in_declaration_shape(property));

        if callable.is_abstract
            && callable.call_signatures.is_empty()
            && callable.construct_signatures.len() == 1
            && (has_properties
                || callable.string_index.is_some()
                || callable.number_index.is_some())
        {
            return true;
        }

        callable.symbol.is_none()
            && !has_properties
            && callable.string_index.is_none()
            && callable.number_index.is_none()
            && (callable.call_signatures.len() == 1
                || (callable.call_signatures.is_empty()
                    && callable.construct_signatures.len() == 1))
    }

    pub(crate) fn composition_member_text(&self, type_id: TypeId) -> String {
        let Some(callable_id) = visitor::callable_shape_id(self.interner, type_id) else {
            return self.print_type(type_id);
        };
        let callable = self.interner.callable_shape(callable_id);
        let has_properties = callable
            .properties
            .iter()
            .any(|property| !self.property_is_hidden_in_declaration_shape(property));

        if callable.symbol.is_none()
            && !has_properties
            && callable.string_index.is_none()
            && callable.number_index.is_none()
            && callable.call_signatures.is_empty()
            && callable.construct_signatures.len() == 1
        {
            return self.print_construct_signature_arrow(
                &callable.construct_signatures[0],
                callable.is_abstract,
            );
        }

        self.print_type(type_id)
    }

    /// Print a type predicate (e.g., `x is string`, `asserts x is string`, `this is Foo`)
    pub(crate) fn print_type_predicate(&self, pred: &tsz_solver::types::TypePredicate) -> String {
        let mut result = String::new();
        if pred.asserts {
            result.push_str("asserts ");
        }
        match &pred.target {
            tsz_solver::types::TypePredicateTarget::This => result.push_str("this"),
            tsz_solver::types::TypePredicateTarget::Identifier(atom) => {
                result.push_str(&self.resolve_atom(*atom));
            }
        }
        if let Some(type_id) = pred.type_id {
            result.push_str(" is ");
            result.push_str(&self.print_type(type_id));
        }
        result
    }

    /// Print a type parameter as a type reference (just the name).
    pub(crate) fn print_type_parameter(
        &self,
        param_info: &tsz_solver::types::TypeParamInfo,
    ) -> String {
        self.resolve_type_param_name(param_info.name)
    }

    pub(crate) fn print_type_parameter_type(
        &self,
        type_id: TypeId,
        param_info: &tsz_solver::types::TypeParamInfo,
    ) -> String {
        self.resolve_type_param_type_name(type_id, param_info.name)
    }

    pub(crate) fn replace_type_param_name_with_any(text: &str, name: &str) -> String {
        let mut result = String::with_capacity(text.len());
        let bytes = text.as_bytes();
        let name_bytes = name.as_bytes();
        let mut last_copied = 0usize;
        let mut i = 0usize;

        while i + name_bytes.len() <= bytes.len() {
            if &bytes[i..i + name_bytes.len()] == name_bytes
                && (i == 0 || !Self::is_identifier_continue(bytes[i - 1]))
                && (i + name_bytes.len() == bytes.len()
                    || !Self::is_identifier_continue(bytes[i + name_bytes.len()]))
            {
                result.push_str(&text[last_copied..i]);
                result.push_str("any");
                i += name_bytes.len();
                last_copied = i;
                continue;
            }
            i += 1;
        }

        result.push_str(&text[last_copied..]);
        result
    }

    const fn is_identifier_continue(byte: u8) -> bool {
        byte == b'_' || byte == b'$' || byte.is_ascii_alphanumeric()
    }

    /// Print a type parameter declaration with constraint and default.
    /// Used in `<T extends Foo = Bar>` positions.
    pub(crate) fn print_type_parameter_decl(
        &self,
        param_info: &tsz_solver::types::TypeParamInfo,
    ) -> String {
        let mut result = String::new();

        if param_info.is_const {
            result.push_str("const ");
        }

        let param_type = self.interner.type_param(*param_info);
        result.push_str(&self.resolve_type_param_type_name(param_type, param_info.name));

        if let Some(constraint) = param_info.constraint {
            result.push_str(" extends ");
            result.push_str(&self.print_type(constraint));
        }

        if let Some(default) = param_info.default {
            result.push_str(" = ");
            result.push_str(&self.print_type(default));
        }

        result
    }

    pub(crate) fn print_lazy_type(&self, def_id: tsz_solver::def::DefId) -> String {
        // Check recursion depth
        if self.current_depth >= self.max_depth {
            return "any".to_string();
        }

        // Try to get the SymbolId for this DefId using TypeCache
        let sym_id = if let Some(cache) = self.type_cache {
            cache.def_to_symbol.get(&def_id).copied()
        } else {
            None
        };

        // If the symbol is a global lib type (e.g. Promise) that is NOT in
        // the current file's symbol arena (multi-file tests: each file has its
        // own binder without lib symbols merged), fall back to the pre-built
        // name map from TypeCache so we can still emit "Promise" instead of "any".
        if let Some(sym_id) = sym_id
            && self.symbol_arena.is_some_and(|a| a.get(sym_id).is_none())
        {
            if let Some(name) = self.type_cache.and_then(|c| c.def_to_name.get(&def_id)) {
                return name.clone();
            }
        }

        // If we have a symbol and it's visible/global, use the name. Otherwise
        // fall back to an import-qualified reference when the emitter can
        // resolve the owning module specifier.
        if let Some(sym_id) = sym_id
            && let Some(arena) = self.symbol_arena
            && let Some(symbol) = arena.get(sym_id)
        {
            // Lazy(DefId) for value-space entities (enums, modules, functions) represents
            // the VALUE side of the symbol. In .d.ts output, these must be prefixed with
            // `typeof` to distinguish from the type-side meaning.
            // E.g., `var x = MyEnum` → `declare var x: typeof MyEnum;`
            // The type-side meaning (e.g., enum member union) uses Enum(DefId, members)
            // and is handled by print_enum, not print_lazy_type.
            let needs_typeof = symbol.has_any_flags(
                symbol_flags::ENUM | symbol_flags::VALUE_MODULE | symbol_flags::FUNCTION,
            );
            if !needs_typeof
                && !self.symbol_is_import_qualifiable(sym_id)
                && !self.is_global_like_symbol(sym_id)
                && let Some(symbol_type) = self
                    .def_type_fallback(def_id)
                    .or_else(|| self.symbol_type_fallback(sym_id))
                && visitor::lazy_def_id(self.interner, symbol_type) != Some(def_id)
                && !self.type_contains_lazy_def(symbol_type, def_id, 0)
            {
                let mut nested = self.clone();
                nested.current_depth += 1;
                return nested.print_type(symbol_type);
            }
            if !needs_typeof
                && self.type_param_scope_contains_name(&symbol.escaped_name)
                && self.global_class_symbol_can_use_global_this(sym_id)
            {
                return format!("globalThis.{}", symbol.escaped_name);
            }
            if let Some(name) = self.print_named_symbol_reference(sym_id, needs_typeof) {
                return name;
            }
            // Preserve canonical names for global-like symbols when visibility
            // heuristics fail (e.g. utility aliases like `Extract`, `FlatArray`).
            if !needs_typeof && self.is_global_like_symbol(sym_id) {
                return symbol.escaped_name.clone();
            }
        }

        if let Some(name) = self
            .type_cache
            .and_then(|cache| cache.def_to_name.get(&def_id))
        {
            return name.clone();
        }

        // Symbol is not visible or we don't have symbol info.
        // Fallback to `any` when we cannot legally name the referenced type.
        "any".to_string()
    }

    pub(crate) fn type_contains_lazy_def(
        &self,
        type_id: TypeId,
        target_def: tsz_solver::def::DefId,
        depth: u32,
    ) -> bool {
        if depth > 64 {
            return true;
        }

        if visitor::lazy_def_id(self.interner, type_id) == Some(target_def) {
            return true;
        }

        if let Some(app_id) = visitor::application_id(self.interner, type_id) {
            let app = self.interner.type_application(app_id);
            return self.type_contains_lazy_def(app.base, target_def, depth + 1)
                || app
                    .args
                    .iter()
                    .copied()
                    .any(|arg| self.type_contains_lazy_def(arg, target_def, depth + 1));
        }

        if let Some(list_id) = visitor::union_list_id(self.interner, type_id)
            .or_else(|| visitor::intersection_list_id(self.interner, type_id))
        {
            return self
                .interner
                .type_list(list_id)
                .iter()
                .copied()
                .any(|member| self.type_contains_lazy_def(member, target_def, depth + 1));
        }

        if let Some(elem_id) = visitor::array_element_type(self.interner, type_id) {
            return self.type_contains_lazy_def(elem_id, target_def, depth + 1);
        }

        if let Some(tuple_id) = visitor::tuple_list_id(self.interner, type_id) {
            return self
                .interner
                .tuple_list(tuple_id)
                .iter()
                .any(|elem| self.type_contains_lazy_def(elem.type_id, target_def, depth + 1));
        }

        if let Some(func_id) = visitor::function_shape_id(self.interner, type_id) {
            let func = self.interner.function_shape(func_id);
            return func.type_params.iter().any(|tp| {
                tp.constraint.is_some_and(|constraint| {
                    self.type_contains_lazy_def(constraint, target_def, depth + 1)
                }) || tp.default.is_some_and(|default| {
                    self.type_contains_lazy_def(default, target_def, depth + 1)
                })
            }) || func
                .params
                .iter()
                .any(|param| self.type_contains_lazy_def(param.type_id, target_def, depth + 1))
                || func.type_predicate.as_ref().is_some_and(|pred| {
                    pred.type_id.is_some_and(|type_id| {
                        self.type_contains_lazy_def(type_id, target_def, depth + 1)
                    })
                })
                || self.type_contains_lazy_def(func.return_type, target_def, depth + 1);
        }

        if let Some(callable_id) = visitor::callable_shape_id(self.interner, type_id) {
            let callable = self.interner.callable_shape(callable_id);
            return callable
                .call_signatures
                .iter()
                .chain(callable.construct_signatures.iter())
                .any(|sig| {
                    sig.type_params.iter().any(|tp| {
                        tp.constraint.is_some_and(|constraint| {
                            self.type_contains_lazy_def(constraint, target_def, depth + 1)
                        }) || tp.default.is_some_and(|default| {
                            self.type_contains_lazy_def(default, target_def, depth + 1)
                        })
                    }) || sig.params.iter().any(|param| {
                        self.type_contains_lazy_def(param.type_id, target_def, depth + 1)
                    }) || sig.type_predicate.as_ref().is_some_and(|pred| {
                        pred.type_id.is_some_and(|type_id| {
                            self.type_contains_lazy_def(type_id, target_def, depth + 1)
                        })
                    }) || self.type_contains_lazy_def(sig.return_type, target_def, depth + 1)
                })
                || callable.properties.iter().any(|prop| {
                    self.type_contains_lazy_def(prop.type_id, target_def, depth + 1)
                        || (prop.write_type != TypeId::UNDEFINED
                            && self.type_contains_lazy_def(prop.write_type, target_def, depth + 1))
                })
                || callable.string_index.as_ref().is_some_and(|idx| {
                    self.type_contains_lazy_def(idx.value_type, target_def, depth + 1)
                })
                || callable.number_index.as_ref().is_some_and(|idx| {
                    self.type_contains_lazy_def(idx.value_type, target_def, depth + 1)
                });
        }

        if let Some(shape_id) = visitor::object_shape_id(self.interner, type_id)
            .or_else(|| visitor::object_with_index_shape_id(self.interner, type_id))
        {
            let shape = self.interner.object_shape(shape_id);
            return shape.properties.iter().any(|prop| {
                self.type_contains_lazy_def(prop.type_id, target_def, depth + 1)
                    || (prop.write_type != TypeId::UNDEFINED
                        && self.type_contains_lazy_def(prop.write_type, target_def, depth + 1))
            }) || shape.string_index.as_ref().is_some_and(|idx| {
                self.type_contains_lazy_def(idx.value_type, target_def, depth + 1)
            }) || shape.number_index.as_ref().is_some_and(|idx| {
                self.type_contains_lazy_def(idx.value_type, target_def, depth + 1)
            });
        }

        if let Some(cond_id) = visitor::conditional_type_id(self.interner, type_id) {
            let cond = self.interner.conditional_type(cond_id);
            return self.type_contains_lazy_def(cond.check_type, target_def, depth + 1)
                || self.type_contains_lazy_def(cond.extends_type, target_def, depth + 1)
                || self.type_contains_lazy_def(cond.true_type, target_def, depth + 1)
                || self.type_contains_lazy_def(cond.false_type, target_def, depth + 1);
        }

        if let Some(template_id) = visitor::template_literal_id(self.interner, type_id) {
            return self
                .interner
                .template_list(template_id)
                .iter()
                .any(|span| match span {
                    tsz_solver::types::TemplateSpan::Text(_) => false,
                    tsz_solver::types::TemplateSpan::Type(inner) => {
                        self.type_contains_lazy_def(*inner, target_def, depth + 1)
                    }
                });
        }

        if let Some(mapped_id) = visitor::mapped_type_id(self.interner, type_id) {
            let mapped = self.interner.mapped_type(mapped_id);
            return mapped.type_param.constraint.is_some_and(|constraint| {
                self.type_contains_lazy_def(constraint, target_def, depth + 1)
            }) || mapped.type_param.default.is_some_and(|default| {
                self.type_contains_lazy_def(default, target_def, depth + 1)
            }) || self.type_contains_lazy_def(mapped.constraint, target_def, depth + 1)
                || self.type_contains_lazy_def(mapped.template, target_def, depth + 1)
                || mapped.name_type.is_some_and(|name_type| {
                    self.type_contains_lazy_def(name_type, target_def, depth + 1)
                });
        }

        if let Some((container, index)) = visitor::index_access_parts(self.interner, type_id) {
            return self.type_contains_lazy_def(container, target_def, depth + 1)
                || self.type_contains_lazy_def(index, target_def, depth + 1);
        }

        if let Some(inner) = visitor::keyof_inner_type(self.interner, type_id)
            .or_else(|| visitor::readonly_inner_type(self.interner, type_id))
            .or_else(|| visitor::no_infer_inner_type(self.interner, type_id))
        {
            return self.type_contains_lazy_def(inner, target_def, depth + 1);
        }

        if let Some((_kind, inner)) = visitor::string_intrinsic_components(self.interner, type_id) {
            return self.type_contains_lazy_def(inner, target_def, depth + 1);
        }

        false
    }
    pub(crate) fn type_contains_symbol_reference(
        &self,
        type_id: TypeId,
        target_sym: SymbolId,
        depth: u32,
    ) -> bool {
        if depth > 64 {
            return true;
        }

        if visitor::type_query_symbol(self.interner, type_id)
            .is_some_and(|sym_ref| sym_ref.0 == target_sym.0)
        {
            return true;
        }

        if let Some(def_id) = visitor::lazy_def_id(self.interner, type_id)
            && self
                .type_cache
                .and_then(|cache| cache.def_to_symbol.get(&def_id))
                .is_some_and(|&sym_id| sym_id == target_sym)
        {
            return true;
        }

        if visitor::object_shape_id(self.interner, type_id)
            .or_else(|| visitor::object_with_index_shape_id(self.interner, type_id))
            .and_then(|shape_id| self.interner.object_shape(shape_id).symbol)
            .is_some_and(|sym_id| sym_id == target_sym)
        {
            return true;
        }

        if let Some(app_id) = visitor::application_id(self.interner, type_id) {
            let app = self.interner.type_application(app_id);
            return self.type_contains_symbol_reference(app.base, target_sym, depth + 1)
                || app
                    .args
                    .iter()
                    .copied()
                    .any(|arg| self.type_contains_symbol_reference(arg, target_sym, depth + 1));
        }

        if let Some(shape_id) = visitor::object_shape_id(self.interner, type_id)
            .or_else(|| visitor::object_with_index_shape_id(self.interner, type_id))
        {
            let shape = self.interner.object_shape(shape_id);
            return shape.properties.iter().any(|property| {
                self.type_contains_symbol_reference(property.type_id, target_sym, depth + 1)
            }) || shape.string_index.is_some_and(|index_info| {
                self.type_contains_symbol_reference(index_info.key_type, target_sym, depth + 1)
                    || self.type_contains_symbol_reference(
                        index_info.value_type,
                        target_sym,
                        depth + 1,
                    )
            }) || shape.number_index.is_some_and(|index_info| {
                self.type_contains_symbol_reference(index_info.key_type, target_sym, depth + 1)
                    || self.type_contains_symbol_reference(
                        index_info.value_type,
                        target_sym,
                        depth + 1,
                    )
            });
        }

        if let Some(type_list_id) = visitor::union_list_id(self.interner, type_id)
            .or_else(|| visitor::intersection_list_id(self.interner, type_id))
        {
            return self
                .interner
                .type_list(type_list_id)
                .iter()
                .copied()
                .any(|member| self.type_contains_symbol_reference(member, target_sym, depth + 1));
        }

        if let Some(elem_id) = visitor::array_element_type(self.interner, type_id) {
            return self.type_contains_symbol_reference(elem_id, target_sym, depth + 1);
        }

        if let Some(tuple_id) = visitor::tuple_list_id(self.interner, type_id) {
            return self.interner.tuple_list(tuple_id).iter().any(|member| {
                self.type_contains_symbol_reference(member.type_id, target_sym, depth + 1)
            });
        }

        if let Some(func_id) = visitor::function_shape_id(self.interner, type_id) {
            return self.function_shape_contains_symbol_reference(func_id, target_sym, depth + 1);
        }

        if let Some(callable_id) = visitor::callable_shape_id(self.interner, type_id) {
            let callable = self.interner.callable_shape(callable_id);
            return callable.call_signatures.iter().any(|sig| {
                self.call_signature_contains_symbol_reference(sig, target_sym, depth + 1)
            }) || callable.construct_signatures.iter().any(|sig| {
                self.call_signature_contains_symbol_reference(sig, target_sym, depth + 1)
            }) || callable.properties.iter().any(|property| {
                self.type_contains_symbol_reference(property.type_id, target_sym, depth + 1)
            }) || callable.string_index.is_some_and(|index_info| {
                self.type_contains_symbol_reference(index_info.key_type, target_sym, depth + 1)
                    || self.type_contains_symbol_reference(
                        index_info.value_type,
                        target_sym,
                        depth + 1,
                    )
            }) || callable.number_index.is_some_and(|index_info| {
                self.type_contains_symbol_reference(index_info.key_type, target_sym, depth + 1)
                    || self.type_contains_symbol_reference(
                        index_info.value_type,
                        target_sym,
                        depth + 1,
                    )
            });
        }

        if let Some(cond_id) = visitor::conditional_type_id(self.interner, type_id) {
            let cond = self.interner.conditional_type(cond_id);
            return self.type_contains_symbol_reference(cond.check_type, target_sym, depth + 1)
                || self.type_contains_symbol_reference(cond.extends_type, target_sym, depth + 1)
                || self.type_contains_symbol_reference(cond.true_type, target_sym, depth + 1)
                || self.type_contains_symbol_reference(cond.false_type, target_sym, depth + 1);
        }

        if let Some(template_id) = visitor::template_literal_id(self.interner, type_id) {
            return self
                .interner
                .template_list(template_id)
                .iter()
                .any(|span| matches!(span, tsz_solver::types::TemplateSpan::Type(inner) if self.type_contains_symbol_reference(*inner, target_sym, depth + 1)));
        }

        if let Some(mapped_id) = visitor::mapped_type_id(self.interner, type_id) {
            let mapped = self.interner.mapped_type(mapped_id);
            return self.type_contains_symbol_reference(mapped.constraint, target_sym, depth + 1)
                || self.type_contains_symbol_reference(mapped.template, target_sym, depth + 1)
                || mapped.name_type.is_some_and(|name_type| {
                    self.type_contains_symbol_reference(name_type, target_sym, depth + 1)
                })
                || mapped.type_param.constraint.is_some_and(|constraint| {
                    self.type_contains_symbol_reference(constraint, target_sym, depth + 1)
                })
                || mapped.type_param.default.is_some_and(|default| {
                    self.type_contains_symbol_reference(default, target_sym, depth + 1)
                });
        }

        if let Some((container, index)) = visitor::index_access_parts(self.interner, type_id) {
            return self.type_contains_symbol_reference(container, target_sym, depth + 1)
                || self.type_contains_symbol_reference(index, target_sym, depth + 1);
        }

        if let Some(inner) = visitor::keyof_inner_type(self.interner, type_id)
            .or_else(|| visitor::readonly_inner_type(self.interner, type_id))
            .or_else(|| visitor::no_infer_inner_type(self.interner, type_id))
        {
            return self.type_contains_symbol_reference(inner, target_sym, depth + 1);
        }

        false
    }

    pub(crate) fn function_shape_contains_symbol_reference(
        &self,
        func_id: tsz_solver::types::FunctionShapeId,
        target_sym: SymbolId,
        depth: u32,
    ) -> bool {
        let func = self.interner.function_shape(func_id);
        func.params
            .iter()
            .any(|param| self.type_contains_symbol_reference(param.type_id, target_sym, depth + 1))
            || self.type_contains_symbol_reference(func.return_type, target_sym, depth + 1)
            || func.this_type.is_some_and(|this_type| {
                self.type_contains_symbol_reference(this_type, target_sym, depth + 1)
            })
            || func.type_params.iter().any(|param| {
                param.constraint.is_some_and(|constraint| {
                    self.type_contains_symbol_reference(constraint, target_sym, depth + 1)
                }) || param.default.is_some_and(|default| {
                    self.type_contains_symbol_reference(default, target_sym, depth + 1)
                })
            })
    }

    pub(crate) fn call_signature_contains_symbol_reference(
        &self,
        signature: &tsz_solver::types::CallSignature,
        target_sym: SymbolId,
        depth: u32,
    ) -> bool {
        signature
            .params
            .iter()
            .any(|param| self.type_contains_symbol_reference(param.type_id, target_sym, depth + 1))
            || self.type_contains_symbol_reference(signature.return_type, target_sym, depth + 1)
            || signature.this_type.is_some_and(|this_type| {
                self.type_contains_symbol_reference(this_type, target_sym, depth + 1)
            })
            || signature.type_params.iter().any(|param| {
                param.constraint.is_some_and(|constraint| {
                    self.type_contains_symbol_reference(constraint, target_sym, depth + 1)
                }) || param.default.is_some_and(|default| {
                    self.type_contains_symbol_reference(default, target_sym, depth + 1)
                })
            })
    }

    /// Check if a symbol is a global (ambient) type that's always accessible.
    /// Global types like Object, Array, Function, etc. have no parent symbol
    /// (parent == `SymbolId::NONE`) and are always referenceable in declarations.
    pub(crate) fn is_global_symbol(&self, sym_id: SymbolId) -> bool {
        let Some(arena) = self.symbol_arena else {
            return false;
        };
        let Some(symbol) = arena.get(sym_id) else {
            return false;
        };
        symbol.declarations.is_empty()
            && !symbol.parent.is_some()
            && self.resolve_symbol_module_path(sym_id).is_none()
            && !(symbol.has_any_flags(symbol_flags::ALIAS) && symbol.import_module.is_some())
    }

    pub(crate) fn intersection_member_priority(&self, type_id: TypeId) -> u8 {
        if let Some(app_id) = visitor::application_id(self.interner, type_id) {
            let app = self.interner.type_application(app_id);
            if self.type_reference_base_is_nameable(app.base) {
                return 0;
            }
            return 1;
        }

        if visitor::type_param_info(self.interner, type_id).is_some() {
            return 2;
        }

        if let Some(sym_ref) = visitor::type_query_symbol(self.interner, type_id) {
            let sym_id = SymbolId(sym_ref.0);
            return u8::from(self.is_symbol_visible(sym_id) || self.symbol_is_nameable(sym_id));
        }

        if let Some(callable_id) = visitor::callable_shape_id(self.interner, type_id) {
            let callable = self.interner.callable_shape(callable_id);
            if let Some(sym_id) = callable.symbol {
                return u8::from(self.is_symbol_visible(sym_id) || self.symbol_is_nameable(sym_id));
            }
            return 0;
        }

        if let Some(shape_id) = visitor::object_shape_id(self.interner, type_id)
            .or_else(|| visitor::object_with_index_shape_id(self.interner, type_id))
        {
            let shape = self.interner.object_shape(shape_id);
            if let Some(sym_id) = shape.symbol {
                return u8::from(self.is_symbol_visible(sym_id) || self.symbol_is_nameable(sym_id));
            }
            return 1;
        }

        1
    }
}
