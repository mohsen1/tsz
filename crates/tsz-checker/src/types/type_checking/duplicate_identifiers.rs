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
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DuplicateDeclarationOrigin {
    SymbolDeclaration,
    TargetedModuleAugmentation,
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
                if !has_cross_file && module_augmentation_declarations.is_empty() {
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

            if !module_augmentation_declarations.is_empty() {
                has_remote = true;
            }

            if has_local && has_remote {
                cross_file_conflicts.push(symbol.escaped_name.clone());
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
                if !has_cross_file && module_augmentation_declarations.is_empty() {
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

            declarations.extend(module_augmentation_declarations);

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
                            let error_node =
                                self.get_declaration_name_node(decl_idx).unwrap_or(decl_idx);
                            self.error_at_node(
                                error_node,
                                diagnostic_messages::OVERLOAD_SIGNATURES_MUST_ALL_BE_PUBLIC_PRIVATE_OR_PROTECTED,
                                diagnostic_codes::OVERLOAD_SIGNATURES_MUST_ALL_BE_PUBLIC_PRIVATE_OR_PROTECTED,
                            );
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
                continue;
            }

            // TS2395
            let mut has_ts2395 = false;
            {
                const SPACE_TYPE: u32 = 1;
                const SPACE_VALUE: u32 = 2;
                const SPACE_NAMESPACE: u32 = 4;

                let any_in_declare_context = self.ctx.is_declaration_file()
                    || declarations.iter().any(|&(decl_idx, _, is_local, _, _)| {
                        is_local && self.is_in_declare_namespace_or_module(decl_idx)
                    });

                let mut error_nodes: Vec<NodeIndex> = Vec::new();

                if !any_in_declare_context {
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
                            conflicts.remove(&idx);
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
                let has_class = declarations.iter().any(|(decl_idx, flags, _, _, _)| {
                    conflicts.contains(decl_idx) && (flags & symbol_flags::CLASS) != 0
                });
                let has_function = declarations.iter().any(|(decl_idx, flags, _, _, _)| {
                    conflicts.contains(decl_idx) && (flags & symbol_flags::FUNCTION) != 0
                });

                if has_class && has_function {
                    // Check if ALL class declarations in conflicts are ambient
                    let all_classes_ambient =
                        declarations.iter().all(|(decl_idx, flags, _, _, _)| {
                            !conflicts.contains(decl_idx)
                                || (flags & symbol_flags::CLASS) == 0
                                || self.is_ambient_declaration(*decl_idx)
                        });

                    if !all_classes_ambient {
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
                                self.error_at_node_msg(
                                    error_node,
                                    diagnostic_codes::DUPLICATE_IDENTIFIER,
                                    &[name.as_str()],
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
                                self.error_at_node_msg(
                                    error_node,
                                    diagnostic_codes::DUPLICATE_IDENTIFIER,
                                    &[name.as_str()],
                                );
                            }
                        }
                    }

                    // Remove class+function from conflicts in both cases
                    // (ambient = valid merge, non-ambient = already reported TS2813/2814)
                    let class_function_indices: Vec<NodeIndex> = declarations
                        .iter()
                        .filter(|(decl_idx, flags, _, _, _)| {
                            conflicts.contains(decl_idx)
                                && ((flags & symbol_flags::CLASS) != 0
                                    || (flags & symbol_flags::FUNCTION) != 0)
                        })
                        .map(|(idx, _, _, _, _)| *idx)
                        .collect();
                    for idx in class_function_indices {
                        conflicts.remove(&idx);
                    }
                    if conflicts.is_empty() {
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

            let has_enum_conflict = declarations.iter().any(|(decl_idx, flags, _, _, _)| {
                conflicts.contains(decl_idx)
                    && (flags & (symbol_flags::REGULAR_ENUM | symbol_flags::CONST_ENUM)) != 0
            });

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

            let (message, code) = if !has_non_block_scoped {
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

                // Determine TS2451 vs TS2300:
                // - Single-file: check if the first declaration (by source position)
                //   is block-scoped, matching tsc's binder processing order.
                // - Cross-file: if ANY declaration is block-scoped → TS2451, because
                //   each file's binder processes independently.
                let has_remote = declarations.iter().any(|(_, _, is_local, _, _)| !*is_local);
                let use_ts2451 = if has_remote {
                    // Cross-file: any block-scoped declaration in the merged symbol
                    // triggers TS2451, even when the current file's conflicting
                    // declaration is non-block-scoped (for example `class Bar {}` in
                    // one file vs `const Bar = 1` in another). Each file is checked
                    // against the merged declaration set, not only the local subset.
                    declarations.iter().any(|(_, flags, _, _, _)| {
                        (flags & symbol_flags::BLOCK_SCOPED_VARIABLE) != 0
                    })
                } else {
                    // Single-file: check first declaration by source position
                    let first_decl_flags = declarations
                        .iter()
                        .filter(|(decl_idx, _, is_local, _, _)| {
                            *is_local && conflicts.contains(decl_idx)
                        })
                        .min_by_key(|(decl_idx, _, _, _, _)| {
                            self.ctx.arena.get(*decl_idx).map_or(u32::MAX, |n| n.pos)
                        })
                        .map(|d| d.1)
                        .unwrap_or(0);
                    (first_decl_flags & symbol_flags::BLOCK_SCOPED_VARIABLE) != 0
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

            for (decl_idx, _decl_flags, is_local, _, _) in declarations {
                if is_local && conflicts.contains(&decl_idx) {
                    let error_node = self.get_declaration_name_node(decl_idx).unwrap_or(decl_idx);
                    self.error_at_node(error_node, &message, code);
                }
            }
        }
    }

    fn get_enclosing_namespace(&self, decl_idx: NodeIndex) -> NodeIndex {
        use tsz_parser::parser::syntax_kind_ext;
        let mut current = decl_idx;
        loop {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return NodeIndex::NONE;
            };
            let parent = ext.parent;
            if parent.is_none() {
                return NodeIndex::NONE;
            }
            let Some(parent_node) = self.ctx.arena.get(parent) else {
                return NodeIndex::NONE;
            };
            if parent_node.kind == syntax_kind_ext::MODULE_DECLARATION {
                return parent;
            }
            if parent_node.kind == syntax_kind_ext::SOURCE_FILE {
                return NodeIndex::NONE;
            }
            current = parent;
        }
    }

    /// Get the SymbolId of the enclosing namespace for a declaration.
    /// Returns `SymbolId::NONE` for file/global scope declarations.
    /// Unlike `get_enclosing_namespace` (which returns a `NodeIndex`), this resolves
    /// to the namespace's symbol, ensuring that separate `namespace M { }` blocks
    /// with the same name map to the same key.
    fn get_enclosing_namespace_symbol(&self, decl_idx: NodeIndex) -> tsz_binder::SymbolId {
        let ns_node = self.get_enclosing_namespace(decl_idx);
        if ns_node.is_none() {
            return tsz_binder::SymbolId::NONE;
        }
        // Look up the symbol for this MODULE_DECLARATION node
        self.ctx
            .binder
            .node_symbols
            .get(&ns_node.0)
            .copied()
            .unwrap_or(tsz_binder::SymbolId::NONE)
    }

    fn module_augmentation_conflict_declarations_for_current_file(
        &self,
        name: &str,
    ) -> Vec<(NodeIndex, u32, bool, bool, DuplicateDeclarationOrigin)> {
        let Some(_arenas) = self.ctx.all_arenas.as_ref() else {
            return Vec::new();
        };

        let mut declarations = Vec::new();

        for (module_spec, augmentations) in &self.ctx.binder.module_augmentations {
            for augmentation in augmentations {
                if augmentation.name != name {
                    continue;
                }

                let Some(arena) = augmentation.arena.as_deref() else {
                    continue;
                };
                let Some(source_file_idx) = self.ctx.get_file_idx_for_arena(arena) else {
                    continue;
                };
                if self
                    .ctx
                    .resolve_import_target_from_file(source_file_idx, module_spec)
                    != Some(self.ctx.current_file_idx)
                {
                    continue;
                }

                let Some(flags) = self.declaration_symbol_flags(arena, augmentation.node) else {
                    continue;
                };
                let is_exported = self.is_declaration_exported(arena, augmentation.node);
                declarations.push((
                    augmentation.node,
                    flags,
                    false,
                    is_exported,
                    DuplicateDeclarationOrigin::TargetedModuleAugmentation,
                ));
            }
        }

        declarations
    }

    /// Get the `NodeIndex` of the nearest enclosing block scope for a declaration.
    /// Returns the first Block, `CaseBlock`, `ForStatement`, etc. ancestor.
    /// Returns `NodeIndex::NONE` if the declaration is directly in a function/module scope.
    fn get_enclosing_block_scope(&self, decl_idx: NodeIndex) -> NodeIndex {
        use tsz_parser::parser::syntax_kind_ext;
        let mut current = decl_idx;
        loop {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return NodeIndex::NONE;
            };
            let parent = ext.parent;
            if parent.is_none() {
                return NodeIndex::NONE;
            }
            let Some(parent_node) = self.ctx.arena.get(parent) else {
                return NodeIndex::NONE;
            };
            match parent_node.kind {
                // Block-creating scopes - return this as the enclosing scope
                syntax_kind_ext::BLOCK
                | syntax_kind_ext::CASE_BLOCK
                | syntax_kind_ext::FOR_STATEMENT
                | syntax_kind_ext::FOR_IN_STATEMENT
                | syntax_kind_ext::FOR_OF_STATEMENT => {
                    return parent;
                }
                // Function/module boundaries - no enclosing block scope
                syntax_kind_ext::FUNCTION_DECLARATION
                | syntax_kind_ext::FUNCTION_EXPRESSION
                | syntax_kind_ext::ARROW_FUNCTION
                | syntax_kind_ext::MODULE_DECLARATION
                | syntax_kind_ext::SOURCE_FILE => {
                    return NodeIndex::NONE;
                }
                _ => {}
            }
            current = parent;
        }
    }

    /// Check diagnostics specific to merged enum declarations.
    ///
    /// - TS2432: In an enum with multiple declarations, only one declaration can
    ///   omit an initializer for its first enum element.
    /// - TS2300: Duplicate enum member names across different enum declarations.
    fn check_merged_enum_declaration_diagnostics(&mut self, declarations: &[(NodeIndex, u32)]) {
        use crate::diagnostics::diagnostic_codes;
        use rustc_hash::FxHashMap;

        let enum_declarations: Vec<NodeIndex> = declarations
            .iter()
            .filter(|&(_decl_idx, flags)| (flags & symbol_flags::ENUM) != 0)
            .map(|(decl_idx, _flags)| *decl_idx)
            .collect();

        if enum_declarations.len() <= 1 {
            return;
        }

        let mut first_member_without_initializer = Vec::new();
        let mut first_member_by_name: FxHashMap<String, (NodeIndex, NodeIndex, bool)> =
            FxHashMap::default();

        for &enum_decl_idx in &enum_declarations {
            let Some(enum_decl_node) = self.ctx.arena.get(enum_decl_idx) else {
                continue;
            };
            let Some(enum_decl) = self.ctx.arena.get_enum(enum_decl_node) else {
                continue;
            };

            if let Some(&first_member_idx) = enum_decl.members.nodes.first()
                && let Some(first_member_node) = self.ctx.arena.get(first_member_idx)
                && let Some(first_member) = self.ctx.arena.get_enum_member(first_member_node)
                && first_member.initializer.is_none()
            {
                first_member_without_initializer.push(first_member_idx);
            }

            for &member_idx in &enum_decl.members.nodes {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    continue;
                };
                let Some(member) = self.ctx.arena.get_enum_member(member_node) else {
                    continue;
                };
                let Some(member_name_node) = self.ctx.arena.get(member.name) else {
                    continue;
                };

                let member_name =
                    if let Some(ident) = self.ctx.arena.get_identifier(member_name_node) {
                        ident.escaped_text.clone()
                    } else if let Some(literal) = self.ctx.arena.get_literal(member_name_node) {
                        literal.text.clone()
                    } else {
                        continue;
                    };

                if let Some((first_member_idx, first_decl_idx, first_reported)) =
                    first_member_by_name.get_mut(&member_name)
                {
                    if *first_decl_idx != enum_decl_idx {
                        if !*first_reported {
                            let first_name_idx = self
                                .ctx
                                .arena
                                .get(*first_member_idx)
                                .and_then(|node| self.ctx.arena.get_enum_member(node))
                                .map(|member| member.name)
                                .unwrap_or(*first_member_idx);
                            self.error_at_node_msg(
                                first_name_idx,
                                diagnostic_codes::DUPLICATE_IDENTIFIER,
                                &[&member_name],
                            );
                            *first_reported = true;
                        }
                        self.error_at_node_msg(
                            member.name,
                            diagnostic_codes::DUPLICATE_IDENTIFIER,
                            &[&member_name],
                        );
                    }
                } else {
                    first_member_by_name
                        .insert(member_name.clone(), (member_idx, enum_decl_idx, false));
                }
            }
        }

        if first_member_without_initializer.len() > 1 {
            // The first declaration that omits an initializer is allowed;
            // only subsequent ones get TS2432.
            for &member_idx in &first_member_without_initializer[1..] {
                self.error_at_node_msg(
                    member_idx,
                    diagnostic_codes::IN_AN_ENUM_WITH_MULTIPLE_DECLARATIONS_ONLY_ONE_DECLARATION_CAN_OMIT_AN_INITIALIZ,
                    &[],
                );
            }
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
                }

                if merged_string_index.is_none()
                    && let Some(local_string) = local_string_index
                {
                    merged_string_index = Some(local_string);
                }

                self.pop_type_parameters(updates);
            }
        }
    }

    /// Check cross-file interface member conflicts (property vs method with same name).
    ///
    /// When the same interface is declared across files, and one file uses a property
    /// signature while the other uses a method signature for the same member name,
    /// tsc emits TS2300 "Duplicate identifier" on the local declarations.
    fn check_cross_file_interface_member_conflicts(
        &mut self,
        sym_id: tsz_binder::SymbolId,
        local_interface_decls: &[NodeIndex],
        remote_interface_decls: &[NodeIndex],
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use rustc_hash::FxHashMap;
        use tsz_parser::parser::syntax_kind_ext;

        // Collect member names and whether they are methods from remote interfaces.
        // Maps member name -> is_method
        let mut remote_members: FxHashMap<String, bool> = FxHashMap::default();

        for &decl_idx in remote_interface_decls {
            // Look up the remote arena for this declaration
            let arenas = self.ctx.binder.declaration_arenas.get(&(sym_id, decl_idx));
            let remote_arenas: Vec<&tsz_parser::parser::NodeArena> = if let Some(arenas) = arenas {
                arenas
                    .iter()
                    .filter(|a| !std::ptr::eq(&***a, self.ctx.arena))
                    .map(|a| &**a)
                    .collect()
            } else {
                continue;
            };

            for remote_arena in remote_arenas {
                let Some(node) = remote_arena.get(decl_idx) else {
                    continue;
                };
                let Some(iface) = remote_arena.get_interface(node) else {
                    continue;
                };

                for &member_idx in &iface.members.nodes {
                    let Some(member_node) = remote_arena.get(member_idx) else {
                        continue;
                    };

                    if member_node.kind == syntax_kind_ext::PROPERTY_SIGNATURE
                        || member_node.kind == syntax_kind_ext::METHOD_SIGNATURE
                    {
                        let is_method = member_node.kind == syntax_kind_ext::METHOD_SIGNATURE;
                        let Some(sig) = remote_arena.get_signature(member_node) else {
                            continue;
                        };
                        let Some(name) =
                            crate::types_domain::queries::core::get_literal_property_name(
                                remote_arena,
                                sig.name,
                            )
                        else {
                            continue;
                        };
                        remote_members.insert(name, is_method);
                    }
                }
            }
        }

        if remote_members.is_empty() {
            return;
        }

        // Now check local interface members against remote members
        for &decl_idx in local_interface_decls {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let Some(iface) = self.ctx.arena.get_interface(node) else {
                continue;
            };

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

                    if let Some(&remote_is_method) = remote_members.get(&name)
                        && is_method != remote_is_method
                    {
                        // Property-vs-method conflict across files
                        let message =
                            format_message(diagnostic_messages::DUPLICATE_IDENTIFIER, &[&name]);
                        self.error_at_node(
                            sig.name,
                            &message,
                            diagnostic_codes::DUPLICATE_IDENTIFIER,
                        );
                    }
                }
            }
        }
    }

    /// Check for declarations that conflict with built-in global identifiers (TS2397).
    ///
    /// TypeScript protects the built-in global names `undefined` and `globalThis`
    /// from being redeclared:
    /// - `var undefined = null;` → TS2397 (value declaration of `undefined`)
    /// - `namespace globalThis {}` → TS2397 (in non-module/script files)
    /// - `var globalThis;` → TS2397 (in non-module/script files)
    ///
    /// Type declarations (interfaces, type aliases, etc.) named `undefined` are
    /// allowed — `checkTypeNameIsReserved` handles those separately.
    pub(crate) fn check_built_in_global_identifier_conflicts(&mut self) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use tsz_parser::parser::syntax_kind_ext;

        let is_external_module = self
            .ctx
            .is_external_module_by_file
            .as_ref()
            .and_then(|m| m.get(&self.ctx.file_name))
            .copied()
            .unwrap_or_else(|| self.ctx.binder.is_external_module());

        // Check `undefined` redeclaration.
        // tsc checks if `undefined` exists in globals and emits TS2397 for each
        // non-type declaration. We check the file-level locals.
        if let Some(sym_id) = self.ctx.binder.file_locals.get("undefined")
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
        {
            for &decl_idx in &symbol.declarations {
                let Some(node) = self.ctx.arena.get(decl_idx) else {
                    continue;
                };
                // Skip type declarations (interfaces, type aliases)
                if node.kind == syntax_kind_ext::INTERFACE_DECLARATION
                    || node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                {
                    continue;
                }
                let error_node = self.get_declaration_name_node(decl_idx).unwrap_or(decl_idx);
                let message = format_message(
                    diagnostic_messages::DECLARATION_NAME_CONFLICTS_WITH_BUILT_IN_GLOBAL_IDENTIFIER,
                    &["undefined"],
                );
                self.error_at_node(
                    error_node,
                    &message,
                    diagnostic_codes::DECLARATION_NAME_CONFLICTS_WITH_BUILT_IN_GLOBAL_IDENTIFIER,
                );
            }
        }

        // Check `globalThis` redeclaration (only in non-module files).
        // In module files (files with import/export), `globalThis` declarations
        // are allowed because they don't conflict with the global scope.
        if !is_external_module
            && let Some(sym_id) = self.ctx.binder.file_locals.get("globalThis")
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
        {
            for &decl_idx in &symbol.declarations {
                let error_node = self.get_declaration_name_node(decl_idx).unwrap_or(decl_idx);
                let message = format_message(
                    diagnostic_messages::DECLARATION_NAME_CONFLICTS_WITH_BUILT_IN_GLOBAL_IDENTIFIER,
                    &["globalThis"],
                );
                self.error_at_node(
                    error_node,
                    &message,
                    diagnostic_codes::DECLARATION_NAME_CONFLICTS_WITH_BUILT_IN_GLOBAL_IDENTIFIER,
                );
            }
        }
    }

    /// Check if a function declaration has a body (is an implementation, not just a signature).
    pub(crate) fn function_has_body(&self, decl_idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::FUNCTION_DECLARATION {
            return false;
        }
        let Some(func) = self.ctx.arena.get_function(node) else {
            return false;
        };
        func.body.is_some()
    }

    /// Get the access modifier of a declaration: 0 = public (default), 1 = private, 2 = protected.
    fn get_access_modifier(&self, decl_idx: NodeIndex) -> u8 {
        use tsz_parser::parser::syntax_kind_ext;
        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return 0;
        };
        let modifiers = match node.kind {
            syntax_kind_ext::METHOD_DECLARATION => self
                .ctx
                .arena
                .get_method_decl(node)
                .and_then(|m| m.modifiers.as_ref()),
            syntax_kind_ext::FUNCTION_DECLARATION => self
                .ctx
                .arena
                .get_function(node)
                .and_then(|f| f.modifiers.as_ref()),
            syntax_kind_ext::METHOD_SIGNATURE => self
                .ctx
                .arena
                .get_signature(node)
                .and_then(|s| s.modifiers.as_ref()),
            _ => None,
        };
        let Some(mods) = modifiers else {
            return 0;
        };
        if self
            .ctx
            .arena
            .has_modifier_ref(Some(mods), SyntaxKind::PrivateKeyword)
        {
            1
        } else if self
            .ctx
            .arena
            .has_modifier_ref(Some(mods), SyntaxKind::ProtectedKeyword)
        {
            2
        } else {
            0
        }
    }

    /// Check if a method declaration or signature is optional (has `question_token`).
    fn is_declaration_optional(&self, decl_idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };
        match node.kind {
            syntax_kind_ext::METHOD_DECLARATION => self
                .ctx
                .arena
                .get_method_decl(node)
                .is_some_and(|m| m.question_token),
            syntax_kind_ext::METHOD_SIGNATURE => self
                .ctx
                .arena
                .get_signature(node)
                .is_some_and(|s| s.question_token),
            _ => false,
        }
    }
}
