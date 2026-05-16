use crate::emitter::Printer;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

pub(in crate::emitter) struct SimpleNamespaceBindingExport {
    export_name: String,
    access: NamespaceBindingExportAccess,
}

enum NamespaceBindingExportAccess {
    Property(String),
    Element(usize),
}

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn simple_namespace_binding_export(
        &self,
        pattern_idx: NodeIndex,
    ) -> Option<SimpleNamespaceBindingExport> {
        let pattern_node = self.arena.get(pattern_idx)?;
        if pattern_node.kind != syntax_kind_ext::ARRAY_BINDING_PATTERN
            && pattern_node.kind != syntax_kind_ext::OBJECT_BINDING_PATTERN
        {
            return None;
        }
        let pattern = self.arena.get_binding_pattern(pattern_node)?;

        let mut found = None;
        for (index, &element_idx) in pattern.elements.nodes.iter().enumerate() {
            let element_node = self.arena.get(element_idx)?;
            if element_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                continue;
            }
            let element = self.arena.get_binding_element(element_node)?;
            if element.dot_dot_dot_token || found.is_some() {
                return None;
            }
            let export_name = self.binding_element_export_name(element.name)?;
            let access = if pattern_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN {
                NamespaceBindingExportAccess::Element(index)
            } else {
                let prop_idx = if element.property_name.is_some() {
                    element.property_name
                } else {
                    element.name
                };
                NamespaceBindingExportAccess::Property(self.binding_element_export_name(prop_idx)?)
            };
            found = Some(SimpleNamespaceBindingExport {
                export_name,
                access,
            });
        }
        found
    }

    pub(in crate::emitter) fn emit_simple_namespace_binding_export(
        &mut self,
        ns_name: &str,
        initializer: NodeIndex,
        binding: &SimpleNamespaceBindingExport,
        wrote_any: &mut bool,
    ) {
        self.write_namespace_export_separator(wrote_any);
        self.write(ns_name);
        self.write(".");
        self.write(&binding.export_name);
        self.write(" = ");
        self.emit_expression(initializer);
        match &binding.access {
            NamespaceBindingExportAccess::Property(prop_name) => {
                if self
                    .arena
                    .get(initializer)
                    .is_some_and(|n| n.is_numeric_literal())
                {
                    self.write(".");
                }
                self.write(".");
                self.write(prop_name);
            }
            NamespaceBindingExportAccess::Element(index) => {
                self.write("[");
                self.write(&index.to_string());
                self.write("]");
            }
        }
    }

    pub(in crate::emitter) fn can_inline_simple_namespace_binding_initializer(
        &self,
        idx: NodeIndex,
    ) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };

        node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
            || node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            || node.is_string_literal()
            || node.is_numeric_literal()
    }

    pub(in crate::emitter) fn reserve_namespace_destructuring_export_temps(
        &mut self,
        module: &tsz_parser::parser::node::ModuleData,
    ) -> (rustc_hash::FxHashMap<NodeIndex, String>, Vec<String>) {
        let mut temps_by_decl = rustc_hash::FxHashMap::default();
        let mut temps = Vec::new();
        let Some(body_node) = self.arena.get(module.body) else {
            return (temps_by_decl, temps);
        };
        let Some(block) = self.arena.get_module_block(body_node) else {
            return (temps_by_decl, temps);
        };
        let Some(ref stmts) = block.statements else {
            return (temps_by_decl, temps);
        };

        for &stmt_idx in &stmts.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::EXPORT_DECLARATION {
                continue;
            }
            let Some(export) = self.arena.get_export_decl(stmt_node) else {
                continue;
            };
            let Some(var_node) = self.arena.get(export.export_clause) else {
                continue;
            };
            if var_node.kind != syntax_kind_ext::VARIABLE_STATEMENT {
                continue;
            }
            let Some(var_stmt) = self.arena.get_variable(var_node) else {
                continue;
            };
            for &decl_list_idx in &var_stmt.declarations.nodes {
                let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                    continue;
                };
                let Some(decl_list) = self.arena.get_variable(decl_list_node) else {
                    continue;
                };
                for &decl_idx in &decl_list.declarations.nodes {
                    let Some(decl_node) = self.arena.get(decl_idx) else {
                        continue;
                    };
                    let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                        continue;
                    };
                    if decl.initializer.is_none() {
                        continue;
                    }
                    if let Some(name_node) = self.arena.get(decl.name)
                        && name_node.is_binding_pattern()
                        && (self.simple_namespace_binding_export(decl.name).is_none()
                            || !self
                                .can_inline_simple_namespace_binding_initializer(decl.initializer))
                    {
                        let temp = self.get_temp_var_name();
                        temps_by_decl.insert(decl_idx, temp.clone());
                        temps.push(temp);
                    }
                }
            }
        }

        (temps_by_decl, temps)
    }

    pub(in crate::emitter) fn emit_namespace_binding_pattern_assignments(
        &mut self,
        ns_name: &str,
        source: &str,
        pattern_idx: NodeIndex,
        wrote_any: &mut bool,
    ) {
        let Some(pattern_node) = self.arena.get(pattern_idx) else {
            return;
        };
        let Some(pattern) = self.arena.get_binding_pattern(pattern_node) else {
            return;
        };

        if pattern_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN {
            for (index, &element_idx) in pattern.elements.nodes.iter().enumerate() {
                let Some(element_node) = self.arena.get(element_idx) else {
                    continue;
                };
                if element_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                    continue;
                }
                let Some(element) = self.arena.get_binding_element(element_node) else {
                    continue;
                };
                if element.dot_dot_dot_token {
                    continue;
                }
                let Some(name) = self.binding_element_export_name(element.name) else {
                    continue;
                };
                self.write_namespace_export_separator(wrote_any);
                self.write(ns_name);
                self.write(".");
                self.write(&name);
                self.write(" = ");
                self.write(source);
                self.write("[");
                self.write(&index.to_string());
                self.write("]");
            }
        } else if pattern_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN {
            for &element_idx in &pattern.elements.nodes {
                let Some(element_node) = self.arena.get(element_idx) else {
                    continue;
                };
                let Some(element) = self.arena.get_binding_element(element_node) else {
                    continue;
                };
                if element.dot_dot_dot_token {
                    continue;
                }
                let prop_idx = if element.property_name.is_some() {
                    element.property_name
                } else {
                    element.name
                };
                let Some(prop_name) = self.binding_element_export_name(prop_idx) else {
                    continue;
                };
                let Some(name) = self.binding_element_export_name(element.name) else {
                    continue;
                };
                self.write_namespace_export_separator(wrote_any);
                self.write(ns_name);
                self.write(".");
                self.write(&name);
                self.write(" = ");
                self.write(source);
                self.write(".");
                self.write(&prop_name);
            }
        }
    }

    pub(in crate::emitter) fn write_namespace_export_separator(&mut self, wrote_any: &mut bool) {
        if *wrote_any {
            self.write(", ");
        }
        *wrote_any = true;
    }

    fn binding_element_export_name(&self, idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(idx)?;
        if let Some(ident) = self.arena.get_identifier(node) {
            return Some(ident.escaped_text.clone());
        }
        self.arena.get_literal(node).map(|lit| lit.text.clone())
    }

    /// Returns true when a variable statement node has no initializers in any of its
    /// declarators (e.g., `export var b: number;`). Used to suppress orphaned leading
    /// comments for exported variable declarations that produce no runtime code.
    pub(in crate::emitter) fn namespace_variable_has_no_initializers(
        &self,
        var_stmt_idx: NodeIndex,
    ) -> bool {
        let Some(var_node) = self.arena.get(var_stmt_idx) else {
            return false;
        };
        let Some(var_stmt) = self.arena.get_variable(var_node) else {
            return false;
        };
        for &decl_list_idx in &var_stmt.declarations.nodes {
            let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                continue;
            };
            let Some(decl_list) = self.arena.get_variable(decl_list_node) else {
                continue;
            };
            for &decl_idx in &decl_list.declarations.nodes {
                let Some(decl_node) = self.arena.get(decl_idx) else {
                    continue;
                };
                let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                    continue;
                };
                if decl.initializer.is_some() {
                    return false;
                }
            }
        }
        true
    }

    /// Get export names from a declaration clause (function, class, variable, enum).
    pub(in crate::emitter) fn get_export_names_from_clause(
        &self,
        clause_idx: NodeIndex,
    ) -> Vec<String> {
        let Some(node) = self.arena.get(clause_idx) else {
            return Vec::new();
        };
        match node.kind {
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                if let Some(var_stmt) = self.arena.get_variable(node) {
                    return self.collect_variable_names(&var_stmt.declarations);
                }
            }
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                if let Some(func) = self.arena.get_function(node)
                    && let Some(name_node) = self.arena.get(func.name)
                    && let Some(ident) = self.arena.get_identifier(name_node)
                {
                    return vec![ident.escaped_text.clone()];
                }
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                if let Some(class) = self.arena.get_class(node)
                    && let Some(name_node) = self.arena.get(class.name)
                    && let Some(ident) = self.arena.get_identifier(name_node)
                {
                    return vec![ident.escaped_text.clone()];
                }
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                if let Some(enum_decl) = self.arena.get_enum(node)
                    && let Some(name_node) = self.arena.get(enum_decl.name)
                    && let Some(ident) = self.arena.get_identifier(name_node)
                {
                    return vec![ident.escaped_text.clone()];
                }
            }
            k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                if let Some(import_decl) = self.arena.get_import_decl(node) {
                    let name = self.get_identifier_text_idx(import_decl.import_clause);
                    if !name.is_empty() {
                        return vec![name];
                    }
                }
            }
            _ => {}
        }
        Vec::new()
    }
}
