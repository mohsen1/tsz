//! Duplicate identifier and declaration conflict checking.
//!
//! This module extends `CheckerState` with methods for detecting:
//! - Duplicate identifier declarations (TS2300, TS2451, TS2392, TS2393)
//! - Merged declaration diagnostics (TS2432, TS2717, TS2413)
//! - Overload signature consistency (TS2383, TS2385, TS2386)
//! - Built-in global identifier conflicts (TS2397)

#[path = "duplicate_identifiers_merge.rs"]
mod duplicate_identifiers_merge;

use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

pub(super) type OuterDeclResult = Option<(tsz_binder::SymbolId, Vec<(NodeIndex, u32)>)>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DuplicateDeclarationOrigin {
    SymbolDeclaration,
    TargetedModuleAugmentation,
    /// Remote declaration from a cross-file UMD global / `declare global` conflict.
    GlobalScopeConflict,
}

impl<'a> CheckerState<'a> {
    /// Check for duplicate identifiers (TS2300, TS2451, TS2392).
    /// Reports when variables, functions, classes, or other declarations
    /// have conflicting names within the same scope.
    pub(crate) fn check_duplicate_identifiers(&mut self) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use rustc_hash::FxHashMap;

        let has_libs = self.ctx.has_lib_loaded();
        let is_external_module = self
            .ctx
            .is_external_module_by_file
            .as_ref()
            .and_then(|m| m.get(&self.ctx.file_name))
            .copied()
            .unwrap_or_else(|| self.ctx.binder.is_external_module());

        // When libs are loaded, scope tables contain ~2000+ merged lib symbols alongside
        // user symbols. Processing all of them in the loops below is a 40-50ms bottleneck
        // per file due to HashMap lookups for each symbol's declarations.
        // Optimization: pre-build a set of user-code symbols from node_symbols, then
        // intersect with non-class scope symbols to preserve the Class-scope exclusion.
        let symbol_ids: FxHashSet<tsz_binder::SymbolId> = if has_libs {
            let user_syms: FxHashSet<tsz_binder::SymbolId> =
                self.ctx.binder.node_symbols.values().copied().collect();
            let mut result = FxHashSet::default();
            if !self.ctx.binder.scopes.is_empty() {
                for scope in &self.ctx.binder.scopes {
                    if scope.kind == tsz_binder::ContainerKind::Class {
                        continue;
                    }
                    for (_, &id) in scope.table.iter() {
                        if user_syms.contains(&id) {
                            result.insert(id);
                        }
                    }
                }
            } else {
                for (_, &id) in self.ctx.binder.file_locals.iter() {
                    if user_syms.contains(&id) {
                        result.insert(id);
                    }
                }
            }
            result
        } else {
            // No libs: use scope tables or file_locals (all are user symbols)
            let mut result = FxHashSet::default();
            if !self.ctx.binder.scopes.is_empty() {
                for scope in &self.ctx.binder.scopes {
                    if scope.kind == tsz_binder::ContainerKind::Class {
                        continue;
                    }
                    for (_, &id) in scope.table.iter() {
                        result.insert(id);
                    }
                }
            } else {
                for (_, &id) in self.ctx.binder.file_locals.iter() {
                    result.insert(id);
                }
            }
            result
        };

        let mut cross_file_conflicts = Vec::new();
        for &sym_id in &symbol_ids {
            let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
                continue;
            };
            let module_augmentation_declarations = self
                .module_augmentation_conflict_declarations_for_current_file(&symbol.escaped_name);
            let script_scope_declarations =
                self.same_name_top_level_script_declarations_for_current_file(&symbol.escaped_name);
            let global_scope_declarations =
                self.global_scope_conflict_declarations_for_current_file(&symbol.escaped_name);

            // Check if single NodeIndex has multiple arenas (cross-file duplicate with
            // same NodeIndex due to identical file structure). In this case, declarations
            // list has only 1 entry but represents 2+ actual declarations.
            if symbol.declarations.len() <= 1 {
                let has_cross_file = symbol.declarations.iter().any(|&decl_idx| {
                    self.ctx
                        .binder
                        .declaration_arenas
                        .get(&(sym_id, decl_idx))
                        .is_some_and(|arenas| arenas.len() > 1)
                });
                if !has_cross_file
                    && module_augmentation_declarations.is_empty()
                    && script_scope_declarations.is_empty()
                    && global_scope_declarations.is_empty()
                {
                    continue;
                }
            }

            let mut has_local = false;
            let mut has_remote = false;
            for &decl_idx in &symbol.declarations {
                if let Some(arenas) = self.ctx.binder.declaration_arenas.get(&(sym_id, decl_idx)) {
                    for arena in arenas {
                        let is_local = std::ptr::eq(&**arena, self.ctx.arena);
                        if let Some(_flags) = self.declaration_symbol_flags(arena, decl_idx) {
                            if has_libs
                                && is_local
                                && !self.declaration_name_matches(decl_idx, &symbol.escaped_name)
                            {
                                continue;
                            }
                            if is_local {
                                has_local = true;
                            } else {
                                has_remote = true;
                            }
                        }
                    }
                } else {
                    let is_local = true; // Fallback
                    if let Some(_flags) = self.declaration_symbol_flags(self.ctx.arena, decl_idx) {
                        if has_libs
                            && is_local
                            && !self.declaration_name_matches(decl_idx, &symbol.escaped_name)
                        {
                            continue;
                        }
                        if is_local {
                            has_local = true;
                        } else {
                            has_remote = true;
                        }
                    }
                }
            }

            if !module_augmentation_declarations.is_empty()
                || !script_scope_declarations.is_empty()
                || !global_scope_declarations.is_empty()
            {
                has_remote = true;
            }

            if has_local && has_remote {
                // Interfaces always merge with other interfaces across files in TypeScript.
                let is_interface_merge = (symbol.flags & symbol_flags::INTERFACE) != 0
                    && (symbol.flags
                        & (symbol_flags::FUNCTION_SCOPED_VARIABLE
                            | symbol_flags::BLOCK_SCOPED_VARIABLE
                            | symbol_flags::TYPE_ALIAS
                            | symbol_flags::REGULAR_ENUM
                            | symbol_flags::CONST_ENUM))
                        == 0;
                // var declarations merge across script files (non-modules).
                let is_var_merge = !is_external_module
                    && (symbol.flags & symbol_flags::FUNCTION_SCOPED_VARIABLE) != 0
                    && (symbol.flags
                        & (symbol_flags::BLOCK_SCOPED_VARIABLE
                            | symbol_flags::CLASS
                            | symbol_flags::FUNCTION
                            | symbol_flags::REGULAR_ENUM
                            | symbol_flags::CONST_ENUM
                            | symbol_flags::TYPE_ALIAS))
                        == 0;
                if !is_interface_merge && !is_var_merge {
                    cross_file_conflicts.push(symbol.escaped_name.clone());
                }
            }
        }

        let emit_ts6200 = cross_file_conflicts.len() >= 8;
        if emit_ts6200 {
            cross_file_conflicts.sort();
            let list = cross_file_conflicts.join(", ");
            let message = format_message(
                diagnostic_messages::DEFINITIONS_OF_THE_FOLLOWING_IDENTIFIERS_CONFLICT_WITH_THOSE_IN_ANOTHER_FILE,
                &[&list],
            );
            // Report at position 0 (start of file) — tsc anchors TS6200 at the
            // SourceFile node which has pos=0, length=0.
            self.error_at_position(
                0,
                0,
                &message,
                diagnostic_codes::DEFINITIONS_OF_THE_FOLLOWING_IDENTIFIERS_CONFLICT_WITH_THOSE_IN_ANOTHER_FILE,
            );
        }

        for sym_id in symbol_ids {
            let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
                continue;
            };
            let module_augmentation_declarations = self
                .module_augmentation_conflict_declarations_for_current_file(&symbol.escaped_name);
            let script_scope_declarations =
                self.same_name_top_level_script_declarations_for_current_file(&symbol.escaped_name);
            let global_scope_declarations =
                self.global_scope_conflict_declarations_for_current_file(&symbol.escaped_name);

            if emit_ts6200
                && cross_file_conflicts
                    .binary_search(&symbol.escaped_name)
                    .is_ok()
            {
                continue;
            }

            // Same cross-file NodeIndex collision check as above.
            if symbol.declarations.len() <= 1 {
                let has_cross_file = symbol.declarations.iter().any(|&decl_idx| {
                    self.ctx
                        .binder
                        .declaration_arenas
                        .get(&(sym_id, decl_idx))
                        .is_some_and(|arenas| arenas.len() > 1)
                });
                if !has_cross_file
                    && module_augmentation_declarations.is_empty()
                    && script_scope_declarations.is_empty()
                    && global_scope_declarations.is_empty()
                {
                    continue;
                }
            }

            if symbol.escaped_name == "constructor" {
                let implementations: Vec<NodeIndex> = symbol
                    .declarations
                    .iter()
                    .filter_map(|&decl_idx| {
                        let constructor = self.ctx.arena.get_constructor_at(decl_idx)?;
                        (constructor.body.is_some()).then_some(decl_idx)
                    })
                    .collect();

                if implementations.len() > 1 {
                    let message =
                        diagnostic_messages::MULTIPLE_CONSTRUCTOR_IMPLEMENTATIONS_ARE_NOT_ALLOWED;
                    for &decl_idx in &implementations {
                        self.error_at_node(
                            decl_idx,
                            message,
                            diagnostic_codes::MULTIPLE_CONSTRUCTOR_IMPLEMENTATIONS_ARE_NOT_ALLOWED,
                        );
                    }
                }
                continue;
            }

            let mut declarations =
                Vec::<(NodeIndex, u32, bool, bool, DuplicateDeclarationOrigin)>::new();
            for &decl_idx in &symbol.declarations {
                // When a declaration NodeIndex has multiple arenas (cross-file
                // merged symbols where different files produced the same NodeIndex),
                // iterate ALL arenas to correctly distinguish local vs remote
                // declarations. Using only .first() would misidentify remote
                // declarations as local when the first arena happens to be the
                // current file's arena.
                if let Some(arenas) = self.ctx.binder.declaration_arenas.get(&(sym_id, decl_idx)) {
                    for arena_arc in arenas {
                        let arena: &tsz_parser::parser::NodeArena = arena_arc;
                        let is_local = std::ptr::eq(arena, self.ctx.arena);

                        if let Some(flags) = self.declaration_symbol_flags(arena, decl_idx) {
                            if has_libs
                                && is_local
                                && !self.declaration_name_matches(decl_idx, &symbol.escaped_name)
                            {
                                continue;
                            }
                            let is_exported = self.is_declaration_exported(arena, decl_idx);
                            declarations.push((
                                decl_idx,
                                flags,
                                is_local,
                                is_exported,
                                DuplicateDeclarationOrigin::SymbolDeclaration,
                            ));
                        }
                    }
                } else {
                    // No declaration_arenas entry: assume current arena (local)
                    let arena = self.ctx.arena;
                    let is_local = true;

                    if let Some(flags) = self.declaration_symbol_flags(arena, decl_idx) {
                        if has_libs
                            && is_local
                            && !self.declaration_name_matches(decl_idx, &symbol.escaped_name)
                        {
                            continue;
                        }
                        let is_exported = self.is_declaration_exported(arena, decl_idx);
                        declarations.push((
                            decl_idx,
                            flags,
                            is_local,
                            is_exported,
                            DuplicateDeclarationOrigin::SymbolDeclaration,
                        ));
                    }
                }
            }

            let has_remote_symbol_decl =
                declarations.iter().any(|(_, _, is_local, _, _)| !*is_local);
            if !has_remote_symbol_decl {
                declarations.extend(script_scope_declarations);
            }
            declarations.extend(module_augmentation_declarations);
            declarations.extend(global_scope_declarations);

            if declarations.len() <= 1 {
                continue;
            }
            let mut func_decls_for_2384 = Vec::new();
            let mut has_ambient_func = false;
            let mut has_non_ambient_func = false;
            for &(decl_idx, flags, is_local, _, _) in &declarations {
                if is_local && (flags & (symbol_flags::FUNCTION | symbol_flags::METHOD)) != 0 {
                    // TS2384 only applies to overload signatures (bodyless declarations).
                    // Skip implementations (declarations with bodies) — a non-ambient
                    // implementation following ambient overloads is valid.
                    if self.function_has_body(decl_idx) {
                        continue;
                    }
                    func_decls_for_2384.push(decl_idx);
                    if self.is_ambient_declaration(decl_idx) {
                        has_ambient_func = true;
                    } else {
                        has_non_ambient_func = true;
                    }
                }
            }
            // Find the implementation (function with body) — used as reference
            // for modifier agreement checks. tsc uses the implementation's flags.
            let impl_decl_idx = func_decls_for_2384.iter().copied().find(|&d| {
                self.function_has_body(d)
                    || self
                        .ctx
                        .arena
                        .get(d)
                        .and_then(|n| self.ctx.arena.get_method_decl(n))
                        .is_some_and(|m| m.body.is_some())
            });

            if has_ambient_func && has_non_ambient_func {
                let ref_is_ambient = impl_decl_idx
                    .map(|d| self.is_ambient_declaration(d))
                    .unwrap_or_else(|| self.is_ambient_declaration(func_decls_for_2384[0]));
                for &decl_idx in &func_decls_for_2384 {
                    if Some(decl_idx) == impl_decl_idx {
                        continue;
                    }
                    if self.is_ambient_declaration(decl_idx) != ref_is_ambient {
                        let error_node =
                            self.get_declaration_name_node(decl_idx).unwrap_or(decl_idx);
                        self.error_at_node(
                            error_node,
                            diagnostic_messages::OVERLOAD_SIGNATURES_MUST_ALL_BE_AMBIENT_OR_NON_AMBIENT,
                            diagnostic_codes::OVERLOAD_SIGNATURES_MUST_ALL_BE_AMBIENT_OR_NON_AMBIENT,
                        );
                    }
                }
            }

            // TS2383: Overload signatures must all be exported or non-exported
            if func_decls_for_2384.len() >= 2 {
                let mut has_exported = false;
                let mut has_non_exported = false;
                let mut func_export_info: Vec<(NodeIndex, bool)> = Vec::new();
                for &(decl_idx, flags, is_local, is_exported, _) in &declarations {
                    if is_local && (flags & (symbol_flags::FUNCTION | symbol_flags::METHOD)) != 0 {
                        func_export_info.push((decl_idx, is_exported));
                        if is_exported {
                            has_exported = true;
                        } else {
                            has_non_exported = true;
                        }
                    }
                }
                if has_exported && has_non_exported && func_export_info.len() >= 2 {
                    let ref_exported = impl_decl_idx
                        .and_then(|d| {
                            func_export_info
                                .iter()
                                .find(|(idx, _)| *idx == d)
                                .map(|(_, e)| *e)
                        })
                        .unwrap_or(func_export_info[0].1);
                    for &(decl_idx, is_exported) in &func_export_info {
                        if Some(decl_idx) == impl_decl_idx {
                            continue;
                        }
                        if is_exported != ref_exported {
                            let error_node =
                                self.get_declaration_name_node(decl_idx).unwrap_or(decl_idx);
                            self.error_at_node(
                                error_node,
                                diagnostic_messages::OVERLOAD_SIGNATURES_MUST_ALL_BE_EXPORTED_OR_NON_EXPORTED,
                                diagnostic_codes::OVERLOAD_SIGNATURES_MUST_ALL_BE_EXPORTED_OR_NON_EXPORTED,
                            );
                        }
                    }
                }
            }

            // TS2385: Overload signatures must all be public, private or protected
            // Applies to class method overloads with mixed access modifiers
            if func_decls_for_2384.len() >= 2 {
                let access_infos: Vec<(NodeIndex, u8)> = func_decls_for_2384
                    .iter()
                    .map(|&decl_idx| (decl_idx, self.get_access_modifier(decl_idx)))
                    .collect();
                let ref_access = impl_decl_idx
                    .map(|d| self.get_access_modifier(d))
                    .unwrap_or(access_infos[0].1);
                let has_mismatch = access_infos.iter().any(|(_, a)| *a != ref_access);
                if has_mismatch {
                    for &(decl_idx, access) in &access_infos {
                        if Some(decl_idx) == impl_decl_idx {
                            continue;
                        }
                        if access != ref_access {
                            // TSC anchors TS2385 at the start of the overload declaration
                            // (including modifiers), not at the declaration name.
                            if let Some(decl_node) = self.ctx.arena.get(decl_idx) {
                                let start = self
                                    .ctx
                                    .arena
                                    .get_declaration_modifiers(decl_node)
                                    .and_then(|mods| mods.nodes.first().copied())
                                    .and_then(|first_mod| self.ctx.arena.get(first_mod))
                                    .map_or(decl_node.pos, |mod_node| mod_node.pos);
                                let length = decl_node.end.saturating_sub(start);
                                self.error(
                                    start,
                                    length,
                                    diagnostic_messages::OVERLOAD_SIGNATURES_MUST_ALL_BE_PUBLIC_PRIVATE_OR_PROTECTED.to_string(),
                                    diagnostic_codes::OVERLOAD_SIGNATURES_MUST_ALL_BE_PUBLIC_PRIVATE_OR_PROTECTED,
                                );
                            }
                        }
                    }
                }
            }

            // TS2386: Overload signatures must all be optional or required
            // Applies to interface/class method overloads with mixed optionality
            if func_decls_for_2384.len() >= 2 {
                let optional_infos: Vec<(NodeIndex, bool)> = func_decls_for_2384
                    .iter()
                    .map(|&decl_idx| (decl_idx, self.is_declaration_optional(decl_idx)))
                    .collect();
                let ref_optional = impl_decl_idx
                    .map(|d| self.is_declaration_optional(d))
                    .unwrap_or(optional_infos[0].1);
                let has_mismatch = optional_infos.iter().any(|(_, o)| *o != ref_optional);
                if has_mismatch {
                    for &(decl_idx, optional) in &optional_infos {
                        if Some(decl_idx) == impl_decl_idx {
                            continue;
                        }
                        if optional != ref_optional {
                            let error_node =
                                self.get_declaration_name_node(decl_idx).unwrap_or(decl_idx);
                            self.error_at_node(
                                error_node,
                                diagnostic_messages::OVERLOAD_SIGNATURES_MUST_ALL_BE_OPTIONAL_OR_REQUIRED,
                                diagnostic_codes::OVERLOAD_SIGNATURES_MUST_ALL_BE_OPTIONAL_OR_REQUIRED,
                            );
                        }
                    }
                }
            }

            // Explicit duplicate-class guard: class declarations cannot merge
            // with other class declarations (only with namespaces/interfaces).
            // Emit TS2300 for duplicate class declarations in the same symbol set.
            let local_class_decls: Vec<(NodeIndex, bool)> = declarations
                .iter()
                .filter(|(_, flags, is_local, _, _)| {
                    *is_local && (flags & symbol_flags::CLASS) != 0
                })
                .map(|(decl_idx, _, _, is_exported, _)| (*decl_idx, *is_exported))
                .collect();
            if local_class_decls.len() > 1 {
                // Skip TS2300 when all class declarations are `export default` —
                // TS2528 ("A module cannot have multiple default exports") handles this.
                let all_default_exports = local_class_decls.iter().all(|&(decl_idx, _)| {
                    self.ctx
                        .arena
                        .get_extended(decl_idx)
                        .and_then(|ext| self.ctx.arena.get(ext.parent))
                        .and_then(|parent| self.ctx.arena.get_export_decl(parent))
                        .is_some_and(|export_data| export_data.is_default_export)
                });
                if all_default_exports {
                    continue;
                }

                // Skip TS2300 when class declarations in merging namespaces differ
                // in export visibility (one exported, one non-exported). tsc allows
                // an exported class and a non-exported class with the same name to
                // coexist in merging namespace declarations.
                let has_exported = local_class_decls.iter().any(|&(_, exp)| exp);
                let has_non_exported = local_class_decls.iter().any(|&(_, exp)| !exp);
                if has_exported && has_non_exported {
                    continue;
                }

                // Skip TS2300 when any class declaration is inside a non-exported
                // namespace body. In TSC, a non-exported `namespace Z` doesn't merge
                // with an exported Z from a dot-notation declaration like `namespace X.Y.Z`.
                // The classes inside them are separate and should not trigger TS2300.
                let any_in_non_exported_ns = local_class_decls
                    .iter()
                    .any(|&(decl_idx, _)| self.is_in_non_exported_namespace_body(decl_idx));
                if any_in_non_exported_ns {
                    continue;
                }

                let message = format_message(
                    diagnostic_messages::DUPLICATE_IDENTIFIER,
                    &[&symbol.escaped_name],
                );
                for (decl_idx, _) in local_class_decls {
                    let error_node = self.get_declaration_name_node(decl_idx).unwrap_or(decl_idx);
                    self.error_at_node(
                        error_node,
                        &message,
                        diagnostic_codes::DUPLICATE_IDENTIFIER,
                    );
                }
                // When duplicate class declarations exist, tsc also flags interface
                // declarations that share the same name. The interface merges into the
                // class symbol, but since the class declarations themselves conflict,
                // every declaration of the name is marked as a duplicate.
                let local_interface_decls: Vec<NodeIndex> = declarations
                    .iter()
                    .filter(|(_, flags, is_local, _, _)| {
                        *is_local && (flags & symbol_flags::INTERFACE) != 0
                    })
                    .map(|(decl_idx, _, _, _, _)| *decl_idx)
                    .collect();
                for decl_idx in local_interface_decls {
                    let error_node = self.get_declaration_name_node(decl_idx).unwrap_or(decl_idx);
                    self.error_at_node(
                        error_node,
                        &message,
                        diagnostic_codes::DUPLICATE_IDENTIFIER,
                    );
                }
                continue;
            }

            // TS2395
            let mut has_ts2395 = false;
            {
                const SPACE_TYPE: u32 = 1;
                const SPACE_VALUE: u32 = 2;
                const SPACE_NAMESPACE: u32 = 4;

                let mut error_nodes: Vec<NodeIndex> = Vec::new();

                {
                    let decl_info: Vec<(NodeIndex, u32, u32, bool, NodeIndex)> = declarations
                        .iter()
                        .filter(|&(_, _, is_local, _, _)| *is_local)
                        .map(|&(decl_idx, flags, _, exported, _)| {
                            let space = if (flags & symbol_flags::INTERFACE) != 0
                                || (flags & symbol_flags::TYPE_ALIAS) != 0
                            {
                                SPACE_TYPE
                            } else if (flags
                                & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE))
                                != 0
                            {
                                if self.is_namespace_declaration_instantiated(decl_idx) {
                                    SPACE_NAMESPACE | SPACE_VALUE
                                } else {
                                    SPACE_NAMESPACE
                                }
                            } else if (flags & symbol_flags::CLASS) != 0
                                || (flags & (symbol_flags::REGULAR_ENUM | symbol_flags::CONST_ENUM))
                                    != 0
                            {
                                SPACE_TYPE | SPACE_VALUE
                            } else if (flags & symbol_flags::VARIABLE) != 0
                                || (flags & symbol_flags::FUNCTION) != 0
                            {
                                SPACE_VALUE
                            } else {
                                0
                            };
                            let scope = self.get_enclosing_namespace(decl_idx);
                            (decl_idx, flags, space, exported, scope)
                        })
                        .collect();

                    type ScopeGroupEntry = (NodeIndex, u32, u32, bool);
                    let mut scope_groups: FxHashMap<NodeIndex, Vec<ScopeGroupEntry>> =
                        FxHashMap::default();
                    for &(decl_idx, flags, space, exported, scope) in &decl_info {
                        scope_groups
                            .entry(scope)
                            .or_default()
                            .push((decl_idx, flags, space, exported));
                    }

                    for group in scope_groups.values() {
                        if group.len() <= 1 {
                            continue;
                        }
                        let all_functions = group
                            .iter()
                            .all(|&(_, flags, _, _)| (flags & symbol_flags::FUNCTION) != 0);
                        if all_functions {
                            continue;
                        }
                        let mut exported_spaces: u32 = 0;
                        let mut non_exported_spaces: u32 = 0;
                        for &(_, _, space, exported) in group {
                            if exported {
                                exported_spaces |= space;
                            } else {
                                non_exported_spaces |= space;
                            }
                        }
                        let common_spaces = exported_spaces & non_exported_spaces;
                        if common_spaces != 0 {
                            has_ts2395 = true;
                            for &(decl_idx, _, space, _) in group {
                                if (space & common_spaces) != 0 {
                                    let error_node = self
                                        .get_declaration_name_node(decl_idx)
                                        .unwrap_or(decl_idx);
                                    error_nodes.push(error_node);
                                }
                            }
                        }
                    }
                }

                if has_ts2395 {
                    let name = symbol.escaped_name.clone();
                    let message = format_message(
                        diagnostic_messages::INDIVIDUAL_DECLARATIONS_IN_MERGED_DECLARATION_MUST_BE_ALL_EXPORTED_OR_ALL_LOCAL,
                        &[&name],
                    );
                    for error_node in error_nodes {
                        self.error_at_node(
                            error_node,
                            &message,
                            diagnostic_codes::INDIVIDUAL_DECLARATIONS_IN_MERGED_DECLARATION_MUST_BE_ALL_EXPORTED_OR_ALL_LOCAL,
                        );
                    }
                }
            }

            // TS2428 only applies to merged interface declarations. Mixed
            // class+interface merges are handled separately by
            // check_merged_class_interface_declaration_diagnostics.
            let interface_decls: Vec<NodeIndex> = declarations
                .iter()
                .filter(|(_, flags, is_local, _, _)| {
                    *is_local && (flags & symbol_flags::INTERFACE) != 0
                })
                .map(|(decl_idx, _, _, _, _)| *decl_idx)
                .collect();
            if interface_decls.len() > 1 {
                use tsz_binder::SymbolId;
                let mut interface_decls_by_scope: FxHashMap<SymbolId, Vec<NodeIndex>> =
                    FxHashMap::default();
                for &decl_idx in &interface_decls {
                    let scope = self.get_enclosing_namespace_symbol(decl_idx);
                    interface_decls_by_scope
                        .entry(scope)
                        .or_default()
                        .push(decl_idx);
                }

                for decls_in_scope in interface_decls_by_scope.into_values() {
                    if decls_in_scope.len() <= 1 {
                        continue;
                    }
                    self.check_merged_interface_declaration_diagnostics(&decls_in_scope);
                    let mismatch =
                        decls_in_scope
                            .as_slice()
                            .split_first()
                            .is_some_and(|(baseline, rest)| {
                                rest.iter().any(|&decl_idx| {
                                    !self.interface_type_parameters_are_merge_compatible(
                                        *baseline, decl_idx,
                                    )
                                })
                            });
                    if mismatch {
                        let message = format_message(
                            diagnostic_messages::ALL_DECLARATIONS_OF_MUST_HAVE_IDENTICAL_TYPE_PARAMETERS,
                            &[&symbol.escaped_name],
                        );
                        for decl_idx in decls_in_scope {
                            let error_node =
                                self.get_declaration_name_node(decl_idx).unwrap_or(decl_idx);
                            self.error_at_node(
                                error_node,
                                &message,
                                diagnostic_codes::ALL_DECLARATIONS_OF_MUST_HAVE_IDENTICAL_TYPE_PARAMETERS,
                            );
                        }
                    }
                }
            }

            let class_interface_decls: Vec<NodeIndex> = declarations
                .iter()
                .filter(|(_, flags, is_local, _, _)| {
                    *is_local && (flags & (symbol_flags::CLASS | symbol_flags::INTERFACE)) != 0
                })
                .map(|(decl_idx, _, _, _, _)| *decl_idx)
                .collect();
            if class_interface_decls.len() > 1 {
                use tsz_binder::SymbolId;
                let mut decls_by_scope: FxHashMap<SymbolId, Vec<NodeIndex>> = FxHashMap::default();
                for &decl_idx in &class_interface_decls {
                    let scope = self.get_enclosing_namespace_symbol(decl_idx);
                    decls_by_scope.entry(scope).or_default().push(decl_idx);
                }

                for (_, decls_in_scope) in decls_by_scope {
                    if decls_in_scope.len() <= 1 {
                        continue;
                    }
                    self.check_merged_class_interface_declaration_diagnostics(&decls_in_scope);

                    // TS2428: check that merged class+interface declarations have
                    // identical type parameters. The interface-only check above handles
                    // interface+interface merges; this handles class+interface merges.
                    let has_class = decls_in_scope.iter().any(|&idx| {
                        self.ctx
                            .arena
                            .get(idx)
                            .is_some_and(|n| self.ctx.arena.get_class(n).is_some())
                    });
                    let has_interface = decls_in_scope.iter().any(|&idx| {
                        self.ctx
                            .arena
                            .get(idx)
                            .is_some_and(|n| self.ctx.arena.get_interface(n).is_some())
                    });
                    if has_class && has_interface {
                        let mismatch = decls_in_scope.as_slice().split_first().is_some_and(
                            |(baseline, rest)| {
                                rest.iter().any(|&decl_idx| {
                                    !self.class_interface_type_parameters_are_merge_compatible(
                                        *baseline, decl_idx,
                                    )
                                })
                            },
                        );
                        if mismatch {
                            let message = format_message(
                                diagnostic_messages::ALL_DECLARATIONS_OF_MUST_HAVE_IDENTICAL_TYPE_PARAMETERS,
                                &[&symbol.escaped_name],
                            );
                            for &decl_idx in &decls_in_scope {
                                let error_node =
                                    self.get_declaration_name_node(decl_idx).unwrap_or(decl_idx);
                                self.error_at_node(
                                    error_node,
                                    &message,
                                    diagnostic_codes::ALL_DECLARATIONS_OF_MUST_HAVE_IDENTICAL_TYPE_PARAMETERS,
                                );
                            }
                        }
                    }
                }
            }

            // Cross-file interface member conflicts: check local interface members
            // against remote interface members for property-vs-method conflicts (TS2300).
            // tsc reports "Duplicate identifier 'X'" when a property signature and method
            // signature with the same name appear across merged interface declarations
            // in different files.
            {
                let local_interface_decls: Vec<NodeIndex> = declarations
                    .iter()
                    .filter(|(_, flags, is_local, _, _)| {
                        *is_local && (flags & symbol_flags::INTERFACE) != 0
                    })
                    .map(|(decl_idx, _, _, _, _)| *decl_idx)
                    .collect();
                let remote_interface_decls: Vec<NodeIndex> = declarations
                    .iter()
                    .filter(|(_, flags, is_local, _, _)| {
                        !*is_local && (flags & symbol_flags::INTERFACE) != 0
                    })
                    .map(|(decl_idx, _, _, _, _)| *decl_idx)
                    .collect();

                if !local_interface_decls.is_empty() && !remote_interface_decls.is_empty() {
                    self.check_cross_file_interface_member_conflicts(
                        sym_id,
                        &local_interface_decls,
                        &remote_interface_decls,
                    );
                }
            }

            let local_declarations_for_enums: Vec<(NodeIndex, u32)> = declarations
                .iter()
                .filter(|&(_, _, is_local, _, _)| *is_local)
                .map(|&(idx, flags, _, _, _)| (idx, flags))
                .collect();
            self.check_merged_enum_declaration_diagnostics(&local_declarations_for_enums);

            let mut conflicts = FxHashSet::default();
            let mut propagate_type_alias_conflict_to_namespaces = false;
            let mut namespace_order_errors = FxHashSet::default();
            let mut has_umd_global_value_conflict = false;

            for i in 0..declarations.len() {
                for j in (i + 1)..declarations.len() {
                    let (decl_idx, decl_flags, decl_is_local, decl_is_exported, decl_origin) =
                        declarations[i];
                    let (other_idx, other_flags, other_is_local, other_is_exported, other_origin) =
                        declarations[j];

                    if !decl_is_local && !other_is_local {
                        continue;
                    }

                    let decl_is_module_scoped_local = is_external_module
                        && decl_is_local
                        && self.get_enclosing_namespace(decl_idx).is_none();
                    let other_is_module_scoped_local = is_external_module
                        && other_is_local
                        && self.get_enclosing_namespace(other_idx).is_none();

                    let decl_is_skippable_remote = !decl_is_local
                        && decl_origin == DuplicateDeclarationOrigin::SymbolDeclaration;
                    let other_is_skippable_remote = !other_is_local
                        && other_origin == DuplicateDeclarationOrigin::SymbolDeclaration;

                    // In external modules, top-level module-scope declarations do not
                    // participate in global namespace duplicate checking against lib
                    // declarations. This preserves TypeScript semantics where external
                    // module declarations are isolated from unrelated global symbol
                    // conflicts, but explicit module augmentations still target this
                    // file's exports and must participate in duplicate checking.
                    if is_external_module
                        && ((decl_is_module_scoped_local && other_is_skippable_remote)
                            || (other_is_module_scoped_local && decl_is_skippable_remote))
                    {
                        continue;
                    }

                    // Check for function overloads

                    // TS2323: exported variable redeclaration.
                    // Only flag when the file is an external module AND both
                    // declarations are individually exported at the module level.
                    // Namespace-internal `export var` redeclarations are allowed
                    // because `var` is function-scoped and redeclarable; TS2323
                    // only applies to module-level export conflicts.
                    let decl_is_var = (decl_flags & symbol_flags::FUNCTION_SCOPED_VARIABLE) != 0;
                    let other_is_var = (other_flags & symbol_flags::FUNCTION_SCOPED_VARIABLE) != 0;
                    if decl_is_var && other_is_var {
                        if is_external_module && decl_is_exported && other_is_exported {
                            if decl_is_local {
                                conflicts.insert(decl_idx);
                            }
                            if other_is_local {
                                conflicts.insert(other_idx);
                            }
                        }
                        continue;
                    }
                    let both_functions = (decl_flags & symbol_flags::FUNCTION) != 0
                        && (other_flags & symbol_flags::FUNCTION) != 0;
                    if both_functions {
                        let decl_has_body = decl_is_local && self.function_has_body(decl_idx);
                        if !other_is_local {
                            continue;
                        }
                        let other_has_body = self.function_has_body(other_idx);

                        if !(decl_has_body && other_has_body) {
                            continue;
                        }

                        if decl_is_local && other_is_local {
                            let decl_scope = self.get_enclosing_block_scope(decl_idx);
                            let other_scope = self.get_enclosing_block_scope(other_idx);
                            if decl_scope != other_scope {
                                continue;
                            }
                        }

                        if decl_is_local {
                            conflicts.insert(decl_idx);
                        }
                        if other_is_local {
                            conflicts.insert(other_idx);
                        }
                        continue;
                    }

                    let both_methods = (decl_flags & symbol_flags::METHOD) != 0
                        && (other_flags & symbol_flags::METHOD) != 0;
                    if both_methods {
                        if decl_is_local && other_is_local {
                            let decl_has_body = self.method_has_body(decl_idx);
                            let other_has_body = self.method_has_body(other_idx);
                            if !(decl_has_body && other_has_body) {
                                continue;
                            }
                        } else {
                            continue;
                        }
                    }

                    let both_interfaces = (decl_flags & symbol_flags::INTERFACE) != 0
                        && (other_flags & symbol_flags::INTERFACE) != 0;
                    if both_interfaces {
                        continue;
                    }

                    let both_enums = (decl_flags & symbol_flags::ENUM) != 0
                        && (other_flags & symbol_flags::ENUM) != 0;
                    if both_enums {
                        continue;
                    }

                    let is_umd_global_value_conflict = decl_is_local
                        && other_is_local
                        && ((self.is_namespace_export_declaration_name_in_current_file(decl_idx)
                            && self
                                .is_block_scoped_global_augmentation_value_decl_in_current_file(
                                    other_idx,
                                    other_flags,
                                ))
                            || (self
                                .is_namespace_export_declaration_name_in_current_file(other_idx)
                                && self
                                    .is_block_scoped_global_augmentation_value_decl_in_current_file(
                                        decl_idx, decl_flags,
                                    )));
                    if is_umd_global_value_conflict {
                        has_umd_global_value_conflict = true;
                        conflicts.insert(decl_idx);
                        conflicts.insert(other_idx);
                        continue;
                    }

                    // Cross-file UMD global value conflict: one declaration is local
                    // and the other is a remote `export as namespace X` or
                    // `declare global { const X }` found by
                    // `global_scope_conflict_declarations_for_current_file`.
                    //
                    // Only triggers when one side is a namespace export and the
                    // other is a block-scoped global augmentation value. Two
                    // namespace exports from different files do NOT conflict
                    // (first one wins — see umdGlobalConflict.ts).
                    let is_cross_file_umd = (decl_is_local != other_is_local)
                        && (decl_origin == DuplicateDeclarationOrigin::GlobalScopeConflict
                            || other_origin == DuplicateDeclarationOrigin::GlobalScopeConflict);
                    if is_cross_file_umd {
                        let (local_idx, local_flags, remote_flags) = if decl_is_local {
                            (decl_idx, decl_flags, other_flags)
                        } else {
                            (other_idx, other_flags, decl_flags)
                        };
                        let local_is_ns_export =
                            self.is_namespace_export_declaration_name_in_current_file(local_idx);
                        let local_is_global_aug = self
                            .is_block_scoped_global_augmentation_value_decl_in_current_file(
                                local_idx,
                                local_flags,
                            );
                        // Remote is a global augmentation value (BLOCK_SCOPED_VARIABLE)
                        // or a namespace export (ALIAS). Conflict only when the two
                        // sides are of different types.
                        let remote_is_block_scoped =
                            (remote_flags & symbol_flags::BLOCK_SCOPED_VARIABLE) != 0;
                        let remote_is_ns_alias = (remote_flags & symbol_flags::ALIAS) != 0;
                        let is_actual_conflict = (local_is_ns_export && remote_is_block_scoped)
                            || (local_is_global_aug && remote_is_ns_alias);
                        if is_actual_conflict {
                            has_umd_global_value_conflict = true;
                            if decl_is_local {
                                conflicts.insert(decl_idx);
                            }
                            if other_is_local {
                                conflicts.insert(other_idx);
                            }
                        }
                        continue;
                    }

                    let decl_is_namespace = (decl_flags
                        & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE))
                        != 0;
                    let other_is_namespace = (other_flags
                        & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE))
                        != 0;

                    if decl_is_namespace && other_is_namespace {
                        continue;
                    }

                    let decl_is_function = (decl_flags & symbol_flags::FUNCTION) != 0;
                    let other_is_function = (other_flags & symbol_flags::FUNCTION) != 0;
                    if (decl_is_namespace && other_is_function)
                        || (decl_is_function && other_is_namespace)
                    {
                        if !decl_is_local || !other_is_local {
                            continue;
                        }

                        let (namespace_idx, function_idx) = if decl_is_namespace {
                            (decl_idx, other_idx)
                        } else {
                            (other_idx, decl_idx)
                        };

                        let namespace_is_instantiated =
                            self.is_namespace_declaration_instantiated(namespace_idx);

                        if !namespace_is_instantiated {
                            continue;
                        }
                        // Skip if the namespace is ambient (`declare namespace`)
                        if self.is_ambient_declaration(namespace_idx) {
                            continue;
                        }
                        if self.is_ambient_function_declaration(function_idx) {
                            continue;
                        }
                        if namespace_idx.0 < function_idx.0 {
                            namespace_order_errors.insert(namespace_idx);
                        }
                        continue;
                    }

                    let decl_is_class = (decl_flags & symbol_flags::CLASS) != 0;
                    let other_is_class = (other_flags & symbol_flags::CLASS) != 0;
                    if (decl_is_namespace && other_is_class)
                        || (decl_is_class && other_is_namespace)
                    {
                        continue;
                    }

                    let decl_is_enum = (decl_flags & symbol_flags::ENUM) != 0;
                    let other_is_enum = (other_flags & symbol_flags::ENUM) != 0;
                    if (decl_is_namespace && other_is_enum) || (decl_is_enum && other_is_namespace)
                    {
                        continue;
                    }

                    let decl_is_variable = (decl_flags & symbol_flags::VARIABLE) != 0;
                    let other_is_variable = (other_flags & symbol_flags::VARIABLE) != 0;
                    if (decl_is_namespace && other_is_variable)
                        || (decl_is_variable && other_is_namespace)
                    {
                        if !decl_is_local || !other_is_local {
                            continue;
                        }
                        let namespace_idx = if decl_is_namespace {
                            decl_idx
                        } else {
                            other_idx
                        };
                        if self.is_namespace_declaration_instantiated(namespace_idx) {
                            if decl_is_local {
                                conflicts.insert(decl_idx);
                            }
                            if other_is_local {
                                conflicts.insert(other_idx);
                            }
                        }
                        continue;
                    }

                    // In checked JS, a value declaration can intentionally pick up
                    // a cross-file class shape from a TS/.d.ts declaration. Treating
                    // that as a duplicate blocks the real semantic check and produces
                    // TS2451 where tsc reports a constructor-side assignability error
                    // (for example salsa/jsContainerMergeTsDeclaration3.ts).
                    let checked_js_value_merges_remote_class = self.is_js_file()
                        && self.ctx.compiler_options.check_js
                        && (decl_is_local != other_is_local)
                        && (((decl_flags & symbol_flags::VARIABLE) != 0
                            && (other_flags & symbol_flags::CLASS) != 0)
                            || ((other_flags & symbol_flags::VARIABLE) != 0
                                && (decl_flags & symbol_flags::CLASS) != 0));
                    if checked_js_value_merges_remote_class {
                        continue;
                    }

                    if Self::declarations_conflict(decl_flags, other_flags) {
                        propagate_type_alias_conflict_to_namespaces |=
                            (decl_flags & symbol_flags::TYPE_ALIAS) != 0
                                || (other_flags & symbol_flags::TYPE_ALIAS) != 0;
                        if decl_is_local {
                            conflicts.insert(decl_idx);
                        }
                        if other_is_local {
                            conflicts.insert(other_idx);
                        }
                    }
                }
            }

            if propagate_type_alias_conflict_to_namespaces {
                for &(decl_idx, decl_flags, is_local, _, _) in &declarations {
                    if is_local
                        && (decl_flags
                            & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE))
                            != 0
                    {
                        conflicts.insert(decl_idx);
                    }
                }
            }

            for idx in namespace_order_errors {
                let error_node = self.get_declaration_name_node(idx).unwrap_or(idx);
                let message = format_message(
                    diagnostic_messages::A_NAMESPACE_DECLARATION_CANNOT_BE_LOCATED_PRIOR_TO_A_CLASS_OR_FUNCTION_WITH_WHIC,
                    &[],
                );
                self.error_at_node(error_node, &message, diagnostic_codes::A_NAMESPACE_DECLARATION_CANNOT_BE_LOCATED_PRIOR_TO_A_CLASS_OR_FUNCTION_WITH_WHIC);
            }

            if conflicts.is_empty() {
                continue;
            }

            // TS2393: Duplicate function implementation.
            {
                let has_non_function_conflict =
                    declarations.iter().any(|(decl_idx, flags, _, _, _)| {
                        conflicts.contains(decl_idx) && (flags & symbol_flags::FUNCTION) == 0
                    });
                let func_impls_with_scope: Vec<(NodeIndex, NodeIndex)> = declarations
                    .iter()
                    .filter(|(decl_idx, flags, is_local, _, _)| {
                        *is_local
                            && conflicts.contains(decl_idx)
                            && (flags & symbol_flags::FUNCTION) != 0
                            && self.function_has_body(*decl_idx)
                    })
                    .map(|(idx, _, _, _, _)| (*idx, self.get_enclosing_block_scope(*idx)))
                    .collect();

                let mut scope_groups: std::collections::HashMap<NodeIndex, Vec<NodeIndex>> =
                    std::collections::HashMap::new();
                for &(idx, scope) in &func_impls_with_scope {
                    scope_groups.entry(scope).or_default().push(idx);
                }

                for group in scope_groups.values() {
                    if group.len() > 1 {
                        for &idx in group {
                            let error_node = self.get_declaration_name_node(idx).unwrap_or(idx);
                            self.error_at_node(
                                error_node,
                                diagnostic_messages::DUPLICATE_FUNCTION_IMPLEMENTATION,
                                diagnostic_codes::DUPLICATE_FUNCTION_IMPLEMENTATION,
                            );
                            if !has_non_function_conflict {
                                conflicts.remove(&idx);
                            }
                        }
                    }
                }
                if conflicts.is_empty() {
                    continue;
                }
            }

            // TS2813 + TS2814: Class-function merge conflict.
            // `declare class` + `function` is a valid merge in TypeScript (ambient class).
            // Only non-ambient class + function triggers these errors.
            {
                let local_class_merge_conflicts: Vec<NodeIndex> = declarations
                    .iter()
                    .filter(|(decl_idx, flags, is_local, _, _)| {
                        *is_local
                            && conflicts.contains(decl_idx)
                            && ((flags & symbol_flags::CLASS) != 0
                                || (flags & symbol_flags::FUNCTION) != 0
                                || ((flags & symbol_flags::VARIABLE) != 0
                                    && self
                                        .declaration_is_checked_js_constructor_value_declaration(
                                            sym_id, *decl_idx,
                                        )))
                    })
                    .map(|(idx, _, _, _, _)| *idx)
                    .collect();
                let has_class_partner =
                    declarations
                        .iter()
                        .any(|(decl_idx, flags, is_local, _, _)| {
                            ((*is_local && conflicts.contains(decl_idx)) || !*is_local)
                                && (flags & symbol_flags::CLASS) != 0
                        });
                let has_function_partner =
                    declarations
                        .iter()
                        .any(|(decl_idx, flags, is_local, _, _)| {
                            ((*is_local && conflicts.contains(decl_idx)) || !*is_local)
                                && (flags & symbol_flags::FUNCTION) != 0
                        });
                let has_js_constructor_value_partner =
                    declarations
                        .iter()
                        .any(|(decl_idx, flags, is_local, _, _)| {
                            ((*is_local && conflicts.contains(decl_idx)) || !*is_local)
                                && (flags & symbol_flags::VARIABLE) != 0
                                && self.declaration_is_checked_js_constructor_value_declaration(
                                    sym_id, *decl_idx,
                                )
                        });

                if !local_class_merge_conflicts.is_empty()
                    && has_class_partner
                    && (has_function_partner || has_js_constructor_value_partner)
                {
                    // Check if ALL class declarations in conflicts are ambient
                    let all_classes_ambient =
                        declarations
                            .iter()
                            .all(|(decl_idx, flags, is_local, _, _)| {
                                !(((*is_local && conflicts.contains(decl_idx)) || !*is_local)
                                    && (flags & symbol_flags::CLASS) != 0)
                                    || (flags & symbol_flags::CLASS) == 0
                                    || self.is_ambient_declaration(*decl_idx)
                            });

                    if has_function_partner && !all_classes_ambient {
                        // Non-ambient class + function: emit TS2813/TS2814
                        let name = symbol.escaped_name.clone();
                        for &(decl_idx, flags, is_local, _, _) in &declarations {
                            if is_local
                                && conflicts.contains(&decl_idx)
                                && (flags & symbol_flags::CLASS) != 0
                            {
                                let error_node =
                                    self.get_declaration_name_node(decl_idx).unwrap_or(decl_idx);
                                let message = format_message(
                                    diagnostic_messages::CLASS_DECLARATION_CANNOT_IMPLEMENT_OVERLOAD_LIST_FOR,
                                    &[&name],
                                );
                                self.error_at_node(
                                    error_node,
                                    &message,
                                    diagnostic_codes::CLASS_DECLARATION_CANNOT_IMPLEMENT_OVERLOAD_LIST_FOR,
                                );
                            }
                        }
                        for &(decl_idx, flags, is_local, _, _) in &declarations {
                            if is_local
                                && conflicts.contains(&decl_idx)
                                && (flags & symbol_flags::FUNCTION) != 0
                            {
                                let error_node =
                                    self.get_declaration_name_node(decl_idx).unwrap_or(decl_idx);
                                self.error_at_node(
                                    error_node,
                                    diagnostic_messages::FUNCTION_WITH_BODIES_CAN_ONLY_MERGE_WITH_CLASSES_THAT_ARE_AMBIENT,
                                    diagnostic_codes::FUNCTION_WITH_BODIES_CAN_ONLY_MERGE_WITH_CLASSES_THAT_ARE_AMBIENT,
                                );
                            }
                        }
                    }

                    // Determine if there are other conflicting declarations
                    // beyond the class+function pair (e.g. var in a 3-way conflict).
                    let has_other_conflicts = conflicts
                        .iter()
                        .any(|idx| !local_class_merge_conflicts.contains(idx));

                    if has_other_conflicts {
                        // 3-way+ conflict: keep class+function in conflicts so
                        // the general TS2300 handler below emits on ALL declarations.
                    } else {
                        // Pure 2-way class+function: remove from conflicts.
                        // Ambient case = valid merge, non-ambient = TS2813/2814 only.
                        for idx in local_class_merge_conflicts {
                            conflicts.remove(&idx);
                        }
                        continue;
                    }
                }
            }

            let has_non_block_scoped = declarations.iter().any(|(decl_idx, flags, _, _, _)| {
                conflicts.contains(decl_idx) && {
                    (flags & symbol_flags::BLOCK_SCOPED_VARIABLE) == 0
                }
            });

            let name = symbol.escaped_name.clone();

            let has_remote_declaration =
                declarations.iter().any(|(_, _, is_local, _, _)| !*is_local);
            let has_enum_conflict = if has_remote_declaration {
                declarations.iter().any(|(_, flags, _, _, _)| {
                    (flags & (symbol_flags::REGULAR_ENUM | symbol_flags::CONST_ENUM)) != 0
                })
            } else {
                declarations.iter().any(|(decl_idx, flags, _, _, _)| {
                    conflicts.contains(decl_idx)
                        && (flags & (symbol_flags::REGULAR_ENUM | symbol_flags::CONST_ENUM)) != 0
                })
            };

            let has_variable_conflict = declarations.iter().any(|(decl_idx, flags, _, _, _)| {
                conflicts.contains(decl_idx) && (flags & symbol_flags::VARIABLE) != 0
            });
            let has_non_variable_conflict =
                declarations.iter().any(|(decl_idx, flags, _, _, _)| {
                    conflicts.contains(decl_idx) && (flags & symbol_flags::VARIABLE) == 0
                });
            let has_accessor_conflict = declarations.iter().any(|(decl_idx, flags, _, _, _)| {
                conflicts.contains(decl_idx)
                    && (flags & (symbol_flags::GET_ACCESSOR | symbol_flags::SET_ACCESSOR)) != 0
            });

            // TS2323: Check exported variable conflict using symbol.is_exported
            let has_exported_variable_conflict = symbol.is_exported && has_variable_conflict;

            let (message, code) = if !has_non_block_scoped || has_umd_global_value_conflict {
                (
                    format_message(
                        diagnostic_messages::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE,
                        &[&name],
                    ),
                    diagnostic_codes::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE,
                )
            } else if has_exported_variable_conflict
                && has_variable_conflict
                && !has_non_variable_conflict
                && !has_accessor_conflict
            {
                (
                    format_message(
                        diagnostic_messages::CANNOT_REDECLARE_EXPORTED_VARIABLE,
                        &[&name],
                    ),
                    diagnostic_codes::CANNOT_REDECLARE_EXPORTED_VARIABLE,
                )
            } else if has_enum_conflict && has_non_block_scoped {
                (
                    diagnostic_messages::ENUM_DECLARATIONS_CAN_ONLY_MERGE_WITH_NAMESPACE_OR_OTHER_ENUM_DECLARATIONS
                        .to_string(),
                    diagnostic_codes::ENUM_DECLARATIONS_CAN_ONLY_MERGE_WITH_NAMESPACE_OR_OTHER_ENUM_DECLARATIONS,
                )
            } else {
                if has_ts2395 {
                    continue;
                }

                // Determine TS2451 vs TS2300 for the mixed case (has_non_block_scoped
                // is true, so at least one declaration is not block-scoped).
                //
                // For mixed var + let/const conflicts:
                //   - Cross-file: always TS2451
                //   - Same-file: TS2451 if first declaration is block-scoped,
                //     TS2300 if first declaration is non-block-scoped (var)
                //
                // For purely non-block-scoped conflicts that span different scopes
                // (e.g., var hoisted from a child block to conflict with a
                // function at the parent level), we fall back to scope-based
                // analysis to choose TS2451 vs TS2300.
                let has_block_scoped_conflict =
                    declarations.iter().any(|(decl_idx, flags, _, _, _)| {
                        conflicts.contains(decl_idx)
                            && (flags & symbol_flags::BLOCK_SCOPED_VARIABLE) != 0
                    });
                let has_function_conflict =
                    declarations.iter().any(|(decl_idx, flags, _, _, _)| {
                        conflicts.contains(decl_idx) && (flags & symbol_flags::FUNCTION) != 0
                    });
                let use_ts2451 = if has_remote_declaration && has_block_scoped_conflict {
                    // Cross-file mixed conflicts always use TS2451.
                    true
                } else if has_block_scoped_conflict && has_function_conflict {
                    // When a function declaration conflicts with a block-scoped
                    // variable (let/const) at the same scope, tsc uses TS2300.
                    false
                } else if has_block_scoped_conflict {
                    // Same-file mixed case (var + let/const, no function):
                    // tsc uses TS2451 if the first conflicting declaration (by
                    // source position) is block-scoped (let/const), TS2300 if
                    // the first conflicting declaration is non-block-scoped (var).
                    let first_conflict = declarations
                        .iter()
                        .filter(|(decl_idx, _, is_local, _, _)| {
                            *is_local && conflicts.contains(decl_idx)
                        })
                        .min_by_key(|(decl_idx, _, _, _, _)| {
                            self.ctx
                                .arena
                                .get(*decl_idx)
                                .map_or(u32::MAX, |node| node.pos)
                        });
                    first_conflict
                        .map(|(_, flags, _, _, _)| {
                            (flags & symbol_flags::BLOCK_SCOPED_VARIABLE) != 0
                        })
                        .unwrap_or(true)
                } else if has_remote_declaration {
                    false
                } else {
                    // No block-scoped variables involved. Check if non-block-scoped
                    // conflicting declarations span different scopes (e.g., var
                    // hoisted from a catch block to conflict with a function at the
                    // top level) — in that case tsc uses TS2451.
                    let conflict_scopes: Vec<Option<tsz_binder::ScopeId>> = declarations
                        .iter()
                        .filter(|(decl_idx, _, is_local, _, _)| {
                            *is_local && conflicts.contains(decl_idx)
                        })
                        .map(|(decl_idx, flags, _, _, _)| {
                            let parent_idx = self
                                .ctx
                                .arena
                                .get_extended(*decl_idx)
                                .map(|ext| ext.parent)
                                .unwrap_or(*decl_idx);
                            let scope = self
                                .ctx
                                .binder
                                .find_enclosing_scope(self.ctx.arena, parent_idx);

                            // For non-block-scoped declarations (var, function declarations)
                            // nested inside block scopes (catch blocks, for-loops, etc.),
                            // walk up to the enclosing function/module scope. `var` hoists
                            // to the function scope, so `var w` inside a catch block is at
                            // the same effective scope as `function w()` at the top level.
                            // Also walk up from Module scopes (namespace blocks): merged
                            // namespace declarations share the same parent scope, so
                            // `namespace C { export var x }` and `namespace C { export
                            // function x() {} }` should resolve to the same effective scope
                            // and get TS2300, not TS2451.
                            if (flags & symbol_flags::BLOCK_SCOPED_VARIABLE) == 0
                                && let Some(sid) = scope
                            {
                                let should_walk_up =
                                    self.ctx.binder.scopes.get(sid.0 as usize).is_some_and(|s| {
                                        matches!(
                                            s.kind,
                                            tsz_binder::ContainerKind::Block
                                                | tsz_binder::ContainerKind::Module
                                        )
                                    });
                                if should_walk_up {
                                    let mut cur = sid;
                                    for _ in 0..20 {
                                        if let Some(s) = self.ctx.binder.scopes.get(cur.0 as usize)
                                        {
                                            if matches!(
                                                s.kind,
                                                tsz_binder::ContainerKind::Function
                                                    | tsz_binder::ContainerKind::SourceFile
                                            ) {
                                                return Some(cur);
                                            }
                                            if s.parent == cur {
                                                break;
                                            }
                                            cur = s.parent;
                                        } else {
                                            break;
                                        }
                                    }
                                }
                            }
                            scope
                        })
                        .collect();
                    let first_scope = conflict_scopes.first().copied().flatten();
                    let all_same_scope = conflict_scopes.iter().all(|s| *s == first_scope);
                    !all_same_scope
                };
                if use_ts2451 {
                    (
                        format_message(
                            diagnostic_messages::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE,
                            &[&name],
                        ),
                        diagnostic_codes::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE,
                    )
                } else {
                    (
                        format_message(diagnostic_messages::DUPLICATE_IDENTIFIER, &[&name]),
                        diagnostic_codes::DUPLICATE_IDENTIFIER,
                    )
                }
            };

            // Check if any conflicting declaration is a var shadowing a block-scoped
            // variable in the same scope. If so, TS2481 applies (emitted by
            // check_var_declared_names_not_shadowed) and we skip TS2451/TS2300 here.
            let has_ts2481_var = declarations.iter().any(|(decl_idx, _, is_local, _, _)| {
                *is_local
                    && conflicts.contains(decl_idx)
                    && self.is_var_shadowing_block_scoped_in_same_scope(*decl_idx)
            });
            if has_ts2481_var {
                continue;
            }
            for (decl_idx, _decl_flags, is_local, _, _) in declarations {
                if is_local && conflicts.contains(&decl_idx) {
                    let error_node = self.get_declaration_name_node(decl_idx).unwrap_or(decl_idx);
                    self.error_at_node(error_node, &message, code);
                }
            }
        }

        self.check_block_scoped_function_outer_conflicts();
        self.check_cross_file_global_augmentation_member_conflicts();
        self.check_cross_file_module_augmentation_member_conflicts();
    }

    fn check_block_scoped_function_outer_conflicts(&mut self) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        let mut seen = FxHashSet::default();

        let block_function_decls: Vec<(tsz_binder::SymbolId, NodeIndex, String)> = self
            .ctx
            .binder
            .symbols
            .iter()
            .filter(|symbol| (symbol.flags & symbol_flags::FUNCTION) != 0)
            .flat_map(|symbol| {
                symbol.declarations.iter().filter_map(|&decl_idx| {
                    let node = self.ctx.arena.get(decl_idx)?;
                    if node.kind != tsz_parser::parser::syntax_kind_ext::FUNCTION_DECLARATION {
                        return None;
                    }
                    if self.get_enclosing_block_scope(decl_idx).is_none() {
                        return None;
                    }
                    let name = self.get_declaration_name_text(decl_idx)?;
                    Some((symbol.id, decl_idx, name))
                })
            })
            .collect();

        for (current_sym_id, decl_idx, name) in block_function_decls {
            let Some((outer_sym_id, outer_decls)) = self
                .find_visible_outer_declarations_for_block_function(
                    decl_idx,
                    current_sym_id,
                    &name,
                )
            else {
                continue;
            };

            if !seen.insert((decl_idx, outer_sym_id)) {
                continue;
            }

            let block_function_has_body = self.function_has_body(decl_idx);
            let block_error_node = self.get_declaration_name_node(decl_idx).unwrap_or(decl_idx);

            let outer_function_impls: Vec<NodeIndex> = outer_decls
                .iter()
                .filter_map(|(outer_decl_idx, flags)| {
                    ((flags & symbol_flags::FUNCTION) != 0
                        && self.function_has_body(*outer_decl_idx)
                        && !self.is_ambient_declaration(*outer_decl_idx))
                    .then_some(*outer_decl_idx)
                })
                .collect();
            if block_function_has_body && !outer_function_impls.is_empty() {
                // Block-scoped or nested function with a body that shadows an outer
                // function also with a body is legal shadowing in TypeScript — not a
                // duplicate implementation.  TS2393 only applies to duplicate function
                // implementations within the *same* scope, which is already handled by
                // the scope-grouped check above.
                continue;
            }

            let has_ambient_outer_function = outer_decls.iter().any(|(outer_decl_idx, flags)| {
                (flags & symbol_flags::FUNCTION) != 0
                    && self.is_ambient_declaration(*outer_decl_idx)
                    && !self.function_has_body(*outer_decl_idx)
            });
            if block_function_has_body && has_ambient_outer_function {
                self.error_at_node(
                    block_error_node,
                    diagnostic_messages::OVERLOAD_SIGNATURES_MUST_ALL_BE_AMBIENT_OR_NON_AMBIENT,
                    diagnostic_codes::OVERLOAD_SIGNATURES_MUST_ALL_BE_AMBIENT_OR_NON_AMBIENT,
                );
                continue;
            }

            let outer_class_decls: Vec<NodeIndex> = outer_decls
                .iter()
                .filter_map(|(outer_decl_idx, flags)| {
                    ((flags & symbol_flags::CLASS) != 0).then_some(*outer_decl_idx)
                })
                .collect();
            if !outer_class_decls.is_empty() {
                let all_classes_ambient = outer_class_decls
                    .iter()
                    .all(|outer_decl_idx| self.is_ambient_declaration(*outer_decl_idx));
                if block_function_has_body && !all_classes_ambient {
                    let message = format_message(
                        diagnostic_messages::CLASS_DECLARATION_CANNOT_IMPLEMENT_OVERLOAD_LIST_FOR,
                        &[&name],
                    );
                    for outer_decl_idx in outer_class_decls {
                        let error_node = self
                            .get_declaration_name_node(outer_decl_idx)
                            .unwrap_or(outer_decl_idx);
                        self.error_at_node(
                            error_node,
                            &message,
                            diagnostic_codes::CLASS_DECLARATION_CANNOT_IMPLEMENT_OVERLOAD_LIST_FOR,
                        );
                    }
                    self.error_at_node(
                        block_error_node,
                        diagnostic_messages::FUNCTION_WITH_BODIES_CAN_ONLY_MERGE_WITH_CLASSES_THAT_ARE_AMBIENT,
                        diagnostic_codes::FUNCTION_WITH_BODIES_CAN_ONLY_MERGE_WITH_CLASSES_THAT_ARE_AMBIENT,
                    );
                }
                continue;
            }

            let block_flags = self
                .declaration_symbol_flags(self.ctx.arena, decl_idx)
                .unwrap_or(symbol_flags::FUNCTION);
            let conflicting_outer_decls: Vec<(NodeIndex, u32)> = outer_decls
                .iter()
                .copied()
                .filter(|(_, flags)| Self::declarations_conflict(block_flags, *flags))
                .collect();
            if conflicting_outer_decls.is_empty() {
                continue;
            }

            // In ES6+, function declarations inside blocks are block-scoped.
            // They don't escape the block, so they don't conflict with
            // let/const in outer scopes. Skip when ALL conflicting outer
            // declarations are block-scoped variables.
            let all_outer_are_block_scoped = conflicting_outer_decls
                .iter()
                .all(|(_, flags)| (flags & symbol_flags::BLOCK_SCOPED_VARIABLE) != 0);
            if all_outer_are_block_scoped {
                continue;
            }

            let first_decl = conflicting_outer_decls
                .iter()
                .copied()
                .chain(std::iter::once((decl_idx, block_flags)))
                .min_by_key(|(decl_idx, _)| {
                    self.ctx
                        .arena
                        .get(*decl_idx)
                        .map_or(u32::MAX, |node| node.pos)
                });

            let use_ts2451 = first_decl
                .map(|(_, flags)| (flags & symbol_flags::BLOCK_SCOPED_VARIABLE) != 0)
                .unwrap_or(false);
            let (message, code) = if use_ts2451 {
                (
                    format_message(
                        diagnostic_messages::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE,
                        &[&name],
                    ),
                    diagnostic_codes::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE,
                )
            } else {
                (
                    format_message(diagnostic_messages::DUPLICATE_IDENTIFIER, &[&name]),
                    diagnostic_codes::DUPLICATE_IDENTIFIER,
                )
            };

            for (outer_decl_idx, _) in conflicting_outer_decls {
                let error_node = self
                    .get_declaration_name_node(outer_decl_idx)
                    .unwrap_or(outer_decl_idx);
                self.error_at_node(error_node, &message, code);
            }
            self.error_at_node(block_error_node, &message, code);
        }
    }

    /// Check diagnostics specific to merged interface declarations.
    ///
    /// - TS2717: Subsequent property declarations with the same name must have identical types.
    /// - TS2413: Merged index signatures must be compatible.
    fn check_merged_interface_declaration_diagnostics(&mut self, declarations: &[NodeIndex]) {
        use crate::diagnostics::diagnostic_codes;
        use rustc_hash::FxHashMap;
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        if declarations.len() <= 1 {
            return;
        }

        // Group by SymbolId (not NodeIndex) so separate `namespace M {}` blocks with
        // the same name are treated as one scope — matching the TS2428 grouping fix.
        let mut declarations_by_scope: FxHashMap<tsz_binder::SymbolId, Vec<NodeIndex>> =
            FxHashMap::default();
        for &decl_idx in declarations {
            let scope = self.get_enclosing_namespace_symbol(decl_idx);
            declarations_by_scope
                .entry(scope)
                .or_default()
                .push(decl_idx);
        }

        for (_, mut declarations_in_scope) in declarations_by_scope {
            if declarations_in_scope.len() <= 1 {
                continue;
            }

            // Merge diagnostics only when interface type parameters are identical.
            // TS2428 is reported separately; once mismatched, compatibility checks
            // should not be compared across declarations in the same scope.
            let Some(first_decl) = declarations_in_scope.first().copied() else {
                continue;
            };
            if !declarations_in_scope[1..].iter().all(|&decl_idx| {
                self.interface_type_parameters_are_merge_compatible(first_decl, decl_idx)
            }) {
                continue;
            }

            declarations_in_scope.sort_by_key(|&decl_idx| {
                self.ctx
                    .arena
                    .get(decl_idx)
                    .map(|node| node.pos)
                    .unwrap_or(u32::MAX)
            });

            let mut merged_string_index: Option<TypeId> = None;
            let mut merged_number_index: Option<TypeId> = None;
            let mut merged_string_index_node: Option<NodeIndex> = None;
            let mut merged_number_index_node: Option<NodeIndex> = None;
            // Track type, whether the member is a method signature, and the
            // name node index. When the same name appears as both property
            // and method across merged declarations, tsc emits TS2300
            // "Duplicate identifier" on both declarations.
            let mut merged_properties: FxHashMap<String, (TypeId, bool, NodeIndex)> =
                FxHashMap::default();

            for &decl_idx in &declarations_in_scope {
                let Some(node) = self.ctx.arena.get(decl_idx) else {
                    continue;
                };
                let Some(iface) = self.ctx.arena.get_interface(node) else {
                    continue;
                };

                // Resolve interface-local type parameters before reading member signatures.
                let (_type_params, updates) = self.push_type_parameters(&iface.type_parameters);

                // (name, name_node, type, is_numeric, is_method)
                let mut local_properties: Vec<(String, NodeIndex, TypeId, bool, bool)> = Vec::new();
                let mut local_string_index: Option<TypeId> = None;
                let mut local_number_index: Option<TypeId> = None;
                let mut local_string_index_node = NodeIndex::NONE;
                let mut local_number_index_node = NodeIndex::NONE;

                for &member_idx in &iface.members.nodes {
                    let Some(member_node) = self.ctx.arena.get(member_idx) else {
                        continue;
                    };

                    if member_node.kind == syntax_kind_ext::PROPERTY_SIGNATURE
                        || member_node.kind == syntax_kind_ext::METHOD_SIGNATURE
                    {
                        let is_method = member_node.kind == syntax_kind_ext::METHOD_SIGNATURE;
                        let Some(sig) = self.ctx.arena.get_signature(member_node) else {
                            continue;
                        };
                        let Some(name) = self.get_property_name(sig.name) else {
                            continue;
                        };

                        let is_numeric_name = self
                            .ctx
                            .arena
                            .get(sig.name)
                            .is_some_and(|n| n.kind == SyntaxKind::NumericLiteral as u16);
                        let property_type = if is_method {
                            // Build a function type from the method signature so we can
                            // compare against a property with the same name (TS2717).
                            let (type_params, tp_updates) =
                                self.push_type_parameters(&sig.type_parameters);
                            let (params, _this_type) = if let Some(ref param_list) = sig.parameters
                            {
                                self.extract_params_from_parameter_list(param_list)
                            } else {
                                (Vec::new(), None)
                            };
                            let return_type = if sig.type_annotation.is_some() {
                                self.get_type_from_type_node(sig.type_annotation)
                            } else {
                                TypeId::ANY
                            };
                            self.pop_type_parameters(tp_updates);
                            self.ctx
                                .types
                                .factory()
                                .function(tsz_solver::FunctionShape {
                                    type_params,
                                    params,
                                    this_type: None,
                                    return_type,
                                    type_predicate: None,
                                    is_constructor: false,
                                    is_method: true,
                                })
                        } else if sig.type_annotation.is_some() {
                            self.get_type_from_type_node(sig.type_annotation)
                        } else {
                            TypeId::ANY
                        };
                        local_properties.push((
                            name,
                            sig.name,
                            property_type,
                            is_numeric_name,
                            is_method,
                        ));
                    } else if member_node.kind == syntax_kind_ext::INDEX_SIGNATURE {
                        let Some(index_sig) = self.ctx.arena.get_index_signature(member_node)
                        else {
                            continue;
                        };
                        let Some(param_idx) = index_sig.parameters.nodes.first().copied() else {
                            continue;
                        };
                        let Some(param_node) = self.ctx.arena.get(param_idx) else {
                            continue;
                        };
                        let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                            continue;
                        };
                        if param.type_annotation.is_none() {
                            continue;
                        }
                        let key_type = self.get_type_from_type_node(param.type_annotation);
                        let value_type = if index_sig.type_annotation.is_none() {
                            continue;
                        } else {
                            self.get_type_from_type_node(index_sig.type_annotation)
                        };
                        if self.type_contains_error(key_type)
                            || self.type_contains_error(value_type)
                        {
                            continue;
                        }

                        if key_type == TypeId::STRING {
                            local_string_index = Some(value_type);
                            local_string_index_node = member_idx;
                        } else if key_type == TypeId::NUMBER {
                            local_number_index = Some(value_type);
                            local_number_index_node = member_idx;
                        }
                    }
                }

                // Apply merged declarations checks for property/method signatures.
                for (name, name_idx, property_type, is_numeric, is_method) in &local_properties {
                    if let Some(&(existing_type, existing_is_method, existing_name_idx)) =
                        merged_properties.get(name)
                    {
                        // Handle property-vs-method conflicts across merged declarations.
                        if *is_method != existing_is_method {
                            if *is_method && !existing_is_method {
                                // Method after property: TS2300 on both declarations.
                                // tsc treats a method signature conflicting with an
                                // existing property signature as a duplicate identifier.
                                let message = crate::diagnostics::format_message(
                                    crate::diagnostics::diagnostic_messages::DUPLICATE_IDENTIFIER,
                                    &[name],
                                );
                                self.error_at_node(
                                    existing_name_idx,
                                    &message,
                                    diagnostic_codes::DUPLICATE_IDENTIFIER,
                                );
                                self.error_at_node(
                                    *name_idx,
                                    &message,
                                    diagnostic_codes::DUPLICATE_IDENTIFIER,
                                );
                            } else {
                                // Property after method: TS2717 comparing property type
                                // against the method's function type.
                                if !self.type_contains_error(*property_type)
                                    && !self.type_contains_error(existing_type)
                                {
                                    let compatible_both_ways = self
                                        .is_assignable_to(existing_type, *property_type)
                                        && self.is_assignable_to(*property_type, existing_type);
                                    if !compatible_both_ways {
                                        let existing_type_str = self.format_type(existing_type);
                                        let property_type_str = self.format_type(*property_type);
                                        self.error_at_node_msg(
                                            *name_idx,
                                            diagnostic_codes::SUBSEQUENT_PROPERTY_DECLARATIONS_MUST_HAVE_THE_SAME_TYPE_PROPERTY_MUST_BE_OF_TYP,
                                            &[name, &existing_type_str, &property_type_str],
                                        );
                                    }
                                }
                            }
                            continue;
                        }

                        if self.type_contains_error(*property_type)
                            || self.type_contains_error(existing_type)
                        {
                            continue;
                        }

                        // For same-kind members, check type compatibility (TS2717).
                        // Method overloads (multiple methods with same name) are valid
                        // and don't need compatibility checking here.
                        if !*is_method {
                            let compatible_both_ways = self
                                .is_assignable_to(existing_type, *property_type)
                                && self.is_assignable_to(*property_type, existing_type);
                            if !compatible_both_ways {
                                let existing_type_str = self.format_type(existing_type);
                                let property_type_str = self.format_type(*property_type);
                                self.error_at_node_msg(
                                    *name_idx,
                                    diagnostic_codes::SUBSEQUENT_PROPERTY_DECLARATIONS_MUST_HAVE_THE_SAME_TYPE_PROPERTY_MUST_BE_OF_TYP,
                                    &[name, &existing_type_str, &property_type_str],
                                );
                            }
                        }
                    } else {
                        // Keep first declaration as canonical for subsequent comparisons.
                        // Matching declarations are not yet merged into this map.
                    }

                    if *is_numeric
                        && let Some(number_index) = local_number_index.or(merged_number_index)
                        && !self.is_assignable_to(*property_type, number_index)
                    {
                        let index_type_str = self.format_type(number_index);
                        self.error_at_node_msg(
                            *name_idx,
                            diagnostic_codes::PROPERTY_OF_TYPE_IS_NOT_ASSIGNABLE_TO_INDEX_TYPE,
                            &[
                                name,
                                &self.format_type(*property_type),
                                "number",
                                &index_type_str,
                            ],
                        );
                    }

                    if let Some(string_index) = local_string_index.or(merged_string_index)
                        && !self.is_assignable_to(*property_type, string_index)
                    {
                        let index_type_str = self.format_type(string_index);
                        self.error_at_node_msg(
                            *name_idx,
                            diagnostic_codes::PROPERTY_OF_TYPE_IS_NOT_ASSIGNABLE_TO_INDEX_TYPE,
                            &[
                                name,
                                &self.format_type(*property_type),
                                "string",
                                &index_type_str,
                            ],
                        );
                    }
                }

                for (name, name_idx, property_type, _is_numeric, is_method) in local_properties {
                    merged_properties
                        .entry(name)
                        .or_insert((property_type, is_method, name_idx));
                }

                // Check declaration-local index signatures against already-seen
                // same-kind signatures.  Number-vs-string (TS2413) cross-checks
                // are handled by check_index_signature_compatibility which sees
                // the merged solver index info and always reports on the number
                // index node (matching TSC).
                if let Some(local_number) = local_number_index
                    && let Some(existing_number) = merged_number_index
                {
                    // TS2374: Duplicate index signature for type 'number'.
                    // Emit on both the first and current occurrence (tsc behavior).
                    if let Some(first_node) = merged_number_index_node {
                        self.error_at_node_msg(
                            first_node,
                            diagnostic_codes::DUPLICATE_INDEX_SIGNATURE_FOR_TYPE,
                            &["number"],
                        );
                        merged_number_index_node = None; // Only report first node once
                    }
                    self.error_at_node_msg(
                        local_number_index_node,
                        diagnostic_codes::DUPLICATE_INDEX_SIGNATURE_FOR_TYPE,
                        &["number"],
                    );

                    let local_str = self.format_type(local_number);
                    let existing_str = self.format_type(existing_number);
                    if !self.is_assignable_to(local_number, existing_number)
                        && !self.is_assignable_to(existing_number, local_number)
                    {
                        self.error_at_node_msg(
                            local_number_index_node,
                            diagnostic_codes::INDEX_TYPE_IS_NOT_ASSIGNABLE_TO_INDEX_TYPE,
                            &["number", &local_str, "number", &existing_str],
                        );
                    }
                }

                if let Some(local_string) = local_string_index
                    && let Some(existing_string) = merged_string_index
                {
                    // TS2374: Duplicate index signature for type 'string'.
                    // Emit on both the first and current occurrence (tsc behavior).
                    if let Some(first_node) = merged_string_index_node {
                        self.error_at_node_msg(
                            first_node,
                            diagnostic_codes::DUPLICATE_INDEX_SIGNATURE_FOR_TYPE,
                            &["string"],
                        );
                        merged_string_index_node = None; // Only report first node once
                    }
                    self.error_at_node_msg(
                        local_string_index_node,
                        diagnostic_codes::DUPLICATE_INDEX_SIGNATURE_FOR_TYPE,
                        &["string"],
                    );

                    let local_str = self.format_type(local_string);
                    let existing_str = self.format_type(existing_string);
                    if !self.is_assignable_to(local_string, existing_string)
                        && !self.is_assignable_to(existing_string, local_string)
                    {
                        self.error_at_node_msg(
                            local_string_index_node,
                            diagnostic_codes::INDEX_TYPE_IS_NOT_ASSIGNABLE_TO_INDEX_TYPE,
                            &["string", &local_str, "string", &existing_str],
                        );
                    }
                }

                if merged_number_index.is_none()
                    && let Some(local_number) = local_number_index
                {
                    merged_number_index = Some(local_number);
                    merged_number_index_node = Some(local_number_index_node);
                }

                if merged_string_index.is_none()
                    && let Some(local_string) = local_string_index
                {
                    merged_string_index = Some(local_string);
                    merged_string_index_node = Some(local_string_index_node);
                }

                self.pop_type_parameters(updates);
            }
        }
    }
}
