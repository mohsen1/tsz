//! Find References implementation for LSP.
//!
//! Given a position in the source, finds all references to the symbol at that position.
//! Returns detailed reference information including:
//! - `isWriteAccess`: whether the reference writes to the symbol (assignment, declaration, etc.)
//! - `isDefinition`: whether the reference is a definition site (declaration, import, etc.)
//! - `lineText`: the full text of the line containing the reference

use crate::binder::SymbolId;
use crate::lsp::position::{Location, Position, Range};
use crate::lsp::resolver::{ScopeCache, ScopeCacheStats, ScopeWalker};
use crate::lsp::utils::find_node_at_offset;
use crate::parser::{NodeIndex, syntax_kind_ext};
use crate::scanner::SyntaxKind;
use rustc_hash::FxHashSet;

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
        use crate::parser::syntax_kind_ext;
        use crate::scanner::SyntaxKind;

        let node = self.arena.get(node_idx)?;
        let kind = node.kind;

        // Only apply to keyword nodes (not identifiers)
        if kind == SyntaxKind::Identifier as u16 || kind == SyntaxKind::PrivateIdentifier as u16 {
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
mod references_tests {
    use super::*;
    use crate::binder::BinderState;
    use crate::lsp::position::LineMap;
    use crate::parser::ParserState;

    #[test]
    fn test_find_references_simple() {
        // const x = 1;
        // x + x;
        let source = "const x = 1;\nx + x;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the first 'x' in "x + x" (line 1, column 0)
        let position = Position::new(1, 0);

        let find_refs =
            FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let references = find_refs.find_references(root, position);

        assert!(references.is_some(), "Should find references for x");

        if let Some(refs) = references {
            // Should find at least the declaration and two usages
            assert!(
                refs.len() >= 2,
                "Should find at least 2 references (declaration + usages)"
            );
        }
    }

    #[test]
    fn test_find_references_for_symbol() {
        let source = "const x = 1;\nx + x;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let symbol_id = binder.file_locals.get("x").expect("Expected symbol for x");

        let line_map = LineMap::build(source);
        let find_refs =
            FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let references = find_refs.find_references_for_symbol(root, symbol_id);

        assert!(references.is_some(), "Should find references for x");
        if let Some(refs) = references {
            assert!(
                refs.len() >= 2,
                "Should find at least 2 references (declaration + usages)"
            );
        }
    }

    #[test]
    fn test_find_references_not_found() {
        let source = "const x = 1;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position outside any identifier
        let position = Position::new(0, 11); // At the semicolon

        let find_refs =
            FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let references = find_refs.find_references(root, position);

        // Should not find references
        assert!(
            references.is_none(),
            "Should not find references at semicolon"
        );
    }

    #[test]
    fn test_find_references_template_expression() {
        let source = "const name = \"Ada\";\nconst msg = `hi ${name}`;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'name' inside the template expression (line 1)
        let position = Position::new(1, 18);

        let find_refs =
            FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let references = find_refs.find_references(root, position);

        assert!(
            references.is_some(),
            "Should find references in template expression"
        );
        let refs = references.unwrap();
        assert!(
            refs.len() >= 2,
            "Should find declaration and template usage"
        );
    }

    #[test]
    fn test_find_references_jsx_expression() {
        let source = "const name = \"Ada\";\nconst el = <div>{name}</div>;";
        let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'name' inside JSX expression (line 1)
        let position = Position::new(1, 17);

        let find_refs =
            FindReferences::new(arena, &binder, &line_map, "test.tsx".to_string(), source);
        let references = find_refs.find_references(root, position);

        assert!(
            references.is_some(),
            "Should find references in JSX expression"
        );
        let refs = references.unwrap();
        assert!(refs.len() >= 2, "Should find declaration and JSX usage");
    }

    #[test]
    fn test_find_references_await_expression() {
        let source = "const value = 1;\nasync function run() {\n  await value;\n}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'value' inside await (line 2)
        let position = Position::new(2, 8);

        let find_refs =
            FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let references = find_refs.find_references(root, position);

        assert!(
            references.is_some(),
            "Should find references in await expression"
        );
        let refs = references.unwrap();
        assert!(refs.len() >= 2, "Should find declaration and await usage");
    }

    #[test]
    fn test_find_references_tagged_template_expression() {
        let source =
            "const tag = (strings: TemplateStringsArray) => strings[0];\nconst msg = tag`hello`;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'tag' inside tagged template (line 1)
        let position = Position::new(1, 16);

        let find_refs =
            FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let references = find_refs.find_references(root, position);

        assert!(
            references.is_some(),
            "Should find references in tagged template"
        );
        let refs = references.unwrap();
        assert!(
            refs.len() >= 2,
            "Should find declaration and tagged template usage"
        );
    }

    #[test]
    fn test_find_references_as_expression() {
        let source = "const value = 1;\nconst result = value as number;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'value' inside the as-expression (line 1)
        let position = Position::new(1, 15);

        let find_refs =
            FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let references = find_refs.find_references(root, position);

        assert!(
            references.is_some(),
            "Should find references in as expression"
        );
        let refs = references.unwrap();
        assert!(
            refs.len() >= 2,
            "Should find declaration and as-expression usage"
        );
    }

    #[test]
    fn test_find_references_binding_pattern() {
        let source = "const { foo } = obj;\nfoo;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'foo' usage (line 1)
        let position = Position::new(1, 0);

        let find_refs =
            FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let references = find_refs.find_references(root, position);

        assert!(
            references.is_some(),
            "Should find references for binding pattern name"
        );
        let refs = references.unwrap();
        assert!(refs.len() >= 2, "Should find declaration and usage");
    }

    #[test]
    fn test_find_references_binding_pattern_initializer() {
        let source = "const value = 1;\nconst { foo = value } = obj;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'value' inside the initializer (line 1)
        let position = Position::new(1, 14);

        let find_refs =
            FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let references = find_refs.find_references(root, position);

        assert!(
            references.is_some(),
            "Should find references in binding pattern initializer"
        );
        let refs = references.unwrap();
        assert!(
            refs.len() >= 2,
            "Should find declaration and initializer usage"
        );
    }

    #[test]
    fn test_find_references_parameter_binding_pattern() {
        let source = "function demo({ foo }: { foo: number }) {\n  return foo;\n}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'foo' usage in the return (line 1)
        let position = Position::new(1, 9);

        let find_refs =
            FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let references = find_refs.find_references(root, position);

        assert!(
            references.is_some(),
            "Should find references for parameter binding name"
        );
        let refs = references.unwrap();
        assert!(
            refs.len() >= 2,
            "Should find parameter declaration and usage"
        );
    }

    #[test]
    fn test_find_references_parameter_array_binding() {
        let source = "function demo([foo]: number[]) {\n  return foo;\n}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'foo' usage in the return (line 1)
        let position = Position::new(1, 9);

        let find_refs =
            FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let references = find_refs.find_references(root, position);

        assert!(
            references.is_some(),
            "Should find references for array binding name"
        );
        let refs = references.unwrap();
        assert!(
            refs.len() >= 2,
            "Should find parameter declaration and usage"
        );
    }

    #[test]
    fn test_find_references_nested_arrow_in_switch_case() {
        let source = "switch (state) {\n  case (() => {\n    const value = 1;\n    return value;\n  })():\n    break;\n}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'value' usage (line 3)
        let position = Position::new(3, 11);

        let find_refs =
            FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let references = find_refs.find_references(root, position);

        assert!(
            references.is_some(),
            "Should find references for switch case locals"
        );
        let refs = references.unwrap();
        assert!(refs.len() >= 2, "Should find declaration and usage");
    }

    #[test]
    fn test_find_references_nested_arrow_in_if_condition() {
        let source = "if ((() => {\n  const value = 1;\n  return value;\n})()) {}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'value' usage (line 2)
        let position = Position::new(2, 9);

        let find_refs =
            FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let references = find_refs.find_references(root, position);

        assert!(
            references.is_some(),
            "Should find references for nested arrow locals in condition"
        );
        let refs = references.unwrap();
        assert!(refs.len() >= 2, "Should find declaration and usage");
    }

    #[test]
    fn test_find_references_export_default_expression() {
        let source = "export default (() => {\n  const value = 1;\n  return value;\n})();";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'value' usage (line 2)
        let position = Position::new(2, 9);

        let find_refs =
            FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let references = find_refs.find_references(root, position);

        assert!(
            references.is_some(),
            "Should find references for export default expression locals"
        );
        let refs = references.unwrap();
        assert!(refs.len() >= 2, "Should find declaration and usage");
    }

    #[test]
    fn test_find_references_labeled_statement_local() {
        let source = "label: {\n  const value = 1;\n  value;\n}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'value' usage (line 2)
        let position = Position::new(2, 2);

        let find_refs =
            FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let references = find_refs.find_references(root, position);

        assert!(
            references.is_some(),
            "Should find references for labeled statement locals"
        );
        let refs = references.unwrap();
        assert!(refs.len() >= 2, "Should find declaration and usage");
    }

    #[test]
    fn test_find_references_with_statement_local() {
        let source = "with (obj) {\n  const value = 1;\n  value;\n}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'value' usage (line 2)
        let position = Position::new(2, 2);

        let find_refs =
            FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let references = find_refs.find_references(root, position);

        assert!(
            references.is_some(),
            "Should find references for with statement locals"
        );
        let refs = references.unwrap();
        assert!(refs.len() >= 2, "Should find declaration and usage");
    }

    #[test]
    fn test_find_references_var_hoisted_in_nested_block() {
        let source = "function demo() {\n  value;\n  if (cond) {\n    var value = 1;\n  }\n}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'value' usage before the declaration (line 1)
        let position = Position::new(1, 2);

        let find_refs =
            FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let references = find_refs.find_references(root, position);

        assert!(
            references.is_some(),
            "Should find references for hoisted var"
        );
        let refs = references.unwrap();
        assert!(refs.len() >= 2, "Should find declaration and usage");
    }

    #[test]
    fn test_find_references_decorator_reference() {
        let source = "const deco = () => {};\n@deco\nclass Foo {}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'deco' usage in the decorator (line 1)
        let position = Position::new(1, 1);

        let find_refs =
            FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let references = find_refs.find_references(root, position);

        assert!(
            references.is_some(),
            "Should find references for decorator usage"
        );
        let refs = references.unwrap();
        assert!(refs.len() >= 2, "Should find declaration and usage");
    }

    #[test]
    fn test_find_references_class_method_local() {
        let source = "class Foo {\n  method() {\n    const value = 1;\n    return value;\n  }\n}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'value' usage (line 3)
        let position = Position::new(3, 11);

        let find_refs =
            FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let references = find_refs.find_references(root, position);

        assert!(
            references.is_some(),
            "Should find references for method local"
        );
        let refs = references.unwrap();
        assert!(refs.len() >= 2, "Should find declaration and usage");
    }

    #[test]
    fn test_find_references_class_self_reference() {
        let source = "class Foo {\n  method() {\n    return Foo;\n  }\n}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'Foo' usage inside the method (line 2)
        let position = Position::new(2, 11);

        let find_refs =
            FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let references = find_refs.find_references(root, position);

        assert!(
            references.is_some(),
            "Should find references for class self name"
        );
        let refs = references.unwrap();
        assert!(refs.len() >= 2, "Should find declaration and usage");
    }

    #[test]
    fn test_find_references_class_expression_name() {
        let source = "const Foo = class Bar {\n  method() {\n    return Bar;\n  }\n};";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'Bar' usage inside the method (line 2)
        let position = Position::new(2, 11);

        let find_refs =
            FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let references = find_refs.find_references(root, position);

        assert!(
            references.is_some(),
            "Should find references for class expression name"
        );
        let refs = references.unwrap();
        assert!(refs.len() >= 2, "Should find declaration and usage");
    }

    #[test]
    fn test_find_references_class_static_block_local() {
        let source = "class Foo {\n  static {\n    const value = 1;\n    value;\n  }\n}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'value' usage inside the static block (line 3)
        let position = Position::new(3, 4);

        let find_refs =
            FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let references = find_refs.find_references(root, position);

        assert!(
            references.is_some(),
            "Should find references for static block locals"
        );
        let refs = references.unwrap();
        assert!(refs.len() >= 2, "Should find declaration and usage");
    }

    // =========================================================================
    // Tests for ReferenceInfo: isWriteAccess, isDefinition, lineText
    // =========================================================================

    /// Helper to get detailed references for a symbol at a given position.
    fn get_detailed_refs(source: &str, file_name: &str, line: u32, col: u32) -> Vec<ReferenceInfo> {
        let mut parser = ParserState::new(file_name.to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let position = Position::new(line, col);

        let find_refs =
            FindReferences::new(arena, &binder, &line_map, file_name.to_string(), source);
        find_refs
            .find_references_detailed(root, position)
            .unwrap_or_default()
    }

    #[test]
    fn test_detailed_refs_const_declaration_is_write_and_definition() {
        // `const x = 1; x + x;`
        // The declaration of x should be isWriteAccess=true, isDefinition=true
        // The usages of x should be isWriteAccess=false, isDefinition=false
        let source = "const x = 1;\nx + x;";
        let refs = get_detailed_refs(source, "test.ts", 1, 0);

        assert!(
            refs.len() >= 2,
            "Should find at least 2 references, got {}",
            refs.len()
        );

        // Find the declaration ref (on line 0, which is "const x = 1;")
        let decl_ref = refs.iter().find(|r| r.location.range.start.line == 0);
        assert!(
            decl_ref.is_some(),
            "Should have a ref on line 0 (declaration)"
        );
        let decl_ref = decl_ref.unwrap();
        assert!(
            decl_ref.is_write_access,
            "Declaration should be a write access"
        );
        assert!(decl_ref.is_definition, "Declaration should be a definition");

        // Find a usage ref (on line 1, which is "x + x;")
        let usage_refs: Vec<_> = refs
            .iter()
            .filter(|r| r.location.range.start.line == 1)
            .collect();
        assert!(
            !usage_refs.is_empty(),
            "Should have at least one usage ref on line 1"
        );
        for ur in &usage_refs {
            assert!(
                !ur.is_write_access,
                "Read-only usage should not be a write access"
            );
            assert!(!ur.is_definition, "Usage should not be a definition");
        }
    }

    #[test]
    fn test_detailed_refs_assignment_is_write_access() {
        // `let x = 1; x = 2;`
        // The assignment `x = 2` should be isWriteAccess=true, isDefinition=false
        let source = "let x = 1;\nx = 2;";
        let refs = get_detailed_refs(source, "test.ts", 0, 4);

        assert!(
            refs.len() >= 2,
            "Should find at least 2 references, got {}",
            refs.len()
        );

        // The ref on line 1 ("x = 2;") is an assignment - should be write
        let assign_ref = refs.iter().find(|r| r.location.range.start.line == 1);
        assert!(
            assign_ref.is_some(),
            "Should have a ref on line 1 (assignment)"
        );
        let assign_ref = assign_ref.unwrap();
        assert!(
            assign_ref.is_write_access,
            "Assignment target should be a write access"
        );
        assert!(!assign_ref.is_definition, "Assignment is not a definition");
    }

    #[test]
    fn test_detailed_refs_compound_assignment_is_write_access() {
        // `let x = 0; x += 1;`
        // The compound assignment `x += 1` should be isWriteAccess=true
        let source = "let x = 0;\nx += 1;";
        let refs = get_detailed_refs(source, "test.ts", 0, 4);

        assert!(
            refs.len() >= 2,
            "Should find at least 2 references, got {}",
            refs.len()
        );

        let compound_ref = refs.iter().find(|r| r.location.range.start.line == 1);
        assert!(
            compound_ref.is_some(),
            "Should have a ref on line 1 (compound assignment)"
        );
        let compound_ref = compound_ref.unwrap();
        assert!(
            compound_ref.is_write_access,
            "Compound assignment target should be a write access"
        );
        assert!(
            !compound_ref.is_definition,
            "Compound assignment is not a definition"
        );
    }

    #[test]
    fn test_detailed_refs_function_declaration_is_definition() {
        // `function foo() {} foo();`
        // The function name at declaration is isDefinition=true, isWriteAccess=true
        // The call site is isDefinition=false, isWriteAccess=false
        let source = "function foo() {}\nfoo();";
        let refs = get_detailed_refs(source, "test.ts", 1, 0);

        assert!(
            refs.len() >= 2,
            "Should find at least 2 references, got {}",
            refs.len()
        );

        // The declaration on line 0
        let decl_ref = refs.iter().find(|r| r.location.range.start.line == 0);
        assert!(
            decl_ref.is_some(),
            "Should have a ref on line 0 (declaration)"
        );
        let decl_ref = decl_ref.unwrap();
        assert!(
            decl_ref.is_definition,
            "Function declaration name should be a definition"
        );
        assert!(
            decl_ref.is_write_access,
            "Function declaration name should be a write access"
        );

        // The call on line 1
        let call_ref = refs.iter().find(|r| r.location.range.start.line == 1);
        assert!(call_ref.is_some(), "Should have a ref on line 1 (call)");
        let call_ref = call_ref.unwrap();
        assert!(
            !call_ref.is_definition,
            "Function call should not be a definition"
        );
        assert!(
            !call_ref.is_write_access,
            "Function call should not be a write access"
        );
    }

    #[test]
    fn test_detailed_refs_class_declaration_is_definition() {
        // `class Foo {} new Foo();`
        let source = "class Foo {}\nnew Foo();";
        let refs = get_detailed_refs(source, "test.ts", 0, 6);

        assert!(
            refs.len() >= 2,
            "Should find at least 2 references, got {}",
            refs.len()
        );

        let decl_ref = refs.iter().find(|r| r.location.range.start.line == 0);
        assert!(decl_ref.is_some(), "Should have declaration ref");
        let decl_ref = decl_ref.unwrap();
        assert!(
            decl_ref.is_definition,
            "Class declaration should be a definition"
        );
        assert!(
            decl_ref.is_write_access,
            "Class declaration should be a write access"
        );

        let usage_ref = refs.iter().find(|r| r.location.range.start.line == 1);
        assert!(usage_ref.is_some(), "Should have usage ref");
        let usage_ref = usage_ref.unwrap();
        assert!(
            !usage_ref.is_definition,
            "new Foo() should not be a definition"
        );
        assert!(
            !usage_ref.is_write_access,
            "new Foo() should not be a write access"
        );
    }

    #[test]
    fn test_detailed_refs_parameter_is_write_and_definition() {
        // `function foo(x: number) { return x; }`
        // Parameter x declaration is isWriteAccess=true, isDefinition=true
        // Usage of x in body is isWriteAccess=false, isDefinition=false
        let source = "function foo(x: number) {\n  return x;\n}";
        let refs = get_detailed_refs(source, "test.ts", 1, 9);

        assert!(
            refs.len() >= 2,
            "Should find at least 2 references, got {}",
            refs.len()
        );

        // The parameter declaration (line 0)
        let param_ref = refs.iter().find(|r| r.location.range.start.line == 0);
        assert!(param_ref.is_some(), "Should have param ref on line 0");
        let param_ref = param_ref.unwrap();
        assert!(
            param_ref.is_definition,
            "Parameter declaration should be a definition"
        );
        assert!(
            param_ref.is_write_access,
            "Parameter declaration should be a write access"
        );

        // The usage in the body (line 1)
        let body_ref = refs.iter().find(|r| r.location.range.start.line == 1);
        assert!(body_ref.is_some(), "Should have body ref on line 1");
        let body_ref = body_ref.unwrap();
        assert!(
            !body_ref.is_definition,
            "Parameter usage should not be a definition"
        );
        assert!(
            !body_ref.is_write_access,
            "Parameter read should not be a write access"
        );
    }

    #[test]
    fn test_detailed_refs_line_text_is_correct() {
        // Verify lineText contains the correct line content
        let source = "const x = 1;\nconsole.log(x);";
        let refs = get_detailed_refs(source, "test.ts", 0, 6);

        assert!(
            refs.len() >= 2,
            "Should find at least 2 references, got {}",
            refs.len()
        );

        let decl_ref = refs.iter().find(|r| r.location.range.start.line == 0);
        assert!(decl_ref.is_some(), "Should have ref on line 0");
        assert_eq!(
            decl_ref.unwrap().line_text,
            "const x = 1;",
            "lineText should be the full line content"
        );

        let usage_ref = refs.iter().find(|r| r.location.range.start.line == 1);
        assert!(usage_ref.is_some(), "Should have ref on line 1");
        assert_eq!(
            usage_ref.unwrap().line_text,
            "console.log(x);",
            "lineText should be the full line content"
        );
    }

    #[test]
    fn test_detailed_refs_interface_declaration_is_definition() {
        // `interface Foo { x: number; } let a: Foo;`
        let source = "interface Foo {\n  x: number;\n}\nlet a: Foo;";
        let refs = get_detailed_refs(source, "test.ts", 0, 10);

        assert!(!refs.is_empty(), "Should find at least 1 reference");

        let decl_ref = refs.iter().find(|r| r.location.range.start.line == 0);
        assert!(decl_ref.is_some(), "Should have declaration ref");
        let decl_ref = decl_ref.unwrap();
        assert!(
            decl_ref.is_definition,
            "Interface declaration should be a definition"
        );
        assert!(
            decl_ref.is_write_access,
            "Interface declaration should be a write access"
        );
    }

    #[test]
    fn test_detailed_refs_enum_declaration_is_definition() {
        // `enum Color { Red } let c = Color.Red;`
        let source = "enum Color {\n  Red\n}\nlet c = Color.Red;";
        let refs = get_detailed_refs(source, "test.ts", 0, 5);

        assert!(!refs.is_empty(), "Should find at least 1 reference");

        let decl_ref = refs.iter().find(|r| r.location.range.start.line == 0);
        assert!(decl_ref.is_some(), "Should have declaration ref on line 0");
        let decl_ref = decl_ref.unwrap();
        assert!(
            decl_ref.is_definition,
            "Enum declaration should be a definition"
        );
        assert!(
            decl_ref.is_write_access,
            "Enum declaration should be a write access"
        );
    }

    #[test]
    fn test_detailed_refs_type_alias_is_definition() {
        // `type Foo = number; let x: Foo;`
        let source = "type Foo = number;\nlet x: Foo;";
        let refs = get_detailed_refs(source, "test.ts", 0, 5);

        assert!(!refs.is_empty(), "Should find at least 1 reference");

        let decl_ref = refs.iter().find(|r| r.location.range.start.line == 0);
        assert!(decl_ref.is_some(), "Should have declaration ref on line 0");
        let decl_ref = decl_ref.unwrap();
        assert!(
            decl_ref.is_definition,
            "Type alias declaration should be a definition"
        );
        assert!(
            decl_ref.is_write_access,
            "Type alias declaration should be a write access"
        );
    }

    #[test]
    fn test_detailed_refs_read_in_expression_not_write() {
        // `let x = 1; let y = x + 2;`
        // x in the expression `x + 2` should be isWriteAccess=false
        let source = "let x = 1;\nlet y = x + 2;";
        let refs = get_detailed_refs(source, "test.ts", 0, 4);

        assert!(
            refs.len() >= 2,
            "Should find at least 2 references, got {}",
            refs.len()
        );

        let expr_ref = refs
            .iter()
            .find(|r| r.location.range.start.line == 1 && !r.is_definition);
        assert!(expr_ref.is_some(), "Should have a read usage ref on line 1");
        let expr_ref = expr_ref.unwrap();
        assert!(
            !expr_ref.is_write_access,
            "Read in expression should not be write access"
        );
    }

    // =========================================================================
    // Tests for find_rename_locations
    // =========================================================================

    #[test]
    fn test_rename_locations_simple() {
        let source = "const x = 1;\nx + x;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let position = Position::new(1, 0);

        let find_refs =
            FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let locations = find_refs.find_rename_locations(root, position);

        assert!(locations.is_some(), "Should find rename locations for x");
        let locs = locations.unwrap();
        assert!(
            locs.len() >= 2,
            "Should find at least 2 rename locations (declaration + usages)"
        );

        // Each location should have a line_text
        for loc in &locs {
            assert!(
                !loc.line_text.is_empty(),
                "Rename location should have non-empty line_text"
            );
        }
    }
}
