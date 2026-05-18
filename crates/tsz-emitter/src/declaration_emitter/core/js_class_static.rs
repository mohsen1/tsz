use rustc_hash::FxHashSet;
use tsz_parser::parser::node::FunctionData;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};

use super::DeclarationEmitter;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn emit_js_class_like_prototype_members_for_declared_class(
        &mut self,
        name_idx: NodeIndex,
        class_members: &NodeList,
    ) {
        let Some(name) = self.get_identifier_text(name_idx) else {
            return;
        };
        let Some(methods) = self.js_class_like_prototype_members.get(&name).cloned() else {
            return;
        };

        let mut declared_names = class_members
            .nodes
            .iter()
            .filter_map(|&member_idx| self.get_member_name_idx(member_idx))
            .filter_map(|member_name_idx| self.get_identifier_text(member_name_idx))
            .collect::<FxHashSet<_>>();
        for (method_name, initializer) in methods {
            let Some(method_name_text) = self.get_identifier_text(method_name) else {
                continue;
            };
            if !declared_names.insert(method_name_text) {
                continue;
            }
            self.emit_js_synthetic_class_method(method_name, initializer);
        }
    }

    pub(in crate::declaration_emitter) fn emit_js_class_static_members_namespace(
        &mut self,
        name_idx: NodeIndex,
        is_exported: bool,
    ) {
        let Some(name) = self.get_identifier_text(name_idx) else {
            return;
        };
        let Some(members) = self.js_class_static_members.get(&name).cloned() else {
            return;
        };
        if members.is_empty() {
            return;
        }

        let members = members
            .into_iter()
            .filter_map(|(member_name, initializer)| {
                self.get_identifier_text(member_name)
                    .map(|member_text| (member_name, member_text, initializer))
            })
            .collect::<Vec<_>>();
        if members.is_empty() {
            return;
        }

        self.write_indent();
        if is_exported {
            self.write("export ");
        }
        if self.should_emit_declare_keyword(is_exported) {
            self.write("declare ");
        }
        self.write("namespace ");
        self.emit_node(name_idx);
        self.write(" {");
        self.write_line();
        self.increase_indent();

        let mut reserved_member_names = members
            .iter()
            .map(|(_, member_text, _)| member_text.clone())
            .collect::<FxHashSet<_>>();
        let mut emitted_keyword_export_alias = false;
        let mut planned_members = Vec::with_capacity(members.len());
        for (_member_name, member_text, initializer) in members {
            let (local_name, export_alias) = self.js_static_namespace_member_local_name(
                &member_text,
                &mut reserved_member_names,
                emitted_keyword_export_alias,
            );
            emitted_keyword_export_alias |= export_alias
                .as_ref()
                .is_some_and(|_| Self::is_js_static_reserved_binding_name(&member_text));
            planned_members.push((initializer, local_name, export_alias));
        }

        let has_export_aliases = planned_members
            .iter()
            .any(|(_, _, export_alias)| export_alias.is_some());
        for (initializer, local_name, export_alias) in planned_members {
            let emit_export = export_alias.is_none() && has_export_aliases;
            if let Some(init_node) = self.arena.get(initializer) {
                if init_node.kind == syntax_kind_ext::ARROW_FUNCTION
                    || init_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                {
                    if let Some(func) = self.arena.get_function(init_node) {
                        if let Some(jsdoc) = self.function_like_jsdoc_for_node(initializer) {
                            self.emit_multiline_jsdoc_comment(&jsdoc);
                        }
                        self.emit_js_namespace_function_member_text(
                            &local_name,
                            func,
                            initializer,
                            emit_export,
                        );
                    }
                } else if let Some(type_text) =
                    self.js_namespace_value_member_type_text(initializer)
                {
                    self.emit_js_namespace_value_member_text(&local_name, &type_text, emit_export);
                }
            }
            if let Some((local_name, exported_name)) = export_alias {
                self.write_indent();
                self.write("export { ");
                self.write(&local_name);
                self.write(" as ");
                self.write(&exported_name);
                self.write(" };");
                self.write_line();
            }
        }

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();
    }

    fn emit_js_namespace_function_member_text(
        &mut self,
        name: &str,
        func: &FunctionData,
        initializer: NodeIndex,
        emit_export: bool,
    ) {
        self.write_indent();
        if emit_export {
            self.write("export ");
        }
        self.write("function ");
        self.write(name);
        if let Some(type_params) = func.type_parameters.as_ref()
            && !type_params.nodes.is_empty()
        {
            self.emit_type_parameters(type_params);
        }
        self.write("(");
        self.emit_parameters_with_body(&func.parameters, func.body);
        self.write(")");
        if func.type_annotation.is_some() {
            self.write(": ");
            self.emit_type(func.type_annotation);
        } else if let Some(return_type_text) = self.jsdoc_return_type_text_for_node(initializer) {
            self.write(": ");
            self.write(&return_type_text);
        } else if func.body.is_some() && self.body_returns_void(func.body) {
            self.write(": void");
        } else if !self.source_is_declaration_file {
            self.write(": any");
        }
        self.write(";");
        self.write_line();
    }

    fn emit_js_namespace_value_member_text(
        &mut self,
        name: &str,
        type_text: &str,
        emit_export: bool,
    ) {
        self.write_indent();
        if emit_export {
            self.write("export ");
        }
        self.write("let ");
        self.write(name);
        self.write(": ");
        self.write(type_text);
        self.write(";");
        self.write_line();
    }

    fn js_static_namespace_member_local_name(
        &self,
        member_text: &str,
        reserved_member_names: &mut FxHashSet<String>,
        emitted_keyword_export_alias: bool,
    ) -> (String, Option<(String, String)>) {
        let needs_alias = Self::is_js_static_reserved_binding_name(member_text)
            || self.reserved_names.contains(member_text)
            || emitted_keyword_export_alias
                && !Self::is_js_static_contextual_keyword_property_name(member_text);
        if !needs_alias {
            return (member_text.to_string(), None);
        }

        let local_name = if Self::is_js_static_reserved_binding_name(member_text) {
            Self::js_static_synthetic_member_name(member_text, reserved_member_names)
        } else {
            let local_name =
                self.generate_js_static_unique_member_name(member_text, reserved_member_names);
            reserved_member_names.insert(local_name.clone());
            local_name
        };
        (
            local_name.clone(),
            Some((local_name, member_text.to_string())),
        )
    }

    fn js_static_synthetic_member_name(
        property_name_text: &str,
        reserved_member_names: &mut FxHashSet<String>,
    ) -> String {
        let base = format!("_{property_name_text}");
        if reserved_member_names.insert(base.clone()) {
            return base;
        }

        let mut suffix = 1usize;
        loop {
            let candidate = format!("{base}_{suffix}");
            if reserved_member_names.insert(candidate.clone()) {
                return candidate;
            }
            suffix += 1;
        }
    }

    fn generate_js_static_unique_member_name(
        &self,
        base: &str,
        reserved_member_names: &FxHashSet<String>,
    ) -> String {
        let mut i = 1usize;
        loop {
            let candidate = format!("{base}_{i}");
            if !self.reserved_names.contains(&candidate)
                && !reserved_member_names.contains(&candidate)
            {
                return candidate;
            }
            i += 1;
        }
    }

    fn is_js_static_reserved_binding_name(text: &str) -> bool {
        matches!(
            text,
            "break"
                | "case"
                | "catch"
                | "class"
                | "const"
                | "continue"
                | "debugger"
                | "default"
                | "delete"
                | "do"
                | "else"
                | "enum"
                | "export"
                | "extends"
                | "false"
                | "finally"
                | "for"
                | "function"
                | "if"
                | "import"
                | "in"
                | "instanceof"
                | "new"
                | "null"
                | "return"
                | "super"
                | "switch"
                | "this"
                | "throw"
                | "true"
                | "try"
                | "typeof"
                | "var"
                | "void"
                | "while"
                | "with"
                | "implements"
                | "interface"
                | "let"
                | "package"
                | "private"
                | "protected"
                | "public"
                | "static"
                | "yield"
        )
    }

    fn is_js_static_contextual_keyword_property_name(text: &str) -> bool {
        matches!(
            text,
            "abstract"
                | "as"
                | "asserts"
                | "any"
                | "async"
                | "await"
                | "boolean"
                | "constructor"
                | "declare"
                | "get"
                | "infer"
                | "is"
                | "keyof"
                | "module"
                | "namespace"
                | "never"
                | "readonly"
                | "require"
                | "number"
                | "object"
                | "set"
                | "string"
                | "symbol"
                | "type"
                | "undefined"
                | "unique"
                | "unknown"
                | "from"
                | "global"
                | "bigint"
                | "of"
        )
    }

    pub(in crate::declaration_emitter) fn emit_js_function_typed_property(
        &mut self,
        name_idx: NodeIndex,
        initializer: NodeIndex,
    ) -> bool {
        let Some(init_node) = self.arena.get(initializer) else {
            return false;
        };
        if init_node.kind != syntax_kind_ext::ARROW_FUNCTION
            && init_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
        {
            return false;
        }
        let Some(func) = self.arena.get_function(init_node) else {
            return false;
        };

        let jsdoc = self.function_like_jsdoc_for_node(initializer);
        if let Some(jsdoc) = jsdoc.as_deref() {
            self.emit_multiline_jsdoc_comment(jsdoc);
        }
        self.write_indent();
        self.emit_node(name_idx);
        self.write(": ");
        self.emit_function_initializer_signature(func);
        if func.type_annotation.is_some() {
            self.emit_type(func.type_annotation);
        } else if let Some(return_type_text) = jsdoc
            .as_deref()
            .and_then(Self::parse_jsdoc_return_type_text)
        {
            self.write(&return_type_text);
        } else if let Some(return_type_text) = self
            .js_function_body_preferred_return_text_for_declaration(
                func.body,
                initializer,
                &func.parameters,
            )
        {
            self.write(&return_type_text);
        } else if func.body.is_some() && self.body_returns_void(func.body) {
            self.write("void");
        } else {
            self.write("any");
        }
        self.write(";");
        self.write_line();
        true
    }
}
