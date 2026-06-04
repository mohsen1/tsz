//! Statement erasure decisions for JavaScript emit.

use super::{JsxEmit, ModuleKind, Printer};
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    /// Check if a top-level statement is erased in JS emit.
    pub(super) fn is_erased_statement(&self, node: &Node) -> bool {
        let is_es_module_output = matches!(
            self.ctx.options.module,
            ModuleKind::ES2015 | ModuleKind::ES2020 | ModuleKind::ES2022 | ModuleKind::ESNext
        );

        match node.kind {
            syntax_kind_ext::INTERFACE_DECLARATION
            | syntax_kind_ext::TYPE_ALIAS_DECLARATION
            | syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION => true,
            syntax_kind_ext::FUNCTION_DECLARATION => {
                if let Some(func) = self.arena.get_function(node) {
                    self.arena
                        .has_modifier(&func.modifiers, SyntaxKind::DeclareKeyword)
                        || (func.body.is_none()
                            && !self.has_recovered_declaration_trailing_comma(node)
                            && !self.has_recovered_anonymous_function_arrow(node, func.name))
                } else {
                    false
                }
            }
            syntax_kind_ext::CLASS_DECLARATION => {
                if let Some(class) = self.arena.get_class(node) {
                    self.arena
                        .has_modifier(&class.modifiers, SyntaxKind::DeclareKeyword)
                } else {
                    false
                }
            }
            syntax_kind_ext::ENUM_DECLARATION => {
                if let Some(enum_decl) = self.arena.get_enum(node) {
                    self.arena
                        .has_modifier(&enum_decl.modifiers, SyntaxKind::DeclareKeyword)
                        || (self
                            .arena
                            .has_modifier(&enum_decl.modifiers, SyntaxKind::ConstKeyword)
                            && !self.ctx.options.preserve_const_enums)
                } else {
                    false
                }
            }
            syntax_kind_ext::MODULE_DECLARATION => {
                if let Some(module) = self.arena.get_module(node) {
                    if node.is_global_augmentation() && module.body.is_none() {
                        return false;
                    }
                    if self
                        .arena
                        .has_modifier(&module.modifiers, SyntaxKind::DeclareKeyword)
                    {
                        return !self.is_recovered_anonymous_declare_module(module);
                    }
                    !self.is_instantiated_module(module.body)
                } else {
                    false
                }
            }
            syntax_kind_ext::VARIABLE_STATEMENT => {
                let Some(var_stmt) = self.arena.get_variable(node) else {
                    return false;
                };
                if self
                    .arena
                    .has_modifier(&var_stmt.modifiers, SyntaxKind::DeclareKeyword)
                {
                    return true;
                }
                // In CJS mode with `export =`, exported variables with no
                // initializers are already covered by the `exports.X = void 0`
                // preamble -- the bare `var X;` is redundant and should be erased.
                if self.ctx.is_commonjs()
                    && self.ctx.module_state.has_export_assignment
                    && self.all_declarations_lack_initializer(&var_stmt.declarations)
                    && self.all_declaration_names_in_exported_set(&var_stmt.declarations)
                {
                    return true;
                }
                false
            }
            syntax_kind_ext::EXPORT_DECLARATION => {
                let Some(export_data) = self.arena.get_export_decl(node) else {
                    return false;
                };
                // `export type { ... }` is always erased
                if export_data.is_type_only {
                    return true;
                }
                // `export <declaration>` - check if the inner declaration is erased
                // (e.g., `export declare namespace Foo { ... }`, `export interface Bar { }`)
                let Some(inner_node) = self.arena.get(export_data.export_clause) else {
                    return false;
                };
                // `export { type A, type B }` — all specifiers individually type-only
                if inner_node.kind == syntax_kind_ext::NAMED_EXPORTS
                    && let Some(named_exports) = self.arena.get_named_imports(inner_node)
                {
                    let all_type_only = !named_exports.elements.nodes.is_empty()
                        && named_exports.elements.nodes.iter().all(|&spec_idx| {
                            if let Some(spec_node) = self.arena.get(spec_idx)
                                && let Some(spec) = self.arena.get_specifier(spec_node)
                            {
                                spec.is_type_only
                                    || self.ctx.options.type_only_nodes.contains(&spec_idx)
                            } else {
                                false
                            }
                        });
                    if all_type_only {
                        return true;
                    }
                }
                // `export default m` where `m` refers to a type-only entity
                // (e.g., a non-instantiated namespace or interface) — the whole
                // statement should be erased since there is nothing to export.
                if export_data.is_default_export
                    && (inner_node.kind == SyntaxKind::Identifier as u16
                        || inner_node.kind == syntax_kind_ext::QUALIFIED_NAME)
                    && !self.export_default_target_has_runtime_value(export_data.export_clause)
                {
                    return true;
                }
                if inner_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                    && let Some(import_data) = self.arena.get_import_decl(inner_node)
                    && !import_data.is_type_only
                {
                    return false;
                }
                self.is_erased_statement(inner_node)
            }
            syntax_kind_ext::IMPORT_DECLARATION => {
                let Some(import_data) = self.arena.get_import_decl(node) else {
                    return false;
                };
                if self.recovered_module_syntax_block_depth > 0 {
                    return false;
                }
                let Some(clause_node) = self.arena.get(import_data.import_clause) else {
                    return false;
                };
                let Some(clause) = self.arena.get_import_clause(clause_node) else {
                    return false;
                };
                // `import type { ... } from '...'` is always erased
                if clause.is_type_only {
                    return true;
                }
                // `import { type A, type B } from '...'` — all specifiers
                // individually type-only → the whole import is erased.
                if let Some(named_node) = self.arena.get(clause.named_bindings)
                    && named_node.kind == syntax_kind_ext::NAMED_IMPORTS
                    && let Some(named_imports) = self.arena.get_named_imports(named_node)
                    && clause.name.is_none()
                {
                    let all_type_only = !named_imports.elements.nodes.is_empty()
                        && named_imports.elements.nodes.iter().all(|&spec_idx| {
                            if let Some(spec_node) = self.arena.get(spec_idx)
                                && let Some(spec) = self.arena.get_specifier(spec_node)
                            {
                                spec.is_type_only
                                    || self.ctx.options.type_only_nodes.contains(&spec_idx)
                            } else {
                                false
                            }
                        });
                    if all_type_only {
                        return true;
                    }
                }
                // JSX factory imports are only runtime imports when this file
                // actually contains JSX that needs the classic factory.
                if self.is_jsx_factory_import_clause(clause) {
                    return false;
                }
                // With --verbatimModuleSyntax, non-type-only imports are
                // always preserved (no heuristic elision).
                if self.ctx.options.verbatim_module_syntax {
                    return false;
                }
                // JS files (.js/.jsx/.cjs/.mjs) do not undergo import elision;
                // tsc's checker treats all imports in JS files as value imports.
                if self.source_is_js_file {
                    return false;
                }
                // In both CJS and ESM modes, erase imports whose bindings
                // have no value-level usage in the rest of the file.
                // In --noCheck mode this uses a text-based heuristic.
                !self.import_has_value_usage_after_node(node, clause)
            }
            syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                let Some(import_data) = self.arena.get_import_decl(node) else {
                    return false;
                };
                if import_data.is_type_only {
                    return true;
                }
                let is_external =
                    self.arena
                        .get(import_data.module_specifier)
                        .is_some_and(|module_node| {
                            module_node.kind == SyntaxKind::StringLiteral as u16
                                || module_node.kind == syntax_kind_ext::EXTERNAL_MODULE_REFERENCE
                        });
                if self
                    .arena
                    .has_modifier(&import_data.modifiers, SyntaxKind::ExportKeyword)
                {
                    return false;
                }
                if self.recovered_module_syntax_block_depth > 0 {
                    return !is_external;
                }
                // TS1147: import = require() inside a namespace is always invalid;
                // tsc erases these regardless of usage.
                if is_external && self.in_namespace_iife {
                    return true;
                }
                if is_external && self.ctx.options.resolved_node_module_to_esm {
                    return !(self.ctx.options.verbatim_module_syntax
                        || self.source_is_js_file
                        || self.import_equals_has_value_usage_after_node(node, import_data));
                }
                if is_es_module_output && is_external {
                    return true;
                }
                if !is_external
                    && self.in_namespace_iife
                    && !self.import_decl_has_runtime_value(import_data)
                {
                    return true;
                }
                if self.ctx.is_commonjs() && is_external {
                    // With --verbatimModuleSyntax or in JS files, non-type-only
                    // import-equals declarations are always preserved.
                    if self.ctx.options.verbatim_module_syntax || self.source_is_js_file {
                        return false;
                    }
                    // When JSX mode requires a factory (React by default),
                    // don't erase imports matching the factory name — JSX
                    // elements implicitly reference it but the text-based
                    // heuristic won't find it in the source.
                    if matches!(
                        self.ctx.options.jsx,
                        JsxEmit::Preserve | JsxEmit::React | JsxEmit::ReactNative
                    ) {
                        let import_name = self.get_identifier_text_idx(import_data.import_clause);
                        let factory_root = self
                            .ctx
                            .options
                            .jsx_factory
                            .as_deref()
                            .and_then(|f| f.split('.').next())
                            .unwrap_or("React");
                        if import_name == factory_root {
                            return false;
                        }
                    }
                    return !self.import_equals_has_value_usage_after_node(node, import_data);
                }
                // Non-external import-equals (namespace aliases like `import Z = M`)
                // must stay in scripts because they create globals that may be
                // consumed externally. In modules, tsc still erases unused
                // aliases, so keep the usage-based rule there.
                if !is_external
                    && is_es_module_output
                    && !self.ctx.options.verbatim_module_syntax
                    && !self.source_is_js_file
                {
                    if !self.ctx.file_is_module {
                        return false;
                    }
                    return !self.import_equals_has_value_usage_after_node(node, import_data);
                }
                false
            }
            syntax_kind_ext::EXPORT_ASSIGNMENT => {
                // In ES module emit, legacy `export =` is erased — but only at the
                // top level (source-file statements).  When `export =` appears inside
                // a function body or namespace IIFE (syntactically invalid positions),
                // tsc emits it verbatim, so we must not erase it.
                is_es_module_output
                    && self.function_scope_depth == 0
                    && !self.in_namespace_iife
                    && self.recovered_module_syntax_block_depth == 0
                    && self
                        .arena
                        .get_export_assignment(node)
                        .is_some_and(|export_assign| export_assign.is_export_equals)
            }
            syntax_kind_ext::EXPRESSION_STATEMENT => {
                // When the parser fails to recognize `declare` as a modifier prefix
                // (e.g., `declare import a = b;` or `declare declare var x;`), it may
                // produce a bare `declare;` expression statement. We check the source to
                // confirm this was a modifier (followed by a keyword on the same line),
                // not a legitimate `declare` identifier expression.
                if let Some(expr_stmt) = self.arena.get_expression_statement(node)
                    && let Some(expr_node) = self.arena.get(expr_stmt.expression)
                    && let Some(ident) = self.arena.get_identifier(expr_node)
                    && ident.escaped_text == "declare"
                {
                    self.is_declare_modifier_artifact(node)
                } else {
                    false
                }
            }
            _ => false,
        }
    }
}
