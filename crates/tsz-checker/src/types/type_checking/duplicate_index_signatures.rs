//! Cross-arena duplicate index-signature detection (TS2374).
//!
//! When a user-file interface merges with a default-lib interface that also
//! declares a same-kind index signature, tsc reports TS2374 on **every**
//! participating signature — including the lib-side one. The existing checker
//! paths (`check_index_signature_compatibility` for within-body duplicates and
//! the local-merge branch in `check_merged_interface_declaration_diagnostics`)
//! only emit on user-arena nodes, so lib-side signatures are silently missed.
//!
//! This module fills that gap with a single cross-arena pass invoked from
//! `check_source_file` after `check_duplicate_identifiers`.

use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

#[derive(Copy, Clone, PartialEq, Eq)]
enum IndexParamKind {
    Number,
    String,
    Symbol,
}

const fn kind_label(kind: IndexParamKind) -> &'static str {
    match kind {
        IndexParamKind::Number => "number",
        IndexParamKind::String => "string",
        IndexParamKind::Symbol => "symbol",
    }
}

/// Emission data for one index-signature occurrence. We pre-resolve the
/// `(file_name, start, length, is_local)` tuple before invoking any
/// `&mut self` emit helper so the borrow over `self.ctx` ends first.
struct SigEmitInfo {
    file_name: String,
    start: u32,
    length: u32,
    is_local: bool,
}

impl<'a> CheckerState<'a> {
    /// Emit TS2374 "Duplicate index signature for type 'X'." on the lib bodies
    /// of merged interface declarations whose combined index-signature count
    /// is `>= 2`.
    ///
    /// Structural rule: when two or more index signatures of the same key kind
    /// (`number`, `string`, or `symbol`) are declared across all merged bodies
    /// of an interface — including bodies in the default-lib arena — tsc
    /// reports TS2374 on each of them. Local-body emissions are produced by
    /// the existing checker paths and must not be duplicated here.
    pub(crate) fn check_lib_merged_interface_duplicate_index_signatures(&mut self) {
        let has_libs = self.ctx.has_lib_loaded() || !self.ctx.binder.lib_symbol_ids.is_empty();
        if !has_libs {
            return;
        }

        // Collect interface symbols declared in user code (exclude pure lib symbols).
        let user_syms: FxHashSet<tsz_binder::SymbolId> =
            self.ctx.binder.node_symbols.values().copied().collect();
        let mut symbol_ids: FxHashSet<tsz_binder::SymbolId> = FxHashSet::default();
        if !self.ctx.binder.scopes.is_empty() {
            for scope in self.ctx.binder.scopes.iter() {
                if scope.kind == tsz_binder::ContainerKind::Class {
                    continue;
                }
                for (_, &id) in scope.table.iter() {
                    if user_syms.contains(&id) {
                        symbol_ids.insert(id);
                    }
                }
            }
        } else {
            for (_, &id) in self.ctx.binder.file_locals.iter() {
                if user_syms.contains(&id) {
                    symbol_ids.insert(id);
                }
            }
        }

        for sym_id in symbol_ids {
            self.emit_lib_merged_interface_duplicate_index_signatures_for(sym_id);
        }
    }

    fn emit_lib_merged_interface_duplicate_index_signatures_for(
        &mut self,
        sym_id: tsz_binder::SymbolId,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        let (decl_indices, is_interface) = {
            let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
                return;
            };
            (
                symbol.declarations.clone(),
                symbol.has_any_flags(symbol_flags::INTERFACE),
            )
        };
        if !is_interface {
            return;
        }

        let mut number_sigs: Vec<SigEmitInfo> = Vec::new();
        let mut string_sigs: Vec<SigEmitInfo> = Vec::new();
        let mut symbol_sigs: Vec<SigEmitInfo> = Vec::new();

        let mut has_local_decl = false;
        let mut has_remote_decl = false;
        for decl_idx in &decl_indices {
            if let Some(arenas) = self.ctx.binder.declaration_arenas.get(&(sym_id, *decl_idx)) {
                for arena in arenas {
                    let is_local = std::ptr::eq(&**arena, self.ctx.arena);
                    if is_local {
                        has_local_decl = true;
                    } else {
                        has_remote_decl = true;
                    }
                    scan_arena(
                        arena,
                        *decl_idx,
                        is_local,
                        &mut number_sigs,
                        &mut string_sigs,
                        &mut symbol_sigs,
                    );
                }
            } else {
                // Declaration not registered in the cross-arena map: it lives
                // in the current file's arena. The binder only populates
                // `declaration_arenas` for cross-file/lib merges.
                has_local_decl = true;
                scan_arena(
                    self.ctx.arena,
                    *decl_idx,
                    true,
                    &mut number_sigs,
                    &mut string_sigs,
                    &mut symbol_sigs,
                );
            }
        }

        // Need at least one local + one remote (lib) body to be relevant —
        // pure local merges are handled by the existing local paths.
        if !has_local_decl || !has_remote_decl {
            return;
        }

        // For any kind whose merged count is >= 2, emit TS2374 at each REMOTE
        // (lib) body's index signature. Local-body emissions are produced by
        // the existing checker paths and must not be duplicated here.
        let mut emit_for = |sigs: &[SigEmitInfo], kind: IndexParamKind| {
            if sigs.len() < 2 {
                return;
            }
            let message = format_message(
                diagnostic_messages::DUPLICATE_INDEX_SIGNATURE_FOR_TYPE,
                &[kind_label(kind)],
            );
            let mut emitted_at: FxHashSet<(String, u32)> = FxHashSet::default();
            for sig in sigs {
                if sig.is_local {
                    continue;
                }
                if !emitted_at.insert((sig.file_name.clone(), sig.start)) {
                    continue;
                }
                self.error_at_position_in_file(
                    sig.file_name.clone(),
                    sig.start,
                    sig.length,
                    &message,
                    diagnostic_codes::DUPLICATE_INDEX_SIGNATURE_FOR_TYPE,
                );
            }
        };
        emit_for(&number_sigs, IndexParamKind::Number);
        emit_for(&string_sigs, IndexParamKind::String);
        emit_for(&symbol_sigs, IndexParamKind::Symbol);
    }
}

/// Walk the index signatures inside one interface declaration body, classify
/// each by the syntactic shape of its parameter type annotation, and append
/// the resulting `SigEmitInfo` to the matching per-kind vector.
///
/// Index signatures use either a built-in keyword type (`number`/`string`/
/// `symbol`) or — much more common in this parser — a `TypeReference` that
/// resolves to one of those global names. We classify by AST shape rather
/// than invoking the full type checker so the scan works on a foreign arena
/// (lib bodies live outside `self.ctx.arena`).
fn scan_arena(
    arena: &tsz_parser::parser::NodeArena,
    decl_idx: NodeIndex,
    is_local: bool,
    number_sigs: &mut Vec<SigEmitInfo>,
    string_sigs: &mut Vec<SigEmitInfo>,
    symbol_sigs: &mut Vec<SigEmitInfo>,
) {
    let Some(decl_node) = arena.get(decl_idx) else {
        return;
    };
    if decl_node.kind != syntax_kind_ext::INTERFACE_DECLARATION {
        return;
    }
    let Some(iface) = arena.get_interface(decl_node) else {
        return;
    };
    let Some(sf) = arena.source_files.first() else {
        return;
    };

    for &member_idx in &iface.members.nodes {
        let Some(member_node) = arena.get(member_idx) else {
            continue;
        };
        if member_node.kind != syntax_kind_ext::INDEX_SIGNATURE {
            continue;
        }
        let Some(index_sig) = arena.get_index_signature(member_node) else {
            continue;
        };
        let Some(param_idx) = index_sig.parameters.nodes.first().copied() else {
            continue;
        };
        let Some(param_node) = arena.get(param_idx) else {
            continue;
        };
        let Some(param) = arena.get_parameter(param_node) else {
            continue;
        };
        if param.type_annotation.is_none() {
            continue;
        }
        let Some(ann_node) = arena.get(param.type_annotation) else {
            continue;
        };
        let kind = match ann_node.kind {
            k if k == SyntaxKind::NumberKeyword as u16 => Some(IndexParamKind::Number),
            k if k == SyntaxKind::StringKeyword as u16 => Some(IndexParamKind::String),
            k if k == SyntaxKind::SymbolKeyword as u16 => Some(IndexParamKind::Symbol),
            k if k == syntax_kind_ext::TYPE_REFERENCE => arena
                .get_type_ref(ann_node)
                .and_then(|tr| arena.get(tr.type_name))
                .and_then(|name_node| arena.get_identifier(name_node))
                .and_then(|ident| match ident.escaped_text.as_str() {
                    "number" => Some(IndexParamKind::Number),
                    "string" => Some(IndexParamKind::String),
                    "symbol" => Some(IndexParamKind::Symbol),
                    _ => None,
                }),
            _ => None,
        };
        let Some(kind) = kind else {
            continue;
        };
        let info = SigEmitInfo {
            file_name: sf.file_name.clone(),
            start: member_node.pos,
            length: member_node.end.saturating_sub(member_node.pos),
            is_local,
        };
        match kind {
            IndexParamKind::Number => number_sigs.push(info),
            IndexParamKind::String => string_sigs.push(info),
            IndexParamKind::Symbol => symbol_sigs.push(info),
        }
    }
}
