//! Symbol Index for O(1) reference lookups.
//!
//! This module provides a global symbol index that enables efficient cross-file
//! reference lookups. Instead of iterating all files linearly (O(N)), the index
//! maps symbol names to their locations across the project.
//!
//! Phase 2.2 of the LSP Performance migration roadmap.

use rustc_hash::{FxHashMap, FxHashSet};

use crate::document_symbols::SymbolKind;
use tsz_binder::{BinderState, symbol_flags};
use tsz_common::position::Location;
use tsz_parser::NodeArena;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner as scanner;

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
/// - Heritage clauses (extends/implements)
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

    /// Symbol name -> symbol kind (Function, Class, Interface, etc.)
    /// Populated from binder data when available, enabling accurate kind
    /// inference in workspace symbol search instead of naming heuristics.
    definition_kinds: FxHashMap<String, SymbolKind>,

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

    /// Sorted list of all symbol names for prefix search.
    /// Enables efficient prefix matching using binary search (O(log N)).
    /// Maintained in sorted order via binary search on insert/update.
    sorted_names: Vec<String>,

    /// Heritage clause tracking: symbol name -> files that extend/implement it
    /// Enables O(1) lookup for Go to Implementation feature
    /// For example, if "class B extends A" and "class C implements A",
    /// then heritage_clauses["A"] = {"B.ts", "C.ts"}
    heritage_clauses: FxHashMap<String, FxHashSet<String>>,

    /// Reverse heritage tracking: class name -> base classes it extends/implements
    /// Enables upward traversal for heritage-aware rename
    /// For example, if "class B extends A, implements I",
    /// then sub_to_bases["B"] = {"A", "I"}
    sub_to_bases: FxHashMap<String, FxHashSet<String>>,
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

    /// Get all files that extend or implement a given symbol.
    ///
    /// This enables O(1) lookup for Go to Implementation feature.
    /// For example, if "class B extends A" and "class C implements A",
    /// then `get_files_with_heritage("A")` returns `["B.ts", "C.ts"]`.
    pub fn get_files_with_heritage(&self, symbol_name: &str) -> Vec<String> {
        self.heritage_clauses
            .get(symbol_name)
            .map(|files| files.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Get all base classes/interfaces that a given class extends or implements.
    ///
    /// This enables upward traversal for heritage-aware rename.
    /// For example, if "class B extends A, implements I",
    /// then `get_bases_for_class("B")` returns `["A", "I"]`.
    pub fn get_bases_for_class(&self, class_name: &str) -> Vec<String> {
        self.sub_to_bases
            .get(class_name)
            .map(|bases| bases.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Update the index for a single file.
    ///
    /// This removes old entries and re-indexes the file based on its
    /// current binder state and AST identifiers.
    pub fn update_file(&mut self, file_name: &str, binder: &BinderState, arena: &NodeArena) {
        self.remove_file(file_name);
        self.index_file(file_name, binder, arena);
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
                        // Only remove from sorted_names if no other files reference this symbol
                        self.remove_sorted_name(sym);
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

        // Remove definition kinds for symbols that no longer have any definitions
        let names_to_remove: Vec<String> = self
            .definition_kinds
            .keys()
            .filter(|name| {
                !self
                    .definitions
                    .get(name.as_str())
                    .map(|defs| !defs.is_empty())
                    .unwrap_or(false)
            })
            .cloned()
            .collect();

        for name in names_to_remove {
            self.definition_kinds.remove(&name);
            self.remove_sorted_name(&name);
        }

        // Remove exports for this file
        if let Some(exports) = self.exports.remove(file_name) {
            // Check if we need to remove any export names from sorted_names
            for export_name in exports {
                // Only remove from sorted_names if this is the only file exporting this name
                let is_still_exported = self
                    .exports
                    .values()
                    .any(|file_exports| file_exports.contains(&export_name));
                if !is_still_exported && !self.name_to_files.contains_key(&export_name) {
                    self.remove_sorted_name(&export_name);
                }
            }
        }

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

        // Remove heritage clause entries for this file
        for files in self.heritage_clauses.values_mut() {
            files.remove(file_name);
        }
        self.heritage_clauses.retain(|_, files| !files.is_empty());

        // Remove sub_to_bases entries for this file
        for bases in self.sub_to_bases.values_mut() {
            bases.remove(file_name);
        }
        self.sub_to_bases.retain(|_, bases| !bases.is_empty());
    }

    /// Index a file during binding.
    ///
    /// Extracts symbol information from the binder state and AST, adding it
    /// to the appropriate index structures. This includes:
    /// - **All identifier mentions** from the AST (via pool scan of identifiers pool)
    /// - Declarations from binder.file_locals
    /// - Symbol kinds from binder flags
    /// - Exports and re-exports
    ///
    /// The pool scan of the identifiers pool enables O(1) candidate filtering
    /// for cross-file reference searches. Before searching for references to a
    /// symbol, we can query `get_files_with_symbol()` to find only the files
    /// that actually contain that identifier string, avoiding expensive Checker
    /// runs on irrelevant files.
    pub fn index_file(&mut self, file_name: &str, binder: &BinderState, arena: &NodeArena) {
        let file_name_owned = file_name.to_string();
        let mut file_symbol_names = FxHashSet::default();

        // Pool Scan: Index all identifier mentions from the AST
        // This enables O(1) candidate filtering when searching for cross-file references
        for id_data in &arena.identifiers {
            // Skip empty identifiers (error recovery)
            if id_data.escaped_text.is_empty() {
                continue;
            }

            let text = &id_data.escaped_text;

            // Add to name_to_files index
            self.name_to_files
                .entry(text.clone())
                .or_default()
                .insert(file_name_owned.clone());

            // Add to sorted names for prefix search
            self.insert_sorted_name(text.clone());

            // Track in reverse mapping for efficient cleanup
            file_symbol_names.insert(text.clone());
        }

        // Index all symbols in file_locals (top-level symbols/declarations)
        for (name, symbol_id) in binder.file_locals.iter() {
            // Add to name_to_files index
            self.name_to_files
                .entry(name.clone())
                .or_default()
                .insert(file_name_owned.clone());

            // Add to sorted names for prefix search
            self.insert_sorted_name(name.clone());

            // Track in reverse mapping for efficient cleanup
            file_symbol_names.insert(name.clone());

            // Extract symbol kind from binder flags and store it
            if let Some(symbol) = binder.symbols.get(*symbol_id) {
                let kind = symbol_flags_to_kind(symbol.flags);
                self.definition_kinds.insert(name.clone(), kind);
            }
        }

        // Index exports from the module_exports if this file is registered
        if let Some(exports) = binder.module_exports.get(file_name) {
            for (export_name, _symbol_id) in exports.iter() {
                self.exports
                    .entry(file_name_owned.clone())
                    .or_default()
                    .insert(export_name.clone());

                // Also add exports to sorted names for prefix search
                self.insert_sorted_name(export_name.clone());
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

                // Add to sorted names for prefix search
                self.insert_sorted_name(export_name.clone());

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

        // Scan for HeritageClause nodes (extends/implements)
        // This enables O(1) lookup for Go to Implementation and upward traversal for rename
        for i in 0..arena.nodes.len() {
            let node_idx = tsz_parser::NodeIndex(i as u32);
            if let Some(node) = arena.get(node_idx) {
                if node.kind == syntax_kind_ext::HERITAGE_CLAUSE {
                    // This is a heritage clause (extends X, implements Y)
                    if let Some(heritage_data) = arena.get_heritage_clause(node) {
                        // Iterate through the types in this heritage clause
                        for type_node_idx in &heritage_data.types.nodes {
                            if let Some(type_name) =
                                self.extract_heritage_type_name(arena, *type_node_idx)
                            {
                                // Track that this file extends/implements the type (for downward lookup)
                                self.heritage_clauses
                                    .entry(type_name.clone())
                                    .or_default()
                                    .insert(file_name_owned.clone());

                                // Track the reverse: which types does this file's class extend/implement
                                // We need to find the class/interface name that owns this heritage clause
                                // To do this efficiently, we'll scan for ClassDeclaration/InterfaceDeclaration nodes
                                // and track their heritage clauses separately below
                                file_symbol_names.insert(type_name);
                            }
                        }
                    }
                }
            }
        }

        // Second pass: Build sub_to_bases mapping
        // For each ClassDeclaration/InterfaceDeclaration, extract its heritage clauses
        for i in 0..arena.nodes.len() {
            let node_idx = tsz_parser::NodeIndex(i as u32);
            if let Some(node) = arena.get(node_idx) {
                let is_class_or_interface = node.kind == syntax_kind_ext::CLASS_DECLARATION
                    || node.kind == syntax_kind_ext::INTERFACE_DECLARATION;

                if is_class_or_interface {
                    if let Some(class_name) = arena.get_identifier_text(node_idx) {
                        let class_name = class_name.to_string();

                        // Look for HeritageClause nodes that follow this class declaration
                        // In TypeScript AST, heritage clauses typically appear as siblings or children
                        // We'll scan forward a reasonable number of nodes to find them
                        let search_window = 50_usize; // Look ahead up to 50 nodes
                        let start = i + 1;
                        let end = (i + 1 + search_window).min(arena.nodes.len());

                        for j in start..end {
                            let heritage_idx = tsz_parser::NodeIndex(j as u32);
                            if let Some(heritage_node) = arena.get(heritage_idx) {
                                if heritage_node.kind == syntax_kind_ext::HERITAGE_CLAUSE {
                                    // Extract base types from this heritage clause
                                    if let Some(heritage_data) =
                                        arena.get_heritage_clause(heritage_node)
                                    {
                                        for type_node_idx in &heritage_data.types.nodes {
                                            if let Some(base_name) = self
                                                .extract_heritage_type_name(arena, *type_node_idx)
                                            {
                                                // Track that this class extends/implements the base type
                                                self.sub_to_bases
                                                    .entry(class_name.clone())
                                                    .or_default()
                                                    .insert(base_name);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
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

        // Add to sorted names for prefix search
        self.insert_sorted_name(symbol_name.to_string());

        // Update file_symbols reverse mapping for efficient cleanup
        self.file_symbols
            .entry(file_name.to_string())
            .or_default()
            .insert(symbol_name.to_string());
    }

    /// Add a symbol definition to the index.
    pub fn add_definition(&mut self, symbol_name: &str, location: Location) {
        // Track which file contains this definition for cleanup
        let file_path = location.file_path.clone();

        // Add to name_to_files so we can track which files have which symbols
        self.name_to_files
            .entry(symbol_name.to_string())
            .or_default()
            .insert(file_path.clone());

        self.definitions
            .entry(symbol_name.to_string())
            .or_default()
            .push(location);

        // Add to sorted names for prefix search
        self.insert_sorted_name(symbol_name.to_string());

        // Track which file contains this definition for cleanup
        self.file_symbols
            .entry(file_path)
            .or_default()
            .insert(symbol_name.to_string());
    }

    /// Add a symbol definition with a known kind to the index.
    ///
    /// This stores both the location and the symbol kind, enabling
    /// accurate kind reporting in workspace symbol search.
    pub fn add_definition_with_kind(
        &mut self,
        symbol_name: &str,
        location: Location,
        kind: SymbolKind,
    ) {
        self.definitions
            .entry(symbol_name.to_string())
            .or_default()
            .push(location);
        self.definition_kinds.insert(symbol_name.to_string(), kind);
    }

    /// Look up the stored symbol kind for a definition name.
    ///
    /// Returns `None` if no kind was recorded (e.g. the definition was
    /// added via `add_definition` without kind information).
    pub fn get_definition_kind(&self, symbol_name: &str) -> Option<SymbolKind> {
        self.definition_kinds.get(symbol_name).copied()
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
        self.definition_kinds.clear();
        self.exports.clear();
        self.imports.clear();
        self.importers.clear();
        self.file_symbols.clear();
        self.sorted_names.clear();
    }

    /// Get all symbols that start with the given prefix.
    ///
    /// Uses binary search on the sorted names vector for O(log N) prefix matching.
    /// This is useful for auto-completion where users type partial names.
    ///
    /// # Example
    /// ```ignore
    /// let matches = index.get_symbols_with_prefix("use");
    /// // Returns: ["useEffect", "useState", "useCallback"]
    /// ```
    pub fn get_symbols_with_prefix(&self, prefix: &str) -> Vec<String> {
        // Empty prefix matches all symbols
        if prefix.is_empty() {
            return self.sorted_names.clone();
        }

        // Binary search to find the first symbol >= prefix
        let start = self
            .sorted_names
            .partition_point(|name| name.as_str() < prefix);

        // Collect all symbols that start with the prefix
        // We iterate from the start position until we find a symbol that doesn't match
        let mut result = Vec::new();
        for name in &self.sorted_names[start..] {
            if name.starts_with(prefix) {
                result.push(name.clone());
            } else {
                // Since sorted_names is sorted, once we find a non-matching name,
                // we can stop iterating
                break;
            }
        }

        result
    }

    /// Insert a name into the sorted vector while maintaining sorted order.
    fn insert_sorted_name(&mut self, name: String) {
        // Binary search to find insertion point
        let pos = self
            .sorted_names
            .partition_point(|n| n.as_str() < name.as_str());

        // Only insert if not already present
        if pos >= self.sorted_names.len() || self.sorted_names[pos] != name {
            self.sorted_names.insert(pos, name);
        }
    }

    /// Remove a name from the sorted vector.
    fn remove_sorted_name(&mut self, name: &str) {
        // Binary search to find the name
        let pos = self.sorted_names.partition_point(|n| n.as_str() < name);

        // Remove if found
        if pos < self.sorted_names.len() && self.sorted_names[pos] == name {
            self.sorted_names.remove(pos);
        }
    }

    /// Extract the type name from a heritage clause expression node.
    ///
    /// This handles:
    /// - Simple identifiers: `extends A` -> returns "A"
    /// - Property access: `implements ns.I` -> returns "I"
    /// - Returns None for complex expressions we can't resolve
    fn extract_heritage_type_name(&self, arena: &NodeArena, node_idx: NodeIndex) -> Option<String> {
        let node = arena.get(node_idx)?;

        // Case 1: Simple identifier (e.g., `extends A`)
        if node.kind == scanner::SyntaxKind::Identifier as u16 {
            return arena.get_identifier_text(node_idx).map(|s| s.to_string());
        }

        // Case 2: Property access expression (e.g., `implements ns.I`)
        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            if let Some(access_expr) = arena.get_access_expr(node) {
                // For `ns.I`, we want the property name ("I")
                // The name_or_argument field contains the member name
                return arena
                    .get_identifier_text(access_expr.name_or_argument)
                    .map(|s| s.to_string());
            }
        }

        // Case 3: Can't handle this expression type
        None
    }
}

/// Convert binder symbol flags to an LSP `SymbolKind`.
///
/// Checks flags in specificity order so that, e.g., a const-enum is
/// reported as `Enum` rather than `Variable`.  Falls back to
/// `SymbolKind::Variable` when no recognisable flag is set.
fn symbol_flags_to_kind(flags: u32) -> SymbolKind {
    if flags & symbol_flags::FUNCTION != 0 {
        SymbolKind::Function
    } else if flags & symbol_flags::CLASS != 0 {
        SymbolKind::Class
    } else if flags & symbol_flags::INTERFACE != 0 {
        SymbolKind::Interface
    } else if flags & symbol_flags::ENUM != 0 {
        SymbolKind::Enum
    } else if flags & symbol_flags::ENUM_MEMBER != 0 {
        SymbolKind::EnumMember
    } else if flags & symbol_flags::TYPE_ALIAS != 0 {
        SymbolKind::TypeParameter
    } else if flags & symbol_flags::TYPE_PARAMETER != 0 {
        SymbolKind::TypeParameter
    } else if flags & symbol_flags::MODULE != 0 {
        SymbolKind::Module
    } else if flags & symbol_flags::METHOD != 0 {
        SymbolKind::Method
    } else if flags & symbol_flags::PROPERTY != 0 {
        SymbolKind::Property
    } else if flags & symbol_flags::CONSTRUCTOR != 0 {
        SymbolKind::Constructor
    } else if flags & symbol_flags::ACCESSOR != 0 {
        SymbolKind::Property
    } else if flags & symbol_flags::BLOCK_SCOPED_VARIABLE != 0 {
        // const declarations get Constant; let gets Variable.
        // The binder uses BLOCK_SCOPED_VARIABLE for both let and const.
        // We report as Variable here; callers that need to distinguish
        // const should check node_flags::CONST on the declaration.
        SymbolKind::Variable
    } else if flags & symbol_flags::FUNCTION_SCOPED_VARIABLE != 0 {
        SymbolKind::Variable
    } else {
        SymbolKind::Variable
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
    use tsz_common::position::{Position, Range};

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

    // =========================================================================
    // Definition kind tracking tests
    // =========================================================================

    #[test]
    fn test_add_definition_with_kind_stores_kind() {
        let mut index = SymbolIndex::new();
        index.add_definition_with_kind(
            "processItems",
            make_location("utils.ts", 0, 0, 12),
            SymbolKind::Function,
        );

        assert_eq!(
            index.get_definition_kind("processItems"),
            Some(SymbolKind::Function)
        );
        // The definition location should also be stored
        let defs = index.find_definitions("processItems");
        assert_eq!(defs.len(), 1);
    }

    #[test]
    fn test_add_definition_without_kind_has_no_kind() {
        let mut index = SymbolIndex::new();
        index.add_definition("foo", make_location("a.ts", 0, 0, 3));

        assert_eq!(index.get_definition_kind("foo"), None);
    }

    #[test]
    fn test_definition_kind_survives_multiple_adds() {
        let mut index = SymbolIndex::new();
        // First add without kind
        index.add_definition("bar", make_location("a.ts", 0, 0, 3));
        assert_eq!(index.get_definition_kind("bar"), None);

        // Then add with kind (simulating a re-index that now has binder data)
        index.add_definition_with_kind("bar", make_location("b.ts", 0, 0, 3), SymbolKind::Class);
        assert_eq!(index.get_definition_kind("bar"), Some(SymbolKind::Class));
        // Both definition locations should exist
        assert_eq!(index.find_definitions("bar").len(), 2);
    }

    #[test]
    fn test_clear_removes_definition_kinds() {
        let mut index = SymbolIndex::new();
        index.add_definition_with_kind("Foo", make_location("a.ts", 0, 0, 3), SymbolKind::Class);

        index.clear();

        assert_eq!(index.get_definition_kind("Foo"), None);
    }

    #[test]
    fn test_remove_file_cleans_up_definition_kinds() {
        let mut index = SymbolIndex::new();

        // Add a definition with kind from file a.ts
        index.add_definition_with_kind(
            "MyClass",
            make_location("a.ts", 0, 0, 7),
            SymbolKind::Class,
        );

        assert_eq!(
            index.get_definition_kind("MyClass"),
            Some(SymbolKind::Class)
        );

        // Remove the file
        index.remove_file("a.ts");

        // The kind should be cleaned up because no definitions remain
        assert_eq!(index.get_definition_kind("MyClass"), None);
        assert!(index.find_definitions("MyClass").is_empty());
    }

    #[test]
    fn test_remove_file_keeps_kind_if_other_file_has_definition() {
        let mut index = SymbolIndex::new();

        // Add definitions in two files
        index.add_definition_with_kind(
            "Config",
            make_location("a.ts", 0, 0, 6),
            SymbolKind::Interface,
        );
        index.add_definition("Config", make_location("b.ts", 0, 0, 6));

        // Remove only a.ts
        index.remove_file("a.ts");

        // b.ts still has a definition, so the kind should be retained
        assert_eq!(index.find_definitions("Config").len(), 1);
        assert_eq!(
            index.get_definition_kind("Config"),
            Some(SymbolKind::Interface)
        );
    }

    // =========================================================================
    // symbol_flags_to_kind tests
    // =========================================================================

    #[test]
    fn test_symbol_flags_to_kind_function() {
        assert_eq!(
            symbol_flags_to_kind(symbol_flags::FUNCTION),
            SymbolKind::Function
        );
    }

    #[test]
    fn test_symbol_flags_to_kind_class() {
        assert_eq!(symbol_flags_to_kind(symbol_flags::CLASS), SymbolKind::Class);
    }

    #[test]
    fn test_symbol_flags_to_kind_interface() {
        assert_eq!(
            symbol_flags_to_kind(symbol_flags::INTERFACE),
            SymbolKind::Interface
        );
    }

    #[test]
    fn test_symbol_flags_to_kind_regular_enum() {
        assert_eq!(
            symbol_flags_to_kind(symbol_flags::REGULAR_ENUM),
            SymbolKind::Enum
        );
    }

    #[test]
    fn test_symbol_flags_to_kind_const_enum() {
        assert_eq!(
            symbol_flags_to_kind(symbol_flags::CONST_ENUM),
            SymbolKind::Enum
        );
    }

    #[test]
    fn test_symbol_flags_to_kind_enum_member() {
        assert_eq!(
            symbol_flags_to_kind(symbol_flags::ENUM_MEMBER),
            SymbolKind::EnumMember
        );
    }

    #[test]
    fn test_symbol_flags_to_kind_type_alias() {
        assert_eq!(
            symbol_flags_to_kind(symbol_flags::TYPE_ALIAS),
            SymbolKind::TypeParameter
        );
    }

    #[test]
    fn test_symbol_flags_to_kind_module() {
        assert_eq!(
            symbol_flags_to_kind(symbol_flags::VALUE_MODULE),
            SymbolKind::Module
        );
        assert_eq!(
            symbol_flags_to_kind(symbol_flags::NAMESPACE_MODULE),
            SymbolKind::Module
        );
    }

    #[test]
    fn test_symbol_flags_to_kind_method() {
        assert_eq!(
            symbol_flags_to_kind(symbol_flags::METHOD),
            SymbolKind::Method
        );
    }

    #[test]
    fn test_symbol_flags_to_kind_property() {
        assert_eq!(
            symbol_flags_to_kind(symbol_flags::PROPERTY),
            SymbolKind::Property
        );
    }

    #[test]
    fn test_symbol_flags_to_kind_constructor() {
        assert_eq!(
            symbol_flags_to_kind(symbol_flags::CONSTRUCTOR),
            SymbolKind::Constructor
        );
    }

    #[test]
    fn test_symbol_flags_to_kind_variable() {
        assert_eq!(
            symbol_flags_to_kind(symbol_flags::BLOCK_SCOPED_VARIABLE),
            SymbolKind::Variable
        );
        assert_eq!(
            symbol_flags_to_kind(symbol_flags::FUNCTION_SCOPED_VARIABLE),
            SymbolKind::Variable
        );
    }

    #[test]
    fn test_symbol_flags_to_kind_none() {
        assert_eq!(
            symbol_flags_to_kind(symbol_flags::NONE),
            SymbolKind::Variable
        );
    }

    #[test]
    fn test_symbol_flags_to_kind_composite_prefers_function() {
        // A symbol with both FUNCTION and BLOCK_SCOPED_VARIABLE flags
        // should be reported as Function (higher specificity).
        let flags = symbol_flags::FUNCTION | symbol_flags::BLOCK_SCOPED_VARIABLE;
        assert_eq!(symbol_flags_to_kind(flags), SymbolKind::Function);
    }

    #[test]
    fn test_symbol_flags_to_kind_class_with_interface_merge() {
        // Class + Interface (declaration merging): Class wins because
        // it is checked first in the specificity order.
        let flags = symbol_flags::CLASS | symbol_flags::INTERFACE;
        assert_eq!(symbol_flags_to_kind(flags), SymbolKind::Class);
    }

    // =========================================================================
    // Prefix matching tests
    // =========================================================================

    #[test]
    fn test_get_symbols_with_prefix_empty() {
        let index = SymbolIndex::new();
        let matches = index.get_symbols_with_prefix("foo");
        assert!(matches.is_empty());
    }

    #[test]
    fn test_get_symbols_with_prefix_single_match() {
        let mut index = SymbolIndex::new();
        index.add_definition("fooBar", make_location("a.ts", 0, 0, 6));

        let matches = index.get_symbols_with_prefix("foo");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0], "fooBar");
    }

    #[test]
    fn test_get_symbols_with_prefix_multiple_matches() {
        let mut index = SymbolIndex::new();
        index.add_definition("fooBar", make_location("a.ts", 0, 0, 6));
        index.add_definition("fooBaz", make_location("a.ts", 1, 0, 6));
        index.add_definition("barQux", make_location("a.ts", 2, 0, 6));

        let matches = index.get_symbols_with_prefix("foo");
        assert_eq!(matches.len(), 2);
        assert!(matches.contains(&"fooBar".to_string()));
        assert!(matches.contains(&"fooBaz".to_string()));
    }

    #[test]
    fn test_get_symbols_with_prefix_case_sensitive() {
        let mut index = SymbolIndex::new();
        index.add_definition("fooBar", make_location("a.ts", 0, 0, 6));
        index.add_definition("FooBaz", make_location("a.ts", 1, 0, 6));

        let matches = index.get_symbols_with_prefix("foo");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0], "fooBar");

        let matches_upper = index.get_symbols_with_prefix("Foo");
        assert_eq!(matches_upper.len(), 1);
        assert_eq!(matches_upper[0], "FooBaz");
    }

    #[test]
    fn test_get_symbols_with_prefix_returns_sorted() {
        let mut index = SymbolIndex::new();
        // Add in non-sorted order
        index.add_definition("zebra", make_location("a.ts", 0, 0, 5));
        index.add_definition("apple", make_location("a.ts", 1, 0, 5));
        index.add_definition("banana", make_location("a.ts", 2, 0, 6));

        let matches = index.get_symbols_with_prefix("a");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0], "apple");

        // All symbols should be in sorted order
        let all_matches = index.get_symbols_with_prefix("");
        assert_eq!(all_matches, vec!["apple", "banana", "zebra"]);
    }

    #[test]
    fn test_get_symbols_with_prefix_after_remove() {
        let mut index = SymbolIndex::new();
        index.add_definition("fooBar", make_location("a.ts", 0, 0, 6));
        index.add_definition("fooBaz", make_location("b.ts", 0, 0, 6));
        index.add_definition("barQux", make_location("c.ts", 0, 0, 6));

        // Remove file a.ts which contains fooBar
        index.remove_file("a.ts");

        let matches = index.get_symbols_with_prefix("foo");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0], "fooBaz");
    }

    #[test]
    fn test_get_symbols_with_prefix_empty_prefix() {
        let mut index = SymbolIndex::new();
        index.add_definition("zebra", make_location("a.ts", 0, 0, 5));
        index.add_definition("apple", make_location("b.ts", 0, 0, 5));

        let matches = index.get_symbols_with_prefix("");
        assert_eq!(matches.len(), 2);
        // Should be sorted
        assert_eq!(matches[0], "apple");
        assert_eq!(matches[1], "zebra");
    }

    #[test]
    fn test_get_symbols_with_prefix_no_match() {
        let mut index = SymbolIndex::new();
        index.add_definition("fooBar", make_location("a.ts", 0, 0, 6));

        let matches = index.get_symbols_with_prefix("xyz");
        assert!(matches.is_empty());
    }
}
