//! Exported-value-declaration detection for property access.
//!
//! Extracted from `helpers.rs` to keep both modules under the 2000-LOC
//! arch-guard limit. These methods remain inherent methods on `CheckerState`,
//! so call sites are unchanged.

use crate::state::CheckerState;
use tsz_binder::symbol_flags;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> CheckerState<'a> {
    /// Check if a symbol has any exported value declarations.
    ///
    /// For merged symbols (e.g., namespace + interface with same name), only the
    /// interface part may be exported while the namespace is not. This helper
    /// checks whether any VALUE-contributing declaration (namespace, function,
    /// class, etc.) has an export modifier.
    ///
    /// Returns `true` if:
    /// - The symbol has no TYPE flags (pure value symbol - trust `is_exported`)
    /// - The symbol has at least one value declaration with export modifier
    pub(crate) fn symbol_has_exported_value_declaration(
        &self,
        sym_id: tsz_binder::SymbolId,
    ) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        // If the symbol only has VALUE flags (no TYPE flags), we can trust is_exported
        let has_type_flags = symbol.has_any_flags(symbol_flags::TYPE);
        if !has_type_flags {
            return symbol.is_exported;
        }

        // For symbols that are both VALUE and TYPE by design (CLASS, ENUM, ENUM_MEMBER),
        // not due to merging with an interface/type-alias, we can trust is_exported.
        // Enum members are considered exported if they're in the enum's exports table.
        // We only need special handling for namespace + interface/type-alias merges.
        let is_merged_with_type_only =
            symbol.has_any_flags(symbol_flags::INTERFACE | symbol_flags::TYPE_ALIAS);
        if !is_merged_with_type_only {
            // Enum members may not have is_exported set, but they're accessible
            // if they're in the enum's exports table (which they must be to get here)
            if symbol.has_any_flags(symbol_flags::ENUM_MEMBER) {
                return true;
            }
            return symbol.is_exported;
        }

        // For lib symbols (decl_file_idx == u32::MAX), trust is_exported since
        // lib declarations have proper export semantics by construction.
        if symbol.decl_file_idx == u32::MAX {
            return symbol.is_exported;
        }

        // For cross-file merged symbols, trust is_exported since declarations
        // may be in different arenas. The cross-file merge logic in the binder
        // correctly tracks export status.
        if self.ctx.all_arenas.is_some() {
            // Check if this looks like a cross-file merged symbol by seeing if
            // any declarations can't be found in the current arena
            let has_cross_file_decl = symbol
                .declarations
                .iter()
                .any(|&decl_idx| self.ctx.arena.get(decl_idx).is_none());
            if has_cross_file_decl {
                return symbol.is_exported;
            }
        }

        // Single-file merged symbol - check declarations individually
        for &decl_idx in &symbol.declarations {
            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };

            // Check if this is a value declaration with export modifier
            if let Some(true) =
                self.check_value_decl_has_export_in_arena(self.ctx.arena, decl_idx, decl_node)
            {
                return true;
            }
        }

        tracing::debug!(
            "symbol_has_exported_value_declaration: returning false for {:?}",
            symbol.escaped_name
        );
        false
    }

    /// Check if a declaration node has an export modifier using a specific arena.
    /// Also checks if the declaration is wrapped in an `EXPORT_DECLARATION` node,
    /// since `export namespace B` creates an `EXPORT_DECLARATION` wrapping `MODULE_DECLARATION`.
    fn check_value_decl_has_export_in_arena(
        &self,
        arena: &tsz_parser::parser::node::NodeArena,
        decl_idx: tsz_parser::NodeIndex,
        decl_node: &tsz_parser::parser::node::Node,
    ) -> Option<bool> {
        // Helper to check if a node is wrapped in an EXPORT_DECLARATION
        let is_inside_export_decl = || -> bool {
            // Get parent node from extended info
            if let Some(ext) = arena.get_extended(decl_idx)
                && let Some(parent_node) = arena.get(ext.parent)
                && parent_node.kind == syntax_kind_ext::EXPORT_DECLARATION
            {
                return true;
            }
            false
        };

        // Helper to check if the declaration is inside a `declare` context (ambient).
        // In ambient contexts, members are implicitly exported.
        let is_inside_declare_context = || -> bool {
            let mut current = decl_idx;
            for _ in 0..10 {
                let Some(ext) = arena.get_extended(current) else {
                    break;
                };
                let Some(parent_node) = arena.get(ext.parent) else {
                    break;
                };
                // Check if parent is a module with `declare` modifier
                if parent_node.kind == syntax_kind_ext::MODULE_DECLARATION
                    && let Some(m) = arena.get_module(parent_node)
                    && m.modifiers
                        .as_ref()
                        .is_some_and(|mods| arena.is_declare_ref(Some(mods)))
                {
                    return true;
                }
                current = ext.parent;
            }
            false
        };

        match decl_node.kind {
            syntax_kind_ext::MODULE_DECLARATION => {
                let module = arena.get_module(decl_node);
                if let Some(m) = module {
                    // Check direct modifiers, parent EXPORT_DECLARATION, or ambient context
                    let has_direct_export = m.modifiers.as_ref().is_some_and(|mods| {
                        arena.has_modifier_ref(Some(mods), SyntaxKind::ExportKeyword)
                    });
                    let has_declare = m
                        .modifiers
                        .as_ref()
                        .is_some_and(|mods| arena.is_declare_ref(Some(mods)));
                    Some(
                        has_direct_export
                            || has_declare
                            || is_inside_export_decl()
                            || is_inside_declare_context(),
                    )
                } else {
                    None
                }
            }
            syntax_kind_ext::FUNCTION_DECLARATION => arena.get_function(decl_node).map(|f| {
                let has_direct_export = f.modifiers.as_ref().is_some_and(|mods| {
                    arena.has_modifier_ref(Some(mods), SyntaxKind::ExportKeyword)
                });
                let has_declare = f
                    .modifiers
                    .as_ref()
                    .is_some_and(|mods| arena.is_declare_ref(Some(mods)));
                has_direct_export
                    || has_declare
                    || is_inside_export_decl()
                    || is_inside_declare_context()
            }),
            syntax_kind_ext::CLASS_DECLARATION => arena.get_class(decl_node).map(|c| {
                let has_direct_export = c.modifiers.as_ref().is_some_and(|mods| {
                    arena.has_modifier_ref(Some(mods), SyntaxKind::ExportKeyword)
                });
                let has_declare = c
                    .modifiers
                    .as_ref()
                    .is_some_and(|mods| arena.is_declare_ref(Some(mods)));
                has_direct_export
                    || has_declare
                    || is_inside_export_decl()
                    || is_inside_declare_context()
            }),
            syntax_kind_ext::ENUM_DECLARATION => arena.get_enum(decl_node).map(|e| {
                let has_direct_export = e.modifiers.as_ref().is_some_and(|mods| {
                    arena.has_modifier_ref(Some(mods), SyntaxKind::ExportKeyword)
                });
                let has_declare = e
                    .modifiers
                    .as_ref()
                    .is_some_and(|mods| arena.is_declare_ref(Some(mods)));
                has_direct_export
                    || has_declare
                    || is_inside_export_decl()
                    || is_inside_declare_context()
            }),
            syntax_kind_ext::VARIABLE_DECLARATION => {
                // For variable declarations, check if inside a declare context
                // (e.g., `declare namespace Foo { var x: number; }`)
                // The export modifier is on the parent VARIABLE_STATEMENT, not the declaration itself.
                // Walk up: VARIABLE_DECLARATION -> VARIABLE_DECLARATION_LIST -> VARIABLE_STATEMENT
                // and check if the VARIABLE_STATEMENT has an `export` modifier.
                let has_export_on_var_stmt = || -> bool {
                    // Walk from VariableDeclaration up to VariableStatement
                    let Some(ext1) = arena.get_extended(decl_idx) else {
                        return false;
                    };
                    // ext1.parent = VariableDeclarationList
                    let Some(ext2) = arena.get_extended(ext1.parent) else {
                        return false;
                    };
                    // ext2.parent = VariableStatement
                    let Some(var_stmt_node) = arena.get(ext2.parent) else {
                        return false;
                    };
                    if var_stmt_node.kind != syntax_kind_ext::VARIABLE_STATEMENT {
                        return false;
                    }
                    arena
                        .get_variable(var_stmt_node)
                        .and_then(|v| v.modifiers.as_ref())
                        .is_some_and(|mods| {
                            arena.has_modifier_ref(Some(mods), SyntaxKind::ExportKeyword)
                        })
                };
                Some(
                    has_export_on_var_stmt()
                        || is_inside_export_decl()
                        || is_inside_declare_context(),
                )
            }
            _ => Some(false), // Skip non-value declarations (interface, type alias)
        }
    }
}
