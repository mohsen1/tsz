//! Declaration emitter - export and import emission.

use super::DeclarationEmitter;
use crate::enums::evaluator::{EnumEvaluator, EnumValue};
use rustc_hash::FxHashSet;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
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

    pub(super) fn emit_declaration_import_attributes(&mut self, attributes: NodeIndex) {
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

        // For JS source files, `export default <Identifier>` referencing a
        // top-level local declaration is hoisted to the very top of the .d.ts
        // ahead of the main statement loop. Suppress the in-source statement
        // here so it isn't duplicated.
        if export.is_default_export
            && self.source_is_js_file
            && export.export_clause.is_some()
            && let Some(expr_node) = self.arena.get(export.export_clause)
            && expr_node.kind == SyntaxKind::Identifier as u16
            && let Some(ident) = self.arena.get_identifier(expr_node)
            && self
                .emitted_js_export_default_names
                .contains(&ident.escaped_text)
        {
            return;
        }

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

    /// Emit CJS export aliases as `export { local as exported };` lines.
    pub(crate) fn emit_js_cjs_export_aliases(&mut self) {
        if self.js_cjs_export_aliases.is_empty() {
            return;
        }
        let aliases = self.js_cjs_export_aliases.clone();
        for (export_name, local_name) in &aliases {
            self.write_indent();
            self.write("export { ");
            self.write(local_name);
            self.write(" as ");
            self.write(export_name);
            self.write(" };");
            self.write_line();
            self.emitted_scope_marker = true;
            self.emitted_module_indicator = true;
        }
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
            // For non-entity-name expressions (object/array literals, calls,
            // primitives), tsc synthesizes a `_default` const with the inferred
            // type and emits `export = _default`. Mirror that for parity with
            // declarationEmitInferredDefaultExportType2.
            if !self.source_is_js_file
                && !self.export_equals_expression_emits_directly(assign.expression)
            {
                let var_name = self.unique_default_export_name();
                self.write("declare const ");
                self.write(&var_name);
                self.write(": ");
                if let Some(type_id) = self.get_node_type(assign.expression) {
                    self.write(&self.print_type_id(type_id));
                } else {
                    self.write("any");
                }
                self.write(";");
                self.write_line();
                self.write_indent();
                self.write("export = ");
                self.write(&var_name);
                self.write(";");
                self.write_line();
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
                // TS2883: Check for non-portable inferred type references
                // in `export default <identifier>` expressions.
                if let Some(file_path) = self.current_file_path.clone()
                    && let Some(type_id) = self
                        .get_node_type_or_names(&[assign.expression])
                        .or_else(|| self.get_type_via_symbol(assign.expression))
                {
                    self.emit_non_portable_type_diagnostic(
                        type_id,
                        "default",
                        &file_path,
                        assign_node.pos,
                        assign_node.end - assign_node.pos,
                    );
                }

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
                            let arg_type_text =
                                self.preferred_expression_type_text(arg_idx).or_else(|| {
                                    self.get_node_type_or_names(&[arg_idx])
                                        .map(|type_id| self.print_type_id(type_id))
                                });
                            if let Some(arg_type_text) = arg_type_text
                                && arg_type_text.starts_with("import(\"")
                                && self.emit_non_portable_import_type_text_diagnostics(
                                    &arg_type_text,
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
        } else if let Some(return_type_text) = self
            .js_function_body_preferred_return_text_for_declaration(
                func.body,
                func.name,
                &func.parameters,
            )
        {
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
                    let printed_type_text = self.print_type_id(return_type_id);
                    self.write(": ");
                    self.write(&printed_type_text);
                    if let Some(name_text) = self.get_identifier_text(func_name)
                        && let Some(name_node) = self.arena.get(func.name)
                        && let Some(file_path) = self.current_file_path.clone()
                    {
                        let _ = self.emit_non_portable_import_type_text_diagnostics(
                            &printed_type_text,
                            &name_text,
                            &file_path,
                            name_node.pos,
                            name_node.end - name_node.pos,
                        );
                    }
                }
            } else if func_body.is_some() {
                if self.body_returns_void(func_body) {
                    self.write(": void");
                } else if let Some(return_text) =
                    self.function_body_preferred_return_type_text(func_body)
                {
                    if let Some(returned_identifier) =
                        self.function_body_unique_return_identifier(func_body)
                        && let Some(return_type_id) =
                            self.reference_declared_type_id(returned_identifier)
                        && let Some(name_text) = self.get_identifier_text(func.name)
                        && let Some(name_node) = self.arena.get(func.name)
                        && let Some(file_path) = self.current_file_path.clone()
                    {
                        self.check_non_portable_type_references(
                            return_type_id,
                            &name_text,
                            &file_path,
                            name_node.pos,
                            name_node.end - name_node.pos,
                        );
                    }
                    if let Some(name_text) = self.get_identifier_text(func.name)
                        && let Some(name_node) = self.arena.get(func.name)
                        && let Some(file_path) = self.current_file_path.clone()
                        && let Some(func_type_id) =
                            self.get_type_via_symbol_for_func(func_idx, func_name)
                    {
                        self.check_non_portable_type_references(
                            func_type_id,
                            &name_text,
                            &file_path,
                            name_node.pos,
                            name_node.end - name_node.pos,
                        );
                    }
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
                if let Some(returned_identifier) =
                    self.function_body_unique_return_identifier(func_body)
                    && let Some(return_type_id) =
                        self.reference_declared_type_id(returned_identifier)
                    && let Some(name_text) = self.get_identifier_text(func.name)
                    && let Some(name_node) = self.arena.get(func.name)
                    && let Some(file_path) = self.current_file_path.clone()
                {
                    self.check_non_portable_type_references(
                        return_type_id,
                        &name_text,
                        &file_path,
                        name_node.pos,
                        name_node.end - name_node.pos,
                    );
                }
                if let Some(name_text) = self.get_identifier_text(func.name)
                    && let Some(name_node) = self.arena.get(func.name)
                    && let Some(file_path) = self.current_file_path.clone()
                    && let Some(func_type_id) =
                        self.get_type_via_symbol_for_func(func_idx, func_name)
                {
                    self.check_non_portable_type_references(
                        func_type_id,
                        &name_text,
                        &file_path,
                        name_node.pos,
                        name_node.end - name_node.pos,
                    );
                }
                self.write(": ");
                self.write(&return_text);
            }
        }

        self.write(";");
        self.write_line();
        if self.source_is_js_file {
            self.emit_js_function_like_class_if_needed(
                func.name,
                &func.parameters,
                func.body,
                true,
                func_idx,
            );
            self.emit_js_namespace_export_aliases_for_name(func.name);
        }
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
        self.class_extends_another = class.heritage_clauses.as_ref().is_some_and(|hc| {
            hc.nodes.iter().any(|&clause_idx| {
                self.arena
                    .get_heritage_clause_at(clause_idx)
                    .is_some_and(|h| h.token == SyntaxKind::ExtendsKeyword as u16)
            })
        });
        self.method_names_with_overloads = rustc_hash::FxHashSet::default();

        // Suppress method implementations that share a computed name with
        // an accessor (tsc emits only the accessor in .d.ts).
        let shadowed = self.computed_names_shadowed_by_accessors(&class.members);
        self.method_names_with_overloads.extend(shadowed);

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
                    let arg_type_text =
                        self.preferred_expression_type_text(arg_idx).or_else(|| {
                            self.get_node_type_or_names(&[arg_idx])
                                .map(|type_id| self.print_type_id(type_id))
                        });
                    if let Some(arg_type_text) = arg_type_text
                        && arg_type_text.starts_with("import(\"")
                        && self.emit_non_portable_import_type_text_diagnostics(
                            &arg_type_text,
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

        let portability_context = self.current_file_path.as_ref().map(|file_path| {
            let (pos, len) = self
                .arena
                .get(export_idx)
                .map(|export_node| (export_node.pos, export_node.end - export_node.pos))
                .unwrap_or_else(|| {
                    let expr_node = self.arena.get(expr_idx).expect("export expr node");
                    (expr_node.pos, expr_node.end - expr_node.pos)
                });
            (file_path.clone(), pos, len, self.diagnostics.len())
        });

        // Default exports are const-like — preserve literal types for simple literals
        if let Some(literal_text) = self.const_literal_initializer_text_deep(expr_idx) {
            self.write(": ");
            self.write(&literal_text);
        } else if let Some(type_text) = self.preferred_expression_type_text(expr_idx) {
            if let Some((file_path, pos, len, diagnostics_before)) = portability_context.as_ref()
                && self.diagnostics.len() == *diagnostics_before
                && type_text.starts_with("import(\"")
                && self.import_type_uses_private_package_subpath(&type_text)
            {
                let _ = self.emit_non_portable_import_type_text_diagnostics(
                    &type_text, "default", file_path, *pos, *len,
                );
                self.emit_non_portable_initializer_declaration_diagnostics(
                    expr_idx, "default", file_path, *pos, *len,
                );
            }
            self.write(": ");
            self.write(&type_text);
        } else if let Some(type_id) = self.get_node_type(expr_idx) {
            let printed_type = self.print_type_id(type_id);
            if let Some((file_path, pos, len, diagnostics_before)) = portability_context.as_ref()
                && self.diagnostics.len() == *diagnostics_before
                && printed_type.starts_with("import(\"")
                && self.import_type_uses_private_package_subpath(&printed_type)
            {
                let _ = self.emit_non_portable_import_type_text_diagnostics(
                    &printed_type,
                    "default",
                    file_path,
                    *pos,
                    *len,
                );
                self.emit_non_portable_initializer_declaration_diagnostics(
                    expr_idx, "default", file_path, *pos, *len,
                );
            }
            self.write(": ");
            self.write(&printed_type);
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

    /// Whether `export = <expr>` can emit `<expr>` directly. True for entity
    /// names (Identifier, qualified `PropertyAccess`), false for value
    /// expressions (object/array literals, calls, primitives) which require
    /// synthesizing a `_default` const with the inferred type.
    fn export_equals_expression_emits_directly(&self, expr_idx: NodeIndex) -> bool {
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };
        match expr_node.kind {
            k if k == SyntaxKind::Identifier as u16 => true,
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                self.arena.get_access_expr(expr_node).is_some_and(|access| {
                    self.export_equals_expression_emits_directly(access.expression)
                })
            }
            _ => false,
        }
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
        self.class_extends_another = class.heritage_clauses.as_ref().is_some_and(|hc| {
            hc.nodes.iter().any(|&clause_idx| {
                self.arena
                    .get_heritage_clause_at(clause_idx)
                    .is_some_and(|h| h.token == SyntaxKind::ExtendsKeyword as u16)
            })
        });
        self.method_names_with_overloads = FxHashSet::default();

        // Suppress method implementations that share a computed name with
        // an accessor (tsc emits only the accessor in .d.ts).
        let shadowed = self.computed_names_shadowed_by_accessors(&class.members);
        self.method_names_with_overloads.extend(shadowed);

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
        let late_bound_members = self.collect_ts_late_bound_assignment_members(func.name);

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
        } else if let Some(return_type_text) = self
            .js_function_body_preferred_return_text_for_declaration(
                func.body,
                func.name,
                &func.parameters,
            )
        {
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
                    if let Some(name_text) = self.get_identifier_text(func_name)
                        && let Some(name_node) = self.arena.get(func_name)
                        && let Some(file_path) = self.current_file_path.clone()
                    {
                        self.check_non_portable_type_references(
                            return_type_id,
                            &name_text,
                            &file_path,
                            name_node.pos,
                            name_node.end - name_node.pos,
                        );
                    }
                    self.write(": ");
                    let printed_type_text = self.print_type_id(return_type_id);
                    self.write(&printed_type_text);
                    if let Some(name_text) = self.get_identifier_text(func_name)
                        && let Some(name_node) = self.arena.get(func_name)
                        && let Some(file_path) = self.current_file_path.clone()
                    {
                        let _ = self.emit_non_portable_import_type_text_diagnostics(
                            &printed_type_text,
                            &name_text,
                            &file_path,
                            name_node.pos,
                            name_node.end - name_node.pos,
                        );
                    }
                }
            } else if func_body.is_some() && self.body_returns_void(func_body) {
                self.write(": void");
            }
        } else if func_body.is_some() && self.body_returns_void(func_body) {
            self.write(": void");
        }

        self.write(";");
        self.write_line();
        self.emit_ts_late_bound_function_namespace_from_members(
            func.name,
            true,
            &late_bound_members,
        );
        if self.source_is_js_file {
            self.emit_js_function_like_class_if_needed(
                func.name,
                &func.parameters,
                func.body,
                true,
                func_idx,
            );
            self.emit_js_namespace_export_aliases_for_name(func.name);
        }
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
        if self.arena.is_declare(&alias.modifiers) && !self.inside_declare_namespace {
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
                let js_var_promoted_to_const;
                let keyword = if flags
                    & (tsz_parser::parser::node_flags::USING
                        | tsz_parser::parser::node_flags::CONST)
                    != 0
                {
                    js_var_promoted_to_const = false;
                    "const"
                } else if flags & tsz_parser::parser::node_flags::LET != 0 {
                    js_var_promoted_to_const = false;
                    "let"
                } else if self.source_is_js_file {
                    js_var_promoted_to_const = true;
                    "const"
                } else {
                    js_var_promoted_to_const = false;
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
                    // For JS `var` promoted to `const`, revert to `var` if
                    // any declaration has a JSDoc @type annotation.
                    let effective_keyword = if js_var_promoted_to_const {
                        let has_jsdoc = regular_decls.iter().any(|(decl_idx, decl)| {
                            self.jsdoc_name_like_type_expr_for_node(*decl_idx).is_some()
                                || self.jsdoc_name_like_type_expr_for_node(decl.name).is_some()
                        }) || self
                            .jsdoc_name_like_type_expr_for_pos(stmt_node.pos)
                            .is_some();
                        if has_jsdoc { "var" } else { keyword }
                    } else {
                        keyword
                    };
                    self.write(effective_keyword);
                    self.write(" ");

                    for (i, (decl_idx, decl)) in regular_decls.iter().enumerate() {
                        if i > 0 {
                            self.write(", ");
                        }

                        self.emit_node(decl.name);
                        // When a variable's initializer is a simple reference to an
                        // import-equals alias (e.g. `var bVal2 = b` where `import b = a.foo`),
                        // tsc emits `typeof b` instead of expanding the type.
                        if !decl.type_annotation.is_some()
                            && decl.initializer.is_some()
                            && let Some(alias_text) =
                                self.initializer_import_alias_typeof_text(decl.initializer)
                        {
                            self.write(": typeof ");
                            self.write(&alias_text);
                        } else {
                            self.emit_variable_decl_type_or_initializer(
                                keyword,
                                stmt_node.pos,
                                *decl_idx,
                                decl.name,
                                decl.type_annotation,
                                decl.initializer,
                            );
                        }
                    }

                    self.write(";");
                    self.write_line();
                }
            }
        }
    }
}

mod imports_and_modules;
