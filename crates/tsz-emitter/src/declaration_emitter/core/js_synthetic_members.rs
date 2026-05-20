use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::type_queries;

use super::DeclarationEmitter;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn emit_js_synthetic_prototype_class_if_needed(
        &mut self,
        name_idx: NodeIndex,
        is_exported: bool,
    ) {
        let Some(name) = self.get_identifier_text(name_idx) else {
            return;
        };
        let Some(methods) = self
            .js_deferred_prototype_method_statements
            .get(&name)
            .cloned()
        else {
            return;
        };
        if methods.is_empty() {
            return;
        }

        self.write_indent();
        if is_exported {
            self.write("export ");
        }
        if self.should_emit_declare_keyword(is_exported) {
            self.write("declare ");
        }
        self.write("class ");
        self.emit_node(name_idx);
        self.write(" {");
        self.write_line();
        self.increase_indent();

        for (method_name, initializer) in methods {
            self.emit_js_synthetic_class_method(method_name, initializer);
        }

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();
    }

    pub(in crate::declaration_emitter) fn emit_js_synthetic_class_method(
        &mut self,
        name_idx: NodeIndex,
        initializer: NodeIndex,
    ) {
        let Some(init_node) = self.arena.get(initializer) else {
            return;
        };
        if init_node.kind != syntax_kind_ext::ARROW_FUNCTION
            && init_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
        {
            return;
        }
        let Some(func) = self.arena.get_function(init_node) else {
            return;
        };

        let jsdoc = self.function_like_jsdoc_for_node(initializer);

        if let Some(jsdoc) = jsdoc.as_deref() {
            self.emit_multiline_jsdoc_comment(jsdoc);
        }
        self.write_indent();
        self.emit_node(name_idx);

        let jsdoc_template_params = if func
            .type_parameters
            .as_ref()
            .is_none_or(|type_params| type_params.nodes.is_empty())
        {
            jsdoc
                .as_deref()
                .map(Self::parse_jsdoc_template_params)
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        if let Some(ref type_params) = func.type_parameters {
            if !type_params.nodes.is_empty() {
                self.emit_type_parameters(type_params);
            } else if !jsdoc_template_params.is_empty() {
                self.emit_jsdoc_template_parameters(&jsdoc_template_params);
            }
        } else if !jsdoc_template_params.is_empty() {
            self.emit_jsdoc_template_parameters(&jsdoc_template_params);
        }

        self.write("(");
        self.emit_parameters_with_body(&func.parameters, func.body);
        self.write(")");

        if func.type_annotation.is_some() {
            self.write(": ");
            self.emit_type(func.type_annotation);
        } else if let Some(return_type_text) = jsdoc
            .as_deref()
            .and_then(Self::parse_jsdoc_return_type_text)
        {
            self.write(": ");
            self.write(&return_type_text);
        } else if let (Some(interner), Some(cache)) = (&self.type_interner, &self.type_cache) {
            let func_type_id = cache
                .node_types
                .get(&initializer.0)
                .copied()
                .or_else(|| self.get_node_type_or_names(&[name_idx, initializer]));
            if let Some(func_type_id) = func_type_id
                && let Some(return_type_id) = type_queries::get_return_type(*interner, func_type_id)
            {
                if return_type_id == tsz_solver::types::TypeId::ANY
                    && func.body.is_some()
                    && self.body_returns_void(func.body)
                {
                    self.write(": void");
                } else {
                    self.write(": ");
                    self.write(&self.print_type_id(return_type_id));
                }
            } else {
                self.emit_js_body_return_annotation(func.body);
            }
        } else {
            self.emit_js_body_return_annotation(func.body);
        }

        self.write(";");
        self.write_line();
    }
}
