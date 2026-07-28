//! Exact source-file import binding lookup.
//!
//! Library stamping can place a cloned global in `file_locals` under the same
//! text as a source import. The per-node symbol map still records which alias
//! the import declaration introduced, so direct lowering must consult it first.

use tsz_binder::{BinderState, SymbolId, symbol_flags};
use tsz_parser::NodeIndex;
use tsz_parser::parser::node::{NodeAccess, NodeArena};
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

fn alias_symbol_for_binding(
    binder: &BinderState,
    binding: NodeIndex,
    declaration: NodeIndex,
) -> Option<SymbolId> {
    binder
        .get_node_symbol(binding)
        .or_else(|| binder.get_node_symbol(declaration))
        .filter(|&sym_id| {
            binder
                .get_symbol(sym_id)
                .is_some_and(|symbol| symbol.has_any_flags(symbol_flags::ALIAS))
        })
}

/// Return the alias symbol introduced by a top-level import binding named
/// `name`, using the binding node rather than the potentially shadowed
/// `file_locals` entry.
pub(crate) fn source_file_import_binding_symbol(
    arena: &NodeArena,
    binder: &BinderState,
    name: &str,
) -> Option<SymbolId> {
    if let Some(file_local) = binder.file_locals.get(name)
        && let Some(symbol) = binder.get_symbol(file_local)
    {
        if symbol.has_any_flags(symbol_flags::ALIAS) && symbol.import_module().is_some() {
            return Some(file_local);
        }
        // A genuine source declaration cannot be displaced by another valid
        // import of the same local name. Only cloned library globals (or
        // recovery aliases without external-module metadata) need the syntax
        // fallback below.
        if !symbol.has_any_flags(symbol_flags::ALIAS)
            && !binder.lib_symbol_ids.contains(&file_local)
        {
            return None;
        }
    }

    for source_file in &arena.source_files {
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt) = arena.get(stmt_idx) else {
                continue;
            };
            if stmt.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
                let Some(import) = arena.get_import_decl(stmt) else {
                    continue;
                };
                if arena.get_identifier_text(import.import_clause) == Some(name)
                    && let Some(sym_id) =
                        alias_symbol_for_binding(binder, import.import_clause, stmt_idx)
                {
                    return Some(sym_id);
                }
                continue;
            }
            if stmt.kind != syntax_kind_ext::IMPORT_DECLARATION {
                continue;
            }
            let Some(import) = arena.get_import_decl(stmt) else {
                continue;
            };
            let Some(clause_node) = arena.get(import.import_clause) else {
                continue;
            };
            let Some(clause) = arena.get_import_clause(clause_node) else {
                continue;
            };

            if arena.get_identifier_text(clause.name) == Some(name)
                && let Some(sym_id) =
                    alias_symbol_for_binding(binder, clause.name, import.import_clause)
            {
                return Some(sym_id);
            }

            let Some(bindings_node) = arena.get(clause.named_bindings) else {
                continue;
            };
            if bindings_node.kind == SyntaxKind::Identifier as u16 {
                if arena.get_identifier_text(clause.named_bindings) == Some(name)
                    && let Some(sym_id) = alias_symbol_for_binding(
                        binder,
                        clause.named_bindings,
                        clause.named_bindings,
                    )
                {
                    return Some(sym_id);
                }
                continue;
            }
            if bindings_node.kind == syntax_kind_ext::NAMESPACE_IMPORT {
                if let Some(namespace) = arena.get_named_imports(bindings_node)
                    && arena.get_identifier_text(namespace.name) == Some(name)
                    && let Some(sym_id) =
                        alias_symbol_for_binding(binder, namespace.name, clause.named_bindings)
                {
                    return Some(sym_id);
                }
                continue;
            }
            if bindings_node.kind != syntax_kind_ext::NAMED_IMPORTS {
                continue;
            }
            let Some(named) = arena.get_named_imports(bindings_node) else {
                continue;
            };
            for &specifier_idx in &named.elements.nodes {
                let Some(specifier_node) = arena.get(specifier_idx) else {
                    continue;
                };
                let Some(specifier) = arena.get_specifier(specifier_node) else {
                    continue;
                };
                let binding = if specifier.name.is_some() {
                    specifier.name
                } else {
                    specifier.property_name
                };
                if arena.get_identifier_text(binding) == Some(name)
                    && let Some(sym_id) = alias_symbol_for_binding(binder, binding, specifier_idx)
                {
                    return Some(sym_id);
                }
            }
        }
    }
    None
}
