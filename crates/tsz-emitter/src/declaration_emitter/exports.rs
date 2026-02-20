//! Declaration emitter - export and import emission
//!
//! Extracted from `mod.rs`: export declarations, export specifiers, import
//! declarations, import filtering, module declarations, and import equals.

use super::DeclarationEmitter;
use crate::enums::evaluator::{EnumEvaluator, EnumValue};
use rustc_hash::FxHashSet;
use tracing::debug;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;
use tsz_solver::type_queries;

impl<'a> DeclarationEmitter<'a> {
    pub(crate) fn emit_export_declaration(&mut self, export_idx: NodeIndex) {
        let Some(export_node) = self.arena.get(export_idx) else {
            return;
        };
        let Some(export) = self.arena.get_export_decl(export_node) else {
            return;
        };

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
                    _ => {}
                }
            }

            self.emit_export_default_expression(export.export_clause);
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
                    self.emit_module_declaration(export.export_clause);
                    self.public_api_scope_depth = prev_public_api_scope_depth;
                    return;
                }
                k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                    // Emit: export import x = require(...)
                    self.write_indent();
                    self.write("export ");
                    self.emit_import_equals_declaration(export.export_clause, true);
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
                    self.emit_named_exports(export.export_clause, !export.is_type_only);
                } else if clause_node.kind == SyntaxKind::Identifier as u16 {
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
        }

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
                // Direct export of declaration
                self.write("export default ");
                self.emit_node(assign.expression);
                self.write(";");
                self.write_line();
            } else {
                // Value expression - synthesize _default variable
                // First, emit the synthesized variable with inferred type
                self.write_indent();
                self.write("declare const _default: ");

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
                self.write("export default _default;");
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

        if let Some(ref type_params) = func.type_parameters
            && !type_params.nodes.is_empty()
        {
            self.emit_type_parameters(type_params);
        }

        self.write("(");
        self.emit_parameters(&func.parameters);
        self.write(")");

        if func.type_annotation.is_some() {
            self.write(": ");
            self.emit_type(func.type_annotation);
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

        let is_abstract = self.has_modifier(&class.modifiers, SyntaxKind::AbstractKeyword as u16);

        self.write_indent();
        self.write("export default ");
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
            self.emit_heritage_clauses(heritage);
        }

        self.write(" {");
        self.write_line();
        self.increase_indent();

        for &member_idx in &class.members.nodes {
            self.emit_class_member(member_idx);
        }

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();
    }

    pub(crate) fn emit_export_default_expression(&mut self, expr_idx: NodeIndex) {
        // Synthesize a _default variable for expression exports
        // First, emit: declare const _default: <type>;
        self.write_indent();
        self.write("declare const _default: ");

        // Get the type of the expression
        if let Some(type_id) = self.get_node_type(expr_idx) {
            self.write(&self.print_type_id(type_id));
        } else {
            self.write("any");
        }

        self.write(";");
        self.write_line();

        // Then, emit: export default _default;
        self.write_indent();
        self.write("export default _default;");
        self.write_line();
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

        self.write("{ ");
        let mut first = true;
        for &spec_idx in &exports.elements.nodes {
            if !first {
                self.write(", ");
            }
            first = false;
            self.emit_export_specifier(spec_idx, allow_type_prefix);
        }
        self.write(" }");
    }

    pub(crate) fn emit_export_specifier(&mut self, spec_idx: NodeIndex, allow_type_prefix: bool) {
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
        if !self.inside_declare_namespace {
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
            self.emit_interface_member(member_idx);
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

        let is_abstract = self.has_modifier(&class.modifiers, SyntaxKind::AbstractKeyword as u16);

        self.write_indent();
        if !self.inside_declare_namespace {
            self.write("export declare ");
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
            self.emit_heritage_clauses(heritage);
        }

        self.write(" {");
        self.write_line();
        self.increase_indent();

        // Reset constructor and method overload tracking for this class
        self.class_has_constructor_overloads = false;
        self.method_names_with_overloads = FxHashSet::default();

        // Emit parameter properties from constructor first (before other members)
        self.emit_parameter_properties(&class.members);

        for &member_idx in &class.members.nodes {
            self.emit_class_member(member_idx);
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
        if !self.inside_declare_namespace {
            self.write("export declare ");
        }
        self.write("function ");
        self.emit_node(func.name);

        if let Some(ref type_params) = func.type_parameters
            && !type_params.nodes.is_empty()
        {
            self.emit_type_parameters(type_params);
        }

        self.write("(");
        self.emit_parameters(&func.parameters);
        self.write(")");

        if func.type_annotation.is_some() {
            self.write(": ");
            self.emit_type(func.type_annotation);
        } else if let (Some(interner), Some(cache)) = (&self.type_interner, &self.type_cache) {
            // No explicit return type, try to infer it
            if let Some(func_type_id) = cache.node_types.get(&func_idx.0)
                && let Some(return_type_id) =
                    type_queries::get_return_type(*interner, *func_type_id)
            {
                self.write(": ");
                self.write(&self.print_type_id(return_type_id));
            }
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
        if !self.inside_declare_namespace {
            self.write("export ");
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

        self.write_indent();
        if !self.inside_declare_namespace {
            self.write("export declare ");
        }
        self.write("enum ");
        self.emit_node(enum_data.name);

        self.write(" {");
        self.write_line();
        self.increase_indent();

        // Evaluate enum member values to get correct auto-increment behavior
        let mut evaluator = EnumEvaluator::new(self.arena);
        let member_values = evaluator.evaluate_enum(enum_idx);

        for (i, &member_idx) in enum_data.members.nodes.iter().enumerate() {
            self.write_indent();
            if let Some(member_node) = self.arena.get(member_idx)
                && let Some(member) = self.arena.get_enum_member(member_node)
            {
                self.emit_node(member.name);
                // Always emit the evaluated value to match TypeScript behavior
                self.write(" = ");
                let member_name = self.get_enum_member_name(member.name);
                if let Some(value) = member_values.get(&member_name) {
                    self.emit_enum_value(value);
                } else {
                    // Fallback to index if evaluation failed
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
                self.write(&format!(
                    "\"{}\"",
                    s.replace('\\', "\\\\").replace('"', "\\\"")
                ));
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
                let flags = decl_list_node.flags as u32;
                let keyword = if flags & tsz_parser::parser::node_flags::CONST != 0 {
                    "const"
                } else if flags & tsz_parser::parser::node_flags::LET != 0 {
                    "let"
                } else {
                    "var"
                };

                for &decl_idx in &decl_list.declarations.nodes {
                    self.write_indent();
                    // Don't emit 'export' or 'declare' keywords inside a declare namespace
                    if !self.inside_declare_namespace {
                        self.write("export declare ");
                    }
                    self.write(keyword);
                    self.write(" ");

                    if let Some(decl_node) = self.arena.get(decl_idx)
                        && let Some(decl) = self.arena.get_variable_declaration(decl_node)
                    {
                        self.emit_node(decl.name);

                        // Determine if we should emit a literal initializer for const
                        let use_literal_initializer = if keyword == "const"
                            && decl.type_annotation.is_none()
                            && decl.initializer.is_some()
                        {
                            // Check if initializer is a primitive literal
                            if let Some(init_node) = self.arena.get(decl.initializer) {
                                let k = init_node.kind;
                                k == SyntaxKind::StringLiteral as u16
                                    || k == SyntaxKind::NumericLiteral as u16
                                    || k == SyntaxKind::TrueKeyword as u16
                                    || k == SyntaxKind::FalseKeyword as u16
                                    || k == SyntaxKind::NullKeyword as u16
                            } else {
                                false
                            }
                        } else {
                            false
                        };

                        // Emit literal initializer for const with primitive literals
                        if use_literal_initializer {
                            self.write(" = ");
                            self.emit_expression(decl.initializer);
                        } else {
                            // Check for unique symbol case: const x = Symbol()
                            let is_unique_symbol = keyword == "const"
                                && decl.initializer.is_some()
                                && self.is_symbol_call(decl.initializer);

                            if decl.type_annotation.is_some() {
                                self.write(": ");
                                self.emit_type(decl.type_annotation);
                            } else if is_unique_symbol {
                                // const x = Symbol() gets : unique symbol
                                self.write(": unique symbol");
                            } else if let Some(type_id) =
                                self.get_node_type_or_names(&[decl_idx, decl.name])
                            {
                                // No explicit type, but we have inferred type from cache
                                self.write(": ");
                                self.write(&self.print_type_id(type_id));
                            }
                        }
                    }

                    self.write(";");
                    self.write_line();
                }
            }
        }
    }

    pub(crate) fn emit_import_declaration_if_needed(&mut self, import_idx: NodeIndex) {
        let Some(import_node) = self.arena.get(import_idx) else {
            return;
        };
        let Some(import) = self.arena.get_import_decl(import_node) else {
            return;
        };

        // Check if this import is being handled by the elision system
        // by checking if any of its imported symbols are in import_symbol_map
        let mut has_elided_symbols = false;
        if import.import_clause.is_some() {
            let binder = match &self.binder {
                Some(b) => b,
                None => {
                    // No binder - fall back to emitting the import
                    self.emit_import_declaration(import_idx);
                    return;
                }
            };

            // Collect symbols from this import clause
            let symbols =
                self.collect_imported_symbols_from_clause(self.arena, binder, import.import_clause);

            // Check if any symbol is in import_symbol_map (meaning it's being elided)
            for (_name, sym_id) in symbols {
                if self.import_symbol_map.contains_key(&sym_id) {
                    has_elided_symbols = true;
                    break;
                }
            }
        }

        if has_elided_symbols {
            // This import is being handled by the elision system via emit_auto_imports
            // Skip emitting it here to avoid duplicates
            debug!(
                "[DEBUG] emit_import_declaration_if_needed: skipping import (handled by elision)"
            );
        } else {
            // Not handled by elision - emit normally
            self.emit_import_declaration(import_idx);
        }
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
            self.emit_import_specifier(spec_idx, allow_type_prefix);
        }
        self.write(" }");
    }

    pub(crate) fn emit_import_specifier(&mut self, spec_idx: NodeIndex, allow_type_prefix: bool) {
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

    pub(crate) fn emit_module_declaration(&mut self, module_idx: NodeIndex) {
        let Some(module_node) = self.arena.get(module_idx) else {
            return;
        };
        let Some(module) = self.arena.get_module(module_node) else {
            return;
        };

        let is_exported = self.has_export_modifier(&module.modifiers);
        if !self.should_emit_public_api_module(is_exported) {
            return;
        }

        self.write_indent();
        // Don't emit 'export' or 'declare' inside a declare namespace
        if !self.inside_declare_namespace {
            if is_exported {
                self.write("export ");
            }
            self.write("declare ");
        }

        // Determine keyword: "module" for string literals, "namespace" for identifiers
        let use_module_keyword = self
            .arena
            .get(module.name)
            .is_some_and(|name_node| name_node.kind == SyntaxKind::StringLiteral as u16);

        self.write(if use_module_keyword {
            "module "
        } else {
            "namespace "
        });
        self.emit_node(module.name);

        if module.body.is_some() {
            self.write(" {");
            self.write_line();
            self.increase_indent();

            // Inside a declare namespace, don't emit 'declare' keyword for members
            let prev_inside_declare_namespace = self.inside_declare_namespace;
            self.inside_declare_namespace = true;
            let prev_public_api_scope_depth = self.public_api_scope_depth;
            if is_exported {
                self.public_api_scope_depth += 1;
            }

            if let Some(body_node) = self.arena.get(module.body) {
                if let Some(module_block) = self.arena.get_module_block(body_node) {
                    if let Some(ref stmts) = module_block.statements {
                        for &stmt_idx in &stmts.nodes {
                            self.emit_statement(stmt_idx);
                        }
                    }
                } else {
                    // Nested namespace: module A.B is represented as ModuleDeclaration with body = ModuleDeclaration of C
                    if let Some(_nested_module) = self.arena.get_module(body_node) {
                        self.emit_module_declaration(module.body);
                    }
                }
            }

            self.public_api_scope_depth = prev_public_api_scope_depth;
            self.inside_declare_namespace = prev_inside_declare_namespace;
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

        let is_exported = self.has_export_modifier(&import_eq.modifiers);
        let is_public_exported = is_exported && !already_exported;

        // Only write indent if not already exported (caller handles indent for exported case)
        if !already_exported {
            self.write_indent();
        }
        if is_public_exported {
            self.write("export ");
        }
        self.write("import ");

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
        let mut first = true;
        for &param_idx in &params.nodes {
            if !first {
                self.write(", ");
            }
            first = false;

            if let Some(param_node) = self.arena.get(param_idx)
                && let Some(param) = self.arena.get_parameter(param_node)
            {
                // Modifiers (public, private, etc for constructor parameters)
                self.emit_member_modifiers(&param.modifiers);

                // Rest parameter
                if param.dot_dot_dot_token {
                    self.write("...");
                }

                // Name
                self.emit_node(param.name);

                // Optional
                if param.question_token {
                    self.write("?");
                }

                // Type
                if param.type_annotation.is_some() {
                    self.write(": ");
                    self.emit_type(param.type_annotation);
                }
            }
        }
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

            self.write(" ");
            self.write(keyword);
            self.write(" ");

            let mut first = true;
            for &type_idx in &heritage.types.nodes {
                if !first {
                    self.write(", ");
                }
                first = false;
                self.emit_type(type_idx);
            }
        }
    }

    pub(crate) fn emit_member_modifiers(&mut self, modifiers: &Option<NodeList>) {
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.arena.get(mod_idx) {
                    match mod_node.kind {
                        // In constructor parameters, strip accessibility and readonly modifiers
                        k if k == SyntaxKind::PublicKeyword as u16 => {
                            if !self.in_constructor_params {
                                self.write("public ");
                            }
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
                        k if k == SyntaxKind::AsyncKeyword as u16 => self.write("async "),
                        k if k == SyntaxKind::AccessorKeyword as u16 => self.write("accessor "),
                        _ => {}
                    }
                }
            }
        }
    }
}
