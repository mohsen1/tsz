//! Module import/export validation and circular re-export detection.

use crate::query_boundaries::declaration_exports;
use crate::state::CheckerState;
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_parser::parser::NodeIndex;

mod circular_alias;
mod verbatim_module_syntax;

// =============================================================================
// Module and Import Checking Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Export Module Specifier Validation
    // =========================================================================

    /// Returns `true` when an `ExportDeclaration` clause is a `NAMED_EXPORTS`
    /// node with zero specifiers (i.e. `export { } from "..."` or
    /// `export type { } from "..."`).
    ///
    /// Such a declaration binds nothing from the module, so tsc skips
    /// module resolution for it and emits no TS2307. Wildcard re-exports
    /// (`export * from "..."`), namespace re-exports (`export * as ns from
    /// "..."`), and absent export clauses are all distinct AST shapes and
    /// fall through to the normal resolution path.
    fn export_named_clause_is_empty(&self, export_clause_idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let Some(clause_node) = self.ctx.arena.get(export_clause_idx) else {
            return false;
        };
        if clause_node.kind != syntax_kind_ext::NAMED_EXPORTS {
            return false;
        }
        self.ctx
            .arena
            .get_named_imports(clause_node)
            .is_some_and(|named| named.elements.nodes.is_empty())
    }

    /// Check export declaration module specifier for unresolved modules.
    ///
    /// Validates that the module specifier in an export ... from "module" statement
    /// can be resolved. Emits TS2307 if the module cannot be found.
    ///
    /// ## Parameters:
    /// - `stmt_idx`: The export declaration statement node
    ///
    /// ## Validation:
    /// - Checks if module exists in `resolved_modules`, `module_exports`, `shorthand_ambient_modules`, or `declared_modules`
    /// - Emits TS2307 for unresolved module specifiers
    /// - Validates re-exported members exist in source module
    /// - Checks for circular re-export chains
    pub(crate) fn check_export_module_specifier(&mut self, stmt_idx: NodeIndex) {
        use crate::diagnostics::diagnostic_codes;

        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        let Some(export_decl) = self.ctx.arena.get_export_decl(node) else {
            return;
        };

        // Skip module resolution for `export { } from "..."` and
        // `export type { } from "..."` — when the export clause is present
        // (NAMED_EXPORTS) and contains zero specifiers, nothing is actually
        // imported from the module, so tsc does not require the module to
        // exist (and emits no extension diagnostics either). We match that
        // behavior structurally on the AST shape:
        // export_decl + NAMED_EXPORTS clause + empty elements list.
        if self.export_named_clause_is_empty(export_decl.export_clause) {
            return;
        }

        let resolution_mode =
            self.requested_resolution_mode(export_decl.attributes, export_decl.is_type_only);

        // Get module specifier string
        let Some(spec_node) = self.ctx.arena.get(export_decl.module_specifier) else {
            return;
        };

        let Some(literal) = self.ctx.arena.get_literal(spec_node) else {
            return;
        };

        let module_name = &literal.text;

        // TS2846/TS5097: re-export module specifiers with TypeScript
        // extensions follow the same rule as imports — tsc's
        // `resolveExternalModule` anchors the check on
        // `findAncestor(location, isExportDeclaration)`, so `export ... from
        // "./x.ts"` reports TS5097 (and `export ... from "./x.d.ts"` reports
        // TS2846) exactly like the `import ... from` forms. `export type`
        // statements are exempt; specifier-level `{ type x }` modifiers are
        // not. Runs before the unresolved-import reporting gate because the
        // import path emits these in that mode too (the module must resolve
        // for either diagnostic to fire).
        let emitted_extension_diagnostic = self.check_module_specifier_ts_extension(
            module_name,
            spec_node.pos,
            spec_node.end.saturating_sub(spec_node.pos),
            export_decl.is_type_only,
            resolution_mode,
        );

        if !self.ctx.report_unresolved_imports {
            return;
        }
        // Re-exports report TS2307 per declaration site. Clear the per-module
        // dedupe entry up front so each `export ... from "x"` statement gets
        // one chance to report unresolved-module diagnostics, while still
        // allowing later resolution passes in the same statement to see that
        // this site already emitted its TS2307.
        self.ctx
            .modules_with_ts2307_emitted
            .remove(module_name.as_str());

        // Check for circular re-exports
        if self.would_create_cycle(module_name) {
            let cycle_path: Vec<&str> = self
                .ctx
                .import_resolution_stack
                .iter()
                .map(std::string::String::as_str)
                .chain(std::iter::once(module_name.as_str()))
                .collect();
            let cycle_str = cycle_path.join(" -> ");
            let message = format!("Circular re-export detected: {cycle_str}");

            // Check if we've already emitted TS2307 for this module (prevents duplicate emissions)
            let module_key = module_name.to_string();
            if !self.ctx.modules_with_ts2307_emitted.contains(&module_key) {
                self.ctx.modules_with_ts2307_emitted.insert(module_key);
                self.error_at_node(
                    export_decl.module_specifier,
                    &message,
                    diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS,
                );
            }
            return;
        }

        // Track re-export for cycle detection
        self.ctx.import_resolution_stack.push(module_name.clone());

        // Check if the module was resolved by the CLI driver (multi-file mode)
        if let Some(ref resolved) = self.ctx.resolved_modules
            && resolved.contains(module_name)
        {
            self.check_export_target_is_module(
                export_decl.module_specifier,
                module_name,
                resolution_mode,
            );
            // Check for circular re-export chains
            if let Some(source_modules) = self.ctx.binder.wildcard_reexports.get(module_name) {
                let mut visited = FxHashSet::default();
                for (source_module, _is_type_only) in source_modules {
                    self.check_reexport_chain_for_cycles(source_module, &mut visited);
                }
            }
            // Validate named re-exports exist in target module
            self.validate_reexported_members(export_decl, module_name, resolution_mode);
            self.ctx.import_resolution_stack.pop();
            return;
        }

        // Check if the module exists in the module_exports map (cross-file module resolution)
        if self.ctx.binder.module_exports.contains_key(module_name)
            && self
                .ctx
                .get_resolution_error_with_mode(module_name, resolution_mode)
                .is_none()
        {
            self.check_export_target_is_module(
                export_decl.module_specifier,
                module_name,
                resolution_mode,
            );
            // Check for circular re-export chains
            if let Some(source_modules) = self.ctx.binder.wildcard_reexports.get(module_name) {
                let mut visited = FxHashSet::default();
                for (source_module, _is_type_only) in source_modules {
                    self.check_reexport_chain_for_cycles(source_module, &mut visited);
                }
            }
            // Validate named re-exports exist in target module
            self.validate_reexported_members(export_decl, module_name, resolution_mode);
            self.ctx.import_resolution_stack.pop();
            return;
        }

        // Skip TS2307 for ambient module declarations
        if self
            .ctx
            .binder
            .shorthand_ambient_modules
            .contains(module_name)
        {
            self.ctx.import_resolution_stack.pop();
            return;
        }

        if self
            .ctx
            .declared_modules_contains(self.ctx.binder, module_name)
        {
            let wrong_context_allows_module_semantics = self
                .is_in_non_module_element_context(stmt_idx)
                && !self.is_inside_function_body(stmt_idx)
                && !self.is_inside_namespace_declaration(stmt_idx);
            if !self.is_in_non_module_element_context(stmt_idx)
                || wrong_context_allows_module_semantics
            {
                self.validate_reexported_members(export_decl, module_name, resolution_mode);
            }
            self.ctx.import_resolution_stack.pop();
            return;
        }

        // AMD/System/classic-resolution: same suppression rule as imports
        // (issue #3077) — surface the missing-module diagnostic only when
        // TS5107 is silenced via `ignoreDeprecations`.
        if self.deprecated_mode_suppresses_module_not_found() {
            self.ctx.import_resolution_stack.pop();
            return;
        }

        // Emit module-not-found diagnostic for unresolved export specifiers.
        // Unlike imports, tsc reports these per re-export site, so we must not
        // suppress later `export ... from "x"` diagnostics just because an
        // earlier re-export from the same missing module already failed.
        //
        // When TS2846 or TS5097 was already emitted for this re-export,
        // suppress the TS2307 "cannot find module" family — tsc prioritizes
        // extension-specific diagnostics over module-not-found errors. Other
        // resolution errors (e.g. TS6142) still surface, mirroring the
        // import-declaration path.
        if self
            .ctx
            .get_resolution_error_with_mode(module_name, resolution_mode)
            .is_some()
        {
            let (message, code) = self.module_not_found_diagnostic(module_name);
            if emitted_extension_diagnostic
                && (code == diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS
                    || code == diagnostic_codes::CANNOT_FIND_MODULE_DID_YOU_MEAN_TO_SET_THE_MODULERESOLUTION_OPTION_TO_NODENEXT_O)
            {
                self.ctx.import_resolution_stack.pop();
                return;
            }
            self.ctx
                .modules_with_ts2307_emitted
                .insert(module_name.to_string());
            self.error_at_node(export_decl.module_specifier, &message, code);
            self.ctx.import_resolution_stack.pop();
            return;
        }

        // The trailing fallback below reports module-not-found for specifiers
        // with no recorded resolution error; skip it when an extension
        // diagnostic already covered this site.
        if emitted_extension_diagnostic {
            self.ctx.import_resolution_stack.pop();
            return;
        }

        let (message, code) = self.module_not_found_diagnostic(module_name);
        self.ctx
            .modules_with_ts2307_emitted
            .insert(module_name.to_string());
        self.error_at_node(export_decl.module_specifier, &message, code);

        self.ctx.import_resolution_stack.pop();
    }

    /// TS2498: Module '{0}' uses 'export =' and cannot be used with 'export *'.
    ///
    /// When a module uses `export = <expr>` (CommonJS-style), wildcard re-exports
    /// (`export * from './module'` or `export * as ns from './module'`) are invalid
    /// because ES module namespace objects cannot be constructed from a CommonJS
    /// single-value export.
    pub(crate) fn check_export_star_of_export_equals_module(&mut self, stmt_idx: NodeIndex) {
        use crate::diagnostics::diagnostic_codes;
        use tsz_parser::parser::syntax_kind_ext;

        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };
        let Some(export_decl) = self.ctx.arena.get_export_decl(node) else {
            return;
        };

        // Only applies to re-exports with a module specifier (... from 'module')
        if export_decl.module_specifier.is_none() {
            return;
        }

        // Only applies to wildcard re-exports:
        //   export * from './module'           → export_clause is NONE
        //   export * as ns from './module'     → export_clause is Identifier/StringLiteral
        // Named exports (export { foo } from) use NAMED_EXPORTS and are not affected.
        if export_decl.export_clause.is_some()
            && let Some(clause_node) = self.ctx.arena.get(export_decl.export_clause)
            && clause_node.kind == syntax_kind_ext::NAMED_EXPORTS
        {
            return;
        }

        // Get module specifier text
        let Some(spec_node) = self.ctx.arena.get(export_decl.module_specifier) else {
            return;
        };
        let Some(literal) = self.ctx.arena.get_literal(spec_node) else {
            return;
        };
        let module_name = literal.text.clone();

        // Check if the target module uses `export =`.
        // First check module_exports (covers identifier-based export assignments like
        // `export = React`). Fall back to checking the target file's AST for
        // EXPORT_ASSIGNMENT nodes (covers non-identifier forms like `export = {}`).
        let has_export_equals = self
            .resolve_effective_module_exports(&module_name)
            .is_some_and(|exports| exports.has("export="))
            || self.target_file_has_export_assignment(&module_name);

        if has_export_equals {
            // TSC uses the resolved module name (without relative prefix or extension)
            // e.g., './a' becomes 'a', '../utils/foo' becomes 'foo'
            let display_name = module_name
                .strip_prefix("./")
                .or_else(|| module_name.strip_prefix("../"))
                .unwrap_or(&module_name);
            // Strip file extension if present
            let display_name = display_name
                .strip_suffix(".ts")
                .or_else(|| display_name.strip_suffix(".js"))
                .or_else(|| display_name.strip_suffix(".tsx"))
                .or_else(|| display_name.strip_suffix(".jsx"))
                .unwrap_or(display_name);
            let quoted = format!("\"{display_name}\"");
            self.error_at_node_msg(
                export_decl.module_specifier,
                diagnostic_codes::MODULE_USES_EXPORT_AND_CANNOT_BE_USED_WITH_EXPORT,
                &[&quoted],
            );
        }
    }

    /// Check if the target module file has an `export =` assignment in its AST.
    ///
    /// This covers non-identifier export assignments (e.g., `export = {}`) where
    /// the binder doesn't create an `"export="` entry in `module_exports`.
    fn target_file_has_export_assignment(&self, module_name: &str) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let Some(target_idx) = self.ctx.resolve_import_target(module_name) else {
            return false;
        };
        let target_arena = self.ctx.get_arena_for_file(target_idx as u32);
        let Some(source_file) = target_arena.source_files.first() else {
            return false;
        };
        // Scan top-level statements for EXPORT_ASSIGNMENT
        for &stmt_idx in &source_file.statements.nodes {
            if let Some(stmt_node) = target_arena.get(stmt_idx)
                && stmt_node.kind == syntax_kind_ext::EXPORT_ASSIGNMENT
            {
                // Verify it's `export =` (not `export default`)
                if let Some(assign) = target_arena.get_export_assignment(stmt_node)
                    && assign.is_export_equals
                {
                    return true;
                }
            }
        }
        false
    }

    pub(crate) fn check_export_target_is_module(
        &mut self,
        module_specifier_idx: NodeIndex,
        module_name: &str,
        resolution_mode: Option<crate::context::ResolutionModeOverride>,
    ) {
        use crate::diagnostics::diagnostic_codes;

        let target_idx = if let Some(mode) = resolution_mode {
            self.ctx.resolve_import_target_from_file_with_mode(
                self.ctx.current_file_idx,
                module_name,
                Some(mode),
            )
        } else {
            self.ctx.resolve_import_target(module_name)
        };
        let Some(target_idx) = target_idx else {
            return;
        };
        let Some(target_binder) = self.ctx.get_binder_for_file(target_idx) else {
            return;
        };
        if target_binder.is_external_module
            || self.is_ambient_module_match(module_name)
            || target_binder
                .declared_modules
                .contains(module_name.trim_matches('"').trim_matches('\''))
        {
            return;
        }
        let arena = self.ctx.get_arena_for_file(target_idx as u32);
        let Some(source_file) = arena.source_files.first() else {
            return;
        };
        let file_name = source_file.file_name.as_str();
        let is_js_like = file_name.ends_with(".js")
            || file_name.ends_with(".jsx")
            || file_name.ends_with(".mjs")
            || file_name.ends_with(".cjs");
        let is_json_module =
            file_name.ends_with(".json") && self.ctx.compiler_options.resolve_json_module;
        if is_js_like || is_json_module {
            return;
        }
        let source_file_name = source_file.file_name.clone();
        self.error_at_node_msg(
            module_specifier_idx,
            diagnostic_codes::FILE_IS_NOT_A_MODULE,
            &[&source_file_name],
        );
    }

    /// Check whether a target module uses `export =`, by examining both the
    /// binder's export tables and the target file's AST for any export assignment
    /// with `is_export_equals: true`. Also detects the JS-equivalent
    /// `module.exports = ...` top-level assignment.
    ///
    /// This is more comprehensive than `module_has_export_equals` which only
    /// detects `export = <identifier>` patterns. This also handles
    /// `export = {}`, `export = expr()`, etc.
    pub(crate) fn target_module_has_export_equals(&self, module_specifier: &str) -> bool {
        // Fast path: check binder tables (works for identifier-based export =)
        if self.module_has_export_equals(module_specifier) {
            return true;
        }

        // Slow path: resolve the target file and scan its AST for export = statements
        let Some(target_idx) = self.ctx.resolve_import_target(module_specifier) else {
            return false;
        };
        let target_arena = self.ctx.get_arena_for_file(target_idx as u32);
        let Some(sf) = target_arena.source_files.first() else {
            return false;
        };
        for &stmt_idx in &sf.statements.nodes {
            let Some(stmt_node) = target_arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind == tsz_parser::parser::syntax_kind_ext::EXPORT_ASSIGNMENT
                && let Some(assign) = target_arena.get_export_assignment(stmt_node)
                && assign.is_export_equals
            {
                return true;
            }
            // Detect JS-style `module.exports = <expr>` top-level assignment.
            // tsc treats this as `export =` for namespace-display purposes
            // (TS2694 message uses the `.export=` qualifier).
            if stmt_node.kind == tsz_parser::parser::syntax_kind_ext::EXPRESSION_STATEMENT
                && let Some(expr_stmt) = target_arena.get_expression_statement(stmt_node)
                && let Some(expr_node) = target_arena.get(expr_stmt.expression)
                && expr_node.kind == tsz_parser::parser::syntax_kind_ext::BINARY_EXPRESSION
                && let Some(binary) = target_arena.get_binary_expr(expr_node)
                && binary.operator_token == tsz_scanner::SyntaxKind::EqualsToken as u16
                && Self::is_module_dot_exports_target(target_arena, binary.left)
            {
                return true;
            }
        }
        false
    }

    /// Detect a `module.exports` property-access expression. Used to recognise
    /// JS-style export-assignment patterns when scanning a target file's AST.
    fn is_module_dot_exports_target(
        arena: &tsz_parser::NodeArena,
        idx: tsz_parser::parser::NodeIndex,
    ) -> bool {
        let Some(node) = arena.get(idx) else {
            return false;
        };
        if node.kind != tsz_parser::parser::syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(access) = arena.get_access_expr(node) else {
            return false;
        };
        let Some(expr_node) = arena.get(access.expression) else {
            return false;
        };
        let Some(expr_id) = arena.get_identifier(expr_node) else {
            return false;
        };
        if expr_id.escaped_text != "module" {
            return false;
        }
        let Some(name_node) = arena.get(access.name_or_argument) else {
            return false;
        };
        let Some(name_id) = arena.get_identifier(name_node) else {
            return false;
        };
        name_id.escaped_text == "exports"
    }

    /// The declared name of an `export = <target>` (or JS
    /// `module.exports = <target>`) module's target, when that target is a
    /// named declaration — the name tsc renders in a TS2694 namespace path for
    /// such a module (`export = shape` → `shape`, with no module path and no
    /// `.export=` qualifier). Covers a namespace/class/function/const target
    /// and an aliased target (`const t = ...; export = t` → `t`).
    ///
    /// Returns `None` when the export target is anonymous (e.g.
    /// `module.exports = { ... }`), where tsc keeps the synthesized
    /// `"mod".export=` form. The named/anonymous decision turns on whether the
    /// export target is a plain identifier (a named symbol reference) versus an
    /// anonymous initializer — not on any rendered text or file-name predicate.
    pub(crate) fn export_equals_target_named_display(
        &self,
        module_specifier: &str,
    ) -> Option<String> {
        use tsz_parser::parser::syntax_kind_ext;
        let target_idx = self.ctx.resolve_import_target(module_specifier)?;
        let target_arena = self.ctx.get_arena_for_file(target_idx as u32);
        let sf = target_arena.source_files.first()?;
        for &stmt_idx in &sf.statements.nodes {
            let Some(stmt_node) = target_arena.get(stmt_idx) else {
                continue;
            };
            // The export target's expression: the RHS of `export = <expr>` or
            // of a top-level JS `module.exports = <expr>`.
            let target_expr = if stmt_node.kind == syntax_kind_ext::EXPORT_ASSIGNMENT {
                match target_arena.get_export_assignment(stmt_node) {
                    Some(assign) if assign.is_export_equals => assign.expression,
                    _ => continue,
                }
            } else if stmt_node.kind == syntax_kind_ext::EXPRESSION_STATEMENT {
                let Some(expr_stmt) = target_arena.get_expression_statement(stmt_node) else {
                    continue;
                };
                let Some(expr_node) = target_arena.get(expr_stmt.expression) else {
                    continue;
                };
                if expr_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
                    continue;
                }
                let Some(binary) = target_arena.get_binary_expr(expr_node) else {
                    continue;
                };
                if binary.operator_token != tsz_scanner::SyntaxKind::EqualsToken as u16
                    || !Self::is_module_dot_exports_target(target_arena, binary.left)
                {
                    continue;
                }
                binary.right
            } else {
                continue;
            };

            // A named target is a plain identifier (`export = shape`); anything
            // else — an object literal, a call, a qualified name — is anonymous
            // and keeps the `"mod".export=` form. When the target module's
            // binder is resident its symbol `escaped_name` is authoritative;
            // otherwise the source identifier (the declared name) is used.
            let node = target_arena.get(target_expr)?;
            let ident_name = target_arena.get_identifier(node)?.escaped_text.as_str();
            let name = self
                .ctx
                .get_binder_for_file(target_idx)
                .and_then(|binder| {
                    binder
                        .file_locals
                        .get(ident_name)
                        .and_then(|sym_id| binder.get_symbol(sym_id))
                        .map(|symbol| symbol.escaped_name.clone())
                })
                .unwrap_or_else(|| ident_name.to_string());
            return Some(name);
        }
        None
    }

    /// Validate that named re-exports exist in the target module.
    ///
    /// For `export { foo, bar as baz } from './module'`, validates that
    /// `foo` and `bar` are actually exported by './module'.
    ///
    /// ## Emits TS2305 when:
    /// - A named re-export doesn't exist in the target module
    fn validate_reexported_members(
        &mut self,
        export_decl: &tsz_parser::parser::node::ExportDeclData,
        module_name: &str,
        resolution_mode: Option<crate::context::ResolutionModeOverride>,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use tsz_parser::parser::syntax_kind_ext;

        // Only validate named exports (not wildcard exports or declarations)
        if export_decl.export_clause.is_none() {
            return;
        }

        let Some(clause_node) = self.ctx.arena.get(export_decl.export_clause) else {
            return;
        };

        // Only check NAMED_EXPORTS (export { x, y } from 'module')
        if clause_node.kind != syntax_kind_ext::NAMED_EXPORTS {
            return;
        }

        let Some(named_exports) = self.ctx.arena.get_named_imports(clause_node) else {
            return;
        };

        // Get the module's canonical export surface.
        let module_exports = self
            .resolve_effective_module_exports_with_mode(module_name, resolution_mode)
            .or_else(|| {
                (self
                    .ctx
                    .declared_modules_contains(self.ctx.binder, module_name)
                    && !self
                        .ctx
                        .binder
                        .shorthand_ambient_modules
                        .contains(module_name))
                .then(tsz_binder::SymbolTable::new)
            });
        // TSC includes source-level quotes in module diagnostic messages
        let quoted_module = format!("\"{module_name}\"");
        let has_json_default_export =
            self.module_has_json_default_export(module_name, Some(self.ctx.current_file_idx));

        let Some(module_exports) = module_exports else {
            return; // Module exports not found - TS2307 already emitted
        };

        // Check each export specifier
        for &specifier_idx in &named_exports.elements.nodes {
            let Some(spec_node) = self.ctx.arena.get(specifier_idx) else {
                continue;
            };

            let Some(specifier) = self.ctx.arena.get_specifier(spec_node) else {
                continue;
            };

            // Skip type-only re-exports since they might reference types that
            // don't appear in the exports table
            if specifier.is_type_only {
                continue;
            }

            // Get the property name (what we're exporting from the source module)
            // For `export { bar as baz }`, property_name is "bar"
            // For `export { foo }`, we use the name "foo"
            let export_name = if specifier.property_name.is_some() {
                if let Some(text) = self.get_identifier_text_from_idx(specifier.property_name) {
                    text
                } else {
                    continue;
                }
            } else if specifier.name.is_some() {
                if let Some(text) = self.get_identifier_text_from_idx(specifier.name) {
                    text
                } else {
                    continue;
                }
            } else {
                continue;
            };

            if export_name == "default"
                && module_exports.has("export=")
                && self.ctx.allow_synthetic_default_imports()
            {
                continue;
            }
            if export_name == "default" && has_json_default_export {
                continue;
            }
            // A `.d.ts` module may have a runtime `default` its declarations
            // never spell out, so `tsc` synthesizes one rather than reporting
            // TS2305. An `__esModule` export withdraws that: the file is
            // claiming to describe a faithful ES module, so a missing
            // `default` really is missing.
            if export_name == "default"
                && !module_exports.has("__esModule")
                && self.module_declarations_can_synthesize_default(module_name)
            {
                continue;
            }

            // Check if this name is exported from the source module
            if export_name != "*" && !module_exports.has(&export_name) {
                let has_default_like_export = has_json_default_export
                    || module_exports.has("default")
                    || module_exports.has("export=")
                    || module_exports.has("module.exports");
                if module_exports.has("module.exports") && has_default_like_export {
                    let message = format_message(
                        diagnostic_messages::MODULE_HAS_NO_EXPORTED_MEMBER_DID_YOU_MEAN_TO_USE_IMPORT_FROM_INSTEAD,
                        &[&quoted_module, &export_name],
                    );
                    self.error_at_node(
                        specifier_idx,
                        &message,
                        diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER_DID_YOU_MEAN_TO_USE_IMPORT_FROM_INSTEAD,
                    );
                    continue;
                }

                // Check for spelling suggestions (TS2724) before TS2305 and TS2614.
                let export_names: Vec<&str> = module_exports
                    .iter()
                    .map(|(name, _)| name.as_str())
                    .collect();
                if let Some(suggestion) = tsz_parser::parser::spelling::get_spelling_suggestion(
                    &export_name,
                    &export_names,
                ) {
                    // TS2724: did you mean?
                    let message = format_message(
                        diagnostic_messages::HAS_NO_EXPORTED_MEMBER_NAMED_DID_YOU_MEAN,
                        &[&quoted_module, &export_name, suggestion],
                    );
                    self.error_at_node(
                        specifier_idx,
                        &message,
                        diagnostic_codes::HAS_NO_EXPORTED_MEMBER_NAMED_DID_YOU_MEAN,
                    );
                } else if has_default_like_export {
                    // TS2614: Symbol doesn't exist but a default export does
                    let message = format_message(
                        diagnostic_messages::MODULE_HAS_NO_EXPORTED_MEMBER_DID_YOU_MEAN_TO_USE_IMPORT_FROM_INSTEAD,
                        &[&quoted_module, &export_name],
                    );
                    self.error_at_node(
                        specifier_idx,
                        &message,
                        diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER_DID_YOU_MEAN_TO_USE_IMPORT_FROM_INSTEAD,
                    );
                } else {
                    // TS2305: Module has no exported member
                    let message = format_message(
                        diagnostic_messages::MODULE_HAS_NO_EXPORTED_MEMBER,
                        &[&quoted_module, &export_name],
                    );
                    self.error_at_node(
                        specifier_idx,
                        &message,
                        diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER,
                    );
                }
            }

            // Mark specifiers that re-export type-only symbols for emit elision.
            // When `export { A } from "mod"` and A is type-only in mod (interface,
            // type alias, uninstantiated namespace, const enum without preserveConstEnums),
            // the emitter must skip it — mark the specifier NodeIndex so the emitter
            // can filter it in `collect_export_names_with_options` and the re-export path.
            if self.import_binding_is_type_only(module_name, &export_name) {
                self.ctx.type_only_nodes.insert(specifier_idx);
            }
        }
    }

    // =========================================================================
    // Dynamic Import Return Type
    // =========================================================================

    /// Get the return type for a dynamic `import()` call.
    ///
    /// Returns Promise<ModuleType> where `ModuleType` is an object containing
    /// all the module's exports. Falls back to Promise<any> or just `any` when:
    /// - The module cannot be resolved
    /// - Promise is not available (ES5 target without lib)
    ///
    /// This method implements Phase 1.3 of the module resolution plan.
    pub(crate) fn get_dynamic_import_type(
        &mut self,
        call: &tsz_parser::parser::node::CallExprData,
    ) -> tsz_solver::TypeId {
        // Get the first argument (module specifier)
        let args = match call.arguments.as_ref() {
            Some(a) => a.nodes.as_slice(),
            None => &[],
        };

        if args.is_empty() {
            return self.create_promise_any();
        }

        let arg_idx = args[0];
        let Some(arg_node) = self.ctx.arena.get(arg_idx) else {
            return self.create_promise_any();
        };

        // Only handle string literal module specifiers
        let Some(literal) = self.ctx.arena.get_literal(arg_node) else {
            return self.create_promise_any();
        };

        let module_name = &literal.text;

        // Check for shorthand ambient modules - imports are typed as `any`
        if self
            .ctx
            .binder
            .shorthand_ambient_modules
            .contains(module_name)
        {
            return self.create_promise_any();
        }

        // Try to get module exports for the namespace type.
        let exports_table = self.resolve_effective_module_exports(module_name);

        if let Some(exports_table) = exports_table {
            // Get export= type if this is a CommonJS module.
            // Also check for `export { X as "module.exports" }` which acts like export=.
            let export_equals_type = exports_table
                .get("export=")
                .or_else(|| exports_table.get("module.exports"))
                .map(|export_equals_sym| self.get_type_of_symbol(export_equals_sym));
            let ordered_exports = self.ordered_namespace_export_entries(&exports_table);

            // Create an object type with all module exports
            let mut props: Vec<tsz_solver::PropertyInfo> = Vec::new();
            for &(name, export_sym_id) in &ordered_exports {
                if name == "export=" {
                    continue;
                }
                let prop_type = self.get_type_of_symbol(export_sym_id);
                let declaration_order = if name == "default" {
                    1
                } else {
                    props.len() as u32 + 2
                };
                let name_atom = self.ctx.types.intern_string(name);
                props.push(declaration_exports::declaration_export_property(
                    name_atom,
                    prop_type,
                    declaration_order,
                ));
            }

            // Merge module augmentations
            // Module augmentations add interfaces/types to existing modules
            // e.g., declare module 'express' { interface Request { user?: User; } }
            if let Some(augmentations) = self.ctx.binder.module_augmentations.get(module_name) {
                for aug in augmentations {
                    // Resolve the augmentation declaration's type against its own
                    // arena/binder (#14853). A cross-file augmentation that adds a
                    // new export otherwise collapsed to `any` here, dropping every
                    // assignability error against the dynamically-imported member.
                    let aug_arena = aug.arena.as_deref().unwrap_or(self.ctx.arena);
                    let aug_type = self
                        .augmentation_export_declaration_type(aug.node, aug_arena)
                        .unwrap_or(tsz_solver::TypeId::ANY);
                    let name_atom = self.ctx.types.intern_string(&aug.name);

                    // Check if this augments an existing export
                    if let Some(existing) = props.iter_mut().find(|p| p.name == name_atom) {
                        // Merge types - for interfaces, this creates an intersection
                        existing.type_id = declaration_exports::module_export_augmented_type(
                            self.ctx.types,
                            existing.type_id,
                            aug_type,
                        );
                        existing.write_type = existing.type_id;
                    } else {
                        // New export from augmentation
                        props.push(declaration_exports::declaration_export_property(
                            name_atom, aug_type, 0,
                        ));
                    }
                }
            }

            // When esModuleInterop / allowSyntheticDefaultImports is enabled
            // and the module uses `export =`, synthesize a `default` property
            // so that `import("./foo").then(f => f.default)` works.
            if let Some(eq_type) = export_equals_type
                && self.ctx.allow_synthetic_default_imports()
            {
                let default_atom = self.ctx.types.intern_string("default");
                if !props.iter().any(|p| p.name == default_atom) {
                    props.push(declaration_exports::declaration_export_property(
                        default_atom,
                        eq_type,
                        1,
                    ));
                }
            }

            Self::normalize_namespace_export_declaration_order(&mut props);
            let module_type =
                declaration_exports::dynamic_import_module_object_type(self.ctx.types, props);
            let display_module_name =
                self.resolve_namespace_display_module_name(&exports_table, module_name);
            self.ctx
                .namespace_module_names
                .insert(module_type, display_module_name);
            return self.create_promise_of(module_type);
        }

        // Module not found - return Promise<any>
        self.create_promise_any()
    }

    /// Create a Promise<any> type.
    fn create_promise_any(&mut self) -> tsz_solver::TypeId {
        self.create_promise_of(tsz_solver::TypeId::ANY)
    }

    /// Create a Promise<T> type for dynamic import expressions.
    ///
    /// Uses the same type resolution path as `var p: Promise<T>` to ensure
    /// structural compatibility. Falls back to `PROMISE_BASE` without lib files.
    fn create_promise_of(&mut self, inner_type: tsz_solver::TypeId) -> tsz_solver::TypeId {
        use tsz_solver::TypeId;

        // Resolve Promise as Lazy(DefId), the same form that type annotations use.
        // `var p: Promise<T>` goes through create_lazy_type_ref → Application(Lazy(DefId), [T]).
        // We must do the same here so that `import()` returns a structurally compatible type.
        if let Some(sym_id) = self.ctx.lib_promise_sym_id() {
            let _ = self.get_type_of_symbol(sym_id);
            // Ensure the Promise DefId has its type parameters and body registered
            // so that resolve_application_property can substitute T with the inner type.
            // Without this, .then() callback parameters remain as unsubstituted `T`.
            self.ensure_def_ready_for_lowering(sym_id, "Promise");
            let promise_base = self
                .ctx
                .lib_promise_type_ref()
                .unwrap_or_else(|| self.ctx.create_lazy_type_ref(sym_id));
            return declaration_exports::dynamic_import_promise_type(
                self.ctx.types,
                promise_base,
                inner_type,
            );
        }

        // Fallback: use synthetic PROMISE_BASE (works without lib files)
        declaration_exports::dynamic_import_promise_type(
            self.ctx.types,
            TypeId::PROMISE_BASE,
            inner_type,
        )
    }

    /// Check `export { x };` (local named exports)
    /// Emits TS2661 if exporting a non-local declaration.
    /// TS2207: The 'type' modifier cannot be used on a named export when 'export type' is
    /// used on its export statement. E.g., `export type { type X as Y }` is invalid because
    /// the specifier-level `type` modifier conflicts with the statement-level `export type`.
    pub(crate) fn check_type_modifier_on_type_only_export(
        &mut self,
        named_exports_idx: tsz_parser::parser::NodeIndex,
    ) {
        use crate::diagnostics::diagnostic_codes;

        let Some(clause_node) = self.ctx.arena.get(named_exports_idx) else {
            return;
        };
        let Some(named_exports) = self.ctx.arena.get_named_imports(clause_node) else {
            return;
        };

        for &specifier_idx in &named_exports.elements.nodes {
            let Some(spec_node) = self.ctx.arena.get(specifier_idx) else {
                continue;
            };
            let Some(specifier) = self.ctx.arena.get_specifier(spec_node) else {
                continue;
            };
            if specifier.is_type_only {
                self.error_at_node(
                    specifier_idx,
                    "The 'type' modifier cannot be used on a named export when 'export type' is used on its export statement.",
                    diagnostic_codes::THE_TYPE_MODIFIER_CANNOT_BE_USED_ON_A_NAMED_EXPORT_WHEN_EXPORT_TYPE_IS_USED_ON_I,
                );
            }
        }
    }

    pub(crate) fn check_local_named_exports(
        &mut self,
        named_exports_idx: tsz_parser::parser::NodeIndex,
    ) {
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        let Some(clause_node) = self.ctx.arena.get(named_exports_idx) else {
            return;
        };
        if clause_node.kind != syntax_kind_ext::NAMED_EXPORTS {
            return;
        }

        // Skip local-export checks when the export is in a wrong context (inside block/function).
        // The grammar error (TS1233) is the primary error; TS2661/TS2304 shouldn't also fire.
        if self.is_in_non_module_element_context(named_exports_idx) {
            return;
        }

        let Some(named_exports) = self.ctx.arena.get_named_imports(clause_node) else {
            return;
        };

        // Check if the export clause is inside an ambient module declaration
        // (e.g., `declare module "m" { export { X }; }`). Inside such blocks,
        // only declarations within the module scope are local — file-level
        // declarations from the outer scope are NOT local to the module.
        let inside_ambient_module =
            self.is_inside_string_literal_module_declaration(named_exports_idx);

        // Detect `export type { … }` on the enclosing statement. tsc still
        // reports TS2661/TS2304 for unresolved names inside a type-only
        // export, but skips TS18043 ("types cannot appear in export
        // declarations in JavaScript files") since the user already marked
        // the clause type-only.
        let enclosing_export_is_type_only = self
            .ctx
            .arena
            .get_extended(named_exports_idx)
            .and_then(|ext| self.ctx.arena.get(ext.parent))
            .and_then(|parent_node| self.ctx.arena.get_export_decl(parent_node))
            .is_some_and(|decl| decl.is_type_only);

        let mut seen_export_names: FxHashMap<String, NodeIndex> = FxHashMap::default();

        for &specifier_idx in &named_exports.elements.nodes {
            let Some(spec_node) = self.ctx.arena.get(specifier_idx) else {
                continue;
            };
            let Some(specifier) = self.ctx.arena.get_specifier(spec_node) else {
                continue;
            };

            let name_idx = if specifier.property_name.is_some() {
                specifier.property_name
            } else {
                specifier.name
            };
            if name_idx.is_none() {
                continue;
            }

            let source_name_is_string_literal = self
                .ctx
                .arena
                .get(name_idx)
                .is_some_and(|name_node| name_node.kind == SyntaxKind::StringLiteral as u16);

            // Check for duplicate exported names in the same export clause
            let export_name_str = self
                .get_identifier_text_from_idx(specifier.name)
                .unwrap_or_else(|| String::from("unknown"));
            match seen_export_names.entry(export_name_str.clone()) {
                std::collections::hash_map::Entry::Occupied(mut entry) => {
                    use tsz_common::diagnostics::{
                        diagnostic_codes, diagnostic_messages, format_message,
                    };
                    let msg = format_message(
                        diagnostic_messages::DUPLICATE_IDENTIFIER,
                        &[&export_name_str],
                    );
                    let code = diagnostic_codes::DUPLICATE_IDENTIFIER;
                    let first_idx = *entry.get();
                    if first_idx != NodeIndex::NONE {
                        self.error_at_node(first_idx, &msg, code);
                        *entry.get_mut() = NodeIndex::NONE;
                    }
                    self.error_at_node(specifier.name, &msg, code);
                }
                std::collections::hash_map::Entry::Vacant(entry) => {
                    entry.insert(specifier.name);
                }
            }

            // `export { "x" as y }` / `export type { "x" as y }` are valid with
            // arbitrary module namespace identifiers. The local-name check only
            // applies to identifier bindings, so skip TS2661/TS2304 probing here.
            if source_name_is_string_literal {
                continue;
            }

            let name_str = self
                .get_identifier_text_from_idx(name_idx)
                .unwrap_or_else(|| String::from("unknown"));
            let has_local_jsdoc_typedef = !inside_ambient_module
                && self.is_js_file()
                && self.ctx.should_resolve_jsdoc()
                && self.file_has_jsdoc_typedef_named(self.ctx.current_file_idx, &name_str);

            // Check if the symbol is a local declaration or import.
            // file_locals includes merged globals from other files and cloned standard-lib
            // globals, so verify the declaration belongs to this file or is an unstamped
            // user-file symbol rather than a standard-lib symbol.
            // Inside ambient module declarations, file-level symbols are not local to the
            // module and should emit TS2661.
            let current_file_idx = self.ctx.current_file_idx as u32;
            let is_local = if inside_ambient_module {
                // Inside `declare module "m"`, only symbols declared within
                // the module's own scope count as local. Check the binder's
                // scope chain: walk from the specifier's scope up to the first
                // Module scope and check its symbol table.
                self.is_name_in_enclosing_module_scope(&name_str, specifier_idx)
            } else if has_local_jsdoc_typedef {
                true
            } else {
                self.ctx
                    .binder
                    .file_locals
                    .get(&name_str)
                    .and_then(|sym_id| self.ctx.binder.get_symbol(sym_id).map(|sym| (sym_id, sym)))
                    .is_some_and(|(sym_id, sym)| {
                        sym.decl_file_idx == current_file_idx
                            || (sym.decl_file_idx == u32::MAX
                                && !self.ctx.symbol_is_from_actual_or_cloned_lib(sym_id))
                    })
            };

            if is_local
                && self.is_js_file()
                && self.ctx.should_resolve_jsdoc()
                && !enclosing_export_is_type_only
                && (self.is_local_symbol_type_only(&name_str) || has_local_jsdoc_typedef)
            {
                self.error_at_node(
                    name_idx,
                    crate::diagnostics::diagnostic_messages::TYPES_CANNOT_APPEAR_IN_EXPORT_DECLARATIONS_IN_JAVASCRIPT_FILES,
                    crate::diagnostics::diagnostic_codes::TYPES_CANNOT_APPEAR_IN_EXPORT_DECLARATIONS_IN_JAVASCRIPT_FILES,
                );
                continue;
            }

            // Rule: when `export { X }` re-exports a local binding whose
            // source has no runtime form — type-only import, type-only
            // declaration, or alias to a type-only / const-enum / `export =`
            // module export — tsc elides the specifier from JS emit; mark
            // the specifier so the emitter matches. The alias branch follows
            // the import through the source module, which is what catches
            // the const-enum / `export =` shapes the plain specifier-side
            // query misses.
            if is_local && !enclosing_export_is_type_only && !specifier.is_type_only {
                use tsz_binder::symbol_flags;
                let sym = self
                    .ctx
                    .binder
                    .file_locals
                    .get(&name_str)
                    .and_then(|sym_id| self.ctx.binder.get_symbol(sym_id));
                let is_type_only = if let Some(sym) = sym
                    && sym.is_type_only
                {
                    true
                } else if let Some(sym) = sym
                    && sym.has_any_flags(symbol_flags::ALIAS)
                    && let Some(module_spec) = sym.import_module()
                {
                    let import_name = sym.import_name().unwrap_or(&name_str);
                    self.import_binding_is_type_only(module_spec, import_name)
                } else {
                    self.is_local_symbol_type_only(&name_str)
                };
                if is_type_only {
                    self.ctx.type_only_nodes.insert(specifier_idx);
                }
            }

            if !is_local {
                // Symbol is not local to the current module/file.
                // Distinguish between accessible-but-not-local (TS2661) and truly missing (TS2304).
                let is_resolvable = self.resolve_identifier_symbol(name_idx).is_some()
                    || matches!(
                        name_str.as_str(),
                        "undefined"
                            | "any"
                            | "unknown"
                            | "never"
                            | "string"
                            | "number"
                            | "boolean"
                            | "symbol"
                            | "object"
                            | "bigint"
                            | "globalThis"
                    );

                if is_resolvable {
                    self.error_at_node_msg(
                        name_idx,
                        crate::diagnostics::diagnostic_codes::CANNOT_EXPORT_ONLY_LOCAL_DECLARATIONS_CAN_BE_EXPORTED_FROM_A_MODULE,
                        &[&name_str],
                    );
                } else {
                    // Route through boundary for TS2304/TS2552 with suggestion collection
                    self.report_not_found_at_boundary(
                        &name_str,
                        name_idx,
                        crate::query_boundaries::name_resolution::NameLookupKind::Value,
                    );
                }
            }
        }
    }

    /// TS18043 for re-exports in JavaScript files:
    /// `export { T } from "mod"` where `T` resolves to a type-only export.
    pub(crate) fn check_js_type_only_reexports(
        &mut self,
        named_exports_idx: NodeIndex,
        module_specifier_idx: NodeIndex,
    ) {
        use tsz_scanner::SyntaxKind;

        if !self.is_js_file() || !self.ctx.should_resolve_jsdoc() || !module_specifier_idx.is_some()
        {
            return;
        }

        let module_specifier = self
            .ctx
            .arena
            .get(module_specifier_idx)
            .and_then(|n| self.ctx.arena.get_literal(n))
            .map(|l| l.text.clone());
        let Some(module_specifier) = module_specifier else {
            return;
        };

        let Some(clause_node) = self.ctx.arena.get(named_exports_idx) else {
            return;
        };
        let Some(named_exports) = self.ctx.arena.get_named_imports(clause_node) else {
            return;
        };

        for &specifier_idx in &named_exports.elements.nodes {
            let Some(spec_node) = self.ctx.arena.get(specifier_idx) else {
                continue;
            };
            let Some(specifier) = self.ctx.arena.get_specifier(spec_node) else {
                continue;
            };
            if specifier.is_type_only {
                continue;
            }

            let source_name_idx = if specifier.property_name.is_some() {
                specifier.property_name
            } else {
                specifier.name
            };
            if !source_name_idx.is_some() {
                continue;
            }

            let source_name_is_string_literal = self
                .ctx
                .arena
                .get(source_name_idx)
                .is_some_and(|name_node| name_node.kind == SyntaxKind::StringLiteral as u16);
            if source_name_is_string_literal {
                continue;
            }

            let Some(source_name) = self.get_identifier_text_from_idx(source_name_idx) else {
                continue;
            };

            if self.is_import_specifier_type_only(&module_specifier, &source_name)
                || self.is_export_type_only_across_binders(&module_specifier, &source_name)
            {
                self.error_at_node(
                    source_name_idx,
                    crate::diagnostics::diagnostic_messages::TYPES_CANNOT_APPEAR_IN_EXPORT_DECLARATIONS_IN_JAVASCRIPT_FILES,
                    crate::diagnostics::diagnostic_codes::TYPES_CANNOT_APPEAR_IN_EXPORT_DECLARATIONS_IN_JAVASCRIPT_FILES,
                );
            }
        }
    }
    /// Check if a node is inside an ambient module declaration with a string-literal name
    /// (e.g., `declare module "m" { ... }`). Returns false for namespace declarations
    /// (e.g., `declare namespace Foo { ... }`).
    fn is_inside_string_literal_module_declaration(
        &self,
        node_idx: tsz_parser::parser::NodeIndex,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        let mut current = node_idx;
        while current.is_some() {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                break;
            };
            current = ext.parent;
            if current.is_none() {
                break;
            }
            let Some(node) = self.ctx.arena.get(current) else {
                break;
            };
            if node.kind != syntax_kind_ext::MODULE_DECLARATION {
                continue;
            }
            let Some(module_decl) = self.ctx.arena.get_module(node) else {
                continue;
            };
            let Some(name_node) = self.ctx.arena.get(module_decl.name) else {
                continue;
            };
            if name_node.kind == SyntaxKind::StringLiteral as u16 {
                return true;
            }
        }
        false
    }

    /// Check if a name is declared within the nearest enclosing Module scope.
    /// Used inside `declare module "m"` blocks to distinguish local declarations
    /// from outer-scope symbols.
    fn is_name_in_enclosing_module_scope(
        &self,
        name: &str,
        node_idx: tsz_parser::parser::NodeIndex,
    ) -> bool {
        use tsz_binder::scopes::ContainerKind;

        // Find the enclosing scope for this node
        let Some(scope_id) = self
            .ctx
            .binder
            .node_scope_ids
            .get(&node_idx.0)
            .copied()
            .or_else(|| {
                // Walk up parent nodes to find one with a scope
                let mut current = node_idx;
                loop {
                    let ext = self.ctx.arena.get_extended(current)?;
                    current = ext.parent;
                    if current.is_none() {
                        return None;
                    }
                    if let Some(&sid) = self.ctx.binder.node_scope_ids.get(&current.0) {
                        return Some(sid);
                    }
                }
            })
        else {
            return false;
        };

        // Walk up the scope chain to find the nearest Module scope
        let mut sid = scope_id;
        while sid.is_some() {
            let Some(scope) = self.ctx.binder.scopes.get(sid.0 as usize) else {
                break;
            };
            if scope.kind == ContainerKind::Module {
                // Check if the name is in this module's scope table
                return scope.table.has(name);
            }
            sid = scope.parent;
        }
        false
    }
}
