//! Find References implementation for LSP.
//!
//! Given a position in the source, finds all references to the symbol at that position.
//! Returns detailed reference information including:
//! - `isWriteAccess`: whether the reference writes to the symbol (assignment, declaration, etc.)
//! - `isDefinition`: whether the reference is a definition site (declaration, import, etc.)
//! - `lineText`: the full text of the line containing the reference

use crate::resolver::{ScopeCache, ScopeCacheStats, ScopeWalker};
use crate::utils::find_node_at_offset;
use rustc_hash::FxHashSet;
use tsz_binder::SymbolId;
use tsz_common::position::{Location, Position, Range};
use tsz_parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;

/// Detailed information about a single reference to a symbol.
///
/// Matches the tsserver references response format, including flags for
/// write access and definition detection, plus the line text for previews.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceInfo {
    /// The location (file + range) of this reference.
    #[serde(flatten)]
    pub location: Location,
    /// Whether this reference is a write access (assignment target, declaration, etc.).
    pub is_write_access: bool,
    /// Whether this reference is a definition site (declaration, import binding, etc.).
    pub is_definition: bool,
    /// The full text of the line containing this reference.
    pub line_text: String,
}

impl ReferenceInfo {
    /// Create a new ReferenceInfo.
    pub fn new(
        location: Location,
        is_write_access: bool,
        is_definition: bool,
        line_text: String,
    ) -> Self {
        Self {
            location,
            is_write_access,
            is_definition,
            line_text,
        }
    }
}

/// A rename location entry for findRenameLocations.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameLocation {
    /// The file path containing this rename location.
    pub file_path: String,
    /// The range of text to rename.
    pub range: Range,
    /// The full text of the line containing this rename location.
    pub line_text: String,
}

define_lsp_provider!(binder FindReferences, "Find References provider.");

impl<'a> FindReferences<'a> {
    /// Find all references to the symbol at the given position.
    ///
    /// Returns a list of locations where the symbol is referenced.
    /// This includes both the declaration(s) and all usages.
    ///
    /// Returns None if no symbol is found at the position.
    pub fn find_references(&self, root: NodeIndex, position: Position) -> Option<Vec<Location>> {
        self.find_references_internal(root, position, None, None)
    }

    pub fn find_references_with_scope_cache(
        &self,
        root: NodeIndex,
        position: Position,
        scope_cache: &mut ScopeCache,
        scope_stats: Option<&mut ScopeCacheStats>,
    ) -> Option<Vec<Location>> {
        self.find_references_internal(root, position, Some(scope_cache), scope_stats)
    }

    fn find_references_internal(
        &self,
        root: NodeIndex,
        position: Position,
        scope_cache: Option<&mut ScopeCache>,
        scope_stats: Option<&mut ScopeCacheStats>,
    ) -> Option<Vec<Location>> {
        // 1. Convert position to byte offset
        let offset = self
            .line_map
            .position_to_offset(position, self.source_text)?;

        // 2. Find the most specific node at this offset
        let node_idx = find_node_at_offset(self.arena, offset);
        if node_idx.is_none() {
            return None;
        }

        // 3. Resolve the node to a symbol
        let symbol_id = self.resolve_symbol_internal(root, node_idx, scope_cache, scope_stats)?;

        // 4. Find all references to this symbol
        let mut walker = ScopeWalker::new(self.arena, self.binder);
        let ref_nodes = walker.find_references(root, symbol_id);

        // 5. Also include the declarations
        let symbol = self.binder.symbols.get(symbol_id)?;
        let mut all_nodes = ref_nodes.clone();
        all_nodes.extend(symbol.declarations.iter().copied());

        // Remove duplicates (a declaration might also be a reference)
        all_nodes.sort_by_key(|n| n.0);
        all_nodes.dedup();

        // 6. Convert to Locations
        let locations: Vec<Location> = all_nodes
            .iter()
            .filter_map(|&idx| self.location_for_node(idx))
            .collect();

        if locations.is_empty() {
            None
        } else {
            Some(locations)
        }
    }

    /// Find references for a specific node (by NodeIndex).
    ///
    /// This is useful when you already have the node index from another operation.
    pub fn find_references_for_node(
        &self,
        root: NodeIndex,
        node_idx: NodeIndex,
    ) -> Option<Vec<Location>> {
        self.find_references_for_node_internal(root, node_idx, None, None)
    }

    pub fn find_references_for_node_with_scope_cache(
        &self,
        root: NodeIndex,
        node_idx: NodeIndex,
        scope_cache: &mut ScopeCache,
        scope_stats: Option<&mut ScopeCacheStats>,
    ) -> Option<Vec<Location>> {
        self.find_references_for_node_internal(root, node_idx, Some(scope_cache), scope_stats)
    }

    pub fn find_references_for_symbol(
        &self,
        root: NodeIndex,
        symbol_id: SymbolId,
    ) -> Option<Vec<Location>> {
        if symbol_id.is_none() {
            return None;
        }

        let mut walker = ScopeWalker::new(self.arena, self.binder);
        let ref_nodes = walker.find_references(root, symbol_id);

        let symbol = self.binder.symbols.get(symbol_id)?;
        let mut all_nodes = ref_nodes;
        all_nodes.extend(symbol.declarations.iter().copied());

        all_nodes.sort_by_key(|n| n.0);
        all_nodes.dedup();

        let locations: Vec<Location> = all_nodes
            .iter()
            .filter_map(|&idx| self.location_for_node(idx))
            .collect();

        if locations.is_empty() {
            None
        } else {
            Some(locations)
        }
    }

    fn find_references_for_node_internal(
        &self,
        root: NodeIndex,
        node_idx: NodeIndex,
        scope_cache: Option<&mut ScopeCache>,
        scope_stats: Option<&mut ScopeCacheStats>,
    ) -> Option<Vec<Location>> {
        if node_idx.is_none() {
            return None;
        }

        // Resolve the node to a symbol
        let symbol_id = self.resolve_symbol_internal(root, node_idx, scope_cache, scope_stats)?;

        // Find all references to this symbol
        let mut walker = ScopeWalker::new(self.arena, self.binder);
        let ref_nodes = walker.find_references(root, symbol_id);

        // Also include the declarations
        let symbol = self.binder.symbols.get(symbol_id)?;
        let mut all_nodes = ref_nodes.clone();
        all_nodes.extend(symbol.declarations.iter().copied());

        // Remove duplicates
        all_nodes.sort_by_key(|n| n.0);
        all_nodes.dedup();

        // Convert to Locations
        let locations: Vec<Location> = all_nodes
            .iter()
            .filter_map(|&idx| self.location_for_node(idx))
            .collect();

        if locations.is_empty() {
            None
        } else {
            Some(locations)
        }
    }

    /// Find only usages (excluding declarations) for the symbol at the given position.
    pub fn find_usages_only(&self, root: NodeIndex, position: Position) -> Option<Vec<Location>> {
        self.find_usages_only_internal(root, position, None, None)
    }

    pub fn find_usages_only_with_scope_cache(
        &self,
        root: NodeIndex,
        position: Position,
        scope_cache: &mut ScopeCache,
        scope_stats: Option<&mut ScopeCacheStats>,
    ) -> Option<Vec<Location>> {
        self.find_usages_only_internal(root, position, Some(scope_cache), scope_stats)
    }

    fn find_usages_only_internal(
        &self,
        root: NodeIndex,
        position: Position,
        scope_cache: Option<&mut ScopeCache>,
        scope_stats: Option<&mut ScopeCacheStats>,
    ) -> Option<Vec<Location>> {
        let offset = self
            .line_map
            .position_to_offset(position, self.source_text)?;
        let node_idx = find_node_at_offset(self.arena, offset);
        if node_idx.is_none() {
            return None;
        }

        let symbol_id = self.resolve_symbol_internal(root, node_idx, scope_cache, scope_stats)?;

        // Find all references (usages only, not declarations)
        let mut walker = ScopeWalker::new(self.arena, self.binder);
        let ref_nodes = walker.find_references(root, symbol_id);

        // Convert to Locations
        let locations: Vec<Location> = ref_nodes
            .iter()
            .filter_map(|&idx| self.location_for_node(idx))
            .collect();

        if locations.is_empty() {
            None
        } else {
            Some(locations)
        }
    }

    /// Find all references with detailed information (isWriteAccess, isDefinition, lineText).
    ///
    /// This is the rich version of `find_references` that returns `ReferenceInfo` structs
    /// matching the tsserver response format.
    pub fn find_references_detailed(
        &self,
        root: NodeIndex,
        position: Position,
    ) -> Option<Vec<ReferenceInfo>> {
        let offset = self
            .line_map
            .position_to_offset(position, self.source_text)?;
        let node_idx = find_node_at_offset(self.arena, offset);
        if node_idx.is_none() {
            return None;
        }

        let symbol_id = self.resolve_symbol_internal(root, node_idx, None, None)?;
        let symbol = self.binder.symbols.get(symbol_id)?;
        let declaration_set: FxHashSet<u32> = symbol.declarations.iter().map(|n| n.0).collect();

        let mut walker = ScopeWalker::new(self.arena, self.binder);
        let ref_nodes = walker.find_references(root, symbol_id);

        let mut all_nodes = ref_nodes;
        all_nodes.extend(symbol.declarations.iter().copied());
        all_nodes.sort_by_key(|n| n.0);
        all_nodes.dedup();

        let results: Vec<ReferenceInfo> = all_nodes
            .iter()
            .filter_map(|&idx| {
                let target_idx = self.name_node_for(idx).unwrap_or(idx);
                let location = self.location_for_node(idx)?;
                let is_def = self.is_definition_node(idx, &declaration_set);
                let is_write = is_def || self.is_write_access_node(target_idx);
                let line_text = self.get_line_text(location.range.start.line);
                Some(ReferenceInfo::new(location, is_write, is_def, line_text))
            })
            .collect();

        if results.is_empty() {
            None
        } else {
            Some(results)
        }
    }

    /// Find references with resolved symbol info for the full references protocol.
    /// Returns the resolved SymbolId along with detailed reference info,
    /// which allows the caller to build definition metadata.
    pub fn find_references_with_symbol(
        &self,
        root: NodeIndex,
        position: Position,
    ) -> Option<(SymbolId, Vec<ReferenceInfo>)> {
        let offset = self
            .line_map
            .position_to_offset(position, self.source_text)?;
        let node_idx = find_node_at_offset(self.arena, offset);
        if node_idx.is_none() {
            return None;
        }

        let symbol_id = self.resolve_symbol_internal(root, node_idx, None, None)?;
        let symbol = self.binder.symbols.get(symbol_id)?;
        let declaration_set: FxHashSet<u32> = symbol.declarations.iter().map(|n| n.0).collect();

        let mut walker = ScopeWalker::new(self.arena, self.binder);
        let ref_nodes = walker.find_references(root, symbol_id);

        let mut all_nodes = ref_nodes;
        all_nodes.extend(symbol.declarations.iter().copied());
        all_nodes.sort_by_key(|n| n.0);
        all_nodes.dedup();

        let mut results: Vec<ReferenceInfo> = all_nodes
            .iter()
            .filter_map(|&idx| {
                let target_idx = self.name_node_for(idx).unwrap_or(idx);
                let location = self.location_for_node(idx)?;
                let is_def = self.is_definition_node(idx, &declaration_set);
                let is_write = is_def || self.is_write_access_node(target_idx);
                let line_text = self.get_line_text(location.range.start.line);
                Some(ReferenceInfo::new(location, is_write, is_def, line_text))
            })
            .collect();

        // Deduplicate by location - when declaration node and identifier node
        // resolve to the same position, keep the one with is_definition=true
        results.sort_by(|a, b| {
            a.location
                .range
                .start
                .line
                .cmp(&b.location.range.start.line)
                .then(
                    a.location
                        .range
                        .start
                        .character
                        .cmp(&b.location.range.start.character),
                )
        });
        results.dedup_by(|a, b| a.location.range == b.location.range);

        if results.is_empty() {
            None
        } else {
            Some((symbol_id, results))
        }
    }

    /// Find rename locations for the symbol at the given position.
    ///
    /// Returns all locations where the symbol name appears and should be renamed.
    /// This is similar to find_references but returns `RenameLocation` entries
    /// suitable for the `findRenameLocations` protocol.
    pub fn find_rename_locations(
        &self,
        root: NodeIndex,
        position: Position,
    ) -> Option<Vec<RenameLocation>> {
        let offset = self
            .line_map
            .position_to_offset(position, self.source_text)?;
        let node_idx = find_node_at_offset(self.arena, offset);
        if node_idx.is_none() {
            return None;
        }

        let symbol_id = self.resolve_symbol_internal(root, node_idx, None, None)?;
        let symbol = self.binder.symbols.get(symbol_id)?;

        let mut walker = ScopeWalker::new(self.arena, self.binder);
        let ref_nodes = walker.find_references(root, symbol_id);

        let mut all_nodes = ref_nodes;
        all_nodes.extend(symbol.declarations.iter().copied());
        all_nodes.sort_by_key(|n| n.0);
        all_nodes.dedup();

        let results: Vec<RenameLocation> = all_nodes
            .iter()
            .filter_map(|&idx| {
                let location = self.location_for_node(idx)?;
                let line_text = self.get_line_text(location.range.start.line);
                Some(RenameLocation {
                    file_path: location.file_path,
                    range: location.range,
                    line_text,
                })
            })
            .collect();

        if results.is_empty() {
            None
        } else {
            Some(results)
        }
    }

    /// Determine whether a node represents a write access to a symbol.
    ///
    /// A reference is considered a write access if it is:
    /// - The left-hand side of an assignment expression (`x = 1`, `x += 1`)
    /// - A variable declaration name (`let x = 1`, `const x = 1`, `var x`)
    /// - A function/class/interface/enum/type alias declaration name
    /// - A parameter declaration name (`function foo(x)`)
    /// - An import binding (`import { x }`, `import x from ...`)
    /// - A for-in/for-of loop variable
    /// - A catch clause variable
    /// - A binding element in destructuring patterns
    ///
    /// Uses AST parent-walking for accurate detection, reusing the same
    /// approach as the highlighting module.
    pub fn is_write_access_node(&self, node_idx: NodeIndex) -> bool {
        if node_idx.is_none() {
            return false;
        }

        // Walk up to the parent to determine context
        let parent_idx = self
            .arena
            .get_extended(node_idx)
            .map(|ext| ext.parent)
            .unwrap_or(NodeIndex::NONE);

        if parent_idx.is_none() {
            return false;
        }

        let parent_node = match self.arena.get(parent_idx) {
            Some(n) => n,
            None => return false,
        };

        let pk = parent_node.kind;

        // Variable declaration: `let x = 1` / `const x = 1` / `var x`
        if pk == syntax_kind_ext::VARIABLE_DECLARATION {
            if let Some(decl) = self.arena.get_variable_declaration(parent_node) {
                if decl.name == node_idx {
                    return true;
                }
            }
        }

        // Parameter declaration: `function foo(x)`
        if pk == syntax_kind_ext::PARAMETER {
            if let Some(param) = self.arena.get_parameter(parent_node) {
                if param.name == node_idx {
                    return true;
                }
            }
        }

        // Function declaration: `function foo() {}`
        if pk == syntax_kind_ext::FUNCTION_DECLARATION
            || pk == syntax_kind_ext::FUNCTION_EXPRESSION
            || pk == syntax_kind_ext::ARROW_FUNCTION
        {
            if let Some(func) = self.arena.get_function(parent_node) {
                if func.name == node_idx {
                    return true;
                }
            }
        }

        // Class declaration/expression: `class Foo {}`
        if pk == syntax_kind_ext::CLASS_DECLARATION || pk == syntax_kind_ext::CLASS_EXPRESSION {
            if let Some(class) = self.arena.get_class(parent_node) {
                if class.name == node_idx {
                    return true;
                }
            }
        }

        // Interface declaration: `interface Foo {}`
        if pk == syntax_kind_ext::INTERFACE_DECLARATION {
            if let Some(iface) = self.arena.get_interface(parent_node) {
                if iface.name == node_idx {
                    return true;
                }
            }
        }

        // Type alias declaration: `type Foo = ...`
        if pk == syntax_kind_ext::TYPE_ALIAS_DECLARATION {
            if let Some(alias) = self.arena.get_type_alias(parent_node) {
                if alias.name == node_idx {
                    return true;
                }
            }
        }

        // Enum declaration: `enum Foo {}`
        if pk == syntax_kind_ext::ENUM_DECLARATION {
            if let Some(enm) = self.arena.get_enum(parent_node) {
                if enm.name == node_idx {
                    return true;
                }
            }
        }

        // Enum member: `enum Foo { Bar }`
        if pk == syntax_kind_ext::ENUM_MEMBER {
            if let Some(member) = self.arena.get_enum_member(parent_node) {
                if member.name == node_idx {
                    return true;
                }
            }
        }

        // Module/namespace declaration: `namespace Foo {}`
        if pk == syntax_kind_ext::MODULE_DECLARATION {
            if let Some(module) = self.arena.get_module(parent_node) {
                if module.name == node_idx {
                    return true;
                }
            }
        }

        // Import specifier: `import { x } from ...`
        if pk == syntax_kind_ext::IMPORT_SPECIFIER {
            if let Some(spec) = self.arena.get_specifier(parent_node) {
                // The local name of the import is a write
                if spec.name == node_idx {
                    return true;
                }
            }
        }

        // Import clause (default import): `import x from ...`
        if pk == syntax_kind_ext::IMPORT_CLAUSE {
            if let Some(clause) = self.arena.get_import_clause(parent_node) {
                if clause.name == node_idx {
                    return true;
                }
            }
        }

        // Namespace import: `import * as ns from ...`
        if pk == syntax_kind_ext::NAMESPACE_IMPORT {
            return true;
        }

        // Binding element: `const { x } = obj` or `const [x] = arr`
        if pk == syntax_kind_ext::BINDING_ELEMENT {
            return true;
        }

        // For-in/for-of: `for (const x of arr)` - the initializer's variable is a write
        if pk == syntax_kind_ext::FOR_IN_STATEMENT || pk == syntax_kind_ext::FOR_OF_STATEMENT {
            if let Some(for_data) = self.arena.get_for_in_of(parent_node) {
                if for_data.initializer == node_idx {
                    return true;
                }
            }
        }

        // Catch clause variable: `catch (e)`
        if pk == syntax_kind_ext::CATCH_CLAUSE {
            return true;
        }

        // Binary expression: check if this is the LHS of an assignment
        if pk == syntax_kind_ext::BINARY_EXPRESSION {
            if let Some(binary) = self.arena.get_binary_expr(parent_node) {
                let op = binary.operator_token;
                let is_assignment = op >= SyntaxKind::EqualsToken as u16
                    && op <= SyntaxKind::CaretEqualsToken as u16;
                if is_assignment && binary.left == node_idx {
                    return true;
                }
            }
        }

        // Prefix unary: `++x` or `--x`
        if pk == syntax_kind_ext::PREFIX_UNARY_EXPRESSION {
            if let Some(unary) = self.arena.get_unary_expr(parent_node) {
                if unary.operator == SyntaxKind::PlusPlusToken as u16
                    || unary.operator == SyntaxKind::MinusMinusToken as u16
                {
                    return true;
                }
            }
        }

        // Postfix unary: `x++` or `x--`
        if pk == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION {
            if let Some(unary) = self.arena.get_unary_expr(parent_node) {
                if unary.operator == SyntaxKind::PlusPlusToken as u16
                    || unary.operator == SyntaxKind::MinusMinusToken as u16
                {
                    return true;
                }
            }
        }

        // Property declaration in class: `class Foo { x = 1; }`
        if pk == syntax_kind_ext::PROPERTY_DECLARATION {
            if let Some(prop) = self.arena.get_property_decl(parent_node) {
                if prop.name == node_idx {
                    return true;
                }
            }
        }

        // Method declaration
        if pk == syntax_kind_ext::METHOD_DECLARATION {
            if let Some(method) = self.arena.get_method_decl(parent_node) {
                if method.name == node_idx {
                    return true;
                }
            }
        }

        // Get/Set accessor
        if pk == syntax_kind_ext::GET_ACCESSOR || pk == syntax_kind_ext::SET_ACCESSOR {
            if let Some(accessor) = self.arena.get_accessor(parent_node) {
                if accessor.name == node_idx {
                    return true;
                }
            }
        }

        // Type parameter: `function foo<T>()`
        if pk == syntax_kind_ext::TYPE_PARAMETER {
            if let Some(tp) = self.arena.get_type_parameter(parent_node) {
                if tp.name == node_idx {
                    return true;
                }
            }
        }

        false
    }

    /// Determine whether a node is a definition site for the symbol.
    ///
    /// A reference is considered a definition if it is one of the symbol's
    /// declaration nodes. This covers:
    /// - Variable declarations (`let x`, `const x`, `var x`)
    /// - Function declarations
    /// - Class declarations
    /// - Interface declarations
    /// - Type alias declarations
    /// - Enum declarations
    /// - Import bindings
    /// - Parameter declarations
    /// - Export declarations (but not re-exports of other modules)
    fn is_definition_node(&self, node_idx: NodeIndex, declaration_set: &FxHashSet<u32>) -> bool {
        if node_idx.is_none() {
            return false;
        }
        // A node is a definition if it (or its name-bearing parent) is in the
        // symbol's declaration list.
        if declaration_set.contains(&node_idx.0) {
            return true;
        }

        // The node itself might be the name child of a declaration.
        // Check if its parent is in the declaration set.
        if let Some(ext) = self.arena.get_extended(node_idx) {
            if !ext.parent.is_none() && declaration_set.contains(&ext.parent.0) {
                // Make sure this node is actually the "name" of the parent declaration
                return self.name_node_for(ext.parent) == Some(node_idx);
            }
        }

        false
    }

    /// Get the full text of the line at the given 0-indexed line number.
    fn get_line_text(&self, line: u32) -> String {
        self.source_text
            .lines()
            .nth(line as usize)
            .unwrap_or("")
            .to_string()
    }

    fn location_for_node(&self, idx: NodeIndex) -> Option<Location> {
        let target_idx = self.name_node_for(idx).unwrap_or(idx);
        let node = self.arena.get(target_idx)?;
        let start_pos = self.line_map.offset_to_position(node.pos, self.source_text);
        let end_pos = self.line_map.offset_to_position(node.end, self.source_text);

        Some(Location {
            file_path: self.file_name.clone(),
            range: Range::new(start_pos, end_pos),
        })
    }

    fn name_node_for(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.arena.get(idx)?;
        match node.kind {
            k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
                let decl = self.arena.get_variable_declaration(node)?;
                if decl.name.is_none() {
                    None
                } else {
                    Some(decl.name)
                }
            }
            k if k == syntax_kind_ext::PARAMETER => {
                let param = self.arena.get_parameter(node)?;
                if param.name.is_none() {
                    None
                } else {
                    Some(param.name)
                }
            }
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                let func = self.arena.get_function(node)?;
                if func.name.is_none() {
                    None
                } else {
                    Some(func.name)
                }
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION
                || k == syntax_kind_ext::CLASS_EXPRESSION =>
            {
                let class = self.arena.get_class(node)?;
                if class.name.is_none() {
                    None
                } else {
                    Some(class.name)
                }
            }
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                let iface = self.arena.get_interface(node)?;
                if iface.name.is_none() {
                    None
                } else {
                    Some(iface.name)
                }
            }
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                let alias = self.arena.get_type_alias(node)?;
                if alias.name.is_none() {
                    None
                } else {
                    Some(alias.name)
                }
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                let enm = self.arena.get_enum(node)?;
                if enm.name.is_none() {
                    None
                } else {
                    Some(enm.name)
                }
            }
            k if k == syntax_kind_ext::ENUM_MEMBER => {
                let member = self.arena.get_enum_member(node)?;
                if member.name.is_none() {
                    None
                } else {
                    Some(member.name)
                }
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                let module = self.arena.get_module(node)?;
                if module.name.is_none() {
                    None
                } else {
                    Some(module.name)
                }
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                let method = self.arena.get_method_decl(node)?;
                if method.name.is_none() {
                    None
                } else {
                    Some(method.name)
                }
            }
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                let prop = self.arena.get_property_decl(node)?;
                if prop.name.is_none() {
                    None
                } else {
                    Some(prop.name)
                }
            }
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                let accessor = self.arena.get_accessor(node)?;
                if accessor.name.is_none() {
                    None
                } else {
                    Some(accessor.name)
                }
            }
            k if k == syntax_kind_ext::IMPORT_SPECIFIER => {
                let spec = self.arena.get_specifier(node)?;
                if !spec.name.is_none() {
                    Some(spec.name)
                } else if !spec.property_name.is_none() {
                    Some(spec.property_name)
                } else {
                    None
                }
            }
            k if k == syntax_kind_ext::EXPORT_SPECIFIER => {
                let spec = self.arena.get_specifier(node)?;
                if !spec.property_name.is_none() {
                    Some(spec.property_name)
                } else if !spec.name.is_none() {
                    Some(spec.name)
                } else {
                    None
                }
            }
            k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                let import = self.arena.get_import_decl(node)?;
                if import.import_clause.is_none() {
                    return None;
                }
                let clause_node = self.arena.get(import.import_clause)?;
                if clause_node.kind == SyntaxKind::Identifier as u16 {
                    Some(import.import_clause)
                } else {
                    None
                }
            }
            k if k == syntax_kind_ext::TYPE_PARAMETER => {
                let param = self.arena.get_type_parameter(node)?;
                if param.name.is_none() {
                    None
                } else {
                    Some(param.name)
                }
            }
            _ => None,
        }
    }

    pub(crate) fn resolve_symbol_for_node_with_scope_cache(
        &self,
        root: NodeIndex,
        node_idx: NodeIndex,
        scope_cache: &mut ScopeCache,
        scope_stats: Option<&mut ScopeCacheStats>,
    ) -> Option<SymbolId> {
        self.resolve_symbol_internal(root, node_idx, Some(scope_cache), scope_stats)
    }

    fn resolve_symbol_internal(
        &self,
        root: NodeIndex,
        node_idx: NodeIndex,
        mut scope_cache: Option<&mut ScopeCache>,
        mut scope_stats: Option<&mut ScopeCacheStats>,
    ) -> Option<SymbolId> {
        let mut walker = ScopeWalker::new(self.arena, self.binder);
        let symbol_id = if let Some(scope_cache) = scope_cache.as_deref_mut() {
            walker.resolve_node_cached(root, node_idx, scope_cache, scope_stats.as_deref_mut())
        } else {
            walker.resolve_node(root, node_idx)
        };

        if symbol_id.is_some() {
            return symbol_id;
        }

        // Fallback: if the cursor is on a keyword (not an identifier), walk up to
        // the parent declaration node and look it up in node_symbols.
        if let Some(sym) = self.try_keyword_declaration_fallback(node_idx) {
            return Some(sym);
        }

        let tag_idx = self.tagged_template_tag(node_idx)?;
        let mut walker = ScopeWalker::new(self.arena, self.binder);
        if let Some(scope_cache) = scope_cache {
            walker.resolve_node_cached(root, tag_idx, scope_cache, scope_stats)
        } else {
            walker.resolve_node(root, tag_idx)
        }
    }

    /// When the cursor is on a keyword (class, function, declare, etc.) that's part
    /// of a declaration, resolve to the declaration's symbol.
    fn try_keyword_declaration_fallback(&self, node_idx: NodeIndex) -> Option<SymbolId> {
        use tsz_parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        let node = self.arena.get(node_idx)?;
        let kind = node.kind;

        // Only apply to keyword nodes, not identifiers, literals, or other tokens
        let is_keyword =
            kind >= SyntaxKind::BreakKeyword as u16 && kind <= SyntaxKind::DeferKeyword as u16;
        if !is_keyword {
            return None;
        }

        // Walk up to parent nodes looking for a declaration
        let mut current = node_idx;
        for _ in 0..5 {
            let ext = self.arena.get_extended(current)?;
            if ext.parent.is_none() {
                break;
            }
            current = ext.parent;

            // Check if this parent node has a symbol in node_symbols
            if let Some(&sym_id) = self.binder.node_symbols.get(&current.0) {
                return Some(sym_id);
            }

            // Also check for specific declaration node kinds
            let parent = self.arena.get(current)?;
            match parent.kind {
                syntax_kind_ext::VARIABLE_STATEMENT
                | syntax_kind_ext::FUNCTION_DECLARATION
                | syntax_kind_ext::CLASS_DECLARATION
                | syntax_kind_ext::INTERFACE_DECLARATION
                | syntax_kind_ext::TYPE_ALIAS_DECLARATION
                | syntax_kind_ext::ENUM_DECLARATION
                | syntax_kind_ext::MODULE_DECLARATION
                | syntax_kind_ext::METHOD_DECLARATION
                | syntax_kind_ext::METHOD_SIGNATURE
                | syntax_kind_ext::PROPERTY_DECLARATION
                | syntax_kind_ext::PROPERTY_SIGNATURE => {
                    // Check node_symbols for this declaration
                    if let Some(&sym_id) = self.binder.node_symbols.get(&current.0) {
                        return Some(sym_id);
                    }
                    break; // Don't walk further up
                }
                _ => {}
            }
        }
        None
    }

    fn tagged_template_tag(&self, node_idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.arena.get(node_idx)?;
        let is_template_node = matches!(
            node.kind,
            k if k == syntax_kind_ext::TEMPLATE_EXPRESSION
                || k == syntax_kind_ext::TEMPLATE_SPAN
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                || k == SyntaxKind::TemplateHead as u16
                || k == SyntaxKind::TemplateMiddle as u16
                || k == SyntaxKind::TemplateTail as u16
        );

        if !is_template_node {
            return None;
        }

        let mut current = node_idx;
        while let Some(ext) = self.arena.get_extended(current) {
            let parent = ext.parent;
            if parent.is_none() {
                break;
            }
            let parent_node = self.arena.get(parent)?;
            if parent_node.kind == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION {
                return self
                    .arena
                    .tagged_templates
                    .get(parent_node.data_index as usize)
                    .map(|tagged| tagged.tag);
            }
            current = parent;
        }

        None
    }
}

#[cfg(test)]
#[path = "tests/references_tests.rs"]
mod references_tests;
