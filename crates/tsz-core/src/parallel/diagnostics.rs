//! Parallel diagnostics post-processing.
//!
//! This module contains the diagnostic collection, suppression, and
//! augmentation-conflict detection logic that runs after the parallel
//! per-file type-check phase completes.
//!
//! # Inputs
//!
//! - `MergedProgram`: the fully merged multi-file program produced by the
//!   binder/merge pipeline.
//! - `FileCheckResult` slices: the per-file diagnostic lists produced by the
//!   parallel checker.
//! - `resolved_module_paths`: the resolved import-path → file-index map built
//!   during `check_files_parallel`.
//! - `checker_lib_files`: the `LibFile` arcs needed for interface-extension
//!   analysis.
//!
//! # Outputs
//!
//! Mutates `file_results` in-place (adds, suppresses, or re-routes diagnostics)
//! and produces `BoundFile` values for lib-interface re-check passes.

use std::sync::Arc;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::binder::state::DeclarationArenaMap;
use crate::binder::{FlowNodeArena, SymbolId};
use crate::checker::diagnostics::Diagnostic;
use crate::lib_loader::LibFile;
use crate::parser::NodeIndex;
use crate::parser::node::NodeArena;
use crate::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

use super::core::{BoundFile, FileCheckResult, MergedProgram, build_sym_to_decl_indices};

fn resolve_export_in_program_file(
    program: &MergedProgram,
    resolved_module_paths: &FxHashMap<(usize, String), usize>,
    file_idx: usize,
    export_name: &str,
    visited: &mut FxHashSet<usize>,
) -> Option<(SymbolId, usize)> {
    if !visited.insert(file_idx) {
        return None;
    }

    let file_name = program.files.get(file_idx)?.file_name.as_str();

    if let Some(exports) = program.module_exports.get(file_name)
        && let Some(sym_id) = exports.get(export_name)
    {
        return Some((sym_id, file_idx));
    }

    if let Some(reexports) = program.reexports.get(file_name)
        && let Some((source_module, original_name)) = reexports.get(export_name)
    {
        let name = original_name.as_deref().unwrap_or(export_name);
        if let Some(&source_idx) = resolved_module_paths.get(&(file_idx, source_module.clone()))
            && let Some(result) = resolve_export_in_program_file(
                program,
                resolved_module_paths,
                source_idx,
                name,
                visited,
            )
        {
            return Some(result);
        }
    }

    if let Some(source_modules) = program.wildcard_reexports.get(file_name) {
        for source_module in source_modules {
            if let Some(&source_idx) = resolved_module_paths.get(&(file_idx, source_module.clone()))
                && let Some(result) = resolve_export_in_program_file(
                    program,
                    resolved_module_paths,
                    source_idx,
                    export_name,
                    visited,
                )
            {
                return Some(result);
            }
        }
    }

    None
}

fn declaration_name_span_for_ts2567(arena: &NodeArena, decl_idx: NodeIndex) -> Option<(u32, u32)> {
    let node = arena.get(decl_idx)?;
    let name_idx = if node.kind == syntax_kind_ext::CLASS_DECLARATION {
        arena.get_class(node).map(|class| class.name)
    } else if node.kind == syntax_kind_ext::INTERFACE_DECLARATION {
        arena.get_interface(node).map(|interface| interface.name)
    } else if node.kind == syntax_kind_ext::FUNCTION_DECLARATION {
        arena.get_function(node).map(|function| function.name)
    } else {
        None
    }?;
    let name_node = arena.get(name_idx)?;
    Some((name_node.pos, name_node.end - name_node.pos))
}

/// Collect diagnostics for enum declarations in module augmentations that
/// conflict with non-enum/non-namespace exports in the augmented module.
///
/// When an augmentation file re-exports a module and adds an `enum` that
/// collides with an existing non-enum symbol, TypeScript emits TS2567.
pub fn collect_reexported_module_augmentation_enum_conflict_diagnostics(
    program: &MergedProgram,
    resolved_module_paths: &FxHashMap<(usize, String), usize>,
) -> Vec<Diagnostic> {
    let _span =
        tracing::debug_span!("parallel_diagnostics::collect_reexported_enum_conflicts").entered();

    use crate::checker::diagnostics::{diagnostic_codes, diagnostic_messages};
    use tsz_binder::symbol_flags;

    let mut diagnostics = Vec::new();

    for (augment_file_idx, file) in program.files.iter().enumerate() {
        for (module_specifier, augmentations) in file.module_augmentations.iter() {
            let Some(&target_file_idx) =
                resolved_module_paths.get(&(augment_file_idx, module_specifier.clone()))
            else {
                continue;
            };

            for augmentation in augmentations {
                let arena = augmentation.arena.as_deref().unwrap_or(file.arena.as_ref());
                let Some(enum_node) = arena.get(augmentation.node) else {
                    continue;
                };
                if enum_node.kind != syntax_kind_ext::ENUM_DECLARATION {
                    continue;
                }
                let Some(enum_decl) = arena.get_enum(enum_node) else {
                    continue;
                };

                let Some((existing_sym_id, owner_idx)) = resolve_export_in_program_file(
                    program,
                    resolved_module_paths,
                    target_file_idx,
                    augmentation.name.as_str(),
                    &mut FxHashSet::default(),
                ) else {
                    continue;
                };
                let Some(existing_symbol) = program.symbols.get(existing_sym_id) else {
                    continue;
                };
                let allowed = (existing_symbol.flags
                    & (symbol_flags::REGULAR_ENUM
                        | symbol_flags::CONST_ENUM
                        | symbol_flags::MODULE))
                    != 0;
                if allowed {
                    continue;
                }

                let Some(enum_name_node) = arena.get(enum_decl.name) else {
                    continue;
                };
                let message = diagnostic_messages::ENUM_DECLARATIONS_CAN_ONLY_MERGE_WITH_NAMESPACE_OR_OTHER_ENUM_DECLARATIONS.to_string();
                diagnostics.push(Diagnostic::error(
                    file.file_name.clone(),
                    enum_name_node.pos,
                    enum_name_node.end - enum_name_node.pos,
                    message.clone(),
                    diagnostic_codes::ENUM_DECLARATIONS_CAN_ONLY_MERGE_WITH_NAMESPACE_OR_OTHER_ENUM_DECLARATIONS,
                ));

                let decl_file_idx = if existing_symbol.decl_file_idx != u32::MAX {
                    existing_symbol.decl_file_idx as usize
                } else {
                    owner_idx
                };
                let Some(decl_file) = program.files.get(decl_file_idx) else {
                    continue;
                };
                let Some((pos, len)) = existing_symbol.declarations.iter().find_map(|&decl_idx| {
                    declaration_name_span_for_ts2567(&decl_file.arena, decl_idx)
                }) else {
                    continue;
                };
                diagnostics.push(Diagnostic::error(
                    decl_file.file_name.clone(),
                    pos,
                    len,
                    message,
                    diagnostic_codes::ENUM_DECLARATIONS_CAN_ONLY_MERGE_WITH_NAMESPACE_OR_OTHER_ENUM_DECLARATIONS,
                ));
            }
        }
    }

    diagnostics
}

/// Re-route and deduplicate TS2567 diagnostics across file results,
/// then append any newly detected conflicts from
/// `collect_reexported_module_augmentation_enum_conflict_diagnostics`.
pub(crate) fn add_reexported_module_augmentation_enum_conflict_diagnostics(
    program: &MergedProgram,
    resolved_module_paths: &FxHashMap<(usize, String), usize>,
    file_results: &mut [FileCheckResult],
) {
    use crate::checker::diagnostics::diagnostic_codes;

    let file_result_by_name: FxHashMap<String, usize> = file_results
        .iter()
        .enumerate()
        .map(|(idx, result)| (result.file_name.clone(), idx))
        .collect();

    let mut rerouted = Vec::new();
    for result in file_results.iter_mut() {
        let current_file = result.file_name.clone();
        result.diagnostics.retain(|diag| {
            if diag.code
                == diagnostic_codes::ENUM_DECLARATIONS_CAN_ONLY_MERGE_WITH_NAMESPACE_OR_OTHER_ENUM_DECLARATIONS
                && diag.file != current_file
                && let Some(&target_idx) = file_result_by_name.get(&diag.file)
            {
                rerouted.push((target_idx, diag.clone()));
                return false;
            }
            true
        });
    }
    for (target_idx, diag) in rerouted {
        file_results[target_idx].diagnostics.push(diag);
    }

    let mut seen: FxHashSet<(usize, u32, u32)> = file_results
        .iter()
        .flat_map(|result| {
            result
                .diagnostics
                .iter()
                .map(|diag| (result.file_idx, diag.start, diag.code))
        })
        .collect();

    for diag in collect_reexported_module_augmentation_enum_conflict_diagnostics(
        program,
        resolved_module_paths,
    ) {
        if let Some(&result_idx) = file_result_by_name.get(&diag.file) {
            let key = (
                result_idx,
                diag.start,
                diagnostic_codes::ENUM_DECLARATIONS_CAN_ONLY_MERGE_WITH_NAMESPACE_OR_OTHER_ENUM_DECLARATIONS,
            );
            if seen.insert(key) {
                file_results[result_idx].diagnostics.push(diag);
            }
        }
    }

    for result in file_results {
        result
            .diagnostics
            .sort_by(|a, b| a.start.cmp(&b.start).then_with(|| a.code.cmp(&b.code)));
    }
}

const PARALLEL_INTERFACE_MEMBER_KIND_PROPERTY: u8 = 1;
const PARALLEL_INTERFACE_MEMBER_KIND_METHOD: u8 = 2;

#[derive(Clone)]
struct ParallelGlobalAugmentationMember {
    file_idx: usize,
    name: String,
    name_node: NodeIndex,
    kind: u8,
}

/// Detect cross-file global-augmentation member conflicts (a property in one
/// file vs. a method of the same name in another) and emit TS2300
/// (`DUPLICATE_IDENTIFIER`) diagnostics.
pub(crate) fn add_parallel_global_augmentation_member_conflict_diagnostics(
    program: &MergedProgram,
    file_results: &mut [FileCheckResult],
) {
    let _span =
        tracing::debug_span!("parallel_diagnostics::add_global_augmentation_member_conflicts")
            .entered();

    use crate::checker::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

    let mut members_by_interface: FxHashMap<String, Vec<ParallelGlobalAugmentationMember>> =
        FxHashMap::default();

    for (file_idx, file) in program.files.iter().enumerate() {
        for (interface_name, augmentations) in file.global_augmentations.iter() {
            let members = members_by_interface
                .entry(interface_name.clone())
                .or_default();
            for augmentation in augmentations {
                let arena = augmentation
                    .arena
                    .as_deref()
                    .unwrap_or_else(|| file.arena.as_ref());
                let Some(node) = arena.get(augmentation.node) else {
                    continue;
                };
                let Some(interface) = arena.get_interface(node) else {
                    continue;
                };

                for &member_idx in &interface.members.nodes {
                    let Some(member_node) = arena.get(member_idx) else {
                        continue;
                    };
                    let kind = match member_node.kind {
                        syntax_kind_ext::PROPERTY_SIGNATURE => {
                            PARALLEL_INTERFACE_MEMBER_KIND_PROPERTY
                        }
                        syntax_kind_ext::METHOD_SIGNATURE => PARALLEL_INTERFACE_MEMBER_KIND_METHOD,
                        _ => continue,
                    };
                    let Some(signature) = arena.get_signature(member_node) else {
                        continue;
                    };
                    let Some(name) =
                        parallel_global_augmentation_member_name(arena, signature.name)
                    else {
                        continue;
                    };
                    members.push(ParallelGlobalAugmentationMember {
                        file_idx,
                        name,
                        name_node: signature.name,
                        kind,
                    });
                }
            }
        }
    }

    let mut seen = FxHashSet::default();
    for members in members_by_interface.values() {
        // Pre-compute cross-file presence: (name, kind) -> (first_file_idx, has_other_file).
        // The conflict check only needs to know whether any *other* file declares the same
        // (name, opposite_kind) — a (usize, bool) per entry is sufficient, no heap set needed.
        let mut name_kind_reach: FxHashMap<(&str, u8), (usize, bool)> = FxHashMap::default();
        for m in members {
            name_kind_reach
                .entry((m.name.as_str(), m.kind))
                .and_modify(|(first, has_other)| {
                    if *first != m.file_idx {
                        *has_other = true;
                    }
                })
                .or_insert((m.file_idx, false));
        }

        for member in members {
            let opposite_kind = if member.kind == PARALLEL_INTERFACE_MEMBER_KIND_METHOD {
                PARALLEL_INTERFACE_MEMBER_KIND_PROPERTY
            } else {
                PARALLEL_INTERFACE_MEMBER_KIND_METHOD
            };
            let has_remote_conflict = name_kind_reach
                .get(&(member.name.as_str(), opposite_kind))
                .is_some_and(|&(first_fid, has_other)| has_other || first_fid != member.file_idx);
            if !has_remote_conflict {
                continue;
            }

            let Some(file_result) = file_results.get_mut(member.file_idx) else {
                continue;
            };
            let Some(file) = program.files.get(member.file_idx) else {
                continue;
            };
            let Some(name_node) = file.arena.get(member.name_node) else {
                continue;
            };
            let key = (
                member.file_idx,
                name_node.pos,
                diagnostic_codes::DUPLICATE_IDENTIFIER,
            );
            if !seen.insert(key) {
                continue;
            }

            let message =
                format_message(diagnostic_messages::DUPLICATE_IDENTIFIER, &[&member.name]);
            file_result.diagnostics.push(Diagnostic::error(
                file.file_name.clone(),
                name_node.pos,
                name_node.end.saturating_sub(name_node.pos),
                message,
                diagnostic_codes::DUPLICATE_IDENTIFIER,
            ));
        }
    }

    for result in file_results {
        result
            .diagnostics
            .sort_by(|a, b| a.start.cmp(&b.start).then_with(|| a.code.cmp(&b.code)));
        result
            .diagnostics
            .dedup_by(|a, b| a.start == b.start && a.code == b.code);
    }
}

fn parallel_global_augmentation_member_name(
    arena: &NodeArena,
    name_idx: NodeIndex,
) -> Option<String> {
    let node = arena.get(name_idx)?;
    if node.kind == SyntaxKind::Identifier as u16 {
        return arena
            .get_identifier(node)
            .map(|ident| ident.escaped_text.clone());
    }
    arena.get_literal(node).map(|literal| literal.text.clone())
}

/// Suppress `TS2709` (`CANNOT_USE_NAMESPACE_AS_A_TYPE`) diagnostics in files
/// where the name is a named import of a symbol that is re-exported from a
/// module which also has a companion type declaration under the same name.
pub(crate) fn suppress_parallel_import_shadowing_namespace_type_diagnostics(
    program: &MergedProgram,
    resolved_module_paths: &FxHashMap<(usize, String), usize>,
    file_results: &mut [FileCheckResult],
) {
    use crate::checker::diagnostics::diagnostic_codes;

    for result in file_results {
        let Some(file) = program.files.get(result.file_idx) else {
            continue;
        };
        let arena = file.arena.as_ref();
        result.diagnostics.retain(|diagnostic| {
            if diagnostic.code != diagnostic_codes::CANNOT_USE_NAMESPACE_AS_A_TYPE {
                return true;
            }
            let Some(name) = source_text_at_span(arena, diagnostic.start, diagnostic.length) else {
                return true;
            };
            !named_import_targets_shadowed_namespace_type(
                program,
                resolved_module_paths,
                result.file_idx,
                name,
            )
        });
    }
}

fn source_text_at_span(arena: &NodeArena, start: u32, length: u32) -> Option<&str> {
    let source = arena.source_files.first()?.text.as_ref();
    let start = usize::try_from(start).ok()?;
    let end = start.checked_add(usize::try_from(length).ok()?)?;
    source.get(start..end)
}

fn named_import_targets_shadowed_namespace_type(
    program: &MergedProgram,
    resolved_module_paths: &FxHashMap<(usize, String), usize>,
    file_idx: usize,
    local_name: &str,
) -> bool {
    let Some(file) = program.files.get(file_idx) else {
        return false;
    };
    let arena = file.arena.as_ref();
    let Some(source_file) = arena.source_files.first() else {
        return false;
    };

    for &stmt_idx in &source_file.statements.nodes {
        let Some(stmt_node) = arena.get(stmt_idx) else {
            continue;
        };
        if stmt_node.kind != syntax_kind_ext::IMPORT_DECLARATION {
            continue;
        }
        let Some(import_decl) = arena.get_import_decl(stmt_node) else {
            continue;
        };
        let Some(module_name) = arena
            .get(import_decl.module_specifier)
            .and_then(|node| arena.get_literal(node))
            .map(|literal| literal.text.as_str())
        else {
            continue;
        };
        let Some(clause_node) = arena.get(import_decl.import_clause) else {
            continue;
        };
        let Some(clause) = arena.get_import_clause(clause_node) else {
            continue;
        };
        let Some(bindings_node) = arena.get(clause.named_bindings) else {
            continue;
        };
        if bindings_node.kind != syntax_kind_ext::NAMED_IMPORTS {
            continue;
        }
        let Some(named) = arena.get_named_imports(bindings_node) else {
            continue;
        };
        for &spec_idx in &named.elements.nodes {
            let Some(spec_node) = arena.get(spec_idx) else {
                continue;
            };
            let Some(spec) = arena.get_specifier(spec_node) else {
                continue;
            };
            let local_name_idx = if spec.name.is_some() {
                spec.name
            } else {
                spec.property_name
            };
            let Some(local_ident) = arena.get_identifier_at(local_name_idx) else {
                continue;
            };
            if local_ident.escaped_text != local_name {
                continue;
            }
            let imported_name_idx = if spec.property_name.is_some() {
                spec.property_name
            } else {
                local_name_idx
            };
            let Some(imported_ident) = arena.get_identifier_at(imported_name_idx) else {
                continue;
            };
            let Some(&target_file_idx) =
                resolved_module_paths.get(&(file_idx, module_name.to_string()))
            else {
                continue;
            };
            if exported_namespace_import_has_type_companion(
                program,
                target_file_idx,
                &imported_ident.escaped_text,
            ) {
                return true;
            }
        }
    }

    false
}

fn exported_namespace_import_has_type_companion(
    program: &MergedProgram,
    file_idx: usize,
    export_name: &str,
) -> bool {
    let Some(file) = program.files.get(file_idx) else {
        return false;
    };
    if !program
        .module_exports
        .get(file.file_name.as_str())
        .is_some_and(|exports| exports.has(export_name))
    {
        return false;
    }
    file_has_namespace_import_named(file.arena.as_ref(), export_name)
        && file_has_type_declaration_named(file.arena.as_ref(), export_name)
}

fn file_has_namespace_import_named(arena: &NodeArena, name: &str) -> bool {
    let Some(source_file) = arena.source_files.first() else {
        return false;
    };
    source_file
        .statements
        .nodes
        .iter()
        .copied()
        .any(|stmt_idx| {
            let Some(stmt_node) = arena.get(stmt_idx) else {
                return false;
            };
            if stmt_node.kind != syntax_kind_ext::IMPORT_DECLARATION {
                return false;
            }
            let Some(import_decl) = arena.get_import_decl(stmt_node) else {
                return false;
            };
            let Some(clause_node) = arena.get(import_decl.import_clause) else {
                return false;
            };
            let Some(clause) = arena.get_import_clause(clause_node) else {
                return false;
            };
            let Some(bindings_node) = arena.get(clause.named_bindings) else {
                return false;
            };
            if bindings_node.kind != syntax_kind_ext::NAMESPACE_IMPORT {
                return false;
            }
            arena
                .get_named_imports(bindings_node)
                .and_then(|namespace_import| arena.get_identifier_at(namespace_import.name))
                .is_some_and(|ident| ident.escaped_text == name)
        })
}

fn file_has_type_declaration_named(arena: &NodeArena, name: &str) -> bool {
    let Some(source_file) = arena.source_files.first() else {
        return false;
    };
    source_file
        .statements
        .nodes
        .iter()
        .copied()
        .any(|stmt_idx| {
            let Some(stmt_node) = arena.get(stmt_idx) else {
                return false;
            };
            let name_idx = match stmt_node.kind {
                kind if kind == syntax_kind_ext::INTERFACE_DECLARATION => arena
                    .get_interface(stmt_node)
                    .map(|interface| interface.name),
                kind if kind == syntax_kind_ext::CLASS_DECLARATION => {
                    arena.get_class(stmt_node).map(|class| class.name)
                }
                kind if kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                    arena.get_type_alias(stmt_node).map(|alias| alias.name)
                }
                kind if kind == syntax_kind_ext::ENUM_DECLARATION => {
                    arena.get_enum(stmt_node).map(|enum_decl| enum_decl.name)
                }
                _ => None,
            };
            name_idx
                .and_then(|idx| arena.get_identifier_at(idx))
                .is_some_and(|ident| ident.escaped_text == name)
        })
}

fn collect_lib_interface_node_symbols(
    arena: &NodeArena,
    statements: &[NodeIndex],
    globals: &crate::binder::SymbolTable,
    fallback_node_symbols: &FxHashMap<u32, SymbolId>,
    affected_interfaces: &FxHashSet<String>,
    node_symbols: &mut FxHashMap<u32, SymbolId>,
) {
    for &stmt_idx in statements {
        let Some(stmt_node) = arena.get(stmt_idx) else {
            continue;
        };

        if stmt_node.kind == syntax_kind_ext::INTERFACE_DECLARATION {
            if let Some(interface) = arena.get_interface(stmt_node)
                && let Some(name) = arena.get_identifier_at(interface.name)
                && affected_interfaces.contains(&name.escaped_text)
                && let Some(sym_id) = globals
                    .get(&name.escaped_text)
                    .or_else(|| fallback_node_symbols.get(&stmt_idx.0).copied())
            {
                node_symbols.insert(stmt_idx.0, sym_id);
                node_symbols.insert(interface.name.0, sym_id);
                if let Some(heritage_clauses) = &interface.heritage_clauses {
                    for &clause_idx in &heritage_clauses.nodes {
                        let Some(clause_node) = arena.get(clause_idx) else {
                            continue;
                        };
                        let Some(heritage) = arena.get_heritage_clause(clause_node) else {
                            continue;
                        };
                        if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                            continue;
                        }
                        for &type_idx in &heritage.types.nodes {
                            let Some(type_node) = arena.get(type_idx) else {
                                continue;
                            };
                            let expr_idx =
                                if let Some(expr_type_args) = arena.get_expr_type_args(type_node) {
                                    expr_type_args.expression
                                } else if type_node.kind == syntax_kind_ext::TYPE_REFERENCE {
                                    arena
                                        .get_type_ref(type_node)
                                        .map_or(type_idx, |type_ref| type_ref.type_name)
                                } else {
                                    type_idx
                                };
                            if let Some(base_name) = entity_name_text_in_arena(arena, expr_idx)
                                && let Some(base_sym_id) = globals
                                    .get(&base_name)
                                    .or_else(|| fallback_node_symbols.get(&expr_idx.0).copied())
                            {
                                node_symbols.insert(expr_idx.0, base_sym_id);
                            }
                        }
                    }
                }
            }
            continue;
        }

        if stmt_node.kind != syntax_kind_ext::MODULE_DECLARATION {
            continue;
        }

        let Some(module_decl) = arena.get_module(stmt_node) else {
            continue;
        };
        if module_decl.body.is_none() {
            continue;
        }
        let Some(body_node) = arena.get(module_decl.body) else {
            continue;
        };
        if body_node.kind != syntax_kind_ext::MODULE_BLOCK {
            continue;
        }
        let Some(block) = arena.get_module_block(body_node) else {
            continue;
        };
        let Some(inner) = &block.statements else {
            continue;
        };
        collect_lib_interface_node_symbols(
            arena,
            &inner.nodes,
            globals,
            fallback_node_symbols,
            affected_interfaces,
            node_symbols,
        );
    }
}

fn interface_name_text(arena: &NodeArena, stmt_idx: NodeIndex) -> Option<String> {
    let node = arena.get(stmt_idx)?;
    let interface = arena.get_interface(node)?;
    let ident = arena.get_identifier_at(interface.name)?;
    Some(ident.escaped_text.clone())
}

fn entity_name_text_in_arena(arena: &NodeArena, idx: NodeIndex) -> Option<String> {
    let node = arena.get(idx)?;
    if node.kind == syntax_kind_ext::TYPE_REFERENCE
        && let Some(type_ref) = arena.get_type_ref(node)
    {
        return entity_name_text_in_arena(arena, type_ref.type_name);
    }
    if node.kind == SyntaxKind::Identifier as u16 {
        return arena
            .get_identifier(node)
            .map(|ident| ident.escaped_text.clone());
    }
    if node.kind == syntax_kind_ext::QUALIFIED_NAME {
        let qn = arena.get_qualified_name(node)?;
        let left = entity_name_text_in_arena(arena, qn.left)?;
        let right = entity_name_text_in_arena(arena, qn.right)?;
        return Some(format!("{left}.{right}"));
    }
    if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
        && let Some(access) = arena.get_access_expr(node)
    {
        let left = entity_name_text_in_arena(arena, access.expression)?;
        let right = arena
            .get(access.name_or_argument)
            .and_then(|right_node| arena.get_identifier(right_node))?;
        return Some(format!("{left}.{}", right.escaped_text));
    }
    None
}

fn collect_direct_base_names(
    arena: &NodeArena,
    interface: &crate::parser::node::InterfaceData,
) -> Vec<String> {
    let Some(heritage_clauses) = &interface.heritage_clauses else {
        return Vec::new();
    };

    let mut names = Vec::new();
    for &clause_idx in &heritage_clauses.nodes {
        let Some(clause_node) = arena.get(clause_idx) else {
            continue;
        };
        let Some(heritage) = arena.get_heritage_clause(clause_node) else {
            continue;
        };
        if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
            continue;
        }
        for &type_idx in &heritage.types.nodes {
            let Some(type_node) = arena.get(type_idx) else {
                continue;
            };
            let expr_idx = if let Some(expr_type_args) = arena.get_expr_type_args(type_node) {
                expr_type_args.expression
            } else if type_node.kind == syntax_kind_ext::TYPE_REFERENCE {
                arena
                    .get_type_ref(type_node)
                    .map_or(type_idx, |type_ref| type_ref.type_name)
            } else {
                type_idx
            };
            if let Some(name) = entity_name_text_in_arena(arena, expr_idx) {
                names.push(name);
            }
        }
    }
    names
}

fn interface_declaration_has_merge_surface(arena: &NodeArena, stmt_idx: NodeIndex) -> bool {
    let Some(node) = arena.get(stmt_idx) else {
        return false;
    };
    let Some(interface) = arena.get_interface(node) else {
        return false;
    };

    !interface.members.nodes.is_empty()
        || interface
            .type_parameters
            .as_ref()
            .is_some_and(|type_params| !type_params.nodes.is_empty())
        || interface
            .heritage_clauses
            .as_ref()
            .is_some_and(|heritage| !heritage.nodes.is_empty())
}

fn collect_user_global_interface_seeds(program: &MergedProgram) -> FxHashSet<String> {
    let mut seeds = FxHashSet::default();

    for file in &program.files {
        if !file.is_external_module
            && let Some(source_file) = file.arena.get_source_file_at(file.source_file)
        {
            for &stmt_idx in &source_file.statements.nodes {
                if interface_declaration_has_merge_surface(file.arena.as_ref(), stmt_idx)
                    && let Some(name) = interface_name_text(file.arena.as_ref(), stmt_idx)
                {
                    seeds.insert(name);
                }
            }
        }

        for (name, augmentations) in file.global_augmentations.iter() {
            let affects_interface = augmentations.iter().any(|augmentation| {
                if (augmentation.flags & crate::binder::symbol_flags::INTERFACE) == 0 {
                    return true;
                }
                let arena = augmentation
                    .arena
                    .as_deref()
                    .unwrap_or_else(|| file.arena.as_ref());
                interface_declaration_has_merge_surface(arena, augmentation.node)
            });
            if affects_interface {
                seeds.insert(name.clone());
            }
        }
    }

    seeds
}

fn member_name_text(arena: &NodeArena, member_idx: NodeIndex) -> Option<String> {
    let member_node = arena.get(member_idx)?;
    if let Some(sig) = arena.get_signature(member_node) {
        return arena
            .get(sig.name)
            .and_then(|name_node| arena.get_identifier(name_node))
            .map(|ident| ident.escaped_text.clone());
    }
    if let Some(accessor) = arena.get_accessor(member_node) {
        return arena
            .get(accessor.name)
            .and_then(|name_node| arena.get_identifier(name_node))
            .map(|ident| ident.escaped_text.clone());
    }
    None
}

fn collect_user_global_interface_member_names(program: &MergedProgram) -> FxHashSet<String> {
    let mut member_names = FxHashSet::default();

    for file in &program.files {
        if file.is_external_module {
            continue;
        }
        let Some(source_file) = file.arena.get_source_file_at(file.source_file) else {
            continue;
        };
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = file.arena.get(stmt_idx) else {
                continue;
            };
            let Some(interface) = file.arena.get_interface(stmt_node) else {
                continue;
            };
            for &member_idx in &interface.members.nodes {
                if let Some(name) = member_name_text(file.arena.as_ref(), member_idx) {
                    member_names.insert(name);
                }
            }
        }
    }

    member_names
}

fn add_user_global_interface_declaration_arenas(
    program: &MergedProgram,
    declaration_arenas: &mut DeclarationArenaMap,
) {
    for file in &program.files {
        if file.is_external_module {
            continue;
        }
        let Some(source_file) = file.arena.get_source_file_at(file.source_file) else {
            continue;
        };
        for &stmt_idx in &source_file.statements.nodes {
            let Some(name) = interface_name_text(file.arena.as_ref(), stmt_idx) else {
                continue;
            };
            let Some(sym_id) = program.globals.get(&name) else {
                continue;
            };
            let target = declaration_arenas.entry((sym_id, stmt_idx)).or_default();
            if !target.iter().any(|arena| Arc::ptr_eq(arena, &file.arena)) {
                target.push(Arc::clone(&file.arena));
            }
        }
    }
}

fn type_node_contains_tag_name_map_indexed_access(
    arena: &NodeArena,
    type_idx: NodeIndex,
    fuel: &mut u32,
) -> bool {
    if type_idx == NodeIndex::NONE || *fuel == 0 {
        return false;
    }
    *fuel -= 1;

    let Some(node) = arena.get(type_idx) else {
        return false;
    };
    if node.kind == syntax_kind_ext::INDEXED_ACCESS_TYPE {
        return arena
            .get_indexed_access_type(node)
            .and_then(|indexed| entity_name_text_in_arena(arena, indexed.object_type))
            .is_some_and(|name| name.contains("TagNameMap"));
    }

    if let Some(type_ref) = arena.get_type_ref(node) {
        return type_ref.type_arguments.as_ref().is_some_and(|args| {
            args.nodes
                .iter()
                .any(|&arg| type_node_contains_tag_name_map_indexed_access(arena, arg, fuel))
        });
    }
    if let Some(composite) = arena.get_composite_type(node) {
        return composite
            .types
            .nodes
            .iter()
            .any(|&ty| type_node_contains_tag_name_map_indexed_access(arena, ty, fuel));
    }
    if let Some(array) = arena.get_array_type(node) {
        return type_node_contains_tag_name_map_indexed_access(arena, array.element_type, fuel);
    }
    if let Some(wrapped) = arena.get_wrapped_type(node) {
        return type_node_contains_tag_name_map_indexed_access(arena, wrapped.type_node, fuel);
    }
    if let Some(type_operator) = arena.get_type_operator(node) {
        return type_node_contains_tag_name_map_indexed_access(
            arena,
            type_operator.type_node,
            fuel,
        );
    }
    if let Some(function_type) = arena.get_function_type(node) {
        if type_node_contains_tag_name_map_indexed_access(
            arena,
            function_type.type_annotation,
            fuel,
        ) {
            return true;
        }
        for &param_idx in &function_type.parameters.nodes {
            let Some(param_node) = arena.get(param_idx) else {
                continue;
            };
            let Some(param) = arena.get_parameter(param_node) else {
                continue;
            };
            if type_node_contains_tag_name_map_indexed_access(arena, param.type_annotation, fuel) {
                return true;
            }
        }
    }
    if let Some(conditional) = arena.get_conditional_type(node) {
        return [
            conditional.check_type,
            conditional.extends_type,
            conditional.true_type,
            conditional.false_type,
        ]
        .into_iter()
        .any(|ty| type_node_contains_tag_name_map_indexed_access(arena, ty, fuel));
    }

    false
}

fn interface_declares_member_named(
    arena: &NodeArena,
    interface: &crate::parser::node::InterfaceData,
    member_names: &FxHashSet<String>,
) -> bool {
    !member_names.is_empty()
        && interface.members.nodes.iter().any(|&member_idx| {
            member_name_text(arena, member_idx).is_some_and(|name| member_names.contains(&name))
        })
}

fn interface_has_indexed_access_member_type(
    arena: &NodeArena,
    interface: &crate::parser::node::InterfaceData,
) -> bool {
    for &member_idx in &interface.members.nodes {
        let Some(member_node) = arena.get(member_idx) else {
            continue;
        };
        if let Some(sig) = arena.get_signature(member_node) {
            let mut fuel = 256;
            if type_node_contains_tag_name_map_indexed_access(arena, sig.type_annotation, &mut fuel)
            {
                return true;
            }
            for &param_idx in sig.parameters.as_ref().map_or(&[][..], |p| &p.nodes) {
                let Some(param_node) = arena.get(param_idx) else {
                    continue;
                };
                let Some(param) = arena.get_parameter(param_node) else {
                    continue;
                };
                let mut fuel = 256;
                if type_node_contains_tag_name_map_indexed_access(
                    arena,
                    param.type_annotation,
                    &mut fuel,
                ) {
                    return true;
                }
            }
        }
        if let Some(accessor) = arena.get_accessor(member_node) {
            let mut fuel = 256;
            if type_node_contains_tag_name_map_indexed_access(
                arena,
                accessor.type_annotation,
                &mut fuel,
            ) {
                return true;
            }
        }
    }

    false
}

/// Compute the set of lib interface names that are transitively affected by
/// user-defined global interface declarations (merges and augmentations).
pub(crate) fn affected_lib_interface_names(
    program: &MergedProgram,
    checker_lib_files: &[Arc<LibFile>],
) -> FxHashSet<String> {
    let _span =
        tracing::debug_span!("parallel_diagnostics::affected_lib_interface_names").entered();

    let seed_interfaces = collect_user_global_interface_seeds(program);
    let mut affected = seed_interfaces.clone();
    let user_member_names = collect_user_global_interface_member_names(program);
    let mut inheritance_graph: FxHashMap<String, FxHashSet<String>> = FxHashMap::default();

    for lib in checker_lib_files {
        let Some(source_file) = lib.arena.get_source_file_at(lib.root_index) else {
            continue;
        };
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = lib.arena.get(stmt_idx) else {
                continue;
            };
            let Some(interface) = lib.arena.get_interface(stmt_node) else {
                continue;
            };
            let Some(name) = interface_name_text(lib.arena.as_ref(), stmt_idx) else {
                continue;
            };
            let bases = collect_direct_base_names(lib.arena.as_ref(), interface);
            inheritance_graph.entry(name).or_default().extend(bases);
        }
    }

    let mut changed = true;
    while changed {
        changed = false;
        for (name, bases) in &inheritance_graph {
            if affected.contains(name) {
                continue;
            }
            if bases.iter().any(|base| affected.contains(base)) {
                changed = affected.insert(name.clone());
            }
        }
    }

    let mut relevant = FxHashSet::default();
    for lib in checker_lib_files {
        let Some(source_file) = lib.arena.get_source_file_at(lib.root_index) else {
            continue;
        };
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = lib.arena.get(stmt_idx) else {
                continue;
            };
            let Some(interface) = lib.arena.get_interface(stmt_node) else {
                continue;
            };
            let Some(name) = interface_name_text(lib.arena.as_ref(), stmt_idx) else {
                continue;
            };
            if !affected.contains(&name) {
                continue;
            }
            if interface_declares_member_named(lib.arena.as_ref(), interface, &user_member_names)
                || interface_has_indexed_access_member_type(lib.arena.as_ref(), interface)
            {
                relevant.insert(name);
            }
        }
    }

    relevant.extend(seed_interfaces);
    let mut ancestor_queue: Vec<String> = relevant.iter().cloned().collect();
    while let Some(name) = ancestor_queue.pop() {
        let Some(bases) = inheritance_graph.get(&name) else {
            continue;
        };
        for base in bases {
            if relevant.insert(base.clone()) {
                ancestor_queue.push(base.clone());
            }
        }
    }

    if relevant.is_empty() {
        affected
    } else {
        relevant
    }
}

/// Compute the subset of affected lib interfaces that actually declare members
/// matching user-global interface member names.
pub(crate) fn affected_lib_extension_interface_names(
    program: &MergedProgram,
    checker_lib_files: &[Arc<LibFile>],
    affected_interfaces: &FxHashSet<String>,
) -> FxHashSet<String> {
    let user_member_names = collect_user_global_interface_member_names(program);
    let mut extension_interfaces = FxHashSet::default();

    for lib in checker_lib_files {
        let Some(source_file) = lib.arena.get_source_file_at(lib.root_index) else {
            continue;
        };
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = lib.arena.get(stmt_idx) else {
                continue;
            };
            let Some(interface) = lib.arena.get_interface(stmt_node) else {
                continue;
            };
            let Some(name) = interface_name_text(lib.arena.as_ref(), stmt_idx) else {
                continue;
            };
            if affected_interfaces.contains(&name)
                && interface_declares_member_named(
                    lib.arena.as_ref(),
                    interface,
                    &user_member_names,
                )
            {
                extension_interfaces.insert(name);
            }
        }
    }

    extension_interfaces
}

/// Return `true` when `lib_file` contains at least one interface declaration
/// whose name is in `affected_interfaces`.
pub(crate) fn lib_file_contains_affected_interface(
    lib_file: &LibFile,
    affected_interfaces: &FxHashSet<String>,
) -> bool {
    if affected_interfaces.is_empty() {
        return false;
    }

    let Some(source_file) = lib_file.arena.get_source_file_at(lib_file.root_index) else {
        return false;
    };
    source_file.statements.nodes.iter().any(|&stmt_idx| {
        interface_name_text(lib_file.arena.as_ref(), stmt_idx)
            .is_some_and(|name| affected_interfaces.contains(&name))
    })
}

/// Build a `BoundFile` for a lib file that only exposes the node-symbol
/// mappings needed for the interface-extension re-check pass.
pub(crate) fn build_lib_bound_file_for_interface_checks(
    program: &MergedProgram,
    lib_file: &Arc<LibFile>,
    affected_interfaces: &FxHashSet<String>,
) -> BoundFile {
    let _span =
        tracing::debug_span!("parallel_diagnostics::build_lib_bound_file_for_interface_checks")
            .entered();

    let mut node_symbols = FxHashMap::default();
    if let Some(source_file) = lib_file.arena.get_source_file_at(lib_file.root_index) {
        collect_lib_interface_node_symbols(
            lib_file.arena.as_ref(),
            &source_file.statements.nodes,
            &program.globals,
            lib_file.binder.node_symbols.as_ref(),
            affected_interfaces,
            &mut node_symbols,
        );
    }

    // Deep-clone the program-wide `declaration_arenas` into a mutable map so
    // we can add user-global-interface entries below. `program.declaration_arenas`
    // is `Arc`-shared; dereferencing before `.clone()` produces an owned inner
    // map without disturbing the shared data.
    let mut declaration_arenas: DeclarationArenaMap = (*program.declaration_arenas).clone();
    add_user_global_interface_declaration_arenas(program, &mut declaration_arenas);
    let sym_to_decl_indices = Arc::new(build_sym_to_decl_indices(&declaration_arenas));

    BoundFile {
        file_name: lib_file.file_name.clone(),
        source_file: lib_file.root_index,
        arena: Arc::clone(&lib_file.arena),
        node_symbols: Arc::new(node_symbols),
        symbol_arenas: Arc::clone(&program.symbol_arenas),
        declaration_arenas: Arc::new(declaration_arenas),
        sym_to_decl_indices,
        module_declaration_exports_publicly: Arc::new(FxHashMap::default()),
        scopes: Arc::new(Vec::new()),
        node_scope_ids: Arc::new(FxHashMap::default()),
        parse_diagnostics: Vec::new(),
        global_augmentations: Arc::new(FxHashMap::default()),
        module_augmentations: Arc::new(FxHashMap::default()),
        augmentation_target_modules: Arc::new(FxHashMap::default()),
        flow_nodes: Arc::new(FlowNodeArena::default()),
        node_flow: Arc::new(FxHashMap::default()),
        switch_clause_to_switch: Arc::new(FxHashMap::default()),
        is_external_module: lib_file.binder.is_external_module,
        expando_properties: Arc::new(FxHashMap::default()),
        file_features: crate::binder::FileFeatures::NONE,
        lib_symbol_reverse_remap: Arc::new(FxHashMap::default()),
        semantic_defs: Arc::new(FxHashMap::default()),
    }
}

/// Suppress TS2339 (`Property X does not exist on type Y`) diagnostics that
/// are direct cascades from TS2454 (`Variable X is used before being assigned`)
/// on the same receiver, avoiding double-reporting on a pattern like
/// `const x = uninit.prop` where `uninit` is not yet assigned.
pub(crate) fn suppress_parallel_ts2339_cascade_diagnostics(
    arena: &NodeArena,
    diagnostics: &mut Vec<Diagnostic>,
) {
    use crate::checker::diagnostics::diagnostic_codes;

    let ts2454_starts: FxHashSet<u32> = diagnostics
        .iter()
        .filter(|diag| diag.code == diagnostic_codes::VARIABLE_IS_USED_BEFORE_BEING_ASSIGNED)
        .map(|diag| diag.start)
        .collect();
    if ts2454_starts.is_empty() {
        return;
    }

    let ts2339_starts: FxHashSet<u32> = diagnostics
        .iter()
        .filter(|diag| diag.code == diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE)
        .map(|diag| diag.start)
        .collect();
    if ts2339_starts.is_empty() {
        return;
    }

    let mut suppressed_ts2339_starts = FxHashSet::default();
    for raw_idx in 0..arena.len() {
        let idx = NodeIndex(raw_idx as u32);
        let Some(node) = arena.get(idx) else {
            continue;
        };
        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            continue;
        }

        let Some(access) = arena.get_access_expr(node) else {
            continue;
        };
        let Some(name_node) = arena.get(access.name_or_argument) else {
            continue;
        };
        if !ts2339_starts.contains(&name_node.pos) {
            continue;
        }

        let receiver_start = arena.get(access.expression).map(|expr| expr.pos);
        if !receiver_start.is_some_and(|start| ts2454_starts.contains(&start)) {
            continue;
        }

        let Some(ext) = arena.get_extended(idx) else {
            continue;
        };
        let parent = ext.parent;
        let Some(parent_node) = arena.get(parent) else {
            continue;
        };
        if parent_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            continue;
        }

        let Some(var_decl) = arena.get_variable_declaration_at(parent) else {
            continue;
        };
        if var_decl.initializer != idx {
            continue;
        }

        suppressed_ts2339_starts.insert(name_node.pos);
    }

    diagnostics.retain(|diag| {
        !(diag.code == diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE
            && suppressed_ts2339_starts.contains(&diag.start))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::parser::node::NodeArena;

    /// TS2339 is only suppressed when it cascades from a TS2454 on the same
    /// receiver position; standalone TS2339s with no TS2454 present must survive.
    #[test]
    fn ts2339_suppression_no_ts2454() {
        let arena = NodeArena::default();
        let mut diagnostics = vec![
            Diagnostic::error(
                "test.ts".to_string(),
                10,
                3,
                "Property 'foo' does not exist on type 'Bar'.".to_string(),
                diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
            ),
            Diagnostic::error(
                "test.ts".to_string(),
                50,
                3,
                "Property 'baz' does not exist on type 'Qux'.".to_string(),
                diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
            ),
        ];
        let original_len = diagnostics.len();
        suppress_parallel_ts2339_cascade_diagnostics(&arena, &mut diagnostics);
        assert_eq!(diagnostics.len(), original_len);
    }

    /// Only TS2339 whose receiver's source position matches a TS2454 start is
    /// suppressed; a TS2454 at an unrelated position must not suppress other TS2339s.
    #[test]
    fn ts2339_suppression_with_unrelated_ts2454() {
        let arena = NodeArena::default();
        let mut diagnostics = vec![
            Diagnostic::error(
                "test.ts".to_string(),
                0,
                5,
                "Variable 'x' is used before being assigned.".to_string(),
                diagnostic_codes::VARIABLE_IS_USED_BEFORE_BEING_ASSIGNED,
            ),
            Diagnostic::error(
                "test.ts".to_string(),
                100,
                3,
                "Property 'foo' does not exist.".to_string(),
                diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
            ),
        ];
        let original_len = diagnostics.len();
        suppress_parallel_ts2339_cascade_diagnostics(&arena, &mut diagnostics);
        // The TS2339 has no parent `VARIABLE_DECLARATION` in the empty arena,
        // so no AST traversal can link it to the TS2454 — nothing should be removed.
        assert_eq!(diagnostics.len(), original_len);
    }

    #[test]
    fn ts2339_suppression_empty_diagnostics() {
        let arena = NodeArena::default();
        let mut diagnostics: Vec<Diagnostic> = Vec::new();
        suppress_parallel_ts2339_cascade_diagnostics(&arena, &mut diagnostics);
        assert!(diagnostics.is_empty());
    }
}
