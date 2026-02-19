use super::is_valid_identifier_name;
use super::{ModuleKind, Printer};
use crate::transform_context::IdentifierId;
use crate::transforms::ClassES5Emitter;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

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

        let before_len = self.writer.len();
        emit_inner(self);
        let inner_emitted = self.writer.len() > before_len;

        self.ctx.options.module = prev_module;

        // If the inner emit produced nothing (e.g., variable declaration with
        // no initializer where only the type annotation was stripped), skip
        // the export assignment. The preamble `exports.X = void 0;` already
        // handles the forward declaration.
        if !inner_emitted {
            return;
        }

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

    pub(super) fn emit_commonjs_default_export_expr_inner(&mut self, node: &Node, idx: NodeIndex) {
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

    /// Emit anonymous default export as a named declaration + export assignment.
    /// TSC pattern: `export default class {}` → `class default_1 {}\nexports.default = default_1;`
    pub(super) fn emit_commonjs_anonymous_default_as_named(&mut self, node: &Node, idx: NodeIndex) {
        // Set temporary override name for anonymous default declarations
        let prev = self.anonymous_default_export_name.take();
        self.anonymous_default_export_name = Some("default_1".to_string());
        self.emit_node_default(node, idx);
        self.anonymous_default_export_name = prev;
        self.write_line();
        self.write("exports.default = default_1;");
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
        // Pass transform directives to the ClassES5Emitter
        es5_emitter.set_transforms(self.transforms.clone());
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
        let mut trailing_comma = false;

        if !clause.name.is_none() {
            has_default = true;
        }

        if !clause.named_bindings.is_none()
            && let Some(bindings_node) = self.arena.get(clause.named_bindings)
        {
            if let Some(named_imports) = self.arena.get_named_imports(bindings_node) {
                if !named_imports.name.is_none() && named_imports.elements.nodes.is_empty() {
                    namespace_name = Some(named_imports.name);
                } else {
                    value_specs = self.collect_value_specifiers(&named_imports.elements);
                    trailing_comma = self
                        .has_trailing_comma_in_source(bindings_node, &named_imports.elements.nodes);
                }
            } else {
                raw_named_bindings = Some(clause.named_bindings);
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
                if trailing_comma {
                    self.write(",");
                }
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
        if !clause.named_bindings.is_none()
            && let Some(bindings_node) = self.arena.get(clause.named_bindings)
        {
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

        // Check if this is a namespace-only import (import * as ns from "mod")
        // to inline: var ns = __importStar(require("mod"));
        let bindings = module_commonjs::get_import_bindings(self.arena, node, &module_var);

        let is_namespace_only = bindings.len() == 1
            && bindings[0].contains("__importStar(")
            && !bindings[0].contains(".default");
        let is_default_only = bindings.len() == 1 && bindings[0].contains("__importDefault(");

        if is_namespace_only {
            // Inline: var ns = __importStar(require("mod"));
            let binding = &bindings[0];
            // Extract the var name from "var ns = __importStar(module_var);"
            if let Some(eq_pos) = binding.find(" = __importStar(") {
                let var_name = &binding[4..eq_pos]; // Skip "var "
                self.write_var_or_const();
                self.write(var_name);
                self.write(" = __importStar(require(\"");
                self.write(&module_spec);
                self.write("\"));");
                self.write_line();
            } else {
                // Fallback
                self.write_var_or_const();
                self.write(&module_var);
                self.write(" = require(\"");
                self.write(&module_spec);
                self.write("\");");
                self.write_line();
                for binding in bindings {
                    self.write(&binding);
                    self.write_line();
                }
            }
        } else if is_default_only {
            // Inline: var name = __importDefault(require("mod"));
            let binding = &bindings[0];
            if let Some(eq_pos) = binding.find(" = ") {
                let var_name = &binding[4..eq_pos]; // Skip "var "
                let rest = &binding[eq_pos + 3..]; // After " = "
                // Check if it's using __importDefault
                if rest.contains("__importDefault") {
                    self.write_var_or_const();
                    self.write(var_name);
                    self.write(" = __importDefault(require(\"");
                    self.write(&module_spec);
                    self.write("\"));");
                    self.write_line();
                } else {
                    self.write_var_or_const();
                    self.write(&module_var);
                    self.write(" = require(\"");
                    self.write(&module_spec);
                    self.write("\");");
                    self.write_line();
                    for binding in bindings {
                        self.write(&binding);
                        self.write_line();
                    }
                }
            } else {
                self.write_var_or_const();
                self.write(&module_var);
                self.write(" = require(\"");
                self.write(&module_spec);
                self.write("\");");
                self.write_line();
                for binding in bindings {
                    self.write(&binding);
                    self.write_line();
                }
            }
        } else {
            // Emit: var module_1 = require("module");
            self.write_var_or_const();
            self.write(&module_var);
            self.write(" = require(\"");
            self.write(&module_spec);
            self.write("\");");
            self.write_line();

            // Emit bindings
            for binding in bindings {
                self.write(&binding);
                self.write_line();
            }
        }
    }

    pub(super) fn emit_import_equals_declaration(&mut self, node: &Node) {
        let before_len = self.writer.len();
        self.emit_import_equals_declaration_inner(node);
        if self.writer.len() > before_len {
            self.write_semicolon();
        }
    }

    pub(super) fn emit_import_equals_declaration_inner(&mut self, node: &Node) {
        let Some(import) = self.arena.get_import_decl(node) else {
            return;
        };

        if !self.import_decl_has_runtime_value(import) {
            return;
        }

        if import.import_clause.is_none() {
            return;
        }

        let Some(module_node) = self.arena.get(import.module_specifier) else {
            return;
        };

        let is_external = module_node.kind == SyntaxKind::StringLiteral as u16
            || module_node.kind == syntax_kind_ext::EXTERNAL_MODULE_REFERENCE;

        // `import X = require("module")` uses const/var based on target.
        // `import X = Y` (entity name) always uses `var` per TSC behavior.
        if is_external {
            self.write_var_or_const();
        } else {
            self.write("var ");
        }
        self.emit(import.import_clause);
        self.write(" = ");

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

        // Filter out type-only import specifiers
        let value_imports: Vec<_> = imports
            .elements
            .nodes
            .iter()
            .filter(|&spec_idx| {
                if let Some(spec_node) = self.arena.get(*spec_idx) {
                    if let Some(spec) = self.arena.get_specifier(spec_node) {
                        !spec.is_type_only
                    } else {
                        true
                    }
                } else {
                    true
                }
            })
            .collect();

        // If all imports are type-only, don't emit the named bindings at all
        if value_imports.is_empty() {
            return;
        }

        if !imports.name.is_none() && value_imports.is_empty() {
            self.write("* as ");
            self.emit(imports.name);
            return;
        }

        self.write("{ ");
        // Convert Vec<&NodeIndex> to Vec<NodeIndex> for emit_comma_separated
        let value_refs: Vec<NodeIndex> = value_imports.iter().map(|&&idx| idx).collect();
        self.emit_comma_separated(&value_refs);
        // Preserve trailing comma from source
        let has_trailing_comma = self.has_trailing_comma_in_source(node, &imports.elements.nodes);
        if has_trailing_comma {
            self.write(",");
        }
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
            // Check if the clause is a declaration (function/class) that doesn't need semicolon
            let clause_is_func_or_class =
                if let Some(clause_node) = self.arena.get(export.export_clause) {
                    clause_node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                        || clause_node.kind == syntax_kind_ext::CLASS_DECLARATION
                } else {
                    false
                };
            self.write("export default ");
            self.emit(export.export_clause);
            if !clause_is_func_or_class {
                self.write_semicolon();
            }
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

        if clause_node.kind == syntax_kind_ext::NAMED_EXPORTS
            && let Some(named_exports) = self.arena.get_named_imports(clause_node)
        {
            let value_specs = self.collect_value_specifiers(&named_exports.elements);
            if value_specs.is_empty() && !named_exports.elements.nodes.is_empty() {
                // All specifiers were type-only — skip the export entirely
                return;
            }
            // Emit `export { ... }` or `export {}` (when originally empty)
            if value_specs.is_empty() {
                self.write("export {}");
            } else {
                self.write("export { ");
                self.emit_comma_separated(&value_specs);
                if self.has_trailing_comma_in_source(clause_node, &named_exports.elements.nodes) {
                    self.write(",");
                }
                self.write(" }");
            }
            if !export.module_specifier.is_none() {
                self.write(" from ");
                self.emit(export.module_specifier);
            }
            self.write_semicolon();
            return;
        }

        // export * as <name> from "..." — clause is an Identifier or StringLiteral
        if !export.module_specifier.is_none()
            && (clause_node.kind == SyntaxKind::Identifier as u16
                || clause_node.kind == SyntaxKind::StringLiteral as u16)
        {
            self.write("export * as ");
            self.emit(export.export_clause);
            self.write(" from ");
            self.emit(export.module_specifier);
            self.write_semicolon();
            return;
        }

        if self.export_clause_is_type_only(clause_node) {
            return;
        }

        // Check if the clause is a declaration that handles its own semicolons
        let is_declaration = clause_node.kind == syntax_kind_ext::VARIABLE_STATEMENT
            || clause_node.kind == syntax_kind_ext::FUNCTION_DECLARATION
            || clause_node.kind == syntax_kind_ext::CLASS_DECLARATION
            || clause_node.kind == syntax_kind_ext::ENUM_DECLARATION
            || clause_node.kind == syntax_kind_ext::MODULE_DECLARATION;

        self.write("export ");
        self.emit(export.export_clause);

        if !export.module_specifier.is_none() {
            self.write(" from ");
            self.emit(export.module_specifier);
        }

        // Don't add semicolon for declarations - they handle their own
        if !is_declaration {
            self.write_semicolon();
        }
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
                self.write_var_or_const();
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
            if let Some(clause_node) = self.arena.get(export.export_clause)
                && let Some(named_exports) = self.arena.get_named_imports(clause_node)
            {
                let value_specs = self.collect_value_specifiers(&named_exports.elements);
                if value_specs.is_empty() {
                    return;
                }

                // First emit the require
                self.write_var_or_const();
                self.write(&module_var);
                self.write(" = require(\"");
                self.write(&module_spec);
                self.write("\");");
                self.write_line();

                for &spec_idx in &named_exports.elements.nodes {
                    if let Some(spec_node) = self.arena.get(spec_idx)
                        && let Some(spec) = self.arena.get_specifier(spec_node)
                    {
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
            return;
        }

        let mut is_anonymous_default = false;
        if export.is_default_export
            && let Some(clause_node) = self.arena.get(export.export_clause)
        {
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

        // Check if export_clause contains a declaration (export const x, export function f, etc.)
        if let Some(clause_node) = self.arena.get(export.export_clause) {
            if self.export_clause_is_type_only(clause_node) {
                return;
            }

            if clause_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
                self.emit_import_equals_declaration(clause_node);
                if !self.ctx.module_state.has_export_assignment
                    && let Some(import_decl) = self.arena.get_import_decl(clause_node)
                {
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
                self.emit_commonjs_anonymous_default_as_named(clause_node, export.export_clause);
                return;
            }

            match clause_node.kind {
                // export const/let/var x = ...
                k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                    if !self.ctx.module_state.has_export_assignment {
                        // Try inline form: exports.x = initializer;
                        // TSC emits this for simple single-binding declarations.
                        if let Some(inline_decls) = self.try_collect_inline_cjs_exports(clause_node)
                        {
                            for (name, init_idx) in &inline_decls {
                                self.write("exports.");
                                self.write(name);
                                self.write(" = ");
                                self.emit(*init_idx);
                                self.write(";");
                                self.write_line();
                            }
                        } else {
                            // Complex case (destructuring): emit declaration then exports
                            let export_names = self.collect_variable_names_from_node(clause_node);
                            self.emit_variable_statement(clause_node);
                            self.write_line();
                            for name in &export_names {
                                self.write("exports.");
                                self.write(name);
                                self.write(" = ");
                                self.write(name);
                                self.write(";");
                                self.write_line();
                            }
                        }
                    } else {
                        self.emit_variable_statement(clause_node);
                        self.write_line();
                    }
                }
                // export function f() {} or export default function f() {}
                k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                    // Emit the function declaration
                    self.emit_function_declaration(clause_node, export.export_clause);
                    self.write_line();

                    // For default exports, emit exports.default = name; after the function.
                    // For named exports, the preamble already emitted exports.f = f;
                    // (since function declarations are hoisted).
                    if !self.ctx.module_state.has_export_assignment
                        && export.is_default_export
                        && let Some(func) = self.arena.get_function(clause_node)
                        && let Some(name) = self.get_identifier_text_opt(func.name)
                    {
                        self.write("exports.default = ");
                        self.write(&name);
                        self.write(";");
                        self.write_line();
                    }
                }
                // export class C {} or export default class C {}
                k if k == syntax_kind_ext::CLASS_DECLARATION => {
                    // Emit the class declaration
                    self.emit_class_declaration(clause_node, export.export_clause);
                    self.write_line();

                    // Get class name and emit export (unless file has export =)
                    if !self.ctx.module_state.has_export_assignment
                        && let Some(class) = self.arena.get_class(clause_node)
                        && let Some(name) = self.get_identifier_text_opt(class.name)
                    {
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
                // export enum E {}
                k if k == syntax_kind_ext::ENUM_DECLARATION => {
                    self.emit_enum_declaration(clause_node, export.export_clause);
                    self.write_line();

                    if !self.ctx.module_state.has_export_assignment
                        && let Some(enum_decl) = self.arena.get_enum(clause_node)
                        && let Some(name) = self.get_identifier_text_opt(enum_decl.name)
                    {
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
                // export namespace N {}
                k if k == syntax_kind_ext::MODULE_DECLARATION => {
                    self.emit_module_declaration(clause_node, export.export_clause);
                    self.write_line();

                    if !self.ctx.module_state.has_export_assignment
                        && let Some(module_decl) = self.arena.get_module(clause_node)
                        && let Some(name) = self.get_module_root_name(module_decl.name)
                    {
                        self.write("exports.");
                        self.write(&name);
                        self.write(" = ");
                        self.write(&name);
                        self.write(";");
                        self.write_line();
                    }
                }
                // export { x, y } - local re-export without module specifier
                k if k == syntax_kind_ext::NAMED_EXPORTS => {
                    // Emit exports.x = x; for each name
                    if let Some(named_exports) = self.arena.get_named_imports(clause_node) {
                        let value_specs = self.collect_value_specifiers(&named_exports.elements);
                        if value_specs.is_empty() {
                            // `export {}` or all type-only → no-op in CommonJS
                            return;
                        }

                        for &spec_idx in &value_specs {
                            if let Some(spec_node) = self.arena.get(spec_idx)
                                && let Some(spec) = self.arena.get_specifier(spec_node)
                            {
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

        // Check if we're inside an AMD/UMD wrapper (original module was AMD/UMD)
        let is_amd_or_umd = matches!(
            self.ctx.original_module_kind,
            Some(ModuleKind::AMD) | Some(ModuleKind::UMD)
        );

        if is_amd_or_umd && export_assign.is_export_equals {
            // AMD/UMD: export = expr → return expr;
            self.write("return ");
            self.emit_expression(export_assign.expression);
            self.write_semicolon();
        } else if self.ctx.is_commonjs() {
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

    /// Collect variable names from a `VARIABLE_STATEMENT` node
    pub(super) fn collect_variable_names_from_node(&self, node: &Node) -> Vec<String> {
        let mut names = Vec::new();
        if let Some(var_stmt) = self.arena.get_variable(node) {
            // VARIABLE_STATEMENT has declarations containing VARIABLE_DECLARATION_LIST
            for &decl_list_idx in &var_stmt.declarations.nodes {
                if let Some(decl_list_node) = self.arena.get(decl_list_idx) {
                    // VARIABLE_DECLARATION_LIST has declarations containing VARIABLE_DECLARATION
                    if let Some(decl_list) = self.arena.get_variable(decl_list_node) {
                        for &decl_idx in &decl_list.declarations.nodes {
                            if let Some(decl_node) = self.arena.get(decl_idx)
                                && let Some(decl) = self.arena.get_variable_declaration(decl_node)
                            {
                                self.collect_binding_names(decl.name, &mut names);
                            }
                        }
                    }
                }
            }
        }
        names
    }

    /// Try to collect inline CJS export info for a variable statement.
    /// Returns Some(vec of (name, `initializer_idx`)) if ALL declarators are simple
    /// identifier bindings with initializers. Returns None if any declarator uses
    /// destructuring or lacks an initializer (in which case we fall back to split form).
    pub(super) fn try_collect_inline_cjs_exports(
        &self,
        node: &Node,
    ) -> Option<Vec<(String, NodeIndex)>> {
        let var_stmt = self.arena.get_variable(node)?;
        let mut result = Vec::new();

        for &decl_list_idx in &var_stmt.declarations.nodes {
            let decl_list_node = self.arena.get(decl_list_idx)?;
            let decl_list = self.arena.get_variable(decl_list_node)?;

            for &decl_idx in &decl_list.declarations.nodes {
                let decl_node = self.arena.get(decl_idx)?;
                let decl = self.arena.get_variable_declaration(decl_node)?;

                // Must be a simple identifier (not destructuring)
                let name_node = self.arena.get(decl.name)?;
                if name_node.kind != SyntaxKind::Identifier as u16 {
                    return None;
                }
                let name = self.arena.get_identifier(name_node)?.escaped_text.clone();

                // Must have an initializer
                if decl.initializer.is_none() {
                    return None;
                }

                result.push((name, decl.initializer));
            }
        }

        if result.is_empty() {
            return None;
        }
        Some(result)
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

        if node.kind == syntax_kind_ext::QUALIFIED_NAME
            && let Some(qn) = self.arena.qualified_names.get(node.data_index as usize)
        {
            return self.get_module_root_name(qn.left);
        }

        None
    }

    /// Get identifier text from a node index
    pub(super) fn get_identifier_text_idx(&self, idx: NodeIndex) -> String {
        if let Some(node) = self.arena.get(idx)
            && node.kind == SyntaxKind::Identifier as u16
            && let Some(id) = self.arena.get_identifier(node)
        {
            return id.escaped_text.clone();
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
            // Check explicit "import type" syntax (parser-set flag)
            if let Some(spec_node) = self.arena.get(spec_idx)
                && let Some(spec) = self.arena.get_specifier(spec_node)
                && spec.is_type_only
            {
                continue;
            }
            // Check implicit type-only imports (type checker side-table)
            // This handles cases like `import { Interface }` where Interface refers to an interface
            if self.ctx.options.type_only_nodes.contains(&spec_idx) {
                continue;
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
            if let Some(node) = self.arena.get(stmt_idx)
                && node.kind == syntax_kind_ext::EXPORT_ASSIGNMENT
            {
                return true;
            }
        }
        false
    }

    /// Check if a file is a module (has any import/export syntax).
    /// TypeScript considers a file a module if it has ANY import/export syntax,
    /// including type-only imports/exports, declared exports, and exported
    /// interfaces/type aliases.
    pub(super) fn file_is_module(&self, statements: &NodeList) -> bool {
        for &stmt_idx in &statements.nodes {
            if let Some(node) = self.arena.get(stmt_idx) {
                match node.kind {
                    k if k == syntax_kind_ext::IMPORT_DECLARATION
                        || k == syntax_kind_ext::EXPORT_DECLARATION
                        || k == syntax_kind_ext::EXPORT_ASSIGNMENT =>
                    {
                        return true;
                    }
                    // import equals: only `import x = require("mod")` makes this a module,
                    // NOT `import x = M.A` (namespace alias, not a module indicator)
                    k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                        if let Some(import_data) = self.arena.get_import_decl(node)
                            && let Some(spec_node) = self.arena.get(import_data.module_specifier)
                            && spec_node.kind == SyntaxKind::StringLiteral as u16
                        {
                            return true;
                        }
                    }
                    // Check for export modifier on any declaration type
                    k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                        if let Some(var_stmt) = self.arena.get_variable(node)
                            && self.has_export_modifier(&var_stmt.modifiers)
                        {
                            return true;
                        }
                    }
                    k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                        if let Some(func) = self.arena.get_function(node)
                            && self.has_export_modifier(&func.modifiers)
                        {
                            return true;
                        }
                    }
                    k if k == syntax_kind_ext::CLASS_DECLARATION => {
                        if let Some(class) = self.arena.get_class(node)
                            && self.has_export_modifier(&class.modifiers)
                        {
                            return true;
                        }
                    }
                    k if k == syntax_kind_ext::ENUM_DECLARATION => {
                        if let Some(enum_decl) = self.arena.get_enum(node)
                            && self.has_export_modifier(&enum_decl.modifiers)
                        {
                            return true;
                        }
                    }
                    k if k == syntax_kind_ext::MODULE_DECLARATION => {
                        if let Some(module) = self.arena.get_module(node)
                            && self.has_export_modifier(&module.modifiers)
                        {
                            return true;
                        }
                    }
                    k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                        if let Some(iface) = self.arena.get_interface(node)
                            && self.has_export_modifier(&iface.modifiers)
                        {
                            return true;
                        }
                    }
                    k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                        if let Some(type_alias) = self.arena.get_type_alias(node)
                            && self.has_export_modifier(&type_alias.modifiers)
                        {
                            return true;
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
                        && !deps.contains(&text)
                    {
                        deps.push(text);
                    }
                }
                continue;
            }

            if node.kind == syntax_kind_ext::EXPORT_DECLARATION
                && let Some(export_decl) = self.arena.get_export_decl(node)
            {
                if !self.export_decl_has_runtime_value(export_decl) {
                    continue;
                }
                if let Some(text) = self.get_module_specifier_text(export_decl.module_specifier)
                    && !deps.contains(&text)
                {
                    deps.push(text);
                }
            }
        }

        deps
    }

    fn get_module_specifier_text(&self, specifier: NodeIndex) -> Option<String> {
        if specifier.is_none() {
            return None;
        }

        let node = self.arena.get(specifier)?;
        let literal = self.arena.get_literal(node)?;

        Some(literal.text.clone())
    }

    pub(super) fn import_decl_has_runtime_value(
        &self,
        import_decl: &tsz_parser::parser::node::ImportDeclData,
    ) -> bool {
        if import_decl.import_clause.is_none() {
            return true;
        }

        let Some(clause_node) = self.arena.get(import_decl.import_clause) else {
            return true;
        };

        if clause_node.kind != syntax_kind_ext::IMPORT_CLAUSE {
            // For `import X = require("module")`, check if it has an external module.
            // For `import X = Y` (qualified name), always treat as runtime value
            // since it produces `var X = Y;` at runtime.
            if let Some(spec_node) = self.arena.get(import_decl.module_specifier) {
                return spec_node.kind == SyntaxKind::StringLiteral as u16
                    || spec_node.kind == SyntaxKind::Identifier as u16
                    || spec_node.kind == syntax_kind_ext::QUALIFIED_NAME
                    || spec_node.kind == syntax_kind_ext::EXTERNAL_MODULE_REFERENCE;
            }
            return false;
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
            if let Some(spec) = self.arena.get_specifier(spec_node)
                && !spec.is_type_only
            {
                return true;
            }
        }

        false
    }

    pub(super) fn export_decl_has_runtime_value(
        &self,
        export_decl: &tsz_parser::parser::node::ExportDeclData,
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
                if let Some(spec) = self.arena.get_specifier(spec_node)
                    && !spec.is_type_only
                {
                    return true;
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
    /// excluding `export =` which is legacy `CommonJS`.
    /// TypeScript emits __esModule for ANY module syntax, including type-only
    /// imports/exports, declared exports, and exported interfaces/type aliases.
    pub(super) fn should_emit_es_module_marker(&self, statements: &NodeList) -> bool {
        // First check: if file has export =, don't emit __esModule at all
        for &stmt_idx in &statements.nodes {
            if let Some(node) = self.arena.get(stmt_idx)
                && node.kind == syntax_kind_ext::EXPORT_ASSIGNMENT
            {
                return false;
            }
        }

        // Second check: look for ANY module syntax (including type-only)
        for &stmt_idx in &statements.nodes {
            if let Some(node) = self.arena.get(stmt_idx) {
                match node.kind {
                    k if k == syntax_kind_ext::IMPORT_DECLARATION => {
                        return true;
                    }
                    // import equals: only `import x = require("mod")` is module syntax
                    k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                        if let Some(import_data) = self.arena.get_import_decl(node)
                            && let Some(spec_node) = self.arena.get(import_data.module_specifier)
                            && spec_node.kind == SyntaxKind::StringLiteral as u16
                        {
                            return true;
                        }
                    }
                    k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                        return true;
                    }
                    // Check for export modifier on any declaration type
                    // (including declare and type-only declarations)
                    k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                        if let Some(var_stmt) = self.arena.get_variable(node)
                            && self.has_export_modifier(&var_stmt.modifiers)
                        {
                            return true;
                        }
                    }
                    k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                        if let Some(func) = self.arena.get_function(node)
                            && self.has_export_modifier(&func.modifiers)
                        {
                            return true;
                        }
                    }
                    k if k == syntax_kind_ext::CLASS_DECLARATION => {
                        if let Some(class) = self.arena.get_class(node)
                            && self.has_export_modifier(&class.modifiers)
                        {
                            return true;
                        }
                    }
                    k if k == syntax_kind_ext::ENUM_DECLARATION => {
                        if let Some(enum_decl) = self.arena.get_enum(node)
                            && self.has_export_modifier(&enum_decl.modifiers)
                        {
                            return true;
                        }
                    }
                    k if k == syntax_kind_ext::MODULE_DECLARATION => {
                        if let Some(module) = self.arena.get_module(node)
                            && self.has_export_modifier(&module.modifiers)
                        {
                            return true;
                        }
                    }
                    k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                        if let Some(iface) = self.arena.get_interface(node)
                            && self.has_export_modifier(&iface.modifiers)
                        {
                            return true;
                        }
                    }
                    k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                        if let Some(type_alias) = self.arena.get_type_alias(node)
                            && self.has_export_modifier(&type_alias.modifiers)
                        {
                            return true;
                        }
                    }
                    _ => {}
                }
            }
        }
        false
    }

    /// Write the appropriate variable declaration keyword based on target.
    /// For ES2015+, use `const` for top-level module imports.
    /// For ES3/ES5, use `var`.
    pub(super) fn write_var_or_const(&mut self) {
        if self.ctx.target_es5 {
            self.write("var ");
        } else {
            self.write("const ");
        }
    }
}
