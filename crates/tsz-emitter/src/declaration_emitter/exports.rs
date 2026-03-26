//! Declaration emitter - export and import emission.

use super::DeclarationEmitter;
use crate::enums::evaluator::{EnumEvaluator, EnumValue};
use rustc_hash::FxHashSet;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;
use tsz_solver::type_queries;

impl<'a> DeclarationEmitter<'a> {
    fn declaration_import_attribute_text(&self, node_idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(node_idx)?;
        if let Some(lit) = self.arena.get_literal(node) {
            return Some(lit.text.clone());
        }
        if let Some(ident) = self.arena.get_identifier(node) {
            return Some(ident.escaped_text.clone());
        }
        self.get_source_slice(node.pos, node.end)
    }

    fn should_emit_declaration_import_attribute(&self, attribute_idx: NodeIndex) -> bool {
        let Some(attr_node) = self.arena.get(attribute_idx) else {
            return false;
        };
        let Some(attr) = self.arena.get_import_attribute_data(attr_node) else {
            return false;
        };

        let Some(name) = self.declaration_import_attribute_text(attr.name) else {
            return true;
        };
        if name != "resolution-mode" {
            return true;
        }

        matches!(
            self.declaration_import_attribute_text(attr.value)
                .as_deref(),
            Some("import" | "require")
        )
    }

    fn emit_declaration_import_attributes(&mut self, attributes: NodeIndex) {
        let Some(attr_node) = self.arena.get(attributes) else {
            return;
        };
        let Some(attrs) = self.arena.get_import_attributes_data(attr_node) else {
            return;
        };
        let filtered: Vec<NodeIndex> = attrs
            .elements
            .nodes
            .iter()
            .copied()
            .filter(|&elem_idx| self.should_emit_declaration_import_attribute(elem_idx))
            .collect();
        if filtered.is_empty() {
            return;
        }

        let keyword = if attrs.token == SyntaxKind::AssertKeyword as u16 {
            "assert"
        } else {
            "with"
        };

        self.write(" ");
        self.write(keyword);
        self.write(" { ");

        for (i, elem_idx) in filtered.iter().copied().enumerate() {
            if i > 0 {
                self.write(", ");
            }

            if let Some(elem_node) = self.arena.get(elem_idx)
                && let Some(attr) = self.arena.get_import_attribute_data(elem_node)
            {
                self.emit_node(attr.name);
                self.write(": ");
                self.emit_node(attr.value);
            }
        }

        self.write(" }");
    }

    pub(crate) fn emit_export_declaration(&mut self, export_idx: NodeIndex) {
        let Some(export_node) = self.arena.get(export_idx) else {
            return;
        };
        let Some(export) = self.arena.get_export_decl(export_node) else {
            return;
        };

        if self.js_skipped_reexports.contains(&export_idx) {
            return;
        }
        if let Some(group) = self.js_grouped_reexports.get(&export_idx).cloned() {
            self.emit_grouped_js_reexports(&group);
            return;
        }

        if let Some(statements) = self
            .js_folded_named_export_statements
            .get(&export_idx)
            .cloned()
        {
            for stmt_idx in statements {
                self.emit_deferred_js_named_export_statement(stmt_idx);
            }
            return;
        }

        if export.is_default_export {
            if export.export_clause.is_some()
                && let Some(clause_node) = self.arena.get(export.export_clause)
            {
                match clause_node.kind {
                    k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                        self.emit_export_default_function(export.export_clause);
                        return;
                    }
                    k if k == syntax_kind_ext::CLASS_DECLARATION => {
                        self.emit_export_default_class(export.export_clause);
                        return;
                    }
                    k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                        self.emit_export_default_interface(export.export_clause);
                        return;
                    }
                    _ => {}
                }
            }

            self.emit_export_default_expression(export_idx, export.export_clause);
            return;
        }

        if self.source_is_js_file
            && export.module_specifier.is_none()
            && export.export_clause.is_some()
            && let Some(clause_node) = self.arena.get(export.export_clause)
            && clause_node.kind == syntax_kind_ext::NAMED_EXPORTS
            && let Some(named) = self.arena.get_named_imports(clause_node)
            && named.name.is_none()
            && named.elements.nodes.is_empty()
        {
            // In JS, a bare `export {};` is only a module marker. Preserve module
            // semantics via the final scope-fix pass instead of eagerly emitting it,
            // so synthesized declaration exports can replace it.
            return;
        }

        // Check if export_clause is a declaration (interface, class, function, type, enum)
        if export.export_clause.is_some()
            && let Some(clause_node) = self.arena.get(export.export_clause)
        {
            match clause_node.kind {
                k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                    // Emit: export interface Foo {...}
                    self.emit_exported_interface(export.export_clause);
                    return;
                }
                k if k == syntax_kind_ext::CLASS_DECLARATION => {
                    self.emit_exported_class(export.export_clause);
                    return;
                }
                k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                    self.emit_exported_function(export.export_clause);
                    return;
                }
                k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                    self.emit_exported_type_alias(export.export_clause);
                    return;
                }
                k if k == syntax_kind_ext::ENUM_DECLARATION => {
                    self.emit_exported_enum(export.export_clause);
                    return;
                }
                k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                    self.emit_exported_variable(export.export_clause);
                    return;
                }
                k if k == syntax_kind_ext::MODULE_DECLARATION => {
                    let prev_public_api_scope_depth = self.public_api_scope_depth;
                    self.public_api_scope_depth += 1;
                    self.emit_module_declaration_with_export(export.export_clause, true);
                    self.public_api_scope_depth = prev_public_api_scope_depth;
                    return;
                }
                k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                    if self.source_is_js_file {
                        self.emit_import_equals_declaration_without_export(export.export_clause);
                    } else {
                        // Emit: export import x = require(...)
                        self.write_indent();
                        self.write("export ");
                        self.emit_import_equals_declaration(export.export_clause, true);
                    }
                    return;
                }
                _ => {}
            }
        }

        // Handle named exports: export { a, b } from "mod"
        // or star exports: export * from "mod"
        self.write_indent();
        self.write("export ");

        if export.is_type_only {
            self.write("type ");
        }

        if export.export_clause.is_some() {
            if let Some(clause_node) = self.arena.get(export.export_clause) {
                if clause_node.kind == syntax_kind_ext::NAMED_EXPORTS {
                    self.emit_named_exports(export.export_clause, true);
                } else if clause_node.kind == SyntaxKind::Identifier as u16
                    || clause_node.kind == SyntaxKind::StringLiteral as u16
                {
                    // export * as <name> from "mod" or export * as "<string>" from "mod"
                    self.emit_namespace_export_clause(export.export_clause);
                } else {
                    self.emit_node(export.export_clause);
                }
            }
        } else {
            self.write("*");
        }

        if export.module_specifier.is_some() {
            self.write(" from ");
            self.emit_node(export.module_specifier);
            self.emit_declaration_import_attributes(export.attributes);
        }

        self.write(";");
        self.write_line();
    }

    fn emit_grouped_js_reexports(&mut self, group: &[NodeIndex]) {
        let Some(&first_idx) = group.first() else {
            return;
        };
        let Some(first_node) = self.arena.get(first_idx) else {
            return;
        };
        let Some(first_export) = self.arena.get_export_decl(first_node) else {
            return;
        };

        self.write_indent();
        self.write("export ");
        self.write("{ ");

        let mut first = true;
        for &export_idx in group {
            let Some(export_node) = self.arena.get(export_idx) else {
                continue;
            };
            let Some(export) = self.arena.get_export_decl(export_node) else {
                continue;
            };
            let Some(clause_node) = self.arena.get(export.export_clause) else {
                continue;
            };
            let Some(named) = self.arena.get_named_imports(clause_node) else {
                continue;
            };
            for &spec_idx in &named.elements.nodes {
                if !first {
                    self.write(", ");
                }
                first = false;
                self.emit_specifier(spec_idx, true);
            }
        }

        self.write(" } from ");
        self.emit_node(first_export.module_specifier);
        self.write(";");
        self.write_line();
    }

    pub(crate) fn emit_export_assignment(&mut self, assign_idx: NodeIndex) {
        let Some(assign_node) = self.arena.get(assign_idx) else {
            return;
        };
        let Some(assign) = self.arena.get_export_assignment(assign_node) else {
            return;
        };

        self.write_indent();
        if assign.is_export_equals {
            if self.source_is_js_file
                && let Some(expr_node) = self.arena.get(assign.expression)
                && expr_node.kind == SyntaxKind::Identifier as u16
                && let Some(ident) = self.arena.get_identifier(expr_node)
                && !self
                    .emitted_js_export_equals_names
                    .insert(ident.escaped_text.clone())
            {
                return;
            }
            self.write("export = ");
            self.emit_expression(assign.expression);
            self.write(";");
            self.write_line();
        } else {
            // export default expression
            // Check if expression is a declaration (function, class) or a value expression
            let Some(expr_node) = self.arena.get(assign.expression) else {
                return;
            };

            let is_declaration = match expr_node.kind {
                k if k == syntax_kind_ext::FUNCTION_DECLARATION => true,
                k if k == syntax_kind_ext::CLASS_DECLARATION => true,
                k if k == syntax_kind_ext::INTERFACE_DECLARATION => true,
                k if k == syntax_kind_ext::ENUM_DECLARATION => true,
                k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => true,
                _ => false,
            };

            if is_declaration {
                match expr_node.kind {
                    k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                        self.emit_export_default_function(assign.expression);
                    }
                    k if k == syntax_kind_ext::CLASS_DECLARATION => {
                        self.emit_export_default_class(assign.expression);
                    }
                    k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                        self.emit_export_default_interface(assign.expression);
                    }
                    _ => {
                        self.write("export default ");
                        self.emit_node(assign.expression);
                        self.write(";");
                        self.write_line();
                    }
                }
            } else if expr_node.kind == SyntaxKind::Identifier as u16 {
                // export default <identifier> — emit directly
                self.write("export default ");
                self.emit_node(assign.expression);
                self.write(";");
                self.write_line();
            } else {
                // Value expression - synthesize _default variable
                let var_name = self.unique_default_export_name();

                // TS2883: Check for non-portable inferred type references
                // in export default expressions
                if let Some(file_path) = self.current_file_path.clone() {
                    let reported = self
                        .get_node_type(assign.expression)
                        .is_some_and(|type_id| {
                            self.emit_non_portable_type_diagnostic(
                                type_id,
                                "default",
                                &file_path,
                                assign_node.pos,
                                assign_node.end - assign_node.pos,
                            )
                        });
                    if !reported
                        && expr_node.kind == syntax_kind_ext::CALL_EXPRESSION
                        && let Some(call) = self.arena.get_call_expr(expr_node)
                        && self.is_object_assign_call(call.expression)
                        && let Some(args) = &call.arguments
                    {
                        for &arg_idx in &args.nodes {
                            if let Some(arg_type_id) = self
                                .get_node_type_or_names(&[arg_idx])
                                .or_else(|| self.get_type_via_symbol(arg_idx))
                                && self.emit_non_portable_type_diagnostic(
                                    arg_type_id,
                                    "default",
                                    &file_path,
                                    assign_node.pos,
                                    assign_node.end - assign_node.pos,
                                )
                            {
                                break;
                            }
                            if self.emit_non_portable_initializer_declaration_diagnostics(
                                arg_idx,
                                "default",
                                &file_path,
                                assign_node.pos,
                                assign_node.end - assign_node.pos,
                            ) {
                                break;
                            }
                            if self.emit_non_portable_expression_symbol_diagnostic(
                                arg_idx,
                                "default",
                                &file_path,
                                assign_node.pos,
                                assign_node.end - assign_node.pos,
                            ) {
                                break;
                            }
                        }
                    }
                }

                // First, emit the synthesized variable with inferred type
                self.write_indent();
                self.write("declare const ");
                self.write(&var_name);
                self.write(": ");

                // Get the type of the expression
                if let Some(type_id) = self.get_node_type(assign.expression) {
                    self.write(&self.print_type_id(type_id));
                } else {
                    self.write("any");
                }

                self.write(";");
                self.write_line();

                // Then, emit export default _default
                self.write_indent();
                self.write("export default ");
                self.write(&var_name);
                self.write(";");
                self.write_line();
            }
        }
    }

    pub(crate) fn emit_export_default_function(&mut self, func_idx: NodeIndex) {
        let Some(func_node) = self.arena.get(func_idx) else {
            return;
        };
        let Some(func) = self.arena.get_function(func_node) else {
            return;
        };

        self.write_indent();
        self.write("export default function ");
        self.emit_node(func.name);

        let jsdoc_template_params = if func
            .type_parameters
            .as_ref()
            .is_none_or(|type_params| type_params.nodes.is_empty())
        {
            self.jsdoc_template_params_for_node(func_idx)
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

        let func_body = func.body;
        let func_name = func.name;
        if func.type_annotation.is_some() {
            self.write(": ");
            self.emit_type(func.type_annotation);
        } else if let Some(return_type_text) = self.jsdoc_return_type_text_for_node(func_idx) {
            self.write(": ");
            self.write(&return_type_text);
        } else if func_body.is_some()
            && self.emit_js_returned_define_property_function_type(func_body)
        {
        } else if let (Some(interner), Some(cache)) = (&self.type_interner, &self.type_cache) {
            // No explicit return type, try to infer it from the type cache
            let func_type_id = cache
                .node_types
                .get(&func_idx.0)
                .copied()
                .or_else(|| self.get_type_via_symbol_for_func(func_idx, func_name));
            if let Some(func_type_id) = func_type_id
                && let Some(return_type_id) = type_queries::get_return_type(*interner, func_type_id)
            {
                // If solver returned `any` but the function body clearly returns void,
                // prefer void (the solver's `any` is a fallback, not an actual inference)
                if return_type_id == tsz_solver::types::TypeId::ANY
                    && func_body.is_some()
                    && self.body_returns_void(func_body)
                {
                    self.write(": void");
                } else {
                    self.write(": ");
                    self.write(&self.print_type_id(return_type_id));
                }
            } else if func_body.is_some() {
                if self.body_returns_void(func_body) {
                    self.write(": void");
                } else if let Some(return_text) =
                    self.function_body_preferred_return_type_text(func_body)
                {
                    self.write(": ");
                    self.write(&return_text);
                }
            }
        } else if func_body.is_some() {
            if self.body_returns_void(func_body) {
                self.write(": void");
            } else if let Some(return_text) =
                self.function_body_preferred_return_type_text(func_body)
            {
                self.write(": ");
                self.write(&return_text);
            }
        }

        self.write(";");
        self.write_line();
    }

    pub(crate) fn emit_export_default_class(&mut self, class_idx: NodeIndex) {
        let Some(class_node) = self.arena.get(class_idx) else {
            return;
        };
        let Some(class) = self.arena.get_class(class_node) else {
            return;
        };

        let is_abstract = self
            .arena
            .has_modifier(&class.modifiers, SyntaxKind::AbstractKeyword);
        let extends_alias = self.emit_synthetic_class_extends_alias_if_needed(
            class.name,
            class.heritage_clauses.as_ref(),
            true,
        );

        self.write_indent();
        self.write("export default ");
        if is_abstract {
            self.write("abstract ");
        }
        // Only add space after "class" if there's a name to emit
        if class.name.is_some()
            && self
                .arena
                .get(class.name)
                .is_some_and(|n| n.kind != SyntaxKind::Unknown as u16)
        {
            self.write("class ");
            self.emit_node(class.name);
        } else {
            self.write("class");
        }

        if let Some(ref type_params) = class.type_parameters
            && !type_params.nodes.is_empty()
        {
            self.emit_type_parameters(type_params);
        }

        if let Some(ref heritage) = class.heritage_clauses {
            self.emit_class_heritage_clauses(heritage, extends_alias.as_deref());
        }

        self.write(" {");
        self.write_line();
        self.increase_indent();

        // Reset constructor and method overload tracking for this class
        self.class_has_constructor_overloads = false;
        self.method_names_with_overloads = rustc_hash::FxHashSet::default();

        // Emit parameter properties from constructor first (before other members)
        self.emit_parameter_properties(&class.members);

        // Emit `#private;` if any member has a private identifier name
        if self.class_has_private_identifier_member(&class.members) {
            self.write_indent();
            self.write("#private;");
            self.write_line();
        }

        for &member_idx in &class.members.nodes {
            let before_jsdoc_len = self.writer.len();
            let saved_comment_idx = self.comment_emit_idx;
            if let Some(mn) = self.arena.get(member_idx) {
                self.emit_leading_jsdoc_comments(mn.pos);
            }
            let before_member_len = self.writer.len();
            self.emit_class_member(member_idx);
            if self.writer.len() == before_member_len {
                // Member didn't emit anything (e.g., skipped implementation overload).
                // Rollback the speculatively emitted JSDoc comments.
                self.writer.truncate(before_jsdoc_len);
                self.comment_emit_idx = saved_comment_idx;
                if let Some(mn) = self.arena.get(member_idx) {
                    self.skip_comments_in_node(mn.pos, mn.end);
                }
            }
        }

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();
    }

    pub(crate) fn emit_export_default_interface(&mut self, iface_idx: NodeIndex) {
        let Some(iface_node) = self.arena.get(iface_idx) else {
            return;
        };
        let Some(iface) = self.arena.get_interface(iface_node) else {
            return;
        };

        self.write_indent();
        self.write("export default interface ");
        self.emit_node(iface.name);

        if let Some(ref type_params) = iface.type_parameters
            && !type_params.nodes.is_empty()
        {
            self.emit_type_parameters(type_params);
        }

        if let Some(ref heritage) = iface.heritage_clauses {
            self.emit_interface_heritage_clauses(heritage);
        }

        self.write(" {");
        self.write_line();
        self.increase_indent();

        for &member_idx in &iface.members.nodes {
            if let Some(mn) = self.arena.get(member_idx) {
                self.emit_leading_jsdoc_comments(mn.pos);
            }
            self.emit_interface_member(member_idx);
        }

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();
    }

    pub(crate) fn emit_export_default_expression(
        &mut self,
        export_idx: NodeIndex,
        expr_idx: NodeIndex,
    ) {
        // If the expression is a simple identifier, emit `export default <name>;` directly.
        // This matches tsc behavior for `export default foo;` where `foo` is declared in scope.
        if let Some(expr_node) = self.arena.get(expr_idx)
            && expr_node.kind == SyntaxKind::Identifier as u16
        {
            self.write_indent();
            self.write("export default ");
            self.emit_node(expr_idx);
            self.write(";");
            self.write_line();
            return;
        }

        // For complex expressions, synthesize a _default variable
        let var_name = self.unique_default_export_name();
        if let Some(expr_node) = self.arena.get(expr_idx)
            && let Some(file_path) = self.current_file_path.clone()
        {
            let (diag_pos, diag_len) = self
                .arena
                .get(export_idx)
                .map(|export_node| (export_node.pos, export_node.end - export_node.pos))
                .unwrap_or((expr_node.pos, expr_node.end - expr_node.pos));
            let reported = self.get_node_type(expr_idx).is_some_and(|type_id| {
                self.emit_non_portable_type_diagnostic(
                    type_id, "default", &file_path, diag_pos, diag_len,
                )
            });

            if !reported
                && expr_node.kind == syntax_kind_ext::CALL_EXPRESSION
                && let Some(call) = self.arena.get_call_expr(expr_node)
                && self.is_object_assign_call(call.expression)
                && let Some(args) = &call.arguments
            {
                for &arg_idx in &args.nodes {
                    if let Some(arg_type_id) = self
                        .get_node_type_or_names(&[arg_idx])
                        .or_else(|| self.get_type_via_symbol(arg_idx))
                        && self.emit_non_portable_type_diagnostic(
                            arg_type_id,
                            "default",
                            &file_path,
                            diag_pos,
                            diag_len,
                        )
                    {
                        break;
                    }
                    if self.emit_non_portable_initializer_declaration_diagnostics(
                        arg_idx, "default", &file_path, diag_pos, diag_len,
                    ) {
                        break;
                    }
                    if self.emit_non_portable_expression_symbol_diagnostic(
                        arg_idx, "default", &file_path, diag_pos, diag_len,
                    ) {
                        break;
                    }
                }
            }
        }

        // First, emit: declare const _default: <type>;
        self.write_indent();
        self.write("declare const ");
        self.write(&var_name);

        // Default exports are const-like — preserve literal types for simple literals
        if let Some(literal_text) = self.const_literal_initializer_text_deep(expr_idx) {
            self.write(": ");
            self.write(&literal_text);
        } else if let Some(type_text) = self.preferred_expression_type_text(expr_idx) {
            self.write(": ");
            self.write(&type_text);
        } else if let Some(type_id) = self.get_node_type(expr_idx) {
            self.write(": ");
            self.write(&self.print_type_id(type_id));
        } else {
            self.write(": any");
        }

        self.write(";");
        self.write_line();

        // Then, emit: export default _default;
        self.write_indent();
        self.write("export default ");
        self.write(&var_name);
        self.write(";");
        self.write_line();
    }

    /// Generate a unique name for the default export synthesized variable.
    /// If `_default` is already in scope, tries `_default_1`, `_default_2`, etc.
    fn unique_default_export_name(&mut self) -> String {
        let base = "_default".to_string();
        if !self.reserved_names.contains(&base) {
            self.reserved_names.insert(base.clone());
            return base;
        }
        for i in 1.. {
            let candidate = format!("_default_{i}");
            if !self.reserved_names.contains(&candidate) {
                self.reserved_names.insert(candidate.clone());
                return candidate;
            }
        }
        unreachable!()
    }

    pub(crate) fn emit_namespace_export_clause(&mut self, clause_idx: NodeIndex) {
        self.write("* as ");
        self.emit_node(clause_idx);
    }

    pub(crate) fn emit_named_exports(&mut self, exports_idx: NodeIndex, allow_type_prefix: bool) {
        let Some(exports_node) = self.arena.get(exports_idx) else {
            return;
        };
        let Some(exports) = self.arena.get_named_imports(exports_node) else {
            return;
        };

        if exports.name.is_some() && exports.elements.nodes.is_empty() {
            self.write("* as ");
            self.emit_node(exports.name);
            return;
        }

        if exports.elements.nodes.is_empty() {
            self.write("{}");
            return;
        }

        self.write("{ ");
        let mut first = true;
        for &spec_idx in &exports.elements.nodes {
            if !first {
                self.write(", ");
            }
            first = false;
            self.emit_specifier(spec_idx, allow_type_prefix);
        }
        self.write(" }");
    }

    /// Emit a named import/export specifier: `[type] [propertyName as] name`
    pub(crate) fn emit_specifier(&mut self, spec_idx: NodeIndex, allow_type_prefix: bool) {
        let Some(spec_node) = self.arena.get(spec_idx) else {
            return;
        };
        let Some(spec) = self.arena.get_specifier(spec_node) else {
            return;
        };

        if allow_type_prefix && spec.is_type_only {
            self.write("type ");
        }

        if spec.property_name.is_some() {
            self.emit_node(spec.property_name);
            self.write(" as ");
        }
        self.emit_node(spec.name);
    }

    // Helper to emit exported interface with "export" prefix
    pub(crate) fn emit_exported_interface(&mut self, iface_idx: NodeIndex) {
        let Some(iface_node) = self.arena.get(iface_idx) else {
            return;
        };
        let Some(iface) = self.arena.get_interface(iface_node) else {
            return;
        };

        self.write_indent();
        if self.should_emit_export_keyword() {
            self.write("export ");
        }
        self.write("interface ");
        self.emit_node(iface.name);

        if let Some(ref type_params) = iface.type_parameters
            && !type_params.nodes.is_empty()
        {
            self.emit_type_parameters(type_params);
        }

        if let Some(ref heritage) = iface.heritage_clauses {
            self.emit_heritage_clauses(heritage);
        }

        self.write(" {");
        self.write_line();
        self.increase_indent();

        for &member_idx in &iface.members.nodes {
            let before_jsdoc_len = self.writer.len();
            let saved_comment_idx = self.comment_emit_idx;
            if let Some(member_node) = self.arena.get(member_idx) {
                self.emit_leading_jsdoc_comments(member_node.pos);
            }
            let before_member_len = self.writer.len();
            self.emit_interface_member(member_idx);
            if self.writer.len() == before_member_len {
                self.writer.truncate(before_jsdoc_len);
                self.comment_emit_idx = saved_comment_idx;
                if let Some(member_node) = self.arena.get(member_idx) {
                    self.skip_comments_in_node(member_node.pos, member_node.end);
                }
            }
        }

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();
    }

    pub(crate) fn emit_exported_class(&mut self, class_idx: NodeIndex) {
        let Some(class_node) = self.arena.get(class_idx) else {
            return;
        };
        let Some(class) = self.arena.get_class(class_node) else {
            return;
        };

        let is_abstract = self
            .arena
            .has_modifier(&class.modifiers, SyntaxKind::AbstractKeyword);
        let extends_alias = self.emit_synthetic_class_extends_alias_if_needed(
            class.name,
            class.heritage_clauses.as_ref(),
            false,
        );

        self.write_indent();
        if self.should_emit_export_keyword() {
            self.write("export ");
        }
        if self.should_emit_declare_keyword(true) {
            self.write("declare ");
        }
        if is_abstract {
            self.write("abstract ");
        }
        self.write("class ");
        self.emit_node(class.name);

        if let Some(ref type_params) = class.type_parameters
            && !type_params.nodes.is_empty()
        {
            self.emit_type_parameters(type_params);
        }

        if let Some(ref heritage) = class.heritage_clauses {
            self.emit_class_heritage_clauses(heritage, extends_alias.as_deref());
        }

        self.write(" {");
        self.write_line();
        self.increase_indent();

        // Reset constructor and method overload tracking for this class
        self.class_has_constructor_overloads = false;
        self.method_names_with_overloads = FxHashSet::default();

        // Emit parameter properties from constructor first (before other members)
        self.emit_parameter_properties(&class.members);

        // Emit `#private;` if any member has a private identifier name (e.g., #foo)
        if self.class_has_private_identifier_member(&class.members) {
            self.write_indent();
            self.write("#private;");
            self.write_line();
        }

        for &member_idx in &class.members.nodes {
            let before_jsdoc_len = self.writer.len();
            let saved_comment_idx = self.comment_emit_idx;
            if let Some(member_node) = self.arena.get(member_idx) {
                self.emit_leading_jsdoc_comments(member_node.pos);
            }
            let before_member_len = self.writer.len();
            self.emit_class_member(member_idx);
            if self.writer.len() == before_member_len {
                self.writer.truncate(before_jsdoc_len);
                self.comment_emit_idx = saved_comment_idx;
                if let Some(member_node) = self.arena.get(member_idx) {
                    self.skip_comments_in_node(member_node.pos, member_node.end);
                }
            }
        }

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();
    }

    pub(crate) fn emit_exported_function(&mut self, func_idx: NodeIndex) {
        let Some(func_node) = self.arena.get(func_idx) else {
            return;
        };
        let Some(func) = self.arena.get_function(func_node) else {
            return;
        };

        // Get function name as string for overload tracking
        let function_name = self.get_function_name(func_idx);

        // Check if this is an overload (no body) or implementation (has body)
        let is_overload = func.body.is_none();
        let is_implementation = !is_overload;

        // Overload handling:
        // - If this is an overload, emit it and mark that this function has overloads
        // - If this is an implementation and the function already has overloads, skip it
        // - If this is an implementation with no overloads, emit it
        if is_overload {
            // Mark that this function name has overload signatures
            if let Some(ref name) = function_name {
                self.function_names_with_overloads.insert(name.clone());
            }
        } else if is_implementation {
            // This is an implementation - check if we've seen overloads for this name
            if let Some(ref name) = function_name
                && self.function_names_with_overloads.contains(name)
            {
                // Skip implementation signature when overloads exist
                return;
            }
        }

        self.write_indent();
        if self.should_emit_export_keyword() {
            self.write("export ");
        }
        if self.should_emit_declare_keyword(true) {
            self.write("declare ");
        }
        self.write("function ");
        self.emit_node(func.name);

        let jsdoc_template_params = if func
            .type_parameters
            .as_ref()
            .is_none_or(|type_params| type_params.nodes.is_empty())
        {
            self.jsdoc_template_params_for_node(func_idx)
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

        let func_body = func.body;
        let func_name = func.name;
        if func.type_annotation.is_some() {
            self.write(": ");
            self.emit_type(func.type_annotation);
        } else if let Some(return_type_text) = self.jsdoc_return_type_text_for_node(func_idx) {
            self.write(": ");
            self.write(&return_type_text);
        } else if func_body.is_some()
            && self.emit_js_returned_define_property_function_type(func_body)
        {
        } else if let (Some(interner), Some(cache)) = (&self.type_interner, &self.type_cache) {
            // No explicit return type, try to infer it from the type cache
            let func_type_id = cache
                .node_types
                .get(&func_idx.0)
                .copied()
                .or_else(|| self.get_type_via_symbol_for_func(func_idx, func_name));
            if let Some(func_type_id) = func_type_id
                && let Some(return_type_id) = type_queries::get_return_type(*interner, func_type_id)
            {
                // If solver returned `any` but the function body clearly returns void,
                // prefer void (the solver's `any` is a fallback, not an actual inference)
                if return_type_id == tsz_solver::types::TypeId::ANY
                    && func_body.is_some()
                    && self.body_returns_void(func_body)
                {
                    self.write(": void");
                } else {
                    self.write(": ");
                    self.write(&self.print_type_id(return_type_id));
                }
            } else if func_body.is_some() && self.body_returns_void(func_body) {
                self.write(": void");
            }
        } else if func_body.is_some() && self.body_returns_void(func_body) {
            self.write(": void");
        }

        self.write(";");
        self.write_line();
    }

    pub(crate) fn emit_exported_type_alias(&mut self, alias_idx: NodeIndex) {
        let Some(alias_node) = self.arena.get(alias_idx) else {
            return;
        };
        let Some(alias) = self.arena.get_type_alias(alias_node) else {
            return;
        };

        self.write_indent();
        if self.should_emit_export_keyword() {
            self.write("export ");
        }
        if self
            .arena
            .has_modifier(&alias.modifiers, SyntaxKind::DeclareKeyword)
            && !self.inside_declare_namespace
        {
            self.write("declare ");
        }
        self.write("type ");
        self.emit_node(alias.name);

        if let Some(ref type_params) = alias.type_parameters
            && !type_params.nodes.is_empty()
        {
            self.emit_type_parameters(type_params);
        }

        self.write(" = ");
        self.emit_type(alias.type_node);
        self.write(";");
        self.write_line();
    }

    pub(crate) fn emit_exported_enum(&mut self, enum_idx: NodeIndex) {
        let Some(enum_node) = self.arena.get(enum_idx) else {
            return;
        };
        let Some(enum_data) = self.arena.get_enum(enum_node) else {
            return;
        };

        let is_const = self
            .arena
            .has_modifier(&enum_data.modifiers, SyntaxKind::ConstKeyword);

        self.write_indent();
        if self.should_emit_export_keyword() {
            self.write("export ");
        }
        if self.should_emit_declare_keyword(true) {
            self.write("declare ");
        }
        if is_const {
            self.write("const ");
        }
        self.write("enum ");
        self.emit_node(enum_data.name);

        self.write(" {");
        self.write_line();
        self.increase_indent();

        // Evaluate enum member values to get correct auto-increment behavior.
        // Seed with accumulated values for cross-enum reference resolution.
        let prior = std::mem::take(&mut self.all_enum_values);
        let mut evaluator = EnumEvaluator::with_prior_values(self.arena, prior);
        let member_values = evaluator.evaluate_enum(enum_idx);
        self.all_enum_values = evaluator.take_all_enum_values();

        for (i, &member_idx) in enum_data.members.nodes.iter().enumerate() {
            self.write_indent();
            if let Some(member_node) = self.arena.get(member_idx)
                && let Some(member) = self.arena.get_enum_member(member_node)
            {
                self.emit_node(member.name);
                let member_name = self.get_enum_member_name(member.name);
                if let Some(value) = member_values.get(&member_name) {
                    match value {
                        crate::enums::evaluator::EnumValue::Computed => {
                            // Computed values: no initializer in .d.ts
                        }
                        _ => {
                            self.write(" = ");
                            self.emit_enum_value(value);
                        }
                    }
                } else {
                    // Fallback to index if evaluation failed
                    self.write(" = ");
                    self.write(&i.to_string());
                }
            }
            if i < enum_data.members.nodes.len() - 1 {
                self.write(",");
            }
            self.write_line();
        }

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();
    }

    /// Get the name of an enum member from its name node
    pub(crate) fn get_enum_member_name(&self, name_idx: NodeIndex) -> String {
        if let Some(name_node) = self.arena.get(name_idx) {
            if let Some(ident) = self.arena.get_identifier(name_node) {
                return ident.escaped_text.clone();
            }
            if let Some(lit) = self.arena.get_literal(name_node) {
                return lit.text.clone();
            }
        }
        String::new()
    }

    /// Emit an evaluated enum value
    pub(crate) fn emit_enum_value(&mut self, value: &EnumValue) {
        match value {
            EnumValue::Number(n) => {
                self.write(&n.to_string());
            }
            EnumValue::String(s) => {
                self.write("\"");
                for ch in s.chars() {
                    match ch {
                        '\\' => self.write("\\\\"),
                        '"' => self.write("\\\""),
                        '\n' => self.write("\\n"),
                        '\r' => self.write("\\r"),
                        '\t' => self.write("\\t"),
                        '\0' => self.write("\\0"),
                        _ => {
                            let mut buf = [0u8; 4];
                            self.write(ch.encode_utf8(&mut buf));
                        }
                    }
                }
                self.write("\"");
            }
            EnumValue::Float(f) => {
                self.write(&Self::format_js_number(*f));
            }
            EnumValue::Computed => {
                // For computed values, emit 0 as fallback
                self.write("0 /* computed */");
            }
        }
    }

    pub(crate) fn emit_exported_variable(&mut self, stmt_idx: NodeIndex) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };
        let Some(var_stmt) = self.arena.get_variable(stmt_node) else {
            return;
        };

        for &decl_list_idx in &var_stmt.declarations.nodes {
            let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                continue;
            };

            if decl_list_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
                && let Some(decl_list) = self.arena.get_variable(decl_list_node)
            {
                // `using` and `await using` declarations emit as `const` in .d.ts
                let flags = decl_list_node.flags as u32;
                let keyword = if flags
                    & (tsz_parser::parser::node_flags::USING
                        | tsz_parser::parser::node_flags::CONST)
                    != 0
                {
                    "const"
                } else if flags & tsz_parser::parser::node_flags::LET != 0 {
                    "let"
                } else {
                    "var"
                };

                // Separate destructuring from regular declarations
                let mut regular_decls = Vec::new();
                for &decl_idx in &decl_list.declarations.nodes {
                    if let Some(decl_node) = self.arena.get(decl_idx)
                        && let Some(decl) = self.arena.get_variable_declaration(decl_node)
                    {
                        let name_node = self.arena.get(decl.name);
                        let is_destructuring = name_node.is_some_and(|n| {
                            n.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                                || n.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                        });

                        if is_destructuring {
                            self.emit_flattened_variable_declaration(decl_idx, keyword, true);
                        } else {
                            regular_decls.push((decl_idx, decl));
                        }
                    }
                }

                if regular_decls.len() == 1 {
                    let (decl_idx, decl) = regular_decls[0];
                    if self.emit_js_function_variable_declaration_if_possible(
                        decl_idx,
                        decl.name,
                        decl.initializer,
                        true,
                    ) {
                        continue;
                    }
                }

                // Emit all regular declarations together on one line
                if !regular_decls.is_empty() {
                    self.write_indent();
                    if self.should_emit_export_keyword() {
                        self.write("export ");
                    }
                    if self.should_emit_declare_keyword(true) {
                        self.write("declare ");
                    }
                    self.write(keyword);
                    self.write(" ");

                    for (i, (decl_idx, decl)) in regular_decls.iter().enumerate() {
                        if i > 0 {
                            self.write(", ");
                        }

                        self.emit_node(decl.name);
                        self.emit_variable_decl_type_or_initializer(
                            keyword,
                            stmt_node.pos,
                            *decl_idx,
                            decl.name,
                            decl.type_annotation,
                            decl.initializer,
                        );
                    }

                    self.write(";");
                    self.write_line();
                }
            }
        }
    }

    pub(crate) fn emit_import_declaration_if_needed(&mut self, import_idx: NodeIndex) {
        // Source imports carry fidelity that auto-generated imports cannot reproduce
        // (aliasing, `type` modifiers, attributes, and source ordering). Emit them
        // through the filtered declaration path and reserve auto-import synthesis for
        // genuinely foreign symbols that have no source import in this file.
        self.emit_import_declaration(import_idx);
    }

    pub(crate) fn emit_import_declaration(&mut self, import_idx: NodeIndex) {
        let Some(import_node) = self.arena.get(import_idx) else {
            return;
        };
        let Some(import) = self.arena.get_import_decl(import_node) else {
            return;
        };

        // Side-effect imports (no clause) are always emitted
        if import.import_clause.is_none() {
            self.write_indent();
            self.write("import ");
            self.emit_node(import.module_specifier);
            self.write(";");
            self.write_line();
            return;
        }

        // Check if we should elide this import based on usage
        let (default_used, named_used) = self.count_used_imports(import);
        if default_used == 0 && named_used == 0 {
            // No used symbols in this import - elide it
            return;
        }

        // Emit the import with filtering
        self.write_indent();
        self.write("import ");

        if let Some(clause_node) = self.arena.get(import.import_clause)
            && let Some(clause) = self.arena.get_import_clause(clause_node)
        {
            if clause.is_type_only {
                self.write("type ");
            }
            if clause.is_deferred {
                self.write("defer ");
            }

            let mut has_default = false;

            // Default import (only if used)
            if clause.name.is_some() && default_used > 0 {
                self.emit_node(clause.name);
                has_default = true;
            }

            // Named imports (filter to used ones)
            if clause.named_bindings.is_some() && named_used > 0 {
                if has_default {
                    self.write(", ");
                }
                self.emit_named_imports_filtered(clause.named_bindings, !clause.is_type_only);
            }

            self.write(" from ");
        }

        self.emit_node(import.module_specifier);
        self.emit_declaration_import_attributes(import.attributes);
        self.write(";");
        self.write_line();
    }

    /// Emit named imports, filtering out unused specifiers.
    ///
    /// This version only emits import specifiers that are in the `used_symbols` set.
    pub(crate) fn emit_named_imports_filtered(
        &mut self,
        imports_idx: NodeIndex,
        allow_type_prefix: bool,
    ) {
        let Some(imports_node) = self.arena.get(imports_idx) else {
            return;
        };
        let Some(imports) = self.arena.get_named_imports(imports_node) else {
            return;
        };

        // Handle namespace imports (* as ns)
        if imports.name.is_some() && imports.elements.nodes.is_empty() {
            // Check if namespace is used
            if self.should_emit_import_specifier(imports.name) {
                self.write("* as ");
                self.emit_node(imports.name);
            }
            return;
        }

        // Filter individual specifiers
        self.write("{ ");
        let mut first = true;
        for &spec_idx in &imports.elements.nodes {
            // Only emit if the specifier is used
            if !self.should_emit_import_specifier(spec_idx) {
                continue;
            }

            if !first {
                self.write(", ");
            }
            first = false;
            self.emit_specifier(spec_idx, allow_type_prefix);
        }
        self.write(" }");
    }

    pub(crate) fn emit_module_declaration(&mut self, module_idx: NodeIndex) {
        self.emit_module_declaration_with_export(module_idx, false);
    }

    fn emit_module_declaration_with_export(
        &mut self,
        module_idx: NodeIndex,
        already_exported: bool,
    ) {
        let Some(module_node) = self.arena.get(module_idx) else {
            return;
        };
        let Some(module) = self.arena.get_module(module_node) else {
            return;
        };

        let is_exported = already_exported
            || self
                .arena
                .has_modifier(&module.modifiers, SyntaxKind::ExportKeyword);
        if !self.should_emit_public_api_module(is_exported, module.name) {
            return;
        }

        self.write_indent();
        if !self.inside_declare_namespace {
            if is_exported {
                self.write("export ");
            }
            if self.should_emit_declare_keyword(is_exported) {
                self.write("declare ");
            }
        } else if is_exported && self.should_emit_export_keyword() {
            self.write("export ");
        }

        // Determine keyword: "module" for string literals, "global" for
        // the `declare global` augmentation, "namespace" for other identifiers.
        let name_node = self.arena.get(module.name);
        let use_module_keyword =
            name_node.is_some_and(|n| n.kind == SyntaxKind::StringLiteral as u16);
        let is_global_augmentation = name_node
            .and_then(|n| self.arena.get_identifier(n))
            .is_some_and(|ident| ident.escaped_text == "global");

        if is_global_augmentation {
            // `declare global { ... }` — emit just "global" without
            // a module/namespace keyword prefix.
            self.write("global");
        } else {
            self.write(if use_module_keyword {
                "module "
            } else {
                "namespace "
            });
            self.emit_node(module.name);
        }

        // Collect dotted namespace name segments: namespace A.B.C { ... }
        // is represented as a chain of ModuleDeclaration nodes
        let mut current_body = module.body;
        let mut innermost_ns_idx = module_idx;
        loop {
            if !current_body.is_some() {
                break;
            }
            let Some(body_node) = self.arena.get(current_body) else {
                break;
            };
            if let Some(nested_mod) = self.arena.get_module(body_node) {
                // Body is another module declaration — emit dotted name
                self.write(".");
                self.emit_node(nested_mod.name);
                innermost_ns_idx = current_body;
                current_body = nested_mod.body;
            } else {
                break;
            }
        }

        if current_body.is_some() {
            // Check if the body is an empty block — tsc emits `namespace X { }` on one line
            let is_empty_body = self
                .arena
                .get(current_body)
                .and_then(|body_node| self.arena.get_module_block(body_node))
                .is_none_or(|module_block| {
                    module_block
                        .statements
                        .as_ref()
                        .is_none_or(|stmts| stmts.nodes.is_empty())
                });

            if is_empty_body {
                // tsc uses single-line `{ }` for empty namespaces nested inside
                // another declare namespace, but multi-line `{\n}` for top-level.
                if self.inside_declare_namespace {
                    self.write(" { }");
                    self.write_line();
                } else {
                    self.write(" {");
                    self.write_line();
                    self.write_indent();
                    self.write("}");
                    self.write_line();
                }
                return;
            }

            self.write(" {");
            self.write_line();
            self.increase_indent();

            // Inside a declare namespace, don't emit 'declare' keyword for members
            let prev_inside_declare_namespace = self.inside_declare_namespace;
            self.inside_declare_namespace = true;
            // Track innermost namespace symbol for context-relative type names
            let prev_enclosing_ns = self.enclosing_namespace_symbol;
            if let Some(binder) = self.binder
                && let Some(ns_sym) = binder.get_node_symbol(innermost_ns_idx)
            {
                self.enclosing_namespace_symbol = Some(ns_sym);
            }
            let prev_public_api_scope_depth = self.public_api_scope_depth;
            let prev_inside_non_ambient_namespace = self.inside_non_ambient_namespace;
            // In declare/ambient namespaces, all members are implicitly public,
            // so disable the API filter (increment depth).
            // In non-declare namespaces, members must have `export` to be public.
            // A namespace is ambient if it has `declare`, or if the source
            // is a .d.ts file, or if it's nested inside an ambient namespace
            // (but NOT if it's nested inside a non-ambient namespace).
            let is_ambient_ns = self
                .arena
                .has_modifier(&module.modifiers, SyntaxKind::DeclareKeyword)
                || self.source_is_declaration_file
                || (prev_inside_declare_namespace && !prev_inside_non_ambient_namespace);
            if is_ambient_ns {
                self.public_api_scope_depth += 1;
            } else {
                self.inside_non_ambient_namespace = true;
            }

            if let Some(body_node) = self.arena.get(current_body)
                && let Some(module_block) = self.arena.get_module_block(body_node)
                && let Some(ref stmts) = module_block.statements
            {
                // Save emission-tracking flags for this namespace scope
                let prev_emitted_non_exported = self.emitted_non_exported_declaration;
                let prev_emitted_scope_marker = self.emitted_scope_marker;
                self.emitted_non_exported_declaration = false;
                self.emitted_scope_marker = false;

                // Pre-scan to check if the body has a mix of exported and
                // non-exported members. When it does, tsc preserves `export`
                // keywords on individual members; otherwise it strips them.
                // Applies to both ambient string-named modules and non-ambient namespaces.
                let prev_ambient_scope_marker = self.ambient_module_has_scope_marker;
                if !is_ambient_ns || use_module_keyword {
                    self.ambient_module_has_scope_marker =
                        self.module_body_has_scope_marker(stmts, !is_ambient_ns);
                }

                for &stmt_idx in &stmts.nodes {
                    self.emit_statement(stmt_idx);
                }

                // tsc emits `export {};` inside a non-ambient namespace
                // body when there is a mix of exported and non-exported
                // members (the "scope-fix marker").
                // Use emission-time tracking instead of source analysis.
                let is_ambient_module = self
                    .arena
                    .has_modifier(&module.modifiers, SyntaxKind::DeclareKeyword)
                    || self.source_is_declaration_file
                    || (prev_inside_declare_namespace && !prev_inside_non_ambient_namespace);

                if !is_ambient_module
                    && self.emitted_non_exported_declaration
                    && !self.emitted_scope_marker
                {
                    self.write_indent();
                    self.write("export {};");
                    self.write_line();
                }

                // Restore tracking flags
                self.emitted_non_exported_declaration = prev_emitted_non_exported;
                self.emitted_scope_marker = prev_emitted_scope_marker;
                self.ambient_module_has_scope_marker = prev_ambient_scope_marker;
            }

            self.public_api_scope_depth = prev_public_api_scope_depth;
            self.inside_non_ambient_namespace = prev_inside_non_ambient_namespace;
            self.inside_declare_namespace = prev_inside_declare_namespace;
            self.enclosing_namespace_symbol = prev_enclosing_ns;
            self.decrease_indent();
            self.write_indent();
            self.write("}");
        } else {
            // Shorthand ambient module: declare module "foo";
            self.write(";");
        }

        self.write_line();
    }

    pub(crate) fn emit_import_equals_declaration(
        &mut self,
        import_idx: NodeIndex,
        already_exported: bool,
    ) {
        let Some(import_node) = self.arena.get(import_idx) else {
            return;
        };
        let Some(import_eq) = self.arena.get_import_decl(import_node) else {
            return;
        };

        let is_exported = self
            .arena
            .has_modifier(&import_eq.modifiers, SyntaxKind::ExportKeyword);
        let is_public_exported = is_exported && !already_exported;

        // Elide non-exported import equals declarations that are not used by the public API
        if !is_exported && !already_exported {
            // When no usage tracking is available, non-exported `import = require(...)`
            // declarations are almost always value-level and not needed in .d.ts output.
            if self.used_symbols.is_none() {
                return;
            }

            if !self.should_emit_public_api_dependency(import_eq.import_clause) {
                return;
            }

            // For namespace-path imports (import x = a.b, not import x = require("...")),
            // tsc only preserves them in .d.ts if the alias targets a type-level entity
            // (class, interface, enum, namespace, type alias, function). If the target is
            // a value-only entity (e.g., a variable), the emitted type resolves directly
            // to the underlying type (e.g., `number`) without needing the alias.
            let is_require_import = self
                .arena
                .get(import_eq.module_specifier)
                .is_some_and(|n| n.kind == SyntaxKind::StringLiteral as u16);
            if !is_require_import
                && !self.import_alias_targets_type_entity(import_eq.module_specifier)
            {
                return;
            }
        }

        // Only write indent if not already exported (caller handles indent for exported case)
        if !already_exported {
            self.write_indent();
        }
        if is_public_exported {
            self.write("export ");
        }
        if import_eq.is_type_only {
            self.write("import type ");
        } else {
            self.write("import ");
        }

        // Emit variable name from import_clause
        if import_eq.import_clause.is_some() {
            self.emit_node(import_eq.import_clause);
        }

        // Emit " = require(...)"
        if let Some(module_node) = self.arena.get(import_eq.module_specifier) {
            if module_node.kind == SyntaxKind::StringLiteral as u16 {
                self.write(" = require(");
                self.emit_node(import_eq.module_specifier);
                self.write(")");
            } else {
                self.write(" = ");
                self.emit_node(import_eq.module_specifier);
            }
        } else {
            self.write(" = ");
        }

        self.write(";");
        self.write_line();
    }

    fn emit_import_equals_declaration_without_export(&mut self, import_idx: NodeIndex) {
        let Some(import_node) = self.arena.get(import_idx) else {
            return;
        };
        let Some(import_eq) = self.arena.get_import_decl(import_node) else {
            return;
        };

        self.write_indent();
        self.write("import ");

        if import_eq.import_clause.is_some() {
            self.emit_node(import_eq.import_clause);
        }

        if let Some(module_node) = self.arena.get(import_eq.module_specifier) {
            if module_node.kind == SyntaxKind::StringLiteral as u16 {
                self.write(" = require(");
                self.emit_node(import_eq.module_specifier);
                self.write(")");
            } else {
                self.write(" = ");
                self.emit_node(import_eq.module_specifier);
            }
        } else {
            self.write(" = ");
        }

        self.write(";");
        self.write_line();
    }

    pub(crate) fn emit_namespace_export_declaration(&mut self, export_idx: NodeIndex) {
        let Some(export_node) = self.arena.get(export_idx) else {
            return;
        };
        let Some(export) = self.arena.get_export_decl(export_node) else {
            return;
        };

        // For "export as namespace" declarations:
        // - export_clause is the namespace name (identifier)

        self.write_indent();
        self.write("export as namespace ");

        // Emit namespace name from export_clause
        if export.export_clause.is_some() {
            self.emit_node(export.export_clause);
        }

        self.write(";");
        self.write_line();
    }

    // Helper methods

    pub(crate) fn emit_parameters(&mut self, params: &NodeList) {
        self.emit_parameters_with_body(params, NodeIndex::NONE);
    }

    pub(crate) fn emit_parameters_with_body(&mut self, params: &NodeList, body_idx: NodeIndex) {
        // Find the index of the last required parameter (no ?, no initializer, no rest).
        // Parameters with initializers before the last required param cannot use `?` syntax;
        // instead they emit `param: Type | undefined` (matching tsc behavior).
        let last_required_idx = params
            .nodes
            .iter()
            .rposition(|&idx| {
                self.arena
                    .get(idx)
                    .and_then(|n| self.arena.get_parameter(n))
                    .is_some_and(|p| {
                        !p.question_token && p.initializer.is_none() && !p.dot_dot_dot_token
                    })
            })
            .unwrap_or(0);

        let mut first = true;
        for (i, &param_idx) in params.nodes.iter().enumerate() {
            if !first {
                self.write(", ");
            }
            first = false;

            if let Some(param_node) = self.arena.get(param_idx)
                && let Some(param) = self.arena.get_parameter(param_node)
            {
                let jsdoc_param = if self.source_is_js_file {
                    self.jsdoc_param_decl_for_parameter(param_idx, i)
                } else {
                    None
                };
                let is_parameter_property = self.in_constructor_params
                    && self.parameter_has_property_modifier(&param.modifiers);

                // For public parameter properties, tsc appends `| undefined` to the
                // constructor parameter type as well as the property declaration.
                // For private/protected parameter properties, the type is hidden on
                // the property (`private x?;`) so no `| undefined` is added to the
                // constructor parameter.
                let is_private_param_property = is_parameter_property
                    && param.modifiers.as_ref().is_some_and(|mods| {
                        mods.nodes.iter().any(|&mod_idx| {
                            self.arena
                                .get(mod_idx)
                                .is_some_and(|n| n.kind == SyntaxKind::PrivateKeyword as u16)
                        })
                    });

                // Inline JSDoc comment before parameter (e.g. /** comment */ a: string)
                self.emit_inline_parameter_comment(param_node.pos);

                // Modifiers (public, private, etc for constructor parameters)
                self.emit_member_modifiers(&param.modifiers);

                // Rest parameter
                if param.dot_dot_dot_token || jsdoc_param.as_ref().is_some_and(|decl| decl.rest) {
                    self.write("...");
                }

                // Name
                self.emit_node(param.name);

                // A parameter with an initializer that appears before the last required
                // parameter is NOT optional — you can't omit it. Instead, its type
                // gets `| undefined` appended. Explicitly optional (?) params always use `?`.
                let has_initializer_before_required =
                    param.initializer.is_some() && !param.question_token && i < last_required_idx;

                if param.question_token
                    || jsdoc_param
                        .as_ref()
                        .is_some_and(|decl| decl.optional && !decl.rest)
                    || (param.initializer.is_some() && !has_initializer_before_required)
                {
                    self.write("?");
                }

                // Type
                if param.type_annotation.is_some() {
                    self.write(": ");
                    if let Some(rescued) = self.rescued_asserts_parameter_type_text(param_idx) {
                        self.write(&rescued);
                    } else {
                        self.emit_type(param.type_annotation);
                    }
                    // For non-private parameter properties with `?`, tsc appends
                    // `| undefined` to both the property declaration and the constructor
                    // parameter type. For private params, the type is hidden so skip.
                    if is_parameter_property && !is_private_param_property && param.question_token {
                        let output = self.writer.get_output();
                        if !output.ends_with("| undefined") {
                            self.write(" | undefined");
                        }
                    }
                } else if let Some(jsdoc_param) = jsdoc_param {
                    self.write(": ");
                    self.write(&jsdoc_param.type_text);
                } else if let Some(type_id) = self.get_node_type_or_names(&[param_idx, param.name])
                {
                    // Inferred type from type cache
                    self.write(": ");
                    self.write(&self.print_type_id(type_id));
                } else if param.initializer.is_some()
                    && let Some(type_text) = self.infer_fallback_type_text(param.initializer)
                {
                    self.write(": ");
                    self.write(&type_text);
                } else if param.dot_dot_dot_token {
                    // Rest parameters without explicit type → any[]
                    self.write(": any[]");
                } else if !self.source_is_declaration_file {
                    // Empty object binding pattern `{}` without a type annotation
                    // gets type `{}` (not `any`), matching tsc behavior.
                    let is_empty_object_binding = self.arena.get(param.name).is_some_and(|n| {
                        n.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                            && self
                                .arena
                                .get_binding_pattern(n)
                                .is_none_or(|bp| bp.elements.nodes.is_empty())
                    });
                    if is_empty_object_binding {
                        self.write(": {}");
                    } else {
                        // In declaration emit from source, parameters without
                        // explicit type annotations default to `any` (matching tsc)
                        self.write(": any");
                    }
                }

                // When strictNullChecks is true and a parameter has an
                // initializer before the last required parameter, tsc appends
                // `| undefined` — but only when the type doesn't already
                // include undefined (to avoid `T | undefined | undefined`).
                if self.strict_null_checks && has_initializer_before_required {
                    let output = self.writer.get_output();
                    if !output.ends_with("| undefined") {
                        self.write(" | undefined");
                    }
                }
            }
        }

        if self.should_emit_js_arguments_rest_param(params, body_idx) {
            if !first {
                self.write(", ");
            }
            self.write("...args: any[]");
        }
    }

    pub(crate) fn parameter_has_property_modifier(&self, modifiers: &Option<NodeList>) -> bool {
        modifiers.as_ref().is_some_and(|mods| {
            mods.nodes.iter().any(|&mod_idx| {
                self.arena.get(mod_idx).is_some_and(|mod_node| {
                    let kind = mod_node.kind;
                    kind == SyntaxKind::PublicKeyword as u16
                        || kind == SyntaxKind::PrivateKeyword as u16
                        || kind == SyntaxKind::ProtectedKeyword as u16
                        || kind == SyntaxKind::ReadonlyKeyword as u16
                        || kind == SyntaxKind::OverrideKeyword as u16
                })
            })
        })
    }

    /// Emit parameters without type annotations (used for private accessors)
    pub(crate) fn emit_parameters_without_types(&mut self, params: &NodeList, omit_types: bool) {
        if !omit_types {
            self.emit_parameters(params);
            return;
        }

        let mut first = true;
        for &param_idx in &params.nodes {
            if !first {
                self.write(", ");
            }
            first = false;

            if let Some(param_node) = self.arena.get(param_idx)
                && let Some(param) = self.arena.get_parameter(param_node)
            {
                // Rest parameter
                if param.dot_dot_dot_token {
                    self.write("...");
                }

                // Name only (no type)
                self.emit_node(param.name);

                // Optional marker still included
                if param.question_token {
                    self.write("?");
                }
            }
        }
    }

    fn should_emit_js_arguments_rest_param(&self, params: &NodeList, body_idx: NodeIndex) -> bool {
        if !self.source_is_js_file || body_idx.is_none() {
            return false;
        }

        let has_rest_param = params.nodes.iter().any(|&param_idx| {
            self.arena
                .get(param_idx)
                .and_then(|param_node| self.arena.get_parameter(param_node))
                .is_some_and(|param| param.dot_dot_dot_token)
        });
        if has_rest_param {
            return false;
        }

        tsz_parser::syntax::transform_utils::contains_arguments_reference(self.arena, body_idx)
    }

    pub(crate) fn emit_type_parameters(&mut self, type_params: &NodeList) {
        self.write("<");
        let mut first = true;
        for &param_idx in &type_params.nodes {
            if !first {
                self.write(", ");
            }
            first = false;

            if let Some(param_node) = self.arena.get(param_idx)
                && let Some(param) = self.arena.get_type_parameter(param_node)
            {
                // Inline JSDoc comment before type parameter
                self.emit_inline_parameter_comment(param_node.pos);

                // Emit variance/const modifiers (in, out, const)
                if let Some(ref mods) = param.modifiers {
                    for &mod_idx in &mods.nodes {
                        if let Some(mod_node) = self.arena.get(mod_idx) {
                            match mod_node.kind {
                                k if k == SyntaxKind::InKeyword as u16 => self.write("in "),
                                k if k == SyntaxKind::OutKeyword as u16 => self.write("out "),
                                k if k == SyntaxKind::ConstKeyword as u16 => self.write("const "),
                                _ => {}
                            }
                        }
                    }
                }

                self.emit_node(param.name);

                if param.constraint.is_some() {
                    self.write(" extends ");
                    self.emit_type(param.constraint);
                }

                if param.default.is_some() {
                    self.write(" = ");
                    self.emit_type(param.default);
                }
            }
        }
        self.write(">");
    }

    pub(crate) fn emit_heritage_clauses(&mut self, clauses: &NodeList) {
        self.emit_heritage_clauses_inner(clauses, false, None);
    }

    pub(crate) fn emit_class_heritage_clauses(
        &mut self,
        clauses: &NodeList,
        extends_alias: Option<&str>,
    ) {
        self.emit_heritage_clauses_inner(clauses, false, extends_alias);
    }

    pub(crate) fn emit_interface_heritage_clauses(&mut self, clauses: &NodeList) {
        self.emit_heritage_clauses_inner(clauses, true, None);
    }

    fn emit_heritage_clauses_inner(
        &mut self,
        clauses: &NodeList,
        is_interface: bool,
        extends_alias: Option<&str>,
    ) {
        for &clause_idx in &clauses.nodes {
            let Some(clause_node) = self.arena.get(clause_idx) else {
                continue;
            };
            let Some(heritage) = self.arena.get_heritage_clause(clause_node) else {
                continue;
            };

            let keyword = match heritage.token {
                k if k == SyntaxKind::ExtendsKeyword as u16 => "extends",
                k if k == SyntaxKind::ImplementsKeyword as u16 => "implements",
                _ => continue,
            };

            // For interfaces, filter out heritage types with non-entity-name
            // expressions (e.g. `typeof X`, parenthesized expressions).
            // tsc strips these in declaration emit.
            let valid_types: Vec<_> = if is_interface {
                heritage
                    .types
                    .nodes
                    .iter()
                    .copied()
                    .filter(|&type_idx| self.is_entity_name_heritage(type_idx))
                    .collect()
            } else {
                heritage.types.nodes.clone()
            };

            if valid_types.is_empty() {
                continue;
            }

            self.write(" ");
            self.write(keyword);
            self.write(" ");

            if heritage.token == SyntaxKind::ExtendsKeyword as u16
                && let Some(alias_name) = extends_alias
            {
                self.write(alias_name);
                if let Some(&type_idx) = valid_types.first()
                    && let Some(type_node) = self.arena.get(type_idx)
                    && let Some(expr) = self.arena.get_expr_type_args(type_node)
                    && let Some(ref type_args) = expr.type_arguments
                    && !type_args.nodes.is_empty()
                {
                    self.emit_type_arguments(type_args);
                }
                continue;
            }

            let mut first = true;
            for &type_idx in &valid_types {
                if !first {
                    self.write(", ");
                }
                first = false;
                self.emit_type(type_idx);
            }
        }
    }

    /// Check if a heritage type expression is an entity name (identifier or
    /// property access chain). Non-entity-name expressions like `typeof X` or
    /// parenthesized expressions are invalid in interface `extends` clauses
    /// and should be stripped in .d.ts output.
    pub(crate) fn is_entity_name_heritage(&self, type_idx: NodeIndex) -> bool {
        let Some(type_node) = self.arena.get(type_idx) else {
            return false;
        };
        // Heritage types may be wrapped in ExpressionWithTypeArguments (when
        // type args are present, e.g. `extends Foo<T>`), or may be bare
        // identifiers / property access chains (e.g. `extends A, B`).
        if let Some(eta) = self.arena.get_expr_type_args(type_node) {
            self.is_entity_name_expr(eta.expression)
        } else {
            self.is_entity_name_expr(type_idx)
        }
    }

    fn is_entity_name_expr(&self, expr_idx: NodeIndex) -> bool {
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };
        if expr_node.kind == SyntaxKind::Identifier as u16
            || expr_node.kind == SyntaxKind::NullKeyword as u16
        {
            return true;
        }
        if expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(access) = self.arena.get_access_expr(expr_node)
        {
            return self.is_entity_name_expr(access.expression);
        }
        false
    }

    /// Pre-scan a module body to determine if it has a "scope marker" —
    /// either an explicit `export {}` statement or a mix of exported and
    /// non-exported members. When true, `export` keywords should be preserved
    /// on individual members inside the ambient module.
    ///
    /// When `non_ambient` is true (non-ambient namespaces), only namespace
    /// declarations count as visible non-exported members. Other non-exported
    /// declarations (classes, interfaces, variables, etc.) are not emitted
    /// in the .d.ts output and should not trigger the scope marker.
    pub(crate) fn module_body_has_scope_marker(
        &self,
        stmts: &tsz_parser::parser::NodeList,
        non_ambient: bool,
    ) -> bool {
        let mut has_exported = false;
        let mut has_non_exported = false;

        for &stmt_idx in &stmts.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };

            match stmt_node.kind {
                k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                    if let Some(export) = self.arena.get_export_decl(stmt_node) {
                        // `export {}` — explicit scope marker
                        if let Some(clause_node) = self.arena.get(export.export_clause)
                            && clause_node.kind == syntax_kind_ext::NAMED_EXPORTS
                            && let Some(named) = self.arena.get_named_imports(clause_node)
                            && named.elements.nodes.is_empty()
                        {
                            return true;
                        }
                        // `export *` or `export * from "mod"` — scope marker
                        // (export_clause is None for bare `export *`)
                        if !export.export_clause.is_some()
                            || self
                                .arena
                                .get(export.export_clause)
                                .is_some_and(|n| n.kind == syntax_kind_ext::NAMESPACE_EXPORT)
                        {
                            return true;
                        }
                        // Check if export_clause wraps a declaration (e.g., `export class Foo`)
                        // — these count as exported members, not scope markers
                        if let Some(clause_node) = self.arena.get(export.export_clause) {
                            let ck = clause_node.kind;
                            if ck == syntax_kind_ext::CLASS_DECLARATION
                                || ck == syntax_kind_ext::FUNCTION_DECLARATION
                                || ck == syntax_kind_ext::INTERFACE_DECLARATION
                                || ck == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                                || ck == syntax_kind_ext::ENUM_DECLARATION
                                || ck == syntax_kind_ext::VARIABLE_STATEMENT
                                || ck == syntax_kind_ext::MODULE_DECLARATION
                                || ck == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                            {
                                has_exported = true;
                                if non_ambient
                                    && ck == syntax_kind_ext::CLASS_DECLARATION
                                    && let Some(class) = self.arena.get_class(clause_node)
                                    && class
                                        .heritage_clauses
                                        .as_ref()
                                        .and_then(|heritage| {
                                            self.non_nameable_extends_heritage_type(heritage)
                                        })
                                        .is_some()
                                {
                                    has_non_exported = true;
                                }
                            } else {
                                // Named exports like `export { a, b }` — scope marker
                                return true;
                            }
                        }
                    }
                }
                k if k == syntax_kind_ext::EXPORT_ASSIGNMENT => {
                    // `export = value` or `export default` — scope marker
                    return true;
                }
                _ => {
                    if self.stmt_has_export_modifier(stmt_node) {
                        has_exported = true;
                        if non_ambient
                            && stmt_node.kind == syntax_kind_ext::CLASS_DECLARATION
                            && let Some(class) = self.arena.get_class(stmt_node)
                            && class
                                .heritage_clauses
                                .as_ref()
                                .and_then(|heritage| {
                                    self.non_nameable_extends_heritage_type(heritage)
                                })
                                .is_some()
                        {
                            has_non_exported = true;
                        }
                    } else {
                        // Skip ImportDeclaration and ImportEqualsDeclaration
                        // as they don't count as non-exported members
                        if stmt_node.kind == syntax_kind_ext::IMPORT_DECLARATION
                            || stmt_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                        {
                            continue;
                        }
                        // In non-ambient namespaces, non-exported declarations
                        // are only emitted in .d.ts if they are referenced by
                        // exported members (via used_symbols). Namespace
                        // declarations are always visible.
                        if non_ambient {
                            if stmt_node.kind == syntax_kind_ext::MODULE_DECLARATION
                                || self.is_ns_member_used_by_exports(stmt_idx)
                            {
                                has_non_exported = true;
                            }
                        } else {
                            has_non_exported = true;
                        }
                    }
                }
            }

            if has_exported && has_non_exported {
                return true;
            }
        }

        false
    }

    pub(crate) fn emit_member_modifiers(&mut self, modifiers: &Option<NodeList>) {
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.arena.get(mod_idx) {
                    match mod_node.kind {
                        // In constructor parameters, strip accessibility and readonly modifiers
                        k if k == SyntaxKind::PublicKeyword as u16 => {
                            // In .d.ts files, `public` is the default and is omitted by tsc.
                            // Only emit it for constructor parameter properties
                            // (which is handled separately and already skips it).
                        }
                        k if k == SyntaxKind::PrivateKeyword as u16 => {
                            if !self.in_constructor_params {
                                self.write("private ");
                            }
                        }
                        k if k == SyntaxKind::ProtectedKeyword as u16 => {
                            if !self.in_constructor_params {
                                self.write("protected ");
                            }
                        }
                        k if k == SyntaxKind::ReadonlyKeyword as u16 => {
                            if !self.in_constructor_params {
                                self.write("readonly ");
                            }
                        }
                        k if k == SyntaxKind::StaticKeyword as u16 => self.write("static "),
                        k if k == SyntaxKind::AbstractKeyword as u16 => self.write("abstract "),
                        k if k == SyntaxKind::OverrideKeyword as u16 => {
                            // tsc strips `override` in .d.ts output — it is not
                            // part of the declaration surface.
                        }
                        k if k == SyntaxKind::AsyncKeyword as u16 => {
                            // tsc strips `async` in .d.ts — the return type already
                            // encodes Promise<T>, so the modifier is redundant.
                        }
                        k if k == SyntaxKind::AccessorKeyword as u16 => self.write("accessor "),
                        k if k == SyntaxKind::DeclareKeyword as u16 => {
                            // tsc strips `declare` from class members in .d.ts — it is
                            // only meaningful at the top-level statement level
                            // (`declare class`, `declare function`, etc.).
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}
