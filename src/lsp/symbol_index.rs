//! Symbol Index for O(1) reference lookups.
//!
//! This module provides a global symbol index that enables efficient cross-file
//! reference lookups. Instead of iterating all files linearly (O(N)), the index
//! maps symbol names to their locations across the project.
//!
//! Phase 2.2 of the LSP Performance migration roadmap.

use rustc_hash::{FxHashMap, FxHashSet};

use crate::binder::BinderState;
use crate::lsp::position::Location;

/// Import kind for tracking how symbols are imported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportKind {
    /// Named import: `import { foo } from 'module'`
    Named,
    /// Default import: `import foo from 'module'`
    Default,
    /// Namespace import: `import * as ns from 'module'`
    Namespace,
    /// Side-effect import: `import 'module'`
    SideEffect,
}

/// Information about an imported symbol.
#[derive(Debug, Clone)]
pub struct ImportInfo {
    /// The local name used in the importing file
    pub local_name: String,
    /// The module specifier (e.g., './utils', 'lodash')
    pub source_module: String,
    /// The exported name from the source module (may differ from local_name)
    pub exported_name: String,
    /// The kind of import
    pub kind: ImportKind,
}

/// Global symbol index for O(1) reference lookups.
///
/// This data structure maintains several mappings to enable efficient
/// cross-file operations:
/// - Symbol name to files containing that symbol
/// - Per-file symbol references
/// - Symbol definitions
/// - Module exports and imports
///
/// Note: This is a foundational implementation for Phase 2.2. Full AST-based
/// usage tracking will be added in a future iteration to capture all reference
/// locations beyond just declarations.
#[derive(Debug, Default)]
pub struct SymbolIndex {
    /// Symbol name -> files containing that symbol
    /// Enables quick lookup of which files use a given identifier
    name_to_files: FxHashMap<String, FxHashSet<String>>,

    /// Symbol name -> file -> reference locations
    /// Nested structure to reduce string duplication and improve lookup efficiency
    symbol_refs: FxHashMap<String, FxHashMap<String, Vec<Location>>>,

    /// Symbol name -> definition locations across the project
    /// Maps symbol names to where they are defined
    definitions: FxHashMap<String, Vec<Location>>,

    /// Module path -> exported symbol names
    /// Tracks what symbols each module exports
    exports: FxHashMap<String, FxHashSet<String>>,

    /// File path -> imported symbols with their source info
    /// Tracks import relationships for dependency analysis
    imports: FxHashMap<String, Vec<ImportInfo>>,

    /// Reverse import graph: module path -> files that import it
    /// Enables finding all files affected when a module changes
    importers: FxHashMap<String, FxHashSet<String>>,

    /// Reverse mapping for efficient cleanup: file -> symbols it contains
    /// This enables O(1) removal of a file's contributions from the index
    file_symbols: FxHashMap<String, FxHashSet<String>>,
}

impl SymbolIndex {
    /// Create a new empty symbol index.
    pub fn new() -> Self {
        Self::default()
    }

    /// Find all references to a symbol by name across the project.
    ///
    /// Returns all locations where the symbol is referenced.
    pub fn find_references(&self, name: &str) -> Vec<Location> {
        let mut result = Vec::new();

        // With nested structure, we can directly get all files for this symbol
        if let Some(file_refs) = self.symbol_refs.get(name) {
            for locations in file_refs.values() {
                result.extend(locations.iter().cloned());
            }
        }

        result
    }

    /// Find all definition locations for a symbol name.
    pub fn find_definitions(&self, name: &str) -> Vec<Location> {
        self.definitions.get(name).cloned().unwrap_or_default()
    }

    /// Get all files that contain references to a given symbol.
    pub fn get_files_with_symbol(&self, name: &str) -> Vec<String> {
        self.name_to_files
            .get(name)
            .map(|files| files.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Get all files that import from a given module.
    ///
    /// This is useful for determining which files need re-checking
    /// when a module changes.
    pub fn get_importing_files(&self, module_path: &str) -> Vec<String> {
        self.importers
            .get(module_path)
            .map(|files| files.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Get all exported symbols from a module.
    pub fn get_exports(&self, module_path: &str) -> Vec<String> {
        self.exports
            .get(module_path)
            .map(|names| names.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Get all imports for a file.
    pub fn get_imports(&self, file_name: &str) -> Vec<ImportInfo> {
        self.imports.get(file_name).cloned().unwrap_or_default()
    }

    /// Update the index for a single file.
    ///
    /// This removes old entries and re-indexes the file based on its
    /// current binder state.
    pub fn update_file(&mut self, file_name: &str, binder: &BinderState) {
        self.remove_file(file_name);
        self.index_file(file_name, binder);
    }

    /// Remove a file from the index.
    ///
    /// This cleans up all entries associated with the file using the
    /// file_symbols reverse mapping for O(1) complexity per symbol.
    pub fn remove_file(&mut self, file_name: &str) {
        // Use the reverse mapping for efficient cleanup
        if let Some(symbols) = self.file_symbols.remove(file_name) {
            for sym in &symbols {
                // Remove from name_to_files
                if let Some(files) = self.name_to_files.get_mut(sym) {
                    files.remove(file_name);
                    if files.is_empty() {
                        self.name_to_files.remove(sym);
                    }
                }

                // Remove from symbol_refs (nested structure)
                if let Some(file_refs) = self.symbol_refs.get_mut(sym) {
                    file_refs.remove(file_name);
                    if file_refs.is_empty() {
                        self.symbol_refs.remove(sym);
                    }
                }
            }
        }

        // Remove definitions that were in this file
        for defs in self.definitions.values_mut() {
            defs.retain(|loc| loc.file_path != file_name);
        }
        self.definitions.retain(|_, defs| !defs.is_empty());

        // Remove exports for this file
        self.exports.remove(file_name);

        // Remove imports for this file and update importers
        if let Some(imports) = self.imports.remove(file_name) {
            for import in imports {
                if let Some(importers) = self.importers.get_mut(&import.source_module) {
                    importers.remove(file_name);
                    if importers.is_empty() {
                        self.importers.remove(&import.source_module);
                    }
                }
            }
        }
    }

    /// Index a file during binding.
    ///
    /// Extracts symbol information from the binder state and adds it
    /// to the appropriate index structures. Currently indexes declarations
    /// and exports. Full usage (reference) tracking requires AST walking
    /// which will be added in a future iteration.
    pub fn index_file(&mut self, file_name: &str, binder: &BinderState) {
        let file_name_owned = file_name.to_string();
        let mut file_symbol_names = FxHashSet::default();

        // Index all symbols in file_locals (top-level symbols/declarations)
        for (name, _symbol_id) in binder.file_locals.iter() {
            // Add to name_to_files index
            self.name_to_files
                .entry(name.clone())
                .or_default()
                .insert(file_name_owned.clone());

            // Track in reverse mapping for efficient cleanup
            file_symbol_names.insert(name.clone());
        }

        // Index exports from the module_exports if this file is registered
        if let Some(exports) = binder.module_exports.get(file_name) {
            for (export_name, _symbol_id) in exports.iter() {
                self.exports
                    .entry(file_name_owned.clone())
                    .or_default()
                    .insert(export_name.clone());
            }
        }

        // Index reexports
        if let Some(reexports) = binder.reexports.get(file_name) {
            for (export_name, (source_module, _original_name)) in reexports {
                // Track that this file exports this name
                self.exports
                    .entry(file_name_owned.clone())
                    .or_default()
                    .insert(export_name.clone());

                // Track the import relationship for reexports
                self.importers
                    .entry(source_module.clone())
                    .or_default()
                    .insert(file_name_owned.clone());
            }
        }

        // Index wildcard reexports
        if let Some(wildcards) = binder.wildcard_reexports.get(file_name) {
            for source_module in wildcards {
                self.importers
                    .entry(source_module.clone())
                    .or_default()
                    .insert(file_name_owned.clone());
            }
        }

        // Store the reverse mapping for efficient cleanup
        if !file_symbol_names.is_empty() {
            self.file_symbols.insert(file_name_owned, file_symbol_names);
        }
    }

    /// Add a symbol reference to the index.
    ///
    /// Called when processing references in a file.
    pub fn add_reference(&mut self, file_name: &str, symbol_name: &str, location: Location) {
        // Add to nested symbol_refs structure
        self.symbol_refs
            .entry(symbol_name.to_string())
            .or_default()
            .entry(file_name.to_string())
            .or_default()
            .push(location);

        // Update name_to_files mapping
        self.name_to_files
            .entry(symbol_name.to_string())
            .or_default()
            .insert(file_name.to_string());

        // Update file_symbols reverse mapping for efficient cleanup
        self.file_symbols
            .entry(file_name.to_string())
            .or_default()
            .insert(symbol_name.to_string());
    }

    /// Add a symbol definition to the index.
    pub fn add_definition(&mut self, symbol_name: &str, location: Location) {
        self.definitions
            .entry(symbol_name.to_string())
            .or_default()
            .push(location);
    }

    /// Add an import relationship to the index.
    pub fn add_import(&mut self, file_name: &str, import_info: ImportInfo) {
        // Track the import
        let source_module = import_info.source_module.clone();
        self.imports
            .entry(file_name.to_string())
            .or_default()
            .push(import_info);

        // Update reverse import graph
        self.importers
            .entry(source_module)
            .or_default()
            .insert(file_name.to_string());
    }

    /// Add an export to the index.
    pub fn add_export(&mut self, file_name: &str, export_name: &str) {
        self.exports
            .entry(file_name.to_string())
            .or_default()
            .insert(export_name.to_string());
    }

    /// Check if the index contains any data for a given file.
    pub fn has_file(&self, file_name: &str) -> bool {
        self.exports.contains_key(file_name)
            || self.imports.contains_key(file_name)
            || self
                .name_to_files
                .values()
                .any(|files| files.contains(file_name))
    }

    /// Get statistics about the index for debugging/monitoring.
    pub fn stats(&self) -> SymbolIndexStats {
        SymbolIndexStats {
            unique_symbols: self.name_to_files.len(),
            total_references: self
                .symbol_refs
                .values()
                .flat_map(|file_refs| file_refs.values())
                .map(|v| v.len())
                .sum(),
            total_definitions: self.definitions.values().map(|v| v.len()).sum(),
            files_with_exports: self.exports.len(),
            files_with_imports: self.imports.len(),
            total_import_relationships: self.importers.values().map(|v| v.len()).sum(),
            indexed_files: self.file_symbols.len(),
        }
    }

    /// Iterate over all definition names in the index.
    ///
    /// Returns an iterator of symbol names that have at least one definition.
    /// Useful for workspace symbol search across the entire project.
    pub fn all_definition_names(&self) -> impl Iterator<Item = &str> {
        self.definitions.keys().map(|s| s.as_str())
    }

    /// Clear all data from the index.
    pub fn clear(&mut self) {
        self.name_to_files.clear();
        self.symbol_refs.clear();
        self.definitions.clear();
        self.exports.clear();
        self.imports.clear();
        self.importers.clear();
        self.file_symbols.clear();
    }
}

/// Statistics about the symbol index.
#[derive(Debug, Clone, Default)]
pub struct SymbolIndexStats {
    /// Number of unique symbol names indexed
    pub unique_symbols: usize,
    /// Total number of reference locations
    pub total_references: usize,
    /// Total number of definition locations
    pub total_definitions: usize,
    /// Number of files with exports
    pub files_with_exports: usize,
    /// Number of files with imports
    pub files_with_imports: usize,
    /// Total import relationships (file -> module pairs)
    pub total_import_relationships: usize,
    /// Number of files tracked in the index
    pub indexed_files: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::position::{Position, Range};

    fn make_location(file: &str, line: u32, start_col: u32, end_col: u32) -> Location {
        Location::new(
            file.to_string(),
            Range::new(Position::new(line, start_col), Position::new(line, end_col)),
        )
    }

    #[test]
    fn test_add_and_find_references() {
        let mut index = SymbolIndex::new();

        // Add references for 'foo' in two files
        index.add_reference("a.ts", "foo", make_location("a.ts", 0, 6, 9));
        index.add_reference("a.ts", "foo", make_location("a.ts", 5, 0, 3));
        index.add_reference("b.ts", "foo", make_location("b.ts", 2, 10, 13));

        let refs = index.find_references("foo");
        assert_eq!(refs.len(), 3);

        let files = index.get_files_with_symbol("foo");
        assert_eq!(files.len(), 2);
        assert!(files.contains(&"a.ts".to_string()));
        assert!(files.contains(&"b.ts".to_string()));
    }

    #[test]
    fn test_add_and_find_definitions() {
        let mut index = SymbolIndex::new();

        index.add_definition("MyClass", make_location("types.ts", 10, 0, 7));
        index.add_definition("MyClass", make_location("types.d.ts", 5, 0, 7));

        let defs = index.find_definitions("MyClass");
        assert_eq!(defs.len(), 2);
    }

    #[test]
    fn test_import_tracking() {
        let mut index = SymbolIndex::new();

        index.add_import(
            "consumer.ts",
            ImportInfo {
                local_name: "foo".to_string(),
                source_module: "./utils".to_string(),
                exported_name: "foo".to_string(),
                kind: ImportKind::Named,
            },
        );

        index.add_import(
            "other.ts",
            ImportInfo {
                local_name: "utils".to_string(),
                source_module: "./utils".to_string(),
                exported_name: "*".to_string(),
                kind: ImportKind::Namespace,
            },
        );

        let importers = index.get_importing_files("./utils");
        assert_eq!(importers.len(), 2);

        let imports = index.get_imports("consumer.ts");
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].local_name, "foo");
    }

    #[test]
    fn test_export_tracking() {
        let mut index = SymbolIndex::new();

        index.add_export("utils.ts", "foo");
        index.add_export("utils.ts", "bar");
        index.add_export("types.ts", "MyType");

        let utils_exports = index.get_exports("utils.ts");
        assert_eq!(utils_exports.len(), 2);

        let types_exports = index.get_exports("types.ts");
        assert_eq!(types_exports.len(), 1);
    }

    #[test]
    fn test_remove_file() {
        let mut index = SymbolIndex::new();

        // Add data for multiple files
        index.add_reference("a.ts", "foo", make_location("a.ts", 0, 0, 3));
        index.add_reference("b.ts", "foo", make_location("b.ts", 0, 0, 3));
        index.add_definition("foo", make_location("a.ts", 0, 0, 3));
        index.add_export("a.ts", "foo");
        index.add_import(
            "b.ts",
            ImportInfo {
                local_name: "foo".to_string(),
                source_module: "./a".to_string(),
                exported_name: "foo".to_string(),
                kind: ImportKind::Named,
            },
        );

        // Remove file a.ts
        index.remove_file("a.ts");

        // Verify a.ts data is removed
        let refs = index.find_references("foo");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].file_path, "b.ts");

        let defs = index.find_definitions("foo");
        assert!(defs.is_empty());

        let exports = index.get_exports("a.ts");
        assert!(exports.is_empty());

        // b.ts data should still exist
        let imports = index.get_imports("b.ts");
        assert_eq!(imports.len(), 1);
    }

    #[test]
    fn test_stats() {
        let mut index = SymbolIndex::new();

        index.add_reference("a.ts", "foo", make_location("a.ts", 0, 0, 3));
        index.add_reference("a.ts", "bar", make_location("a.ts", 1, 0, 3));
        index.add_definition("foo", make_location("a.ts", 0, 0, 3));
        index.add_export("a.ts", "foo");
        index.add_import(
            "b.ts",
            ImportInfo {
                local_name: "foo".to_string(),
                source_module: "./a".to_string(),
                exported_name: "foo".to_string(),
                kind: ImportKind::Named,
            },
        );

        let stats = index.stats();
        assert_eq!(stats.unique_symbols, 2); // foo, bar
        assert_eq!(stats.total_references, 2);
        assert_eq!(stats.total_definitions, 1);
        assert_eq!(stats.files_with_exports, 1);
        assert_eq!(stats.files_with_imports, 1);
    }

    #[test]
    fn test_clear() {
        let mut index = SymbolIndex::new();

        index.add_reference("a.ts", "foo", make_location("a.ts", 0, 0, 3));
        index.add_definition("foo", make_location("a.ts", 0, 0, 3));

        index.clear();

        let refs = index.find_references("foo");
        assert!(refs.is_empty());

        let defs = index.find_definitions("foo");
        assert!(defs.is_empty());
    }
}
