use super::is_valid_identifier_name;
use super::{ModuleKind, Printer};
use crate::parser::syntax_kind_ext;
use crate::parser::node::Node;
use crate::parser::{NodeIndex, NodeList};
use crate::scanner::SyntaxKind;
use crate::transform_context::IdentifierId;
use crate::transforms::class_es5::ClassES5Emitter;

impl<'a> Printer<'a> {
    pub(super) fn emit_commonjs_export<F>(
        &mut self,
        names: &[IdentifierId],
        is_default: bool,
        mut emit_inner: F,
    ) where
        F: FnMut(&mut Self),
    {
        if names.is_empty() {
            emit_inner(self);
            return;
        }

        let prev_module = self.ctx.options.module;
        self.ctx.options.module = ModuleKind::None;

        emit_inner(self);

        self.ctx.options.module = prev_module;

        self.write_line();
        if is_default {
            self.write("exports.default = ");
            self.write_identifier_by_id(names[0]);
            self.write(";");
        } else {
            for (i, name) in names.iter().enumerate() {
                if i > 0 {
                    self.write_line();
                }
                self.write("exports.");
                self.write_identifier_by_id(*name);
                self.write(" = ");
                self.write_identifier_by_id(*name);
                self.write(";");
            }
        }
        self.write_line();
    }

    pub(super) fn emit_commonjs_default_export_expr(&mut self, node: &Node, idx: NodeIndex) {
        self.emit_commonjs_default_export_assignment(|this| {
            this.emit_commonjs_default_export_expr_inner(node, idx);
        });
    }

    pub(super) fn emit_commonjs_default_export_expr_inner(
        &mut self,
        node: &Node,
        idx: NodeIndex,
    ) {
        match node.kind {
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                self.emit_function_expression(node, idx);
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                self.emit_class_es6(node, idx);
            }
            _ => {
                self.emit_node_default(node, idx);
            }
        }
    }

    pub(super) fn emit_commonjs_default_export_assignment<F>(&mut self, mut emit_inner: F)
    where
        F: FnMut(&mut Self),
    {
        self.write("exports.default = ");
        emit_inner(self);
        self.write_semicolon();
        self.write_line();
    }

    pub(super) fn emit_commonjs_default_export_class_es5(&mut self, class_node: NodeIndex) {
        let Some(node) = self.arena.get(class_node) else {
            return;
        };

        if node.kind != syntax_kind_ext::CLASS_DECLARATION {
            self.emit_node_default(node, class_node);
            return;
        }

        let temp_name = format!("{}_default", self.get_temp_var_name());
        let mut es5_emitter = ClassES5Emitter::new(self.arena);
        es5_emitter.set_indent_level(self.writer.indent_level());
        if let Some(text) = self.source_text_for_map() {
            if self.writer.has_source_map() {
                es5_emitter.set_source_map_context(text, self.writer.current_source_index());
            } else {
                es5_emitter.set_source_text(text);
            }
        }
        let es5_output = es5_emitter.emit_class_with_name(class_node, &temp_name);
        let mappings = es5_emitter.take_mappings();
        if !mappings.is_empty() && self.writer.has_source_map() {
            self.writer.write("");
            let base_line = self.writer.current_line();
            let base_column = self.writer.current_column();
            self.writer
                .add_offset_mappings(base_line, base_column, &mappings);
            self.writer.write(&es5_output);
        } else {
            self.write(&es5_output);
        }
        self.write_line();
        self.write("exports.default = ");
        self.write(&temp_name);
        self.write(";");
        self.write_line();
    }

    // =========================================================================
    // Imports/Exports
    // =========================================================================

    pub(super) fn emit_import_declaration(&mut self, node: &Node) {
        if self.ctx.is_commonjs() {
            self.emit_import_declaration_commonjs(node);
        } else {
            self.emit_import_declaration_es6(node);
        }
    }

    pub(super) fn emit_import_declaration_es6(&mut self, node: &Node) {
        let Some(import) = self.arena.get_import_decl(node) else {
            return;
        };

        if import.import_clause.is_none() {
            self.write("import ");
            self.emit(import.module_specifier);
            self.write_semicolon();
            return;
        }

        let Some(clause_node) = self.arena.get(import.import_clause) else {
            return;
        };
        let Some(clause) = self.arena.get_import_clause(clause_node) else {
            return;
        };

        if clause.is_type_only {
            return;
        }

        let mut has_default = false;
        let mut namespace_name = None;
        let mut value_specs = Vec::new();
        let mut raw_named_bindings = None;

        if !clause.name.is_none() {
            has_default = true;
        }

        if !clause.named_bindings.is_none() {
            if let Some(bindings_node) = self.arena.get(clause.named_bindings) {
                if let Some(named_imports) = self.arena.get_named_imports(bindings_node) {
                    if !named_imports.name.is_none() && named_imports.elements.nodes.is_empty() {
                        namespace_name = Some(named_imports.name);
                    } else {
                        value_specs = self.collect_value_specifiers(&named_imports.elements);
                    }
                } else {
                    raw_named_bindings = Some(clause.named_bindings);
                }
            }
        }

        let has_named =
            namespace_name.is_some() || !value_specs.is_empty() || raw_named_bindings.is_some();
        if !has_default && !has_named {
            return;
        }

        self.write("import ");
        if has_default {
            self.emit(clause.name);
        }

        if has_named {
            if has_default {
                self.write(", ");
            }
            if let Some(name) = namespace_name {
                self.write("* as ");
                self.emit(name);
            } else if !value_specs.is_empty() {
                self.write("{ ");
                self.emit_comma_separated(&value_specs);
                self.write(" }");
            } else if let Some(raw_node) = raw_named_bindings {
                self.emit(raw_node);
            }
        }

        self.write(" from ");
        self.emit(import.module_specifier);
        self.write_semicolon();
    }

    pub(super) fn emit_import_declaration_commonjs(&mut self, node: &Node) {
        use crate::transforms::module_commonjs;

        let Some(import) = self.arena.get_import_decl(node) else {
            return;
        };

        let Some(clause_node) = self.arena.get(import.import_clause) else {
            // Side-effect import: import "module"; -> emit require
            let module_spec = if let Some(spec_node) = self.arena.get(import.module_specifier) {
                if let Some(lit) = self.arena.get_literal(spec_node) {
                    lit.text.clone()
                } else {
                    return;
                }
            } else {
                return;
            };

            self.write("require(\"");
            self.write(&module_spec);
            self.write("\");");
            self.write_line();
            return;
        };
        let Some(clause) = self.arena.get_import_clause(clause_node) else {
            return;
        };

        if clause.is_type_only {
            return;
        }

        let mut has_value_binding = !clause.name.is_none();
        if !clause.named_bindings.is_none() {
            if let Some(bindings_node) = self.arena.get(clause.named_bindings) {
                if let Some(named_imports) = self.arena.get_named_imports(bindings_node) {
                    if !named_imports.name.is_none() && named_imports.elements.nodes.is_empty() {
                        has_value_binding = true;
                    } else {
                        let value_specs = self.collect_value_specifiers(&named_imports.elements);
                        if !value_specs.is_empty() {
                            has_value_binding = true;
                        }
                    }
                } else {
                    has_value_binding = true;
                }
            }
        }

        if !has_value_binding {
            return;
        }

        // Get module specifier and generate var name
        let module_spec = if let Some(spec_node) = self.arena.get(import.module_specifier) {
            if let Some(lit) = self.arena.get_literal(spec_node) {
                lit.text.clone()
            } else {
                return;
            }
        } else {
            return;
        };

        // Generate module var name: "./foo" -> "foo_1"
        let module_var = format!("{}_1", module_commonjs::sanitize_module_name(&module_spec));

        // Emit: var module_1 = require("module");
        self.write("var ");
        self.write(&module_var);
        self.write(" = require(\"");
        self.write(&module_spec);
        self.write("\");");
        self.write_line();

        // Emit bindings
        let bindings = module_commonjs::get_import_bindings(self.arena, node, &module_var);
        for binding in bindings {
            self.write(&binding);
            self.write_line();
        }
    }

    pub(super) fn emit_import_equals_declaration(&mut self, node: &Node) {
        self.emit_import_equals_declaration_inner(node);
        self.write_semicolon();
    }

    pub(super) fn emit_import_equals_declaration_inner(&mut self, node: &Node) {
        let Some(import) = self.arena.get_import_decl(node) else {
            return;
        };

        if import.import_clause.is_none() {
            return;
        }

        self.write("var ");
        self.emit(import.import_clause);
        self.write(" = ");

        let Some(module_node) = self.arena.get(import.module_specifier) else {
            return;
        };

        if module_node.kind == SyntaxKind::StringLiteral as u16 {
            if let Some(lit) = self.arena.get_literal(module_node) {
                self.write("require(\"");
                self.write(&lit.text);
                self.write("\")");
            }
            return;
        }

        self.emit_entity_name(import.module_specifier);
    }

    pub(super) fn emit_import_clause(&mut self, node: &Node) {
        let Some(clause) = self.arena.get_import_clause(node) else {
            return;
        };

        let mut has_default = false;

        // Default import
        if !clause.name.is_none() {
            self.emit(clause.name);
            has_default = true;
        }

        // Named bindings
        if !clause.named_bindings.is_none() {
            if has_default {
                self.write(", ");
            }
            self.emit(clause.named_bindings);
        }
    }

    pub(super) fn emit_named_imports(&mut self, node: &Node) {
        let Some(imports) = self.arena.get_named_imports(node) else {
            return;
        };

        if !imports.name.is_none() && imports.elements.nodes.is_empty() {
            self.write("* as ");
            self.emit(imports.name);
            return;
        }

        self.write("{ ");
        self.emit_comma_separated(&imports.elements.nodes);
        self.write(" }");
    }

    pub(super) fn emit_import_specifier(&mut self, node: &Node) {
        let Some(spec) = self.arena.get_specifier(node) else {
            return;
        };

        if !spec.property_name.is_none() {
            self.emit(spec.property_name);
            self.write(" as ");
        }
        self.emit(spec.name);
    }

    pub(super) fn emit_export_declaration(&mut self, node: &Node) {
        if self.ctx.is_commonjs() {
            self.emit_export_declaration_commonjs(node);
        } else {
            self.emit_export_declaration_es6(node);
        }
    }

    pub(super) fn emit_export_declaration_es6(&mut self, node: &Node) {
        let Some(export) = self.arena.get_export_decl(node) else {
            return;
        };

        if export.is_type_only {
            return;
        }

        if export.is_default_export {
            self.write("export default ");
            self.emit(export.export_clause);
            self.write_semicolon();
            return;
        }

        if export.export_clause.is_none() {
            self.write("export *");
            if !export.module_specifier.is_none() {
                self.write(" from ");
                self.emit(export.module_specifier);
            }
            self.write_semicolon();
            return;
        }

        let Some(clause_node) = self.arena.get(export.export_clause) else {
            return;
        };

        if clause_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
            self.write("export ");
            self.emit_import_equals_declaration_inner(clause_node);
            self.write_semicolon();
            return;
        }

        if clause_node.kind == syntax_kind_ext::NAMED_EXPORTS {
            if let Some(named_exports) = self.arena.get_named_imports(clause_node) {
                let value_specs = self.collect_value_specifiers(&named_exports.elements);
                if value_specs.is_empty() {
                    return;
                }
                self.write("export { ");
                self.emit_comma_separated(&value_specs);
                self.write(" }");
                if !export.module_specifier.is_none() {
                    self.write(" from ");
                    self.emit(export.module_specifier);
                }
                self.write_semicolon();
                return;
            }
        }

        if self.export_clause_is_type_only(clause_node) {
            return;
        }

        self.write("export ");
        self.emit(export.export_clause);

        if !export.module_specifier.is_none() {
            self.write(" from ");
            self.emit(export.module_specifier);
        }

        self.write_semicolon();
    }

    pub(super) fn emit_export_declaration_commonjs(&mut self, node: &Node) {
        use crate::transforms::module_commonjs;

        let Some(export) = self.arena.get_export_decl(node) else {
            return;
        };

        if export.is_type_only {
            return;
        }

        // Re-export from another module: export { x } from "module";
        if !export.module_specifier.is_none() {
            let module_spec = if let Some(spec_node) = self.arena.get(export.module_specifier) {
                if let Some(lit) = self.arena.get_literal(spec_node) {
                    lit.text.clone()
                } else {
                    return;
                }
            } else {
                return;
            };

            let module_var = format!("{}_1", module_commonjs::sanitize_module_name(&module_spec));

            if export.export_clause.is_none() {
                // First emit the require
                self.write("var ");
                self.write(&module_var);
                self.write(" = require(\"");
                self.write(&module_spec);
                self.write("\");");
                self.write_line();

                self.write("__exportStar(");
                self.write(&module_var);
                self.write(", exports);");
                self.write_line();
                return;
            }

            // Then emit Object.defineProperty for each export
            if let Some(clause_node) = self.arena.get(export.export_clause) {
                if let Some(named_exports) = self.arena.get_named_imports(clause_node) {
                    let value_specs = self.collect_value_specifiers(&named_exports.elements);
                    if value_specs.is_empty() {
                        return;
                    }

                    // First emit the require
                    self.write("var ");
                    self.write(&module_var);
                    self.write(" = require(\"");
                    self.write(&module_spec);
                    self.write("\");");
                    self.write_line();

                    for &spec_idx in &named_exports.elements.nodes {
                        if let Some(spec_node) = self.arena.get(spec_idx) {
                            if let Some(spec) = self.arena.get_specifier(spec_node) {
                                if spec.is_type_only {
                                    continue;
                                }
                                // Get export name and import name
                                let export_name = self.get_identifier_text_idx(spec.name);
                                let import_name = if !spec.property_name.is_none() {
                                    self.get_identifier_text_idx(spec.property_name)
                                } else {
                                    export_name.clone()
                                };

                                // Object.defineProperty(exports, "name", { enumerable: true, get: function () { return mod.name; } });
                                self.write("Object.defineProperty(exports, \"");
                                self.write(&export_name);
                                self.write("\", { enumerable: true, get: function () { return ");
                                self.write(&module_var);
                                self.write(".");
                                self.write(&import_name);
                                self.write("; } });");
                                self.write_line();
                            }
                        }
                    }
                }
            }
            return;
        }

        let mut is_anonymous_default = false;
        if export.is_default_export {
            if let Some(clause_node) = self.arena.get(export.export_clause) {
                match clause_node.kind {
                    k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                        if let Some(func) = self.arena.get_function(clause_node) {
                            let func_name = self.get_identifier_text_idx(func.name);
                            is_anonymous_default =
                                func_name == "function" || !is_valid_identifier_name(&func_name);
                        }
                    }
                    k if k == syntax_kind_ext::CLASS_DECLARATION => {
                        if let Some(class) = self.arena.get_class(clause_node) {
                            let class_name = self.get_identifier_text_idx(class.name);
                            is_anonymous_default = !is_valid_identifier_name(&class_name);
                        }
                    }
                    _ => {}
                }
            }
        }

        // Check if export_clause contains a declaration (export const x, export function f, etc.)
        if let Some(clause_node) = self.arena.get(export.export_clause) {
            if self.export_clause_is_type_only(clause_node) {
                return;
            }

            if clause_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
                self.emit_import_equals_declaration(clause_node);
                if !self.ctx.module_state.has_export_assignment {
                    if let Some(import_decl) = self.arena.get_import_decl(clause_node) {
                        let name = self.get_identifier_text_idx(import_decl.import_clause);
                        if !name.is_empty() {
                            self.write_line();
                            self.write("exports.");
                            self.write(&name);
                            self.write(" = ");
                            self.write(&name);
                            self.write(";");
                            self.write_line();
                        }
                    }
                }
                return;
            }

            let clause_kind = clause_node.kind;
            let is_decl = clause_kind == syntax_kind_ext::VARIABLE_STATEMENT
                || clause_kind == syntax_kind_ext::FUNCTION_DECLARATION
                || clause_kind == syntax_kind_ext::CLASS_DECLARATION
                || clause_kind == syntax_kind_ext::ENUM_DECLARATION
                || clause_kind == syntax_kind_ext::MODULE_DECLARATION;

            if is_decl && self.transforms.has_transform(export.export_clause) {
                self.emit(export.export_clause);
                return;
            }

            if is_anonymous_default {
                self.emit_commonjs_default_export_expr(clause_node, export.export_clause);
                return;
            }

            match clause_node.kind {
                // export const/let/var x = ...
                k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                    // Collect export names before emitting
                    let export_names = self.collect_variable_names_from_node(clause_node);

                    // Emit the variable declaration
                    self.emit_variable_statement(clause_node);
                    self.write_line();

                    // Emit exports.x = x; for each name (unless file has export =)
                    if !self.ctx.module_state.has_export_assignment {
                        for name in &export_names {
                            self.write("exports.");
                            self.write(name);
                            self.write(" = ");
                            self.write(name);
                            self.write(";");
                            self.write_line();
                        }
                    }
                }
                // export function f() {} or export default function f() {}
                k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                    // Emit the function declaration
                    self.emit_function_declaration(clause_node, export.export_clause);
                    self.write_line();

                    // Get function name and emit export (unless file has export =)
                    if !self.ctx.module_state.has_export_assignment {
                        if let Some(func) = self.arena.get_function(clause_node) {
                            if let Some(name) = self.get_identifier_text_opt(func.name) {
                                if export.is_default_export {
                                    self.write("exports.default = ");
                                } else {
                                    self.write("exports.");
                                    self.write(&name);
                                    self.write(" = ");
                                }
                                self.write(&name);
                                self.write(";");
                                self.write_line();
                            }
                        }
                    }
                }
                // export class C {} or export default class C {}
                k if k == syntax_kind_ext::CLASS_DECLARATION => {
                    // Emit the class declaration
                    self.emit_class_declaration(clause_node, export.export_clause);
                    self.write_line();

                    // Get class name and emit export (unless file has export =)
                    if !self.ctx.module_state.has_export_assignment {
                        if let Some(class) = self.arena.get_class(clause_node) {
                            if let Some(name) = self.get_identifier_text_opt(class.name) {
                                if export.is_default_export {
                                    self.write("exports.default = ");
                                } else {
                                    self.write("exports.");
                                    self.write(&name);
                                    self.write(" = ");
                                }
                                self.write(&name);
                                self.write(";");
                                self.write_line();
                            }
                        }
                    }
                }
                // export enum E {}
                k if k == syntax_kind_ext::ENUM_DECLARATION => {
                    self.emit_enum_declaration(clause_node, export.export_clause);
                    self.write_line();

                    if !self.ctx.module_state.has_export_assignment {
                        if let Some(enum_decl) = self.arena.get_enum(clause_node) {
                            if let Some(name) = self.get_identifier_text_opt(enum_decl.name) {
                                if export.is_default_export {
                                    self.write("exports.default = ");
                                } else {
                                    self.write("exports.");
                                    self.write(&name);
                                    self.write(" = ");
                                }
                                self.write(&name);
                                self.write(";");
                                self.write_line();
                            }
                        }
                    }
                }
                // export namespace N {}
                k if k == syntax_kind_ext::MODULE_DECLARATION => {
                    self.emit_module_declaration(clause_node, export.export_clause);
                    self.write_line();

                    if !self.ctx.module_state.has_export_assignment {
                        if let Some(module_decl) = self.arena.get_module(clause_node) {
                            if let Some(name) = self.get_module_root_name(module_decl.name) {
                                self.write("exports.");
                                self.write(&name);
                                self.write(" = ");
                                self.write(&name);
                                self.write(";");
                                self.write_line();
                            }
                        }
                    }
                }
                // export { x, y } - local re-export without module specifier
                k if k == syntax_kind_ext::NAMED_EXPORTS => {
                    // Emit exports.x = x; for each name
                    if let Some(named_exports) = self.arena.get_named_imports(clause_node) {
                        let value_specs = self.collect_value_specifiers(&named_exports.elements);
                        if value_specs.is_empty() {
                            return;
                        }

                        for &spec_idx in &value_specs {
                            if let Some(spec_node) = self.arena.get(spec_idx) {
                                if let Some(spec) = self.arena.get_specifier(spec_node) {
                                    let export_name = self.get_identifier_text_idx(spec.name);
                                    let local_name = if !spec.property_name.is_none() {
                                        self.get_identifier_text_idx(spec.property_name)
                                    } else {
                                        export_name.clone()
                                    };

                                    self.write("exports.");
                                    self.write(&export_name);
                                    self.write(" = ");
                                    self.write(&local_name);
                                    self.write(";");
                                    self.write_line();
                                }
                            }
                        }
                    }
                }
                // Type-only declarations (interface, type alias) - skip for CommonJS
                k if k == syntax_kind_ext::INTERFACE_DECLARATION => {}
                k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {}
                // export default <expression> - emit as exports.default = expr;
                _ => {
                    // This is likely an expression-based default export: export default 42;
                    self.write("exports.default = ");
                    self.emit(export.export_clause);
                    self.write_semicolon();
                }
            }
        }
    }

    /// Emit export assignment (export = expr or export default expr)
    pub(super) fn emit_export_assignment(&mut self, node: &Node) {
        let Some(export_assign) = self.arena.get_export_assignment(node) else {
            return;
        };

        if self.ctx.is_commonjs() {
            // CommonJS: export = expr → module.exports = expr;
            //           export default expr → exports.default = expr;
            if export_assign.is_export_equals {
                self.write("module.exports = ");
            } else {
                self.write("exports.default = ");
            }
            self.emit_expression(export_assign.expression);
            self.write_semicolon();
        } else {
            // ES6: export = expr (not valid ES6, but emit as export default)
            //      export default expr → export default expr;
            self.write("export default ");
            self.emit_expression(export_assign.expression);
            self.write_semicolon();
        }
    }

    /// Collect variable names from a VARIABLE_STATEMENT node
    pub(super) fn collect_variable_names_from_node(&self, node: &Node) -> Vec<String> {
        let mut names = Vec::new();
        if let Some(var_stmt) = self.arena.get_variable(node) {
            // VARIABLE_STATEMENT has declarations containing VARIABLE_DECLARATION_LIST
            for &decl_list_idx in &var_stmt.declarations.nodes {
                if let Some(decl_list_node) = self.arena.get(decl_list_idx) {
                    // VARIABLE_DECLARATION_LIST has declarations containing VARIABLE_DECLARATION
                    if let Some(decl_list) = self.arena.get_variable(decl_list_node) {
                        for &decl_idx in &decl_list.declarations.nodes {
                            if let Some(decl_node) = self.arena.get(decl_idx) {
                                if let Some(decl) = self.arena.get_variable_declaration(decl_node) {
                                    self.collect_binding_names(decl.name, &mut names);
                                }
                            }
                        }
                    }
                }
            }
        }
        names
    }

    /// Get identifier text from optional node index
    pub(super) fn get_identifier_text_opt(&self, idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(idx)?;
        if node.kind == SyntaxKind::Identifier as u16 {
            self.arena
                .get_identifier(node)
                .map(|id| id.escaped_text.clone())
        } else {
            None
        }
    }

    pub(super) fn get_module_root_name(&self, name_idx: NodeIndex) -> Option<String> {
        if name_idx.is_none() {
            return None;
        }

        let node = self.arena.get(name_idx)?;
        if node.kind == SyntaxKind::Identifier as u16 {
            return self
                .arena
                .get_identifier(node)
                .map(|id| id.escaped_text.clone());
        }

        if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            if let Some(qn) = self.arena.qualified_names.get(node.data_index as usize) {
                return self.get_module_root_name(qn.left);
            }
        }

        None
    }

    /// Get identifier text from a node index
    pub(super) fn get_identifier_text_idx(&self, idx: NodeIndex) -> String {
        if let Some(node) = self.arena.get(idx) {
            if node.kind == SyntaxKind::Identifier as u16 {
                if let Some(id) = self.arena.get_identifier(node) {
                    return id.escaped_text.clone();
                }
            }
        }
        String::new()
    }

    pub(super) fn emit_entity_name(&mut self, idx: NodeIndex) {
        if idx.is_none() {
            return;
        }

        let Some(node) = self.arena.get(idx) else {
            return;
        };

        match node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                if let Some(id) = self.arena.get_identifier(node) {
                    self.write(&id.escaped_text);
                }
            }
            k if k == SyntaxKind::ThisKeyword as u16 => self.write("this"),
            k if k == SyntaxKind::SuperKeyword as u16 => self.write("super"),
            k if k == syntax_kind_ext::QUALIFIED_NAME => {
                if let Some(name) = self.arena.get_qualified_name(node) {
                    self.emit_entity_name(name.left);
                    self.write(".");
                    self.emit_entity_name(name.right);
                }
            }
            _ => {}
        }
    }

    pub(super) fn emit_named_exports(&mut self, node: &Node) {
        // Named exports uses the same data structure as named imports
        let Some(exports) = self.arena.get_named_imports(node) else {
            self.write("{ }");
            return;
        };

        self.write("{ ");
        self.emit_comma_separated(&exports.elements.nodes);
        self.write(" }");
    }

    pub(super) fn emit_export_specifier(&mut self, node: &Node) {
        let Some(spec) = self.arena.get_specifier(node) else {
            return;
        };

        if !spec.property_name.is_none() {
            self.emit(spec.property_name);
            self.write(" as ");
        }
        self.emit(spec.name);
    }

    pub(super) fn collect_value_specifiers(&self, elements: &NodeList) -> Vec<NodeIndex> {
        let mut specs = Vec::new();
        for &spec_idx in &elements.nodes {
            if let Some(spec_node) = self.arena.get(spec_idx) {
                if let Some(spec) = self.arena.get_specifier(spec_node) {
                    if spec.is_type_only {
                        continue;
                    }
                }
            }
            specs.push(spec_idx);
        }
        specs
    }

    pub(super) fn export_clause_is_type_only(&self, clause_node: &Node) -> bool {
        match clause_node.kind {
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => true,
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => true,
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                let Some(enum_decl) = self.arena.get_enum(clause_node) else {
                    return false;
                };
                self.has_declare_modifier(&enum_decl.modifiers)
                    || self.has_modifier(&enum_decl.modifiers, SyntaxKind::ConstKeyword as u16)
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                let Some(class_decl) = self.arena.get_class(clause_node) else {
                    return false;
                };
                self.has_declare_modifier(&class_decl.modifiers)
            }
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                let Some(func_decl) = self.arena.get_function(clause_node) else {
                    return false;
                };
                self.has_declare_modifier(&func_decl.modifiers)
            }
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                let Some(var_decl) = self.arena.get_variable(clause_node) else {
                    return false;
                };
                self.has_declare_modifier(&var_decl.modifiers)
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                let Some(module_decl) = self.arena.get_module(clause_node) else {
                    return false;
                };
                self.has_declare_modifier(&module_decl.modifiers)
            }
            _ => false,
        }
    }

    /// Check if the file contains an export assignment (export =)
    pub(super) fn has_export_assignment(&self, statements: &NodeList) -> bool {
        for &stmt_idx in &statements.nodes {
            if let Some(node) = self.arena.get(stmt_idx) {
                if node.kind == syntax_kind_ext::EXPORT_ASSIGNMENT {
                    return true;
                }
            }
        }
        false
    }

    /// Check if a file is a runtime module (has value imports/exports).
    pub(super) fn file_is_module(&self, statements: &NodeList) -> bool {
        for &stmt_idx in &statements.nodes {
            if let Some(node) = self.arena.get(stmt_idx) {
                match node.kind {
                    k if k == syntax_kind_ext::IMPORT_DECLARATION
                        || k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION =>
                    {
                        if let Some(import_decl) = self.arena.get_import_decl(node) {
                            if self.import_decl_has_runtime_value(import_decl) {
                                return true;
                            }
                        }
                    }
                    k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                        if let Some(export_decl) = self.arena.get_export_decl(node) {
                            if self.export_decl_has_runtime_value(export_decl) {
                                return true;
                            }
                        }
                    }
                    k if k == syntax_kind_ext::EXPORT_ASSIGNMENT => return true,
                    // Check for export modifier on declarations
                    k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                        if let Some(var_stmt) = self.arena.get_variable(node) {
                            if self.has_export_modifier(&var_stmt.modifiers)
                                && !self.has_declare_modifier(&var_stmt.modifiers)
                            {
                                return true;
                            }
                        }
                    }
                    k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                        if let Some(func) = self.arena.get_function(node) {
                            if self.has_export_modifier(&func.modifiers)
                                && !self.has_declare_modifier(&func.modifiers)
                            {
                                return true;
                            }
                        }
                    }
                    k if k == syntax_kind_ext::CLASS_DECLARATION => {
                        if let Some(class) = self.arena.get_class(node) {
                            if self.has_export_modifier(&class.modifiers)
                                && !self.has_declare_modifier(&class.modifiers)
                            {
                                return true;
                            }
                        }
                    }
                    k if k == syntax_kind_ext::ENUM_DECLARATION => {
                        if let Some(enum_decl) = self.arena.get_enum(node) {
                            if self.has_export_modifier(&enum_decl.modifiers)
                                && !self.has_declare_modifier(&enum_decl.modifiers)
                                && !self.has_modifier(
                                    &enum_decl.modifiers,
                                    SyntaxKind::ConstKeyword as u16,
                                )
                            {
                                return true;
                            }
                        }
                    }
                    k if k == syntax_kind_ext::MODULE_DECLARATION => {
                        if let Some(module) = self.arena.get_module(node) {
                            if self.has_export_modifier(&module.modifiers)
                                && !self.has_declare_modifier(&module.modifiers)
                            {
                                return true;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        false
    }

    pub(super) fn collect_module_dependencies(&self, statements: &[NodeIndex]) -> Vec<String> {
        let mut deps = Vec::new();
        for &stmt_idx in statements {
            let Some(node) = self.arena.get(stmt_idx) else {
                continue;
            };

            if node.kind == syntax_kind_ext::IMPORT_DECLARATION
                || node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
            {
                if let Some(import_decl) = self.arena.get_import_decl(node) {
                    if !self.import_decl_has_runtime_value(import_decl) {
                        continue;
                    }
                    if let Some(text) = self.get_module_specifier_text(import_decl.module_specifier)
                    {
                        if !deps.contains(&text) {
                            deps.push(text);
                        }
                    }
                }
                continue;
            }

            if node.kind == syntax_kind_ext::EXPORT_DECLARATION {
                if let Some(export_decl) = self.arena.get_export_decl(node) {
                    if !self.export_decl_has_runtime_value(export_decl) {
                        continue;
                    }
                    if let Some(text) = self.get_module_specifier_text(export_decl.module_specifier)
                    {
                        if !deps.contains(&text) {
                            deps.push(text);
                        }
                    }
                }
            }
        }

        deps
    }

    fn get_module_specifier_text(&self, specifier: NodeIndex) -> Option<String> {
        if specifier.is_none() {
            return None;
        }

        let Some(node) = self.arena.get(specifier) else {
            return None;
        };
        let Some(literal) = self.arena.get_literal(node) else {
            return None;
        };

        Some(literal.text.clone())
    }

    pub(super) fn import_decl_has_runtime_value(
        &self,
        import_decl: &crate::parser::node::ImportDeclData,
    ) -> bool {
        if import_decl.import_clause.is_none() {
            return true;
        }

        let Some(clause_node) = self.arena.get(import_decl.import_clause) else {
            return true;
        };

        if clause_node.kind != syntax_kind_ext::IMPORT_CLAUSE {
            return self.import_equals_has_external_module(import_decl.module_specifier);
        }

        let Some(clause) = self.arena.get_import_clause(clause_node) else {
            return true;
        };

        if clause.is_type_only {
            return false;
        }

        if !clause.name.is_none() {
            return true;
        }

        if clause.named_bindings.is_none() {
            return false;
        }

        let Some(bindings_node) = self.arena.get(clause.named_bindings) else {
            return false;
        };

        let Some(named) = self.arena.get_named_imports(bindings_node) else {
            return true;
        };

        if !named.name.is_none() {
            return true;
        }

        if named.elements.nodes.is_empty() {
            return true;
        }

        for &spec_idx in &named.elements.nodes {
            let Some(spec_node) = self.arena.get(spec_idx) else {
                continue;
            };
            if let Some(spec) = self.arena.get_specifier(spec_node) {
                if !spec.is_type_only {
                    return true;
                }
            }
        }

        false
    }

    pub(super) fn import_equals_has_external_module(&self, module_specifier: NodeIndex) -> bool {
        if module_specifier.is_none() {
            return false;
        }

        let Some(node) = self.arena.get(module_specifier) else {
            return false;
        };

        node.kind == SyntaxKind::StringLiteral as u16
    }

    pub(super) fn export_decl_has_runtime_value(
        &self,
        export_decl: &crate::parser::node::ExportDeclData,
    ) -> bool {
        if export_decl.is_type_only {
            return false;
        }

        if export_decl.is_default_export {
            return true;
        }

        if export_decl.export_clause.is_none() {
            return true;
        }

        let Some(clause_node) = self.arena.get(export_decl.export_clause) else {
            return false;
        };

        if let Some(named) = self.arena.get_named_imports(clause_node) {
            if !named.name.is_none() {
                return true;
            }

            if named.elements.nodes.is_empty() {
                return true;
            }

            for &spec_idx in &named.elements.nodes {
                let Some(spec_node) = self.arena.get(spec_idx) else {
                    continue;
                };
                if let Some(spec) = self.arena.get_specifier(spec_node) {
                    if !spec.is_type_only {
                        return true;
                    }
                }
            }

            return false;
        }

        if self.export_clause_is_type_only(clause_node) {
            return false;
        }

        true
    }

    /// Check if we should emit the __esModule marker.
    /// Returns true if the file contains any ES6 module syntax (import/export),
    /// excluding `export =` which is legacy CommonJS.
    pub(super) fn should_emit_es_module_marker(&self, statements: &NodeList) -> bool {
        // First check: if file has export =, don't emit __esModule at all
        for &stmt_idx in &statements.nodes {
            if let Some(node) = self.arena.get(stmt_idx) {
                if node.kind == syntax_kind_ext::EXPORT_ASSIGNMENT {
                    return false;
                }
            }
        }

        // Second check: look for runtime module syntax
        for &stmt_idx in &statements.nodes {
            if let Some(node) = self.arena.get(stmt_idx) {
                match node.kind {
                    k if k == syntax_kind_ext::IMPORT_DECLARATION
                        || k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION =>
                    {
                        if let Some(import_decl) = self.arena.get_import_decl(node) {
                            if self.import_decl_has_runtime_value(import_decl) {
                                return true;
                            }
                        }
                    }
                    k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                        if let Some(export_decl) = self.arena.get_export_decl(node) {
                            if self.export_decl_has_runtime_value(export_decl) {
                                return true;
                            }
                        }
                    }
                    // Note: EXPORT_ASSIGNMENT (export =) is excluded - it's CommonJS style
                    // Check for export modifier on declarations
                    k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                        if let Some(var_stmt) = self.arena.get_variable(node) {
                            if self.has_export_modifier(&var_stmt.modifiers)
                                && !self.has_declare_modifier(&var_stmt.modifiers)
                            {
                                return true;
                            }
                        }
                    }
                    k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                        if let Some(func) = self.arena.get_function(node) {
                            if self.has_export_modifier(&func.modifiers)
                                && !self.has_declare_modifier(&func.modifiers)
                            {
                                return true;
                            }
                        }
                    }
                    k if k == syntax_kind_ext::CLASS_DECLARATION => {
                        if let Some(class) = self.arena.get_class(node) {
                            if self.has_export_modifier(&class.modifiers)
                                && !self.has_declare_modifier(&class.modifiers)
                            {
                                return true;
                            }
                        }
                    }
                    k if k == syntax_kind_ext::ENUM_DECLARATION => {
                        if let Some(enum_decl) = self.arena.get_enum(node) {
                            if self.has_export_modifier(&enum_decl.modifiers)
                                && !self.has_declare_modifier(&enum_decl.modifiers)
                                && !self.has_modifier(
                                    &enum_decl.modifiers,
                                    SyntaxKind::ConstKeyword as u16,
                                )
                            {
                                return true;
                            }
                        }
                    }
                    k if k == syntax_kind_ext::MODULE_DECLARATION => {
                        if let Some(module) = self.arena.get_module(node) {
                            if self.has_export_modifier(&module.modifiers)
                                && !self.has_declare_modifier(&module.modifiers)
                            {
                                return true;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        false
    }

    /// Emit CommonJS module preamble
    pub(super) fn emit_commonjs_preamble(&mut self, statements: &NodeList) {
        use crate::transforms::module_commonjs;

        // "use strict";
        self.write("\"use strict\";");
        self.write_line();

        // Emit __esModule if this is an ES module (has imports or ES exports)
        // Note: 'export =' is CommonJS style and doesn't get __esModule
        if self.should_emit_es_module_marker(statements) {
            self.write("Object.defineProperty(exports, \"__esModule\", { value: true });");
            self.write_line();
        }

        // Collect and emit exports initialization
        // TypeScript emits: exports.C = void 0; (NOT Object.defineProperty)
        let export_names = module_commonjs::collect_export_names(self.arena, &statements.nodes);
        if !export_names.is_empty() {
            // exports.a = exports.b = void 0;
            for (i, name) in export_names.iter().enumerate() {
                if i > 0 {
                    self.write(" = ");
                }
                self.write("exports.");
                self.write(name);
            }
            self.write(" = void 0;");
            self.write_line();
        }
    }

    /// Detect which CommonJS import/export helpers are needed for the file
    pub(super) fn detect_commonjs_helpers(
        &self,
        statements: &NodeList,
        helpers: &mut crate::transforms::helpers::HelpersNeeded,
    ) {
        use crate::parser::syntax_kind_ext;

        for &stmt_idx in &statements.nodes {
            let Some(node) = self.arena.get(stmt_idx) else {
                continue;
            };

            match node.kind {
                k if k == syntax_kind_ext::IMPORT_DECLARATION => {
                    if let Some(import) = self.arena.get_import_decl(node) {
                        // Check for: import * as ns from "mod"
                        if let Some(clause_node) = self.arena.get(import.import_clause) {
                            if let Some(clause) = self.arena.get_import_clause(clause_node) {
                                if clause.is_type_only {
                                    continue;
                                }
                                if let Some(bindings_node) = self.arena.get(clause.named_bindings) {
                                    // NAMESPACE_IMPORT = 275
                                    if bindings_node.kind == syntax_kind_ext::NAMESPACE_IMPORT {
                                        helpers.import_star = true;
                                        helpers.create_binding = true; // __importStar depends on __createBinding
                                    } else if let Some(named_imports) =
                                        self.arena.get_named_imports(bindings_node)
                                    {
                                        if !named_imports.name.is_none()
                                            && named_imports.elements.nodes.is_empty()
                                        {
                                            helpers.import_star = true;
                                            helpers.create_binding = true;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                    if let Some(export) = self.arena.get_export_decl(node) {
                        if export.is_type_only {
                            continue;
                        }
                        // Check for: export * from "mod" (module_specifier present, no export_clause)
                        if !export.module_specifier.is_none() && export.export_clause.is_none() {
                            helpers.export_star = true;
                            helpers.create_binding = true; // __exportStar depends on __createBinding
                        }
                    }
                }
                _ => {}
            }
        }
    }
}
