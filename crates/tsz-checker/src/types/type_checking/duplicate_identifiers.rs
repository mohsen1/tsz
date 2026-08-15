//! Duplicate identifier and declaration conflict checking.
//!
//! This module extends `CheckerState` with methods for detecting:
//! - Duplicate identifier declarations (TS2300, TS2451, TS2392, TS2393)
//! - Merged declaration diagnostics (TS2432, TS2717, TS2413)
//! - Overload signature consistency (TS2383, TS2385, TS2386)
//! - Built-in global identifier conflicts (TS2397)

#[path = "duplicate_identifiers_flag_agreement.rs"]
mod duplicate_identifiers_flag_agreement;
#[path = "duplicate_identifiers_followup.rs"]
mod duplicate_identifiers_followup;
#[path = "duplicate_identifiers_merge.rs"]
mod duplicate_identifiers_merge;
#[path = "duplicate_identifiers_preflight.rs"]
mod duplicate_identifiers_preflight;
#[path = "duplicate_identifiers_scan.rs"]
mod duplicate_identifiers_scan;

use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

pub(super) type OuterDeclResult = Option<(tsz_binder::SymbolId, Vec<(NodeIndex, u32)>)>;
type DuplicateDeclList = Vec<(NodeIndex, u32, bool, bool, DuplicateDeclarationOrigin)>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DuplicateDeclarationOrigin {
    SymbolDeclaration,
    TargetedModuleAugmentation,
    CurrentFileAugmentationTargetExport(usize),
    /// Remote declaration from a cross-file UMD global / `declare global` conflict.
    GlobalScopeConflict,
}

/// Pass-1 working set for `check_duplicate_identifiers`, produced by
/// `duplicate_identifiers_scan_symbols` and consumed by the per-symbol pass.
struct DuplicateIdentifierScanState {
    has_libs: bool,
    is_external_module: bool,
    global_scope_conflict_cache: rustc_hash::FxHashMap<String, DuplicateDeclList>,
    may_have_default_import_alias_conflicts: bool,
    pass2_symbol_ids: Vec<tsz_binder::SymbolId>,
}

impl<'a> CheckerState<'a> {
    /// TS2385 anchor: the overload's NAME token for methods, but the
    /// declaration start including modifiers for constructors
    /// (classConstructorOverloadsAccessibility anchors `private constructor`
    /// at the modifier).
    fn ts2385_anchor_span(&self, decl_idx: NodeIndex) -> Option<(u32, u32)> {
        let node = self.ctx.arena.get(decl_idx)?;
        if node.kind == tsz_parser::parser::syntax_kind_ext::CONSTRUCTOR {
            let start = self
                .ctx
                .arena
                .get_declaration_modifiers(node)
                .and_then(|mods| mods.nodes.first().copied())
                .and_then(|first_mod| self.ctx.arena.get(first_mod))
                .map_or(node.pos, |mod_node| mod_node.pos);
            return Some((start, node.end.saturating_sub(start)));
        }
        None
    }

    /// `true` when `decl_idx`'s nearest enclosing `MODULE_DECLARATION` (namespace
    /// or `module`/`declare module "x"`) is itself ambient and is not a global
    /// augmentation (`declare global { ... }`). Used to exempt overload-signature
    /// members of an ambient module/namespace body from the TS2383 export-
    /// consistency check, which tsc does not apply there.
    fn is_within_ambient_module_container(&self, decl_idx: NodeIndex) -> bool {
        let mut current = decl_idx;
        for _ in 0..100 {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            if ext.parent.is_none() {
                return false;
            }
            let parent = ext.parent;
            let Some(parent_node) = self.ctx.arena.get(parent) else {
                return false;
            };
            if parent_node.kind == syntax_kind_ext::MODULE_DECLARATION {
                return !parent_node.is_global_augmentation()
                    && self.is_ambient_declaration(parent);
            }
            current = parent;
        }
        false
    }

    /// Check for duplicate identifiers (TS2300, TS2451, TS2392).
    /// Reports when variables, functions, classes, or other declarations
    /// have conflicting names within the same scope.
    pub(crate) fn check_duplicate_identifiers(&mut self) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use rustc_hash::FxHashMap;

        let DuplicateIdentifierScanState {
            has_libs,
            is_external_module,
            mut global_scope_conflict_cache,
            may_have_default_import_alias_conflicts,
            pass2_symbol_ids,
        } = self.duplicate_identifiers_scan_symbols();

        for sym_id in pass2_symbol_ids {
            let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
                continue;
            };
            let module_augmentation_declarations = self
                .module_augmentation_conflict_declarations_for_current_file(&symbol.escaped_name);
            let script_scope_declarations = if self
                .symbol_is_current_file_top_level_script_declaration(&symbol.escaped_name, sym_id)
            {
                self.same_name_top_level_script_declarations_for_current_file(&symbol.escaped_name)
            } else {
                Vec::new()
            };
            let global_scope_declarations = if let Some(cached) =
                global_scope_conflict_cache.get(symbol.escaped_name.as_str())
            {
                cached.clone()
            } else {
                let declarations =
                    self.global_scope_conflict_declarations_for_current_file(&symbol.escaped_name);
                global_scope_conflict_cache
                    .insert(symbol.escaped_name.clone(), declarations.clone());
                declarations
            };
            let default_import_alias_conflicts = if may_have_default_import_alias_conflicts {
                self.default_import_alias_conflict_declarations_for_current_file(
                    &symbol.escaped_name,
                )
            } else {
                Vec::new()
            };
            let module_block_scoped_conflicts = self
                .module_file_block_scoped_conflict_declarations_for_current_file(
                    &symbol.escaped_name,
                    symbol.flags,
                );

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
                    && default_import_alias_conflicts.is_empty()
                    && module_block_scoped_conflicts.is_empty()
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
                        constructor.body.is_some().then_some(decl_idx)
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
            declarations.extend(default_import_alias_conflicts);
            declarations.extend(module_block_scoped_conflicts);

            if declarations.len() <= 1 {
                continue;
            }
            // TS2383 / TS2384: overload-group export/ambient flag agreement,
            // extracted to `duplicate_identifiers_flag_agreement.rs`. Returns the
            // bodyless overload signatures the TS2385/TS2386 arms below consume.
            let func_decls_for_2384 = self.check_overload_flag_agreement(&declarations);

            // TS2385: Overload signatures must all be public, private or protected
            // Applies to class method overloads with mixed access modifiers
            if func_decls_for_2384.len() >= 2 {
                let access_infos: Vec<(NodeIndex, u8)> = func_decls_for_2384
                    .iter()
                    .map(|&decl_idx| (decl_idx, self.get_access_modifier(decl_idx)))
                    .collect();
                let ref_access = access_infos[0].1;
                let has_mismatch = access_infos.iter().any(|(_, a)| *a != ref_access);
                if has_mismatch {
                    for &(decl_idx, access) in &access_infos {
                        if access != ref_access {
                            // tsc 7.0.2 anchors TS2385 at the overload's NAME
                            // token for methods, but at the declaration start
                            // (modifiers included) for constructors.
                            if let Some((start, length)) = self.ts2385_anchor_span(decl_idx) {
                                self.error(
                                    start,
                                    length,
                                    diagnostic_messages::OVERLOAD_SIGNATURES_MUST_ALL_BE_PUBLIC_PRIVATE_OR_PROTECTED.to_string(),
                                    diagnostic_codes::OVERLOAD_SIGNATURES_MUST_ALL_BE_PUBLIC_PRIVATE_OR_PROTECTED,
                                );
                            } else {
                                let anchor =
                                    self.get_declaration_name_node(decl_idx).unwrap_or(decl_idx);
                                self.error_at_node_msg(
                                    anchor,
                                    diagnostic_codes::OVERLOAD_SIGNATURES_MUST_ALL_BE_PUBLIC_PRIVATE_OR_PROTECTED,
                                    &[],
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
                let ref_optional = optional_infos[0].1;
                let has_mismatch = optional_infos.iter().any(|(_, o)| *o != ref_optional);
                if has_mismatch {
                    for &(decl_idx, optional) in &optional_infos {
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
            let all_class_decls: Vec<(NodeIndex, bool, bool)> = declarations
                .iter()
                .filter(|(_, flags, _, _, _)| (flags & symbol_flags::CLASS) != 0)
                .map(|(decl_idx, _, is_local, is_exported, _)| (*decl_idx, *is_local, *is_exported))
                .collect();
            let local_class_decls: Vec<(NodeIndex, bool)> = all_class_decls
                .iter()
                .filter(|(_, is_local, _)| *is_local)
                .map(|(decl_idx, _, is_exported)| (*decl_idx, *is_exported))
                .collect();
            if all_class_decls.len() > 1 && !local_class_decls.is_empty() {
                // Skip TS2300 when all class declarations are `export default` —
                // TS2528 ("A module cannot have multiple default exports") handles this.
                let all_default_exports = all_class_decls.iter().all(|&(decl_idx, _, _)| {
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
                let has_exported = all_class_decls.iter().any(|&(_, _, exp)| exp);
                let has_non_exported = all_class_decls.iter().any(|&(_, _, exp)| !exp);
                if has_exported && has_non_exported {
                    continue;
                }

                // Skip TS2300 when any class declaration is inside a non-exported
                // namespace body. In TSC, a non-exported `namespace Z` doesn't merge
                // with an exported Z from a dot-notation declaration like `namespace X.Y.Z`.
                // The classes inside them are separate and should not trigger TS2300.
                let any_in_non_exported_ns = all_class_decls
                    .iter()
                    .any(|&(decl_idx, _, _)| self.is_in_non_exported_namespace_body(decl_idx));
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

            // TS2652 / TS2395
            //
            // Default exports are a third visibility class, distinct from ordinary
            // exported declarations. A merged declaration whose default-exported
            // type/value/namespace space intersects any non-default declaration
            // reports TS2652. Only ordinary exported-vs-local intersections report
            // TS2395, and a declaration claimed by TS2652 must not also report
            // TS2395.
            let mut ts2652_error_nodes: Vec<NodeIndex> = Vec::new();
            let mut ts2395_error_nodes: Vec<NodeIndex> = Vec::new();
            // A symbol whose local declarations are all plain variables is
            // owned end to end by `try_emit_variable_redeclaration_family`
            // below (including its own TS2395), which models `tsc`'s
            // two-table mechanism this single-group scan cannot: a variable
            // pair can merge exported and non-exported declarations without
            // ever colliding (`export var a; var a;` is legal `var`
            // redeclaration but still reports TS2395), which this scan would
            // also catch, but not with the right footprint once a real
            // collision is also in play (see #16170).
            let is_pure_variable_family = self.declarations_are_pure_variable_family(&declarations);

            // Skip TS2395 when all local declarations are in a `declare namespace`
            // (identifier-named ambient namespace). In these contexts, the
            // distinction between exported and non-exported declarations is
            // irrelevant because ambient declarations don't produce runtime code.
            // However, `declare module "..."` (string-literal-named module
            // declarations) SHOULD still emit TS2395 — tsc treats these as
            // module scopes where export consistency matters.
            let suppress_ts2395_for_ambient = declarations
                .iter()
                .filter(|(_, _, is_local, _, _)| *is_local)
                .all(|(decl_idx, _, _, _, _)| self.is_in_ambient_namespace_not_module(*decl_idx));

            const SPACE_TYPE: u32 = 1;
            const SPACE_VALUE: u32 = 2;
            const SPACE_NAMESPACE: u32 = 4;

            let decl_info: Vec<(NodeIndex, u32, u32, bool, bool, NodeIndex)> = declarations
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
                        || (flags & (symbol_flags::REGULAR_ENUM | symbol_flags::CONST_ENUM)) != 0
                    {
                        SPACE_TYPE | SPACE_VALUE
                    } else if (flags & symbol_flags::VARIABLE) != 0
                        || (flags & symbol_flags::FUNCTION) != 0
                    {
                        SPACE_VALUE
                    } else if (flags & symbol_flags::ALIAS) != 0 {
                        // Import-equals declarations (`import Foo = X`) occupy both
                        // type and value space, so they trigger TS2395 against an
                        // exported type/value alias of the same name.
                        //
                        // Other alias forms (`import * as X`, `import { X }`,
                        // `import X from`) collide with local declarations as TS2440
                        // (Import declaration conflicts with local declaration of X)
                        // — tsc does not double-report TS2395 for them. Skip space
                        // contribution for those by limiting to ImportEqualsDeclaration.
                        if self.ctx.arena.get(decl_idx).is_some_and(|n| {
                            n.kind == tsz_parser::parser::syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                        }) {
                            SPACE_VALUE | SPACE_TYPE
                        } else {
                            0
                        }
                    } else {
                        0
                    };
                    let default_exported = exported
                        && self.declaration_participates_in_default_export_conflict(decl_idx);
                    let scope = self.get_enclosing_namespace(decl_idx);
                    (decl_idx, flags, space, exported, default_exported, scope)
                })
                .collect();

            type ScopeGroupEntry = (NodeIndex, u32, u32, bool, bool);
            // At most one distinct scope per declaration (#11617).
            let mut scope_groups: FxHashMap<NodeIndex, Vec<ScopeGroupEntry>> =
                FxHashMap::with_capacity_and_hasher(decl_info.len(), Default::default());
            for &(decl_idx, flags, space, exported, default_exported, scope) in &decl_info {
                scope_groups.entry(scope).or_default().push((
                    decl_idx,
                    flags,
                    space,
                    exported,
                    default_exported,
                ));
            }

            if !is_pure_variable_family {
                for group in scope_groups.values() {
                    if group.len() <= 1 {
                        continue;
                    }
                    let all_functions = group
                        .iter()
                        .all(|&(_, flags, _, _, _)| (flags & symbol_flags::FUNCTION) != 0);
                    let mut default_exported_spaces: u32 = 0;
                    let mut exported_spaces: u32 = 0;
                    let mut non_exported_spaces: u32 = 0;
                    for &(_, _, space, exported, default_exported) in group {
                        if default_exported {
                            default_exported_spaces |= space;
                        } else if exported {
                            exported_spaces |= space;
                        } else {
                            non_exported_spaces |= space;
                        }
                    }
                    let non_default_spaces = exported_spaces | non_exported_spaces;
                    // A group made entirely of function declarations is one
                    // overload group, not a merged declaration: visibility
                    // disagreements there are overload flag-agreement errors
                    // (TS2383, via the pass above) or duplicate-implementation
                    // errors (TS2393), never TS2652/TS2395. A single non-function
                    // member (namespace, class, variable) restores both checks
                    // for the whole group.
                    let default_conflict_spaces = if all_functions {
                        0
                    } else {
                        default_exported_spaces & non_default_spaces
                    };
                    let export_local_conflict_spaces =
                        if all_functions || suppress_ts2395_for_ambient {
                            0
                        } else {
                            exported_spaces & non_exported_spaces
                        };

                    if default_conflict_spaces == 0 && export_local_conflict_spaces == 0 {
                        continue;
                    }

                    for &(decl_idx, _, space, _, _) in group {
                        let error_node =
                            self.get_declaration_name_node(decl_idx).unwrap_or(decl_idx);
                        if (space & default_conflict_spaces) != 0 {
                            ts2652_error_nodes.push(error_node);
                        } else if (space & export_local_conflict_spaces) != 0 {
                            ts2395_error_nodes.push(error_node);
                        }
                    }
                }
            }

            let has_merge_visibility_diagnostic =
                !ts2652_error_nodes.is_empty() || !ts2395_error_nodes.is_empty();

            if !ts2652_error_nodes.is_empty() {
                let name = symbol.escaped_name.clone();
                let message = format_message(
                    diagnostic_messages::MERGED_DECLARATION_CANNOT_INCLUDE_A_DEFAULT_EXPORT_DECLARATION_CONSIDER_ADDING_A,
                    &[&name],
                );
                for error_node in ts2652_error_nodes {
                    self.error_at_node(
                        error_node,
                        &message,
                        diagnostic_codes::MERGED_DECLARATION_CANNOT_INCLUDE_A_DEFAULT_EXPORT_DECLARATION_CONSIDER_ADDING_A,
                    );
                }
            }

            if !ts2395_error_nodes.is_empty() {
                let name = symbol.escaped_name.clone();
                let message = format_message(
                    diagnostic_messages::INDIVIDUAL_DECLARATIONS_IN_MERGED_DECLARATION_MUST_BE_ALL_EXPORTED_OR_ALL_LOCAL,
                    &[&name],
                );
                for error_node in ts2395_error_nodes {
                    self.error_at_node(
                        error_node,
                        &message,
                        diagnostic_codes::INDIVIDUAL_DECLARATIONS_IN_MERGED_DECLARATION_MUST_BE_ALL_EXPORTED_OR_ALL_LOCAL,
                    );
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
                    FxHashMap::with_capacity_and_hasher(interface_decls.len(), Default::default());
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
                        !self.interface_type_parameters_are_group_merge_compatible(&decls_in_scope);
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

            // Cross-file (cross-arena) global interface merge TS2717: the
            // `interface_decls` collection above is filtered to local decls, so a
            // member-type conflict spanning files is invisible to the same-file
            // merge check. Handle it here for global-script interfaces only.
            if !is_external_module && symbol.has_any_flags(symbol_flags::INTERFACE) {
                let name = symbol.escaped_name.clone();
                self.check_cross_file_global_interface_member_conflicts(&name);
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
                let mut decls_by_scope: FxHashMap<SymbolId, Vec<NodeIndex>> =
                    FxHashMap::with_capacity_and_hasher(
                        class_interface_decls.len(),
                        Default::default(),
                    );
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
                        // Reuse the group-merge rule (it builds profiles from
                        // both class and interface nodes): every type-parameter
                        // position present on only some declarations must carry
                        // a default, and overlapping positions must agree on
                        // name/constraint/default. This rejects an arity
                        // mismatch with a non-defaulted extra (`class A<T>` +
                        // `interface A`) — tsc's `areTypeParametersIdentical`
                        // count-in-[min,max] rule — while still allowing
                        // defaulted extras (React's `class C<P, S>` +
                        // `interface C<P, S, SS = any>`). The earlier
                        // overlap-only check silently accepted the former.
                        let mismatch = !self
                            .interface_type_parameters_are_group_merge_compatible(&decls_in_scope);
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
            self.check_enum_namespace_export_collisions(&local_declarations_for_enums);

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
                    // `arena_for_declaration_or` falls back to the per-symbol
                    // `symbol_arenas` map (the last arena the binder touched
                    // for this symbol) when no precise per-declaration entry
                    // exists. That legacy map goes stale the moment a local
                    // declaration merges into a lib-derived symbol without
                    // registering its own `declaration_arenas` entry (e.g. a
                    // module-scoped `var` shadowing-disabled merge into a lib
                    // global): the lookup then silently resolves the local
                    // declaration to the lib's arena instead of the current
                    // file, corrupting `same_source_file` and the flag
                    // normalization below. `decl_is_local`/`other_is_local`
                    // are already derived from the authoritative
                    // `declaration_arenas` walk (or the "no entry means
                    // local" default) that built `declarations`, so trust
                    // them directly instead of re-deriving the arena.
                    let decl_arena = if decl_is_local {
                        self.ctx.arena
                    } else {
                        self.ctx
                            .binder
                            .arena_for_declaration_or(sym_id, decl_idx, self.ctx.arena)
                    };
                    let other_arena = if other_is_local {
                        self.ctx.arena
                    } else {
                        self.ctx
                            .binder
                            .arena_for_declaration_or(sym_id, other_idx, self.ctx.arena)
                    };
                    let decl_conflict_flags =
                        self.normalize_duplicate_conflict_flags(decl_arena, decl_idx, decl_flags);
                    let other_conflict_flags = self.normalize_duplicate_conflict_flags(
                        other_arena,
                        other_idx,
                        other_flags,
                    );
                    let same_source_file = decl_arena
                        .source_files
                        .first()
                        .zip(other_arena.source_files.first())
                        .is_some_and(|(a, b)| a.file_name == b.file_name);

                    if !decl_is_local && !other_is_local {
                        continue;
                    }

                    let decl_is_module_scoped_local = is_external_module
                        && decl_is_local
                        && self.get_enclosing_namespace(decl_idx).is_none();
                    let other_is_module_scoped_local = is_external_module
                        && other_is_local
                        && self.get_enclosing_namespace(other_idx).is_none();
                    let decl_is_remote_global_namespace_alias = !decl_is_local
                        && decl_origin == DuplicateDeclarationOrigin::GlobalScopeConflict
                        && (decl_conflict_flags & symbol_flags::ALIAS) != 0
                        && (decl_conflict_flags & symbol_flags::FUNCTION_SCOPED_VARIABLE) != 0
                        && (decl_conflict_flags & symbol_flags::BLOCK_SCOPED_VARIABLE) == 0;
                    let other_is_remote_global_namespace_alias = !other_is_local
                        && other_origin == DuplicateDeclarationOrigin::GlobalScopeConflict
                        && (other_conflict_flags & symbol_flags::ALIAS) != 0
                        && (other_conflict_flags & symbol_flags::FUNCTION_SCOPED_VARIABLE) != 0
                        && (other_conflict_flags & symbol_flags::BLOCK_SCOPED_VARIABLE) == 0;

                    let decl_is_skippable_remote = !decl_is_local
                        && decl_origin == DuplicateDeclarationOrigin::SymbolDeclaration
                        && (decl_conflict_flags & symbol_flags::ALIAS) == 0;
                    let other_is_skippable_remote = !other_is_local
                        && other_origin == DuplicateDeclarationOrigin::SymbolDeclaration
                        && (other_conflict_flags & symbol_flags::ALIAS) == 0;

                    // In external modules, top-level module-scope declarations do not
                    // participate in global namespace duplicate checking against lib
                    // declarations. This preserves TypeScript semantics where external
                    // module declarations are isolated from unrelated global symbol
                    // conflicts, but explicit module augmentations still target this
                    // file's exports and must participate in duplicate checking.
                    if is_external_module
                        && !same_source_file
                        && ((decl_is_module_scoped_local && other_is_skippable_remote)
                            || (other_is_module_scoped_local && decl_is_skippable_remote))
                    {
                        continue;
                    }
                    if is_external_module
                        && ((decl_is_module_scoped_local && other_is_remote_global_namespace_alias)
                            || (other_is_module_scoped_local
                                && decl_is_remote_global_namespace_alias))
                    {
                        continue;
                    }

                    // Targeted module augmentations allow merging: interface+interface,
                    // function+function (overloads), and import aliases (which are not local
                    // declarations — they reference the source module's export and never
                    // conflict with an augmentation of that same source module).
                    // Property-vs-method mismatches are handled by the dedicated cross-file
                    // interface-member conflict pass above.
                    if (decl_origin.is_targeted_module_augmentation()
                        || other_origin.is_targeted_module_augmentation())
                        && (((decl_flags & symbol_flags::INTERFACE) != 0
                            && (other_flags & symbol_flags::INTERFACE) != 0)
                            || ((decl_flags & symbol_flags::FUNCTION) != 0
                                && (other_flags & symbol_flags::FUNCTION) != 0)
                            || (decl_is_local && self.node_is_import_alias(decl_flags, decl_idx))
                            || (other_is_local
                                && self.node_is_import_alias(other_flags, other_idx)))
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
                        // A remote (non-local) declaration's index belongs to another
                        // file's arena, so `get_enclosing_namespace` (which always reads
                        // `self.ctx.arena`) cannot be asked about it; fall back to
                        // module-scope (the pre-existing behavior) in that case.
                        let decl_at_module_scope =
                            !decl_is_local || self.get_enclosing_namespace(decl_idx).is_none();
                        let other_at_module_scope =
                            !other_is_local || self.get_enclosing_namespace(other_idx).is_none();
                        if is_external_module
                            && decl_is_exported
                            && other_is_exported
                            && decl_at_module_scope
                            && other_at_module_scope
                        {
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
                        let decl_has_body = self.function_decl_has_body_for_duplicate_symbol(
                            sym_id,
                            decl_idx,
                            decl_is_local,
                        );
                        let other_has_body = self.function_decl_has_body_for_duplicate_symbol(
                            sym_id,
                            other_idx,
                            other_is_local,
                        );

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

                    // In tsc, `import { A } from "m"` lives in file locals and
                    // `export { A } from "m"` lives in the file's exports table —
                    // different slots, no collision. Our binder merges both into
                    // the same symbol, so suppress TS2300 only when exactly one
                    // side is a re-export specifier; two re-exports of the same
                    // exported name share the exports slot and DO collide.
                    let both_aliases = (decl_flags & symbol_flags::ALIAS) != 0
                        && (other_flags & symbol_flags::ALIAS) != 0;
                    if both_aliases {
                        let decl_is_reexport = self.is_reexport_specifier(decl_idx);
                        let other_is_reexport = self.is_reexport_specifier(other_idx);
                        if decl_is_reexport != other_is_reexport {
                            continue;
                        }
                    }

                    // Import alias referencing a remote non-alias declaration
                    // is not a conflict — suppress the false duplicate.
                    if !same_source_file {
                        let decl_is_import_alias = self.node_is_import_alias(decl_flags, decl_idx);
                        let other_is_import_alias =
                            self.node_is_import_alias(other_flags, other_idx);
                        if (decl_is_import_alias && (other_flags & symbol_flags::ALIAS) == 0)
                            || (other_is_import_alias && (decl_flags & symbol_flags::ALIAS) == 0)
                        {
                            continue;
                        }
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
                    let is_cross_file_umd_candidate = (decl_is_local != other_is_local)
                        && (decl_origin == DuplicateDeclarationOrigin::GlobalScopeConflict
                            || other_origin == DuplicateDeclarationOrigin::GlobalScopeConflict);
                    if is_cross_file_umd_candidate {
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
                        let remote_is_ns_alias = (remote_flags & symbol_flags::ALIAS) != 0
                            && (remote_flags & symbol_flags::FUNCTION_SCOPED_VARIABLE) != 0
                            && (remote_flags & symbol_flags::BLOCK_SCOPED_VARIABLE) == 0;
                        // `export as namespace X` from another file should only
                        // conflict with local block-scoped global-augmentation
                        // values (`declare global { const/let X }`). It should
                        // not collide with local imports/functions/namespace
                        // exports (for example umd-augmentation-1.ts,
                        // sourceFileMergeWithFunction.ts, umdGlobalConflict.ts).
                        if remote_is_ns_alias && !local_is_global_aug {
                            continue;
                        }
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
                            continue;
                        }
                        // Non-UMD GlobalScopeConflict pairs still need regular
                        // duplicate-identifier checks (for example JSX/runtime
                        // and synthetic default-import alias conflicts).
                    }

                    let decl_is_namespace = (decl_flags
                        & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE))
                        != 0;
                    let other_is_namespace = (other_flags
                        & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE))
                        != 0;
                    let decl_is_namespace_for_conflict = (decl_conflict_flags
                        & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE))
                        != 0;
                    let other_is_namespace_for_conflict = (other_conflict_flags
                        & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE))
                        != 0;

                    if decl_is_namespace_for_conflict && other_is_namespace_for_conflict {
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
                        if self.is_ambient_declaration(function_idx)
                            || self.is_ambient_function_declaration(function_idx)
                        {
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
                        // Use the value-resolving variant: a namespace whose only
                        // body is `export { TypeAlias }` resolves to a type-only
                        // re-export and must not conflict with a value of the
                        // same name (`declare const X` + `declare namespace X`).
                        if self.is_namespace_declaration_value_instantiated(namespace_idx) {
                            if decl_is_local {
                                conflicts.insert(decl_idx);
                            }
                            if other_is_local {
                                conflicts.insert(other_idx);
                            }
                        }
                        continue;
                    }

                    // A `var`/`let`/`const` variable does not declaration-merge
                    // with a class, in JS or TS: only a `function` declaration
                    // merges with a class's static side (already excluded above
                    // via `FUNCTION_EXCLUDES`, which omits `CLASS`). tsc always
                    // reports the redeclaration here — `TS2451` when the variable
                    // side is block-scoped, `TS2300` otherwise (verified against
                    // `typescript@7.0.2`: `declare class A {}` + `const A = {}`
                    // reports `TS2451` on both sides, regardless of `checkJs`).
                    // `Self::declarations_conflict` below already flags
                    // variable-vs-class via `BLOCK_SCOPED_VARIABLE_EXCLUDES`/
                    // `FUNCTION_SCOPED_VARIABLE_EXCLUDES` (both include `CLASS`),
                    // so no JS-specific carve-out is needed or correct here.

                    if Self::declarations_conflict(decl_conflict_flags, other_conflict_flags) {
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

            // The "default-import alias vs local namespace" TS2300 rule is a
            // JS parity quirk — tsc only emits the namespace/default-import
            // collision in checked JS files. Under TS, a default import that
            // aliases a local namespace is allowed. Keeping the check
            // unrestricted broke several TS-only tests (e.g.
            // allowImportClausesToMergeWithTypes.ts,
            // exportAssignmentWithoutAllowSyntheticDefaultImportsError.ts)
            // that define a local namespace/value-module with the same name
            // as a remote default import.
            if conflicts.is_empty() && self.is_js_file() && self.ctx.should_resolve_jsdoc() {
                let has_remote_default_import_alias_conflict =
                    declarations.iter().any(|(_, flags, is_local, _, origin)| {
                        !*is_local
                            && *origin == DuplicateDeclarationOrigin::GlobalScopeConflict
                            && (flags & symbol_flags::BLOCK_SCOPED_VARIABLE) != 0
                            && (flags & symbol_flags::ALIAS) != 0
                    });
                if has_remote_default_import_alias_conflict {
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
            }

            for idx in namespace_order_errors {
                let error_node = self.get_declaration_name_node(idx).unwrap_or(idx);
                let message = format_message(
                    diagnostic_messages::A_NAMESPACE_DECLARATION_CANNOT_BE_LOCATED_PRIOR_TO_A_CLASS_OR_FUNCTION_WITH_WHIC,
                    &[],
                );
                self.error_at_node(error_node, &message, diagnostic_codes::A_NAMESPACE_DECLARATION_CANNOT_BE_LOCATED_PRIOR_TO_A_CLASS_OR_FUNCTION_WITH_WHIC);
            }

            // A pure-variable-family symbol can need reporting (TS2395) even
            // when no pair of its declarations ever collides under this
            // scan's pairwise `conflicts` model — `export var a; var a;` is
            // legal `var` redeclaration (no entry in `conflicts`) but still
            // reports TS2395 — so it must still reach
            // `try_emit_variable_redeclaration_family` below.
            if conflicts.is_empty() && !is_pure_variable_family {
                continue;
            }

            // Multiple default exports are classified by the dedicated export pass
            // (TS2323/TS2528/TS2813/TS2814). If the generic duplicate-identifier
            // pass also reports TS2300 for the synthetic `default` symbol, we end
            // up with the wrong diagnostic family for re-exports and declaration
            // forms that tsc handles through export-default rules instead.
            if symbol.escaped_name == "default"
                && declarations
                    .iter()
                    .filter(|(decl_idx, _, is_local, _, _)| {
                        !*is_local || conflicts.contains(decl_idx)
                    })
                    .all(|(decl_idx, _, _, _, _)| {
                        self.declaration_participates_in_default_export_conflict(*decl_idx)
                    })
            {
                continue;
            }

            // TS2393: Duplicate function implementation.
            {
                let has_non_function_conflict =
                    declarations.iter().any(|(decl_idx, flags, _, _, _)| {
                        conflicts.contains(decl_idx) && (flags & symbol_flags::FUNCTION) == 0
                    });
                let has_remote_function_implementation =
                    declarations
                        .iter()
                        .any(|(decl_idx, flags, is_local, _, _)| {
                            !*is_local
                                && (flags & symbol_flags::FUNCTION) != 0
                                && self.function_decl_has_body_for_duplicate_symbol(
                                    sym_id, *decl_idx, false,
                                )
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

                // Group duplicate function implementations by block scope via the
                // fast `FxHashMap` (not the default SipHash map), pre-sized to the
                // candidate count (#11617).
                let mut scope_groups: FxHashMap<NodeIndex, Vec<NodeIndex>> =
                    FxHashMap::with_capacity_and_hasher(
                        func_impls_with_scope.len(),
                        Default::default(),
                    );
                for &(idx, scope) in &func_impls_with_scope {
                    scope_groups.entry(scope).or_default().push(idx);
                }

                let mut duplicate_impl_family_found = false;
                for group in scope_groups.values() {
                    if group.len() > 1 || has_remote_function_implementation {
                        duplicate_impl_family_found = true;
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

                // Once this symbol has a genuine duplicate-implementation
                // family, tsc reports TS2393 on every other local
                // function-family declaration too — including the bodyless
                // overload signatures that would otherwise stay clean (e.g. a
                // namespace reopened with two implementations still needs its
                // own overload signatures flagged, not just the bodies).
                if duplicate_impl_family_found {
                    for (decl_idx, flags, is_local, _, _) in declarations.iter() {
                        if *is_local
                            && (flags & symbol_flags::FUNCTION) != 0
                            && !self.function_has_body(*decl_idx)
                        {
                            let error_node = self
                                .get_declaration_name_node(*decl_idx)
                                .unwrap_or(*decl_idx);
                            self.error_at_node(
                                error_node,
                                diagnostic_messages::DUPLICATE_FUNCTION_IMPLEMENTATION,
                                diagnostic_codes::DUPLICATE_FUNCTION_IMPLEMENTATION,
                            );
                            conflicts.remove(decl_idx);
                        }
                    }
                }
                if conflicts.is_empty() && !is_pure_variable_family {
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
                            (!*is_local || conflicts.contains(decl_idx))
                                && (flags & symbol_flags::CLASS) != 0
                        });
                let has_function_partner =
                    declarations
                        .iter()
                        .any(|(decl_idx, flags, is_local, _, _)| {
                            (!*is_local || conflicts.contains(decl_idx))
                                && (flags & symbol_flags::FUNCTION) != 0
                        });
                let has_js_constructor_value_partner =
                    declarations
                        .iter()
                        .any(|(decl_idx, flags, is_local, _, _)| {
                            (!*is_local || conflicts.contains(decl_idx))
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
                                (flags & symbol_flags::CLASS) == 0
                                    || self.is_ambient_declaration(*decl_idx)
                                    || (*is_local && !conflicts.contains(decl_idx))
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
            let remote_alias_conflict =
                declarations.iter().any(|(_, flags, is_local, _, origin)| {
                    !*is_local
                        && *origin == DuplicateDeclarationOrigin::GlobalScopeConflict
                        && (flags & symbol_flags::BLOCK_SCOPED_VARIABLE) != 0
                        && (flags & symbol_flags::ALIAS) != 0
                });

            let name = symbol.escaped_name.clone();

            let has_remote_declaration =
                declarations.iter().any(|(_, _, is_local, _, _)| !*is_local);
            // Only force TS2300 for cross-file targeted module augmentation when
            // the conflict genuinely isn't a block-scoped variable redeclaration.
            // When every conflicting decl is `const`/`let`, tsc emits TS2451 even
            // across augmentation (see exportAsNamespace_augment.ts).
            let force2300 =
                remote_alias_conflict || self.targeted_aug_should_force_ts2300(&declarations);
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
            let is_import_equals_like = |decl_idx: NodeIndex| {
                self.ctx.arena.get(decl_idx).is_some_and(|decl_node| {
                    if decl_node.kind
                        == tsz_parser::parser::syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                    {
                        return true;
                    }
                    if decl_node.kind == tsz_parser::parser::syntax_kind_ext::EXPORT_DECLARATION {
                        return self
                            .ctx
                            .arena
                            .get_export_decl(decl_node)
                            .and_then(|export_decl| self.ctx.arena.get(export_decl.export_clause))
                            .is_some_and(|export_clause| {
                                export_clause.kind
                                    == tsz_parser::parser::syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                            });
                    }
                    false
                })
            };
            let has_import_equals_conflict = declarations.iter().any(|(decl_idx, _, _, _, _)| {
                conflicts.contains(decl_idx) && is_import_equals_like(*decl_idx)
            });
            let has_non_variable_conflict =
                declarations.iter().any(|(decl_idx, flags, _, _, _)| {
                    conflicts.contains(decl_idx) && (flags & symbol_flags::VARIABLE) == 0
                });
            let has_accessor_conflict = declarations.iter().any(|(decl_idx, flags, _, _, _)| {
                conflicts.contains(decl_idx)
                    && (flags & (symbol_flags::GET_ACCESSOR | symbol_flags::SET_ACCESSOR)) != 0
            });

            // Whether any declaration taking part in this conflict is block-scoped
            // (`let`/`const`). `conflicts` only tracks local declarations, so a
            // remote (cross-file) block-scoped declaration has to be looked up
            // separately. Both the TS2323 arm and the TS2451-vs-TS2300 fallback
            // below turn on this, so it is computed once here.
            let has_block_scoped_conflict =
                declarations.iter().any(|(decl_idx, flags, _, _, _)| {
                    conflicts.contains(decl_idx)
                        && (flags & symbol_flags::BLOCK_SCOPED_VARIABLE) != 0
                });
            // See duplicateIdentifierRelatedSpans1.ts: a local `class Bar`
            // conflicts with a remote `const Bar` from another file — tsc emits
            // TS2451.
            let has_remote_block_scoped_conflict =
                declarations.iter().any(|(_, flags, is_local, _, _)| {
                    !*is_local && (flags & symbol_flags::BLOCK_SCOPED_VARIABLE) != 0
                });

            // TS2323: Check exported variable conflict using symbol.is_exported.
            // A `let`/`const` carries `VARIABLE` alongside `BLOCK_SCOPED_VARIABLE`,
            // so "every conflicting declaration is a variable" does not by itself
            // mean "every conflicting declaration is a `var`". tsc only reaches
            // `Cannot_redeclare_exported_variable_0` when no block-scoped binding
            // takes part; the moment one does, the binder's own redeclaration
            // message wins and the choice is TS2451-vs-TS2300 by source order.
            let has_exported_variable_conflict = symbol.is_exported
                && has_variable_conflict
                && !has_block_scoped_conflict
                && !has_remote_block_scoped_conflict;

            // A symbol whose declarations are all plain `var`/`let`/`const`
            // goes through tsc's two independent reporting passes, which can
            // co-emit TS2323 and TS2300 on one declaration and give the two
            // codes different footprints over the group. The single-code
            // selection below cannot express either, so that family is modelled
            // directly in `duplicate_identifiers_variable_family`.
            if self.try_emit_variable_redeclaration_family(
                &declarations,
                &name,
                is_external_module,
                force2300 || has_umd_global_value_conflict,
            ) {
                continue;
            }

            let (message, code) = if !has_non_block_scoped && !force2300
                || has_umd_global_value_conflict
            {
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
                && !force2300
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
                if has_merge_visibility_diagnostic {
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
                let has_function_conflict =
                    declarations.iter().any(|(decl_idx, flags, _, _, _)| {
                        conflicts.contains(decl_idx) && (flags & symbol_flags::FUNCTION) != 0
                    });
                let use_ts2451 = if has_remote_declaration
                    && (has_block_scoped_conflict || has_remote_block_scoped_conflict)
                {
                    // Cross-file mixed conflicts generally use TS2451, except for
                    // synthetic default-import alias collisions where tsc reports
                    // TS2300 (for example impliedNodeFormatInterop1.ts).
                    !force2300
                } else if has_block_scoped_conflict && has_function_conflict {
                    // When a function declaration conflicts with a block-scoped
                    // variable (let/const) at the same scope, tsc uses TS2300.
                    false
                } else if has_block_scoped_conflict {
                    // Same-file mixed case (`var` + `let`/`const`, optionally
                    // plus other conflict kinds): tsc uses TS2451 if the first
                    // conflicting declaration by source position is block-scoped,
                    // and TS2300 if the first is non-block-scoped (`var`).
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

            // The remote-block-scoped-alias ⇄ default-export TS2300 redirect
            // is a JS parity quirk (paired with the block at ~L1342). Gate it
            // on JS-file context so TS files that legitimately export an
            // interface or namespace as default, then also import a value of
            // the same name (e.g. allowImportClausesToMergeWithTypes.ts),
            // don't produce a false TS2300 at the `default` export site.
            if code == diagnostic_codes::DUPLICATE_IDENTIFIER
                && remote_alias_conflict
                && self.is_js_file()
                && self.ctx.should_resolve_jsdoc()
                && let Some(default_export_ident) =
                    self.current_file_default_export_identifier_named(&name)
            {
                self.error_at_node(default_export_ident, &message, code);
                continue;
            }
            if code == diagnostic_codes::DUPLICATE_IDENTIFIER
                && has_remote_declaration
                && has_import_equals_conflict
            {
                continue;
            }
            if code == diagnostic_codes::DUPLICATE_IDENTIFIER
                && self.ctx.import_conflict_names.contains(&name)
            {
                continue;
            }

            self.emit_duplicate_identifier_diagnostics(
                sym_id,
                &declarations,
                &conflicts,
                code,
                &message,
            );
        }

        self.check_block_scoped_function_outer_conflicts();
        self.check_global_augmentation_const_enum_rebind_diagnostics();
        self.check_cross_file_global_augmentation_member_conflicts();
        self.check_cross_file_module_augmentation_member_conflicts();
        self.check_alias_partner_merge_export_consistency();
    }
}
