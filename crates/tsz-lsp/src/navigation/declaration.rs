//! Go-to-Declaration implementation for LSP.
//!
//! Provides `textDocument/declaration` which navigates to the *declaration*
//! of a symbol (e.g., `declare function foo(): void`), as opposed to
//! `textDocument/definition` which prefers the *implementation*.
//!
//! In TypeScript this distinction matters for:
//! - Ambient declarations (`declare` keyword)
//! - `.d.ts` files vs `.ts` files
//! - Interface declarations vs class implementations

use crate::resolver::ScopeWalker;
use crate::utils::{find_node_at_offset, is_symbol_query_node};
use tsz_common::position::{Location, Position, Range};
use tsz_parser::NodeIndex;
use tsz_parser::syntax_kind_ext;

define_lsp_provider!(binder GoToDeclarationProvider, "Provider for Go to Declaration.");

impl<'a> GoToDeclarationProvider<'a> {
    /// Get the declaration location for the symbol at the given position.
    ///
    /// Returns locations of declaration sites for the symbol. This prefers
    /// ambient declarations (`declare` keyword), interface declarations,
    /// and `.d.ts` type declarations over implementation sites.
    ///
    /// Falls back to all declarations if no ambient declarations are found.
    pub fn get_declaration(&self, root: NodeIndex, position: Position) -> Option<Vec<Location>> {
        let offset = self
            .line_map
            .position_to_offset(position, self.source_text)?;

        let node_idx = find_node_at_offset(self.arena, offset);
        if node_idx.is_none() {
            return None;
        }
        if !is_symbol_query_node(self.arena, node_idx) {
            return None;
        }

        let mut walker = ScopeWalker::new(self.arena, self.binder);
        let symbol_id = walker.resolve_node(root, node_idx)?;

        let symbol = self.binder.symbols.get(symbol_id)?;

        if symbol.declarations.is_empty() {
            return None;
        }

        // Prefer ambient/interface declarations (the "declaration" sites)
        let ambient_locations: Vec<Location> = symbol
            .declarations
            .iter()
            .filter(|&&decl_idx| self.is_declaration_site(decl_idx))
            .filter_map(|&decl_idx| self.location_for_declaration(decl_idx))
            .collect();

        if !ambient_locations.is_empty() {
            return Some(ambient_locations);
        }

        // Fall back to all declarations
        let all_locations: Vec<Location> = symbol
            .declarations
            .iter()
            .filter_map(|&decl_idx| self.location_for_declaration(decl_idx))
            .collect();

        if all_locations.is_empty() {
            None
        } else {
            Some(all_locations)
        }
    }

    /// Check if a declaration node is a "declaration site" (ambient, interface, etc.)
    /// rather than an implementation site.
    fn is_declaration_site(&self, decl_idx: NodeIndex) -> bool {
        let node = match self.arena.get(decl_idx) {
            Some(n) => n,
            None => return false,
        };

        // Interface declarations are always declaration sites
        if node.kind == syntax_kind_ext::INTERFACE_DECLARATION {
            return true;
        }

        // Type alias declarations are declaration sites
        if node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION {
            return true;
        }

        // Check for `declare` modifier (ambient declarations)
        if self.has_declare_modifier(decl_idx) {
            return true;
        }

        // Check if the file is a .d.ts file
        if self.file_name.ends_with(".d.ts") {
            return true;
        }

        false
    }

    /// Check if a declaration node has the `declare` modifier.
    fn has_declare_modifier(&self, decl_idx: NodeIndex) -> bool {
        // Check the node's modifiers for DeclareKeyword
        let node = match self.arena.get(decl_idx) {
            Some(n) => n,
            None => return false,
        };

        // Walk children looking for DeclareKeyword
        for (i, child) in self.arena.nodes.iter().enumerate() {
            let idx = NodeIndex(i as u32);
            let parent = self
                .arena
                .get_extended(idx)
                .map_or(NodeIndex::NONE, |ext| ext.parent);

            if parent == decl_idx && child.kind == tsz_scanner::SyntaxKind::DeclareKeyword as u16 {
                return true;
            }

            // Only check nodes within the declaration's span
            if child.pos > node.end {
                break;
            }
        }
        false
    }

    /// Get the location for a declaration node, focusing on the name.
    fn location_for_declaration(&self, decl_idx: NodeIndex) -> Option<Location> {
        let name_idx = self.name_node_for_declaration(decl_idx).unwrap_or(decl_idx);
        let node = self.arena.get(name_idx)?;

        let start_pos = self.line_map.offset_to_position(node.pos, self.source_text);
        let end_pos = self.line_map.offset_to_position(node.end, self.source_text);

        Some(Location {
            file_path: self.file_name.clone(),
            range: Range::new(start_pos, end_pos),
        })
    }

    /// Get the name node for a declaration (the identifier).
    fn name_node_for_declaration(&self, decl_idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.arena.get(decl_idx)?;
        match node.kind {
            k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
                let decl = self.arena.get_variable_declaration(node)?;
                if decl.name.is_some() {
                    Some(decl.name)
                } else {
                    None
                }
            }
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                let func = self.arena.get_function(node)?;
                if func.name.is_some() {
                    Some(func.name)
                } else {
                    None
                }
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                let class = self.arena.get_class(node)?;
                if class.name.is_some() {
                    Some(class.name)
                } else {
                    None
                }
            }
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                let iface = self.arena.get_interface(node)?;
                if iface.name.is_some() {
                    Some(iface.name)
                } else {
                    None
                }
            }
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                let alias = self.arena.get_type_alias(node)?;
                if alias.name.is_some() {
                    Some(alias.name)
                } else {
                    None
                }
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                let enm = self.arena.get_enum(node)?;
                if enm.name.is_some() {
                    Some(enm.name)
                } else {
                    None
                }
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                let module = self.arena.get_module(node)?;
                if module.name.is_some() {
                    Some(module.name)
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

#[cfg(test)]
#[path = "../../tests/declaration_tests.rs"]
mod declaration_tests;
