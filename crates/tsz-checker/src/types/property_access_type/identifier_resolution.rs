//! Identifier-named property access resolution.

use crate::classes_domain::class_summary::ClassMemberKind;
use crate::query_boundaries::common::PropertyAccessResult;
use crate::query_boundaries::property_access as access_query;
use crate::state::CheckerState;
use tsz_binder::symbol_flags;
use tsz_parser::parser::node::{AccessExprData, Node};
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

pub(super) struct IdentifierPropertyAccessRequest {
    pub(super) object_type: TypeId,
    pub(super) original_object_type: TypeId,
    pub(super) display_object_type: TypeId,
    pub(super) skip_flow_narrowing: bool,
    pub(super) skip_result_flow_for_result: bool,
    pub(super) write_presence_only: bool,
    pub(super) receiver_has_daa_error: bool,
    pub(super) accessibility_error_emitted: bool,
    pub(super) commonjs_named_props_disallowed: bool,
    pub(super) is_this_access: bool,
    pub(super) js_expando_before_assignment: bool,
}

impl<'a> CheckerState<'a> {
    pub(super) fn resolve_identifier_property_access(
        &mut self,
        idx: NodeIndex,
        access: &AccessExprData,
        name_node: &Node,
        request: IdentifierPropertyAccessRequest,
    ) -> TypeId {
        let is_this_access = request.is_this_access;
        let mut additional_bound_type_params = None;
        let member_type = self.resolve_identifier_property_access_inner(
            idx,
            access,
            name_node,
            request,
            &mut additional_bound_type_params,
        );
        self.bind_omitted_base_type_args_for_this_member(
            member_type,
            is_this_access,
            additional_bound_type_params.as_deref(),
        )
    }

    /// Bind "dangling" base-class type parameters of a `this`-member read to the
    /// base parameter's `default → constraint → unknown`.
    ///
    /// When a class extends a generic base WITHOUT type arguments
    /// (`class Der extends Base`, where `Base<P = …>`), the omitted argument is
    /// never bound on the `this`-member resolution path (an external receiver
    /// reads the already-defaulted instance shape, so it is unaffected). The bare
    /// parameter `P` would otherwise leak into the value's type — a false
    /// `TS2339`/`TS7053`/`TS2322` on `this.member`, the raw-parameter sibling of
    /// the `error`/`never`-in-a-type-argument-slot leak family (#13484). `tsc`
    /// binds such an omitted base argument to its default
    /// (`fillMissingTypeArguments`); this does the same.
    ///
    /// Type parameters of the enclosing generic context (a class's / function's
    /// own parameters, e.g. `T` of a generic `Box<T>`) stay in scope and are
    /// preserved, so only genuinely unbound base parameters are resolved. Gated on
    /// the `this`-receiver flag and the cheap memoized free-parameter predicate so
    /// concrete results and non-`this` deferred-generic reads are untouched.
    fn bind_omitted_base_type_args_for_this_member(
        &mut self,
        member_type: TypeId,
        is_this_access: bool,
        additional_bound_type_params: Option<&[TypeId]>,
    ) -> TypeId {
        if !is_this_access
            || !crate::query_boundaries::common::contains_free_type_parameters(
                self.ctx.types,
                member_type,
            )
        {
            return member_type;
        }
        let mut in_scope = self.member_type_parameter_ids_in_scope();
        if let Some(additional_bound_type_params) = additional_bound_type_params {
            in_scope.extend(additional_bound_type_params.iter().copied());
        }
        crate::query_boundaries::common::resolve_unbound_type_params_to_defaults(
            self.ctx.types,
            member_type,
            &in_scope,
        )
    }

    fn resolve_identifier_property_access_inner(
        &mut self,
        idx: NodeIndex,
        access: &AccessExprData,
        name_node: &Node,
        request: IdentifierPropertyAccessRequest,
        additional_bound_type_params: &mut Option<Vec<TypeId>>,
    ) -> TypeId {
        let IdentifierPropertyAccessRequest {
            object_type,
            original_object_type,
            mut display_object_type,
            skip_flow_narrowing,
            skip_result_flow_for_result,
            write_presence_only,
            receiver_has_daa_error,
            accessibility_error_emitted,
            commonjs_named_props_disallowed,
            is_this_access,
            js_expando_before_assignment,
        } = request;
        let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
            return TypeId::ANY;
        };
        let property_name = &ident.escaped_text;
        let effective_write_result = |type_id: TypeId, write_type: Option<TypeId>| -> TypeId {
            if skip_flow_narrowing {
                if write_presence_only {
                    TypeId::ANY
                } else {
                    write_type.unwrap_or(type_id)
                }
            } else {
                type_id
            }
        };

        if self.report_namespace_value_access_for_type_only_import_equals_expr(access.expression) {
            return TypeId::ERROR;
        }

        if let Some(base_sym_id) = self.resolve_identifier_symbol(access.expression)
            && let Some(base_symbol) = self.ctx.binder.get_symbol(base_sym_id)
            && base_symbol.has_any_flags(symbol_flags::ALIAS)
            && base_symbol.import_module().is_some()
            && base_symbol.import_name().is_none_or(|name| name == "*")
        {
            if let Some(member_type) =
                self.resolve_namespace_value_member_from_symbol(base_sym_id, property_name)
            {
                return self.finalize_property_access_result(
                    idx,
                    member_type,
                    skip_flow_narrowing,
                    false,
                );
            }

            if self.is_in_type_only_position(idx)
                && let Some(member_sym_id) =
                    base_symbol.import_module().and_then(|module_specifier| {
                        self.resolve_effective_module_exports_from_file(
                            module_specifier,
                            Some(base_symbol.decl_file_idx as usize),
                        )
                        .and_then(|exports| exports.get(property_name))
                    })
            {
                let member_type = self.get_type_of_symbol(member_sym_id);
                if member_type != TypeId::ERROR && member_type != TypeId::UNKNOWN {
                    return self.finalize_property_access_result(
                        idx,
                        member_type,
                        skip_flow_narrowing,
                        false,
                    );
                }
            }
        }

        if let Some(base_sym_id) = self.resolve_identifier_symbol(access.expression)
            && let Some(base_symbol) = self.ctx.binder.get_symbol(base_sym_id)
            && base_symbol.has_any_flags(symbol_flags::ALIAS)
            && let Some(decl_node) = self.ctx.arena.get(base_symbol.value_declaration)
            && decl_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
            && let Some(import_decl) = self.ctx.arena.get_import_decl(decl_node)
            && let Some(module_specifier) =
                self.get_require_module_specifier(import_decl.module_specifier)
            && let Some(surface) = self.resolve_js_export_surface_for_module(
                &module_specifier,
                Some(self.ctx.current_file_idx),
            )
            && surface.has_commonjs_exports
            && let Some(member_type) = surface.lookup_named_export(property_name, self.ctx.types)
        {
            return self.finalize_property_access_result(
                idx,
                member_type,
                skip_flow_narrowing,
                false,
            );
        }

        let enum_instance_like_access = self
            .is_enum_instance_property_access(object_type, access.expression)
            || access_query::type_parameter_constraint(self.ctx.types, object_type).is_some_and(
                |constraint| access_query::enum_def_id(self.ctx.types, constraint).is_some(),
            );
        let hidden_qualified_namespace_member_apparent_type = self
            .qualified_namespace_member_hidden_on_exported_surface(
                idx,
                access.expression,
                property_name,
            );
        let hidden_qualified_namespace_member =
            hidden_qualified_namespace_member_apparent_type.is_some();

        if !skip_flow_narrowing
            && !enum_instance_like_access
            && !hidden_qualified_namespace_member
            && let Some(obj_node) = self.ctx.arena.get(access.expression)
            && let Some(obj_ident) = self.ctx.arena.get_identifier(obj_node)
            && let Some(member_type) =
                self.resolve_umd_global_member_by_name(&obj_ident.escaped_text, property_name)
        {
            if let Some(umd_sym_id) =
                self.resolve_umd_global_symbol_by_name(&obj_ident.escaped_text)
            {
                let is_pure_umd_alias = self
                    .get_cross_file_symbol(umd_sym_id)
                    .or_else(|| self.ctx.binder.get_symbol(umd_sym_id))
                    .is_some_and(|symbol| {
                        symbol.is_umd_export
                            && (symbol.flags & tsz_binder::symbol_flags::VALUE) == 0
                    });
                if is_pure_umd_alias
                    && self.current_file_is_module_for_umd_global_access()
                    && !self.ctx.compiler_options.allow_umd_global_access
                    && !self.has_non_umd_global_value(&obj_ident.escaped_text)
                {
                    use crate::diagnostics::diagnostic_codes;
                    self.error_at_node_msg(
                    access.expression,
                    diagnostic_codes::REFERS_TO_A_UMD_GLOBAL_BUT_THE_CURRENT_FILE_IS_A_MODULE_CONSIDER_ADDING_AN_IMPOR,
                    &[&obj_ident.escaped_text],
                );
                }
            }
            return self.finalize_property_access_result(
                idx,
                member_type,
                skip_flow_narrowing,
                false,
            );
        }

        if !skip_flow_narrowing
            && !enum_instance_like_access
            && !hidden_qualified_namespace_member
            && let Some(member_type) =
                self.resolve_shadowed_global_value_member(access.expression, property_name)
        {
            return self.finalize_property_access_result(
                idx,
                member_type,
                skip_flow_narrowing,
                false,
            );
        }

        // Fallback for namespace/export member accesses where type-only namespace
        // classification misses the object form but symbol resolution can still
        // identify `A.B` as a concrete exported value member.
        if !hidden_qualified_namespace_member
            && let Some(member_sym_id) = self.resolve_qualified_symbol(idx)
            && let Some(member_symbol) = self
                .get_cross_file_symbol(member_sym_id)
                .or_else(|| self.ctx.binder.get_symbol(member_sym_id))
        {
            // Skip type-only members (e.g., `export type { A }`, interfaces).
            // These should not be resolved as values; let the code fall
            // through to TS2693 "type only" or TS2339 "property doesn't exist" handling.
            let transitively_type_only =
                self.is_namespace_member_transitively_type_only(access.expression, property_name);
            if !member_symbol.is_type_only
                && !self.symbol_member_is_type_only(member_sym_id, Some(property_name))
                && member_symbol.has_any_flags(symbol_flags::VALUE)
                && !transitively_type_only
                // For merged symbols (e.g., namespace + interface), verify that the VALUE
                // part is actually exported. If only the TYPE part is exported, the value
                // is not accessible at runtime.
                && self.symbol_has_exported_value_declaration(member_sym_id)
            {
                let parent_sym_id = member_symbol.parent;
                if let Some(parent_symbol) = self
                    .get_cross_file_symbol(parent_sym_id)
                    .or_else(|| self.ctx.binder.get_symbol(parent_sym_id))
                    && parent_symbol.has_any_flags(symbol_flags::MODULE | symbol_flags::ENUM)
                {
                    // If the member is an enum (not an enum member), return
                    // the enum object type so property access on enum members
                    // (e.g., M3.Color.Blue) resolves correctly.
                    let member_type = if member_symbol.has_any_flags(symbol_flags::ENUM)
                        && !member_symbol.has_any_flags(symbol_flags::ENUM_MEMBER)
                    {
                        self.enum_object_type(member_sym_id)
                            .unwrap_or_else(|| self.get_type_of_symbol(member_sym_id))
                    } else if member_symbol.has_any_flags(symbol_flags::INTERFACE)
                        && member_symbol.has_any_flags(symbol_flags::VALUE)
                    {
                        // When a namespace member is both an interface and a value
                        // (e.g., `interface NumberFormat` + `var NumberFormat: { new(): ... }`
                        // in namespace Intl), resolve the value declaration's type so
                        // construct signatures are available for `new NS.Member()`.
                        // This mirrors the merged-symbol resolution in get_type_of_identifier.
                        let value_decl = member_symbol.value_declaration;
                        let declarations = member_symbol.declarations.clone();
                        let preferred = self
                            .preferred_value_declaration(member_sym_id, value_decl, &declarations)
                            .unwrap_or(value_decl);
                        let mut val_type =
                            self.type_of_value_declaration_for_symbol(member_sym_id, preferred);
                        if val_type == TypeId::UNKNOWN || val_type == TypeId::ERROR {
                            for &decl_idx in &declarations {
                                if decl_idx == preferred {
                                    continue;
                                }
                                let candidate = self
                                    .type_of_value_declaration_for_symbol(member_sym_id, decl_idx);
                                if candidate != TypeId::UNKNOWN && candidate != TypeId::ERROR {
                                    val_type = candidate;
                                    break;
                                }
                            }
                        }
                        if val_type != TypeId::UNKNOWN && val_type != TypeId::ERROR {
                            val_type
                        } else {
                            self.get_type_of_symbol(member_sym_id)
                        }
                    } else {
                        // For merged interface+variable symbols (e.g.,
                        // `interface Foo` + `var Foo: FooConstructor`), prefer the
                        // variable's type in value position so construct signatures
                        // are visible to `new` expressions.
                        self.merged_value_type_for_symbol_if_available(member_sym_id)
                            .unwrap_or_else(|| self.get_type_of_symbol(member_sym_id))
                    };
                    if member_type != TypeId::ERROR && member_type != TypeId::UNKNOWN {
                        return self.finalize_property_access_result(
                            idx,
                            member_type,
                            skip_flow_narrowing,
                            false,
                        );
                    }
                }
            }
        }

        let type_only_namespace_access_name = self.type_only_namespace_member_access_name(idx);
        if self.namespace_has_type_only_member(object_type, property_name)
            || type_only_namespace_access_name.is_some()
        {
            if self.is_js_file()
                && self.ctx.compiler_options.check_js
                && let Some(ns_name) = self.entity_name_text(access.expression)
                && let Some(member_sym_id) =
                    self.resolve_namespace_member_from_all_binders(&ns_name, property_name)
            {
                if !self.symbol_member_is_type_only(member_sym_id, Some(property_name)) {
                    let value_type = self.get_type_of_symbol(member_sym_id);
                    if value_type != TypeId::UNKNOWN && value_type != TypeId::ERROR {
                        return value_type;
                    }
                }

                if let Some(member_symbol) = self
                    .ctx
                    .binder
                    .get_symbol(member_sym_id)
                    .or_else(|| self.get_cross_file_symbol(member_sym_id))
                {
                    let checked_js_decl = if member_symbol.value_declaration.is_some() {
                        self.checked_js_constructor_value_declaration(
                            member_sym_id,
                            member_symbol.value_declaration,
                            &member_symbol.declarations,
                        )
                    } else {
                        member_symbol
                            .declarations
                            .iter()
                            .copied()
                            .find(|&decl_idx| {
                                self.declaration_is_checked_js_constructor_value_declaration(
                                    member_sym_id,
                                    decl_idx,
                                )
                            })
                    };
                    if let Some(checked_js_decl) = checked_js_decl {
                        let value_type = self
                            .type_of_value_declaration_for_symbol(member_sym_id, checked_js_decl);
                        if value_type != TypeId::UNKNOWN && value_type != TypeId::ERROR {
                            return value_type;
                        }
                    }
                }
            }
            // TS2307 already covers missing modules; suppress property follow-on noise.
            if self.is_property_access_on_unresolved_import(access.expression) {
                return TypeId::ERROR;
            }
            // Don't emit TS2693 in heritage clause context — the heritage
            // checker will emit the appropriate error (e.g., TS2689).
            // Also suppress in JS/checkJs when the access sits on an
            // assignment LHS chain (e.g., `ns.Interface = function() {}`
            // or `ns.Interface.prototype.fn = ...`). tsc treats these as
            // prototype-property-assignment merges and does not emit TS2708.
            if self
                .find_enclosing_heritage_clause(access.name_or_argument)
                .is_none()
                && !(self.is_js_file()
                    && self.ctx.compiler_options.check_js
                    && self.property_access_is_write_target_or_base(idx))
                && let Some(ns_name) = type_only_namespace_access_name
                    .or_else(|| self.entity_name_text(access.expression))
            {
                self.report_wrong_meaning_diagnostic(
                    &ns_name,
                    access.expression,
                    crate::query_boundaries::name_resolution::NameLookupKind::Namespace,
                );
                // tsc does NOT emit TS2693 for the type-only member
                // when TS2708 was already emitted for the namespace.
            }
            return TypeId::ERROR;
        }
        if let Some(display_type) = hidden_qualified_namespace_member_apparent_type.as_deref() {
            if !access.question_dot_token
                && !property_name.starts_with('#')
                && !accessibility_error_emitted
            {
                self.error_property_not_exist_with_apparent_type(
                    property_name,
                    display_type,
                    access.name_or_argument,
                );
            }
            return TypeId::ERROR;
        }
        if self.is_namespace_value_type(object_type) && !enum_instance_like_access {
            let hidden_qualified_namespace_member_apparent_type = self
                .qualified_namespace_member_hidden_on_exported_surface(
                    idx,
                    access.expression,
                    property_name,
                );
            let hidden_qualified_namespace_member =
                hidden_qualified_namespace_member_apparent_type.is_some();
            if !hidden_qualified_namespace_member {
                let namespace_object_type = self.resolve_type_for_property_access(object_type);
                if let Some(member_type) =
                    self.resolve_namespace_value_member(namespace_object_type, property_name)
                {
                    return self.finalize_property_access_result(
                        idx,
                        member_type,
                        skip_flow_narrowing,
                        false,
                    );
                }
            }

            // When the object type is a TypeQuery (typeof M) for a namespace,
            // try to resolve the property from the namespace symbol's exports.
            // This handles `var m: typeof M; m.Point` where `m` is a variable
            // typed as `typeof Namespace`.
            if let Some(ns_member_type) =
                self.resolve_namespace_typeof_member(object_type, property_name)
            {
                return self.finalize_property_access_result(
                    idx,
                    ns_member_type,
                    skip_flow_narrowing,
                    false,
                );
            }
            // A bare namespace's `typeof` type has no implicit `prototype` member
            // (`declare namespace C { function bar(): void }` — `C.prototype = {}`
            // is a real `TS2339`, oracle-verified). The exemption only applies
            // when the root symbol is ALSO callable (function/class merged with
            // the namespace, the constructor-plus-namespace idiom), matching
            // tsc's `getPropertyOfType` treating `prototype` as implicit only on
            // a function/class value's apparent type.
            if self.is_js_file()
                && property_name == "prototype"
                && self.property_access_is_direct_write_target(idx)
                && self
                    .resolve_identifier_symbol(access.expression)
                    .and_then(|sym_id| self.ctx.binder.get_symbol(sym_id))
                    .is_some_and(|sym| {
                        !(sym.has_any_flags(symbol_flags::ALIAS) && sym.import_module().is_some())
                            && sym.has_any_flags(symbol_flags::FUNCTION | symbol_flags::CLASS)
                    })
                && self.js_prototype_write_root_is_callable_or_constructible(access.expression)
            {
                return TypeId::ANY;
            }
            if self.find_enclosing_computed_property(idx).is_some()
                && self.get_symbol_property_name_from_expr(idx).is_some()
            {
                return TypeId::SYMBOL;
            }
            let namespace_class_recovery = self
                .resolve_class_access_with_current_member_initializer_recovery(
                    access.expression,
                    object_type,
                );
            let mut namespace_class_chain_summary = None;
            if let Some(member_type) = self.recover_property_from_class_chain_summary(
                namespace_class_recovery.1,
                namespace_class_recovery.0,
                &mut namespace_class_chain_summary,
                property_name,
            ) {
                return self.finalize_property_access_result(
                    idx,
                    member_type,
                    skip_flow_narrowing,
                    false,
                );
            }
            if !access.question_dot_token
                && !property_name.starts_with('#')
                && !accessibility_error_emitted
                && !self.is_property_access_on_unresolved_import(access.expression)
            {
                // Check if the base expression is an uninstantiated namespace.
                // tsc emits TS2708 "Cannot use namespace 'X' as a value" on the
                // namespace identifier, not TS2339 on the property.
                if let Some(ns_name) = self.uninstantiated_namespace_name(access.expression) {
                    self.report_wrong_meaning_diagnostic(
                        &ns_name,
                        access.expression,
                        crate::query_boundaries::name_resolution::NameLookupKind::Namespace,
                    );
                } else {
                    self.error_property_not_exist_at(
                        property_name,
                        display_object_type,
                        access.name_or_argument,
                    );
                }
            }
            return TypeId::ERROR;
        }

        let external_prototype_owner_instance_type = self
            .find_enclosing_non_arrow_function(access.expression)
            .and_then(|func_idx| self.js_prototype_owner_expression_for_node(func_idx))
            .and_then(|owner_expr| {
                // Only for external/imported prototype owners. Local function/class
                // owners are handled by regular JS prototype-this logic.
                if self
                    .js_prototype_owner_function_target(owner_expr)
                    .is_some()
                {
                    return None;
                }
                let owner_type = self.get_type_of_node(owner_expr);
                if owner_type == TypeId::ANY
                    || owner_type == TypeId::UNKNOWN
                    || owner_type == TypeId::ERROR
                {
                    return None;
                }
                let owner_type_for_access = self.resolve_type_for_property_access(owner_type);
                match self.resolve_property_access_with_env(owner_type_for_access, "prototype") {
                    PropertyAccessResult::Success { type_id, .. }
                    | PropertyAccessResult::PossiblyNullOrUndefined {
                        property_type: Some(type_id),
                        ..
                    } => Some(type_id),
                    _ => None,
                }
            });

        let mut object_type_for_access = if enum_instance_like_access {
            self.apparent_enum_instance_type(object_type)
                .unwrap_or_else(|| self.resolve_type_for_property_access(object_type))
        } else {
            self.resolve_type_for_property_access(object_type)
        };
        if object_type_for_access == TypeId::ANY
            && is_this_access
            && let Some(owner_instance_type) = external_prototype_owner_instance_type
        {
            object_type_for_access = owner_instance_type;
        }
        if object_type_for_access == TypeId::ANY {
            return TypeId::ANY;
        }
        if object_type_for_access == TypeId::ERROR {
            return TypeId::ERROR; // Return ERROR instead of ANY to expose type errors
        }
        // In write context (skip_flow_narrowing), skip this shortcut:
        // resolve_namespace_value_member returns the symbol's read type, which
        // doesn't account for divergent getter/setter types. The full property
        // access path below correctly uses write_type for setter parameters.
        //
        // Do this after resolving the base type for property access so cross-file
        // enum/namespace objects (e.g. imported class statics initialized to enums)
        // classify the same way as local ones.
        if !skip_flow_narrowing
            && !enum_instance_like_access
            && !hidden_qualified_namespace_member
            && let Some(member_type) =
                self.resolve_namespace_value_member(object_type_for_access, property_name)
        {
            return self.finalize_property_access_result(
                idx,
                member_type,
                skip_flow_narrowing,
                false,
            );
        }

        if self.ctx.strict_bind_call_apply()
            && let Some(strict_method_type) = self.strict_bind_call_apply_method_type(
                object_type_for_access,
                access.expression,
                property_name,
            )
        {
            return self.finalize_property_access_result(
                idx,
                strict_method_type,
                skip_flow_narrowing,
                false,
            );
        }

        if let Some(iterator_method_type) =
            self.synthesized_array_iterator_method_type(object_type_for_access, property_name)
        {
            return self.finalize_property_access_result(
                idx,
                iterator_method_type,
                skip_flow_narrowing,
                false,
            );
        }

        // Private fields take this `any` fallback too (`skip_private: false`):
        // the accessibility path reports TS2855/TS2340 for every parent
        // instance field reached via super regardless of visibility, and
        // typing the private ones differently from the public ones makes
        // repeated `var x = super.publicField; var x = super.privateField;`
        // declarations diverge (`any` vs the field type) and cascade a
        // spurious TS2403.
        if self.is_super_expression(access.expression)
            && let Some((class_idx, is_static_access)) =
                self.resolve_class_for_access(access.expression, object_type_for_access)
            && !is_static_access
            && matches!(
                self.class_chain_member_kind_name_only(class_idx, property_name, false, false)
                    .map(|(kind, _)| kind),
                Some(ClassMemberKind::Field)
            )
        {
            return TypeId::ANY;
        }

        // Use the environment-aware resolver so that array methods, boxed
        // primitive types, and other lib-registered types are available.
        let lookup_object_type = self
            .defaulted_property_access_receiver(original_object_type)
            .or_else(|| {
                crate::query_boundaries::common::is_generic_application(
                    self.ctx.types,
                    original_object_type,
                )
                .then_some(original_object_type)
            })
            .unwrap_or(object_type_for_access);
        let mut result = self.resolve_property_access_with_env(lookup_object_type, property_name);
        let direct_class_this_receiver = self.is_this_expression(access.expression)
            && (self.ctx.enclosing_class.is_some()
                || self.nearest_enclosing_class(access.expression).is_some())
            && !self.is_this_in_nested_function_inside_class(idx)
            && !self.is_this_in_static_class_member(idx);
        if matches!(result, PropertyAccessResult::PropertyNotFound { .. })
            && direct_class_this_receiver
            && let Some(member_type) = self.direct_this_class_member_declared_type(property_name)
        {
            return self.finalize_property_access_result(
                idx,
                member_type,
                skip_flow_narrowing,
                false,
            );
        }
        if matches!(result, PropertyAccessResult::PropertyNotFound { .. })
            && direct_class_this_receiver
            && let Some((member_type, bound_type_params)) =
                self.recover_bare_this_lexical_class_header_member(access.expression, property_name)
        {
            let result = self.finalize_property_access_result_with_bound_type_params(
                idx,
                member_type,
                skip_flow_narrowing,
                false,
                &bound_type_params,
            );
            *additional_bound_type_params = Some(bound_type_params);
            return result;
        }
        // Flow predicate narrowing can produce unions/intersections like
        // `C2 | (C2 & C1)` or `(D1 & C2) | (D1 & C1)`. Looking up properties
        // directly on those unevaluated shells may fall back to a bare `any`.
        // Retry on the evaluated receiver to recover the concrete property type.
        if matches!(
            result,
            PropertyAccessResult::Success {
                type_id: TypeId::ANY,
                from_index_signature: false,
                ..
            }
        ) && !crate::query_boundaries::state::checking::is_type_parameter_like(
            self.ctx.types,
            object_type_for_access,
        ) {
            let evaluated_receiver = self.evaluate_type_with_env(object_type_for_access);
            if evaluated_receiver != object_type_for_access
                && evaluated_receiver != TypeId::ANY
                && evaluated_receiver != TypeId::ERROR
            {
                let retry =
                    self.resolve_property_access_with_env(evaluated_receiver, property_name);
                let retry_improved = match retry {
                    PropertyAccessResult::Success {
                        type_id,
                        from_index_signature,
                        ..
                    } => type_id != TypeId::ANY || from_index_signature,
                    _ => true,
                };
                if retry_improved {
                    object_type_for_access = evaluated_receiver;
                    result = retry;
                }
            }
        }
        match result {
            PropertyAccessResult::Success {
                type_id: mut prop_type,
                write_type,
                from_index_signature,
            } => {
                let mut used_class_chain_method_type = false;
                if property_name == "exports"
                    && prop_type == TypeId::ANY
                    && self.is_js_file()
                    && let Some(obj_node) = self.ctx.arena.get(access.expression)
                    && let Some(ident) = self.ctx.arena.get_identifier(obj_node)
                    && ident.escaped_text == "module"
                    && self.current_file_commonjs_module_identifier_is_unshadowed(access.expression)
                {
                    return self.current_file_commonjs_namespace_type();
                }

                // Recover inherited methods from the class chain when early
                // initializer checking runs before `ctx.enclosing_class` is active.
                if direct_class_this_receiver
                    && self.ctx.enclosing_class.is_none()
                    && let Some(class_idx) = self.nearest_enclosing_class(access.expression)
                {
                    let summary = self.summarize_class_chain(class_idx);
                    if matches!(
                        summary.member_kind(property_name, false, true),
                        Some(ClassMemberKind::Method | ClassMemberKind::Accessor)
                    ) && let Some(member_info) = summary.member_info(property_name, false, true)
                    {
                        prop_type = member_info.type_id;
                        used_class_chain_method_type = true;
                    }
                }

                // A bare type-parameter receiver can fall back to `any` here
                // when the constraint only exposes the property on some union
                // members. Preserve TS2339 for direct reads like `value.foo`
                // but avoid firing after control-flow has already refined the
                // receiver to a narrower view.
                if !skip_flow_narrowing
                    && !from_index_signature
                    && prop_type == TypeId::ANY
                    && object_type == object_type_for_access
                    && object_type_for_access == original_object_type
                    && crate::query_boundaries::state::checking::is_type_parameter_like(
                        self.ctx.types,
                        object_type_for_access,
                    )
                    && !self.type_parameter_constraint_has_explicit_property(
                        object_type_for_access,
                        property_name,
                    )
                {
                    let generic_class_recovery = self
                        .resolve_class_access_with_current_member_initializer_recovery(
                            access.expression,
                            object_type_for_access,
                        );
                    let mut generic_class_chain_summary = None;
                    if let Some(member_type) = self.recover_property_from_class_chain_summary(
                        generic_class_recovery.1,
                        generic_class_recovery.0,
                        &mut generic_class_chain_summary,
                        property_name,
                    ) {
                        return self.finalize_property_access_result(
                            idx,
                            member_type,
                            skip_flow_narrowing,
                            false,
                        );
                    }
                    // Suppress TS2339 for index access types on type parameters.
                    // When accessing properties on types like T[keyof T], we cannot
                    // determine what properties exist until T is instantiated.
                    if !crate::query_boundaries::common::is_index_access_type(
                        self.ctx.types,
                        object_type_for_access,
                    ) {
                        self.error_property_not_exist_at(
                            property_name,
                            object_type_for_access,
                            access.name_or_argument,
                        );
                    }
                    return TypeId::ERROR;
                }

                if let Some((recovered_type, recovered_method)) = self
                    .recover_direct_this_class_chain_member(
                        direct_class_this_receiver,
                        used_class_chain_method_type,
                        access.expression,
                        property_name,
                        prop_type,
                        object_type_for_access,
                        original_object_type,
                    )
                {
                    prop_type = recovered_type;
                    used_class_chain_method_type = recovered_method;
                }

                if let Some(recovered_type) = self.substitute_direct_this_property_shape_type(
                    direct_class_this_receiver,
                    used_class_chain_method_type,
                    object_type_for_access,
                    property_name,
                ) {
                    prop_type = recovered_type;
                }

                // Substitute polymorphic `this` type with the receiver type.
                // E.g., for `class C<T> { x = this; }`, accessing `c.x` where
                // `c: C<string>` should yield `C<string>`, not raw `ThisType`.
                //
                // `super.method` is special: the property lookup happens on the
                // base instance type, but polymorphic `this` in the base member
                // must bind to the *enclosing* class's `this`-type — matching
                // tsc's `getTypeWithThisArgument(baseType, enclosingClassThisType)`
                // — not the base instance type. JS's `super.foo()` never receives
                // a fresh base-class instance: the runtime receiver stays the
                // current instance, so `super.compare(other)` inside
                // `Dog.compare(other: this)` must see `(other: this) => boolean`,
                // and `super.returnThis()` where the base infers a `this` return
                // must yield the enclosing class's `this`, not the base class.
                //
                // The enclosing class's `this`-type is the polymorphic `ThisType`
                // marker itself (contextually the enclosing class): binding
                // `this` to it is the identity, so the base member's `this` stays
                // polymorphic and is rebound to the actual receiver at the
                // eventual direct-access site. This is what threads the receiver
                // through a multi-level `super` chain (`A -> B -> C`): each hop's
                // inferred `this` return stays polymorphic, so `new C().m()`
                // resolves to `C`. Baking it to the base instance type here
                // instead — the previous behavior when `current_this_type()` was
                // unavailable during lazy return-type inference — collapsed the
                // whole chain to the root base class.
                //
                // Static `super` keeps the constructor-`this` binding computed by
                // `current_this_type()`; only the instance case is the marker.
                let this_substitution_target = if self.is_super_expression(access.expression) {
                    if self.is_this_in_static_class_member(access.expression) {
                        self.current_this_type().unwrap_or(original_object_type)
                    } else {
                        self.ctx.types.this_type()
                    }
                } else if direct_class_this_receiver
                    || crate::query_boundaries::common::contains_this_type(
                        self.ctx.types,
                        original_object_type,
                    )
                {
                    // Either the receiver *is* `this`, or its type still refers to
                    // the enclosing class's polymorphic `this` (e.g.
                    // `this.children: this[]`, `this.pair: [this, this]`). Such a
                    // receiver is not a concrete anchor for `this`: a member whose
                    // type also mentions `this` — the element `this` of
                    // `Array<this>.push`/`indexOf`, a `[this, this]` slot — is
                    // already in the correct scope and must stay `this`.
                    // Substituting `this` with the receiver type would conflate the
                    // member's `this` with the whole receiver shape (turning the
                    // `this` element of `this[]` into `this[]`), drawing a spurious
                    // TS2345.
                    self.ctx.types.this_type()
                } else {
                    original_object_type
                };
                //
                // Skip substitution when prop_type IS the receiver type. This
                // prevents creating a new TypeId when accessing properties like
                // `self2: D` where D is the current class instance type. Without
                // this guard, `this.self2` would return D_subst (a new TypeId)
                // instead of D, causing assignment mismatches in polymorphic
                // `this` checks (e.g., `this.self = this.self2` would fail
                // because D_subst != D even though they're semantically equal).
                // When the substitution target is itself a *compound* `this`-relative
                // type (e.g. accessing a member on `this.children: this[]` inside the
                // class body), the apparent type's own `this` was already bound to the
                // receiver by the solver, and any `this` remaining in `prop_type` is
                // element-derived — it is the *same* polymorphic `this` and must stay
                // polymorphic. Substituting it with the this-bearing receiver would
                // spuriously nest `this` (e.g. `push(...items: this[])` would become
                // `this[][]`, drawing a false TS2345). The empty branch also
                // short-circuits the raw-recovery `else if` below, which would
                // otherwise re-introduce the same nesting.
                // An instance `super` receiver must force the raw-recovery path
                // below. The solver eagerly binds a returned `this` to the lookup
                // receiver — here the *base* instance — so `super.m()`'s stored
                // type comes back as `() => Base` (whose `Base` still transitively
                // mentions `this` through its own members). That makes the plain
                // `contains_this_type(prop_type)` branch a no-op identity against
                // the enclosing-`this` marker, leaving the return baked to the
                // base class. Re-resolving with `this` binding deferred recovers
                // the raw `() => this`, which the marker then keeps polymorphic so
                // it rebinds to the real receiver at the direct-access site.
                let super_receiver = self.is_super_expression(access.expression)
                    && !self.is_this_in_static_class_member(access.expression);
                if self.receiver_expr_is_this_relative(access.expression)
                    && self.type_is_compound_this_relative(this_substitution_target)
                {
                    // Leave `prop_type` as the solver produced it.
                } else if !super_receiver
                    && crate::query_boundaries::common::contains_this_type(
                        self.ctx.types,
                        prop_type,
                    )
                    && prop_type != this_substitution_target
                {
                    prop_type = crate::query_boundaries::common::substitute_this_type(
                        self.ctx.types,
                        prop_type,
                        this_substitution_target,
                    );
                } else if !used_class_chain_method_type {
                    // When a method returns `this` on an intersection member,
                    // the solver's object visitor eagerly binds `this` to the
                    // structural (flattened) object — so `contains_this_type`
                    // above returns false. Re-resolve with `this` binding
                    // deferred to recover raw `ThisType`, then substitute with
                    // the nominal receiver (e.g., Thing5 instead of {a,b,c}).
                    let raw =
                        crate::query_boundaries::property_access::resolve_property_access_raw_this(
                            self.ctx.types,
                            object_type_for_access,
                            self.ctx.types.intern_string(property_name),
                        );
                    if let PropertyAccessResult::Success {
                        type_id: raw_type, ..
                    } = raw
                        && crate::query_boundaries::common::contains_this_type(
                            self.ctx.types,
                            raw_type,
                        )
                    {
                        prop_type = crate::query_boundaries::common::substitute_this_type(
                            self.ctx.types,
                            raw_type,
                            this_substitution_target,
                        );
                    }
                }

                if skip_flow_narrowing
                    && from_index_signature
                    && crate::query_boundaries::state::checking::is_type_parameter_like(
                        self.ctx.types,
                        object_type,
                    )
                    && !self
                        .type_parameter_constraint_has_explicit_property(object_type, property_name)
                {
                    self.error_property_not_exist_at(
                        property_name,
                        object_type,
                        access.name_or_argument,
                    );
                    return TypeId::ERROR;
                }

                if skip_flow_narrowing
                    && from_index_signature
                    && self.generic_mapped_receiver_lacks_property_access_name(
                        original_object_type,
                        property_name,
                    )
                {
                    self.error_property_not_exist_at(
                        property_name,
                        original_object_type,
                        access.name_or_argument,
                    );
                    return TypeId::ERROR;
                }

                let union_has_explicit_member = from_index_signature
                    && self
                        .union_has_explicit_property_member(object_type_for_access, property_name);
                // Check for error 4111: property access from index signature
                if from_index_signature
                    && self
                        .ctx
                        .compiler_options
                        .no_property_access_from_index_signature
                    && !union_has_explicit_member
                {
                    use crate::diagnostics::diagnostic_codes;
                    self.error_at_node(
                    access.name_or_argument,
                    &format!(
                        "Property '{property_name}' comes from an index signature, so it must be accessed with ['{property_name}']."
                    ),
                    diagnostic_codes::PROPERTY_COMES_FROM_AN_INDEX_SIGNATURE_SO_IT_MUST_BE_ACCESSED_WITH,
                );
                }
                if skip_flow_narrowing
                    && self.union_write_requires_existing_named_member(
                        object_type_for_access,
                        property_name,
                    )
                {
                    self.error_property_not_exist_at(
                        property_name,
                        object_type_for_access,
                        access.name_or_argument,
                    );
                    return TypeId::ERROR;
                }
                // When in a write context (assignment target), use the setter
                // type if the property has divergent getter/setter types.
                let effective_type = effective_write_result(prop_type, write_type);
                self.finalize_property_access_result(
                    idx,
                    effective_type,
                    skip_flow_narrowing,
                    skip_result_flow_for_result,
                )
            }

            PropertyAccessResult::PropertyNotFound { .. } => {
                // `T extends Alias<...>` exposes members from the constraint even
                // when the first lookup on `T` cannot see through the alias shell.
                if crate::query_boundaries::state::checking::is_type_parameter_like(
                    self.ctx.types,
                    object_type_for_access,
                ) && let Some(constraint) =
                    crate::query_boundaries::state::checking::type_parameter_constraint(
                        self.ctx.types,
                        object_type_for_access,
                    )
                {
                    let mut candidates = Vec::with_capacity(3);
                    candidates.push(constraint);
                    let property_expanded =
                        self.evaluate_application_type_for_property_access(constraint);
                    if property_expanded != constraint {
                        candidates.push(property_expanded);
                    }
                    let evaluated = self.evaluate_type_with_env(constraint);
                    if evaluated != constraint && evaluated != property_expanded {
                        candidates.push(evaluated);
                    }

                    for candidate in candidates {
                        match self.resolve_property_access_with_env(candidate, property_name) {
                            PropertyAccessResult::Success {
                                type_id,
                                write_type,
                                ..
                            } => {
                                return self.finalize_property_access_result(
                                    idx,
                                    effective_write_result(type_id, write_type),
                                    skip_flow_narrowing,
                                    false,
                                );
                            }
                            PropertyAccessResult::PossiblyNullOrUndefined {
                                property_type: Some(type_id),
                                ..
                            } => {
                                return self.finalize_property_access_result(
                                    idx,
                                    type_id,
                                    skip_flow_narrowing,
                                    false,
                                );
                            }
                            _ => {}
                        }
                    }
                }
                if self.is_array_constructor_is_array_recovery(access.expression, property_name) {
                    return self.finalize_property_access_result(
                        idx,
                        TypeId::ANY,
                        skip_flow_narrowing,
                        false,
                    );
                }
                if self.is_stale_unconstrained_type_parameter(object_type_for_access) {
                    // Stale type parameter from two-pass resolution.
                    // The updated version in scope has a constraint (likely ERROR),
                    // so suppress the cascading TS2339.
                    return TypeId::ERROR;
                }

                let early_class_recovery = self
                    .resolve_class_access_with_current_member_initializer_recovery(
                        access.expression,
                        object_type_for_access,
                    );
                let mut early_class_chain_summary = None;
                if let Some(member_type) = self.recover_property_from_class_chain_summary(
                    early_class_recovery.1,
                    early_class_recovery.0,
                    &mut early_class_chain_summary,
                    property_name,
                ) {
                    return self.finalize_property_access_result(
                        idx,
                        member_type,
                        skip_flow_narrowing,
                        false,
                    );
                }

                // Special case: unconstrained type parameters should emit TS2339
                // because they have no properties by definition.
                if crate::query_boundaries::state::checking::is_type_parameter_like(
                    self.ctx.types,
                    object_type_for_access,
                ) && crate::query_boundaries::common::type_parameter_constraint(
                    self.ctx.types,
                    object_type_for_access,
                )
                .is_none()
                {
                    if !property_name.starts_with('#') && !accessibility_error_emitted {
                        self.error_property_not_exist_at(
                            property_name,
                            object_type_for_access,
                            access.name_or_argument,
                        );
                    }
                    return TypeId::ERROR;
                }

                // For JS files with checkJs enabled, when accessing properties on
                // new expression results that don't exist, emit TS2339 instead of
                // falling through to expando/any fallbacks. This ensures proper
                // error reporting for imported class instances like `new A().foo`.
                if self.is_js_file()
                    && self.ctx.compiler_options.check_js
                    && !skip_flow_narrowing
                    && !accessibility_error_emitted
                    && !property_name.starts_with('#')
                {
                    // Check if the object expression is a new expression
                    let is_new_expression = self
                        .ctx
                        .arena
                        .get(access.expression)
                        .is_some_and(|n| n.kind == syntax_kind_ext::NEW_EXPRESSION);

                    if is_new_expression {
                        self.error_property_not_exist_at(
                            property_name,
                            object_type_for_access,
                            access.name_or_argument,
                        );
                        return TypeId::ERROR;
                    }
                }

                let (resolved_class_access, current_class_member_initializer_receiver) = self
                    .resolve_class_access_with_current_member_initializer_recovery(
                        access.expression,
                        object_type_for_access,
                    );
                let mut class_chain_summary = None;
                let static_this_member_context = is_this_access
                    && (self
                        .find_enclosing_static_block(access.expression)
                        .is_some()
                        || self
                            .find_enclosing_function(access.expression)
                            .map(|func_idx| {
                                let mut member_idx = func_idx;
                                if let Some(func_node) = self.ctx.arena.get(func_idx)
                                    && (func_node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                                        || func_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION)
                                    && let Some(ext) = self.ctx.arena.get_extended(func_idx)
                                    && let Some(parent_node) = self.ctx.arena.get(ext.parent)
                                    && (parent_node.kind == syntax_kind_ext::METHOD_DECLARATION
                                        || parent_node.kind == syntax_kind_ext::GET_ACCESSOR
                                        || parent_node.kind == syntax_kind_ext::SET_ACCESSOR)
                                {
                                    member_idx = ext.parent;
                                }
                                self.class_member_is_static(member_idx)
                            })
                            .unwrap_or(false));

                if !access.question_dot_token
                    && static_this_member_context
                    && let Some(class_idx) = self.nearest_enclosing_class(access.expression)
                {
                    let summary = self.summarize_class_chain(class_idx);
                    if let Some(member_info) = summary.member_info(property_name, true, true) {
                        return self.finalize_property_access_result(
                            idx,
                            effective_write_result(member_info.type_id, Some(member_info.type_id)),
                            skip_flow_narrowing,
                            false,
                        );
                    }
                    if summary.member_info(property_name, false, true).is_some() {
                        self.error_property_not_exist_at(
                            property_name,
                            object_type_for_access,
                            access.name_or_argument,
                        );
                        return TypeId::ERROR;
                    }
                }
                if !access.question_dot_token
                    && is_this_access
                    && let Some((class_idx, is_static_access)) = resolved_class_access
                    && is_static_access
                    && self
                        .class_chain_member_kind_name_only(class_idx, property_name, true, true)
                        .is_none()
                    && self
                        .class_chain_member_kind_name_only(class_idx, property_name, false, true)
                        .is_some()
                {
                    self.error_property_not_exist_at(
                        property_name,
                        object_type_for_access,
                        access.name_or_argument,
                    );
                    return TypeId::ERROR;
                }

                if let Some(augmented_type) = self.resolve_array_global_augmentation_property(
                    object_type_for_access,
                    property_name,
                ) {
                    return self.finalize_property_access_result(
                        idx,
                        augmented_type,
                        skip_flow_narrowing,
                        false,
                    );
                }
                if let Some(augmented_type) = self.resolve_general_global_augmentation_property(
                    object_type_for_access,
                    property_name,
                ) {
                    return self.finalize_property_access_result(
                        idx,
                        augmented_type,
                        skip_flow_narrowing,
                        false,
                    );
                }
                if let Some(augmented_type) =
                    self.resolve_module_augmentation_property(object_type_for_access, property_name)
                {
                    return self.finalize_property_access_result(
                        idx,
                        augmented_type,
                        skip_flow_narrowing,
                        false,
                    );
                }
                if crate::query_boundaries::property_access::is_function_type(
                    self.ctx.types,
                    object_type_for_access,
                ) && let Some(func_iface) = self.resolve_lib_type_by_name("Function")
                    && let PropertyAccessResult::Success { type_id, .. } =
                        self.resolve_property_access_with_env(func_iface, property_name)
                {
                    return self.finalize_property_access_result(
                        idx,
                        type_id,
                        skip_flow_narrowing,
                        false,
                    );
                }
                if let Some(result) = self.resolve_mixin_static_member_property_access(
                    idx,
                    access.expression,
                    object_type_for_access,
                    property_name,
                    skip_flow_narrowing,
                ) {
                    return result;
                }
                if let Some((class_idx, is_static_access)) = resolved_class_access
                    && !is_static_access
                    && let Some(interface_type) =
                        self.recover_property_from_implemented_interfaces(class_idx, property_name)
                {
                    return self.finalize_property_access_result(
                        idx,
                        interface_type,
                        skip_flow_narrowing,
                        false,
                    );
                }
                if let Some(member_type) = self.recover_property_from_class_chain_summary(
                    current_class_member_initializer_receiver,
                    resolved_class_access,
                    &mut class_chain_summary,
                    property_name,
                ) {
                    return self.finalize_property_access_result(
                        idx,
                        member_type,
                        skip_flow_narrowing,
                        false,
                    );
                }
                if access.question_dot_token {
                    return TypeId::UNDEFINED;
                }
                if property_name == "exports"
                    && self.is_js_file()
                    && let Some(obj_node) = self.ctx.arena.get(access.expression)
                    && let Some(ident) = self.ctx.arena.get_identifier(obj_node)
                    && ident.escaped_text == "module"
                    && self.current_file_commonjs_module_identifier_is_unshadowed(access.expression)
                {
                    return self.current_file_commonjs_namespace_type();
                }
                if self.is_js_file()
                    && self.ctx.compiler_options.check_js
                    && skip_flow_narrowing
                    && self.property_access_is_direct_write_target(idx)
                    && let Some(jsdoc_type) = self
                        .enclosing_expression_statement(idx)
                        .and_then(|stmt_idx| self.js_statement_declared_type(stmt_idx))
                        .or_else(|| self.jsdoc_type_annotation_for_node_direct(idx))
                {
                    return jsdoc_type;
                }
                let checked_js_write_has_non_expando_global_type = skip_flow_narrowing
                    && self.property_access_is_direct_write_target(idx)
                    && self.is_js_file()
                    && self.ctx.compiler_options.check_js
                    && self
                        .ctx
                        .arena
                        .get(access.expression)
                        .is_some_and(|expr_node| expr_node.kind == SyntaxKind::Identifier as u16)
                    && self
                        .ctx
                        .arena
                        .get_identifier_at(access.expression)
                        .is_some_and(|ident| {
                            self.cross_file_global_value_type_by_name(&ident.escaped_text, false)
                                .is_some_and(|preferred_type| {
                                    preferred_type != TypeId::ANY
                                        && preferred_type != TypeId::UNKNOWN
                                        && preferred_type != TypeId::ERROR
                                        && !crate::query_boundaries::common::is_function_type(
                                            self.ctx.types,
                                            preferred_type,
                                        )
                                })
                        });
                if self.is_js_file()
                    && self.ctx.compiler_options.check_js
                    && !checked_js_write_has_non_expando_global_type
                    && !self.property_access_root_is_imported_namespace(access.expression)
                    && let Some(expr_text) = self.expression_text(idx)
                    && let Some(jsdoc_type) =
                        if skip_flow_narrowing && self.property_access_is_direct_write_target(idx) {
                            self.resolve_jsdoc_assigned_value_type_for_write(&expr_text)
                        } else {
                            self.resolve_jsdoc_declared_assigned_value_type(&expr_text)
                        }
                {
                    return jsdoc_type;
                }
                if js_expando_before_assignment && !checked_js_write_has_non_expando_global_type {
                    return TypeId::ANY;
                }
                if skip_flow_narrowing
                    && self.is_js_file()
                    && self.ctx.compiler_options.check_js
                    && self.property_access_is_direct_write_target(idx)
                    && !checked_js_write_has_non_expando_global_type
                    && !commonjs_named_props_disallowed
                    && self.is_expando_property_read(access.expression, property_name)
                {
                    return TypeId::ANY;
                }
                // Check for expando property reads: X.prop where X.prop = value was assigned
                // Recover the assigned value type when we can, then fall back to `any`.
                if !skip_flow_narrowing
                    && !commonjs_named_props_disallowed
                    && self.is_expando_property_read(access.expression, property_name)
                {
                    if let Some(expando_type) =
                        self.expando_property_read_type(idx, access.expression, property_name)
                    {
                        return expando_type;
                    }
                    return TypeId::ANY;
                }
                // Check for expando function pattern: func.prop = value
                // TypeScript allows property assignments to function/class declarations
                // without emitting TS2339. The assigned properties become part of the
                // function's type (expando pattern).
                let static_class_this_write = is_this_access
                    && resolved_class_access.is_some_and(|(_, is_static_access)| is_static_access);
                if !commonjs_named_props_disallowed
                    && !static_class_this_write
                    && !checked_js_write_has_non_expando_global_type
                    && self.is_expando_function_assignment(
                        idx,
                        access.expression,
                        object_type_for_access,
                    )
                {
                    return TypeId::ANY;
                }
                if !commonjs_named_props_disallowed
                    && self.is_js_expando_object_assignment(
                        idx,
                        access.expression,
                        object_type_for_access,
                        property_name,
                    )
                    && !checked_js_write_has_non_expando_global_type
                {
                    return TypeId::ANY;
                }

                // JavaScript files allow dynamic property assignment on 'this' without errors.
                // In JS files, accessing a property on 'this' that doesn't exist should not error
                // and should return 'any' type, matching TypeScript's behavior.
                let has_explicit_this_context = is_this_access
                    && self
                        .current_this_type()
                        .is_some_and(|ty| ty != TypeId::ANY && ty != TypeId::UNKNOWN);
                let this_direct_write_rhs_is_void_zero = is_this_access
                    && self
                        .property_access_direct_write_rhs(idx)
                        .is_some_and(|rhs| self.js_assignment_rhs_is_void_zero(rhs));
                let has_jsdoc_this_context = is_this_access
                    && self.is_js_file()
                    && self.enclosing_function_has_jsdoc_this_tag(access.expression);
                // When `this` type comes from a ThisType<T> marker (e.g., Vue 2
                // Options API pattern), property access on unresolved type parameters
                // should not emit TS2339. The type parameters will be inferred from the
                // object literal, creating a circular dependency that tsc handles by
                // deferring the check.
                // Also handle intersections containing type parameters (e.g.,
                // `Data & Readonly<Props> & Instance` from
                // `ThisType<Data & Readonly<Props> & Instance>` before inference).
                // Only suppress when `this` doesn't have an explicit type context
                // to ensure we still emit TS2339 for regular object literal methods.
                let this_owner_is_object_literal = self
                    .this_has_contextual_owner(access.expression)
                    .and_then(|owner_idx| self.ctx.arena.get(owner_idx))
                    .is_some_and(|owner_node| {
                        owner_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                    });
                let this_prototype_owner_expr = self
                    .find_enclosing_non_arrow_function(access.expression)
                    .and_then(|func_idx| self.js_prototype_owner_expression_for_node(func_idx));
                let this_owner_is_js_prototype_method =
                    this_prototype_owner_expr.is_some_and(|owner_expr| {
                        self.js_prototype_owner_function_target(owner_expr)
                            .is_some()
                    });
                let this_owner_is_external_js_prototype_method = this_prototype_owner_expr
                    .is_some_and(|owner_expr| {
                        self.js_prototype_owner_function_target(owner_expr)
                            .is_none()
                    });
                if is_this_access
                    && this_owner_is_object_literal
                    && !has_explicit_this_context
                    && self.ctx.this_type_stack.last().is_some_and(|&top| {
                        access_query::is_this_type(self.ctx.types, top)
                            || crate::query_boundaries::common::contains_type_parameters(
                                self.ctx.types,
                                top,
                            )
                            || crate::query_boundaries::common::contains_type_parameters(
                                self.ctx.types,
                                top,
                            )
                    })
                {
                    return TypeId::ANY;
                }
                if self.is_js_file()
                    && is_this_access
                    && this_owner_is_js_prototype_method
                    && self.property_access_is_direct_write_target(idx)
                {
                    return TypeId::ANY;
                }
                if self.is_js_file()
                    && is_this_access
                    && skip_flow_narrowing
                    && self.property_access_is_direct_write_target(idx)
                    && !this_owner_is_external_js_prototype_method
                {
                    let object_literal_owned_this = self
                        .this_has_contextual_owner(access.expression)
                        .and_then(|owner_idx| self.ctx.arena.get(owner_idx))
                        .is_some_and(|owner_node| {
                            owner_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                        });
                    let prototype_object_literal_expando_write = object_literal_owned_this
                        && self.is_js_prototype_object_literal_expando_write(
                            access.expression,
                            property_name,
                        );
                    // A function expression assigned as the RHS of a property/
                    // element write (`o.m = function () { this.q = 1; }`) gets a
                    // real contextual `this` from the assignment's base object
                    // (#16964/#16978) — tsc checks writes against that base type
                    // (TS2339 for an absent member) instead of treating them as
                    // loose JS expando writes. This predates that mechanism and
                    // otherwise blanket-suppresses every JS `this`-write; carve
                    // out the assignment-receiver shape specifically so the
                    // narrower JSDoc-`@this`/`this:`-parameter void-zero carve-out
                    // below is untouched.
                    let this_is_property_assignment_receiver = self
                        .enclosing_function_assignment_rhs_this_type(access.expression)
                        .is_some();
                    if !(has_jsdoc_this_context
                        || (object_literal_owned_this && !prototype_object_literal_expando_write)
                        || (has_explicit_this_context && this_direct_write_rhs_is_void_zero)
                        || this_is_property_assignment_receiver)
                    {
                        return TypeId::ANY;
                    }
                }
                if self.is_js_file() && is_this_access && !has_explicit_this_context {
                    // Allow dynamic property on `this` in loose JS contexts, but
                    // keep checks when `this` is contextually owned by a class/object
                    // member (checkJs should still enforce member-consistent typing).
                    if self.this_has_contextual_owner(access.expression).is_none() {
                        return TypeId::ANY;
                    }
                }
                // Same structural rule as the `is_namespace_value_type` branch
                // above: a bare namespace's `typeof` type has no implicit
                // `prototype` member (`declare namespace C { ... } C.prototype = {}`
                // is a real `TS2339`, oracle-verified). The exemption only
                // applies when the root symbol is ALSO callable (function/class),
                // the constructor-plus-namespace idiom.
                if self.is_js_file()
                    && property_name == "prototype"
                    && self.property_access_is_direct_write_target(idx)
                    && self
                        .resolve_identifier_symbol(access.expression)
                        .and_then(|sym_id| self.ctx.binder.get_symbol(sym_id))
                        .is_some_and(|sym| {
                            !(sym.has_any_flags(symbol_flags::ALIAS)
                                && sym.import_module().is_some())
                                && sym.has_any_flags(symbol_flags::FUNCTION | symbol_flags::CLASS)
                        })
                    && self.js_prototype_write_root_is_callable_or_constructible(access.expression)
                {
                    return TypeId::ANY;
                }
                if self.is_js_file()
                    && self.is_super_expression(access.expression)
                    && let Some((class_idx, is_static_access)) = resolved_class_access
                    && is_static_access
                    && matches!(
                        self.class_chain_member_kind_name_only(
                            class_idx,
                            property_name,
                            true,
                            true,
                        )
                        .map(|(kind, _)| kind),
                        Some(ClassMemberKind::Field)
                    )
                {
                    return TypeId::ANY;
                }
                // A `super.member` access that does not resolve on the receiver
                // side reaches the nonexistent-property diagnostics exactly like
                // an ordinary access. `resolved_class_access` already carries the
                // receiver's static-ness for super: an instance-context `super.`
                // reads the base *instance* type (`is_static_access == false`), a
                // static-context `super.` reads the base *constructor* type
                // (`is_static_access == true`). So the shared TS2576/TS2339 rule
                // below applies to super with no super-specific gate — matching
                // the element-access path in `computation/access.rs`.
                if let Some((class_idx, is_static_access)) = resolved_class_access
                    && is_static_access
                {
                    if class_chain_summary.is_none() {
                        class_chain_summary = Some(self.summarize_class_chain(class_idx));
                    }
                    if let Some(member_info) = class_chain_summary
                        .as_ref()
                        .and_then(|summary| summary.member_info(property_name, true, true))
                    {
                        return self.finalize_property_access_result(
                            idx,
                            effective_write_result(member_info.type_id, Some(member_info.type_id)),
                            skip_flow_narrowing,
                            false,
                        );
                    }
                }

                // A `super.member` miss reaches the nonexistent-property
                // diagnostics only when `super` is itself a valid reference. When
                // `super` is invalid — a grammar error on the keyword (TS1034, e.g.
                // `super` in a parameter default → `superAccess2.ts` reports TS1034
                // only), no enclosing derived class (TS2335), or a regular-function
                // boundary between `super` and its class member (TS2660, e.g.
                // `typeOfThisInStaticMembers9.ts` `function () { return super.f }`)
                // — tsc reports only that super-validity diagnostic and suppresses
                // the member lookup, so the dependent TS2576/TS2339 must be
                // suppressed too. `super_property_reference_is_valid` mirrors the
                // gates `check_super_expression` applies (parse-error short-circuit
                // + derived-class + valid-member-context), keeping the two super
                // diagnostic families consistent.
                let super_reference_invalid = self.is_super_expression(access.expression)
                    && !self.super_property_reference_is_valid(access.expression);

                // TS2576: an instance receiver (`instance.member` or an
                // instance-context `super.member`) where `member` exists on the
                // class static side. This diagnostic only needs to know whether a
                // static member exists, not its full type. `super` is included:
                // its instance-context receiver is the base instance type, so a
                // static-only base member is genuinely absent from the receiver
                // and tsc suggests the static member here too (`superAccess.ts`
                // `super.S1`, `superPropertyAccess2.ts` `super.bar` in the ctor).
                if let Some((class_idx, is_static_access)) = resolved_class_access
                    && !is_static_access
                    && !super_reference_invalid
                    && self
                        .class_chain_member_kind_name_only(class_idx, property_name, true, true)
                        .is_some()
                {
                    use crate::diagnostics::{
                        diagnostic_codes, diagnostic_messages, format_message,
                    };

                    let object_type_str =
                        self.format_type_for_assignability_message(display_object_type);
                    let static_member_name = format!("{object_type_str}.{property_name}");
                    let message = format_message(
                    diagnostic_messages::PROPERTY_DOES_NOT_EXIST_ON_TYPE_DID_YOU_MEAN_TO_ACCESS_THE_STATIC_MEMBER_INSTEAD,
                    &[property_name, &object_type_str, &static_member_name],
                );
                    // Report at the property name node, not the full expression (matches tsc behavior)
                    self.error_at_node(
                    access.name_or_argument,
                    &message,
                    diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE_DID_YOU_MEAN_TO_ACCESS_THE_STATIC_MEMBER_INSTEAD,
                );
                    return TypeId::ERROR;
                }

                // Don't emit TS2339 for private fields (starting with #) - they're handled elsewhere.
                // Also suppress when accessibility check already emitted TS2341/TS2445
                // (property exists but is private/protected — not truly "not found").
                // A `super.member` miss that exists on the opposite static/instance
                // side is NOT suppressed: tsc reports it through the same
                // nonexistent-property path. The TS2576 branch above already
                // claims the instance-receiver + static-side case; what remains
                // here is the static-receiver + instance-side case, which tsc
                // reports as plain TS2339 against the receiver (`typeof C` in a
                // static context, e.g. `superPropertyAccess2.ts` `super.x` in a
                // static method).
                // Also suppress TS2339 when base expression is a property access on an unresolved import
                // (TS2307 was already emitted for the missing module).
                // Suppress TS2339 when evaluating a computed property name
                // inside a class that is currently being constructed. The
                // property lookup may fail because the class instance type
                // hasn't been fully registered yet (circular reference).
                // tsc handles this gracefully and does not emit TS2339.
                let in_circular_computed_property =
                    self.ctx.checking_computed_property_name.is_some()
                        && !self.ctx.class_instance_resolution_set.is_empty();
                let in_current_class_construction = self.has_recoverable_current_class_member(
                    current_class_member_initializer_receiver,
                    resolved_class_access,
                    &mut class_chain_summary,
                    property_name,
                );
                if !property_name.starts_with('#')
                    && !accessibility_error_emitted
                    && !super_reference_invalid
                    && !self.is_property_access_on_unresolved_import(access.expression)
                    && !in_circular_computed_property
                    && !in_current_class_construction
                {
                    if self.is_js_file()
                        && self.is_current_file_commonjs_export_base(access.expression)
                    {
                        let export_namespace_type = self.current_file_commonjs_namespace_type();
                        display_object_type = export_namespace_type;
                        if let PropertyAccessResult::Success {
                            type_id,
                            write_type,
                            ..
                        } = self
                            .resolve_property_access_with_env(export_namespace_type, property_name)
                        {
                            return self.finalize_property_access_result(
                                idx,
                                effective_write_result(type_id, write_type),
                                skip_flow_narrowing,
                                false,
                            );
                        }
                    }
                    // Property access expressions are VALUE context - always emit TS2339.
                    // TS2694 (namespace has no exported member) is for TYPE context only,
                    // which is handled separately in type name resolution.
                    // Use display_object_type to preserve literal types in error messages
                    // while maintaining nominal identity (e.g., D<string>)
                    // Report at the property name node, not the full expression (matches tsc behavior)
                    if let Some(sym_id) = self.resolve_qualified_symbol(access.expression)
                        && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                        && symbol.has_any_flags(tsz_binder::symbol_flags::ENUM)
                        && !symbol.has_any_flags(tsz_binder::symbol_flags::ENUM_MEMBER)
                    {
                        self.error_property_not_exist_on_enum(
                            property_name,
                            &symbol.escaped_name.to_string(),
                            display_object_type,
                            access.name_or_argument,
                        );
                        return TypeId::ERROR;
                    }

                    if let Some(type_id) = self.declared_receiver_property_type(
                        access.expression,
                        display_object_type,
                        property_name,
                    ) {
                        return type_id;
                    }

                    if enum_instance_like_access {
                        let enum_display: Option<String> = access_query::type_parameter_constraint(
                            self.ctx.types,
                            display_object_type,
                        )
                        .filter(|constraint| {
                            access_query::enum_def_id(self.ctx.types, *constraint).is_some()
                        })
                        .map(|constraint| self.format_type_for_assignability_message(constraint))
                        .or_else(|| {
                            access_query::enum_def_id(self.ctx.types, display_object_type).map(
                                |_| self.format_type_for_assignability_message(display_object_type),
                            )
                        });
                        if let Some(enum_display) = enum_display {
                            self.error_property_not_exist_with_apparent_type(
                                property_name,
                                &enum_display,
                                access.name_or_argument,
                            );
                        } else {
                            // Suppress TS2339 for index access types (like T[keyof T])
                            // or for unknown/error types that result from unresolved generics.
                            //
                            // Do not suppress bare type parameters here: tsc reports property
                            // misses on generic receivers and formats constrained type parameters
                            // through their constraint (for example, `T extends A | B` reports
                            // the miss on `A | B`).
                            let should_suppress_inner =
                                crate::query_boundaries::common::is_index_access_type(
                                    self.ctx.types,
                                    display_object_type,
                                ) || display_object_type == TypeId::UNKNOWN
                                    || display_object_type == TypeId::ERROR;
                            if !should_suppress_inner {
                                self.error_property_not_exist_at(
                                    property_name,
                                    self.diagnostic_display_type_for_missing_property(
                                        object_type,
                                        display_object_type,
                                    ),
                                    access.name_or_argument,
                                );
                            }
                        }
                    } else {
                        // Suppress TS2339 for IndexAccess display (T[keyof T]) when the
                        // evaluated receiver still contains type parameters; tsc emits
                        // TS2339 once the access resolves to a concrete shape (e.g. E[K]
                        // → A | B). Also suppress for unknown/error fallbacks. Bare type
                        // parameters are NOT suppressed — tsc reports property misses on
                        // generic receivers via their constraint.
                        let display_is_index_access =
                            crate::query_boundaries::common::is_index_access_type(
                                self.ctx.types,
                                display_object_type,
                            );
                        let evaluated_receiver_is_resolved =
                            !crate::query_boundaries::common::contains_type_parameters(
                                self.ctx.types,
                                object_type_for_access,
                            );
                        let should_suppress = (display_is_index_access
                            && !evaluated_receiver_is_resolved)
                            || display_object_type == TypeId::UNKNOWN
                            || display_object_type == TypeId::ERROR;
                        if !should_suppress {
                            self.error_property_not_exist_at(
                                property_name,
                                self.diagnostic_display_type_for_missing_property(
                                    object_type,
                                    display_object_type,
                                ),
                                access.name_or_argument,
                            );
                        }
                    }
                }
                if in_current_class_construction {
                    return TypeId::ANY;
                }
                if receiver_has_daa_error {
                    return self.finalize_property_access_result(
                        idx,
                        TypeId::ERROR,
                        skip_flow_narrowing,
                        false,
                    );
                }
                TypeId::ERROR
            }

            PropertyAccessResult::PossiblyNullOrUndefined {
                property_type,
                cause,
            } => self.handle_possibly_null_or_undefined_access(
                super::nullish_access::NullishAccessSite {
                    idx,
                    expression: access.expression,
                    name_or_argument: access.name_or_argument,
                    question_dot_token: access.question_dot_token,
                    property_type,
                    cause,
                    object_type_for_access,
                    property_name,
                    skip_flow_narrowing,
                    receiver_has_daa_error,
                },
            ),

            PropertyAccessResult::IsUnknown => {
                // Shared unknown-object decision gate (TS18046/TS2571 under
                // strictNullChecks; the `Object.prototype` apparent-surface
                // fallback otherwise). `None` means the property is genuinely
                // missing even from that surface, so property access reports
                // it instead of falling through to index signatures the way
                // element access does.
                if let Some(result) = self
                    .unknown_object_access_result(access.expression, Some(property_name.as_str()))
                {
                    return result;
                }
                self.error_property_not_exist_at(
                    property_name,
                    object_type_for_access,
                    access.name_or_argument,
                );
                TypeId::ERROR
            }
        }
    }
}
