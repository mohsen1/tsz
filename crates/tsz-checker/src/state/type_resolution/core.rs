//! Type reference resolution: interfaces, type aliases, and type references
//! on `CheckerState`.

use crate::query_boundaries::state::type_resolution as query;
use crate::state::CheckerState;
use crate::symbol_resolver::TypeSymbolResolution;
use tsz_binder::symbol_flags;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
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
                return self.check_import_type_and_resolve(call_idx, type_name_idx, idx);
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
                let lowering = tsz_lowering::TypeLowering::with_hybrid_resolver(
                    self.ctx.arena,
                    self.ctx.types,
                    &type_resolver,
                    &def_id_resolver,
                    &value_resolver,
                )
                .with_type_param_bindings(type_param_bindings);
                let type_id = lowering.lower_type(idx);

                // Eagerly evaluate type alias applications to detect TS2589
                // (excessive instantiation depth). Without this, the application
                // stays lazy and deep recursion is never detected.
                // Skip when args contain type parameters — the alias body
                // may be self-referential (recursive conditional), and eager
                // evaluation with unresolved params causes false TS2589.
                // Actual TS2589 detection happens at instantiation with concrete types.
                let lib_binders = self.get_lib_binders();
                let is_type_alias = self
                    .ctx
                    .binder
                    .get_symbol_with_libs(sym_id, &lib_binders)
                    .is_some_and(|s| s.flags & symbol_flags::TYPE_ALIAS != 0);
                if is_type_alias {
                    let args_have_type_params = query::get_application_info(
                        self.ctx.types,
                        type_id,
                    )
                    .is_some_and(|(_, args)| {
                        args.iter()
                            .any(|&arg| query::contains_type_parameters(self.ctx.types, arg))
                    });
                    if !args_have_type_params {
                        self.ctx.depth_exceeded.set(false);
                        let _ = self.evaluate_type_with_env_uncached(type_id);
                        if self.ctx.depth_exceeded.get() {
                            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                            self.error_at_node(
                                idx,
                                diagnostic_messages::TYPE_INSTANTIATION_IS_EXCESSIVELY_DEEP_AND_POSSIBLY_INFINITE,
                                diagnostic_codes::TYPE_INSTANTIATION_IS_EXCESSIVELY_DEEP_AND_POSSIBLY_INFINITE,
                            );
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
                let required_count = self.count_required_type_params(sym_id);
                if required_count > 0 {
                    // Use the resolved symbol's name (already alias-resolved by
                    // resolve_qualified_symbol_in_type_position).
                    let name = self
                        .get_symbol_globally(sym_id)
                        .map(|s| s.escaped_name.clone())
                        .or_else(|| self.entity_name_text(type_name_idx))
                        .unwrap_or_else(|| "<unknown>".to_string());
                    // tsc displays type name with param names: Foo<T, U>
                    let type_params = self.get_type_params_for_symbol(sym_id);
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
                if let Some(qn) = self
                    .ctx
                    .arena
                    .get(type_name_idx)
                    .and_then(|n| self.ctx.arena.get_qualified_name(n))
                    && let Some(right_node) = self.ctx.arena.get(qn.right)
                    && let Some(right_ident) = self.ctx.arena.get_identifier(right_node)
                    && let Some(left_node) = self.ctx.arena.get(qn.left)
                    && left_node.kind == SyntaxKind::Identifier as u16
                    && let Some(left_sym_id) =
                        self.resolve_identifier_symbol_as_qualified_type_anchor(qn.left)
                {
                    let lib_binders = self.get_lib_binders();
                    if let Some(left_symbol) = self
                        .ctx
                        .binder
                        .get_symbol_with_libs(left_sym_id, &lib_binders)
                        && let Some(module_specifier) = left_symbol.import_module.as_ref()
                    {
                        result = self.apply_module_augmentations(
                            module_specifier,
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
            let is_known_global = self.is_known_global_type_name(name);

            if has_type_args {
                let is_builtin_array =
                    name == "Array" || name == "ReadonlyArray" || name == "ConcatArray";
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
                        // Route through wrong-meaning boundary: value used as type
                        use crate::query_boundaries::name_resolution::NameLookupKind;
                        self.report_wrong_meaning_diagnostic(
                            name,
                            type_name_idx,
                            NameLookupKind::Value,
                        );
                        return TypeId::ERROR;
                    }
                    TypeSymbolResolution::NotFound => None,
                };
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
                    // Only try resolving from lib binders if lib files are loaded (noLib is false)
                    if has_libs {
                        // Try resolving from lib binders before falling back to UNKNOWN
                        // First check if the global type exists via binder's get_global_type
                        let lib_binders = self.get_lib_binders();
                        if let Some(_global_sym) = self
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
                                    // Create a TypeApplication to instantiate the generic type
                                    return self
                                        .ctx
                                        .types
                                        .factory()
                                        .application(type_id, type_args);
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
                    // and falls through to the is_known_global_type_name check below,
                    // which emits TS2318 via error_cannot_find_global_type
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
                let lowering = tsz_lowering::TypeLowering::with_hybrid_resolver(
                    self.ctx.arena,
                    self.ctx.types,
                    &type_resolver,
                    &def_id_resolver,
                    &value_resolver,
                )
                .with_type_param_bindings(type_param_bindings);
                let mut result = lowering.lower_type(idx);

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
                        .is_some_and(|s| s.flags & symbol_flags::TYPE_ALIAS != 0);
                    if is_type_alias {
                        let args_have_type_params = query::get_application_info(
                            self.ctx.types,
                            result,
                        )
                        .is_some_and(|(_, args)| {
                            args.iter()
                                .any(|&arg| query::contains_type_parameters(self.ctx.types, arg))
                        });
                        if !args_have_type_params {
                            // Reset depth_exceeded before evaluation so we detect fresh depth exceedance
                            self.ctx.depth_exceeded.set(false);
                            let _ = self.evaluate_type_with_env_uncached(result);

                            // TS2589: emit at the type reference node if depth was exceeded
                            let exceeded = self.ctx.depth_exceeded.get();

                            // Also detect circular mapped-type aliases that the evaluator
                            // can't expand: if the alias body is a mapped type that
                            // references itself in the template (e.g.,
                            // `type Circular<T> = {[P in keyof T]: Circular<T>}`),
                            // any concrete instantiation is infinitely recursive.
                            let circular_mapped = !exceeded
                                && query::get_application_info(self.ctx.types, result)
                                    .and_then(|(base, _)| {
                                        query::get_lazy_def_id(self.ctx.types, base)
                                    })
                                    .and_then(|def_id| {
                                        self.ctx.def_to_symbol.borrow().get(&def_id).copied()
                                    })
                                    .is_some_and(|ref_sym| {
                                        // The base is a type alias whose body is a mapped
                                        // type that references itself in its template
                                        self.ctx.binder.get_symbol(ref_sym).is_some_and(|symbol| {
                                            symbol.flags & symbol_flags::TYPE_ALIAS != 0
                                                && symbol.declarations.iter().any(|&decl_idx| {
                                                    self.alias_has_self_referencing_mapped_body(
                                                        ref_sym, decl_idx,
                                                    )
                                                })
                                        })
                                    });

                            if exceeded || circular_mapped {
                                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                                self.error_at_node(
                                    idx,
                                    diagnostic_messages::TYPE_INSTANTIATION_IS_EXCESSIVELY_DEEP_AND_POSSIBLY_INFINITE,
                                    diagnostic_codes::TYPE_INSTANTIATION_IS_EXCESSIVELY_DEEP_AND_POSSIBLY_INFINITE,
                                );
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
                                            base_symbol.flags & symbol_flags::CLASS != 0
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
                                                    base_symbol.flags & symbol_flags::CLASS != 0
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

                if self.ctx.has_lib_loaded() {
                    let (base_type, _) = self.resolve_lib_type_with_params(name);
                    if let Some(base_type) = base_type {
                        return self.ctx.types.factory().application(base_type, type_args);
                    }
                }
            }
            return TypeId::ANY;
        }

        self.error_cannot_find_global_type(name, type_name_idx);

        if self.is_promise_like_name(name)
            && let Some(args) = &type_ref.type_arguments
        {
            let type_args: Vec<TypeId> = args
                .nodes
                .iter()
                .map(|&arg_idx| self.get_type_from_type_node(arg_idx))
                .collect();
            if !type_args.is_empty() {
                return self
                    .ctx
                    .types
                    .factory()
                    .application(TypeId::PROMISE_BASE, type_args);
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
