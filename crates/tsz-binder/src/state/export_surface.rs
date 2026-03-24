//! Structured summary of a file's public API surface.
//!
//! `ExportSurface` captures the export topology of a single file — exported
//! locals, named re-exports, wildcard re-exports, augmentations, and overload
//! grouping — derived from stable binder facts.  Downstream consumers
//! (declaration emitter, LSP, export-signature hasher) query the summary
//! instead of re-walking the AST or rediscovering semantic facts.
//!
//! # Architecture
//!
//! The summary is the **single extraction point** for export information.
//! It does not perform type computation (that belongs to the solver).  It
//! reads binder symbol tables, flags, and structural AST properties to
//! produce a position-independent, reusable snapshot.
//!
//! # Construction
//!
//! ```text
//! BinderState + NodeArena  ──► ExportSurface::from_binder()
//! ```

use crate::{BinderState, SymbolId};
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

// ── Public types ──────────────────────────────────────────────────────

/// A structured summary of a file's public declaration surface.
///
/// Derived entirely from binder facts and AST structure, without
/// performing any type computation.
///
/// # Two export populations
///
/// TypeScript has two sources of "exported symbols":
/// - **Module exports** (`binder.module_exports`): symbols registered as
///   module-level exports (the primary export table).
/// - **File-local exports** (`binder.file_locals` filtered to `is_exported`):
///   symbols that carry the `export` flag but may not appear in
///   `module_exports` (e.g. re-exported aliases).
///
/// These are kept separate to preserve hash-section compatibility with
/// `ExportSignatureInput` (section 0 vs section 5).
#[derive(Clone, Debug, Default)]
pub struct ExportSurface {
    /// Module-level exports from `binder.module_exports`, keyed by public name.
    pub module_exports: FxHashMap<String, ExportedSymbol>,

    /// File-local symbols that are exported but not in `module_exports`.
    pub file_exported_locals: FxHashMap<String, ExportedSymbol>,

    /// Named re-exports (`export { x } from './mod'`).
    pub named_reexports: Vec<NamedReexport>,

    /// Wildcard re-exports (`export * from './mod'`).
    pub wildcard_reexports: Vec<WildcardReexport>,

    /// Global augmentations (`declare global { ... }`).
    pub global_augmentations: Vec<(String, usize)>,

    /// Module augmentations (`declare module "foo" { ... }`).
    pub module_augmentations: Vec<(String, Vec<String>)>,

    /// Top-level function names that have overload signatures.
    /// If present the implementation signature should be skipped during
    /// declaration emit.
    pub overloaded_functions: FxHashSet<String>,

    /// Whether the file has both imports and exports, meaning the
    /// declaration emitter should filter to only emit public API members.
    pub has_public_api_scope: bool,

    /// Whether the file uses `export =` syntax (CommonJS-style).
    pub has_export_equals: bool,

    /// The file's default-export symbol id, if any.
    pub default_export: Option<SymbolId>,
}

/// A single exported local symbol.
#[derive(Clone, Debug)]
pub struct ExportedSymbol {
    pub symbol_id: SymbolId,
    pub flags: u32,
    pub is_type_only: bool,
}

/// A named re-export entry.
#[derive(Clone, Debug)]
pub struct NamedReexport {
    pub export_name: String,
    pub source_module: String,
    pub original_name: Option<String>,
}

/// A wildcard re-export entry.
#[derive(Clone, Debug)]
pub struct WildcardReexport {
    pub source_module: String,
    pub is_type_only: bool,
}

// ── Construction ──────────────────────────────────────────────────────

impl ExportSurface {
    /// Build an `ExportSurface` from binder state and AST.
    ///
    /// `file_name` is the key used in `BinderState::module_exports` (the
    /// canonical file path or module name).  `root_idx` is the source-file
    /// root node used for overload and public-API scope scanning.
    pub fn from_binder(
        binder: &BinderState,
        arena: &NodeArena,
        file_name: &str,
        root_idx: NodeIndex,
    ) -> Self {
        let mut surface = Self::default();

        // 1. Module-level exports (hash section 0 in ExportSignatureInput)
        if let Some(exports) = binder.module_exports.get(file_name) {
            for (name, sym_id) in exports.iter() {
                if let Some(symbol) = binder.get_symbol(*sym_id) {
                    surface.module_exports.insert(
                        name.clone(),
                        ExportedSymbol {
                            symbol_id: *sym_id,
                            flags: symbol.flags,
                            is_type_only: symbol.is_type_only,
                        },
                    );
                }
            }
        }

        // Check for default export
        if let Some(entry) = surface.module_exports.get("default") {
            surface.default_export = Some(entry.symbol_id);
        }

        // 2. Named re-exports
        if let Some(reexports) = binder.reexports.get(file_name) {
            let mut entries: Vec<_> = reexports.iter().collect();
            entries.sort_by_key(|(name, _)| name.as_str());
            for (export_name, (source_module, original_name)) in entries {
                surface.named_reexports.push(NamedReexport {
                    export_name: export_name.clone(),
                    source_module: source_module.clone(),
                    original_name: original_name.clone(),
                });
            }
        }

        // 3. Wildcard re-exports
        if let Some(wildcards) = binder.wildcard_reexports.get(file_name) {
            let type_only_entries = binder.wildcard_reexports_type_only.get(file_name);
            for (i, module) in wildcards.iter().enumerate() {
                let is_type_only = type_only_entries
                    .and_then(|v| v.get(i))
                    .is_some_and(|(_, to)| *to);
                surface.wildcard_reexports.push(WildcardReexport {
                    source_module: module.clone(),
                    is_type_only,
                });
            }
            surface
                .wildcard_reexports
                .sort_by(|a, b| a.source_module.cmp(&b.source_module));
        }

        // 4. Global augmentations
        {
            let mut names: Vec<&String> = binder.global_augmentations.keys().collect();
            names.sort();
            for name in names {
                let count = binder
                    .global_augmentations
                    .get(name.as_str())
                    .map_or(0, Vec::len);
                surface.global_augmentations.push((name.clone(), count));
            }
        }

        // 5. Module augmentations
        {
            let mut modules: Vec<&String> = binder.module_augmentations.keys().collect();
            modules.sort();
            for module in modules {
                let mut aug_names: Vec<String> = binder
                    .module_augmentations
                    .get(module.as_str())
                    .map(|augs| augs.iter().map(|a| a.name.clone()).collect())
                    .unwrap_or_default();
                aug_names.sort();
                surface
                    .module_augmentations
                    .push((module.clone(), aug_names));
            }
        }

        // 6. Export equals
        surface.has_export_equals = binder.modules_with_export_equals.contains(file_name);

        // 7. Overload pre-scan + public-API scope detection from AST
        surface.scan_ast_structure(arena, root_idx);

        // 8. File-local exported symbols (hash section 5 in ExportSignatureInput)
        //    This includes ALL exported file_locals — even those already in
        //    module_exports — because the ExportSignatureInput hash sections
        //    are independently tagged and the original `from_binder()` puts
        //    them in both section 0 and section 5.
        for (name, sym_id) in binder.file_locals.iter() {
            if let Some(symbol) = binder.get_symbol(*sym_id)
                && symbol.is_exported
            {
                surface.file_exported_locals.insert(
                    name.clone(),
                    ExportedSymbol {
                        symbol_id: *sym_id,
                        flags: symbol.flags,
                        is_type_only: symbol.is_type_only,
                    },
                );
            }
        }

        surface
    }

    /// Scan AST for public-API scope detection and overload grouping.
    fn scan_ast_structure(&mut self, arena: &NodeArena, root_idx: NodeIndex) {
        let Some(root_node) = arena.get(root_idx) else {
            return;
        };
        let Some(source_file) = arena.get_source_file(root_node) else {
            return;
        };

        if source_file.is_declaration_file {
            self.has_public_api_scope = false;
            return;
        }

        let mut has_import = false;
        let mut has_export = false;

        for &stmt_idx in &source_file.statements.nodes {
            let Some(node) = arena.get(stmt_idx) else {
                continue;
            };

            // Track imports
            if node.kind == syntax_kind_ext::IMPORT_DECLARATION
                || node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
            {
                has_import = true;
            }

            // Track exports
            match node.kind {
                k if k == syntax_kind_ext::EXPORT_DECLARATION
                    || k == syntax_kind_ext::EXPORT_ASSIGNMENT =>
                {
                    has_export = true;
                }
                k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                    if let Some(func) = arena.get_function(node) {
                        if arena.has_modifier(&func.modifiers, SyntaxKind::ExportKeyword) {
                            has_export = true;
                        }
                        // Also detect overloads
                        if func.body.is_none()
                            && let Some(name_node) = arena.get(func.name)
                            && let Some(ident) = arena.get_identifier(name_node)
                        {
                            self.overloaded_functions.insert(ident.escaped_text.clone());
                        }
                    }
                }
                k if k == syntax_kind_ext::CLASS_DECLARATION => {
                    if let Some(class) = arena.get_class(node)
                        && arena.has_modifier(&class.modifiers, SyntaxKind::ExportKeyword)
                    {
                        has_export = true;
                    }
                }
                k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                    if let Some(iface) = arena.get_interface(node)
                        && arena.has_modifier(&iface.modifiers, SyntaxKind::ExportKeyword)
                    {
                        has_export = true;
                    }
                }
                k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                    if let Some(alias) = arena.get_type_alias(node)
                        && arena.has_modifier(&alias.modifiers, SyntaxKind::ExportKeyword)
                    {
                        has_export = true;
                    }
                }
                k if k == syntax_kind_ext::ENUM_DECLARATION => {
                    if let Some(enum_data) = arena.get_enum(node)
                        && arena.has_modifier(&enum_data.modifiers, SyntaxKind::ExportKeyword)
                    {
                        has_export = true;
                    }
                }
                k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                    if let Some(var_stmt) = arena.get_variable(node)
                        && arena.has_modifier(&var_stmt.modifiers, SyntaxKind::ExportKeyword)
                    {
                        has_export = true;
                    }
                }
                k if k == syntax_kind_ext::MODULE_DECLARATION => {
                    if let Some(module) = arena.get_module(node)
                        && arena.has_modifier(&module.modifiers, SyntaxKind::ExportKeyword)
                    {
                        has_export = true;
                    }
                }
                _ => {}
            }
        }

        // Match tsc/emitter behavior: filter to public API when the file
        // has ANY module indicator (import or export).
        self.has_public_api_scope = has_import || has_export;
    }

    // ── Query methods ─────────────────────────────────────────────────

    /// Check if a name is directly exported from this file (in either
    /// `module_exports` or `file_exported_locals`).
    pub fn is_exported(&self, name: &str) -> bool {
        self.module_exports.contains_key(name) || self.file_exported_locals.contains_key(name)
    }

    /// Check if a name is a type-only export.
    pub fn is_type_only_export(&self, name: &str) -> bool {
        self.module_exports
            .get(name)
            .or_else(|| self.file_exported_locals.get(name))
            .is_some_and(|e| e.is_type_only)
    }

    /// Get all exported names (sorted, from both populations).
    pub fn exported_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self
            .module_exports
            .keys()
            .chain(self.file_exported_locals.keys())
            .map(String::as_str)
            .collect();
        names.sort();
        names.dedup();
        names
    }

    /// Check whether a given function name has overload signatures.
    pub fn has_overloads(&self, function_name: &str) -> bool {
        self.overloaded_functions.contains(function_name)
    }

    /// Get the `SymbolId` for a specific export name.
    pub fn symbol_for_export(&self, name: &str) -> Option<SymbolId> {
        self.module_exports
            .get(name)
            .or_else(|| self.file_exported_locals.get(name))
            .map(|e| e.symbol_id)
    }

    /// Total number of unique public API entries (exports + re-exports).
    ///
    /// Note: `module_exports` and `file_exported_locals` may overlap
    /// (the same symbol can appear in both), so we count the union.
    pub fn public_api_size(&self) -> usize {
        let unique_locals = self
            .file_exported_locals
            .keys()
            .filter(|k| !self.module_exports.contains_key(k.as_str()))
            .count();
        self.module_exports.len()
            + unique_locals
            + self.named_reexports.len()
            + self.wildcard_reexports.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_surface() {
        let surface = ExportSurface::default();
        assert!(!surface.has_public_api_scope);
        assert!(!surface.has_export_equals);
        assert_eq!(surface.public_api_size(), 0);
        assert!(surface.exported_names().is_empty());
    }
}
