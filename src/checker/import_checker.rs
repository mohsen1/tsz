//! Import/Export Checking Module
//!
//! This module contains methods for validating import and export declarations.
//! It handles:
//! - Import declaration validation (TS2307, TS2305)
//! - Export assignment validation (TS2309)
//! - Import equals declaration validation (TS1202)
//! - Re-export chain cycle detection
//! - Module body validation
//!
//! This module extends CheckerState with import/export methods as part of
//! the Phase 2 architecture refactoring (task 2.3 - file splitting).

use crate::checker::state::CheckerState;
use crate::parser::NodeIndex;
use crate::parser::syntax_kind_ext;
use crate::scanner::SyntaxKind;
use std::collections::HashSet;

// =============================================================================
// Import/Export Checking Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Import Member Validation
    // =========================================================================

    /// Check that imported members exist in the module's exports.
    ///
    /// Validates that each named import from a module actually exists in that
    /// module's export table.
    pub(crate) fn check_imported_members(
        &mut self,
        import: &crate::parser::node::ImportDeclData,
        module_name: &str,
    ) {
        use crate::checker::types::diagnostics::{
            diagnostic_codes, diagnostic_messages, format_message,
        };

        let clause_node = match self.ctx.arena.get(import.import_clause) {
            Some(node) => node,
            None => return,
        };

        let clause = match self.ctx.arena.get_import_clause(clause_node) {
            Some(c) => c,
            None => return,
        };

        let bindings_node = match self.ctx.arena.get(clause.named_bindings) {
            Some(node) => node,
            None => return,
        };

        if bindings_node.kind == crate::parser::syntax_kind_ext::NAMED_IMPORTS {
            let named_imports = match self.ctx.arena.get_named_imports(bindings_node) {
                Some(ni) => ni,
                None => return,
            };

            let exports_table = match self.ctx.binder.module_exports.get(module_name) {
                Some(table) => table,
                None => return,
            };

            for element_idx in &named_imports.elements.nodes {
                let element_node = match self.ctx.arena.get(*element_idx) {
                    Some(node) => node,
                    None => continue,
                };

                let specifier = match self.ctx.arena.get_specifier(element_node) {
                    Some(s) => s,
                    None => continue,
                };

                let name_idx = if specifier.property_name.is_none() {
                    specifier.name
                } else {
                    specifier.property_name
                };

                let name_node = match self.ctx.arena.get(name_idx) {
                    Some(node) => node,
                    None => continue,
                };

                let identifier = match self.ctx.arena.get_identifier(name_node) {
                    Some(id) => id,
                    None => continue,
                };

                let import_name = &identifier.escaped_text;

                if !exports_table.has(import_name) {
                    let message = format_message(
                        diagnostic_messages::MODULE_HAS_NO_EXPORTED_MEMBER,
                        &[module_name, import_name],
                    );
                    self.error_at_node(
                        specifier.name,
                        &message,
                        diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER,
                    );
                }
            }
        }
    }

    // =========================================================================
    // Module Body Validation
    // =========================================================================

    /// Check a module body for statements and function implementations.
    pub(crate) fn check_module_body(&mut self, body_idx: NodeIndex) {
        let Some(body_node) = self.ctx.arena.get(body_idx) else {
            return;
        };

        if body_node.kind == syntax_kind_ext::MODULE_BLOCK {
            if let Some(block) = self.ctx.arena.get_module_block(body_node)
                && let Some(ref statements) = block.statements
            {
                for &stmt_idx in &statements.nodes {
                    self.check_statement(stmt_idx);
                }
                self.check_function_implementations(&statements.nodes);
            }
        } else if body_node.kind == syntax_kind_ext::MODULE_DECLARATION {
            self.check_statement(body_idx);
        }
    }

    // =========================================================================
    // Export Assignment Validation
    // =========================================================================

    /// Check for export assignment conflicts with other exported elements.
    ///
    /// Validates that `export = X` is not used when there are also other
    /// exported elements (TS2309).
    pub(crate) fn check_export_assignment(&mut self, statements: &[NodeIndex]) {
        use crate::checker::types::diagnostics::diagnostic_codes;

        let mut export_assignment_idx: Option<NodeIndex> = None;
        let mut has_other_exports = false;

        for &stmt_idx in statements {
            let Some(node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };

            match node.kind {
                syntax_kind_ext::EXPORT_ASSIGNMENT => {
                    export_assignment_idx = Some(stmt_idx);

                    if let Some(export_data) = self.ctx.arena.get_export_assignment(node) {
                        self.get_type_of_node(export_data.expression);
                    }
                }
                syntax_kind_ext::EXPORT_DECLARATION => {
                    has_other_exports = true;
                }
                _ => {
                    if self.has_export_modifier(stmt_idx) {
                        has_other_exports = true;
                    }
                }
            }
        }

        if let Some(export_idx) = export_assignment_idx
            && has_other_exports
        {
            self.error_at_node(
                export_idx,
                "An export assignment cannot be used in a module with other exported elements.",
                diagnostic_codes::EXPORT_ASSIGNMENT_WITH_OTHER_EXPORTS,
            );
        }
    }

    /// Check if a statement has an export modifier.
    pub(crate) fn has_export_modifier(&self, stmt_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return false;
        };

        let Some(mods) = self.get_declaration_modifiers(node) else {
            return false;
        };

        mods.nodes.iter().any(|&mod_idx| {
            self.ctx
                .arena
                .get(mod_idx)
                .is_some_and(|mod_node| mod_node.kind == SyntaxKind::ExportKeyword as u16)
        })
    }

    // =========================================================================
    // Import Equals Declaration Validation
    // =========================================================================

    /// Check an import equals declaration for ESM compatibility, unresolved modules,
    /// and conflicts with local declarations.
    ///
    /// Validates `import x = require()` and `import x = Namespace` style imports:
    /// - TS1202 when import assignment is used in ES modules
    /// - TS2307 when the module cannot be found
    /// - TS2440 when import conflicts with a local declaration
    pub(crate) fn check_import_equals_declaration(&mut self, stmt_idx: NodeIndex) {
        use crate::binder::symbol_flags;
        use crate::checker::types::diagnostics::{
            diagnostic_codes, diagnostic_messages, format_message,
        };

        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };
        let Some(import) = self.ctx.arena.get_import_decl(node) else {
            return;
        };

        // Get the import alias name (e.g., 'a' in 'import a = M')
        let import_name = self
            .ctx
            .arena
            .get(import.import_clause)
            .and_then(|n| self.ctx.arena.get_identifier(n))
            .map(|id| id.escaped_text.clone());

        // Check for TS2440: Import declaration conflicts with local declaration
        // This error is specific to ImportEqualsDeclaration (not ES6 imports).
        // It occurs when:
        // 1. The import introduces a name that already has a value declaration
        // 2. The value declaration is in the same file (local)
        //
        // Note: The binder does NOT merge import equals declarations - it creates
        // a new symbol and overwrites the scope. So we need to find ALL symbols
        // with the same name and check if any non-import has VALUE flags.
        if let Some(ref name) = import_name {
            // Get the symbol for this import
            let import_sym_id = self.ctx.binder.node_symbols.get(&stmt_idx.0).copied();

            // Find all symbols with this name (there may be multiple due to shadowing)
            let all_symbols = self.ctx.binder.symbols.find_all_by_name(name);

            for sym_id in all_symbols {
                // Skip the import's own symbol
                if Some(sym_id) == import_sym_id {
                    continue;
                }

                if let Some(sym) = self.ctx.binder.symbols.get(sym_id) {
                    // Check if this symbol has value semantics
                    let is_value = (sym.flags & symbol_flags::VALUE) != 0;
                    let is_alias = (sym.flags & symbol_flags::ALIAS) != 0;

                    // Skip if this is also an import/alias
                    if is_alias {
                        continue;
                    }

                    // Check if this symbol has any declaration in the CURRENT file
                    // A declaration is in the current file if it's in node_symbols
                    let has_local_declaration = sym.declarations.iter().any(|&decl_idx| {
                        // The declaration is local if its node_symbols entry points to this symbol
                        self.ctx.binder.node_symbols.get(&decl_idx.0) == Some(&sym_id)
                    });

                    if is_value && has_local_declaration {
                        let message = format_message(
                            diagnostic_messages::IMPORT_DECLARATION_CONFLICTS_WITH_LOCAL,
                            &[name],
                        );
                        self.error_at_node(
                            stmt_idx,
                            &message,
                            diagnostic_codes::IMPORT_DECLARATION_CONFLICTS_WITH_LOCAL,
                        );
                        return; // Don't emit further errors for this import
                    }
                }
            }
        }

        let Some(ref_node) = self.ctx.arena.get(import.module_specifier) else {
            return;
        };

        // Handle namespace imports: import x = Namespace or import x = Namespace.Member
        // These need to emit TS2503 ("Cannot find namespace") if not found
        if ref_node.kind != SyntaxKind::StringLiteral as u16 {
            self.check_namespace_import(import.module_specifier);
            return;
        }

        // TS1202: Import assignment cannot be used when targeting ECMAScript modules.
        // Files with import/export statements are ESM modules by default.
        // Check if either: (1) module kind is explicitly ESM, or (2) file has imports (external module)
        if self.ctx.compiler_options.module.is_es_module() || self.ctx.binder.is_external_module() {
            self.error_at_node(
                stmt_idx,
                "Import assignment cannot be used when targeting ECMAScript modules. Consider using 'import * as ns from \"mod\"', 'import {a} from \"mod\"', 'import d from \"mod\"', or another module format instead.",
                diagnostic_codes::IMPORT_ASSIGNMENT_CANNOT_BE_USED_WITH_ESM,
            );
        }

        if !self.ctx.report_unresolved_imports {
            return;
        }

        let Some(literal) = self.ctx.arena.get_literal(ref_node) else {
            return;
        };
        let module_name = &literal.text;

        if let Some(ref resolved) = self.ctx.resolved_modules
            && resolved.contains(module_name)
        {
            return;
        }

        if self.ctx.binder.module_exports.contains_key(module_name) {
            return;
        }

        if self
            .ctx
            .binder
            .shorthand_ambient_modules
            .contains(module_name)
        {
            return;
        }

        if self.ctx.binder.declared_modules.contains(module_name) {
            return;
        }

        // Check for specific resolution error from driver (TS2834, TS2835, TS2792, etc.)
        let module_key = module_name.to_string();
        if let Some(error) = self.ctx.get_resolution_error(module_name) {
            // Extract error values before mutable borrow
            let error_code = error.code;
            let error_message = error.message.clone();
            if !self.ctx.modules_with_ts2307_emitted.contains(&module_key) {
                self.ctx
                    .modules_with_ts2307_emitted
                    .insert(module_key.clone());
                self.error_at_node(import.module_specifier, &error_message, error_code);
            }
            return;
        }

        // Fallback: Emit generic TS2307 if no specific error was found
        // Check if we've already emitted TS2307 for this module (prevents duplicate emissions)
        let module_key = module_name.to_string();
        if self.ctx.modules_with_ts2307_emitted.contains(&module_key) {
            return;
        }

        let message = format_message(diagnostic_messages::CANNOT_FIND_MODULE, &[module_name]);
        self.ctx
            .modules_with_ts2307_emitted
            .insert(module_key.clone());
        self.error_at_node(
            import.module_specifier,
            &message,
            diagnostic_codes::CANNOT_FIND_MODULE,
        );
    }

    // =========================================================================
    // Namespace Import Validation (TS2503)
    // =========================================================================

    /// Check a namespace import (import x = Namespace or import x = Namespace.Member).
    /// Emits TS2503 "Cannot find namespace" if the namespace cannot be resolved.
    fn check_namespace_import(&mut self, module_ref: NodeIndex) {
        use crate::checker::types::diagnostics::{
            diagnostic_codes, diagnostic_messages, format_message,
        };

        let Some(ref_node) = self.ctx.arena.get(module_ref) else {
            return;
        };

        // Handle simple identifier: import x = Namespace
        if ref_node.kind == SyntaxKind::Identifier as u16 {
            if let Some(ident) = self.ctx.arena.get_identifier(ref_node) {
                let name = &ident.escaped_text;
                // Try to resolve the identifier as a namespace/module
                if self.resolve_identifier_symbol(module_ref).is_none() {
                    let message = format_message(
                        diagnostic_messages::CANNOT_FIND_NAMESPACE,
                        &[name],
                    );
                    self.error_at_node(
                        module_ref,
                        &message,
                        diagnostic_codes::CANNOT_FIND_NAMESPACE,
                    );
                }
            }
            return;
        }

        // Handle qualified name: import x = Namespace.Member
        if ref_node.kind == syntax_kind_ext::QUALIFIED_NAME {
            if let Some(qn) = self.ctx.arena.get_qualified_name(ref_node) {
                // Check the leftmost part first - this is what determines TS2503 vs TS2694
                let left_name = self.get_leftmost_identifier_name(qn.left);
                if let Some(name) = left_name {
                    // Try to resolve the left identifier
                    let left_resolved = self.resolve_leftmost_qualified_name(qn.left);
                    if left_resolved.is_none() {
                        let message = format_message(
                            diagnostic_messages::CANNOT_FIND_NAMESPACE,
                            &[&name],
                        );
                        self.error_at_node(
                            qn.left,
                            &message,
                            diagnostic_codes::CANNOT_FIND_NAMESPACE,
                        );
                    }
                    // TODO: If left is resolved, check if right member exists (TS2694)
                }
            }
        }
    }

    /// Get the leftmost identifier name from a node (handles nested QualifiedNames).
    fn get_leftmost_identifier_name(&self, idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(idx)?;
        if node.kind == SyntaxKind::Identifier as u16 {
            let ident = self.ctx.arena.get_identifier(node)?;
            return Some(ident.escaped_text.clone());
        }
        if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            let qn = self.ctx.arena.get_qualified_name(node)?;
            return self.get_leftmost_identifier_name(qn.left);
        }
        None
    }

    /// Resolve the leftmost identifier in a potentially nested QualifiedName.
    fn resolve_leftmost_qualified_name(&self, idx: NodeIndex) -> Option<crate::binder::SymbolId> {
        let node = self.ctx.arena.get(idx)?;
        if node.kind == SyntaxKind::Identifier as u16 {
            return self.resolve_identifier_symbol(idx);
        }
        if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            let qn = self.ctx.arena.get_qualified_name(node)?;
            return self.resolve_leftmost_qualified_name(qn.left);
        }
        None
    }

    // =========================================================================
    // Import Declaration Validation
    // =========================================================================

    /// Check an import declaration for unresolved modules and missing exports.
    pub(crate) fn check_import_declaration(&mut self, stmt_idx: NodeIndex) {
        use crate::checker::types::diagnostics::{
            diagnostic_codes, diagnostic_messages, format_message,
        };

        if !self.ctx.report_unresolved_imports {
            return;
        }

        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        let Some(import) = self.ctx.arena.get_import_decl(node) else {
            return;
        };

        let Some(spec_node) = self.ctx.arena.get(import.module_specifier) else {
            return;
        };

        let Some(literal) = self.ctx.arena.get_literal(spec_node) else {
            return;
        };

        let module_name = &literal.text;

        if self.would_create_cycle(module_name) {
            let cycle_path: Vec<&str> = self
                .ctx
                .import_resolution_stack
                .iter()
                .map(|s| s.as_str())
                .chain(std::iter::once(module_name.as_str()))
                .collect();
            let cycle_str = cycle_path.join(" -> ");
            let message = format!("Circular import detected: {}", cycle_str);

            // Check if we've already emitted TS2307 for this module (prevents duplicate emissions)
            let module_key = module_name.to_string();
            if !self.ctx.modules_with_ts2307_emitted.contains(&module_key) {
                self.ctx.modules_with_ts2307_emitted.insert(module_key);
                self.error_at_node(
                    import.module_specifier,
                    &message,
                    diagnostic_codes::CANNOT_FIND_MODULE,
                );
            }
            return;
        }

        self.ctx.import_resolution_stack.push(module_name.clone());

        // Check for specific resolution error from driver FIRST (TS2834, TS2835, TS2792, etc.)
        // This must be checked before resolved_modules to catch extensionless import errors
        let module_key = module_name.to_string();
        if let Some(error) = self.ctx.get_resolution_error(module_name) {
            // Extract error values before mutable borrow
            let error_code = error.code;
            let error_message = error.message.clone();
            // Check if we've already emitted an error for this module (prevents duplicate emissions)
            if !self.ctx.modules_with_ts2307_emitted.contains(&module_key) {
                self.ctx
                    .modules_with_ts2307_emitted
                    .insert(module_key.clone());
                self.error_at_node(import.module_specifier, &error_message, error_code);
            }
            self.ctx.import_resolution_stack.pop();
            return;
        }

        // Check if module was successfully resolved
        if let Some(ref resolved) = self.ctx.resolved_modules
            && resolved.contains(module_name)
        {
            self.check_imported_members(import, module_name);

            if let Some(source_modules) = self.ctx.binder.wildcard_reexports.get(module_name) {
                let mut visited = HashSet::new();
                for source_module in source_modules {
                    self.check_reexport_chain_for_cycles(source_module, &mut visited);
                }
            }

            self.ctx.import_resolution_stack.pop();
            return;
        }

        if self.ctx.binder.module_exports.contains_key(module_name) {
            self.check_imported_members(import, module_name);

            if let Some(source_modules) = self.ctx.binder.wildcard_reexports.get(module_name) {
                let mut visited = HashSet::new();
                for source_module in source_modules {
                    self.check_reexport_chain_for_cycles(source_module, &mut visited);
                }
            }

            self.ctx.import_resolution_stack.pop();
            return;
        }

        if self.is_ambient_module_match(module_name) {
            self.ctx.import_resolution_stack.pop();
            return;
        }

        // Fallback: Emit generic TS2307 if no specific error was found
        // Check if we've already emitted TS2307 for this module (prevents duplicate emissions)
        if !self.ctx.modules_with_ts2307_emitted.contains(&module_key) {
            self.ctx
                .modules_with_ts2307_emitted
                .insert(module_key.clone());
            let message = format_message(diagnostic_messages::CANNOT_FIND_MODULE, &[module_name]);
            self.error_at_node(
                import.module_specifier,
                &message,
                diagnostic_codes::CANNOT_FIND_MODULE,
            );
        }

        self.ctx.import_resolution_stack.pop();
    }

    // =========================================================================
    // Re-export Cycle Detection
    // =========================================================================

    /// Check re-export chains for circular dependencies.
    pub(crate) fn check_reexport_chain_for_cycles(
        &mut self,
        module_name: &str,
        visited: &mut HashSet<String>,
    ) {
        use crate::checker::types::diagnostics::{diagnostic_codes, diagnostic_messages};

        if visited.contains(module_name) {
            let cycle_path: Vec<&str> = visited
                .iter()
                .map(|s| s.as_str())
                .chain(std::iter::once(module_name))
                .collect();
            let cycle_str = cycle_path.join(" -> ");
            let message = format!("{}: {}", diagnostic_messages::CANNOT_FIND_MODULE, cycle_str);

            // Check if we've already emitted TS2307 for this module (prevents duplicate emissions)
            let module_key = module_name.to_string();
            if !self.ctx.modules_with_ts2307_emitted.contains(&module_key) {
                self.ctx.modules_with_ts2307_emitted.insert(module_key);
                self.error(0, 0, message, diagnostic_codes::CANNOT_FIND_MODULE);
            }
            return;
        }

        visited.insert(module_name.to_string());

        if let Some(source_modules) = self.ctx.binder.wildcard_reexports.get(module_name) {
            for source_module in source_modules {
                self.check_reexport_chain_for_cycles(source_module, visited);
            }
        }

        if let Some(reexports) = self.ctx.binder.reexports.get(module_name) {
            for (source_module, _) in reexports.values() {
                self.check_reexport_chain_for_cycles(source_module, visited);
            }
        }

        visited.remove(module_name);
    }

    /// Check if adding a module to the resolution path would create a cycle.
    pub(crate) fn would_create_cycle(&self, module: &str) -> bool {
        self.ctx
            .import_resolution_stack
            .contains(&module.to_string())
    }
}
