//! Import and export declaration binding.
//!
//! This module handles binding of import declarations, export declarations,
//! import-equals declarations, and related export marking/symbol resolution.

use crate::state::BinderState;
use crate::{ContainerKind, SymbolTable, symbol_flags};
use tsz_parser::NodeIndex;
use tsz_parser::parser::node::{Node, NodeArena};
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl BinderState {
    pub(crate) fn bind_import_declaration(
        &mut self,
        arena: &NodeArena,
        node: &Node,
        _idx: NodeIndex,
    ) {
        if let Some(import) = arena.get_import_decl(node) {
            // Get module specifier for cross-file module resolution
            let module_specifier = if import.module_specifier.is_none() {
                None
            } else {
                arena
                    .get(import.module_specifier)
                    .and_then(|spec_node| arena.get_literal(spec_node))
                    .map(|lit| lit.text.clone())
            };

            if let Some(clause_node) = arena.get(import.import_clause)
                && let Some(clause) = arena.get_import_clause(clause_node)
            {
                let clause_type_only = clause.is_type_only;
                // Default import
                if !clause.name.is_none()
                    && let Some(name) = Self::get_identifier_name(arena, clause.name)
                {
                    // Use import_clause node as the declaration node
                    let sym_id =
                        self.declare_symbol(name, symbol_flags::ALIAS, import.import_clause, false);
                    if let Some(sym) = self.symbols.get_mut(sym_id) {
                        sym.is_type_only = clause_type_only;
                        // Track module for cross-file resolution
                        if let Some(ref specifier) = module_specifier {
                            sym.import_module = Some(specifier.clone());
                            // Default imports (`import X from "mod"`) resolve the module's
                            // **default** export, regardless of the local binding name.
                            sym.import_name = Some("default".to_string());
                        }
                    }
                    self.node_symbols.insert(clause.name.0, sym_id);
                }

                // Named imports
                if !clause.named_bindings.is_none()
                    && let Some(bindings_node) = arena.get(clause.named_bindings)
                {
                    if bindings_node.kind == SyntaxKind::Identifier as u16 {
                        if let Some(name) = Self::get_identifier_name(arena, clause.named_bindings)
                        {
                            let sym_id = self.declare_symbol(
                                name,
                                symbol_flags::ALIAS,
                                clause.named_bindings,
                                false,
                            );
                            if let Some(sym) = self.symbols.get_mut(sym_id) {
                                sym.is_type_only = clause_type_only;
                                // Track module for cross-file resolution
                                if let Some(ref specifier) = module_specifier {
                                    sym.import_module = Some(specifier.clone());
                                }
                            }
                            self.node_symbols.insert(clause.named_bindings.0, sym_id);
                        }
                    } else if let Some(named) = arena.get_named_imports(bindings_node) {
                        // Handle namespace import: import * as ns from 'module'
                        if !named.name.is_none()
                            && let Some(name) = Self::get_identifier_name(arena, named.name)
                        {
                            // Use named_bindings (NamespaceImport) as the declaration node
                            let sym_id = self.declare_symbol(
                                name,
                                symbol_flags::ALIAS,
                                clause.named_bindings,
                                false,
                            );
                            if let Some(sym) = self.symbols.get_mut(sym_id) {
                                sym.is_type_only = clause_type_only;
                                // Track module for cross-file resolution
                                if let Some(ref specifier) = module_specifier {
                                    sym.import_module = Some(specifier.clone());
                                }
                            }
                            self.node_symbols.insert(named.name.0, sym_id);
                            self.node_symbols.insert(clause.named_bindings.0, sym_id);
                        }
                        // Handle named imports: import { foo, bar } from 'module'
                        for &spec_idx in &named.elements.nodes {
                            if let Some(spec_node) = arena.get(spec_idx)
                                && let Some(spec) = arena.get_specifier(spec_node)
                            {
                                let spec_type_only = clause_type_only || spec.is_type_only;
                                let local_ident = if spec.name.is_none() {
                                    spec.property_name
                                } else {
                                    spec.name
                                };
                                let local_name = Self::get_identifier_name(arena, local_ident);

                                if let Some(name) = local_name {
                                    let sym_id = self.declare_symbol(
                                        name,
                                        symbol_flags::ALIAS,
                                        spec_idx,
                                        false,
                                    );

                                    // Get property name before mutable borrow to avoid borrow checker error
                                    let prop_name =
                                        if !spec.name.is_none() && !spec.property_name.is_none() {
                                            Self::get_identifier_name(arena, spec.property_name)
                                        } else {
                                            None
                                        };

                                    if let Some(sym) = self.symbols.get_mut(sym_id) {
                                        sym.is_type_only = spec_type_only;
                                        // Track module and original name for cross-file resolution
                                        if let Some(ref specifier) = module_specifier {
                                            sym.import_module = Some(specifier.clone());
                                            // For renamed imports (import { foo as bar }), track original name
                                            if let Some(prop_name) = prop_name {
                                                sym.import_name = Some(prop_name.to_string());
                                            } else {
                                                // For non-renamed imports (import { foo }), still set
                                                // import_name so the checker can distinguish named
                                                // imports from namespace imports (import * as ns).
                                                sym.import_name = Some(name.to_string());
                                            }
                                        }
                                    }
                                    self.node_symbols.insert(spec_idx.0, sym_id);
                                    self.node_symbols.insert(local_ident.0, sym_id);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Bind import equals declaration: import x = ns.member or import x = require("...")
    pub(crate) fn bind_import_equals_declaration(
        &mut self,
        arena: &NodeArena,
        node: &Node,
        idx: NodeIndex,
    ) {
        if let Some(import) = arena.get_import_decl(node) {
            // import_clause holds the alias name (e.g., 'x' in 'import x = ...')
            if let Some(name) = Self::get_identifier_name(arena, import.import_clause) {
                // Check if exported (for export import x = ns.member)
                let is_exported = Self::has_export_modifier(arena, import.modifiers.as_ref());

                // Get module specifier for external module require imports
                // e.g., import ts = require("typescript") -> module_specifier = "typescript"
                let module_specifier = if import.module_specifier.is_none() {
                    None
                } else {
                    arena.get(import.module_specifier).and_then(|spec_node| {
                        if spec_node.kind == SyntaxKind::StringLiteral as u16 {
                            arena.get_literal(spec_node).map(|lit| lit.text.clone())
                        } else {
                            None
                        }
                    })
                };

                // Create symbol with ALIAS flag
                let sym_id = self.declare_symbol(name, symbol_flags::ALIAS, idx, is_exported);

                if let Some(sym) = self.symbols.get_mut(sym_id) {
                    // If this is the first value declaration, or if we're merging compatible
                    // declarations where this should be the value declaration.
                    // For aliases, we generally track the first one as value decl,
                    // but for duplicates we just want to ensure it's recorded.
                    if sym.value_declaration.is_none() {
                        sym.value_declaration = idx;
                    }

                    // Track module for cross-file resolution and unresolved import detection
                    if let Some(ref specifier) = module_specifier {
                        // If multiple declarations have different specifiers, we might overwrite here.
                        // For valid merges (if any), this logic might need refinement,
                        // but for duplicates it doesn't matter much which one wins for resolution
                        // as long as we report the error.
                        sym.import_module = Some(specifier.clone());
                    }
                }

                // declare_symbol handles adding to current_scope and node_symbols.
                // We still need to explicit export handling if declare_symbol didn't handle it fully?
                // declare_symbol takes is_exported flag.
            }
        }
    }

    /// Mark symbols associated with a declaration node as exported.
    /// This is required because the parser wraps exported declarations in `ExportDeclaration`
    /// nodes instead of attaching modifiers to the declaration itself.
    pub(crate) fn mark_exported_symbols(&mut self, arena: &NodeArena, idx: NodeIndex) {
        // 1. Try direct symbol lookup (Function, Class, Enum, Module, Interface, TypeAlias)
        if let Some(sym_id) = self.node_symbols.get(&idx.0) {
            if let Some(sym) = self.symbols.get_mut(*sym_id) {
                sym.is_exported = true;
            }
            return;
        }

        // 2. Handle VariableStatement -> VariableDeclarationList -> VariableDeclaration
        // Variable statements don't have a symbol; their declarations do.
        if let Some(node) = arena.get(idx)
            && node.kind == syntax_kind_ext::VARIABLE_STATEMENT
            && let Some(var) = arena.get_variable(node)
        {
            for &list_idx in &var.declarations.nodes {
                if let Some(list_node) = arena.get(list_idx)
                    && let Some(list) = arena.get_variable(list_node)
                {
                    for &decl_idx in &list.declarations.nodes {
                        if let Some(sym_id) = self.node_symbols.get(&decl_idx.0)
                            && let Some(sym) = self.symbols.get_mut(*sym_id)
                        {
                            sym.is_exported = true;
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn bind_export_declaration(
        &mut self,
        arena: &NodeArena,
        node: &Node,
        _idx: NodeIndex,
    ) {
        if let Some(export) = arena.get_export_decl(node) {
            // Export clause can be:
            // - NamedExports: export { foo, bar }
            // - NamespaceExport: export * as ns from 'mod'
            // - Declaration: export function/class/const/etc
            // - or NONE for: export * from 'mod'

            // Check if the entire export declaration is type-only: export type { ... }
            let export_type_only = export.is_type_only;

            // export default ...
            //
            // Note: the parser represents `export default ...` as an EXPORT_DECLARATION with
            // `is_default_export = true`, so we must handle it *before* the "namespace export"
            // fallback that matches any identifier clause.
            if export.is_default_export {
                // Always bind the exported expression/declaration so inner references are visited.
                self.bind_node(arena, export.export_clause);

                // Mark the exported declaration (e.g. `function f() {}`) as exported
                // so it isn't flagged as an unused local (TS6133).
                self.mark_exported_symbols(arena, export.export_clause);

                // Synthesize a "default" export symbol for cross-file import resolution.
                // This enables `import X from './file'` to resolve the default export.
                let default_sym_id = self.symbols.alloc(
                    symbol_flags::ALIAS | symbol_flags::EXPORT_VALUE,
                    "default".to_string(),
                );
                if let Some(default_sym) = self.symbols.get_mut(default_sym_id) {
                    default_sym.is_exported = true;
                    default_sym.is_type_only = export_type_only;
                    default_sym.declarations.push(export.export_clause);
                    default_sym.value_declaration = export.export_clause;
                }
                // Add to current scope so it's captured as a module export
                self.current_scope
                    .set("default".to_string(), default_sym_id);

                // Add to file_locals only if we are at the top-level source file scope
                if self.current_scope_id.is_some()
                    && self
                        .scopes
                        .get(self.current_scope_id.0 as usize)
                        .is_some_and(|scope| scope.kind == ContainerKind::SourceFile)
                {
                    self.file_locals.set("default".to_string(), default_sym_id);
                }

                self.node_symbols
                    .insert(export.export_clause.0, default_sym_id);

                // Also mark the underlying local symbol as exported if it exists
                if let Some(name) = Self::get_identifier_name(arena, export.export_clause) {
                    if let Some(sym_id) = self
                        .current_scope
                        .get(name)
                        .or_else(|| self.file_locals.get(name))
                        && let Some(sym) = self.symbols.get_mut(sym_id)
                    {
                        sym.is_exported = true;
                        sym.is_type_only = export_type_only;
                    }
                } else if let Some(clause_node) = arena.get(export.export_clause)
                    && Self::is_declaration(clause_node.kind)
                {
                    self.mark_exported_symbols(arena, export.export_clause);
                }

                return;
            }

            if !export.export_clause.is_none()
                && let Some(clause_node) = arena.get(export.export_clause)
            {
                // Check if it's named exports { foo, bar }
                if let Some(named) = arena.get_named_imports(clause_node) {
                    // Check if this is a re-export: export { foo } from 'module'
                    if export.module_specifier.is_none() {
                        // Regular export { foo, bar } without 'from' clause
                        // This can be either:
                        // 1. Top-level exports from a module
                        // 2. Namespace member re-exports: namespace N { export { x } }
                        //
                        // For namespaces, we need to add the exported symbols to the namespace's exports table
                        // so they can be accessed as N.x

                        // Check if we're inside a namespace
                        let current_namespace_sym_id = self
                            .scope_chain
                            .get(self.current_scope_idx)
                            .and_then(|ctx| {
                                (ctx.container_kind == ContainerKind::Module)
                                    .then_some(ctx.container_node)
                            })
                            .and_then(|container_idx| {
                                self.node_symbols.get(&container_idx.0).copied()
                            });

                        for &spec_idx in &named.elements.nodes {
                            if let Some(spec_node) = arena.get(spec_idx)
                                && let Some(spec) = arena.get_specifier(spec_node)
                            {
                                // Determine if this specifier is type-only
                                // (either from export type { ... } or export { type foo })
                                let spec_type_only = export_type_only || spec.is_type_only;

                                // For export { foo }, property_name is NONE, name is "foo"
                                // For export { foo as bar }, property_name is "foo", name is "bar"
                                let original_name = if spec.property_name.is_none() {
                                    Self::get_identifier_name(arena, spec.name)
                                } else {
                                    Self::get_identifier_name(arena, spec.property_name)
                                };

                                let exported_name = if spec.name.is_none() {
                                    original_name
                                } else {
                                    Self::get_identifier_name(arena, spec.name)
                                };

                                if let (Some(orig), Some(exp)) = (original_name, exported_name) {
                                    // Resolve the original symbol in the current scope
                                    let resolved_sym_id = self
                                        .current_scope
                                        .get(orig)
                                        .or_else(|| self.file_locals.get(orig));

                                    if let Some(sym_id) = resolved_sym_id {
                                        // Mark the original symbol as exported so it appears
                                        // in module_exports for cross-file import resolution
                                        if let Some(orig_sym) = self.symbols.get_mut(sym_id) {
                                            orig_sym.is_exported = true;
                                            if spec_type_only {
                                                orig_sym.is_type_only = true;
                                            }
                                        }

                                        // Create export symbol (EXPORT_VALUE for value exports)
                                        let export_sym_id = self
                                            .symbols
                                            .alloc(symbol_flags::EXPORT_VALUE, exp.to_string());
                                        // Set is_type_only and is_exported on the symbol
                                        if let Some(sym) = self.symbols.get_mut(export_sym_id) {
                                            sym.is_exported = true;
                                            sym.is_type_only = spec_type_only;
                                            // Store the target symbol for re-exports within namespaces
                                            if let Some(ns_sym_id) = current_namespace_sym_id {
                                                // This is a namespace re-export - add to namespace's exports
                                                if let Some(ns_sym) =
                                                    self.symbols.get_mut(ns_sym_id)
                                                {
                                                    let exports =
                                                        ns_sym.exports.get_or_insert_with(|| {
                                                            Box::new(SymbolTable::new())
                                                        });
                                                    exports.set(exp.to_string(), sym_id);
                                                }
                                            }
                                        }
                                        self.node_symbols.insert(spec_idx.0, export_sym_id);

                                        // Add alias to file_locals so it appears in
                                        // module_exports for cross-file import resolution.
                                        // Only needed when the exported name differs from the
                                        // original (i.e., `export { v as v1 }` — v1 needs to be
                                        // findable). When orig == exp, the original symbol is
                                        // already in file_locals and marked exported.
                                        if orig != exp {
                                            self.file_locals.set(exp.to_string(), sym_id);
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        // Get the module name from module_specifier
                        let module_name = if export.module_specifier.is_some() {
                            let idx = export.module_specifier;
                            arena
                                .get(idx)
                                .and_then(|node| arena.get_literal(node))
                                .map(|lit| lit.text.clone())
                        } else {
                            None
                        };

                        if let Some(source_module) = module_name {
                            let current_file = self.debugger.current_file.clone();

                            // Collect all the export mappings first (before mutable borrow)
                            // Also collect node indices and names for creating symbols
                            let mut export_mappings: Vec<(String, Option<String>, NodeIndex)> =
                                Vec::new();
                            for &spec_idx in &named.elements.nodes {
                                if let Some(spec_node) = arena.get(spec_idx)
                                    && let Some(spec) = arena.get_specifier(spec_node)
                                {
                                    // Get the original name (property_name) and exported name (name)
                                    let original_name = if spec.property_name.is_some() {
                                        Self::get_identifier_name(arena, spec.property_name)
                                    } else {
                                        None
                                    };
                                    let exported_name = if spec.name.is_some() {
                                        Self::get_identifier_name(arena, spec.name)
                                    } else {
                                        None
                                    };

                                    if let Some(exported) = exported_name.or(original_name) {
                                        export_mappings.push((
                                            exported.to_string(),
                                            original_name.map(std::string::ToString::to_string),
                                            spec_idx,
                                        ));
                                    }
                                }
                            }

                            // Create symbols for re-export specifiers so they can be tracked
                            // in the compilation cache for incremental invalidation
                            for (exported, _, spec_idx) in &export_mappings {
                                // Use declare_symbol to add to file_locals
                                let sym_id = self.declare_symbol(
                                    exported,
                                    symbol_flags::ALIAS | symbol_flags::EXPORT_VALUE,
                                    *spec_idx,
                                    true, // re-exports are always "exported"
                                );
                                if let Some(sym) = self.symbols.get_mut(sym_id) {
                                    sym.is_exported = true;
                                    sym.is_type_only = export_type_only;
                                }
                                self.node_symbols.insert(spec_idx.0, sym_id);
                            }

                            // Now apply the mutable borrow to insert the mappings
                            let file_reexports = self.reexports.entry(current_file).or_default();
                            for (exported, original, _) in export_mappings {
                                file_reexports.insert(exported, (source_module.clone(), original));
                            }
                        }
                    }
                }
                // Check if it's an exported declaration (function, class, variable, etc.)
                else if Self::is_declaration(clause_node.kind) {
                    // Recursively bind the declaration
                    // This handles: export function foo() {}, export class Bar {}, export const x = 1
                    self.bind_node(arena, export.export_clause);

                    // FIX: Explicitly mark the bound symbol(s) as exported
                    // because the inner declaration node lacks the 'export' modifier
                    self.mark_exported_symbols(arena, export.export_clause);
                }
                // Namespace export: export * as ns from 'mod'
                else if let Some(name) = Self::get_identifier_name(arena, export.export_clause) {
                    let sym_id = self.symbols.alloc(symbol_flags::ALIAS, name.to_string());
                    // Set is_type_only and is_exported for namespace exports
                    if let Some(sym) = self.symbols.get_mut(sym_id) {
                        sym.is_exported = true;
                        sym.is_type_only = export_type_only;
                        sym.declarations.push(export.export_clause);

                        if !export.module_specifier.is_none()
                            && let Some(spec_node) = arena.get(export.module_specifier)
                            && let Some(lit) = arena.get_literal(spec_node)
                        {
                            sym.import_module = Some(lit.text.clone());
                            // Use '*' to indicate it's a namespace export, similar to namespace imports
                            sym.import_name = Some("*".to_string());
                        }
                    }

                    self.current_scope.set(name.to_string(), sym_id);
                    self.node_symbols.insert(export.export_clause.0, sym_id);

                    // Add to module exports
                    let current_file = self.debugger.current_file.clone();
                    self.module_exports
                        .entry(current_file)
                        .or_default()
                        .set(name.to_string(), sym_id);
                }
            }

            // Handle `export * from 'module'` (wildcard re-exports)
            // This is when export_clause is None but module_specifier is not None
            if export.export_clause.is_none() && !export.module_specifier.is_none() {
                let module_name = if export.module_specifier.is_some() {
                    let idx = export.module_specifier;
                    arena
                        .get(idx)
                        .and_then(|node| arena.get_literal(node))
                        .map(|lit| lit.text.clone())
                } else {
                    None
                };

                if let Some(source_module) = module_name {
                    let current_file = self.debugger.current_file.clone();
                    // Add to wildcard_reexports list - a file can have multiple export * from
                    self.wildcard_reexports
                        .entry(current_file)
                        .or_default()
                        .push(source_module);
                }
            }
        }
    }

    /// Check if a node kind is a declaration that should be bound
    pub(crate) const fn is_declaration(kind: u16) -> bool {
        kind == syntax_kind_ext::FUNCTION_DECLARATION
            || kind == syntax_kind_ext::CLASS_DECLARATION
            || kind == syntax_kind_ext::VARIABLE_STATEMENT
            || kind == syntax_kind_ext::INTERFACE_DECLARATION
            || kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
            || kind == syntax_kind_ext::ENUM_DECLARATION
            || kind == syntax_kind_ext::MODULE_DECLARATION
            || kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
    }
}
