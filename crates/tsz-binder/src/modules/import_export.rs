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

            // Record the import source for dependency tracking
            if let Some(ref spec) = module_specifier {
                self.file_import_sources.push(spec.clone());
            }

            if let Some(clause_node) = arena.get(import.import_clause)
                && let Some(clause) = arena.get_import_clause(clause_node)
            {
                let clause_type_only = clause.is_type_only;
                // Default import
                if clause.name.is_some()
                    && let Some(name) = Self::get_identifier_name(arena, clause.name)
                {
                    // Use import_clause node as the declaration node
                    let sym_id = self.declare_symbol(
                        arena,
                        name,
                        symbol_flags::ALIAS,
                        import.import_clause,
                        false,
                    );
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
                if clause.named_bindings.is_some()
                    && let Some(bindings_node) = arena.get(clause.named_bindings)
                {
                    if bindings_node.kind == SyntaxKind::Identifier as u16 {
                        if let Some(name) = Self::get_identifier_name(arena, clause.named_bindings)
                        {
                            let sym_id = self.declare_symbol(
                                arena,
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
                        if named.name.is_some()
                            && let Some(name) = Self::get_identifier_name(arena, named.name)
                        {
                            // Use named_bindings (NamespaceImport) as the declaration node
                            let sym_id = self.declare_symbol(
                                arena,
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
                                    // Namespace import: mark as `*` so type display
                                    // renders `typeof import("mod")` instead of `typeof ns`.
                                    sym.import_name = Some("*".to_string());
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
                                        arena,
                                        name,
                                        symbol_flags::ALIAS,
                                        spec_idx,
                                        false,
                                    );

                                    // Get property name before mutable borrow to avoid borrow checker error
                                    let prop_name =
                                        if spec.name.is_some() && spec.property_name.is_some() {
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

                // Record the import source for dependency tracking
                if let Some(ref spec) = module_specifier {
                    self.file_import_sources.push(spec.clone());
                }

                // Create symbol with ALIAS flag
                let sym_id =
                    self.declare_symbol(arena, name, symbol_flags::ALIAS, idx, is_exported);

                if let Some(sym) = self.symbols.get_mut(sym_id) {
                    let span = arena.get(idx).map(|node| (node.pos, node.end));
                    // If this is the first value declaration, or if we're merging compatible
                    // declarations where this should be the value declaration.
                    // For aliases, we generally track the first one as value decl,
                    // but for duplicates we just want to ensure it's recorded.
                    if sym.value_declaration.is_none() {
                        sym.set_value_declaration(idx, span);
                    }

                    // Track module for cross-file resolution and unresolved import detection
                    if let Some(ref specifier) = module_specifier {
                        // If multiple declarations have different specifiers, we might overwrite here.
                        // For valid merges (if any), this logic might need refinement,
                        // but for duplicates it doesn't matter much which one wins for resolution
                        // as long as we report the error.
                        sym.import_module = Some(specifier.clone());
                    }

                    // Propagate type-only flag from `import type X = require('...')`
                    if import.is_type_only {
                        sym.is_type_only = true;
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
    ///
    /// Also updates the `semantic_defs` entry for the symbol to reflect the
    /// export visibility, since `record_semantic_def` may have fired before the
    /// `ExportDeclaration` wrapper was processed.
    pub(crate) fn mark_exported_symbols(&mut self, arena: &NodeArena, idx: NodeIndex) {
        // 1. Try direct symbol lookup (Function, Class, Enum, Module, Interface, TypeAlias)
        if let Some(&sym_id) = self.node_symbols.get(&idx.0) {
            if let Some(sym) = self.symbols.get_mut(sym_id) {
                sym.is_exported = true;
            }
            // Propagate to semantic_defs (record_semantic_def was called before
            // the ExportDeclaration wrapper set is_exported on the symbol).
            if let Some(entry) = self.semantic_defs.get_mut(&sym_id) {
                entry.is_exported = true;
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
                        if let Some(&sym_id) = self.node_symbols.get(&decl_idx.0) {
                            if let Some(sym) = self.symbols.get_mut(sym_id) {
                                sym.is_exported = true;
                            }
                            if let Some(entry) = self.semantic_defs.get_mut(&sym_id) {
                                entry.is_exported = true;
                            }
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

            // Record export-from module specifier for dependency tracking
            if export.module_specifier.is_some()
                && let Some(spec_node) = arena.get(export.module_specifier)
                && let Some(lit) = arena.get_literal(spec_node)
            {
                self.file_import_sources.push(lit.text.clone());
            }

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
                    let span = arena
                        .get(export.export_clause)
                        .map(|node| (node.pos, node.end));
                    default_sym.is_exported = true;
                    default_sym.is_type_only = export_type_only;
                    default_sym.add_declaration(export.export_clause, span);
                    default_sym.set_value_declaration(export.export_clause, span);
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

                // Also mark the underlying local symbol as exported if it exists.
                // For `export default class Foo`, get_identifier_name returns None
                // (ClassDeclaration is not an Identifier node), so we also try
                // looking up the symbol via get_declaration_name.
                let local_name = Self::get_identifier_name(arena, export.export_clause)
                    .or_else(|| Self::get_declaration_name(arena, export.export_clause));
                if let Some(name) = local_name {
                    if let Some(sym_id) = self
                        .current_scope
                        .get(name)
                        .or_else(|| self.file_locals.get(name))
                        && let Some(sym) = self.symbols.get_mut(sym_id)
                    {
                        sym.is_exported = true;
                        // Set EXPORT_VALUE on the local symbol when the declaration
                        // itself has `export default` (e.g., `export default class Foo`).
                        // This distinguishes it from `function f() {} export default f;`
                        // where `f` is a local name re-exported via a separate statement.
                        if let Some(clause_node) = arena.get(export.export_clause)
                            && Self::is_declaration(clause_node.kind)
                        {
                            sym.flags |= symbol_flags::EXPORT_VALUE;
                        }
                        // Only escalate is_type_only to true; never downgrade.
                        if export_type_only {
                            sym.is_type_only = true;
                        }
                    }
                } else if let Some(clause_node) = arena.get(export.export_clause)
                    && Self::is_declaration(clause_node.kind)
                {
                    self.mark_exported_symbols(arena, export.export_clause);
                }

                return;
            }

            if export.export_clause.is_some()
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
                                        // Mark the original symbol as exported.
                                        //
                                        // For is_type_only: only SET it when this is the
                                        // first export AND it's type-only. Never CLEAR it,
                                        // because the flag may come from `import type` and
                                        // must be preserved for re-export chains.
                                        if let Some(orig_sym) = self.symbols.get_mut(sym_id) {
                                            if spec_type_only && !orig_sym.is_exported {
                                                orig_sym.is_type_only = true;
                                            }
                                            orig_sym.is_exported = true;
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
                                            // When a type-only export specifier renames a
                                            // symbol that is NOT itself type-only, clone
                                            // the symbol so module_exports gets the correct
                                            // is_type_only per export name. Example:
                                            //   export { type };           // value export
                                            //   export { type type as foo }; // type-only
                                            // Without cloning, both "type" and "foo" would
                                            // share the same symbol with is_type_only=false.
                                            let orig_is_type_only = self
                                                .symbols
                                                .get(sym_id)
                                                .is_some_and(|s| s.is_type_only);
                                            if spec_type_only && !orig_is_type_only {
                                                let clone_id = {
                                                    let src =
                                                        self.symbols.get(sym_id).cloned().expect(
                                                            "symbol exists for resolved sym_id",
                                                        );
                                                    self.symbols.alloc_from(&src)
                                                };
                                                if let Some(clone_sym) =
                                                    self.symbols.get_mut(clone_id)
                                                {
                                                    clone_sym.is_type_only = true;
                                                    clone_sym.is_exported = true;
                                                }
                                                self.file_locals.set(exp.to_string(), clone_id);
                                            } else {
                                                self.file_locals.set(exp.to_string(), sym_id);
                                            }
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
                            for (exported, original, spec_idx) in &export_mappings {
                                // Use declare_symbol to add to file_locals
                                let sym_id = self.declare_symbol(
                                    arena,
                                    exported,
                                    symbol_flags::ALIAS | symbol_flags::EXPORT_VALUE,
                                    *spec_idx,
                                    true, // re-exports are always "exported"
                                );
                                if let Some(sym) = self.symbols.get_mut(sym_id) {
                                    sym.is_exported = true;
                                    sym.is_type_only = export_type_only;
                                    sym.import_module = Some(source_module.clone());
                                    sym.import_name = Some(
                                        original
                                            .as_ref()
                                            .cloned()
                                            .unwrap_or_else(|| exported.clone()),
                                    );
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
                // Handle `export import { ... } from "..."` — the parser wraps the
                // malformed import inside an ExportDeclaration with the ImportDeclaration
                // as export_clause.  We still need to bind the import's named symbols so
                // that references to them don't produce false TS2304 errors.
                else if clause_node.kind == syntax_kind_ext::IMPORT_DECLARATION {
                    self.bind_import_declaration(arena, clause_node, export.export_clause);
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
                // Namespace export: export * as ns from 'mod'  OR  UMD: export as namespace Foo
                else if let Some(name) = Self::get_identifier_name(arena, export.export_clause) {
                    let is_umd = export.module_specifier.is_none()
                        && node.kind == syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION;

                    let sym_id = self.symbols.alloc(symbol_flags::ALIAS, name.to_string());
                    if let Some(sym) = self.symbols.get_mut(sym_id) {
                        let span = arena
                            .get(export.export_clause)
                            .map(|node| (node.pos, node.end));
                        sym.is_exported = true;
                        sym.is_type_only = export_type_only;
                        sym.add_declaration(export.export_clause, span);
                        sym.is_umd_export = is_umd;

                        if is_umd {
                            // `export as namespace Foo` creates a global alias to the
                            // current file's external-module export surface.
                            sym.import_module = Some(self.debugger.current_file.clone());
                            sym.import_name = Some("*".to_string());
                        } else if export.module_specifier.is_some()
                            && let Some(spec_node) = arena.get(export.module_specifier)
                            && let Some(lit) = arena.get_literal(spec_node)
                        {
                            sym.import_module = Some(lit.text.clone());
                            // Use '*' to indicate it's a namespace export, similar to namespace imports
                            sym.import_name = Some("*".to_string());
                        }
                    }

                    // `export * as ns from "./mod"` creates an exported name but does NOT create
                    // a same-file lexical binding for `ns`. Keep it out of current_scope so local
                    // references still resolve like tsc, while cross-file consumers use
                    // `module_exports`. UMD `export as namespace Foo` is different: it does create
                    // a global binding, so keep the old current_scope/file_locals behavior there.
                    //
                    // If a TYPE_ALIAS already exists, preserve it as the local/type binding and
                    // record the ALIAS partner for value/namespace resolution.
                    let existing_type_alias_id = self.current_scope.get(name).filter(|id| {
                        self.symbols
                            .get(*id)
                            .is_some_and(|s| s.flags & symbol_flags::TYPE_ALIAS != 0)
                    });
                    if let Some(type_alias_id) = existing_type_alias_id {
                        self.alias_partners.insert(type_alias_id, sym_id);
                    } else if is_umd {
                        self.current_scope.set(name.to_string(), sym_id);
                    }
                    self.node_symbols.insert(export.export_clause.0, sym_id);

                    if is_umd {
                        // UMD namespace exports register a global name.
                        // Add to file_locals + root scope so the name is visible cross-file.
                        self.file_locals.set(name.to_string(), sym_id);
                        if let Some(root_scope) = self.scopes.first_mut()
                            && !root_scope.table.has(name)
                        {
                            root_scope.table.set(name.to_string(), sym_id);
                        }
                    } else {
                        // Regular namespace re-export — add to module exports
                        let current_file = self.debugger.current_file.clone();
                        self.module_exports
                            .entry(current_file)
                            .or_default()
                            .set(name.to_string(), sym_id);
                    }
                }
            }

            // Handle `export * from 'module'` (wildcard re-exports)
            // This is when export_clause is None but module_specifier is not None
            if export.export_clause.is_none() && export.module_specifier.is_some() {
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
                        .entry(current_file.clone())
                        .or_default()
                        .push(source_module.clone());
                    self.wildcard_reexports_type_only
                        .entry(current_file)
                        .or_default()
                        .push((source_module, export_type_only));
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
