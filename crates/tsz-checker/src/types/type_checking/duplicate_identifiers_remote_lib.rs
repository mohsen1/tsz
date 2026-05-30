//! Remote default-lib duplicate identifier diagnostic helpers.
//!
//! Extracted from `duplicate_identifiers_helpers.rs` to keep that module
//! under 2000 LOC. All methods here are `impl CheckerState` helpers called
//! from `check_duplicate_identifiers` or its sub-routines.

use super::duplicate_identifiers::DuplicateDeclarationOrigin;
use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> CheckerState<'a> {
    pub(super) fn emit_remote_default_lib_duplicate_identifier_diagnostics(
        &mut self,
        sym_id: tsz_binder::SymbolId,
        declarations: &[(
            NodeIndex,
            u32,
            bool,
            bool,
            super::duplicate_identifiers::DuplicateDeclarationOrigin,
        )],
        conflicts: &FxHashSet<NodeIndex>,
        code: u32,
        message: &str,
    ) {
        use crate::diagnostics::diagnostic_codes;

        if code != diagnostic_codes::DUPLICATE_IDENTIFIER {
            return;
        }

        let local_conflicts: Vec<(NodeIndex, u32)> = declarations
            .iter()
            .filter_map(|(decl_idx, flags, is_local, _, _)| {
                (*is_local && conflicts.contains(decl_idx)).then_some((*decl_idx, *flags))
            })
            .collect();
        if local_conflicts.is_empty() {
            return;
        }
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return;
        };
        let name = symbol.escaped_name.clone();

        let mut emitted_at: FxHashSet<(String, u32)> = FxHashSet::default();
        for (remote_idx, remote_flags, is_local, _, origin) in declarations {
            if *is_local || *origin != DuplicateDeclarationOrigin::SymbolDeclaration {
                continue;
            }
            if (remote_flags & symbol_flags::ALIAS) != 0 {
                continue;
            }

            if let Some(arenas) = self
                .ctx
                .binder
                .declaration_arenas
                .get(&(sym_id, *remote_idx))
            {
                let arenas: Vec<_> = arenas.iter().cloned().collect();
                for arena_arc in arenas {
                    let arena: &tsz_parser::parser::NodeArena = &arena_arc;
                    self.emit_remote_default_lib_duplicate_identifier_for_arena(
                        arena,
                        *remote_idx,
                        &local_conflicts,
                        message,
                        code,
                        &mut emitted_at,
                    );
                }
            }
        }
        if let Some(all_arenas) = self.ctx.all_arenas.as_ref() {
            let arenas: Vec<_> = all_arenas.iter().cloned().collect();
            for arena_arc in arenas {
                let arena: &tsz_parser::parser::NodeArena = &arena_arc;
                self.emit_named_remote_default_lib_duplicate_identifiers_in_arena(
                    arena,
                    &name,
                    message,
                    code,
                    &mut emitted_at,
                );
            }
        }
        let lib_arenas: Vec<_> = self
            .ctx
            .lib_contexts
            .iter()
            .map(|lib_ctx| std::sync::Arc::clone(&lib_ctx.arena))
            .collect();
        for arena_arc in lib_arenas {
            let arena: &tsz_parser::parser::NodeArena = &arena_arc;
            self.emit_named_remote_default_lib_duplicate_identifiers_in_arena(
                arena,
                &name,
                message,
                code,
                &mut emitted_at,
            );
        }
    }

    fn emit_named_remote_default_lib_duplicate_identifiers_in_arena(
        &mut self,
        arena: &tsz_parser::parser::NodeArena,
        name: &str,
        message: &str,
        code: u32,
        emitted_at: &mut FxHashSet<(String, u32)>,
    ) {
        if std::ptr::eq(arena, self.ctx.arena) {
            return;
        }
        let Some(sf) = arena.source_files.first() else {
            return;
        };
        if !is_default_lib_file_name(&sf.file_name) {
            return;
        }

        for (remote_idx, remote_flags) in self.default_lib_declarations_in_arena(arena, name) {
            if (remote_flags & symbol_flags::ALIAS) != 0 {
                continue;
            }
            let Some(remote_node) = arena.get(remote_idx) else {
                continue;
            };
            let name_idx = remote_declaration_name_node(arena, remote_node, remote_idx);
            let Some(name_node) = arena.get(name_idx) else {
                continue;
            };
            let start = name_node.pos;
            let length = name_node.end.saturating_sub(start);
            if !emitted_at.insert((sf.file_name.clone(), start)) {
                continue;
            }
            self.error_at_position_in_file(sf.file_name.clone(), start, length, message, code);
        }
    }

    fn default_lib_declarations_in_arena(
        &self,
        arena: &tsz_parser::parser::NodeArena,
        name: &str,
    ) -> Vec<(NodeIndex, u32)> {
        let Some(source_file) = arena.source_files.first() else {
            return Vec::new();
        };

        let mut declarations = Vec::new();
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
                if let Some(var_stmt) = arena.get_variable(stmt_node) {
                    for &dl_idx in &var_stmt.declarations.nodes {
                        let Some(dl_node) = arena.get(dl_idx) else {
                            continue;
                        };
                        let Some(dl_data) = arena.get_variable(dl_node) else {
                            continue;
                        };
                        for &vd_idx in &dl_data.declarations.nodes {
                            let Some(vd_node) = arena.get(vd_idx) else {
                                continue;
                            };
                            let Some(var_decl) = arena.get_variable_declaration(vd_node) else {
                                continue;
                            };
                            if arena
                                .get_identifier_at(var_decl.name)
                                .is_some_and(|ident| ident.escaped_text == name)
                                && let Some(flags) = self.declaration_symbol_flags(arena, vd_idx)
                            {
                                declarations.push((vd_idx, flags));
                            }
                        }
                    }
                }
                continue;
            }

            let matches_name = match stmt_node.kind {
                syntax_kind_ext::FUNCTION_DECLARATION => arena
                    .get_function(stmt_node)
                    .and_then(|decl| arena.get_identifier_at(decl.name))
                    .is_some_and(|ident| ident.escaped_text == name),
                syntax_kind_ext::CLASS_DECLARATION => arena
                    .get_class(stmt_node)
                    .and_then(|decl| arena.get_identifier_at(decl.name))
                    .is_some_and(|ident| ident.escaped_text == name),
                syntax_kind_ext::INTERFACE_DECLARATION => arena
                    .get_interface(stmt_node)
                    .and_then(|decl| arena.get_identifier_at(decl.name))
                    .is_some_and(|ident| ident.escaped_text == name),
                _ => false,
            };
            if matches_name && let Some(flags) = self.declaration_symbol_flags(arena, stmt_idx) {
                declarations.push((stmt_idx, flags));
            }
        }
        declarations
    }

    fn emit_remote_default_lib_duplicate_identifier_for_arena(
        &mut self,
        arena: &tsz_parser::parser::NodeArena,
        remote_idx: NodeIndex,
        local_conflicts: &[(NodeIndex, u32)],
        message: &str,
        code: u32,
        emitted_at: &mut FxHashSet<(String, u32)>,
    ) {
        if std::ptr::eq(arena, self.ctx.arena) {
            return;
        }
        let Some(sf) = arena.source_files.first() else {
            return;
        };
        if !is_default_lib_file_name(&sf.file_name) {
            return;
        }
        let Some(remote_node) = arena.get(remote_idx) else {
            return;
        };
        let Some(actual_remote_flags) = self.declaration_symbol_flags(arena, remote_idx) else {
            return;
        };
        let remote_conflict_flags =
            self.normalize_duplicate_conflict_flags(arena, remote_idx, actual_remote_flags);
        let conflicts_with_local = local_conflicts.iter().any(|(local_idx, local_flags)| {
            let local_conflict_flags =
                self.normalize_duplicate_conflict_flags(self.ctx.arena, *local_idx, *local_flags);
            Self::declarations_conflict(local_conflict_flags, remote_conflict_flags)
        });
        if !conflicts_with_local {
            return;
        }

        let name_idx = remote_declaration_name_node(arena, remote_node, remote_idx);
        let Some(name_node) = arena.get(name_idx) else {
            return;
        };
        let start = name_node.pos;
        let length = name_node.end.saturating_sub(start);
        if !emitted_at.insert((sf.file_name.clone(), start)) {
            return;
        }
        self.error_at_position_in_file(sf.file_name.clone(), start, length, message, code);
    }

    /// Mirror tsc's `addDuplicateLocations` plain-JS suppression for
    /// cross-file duplicate-identifier conflicts. Returns `true` when the
    /// local plain-JS site was suppressed and a remote anchor emitted.
    pub(super) fn try_redirect_dup_id_to_non_plain_js_remote(
        &mut self,
        sym_id: tsz_binder::SymbolId,
        declarations: &[(
            NodeIndex,
            u32,
            bool,
            bool,
            super::duplicate_identifiers::DuplicateDeclarationOrigin,
        )],
        conflicts: &FxHashSet<NodeIndex>,
        code: u32,
        message: &str,
    ) -> bool {
        use crate::context::should_resolve_jsdoc_for_file;
        use crate::diagnostics::diagnostic_codes;

        if code != diagnostic_codes::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE {
            return false;
        }
        let local_is_plain_js = self.is_js_file() && !self.ctx.should_resolve_jsdoc();
        if !local_is_plain_js {
            return false;
        }
        let has_local_conflict = declarations
            .iter()
            .any(|(decl_idx, _, is_local, _, _)| *is_local && conflicts.contains(decl_idx));
        if !has_local_conflict {
            return false;
        }

        let mut emitted_at: FxHashSet<(String, u32)> = FxHashSet::default();
        let mut emitted = false;
        for (decl_idx, _, is_local, _, _) in declarations {
            if *is_local {
                continue;
            }
            let Some(arenas) = self.ctx.binder.declaration_arenas.get(&(sym_id, *decl_idx)) else {
                continue;
            };
            for arena_arc in arenas {
                let arena: &tsz_parser::parser::NodeArena = arena_arc;
                if std::ptr::eq(arena, self.ctx.arena) {
                    continue;
                }
                let Some(sf) = arena.source_files.first() else {
                    continue;
                };
                if !should_resolve_jsdoc_for_file(
                    &sf.file_name,
                    sf.text.as_ref(),
                    &self.ctx.compiler_options,
                ) {
                    continue;
                }
                let Some(remote_node) = arena.get(*decl_idx) else {
                    continue;
                };
                let name_idx = remote_declaration_name_node(arena, remote_node, *decl_idx);
                let Some(name_node) = arena.get(name_idx) else {
                    continue;
                };
                let start = name_node.pos;
                let length = name_node.end.saturating_sub(start);
                if !emitted_at.insert((sf.file_name.clone(), start)) {
                    continue;
                }
                self.error_at_position_in_file(sf.file_name.clone(), start, length, message, code);
                emitted = true;
            }
        }
        emitted
    }
}

fn is_default_lib_file_name(file_name: &str) -> bool {
    let base = file_name.rsplit(['/', '\\']).next().unwrap_or(file_name);
    base.starts_with("lib.") && base.ends_with(".d.ts")
}

/// Resolve `decl_idx` to its declaration name node within `arena`. Mirrors
/// `CheckerState::get_declaration_name_node` but operates on an arbitrary
/// arena. Falls back to `decl_idx` itself when the declaration kind is not
/// recognized.
fn remote_declaration_name_node(
    arena: &tsz_parser::parser::NodeArena,
    remote_node: &tsz_parser::parser::node::Node,
    decl_idx: NodeIndex,
) -> NodeIndex {
    use tsz_scanner::SyntaxKind;
    match remote_node.kind {
        syntax_kind_ext::FUNCTION_DECLARATION => arena.get_function(remote_node).map(|d| d.name),
        syntax_kind_ext::VARIABLE_DECLARATION => {
            arena.get_variable_declaration(remote_node).map(|d| d.name)
        }
        syntax_kind_ext::CLASS_DECLARATION => arena.get_class(remote_node).map(|d| d.name),
        syntax_kind_ext::INTERFACE_DECLARATION => arena.get_interface(remote_node).map(|d| d.name),
        syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
            arena.get_type_alias(remote_node).map(|d| d.name)
        }
        syntax_kind_ext::ENUM_DECLARATION => arena.get_enum(remote_node).map(|d| d.name),
        syntax_kind_ext::MODULE_DECLARATION => arena.get_module(remote_node).map(|d| d.name),
        k if k == SyntaxKind::Identifier as u16 => Some(decl_idx),
        _ => None,
    }
    .unwrap_or(decl_idx)
}
