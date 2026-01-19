//! Import Tracking and Analysis
//!
//! This module provides data structures and utilities for tracking imports
//! in TypeScript/JavaScript source files, including:
//! - ES6 imports (named, default, namespace)
//! - CommonJS require() calls
//! - Dynamic imports
//! - Type-only imports

use crate::binder::SymbolId;
use crate::parser::NodeIndex;
use rustc_hash::{FxHashMap, FxHashSet};
use std::path::PathBuf;

/// Kind of import statement
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImportKind {
    /// Named import: `import { foo } from 'mod'`
    Named,
    /// Default import: `import foo from 'mod'`
    Default,
    /// Namespace import: `import * as foo from 'mod'`
    Namespace,
    /// Side-effect import: `import 'mod'`
    SideEffect,
    /// Dynamic import: `import('mod')`
    Dynamic,
    /// CommonJS require: `require('mod')`
    Require,
    /// Type-only import: `import type { Foo } from 'mod'`
    TypeOnly,
    /// Type-only named import: `import { type Foo } from 'mod'`
    TypeOnlyNamed,
}

/// Information about a single imported binding
#[derive(Debug, Clone)]
pub struct ImportedBinding {
    /// Local name used in this file
    pub local_name: String,
    /// Exported name from the source module (may differ from local_name)
    pub imported_name: String,
    /// The module specifier
    pub module_specifier: String,
    /// Kind of import
    pub kind: ImportKind,
    /// AST node index of the import declaration
    pub declaration_node: NodeIndex,
    /// AST node index of the binding itself (import specifier)
    pub binding_node: NodeIndex,
    /// Symbol ID for this import binding
    pub symbol_id: SymbolId,
    /// Whether this is a type-only import
    pub is_type_only: bool,
    /// Resolved path (if resolution succeeded)
    pub resolved_path: Option<PathBuf>,
}

/// Information about an import declaration
#[derive(Debug, Clone)]
pub struct ImportDeclaration {
    /// Module specifier string
    pub module_specifier: String,
    /// AST node index
    pub node: NodeIndex,
    /// Individual bindings from this import
    pub bindings: Vec<ImportedBinding>,
    /// Whether the entire import is type-only
    pub is_type_only: bool,
    /// Whether this is a side-effect only import
    pub is_side_effect_only: bool,
    /// Source position (start)
    pub start: u32,
    /// Source position (end)
    pub end: u32,
}

/// Tracks all imports in a source file
#[derive(Debug, Default)]
pub struct ImportTracker {
    /// All import declarations in this file
    pub declarations: Vec<ImportDeclaration>,
    /// Map from local name to imported binding
    pub bindings_by_name: FxHashMap<String, ImportedBinding>,
    /// Map from module specifier to import declarations
    pub declarations_by_specifier: FxHashMap<String, Vec<usize>>,
    /// All module specifiers imported in this file
    pub imported_modules: FxHashSet<String>,
    /// Type-only imports by local name
    pub type_only_imports: FxHashSet<String>,
    /// Default imports by module specifier
    pub default_imports: FxHashMap<String, ImportedBinding>,
    /// Namespace imports by module specifier
    pub namespace_imports: FxHashMap<String, ImportedBinding>,
    /// Dynamic imports
    pub dynamic_imports: Vec<DynamicImport>,
    /// CommonJS require calls
    pub require_calls: Vec<RequireCall>,
}

/// Information about a dynamic import expression
#[derive(Debug, Clone)]
pub struct DynamicImport {
    /// The module specifier (if statically known)
    pub module_specifier: Option<String>,
    /// AST node index
    pub node: NodeIndex,
    /// Whether the specifier is dynamic (non-literal)
    pub is_dynamic_specifier: bool,
    /// Source position
    pub start: u32,
    pub end: u32,
}

/// Information about a require() call
#[derive(Debug, Clone)]
pub struct RequireCall {
    /// The module specifier (if statically known)
    pub module_specifier: Option<String>,
    /// AST node index
    pub node: NodeIndex,
    /// Whether this is a destructuring require: `const { foo } = require('mod')`
    pub is_destructuring: bool,
    /// Local binding name (if simple assignment)
    pub binding_name: Option<String>,
    /// Source position
    pub start: u32,
    pub end: u32,
}

impl ImportTracker {
    /// Create a new import tracker
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an import declaration
    pub fn add_declaration(&mut self, decl: ImportDeclaration) {
        let idx = self.declarations.len();

        // Track by specifier
        self.declarations_by_specifier
            .entry(decl.module_specifier.clone())
            .or_default()
            .push(idx);

        // Add to imported modules set
        self.imported_modules.insert(decl.module_specifier.clone());

        // Process bindings
        for binding in &decl.bindings {
            self.bindings_by_name
                .insert(binding.local_name.clone(), binding.clone());

            if binding.is_type_only {
                self.type_only_imports.insert(binding.local_name.clone());
            }

            match binding.kind {
                ImportKind::Default => {
                    self.default_imports
                        .insert(binding.module_specifier.clone(), binding.clone());
                }
                ImportKind::Namespace => {
                    self.namespace_imports
                        .insert(binding.module_specifier.clone(), binding.clone());
                }
                _ => {}
            }
        }

        self.declarations.push(decl);
    }

    /// Add a dynamic import
    pub fn add_dynamic_import(&mut self, import: DynamicImport) {
        if let Some(ref spec) = import.module_specifier {
            self.imported_modules.insert(spec.clone());
        }
        self.dynamic_imports.push(import);
    }

    /// Add a require call
    pub fn add_require_call(&mut self, require: RequireCall) {
        if let Some(ref spec) = require.module_specifier {
            self.imported_modules.insert(spec.clone());
        }
        self.require_calls.push(require);
    }

    /// Get binding by local name
    pub fn get_binding(&self, name: &str) -> Option<&ImportedBinding> {
        self.bindings_by_name.get(name)
    }

    /// Check if a name is imported
    pub fn is_imported(&self, name: &str) -> bool {
        self.bindings_by_name.contains_key(name)
    }

    /// Check if a name is a type-only import
    pub fn is_type_only_import(&self, name: &str) -> bool {
        self.type_only_imports.contains(name)
    }

    /// Get all imports from a specific module
    pub fn get_imports_from_module(&self, specifier: &str) -> Vec<&ImportedBinding> {
        self.bindings_by_name
            .values()
            .filter(|b| b.module_specifier == specifier)
            .collect()
    }

    /// Get the default import for a module
    pub fn get_default_import(&self, specifier: &str) -> Option<&ImportedBinding> {
        self.default_imports.get(specifier)
    }

    /// Get the namespace import for a module
    pub fn get_namespace_import(&self, specifier: &str) -> Option<&ImportedBinding> {
        self.namespace_imports.get(specifier)
    }

    /// Get all imported module specifiers
    pub fn get_imported_modules(&self) -> impl Iterator<Item = &String> {
        self.imported_modules.iter()
    }

    /// Check if this file imports from a specific module
    pub fn imports_from(&self, specifier: &str) -> bool {
        self.imported_modules.contains(specifier)
    }

    /// Get statistics about imports
    pub fn stats(&self) -> ImportStats {
        let mut stats = ImportStats::default();

        for binding in self.bindings_by_name.values() {
            match binding.kind {
                ImportKind::Named => stats.named_imports += 1,
                ImportKind::Default => stats.default_imports += 1,
                ImportKind::Namespace => stats.namespace_imports += 1,
                ImportKind::SideEffect => stats.side_effect_imports += 1,
                ImportKind::TypeOnly | ImportKind::TypeOnlyNamed => stats.type_only_imports += 1,
                _ => {}
            }
        }

        stats.dynamic_imports = self.dynamic_imports.len();
        stats.require_calls = self.require_calls.len();
        stats.total_modules = self.imported_modules.len();

        stats
    }

    /// Clear all tracked imports
    pub fn clear(&mut self) {
        self.declarations.clear();
        self.bindings_by_name.clear();
        self.declarations_by_specifier.clear();
        self.imported_modules.clear();
        self.type_only_imports.clear();
        self.default_imports.clear();
        self.namespace_imports.clear();
        self.dynamic_imports.clear();
        self.require_calls.clear();
    }
}

/// Statistics about imports in a file
#[derive(Debug, Clone, Default)]
pub struct ImportStats {
    pub named_imports: usize,
    pub default_imports: usize,
    pub namespace_imports: usize,
    pub side_effect_imports: usize,
    pub type_only_imports: usize,
    pub dynamic_imports: usize,
    pub require_calls: usize,
    pub total_modules: usize,
}

/// Builder for creating ImportedBinding
pub struct ImportedBindingBuilder {
    local_name: String,
    imported_name: String,
    module_specifier: String,
    kind: ImportKind,
    declaration_node: NodeIndex,
    binding_node: NodeIndex,
    symbol_id: SymbolId,
    is_type_only: bool,
    resolved_path: Option<PathBuf>,
}

impl ImportedBindingBuilder {
    pub fn new(local_name: impl Into<String>, module_specifier: impl Into<String>) -> Self {
        let local = local_name.into();
        Self {
            imported_name: local.clone(),
            local_name: local,
            module_specifier: module_specifier.into(),
            kind: ImportKind::Named,
            declaration_node: NodeIndex::NONE,
            binding_node: NodeIndex::NONE,
            symbol_id: SymbolId::NONE,
            is_type_only: false,
            resolved_path: None,
        }
    }

    pub fn imported_name(mut self, name: impl Into<String>) -> Self {
        self.imported_name = name.into();
        self
    }

    pub fn kind(mut self, kind: ImportKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn declaration_node(mut self, node: NodeIndex) -> Self {
        self.declaration_node = node;
        self
    }

    pub fn binding_node(mut self, node: NodeIndex) -> Self {
        self.binding_node = node;
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

    pub fn resolved_path(mut self, path: PathBuf) -> Self {
        self.resolved_path = Some(path);
        self
    }

    pub fn build(self) -> ImportedBinding {
        ImportedBinding {
            local_name: self.local_name,
            imported_name: self.imported_name,
            module_specifier: self.module_specifier,
            kind: self.kind,
            declaration_node: self.declaration_node,
            binding_node: self.binding_node,
            symbol_id: self.symbol_id,
            is_type_only: self.is_type_only,
            resolved_path: self.resolved_path,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_import_tracker_basic() {
        let mut tracker = ImportTracker::new();

        let binding = ImportedBindingBuilder::new("useState", "react")
            .kind(ImportKind::Named)
            .imported_name("useState")
            .build();

        let decl = ImportDeclaration {
            module_specifier: "react".to_string(),
            node: NodeIndex::NONE,
            bindings: vec![binding],
            is_type_only: false,
            is_side_effect_only: false,
            start: 0,
            end: 30,
        };

        tracker.add_declaration(decl);

        assert!(tracker.is_imported("useState"));
        assert!(!tracker.is_imported("useEffect"));
        assert!(tracker.imports_from("react"));
    }

    #[test]
    fn test_import_tracker_type_only() {
        let mut tracker = ImportTracker::new();

        let binding = ImportedBindingBuilder::new("Props", "./types")
            .kind(ImportKind::TypeOnly)
            .type_only(true)
            .build();

        let decl = ImportDeclaration {
            module_specifier: "./types".to_string(),
            node: NodeIndex::NONE,
            bindings: vec![binding],
            is_type_only: true,
            is_side_effect_only: false,
            start: 0,
            end: 35,
        };

        tracker.add_declaration(decl);

        assert!(tracker.is_type_only_import("Props"));
    }

    #[test]
    fn test_import_stats() {
        let mut tracker = ImportTracker::new();

        // Add named import
        tracker.add_declaration(ImportDeclaration {
            module_specifier: "lodash".to_string(),
            node: NodeIndex::NONE,
            bindings: vec![ImportedBindingBuilder::new("map", "lodash")
                .kind(ImportKind::Named)
                .build()],
            is_type_only: false,
            is_side_effect_only: false,
            start: 0,
            end: 0,
        });

        // Add default import
        tracker.add_declaration(ImportDeclaration {
            module_specifier: "react".to_string(),
            node: NodeIndex::NONE,
            bindings: vec![ImportedBindingBuilder::new("React", "react")
                .kind(ImportKind::Default)
                .build()],
            is_type_only: false,
            is_side_effect_only: false,
            start: 0,
            end: 0,
        });

        let stats = tracker.stats();
        assert_eq!(stats.named_imports, 1);
        assert_eq!(stats.default_imports, 1);
        assert_eq!(stats.total_modules, 2);
    }

    #[test]
    fn test_get_imports_from_module() {
        let mut tracker = ImportTracker::new();

        tracker.add_declaration(ImportDeclaration {
            module_specifier: "lodash".to_string(),
            node: NodeIndex::NONE,
            bindings: vec![
                ImportedBindingBuilder::new("map", "lodash")
                    .kind(ImportKind::Named)
                    .build(),
                ImportedBindingBuilder::new("filter", "lodash")
                    .kind(ImportKind::Named)
                    .build(),
            ],
            is_type_only: false,
            is_side_effect_only: false,
            start: 0,
            end: 0,
        });

        let lodash_imports = tracker.get_imports_from_module("lodash");
        assert_eq!(lodash_imports.len(), 2);
    }
}
