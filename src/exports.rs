//! Export Tracking and Analysis
//!
//! This module provides data structures and utilities for tracking exports
//! in TypeScript/JavaScript source files, including:
//! - Named exports
//! - Default exports
//! - Re-exports (including `export * from`)
//! - CommonJS module.exports

use crate::binder::SymbolId;
use crate::parser::NodeIndex;
use rustc_hash::{FxHashMap, FxHashSet};

/// Kind of export statement
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExportKind {
    /// Named export: `export { foo }`
    Named,
    /// Default export: `export default foo`
    Default,
    /// Named export from declaration: `export function foo() {}`
    Declaration,
    /// Re-export: `export { foo } from 'mod'`
    ReExport,
    /// Namespace re-export: `export * from 'mod'`
    NamespaceReExport,
    /// Namespace re-export with alias: `export * as ns from 'mod'`
    NamespaceReExportAs,
    /// Type-only export: `export type { Foo }`
    TypeOnly,
    /// CommonJS: `module.exports = ...`
    CommonJSDefault,
    /// CommonJS: `exports.foo = ...`
    CommonJSNamed,
}

/// Information about a single exported binding
#[derive(Debug, Clone)]
pub struct ExportedBinding {
    /// Exported name (name visible to importers)
    pub exported_name: String,
    /// Local name in this file (may differ from exported_name)
    pub local_name: String,
    /// Kind of export
    pub kind: ExportKind,
    /// AST node index of the export declaration
    pub declaration_node: NodeIndex,
    /// AST node index of the local declaration (if any)
    pub local_declaration_node: NodeIndex,
    /// Symbol ID for the exported binding
    pub symbol_id: SymbolId,
    /// Whether this is a type-only export
    pub is_type_only: bool,
    /// For re-exports, the source module specifier
    pub source_module: Option<String>,
    /// For re-exports, the original name in the source module
    pub original_name: Option<String>,
}

/// Information about an export declaration
#[derive(Debug, Clone)]
pub struct ExportDeclaration {
    /// AST node index
    pub node: NodeIndex,
    /// Individual bindings from this export
    pub bindings: Vec<ExportedBinding>,
    /// Whether the entire export is type-only
    pub is_type_only: bool,
    /// For re-exports, the source module
    pub from_module: Option<String>,
    /// Source position (start)
    pub start: u32,
    /// Source position (end)
    pub end: u32,
}

/// Namespace re-export: `export * from 'module'` or `export * as ns from 'module'`
#[derive(Debug, Clone)]
pub struct NamespaceReExport {
    /// Source module specifier
    pub module_specifier: String,
    /// Alias name (for `export * as ns from 'mod'`)
    pub alias: Option<String>,
    /// AST node index
    pub node: NodeIndex,
    /// Source position
    pub start: u32,
    pub end: u32,
}

/// Tracks all exports in a source file
#[derive(Debug, Default)]
pub struct ExportTracker {
    /// All export declarations in this file
    pub declarations: Vec<ExportDeclaration>,
    /// Map from exported name to binding
    pub bindings_by_name: FxHashMap<String, ExportedBinding>,
    /// Default export (if any)
    pub default_export: Option<ExportedBinding>,
    /// Type-only exports by name
    pub type_only_exports: FxHashSet<String>,
    /// Namespace re-exports (`export * from`)
    pub namespace_reexports: Vec<NamespaceReExport>,
    /// Named re-exports by exported name
    pub reexports: FxHashMap<String, ExportedBinding>,
    /// All modules this file re-exports from
    pub reexport_sources: FxHashSet<String>,
    /// CommonJS exports
    pub commonjs_exports: Vec<CommonJSExport>,
    /// Whether this file has a default export
    pub has_default_export: bool,
    /// Whether this file uses CommonJS exports
    pub has_commonjs_exports: bool,
}

/// CommonJS export information
#[derive(Debug, Clone)]
pub struct CommonJSExport {
    /// Export kind (default or named)
    pub kind: ExportKind,
    /// Property name (for `exports.foo = ...`)
    pub property_name: Option<String>,
    /// AST node index
    pub node: NodeIndex,
    /// Source position
    pub start: u32,
    pub end: u32,
}

impl ExportTracker {
    /// Create a new export tracker
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an export declaration
    pub fn add_declaration(&mut self, decl: ExportDeclaration) {
        // Track re-export sources
        if let Some(ref from) = decl.from_module {
            self.reexport_sources.insert(from.clone());
        }

        // Process bindings
        for binding in &decl.bindings {
            match binding.kind {
                ExportKind::Default => {
                    self.default_export = Some(binding.clone());
                    self.has_default_export = true;
                }
                ExportKind::ReExport | ExportKind::NamespaceReExportAs => {
                    self.reexports
                        .insert(binding.exported_name.clone(), binding.clone());
                }
                _ => {
                    self.bindings_by_name
                        .insert(binding.exported_name.clone(), binding.clone());
                }
            }

            if binding.is_type_only {
                self.type_only_exports.insert(binding.exported_name.clone());
            }
        }

        self.declarations.push(decl);
    }

    /// Add a namespace re-export
    pub fn add_namespace_reexport(&mut self, reexport: NamespaceReExport) {
        self.reexport_sources
            .insert(reexport.module_specifier.clone());

        if let Some(ref alias) = reexport.alias {
            // `export * as ns from 'mod'` creates a named export
            let binding = ExportedBinding {
                exported_name: alias.clone(),
                local_name: alias.clone(),
                kind: ExportKind::NamespaceReExportAs,
                declaration_node: reexport.node,
                local_declaration_node: NodeIndex::NONE,
                symbol_id: SymbolId::NONE,
                is_type_only: false,
                source_module: Some(reexport.module_specifier.clone()),
                original_name: None,
            };
            self.bindings_by_name.insert(alias.clone(), binding);
        }

        self.namespace_reexports.push(reexport);
    }

    /// Add a CommonJS export
    pub fn add_commonjs_export(&mut self, export: CommonJSExport) {
        self.has_commonjs_exports = true;

        if matches!(export.kind, ExportKind::CommonJSDefault) {
            self.has_default_export = true;
        }

        self.commonjs_exports.push(export);
    }

    /// Get an exported binding by name
    pub fn get_export(&self, name: &str) -> Option<&ExportedBinding> {
        if name == "default" {
            return self.default_export.as_ref();
        }

        self.bindings_by_name
            .get(name)
            .or_else(|| self.reexports.get(name))
    }

    /// Check if a name is exported
    pub fn is_exported(&self, name: &str) -> bool {
        if name == "default" {
            return self.has_default_export;
        }

        self.bindings_by_name.contains_key(name) || self.reexports.contains_key(name)
    }

    /// Check if a name is a type-only export
    pub fn is_type_only_export(&self, name: &str) -> bool {
        self.type_only_exports.contains(name)
    }

    /// Get all exported names
    pub fn get_exported_names(&self) -> impl Iterator<Item = &String> {
        let named = self.bindings_by_name.keys();
        let reexported = self.reexports.keys();
        named.chain(reexported)
    }

    /// Get all direct exports (excluding re-exports)
    pub fn get_direct_exports(&self) -> impl Iterator<Item = &ExportedBinding> {
        self.bindings_by_name.values()
    }

    /// Get all re-exports
    pub fn get_reexports(&self) -> impl Iterator<Item = &ExportedBinding> {
        self.reexports.values()
    }

    /// Check if this file re-exports from a module
    pub fn reexports_from(&self, specifier: &str) -> bool {
        self.reexport_sources.contains(specifier)
    }

    /// Get statistics about exports
    pub fn stats(&self) -> ExportStats {
        let mut stats = ExportStats::default();

        for binding in self.bindings_by_name.values() {
            match binding.kind {
                ExportKind::Named | ExportKind::Declaration => stats.named_exports += 1,
                ExportKind::Default => stats.default_exports += 1,
                ExportKind::TypeOnly => stats.type_only_exports += 1,
                _ => {}
            }
        }

        if self.has_default_export {
            stats.default_exports = 1;
        }

        stats.reexports = self.reexports.len();
        stats.namespace_reexports = self.namespace_reexports.len();
        stats.commonjs_exports = self.commonjs_exports.len();
        stats.reexport_sources = self.reexport_sources.len();

        stats
    }

    /// Clear all tracked exports
    pub fn clear(&mut self) {
        self.declarations.clear();
        self.bindings_by_name.clear();
        self.default_export = None;
        self.type_only_exports.clear();
        self.namespace_reexports.clear();
        self.reexports.clear();
        self.reexport_sources.clear();
        self.commonjs_exports.clear();
        self.has_default_export = false;
        self.has_commonjs_exports = false;
    }

    /// Resolve an export considering re-exports
    ///
    /// This follows re-export chains to find the original export.
    /// Returns the module specifier and export name to look up.
    pub fn resolve_export(&self, name: &str) -> ExportResolution {
        // Check direct exports first
        if let Some(binding) = self.bindings_by_name.get(name)
            && binding.source_module.is_none() {
                return ExportResolution::Direct(binding.clone());
            }

        // Check named re-exports
        if let Some(binding) = self.reexports.get(name)
            && let Some(ref source) = binding.source_module {
                let original = binding.original_name.as_deref().unwrap_or(name);
                return ExportResolution::ReExport {
                    source_module: source.clone(),
                    original_name: original.to_string(),
                };
            }

        // Check namespace re-exports (export * from)
        if !self.namespace_reexports.is_empty() {
            return ExportResolution::PossibleNamespaceReExport {
                sources: self
                    .namespace_reexports
                    .iter()
                    .filter(|r| r.alias.is_none())
                    .map(|r| r.module_specifier.clone())
                    .collect(),
                name: name.to_string(),
            };
        }

        ExportResolution::NotFound
    }
}

/// Result of resolving an export
#[derive(Debug, Clone)]
pub enum ExportResolution {
    /// Export is defined directly in this file
    Direct(ExportedBinding),
    /// Export is re-exported from another module
    ReExport {
        source_module: String,
        original_name: String,
    },
    /// Export might come from a namespace re-export
    PossibleNamespaceReExport { sources: Vec<String>, name: String },
    /// Export not found
    NotFound,
}

/// Statistics about exports in a file
#[derive(Debug, Clone, Default)]
pub struct ExportStats {
    pub named_exports: usize,
    pub default_exports: usize,
    pub type_only_exports: usize,
    pub reexports: usize,
    pub namespace_reexports: usize,
    pub commonjs_exports: usize,
    pub reexport_sources: usize,
}

/// Builder for creating ExportedBinding
pub struct ExportedBindingBuilder {
    exported_name: String,
    local_name: String,
    kind: ExportKind,
    declaration_node: NodeIndex,
    local_declaration_node: NodeIndex,
    symbol_id: SymbolId,
    is_type_only: bool,
    source_module: Option<String>,
    original_name: Option<String>,
}

impl ExportedBindingBuilder {
    pub fn new(exported_name: impl Into<String>) -> Self {
        let name = exported_name.into();
        Self {
            local_name: name.clone(),
            exported_name: name,
            kind: ExportKind::Named,
            declaration_node: NodeIndex::NONE,
            local_declaration_node: NodeIndex::NONE,
            symbol_id: SymbolId::NONE,
            is_type_only: false,
            source_module: None,
            original_name: None,
        }
    }

    pub fn local_name(mut self, name: impl Into<String>) -> Self {
        self.local_name = name.into();
        self
    }

    pub fn kind(mut self, kind: ExportKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn declaration_node(mut self, node: NodeIndex) -> Self {
        self.declaration_node = node;
        self
    }

    pub fn local_declaration_node(mut self, node: NodeIndex) -> Self {
        self.local_declaration_node = node;
        self
    }

    pub fn symbol_id(mut self, id: SymbolId) -> Self {
        self.symbol_id = id;
        self
    }

    pub fn type_only(mut self, is_type_only: bool) -> Self {
        self.is_type_only = is_type_only;
        self
    }

    pub fn source_module(mut self, module: impl Into<String>) -> Self {
        self.source_module = Some(module.into());
        self
    }

    pub fn original_name(mut self, name: impl Into<String>) -> Self {
        self.original_name = Some(name.into());
        self
    }

    pub fn build(self) -> ExportedBinding {
        ExportedBinding {
            exported_name: self.exported_name,
            local_name: self.local_name,
            kind: self.kind,
            declaration_node: self.declaration_node,
            local_declaration_node: self.local_declaration_node,
            symbol_id: self.symbol_id,
            is_type_only: self.is_type_only,
            source_module: self.source_module,
            original_name: self.original_name,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_export_tracker_basic() {
        let mut tracker = ExportTracker::new();

        let binding = ExportedBindingBuilder::new("useState")
            .kind(ExportKind::Named)
            .build();

        let decl = ExportDeclaration {
            node: NodeIndex::NONE,
            bindings: vec![binding],
            is_type_only: false,
            from_module: None,
            start: 0,
            end: 25,
        };

        tracker.add_declaration(decl);

        assert!(tracker.is_exported("useState"));
        assert!(!tracker.is_exported("useEffect"));
    }

    #[test]
    fn test_export_tracker_default() {
        let mut tracker = ExportTracker::new();

        let binding = ExportedBindingBuilder::new("default")
            .local_name("MyComponent")
            .kind(ExportKind::Default)
            .build();

        let decl = ExportDeclaration {
            node: NodeIndex::NONE,
            bindings: vec![binding],
            is_type_only: false,
            from_module: None,
            start: 0,
            end: 30,
        };

        tracker.add_declaration(decl);

        assert!(tracker.has_default_export);
        assert!(tracker.is_exported("default"));
    }

    #[test]
    fn test_export_tracker_reexport() {
        let mut tracker = ExportTracker::new();

        let binding = ExportedBindingBuilder::new("map")
            .kind(ExportKind::ReExport)
            .source_module("lodash")
            .build();

        let decl = ExportDeclaration {
            node: NodeIndex::NONE,
            bindings: vec![binding],
            is_type_only: false,
            from_module: Some("lodash".to_string()),
            start: 0,
            end: 30,
        };

        tracker.add_declaration(decl);

        assert!(tracker.reexports_from("lodash"));
        assert!(tracker.is_exported("map"));
    }

    #[test]
    fn test_namespace_reexport() {
        let mut tracker = ExportTracker::new();

        tracker.add_namespace_reexport(NamespaceReExport {
            module_specifier: "./utils".to_string(),
            alias: None,
            node: NodeIndex::NONE,
            start: 0,
            end: 25,
        });

        assert!(tracker.reexports_from("./utils"));
        assert_eq!(tracker.namespace_reexports.len(), 1);
    }

    #[test]
    fn test_resolve_export() {
        let mut tracker = ExportTracker::new();

        // Direct export
        tracker.add_declaration(ExportDeclaration {
            node: NodeIndex::NONE,
            bindings: vec![
                ExportedBindingBuilder::new("foo")
                    .kind(ExportKind::Named)
                    .build(),
            ],
            is_type_only: false,
            from_module: None,
            start: 0,
            end: 0,
        });

        // Re-export
        tracker.add_declaration(ExportDeclaration {
            node: NodeIndex::NONE,
            bindings: vec![
                ExportedBindingBuilder::new("bar")
                    .kind(ExportKind::ReExport)
                    .source_module("./other")
                    .build(),
            ],
            is_type_only: false,
            from_module: Some("./other".to_string()),
            start: 0,
            end: 0,
        });

        match tracker.resolve_export("foo") {
            ExportResolution::Direct(b) => assert_eq!(b.exported_name, "foo"),
            _ => panic!("Expected direct export"),
        }

        match tracker.resolve_export("bar") {
            ExportResolution::ReExport { source_module, .. } => {
                assert_eq!(source_module, "./other")
            }
            _ => panic!("Expected re-export"),
        }
    }

    #[test]
    fn test_export_stats() {
        let mut tracker = ExportTracker::new();

        tracker.add_declaration(ExportDeclaration {
            node: NodeIndex::NONE,
            bindings: vec![
                ExportedBindingBuilder::new("a")
                    .kind(ExportKind::Named)
                    .build(),
                ExportedBindingBuilder::new("b")
                    .kind(ExportKind::Named)
                    .build(),
            ],
            is_type_only: false,
            from_module: None,
            start: 0,
            end: 0,
        });

        tracker.add_namespace_reexport(NamespaceReExport {
            module_specifier: "./other".to_string(),
            alias: None,
            node: NodeIndex::NONE,
            start: 0,
            end: 0,
        });

        let stats = tracker.stats();
        assert_eq!(stats.named_exports, 2);
        assert_eq!(stats.namespace_reexports, 1);
    }
}
