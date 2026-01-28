//! Namespace Type Checking Module
//!
//! This module contains namespace type checking methods for CheckerState
//! as part of Phase 2 architecture refactoring.
//!
//! The methods in this module handle:
//! - Merging namespace exports into constructor types
//! - Merging namespace exports into function types
//! - Resolving re-exported members through export chains
//!
//! These operations are necessary for TypeScript's declaration merging feature
//! where a class/function and namespace with the same name can be merged.

use crate::binder::SymbolId;
use crate::checker::state::CheckerState;
use crate::interner::Atom;
use crate::solver::TypeId;
use std::sync::Arc;

// =============================================================================
// Namespace Type Checking
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Namespace Export Merging
    // =========================================================================

    /// Merge namespace exports into a constructor type for class+namespace merging.
    ///
    /// When a class and namespace are merged (same name), the namespace's exports
    /// become accessible as static properties on the class constructor type.
    ///
    /// ## TypeScript Example:
    /// ```typescript
    /// class Foo {
    ///   static bar = 1;
    /// }
    /// namespace Foo {
    ///   export const baz = 2;
    /// }
    /// // Foo.bar and Foo.baz are both accessible
    /// ```
    pub(crate) fn merge_namespace_exports_into_constructor(
        &mut self,
        sym_id: SymbolId,
        ctor_type: TypeId,
    ) -> TypeId {
        use crate::solver::type_queries::get_callable_shape;
        use crate::solver::{CallableShape, PropertyInfo};
        use rustc_hash::FxHashMap;

        // Check recursion depth to prevent stack overflow
        const MAX_MERGE_DEPTH: u32 = 32;
        let depth = self.ctx.symbol_resolution_depth.get();
        if depth >= MAX_MERGE_DEPTH {
            return ctor_type; // Prevent infinite recursion in merge
        }

        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return ctor_type;
        };
        let Some(exports) = symbol.exports.as_ref() else {
            return ctor_type;
        };
        let Some(shape) = get_callable_shape(self.ctx.types, ctor_type) else {
            return ctor_type;
        };

        let mut props: FxHashMap<Atom, PropertyInfo> = shape
            .properties
            .iter()
            .map(|prop| (prop.name, prop.clone()))
            .collect();

        // Merge ALL exports from the namespace into the constructor type.
        // This includes both value exports (consts, functions) and type-only exports (interfaces, type aliases).
        // For merged class+namespace symbols, TypeScript allows accessing both value and type members.
        for (name, member_id) in exports.iter() {
            // Skip if this member is already being resolved (prevents infinite recursion)
            if self.ctx.symbol_resolution_set.contains(member_id) {
                continue; // Skip circular references
            }

            let type_id = self.get_type_of_symbol(*member_id);
            let name_atom = self.ctx.types.intern_string(name);
            props.entry(name_atom).or_insert(PropertyInfo {
                name: name_atom,
                type_id,
                write_type: type_id,
                optional: false,
                readonly: false,
                is_method: false,
            });
        }

        let properties: Vec<PropertyInfo> = props.into_values().collect();
        self.ctx.types.callable(CallableShape {
            call_signatures: shape.call_signatures.clone(),
            construct_signatures: shape.construct_signatures.clone(),
            properties,
            string_index: shape.string_index.clone(),
            number_index: shape.number_index.clone(),
        })
    }

    /// Merge namespace exports into a function type for function+namespace merging.
    ///
    /// This is similar to merge_namespace_exports_into_constructor but for functions.
    /// When a function and namespace are merged (same name), the namespace's exports
    /// become accessible as static properties on the function type.
    ///
    /// ## TypeScript Example:
    /// ```typescript
    /// function Model() {}
    /// namespace Model {
    ///   export interface Options {}
    /// }
    /// let opts: Model.Options;  // Works because Options is merged into Model
    /// ```
    pub(crate) fn merge_namespace_exports_into_function(
        &mut self,
        sym_id: SymbolId,
        function_type: TypeId,
    ) -> (TypeId, Vec<crate::solver::TypeParamInfo>) {
        use crate::solver::type_queries::get_callable_shape;
        use crate::solver::{CallableShape, PropertyInfo};
        use rustc_hash::FxHashMap;

        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return (function_type, Vec::new());
        };
        let Some(exports) = symbol.exports.as_ref() else {
            return (function_type, Vec::new());
        };
        let Some(shape) = get_callable_shape(self.ctx.types, function_type) else {
            return (function_type, Vec::new());
        };

        let mut props: FxHashMap<Atom, PropertyInfo> = shape
            .properties
            .iter()
            .map(|prop| (prop.name, prop.clone()))
            .collect();

        // Merge ALL exports from the namespace into the function type.
        // This allows accessing namespace members via FunctionName.Member.
        for (name, member_id) in exports.iter() {
            // Skip if this member is already being resolved (prevents infinite recursion)
            if self.ctx.symbol_resolution_set.contains(member_id) {
                continue; // Skip circular references
            }

            let type_id = self.get_type_of_symbol(*member_id);
            let name_atom = self.ctx.types.intern_string(name);
            props.entry(name_atom).or_insert(PropertyInfo {
                name: name_atom,
                type_id,
                write_type: type_id,
                optional: false,
                readonly: false,
                is_method: false,
            });
        }

        let properties: Vec<PropertyInfo> = props.into_values().collect();
        let merged_type = self.ctx.types.callable(CallableShape {
            call_signatures: shape.call_signatures.clone(),
            construct_signatures: shape.construct_signatures.clone(),
            properties,
            string_index: shape.string_index.clone(),
            number_index: shape.number_index.clone(),
        });

        (merged_type, Vec::new())
    }

    // =========================================================================
    // Re-export Resolution
    // =========================================================================

    /// Resolve a re-exported member from a module by following re-export chains.
    ///
    /// This function handles cases where a namespace member is re-exported from
    /// another module using `export { foo } from './bar'` or `export * from './bar'`.
    ///
    /// ## Re-export Chain Resolution:
    /// 1. Check if the member is directly exported from the module
    /// 2. If not, check for named re-exports: `export { foo } from 'bar'`
    /// 3. If not found, check wildcard re-exports: `export * from 'bar'`
    /// 4. Recursively follow re-export chains to find the original member
    ///
    /// ## TypeScript Examples:
    /// ```typescript
    /// // bar.ts
    /// export const foo = 42;
    ///
    /// // a.ts
    /// export { foo } from './bar';
    ///
    /// // b.ts
    /// export * from './a';
    ///
    /// // main.ts
    /// import * as b from './b';
    /// let x = b.foo;  // Should find foo through re-export chain
    /// ```
    pub(crate) fn resolve_reexported_member(
        &self,
        module_specifier: &str,
        member_name: &str,
        lib_binders: &[Arc<crate::binder::BinderState>],
    ) -> Option<SymbolId> {
        // First, check if it's a direct export from this module
        if let Some(module_exports) = self.ctx.binder.module_exports.get(module_specifier) {
            if let Some(sym_id) = module_exports.get(member_name) {
                // Found direct export - but we need to resolve if it's itself a re-export
                // Get the symbol and check if it's an alias
                if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                    if symbol.flags & crate::binder::symbol_flags::ALIAS != 0 {
                        // Follow the alias
                        if let Some(ref import_module) = symbol.import_module {
                            let export_name = symbol.import_name.as_deref().unwrap_or(member_name);
                            return self.resolve_reexported_member(
                                import_module,
                                export_name,
                                lib_binders,
                            );
                        }
                    }
                }
                return Some(sym_id);
            }
        }

        // Check for named re-exports: `export { foo } from 'bar'`
        if let Some(file_reexports) = self.ctx.binder.reexports.get(module_specifier) {
            if let Some((source_module, original_name)) = file_reexports.get(member_name) {
                let name_to_lookup = original_name.as_deref().unwrap_or(member_name);
                return self.resolve_reexported_member(source_module, name_to_lookup, lib_binders);
            }
        }

        // Check for wildcard re-exports: `export * from 'bar'`
        if let Some(source_modules) = self.ctx.binder.wildcard_reexports.get(module_specifier) {
            for source_module in source_modules {
                if let Some(sym_id) =
                    self.resolve_reexported_member(source_module, member_name, lib_binders)
                {
                    return Some(sym_id);
                }
            }
        }

        // Check lib binders for the module
        for lib_binder in lib_binders {
            // First check lib binder's module_exports
            if let Some(module_exports) = lib_binder.module_exports.get(module_specifier) {
                if let Some(sym_id) = module_exports.get(member_name) {
                    return Some(sym_id);
                }
            }
            // Then check lib binder's re-exports
            if let Some(file_reexports) = lib_binder.reexports.get(module_specifier) {
                if let Some((source_module, original_name)) = file_reexports.get(member_name) {
                    let name_to_lookup = original_name.as_deref().unwrap_or(member_name);
                    return self.resolve_reexported_member(
                        source_module,
                        name_to_lookup,
                        lib_binders,
                    );
                }
            }
            // Then check lib binder's wildcard re-exports
            if let Some(source_modules) = lib_binder.wildcard_reexports.get(module_specifier) {
                for source_module in source_modules {
                    if let Some(sym_id) =
                        self.resolve_reexported_member(source_module, member_name, lib_binders)
                    {
                        return Some(sym_id);
                    }
                }
            }
        }

        None
    }
}
