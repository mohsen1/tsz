//! Type reference resolution: interfaces, type aliases, and type references
//! on `CheckerState`.

use crate::query_boundaries::state::type_resolution as query;
use crate::state::CheckerState;
use crate::symbol_resolver::TypeSymbolResolution;
use tsz_binder::symbol_flags;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::{NodeIndex, NodeList, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn same_file_type_alias_parts_for_name(
        &self,
        name: &str,
    ) -> Option<(Option<NodeList>, NodeIndex, Option<tsz_binder::SymbolId>)> {
        self.ctx
            .arena
            .nodes
            .iter()
            .enumerate()
            .find_map(|(idx, node)| {
                let type_alias = self.ctx.arena.get_type_alias(node)?;
                let alias_name = self.ctx.arena.get_identifier_text(type_alias.name)?;
                (alias_name == name).then(|| {
                    (
                        type_alias.type_parameters.clone(),
                        type_alias.type_node,
                        self.ctx.binder.node_symbols.get(&(idx as u32)).copied(),
                    )
                })
            })
    }

    /// Get type from a type reference node (e.g., "number", "string", "`MyType`").
    pub(crate) fn get_type_from_type_reference(&mut self, idx: NodeIndex) -> TypeId {
        // Fuel check: prevent infinite loops in circular type references
        if !self.ctx.consume_fuel() {
            return TypeId::ERROR;
        }

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        // Get the TypeRefData from the arena
        let Some(type_ref) = self.ctx.arena.get_type_ref(node) else {
            return TypeId::ERROR; // Missing type ref data - propagate error
        };

        let type_name_idx = type_ref.type_name;
        let has_type_args = type_ref
            .type_arguments
            .as_ref()
            .is_some_and(|args| !args.nodes.is_empty());

        // Check if type_name is an import type call expression: import("./module")
        // or a qualified name rooted in one: import("./module").Foo
        if let Some(name_node) = self.ctx.arena.get(type_name_idx) {
            let import_call_idx = if name_node.kind == syntax_kind_ext::CALL_EXPRESSION {
                // Direct import type: import("./module")
                Some(type_name_idx)
            } else if name_node.kind == syntax_kind_ext::QUALIFIED_NAME {
                // Qualified import type: import("./module").Foo.Bar
                // Walk left chain to find the root CALL_EXPRESSION
                self.find_leftmost_import_call(type_name_idx)
            } else {
                None
            };

            if let Some(call_idx) = import_call_idx {
                let resolved = self.check_import_type_and_resolve(call_idx, type_name_idx, idx);
                // Apply type arguments to import types: import("./foo").Bar<{x: number}>
                // Without this, the type parameter T remains uninstantiated and
                // assignability checks fail with false TS2322 errors.
                if has_type_args
                    && resolved != TypeId::ERROR
                    && let Some(args) = &type_ref.type_arguments
                {
                    // Validate type arguments against constraints (TS2344)
                    if !self.is_inside_type_parameter_declaration(idx)
                        && let Some(sym_id) =
                            self.resolve_import_type_target_symbol(call_idx, type_name_idx)
                    {
                        self.validate_type_reference_type_arguments(sym_id, args, idx);
                    }
                    let type_args: Vec<TypeId> = args
                        .nodes
                        .iter()
                        .map(|&arg_idx| self.get_type_from_type_node(arg_idx))
                        .collect();
                    if !type_args.is_empty() {
                        return self.ctx.types.application(resolved, type_args);
                    }
                }
                return resolved;
            }
        }

        // Check if type_name is a qualified name (A.B)
        if let Some(name_node) = self.ctx.arena.get(type_name_idx)
            && name_node.kind == syntax_kind_ext::QUALIFIED_NAME
        {
            if has_type_args {
                let sym_id = match self.resolve_qualified_symbol_in_type_position(type_name_idx) {
                    TypeSymbolResolution::Type(sym_id) => {
                        self.check_for_static_member_class_type_param_reference(
                            sym_id,
                            type_name_idx,
                        );
                        sym_id
                    }
                    TypeSymbolResolution::ValueOnly(_) => {
                        let name = self
                            .entity_name_text(type_name_idx)
                            .unwrap_or_else(|| "<unknown>".to_string());
                        // Route through wrong-meaning boundary: value used as type
                        use crate::query_boundaries::name_resolution::NameLookupKind;
                        self.report_wrong_meaning_diagnostic(
                            &name,
                            type_name_idx,
                            NameLookupKind::Value,
                        );
                        // Visit type arguments so nested unresolved identifiers
                        // (e.g. `T` in `ns.Foo<T>`) surface their own diagnostics.
                        if let Some(args) = &type_ref.type_arguments {
                            for &arg_idx in &args.nodes {
                                let _ = self.get_type_from_type_node(arg_idx);
                            }
                        }
                        return TypeId::ERROR;
                    }
                    TypeSymbolResolution::NotFound => {
                        if let Some(sym_id) = self.resolve_qualified_symbol(type_name_idx) {
                            if let Some(args) = &type_ref.type_arguments
                                && !self.is_inside_type_parameter_declaration(idx)
                            {
                                // Suppress TS2315 cascading errors when the left side
                                // of the qualified name is an unresolved import
                                // (e.g., `React.Component<P>` where 'react' module
                                // couldn't be resolved).
                                if let Some(qn) =
                                    self.ctx.arena.get_qualified_name_at(type_name_idx)
                                {
                                    if !self.is_unresolved_import_symbol(qn.left) {
                                        self.validate_type_reference_type_arguments(
                                            sym_id, args, idx,
                                        );
                                    }
                                } else {
                                    self.validate_type_reference_type_arguments(sym_id, args, idx);
                                }
                            }
                            return self.type_reference_symbol_type(sym_id);
                        }
                        let _ = self.resolve_qualified_name(type_name_idx);
                        // Visit type arguments so nested unresolved identifiers
                        // (e.g. `T` in `E.F<T>`) surface their own TS2304 diagnostics.
                        // Without this, `var v3: E.F<T>` only reports TS2503 for `E`
                        // and silently drops the unresolved `T`, diverging from tsc.
                        if let Some(args) = &type_ref.type_arguments {
                            for &arg_idx in &args.nodes {
                                let _ = self.get_type_from_type_node(arg_idx);
                            }
                        }
                        return TypeId::ERROR;
                    }
                };
                if let Some(args) = &type_ref.type_arguments {
                    if self.should_resolve_recursive_type_alias(sym_id, args) {
                        // Ensure the base type symbol is resolved first so its type params
                        // are available in the type_env for Application expansion
                        let _ = self.get_type_of_symbol(sym_id);
                    }
                    for &arg_idx in &args.nodes {
                        let _ = self.get_type_from_type_node(arg_idx);
                    }
                    // Validate type arguments against constraints (TS2344)
                    // Skip validation inside type parameter declarations (constraints/defaults)
                    if !self.is_inside_type_parameter_declaration(idx)
                        && self.validate_type_reference_type_arguments(sym_id, args, idx)
                    {
                        // Wrong number of type arguments (TS2314/TS2707).
                        // Return ERROR to match tsc's errorType propagation and
                        // prevent cascading diagnostics (e.g., false TS2322 on
                        // return statements whose return type has bad arg count).
                        return TypeId::ERROR;
                    }
                }
                let type_param_bindings = self.get_type_param_bindings();
                let type_resolver =
                    |node_idx: NodeIndex| self.resolve_type_symbol_for_lowering(node_idx);
                // Stable-identity helper: prefer Lazy(DefId) over Ref(SymbolRef)
                let def_id_resolver =
                    |node_idx: NodeIndex| self.resolve_def_id_for_lowering(node_idx);
                let value_resolver =
                    |node_idx: NodeIndex| self.resolve_value_symbol_for_lowering(node_idx);
                // Name-based DefId fallback for qualified names whose
                // NodeIndex resolver path can't see imported namespace
                // members (e.g. cross-file `util.OmitKeys` references inside
                // an alias body whose TypeReference node was bound in a
                // sibling file). Without this fallback the lowering writes
                // `Application(UnresolvedTypeName("util.OmitKeys"), args)`,
                // which silently disappears from downstream object spread
                // and intersection reduction.
                let name_resolver = |type_name: &str| -> Option<tsz_solver::def::DefId> {
                    (!self.ctx.file_local_type_shadow_for_lib_name(type_name))
                        .then(|| self.resolve_actual_lib_name_to_def_id_for_lowering(type_name))
                        .flatten()
                        .or_else(|| self.resolve_entity_name_text_to_def_id_for_lowering(type_name))
                        .or_else(|| {
                            crate::types_domain::queries::lib_resolution::resolve_name_to_lib_symbol(
                                type_name,
                                self.ctx.binder,
                                self.ctx.global_file_locals_index.as_deref(),
                                self.ctx
                                    .all_binders
                                    .as_ref()
                                    .map(|binders| binders.as_ref().as_slice()),
                                &self.ctx.lib_contexts,
                            )
                            .map(|sym_id| self.ctx.get_canonical_lib_def_id(type_name, sym_id))
                        })
                };
                let type_query_override = |expr_name_idx: NodeIndex| -> Option<TypeId> {
                    let type_query_idx = self.ctx.arena.get_extended(expr_name_idx)?.parent;
                    let type_query_node = self.ctx.arena.get(type_query_idx)?;
                    if type_query_node.kind != syntax_kind_ext::TYPE_QUERY {
                        return None;
                    }
                    self.ctx
                        .node_types
                        .get(&type_query_idx.0)
                        .copied()
                        .filter(|&type_id| type_id != TypeId::ANY && type_id != TypeId::ERROR)
                };
                let lowering = tsz_lowering::TypeLowering::with_hybrid_resolver(
                    self.ctx.arena,
                    self.ctx.types,
                    &type_resolver,
                    &def_id_resolver,
                    &value_resolver,
                )
                .with_type_param_bindings(type_param_bindings)
                .with_name_def_id_resolver(&name_resolver)
                .with_type_query_override(&type_query_override);
                let mut type_id = lowering.lower_type(idx);
                if query::get_application_info(self.ctx.types, type_id).is_none()
                    && let Some(args) = &type_ref.type_arguments
                {
                    let type_args = args
                        .nodes
                        .iter()
                        .map(|&arg_idx| self.get_type_from_type_node(arg_idx))
                        .collect::<Vec<_>>();
                    if !type_args.is_empty() {
                        let base_type = self.type_reference_symbol_type(sym_id);
                        type_id = self.ctx.types.application(base_type, type_args);
                    }
                }

                // Eagerly evaluate type alias applications to detect TS2589
                // (excessive instantiation depth). Without this, the application
                // stays lazy and deep recursion is never detected.
                // Skip when args contain type parameters — the alias body
                // may be self-referential (recursive conditional), and eager
                // evaluation with unresolved params causes false TS2589.
                // Actual TS2589 detection happens at instantiation with concrete types.
                let lib_binders = self.get_lib_binders();
                let symbol_info = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders);
                let is_type_alias =
                    symbol_info.is_some_and(|s| s.has_any_flags(symbol_flags::TYPE_ALIAS));
                // TS2589 detection for class types with generic type arguments that may
                // recursively expand (e.g., `Foo<[...Elements, "abc"]>` where mapped types
                // in the class cause infinite type instantiation)
                let is_class = symbol_info.is_some_and(|s| s.has_any_flags(symbol_flags::CLASS));

                if is_type_alias || is_class {
                    let args_have_type_params =
                        type_ref.type_arguments.as_ref().is_some_and(|args| {
                            self.type_arg_nodes_contain_scoped_type_parameter_for_depth_check(args)
                        }) || query::get_application_info(self.ctx.types, type_id).is_some_and(
                            |(_, args)| {
                                args.iter().any(|&arg| {
                                    query::contains_type_parameters(self.ctx.types, arg)
                                })
                            },
                        );
                    // For type aliases: skip TS2589 detection if args contain type params
                    // to avoid false positives with recursive conditional types.
                    // For classes: always check because recursive tuple spreads (e.g.,
                    // `Foo<[...Elements, "abc"]>`) need depth detection even with type params.
                    let should_check_depth = is_class || !args_have_type_params;
                    if should_check_depth {
                        // During symbol resolution, ensure_relation_input_ready is skipped,
                        // leaving the alias body unregistered in the TypeEnvironment. Without
                        // it the evaluator returns the Application unchanged and TS2589 is missed.
                        if let Some(base_def_id) =
                            crate::query_boundaries::common::get_application_lazy_def_id(
                                self.ctx.types,
                                type_id,
                            )
                        {
                            let _ = self.resolve_and_insert_def_type(base_def_id);
                        }

                        self.ctx.depth_exceeded.set(false);
                        // Use the regular evaluator for ordinary type-reference
                        // probes. The TS2589-specific evaluator treats any repeated
                        // Application cycle as overflow, which is too aggressive for
                        // bounded recursive conditional aliases.
                        let exceeded = {
                            let _ = self.evaluate_type_with_env_uncached(type_id);
                            self.ctx.depth_exceeded.get()
                        };

                        // TS2589: emit at the type reference node if depth was exceeded

                        // Also detect circular mapped-type aliases that the evaluator
                        // can't expand: if the alias body is a mapped type that
                        // references itself in the template (e.g.,
                        // `type Circular<T> = {[P in keyof T]: Circular<T>}`),
                        // any concrete instantiation is infinitely recursive.
                        // Check unconditionally (even when exceeded) so we can
                        // emit TS2615 alongside TS2589.
                        let circular_mapped = is_type_alias
                            && query::get_application_info(self.ctx.types, type_id)
                                .and_then(|(base, _)| query::get_lazy_def_id(self.ctx.types, base))
                                .and_then(|def_id| self.ctx.def_to_symbol_id(def_id))
                                .is_some_and(|ref_sym| {
                                    // The base is a type alias whose body is a mapped
                                    // type that references itself in its template
                                    self.ctx.binder.get_symbol(ref_sym).is_some_and(|symbol| {
                                        symbol.has_any_flags(symbol_flags::TYPE_ALIAS)
                                            && symbol.declarations.iter().any(|&decl_idx| {
                                                self.alias_has_self_referencing_mapped_body(
                                                    ref_sym, decl_idx,
                                                )
                                            })
                                    })
                                });

                        if exceeded || circular_mapped {
                            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                            let (message, code) = if exceeded
                                && is_type_alias
                                && self.type_alias_is_unconditional_tuple_spread(sym_id)
                            {
                                (
                                    diagnostic_messages::TYPE_PRODUCES_A_TUPLE_TYPE_THAT_IS_TOO_LARGE_TO_REPRESENT,
                                    diagnostic_codes::TYPE_PRODUCES_A_TUPLE_TYPE_THAT_IS_TOO_LARGE_TO_REPRESENT,
                                )
                            } else {
                                (
                                    diagnostic_messages::TYPE_INSTANTIATION_IS_EXCESSIVELY_DEEP_AND_POSSIBLY_INFINITE,
                                    diagnostic_codes::TYPE_INSTANTIATION_IS_EXCESSIVELY_DEEP_AND_POSSIBLY_INFINITE,
                                )
                            };
                            self.error_at_node(idx, message, code);

                            // TS2615: When a circular mapped type is involved,
                            // also emit the property-circularity diagnostic.
                            if circular_mapped {
                                self.emit_ts2615_for_circular_mapped_type(idx, type_id);
                            }

                            // tsc returns `any` for excessively deep types to
                            // suppress cascading errors (e.g., TS2322).
                            return TypeId::ANY;
                        }
                    }
                }

                return type_id;
            }
            // No type arguments provided - check if this generic type requires them
            // Also, use type_reference_symbol_type to preserve nominal identity for enum members
            let qn_sym_res = self.resolve_qualified_symbol_in_type_position(type_name_idx);
            if let TypeSymbolResolution::Type(sym_id) = qn_sym_res {
                self.check_for_static_member_class_type_param_reference(sym_id, type_name_idx);
                let name = self
                    .get_symbol_globally(sym_id)
                    .map(|s| s.escaped_name.clone())
                    .or_else(|| self.entity_name_text(type_name_idx))
                    .unwrap_or_else(|| "<unknown>".to_string());
                let required_count = self.count_required_reference_type_params(sym_id, &name);
                if required_count > 0 {
                    // tsc displays type name with param names: Foo<T, U>
                    let type_params = self.get_reference_type_params_for_symbol(sym_id, &name);
                    let display_name = Self::format_generic_display_name_with_interner(
                        &name,
                        &type_params,
                        self.ctx.types,
                    );
                    self.error_generic_type_requires_type_arguments_at(
                        &display_name,
                        required_count,
                        idx,
                    );
                    // tsc returns errorType when a generic type is used without
                    // required type arguments. This prevents cascading errors
                    // like TS2454 on variables with erroneous type annotations.
                    return TypeId::ERROR;
                }

                // TSZ-4: Use type_reference_symbol_type to preserve nominal identity
                // This ensures enum members return TypeData::Enum instead of primitives
                let mut result = self.type_reference_symbol_type(sym_id);
                let pre_augmentation_result = result;

                // For `import * as x from "m"; type T = x.A`, apply module augmentations
                // to the referenced member type (A) using the module specifier from `x`.
                // Also handles nested qualified names like `ns.Root.Foo` by walking
                // up the chain to find the root identifier's module specifier.
                if let Some(qn) = self
                    .ctx
                    .arena
                    .get(type_name_idx)
                    .and_then(|n| self.ctx.arena.get_qualified_name(n))
                    && let Some(right_node) = self.ctx.arena.get(qn.right)
                    && let Some(right_ident) = self.ctx.arena.get_identifier(right_node)
                {
                    let module_specifier = if let Some(left_node) = self.ctx.arena.get(qn.left)
                        && left_node.kind == SyntaxKind::Identifier as u16
                        && let Some(left_sym_id) =
                            self.resolve_identifier_symbol_as_qualified_type_anchor(qn.left)
                    {
                        let lib_binders = self.get_lib_binders();
                        self.ctx
                            .binder
                            .get_symbol_with_libs(left_sym_id, &lib_binders)
                            .and_then(|s| s.import_module.clone())
                    } else {
                        // Nested qualified name (e.g., ns.Root.Foo) — walk to
                        // root identifier to extract the module specifier.
                        let lib_binders = self.get_lib_binders();
                        self.extract_root_module_specifier(qn.left, &lib_binders)
                    };
                    if let Some(module_specifier) = module_specifier {
                        result = self.apply_module_augmentations(
                            &module_specifier,
                            &right_ident.escaped_text,
                            result,
                        );
                    }
                }

                // After applying module augmentations, update the DefId->TypeId mapping
                // for the original symbol so that self-referential Lazy(DefId) types
                // within the merged type resolve to the augmented version.
                // Without this, `self: Foo` inside `declare module "./m" { interface Foo { self: Foo } }`
                // would resolve to the un-augmented Foo, causing false TS2339 on `f.self.self`.
                if result != pre_augmentation_result {
                    let def_id = self.ctx.get_or_create_def_id(sym_id);
                    self.ctx.symbol_types.insert(sym_id, result);
                    self.ctx.symbol_instance_types.insert(sym_id, result);
                    let type_params = self.ctx.get_def_type_params(def_id).unwrap_or_default();
                    self.ctx
                        .register_def_auto_params_in_envs(def_id, result, type_params);
                }

                // For simple name type refs like `import { Foo } from "./m"; type T = Foo`,
                // apply module augmentations using the import symbol's module specifier.
                if result != TypeId::ERROR {
                    let lib_binders = self.get_lib_binders();
                    let imported_module = self
                        .ctx
                        .binder
                        .get_symbol_with_libs(sym_id, &lib_binders)
                        .and_then(|symbol| {
                            symbol.import_module.as_ref().map(|module_specifier| {
                                (
                                    module_specifier.clone(),
                                    symbol
                                        .import_name
                                        .clone()
                                        .unwrap_or_else(|| symbol.escaped_name.clone()),
                                )
                            })
                        });
                    if let Some((module_specifier, aug_name)) = imported_module {
                        result =
                            self.apply_module_augmentations(&module_specifier, &aug_name, result);
                    }
                }

                return result;
            }
            return self.resolve_qualified_name(type_name_idx);
        }

        // Get the identifier for the type name
        if let Some(name_node) = self.ctx.arena.get(type_name_idx)
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
        {
            let name = ident.escaped_text.as_str();
            let has_libs = self.ctx.has_lib_loaded();
            let is_known_global = self.is_well_known_lib_type_name(name);

            if has_type_args {
                let is_array_like_name = matches!(name, "Array" | "ReadonlyArray" | "ConcatArray");
                let type_param = self.lookup_type_parameter(name);
                if type_param.is_some() {
                    self.check_type_parameter_reference_for_computed_property(name, type_name_idx);
                    if let Some(enclosing_class) = self.ctx.enclosing_class.as_ref()
                        && self.is_in_static_class_member_context(type_name_idx)
                        && enclosing_class.type_param_names.iter().any(|n| n == name)
                    {
                        use crate::diagnostics::diagnostic_codes;
                        self.error_at_node(
                            type_name_idx,
                            "Static members cannot reference class type parameters.",
                            diagnostic_codes::STATIC_MEMBERS_CANNOT_REFERENCE_CLASS_TYPE_PARAMETERS,
                        );
                    }
                    // TS2315: Type parameters are not generic — they cannot be
                    // used with type arguments (e.g., `U<string>` where U is a
                    // type parameter).
                    if has_type_args && let Some(args) = &type_ref.type_arguments {
                        self.error_at_node_msg(
                            type_name_idx,
                            crate::diagnostics::diagnostic_codes::TYPE_IS_NOT_GENERIC,
                            &[name],
                        );
                        // Still resolve type arguments for noUnusedLocals (TS6133)
                        for &arg_idx in &args.nodes {
                            let _ = self.get_type_from_type_node(arg_idx);
                        }
                    }
                }
                let type_resolution =
                    self.resolve_identifier_symbol_in_type_position(type_name_idx);
                let sym_id = match type_resolution {
                    TypeSymbolResolution::Type(sym_id) => {
                        self.check_for_static_member_class_type_param_reference(
                            sym_id,
                            type_name_idx,
                        );
                        Some(sym_id)
                    }
                    TypeSymbolResolution::ValueOnly(_) => {
                        if let Some(target_sym_id) = self
                            .resolve_type_symbol_for_lowering(type_name_idx)
                            .map(tsz_binder::SymbolId)
                            .or_else(|| self.resolve_type_only_import_alias_target_symbol(name))
                        {
                            Some(target_sym_id)
                        } else {
                            // Route through wrong-meaning boundary: value used as type
                            use crate::query_boundaries::name_resolution::NameLookupKind;
                            self.report_wrong_meaning_diagnostic(
                                name,
                                type_name_idx,
                                NameLookupKind::Value,
                            );
                            return TypeId::ERROR;
                        }
                    }
                    TypeSymbolResolution::NotFound => None,
                };
                let sym_id = self
                    .resolve_type_symbol_for_lowering(type_name_idx)
                    .map(tsz_binder::SymbolId)
                    .or(sym_id)
                    .or_else(|| {
                        self.ctx
                            .binder
                            .file_locals
                            .get(name)
                            .filter(|&sym_id| self.symbol_has_declared_type_meaning(sym_id))
                    })
                    .or_else(|| {
                        let entries = self.ctx.global_file_locals_index.as_ref()?.get(name)?;
                        entries.iter().find_map(|&(file_idx, sym_id)| {
                            if file_idx != self.ctx.current_file_idx {
                                return None;
                            }
                            let binder = self.ctx.get_binder_for_file(file_idx)?;
                            binder
                                .get_symbol(sym_id)
                                .is_some_and(|symbol| symbol.has_any_flags(symbol_flags::TYPE))
                                .then_some(sym_id)
                        })
                    });
                let lib_binders = self.get_lib_binders();
                let resolved_symbol_matches_name = sym_id.is_some_and(|sym_id| {
                    self.ctx
                        .binder
                        .get_symbol(sym_id)
                        .or_else(|| {
                            self.ctx
                                .resolve_symbol_file_index(sym_id)
                                .and_then(|file_idx| self.ctx.get_binder_for_file(file_idx))
                                .and_then(|binder| binder.get_symbol(sym_id))
                        })
                        .or_else(|| self.get_cross_file_symbol(sym_id))
                        .or_else(|| self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders))
                        .is_some_and(|symbol| symbol.escaped_name == name)
                });
                let sym_id = if !resolved_symbol_matches_name
                    && !self.ctx.file_local_type_shadow_for_lib_name(name)
                    && self.ctx.actual_lib_context_has_bare_name(name)
                {
                    None
                } else {
                    sym_id
                };
                let is_builtin_array = is_array_like_name
                    && type_param.is_none()
                    && !(self.ctx.actual_lib_context_has_bare_name(name)
                        && self.ctx.same_file_type_declaration_exists(name))
                    && !self.ctx.file_local_type_shadow_for_lib_name(name)
                    && sym_id.is_none_or(|sym_id| self.ctx.symbol_is_from_actual_lib(sym_id));
                if !is_builtin_array
                    && is_array_like_name
                    && self.ctx.actual_lib_context_has_bare_name(name)
                    && self.ctx.same_file_type_declaration_exists(name)
                    && let Some((type_params, type_node, alias_sym_id)) =
                        self.same_file_type_alias_parts_for_name(name)
                    && let Some(args) = &type_ref.type_arguments
                {
                    let type_args = args
                        .nodes
                        .iter()
                        .map(|&arg_idx| self.get_type_from_type_node(arg_idx))
                        .collect::<Vec<_>>();
                    let (params, updates) = self.push_type_parameters(&type_params);
                    let body = self.get_type_from_type_node(type_node);
                    self.pop_type_parameters(updates);
                    if params.len() == type_args.len() {
                        if let Some(alias_sym_id) = alias_sym_id {
                            let def_id = self.ctx.get_or_create_def_id(alias_sym_id);
                            self.ctx.symbol_types.insert(alias_sym_id, body);
                            self.ctx.register_resolved_type(alias_sym_id, body, params);
                            self.ctx.clear_type_evaluation_caches_for_def(def_id);
                            let base = self.ctx.types.factory().lazy(def_id);
                            return self.ctx.types.factory().application(base, type_args);
                        }
                        return crate::query_boundaries::common::instantiate_generic(
                            self.ctx.types,
                            body,
                            &params,
                            &type_args,
                        );
                    }
                }
                if let Some(sym_id) = sym_id
                    && self.symbol_is_namespace_only(sym_id)
                {
                    self.error_namespace_used_as_type_at(name, type_name_idx);
                    return TypeId::ERROR;
                }
                // TS2318: Array<T> with noLib should emit "Cannot find global type 'Array'"
                if is_builtin_array && !has_libs && sym_id.is_none() {
                    self.error_cannot_find_global_type(name, type_name_idx);
                    // Still process type arguments to avoid cascading errors
                    if let Some(args) = &type_ref.type_arguments {
                        for &arg_idx in &args.nodes {
                            let _ = self.get_type_from_type_node(arg_idx);
                        }
                    }
                    return TypeId::ERROR;
                }

                // Canonical built-in `Array<T>` / `ReadonlyArray<T>` form: lower
                // via the solver `array` factory (and `readonly_type` for
                // ReadonlyArray) so the generic-form annotation interns to the
                // same TypeId as the shorthand `T[]` / `readonly T[]`. Without
                // this, `Array<T>` becomes `Application(Lazy(GlobalArrayDef),
                // [T])` and bidirectional identity comparisons against `T[]`
                // fail (false TS2403 on redeclarations like
                // `var a: Array<X>; var a: X[]`).
                //
                // Skipped when the name is shadowed by a user-defined type
                // alias (e.g. `type Array<T> = { custom: T };`). The existing
                // `is_builtin_array` predicate uses `symbol_is_from_actual_lib`
                // which is unreliable here — the binder often registers a
                // local proxy symbol for unshadowed lib references — so we
                // detect shadowing structurally via the resolved symbol's
                // `TYPE_ALIAS` flag instead. A locally-merged `interface
                // Array<T> { ... }` is declaration merging with the lib's
                // Array, not shadowing, so it still canonicalizes.
                //
                // ConcatArray is excluded — it's a distinct lib interface, not
                // an alias for `T[]`.
                let array_is_unshadowed = (name == "Array" || name == "ReadonlyArray")
                    && type_param.is_none()
                    && !(self.ctx.actual_lib_context_has_bare_name(name)
                        && self.ctx.same_file_type_declaration_exists(name))
                    && !self.ctx.file_local_type_shadow_for_lib_name(name)
                    && match sym_id {
                        None => true,
                        Some(sid) => {
                            use tsz_binder::symbols::symbol_flags;
                            let lib_binders = self.get_lib_binders();
                            let symbol = self.ctx.binder.get_symbol_with_libs(sid, &lib_binders);
                            !symbol.is_some_and(|s| s.has_any_flags(symbol_flags::TYPE_ALIAS))
                        }
                    };
                if array_is_unshadowed
                    && let Some(args) = &type_ref.type_arguments
                    && let Some(&first_arg) = args.nodes.first()
                {
                    // Process all type-argument nodes so their referenced
                    // symbols get registered (matching the lowering path's
                    // side effects). Only the first arg is used semantically.
                    for &arg_idx in &args.nodes {
                        let _ = self.get_type_from_type_node(arg_idx);
                    }
                    let elem_type = self.get_type_from_type_node(first_arg);
                    let factory = self.ctx.types.factory();
                    let array_type = factory.array(elem_type);
                    if name == "ReadonlyArray" {
                        return factory.readonly_type(array_type);
                    }
                    return array_type;
                }

                // Compiler-intrinsic types (NoInfer, string manipulation) must go
                // through the lowering path which creates the correct TypeData
                // variants (NoInfer, StringIntrinsic). The lib binder fallback
                // below would create generic Application(Lazy(DefId), args) which
                // can't be evaluated because intrinsic types have no body.
                let is_intrinsic_type = matches!(
                    name,
                    "NoInfer" | "Uppercase" | "Lowercase" | "Capitalize" | "Uncapitalize"
                );
                if !is_intrinsic_type
                    && !is_builtin_array
                    && type_param.is_none()
                    && sym_id.is_none()
                {
                    if let Some((body_type, type_params)) =
                        self.resolve_global_jsdoc_typedef_info(name)
                    {
                        if let Some(args) = &type_ref.type_arguments
                            && !self.is_inside_type_parameter_declaration(idx)
                        {
                            let display_name = Self::format_generic_display_name_with_interner(
                                name,
                                &type_params,
                                self.ctx.types,
                            );
                            if self.validate_jsdoc_type_reference_type_arguments_against_params(
                                &type_params,
                                args,
                                type_name_idx,
                                &display_name,
                            ) {
                                return TypeId::ERROR;
                            }

                            let type_args: Vec<TypeId> = args
                                .nodes
                                .iter()
                                .map(|&arg_idx| self.get_type_from_type_node(arg_idx))
                                .collect();
                            if !type_params.is_empty() && !type_args.is_empty() {
                                return crate::query_boundaries::common::instantiate_generic(
                                    self.ctx.types,
                                    body_type,
                                    &type_params,
                                    &type_args,
                                );
                            }
                        }
                        return body_type;
                    }
                    // Only try resolving from lib binders if lib files are loaded (noLib is false)
                    if has_libs {
                        // Try resolving from lib binders before falling back to UNKNOWN
                        // First check if the global type exists via binder's get_global_type
                        let lib_binders = self.get_lib_binders();
                        if let Some(global_sym) = self
                            .ctx
                            .binder
                            .get_global_type_with_libs(name, &lib_binders)
                        {
                            // Global type symbol exists in lib binders - try to resolve it
                            if let Some(type_id) = self.resolve_lib_type_by_name(name) {
                                // Successfully resolved - create a TypeApplication if there are type arguments
                                if let Some(args) = &type_ref.type_arguments
                                    && !args.nodes.is_empty()
                                {
                                    // Collect type argument IDs
                                    let type_args: Vec<TypeId> = args
                                        .nodes
                                        .iter()
                                        .map(|&arg_idx| self.get_type_from_type_node(arg_idx))
                                        .collect();
                                    let def_id =
                                        self.ctx.get_canonical_lib_def_id(name, global_sym);
                                    let base = self.ctx.types.factory().lazy(def_id);
                                    return self.ctx.types.factory().application(base, type_args);
                                }
                                return type_id;
                            }
                            // Symbol exists but failed to resolve - this is an error condition
                            // The type is declared but we couldn't get its TypeId, which shouldn't happen
                            // Fall through to emit error below
                        }
                        // Fall back to resolve_lib_type_by_name for cases where type may exist
                        // but get_global_type_with_libs doesn't find it
                        if let Some(type_id) = self.resolve_lib_type_by_name(name) {
                            // Successfully resolved via alternate path - create TypeApplication if there are type arguments
                            if let Some(args) = &type_ref.type_arguments
                                && !args.nodes.is_empty()
                            {
                                // Collect type argument IDs
                                let type_args: Vec<TypeId> = args
                                    .nodes
                                    .iter()
                                    .map(|&arg_idx| self.get_type_from_type_node(arg_idx))
                                    .collect();
                                // Create a TypeApplication to instantiate the generic type
                                return self.ctx.types.factory().application(type_id, type_args);
                            }
                            return type_id;
                        }
                    }
                    // When has_lib_loaded() is false (noLib is true), the above block is skipped
                    // and falls through to the well-known-lib-type recovery path below.
                    if is_known_global {
                        return self.handle_missing_global_type_with_args(
                            name,
                            type_ref,
                            type_name_idx,
                        );
                    }
                    if name == "await" {
                        self.error_cannot_find_name_did_you_mean_at(name, "Awaited", type_name_idx);
                        return TypeId::ERROR;
                    }
                    // Suppress TS2304 if this is an unresolved import (TS2307 was already emitted)
                    if self.is_unresolved_import_symbol(type_name_idx) {
                        return TypeId::ANY;
                    }
                    // Route through boundary for TS2304/TS2552 with spelling suggestions
                    let _ = self.resolve_type_name_or_report(name, type_name_idx);
                    // Process type arguments to emit TS2304 for nested unresolved types.
                    // E.g., `Foo<Bar<T>>` should emit TS2304 for Foo, Bar, and T.
                    if let Some(args) = &type_ref.type_arguments {
                        for &arg_idx in &args.nodes {
                            let _ = self.get_type_from_type_node(arg_idx);
                        }
                    }
                    return TypeId::ERROR;
                }
                if !is_builtin_array
                    && let Some(sym_id) = sym_id
                    && let Some(args) = &type_ref.type_arguments
                    && self.should_resolve_recursive_type_alias(sym_id, args)
                {
                    // Ensure the base type symbol is resolved first so its type params
                    // are available in the type_env for Application expansion
                    let _ = self.get_type_of_symbol(sym_id);
                }

                // Check for unresolved import before creating TypeApplication
                // This prevents creating TypeApplication(error<T>) which causes cascading errors
                if !is_builtin_array
                    && sym_id.is_some()
                    && self.is_unresolved_import_symbol(type_name_idx)
                {
                    return TypeId::ERROR;
                }

                // Also ensure type arguments are resolved and in type_env
                // This is needed so that when we evaluate the Application, we can
                // resolve Ref types in the arguments
                if let Some(args) = &type_ref.type_arguments {
                    for &arg_idx in &args.nodes {
                        // Recursively get type from the arg - this will add any referenced
                        // symbols to type_env
                        let _ = self.get_type_from_type_node(arg_idx);
                    }
                    // Validate type arguments against constraints (TS2344)
                    // Skip validation inside type parameter declarations (constraints/defaults)
                    if !is_builtin_array
                        && !self.is_inside_type_parameter_declaration(idx)
                        && let Some(sym_id) = sym_id
                        && self.validate_type_reference_type_arguments(sym_id, args, idx)
                    {
                        // Wrong number of type arguments (TS2314/TS2707).
                        // Return ERROR to match tsc's errorType propagation and
                        // prevent cascading diagnostics (e.g., false TS2322 on
                        // return statements whose return type has bad arg count).
                        return TypeId::ERROR;
                    }
                }
                if matches!(
                    name,
                    "Uppercase" | "Lowercase" | "Capitalize" | "Uncapitalize"
                ) && !is_builtin_array
                    && self.ctx.file_local_type_shadow_for_lib_name(name)
                    && self.ctx.same_file_type_declaration_exists(name)
                    && let Some(sym_id) = sym_id
                {
                    self.ensure_def_ready_for_lowering(sym_id, name);
                    let type_args = type_ref
                        .type_arguments
                        .as_ref()
                        .map(|args| {
                            args.nodes
                                .iter()
                                .map(|&arg_idx| self.get_type_from_type_node(arg_idx))
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    let def_id = self
                        .resolve_def_id_for_lowering(type_name_idx)
                        .unwrap_or_else(|| self.ctx.get_or_create_def_id(sym_id));
                    let base = self.ctx.types.factory().lazy(def_id);
                    return if type_args.is_empty() {
                        base
                    } else {
                        self.ctx.types.factory().application(base, type_args)
                    };
                }
                if name == "Readonly"
                    && !is_intrinsic_type
                    && !is_builtin_array
                    && type_param.is_none()
                    && !self.ctx.file_local_type_shadow_for_lib_name(name)
                    && self.ctx.actual_lib_def_id_for_bare_name(name).is_some()
                    && let Some(args) = &type_ref.type_arguments
                    && let Some(&arg_idx) = args.nodes.first()
                {
                    let arg_type = self.get_type_from_type_node(arg_idx);
                    let resolved_arg = self.evaluate_type_with_resolution(arg_type);
                    let array_like =
                        crate::query_boundaries::type_checking_utilities::classify_array_like(
                            self.ctx.types,
                            resolved_arg,
                        );
                    if matches!(
                        array_like,
                        crate::query_boundaries::common::ArrayLikeKind::Array(_)
                            | crate::query_boundaries::common::ArrayLikeKind::Tuple
                            | crate::query_boundaries::common::ArrayLikeKind::Readonly(_)
                    ) {
                        return self.ctx.types.factory().readonly_type(resolved_arg);
                    }
                }
                if !is_builtin_array && let Some(sym_id) = sym_id {
                    // Generic user-defined references lower to Application(Lazy(def), args).
                    // Ensure the base symbol has already materialized its structural
                    // body in the type environment before we hand the Application to
                    // later inference/evaluation paths.
                    let _ = self.type_reference_symbol_type(sym_id);
                }
                // Ensure the symbol's DefId has type params cached and body
                // registered so the Solver can expand Application(Lazy(DefId), Args).
                if let Some(sym_id) = sym_id {
                    self.ensure_def_ready_for_lowering(sym_id, name);
                }
                let type_param_bindings = self.get_type_param_bindings();
                let type_resolver =
                    |node_idx: NodeIndex| self.resolve_type_symbol_for_lowering(node_idx);
                // Stable-identity helper: prefer Lazy(DefId) over Ref(SymbolRef)
                let def_id_resolver =
                    |node_idx: NodeIndex| self.resolve_def_id_for_lowering(node_idx);
                let value_resolver =
                    |node_idx: NodeIndex| self.resolve_value_symbol_for_lowering(node_idx);
                let lazy_type_params_resolver =
                    |def_id: tsz_solver::def::DefId| self.ctx.get_def_type_params(def_id);
                // Name-based DefId fallback (see sibling lowering above for
                // rationale).
                let name_resolver = |type_name: &str| -> Option<tsz_solver::def::DefId> {
                    (!self.ctx.file_local_type_shadow_for_lib_name(type_name))
                        .then(|| self.resolve_actual_lib_name_to_def_id_for_lowering(type_name))
                        .flatten()
                        .or_else(|| self.resolve_entity_name_text_to_def_id_for_lowering(type_name))
                        .or_else(|| {
                            crate::types_domain::queries::lib_resolution::resolve_name_to_lib_symbol(
                                type_name,
                                self.ctx.binder,
                                self.ctx.global_file_locals_index.as_deref(),
                                self.ctx
                                    .all_binders
                                    .as_ref()
                                    .map(|binders| binders.as_ref().as_slice()),
                                &self.ctx.lib_contexts,
                            )
                            .map(|sym_id| self.ctx.get_canonical_lib_def_id(type_name, sym_id))
                        })
                };
                let type_query_override = |expr_name_idx: NodeIndex| -> Option<TypeId> {
                    let type_query_idx = self.ctx.arena.get_extended(expr_name_idx)?.parent;
                    let type_query_node = self.ctx.arena.get(type_query_idx)?;
                    if type_query_node.kind != syntax_kind_ext::TYPE_QUERY {
                        return None;
                    }
                    self.ctx
                        .node_types
                        .get(&type_query_idx.0)
                        .copied()
                        .filter(|&type_id| type_id != TypeId::ANY && type_id != TypeId::ERROR)
                };
                let lowering = tsz_lowering::TypeLowering::with_hybrid_resolver(
                    self.ctx.arena,
                    self.ctx.types,
                    &type_resolver,
                    &def_id_resolver,
                    &value_resolver,
                )
                .with_type_param_bindings(type_param_bindings)
                .with_lazy_type_params_resolver(&lazy_type_params_resolver)
                .with_name_def_id_resolver(&name_resolver)
                .with_type_query_override(&type_query_override);
                let mut result = lowering.lower_type(idx);
                if let Some((base, app_args)) = query::get_application_info(self.ctx.types, result)
                    && !is_builtin_array
                    && query::get_lazy_def_id(self.ctx.types, base).is_none()
                    && let Some(sym_id) = sym_id
                {
                    let def_id = self
                        .resolve_def_id_for_lowering(type_name_idx)
                        .unwrap_or_else(|| self.ctx.get_or_create_def_id(sym_id));
                    let lazy_base = self.ctx.types.factory().lazy(def_id);
                    result = self
                        .ctx
                        .types
                        .factory()
                        .application(lazy_base, app_args.to_vec());
                }

                // Ensure Application types from lib types have their base DefId
                // fully registered (body + params) in BOTH type environments.
                // NarrowingContext (used for flow analysis) needs both body and params
                // to instantiate generics like ArrayLike<any> during narrowing.
                // type_env and type_environment are separate TypeEnvironment instances:
                // type_env is the working copy modified during type resolution,
                // type_environment is the snapshot used by FlowAnalyzer.
                if let Some((app_base, _app_args)) =
                    query::get_application_info(self.ctx.types, result)
                    && let Some(app_def_id) = query::get_lazy_def_id(self.ctx.types, app_base)
                    && !self.ctx.lib_contexts.is_empty()
                {
                    // Check if body+params are fully registered in type_environment
                    // (the one used by FlowAnalyzer/NarrowingContext)
                    let needs_flow_env_fix = self
                        .ctx
                        .type_environment
                        .try_borrow()
                        .map(|env| {
                            env.get_def(app_def_id).is_none()
                                || env.get_def_params(app_def_id).is_none()
                        })
                        .unwrap_or(false);

                    if needs_flow_env_fix {
                        // Try to get body and params. The body may already be in type_env
                        // (registered by another path), but params might only be in
                        // CheckerContext's def_type_params storage.
                        let body = self
                            .ctx
                            .type_env
                            .try_borrow()
                            .ok()
                            .and_then(|env| env.get_def(app_def_id))
                            .or_else(|| {
                                // Fallback: re-resolve the lib type
                                self.resolve_lib_type_by_name(name).and_then(|_| {
                                    self.ctx
                                        .type_env
                                        .try_borrow()
                                        .ok()
                                        .and_then(|env| env.get_def(app_def_id))
                                })
                            });
                        let params = self
                            .ctx
                            .type_env
                            .try_borrow()
                            .ok()
                            .and_then(|env| env.get_def_params(app_def_id).map(|s| s.to_vec()))
                            .or_else(|| self.ctx.get_def_type_params(app_def_id));

                        if let (Some(body), Some(params)) = (body, params) {
                            // Register in both envs so evaluator and flow
                            // analyzer see the same Application body + params.
                            self.ctx
                                .register_def_with_params_in_envs(app_def_id, body, params);
                        }
                    }
                }

                // Eagerly evaluate type alias applications to detect TS2589
                // (excessive instantiation depth). Without this, the application
                // stays lazy and deep recursion is never detected.
                // Skip when args contain type parameters — the alias body
                // may be self-referential (recursive conditional), and eager
                // evaluation with unresolved params causes false TS2589.
                if let Some(sym_id) = sym_id {
                    let lib_binders = self.get_lib_binders();
                    let is_type_alias = self
                        .ctx
                        .binder
                        .get_symbol_with_libs(sym_id, &lib_binders)
                        .is_some_and(|s| s.has_any_flags(symbol_flags::TYPE_ALIAS));
                    if is_type_alias {
                        let args_have_type_params =
                            type_ref.type_arguments.as_ref().is_some_and(|args| {
                                self.type_arg_nodes_contain_scoped_type_parameter_for_depth_check(
                                    args,
                                )
                            }) || query::get_application_info(self.ctx.types, result).is_some_and(
                                |(_, args)| {
                                    args.iter().any(|&arg| {
                                        query::contains_type_parameters(self.ctx.types, arg)
                                    })
                                },
                            );
                        if !args_have_type_params {
                            // Reset depth_exceeded before evaluation so we detect fresh depth exceedance
                            self.ctx.depth_exceeded.set(false);
                            // Use the regular evaluator for ordinary type-reference
                            // probes. The TS2589-specific evaluator treats any repeated
                            // Application cycle as overflow, which is too aggressive for
                            // bounded recursive conditional aliases.
                            let exceeded = {
                                let _ = self.evaluate_type_with_env_uncached(result);
                                self.ctx.depth_exceeded.get()
                            };

                            // TS2589: emit at the type reference node if depth was exceeded

                            // Also detect circular mapped-type aliases that the evaluator
                            // can't expand: if the alias body is a mapped type that
                            // references itself in the template (e.g.,
                            // `type Circular<T> = {[P in keyof T]: Circular<T>}`),
                            // any concrete instantiation is infinitely recursive.
                            // Check unconditionally (even when exceeded) so we can
                            // emit TS2615 alongside TS2589.
                            let application_alias_symbol =
                                query::get_application_info(self.ctx.types, result)
                                    .and_then(|(base, _)| {
                                        query::get_lazy_def_id(self.ctx.types, base)
                                    })
                                    .and_then(|def_id| self.ctx.def_to_symbol_id(def_id));
                            let circular_mapped = application_alias_symbol.is_some_and(|ref_sym| {
                                // The base is a type alias whose body is a mapped
                                // type that references itself in its template
                                self.ctx.binder.get_symbol(ref_sym).is_some_and(|symbol| {
                                    symbol.has_any_flags(symbol_flags::TYPE_ALIAS)
                                        && symbol.declarations.iter().any(|&decl_idx| {
                                            self.alias_has_self_referencing_mapped_body(
                                                ref_sym, decl_idx,
                                            )
                                        })
                                })
                            });
                            let tuple_spread_alias =
                                application_alias_symbol.is_some_and(|ref_sym| {
                                    self.type_alias_is_unconditional_tuple_spread(ref_sym)
                                });

                            if exceeded || circular_mapped {
                                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                                let (message, code) = if exceeded && tuple_spread_alias {
                                    (
                                        diagnostic_messages::TYPE_PRODUCES_A_TUPLE_TYPE_THAT_IS_TOO_LARGE_TO_REPRESENT,
                                        diagnostic_codes::TYPE_PRODUCES_A_TUPLE_TYPE_THAT_IS_TOO_LARGE_TO_REPRESENT,
                                    )
                                } else {
                                    (
                                        diagnostic_messages::TYPE_INSTANTIATION_IS_EXCESSIVELY_DEEP_AND_POSSIBLY_INFINITE,
                                        diagnostic_codes::TYPE_INSTANTIATION_IS_EXCESSIVELY_DEEP_AND_POSSIBLY_INFINITE,
                                    )
                                };
                                self.error_at_node(idx, message, code);

                                // TS2615: When a circular mapped type is involved,
                                // also emit the property-circularity diagnostic.
                                if circular_mapped {
                                    self.emit_ts2615_for_circular_mapped_type(idx, result);
                                }

                                // tsc returns `any` for excessively deep types to
                                // suppress cascading errors (e.g., TS2322).
                                result = TypeId::ANY;
                            }
                        }
                    }
                }

                // Apply module augmentations to generic type references.
                // For types like Observable<number>, the Application hasn't been
                // evaluated yet. We augment the DefId's body in type_env so that
                // when the solver evaluates the Application, the augmented members
                // (e.g., map() from `declare module "./observable"`) are included.
                if let Some(sym_id) = sym_id {
                    let lib_binders = self.get_lib_binders();
                    let imported_module = self
                        .ctx
                        .binder
                        .get_symbol_with_libs(sym_id, &lib_binders)
                        .and_then(|symbol| {
                            symbol.import_module.as_ref().map(|module_specifier| {
                                (
                                    module_specifier.clone(),
                                    symbol
                                        .import_name
                                        .clone()
                                        .unwrap_or_else(|| symbol.escaped_name.clone()),
                                )
                            })
                        })
                        .or_else(|| {
                            self.resolve_named_import_module_for_local_name(name)
                                .map(|module_specifier| (module_specifier, name.to_string()))
                        });
                    if let Some((module_specifier, aug_name)) = imported_module {
                        // Get the Application's base DefId and augment its body
                        if let Some((app_base, _)) =
                            query::get_application_info(self.ctx.types, result)
                            && let Some(base_def_id) =
                                query::get_lazy_def_id(self.ctx.types, app_base)
                        {
                            let base_sym_id = self.ctx.def_to_symbol_id_with_fallback(base_def_id);
                            let target_class_sym_id = base_sym_id
                                .filter(|&candidate_sym_id| {
                                    self.ctx
                                        .binder
                                        .get_symbol_with_libs(candidate_sym_id, &lib_binders)
                                        .is_some_and(|base_symbol| {
                                            base_symbol.has_any_flags(symbol_flags::CLASS)
                                        })
                                })
                                .or_else(|| {
                                    self.resolve_cross_file_export(&module_specifier, &aug_name)
                                        .or_else(|| {
                                            self.ctx
                                                .binder
                                                .module_exports
                                                .get(&module_specifier)
                                                .and_then(|exports| exports.get(&aug_name))
                                        })
                                        .filter(|&candidate_sym_id| {
                                            self.ctx
                                                .binder
                                                .get_symbol_with_libs(
                                                    candidate_sym_id,
                                                    &lib_binders,
                                                )
                                                .is_some_and(|base_symbol| {
                                                    base_symbol.has_any_flags(symbol_flags::CLASS)
                                                })
                                        })
                                });
                            let base_is_class = target_class_sym_id.is_some();
                            // Try to get the body from type_env (interface) or
                            // class_instance_types (class). If the environment has
                            // not been primed for an imported class yet, recover the
                            // class instance type directly from the target symbol.
                            let body = self
                                .ctx
                                .type_env
                                .try_borrow()
                                .ok()
                                .and_then(|env| {
                                    let def = env.get_def(base_def_id);
                                    let inst = env.get_class_instance_type(base_def_id);
                                    def.or(inst)
                                })
                                .or_else(|| {
                                    if base_is_class {
                                        target_class_sym_id.and_then(|candidate_sym_id| {
                                            self.class_instance_type_from_symbol(candidate_sym_id)
                                        })
                                    } else {
                                        None
                                    }
                                });
                            if let Some(body) = body {
                                let augmented = self.apply_module_augmentations(
                                    &module_specifier,
                                    &aug_name,
                                    body,
                                );
                                if augmented != body {
                                    // Update both envs so evaluator and flow
                                    // analyzer see the augmented type.
                                    self.ctx.register_augmented_def_in_envs(
                                        base_def_id,
                                        augmented,
                                        base_is_class,
                                    );
                                }
                            }
                        }
                        let has_same_arena_augmentation = self
                            .get_module_augmentation_declarations(&module_specifier, &aug_name)
                            .iter()
                            .any(|augmentation| {
                                augmentation.arena.as_ref().is_none_or(|arena| {
                                    std::ptr::eq(arena.as_ref(), self.ctx.arena)
                                })
                            });
                        if !has_same_arena_augmentation {
                            if let Some((_, app_args)) =
                                query::get_application_info(self.ctx.types, result)
                            {
                                let augmentation_members = self
                                    .get_module_augmentation_members_instantiated(
                                        &module_specifier,
                                        &aug_name,
                                        &app_args,
                                    );
                                if !augmentation_members.is_empty() {
                                    let aug_object =
                                        self.ctx.types.factory().object(augmentation_members);
                                    result =
                                        self.ctx.types.factory().intersection2(result, aug_object);
                                }
                            } else {
                                result = self.apply_module_augmentations(
                                    &module_specifier,
                                    &aug_name,
                                    result,
                                );
                            }
                        }
                    }
                }

                return result;
            }

            // Handle Array/ReadonlyArray/ConcatArray without type arguments
            if name == "Array" || name == "ReadonlyArray" || name == "ConcatArray" {
                // TS2314: array-like built-ins require a type argument
                // Skip in heritage clauses: `class C extends Array {}` is valid
                if !self.is_direct_heritage_type_reference(idx) {
                    // tsc displays the type name with its type parameters: Array<T>
                    let display_name = format!("{name}<T>");
                    self.error_generic_type_requires_type_arguments_at(&display_name, 1, idx);
                    // Return ERROR to prevent cascading assignment errors (TS2322)
                    // when using Array without type arguments
                    return TypeId::ERROR;
                }
                return self.resolve_array_type_reference(name, type_name_idx, type_ref);
            }

            // Built-in primitive keywords
            if let Some(builtin) = Self::resolve_primitive_keyword(name) {
                return builtin;
            }

            // Type parameter (generic like T in function<T>)
            if let Some(type_param) = self.lookup_type_parameter(name) {
                self.check_type_parameter_reference_for_computed_property(name, type_name_idx);
                // TS1212/TS1213/TS1214: Strict-mode reserved word used as type reference
                if crate::state_checking::is_strict_mode_reserved_name(name)
                    && self.is_strict_mode_for_node(type_name_idx)
                {
                    self.emit_strict_mode_reserved_word_error(type_name_idx, name, true);
                }
                if let Some(enclosing_class) = self.ctx.enclosing_class.as_ref()
                    && self.is_in_static_class_member_context(type_name_idx)
                    && enclosing_class.type_param_names.iter().any(|n| n == name)
                    && !self
                        .type_parameter_name_is_shadowed_before_static_member(name, type_name_idx)
                {
                    use crate::diagnostics::diagnostic_codes;
                    self.error_at_node(
                        type_name_idx,
                        "Static members cannot reference class type parameters.",
                        diagnostic_codes::STATIC_MEMBERS_CANNOT_REFERENCE_CLASS_TYPE_PARAMETERS,
                    );
                }
                return type_param;
            }

            // TS1212/TS1213/TS1214: Strict-mode reserved word used as type reference
            // (even when it doesn't resolve to a type parameter).
            // Use AST walk for class context detection because `enclosing_class` may
            // not be set during lazy type resolution, leading to TS1212 (general) when
            // TS1213 (class-specific) is correct.
            if crate::state_checking::is_strict_mode_reserved_name(name)
                && self.is_strict_mode_for_node(type_name_idx)
            {
                self.emit_strict_mode_reserved_word_error_with_ast_walk(type_name_idx, name);
            }

            // Named type without type arguments — check generics, apply defaults
            return self.resolve_simple_type_reference(idx, type_name_idx, name, type_ref);
        }

        // Unknown type name node kind - propagate error
        TypeId::ERROR
    }

    pub(crate) fn handle_missing_global_type_with_args(
        &mut self,
        name: &str,
        type_ref: &tsz_parser::parser::node::TypeRefData,
        type_name_idx: NodeIndex,
    ) -> TypeId {
        if self.is_mapped_type_utility(name) {
            if self.ctx.compiler_options.no_lib {
                self.report_missing_lib_type_name(name, type_name_idx);
                return TypeId::ANY;
            }

            if let Some(args) = &type_ref.type_arguments {
                let type_args: Vec<TypeId> = args
                    .nodes
                    .iter()
                    .map(|&arg_idx| self.get_type_from_type_node(arg_idx))
                    .collect();

                if name == "Pick" && type_args.len() == 2 {
                    let factory = self.ctx.types.factory();
                    let key_param = tsz_solver::TypeParamInfo {
                        name: self.ctx.types.intern_string("__pick_key"),
                        constraint: None,
                        default: None,
                        is_const: false,
                    };
                    let key_type = self.ctx.types.type_param(key_param);
                    return factory.mapped(tsz_solver::MappedType {
                        type_param: key_param,
                        constraint: type_args[1],
                        name_type: None,
                        template: factory.index_access(type_args[0], key_type),
                        readonly_modifier: None,
                        optional_modifier: None,
                    });
                }

                let (base_type, _) = self.resolve_lib_type_with_params(name);
                if let Some(base_type) = base_type {
                    return self.ctx.types.factory().application(base_type, type_args);
                }
            }
            return TypeId::ANY;
        }

        self.report_missing_lib_type_name(name, type_name_idx);

        if !self.ctx.compiler_options.no_lib
            && self.is_promise_like_name(name)
            && let Some(args) = &type_ref.type_arguments
        {
            let type_args: Vec<TypeId> = args
                .nodes
                .iter()
                .map(|&arg_idx| self.get_type_from_type_node(arg_idx))
                .collect();
            if !type_args.is_empty() {
                let promise_base = {
                    let lib_binders = self.get_lib_binders();
                    crate::types_domain::queries::lib_resolution::resolve_name_to_lib_symbol(
                        name,
                        self.ctx.binder,
                        self.ctx.global_file_locals_index.as_deref(),
                        self.ctx
                            .all_binders
                            .as_ref()
                            .map(|binders| binders.as_ref().as_slice()),
                        &self.ctx.lib_contexts,
                    )
                    .or_else(|| {
                        lib_binders
                            .iter()
                            .find_map(|binder| binder.file_locals.get(name))
                    })
                    .map(|sym_id| {
                        let _ = self.resolve_lib_type_by_name(name);
                        let def_id = self.ctx.get_canonical_lib_def_id(name, sym_id);
                        self.ctx.types.factory().lazy(def_id)
                    })
                    .unwrap_or(TypeId::PROMISE_BASE)
                };
                return self
                    .ctx
                    .types
                    .factory()
                    .application(promise_base, type_args);
            }
        }

        if let Some(args) = &type_ref.type_arguments {
            for &arg_idx in &args.nodes {
                let _ = self.get_type_from_type_node(arg_idx);
            }
        }
        TypeId::ERROR
    }

    /// Resolve a primitive keyword like `number`, `string`, etc.
    fn resolve_primitive_keyword(name: &str) -> Option<TypeId> {
        match name {
            "number" => Some(TypeId::NUMBER),
            "string" => Some(TypeId::STRING),
            "boolean" => Some(TypeId::BOOLEAN),
            "void" => Some(TypeId::VOID),
            "any" => Some(TypeId::ANY),
            "never" => Some(TypeId::NEVER),
            "unknown" => Some(TypeId::UNKNOWN),
            "undefined" => Some(TypeId::UNDEFINED),
            "null" => Some(TypeId::NULL),
            "object" => Some(TypeId::OBJECT),
            "bigint" => Some(TypeId::BIGINT),
            "symbol" => Some(TypeId::SYMBOL),
            _ => None,
        }
    }
}
