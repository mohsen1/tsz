use super::super::{ModuleKind, Printer};
use crate::transforms::{ClassDecoratorInfo, ClassES5Emitter};
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    /// Write `exports.name` or `exports["name"]` depending on whether the name
    /// is a valid JS identifier. Does NOT write ` = `.
    pub(in crate::emitter) fn write_export_property_access(&mut self, export_name: &str) {
        if super::super::is_valid_identifier_name(export_name) {
            self.write("exports.");
            self.write(export_name);
        } else {
            self.write("exports[\"");
            self.write(export_name);
            self.write("\"]");
        }
    }

    /// Write a CJS/System export assignment for a named or default export.
    /// In System modules, uses `exports_1("name", value)` format.
    /// In CJS modules, uses `exports.name = value` format.
    /// After calling this, the caller should write the VALUE and terminator.
    pub(in crate::emitter) fn write_export_binding_start(&mut self, export_name: &str) {
        if self.in_system_execute_body {
            self.write("exports_1(\"");
            self.write(export_name);
            self.write("\", ");
        } else if super::super::is_valid_identifier_name(export_name) {
            self.write("exports.");
            self.write(export_name);
            self.write(" = ");
        } else {
            self.write("exports[\"");
            self.write(export_name);
            self.write("\"] = ");
        }
    }

    /// Write the terminator for an export binding started with `write_export_binding_start`.
    pub(in crate::emitter) fn write_export_binding_end(&mut self) {
        if self.in_system_execute_body {
            self.write(");");
        } else {
            self.write(";");
        }
    }

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
                    self.rewrite_module_spec(&lit.text)
                } else {
                    return;
                }
            } else {
                return;
            };

            // Handle `export * from "mod"` -> `__exportStar(require("mod"), exports);`
            // tsc emits an inline require (no temp variable), so we do the same to
            // avoid wasting a module_var counter value.
            if export.export_clause.is_none() {
                self.write_helper("__exportStar");
                self.write("(require(\"");
                self.write(&module_spec);
                self.write("\"), exports);");
                self.write_line();
                return;
            }

            // Handle `export * as ns from "mod"` -> `exports.ns = require("mod")` or
            // `exports.ns = __importStar(require("mod"))` when esModuleInterop is enabled.
            if let Some(clause_node) = self.arena.get(export.export_clause)
                && clause_node.kind != syntax_kind_ext::NAMED_EXPORTS
            {
                let ns_name = if clause_node.kind == SyntaxKind::StringLiteral as u16 {
                    self.arena
                        .get_literal(clause_node)
                        .map(|lit| lit.text.clone())
                        .unwrap_or_default()
                } else {
                    self.get_identifier_text_idx(export.export_clause)
                };
                if !ns_name.is_empty() {
                    let needs_bracket = !super::super::is_valid_identifier_name(&ns_name);
                    if needs_bracket {
                        self.write("exports[\"");
                        self.write(&ns_name);
                        self.write("\"] = ");
                    } else {
                        self.write("exports.");
                        self.write(&ns_name);
                        self.write(" = ");
                    }
                    if self.ctx.options.es_module_interop {
                        self.write_helper("__importStar");
                        self.write("(require(\"");
                        self.write(&module_spec);
                        self.write("\"));");
                    } else {
                        self.write("require(\"");
                        self.write(&module_spec);
                        self.write("\");");
                    }
                    self.write_line();
                }
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

                // Re-export void 0 initialization is handled in the preamble
                // (collect_export_names_with_options) so it's chained with local exports.

                let module_var = self.next_commonjs_module_var(&module_spec);
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
                        // Get export name and import name (can be string literals)
                        let Some(export_name) = self.get_specifier_name_text(spec.name) else {
                            continue;
                        };
                        let import_name = if spec.property_name.is_some() {
                            self.get_specifier_name_text(spec.property_name)
                                .unwrap_or_else(|| export_name.clone())
                        } else {
                            export_name.clone()
                        };

                        // Object.defineProperty(exports, "name", { enumerable: true, get: function () { return mod.name; } });
                        // When esModuleInterop is enabled and the imported name is "default",
                        // wrap with __importDefault: return __importDefault(mod).default;
                        self.write("Object.defineProperty(exports, \"");
                        self.write(&export_name);
                        self.write("\", { enumerable: true, get: function () { return ");
                        if self.ctx.options.es_module_interop && import_name == "default" {
                            self.write_helper("__importDefault");
                            self.write("(");
                            self.write(&module_var);
                            self.write(").default");
                        } else {
                            self.write_module_property_access(&module_var, &import_name);
                        }
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
                && !self.export_default_target_has_runtime_value(export.export_clause)
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
                            let decl_count = inline_decls.len();
                            for (i, (decoded_name, emit_name, init_idx)) in
                                inline_decls.iter().enumerate()
                            {
                                // Track that this variable was inlined (no local declaration).
                                // Use decoded name for set tracking (matching uses decoded text).
                                self.ctx
                                    .module_state
                                    .inlined_var_exports
                                    .insert(decoded_name.clone());
                                self.write("exports.");
                                // Use emit_name to preserve unicode escapes in output.
                                self.write(emit_name);
                                self.write(" = ");
                                // emit_identifier handles `x → exports.x` substitution
                                // for inline-exported variable names automatically.
                                self.emit(*init_idx);
                                self.write(";");
                                // Skip write_line() on the last declaration so the
                                // caller can emit trailing comments before the newline.
                                if i < decl_count - 1 {
                                    self.write_line();
                                }
                            }
                        } else {
                            // Complex case (destructuring): transform into comma
                            // expression that directly assigns to exports, matching tsc.
                            self.emit_cjs_destructuring_export(clause_node);
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
                        self.write_export_binding_start("default");
                        self.write(&name);
                        self.write_export_binding_end();
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
                                // Check for member decorators too
                                let has_member_decorators =
                                    class.members.nodes.iter().any(|&m_idx| {
                                        let Some(m_node) = self.arena.get(m_idx) else {
                                            return false;
                                        };
                                        let mods = match m_node.kind {
                                            k if k == syntax_kind_ext::METHOD_DECLARATION => self
                                                .arena
                                                .get_method_decl(m_node)
                                                .and_then(|m| m.modifiers.as_ref()),
                                            k if k == syntax_kind_ext::PROPERTY_DECLARATION => self
                                                .arena
                                                .get_property_decl(m_node)
                                                .and_then(|p| p.modifiers.as_ref()),
                                            k if k == syntax_kind_ext::GET_ACCESSOR
                                                || k == syntax_kind_ext::SET_ACCESSOR =>
                                            {
                                                self.arena
                                                    .get_accessor(m_node)
                                                    .and_then(|a| a.modifiers.as_ref())
                                            }
                                            _ => None,
                                        };
                                        mods.is_some_and(|m| {
                                            m.nodes.iter().any(|&mod_idx| {
                                                self.arena.get(mod_idx).is_some_and(|n| {
                                                    n.kind == syntax_kind_ext::DECORATOR
                                                })
                                            })
                                        })
                                    });

                                let mut es5_emitter = ClassES5Emitter::new(self.arena);
                                es5_emitter.set_temp_var_counter(
                                    self.ctx.destructuring_state.temp_var_counter,
                                );
                                es5_emitter.set_indent_level(self.writer.indent_level());
                                es5_emitter.set_transforms(self.transforms.clone());
                                es5_emitter.set_remove_comments(self.ctx.options.remove_comments);
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
                                if self.ctx.options.import_helpers
                                    && self.ctx.is_effectively_commonjs()
                                {
                                    es5_emitter.set_tslib_prefix(true);
                                }
                                es5_emitter.set_use_define_for_class_fields(
                                    self.ctx.options.use_define_for_class_fields,
                                );
                                // Pass decorator info so __decorate calls are inside the IIFE
                                es5_emitter.set_decorator_info(ClassDecoratorInfo {
                                    class_decorators: legacy_decorators.clone(),
                                    has_member_decorators,
                                    emit_decorator_metadata: self
                                        .ctx
                                        .options
                                        .emit_decorator_metadata,
                                });
                                let output = es5_emitter.emit_class(export.export_clause);
                                self.ctx.destructuring_state.temp_var_counter =
                                    es5_emitter.temp_var_counter();
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
                                self.write_line();
                                // For ES5, decorator calls are inside the IIFE,
                                // but we still need the CommonJS export assignment
                                self.write("exports.");
                                self.write(&name);
                                self.write(" = ");
                                self.write(&name);
                                self.write(";");
                                self.write_line();
                            } else {
                                self.emit_class_es6_with_options(
                                    clause_node,
                                    export.export_clause,
                                    true,
                                    Some(("let", name.clone())),
                                );
                                self.write_line();
                                // CommonJS export assignment
                                self.write("exports.");
                                self.write(&name);
                                self.write(" = ");
                                self.write(&name);
                                self.write(";");
                                self.write_line();
                                // Emit __decorate call for ES2015+
                                let members = class.members.nodes.clone();
                                self.emit_legacy_class_decorator_assignment(
                                    &name,
                                    &legacy_decorators,
                                    true,  // commonjs_exported
                                    false, // commonjs_default
                                    false, // emit_commonjs_pre_assignment (already emitted above)
                                    &members,
                                );
                            }
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
                            self.write_export_binding_start("default");
                            self.write(&name);
                            self.write_export_binding_end();
                            self.write_line();
                        } else if !named_export_emitted_with_class {
                            self.write_export_binding_start(&name);
                            self.write(&name);
                            self.write_export_binding_end();
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
                        enum_emitter
                            .set_preserve_const_enums(self.ctx.options.preserve_const_enums);
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
                        && let Some(enum_decl) = self.arena.get_enum(clause_node)
                        && let Some(name) = self.get_identifier_text_opt(enum_decl.name)
                    {
                        // For non-default CJS exported enums, fold exports.Name into
                        // the IIFE tail: (E || (exports.E = E = {}))
                        // This matches tsc's compact form instead of a separate
                        // exports.E = E; statement.
                        // Note: fold applies even with has_export_assignment — `export =`
                        // and named exports are orthogonal in CJS.
                        let mut enum_emitter = crate::transforms::EnumES5Emitter::new(self.arena);
                        enum_emitter.set_indent_level(self.writer.indent_level());
                        enum_emitter
                            .set_preserve_const_enums(self.ctx.options.preserve_const_enums);
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
                            let export_name = if export.is_default_export {
                                "default".to_string()
                            } else {
                                name.clone()
                            };
                            self.write_export_binding_start(&export_name);
                            self.write(&name);
                            self.write_export_binding_end();
                            self.write_line();
                        }
                    }
                }
                // export namespace N {}
                k if k == syntax_kind_ext::MODULE_DECLARATION => {
                    if !export.is_default_export {
                        // Fold exports.Name into the IIFE tail:
                        // (N || (exports.N = N = {})) instead of separate
                        // `exports.N = N;` after the IIFE.
                        // Note: fold is used even when has_export_assignment is true —
                        // `export = X` sets module.exports but named exports like
                        // `export enum E` still get their own exports.E binding.
                        self.pending_cjs_namespace_export_fold = true;
                        self.emit_module_declaration(clause_node, export.export_clause);
                        // If the flag was consumed (instantiated namespace),
                        // no separate export needed. If still set, the namespace
                        // was non-instantiated/skipped, clear it.
                        self.pending_cjs_namespace_export_fold = false;
                    } else {
                        self.emit_module_declaration(clause_node, export.export_clause);
                    }
                }
                // export { x, y } - local re-export without module specifier
                k if k == syntax_kind_ext::NAMED_EXPORTS => {
                    // Emit exports.x = x; for each name
                    if let Some(named_exports) = self.arena.get_named_imports(clause_node) {
                        let value_specs =
                            self.collect_local_export_value_specifiers(&named_exports.elements);
                        if value_specs.is_empty() {
                            // `export {}` or all type-only → no-op in CommonJS
                            return;
                        }

                        for &spec_idx in &value_specs {
                            if let Some(spec_node) = self.arena.get(spec_idx)
                                && let Some(spec) = self.arena.get_specifier(spec_node)
                            {
                                let Some(export_name) = self.get_specifier_name_text(spec.name)
                                else {
                                    continue;
                                };
                                let local_name = if spec.property_name.is_some() {
                                    self.get_specifier_name_text(spec.property_name)
                                        .unwrap_or_else(|| export_name.clone())
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
                                    .any(|(e, _)| e == &export_name)
                                {
                                    continue;
                                }

                                // Skip namespace/enum export specifiers already folded
                                // into the IIFE closing arg (e.g., `(A || (exports.A = A = {}))`).
                                if self
                                    .ctx
                                    .module_state
                                    .iife_exported_names
                                    .contains(&local_name)
                                {
                                    continue;
                                }

                                // Skip names already emitted inline after their declarations.
                                if self
                                    .ctx
                                    .module_state
                                    .inline_exported_names
                                    .contains(&export_name)
                                {
                                    continue;
                                }

                                self.write_export_property_access(&export_name);
                                self.write(" = ");
                                // When the local name was inlined (no local var exists),
                                // use exports.local_name. Otherwise use local name.
                                if export_name != local_name
                                    && self
                                        .ctx
                                        .module_state
                                        .inlined_var_exports
                                        .contains(&local_name)
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
                // In System modules, use exports_1("default", expr) instead.
                _ => {
                    // `export default X` — use `exports.X` only when the variable
                    // was inlined (`exports.x = val;` with no local declaration),
                    // otherwise use the local name (class/function/enum have local
                    // declarations).
                    if let Some(expr_node) = self.arena.get(export.export_clause)
                        && expr_node.kind == SyntaxKind::Identifier as u16
                    {
                        let ident = self.get_identifier_text_idx(export.export_clause);
                        if self.ctx.module_state.inlined_var_exports.contains(&ident) {
                            self.write("exports.default = exports.");
                            self.write(&ident);
                            self.write(";");
                            self.write_line();
                            return;
                        }
                    }
                    if self.in_system_execute_body {
                        self.write("exports_1(\"default\", ");
                        self.emit(export.export_clause);
                        self.write(");");
                    } else {
                        self.write("exports.default = ");
                        self.emit(export.export_clause);
                        self.write_semicolon();
                    }
                }
            }
        }
    }

    /// Emit an exported variable statement with destructuring binding patterns
    /// as a CJS/AMD comma expression that directly assigns to `exports.*`.
    ///
    /// For `export const { x, ...rest } = expr;` with esnext target:
    /// ```js
    /// _a = expr, exports.x = _a.x, exports.rest = __rest(_a, ["x"]);
    /// ```
    ///
    /// For `export const { x, ...rest } = expr;` with es5 target:
    /// ```js
    /// exports.x = (_a = expr, _a).x, exports.rest = __rest(_a, ["x"]);
    /// ```
    ///
    /// For empty patterns like `export const {} = {};` with esnext target:
    /// ```js
    /// _a = {};
    /// ```
    ///
    /// For empty patterns with es5 target:
    /// ```js
    /// exports._b = _a = {};
    /// ```
    /// Check if a `VARIABLE_STATEMENT` has any destructuring binding patterns.
    pub(in crate::emitter) fn variable_stmt_has_binding_pattern(
        &self,
        node: &tsz_parser::parser::node::Node,
    ) -> bool {
        let Some(var_stmt) = self.arena.get_variable(node) else {
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
                if let Some(name_node) = self.arena.get(decl.name)
                    && (name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                        || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN)
                {
                    return true;
                }
            }
        }
        false
    }

    pub(in crate::emitter) fn emit_cjs_destructuring_export(
        &mut self,
        clause_node: &tsz_parser::parser::node::Node,
    ) {
        let Some(var_stmt) = self.arena.get_variable(clause_node) else {
            return;
        };

        // Walk through declaration lists to find the variable declaration
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

                let Some(name_node) = self.arena.get(decl.name) else {
                    continue;
                };

                let is_binding_pattern = name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                    || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN;

                if !is_binding_pattern {
                    // Simple identifier — shouldn't reach here, but handle gracefully
                    let name = self.get_identifier_text_idx(decl.name);
                    if !name.is_empty() {
                        self.write("exports.");
                        self.write(&name);
                        self.write(" = ");
                        self.emit(decl.initializer);
                        self.write(";");
                    }
                    continue;
                }

                // Get binding pattern elements
                let Some(pattern) = self.arena.get_binding_pattern(name_node) else {
                    continue;
                };

                // Collect non-rest elements and rest element
                let mut non_rest_elems: Vec<(String, String)> = Vec::new(); // (export_name, prop_name)
                let mut rest_elem: Option<String> = None;
                let mut excluded_props: Vec<String> = Vec::new();

                for &elem_idx in &pattern.elements.nodes {
                    let Some(elem_node) = self.arena.get(elem_idx) else {
                        continue;
                    };
                    let Some(elem) = self.arena.get_binding_element(elem_node) else {
                        continue;
                    };

                    if elem.dot_dot_dot_token {
                        // Rest element
                        let rest_name = self.get_identifier_text(elem.name);
                        rest_elem = Some(rest_name);
                        continue;
                    }

                    // Get the variable (export) name
                    let var_name = self.get_identifier_text(elem.name);

                    // Get the property name to access on the source object
                    let prop_name = if elem.property_name.is_some() {
                        let pn = self.get_identifier_text_idx(elem.property_name);
                        if pn.is_empty() { var_name.clone() } else { pn }
                    } else {
                        var_name.clone()
                    };

                    excluded_props.push(prop_name.clone());
                    non_rest_elems.push((var_name, prop_name));
                }

                let is_empty = non_rest_elems.is_empty() && rest_elem.is_none();

                // Optimization: when there's exactly one binding (no rest), skip the
                // temp variable and emit `exports.x = (rhs).x` directly. tsc does this.
                if non_rest_elems.len() == 1 && rest_elem.is_none() {
                    let (export_name, prop_name) = &non_rest_elems[0];
                    // Check if RHS is a numeric literal — needs special formatting
                    // because `1.toString` is a JS parse error (`.` is decimal point).
                    // tsc emits `1..toString` (trailing dot on number, then prop access).
                    let init_is_numeric = decl.initializer.is_some()
                        && self
                            .arena
                            .get(decl.initializer)
                            .is_some_and(|n| n.kind == SyntaxKind::NumericLiteral as u16);
                    self.write("exports.");
                    self.write(export_name);
                    self.write(" = ");
                    self.emit(decl.initializer);
                    if init_is_numeric {
                        // Emit extra dot for numeric literal property access: 1..toString
                        self.write(".");
                    }
                    self.write(".");
                    self.write(prop_name);
                    self.write(";");
                    continue;
                }

                // Generate a hoisted temp var for the RHS.
                // CJS destructuring temps are placed BEFORE __esModule marker.
                let temp_name = self.make_unique_name_cjs_destructuring();

                if is_empty {
                    // Empty binding pattern
                    if self.ctx.target_es5 {
                        // es5: exports._b = _a = expr;
                        // _b is only used as export property name, no local var needed.
                        let export_temp = self.make_unique_name();
                        self.write("exports.");
                        self.write(&export_temp);
                        self.write(" = ");
                        self.write(&temp_name);
                        self.write(" = ");
                        self.emit(decl.initializer);
                        self.write(";");
                    } else {
                        // esnext: _a = expr;
                        self.write(&temp_name);
                        self.write(" = ");
                        self.emit(decl.initializer);
                        self.write(";");
                    }
                } else if self.ctx.target_es5 {
                    // es5 non-empty: exports.x = (_a = expr, _a).x, exports.rest = __rest(_a, ["x"]);
                    let mut first = true;
                    for (export_name, prop_name) in &non_rest_elems {
                        if !first {
                            self.write(", ");
                        }
                        self.write("exports.");
                        self.write(export_name);
                        self.write(" = (");
                        if first {
                            self.write(&temp_name);
                            self.write(" = ");
                            self.emit(decl.initializer);
                            self.write(", ");
                            self.write(&temp_name);
                        } else {
                            self.write(&temp_name);
                        }
                        self.write(").");
                        self.write(prop_name);
                        first = false;
                    }
                    if let Some(rest_name) = &rest_elem {
                        if !first {
                            self.write(", ");
                        }
                        self.write("exports.");
                        self.write(rest_name);
                        self.write(" = ");
                        self.write_helper("__rest");
                        self.write("(");
                        if first {
                            // Only rest, no non-rest elements — assign temp first
                            self.write(&temp_name);
                            self.write(" = ");
                            self.emit(decl.initializer);
                            self.write(", ");
                            self.write(&temp_name);
                        } else {
                            self.write(&temp_name);
                        }
                        self.write(", [");
                        for (i, prop) in excluded_props.iter().enumerate() {
                            if i > 0 {
                                self.write(", ");
                            }
                            self.write("\"");
                            self.write(prop);
                            self.write("\"");
                        }
                        self.write("])");
                    }
                    self.write(";");
                } else {
                    // esnext non-empty: _a = expr, exports.x = _a.x, exports.rest = __rest(_a, ["x"]);
                    self.write(&temp_name);
                    self.write(" = ");
                    self.emit(decl.initializer);

                    for (export_name, prop_name) in &non_rest_elems {
                        self.write(", ");
                        self.write("exports.");
                        self.write(export_name);
                        self.write(" = ");
                        self.write(&temp_name);
                        self.write(".");
                        self.write(prop_name);
                    }

                    if let Some(rest_name) = &rest_elem {
                        self.write(", ");
                        self.write("exports.");
                        self.write(rest_name);
                        self.write(" = ");
                        self.write_helper("__rest");
                        self.write("(");
                        self.write(&temp_name);
                        self.write(", [");
                        for (i, prop) in excluded_props.iter().enumerate() {
                            if i > 0 {
                                self.write(", ");
                            }
                            self.write("\"");
                            self.write(prop);
                            self.write("\"");
                        }
                        self.write("])");
                    }

                    self.write(";");
                }
            }
        }
    }
}
