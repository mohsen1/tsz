use super::super::{ModuleKind, Printer};
use crate::transforms::ClassES5Emitter;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn emit_export_declaration_commonjs(
        &mut self,
        node: &tsz_parser::parser::node::Node,
    ) {
        let Some(export) = self.arena.get_export_decl(node) else {
            return;
        };

        if export.is_type_only {
            return;
        }

        // Re-export from another module: export { x } from "module";
        if export.module_specifier.is_some() {
            let module_spec = if let Some(spec_node) = self.arena.get(export.module_specifier) {
                if let Some(lit) = self.arena.get_literal(spec_node) {
                    lit.text.clone()
                } else {
                    return;
                }
            } else {
                return;
            };

            let module_var = self.next_commonjs_module_var(&module_spec);

            if export.export_clause.is_none() {
                // TSC emits `var` for CommonJS re-export helper bindings.
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
            if let Some(clause_node) = self.arena.get(export.export_clause)
                && let Some(named_exports) = self.arena.get_named_imports(clause_node)
            {
                let value_specs = self.collect_value_specifiers(&named_exports.elements);
                if value_specs.is_empty() {
                    return;
                }

                for &spec_idx in &named_exports.elements.nodes {
                    if let Some(spec_node) = self.arena.get(spec_idx)
                        && let Some(spec) = self.arena.get_specifier(spec_node)
                    {
                        if spec.is_type_only {
                            continue;
                        }
                        let export_name = self.get_identifier_text_idx(spec.name);
                        if export_name.is_empty() {
                            continue;
                        }
                        self.write("exports.");
                        self.write(&export_name);
                        self.write(" = void 0;");
                        self.write_line();
                    }
                }

                // TSC emits `var` for CommonJS re-export helper bindings.
                self.write("var ");
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
                        let import_name = if spec.property_name.is_some() {
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
                        is_anonymous_default = func_name == "function"
                            || !super::super::is_valid_identifier_name(&func_name);
                    }
                }
                k if k == syntax_kind_ext::CLASS_DECLARATION => {
                    if let Some(class) = self.arena.get_class(clause_node) {
                        let class_name = self.get_identifier_text_idx(class.name);
                        is_anonymous_default = !super::super::is_valid_identifier_name(&class_name);
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
            if export.is_default_export
                && (clause_node.kind == SyntaxKind::Identifier as u16
                    || clause_node.kind == syntax_kind_ext::QUALIFIED_NAME)
                && !self.namespace_alias_target_has_runtime_value(export.export_clause, None)
            {
                // `export default T` where `T` is type-only has no JS runtime emit.
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
                let is_legacy_decorated_export_class = clause_kind
                    == syntax_kind_ext::CLASS_DECLARATION
                    && self.ctx.options.legacy_decorators
                    && !export.is_default_export
                    && self.arena.get_class(clause_node).is_some_and(|class| {
                        !self.collect_class_decorators(&class.modifiers).is_empty()
                    });
                if !is_legacy_decorated_export_class {
                    self.emit(export.export_clause);
                    return;
                }
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
                                if let Some(init_node) = self.arena.get(*init_idx)
                                    && init_node.kind == SyntaxKind::Identifier as u16
                                {
                                    let ident = self.get_identifier_text_idx(*init_idx);
                                    if self
                                        .ctx
                                        .module_state
                                        .pending_exports
                                        .iter()
                                        .any(|n| n == &ident)
                                    {
                                        self.write("exports.");
                                        self.write(&ident);
                                    } else {
                                        self.emit(*init_idx);
                                    }
                                } else {
                                    self.emit(*init_idx);
                                }
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
                    // For default exports of named functions, tsc emits the
                    // `exports.default = name;` assignment BEFORE the function
                    // declaration. This works because JS function declarations
                    // are hoisted, so the binding exists at the top of the scope.
                    // When the default export was already hoisted to the preamble,
                    // skip the inline emission to avoid duplicates.
                    if !self.ctx.module_state.has_export_assignment
                        && !self.ctx.module_state.default_func_export_hoisted
                        && export.is_default_export
                        && let Some(func) = self.arena.get_function(clause_node)
                        && let Some(name) = self.get_identifier_text_opt(func.name)
                    {
                        self.write("exports.default = ");
                        self.write(&name);
                        self.write(";");
                        self.write_line();
                    }

                    // Emit the function declaration
                    self.emit_function_declaration(clause_node, export.export_clause);
                    self.write_line();
                }
                // export class C {} or export default class C {}
                k if k == syntax_kind_ext::CLASS_DECLARATION => {
                    let mut named_export_emitted_with_class = false;
                    if !self.ctx.module_state.has_export_assignment
                        && !export.is_default_export
                        && let Some(class) = self.arena.get_class(clause_node)
                        && let Some(name) = self.get_identifier_text_opt(class.name)
                    {
                        // Keep named class export assignment immediately after the class
                        // declaration and before lowered static blocks/IIFEs.
                        self.pending_commonjs_class_export_name = Some(name);
                        named_export_emitted_with_class = true;
                    }

                    if self.ctx.options.legacy_decorators
                        && !self.ctx.module_state.has_export_assignment
                        && !export.is_default_export
                        && let Some(class) = self.arena.get_class(clause_node)
                    {
                        let legacy_decorators = self.collect_class_decorators(&class.modifiers);
                        if !legacy_decorators.is_empty()
                            && let Some(name) = self.get_identifier_text_opt(class.name)
                        {
                            // Clear pending_commonjs_class_export_name to avoid duplicate
                            // exports.X = X; — the decorator assignment path handles the
                            // pre-assignment itself via emit_commonjs_pre_assignment=true.
                            self.pending_commonjs_class_export_name = None;
                            if self.ctx.target_es5 {
                                let mut es5_emitter = ClassES5Emitter::new(self.arena);
                                es5_emitter.set_indent_level(self.writer.indent_level());
                                es5_emitter.set_transforms(self.transforms.clone());
                                if let Some(text) = self.source_text_for_map() {
                                    if self.writer.has_source_map() {
                                        es5_emitter.set_source_map_context(
                                            text,
                                            self.writer.current_source_index(),
                                        );
                                    } else {
                                        es5_emitter.set_source_text(text);
                                    }
                                }
                                let output = es5_emitter.emit_class(export.export_clause);
                                let mappings = es5_emitter.take_mappings();
                                if !mappings.is_empty() && self.writer.has_source_map() {
                                    self.writer.write("");
                                    let base_line = self.writer.current_line();
                                    let base_column = self.writer.current_column();
                                    self.writer.add_offset_mappings(
                                        base_line,
                                        base_column,
                                        &mappings,
                                    );
                                    self.writer.write(&output);
                                } else {
                                    self.write(&output);
                                }
                                while self.comment_emit_idx < self.all_comments.len()
                                    && self.all_comments[self.comment_emit_idx].end
                                        <= clause_node.end
                                {
                                    self.comment_emit_idx += 1;
                                }
                            } else {
                                self.emit_class_es6_with_options(
                                    clause_node,
                                    export.export_clause,
                                    true,
                                    Some(("let", name.clone())),
                                );
                            }
                            self.write_line();
                            self.emit_legacy_class_decorator_assignment(
                                &name,
                                &legacy_decorators,
                                true,
                                false,
                                true,
                            );
                            self.write_line();
                            return;
                        }
                    }

                    // Emit the class declaration
                    self.emit_class_declaration(clause_node, export.export_clause);
                    // Only write a newline if we're not already at line start
                    // (class declarations with lowered static fields already end
                    // with write_line() after the last `ClassName.field = value;`)
                    if !self.writer.is_at_line_start() {
                        self.write_line();
                    }

                    // Get class name and emit export (unless file has export =)
                    if !self.ctx.module_state.has_export_assignment
                        && let Some(class) = self.arena.get_class(clause_node)
                        && let Some(name) = self.get_identifier_text_opt(class.name)
                    {
                        if export.is_default_export {
                            self.write("exports.default = ");
                            self.write(&name);
                            self.write(";");
                            self.write_line();
                        } else if !named_export_emitted_with_class {
                            self.write("exports.");
                            self.write(&name);
                            self.write(" = ");
                            self.write(&name);
                            self.write(";");
                            self.write_line();
                        } else {
                            // Named exports were already emitted at class-body boundary.
                        }
                    }
                }
                // export enum E {}
                k if k == syntax_kind_ext::ENUM_DECLARATION => {
                    let is_amd_or_umd_wrapped = matches!(
                        self.ctx.original_module_kind,
                        Some(ModuleKind::AMD | ModuleKind::UMD)
                    );
                    if is_amd_or_umd_wrapped
                        && !export.is_default_export
                        && !self.ctx.module_state.has_export_assignment
                        && let Some(enum_decl) = self.arena.get_enum(clause_node)
                        && let Some(name) = self.get_identifier_text_opt(enum_decl.name)
                    {
                        let mut enum_emitter = crate::transforms::EnumES5Emitter::new(self.arena);
                        enum_emitter.set_indent_level(self.writer.indent_level());
                        if let Some(text) = self.source_text {
                            enum_emitter.set_source_text(text);
                        }
                        let mut output = enum_emitter.emit_enum(export.export_clause);
                        let from = format!("({name} || ({name} = {{}}))");
                        let to = format!("({name} || (exports.{name} = {name} = {{}}))");
                        output = output.replacen(&from, &to, 1);
                        let mut emit_text = output.trim_end_matches('\n');
                        while let Some((first, rest)) = emit_text.split_once('\n') {
                            if first.trim().is_empty() {
                                emit_text = rest;
                                continue;
                            }
                            break;
                        }
                        self.write(emit_text);
                    } else if !export.is_default_export
                        && !self.ctx.module_state.has_export_assignment
                        && let Some(enum_decl) = self.arena.get_enum(clause_node)
                        && let Some(name) = self.get_identifier_text_opt(enum_decl.name)
                    {
                        // For non-default CJS exported enums, fold exports.Name into
                        // the IIFE tail: (E || (exports.E = E = {}))
                        // This matches tsc's compact form instead of a separate
                        // exports.E = E; statement.
                        let mut enum_emitter = crate::transforms::EnumES5Emitter::new(self.arena);
                        enum_emitter.set_indent_level(self.writer.indent_level());
                        if let Some(text) = self.source_text {
                            enum_emitter.set_source_text(text);
                        }
                        let mut output = enum_emitter.emit_enum(export.export_clause);
                        let from = format!("({name} || ({name} = {{}}))");
                        let to = format!("({name} || (exports.{name} = {name} = {{}}))");
                        output = output.replacen(&from, &to, 1);
                        let emit_text = output.trim_end_matches('\n');
                        self.write(emit_text);
                    } else {
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
                }
                // export namespace N {}
                k if k == syntax_kind_ext::MODULE_DECLARATION => {
                    if !export.is_default_export && !self.ctx.module_state.has_export_assignment {
                        // Fold exports.Name into the IIFE tail:
                        // (N || (exports.N = N = {})) instead of separate
                        // `exports.N = N;` after the IIFE.
                        self.pending_cjs_namespace_export_fold = true;
                        self.emit_module_declaration(clause_node, export.export_clause);
                        // If the flag was consumed (instantiated namespace),
                        // no separate export needed. If still set, the namespace
                        // was non-instantiated/skipped, clear it.
                        self.pending_cjs_namespace_export_fold = false;
                    } else {
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
                                let local_name = if spec.property_name.is_some() {
                                    self.get_identifier_text_idx(spec.property_name)
                                } else {
                                    export_name.clone()
                                };

                                // Skip function export specifiers already handled
                                // by the preamble (`exports.f = f;` before statements).
                                if self
                                    .ctx
                                    .module_state
                                    .hoisted_func_exports
                                    .iter()
                                    .any(|n| n == &export_name)
                                {
                                    continue;
                                }

                                self.write("exports.");
                                self.write(&export_name);
                                self.write(" = ");
                                if export_name != local_name
                                    && self
                                        .ctx
                                        .module_state
                                        .pending_exports
                                        .iter()
                                        .any(|n| n == &local_name)
                                {
                                    self.write("exports.");
                                }
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
                    if let Some(expr_node) = self.arena.get(export.export_clause)
                        && expr_node.kind == SyntaxKind::Identifier as u16
                    {
                        let ident = self.get_identifier_text_idx(export.export_clause);
                        if self
                            .ctx
                            .module_state
                            .pending_exports
                            .iter()
                            .any(|n| n == &ident)
                        {
                            self.write("exports.default = exports.");
                            self.write(&ident);
                            self.write(";");
                            self.write_line();
                            return;
                        }
                    }

                    self.write("exports.default = ");
                    self.emit(export.export_clause);
                    self.write_semicolon();
                }
            }
        }
    }
}
