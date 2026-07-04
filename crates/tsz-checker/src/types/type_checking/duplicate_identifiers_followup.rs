//! Follow-up duplicate and merged declaration diagnostics.

use crate::query_boundaries::type_checking as type_checking_query;
use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// TS2395 across binder-split `import X = ...` ↔ `type X = ...`
    /// partnerships.
    ///
    /// The binder splits an import-equals declaration (`ALIAS`) and a
    /// same-name local `type X = ...` / `interface X` (`TYPE_ALIAS` /
    /// `INTERFACE`) into two separate symbols, recording the partnership in
    /// `alias_partners`. Per-symbol duplicate-identifier checks therefore see
    /// only one declaration each and skip the merged-declaration
    /// export-consistency rule. tsc treats them as a single merged
    /// declaration, so we must emit TS2395 across the partnership when their
    /// export status differs in the same scope.
    ///
    /// Only `IMPORT_EQUALS_DECLARATION` aliases participate in this rule.
    /// ES6 imports (`import { X }`, `import X from`) and namespace re-exports
    /// (`export * as X`) populate the same `alias_partners` map but tsc does
    /// not treat them as merged declarations for TS2395 purposes:
    /// - `import { X }` + `type X` already reports TS2440 alone.
    /// - `export * as X` + `export type X` is a legitimate dual-namespace
    ///   merge with no diagnostic.
    pub(crate) fn check_alias_partner_merge_export_consistency(&mut self) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use tsz_parser::parser::syntax_kind_ext;

        // Snapshot the partner map up-front: we need to mutate `self` (emit
        // diagnostics) while iterating, and the map is shared via Arc.
        let pairs: Vec<(tsz_binder::SymbolId, tsz_binder::SymbolId)> =
            if let Some(ref ap) = self.ctx.program_alias_partners {
                ap.iter().map(|(&a, &b)| (a, b)).collect()
            } else {
                self.ctx
                    .binder
                    .alias_partners
                    .iter()
                    .map(|(&a, &b)| (a, b))
                    .collect()
            };

        for (type_alias_id, alias_id) in pairs {
            // Both halves of the partnership must be local symbols in the
            // current binder; cross-file alias partners are handled by their
            // own merge logic.
            let Some(type_alias_sym) = self.ctx.binder.get_symbol(type_alias_id) else {
                continue;
            };
            let Some(alias_sym) = self.ctx.binder.get_symbol(alias_id) else {
                continue;
            };

            // Restrict to import-equals partnerships. The binder records
            // partnerships for ES6 import bindings and `export * as ns` too,
            // but tsc only treats import-equals as a merged-declaration peer
            // of a same-named type alias.
            let alias_is_import_equals = alias_sym.declarations.iter().any(|&decl_idx| {
                self.ctx
                    .arena
                    .get(decl_idx)
                    .is_some_and(|n| n.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION)
            });
            if !alias_is_import_equals {
                continue;
            }

            // The partnership is meaningful only when both halves' declarations
            // are local to this file. Symbol-level `is_exported` is the source
            // of truth: each partner symbol holds a single declaration kind
            // (binder split), so `is_exported` reflects that declaration's
            // status. Per-decl `is_declaration_exported` for IMPORT_EQUALS_
            // DECLARATION is intentionally unaware of the export modifier
            // because the existing per-symbol TS2395 logic relies on that.
            let mut decls: Vec<NodeIndex> = Vec::new();
            let mut all_local = true;
            for &decl_idx in type_alias_sym
                .declarations
                .iter()
                .chain(alias_sym.declarations.iter())
            {
                if !self.ctx.binder.node_symbols.contains_key(&decl_idx.0) {
                    all_local = false;
                    break;
                }
                decls.push(decl_idx);
            }
            if !all_local || decls.len() < 2 {
                continue;
            }

            // Skip when both partner symbols agree on export status.
            if type_alias_sym.is_exported == alias_sym.is_exported {
                continue;
            }

            // Skip when all declarations live in an ambient `declare namespace`
            // (identifier-named ambient namespace), matching the existing
            // per-symbol TS2395 suppression for ambient contexts.
            if decls
                .iter()
                .all(|&d| self.is_in_ambient_namespace_not_module(d))
            {
                continue;
            }

            let name = type_alias_sym.escaped_name.clone();
            let message = format_message(
                diagnostic_messages::INDIVIDUAL_DECLARATIONS_IN_MERGED_DECLARATION_MUST_BE_ALL_EXPORTED_OR_ALL_LOCAL,
                &[&name],
            );
            for decl_idx in decls {
                let error_node = self.get_declaration_name_node(decl_idx).unwrap_or(decl_idx);
                self.error_at_node(
                    error_node,
                    &message,
                    diagnostic_codes::INDIVIDUAL_DECLARATIONS_IN_MERGED_DECLARATION_MUST_BE_ALL_EXPORTED_OR_ALL_LOCAL,
                );
            }
        }
    }

    pub(crate) fn check_block_scoped_function_outer_conflicts(&mut self) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        let mut seen = FxHashSet::default();

        // When libs are loaded the binder's symbol table also holds the
        // thousands of merged lib symbols. Block-scoped function conflicts
        // can only involve functions declared by nodes of the *current file*,
        // so walk the symbols bound from this file's nodes (mirrors
        // `collect_duplicate_check_symbol_ids`) instead of filtering the full
        // lib-merged table once per checked file. Ids are sorted so the
        // candidate order matches the symbol-table iteration order the
        // unrestricted path produces.
        let has_libs = self.ctx.has_lib_loaded() || !self.ctx.binder.lib_symbol_ids.is_empty();

        let function_symbol_decls = |symbol: &tsz_binder::Symbol| {
            let sym_id = symbol.id;
            symbol
                .declarations
                .iter()
                .filter_map(|&decl_idx| {
                    let node = self.ctx.arena.get(decl_idx)?;
                    if node.kind != tsz_parser::parser::syntax_kind_ext::FUNCTION_DECLARATION {
                        return None;
                    }
                    if self.get_enclosing_block_scope(decl_idx).is_none() {
                        return None;
                    }
                    let name = self.get_declaration_name_text(decl_idx)?;
                    Some((sym_id, decl_idx, name))
                })
                .collect::<Vec<_>>()
        };

        let block_function_decls: Vec<(tsz_binder::SymbolId, NodeIndex, String)> = if has_libs {
            let mut user_sym_ids: Vec<tsz_binder::SymbolId> =
                self.ctx.binder.node_symbols.values().copied().collect();
            user_sym_ids.sort_unstable();
            user_sym_ids.dedup();
            user_sym_ids
                .into_iter()
                .filter_map(|sym_id| self.ctx.binder.get_symbol(sym_id))
                .filter(|symbol| symbol.has_any_flags(symbol_flags::FUNCTION))
                .flat_map(&function_symbol_decls)
                .collect()
        } else {
            self.ctx
                .binder
                .symbols
                .iter()
                .filter(|symbol| symbol.has_any_flags(symbol_flags::FUNCTION))
                .flat_map(function_symbol_decls)
                .collect()
        };

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
            // let/const OR var in outer scopes (the var binds at function
            // scope, the block function binds at block scope — different
            // scopes ⇒ no merge ⇒ no TS2300/TS2451). Match tsc:
            // duplicateIdentifierInCatchBlock.ts only emits errors for the
            // `var w` ⇄ outer `function w` collision (where the function is
            // at function scope itself), not for the catch-block-nested
            // `function v / function x` cases.
            let all_outer_are_simple_variables =
                conflicting_outer_decls.iter().all(|(_, flags)| {
                    (flags
                        & (symbol_flags::BLOCK_SCOPED_VARIABLE
                            | symbol_flags::FUNCTION_SCOPED_VARIABLE))
                        != 0
                });
            if all_outer_are_simple_variables && self.ctx.compiler_options.target.supports_es2015()
            {
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
    pub(crate) fn check_merged_interface_declaration_diagnostics(
        &mut self,
        declarations: &[NodeIndex],
    ) {
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

        for (scope, mut declarations_in_scope) in declarations_by_scope {
            if declarations_in_scope.len() <= 1 {
                continue;
            }

            // Merge diagnostics only when interface type parameters are identical.
            // TS2428 is reported separately; once mismatched, compatibility checks
            // should not be compared across declarations in the same scope.
            if !self.interface_type_parameters_are_group_merge_compatible(&declarations_in_scope) {
                continue;
            }
            let allow_symbol_constructor_refinement =
                self.is_global_symbol_constructor_interface_group(scope, &declarations_in_scope);

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
                            type_checking_query::method_function_type(
                                self.ctx.types,
                                type_params,
                                params,
                                return_type,
                            )
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
                                    && !self
                                        .duplicate_decl_types_match(existing_type, *property_type)
                                {
                                    let existing_type_str = self.format_type(existing_type);
                                    let property_type_str = self.format_type(*property_type);
                                    self.error_at_node_msg(
                                        *name_idx,
                                        diagnostic_codes::SUBSEQUENT_PROPERTY_DECLARATIONS_MUST_HAVE_THE_SAME_TYPE_PROPERTY_MUST_BE_OF_TYP,
                                        &[name, &existing_type_str, &property_type_str],
                                    );
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
                            if allow_symbol_constructor_refinement
                                && self.is_symbol_constructor_symbol_refinement_pair(
                                    existing_type,
                                    *property_type,
                                )
                            {
                                continue;
                            }
                            if !self.duplicate_decl_types_match(existing_type, *property_type) {
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
                        && !self.duplicate_decl_type_matches_index(*property_type, number_index)
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
                        && !self.duplicate_decl_type_matches_index(*property_type, string_index)
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
                    if !self.duplicate_index_types_overlap(local_number, existing_number) {
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
                    if !self.duplicate_index_types_overlap(local_string, existing_string) {
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

    /// Collect every global-scope interface declaration named `name` across the
    /// program, paired with the index of the file that declares it.
    ///
    /// Only top-level interface declarations in files that are NOT external
    /// modules are returned: those are the declarations that merge into a single
    /// global interface symbol across files. Module-scoped interfaces with the
    /// same name in different files are distinct symbols and never merge. This
    /// runs only for the rare global-script program (the caller gates on the
    /// current file being a non-module script), so a direct scan is sufficient.
    fn global_script_interface_declarations(&self, name: &str) -> Vec<(NodeIndex, usize)> {
        use tsz_parser::parser::syntax_kind_ext;

        let Some(all_arenas) = self.ctx.all_arenas.as_ref() else {
            return Vec::new();
        };

        let mut out = Vec::new();
        for (file_idx, arena) in all_arenas.iter().enumerate() {
            let Some(binder) = self.ctx.get_binder_for_file(file_idx) else {
                continue;
            };
            if binder.is_external_module() {
                continue;
            }
            let Some(source_file) = arena.source_files.first() else {
                continue;
            };
            for &stmt_idx in &source_file.statements.nodes {
                let Some(stmt_node) = arena.get(stmt_idx) else {
                    continue;
                };
                if stmt_node.kind != syntax_kind_ext::INTERFACE_DECLARATION {
                    continue;
                }
                if arena
                    .get_interface(stmt_node)
                    .and_then(|decl| arena.get_identifier_at(decl.name))
                    .is_some_and(|ident| ident.escaped_text == name)
                {
                    out.push((stmt_idx, file_idx));
                }
            }
        }
        out
    }

    /// Cross-file (cross-arena) global interface merge — TS2717.
    ///
    /// When the same global-scope interface is declared in two or more global
    /// script files (non-module `.ts` / `.d.ts`) and the declarations disagree
    /// on a property's type, tsc reports TS2717
    /// ("Subsequent property declarations must have the same type") anchored at
    /// the subsequent declaration. The same-file merge path
    /// ([`Self::check_merged_interface_declaration_diagnostics`]) only ever sees
    /// the current file's (local) declarations, because the upstream
    /// `interface_decls` collection filters to `is_local`; a conflict that spans
    /// files is therefore invisible to it and tsz emits nothing.
    ///
    /// This routine closes that gap order-independently — it never relies on the
    /// merged symbol type (which is only populated cross-file once some file has
    /// been checked, a per-file/parallel-order race). Instead it resolves the
    /// canonical first-declaration property type directly:
    /// - when the first declaration in program order is in the current file, via
    ///   the full local type resolver, covering every annotation form;
    /// - when it is in another file, via the foreign arena using only annotations
    ///   that need no checker context (keyword / intrinsic types). Any other
    ///   foreign first-declaration annotation is left unresolved and that property
    ///   is conservatively skipped, so the check never emits a false positive.
    ///
    /// Each LOCAL subsequent property signature is compared against that canonical
    /// type and TS2717 is emitted at the local member when the types differ.
    /// Diagnostics are anchored only on local members, so each file's check
    /// contributes its own subsequent-declaration errors and the union across all
    /// files reproduces tsc's full set. Overlaps with the same-file path collapse
    /// on the shared `(start, 2717)` dedup key.
    pub(crate) fn check_cross_file_global_interface_member_conflicts(&mut self, name: &str) {
        use crate::diagnostics::diagnostic_codes;
        use crate::types_domain::queries::core::get_literal_property_name;
        use rustc_hash::FxHashMap;
        use std::sync::Arc;
        use tsz_parser::parser::syntax_kind_ext;

        // A single-file program cannot have a cross-file merge.
        let Some(all_arenas) = self.ctx.all_arenas.as_ref() else {
            return;
        };
        if all_arenas.len() <= 1 {
            return;
        }
        // Own a handle to the arenas so the foreign-arena reads below do not
        // borrow `self`, leaving `&mut self` free for type resolution.
        let all_arenas = Arc::clone(all_arenas);

        let decls = self.global_script_interface_declarations(name);
        if decls.len() <= 1 {
            return;
        }
        // Require declarations spanning at least two distinct files. A pure
        // same-file merge is already handled by
        // `check_merged_interface_declaration_diagnostics`.
        let mut files: Vec<usize> = decls.iter().map(|&(_, f)| f).collect();
        files.sort_unstable();
        files.dedup();
        if files.len() <= 1 {
            return;
        }

        let current = self.ctx.current_file_idx;

        // Generic global interfaces are skipped: a member that references the
        // interface's own type parameter resolves to a distinct `TypeParameter`
        // instance per declaration, which would compare unequal and produce a
        // false TS2717. Cross-file generic global merges with conflicting members
        // are vanishingly rare; the property-identity facet covers the reported
        // cases.
        for &(decl_idx, file_idx) in &decls {
            let Some(arena) = all_arenas.get(file_idx) else {
                continue;
            };
            if arena
                .get(decl_idx)
                .and_then(|node| arena.get_interface(node))
                .is_some_and(|iface| {
                    iface
                        .type_parameters
                        .as_ref()
                        .is_some_and(|tp| !tp.nodes.is_empty())
                })
            {
                return;
            }
        }

        // Order declarations by (file index, declaration position) = program
        // order, matching how tsc picks the "first" declaration of a property.
        let mut ordered = decls;
        ordered.sort_by_key(|&(decl_idx, file_idx)| {
            let pos = all_arenas
                .get(file_idx)
                .and_then(|arena| arena.get(decl_idx))
                .map_or(u32::MAX, |node| node.pos);
            (file_idx, pos)
        });

        // Pre-collect every declaration's property members (read-only, from its
        // own arena) before any `&mut self` type resolution.
        struct Member {
            file_idx: usize,
            is_local: bool,
            name_node: NodeIndex,
            type_node: NodeIndex,
            prop_name: String,
        }
        let mut members: Vec<Member> = Vec::new();
        for &(decl_idx, file_idx) in &ordered {
            let Some(arena) = all_arenas.get(file_idx) else {
                continue;
            };
            let Some(node) = arena.get(decl_idx) else {
                continue;
            };
            let Some(iface) = arena.get_interface(node) else {
                continue;
            };
            let is_local = file_idx == current;
            for &member_idx in &iface.members.nodes {
                let Some(member_node) = arena.get(member_idx) else {
                    continue;
                };
                if member_node.kind != syntax_kind_ext::PROPERTY_SIGNATURE {
                    continue;
                }
                let Some(sig) = arena.get_signature(member_node) else {
                    continue;
                };
                let Some(prop_name) = get_literal_property_name(arena, sig.name) else {
                    continue;
                };
                members.push(Member {
                    file_idx,
                    is_local,
                    name_node: sig.name,
                    type_node: sig.type_annotation,
                    prop_name,
                });
            }
        }

        // Canonical first-declaration property type per name. `None` records that
        // the first declaration's type could not be resolved without its file's
        // checker context (a foreign non-keyword annotation); subsequent
        // comparisons for that name are then conservatively skipped.
        let mut canonical: FxHashMap<String, Option<TypeId>> = FxHashMap::default();

        for m in members {
            if let Some(&first) = canonical.get(&m.prop_name) {
                // Subsequent declaration of this property.
                let Some(first_type) = first else {
                    continue;
                };
                // Remote subsequents are reported when their own file is checked.
                if !m.is_local {
                    continue;
                }
                let local_type = if m.type_node.is_some() {
                    self.get_type_from_type_node(m.type_node)
                } else {
                    TypeId::ANY
                };
                if self.type_contains_error(first_type) || self.type_contains_error(local_type) {
                    continue;
                }
                // TS2717 uses type identity; `duplicate_decl_types_match` is the
                // same bidirectional comparison the same-file merge path uses.
                if self.duplicate_decl_types_match(first_type, local_type) {
                    continue;
                }
                let display_name = self
                    .get_member_name_display_text(m.name_node)
                    .unwrap_or_else(|| m.prop_name.clone());
                let first_str = self.format_type(first_type);
                let local_str = self.format_type(local_type);
                self.error_at_node_msg(
                    m.name_node,
                    diagnostic_codes::SUBSEQUENT_PROPERTY_DECLARATIONS_MUST_HAVE_THE_SAME_TYPE_PROPERTY_MUST_BE_OF_TYP,
                    &[&display_name, &first_str, &local_str],
                );
            } else {
                // First declaration of this property: resolve its canonical type.
                let first_type = if m.is_local {
                    Some(if m.type_node.is_some() {
                        self.get_type_from_type_node(m.type_node)
                    } else {
                        TypeId::ANY
                    })
                } else if let Some(arena) = all_arenas.get(m.file_idx) {
                    foreign_keyword_type_id(arena, m.type_node)
                } else {
                    None
                };
                canonical.insert(m.prop_name, first_type);
            }
        }
    }

    pub(crate) fn declaration_participates_in_default_export_conflict(
        &self,
        decl_idx: NodeIndex,
    ) -> bool {
        let mut current = decl_idx;
        let mut export_idx = None;
        for _ in 0..4 {
            let Some(node) = self.ctx.arena.get(current) else {
                break;
            };
            if self.ctx.arena.get_export_decl(node).is_some() {
                export_idx = Some(current);
                break;
            }
            let Some(parent) = self.ctx.arena.parent_of(current) else {
                break;
            };
            if parent.is_none() {
                break;
            }
            current = parent;
        }

        let Some(export_idx) = export_idx else {
            return false;
        };
        let Some(export_decl) = self.ctx.arena.get_export_decl_at(export_idx) else {
            return false;
        };
        if export_decl.is_default_export {
            return true;
        }

        let Some(clause_node) = self.ctx.arena.get(export_decl.export_clause) else {
            return false;
        };
        let Some(named_exports) = self.ctx.arena.get_named_imports(clause_node) else {
            return false;
        };

        named_exports.elements.nodes.iter().any(|&specifier_idx| {
            let Some(specifier_node) = self.ctx.arena.get(specifier_idx) else {
                return false;
            };
            let Some(specifier) = self.ctx.arena.get_specifier(specifier_node) else {
                return false;
            };
            !specifier.is_type_only
                && self
                    .get_identifier_text_from_idx(specifier.name)
                    .is_some_and(|name| name == "default")
        })
    }
}

/// Resolve a type-annotation node to its `TypeId` when — and only when — the
/// annotation is a primitive / intrinsic type that maps to a fixed,
/// interner-stable `TypeId` with no symbol-resolution or type-parameter context
/// required.
///
/// Used to resolve a property's type from a FOREIGN arena (a different file's
/// AST) for the cross-file interface-merge TS2717 check, where the current
/// checker context cannot lower the node. Primitive types cover the overwhelming
/// majority of real cross-file property conflicts; any other annotation returns
/// `None` so the caller conservatively skips the property (never a false
/// positive).
///
/// Two encodings reach here: a bare keyword token (e.g. `void`, `null`) and —
/// more commonly — a `TYPE_REFERENCE` whose name is a primitive (`number`,
/// `string`, ...). A reference with type arguments (`Array<number>`) or a
/// non-primitive name is left unresolved. The keyword/name → `TypeId` mappings
/// reuse the shared lib-resolution tables so this stays in lockstep with how the
/// lowering pipeline resolves the same syntax.
fn foreign_keyword_type_id(
    arena: &tsz_parser::parser::node::NodeArena,
    type_node: NodeIndex,
) -> Option<TypeId> {
    use crate::types_domain::queries::lib_resolution::{
        keyword_name_to_type_id, keyword_syntax_to_type_id,
    };
    use tsz_parser::parser::syntax_kind_ext;

    // A property with no annotation is implicitly `any`.
    if type_node.is_none() {
        return Some(TypeId::ANY);
    }
    let node = arena.get(type_node)?;

    // Primitive named type reference, e.g. `number` / `string` (no type args).
    if node.kind == syntax_kind_ext::TYPE_REFERENCE
        && let Some(type_ref) = arena.get_type_ref(node)
    {
        if type_ref.type_arguments.is_some() {
            return None;
        }
        let name = arena.get_identifier_at(type_ref.type_name)?;
        return keyword_name_to_type_id(&name.escaped_text);
    }

    // Bare keyword token encodings.
    keyword_syntax_to_type_id(node.kind)
}
