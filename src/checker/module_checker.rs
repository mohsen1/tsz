//! Module and Import Checking Module
//!
//! This module contains methods for validating module imports and exports.
//! It handles:
//! - Dynamic import module specifier resolution (TS2307)
//! - Export declaration module specifier resolution
//! - Circular re-export detection
//!
//! This module extends CheckerState with module-related methods as part of
//! the Phase 2 architecture refactoring (task 2.3 - file splitting).

use crate::checker::state::CheckerState;
use crate::parser::NodeIndex;
use std::collections::HashSet;

// =============================================================================
// Module and Import Checking Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Dynamic Import Validation
    // =========================================================================

    /// Check dynamic import module specifier for unresolved modules.
    ///
    /// Validates that the module specifier in a dynamic import() call
    /// can be resolved. Emits TS2307 if the module cannot be found.
    ///
    /// ## Parameters:
    /// - `call`: The call expression node for the import() call
    ///
    /// ## Validation:
    /// - Only checks string literal specifiers (dynamic specifiers cannot be statically checked)
    /// - Checks if module exists in resolved_modules, module_exports, shorthand_ambient_modules, or declared_modules
    /// - Emits TS2307 for unresolved module specifiers
    /// - Validates CommonJS vs ESM import compatibility
    pub(crate) fn check_dynamic_import_module_specifier(
        &mut self,
        call: &crate::parser::node::CallExprData,
    ) {
        use crate::checker::types::diagnostics::{
            diagnostic_codes, diagnostic_messages, format_message,
        };

        if !self.ctx.report_unresolved_imports {
            return;
        }

        // Get the first argument (module specifier)
        let args = call
            .arguments
            .as_ref()
            .map(|a| &a.nodes)
            .map(|n| n.as_slice())
            .unwrap_or(&[]);

        if args.is_empty() {
            return; // No argument - will be caught by argument count check
        }

        let arg_idx = args[0];
        let Some(arg_node) = self.ctx.arena.get(arg_idx) else {
            return;
        };

        // Only check string literal module specifiers
        // Dynamic specifiers (variables, template literals) cannot be statically checked
        let Some(literal) = self.ctx.arena.get_literal(arg_node) else {
            return;
        };

        let module_name = &literal.text;

        // Check if the module was resolved by the CLI driver (multi-file mode)
        if let Some(ref resolved) = self.ctx.resolved_modules
            && resolved.contains(module_name)
        {
            // Additional validation: check for ESM/CommonJS compatibility
            // If this is an ESM file, importing from a CommonJS module might need special handling
            return; // Module exists
        }

        // Check if the module exists in the module_exports map (cross-file module resolution)
        if self.ctx.binder.module_exports.contains_key(module_name) {
            return; // Module exists
        }

        // Check if this is a shorthand ambient module (declare module "foo")
        if self
            .ctx
            .binder
            .shorthand_ambient_modules
            .contains(module_name)
        {
            return; // Ambient module exists
        }

        // Check declared modules (regular ambient modules with body)
        if self.ctx.binder.declared_modules.contains(module_name) {
            return; // Declared module exists
        }

        // Check for specific resolution error from driver (TS2834, TS2835, TS2792, etc.)
        let module_key = module_name.to_string();
        if let Some(error) = self.ctx.get_resolution_error(module_name) {
            // Extract error values before mutable borrow
            let error_code = error.code;
            let error_message = error.message.clone();
            if !self.ctx.modules_with_ts2307_emitted.contains(&module_key) {
                self.ctx.modules_with_ts2307_emitted.insert(module_key.clone());
                self.error_at_node(arg_idx, &error_message, error_code);
            }
            return;
        }

        // Fallback: Module not found - emit TS2307
        // Check if we've already emitted TS2307 for this module (prevents duplicate emissions)
        let module_key = module_name.to_string();
        if !self.ctx.modules_with_ts2307_emitted.contains(&module_key) {
            self.ctx
                .modules_with_ts2307_emitted
                .insert(module_key.clone());
            let message = format_message(diagnostic_messages::CANNOT_FIND_MODULE, &[module_name]);
            self.error_at_node(arg_idx, &message, diagnostic_codes::CANNOT_FIND_MODULE);
        }
    }

    // =========================================================================
    // Export Module Specifier Validation
    // =========================================================================

    /// Check export declaration module specifier for unresolved modules.
    ///
    /// Validates that the module specifier in an export ... from "module" statement
    /// can be resolved. Emits TS2307 if the module cannot be found.
    ///
    /// ## Parameters:
    /// - `stmt_idx`: The export declaration statement node
    ///
    /// ## Validation:
    /// - Checks if module exists in resolved_modules, module_exports, shorthand_ambient_modules, or declared_modules
    /// - Emits TS2307 for unresolved module specifiers
    /// - Validates re-exported members exist in source module
    /// - Checks for circular re-export chains
    pub(crate) fn check_export_module_specifier(&mut self, stmt_idx: NodeIndex) {
        use crate::checker::types::diagnostics::{
            diagnostic_codes, diagnostic_messages, format_message,
        };

        if !self.ctx.report_unresolved_imports {
            return;
        }

        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        let Some(export_decl) = self.ctx.arena.get_export_decl(node) else {
            return;
        };

        // Get module specifier string
        let Some(spec_node) = self.ctx.arena.get(export_decl.module_specifier) else {
            return;
        };

        let Some(literal) = self.ctx.arena.get_literal(spec_node) else {
            return;
        };

        let module_name = &literal.text;

        // Check for circular re-exports
        if self.would_create_cycle(module_name) {
            let cycle_path: Vec<&str> = self
                .ctx
                .import_resolution_stack
                .iter()
                .map(|s| s.as_str())
                .chain(std::iter::once(module_name.as_str()))
                .collect();
            let cycle_str = cycle_path.join(" -> ");
            let message = format!("Circular re-export detected: {}", cycle_str);

            // Check if we've already emitted TS2307 for this module (prevents duplicate emissions)
            let module_key = module_name.to_string();
            if !self.ctx.modules_with_ts2307_emitted.contains(&module_key) {
                self.ctx.modules_with_ts2307_emitted.insert(module_key);
                self.error_at_node(
                    export_decl.module_specifier,
                    &message,
                    diagnostic_codes::CANNOT_FIND_MODULE,
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
            // Check for circular re-export chains
            if let Some(source_modules) = self.ctx.binder.wildcard_reexports.get(module_name) {
                let mut visited = HashSet::new();
                for source_module in source_modules {
                    self.check_reexport_chain_for_cycles(source_module, &mut visited);
                }
            }
            // Validate named re-exports exist in target module
            self.validate_reexported_members(&export_decl, module_name);
            self.ctx.import_resolution_stack.pop();
            return;
        }

        // Check if the module exists in the module_exports map (cross-file module resolution)
        if self.ctx.binder.module_exports.contains_key(module_name) {
            // Check for circular re-export chains
            if let Some(source_modules) = self.ctx.binder.wildcard_reexports.get(module_name) {
                let mut visited = HashSet::new();
                for source_module in source_modules {
                    self.check_reexport_chain_for_cycles(source_module, &mut visited);
                }
            }
            // Validate named re-exports exist in target module
            self.validate_reexported_members(&export_decl, module_name);
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

        if self.ctx.binder.declared_modules.contains(module_name) {
            self.ctx.import_resolution_stack.pop();
            return;
        }

        // Emit TS2307 for unresolved export module specifiers
        // Check if we've already emitted TS2307 for this module (prevents duplicate emissions)
        let module_key = module_name.to_string();
        if !self.ctx.modules_with_ts2307_emitted.contains(&module_key) {
            self.ctx
                .modules_with_ts2307_emitted
                .insert(module_key.clone());
            let message = format_message(diagnostic_messages::CANNOT_FIND_MODULE, &[module_name]);
            self.error_at_node(
                export_decl.module_specifier,
                &message,
                diagnostic_codes::CANNOT_FIND_MODULE,
            );
        }

        self.ctx.import_resolution_stack.pop();
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
        export_decl: &crate::parser::node::ExportDeclData,
        module_name: &str,
    ) {
        use crate::checker::types::diagnostics::{
            diagnostic_codes, diagnostic_messages, format_message,
        };
        use crate::parser::syntax_kind_ext;

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

        // Get the module's exports (try local binder first, then cross-file)
        let module_exports = self
            .ctx
            .binder
            .module_exports
            .get(module_name)
            .cloned()
            .or_else(|| self.resolve_cross_file_namespace_exports(module_name));

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
            let export_name = if !specifier.property_name.is_none() {
                if let Some(text) = self.get_identifier_text_from_idx(specifier.property_name) {
                    text
                } else {
                    continue;
                }
            } else if !specifier.name.is_none() {
                if let Some(text) = self.get_identifier_text_from_idx(specifier.name) {
                    text
                } else {
                    continue;
                }
            } else {
                continue;
            };

            // Check if this name is exported from the source module
            if !module_exports.has(&export_name) {
                // TS2305: Module has no exported member
                let message = format_message(
                    diagnostic_messages::MODULE_HAS_NO_EXPORTED_MEMBER,
                    &[&export_name, module_name],
                );
                self.error_at_node(
                    specifier_idx,
                    &message,
                    diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER,
                );
            }
        }
    }

    // =========================================================================
    // Dynamic Import Return Type
    // =========================================================================

    /// Get the return type for a dynamic import() call.
    ///
    /// Returns Promise<ModuleType> where ModuleType is an object containing
    /// all the module's exports. Falls back to Promise<any> or just `any` when:
    /// - The module cannot be resolved
    /// - Promise is not available (ES5 target without lib)
    ///
    /// This method implements Phase 1.3 of the module resolution plan.
    pub(crate) fn get_dynamic_import_type(
        &mut self,
        call: &crate::parser::node::CallExprData,
    ) -> crate::solver::TypeId {
        use crate::solver::PropertyInfo;

        // Get the first argument (module specifier)
        let args = call
            .arguments
            .as_ref()
            .map(|a| &a.nodes)
            .map(|n| n.as_slice())
            .unwrap_or(&[]);

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

        // Try to get module exports for the namespace type
        let exports_table = self
            .ctx
            .binder
            .module_exports
            .get(module_name)
            .cloned()
            .or_else(|| self.resolve_cross_file_namespace_exports(module_name));

        if let Some(exports_table) = exports_table {
            // Create an object type with all module exports
            let mut props: Vec<PropertyInfo> = Vec::new();
            for (name, &export_sym_id) in exports_table.iter() {
                let prop_type = self.get_type_of_symbol(export_sym_id);
                let name_atom = self.ctx.types.intern_string(name);
                props.push(PropertyInfo {
                    name: name_atom,
                    type_id: prop_type,
                    write_type: prop_type,
                    optional: false,
                    readonly: false,
                    is_method: false,
                });
            }

            // Merge module augmentations
            // Module augmentations add interfaces/types to existing modules
            // e.g., declare module 'express' { interface Request { user?: User; } }
            if let Some(augmentations) = self.ctx.binder.module_augmentations.get(module_name) {
                for (decl_name, decl_idx) in augmentations {
                    // Get the type of the augmentation declaration
                    let aug_type = self.get_type_of_node(*decl_idx);
                    let name_atom = self.ctx.types.intern_string(decl_name);

                    // Check if this augments an existing export
                    if let Some(existing) = props.iter_mut().find(|p| p.name == name_atom) {
                        // Merge types - for interfaces, this creates an intersection
                        existing.type_id = self
                            .ctx
                            .types
                            .intersection(vec![existing.type_id, aug_type]);
                        existing.write_type = existing.type_id;
                    } else {
                        // New export from augmentation
                        props.push(PropertyInfo {
                            name: name_atom,
                            type_id: aug_type,
                            write_type: aug_type,
                            optional: false,
                            readonly: false,
                            is_method: false,
                        });
                    }
                }
            }

            let module_type = self.ctx.types.object(props);
            return self.create_promise_of(module_type);
        }

        // Module not found - return Promise<any>
        self.create_promise_any()
    }

    /// Create a Promise<any> type.
    fn create_promise_any(&self) -> crate::solver::TypeId {
        self.create_promise_of(crate::solver::TypeId::ANY)
    }

    /// Create a Promise<T> type.
    ///
    /// Uses the synthetic PROMISE_BASE type to create Promise<T>.
    /// This works even without lib files since PROMISE_BASE is a built-in type.
    fn create_promise_of(&self, inner_type: crate::solver::TypeId) -> crate::solver::TypeId {
        use crate::solver::TypeId;

        // Use PROMISE_BASE as the Promise constructor
        // This is a synthetic type that allows Promise<T> to work without lib files
        self.ctx
            .types
            .application(TypeId::PROMISE_BASE, vec![inner_type])
    }
}
