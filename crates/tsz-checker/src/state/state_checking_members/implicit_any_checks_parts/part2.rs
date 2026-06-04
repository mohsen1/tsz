impl<'a> CheckerState<'a> {
    /// Re-check closures that deferred TS7006 during type env building.
    /// Called after `is_checking_statements` is set to true. These closures were
    /// processed before statement-checking mode, so their `skip_implicit_any` was
    /// true. Their cached types prevent `get_type_of_function` from re-running,
    /// so we explicitly walk their parameters and emit TS7006 here.
    pub(crate) fn recheck_deferred_implicit_any_closures(&mut self) {
        let deferred = std::mem::take(&mut self.ctx.deferred_implicit_any_closures);
        for func_idx in deferred {
            if self.closure_has_contextual_type(func_idx) {
                continue;
            }
            // Skip closures with JSDoc annotations — JSDoc @param, @type, @template
            // etc. can provide type information that suppresses TS7006. The normal
            // get_type_of_function path handles this; we conservatively skip here.
            if self.find_jsdoc_for_function(func_idx).is_some() {
                continue;
            }
            let Some(node) = self.ctx.arena.get(func_idx) else {
                continue;
            };
            let parameters = if let Some(func) = self.ctx.arena.get_function(node) {
                &func.parameters
            } else if let Some(method) = self.ctx.arena.get_method_decl(node) {
                &method.parameters
            } else if let Some(accessor) = self.ctx.arena.get_accessor(node) {
                &accessor.parameters
            } else {
                continue;
            };
            let param_nodes: Vec<_> = parameters.nodes.clone();
            let mut param_index = 0;
            for &param_idx in &param_nodes {
                let Some(param_node) = self.ctx.arena.get(param_idx) else {
                    continue;
                };
                let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                    continue;
                };
                // Skip `this` parameter
                if let Some(name_node) = self.ctx.arena.get(param.name)
                    && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                    && ident.escaped_text.as_str() == "this"
                {
                    continue;
                }
                // Skip parameters with type annotations
                if param.type_annotation.is_some() {
                    param_index += 1;
                    continue;
                }
                self.maybe_report_implicit_any_parameter(param, false, param_index);
                param_index += 1;
            }
            self.ctx.implicit_any_checked_closures.insert(func_idx);
        }

        // Re-check closures whose TS7006 was emitted during return-type inference
        // speculation and then rolled back. These closures had genuinely untyped
        // parameters at the time of first processing (inside infer_return_type_from_body).
        // Even if a later call inference retry provided contextual types (adding the
        // closure to implicit_any_contextual_closures), tsc would have kept the TS7006
        // from the initial inference pass. So we unconditionally re-emit here.
        let speculative = std::mem::take(&mut self.ctx.speculative_implicit_any_closures);
        for func_idx in speculative {
            if self.find_jsdoc_for_function(func_idx).is_some() {
                continue;
            }
            let Some(node) = self.ctx.arena.get(func_idx) else {
                continue;
            };
            let parameters = if let Some(func) = self.ctx.arena.get_function(node) {
                &func.parameters
            } else if let Some(method) = self.ctx.arena.get_method_decl(node) {
                &method.parameters
            } else if let Some(accessor) = self.ctx.arena.get_accessor(node) {
                &accessor.parameters
            } else {
                continue;
            };
            let param_nodes: Vec<_> = parameters.nodes.clone();
            let mut param_index = 0;
            for &param_idx in &param_nodes {
                let Some(param_node) = self.ctx.arena.get(param_idx) else {
                    continue;
                };
                let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                    continue;
                };
                if let Some(name_node) = self.ctx.arena.get(param.name)
                    && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                    && ident.escaped_text.as_str() == "this"
                {
                    continue;
                }
                if param.type_annotation.is_some() {
                    param_index += 1;
                    continue;
                }
                self.maybe_report_implicit_any_parameter(param, false, param_index);
                param_index += 1;
            }
        }
    }

    /// Walk a type annotation looking for `FunctionType`/`ConstructorType` nodes and
    /// emit TS7006/TS7019 for any parameters that lack explicit type annotations when
    /// `--noImplicitAny` is enabled.
    ///
    /// Called for class property type annotations in ambient (declare) classes, where
    /// `check_type_for_missing_names` is not invoked because there is no initializer.
    /// Example: `pub_f10: (x) => string` — tsc emits TS7006 for `x`.
    pub(crate) fn check_type_annotation_for_implicit_any_params(&mut self, type_idx: NodeIndex) {
        use tsz_parser::parser::syntax_kind_ext;
        let Some(node) = self.ctx.arena.get(type_idx) else {
            return;
        };
        match node.kind {
            k if k == syntax_kind_ext::FUNCTION_TYPE || k == syntax_kind_ext::CONSTRUCTOR_TYPE => {
                if let Some(func_type) = self.ctx.arena.get_function_type(node) {
                    for (pi, &param_idx) in func_type.parameters.nodes.iter().enumerate() {
                        if let Some(param_node) = self.ctx.arena.get(param_idx)
                            && let Some(param) = self.ctx.arena.get_parameter(param_node)
                        {
                            self.maybe_report_implicit_any_parameter(param, false, pi);
                        }
                    }
                    // Recurse into return type for nested function types like `() => (x) => void`
                    if func_type.type_annotation.is_some() {
                        self.check_type_annotation_for_implicit_any_params(
                            func_type.type_annotation,
                        );
                    }
                }
            }
            k if k == syntax_kind_ext::UNION_TYPE || k == syntax_kind_ext::INTERSECTION_TYPE => {
                if let Some(composite) = self.ctx.arena.get_composite_type(node) {
                    for &member_idx in composite.types.nodes.clone().iter() {
                        self.check_type_annotation_for_implicit_any_params(member_idx);
                    }
                }
            }
            k if k == syntax_kind_ext::ARRAY_TYPE => {
                if let Some(arr) = self.ctx.arena.get_array_type(node) {
                    self.check_type_annotation_for_implicit_any_params(arr.element_type);
                }
            }
            k if k == syntax_kind_ext::OPTIONAL_TYPE
                || k == syntax_kind_ext::REST_TYPE
                || k == syntax_kind_ext::PARENTHESIZED_TYPE =>
            {
                if let Some(wrapped) = self.ctx.arena.get_wrapped_type(node) {
                    self.check_type_annotation_for_implicit_any_params(wrapped.type_node);
                }
            }
            _ => {}
        }
    }
}
