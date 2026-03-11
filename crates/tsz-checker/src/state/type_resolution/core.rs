//! Type reference resolution: interfaces, type aliases, and type references
//! on `CheckerState`.

use crate::query_boundaries::state::type_resolution as query;
use crate::state::CheckerState;
use crate::symbol_resolver::TypeSymbolResolution;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::node::{NodeAccess, NodeArena};
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;
use tsz_solver::def::DefId;
use tsz_solver::is_compiler_managed_type;

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
                        self.error_value_only_type_at(&name, type_name_idx);
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
                    if !self.is_inside_type_parameter_declaration(idx) {
                        self.validate_type_reference_type_arguments(sym_id, args, idx);
                    }
                }
                let type_param_bindings = self.get_type_param_bindings();
                let type_resolver =
                    |node_idx: NodeIndex| self.resolve_type_symbol_for_lowering(node_idx);
                // Use DefId resolver to prefer Lazy(DefId) over Ref(SymbolRef)
                let def_id_resolver = |node_idx: NodeIndex| -> Option<DefId> {
                    self.resolve_type_symbol_for_lowering(node_idx)
                        .map(|sym_id| self.ctx.get_or_create_def_id(SymbolId(sym_id)))
                };
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
                        *self.ctx.depth_exceeded.borrow_mut() = false;
                        let _ = self.evaluate_type_with_env_uncached(type_id);
                        if *self.ctx.depth_exceeded.borrow() {
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

                // For simple name type refs like `import { Foo } from "./m"; type T = Foo`,
                // apply module augmentations using the import symbol's module specifier.
                if result != TypeId::ERROR {
                    let lib_binders = self.get_lib_binders();
                    if let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders)
                        && let Some(module_specifier) = symbol.import_module.as_ref()
                    {
                        let aug_name = symbol
                            .import_name
                            .as_deref()
                            .unwrap_or(&symbol.escaped_name);
                        result =
                            self.apply_module_augmentations(module_specifier, aug_name, result);
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
                        && enclosing_class.in_static_member
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
                        self.error_value_only_type_at(name, type_name_idx);
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
                    self.error_cannot_find_name_at(name, type_name_idx);
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
                    {
                        self.validate_type_reference_type_arguments(sym_id, args, idx);
                    }
                }
                // Cache type parameters for the symbol's DefId before lowering.
                // This enables the Solver to expand Application(Lazy(DefId), Args)
                // for generic interfaces like Promise<T>, Map<K,V>, Set<T>.
                if let Some(sym_id) = sym_id {
                    let def_id = self.ctx.get_or_create_def_id(sym_id);
                    if self.ctx.get_def_type_params(def_id).is_none() {
                        // Try file arena first
                        let mut found = false;
                        if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                            for &decl_idx in &symbol.declarations {
                                if let Some(node) = self.ctx.arena.get(decl_idx) {
                                    if let Some(iface) = self.ctx.arena.get_interface(node)
                                        && let Some(ref tpl) = iface.type_parameters
                                    {
                                        // Verify name matches to prevent NodeIndex collisions
                                        if let Some(iface_name_node) =
                                            self.ctx.arena.get(iface.name)
                                            && let Some(iface_ident) =
                                                self.ctx.arena.get_identifier(iface_name_node)
                                            && self.ctx.arena.resolve_identifier_text(iface_ident)
                                                != name
                                        {
                                            continue;
                                        }
                                        let (params, updates) =
                                            self.push_type_parameters(&Some(tpl.clone()));
                                        self.pop_type_parameters(updates);
                                        if !params.is_empty() {
                                            self.ctx.insert_def_type_params(def_id, params);
                                            found = true;
                                        }
                                        break;
                                    }

                                    if let Some(type_alias) = self.ctx.arena.get_type_alias(node) {
                                        // Verify name matches to prevent NodeIndex collisions
                                        if let Some(alias_name_node) =
                                            self.ctx.arena.get(type_alias.name)
                                            && let Some(alias_ident) =
                                                self.ctx.arena.get_identifier(alias_name_node)
                                            && self.ctx.arena.resolve_identifier_text(alias_ident)
                                                != name
                                        {
                                            continue;
                                        }
                                        let (params, updates) =
                                            self.push_type_parameters(&type_alias.type_parameters);
                                        self.pop_type_parameters(updates);
                                        if !params.is_empty() {
                                            self.ctx.insert_def_type_params(def_id, params);
                                            found = true;
                                        }
                                        break;
                                    }
                                }
                            }
                        }
                        // If not found in file arena, use resolve_lib_type_by_name
                        // which lowers the full interface from lib arenas and registers
                        // both the body type and type params in type_env.
                        if !found && !self.ctx.lib_contexts.is_empty() {
                            let _ = self.resolve_lib_type_by_name(name);
                        }
                    }

                    // Ensure the body type is registered in type_env for generic
                    // lib interfaces. The solver's resolve_lazy needs the body to
                    // perform property access with type parameter substitution.
                    if self.ctx.get_def_type_params(def_id).is_some()
                        && !self.ctx.lib_contexts.is_empty()
                    {
                        let has_body = self
                            .ctx
                            .type_env
                            .try_borrow()
                            .map(|env| env.get_def(def_id).is_some())
                            .unwrap_or(false);
                        if !has_body {
                            let _ = self.resolve_lib_type_by_name(name);
                        }
                    }
                }
                let type_param_bindings = self.get_type_param_bindings();
                let type_resolver =
                    |node_idx: NodeIndex| self.resolve_type_symbol_for_lowering(node_idx);
                // Use DefId resolver to prefer Lazy(DefId) over Ref(SymbolRef)
                let def_id_resolver = |node_idx: NodeIndex| -> Option<DefId> {
                    self.resolve_type_symbol_for_lowering(node_idx)
                        .map(|sym_id| self.ctx.get_or_create_def_id(SymbolId(sym_id)))
                };
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
                let result = lowering.lower_type(idx);

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
                            // Also fix type_env if it's missing params
                            if let Ok(mut env) = self.ctx.type_env.try_borrow_mut()
                                && env.get_def_params(app_def_id).is_none()
                            {
                                env.insert_def_with_params(app_def_id, body, params.clone());
                            }
                            if let Ok(mut env) = self.ctx.type_environment.try_borrow_mut() {
                                env.insert_def_with_params(app_def_id, body, params);
                            }
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
                            *self.ctx.depth_exceeded.borrow_mut() = false;
                            let _ = self.evaluate_type_with_env_uncached(result);

                            // TS2589: emit at the type reference node if depth was exceeded
                            if *self.ctx.depth_exceeded.borrow() {
                                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                                self.error_at_node(
                                    idx,
                                    diagnostic_messages::TYPE_INSTANTIATION_IS_EXCESSIVELY_DEEP_AND_POSSIBLY_INFINITE,
                                    diagnostic_codes::TYPE_INSTANTIATION_IS_EXCESSIVELY_DEEP_AND_POSSIBLY_INFINITE,
                                );
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
                    if let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders)
                        && let Some(module_specifier) = symbol.import_module.as_ref()
                    {
                        let aug_name = symbol
                            .import_name
                            .as_deref()
                            .unwrap_or(&symbol.escaped_name);
                        // Get the Application's base DefId and augment its body
                        if let Some((app_base, _)) =
                            query::get_application_info(self.ctx.types, result)
                            && let Some(base_def_id) =
                                query::get_lazy_def_id(self.ctx.types, app_base)
                        {
                            // Try to get the body from type_env (interface) or
                            // class_instance_types (class).
                            let body = self.ctx.type_env.try_borrow().ok().and_then(|env| {
                                let def = env.get_def(base_def_id);
                                let inst = env.get_class_instance_type(base_def_id);
                                def.or(inst)
                            });
                            if let Some(body) = body {
                                let augmented = self.apply_module_augmentations(
                                    module_specifier,
                                    aug_name,
                                    body,
                                );
                                if augmented != body {
                                    // Update the body in type_env so that all future
                                    // evaluations of this Application use the augmented type.
                                    if let Ok(mut env) = self.ctx.type_env.try_borrow_mut() {
                                        if env.get_class_instance_type(base_def_id).is_some() {
                                            env.insert_class_instance_type(base_def_id, augmented);
                                        } else {
                                            let params =
                                                env.get_def_params(base_def_id).map(|s| s.to_vec());
                                            if let Some(params) = params {
                                                env.insert_def_with_params(
                                                    base_def_id,
                                                    augmented,
                                                    params,
                                                );
                                            } else {
                                                env.insert_def(base_def_id, augmented);
                                            }
                                        }
                                    }
                                    // Also update type_environment (flow analyzer snapshot)
                                    if let Ok(mut env) = self.ctx.type_environment.try_borrow_mut()
                                    {
                                        if env.get_class_instance_type(base_def_id).is_some() {
                                            env.insert_class_instance_type(base_def_id, augmented);
                                        } else {
                                            let params =
                                                env.get_def_params(base_def_id).map(|s| s.to_vec());
                                            if let Some(params) = params {
                                                env.insert_def_with_params(
                                                    base_def_id,
                                                    augmented,
                                                    params,
                                                );
                                            } else {
                                                env.insert_def(base_def_id, augmented);
                                            }
                                        }
                                    }
                                }
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
                    && enclosing_class.in_static_member
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
                    let key_type = self.ctx.types.type_param(key_param.clone());
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

    /// Resolve `Array<T>`, `ReadonlyArray<T>`, or `ConcatArray<T>` without explicit type arguments.
    fn resolve_array_type_reference(
        &mut self,
        name: &str,
        type_name_idx: NodeIndex,
        type_ref: &tsz_parser::parser::node::TypeRefData,
    ) -> TypeId {
        let factory = self.ctx.types.factory();
        if let Some(type_id) = self.resolve_named_type_reference(name, type_name_idx) {
            return type_id;
        }
        if !self.ctx.has_lib_loaded() {
            self.error_cannot_find_global_type(name, type_name_idx);
            if let Some(args) = &type_ref.type_arguments {
                for &arg_idx in &args.nodes {
                    let _ = self.get_type_from_type_node(arg_idx);
                }
            }
            return TypeId::ERROR;
        }
        let elem_type = type_ref
            .type_arguments
            .as_ref()
            .and_then(|args| args.nodes.first().copied())
            .map_or(TypeId::ERROR, |idx| self.get_type_from_type_node(idx));
        let array_type = factory.array(elem_type);
        if name == "ReadonlyArray" {
            factory.readonly_type(array_type)
        } else {
            array_type
        }
    }

    /// Resolve a simple (non-array-like, non-primitive) type reference without type arguments.
    /// Handles generic validation, default type arguments, and error reporting.
    fn resolve_simple_type_reference(
        &mut self,
        idx: NodeIndex,
        type_name_idx: NodeIndex,
        name: &str,
        type_ref: &tsz_parser::parser::node::TypeRefData,
    ) -> TypeId {
        let factory = self.ctx.types.factory();
        if name != "Array" && name != "ReadonlyArray" && name != "ConcatArray" {
            match self.resolve_identifier_symbol_in_type_position(type_name_idx) {
                TypeSymbolResolution::Type(sym_id) => {
                    self.check_for_static_member_class_type_param_reference(sym_id, type_name_idx);
                    if self.ctx.has_lib_loaded() && self.ctx.symbol_is_from_lib(sym_id) {
                        self.prime_lib_type_params(name);
                    }
                    if self.symbol_is_namespace_only(sym_id) {
                        self.error_namespace_used_as_type_at(name, type_name_idx);
                        return TypeId::ERROR;
                    }
                    let mut type_params = self.get_type_params_for_symbol(sym_id);
                    if type_params.is_empty() {
                        type_params =
                            self.extract_declared_type_params_for_reference_symbol(sym_id, name);
                        if !type_params.is_empty() {
                            let def_id = self.ctx.get_or_create_def_id(sym_id);
                            self.ctx.insert_def_type_params(def_id, type_params.clone());
                        }
                    }
                    let required_count = type_params.iter().filter(|p| p.default.is_none()).count();
                    if required_count > 0 {
                        // tsc uses the original declaration name, not the local alias.
                        // e.g., `export type { A as B }` → `let d: B` reports 'A<T>', not 'B<T>'.
                        // Resolve through aliases to get the target symbol's name.
                        let resolved_name = {
                            let mut visited_aliases = Vec::new();
                            self.resolve_alias_symbol(sym_id, &mut visited_aliases)
                                .and_then(|target| {
                                    self.get_symbol_globally(target)
                                        .map(|s| s.escaped_name.clone())
                                })
                                .unwrap_or_else(|| name.to_string())
                        };
                        let display_name = Self::format_generic_display_name_with_interner(
                            &resolved_name,
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
                    // Apply default type arguments if no explicit args were provided
                    if type_ref
                        .type_arguments
                        .as_ref()
                        .is_none_or(|args| args.nodes.is_empty())
                    {
                        let has_defaults = type_params.iter().any(|p| p.default.is_some());
                        if has_defaults {
                            let default_args: Vec<TypeId> = type_params
                                .iter()
                                .map(|p| p.default.unwrap_or(TypeId::UNKNOWN))
                                .collect();
                            let def_id = self.ctx.get_or_create_def_id(sym_id);
                            // Resolve the type alias body so its type params and body
                            // are registered in type_env. Without this, Application
                            // expansion via try_expand_application fails because
                            // resolve_lazy(def_id) returns None (body not registered).
                            // This is critical for cross-file generic constraints like
                            // `TBase extends Constructor` where Constructor<T = {}>.
                            let _ = self.get_type_of_symbol(sym_id);
                            let base_type_id = factory.lazy(def_id);
                            return factory.application(base_type_id, default_args);
                        }
                    }
                }
                TypeSymbolResolution::ValueOnly(_) => {
                    self.error_value_only_type_at(name, type_name_idx);
                    return TypeId::ERROR;
                }
                TypeSymbolResolution::NotFound => {}
            }
        }

        // Create DefIds for type aliases (enables DefId-based resolution)
        if let TypeSymbolResolution::Type(sym_id) =
            self.resolve_identifier_symbol_in_type_position(type_name_idx)
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            && symbol.flags & symbol_flags::TYPE_ALIAS != 0
        {
            let _def_id = self.ctx.get_or_create_def_id(sym_id);
        }

        if let Some(type_id) = self.resolve_named_type_reference(name, type_name_idx) {
            return type_id;
        }
        if let Some(type_id) = self.resolve_global_jsdoc_typedef_type(name) {
            return type_id;
        }
        if name == "await" {
            self.error_cannot_find_name_did_you_mean_at(name, "Awaited", type_name_idx);
            return TypeId::ERROR;
        }
        if self.is_known_global_type_name(name) {
            self.error_cannot_find_global_type(name, type_name_idx);
            return TypeId::ERROR;
        }
        if self.is_unresolved_import_symbol(type_name_idx) {
            return TypeId::ANY;
        }
        self.error_cannot_find_name_at(name, type_name_idx);
        TypeId::ERROR
    }

    fn extract_declared_type_params_for_reference_symbol(
        &mut self,
        sym_id: SymbolId,
        expected_name: &str,
    ) -> Vec<tsz_solver::TypeParamInfo> {
        let Some(symbol) = self.get_symbol_globally(sym_id) else {
            return Vec::new();
        };
        let declarations = symbol.declarations.clone();

        for &decl_idx in &declarations {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };

            if let Some(type_alias) = self.ctx.arena.get_type_alias(node) {
                if let Some(name_node) = self.ctx.arena.get(type_alias.name)
                    && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                    && ident.escaped_text != expected_name
                {
                    continue;
                }
                let (params, updates) = self.push_type_parameters(&type_alias.type_parameters);
                self.pop_type_parameters(updates);
                if !params.is_empty() {
                    return params;
                }
            }

            if let Some(iface) = self.ctx.arena.get_interface(node) {
                if let Some(name_node) = self.ctx.arena.get(iface.name)
                    && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                    && ident.escaped_text != expected_name
                {
                    continue;
                }
                let (params, updates) = self.push_type_parameters(&iface.type_parameters);
                self.pop_type_parameters(updates);
                if !params.is_empty() {
                    return params;
                }
            }

            if let Some(class) = self.ctx.arena.get_class(node) {
                if let Some(name_node) = self.ctx.arena.get(class.name)
                    && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                    && ident.escaped_text != expected_name
                {
                    continue;
                }
                let (params, updates) = self.push_type_parameters(&class.type_parameters);
                self.pop_type_parameters(updates);
                if !params.is_empty() {
                    return params;
                }
            }
        }

        Vec::new()
    }

    fn symbol_is_namespace_only(&self, sym_id: SymbolId) -> bool {
        let lib_binders = self.get_lib_binders();
        if let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders) {
            if symbol.flags & symbol_flags::ALIAS != 0 {
                let mut visited = Vec::new();
                if let Some(target_sym_id) = self.resolve_alias_symbol(sym_id, &mut visited)
                    && target_sym_id != sym_id
                {
                    return self.symbol_is_namespace_only(target_sym_id);
                }
            }

            let is_namespace = (symbol.flags
                & (symbol_flags::MODULE
                    | symbol_flags::NAMESPACE_MODULE
                    | symbol_flags::VALUE_MODULE))
                != 0;
            let has_type = (symbol.flags & symbol_flags::TYPE) != 0;
            return is_namespace && !has_type;
        }
        false
    }

    pub(crate) fn should_resolve_recursive_type_alias(
        &self,
        sym_id: SymbolId,
        type_args: &tsz_parser::parser::NodeList,
    ) -> bool {
        if !self.ctx.symbol_resolution_set.contains(&sym_id) {
            return true;
        }
        if self.ctx.symbol_resolution_stack.last().copied() != Some(sym_id) {
            return true;
        }
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return true;
        };

        // Check if this is a type alias (original behavior)
        if symbol.flags & symbol_flags::TYPE_ALIAS != 0 {
            return self.type_args_match_alias_params(sym_id, type_args);
        }

        // For classes and interfaces, allow recursive references in type parameter constraints
        // Don't force eager resolution - this prevents false cycle detection for patterns like:
        // class C<T extends C<T>>
        // interface I<T extends I<T>>
        if symbol.flags & (symbol_flags::CLASS | symbol_flags::INTERFACE) != 0 {
            // Only resolve if we're not in a direct self-reference scenario
            // The symbol_resolution_stack check above handles direct recursion
            return false;
        }

        // For other symbol types, use type args matching
        self.type_args_match_alias_params(sym_id, type_args)
    }

    pub(crate) fn type_args_match_alias_params(
        &self,
        sym_id: SymbolId,
        type_args: &tsz_parser::parser::NodeList,
    ) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        if symbol.flags & symbol_flags::TYPE_ALIAS == 0 {
            return false;
        }

        let decl_idx = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else {
            symbol
                .declarations
                .first()
                .copied()
                .unwrap_or(NodeIndex::NONE)
        };
        if decl_idx.is_none() {
            return false;
        }
        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };
        let Some(type_alias) = self.ctx.arena.get_type_alias(node) else {
            return false;
        };
        let Some(type_params) = &type_alias.type_parameters else {
            return false;
        };
        if type_params.nodes.len() != type_args.nodes.len() {
            return false;
        }

        for (&param_idx, &arg_idx) in type_params.nodes.iter().zip(type_args.nodes.iter()) {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                return false;
            };
            let Some(param) = self.ctx.arena.get_type_parameter(param_node) else {
                return false;
            };
            let Some(param_name) = self
                .ctx
                .arena
                .get(param.name)
                .and_then(|node| self.ctx.arena.get_identifier(node))
                .map(|ident| ident.escaped_text.as_str())
            else {
                return false;
            };

            let Some(arg_node) = self.ctx.arena.get(arg_idx) else {
                return false;
            };
            if arg_node.kind == syntax_kind_ext::TYPE_REFERENCE {
                let Some(arg_ref) = self.ctx.arena.get_type_ref(arg_node) else {
                    return false;
                };
                if arg_ref
                    .type_arguments
                    .as_ref()
                    .is_some_and(|list| !list.nodes.is_empty())
                {
                    return false;
                }
                let Some(arg_name_node) = self.ctx.arena.get(arg_ref.type_name) else {
                    return false;
                };
                let Some(arg_ident) = self.ctx.arena.get_identifier(arg_name_node) else {
                    return false;
                };
                if arg_ident.escaped_text != param_name {
                    return false;
                }
            } else if arg_node.kind == SyntaxKind::Identifier as u16 {
                let Some(arg_ident) = self.ctx.arena.get_identifier(arg_node) else {
                    return false;
                };
                if arg_ident.escaped_text != param_name {
                    return false;
                }
            } else {
                return false;
            }
        }

        true
    }

    pub(crate) fn class_instance_type_from_symbol(&mut self, sym_id: SymbolId) -> Option<TypeId> {
        if let Some(&instance_type) = self.ctx.symbol_instance_types.get(&sym_id) {
            return Some(instance_type);
        }
        self.class_instance_type_with_params_from_symbol(sym_id)
            .map(|(instance_type, _)| instance_type)
    }

    pub(crate) fn class_instance_type_with_params_from_symbol(
        &mut self,
        sym_id: SymbolId,
    ) -> Option<(TypeId, Vec<tsz_solver::TypeParamInfo>)> {
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let decl_idx = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else {
            symbol
                .declarations
                .first()
                .copied()
                .unwrap_or(NodeIndex::NONE)
        };
        if decl_idx.is_none() {
            return None;
        }
        if let Some(class) = self.ctx.arena.get_class_at(decl_idx) {
            let canonical_sym = self.ctx.binder.get_node_symbol(decl_idx);
            let active_class_sym = canonical_sym.unwrap_or(sym_id);
            // Check if we're already resolving this class - return fallback to break cycle.
            // Return a Lazy(DefId) placeholder so that the parameter type remains
            // dynamically resolvable.  During class building the Lazy resolves to
            // the partial instance type via class_instance_type_cache; after
            // building completes it resolves to the final type via
            // symbol_instance_types.
            if self.ctx.class_instance_resolution_set.contains(&sym_id)
                || canonical_sym
                    .is_some_and(|sym| self.ctx.class_instance_resolution_set.contains(&sym))
            {
                let fallback = self.ctx.create_lazy_type_ref(active_class_sym);
                return Some((fallback, Vec::new()));
            }

            let (params, updates) = self.push_type_parameters(&class.type_parameters);
            if let Some(&instance_type) = self
                .ctx
                .symbol_instance_types
                .get(&sym_id)
                .or_else(|| self.ctx.symbol_instance_types.get(&active_class_sym))
            {
                self.pop_type_parameters(updates);
                return Some((instance_type, params));
            }

            let instance_type = self.get_class_instance_type(decl_idx, class);
            self.ctx.symbol_instance_types.insert(sym_id, instance_type);
            if active_class_sym != sym_id {
                self.ctx
                    .symbol_instance_types
                    .insert(active_class_sym, instance_type);
            }

            // Register the class instance type in the TypeEnvironment immediately
            // so that Lazy(DefId) fallbacks (created by the recursion guard above)
            // can resolve via resolve_lazy during property access checks.
            let def_id = self.ctx.get_or_create_def_id(active_class_sym);
            if let Ok(mut env) = self.ctx.type_environment.try_borrow_mut() {
                env.insert_class_instance_type(def_id, instance_type);
            }

            self.pop_type_parameters(updates);
            return Some((instance_type, params));
        }

        // Cross-file fallback: class declaration is not in the current arena.
        // Delegate to a child checker with the symbol's arena.
        self.delegate_cross_arena_class_instance_type(sym_id)
    }

    pub(crate) fn type_reference_symbol_type(&mut self, sym_id: SymbolId) -> TypeId {
        let symbol_meta = self.get_cross_file_symbol(sym_id).map(|symbol| {
            (
                symbol.escaped_name.clone(),
                symbol.flags,
                symbol.declarations.clone(),
                symbol.value_declaration,
            )
        });

        if let Some((name, flags, _, _)) = symbol_meta.as_ref() {
            tracing::debug!(
                sym_id = sym_id.0,
                name = %name,
                flags = *flags,
                "type_reference_symbol_type: ENTRY"
            );
        }
        // Recursion depth check: prevents stack overflow from circular
        // interface/class type references (e.g. I<T extends I<T>>)
        if !self.ctx.enter_recursion() {
            return TypeId::ERROR;
        }

        if let Some((escaped_name, flags, declarations, value_declaration)) = symbol_meta {
            // For classes, return Lazy(DefId) to preserve class names in error messages
            // (e.g., "type MyClass" instead of expanded object shape)
            //
            // Special case: For merged class+namespace symbols, we still need the constructor type
            // to access namespace members via Foo.Bar. But we should still return Lazy for consistency.
            if flags & symbol_flags::CLASS != 0 {
                // For classes in TYPE position, return the INSTANCE TYPE directly
                // This is critical for nominal type checking to work correctly
                let instance_type_opt = self.class_instance_type_from_symbol(sym_id);

                if let Some(instance_type) = instance_type_opt {
                    // Register instance type → DefId so the TypeFormatter can display
                    // the class name (e.g., "A") even when the type was resolved via
                    // cross-file delegation and produced a different TypeId than the
                    // original get_class_instance_type_inner call.
                    let def_id = self.ctx.get_or_create_def_id(sym_id);
                    self.ctx
                        .definition_store
                        .register_type_to_def(instance_type, def_id);

                    self.ctx.leave_recursion();
                    return instance_type;
                }

                // Fallback: if instance type couldn't be computed, return Lazy
                let lazy_type = self.ctx.create_lazy_type_ref(sym_id);
                self.ctx.leave_recursion();
                return lazy_type;
            }
            if flags & symbol_flags::INTERFACE != 0 {
                if !declarations.is_empty() {
                    // Return Lazy(DefId) for interface type references to preserve
                    // interface names in error messages. Compute and cache the structural
                    // type first so resolve_lazy() can return it for type checking.
                    // For merged interface+namespace symbols, get_type_of_symbol returns the
                    // namespace type (from compute_type_of_symbol's namespace branch). We need
                    // the interface type for type-position usage, so compute it directly from
                    // the interface declarations.
                    let is_merged_with_namespace =
                        flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE) != 0;

                    let mut structural_type = if is_merged_with_namespace {
                        // Compute the interface type directly, bypassing get_type_of_symbol
                        // which would return the namespace type for merged symbols.
                        self.compute_interface_type_from_declarations(sym_id)
                    } else {
                        self.get_type_of_symbol(sym_id)
                    };

                    // Cross-file fallback: if the structural type could not be
                    // computed locally, the declarations may be in a different
                    // arena/binder. Delegate to a child checker with the symbol's
                    // home arena instead of silently degrading imported types.
                    if (structural_type == TypeId::UNKNOWN || structural_type == TypeId::ERROR)
                        && let Some(delegate_type) =
                            self.delegate_cross_arena_interface_type(sym_id)
                    {
                        structural_type = delegate_type;
                    }

                    // Step 1.25: Apply module augmentations to the structural type.
                    // If this symbol was reached via an import alias, merge augmentation
                    // members into the base type. This ensures ALL access paths — type
                    // references, Application evaluation, and value-position prototype
                    // access — see the augmented members.
                    if structural_type != TypeId::ERROR
                        && structural_type != TypeId::UNKNOWN
                        && let Some(local_sym) = self.ctx.binder.get_symbol(sym_id)
                        && let Some(module_specifier) = local_sym.import_module.as_ref()
                    {
                        let aug_name = local_sym
                            .import_name
                            .as_deref()
                            .unwrap_or(&local_sym.escaped_name);
                        structural_type = self.apply_module_augmentations(
                            module_specifier,
                            aug_name,
                            structural_type,
                        );
                    }

                    // Step 1.5: Cache type parameters for generic interfaces (Promise<T>, Map<K,V>, etc.)
                    // This must use canonical symbol-based extraction, not raw NodeIndex lookups
                    // against the local arena. Lib and cross-file symbols can share NodeIndex values
                    // with unrelated local declarations, which corrupts cached generic metadata.
                    let def_id = self.ctx.get_or_create_def_id(sym_id);
                    if self.ctx.get_def_type_params(def_id).is_none() {
                        let params = self.get_type_params_for_symbol(sym_id);
                        if !params.is_empty() {
                            self.ctx.insert_def_type_params(def_id, params);
                        }
                    }

                    // Step 1.75: Ensure the DefId→TypeId mapping exists in the TypeEnvironment.
                    // When get_type_of_symbol hits the symbol_types cache (common for cross-file
                    // lib types like ArrayLike, Iterable, Promise), it returns early and skips
                    // the TypeEnvironment registration block. This leaves resolve_lazy(DefId)
                    // returning None, breaking Application type resolution in narrowing contexts
                    // (e.g., type predicate narrowing can't check if ArrayLike<any> is assignable
                    // to { length: unknown } because the Application can't be expanded).
                    if structural_type != TypeId::ERROR
                        && structural_type != TypeId::ANY
                        && structural_type != TypeId::UNKNOWN
                        && let Ok(mut env) = self.ctx.type_env.try_borrow_mut()
                        && env.get_def(def_id).is_none()
                    {
                        let type_params = self.ctx.get_def_type_params(def_id).unwrap_or_default();
                        if type_params.is_empty() {
                            env.insert_def(def_id, structural_type);
                        } else {
                            env.insert_def_with_params(def_id, structural_type, type_params);
                        }
                    }

                    // For merged interface+namespace symbols, return the structural type
                    // directly instead of Lazy wrapper. The Lazy wrapper causes property
                    // access to incorrectly classify the type as a namespace value,
                    // blocking interface member resolution.
                    //
                    // Also return structural type for interfaces with index signatures
                    // (ObjectWithIndex) — Lazy causes issues with flow analysis there.
                    //
                    // Also return Unknown directly when cross-file interface resolution
                    // fails — wrapping in Lazy(DefId) would create an unresolvable ref.
                    if is_merged_with_namespace
                        || query::is_object_with_index_type(self.ctx.types, structural_type)
                        || structural_type == TypeId::UNKNOWN
                    {
                        self.ctx.leave_recursion();
                        return structural_type;
                    }

                    // Return Lazy wrapper for regular interfaces
                    let lazy_type = self.ctx.create_lazy_type_ref(sym_id);
                    self.ctx.leave_recursion();
                    return lazy_type;
                }
                if value_declaration.is_some() {
                    let result = self.get_type_of_interface(value_declaration);
                    self.ctx.leave_recursion();
                    return result;
                }
            }

            // For type aliases, resolve the body type using the correct arena.
            // Search declarations[] for the actual type alias decl (merged symbols
            // may have value_declaration pointing to a var decl, not the type alias).
            if flags & symbol_flags::TYPE_ALIAS != 0 {
                let has_type_alias_decl = declarations.iter().any(|&d| {
                    self.ctx
                        .arena
                        .get(d)
                        .and_then(|n| {
                            if n.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION {
                                // Verify name matches to prevent NodeIndex collisions
                                let type_alias = self.ctx.arena.get_type_alias(n)?;
                                let name = self.ctx.arena.get_identifier_text(type_alias.name)?;
                                Some(name == escaped_name.as_str())
                            } else {
                                Some(false)
                            }
                        })
                        .unwrap_or(false)
                }) || value_declaration.is_some()
                    || !declarations.is_empty();
                if has_type_alias_decl {
                    // Return structural type directly for type aliases (not Lazy) so
                    // conditional types are fully resolved during assignability checking.
                    let structural_type = self.get_type_of_symbol(sym_id);
                    let structural_type = if structural_type != TypeId::ERROR
                        && structural_type != TypeId::UNKNOWN
                        && !tsz_solver::type_queries::contains_type_parameters_db(
                            self.ctx.types,
                            structural_type,
                        ) {
                        self.evaluate_type_with_env(structural_type)
                    } else {
                        structural_type
                    };
                    // Register for alias-name formatting in diagnostics
                    self.ctx
                        .register_resolved_type(sym_id, structural_type, Vec::new());
                    self.ctx.leave_recursion();
                    return structural_type;
                }
            }
        }
        let result = self.get_type_of_symbol(sym_id);
        // TYPE_ALIAS + ALIAS merge: prefer the type alias body in type reference position
        let result = self
            .ctx
            .import_type_alias_types
            .get(&sym_id)
            .copied()
            .unwrap_or(result);
        self.ctx.leave_recursion();
        result
    }

    /// Compute the interface structural type from declarations, bypassing `get_type_of_symbol`.
    ///
    /// For merged interface+namespace symbols, `get_type_of_symbol` returns the namespace
    /// type (via the MODULE branch in `compute_type_of_symbol`). This helper computes the
    /// interface type directly from the interface declarations, which is needed when the
    /// symbol is used in type position (e.g., `var f: Foo` where Foo is interface+namespace).
    pub(crate) fn compute_interface_type_from_declarations(&mut self, sym_id: SymbolId) -> TypeId {
        use tsz_lowering::TypeLowering;

        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return TypeId::ERROR;
        };
        let declarations = symbol.declarations.clone();

        if declarations.is_empty() {
            return TypeId::ERROR;
        }

        // Pre-compute computed property names that the lowering can't resolve from AST alone.
        // This handles cases like `[k]` where k is a `const` unique symbol variable.
        let computed_names = self.precompute_computed_property_names(&declarations);
        let prewarmed_type_params = self.prewarm_member_type_reference_params(&declarations);

        // Get type parameters from the first interface declaration
        let first_decl = declarations.first().copied().unwrap_or(NodeIndex::NONE);
        let mut params = Vec::new();
        let mut updates = Vec::new();
        if first_decl.is_some()
            && let Some(node) = self.ctx.arena.get(first_decl)
            && let Some(interface) = self.ctx.arena.get_interface(node)
        {
            (params, updates) = self.push_type_parameters(&interface.type_parameters);
        }

        let type_param_bindings = self.get_type_param_bindings();
        let type_resolver = |node_idx: NodeIndex| self.resolve_type_symbol_for_lowering(node_idx);
        let def_id_resolver = |node_idx: NodeIndex| -> Option<tsz_solver::def::DefId> {
            self.resolve_type_symbol_for_lowering(node_idx)
                .map(|sym_id_raw| {
                    self.ctx
                        .get_or_create_def_id(tsz_binder::SymbolId(sym_id_raw))
                })
        };
        let value_resolver = |node_idx: NodeIndex| self.resolve_value_symbol_for_lowering(node_idx);
        let computed_name_resolver = |expr_idx: NodeIndex| -> Option<tsz_common::Atom> {
            computed_names.get(&expr_idx).copied()
        };
        let lazy_type_params_resolver = |def_id: tsz_solver::def::DefId| {
            prewarmed_type_params
                .get(&def_id)
                .cloned()
                .or_else(|| self.ctx.get_def_type_params(def_id))
        };
        let lowering = TypeLowering::with_hybrid_resolver(
            self.ctx.arena,
            self.ctx.types,
            &type_resolver,
            &def_id_resolver,
            &value_resolver,
        )
        .with_type_param_bindings(type_param_bindings)
        .with_computed_name_resolver(&computed_name_resolver)
        .with_lazy_type_params_resolver(&lazy_type_params_resolver);
        let interface_type =
            lowering.lower_interface_declarations_with_symbol(&declarations, sym_id);

        self.pop_type_parameters(updates);
        let _ = params; // params are not needed for this path

        self.merge_interface_heritage_types(&declarations, interface_type)
    }

    pub(crate) fn prewarm_member_type_reference_params(
        &mut self,
        declarations: &[NodeIndex],
    ) -> rustc_hash::FxHashMap<tsz_solver::def::DefId, Vec<tsz_solver::TypeParamInfo>> {
        let mut stack = Vec::new();
        let mut params_by_def = rustc_hash::FxHashMap::default();

        for &decl_idx in declarations {
            stack.push(decl_idx);

            while let Some(node_idx) = stack.pop() {
                let Some(node) = self.ctx.arena.get(node_idx) else {
                    continue;
                };

                if node.kind == syntax_kind_ext::TYPE_REFERENCE
                    && let Some(type_ref) = self.ctx.arena.get_type_ref(node)
                {
                    let has_type_args = type_ref
                        .type_arguments
                        .as_ref()
                        .is_some_and(|args| !args.nodes.is_empty());
                    if !has_type_args
                        && let Some(sym_id_raw) =
                            self.resolve_type_symbol_for_lowering(type_ref.type_name)
                    {
                        let sym_id = tsz_binder::SymbolId(sym_id_raw);
                        let def_id = self.ctx.get_or_create_def_id(sym_id);
                        let params = self.get_type_params_for_symbol(sym_id);
                        if !params.is_empty() {
                            params_by_def.insert(def_id, params);
                        }
                    }
                }

                stack.extend(self.ctx.arena.get_children(node_idx));
            }
        }

        params_by_def
    }

    /// Pre-compute property names for computed property name expressions in interface members.
    /// Iterates over all members of all declarations, finds `COMPUTED_PROPERTY_NAME` nodes,
    /// evaluates the expression type, and builds a map from expression `NodeIndex` to Atom.
    pub(crate) fn precompute_computed_property_names(
        &mut self,
        declarations: &[NodeIndex],
    ) -> rustc_hash::FxHashMap<NodeIndex, tsz_common::Atom> {
        use tsz_parser::parser::syntax_kind_ext;
        let mut map = rustc_hash::FxHashMap::default();
        for &decl_idx in declarations {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let Some(interface) = self.ctx.arena.get_interface(node) else {
                continue;
            };
            for &member_idx in &interface.members.nodes {
                let Some(member) = self.ctx.arena.get(member_idx) else {
                    continue;
                };
                // Get the name node from signature or accessor
                let name_idx = if let Some(sig) = self.ctx.arena.get_signature(member) {
                    sig.name
                } else if let Some(acc) = self.ctx.arena.get_accessor(member) {
                    acc.name
                } else {
                    continue;
                };
                let Some(name_node) = self.ctx.arena.get(name_idx) else {
                    continue;
                };
                if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                    continue;
                }
                let Some(computed) = self.ctx.arena.get_computed_property(name_node) else {
                    continue;
                };
                // Set checking_computed_property_name so that TS2467 (type parameter
                // reference in computed property name) is properly emitted.
                let prev = self.ctx.checking_computed_property_name;
                self.ctx.checking_computed_property_name = Some(name_idx);
                // Preserve literal types so that string literal expressions like
                // ["computed"] resolve to the literal type "computed" rather than
                // widening to `string`. Without this, get_literal_property_name
                // cannot extract the property name from the widened type.
                let prev_preserve = self.ctx.preserve_literal_types;
                self.ctx.preserve_literal_types = true;
                // Evaluate the expression type and get the property name
                let expr_type = self.get_type_of_node(computed.expression);
                self.ctx.preserve_literal_types = prev_preserve;
                self.ctx.checking_computed_property_name = prev;
                if let Some(name) =
                    tsz_solver::type_queries::get_literal_property_name(self.ctx.types, expr_type)
                {
                    map.insert(computed.expression, name);
                }
            }
        }
        map
    }

    /// Like `type_reference_symbol_type` but also returns the type parameters used.
    ///
    /// This is critical for Application type evaluation: when instantiating a generic
    /// type, we need the body type AND the type parameters to be built from the SAME
    /// call to `push_type_parameters`, so the `TypeIds` in the body match those in the
    /// substitution. Otherwise, substitution fails because the `TypeIds` don't match.
    pub(crate) fn type_reference_symbol_type_with_params(
        &mut self,
        sym_id: SymbolId,
    ) -> (TypeId, Vec<tsz_solver::TypeParamInfo>) {
        use tsz_lowering::TypeLowering;

        if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
            tracing::debug!(
                sym_id = sym_id.0,
                name = %symbol.escaped_name,
                flags = symbol.flags,
                num_decls = symbol.declarations.len(),
                has_value_decl = symbol.value_declaration.is_some(),
                "type_reference_symbol_type_with_params: ENTRY"
            );
        }

        if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
            // For classes, use class_instance_type_with_params_from_symbol which
            // returns both the instance type AND the type params used to build it
            if symbol.flags & symbol_flags::CLASS != 0
                && let Some((instance_type, params)) =
                    self.class_instance_type_with_params_from_symbol(sym_id)
            {
                // Store type parameters for DefId-based resolution
                if let Some(def_id) = self.ctx.get_existing_def_id(sym_id) {
                    self.ctx.insert_def_type_params(def_id, params.clone());
                }
                return (instance_type, params);
            }

            // When a symbol has both TYPE_ALIAS and INTERFACE flags (e.g., local
            // `type Request<T> = ...` merged with lib's `interface Request`), the
            // local type alias should take precedence. Check whether the TYPE_ALIAS
            // declaration lives in the current arena and skip the INTERFACE path if so.
            let prefer_type_alias_over_interface = symbol.flags & symbol_flags::TYPE_ALIAS != 0
                && symbol.flags & symbol_flags::INTERFACE != 0
                && symbol.declarations.iter().any(|&d| {
                    self.ctx
                        .arena
                        .get(d)
                        .and_then(|n| {
                            if n.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION {
                                let type_alias = self.ctx.arena.get_type_alias(n)?;
                                let name = self.ctx.arena.get_identifier_text(type_alias.name)?;
                                Some(name == symbol.escaped_name.as_str())
                            } else {
                                Some(false)
                            }
                        })
                        .unwrap_or(false)
                });

            // For interfaces, lower with type parameters and return both
            if symbol.flags & symbol_flags::INTERFACE != 0
                && !symbol.declarations.is_empty()
                && !prefer_type_alias_over_interface
            {
                // Build per-declaration arena pairs for multi-arena support
                // (e.g. Promise has declarations in lib.es5.d.ts, lib.es2018.promise.d.ts, etc.)
                let fallback_arena: &NodeArena = self
                    .ctx
                    .binder
                    .symbol_arenas
                    .get(&sym_id)
                    .map_or(self.ctx.arena, |arena| arena.as_ref());

                let has_declaration_arenas = symbol.declarations.iter().any(|&decl_idx| {
                    self.ctx
                        .binder
                        .declaration_arenas
                        .contains_key(&(sym_id, decl_idx))
                });

                let decls_with_arenas: Vec<(NodeIndex, &NodeArena)> = symbol
                    .declarations
                    .iter()
                    .flat_map(|&decl_idx| {
                        if let Some(arenas) =
                            self.ctx.binder.declaration_arenas.get(&(sym_id, decl_idx))
                        {
                            arenas
                                .iter()
                                .map(|arc| (decl_idx, arc.as_ref()))
                                .collect::<Vec<_>>()
                        } else if has_declaration_arenas {
                            // This symbol has lib declarations (with declaration_arenas
                            // entries) but THIS declaration has no entry — it was added
                            // during user-file binding and lives in the user arena.
                            vec![(decl_idx, self.ctx.arena)]
                        } else {
                            vec![(decl_idx, fallback_arena)]
                        }
                    })
                    .collect();

                // Get type parameters from first declaration that has them,
                // along with the arena they came from (needed for lib interfaces).
                let type_params_with_arena: Option<(tsz_parser::parser::NodeList, &NodeArena)> =
                    decls_with_arenas.iter().find_map(|(decl_idx, arena)| {
                        arena
                            .get(*decl_idx)
                            .and_then(|node| arena.get_interface(node))
                            .and_then(|iface| {
                                iface.type_parameters.clone().map(|tpl| (tpl, *arena))
                            })
                    });
                let type_params_list = type_params_with_arena.as_ref().map(|(tpl, _)| tpl.clone());

                // Push type params, lower interface, pop type params.
                // push_type_parameters uses self.ctx.arena (user arena) to read
                // type param nodes. For lib interfaces the nodes are in a lib arena,
                // so push_type_parameters may return empty params. In that case,
                // extract params directly from the lib arena.
                let (mut params, updates) = self.push_type_parameters(&type_params_list);
                if params.is_empty() {
                    // For lib/multi-arena interfaces, local push_type_parameters may fail
                    // to read type parameter nodes from self.ctx.arena. Reuse canonical
                    // type-parameter extraction so defaults/constraints are preserved.
                    let canonical_params = self.get_type_params_for_symbol(sym_id);
                    if !canonical_params.is_empty() {
                        params = canonical_params;
                    }
                }

                let type_param_bindings = self.get_type_param_bindings();

                // For multi-arena interfaces (e.g. PromiseConstructor declared in
                // lib.es2015.promise.d.ts AND lib.es2015.iterable.d.ts), the resolver
                // must look up identifier text from ALL declaration arenas, not just
                // self.ctx.arena. NodeIndices from different arenas may collide, so
                // using self.ctx.arena alone could resolve to the wrong node.
                let binder = &self.ctx.binder;
                let lib_binders = self.get_lib_binders();
                let multi_arena_resolve = |node_idx: NodeIndex| -> Option<SymbolId> {
                    // Use checker-accessible compiler-managed type detection helper.

                    // Try each declaration arena to find the identifier text
                    let ident_name = decls_with_arenas
                        .iter()
                        .find_map(|(_, arena)| arena.get_identifier_text(node_idx))
                        .or_else(|| fallback_arena.get_identifier_text(node_idx))?;
                    if is_compiler_managed_type(ident_name) {
                        return None;
                    }
                    let sym_id = binder.file_locals.get(ident_name)?;
                    let symbol = binder.get_symbol_with_libs(sym_id, &lib_binders)?;
                    ((symbol.flags & symbol_flags::TYPE) != 0).then_some(sym_id)
                };

                let type_resolver = |node_idx: NodeIndex| -> Option<u32> {
                    if has_declaration_arenas {
                        multi_arena_resolve(node_idx).map(|s| s.0)
                    } else {
                        self.resolve_type_symbol_for_lowering(node_idx)
                    }
                };
                let value_resolver =
                    |node_idx: NodeIndex| self.resolve_value_symbol_for_lowering(node_idx);

                // Add def_id_resolver for DefId-based resolution
                let def_id_resolver = |node_idx: NodeIndex| -> Option<tsz_solver::def::DefId> {
                    if has_declaration_arenas {
                        multi_arena_resolve(node_idx)
                            .map(|sym_id| self.ctx.get_or_create_def_id(sym_id))
                    } else {
                        self.resolve_type_symbol_for_lowering(node_idx)
                            .map(|sym_id| {
                                self.ctx.get_or_create_def_id(tsz_binder::SymbolId(sym_id))
                            })
                    }
                };
                let name_resolver = |type_name: &str| -> Option<tsz_solver::def::DefId> {
                    self.resolve_entity_name_text_to_def_id_for_lowering(type_name)
                };

                let lowering = TypeLowering::with_hybrid_resolver(
                    fallback_arena,
                    self.ctx.types,
                    &type_resolver,
                    &def_id_resolver,
                    &value_resolver,
                )
                .with_type_param_bindings(type_param_bindings);
                let lowering = if has_declaration_arenas {
                    lowering.with_name_def_id_resolver(&name_resolver)
                } else {
                    lowering
                };

                // Use merged interface lowering for multi-arena declarations
                let has_multi_arenas = has_declaration_arenas;
                let interface_type = if has_multi_arenas {
                    let (ty, _merged_params) = lowering
                        .lower_merged_interface_declarations_with_symbol(
                            &decls_with_arenas,
                            Some(sym_id),
                        );
                    ty
                } else {
                    lowering.lower_interface_declarations_with_symbol(&symbol.declarations, sym_id)
                };
                // First try the standard heritage merge (works for user-arena interfaces).
                let mut merged =
                    self.merge_interface_heritage_types(&symbol.declarations, interface_type);
                // If standard merge didn't propagate heritage (common for lib interfaces
                // whose declarations live in lib arenas invisible to self.ctx.arena),
                // fall back to the lib-aware heritage merge.
                if merged == interface_type {
                    let name = symbol.escaped_name.clone();
                    merged = self.merge_lib_interface_heritage(merged, &name);
                }

                self.pop_type_parameters(updates);
                if let Some(def_id) = self.ctx.get_existing_def_id(sym_id) {
                    let canonical_params = self.get_type_params_for_symbol(sym_id);
                    if !canonical_params.is_empty() {
                        self.ctx.insert_def_type_params(def_id, canonical_params);
                    } else {
                        self.ctx.insert_def_type_params(def_id, params.clone());
                    }
                }
                return (merged, params);
            }

            // For type aliases, get body type and params together
            if symbol.flags & symbol_flags::TYPE_ALIAS != 0 {
                // When a type alias name collides with a global value declaration
                // (e.g., user-defined `type Proxy<T>` vs global `declare var Proxy`),
                // the merged symbol's value_declaration points to the var decl, not the
                // type alias. We must search declarations[] to find the actual type alias.
                let decl_idx = symbol
                    .declarations
                    .iter()
                    .copied()
                    .find(|&d| {
                        self.ctx
                            .arena
                            .get(d)
                            .and_then(|n| {
                                if n.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION {
                                    // Verify name matches to prevent NodeIndex collisions
                                    let type_alias = self.ctx.arena.get_type_alias(n)?;
                                    let name =
                                        self.ctx.arena.get_identifier_text(type_alias.name)?;
                                    Some(name == symbol.escaped_name.as_str())
                                } else {
                                    Some(false)
                                }
                            })
                            .unwrap_or(false)
                    })
                    .unwrap_or_else(|| {
                        if symbol.value_declaration.is_some() {
                            symbol.value_declaration
                        } else {
                            symbol
                                .declarations
                                .first()
                                .copied()
                                .unwrap_or(NodeIndex::NONE)
                        }
                    });

                if decl_idx.is_some() {
                    // Try user arena first (fast path for user-defined type aliases)
                    if let Some(node) = self.ctx.arena.get(decl_idx)
                        && let Some(type_alias) = self.ctx.arena.get_type_alias(node)
                    {
                        let (params, updates) =
                            self.push_type_parameters(&type_alias.type_parameters);
                        let alias_type = self.get_type_from_type_node(type_alias.type_node);
                        self.pop_type_parameters(updates);
                        if let Some(def_id) = self.ctx.get_existing_def_id(sym_id) {
                            self.ctx.insert_def_type_params(def_id, params.clone());
                        }
                        return (alias_type, params);
                    }

                    // For lib type aliases (e.g. Awaited<T>), use TypeLowering with the
                    // correct lib arena. get_type_from_type_node uses self.ctx.arena which
                    // doesn't have lib nodes, so we must use TypeLowering directly.
                    let lib_arena = self
                        .ctx
                        .binder
                        .declaration_arenas
                        .get(&(sym_id, decl_idx))
                        .and_then(|v| v.first())
                        .map(std::convert::AsRef::as_ref)
                        .or_else(|| {
                            self.ctx
                                .binder
                                .symbol_arenas
                                .get(&sym_id)
                                .map(std::convert::AsRef::as_ref)
                        });

                    if let Some(lib_arena) = lib_arena
                        && let Some(node) = lib_arena.get(decl_idx)
                        && let Some(type_alias) = lib_arena.get_type_alias(node)
                    {
                        let type_param_bindings = self.get_type_param_bindings();
                        let binder = &self.ctx.binder;
                        let lib_binders = self.get_lib_binders();
                        let namespace_prefix = {
                            let mut parent = lib_arena
                                .get_extended(decl_idx)
                                .map_or(NodeIndex::NONE, |info| info.parent);
                            let mut prefixes = Vec::new();
                            while !parent.is_none() {
                                let Some(parent_node) = lib_arena.get(parent) else {
                                    break;
                                };
                                if parent_node.kind == syntax_kind_ext::MODULE_DECLARATION
                                    && let Some(module) = lib_arena.get_module(parent_node)
                                    && let Some(name_node) = lib_arena.get(module.name)
                                    && name_node.kind == SyntaxKind::Identifier as u16
                                    && let Some(name_ident) = lib_arena.get_identifier(name_node)
                                {
                                    prefixes.push(name_ident.escaped_text.clone());
                                }
                                parent = lib_arena
                                    .get_extended(parent)
                                    .map_or(NodeIndex::NONE, |info| info.parent);
                            }
                            (!prefixes.is_empty())
                                .then(|| prefixes.into_iter().rev().collect::<Vec<_>>().join("."))
                        };
                        let resolve_type_name = |name: &str| -> Option<SymbolId> {
                            namespace_prefix
                                .as_ref()
                                .and_then(|prefix| {
                                    let mut scoped =
                                        String::with_capacity(prefix.len() + 1 + name.len());
                                    scoped.push_str(prefix);
                                    scoped.push('.');
                                    scoped.push_str(name);
                                    self.resolve_entity_name_text_to_def_id_for_lowering(&scoped)
                                        .and_then(|def_id| {
                                            self.ctx.def_to_symbol_id_with_fallback(def_id)
                                        })
                                })
                                .or_else(|| {
                                    self.resolve_entity_name_text_to_def_id_for_lowering(name)
                                        .and_then(|def_id| {
                                            self.ctx.def_to_symbol_id_with_fallback(def_id)
                                        })
                                })
                        };

                        let type_resolver = |node_idx: NodeIndex| -> Option<u32> {
                            let ident_name = lib_arena.get_identifier_text(node_idx)?;
                            if is_compiler_managed_type(ident_name) {
                                return None;
                            }
                            let sym_id = resolve_type_name(ident_name)?;
                            let symbol = binder.get_symbol_with_libs(sym_id, &lib_binders)?;
                            ((symbol.flags & symbol_flags::TYPE) != 0).then_some(sym_id.0)
                        };
                        let value_resolver = |node_idx: NodeIndex| -> Option<u32> {
                            self.resolve_value_symbol_for_lowering(node_idx)
                        };
                        let def_id_resolver =
                            |node_idx: NodeIndex| -> Option<tsz_solver::def::DefId> {
                                let ident_name = lib_arena.get_identifier_text(node_idx)?;
                                if is_compiler_managed_type(ident_name) {
                                    return None;
                                }
                                let sym_id = resolve_type_name(ident_name)?;
                                let symbol = binder.get_symbol_with_libs(sym_id, &lib_binders)?;
                                ((symbol.flags & symbol_flags::TYPE) != 0)
                                    .then(|| self.ctx.get_or_create_def_id(sym_id))
                            };
                        let name_resolver = |type_name: &str| -> Option<tsz_solver::def::DefId> {
                            namespace_prefix
                                .as_ref()
                                .and_then(|prefix| {
                                    let mut scoped =
                                        String::with_capacity(prefix.len() + 1 + type_name.len());
                                    scoped.push_str(prefix);
                                    scoped.push('.');
                                    scoped.push_str(type_name);
                                    self.resolve_entity_name_text_to_def_id_for_lowering(&scoped)
                                })
                                .or_else(|| {
                                    self.resolve_entity_name_text_to_def_id_for_lowering(type_name)
                                })
                        };

                        let lowering = TypeLowering::with_hybrid_resolver(
                            lib_arena,
                            self.ctx.types,
                            &type_resolver,
                            &def_id_resolver,
                            &value_resolver,
                        )
                        .with_type_param_bindings(type_param_bindings)
                        .with_name_def_id_resolver(&name_resolver);
                        let (alias_type, params) =
                            lowering.lower_type_alias_declaration(type_alias);
                        if let Some(def_id) = self.ctx.get_existing_def_id(sym_id) {
                            self.ctx.insert_def_type_params(def_id, params.clone());
                        }
                        return (alias_type, params);
                    }
                }
            }
        }

        // Fallback: get type of symbol and params separately
        let body_type = self.get_type_of_symbol(sym_id);
        let type_params = self.get_type_params_for_symbol(sym_id);
        (body_type, type_params)
    }
}
