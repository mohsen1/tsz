//! Project operations: references and rename.

use std::cmp::Ordering;
use std::collections::VecDeque;
use web_time::Instant;

use rustc_hash::FxHashSet;

use crate::navigation::implementation::{GoToImplementationProvider, TargetKind};
use crate::navigation::references::FindReferences;
use crate::rename::{RenameProvider, TextEdit, WorkspaceEdit};
use crate::resolver::ScopeCacheStats;
use crate::utils::find_node_at_offset;
use tsz_common::position::{Location, Position};
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;

use super::{
    ImportKind, ImportSpecifierTarget, NamespaceReexportTarget, Project, ProjectFile,
    ProjectRequestKind,
};

impl Project {
    fn symbol_text(
        arena: &tsz_parser::parser::node::NodeArena,
        node_idx: NodeIndex,
    ) -> Option<&str> {
        arena
            .get_identifier_text(node_idx)
            .or_else(|| arena.get_literal_text(node_idx))
    }

    fn collect_file_references(
        file: &mut ProjectFile,
        node_idx: NodeIndex,
        scope_stats: Option<&mut ScopeCacheStats>,
        output: &mut Vec<Location>,
    ) {
        if node_idx.is_none() {
            return;
        }

        let find_refs = FindReferences::new(
            file.parser.get_arena(),
            &file.binder,
            &file.line_map,
            file.file_name.clone(),
            file.parser.get_source_text(),
        );

        if let Some(mut refs) = find_refs.find_references_for_node_with_scope_cache(
            file.root(),
            node_idx,
            &mut file.scope_cache,
            scope_stats,
        ) {
            output.append(&mut refs);
        }
    }

    fn collect_file_rename_edits(
        file: &mut ProjectFile,
        node_idx: NodeIndex,
        new_name: &str,
        output: &mut WorkspaceEdit,
    ) {
        let mut locations = Vec::new();
        Self::collect_file_references(file, node_idx, None, &mut locations);
        for location in locations {
            output.add_edit(
                location.file_path,
                TextEdit::new(location.range, new_name.to_string()),
            );
        }
    }

    fn dedup_workspace_edit(workspace_edit: &mut WorkspaceEdit) {
        for edits in workspace_edit.changes.values_mut() {
            let mut seen = FxHashSet::default();
            edits.retain(|edit| {
                let key = (
                    edit.range.start.line,
                    edit.range.start.character,
                    edit.range.end.line,
                    edit.range.end.character,
                );
                seen.insert(key)
            });
        }
    }

    fn import_binding_nodes(
        &self,
        file: &ProjectFile,
        target_file: &str,
        export_name: &str,
    ) -> Vec<NodeIndex> {
        let mut bindings = Vec::new();
        let arena = file.arena();

        let Some(source_file) = arena.get_source_file_at(file.root()) else {
            return bindings;
        };

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::IMPORT_DECLARATION
                && stmt_node.kind != syntax_kind_ext::IMPORT_EQUALS_DECLARATION
            {
                continue;
            }

            let Some(import) = arena.get_import_decl(stmt_node) else {
                continue;
            };
            let Some(module_specifier) = arena.get_literal_text(import.module_specifier) else {
                continue;
            };
            let Some(resolved) = self.resolve_module_specifier(file.file_name(), module_specifier)
            else {
                continue;
            };
            if resolved != target_file {
                continue;
            }

            if import.import_clause.is_none() {
                continue;
            }

            let Some(clause) = arena.get_import_clause_at(import.import_clause) else {
                continue;
            };

            if export_name == "default" && clause.name.is_some() {
                bindings.push(clause.name);
            }

            if clause.named_bindings.is_none() {
                continue;
            }

            let Some(named) = arena.get_named_imports_at(clause.named_bindings) else {
                continue;
            };

            for &spec_idx in &named.elements.nodes {
                let Some(spec) = arena.get_specifier_at(spec_idx) else {
                    continue;
                };

                let export_ident = if spec.property_name.is_some() {
                    spec.property_name
                } else {
                    spec.name
                };
                let Some(imported_name) = Self::symbol_text(arena, export_ident) else {
                    continue;
                };
                if imported_name != export_name {
                    continue;
                }

                bindings.push(spec_idx);
            }
        }

        bindings
    }

    fn import_specifier_targets_for_export(
        &self,
        file: &ProjectFile,
        target_file: &str,
        export_name: &str,
    ) -> Vec<ImportSpecifierTarget> {
        let mut targets = Vec::new();
        let arena = file.arena();

        let Some(source_file) = arena.get_source_file_at(file.root()) else {
            return targets;
        };

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::IMPORT_DECLARATION
                && stmt_node.kind != syntax_kind_ext::IMPORT_EQUALS_DECLARATION
            {
                continue;
            }

            let Some(import) = arena.get_import_decl(stmt_node) else {
                continue;
            };
            let Some(module_specifier) = arena.get_literal_text(import.module_specifier) else {
                continue;
            };
            let Some(resolved) = self.resolve_module_specifier(file.file_name(), module_specifier)
            else {
                continue;
            };
            if resolved != target_file {
                continue;
            }

            if import.import_clause.is_none() {
                continue;
            }

            let Some(clause) = arena.get_import_clause_at(import.import_clause) else {
                continue;
            };

            if clause.named_bindings.is_none() {
                continue;
            }

            let Some(bindings_node) = arena.get(clause.named_bindings) else {
                continue;
            };
            if bindings_node.kind == SyntaxKind::Identifier as u16 {
                continue;
            }

            let Some(named) = arena.get_named_imports(bindings_node) else {
                continue;
            };

            for &spec_idx in &named.elements.nodes {
                let Some(spec) = arena.get_specifier_at(spec_idx) else {
                    continue;
                };

                let export_ident = if spec.property_name.is_some() {
                    spec.property_name
                } else {
                    spec.name
                };
                let Some(export_text) = Self::symbol_text(arena, export_ident) else {
                    continue;
                };
                if export_text != export_name {
                    continue;
                }

                let local_ident = if spec.name.is_some() {
                    spec.name
                } else {
                    spec.property_name
                };
                let property_name = (spec.property_name.is_some()).then_some(spec.property_name);

                targets.push(ImportSpecifierTarget {
                    local_ident,
                    property_name,
                });
            }
        }

        targets
    }

    fn named_import_local_names(
        &self,
        file: &ProjectFile,
        target_file: &str,
        export_name: &str,
    ) -> Vec<String> {
        let mut locals = Vec::new();
        let arena = file.arena();

        let Some(source_file) = arena.get_source_file_at(file.root()) else {
            return locals;
        };

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::IMPORT_DECLARATION
                && stmt_node.kind != syntax_kind_ext::IMPORT_EQUALS_DECLARATION
            {
                continue;
            }

            let Some(import) = arena.get_import_decl(stmt_node) else {
                continue;
            };
            let Some(module_specifier) = arena.get_literal_text(import.module_specifier) else {
                continue;
            };
            let Some(resolved) = self.resolve_module_specifier(file.file_name(), module_specifier)
            else {
                continue;
            };
            if resolved != target_file {
                continue;
            }

            if import.import_clause.is_none() {
                continue;
            }

            let Some(clause) = arena.get_import_clause_at(import.import_clause) else {
                continue;
            };

            if clause.named_bindings.is_none() {
                continue;
            }

            let Some(bindings_node) = arena.get(clause.named_bindings) else {
                continue;
            };
            if bindings_node.kind == SyntaxKind::Identifier as u16 {
                continue;
            }

            let Some(named) = arena.get_named_imports(bindings_node) else {
                continue;
            };

            for &spec_idx in &named.elements.nodes {
                let Some(spec) = arena.get_specifier_at(spec_idx) else {
                    continue;
                };

                let export_ident = if spec.property_name.is_some() {
                    spec.property_name
                } else {
                    spec.name
                };
                let Some(export_text) = Self::symbol_text(arena, export_ident) else {
                    continue;
                };
                if export_text != export_name {
                    continue;
                }

                let local_ident = if spec.name.is_some() {
                    spec.name
                } else {
                    spec.property_name
                };
                let Some(local_text) = Self::symbol_text(arena, local_ident) else {
                    continue;
                };
                locals.push(local_text.to_string());
            }
        }

        locals
    }

    fn reexport_targets_for(
        &self,
        source_file: &str,
        export_name: &str,
        refs: &mut Vec<Location>,
    ) -> (Vec<(String, String)>, Vec<NamespaceReexportTarget>) {
        let mut targets = Vec::new();
        let mut namespace_targets = Vec::new();

        for (file_name, file) in &self.files {
            let arena = file.arena();
            let Some(source_file_node) = arena.get_source_file_at(file.root()) else {
                continue;
            };

            for &stmt_idx in &source_file_node.statements.nodes {
                let Some(stmt_node) = arena.get(stmt_idx) else {
                    continue;
                };
                if stmt_node.kind != syntax_kind_ext::EXPORT_DECLARATION {
                    continue;
                }

                let Some(export) = arena.get_export_decl(stmt_node) else {
                    continue;
                };
                if export.module_specifier.is_none() {
                    continue;
                }

                let Some(module_specifier) = arena.get_literal_text(export.module_specifier) else {
                    continue;
                };
                let Some(resolved) =
                    self.resolve_module_specifier(file.file_name(), module_specifier)
                else {
                    continue;
                };
                if resolved != source_file {
                    continue;
                }

                if export.export_clause.is_none() {
                    if export_name != "default" {
                        targets.push((file_name.clone(), export_name.to_string()));
                    }
                    continue;
                }

                let Some(clause_node) = arena.get(export.export_clause) else {
                    continue;
                };
                if clause_node.kind != syntax_kind_ext::NAMED_EXPORTS {
                    if clause_node.kind == SyntaxKind::Identifier as u16
                        && let Some(ns_name) = arena.get_identifier_text(export.export_clause)
                    {
                        namespace_targets.push(NamespaceReexportTarget {
                            file: file_name.clone(),
                            namespace: ns_name.to_string(),
                            member: export_name.to_string(),
                        });
                    }
                    continue;
                }

                let Some(named) = arena.get_named_imports(clause_node) else {
                    continue;
                };
                for &spec_idx in &named.elements.nodes {
                    let Some(spec) = arena.get_specifier_at(spec_idx) else {
                        continue;
                    };

                    let import_ident = if spec.property_name.is_some() {
                        spec.property_name
                    } else {
                        spec.name
                    };
                    let Some(import_text) = Self::symbol_text(arena, import_ident) else {
                        continue;
                    };
                    if import_text != export_name {
                        continue;
                    }

                    if let Some(location) = file.node_location(import_ident) {
                        refs.push(location);
                    }

                    let export_ident = if spec.name.is_some() {
                        spec.name
                    } else {
                        spec.property_name
                    };
                    if let Some(export_text) = Self::symbol_text(arena, export_ident) {
                        targets.push((file_name.clone(), export_text.to_string()));
                    }
                }
            }
        }

        (targets, namespace_targets)
    }

    fn namespace_import_names(&self, file: &ProjectFile, target_file: &str) -> Vec<String> {
        let mut names = Vec::new();
        let arena = file.arena();

        let Some(source_file) = arena.get_source_file_at(file.root()) else {
            return names;
        };

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::IMPORT_DECLARATION
                && stmt_node.kind != syntax_kind_ext::IMPORT_EQUALS_DECLARATION
            {
                continue;
            }

            let Some(import) = arena.get_import_decl(stmt_node) else {
                continue;
            };
            let Some(module_specifier) = arena.get_literal_text(import.module_specifier) else {
                continue;
            };
            let Some(resolved) = self.resolve_module_specifier(file.file_name(), module_specifier)
            else {
                continue;
            };
            if resolved != target_file {
                continue;
            }

            if import.import_clause.is_none() {
                continue;
            }

            let Some(clause) = arena.get_import_clause_at(import.import_clause) else {
                continue;
            };

            if clause.named_bindings.is_none() {
                continue;
            }

            let Some(bindings_node) = arena.get(clause.named_bindings) else {
                continue;
            };
            if bindings_node.kind != syntax_kind_ext::NAMESPACE_IMPORT {
                continue;
            }

            let Some(bindings) = arena.get_named_imports(bindings_node) else {
                continue;
            };
            if let Some(name) = arena.get_identifier_text(bindings.name) {
                names.push(name.to_string());
            }
        }

        names
    }

    fn collect_namespace_member_locations(
        &self,
        file: &ProjectFile,
        namespace_name: &str,
        export_name: &str,
        output: &mut Vec<Location>,
    ) {
        let arena = file.arena();
        let expected_symbol = file.binder().file_locals.get(namespace_name);

        for node in &arena.nodes {
            if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                && node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            {
                continue;
            }

            let Some(access) = arena.get_access_expr(node) else {
                continue;
            };
            let expr_idx = access.expression;
            let Some(expr_node) = arena.get(expr_idx) else {
                continue;
            };
            if expr_node.kind != SyntaxKind::Identifier as u16 {
                continue;
            }

            let Some(expr_text) = arena.get_identifier_text(expr_idx) else {
                continue;
            };
            if expr_text != namespace_name {
                continue;
            }

            if let Some(sym_id) = expected_symbol
                && file.binder().resolve_identifier(arena, expr_idx) != Some(sym_id)
            {
                continue;
            }

            let member_idx = access.name_or_argument;
            let matches = if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                arena.get_identifier_text(member_idx) == Some(export_name)
            } else {
                arena.get_literal_text(member_idx) == Some(export_name)
            };

            if !matches {
                continue;
            }

            if let Some(location) = file.node_location(member_idx) {
                output.push(location);
            }
        }
    }

    /// Check if a symbol is a class/interface member that should use heritage discovery.
    ///
    /// Returns true if the symbol is a PROPERTY, METHOD, or ACCESSOR that is NOT private.
    /// Private members are strictly local to the class and should not participate in heritage discovery.
    const fn is_heritage_member_symbol(_file: &ProjectFile, symbol: &tsz_binder::Symbol) -> bool {
        use tsz_binder::symbol_flags;

        // Check if it's a member type
        let is_member = symbol.has_any_flags(
            symbol_flags::PROPERTY
                | symbol_flags::METHOD
                | symbol_flags::CONSTRUCTOR
                | symbol_flags::GET_ACCESSOR
                | symbol_flags::SET_ACCESSOR,
        );

        if !is_member {
            return false;
        }

        // Exclude private members - they're strictly local to the class
        if symbol.has_any_flags(symbol_flags::PRIVATE) {
            return false;
        }

        true
    }

    /// Find all heritage members (upward and downward) for a class/interface member.
    ///
    /// This performs bidirectional traversal:
    /// - **Upward**: Walks up the extends/implements chain to find base class members
    /// - **Downward**: Finds all derived classes that override/implement this member
    ///
    /// # Arguments
    /// * `file` - The file containing the initial symbol
    /// * `symbol_id` - The symbol ID for the member
    /// * `member_name` - The member name (used for matching)
    ///
    /// # Returns
    /// A set of (`file_path`, `symbol_id`) pairs representing all related members in the heritage chain
    fn find_all_heritage_members(
        &self,
        file: &ProjectFile,
        symbol_id: tsz_binder::SymbolId,
        member_name: &str,
    ) -> FxHashSet<(String, tsz_binder::SymbolId)> {
        use tsz_binder::symbol_flags;

        let mut result = FxHashSet::default();
        let symbol = match file.binder().symbols.get(symbol_id) {
            Some(s) => s,
            None => return result,
        };

        // Get the parent class/interface symbol
        let parent_symbol_id = symbol.parent;
        if parent_symbol_id.is_none() {
            // No parent - not a class/interface member
            return result;
        }

        let parent_symbol = match file.binder().symbols.get(parent_symbol_id) {
            Some(s) => s,
            None => return result,
        };

        // Check if parent is a class or interface
        let is_class_or_interface =
            parent_symbol.has_any_flags(symbol_flags::CLASS | symbol_flags::INTERFACE);
        if !is_class_or_interface {
            return result;
        }

        let parent_name = &parent_symbol.escaped_name;

        // Add the current symbol to results
        result.insert((file.file_name().to_string(), symbol_id));

        // Upward search: Find base class members using sub_to_bases
        if let Some(base_members) = self.find_base_class_members(file, parent_name, member_name) {
            result.extend(base_members);
        }

        // Downward search: Find all derived classes using heritage_clauses
        let derived_files = self.symbol_index.get_files_with_heritage(parent_name);

        for derived_file_path in derived_files {
            // Skip the current file (we already added it)
            if derived_file_path == file.file_name() {
                continue;
            }

            if let Some(derived_file) = self.files.get(&derived_file_path) {
                // Search for symbols with the same member name in the derived file
                for (idx, sym) in derived_file.binder().symbols.iter().enumerate() {
                    if sym.escaped_name == member_name
                        && Self::is_heritage_member_symbol(derived_file, sym)
                    {
                        // This is a matching member in a derived class
                        let sym_id = tsz_binder::SymbolId(idx as u32);
                        result.insert((derived_file_path.clone(), sym_id));
                    }
                }
            }
        }

        result
    }

    /// Find base class members by walking up the heritage chain.
    ///
    /// This searches for members with the same name in base classes/interfaces
    /// that the parent class extends or implements, using the new `sub_to_bases` mapping.
    ///
    /// # Arguments
    /// * `file` - The file containing the derived class
    /// * `class_name` - The name of the class/interface
    /// * `member_name` - The member name to search for
    ///
    /// # Returns
    /// A set of (`file_path`, `symbol_id`) pairs representing base class members, or None if
    /// no base members were found
    fn find_base_class_members(
        &self,
        _file: &ProjectFile,
        class_name: &str,
        member_name: &str,
    ) -> Option<FxHashSet<(String, tsz_binder::SymbolId)>> {
        // Use SymbolIndex to find base types efficiently
        let base_types = self.symbol_index.get_bases_for_class(class_name);

        if base_types.is_empty() {
            return None;
        }

        let mut result = FxHashSet::default();
        let mut visited_classes = FxHashSet::default();
        visited_classes.insert(class_name.to_string());

        // For each base type, search for the member
        for base_type_name in base_types {
            // Prevent infinite loops in case of circular heritage
            if visited_classes.contains(&base_type_name) {
                continue;
            }
            visited_classes.insert(base_type_name.clone());

            // Find files that define this base type
            let base_files = self.symbol_index.get_files_with_symbol(&base_type_name);

            for base_file_path in base_files {
                if let Some(base_file) = self.files.get(&base_file_path) {
                    // Search for the base type symbol in this file
                    if let Some(base_type_symbol_id) =
                        self.find_class_symbol(base_file, &base_type_name)
                    {
                        // Search for the member in the base type
                        if let Some(base_member_symbol_id) =
                            self.find_member_in_class(base_file, base_type_symbol_id, member_name)
                        {
                            result.insert((base_file_path, base_member_symbol_id));

                            // Recursively search up the hierarchy
                            if let Some(ancestors) = self.find_base_class_members(
                                base_file,
                                &base_type_name,
                                member_name,
                            ) {
                                result.extend(ancestors);
                            }
                        }
                    }
                }
            }
        }

        if result.is_empty() {
            None
        } else {
            Some(result)
        }
    }

    /// Find a class/interface symbol by name in a file.
    fn find_class_symbol(
        &self,
        file: &ProjectFile,
        class_name: &str,
    ) -> Option<tsz_binder::SymbolId> {
        use tsz_binder::symbol_flags;

        for (idx, symbol) in file.binder().symbols.iter().enumerate() {
            if symbol.escaped_name == class_name
                && symbol.has_any_flags(symbol_flags::CLASS | symbol_flags::INTERFACE)
            {
                return Some(tsz_binder::SymbolId(idx as u32));
            }
        }
        None
    }

    /// Find a member symbol by name in a class/interface.
    fn find_member_in_class(
        &self,
        file: &ProjectFile,
        class_symbol_id: tsz_binder::SymbolId,
        member_name: &str,
    ) -> Option<tsz_binder::SymbolId> {
        let class_symbol = file.binder().symbols.get(class_symbol_id)?;

        // Search in the class's members
        if let Some(members) = &class_symbol.members
            && let Some(member_symbol_id) = members.get(member_name)
        {
            // Verify it's a heritage member (not private, is a property/method/accessor)
            if let Some(member_symbol) = file.binder().symbols.get(member_symbol_id)
                && Self::is_heritage_member_symbol(file, member_symbol)
            {
                return Some(member_symbol_id);
            }
        }

        None
    }

    /// Find references within a single file.
    pub fn find_references(
        &mut self,
        file_name: &str,
        position: Position,
    ) -> Option<Vec<Location>> {
        self.touch_file(file_name);
        let start = Instant::now();
        let mut scope_stats = ScopeCacheStats::default();
        let result = (|| {
            let (node_idx, symbol_id, local_name) = {
                let file = self.files.get_mut(file_name)?;
                let offset = file
                    .line_map
                    .position_to_offset(position, file.parser.get_source_text())?;
                let node_idx = find_node_at_offset(file.parser.get_arena(), offset);
                if node_idx.is_none() {
                    return None;
                }

                let finder = FindReferences::new(
                    file.parser.get_arena(),
                    &file.binder,
                    &file.line_map,
                    file.file_name.clone(),
                    file.parser.get_source_text(),
                );
                let symbol_id = finder.resolve_symbol_for_node_with_scope_cache(
                    file.root(),
                    node_idx,
                    &mut file.scope_cache,
                    Some(&mut scope_stats),
                )?;
                let symbol = file.binder().symbols.get(symbol_id)?;
                let local_name = symbol.escaped_name.clone();
                (node_idx, symbol_id, local_name)
            };

            let mut locations = Vec::new();

            // Check if this is a heritage member symbol (class/interface member)
            // If so, we need to find all related symbols across the inheritance hierarchy
            let is_heritage_member = {
                let file = self.files.get(file_name)?;
                let symbol = file.binder().symbols.get(symbol_id);
                symbol.is_some_and(|s| Self::is_heritage_member_symbol(file, s))
            };

            if is_heritage_member {
                // Heritage-aware reference discovery
                let file = self.files.get(file_name)?;
                let heritage_symbols = self.find_all_heritage_members(file, symbol_id, &local_name);

                // Collect references for all related symbols in the heritage chain
                for (heritage_file_path, heritage_symbol_id) in heritage_symbols {
                    if let Some(heritage_file) = self.files.get_mut(&heritage_file_path) {
                        // Find the declaration node for this symbol
                        let symbol = heritage_file.binder().symbols.get(heritage_symbol_id);
                        if let Some(sym) = symbol {
                            // Use the first declaration of the symbol
                            if let Some(&decl_node) = sym.declarations.first() {
                                Self::collect_file_references(
                                    heritage_file,
                                    decl_node,
                                    Some(&mut scope_stats),
                                    &mut locations,
                                );
                            }
                        }
                    }
                }
            } else {
                // Standard reference discovery (non-heritage)
                let file = self.files.get_mut(file_name)?;
                Self::collect_file_references(
                    file,
                    node_idx,
                    Some(&mut scope_stats),
                    &mut locations,
                );
            }

            let (import_targets, export_names, source_file_name) = {
                let file = self.files.get(file_name)?;
                let import_targets = file.import_targets_for_local(&local_name);
                let export_names = if import_targets.is_empty() {
                    file.exported_names_for_symbol(symbol_id)
                } else {
                    Vec::new()
                };
                (import_targets, export_names, file.file_name().to_string())
            };

            let mut cross_targets: Vec<(String, String)> = Vec::new();
            if !import_targets.is_empty() {
                for target in import_targets {
                    let Some(resolved) =
                        self.resolve_module_specifier(&source_file_name, &target.module_specifier)
                    else {
                        continue;
                    };
                    match target.kind {
                        ImportKind::Named(name) => cross_targets.push((resolved, name)),
                        ImportKind::Default => {
                            cross_targets.push((resolved, "default".to_string()));
                        }
                        ImportKind::Namespace => {}
                    }
                }
            } else {
                for export_name in export_names {
                    cross_targets.push((source_file_name.clone(), export_name));
                }
            }

            let mut expanded_targets = Vec::new();
            let mut pending = cross_targets;
            let mut seen_targets: FxHashSet<(String, String)> = FxHashSet::default();
            let mut namespace_targets = Vec::new();

            while let Some((def_file, export_name)) = pending.pop() {
                if !seen_targets.insert((def_file.clone(), export_name.clone())) {
                    continue;
                }
                expanded_targets.push((def_file.clone(), export_name.clone()));

                let mut reexport_refs = Vec::new();
                let (reexports, reexport_namespaces) =
                    self.reexport_targets_for(&def_file, &export_name, &mut reexport_refs);
                locations.extend(reexport_refs);
                pending.extend(reexports);
                namespace_targets.extend(reexport_namespaces);
            }

            for (def_file, export_name) in expanded_targets {
                let export_nodes = {
                    let target_file = self.files.get(&def_file);
                    target_file
                        .map(|file| file.export_nodes(&export_name))
                        .unwrap_or_default()
                };
                if !export_nodes.is_empty()
                    && let Some(target_file) = self.files.get_mut(&def_file)
                {
                    for node in export_nodes {
                        Self::collect_file_references(
                            target_file,
                            node,
                            Some(&mut scope_stats),
                            &mut locations,
                        );
                    }
                }

                // Pool Scan Optimization: Use SymbolIndex for O(M) candidate filtering
                // Instead of O(N) where N = all files, we get O(M) where M = files containing the symbol
                let candidate_files = self.get_candidate_files_for_symbol(&export_name);

                for other_name in &candidate_files {
                    if other_name == &def_file {
                        continue;
                    }

                    let binding_nodes = {
                        let other_file = self.files.get(other_name);
                        other_file
                            .map(|file| self.import_binding_nodes(file, &def_file, &export_name))
                            .unwrap_or_default()
                    };
                    if !binding_nodes.is_empty()
                        && let Some(other_file) = self.files.get_mut(other_name)
                    {
                        for node in binding_nodes {
                            Self::collect_file_references(
                                other_file,
                                node,
                                Some(&mut scope_stats),
                                &mut locations,
                            );
                        }
                    }

                    let namespace_names = {
                        let other_file = self.files.get(other_name);
                        other_file
                            .map(|file| self.namespace_import_names(file, &def_file))
                            .unwrap_or_default()
                    };
                    if !namespace_names.is_empty()
                        && let Some(other_file) = self.files.get(other_name)
                    {
                        for namespace_name in namespace_names {
                            self.collect_namespace_member_locations(
                                other_file,
                                &namespace_name,
                                &export_name,
                                &mut locations,
                            );
                        }
                    }
                }
            }

            let mut seen_namespace_targets: FxHashSet<(String, String, String)> =
                FxHashSet::default();
            for target in namespace_targets {
                if !seen_namespace_targets.insert((
                    target.file.clone(),
                    target.namespace.clone(),
                    target.member.clone(),
                )) {
                    continue;
                }

                // Pool Scan Optimization: Use SymbolIndex for O(M) candidate filtering
                let candidate_files = self.get_candidate_files_for_symbol(&target.member);

                for other_name in &candidate_files {
                    if other_name == &target.file {
                        continue;
                    }

                    let local_names = {
                        let other_file = self.files.get(other_name);
                        other_file
                            .map(|file| {
                                self.named_import_local_names(file, &target.file, &target.namespace)
                            })
                            .unwrap_or_default()
                    };
                    if local_names.is_empty() {
                        continue;
                    }

                    if let Some(other_file) = self.files.get(other_name) {
                        for local_name in local_names {
                            self.collect_namespace_member_locations(
                                other_file,
                                &local_name,
                                &target.member,
                                &mut locations,
                            );
                        }
                    }
                }
            }

            if locations.is_empty() {
                return None;
            }

            locations.sort_by(|a, b| {
                let file_cmp = a.file_path.cmp(&b.file_path);
                if file_cmp != Ordering::Equal {
                    return file_cmp;
                }
                let start_cmp = (a.range.start.line, a.range.start.character)
                    .cmp(&(b.range.start.line, b.range.start.character));
                if start_cmp != Ordering::Equal {
                    return start_cmp;
                }
                (a.range.end.line, a.range.end.character)
                    .cmp(&(b.range.end.line, b.range.end.character))
            });
            locations.dedup_by(|a, b| a.file_path == b.file_path && a.range == b.range);

            Some(locations)
        })();

        self.performance
            .record(ProjectRequestKind::References, start.elapsed(), scope_stats);

        result
    }

    /// Find all implementations of an interface or class across the project.
    ///
    /// This performs a transitive search: if `class B extends A` and `class C extends B`,
    /// searching for implementations of `A` will return both `B` and `C`.
    ///
    /// # Arguments
    /// * `file_name` - The file containing the cursor position
    /// * `position` - The cursor position where the user invoked "Go to Implementation"
    ///
    /// # Returns
    /// A vector of locations where the target is implemented, or None if:
    /// - No symbol is found at the position
    /// - The symbol is not an interface or class
    /// - No implementations are found
    pub fn get_implementations(
        &mut self,
        file_name: &str,
        position: Position,
    ) -> Option<Vec<Location>> {
        let start = Instant::now();

        let result: Option<Vec<Location>> = (|| {
            // Step 1: Resolve the initial target at the cursor position
            let (initial_name, initial_kind): (String, TargetKind) = {
                let file = self.files.get(file_name)?;
                let offset = file
                    .line_map
                    .position_to_offset(position, file.parser.get_source_text())?;

                let provider = GoToImplementationProvider::from_context(file.provider_context());

                // Resolve the target kind for the symbol at the position
                let node_idx = find_node_at_offset(file.parser.get_arena(), offset);
                if node_idx.is_none() {
                    return None;
                }

                // First, try to resolve the symbol at the node
                let symbol_id = provider.resolve_symbol_at_node(node_idx)?;
                let symbol = file.binder.symbols.get(symbol_id)?;
                let target_kind = provider.determine_target_kind(symbol)?;

                (symbol.escaped_name.clone(), target_kind)
            };

            // Step 2: Iterative worklist for transitive search
            let mut results: Vec<Location> = Vec::new();
            let mut queue: VecDeque<(String, String, TargetKind)> = VecDeque::new();
            let mut processed: FxHashSet<(String, String)> = FxHashSet::default();

            // Start with the initial target
            queue.push_back((file_name.to_string(), initial_name, initial_kind));

            while let Some((curr_file, curr_name, curr_kind)) = queue.pop_front() {
                // Skip if we've already processed this (file, name) pair
                if !processed.insert((curr_file.clone(), curr_name.clone())) {
                    continue;
                }

                // Use SymbolIndex to get candidate files that might implement this
                let candidates = self.symbol_index.get_files_with_heritage(&curr_name);

                for candidate_path in candidates {
                    // Skip if candidate file is not loaded
                    let Some(candidate_file) = self.files.get(&candidate_path) else {
                        continue;
                    };

                    // Search this candidate file for implementations
                    let provider =
                        GoToImplementationProvider::from_context(candidate_file.provider_context());

                    let found = provider.find_implementations_for_name(&curr_name, curr_kind);

                    for impl_result in found {
                        // Add the implementation location to results (avoid duplicates)
                        if !results.iter().any(|loc| {
                            loc.file_path == impl_result.location.file_path
                                && loc.range.start.line == impl_result.location.range.start.line
                        }) {
                            results.push(impl_result.location.clone());
                        }

                        // Add to queue for transitive search (find classes that extend this implementation)
                        // Only ConcreteClass and AbstractClass can be extended (not interfaces)
                        let next_kind = TargetKind::ConcreteClass;
                        queue.push_back((
                            candidate_path.clone(),
                            impl_result.name.clone(),
                            next_kind,
                        ));
                    }
                }
            }

            if results.is_empty() {
                None
            } else {
                Some(results)
            }
        })();

        self.performance.record(
            ProjectRequestKind::Implementations,
            start.elapsed(),
            ScopeCacheStats::default(),
        );

        result
    }

    /// Rename a symbol across files in the project.
    pub fn get_rename_edits(
        &mut self,
        file_name: &str,
        position: Position,
        new_name: String,
    ) -> Result<WorkspaceEdit, String> {
        self.touch_file(file_name);
        let start = Instant::now();

        // Step 1: Normalize the new name
        let normalized_name = {
            let file = self
                .files
                .get(file_name)
                .ok_or_else(|| "You cannot rename this element.".to_string())?;
            let provider = RenameProvider::from_context(file.provider_context());
            provider.normalize_rename_at_position(position, &new_name)?
        };

        // Step 2: Resolve the symbol at the cursor position
        let (symbol_id, local_name) = {
            let file = self
                .files
                .get_mut(file_name)
                .ok_or_else(|| "You cannot rename this element.".to_string())?;
            let offset = file
                .line_map
                .position_to_offset(position, file.source_text())
                .ok_or_else(|| "Could not find symbol to rename".to_string())?;
            let node_idx = find_node_at_offset(file.arena(), offset);
            if node_idx.is_none() {
                return Err("Could not find symbol to rename".to_string());
            }

            let finder = FindReferences::new(
                file.parser.get_arena(),
                &file.binder,
                &file.line_map,
                file.file_name.clone(),
                file.parser.get_source_text(),
            );
            let symbol_id = finder
                .resolve_symbol_for_node_with_scope_cache(
                    file.root(),
                    node_idx,
                    &mut file.scope_cache,
                    None,
                )
                .ok_or_else(|| "Could not find symbol to rename".to_string())?;
            let symbol = file
                .binder()
                .symbols
                .get(symbol_id)
                .ok_or_else(|| "Could not find symbol to rename".to_string())?;
            let local_name = symbol.escaped_name.clone();

            (symbol_id, local_name)
        };

        // Step 3: Check if this is a heritage member (class/interface member)
        let is_heritage_member = {
            let file = self
                .files
                .get(file_name)
                .ok_or_else(|| "Could not find file".to_string())?;
            let symbol = file.binder().symbols.get(symbol_id);
            symbol.is_some_and(|s| Self::is_heritage_member_symbol(file, s))
        };

        // Step 4: If heritage member, use heritage-aware rename logic
        if is_heritage_member {
            return self.get_heritage_rename_edits(
                file_name,
                symbol_id,
                &local_name,
                normalized_name,
                start,
            );
        }

        // Step 5: Otherwise, use standard rename logic (imports/exports)
        let scope_stats = ScopeCacheStats::default();
        let result = (|| {
            let (import_targets, export_names, source_file_name) = {
                let file = self
                    .files
                    .get_mut(file_name)
                    .ok_or_else(|| "You cannot rename this element.".to_string())?;
                let import_targets = file.import_targets_for_local(&local_name);
                let export_names = file.exported_names_for_symbol(symbol_id);
                let source_file_name = file.file_name().to_string();
                (import_targets, export_names, source_file_name)
            };

            let mut workspace_edit = {
                let file = self
                    .files
                    .get_mut(file_name)
                    .ok_or_else(|| "You cannot rename this element.".to_string())?;
                let root = file.root();
                let provider = RenameProvider::from_context(file.provider_context());
                provider.provide_rename_edits_for_symbol(
                    root,
                    symbol_id,
                    normalized_name.clone(),
                )?
            };

            let mut cross_targets = Vec::new();

            if !import_targets.is_empty() {
                for target in import_targets {
                    let Some(resolved) =
                        self.resolve_module_specifier(&source_file_name, &target.module_specifier)
                    else {
                        continue;
                    };

                    match target.kind {
                        ImportKind::Named(name) => {
                            if name == local_name {
                                cross_targets.push((resolved, name));
                            }
                        }
                        ImportKind::Default => {
                            cross_targets.push((resolved, "default".to_string()));
                        }
                        ImportKind::Namespace => {}
                    }
                }
            }

            let mut export_names: Vec<String> = export_names
                .into_iter()
                .filter(|name| name == &local_name)
                .collect();
            export_names.sort();
            export_names.dedup();

            for export_name in export_names {
                cross_targets.push((source_file_name.clone(), export_name));
            }

            if cross_targets.is_empty() {
                Self::dedup_workspace_edit(&mut workspace_edit);
                return Ok(workspace_edit);
            }

            let mut pending = cross_targets;
            let mut seen_targets: FxHashSet<(String, String)> = FxHashSet::default();
            let mut namespace_targets = Vec::new();

            while let Some((def_file, export_name)) = pending.pop() {
                if !seen_targets.insert((def_file.clone(), export_name.clone())) {
                    continue;
                }

                if def_file != file_name {
                    let export_nodes = {
                        let target_file = self.files.get(&def_file);
                        target_file
                            .map(|file| file.export_nodes(&export_name))
                            .unwrap_or_default()
                    };
                    if !export_nodes.is_empty()
                        && let Some(target_file) = self.files.get_mut(&def_file)
                    {
                        for node in export_nodes {
                            Self::collect_file_rename_edits(
                                target_file,
                                node,
                                &normalized_name,
                                &mut workspace_edit,
                            );
                        }
                    }
                }

                let mut reexport_refs = Vec::new();
                let (reexports, reexport_namespaces) =
                    self.reexport_targets_for(&def_file, &export_name, &mut reexport_refs);
                for location in reexport_refs {
                    workspace_edit.add_edit(
                        location.file_path,
                        TextEdit::new(location.range, normalized_name.clone()),
                    );
                }

                for (reexport_file, reexport_name) in reexports {
                    if reexport_name == export_name {
                        pending.push((reexport_file, reexport_name));
                    }
                }

                namespace_targets.extend(reexport_namespaces);

                // Pool Scan Optimization: Use SymbolIndex for O(M) candidate filtering
                // Instead of O(N) where N = all files, we get O(M) where M = files containing the symbol
                let candidate_files = self.get_candidate_files_for_symbol(&export_name);

                for other_name in &candidate_files {
                    if other_name == &def_file {
                        continue;
                    }

                    let import_targets = {
                        let other_file = self.files.get(other_name);
                        other_file
                            .map(|file| {
                                self.import_specifier_targets_for_export(
                                    file,
                                    &def_file,
                                    &export_name,
                                )
                            })
                            .unwrap_or_default()
                    };
                    if !import_targets.is_empty()
                        && let Some(other_file) = self.files.get_mut(other_name)
                    {
                        for target in import_targets {
                            if let Some(property_name) = target.property_name {
                                if let Some(location) = other_file.node_location(property_name) {
                                    workspace_edit.add_edit(
                                        location.file_path,
                                        TextEdit::new(location.range, normalized_name.clone()),
                                    );
                                }
                            } else {
                                if other_name == file_name {
                                    continue;
                                }
                                Self::collect_file_rename_edits(
                                    other_file,
                                    target.local_ident,
                                    &normalized_name,
                                    &mut workspace_edit,
                                );
                            }
                        }
                    }

                    let namespace_names = {
                        let other_file = self.files.get(other_name);
                        other_file
                            .map(|file| self.namespace_import_names(file, &def_file))
                            .unwrap_or_default()
                    };
                    if !namespace_names.is_empty()
                        && let Some(other_file) = self.files.get(other_name)
                    {
                        let mut locations = Vec::new();
                        for namespace_name in namespace_names {
                            self.collect_namespace_member_locations(
                                other_file,
                                &namespace_name,
                                &export_name,
                                &mut locations,
                            );
                        }
                        for location in locations {
                            workspace_edit.add_edit(
                                location.file_path,
                                TextEdit::new(location.range, normalized_name.clone()),
                            );
                        }
                    }
                }
            }

            let mut seen_namespace_targets: FxHashSet<(String, String, String)> =
                FxHashSet::default();
            for target in namespace_targets {
                if !seen_namespace_targets.insert((
                    target.file.clone(),
                    target.namespace.clone(),
                    target.member.clone(),
                )) {
                    continue;
                }

                // Pool Scan Optimization: Use SymbolIndex for O(M) candidate filtering
                let candidate_files = self.get_candidate_files_for_symbol(&target.member);

                for other_name in &candidate_files {
                    if other_name == &target.file {
                        continue;
                    }

                    let local_names = {
                        let other_file = self.files.get(other_name);
                        other_file
                            .map(|file| {
                                self.named_import_local_names(file, &target.file, &target.namespace)
                            })
                            .unwrap_or_default()
                    };
                    if local_names.is_empty() {
                        continue;
                    }

                    if let Some(other_file) = self.files.get(other_name) {
                        let mut locations = Vec::new();
                        for local_name in local_names {
                            self.collect_namespace_member_locations(
                                other_file,
                                &local_name,
                                &target.member,
                                &mut locations,
                            );
                        }
                        for location in locations {
                            workspace_edit.add_edit(
                                location.file_path,
                                TextEdit::new(location.range, normalized_name.clone()),
                            );
                        }
                    }
                }
            }

            Self::dedup_workspace_edit(&mut workspace_edit);
            Ok(workspace_edit)
        })();

        self.performance
            .record(ProjectRequestKind::Rename, start.elapsed(), scope_stats);

        result
    }

    /// Heritage-aware rename: Renames a class/interface member across the entire
    /// inheritance hierarchy.
    ///
    /// This handles renaming members that are overridden in derived classes or
    /// override base class members. For example, renaming `Base.foo()` should
    /// also rename `Derived.foo()` when `Derived extends Base`.
    ///
    /// # Arguments
    /// * `file_name` - The file containing the symbol being renamed
    /// * `symbol_id` - The `SymbolId` of the member being renamed
    /// * `local_name` - The current name of the member
    /// * `new_name` - The new name for the member
    /// * `start` - Instant for performance tracking
    ///
    /// # Returns
    /// * `Ok(WorkspaceEdit)` - The workspace edit with all rename changes
    /// * `Err(String)` - Error message if rename failed
    fn get_heritage_rename_edits(
        &mut self,
        file_name: &str,
        symbol_id: tsz_binder::SymbolId,
        local_name: &str,
        new_name: String,
        start: Instant,
    ) -> Result<WorkspaceEdit, String> {
        let mut workspace_edit = WorkspaceEdit::default();

        // Get the file containing the symbol
        let file = self
            .files
            .get(file_name)
            .ok_or_else(|| "Could not find file".to_string())?;

        // Find ALL related symbols in the inheritance hierarchy
        let heritage_symbols = self.find_all_heritage_members(file, symbol_id, local_name);

        // For each heritage symbol, find all its references and generate rename edits
        for (_heritage_file_path, heritage_symbol_id) in heritage_symbols {
            // Use pool scan optimization: get candidate files that contain this symbol name
            let candidate_files = self.get_candidate_files_for_symbol(local_name);

            for target_file_path in candidate_files {
                let target_file = match self.files.get_mut(&target_file_path) {
                    Some(f) => f,
                    None => continue,
                };

                // Create a RenameProvider for this file
                let target_root = target_file.root();
                let provider = RenameProvider::from_context(target_file.provider_context());

                // Get rename edits for this specific heritage symbol in this file
                // Note: We must use the heritage_symbol_id, not the original symbol_id,
                // because Base.foo and Derived.foo are different SymbolIds
                match provider.provide_rename_edits_for_symbol(
                    target_root,
                    heritage_symbol_id,
                    new_name.clone(),
                ) {
                    Ok(edits) => {
                        // Merge the edits into the workspace edit
                        for (file_path, text_edits) in edits.changes {
                            for edit in text_edits {
                                workspace_edit.add_edit(file_path.clone(), edit);
                            }
                        }
                    }
                    Err(_) => {
                        // If we can't find references in this file, continue silently
                        // This can happen if the file doesn't actually reference this symbol
                        continue;
                    }
                }
            }
        }

        // Deduplicate the workspace edit in case multiple symbols produced edits for the same location
        Self::dedup_workspace_edit(&mut workspace_edit);

        self.performance.record(
            ProjectRequestKind::Rename,
            start.elapsed(),
            ScopeCacheStats::default(),
        );

        Ok(workspace_edit)
    }
}
