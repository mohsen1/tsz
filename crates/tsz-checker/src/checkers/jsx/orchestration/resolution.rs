//! JSX namespace/symbol resolution, element type lookups, intrinsic elements,
//! closing element checks, children contextual type, and attribute name extraction.

use crate::context::TypingRequest;
use crate::state::CheckerState;
use crate::symbols_domain::name_text::entity_name_text_in_arena;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(in crate::checkers_domain::jsx) fn get_default_instantiated_generic_class_props_type(
        &mut self,
        component_type: TypeId,
    ) -> Option<TypeId> {
        use crate::query_boundaries::common::PropertyAccessResult;

        let sigs = crate::query_boundaries::common::construct_signatures_for_type(
            self.ctx.types,
            component_type,
        )?;
        let generic: Vec<_> = sigs
            .iter()
            .filter(|sig| !sig.type_params.is_empty())
            .collect();
        if generic.len() != 1 {
            return None;
        }

        let sig = generic[0];
        let props = sig.params.first().map(|param| param.type_id).or_else(|| {
            let evaluated_return_type = self.evaluate_type_with_env(sig.return_type);
            match self.get_element_attributes_property_name_with_check(None) {
                None => match self.resolve_property_access_with_env(sig.return_type, "props") {
                    PropertyAccessResult::Success { type_id, .. } => Some(type_id),
                    _ => {
                        match self.resolve_property_access_with_env(evaluated_return_type, "props")
                        {
                            PropertyAccessResult::Success { type_id, .. } => Some(type_id),
                            _ => None,
                        }
                    }
                },
                Some(name) if name.is_empty() => Some(sig.return_type),
                Some(name) => match self.resolve_property_access_with_env(sig.return_type, &name) {
                    PropertyAccessResult::Success { type_id, .. } => Some(type_id),
                    _ => {
                        match self.resolve_property_access_with_env(evaluated_return_type, &name) {
                            PropertyAccessResult::Success { type_id, .. } => Some(type_id),
                            _ => None,
                        }
                    }
                },
            }
        })?;

        let type_args: Vec<_> = sig
            .type_params
            .iter()
            .map(|param| {
                param
                    .default
                    .or(param.constraint)
                    .unwrap_or(TypeId::UNKNOWN)
            })
            .collect();
        let substitution = crate::query_boundaries::common::TypeSubstitution::from_args(
            self.ctx.types,
            &sig.type_params,
            &type_args,
        );
        let instantiated =
            crate::query_boundaries::common::instantiate_type(self.ctx.types, props, &substitution);
        let evaluated =
            if crate::query_boundaries::common::is_union_type(self.ctx.types, instantiated)
                || crate::computation::call_inference::should_preserve_contextual_application_shape(
                    self.ctx.types,
                    instantiated,
                )
            {
                instantiated
            } else {
                self.evaluate_type_with_env(instantiated)
            };
        let managed = self.apply_jsx_library_managed_attributes(component_type, evaluated);
        if managed == TypeId::ANY
            || managed == TypeId::UNKNOWN
            || managed == TypeId::ERROR
            || crate::query_boundaries::common::contains_type_parameters(self.ctx.types, managed)
        {
            None
        } else {
            Some(managed)
        }
    }

    pub(in crate::checkers_domain::jsx) fn infer_jsx_generic_class_component_signature(
        &mut self,
        _element_idx: NodeIndex,
        component_type: TypeId,
    ) -> Option<tsz_solver::FunctionShape> {
        let call_sig = crate::query_boundaries::common::construct_signatures_for_type(
            self.ctx.types,
            component_type,
        )?
        .first()?
        .clone();
        let mut function_shape = tsz_solver::FunctionShape {
            type_params: call_sig.type_params,
            params: call_sig.params,
            this_type: call_sig.this_type,
            return_type: call_sig.return_type,
            type_predicate: call_sig.type_predicate,
            is_constructor: true,
            is_method: call_sig.is_method,
        };
        if function_shape.type_params.is_empty() {
            return None;
        }

        if function_shape.params.is_empty() {
            use crate::query_boundaries::common::PropertyAccessResult;

            let evaluated_return_type = self.evaluate_type_with_env(function_shape.return_type);
            let synthesized_param_type = match self
                .get_element_attributes_property_name_with_check(None)
            {
                None => {
                    match self.resolve_property_access_with_env(function_shape.return_type, "props")
                    {
                        PropertyAccessResult::Success { type_id, .. } => Some(type_id),
                        _ => match self
                            .resolve_property_access_with_env(evaluated_return_type, "props")
                        {
                            PropertyAccessResult::Success { type_id, .. } => Some(type_id),
                            _ => None,
                        },
                    }
                }
                Some(name) if name.is_empty() => Some(function_shape.return_type),
                Some(name) => {
                    match self.resolve_property_access_with_env(function_shape.return_type, &name) {
                        PropertyAccessResult::Success { type_id, .. } => Some(type_id),
                        _ => match self
                            .resolve_property_access_with_env(evaluated_return_type, &name)
                        {
                            PropertyAccessResult::Success { type_id, .. } => Some(type_id),
                            _ => None,
                        },
                    }
                }
            }
            .filter(|type_id| !matches!(*type_id, TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN));

            if let Some(type_id) = synthesized_param_type {
                let props_name = self.ctx.types.intern_string("props");
                function_shape
                    .params
                    .push(tsz_solver::ParamInfo::required(props_name, type_id));
            }
        }

        if function_shape.params.is_empty() {
            return None;
        }

        Some(function_shape)
    }

    /// Get the type of a JSX opening element (Rule #36: case-sensitive tag lookup).
    #[allow(dead_code)]
    pub(crate) fn get_type_of_jsx_opening_element(&mut self, idx: NodeIndex) -> TypeId {
        self.get_type_of_jsx_opening_element_with_children(idx, &TypingRequest::NONE, None)
    }

    pub(crate) fn get_type_of_jsx_opening_element_with_children(
        &mut self,
        idx: NodeIndex,
        request: &TypingRequest,
        children_ctx: Option<crate::checkers_domain::JsxChildrenContext>,
    ) -> TypeId {
        self.check_jsx_factory_in_scope(idx);
        self.check_jsx_import_source(idx);

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ANY;
        };
        let Some(jsx_opening) = self.ctx.arena.get_jsx_opening(node) else {
            return TypeId::ANY;
        };
        let tag_name_idx = jsx_opening.tag_name;
        let Some(tag_name_node) = self.ctx.arena.get(tag_name_idx) else {
            return TypeId::ANY;
        };
        // Namespaced tags (e.g., `svg:path`) are always intrinsic.
        let (tag_name, namespaced_tag_owned, is_intrinsic) = if tag_name_node.kind
            == tsz_scanner::SyntaxKind::Identifier as u16
        {
            let name = self
                .ctx
                .arena
                .get_identifier(tag_name_node)
                .map(|id| id.escaped_text.as_str());
            let intrinsic = name
                .as_ref()
                .is_some_and(|n| n.chars().next().is_some_and(|c| c.is_ascii_lowercase()));
            (name, None::<String>, intrinsic)
        } else if tag_name_node.kind == syntax_kind_ext::JSX_NAMESPACED_NAME {
            // Namespaced tags like `svg:path` → always intrinsic.
            // Build "namespace:name" string for IntrinsicElements lookup.
            // If the namespace part starts with uppercase (e.g., `<A:foo>`),
            // emit TS2639: React components cannot include JSX namespace names.
            let ns_str = self
                    .ctx
                    .arena
                    .get_jsx_namespaced_name(tag_name_node)
                    .and_then(|ns| {
                        let ns_id = self.ctx.arena.get(ns.namespace)?;
                        let ns_text = self.ctx.arena.get_identifier(ns_id)?.escaped_text.as_str();
                        // TS2639: React components (uppercase first char) cannot use
                        // namespace names. Only in React-based JSX modes.
                        if ns_text
                            .chars()
                            .next()
                            .is_some_and(|c| c.is_ascii_uppercase())
                        {
                            use tsz_common::checker_options::JsxMode;
                            let jsx_mode = self.effective_jsx_mode();
                            if matches!(jsx_mode, JsxMode::React | JsxMode::ReactJsx | JsxMode::ReactJsxDev) {
                                self.error_at_node(
                                    tag_name_idx,
                                    crate::diagnostics::diagnostic_messages::REACT_COMPONENTS_CANNOT_INCLUDE_JSX_NAMESPACE_NAMES,
                                    crate::diagnostics::diagnostic_codes::REACT_COMPONENTS_CANNOT_INCLUDE_JSX_NAMESPACE_NAMES,
                                );
                            }
                        }
                        let name_id = self.ctx.arena.get(ns.name)?;
                        let name_text = self
                            .ctx
                            .arena
                            .get_identifier(name_id)?
                            .escaped_text
                            .as_str();
                        Some(format!("{ns_text}:{name_text}"))
                    });
            (None, ns_str, true)
        } else {
            // Property access expression (e.g., React.Component)
            (None, None, false)
        };
        // Unify: for namespaced tags, use the owned string; for simple tags, use the borrowed &str.
        let effective_tag: Option<&str> = tag_name.or(namespaced_tag_owned.as_deref());

        if is_intrinsic {
            let ie_type = self.get_intrinsic_elements_type();
            // Intrinsic elements: look up JSX.IntrinsicElements[tagName]
            if let Some(tag) = effective_tag
                && ie_type.is_some()
            {
                let evaluated_props = self
                    .get_jsx_intrinsic_props_for_tag(idx, tag, true)
                    .unwrap_or(TypeId::ANY);

                // Check JSX attributes against the resolved props type.
                // For intrinsic elements, the display target is just the props type
                // (tsc doesn't wrap intrinsic element props in IntrinsicAttributes).
                let display_target = self.build_jsx_display_target(evaluated_props, None);
                self.check_jsx_attributes_against_props(
                    jsx_opening.attributes,
                    evaluated_props,
                    jsx_opening.tag_name,
                    None,
                    None,
                    false, // intrinsic elements never have raw type params
                    display_target,
                    None,
                    request,
                    children_ctx,
                );

                // tsc types ALL JSX expressions (both intrinsic and component) as
                // JSX.Element. Returning IntrinsicElements["tag"] causes false TS2322
                // when the JSX expression is used in a context expecting JSX.Element
                // (e.g., as a return value or assigned to a variable of type JSX.Element).
                if let Some(jsx_sym_id) = self.get_jsx_namespace_type() {
                    let lib_binders = self.get_lib_binders();
                    if let Some(symbol) = self
                        .ctx
                        .binder
                        .get_symbol_with_libs(jsx_sym_id, &lib_binders)
                        && let Some(exports) = symbol.exports.as_ref()
                        && let Some(element_sym_id) = exports.get("Element")
                    {
                        return self.type_reference_symbol_type(element_sym_id);
                    }
                }
                return TypeId::ANY;
            }
            // TS7026: JSX element implicitly has type 'any' because no interface 'JSX.IntrinsicElements' exists.
            // tsc emits this unconditionally (regardless of noImplicitAny) when JSX.IntrinsicElements is absent.
            // The word "implicitly" in the message refers to the missing JSX infrastructure, not the noImplicitAny flag.
            //
            // Suppression rules (matching tsc behaviour):
            // 1. ReactJsx/ReactJsxDev modes use jsxImportSource for element types; they do not rely on
            //    the global JSX.IntrinsicElements, so TS7026 must not fire.
            // 2. When @jsxImportSource pragma or jsxImportSource config is set, the JSX namespace
            //    comes from the import source module, not the global scope. TS7026 must not fire.
            // 3. When the file has parser-level errors (e.g. malformed JSX attributes → TS1145),
            //    tsc suppresses TS7026 to avoid double-reporting in error-recovery situations.
            let suppress_for_import_source = self.should_suppress_ts7026_for_import_source();
            let file_has_any_parse_diag =
                self.ctx.has_parse_errors || !self.ctx.all_parse_error_positions.is_empty();
            let recovered_adjacent_sibling =
                self.file_has_same_line_adjacent_jsx_recovery_pattern();
            if !suppress_for_import_source
                && !file_has_any_parse_diag
                && !recovered_adjacent_sibling
            {
                use crate::diagnostics::diagnostic_codes;
                self.error_at_node_msg(
                    idx,
                    diagnostic_codes::JSX_ELEMENT_IMPLICITLY_HAS_TYPE_ANY_BECAUSE_NO_INTERFACE_JSX_EXISTS,
                    &["IntrinsicElements"],
                );
            }
            // Grammar check: TS17000 for empty expressions in JSX attributes.
            self.check_grammar_jsx_element(jsx_opening.attributes);

            // Even when IntrinsicElements is missing, evaluate attribute expressions
            // to trigger definite-assignment checks (TS2454) and other diagnostics.
            // tsc evaluates these expressions regardless of JSX infrastructure availability.
            if let Some(attrs_node) = self.ctx.arena.get(jsx_opening.attributes)
                && let Some(attrs) = self.ctx.arena.get_jsx_attributes(attrs_node)
            {
                for &attr_idx in &attrs.properties.nodes {
                    if let Some(attr_node) = self.ctx.arena.get(attr_idx) {
                        if attr_node.kind == syntax_kind_ext::JSX_SPREAD_ATTRIBUTE {
                            if let Some(spread_data) =
                                self.ctx.arena.get_jsx_spread_attribute(attr_node)
                            {
                                self.compute_type_of_node(spread_data.expression);
                            }
                        } else if attr_node.kind == syntax_kind_ext::JSX_ATTRIBUTE
                            && let Some(attr_data) = self.ctx.arena.get_jsx_attribute(attr_node)
                            && attr_data.initializer.is_some()
                        {
                            self.compute_type_of_node(attr_data.initializer);
                        }
                    }
                }
            }
            TypeId::ANY
        } else {
            // Component: resolve as variable expression
            // The tag name is a reference to a component (function or class)
            let component_type = self.compute_type_of_node(tag_name_idx);

            // If the JSX element has explicit type arguments (e.g., <Component<T>>),
            // instantiate the component type with the provided arguments.
            // For function types (SFCs), directly instantiate the function's type params.
            // For other types (class components, type aliases), create an Application type.
            let component_type = if let Some(ref type_args_nodes) = jsx_opening.type_arguments {
                let type_args: Vec<TypeId> = type_args_nodes
                    .nodes
                    .iter()
                    .map(|&arg_idx| self.get_type_from_type_node(arg_idx))
                    .collect();
                if !type_args.is_empty() {
                    self.instantiate_jsx_component_with_type_args(component_type, &type_args)
                } else {
                    component_type
                }
            } else {
                component_type
            };
            let declared_component_type =
                self.get_jsx_identifier_declared_type(tag_name_idx, component_type);
            let prefer_declared_component_type = matches!(
                component_type,
                TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN
            )
                || (!crate::query_boundaries::common::contains_type_parameters(
                    self.ctx.types,
                    component_type,
                ) && crate::query_boundaries::common::contains_type_parameters(
                    self.ctx.types,
                    declared_component_type,
                ));
            let component_type = if prefer_declared_component_type {
                declared_component_type
            } else {
                component_type
            };

            let component_metadata_type =
                self.get_jsx_component_metadata_type(tag_name_idx, component_type);
            let resolved_component_type =
                self.normalize_jsx_component_type_for_resolution(component_type);
            let specific_intrinsic_tag = self.get_jsx_specific_string_literal_component_tag_name(
                tag_name_idx,
                resolved_component_type,
            );
            let tried_specific_intrinsic_lookup =
                specific_intrinsic_tag.is_some() && self.get_intrinsic_elements_type().is_some();

            if let Some(tag) = specific_intrinsic_tag.as_deref()
                && let Some(props_type) = self.get_jsx_intrinsic_props_for_tag(idx, tag, true)
                && props_type != TypeId::ERROR
            {
                let display_target = self.build_jsx_display_target(props_type, None);
                self.check_jsx_attributes_against_props(
                    jsx_opening.attributes,
                    props_type,
                    jsx_opening.tag_name,
                    None,
                    None,
                    false,
                    display_target,
                    None,
                    request,
                    children_ctx,
                );
                if let Some(jsx_sym_id) = self.get_jsx_namespace_type() {
                    let lib_binders = self.get_lib_binders();
                    if let Some(symbol) = self
                        .ctx
                        .binder
                        .get_symbol_with_libs(jsx_sym_id, &lib_binders)
                        && let Some(exports) = symbol.exports.as_ref()
                        && let Some(element_sym_id) = exports.get("Element")
                    {
                        return self.type_reference_symbol_type(element_sym_id);
                    }
                }
                return TypeId::ANY;
            }

            // If the resolved type is string-like or a keyof type (e.g., `keyof ReactHTML`),
            // treat it as an intrinsic element. tsc allows `<Tag>` where Tag has a string
            // type without emitting TS2604.
            if (self.is_jsx_string_tag_type(resolved_component_type)
                || crate::query_boundaries::common::is_keyof_type(
                    self.ctx.types,
                    resolved_component_type,
                ))
                && !tried_specific_intrinsic_lookup
            {
                let needs_dynamic_intrinsic_props_check =
                    crate::query_boundaries::common::contains_type_parameters(
                        self.ctx.types,
                        component_type,
                    ) || crate::query_boundaries::common::contains_type_parameters(
                        self.ctx.types,
                        resolved_component_type,
                    ) || crate::query_boundaries::common::is_keyof_type(
                        self.ctx.types,
                        component_type,
                    ) || crate::query_boundaries::common::is_keyof_type(
                        self.ctx.types,
                        resolved_component_type,
                    );
                if needs_dynamic_intrinsic_props_check
                    && let Some((props_type, raw_has_type_params, display_target)) = self
                        .get_jsx_dynamic_intrinsic_props_for_component_type(
                            tag_name_idx,
                            component_type,
                        )
                {
                    self.check_jsx_attributes_against_props(
                        jsx_opening.attributes,
                        props_type,
                        jsx_opening.tag_name,
                        None,
                        None,
                        raw_has_type_params,
                        display_target,
                        None,
                        request,
                        children_ctx,
                    );
                } else {
                    self.check_grammar_jsx_element(jsx_opening.attributes);
                }
                if let Some(jsx_sym_id) = self.get_jsx_namespace_type() {
                    let lib_binders = self.get_lib_binders();
                    if let Some(symbol) = self
                        .ctx
                        .binder
                        .get_symbol_with_libs(jsx_sym_id, &lib_binders)
                        && let Some(exports) = symbol.exports.as_ref()
                        && let Some(element_sym_id) = exports.get("Element")
                    {
                        return self.type_reference_symbol_type(element_sym_id);
                    }
                }
                return TypeId::ANY;
            }

            let jsx_element_expr_type = self.get_jsx_element_type_for_check();
            let reported_factory_arity =
                self.check_jsx_sfc_factory_arity(resolved_component_type, tag_name_idx);
            let recovered_props = if reported_factory_arity {
                None
            } else {
                self.recover_jsx_component_props_type(
                    jsx_opening.attributes,
                    component_metadata_type,
                    Some(idx),
                    request,
                )
            };
            // Class components with multiple construct signatures (e.g. React.Component
            // in react16.d.ts has 2 constructors) must go through overload resolution
            // even when props extraction succeeds. tsc treats JSX elements as calls to
            // the constructor overloads, emitting TS2769 when all overloads fail rather
            // than TS2322 on individual attributes.
            let has_multi_construct = self.has_multi_construct_overloads(resolved_component_type)
                || self.has_multi_construct_overloads(component_type)
                || self.has_multi_construct_overloads(component_metadata_type);
            let uses_jsx_overload_resolution = has_multi_construct
                || (recovered_props.is_none()
                    && (self.is_overloaded_sfc(resolved_component_type)
                        || self.has_multi_signature_overloads(resolved_component_type)));

            if let Some((props_type, raw_has_type_params)) = recovered_props {
                if has_multi_construct {
                    // Class component with overloaded constructors: use overload
                    // resolution to match tsc's TS2769 behavior.
                    self.check_jsx_overloaded_sfc(
                        resolved_component_type,
                        jsx_opening.attributes,
                        jsx_opening.tag_name,
                        children_ctx,
                    );
                } else {
                    // TS2786: component return type must be valid JSX element
                    self.check_jsx_component_return_type(resolved_component_type, tag_name_idx);
                    let props_type = self
                        .narrow_jsx_props_union_from_attributes(jsx_opening.attributes, props_type);
                    let preferred_props_display =
                        self.get_jsx_component_props_display_text(tag_name_idx);
                    let display_target = self.build_jsx_display_target_with_preferred_props(
                        props_type,
                        Some(resolved_component_type),
                        preferred_props_display.as_deref(),
                    );
                    self.check_jsx_attributes_against_props(
                        jsx_opening.attributes,
                        props_type,
                        jsx_opening.tag_name,
                        Some(component_metadata_type),
                        Some(component_type),
                        raw_has_type_params,
                        display_target,
                        preferred_props_display.as_deref(),
                        request,
                        children_ctx,
                    );
                }
            } else if uses_jsx_overload_resolution {
                // JSX overload resolution: try each call signature (including generic
                // ones) against the provided attributes. If no overload matches, emit
                // TS2769. The `has_multi_signature_overloads` fallback covers cases
                // where all overloads are generic and props extraction returned None.
                self.check_jsx_overloaded_sfc(
                    resolved_component_type,
                    jsx_opening.attributes,
                    jsx_opening.tag_name,
                    children_ctx,
                );
            } else {
                // TS2786: component return type must be valid JSX element
                self.check_jsx_component_return_type(resolved_component_type, tag_name_idx);

                // Grammar check: TS17000 for empty expressions in JSX attributes.
                self.check_grammar_jsx_element(jsx_opening.attributes);

                // TS2604: JSX element type does not have any construct or call signatures.
                // Emit when the component type is concrete but lacks call/construct signatures.
                self.check_jsx_element_has_signatures(resolved_component_type, tag_name_idx);

                // Even when we can't extract component props (e.g., no ElementAttributesProperty),
                // check IntrinsicAttributes / IntrinsicClassAttributes<T> for required
                // properties (e.g., required `key`/`ref`). tsc checks these independently
                // of component props extraction.
                self.check_jsx_intrinsic_attributes_only(
                    resolved_component_type,
                    jsx_opening.attributes,
                    jsx_opening.tag_name,
                );

                // For generic SFCs (e.g., `Component<T>(props: T)`), we can't infer
                // type arguments, but we CAN check that spread attributes satisfy
                // IntrinsicAttributes. tsc checks spreads against
                // `IntrinsicAttributes & inferred_props` and emits TS2322 when an
                // unconstrained type parameter doesn't satisfy IntrinsicAttributes.
                self.check_generic_sfc_spread_intrinsic_attrs(
                    resolved_component_type,
                    jsx_opening.attributes,
                    jsx_opening.tag_name,
                );

                // Evaluate attribute values to trigger nested JSX processing and
                // definite-assignment checks, even when props type is unknown.
                // For generic components, set ANY contextual type to prevent
                // false TS7006 on callback parameters in JSX attributes when we
                // cannot recover a concrete props shape.
                let gen_ctx = self.is_generic_jsx_component(resolved_component_type);
                let inferred_generic_props = if gen_ctx {
                    self.infer_jsx_generic_component_props_type(
                        jsx_opening.attributes,
                        resolved_component_type,
                        request,
                    )
                    .or_else(|| {
                        self.get_default_instantiated_generic_sfc_props_type(
                            resolved_component_type,
                        )
                    })
                } else {
                    None
                };
                let generic_attr_fallback = if gen_ctx {
                    request.read().normal_origin().contextual(TypeId::ANY)
                } else {
                    request.read().normal_origin().contextual_opt(None)
                };
                if let Some(attrs_node) = self.ctx.arena.get(jsx_opening.attributes)
                    && let Some(attrs) = self.ctx.arena.get_jsx_attributes(attrs_node)
                {
                    for &attr_idx in &attrs.properties.nodes {
                        if let Some(attr_node) = self.ctx.arena.get(attr_idx) {
                            if attr_node.kind == syntax_kind_ext::JSX_SPREAD_ATTRIBUTE {
                                if let Some(spread_data) =
                                    self.ctx.arena.get_jsx_spread_attribute(attr_node)
                                {
                                    let spread_request = inferred_generic_props
                                        .map(|props| {
                                            request.read().normal_origin().contextual(props)
                                        })
                                        .unwrap_or(generic_attr_fallback);
                                    self.compute_type_of_node_with_request(
                                        spread_data.expression,
                                        &spread_request,
                                    );
                                }
                            } else if attr_node.kind == syntax_kind_ext::JSX_ATTRIBUTE
                                && let Some(attr_data) = self.ctx.arena.get_jsx_attribute(attr_node)
                                && attr_data.initializer.is_some()
                            {
                                let attr_value_idx = if let Some(init_node) =
                                    self.ctx.arena.get(attr_data.initializer)
                                {
                                    if init_node.kind == syntax_kind_ext::JSX_EXPRESSION {
                                        self.ctx
                                            .arena
                                            .get_jsx_expression(init_node)
                                            .map(|expr| expr.expression)
                                            .unwrap_or(attr_data.initializer)
                                    } else {
                                        attr_data.initializer
                                    }
                                } else {
                                    continue;
                                };
                                if gen_ctx
                                    && inferred_generic_props.is_none()
                                    && let Some(value_node) = self.ctx.arena.get(attr_value_idx)
                                    && matches!(
                                        value_node.kind,
                                        syntax_kind_ext::ARROW_FUNCTION
                                            | syntax_kind_ext::FUNCTION_EXPRESSION
                                    )
                                {
                                    continue;
                                }
                                let attr_request = if let Some(props_type) = inferred_generic_props
                                {
                                    let Some(name_node) = self.ctx.arena.get(attr_data.name) else {
                                        continue;
                                    };
                                    let Some(attr_name) = self.get_jsx_attribute_name(name_node)
                                    else {
                                        continue;
                                    };
                                    let props_for_access =
                                        self.normalize_jsx_required_props_target(props_type);
                                    match self.resolve_property_access_with_env(
                                        props_for_access,
                                        &attr_name,
                                    ) {
                                        crate::query_boundaries::common::PropertyAccessResult::Success { type_id, .. } => {
                                            request
                                                .read()
                                                .normal_origin()
                                                .contextual(crate::query_boundaries::common::remove_undefined(self.ctx.types, type_id))
                                        }
                                        _ => {
                                            if attr_name != "as"
                                                && let Some(as_tag) = self
                                                    .collect_jsx_union_resolution_attrs(
                                                        jsx_opening.attributes,
                                                    )
                                                    .and_then(|attrs| {
                                                        attrs.into_iter().find_map(|(name, ty)| {
                                                            if name == "as" {
                                                                ty.and_then(|ty| {
                                                                    self.get_jsx_single_string_literal_tag_name(ty)
                                                                })
                                                            } else {
                                                                None
                                                            }
                                                        })
                                                    })
                                                && let Some(intrinsic_props) = self
                                                    .get_jsx_intrinsic_props_for_tag(idx, &as_tag, false)
                                            {
                                                let intrinsic_props =
                                                    self.normalize_jsx_required_props_target(
                                                        intrinsic_props,
                                                    );
                                                if let crate::query_boundaries::common::PropertyAccessResult::Success { type_id, .. } =
                                                    self.resolve_property_access_with_env(
                                                        intrinsic_props,
                                                        &attr_name,
                                                    )
                                                {
                                                    request
                                                        .read()
                                                        .normal_origin()
                                                        .contextual(crate::query_boundaries::common::remove_undefined(
                                                            self.ctx.types,
                                                            type_id,
                                                        ))
                                                } else {
                                                    generic_attr_fallback
                                                }
                                            } else {
                                                generic_attr_fallback
                                            }
                                        }
                                    }
                                } else {
                                    generic_attr_fallback
                                };
                                if gen_ctx
                                    && let Some(value_node) = self.ctx.arena.get(attr_value_idx)
                                    && matches!(
                                        value_node.kind,
                                        syntax_kind_ext::ARROW_FUNCTION
                                            | syntax_kind_ext::FUNCTION_EXPRESSION
                                    )
                                {
                                    let has_function_context =
                                        attr_request.contextual_type.is_some_and(|ctx_type| {
                                            let ctx_type =
                                                self.resolve_type_for_property_access(ctx_type);
                                            crate::query_boundaries::common::function_shape_for_type(
                                                self.ctx.types,
                                                ctx_type,
                                            )
                                            .is_some()
                                                || crate::query_boundaries::common::call_signatures_for_type(
                                                    self.ctx.types,
                                                    ctx_type,
                                                )
                                                .is_some_and(|sigs| !sigs.is_empty())
                                        });
                                    if !has_function_context {
                                        // Preserve transport for generic callback attrs
                                        // even when we cannot recover a concrete function
                                        // context yet.
                                        let callback_request =
                                            request.read().contextual(TypeId::ANY);
                                        self.compute_type_of_node_with_request(
                                            attr_value_idx,
                                            &callback_request,
                                        );
                                        continue;
                                    }
                                }
                                self.compute_type_of_node_with_request(
                                    attr_value_idx,
                                    &attr_request,
                                );
                            }
                        }
                    }
                }
            }

            // The type of a JSX component element expression is always JSX.Element
            // (i.e. React.ReactElement<any>), not the component constructor/function
            // type. Returning the component type causes false TS2322 errors when the
            // JSX expression is used in a position that expects JSX.Element (e.g. as
            // the return value of `render(): JSX.Element`).
            // We look up JSX.Element directly here instead of calling get_jsx_element_type()
            // to avoid re-running the factory-in-scope diagnostics that were already
            // emitted at the top of get_type_of_jsx_opening_element.
            if let Some(element_type) = jsx_element_expr_type {
                return element_type;
            }
            // Fallback: return ANY when JSX.Element can't be resolved (e.g. no JSX types configured)
            TypeId::ANY
        }
    }
    /// Emit TS7026 for a JSX closing element if no `JSX.IntrinsicElements` exists.
    /// Covers the closing tag; opening tag is handled by `get_type_of_jsx_opening_element`.
    pub(crate) fn check_jsx_closing_element_for_implicit_any(&mut self, idx: NodeIndex) {
        // TS7026 is emitted unconditionally (not gated on noImplicitAny) when JSX.IntrinsicElements is absent.
        let Some(node) = self.ctx.arena.get(idx) else {
            return;
        };
        let Some(jsx_closing) = self.ctx.arena.get_jsx_closing(node) else {
            return;
        };
        let tag_name_idx = jsx_closing.tag_name;
        let Some(tag_name_node) = self.ctx.arena.get(tag_name_idx) else {
            return;
        };
        let is_intrinsic = if tag_name_node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
            self.ctx
                .arena
                .get_identifier(tag_name_node)
                .is_some_and(|id| {
                    id.escaped_text
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_ascii_lowercase())
                })
        } else if tag_name_node.kind == syntax_kind_ext::JSX_NAMESPACED_NAME {
            // Namespaced tags (e.g., `</svg:path>`) are always intrinsic
            true
        } else {
            false
        };
        // Same suppression rules as the opening-element TS7026 check.
        let suppress_for_import_source = self.should_suppress_ts7026_for_import_source();
        let file_has_any_parse_diag =
            self.ctx.has_parse_errors || !self.ctx.all_parse_error_positions.is_empty();
        let recovered_adjacent_sibling = self.file_has_same_line_adjacent_jsx_recovery_pattern();
        if is_intrinsic
            && self.get_intrinsic_elements_type().is_none()
            && !suppress_for_import_source
            && !file_has_any_parse_diag
            && !recovered_adjacent_sibling
        {
            use crate::diagnostics::diagnostic_codes;
            self.error_at_node_msg(
                idx,
                diagnostic_codes::JSX_ELEMENT_IMPLICITLY_HAS_TYPE_ANY_BECAUSE_NO_INTERFACE_JSX_EXISTS,
                &["IntrinsicElements"],
            );
            return;
        }

        if is_intrinsic
            && let Some(tag_name) = self.get_jsx_intrinsic_tag_name(tag_name_idx)
            && let Some(props) = self.get_jsx_intrinsic_props_for_tag(idx, &tag_name, true)
            && props == TypeId::ERROR
        {}
    }
    /// Get the global JSX namespace type (resolves factory-scoped then global `JSX`).
    pub(crate) fn get_jsx_namespace_type(&mut self) -> Option<SymbolId> {
        if let Some(cached) = self.ctx.jsx_namespace_symbol_cache {
            return cached;
        }

        let resolved = if let Some(jsx_sym) = self.resolve_jsx_namespace_from_factory() {
            Some(jsx_sym)
        } else if let Some(sym_id) = self.ctx.binder.file_locals.get("JSX") {
            if self.ctx.binder.global_augmentations.contains_key("JSX")
                || self.ctx.binder.lib_symbol_ids.contains(&sym_id)
            {
                Some(sym_id)
            } else if !self.ctx.binder.is_external_module() {
                // Top-level `declare namespace JSX { ... }` inside an external module is
                // module-local, not a global JSX namespace. Only script files may use a
                // plain file-local `JSX` as the global fallback.
                Some(sym_id)
            } else {
                self.get_cross_file_global_augmentation_symbol_id("JSX")
                    .or_else(|| {
                        self.get_cross_file_script_global_symbol_id("JSX")
                            .or_else(|| {
                                let lib_binders = self.get_lib_binders();
                                lib_binders
                                    .iter()
                                    .find_map(|lib_binder| lib_binder.file_locals.get("JSX"))
                            })
                    })
            }
        } else if let Some(sym_id) = self.get_cross_file_global_augmentation_symbol_id("JSX") {
            Some(sym_id)
        } else if self.ctx.binder.is_external_module() {
            self.get_cross_file_script_global_symbol_id("JSX")
                .or_else(|| {
                    let lib_binders = self.get_lib_binders();
                    lib_binders
                        .iter()
                        .find_map(|lib_binder| lib_binder.file_locals.get("JSX"))
                })
                .or_else(|| {
                    let lib_binders = self.get_lib_binders();
                    self.ctx
                        .binder
                        .get_global_type_with_libs("JSX", &lib_binders)
                })
        } else {
            let lib_binders = self.get_lib_binders();
            self.ctx
                .binder
                .get_global_type_with_libs("JSX", &lib_binders)
        };

        self.ctx.jsx_namespace_symbol_cache = Some(resolved);
        resolved
    }

    pub(in crate::checkers_domain::jsx) fn should_suppress_ts7026_for_import_source(
        &mut self,
    ) -> bool {
        use tsz_common::checker_options::JsxMode;

        let jsx_mode = self.effective_jsx_mode();
        let uses_import_source = jsx_mode == JsxMode::ReactJsx
            || jsx_mode == JsxMode::ReactJsxDev
            || !self.ctx.compiler_options.jsx_import_source.is_empty();
        if !uses_import_source {
            return false;
        }

        match self.get_jsx_namespace_type() {
            Some(jsx_sym_id) => self
                .resolve_jsx_namespace_target_symbol_id(jsx_sym_id)
                .is_some(),
            None => true,
        }
    }

    pub(in crate::checkers_domain::jsx) fn get_cross_file_script_global_symbol_id(
        &self,
        name: &str,
    ) -> Option<SymbolId> {
        let all_binders = self.ctx.all_binders.as_ref()?;

        if let Some(entries) = self
            .ctx
            .global_file_locals_index
            .as_ref()
            .and_then(|idx| idx.get(name))
        {
            for &(file_idx, sym_id) in entries {
                let Some(binder) = all_binders.get(file_idx) else {
                    continue;
                };
                if binder.is_external_module() {
                    continue;
                }
                self.ctx.register_symbol_file_target(sym_id, file_idx);
                return Some(sym_id);
            }
        }

        for (file_idx, binder) in all_binders.iter().enumerate() {
            if binder.is_external_module() {
                continue;
            }
            if let Some(sym_id) = binder.file_locals.get(name) {
                self.ctx.register_symbol_file_target(sym_id, file_idx);
                return Some(sym_id);
            }
        }

        None
    }

    pub(in crate::checkers_domain::jsx) fn get_cross_file_global_augmentation_symbol_id(
        &self,
        name: &str,
    ) -> Option<SymbolId> {
        let all_binders = self.ctx.all_binders.as_ref()?;

        if let Some(entries) = self
            .ctx
            .global_file_locals_index
            .as_ref()
            .and_then(|idx| idx.get(name))
        {
            for &(file_idx, sym_id) in entries {
                let Some(binder) = all_binders.get(file_idx) else {
                    continue;
                };
                if !binder.global_augmentations.contains_key(name) {
                    continue;
                }
                if let Some(augmentations) = binder.global_augmentations.get(name) {
                    for augmentation in augmentations {
                        if let Some(sym_id) = binder.node_symbols.get(&augmentation.node.0) {
                            self.ctx.register_symbol_file_target(*sym_id, file_idx);
                            return Some(*sym_id);
                        }
                    }
                }
                self.ctx.register_symbol_file_target(sym_id, file_idx);
                return Some(sym_id);
            }
        }

        for (file_idx, binder) in all_binders.iter().enumerate() {
            if !binder.global_augmentations.contains_key(name) {
                continue;
            }
            if let Some(augmentations) = binder.global_augmentations.get(name) {
                for augmentation in augmentations {
                    if let Some(sym_id) = binder.node_symbols.get(&augmentation.node.0) {
                        self.ctx.register_symbol_file_target(*sym_id, file_idx);
                        return Some(*sym_id);
                    }
                }
            }
            if let Some(sym_id) = binder.file_locals.get(name) {
                self.ctx.register_symbol_file_target(sym_id, file_idx);
                return Some(sym_id);
            }
        }

        None
    }

    pub(in crate::checkers_domain::jsx) fn get_jsx_namespace_export_symbol_id(
        &mut self,
        export_name: &str,
    ) -> Option<SymbolId> {
        let jsx_sym_id = self.get_jsx_namespace_type()?;
        let jsx_sym_id = self.resolve_jsx_namespace_target_symbol_id(jsx_sym_id)?;
        let file_idx = self.ctx.resolve_symbol_file_index(jsx_sym_id);
        let export_sym_id = if let Some(symbol) = self.get_cross_file_symbol(jsx_sym_id) {
            symbol.exports.as_ref()?.get(export_name)?
        } else {
            let lib_binders = self.get_lib_binders();
            let symbol = self
                .ctx
                .binder
                .get_symbol_with_libs(jsx_sym_id, &lib_binders)?;
            symbol.exports.as_ref()?.get(export_name)?
        };
        if let Some(file_idx) = file_idx {
            self.ctx
                .register_symbol_file_target(export_sym_id, file_idx);
        }
        Some(export_sym_id)
    }

    pub(in crate::checkers_domain::jsx) fn resolve_jsx_namespace_target_symbol_id(
        &mut self,
        sym_id: SymbolId,
    ) -> Option<SymbolId> {
        self.resolve_symbol_id_from_origin(sym_id, &mut Vec::new())
    }

    pub(in crate::checkers_domain::jsx) fn resolve_symbol_id_from_origin(
        &mut self,
        sym_id: SymbolId,
        visited: &mut Vec<SymbolId>,
    ) -> Option<SymbolId> {
        if visited.contains(&sym_id) {
            return None;
        }
        visited.push(sym_id);

        let source_file_idx = self
            .ctx
            .resolve_symbol_file_index(sym_id)
            .unwrap_or(self.ctx.current_file_idx);

        let (import_module, import_name, escaped_name, decl_idx) =
            if let Some(symbol) = self.get_cross_file_symbol(sym_id) {
                if !symbol.has_any_flags(symbol_flags::ALIAS) {
                    return Some(sym_id);
                }
                (
                    symbol.import_module.clone(),
                    symbol.import_name.clone(),
                    symbol.escaped_name.clone(),
                    symbol.primary_declaration()?,
                )
            } else {
                let lib_binders = self.get_lib_binders();
                let symbol = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders)?;
                if !symbol.has_any_flags(symbol_flags::ALIAS) {
                    return Some(sym_id);
                }
                (
                    symbol.import_module.clone(),
                    symbol.import_name.clone(),
                    symbol.escaped_name.clone(),
                    symbol.primary_declaration()?,
                )
            };

        if let Some(module_name) = import_module.as_deref() {
            let export_name = import_name.as_deref().unwrap_or(escaped_name.as_str());
            let target_sym_id = self.resolve_cross_file_export_from_file(
                module_name,
                export_name,
                Some(source_file_idx),
            )?;
            return self.resolve_symbol_id_from_origin(target_sym_id, visited);
        }

        let arena = self.ctx.get_arena_for_file(source_file_idx as u32);
        let decl_node = arena.get(decl_idx)?;
        if decl_node.kind != syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
            return Some(sym_id);
        }
        let import = arena.get_import_decl(decl_node)?;
        let entity_name = Self::entity_name_text_in_arena(arena, import.module_specifier)?;
        let target_sym_id =
            self.resolve_entity_name_from_file(source_file_idx, &entity_name, visited)?;
        Some(target_sym_id)
    }

    pub(in crate::checkers_domain::jsx) fn resolve_entity_name_from_file(
        &mut self,
        file_idx: usize,
        name: &str,
        visited: &mut Vec<SymbolId>,
    ) -> Option<SymbolId> {
        let binder = self.ctx.get_binder_for_file(file_idx)?;
        let mut segments = name.split('.');
        let root_name = segments.next()?;
        let mut current_sym = binder.file_locals.get(root_name)?;
        self.ctx.register_symbol_file_target(current_sym, file_idx);
        current_sym = self
            .resolve_symbol_id_from_origin(current_sym, visited)
            .unwrap_or(current_sym);

        for segment in segments {
            let current_file_idx = self
                .ctx
                .resolve_symbol_file_index(current_sym)
                .unwrap_or(file_idx);
            let member_sym_id = if let Some(symbol) = self.get_cross_file_symbol(current_sym) {
                symbol
                    .exports
                    .as_ref()
                    .and_then(|exports| exports.get(segment))
                    .or_else(|| {
                        symbol
                            .members
                            .as_ref()
                            .and_then(|members| members.get(segment))
                    })?
            } else {
                let lib_binders = self.get_lib_binders();
                let symbol = self
                    .ctx
                    .binder
                    .get_symbol_with_libs(current_sym, &lib_binders)?;
                symbol
                    .exports
                    .as_ref()
                    .and_then(|exports| exports.get(segment))
                    .or_else(|| {
                        symbol
                            .members
                            .as_ref()
                            .and_then(|members| members.get(segment))
                    })?
            };
            self.ctx
                .register_symbol_file_target(member_sym_id, current_file_idx);
            current_sym = self
                .resolve_symbol_id_from_origin(member_sym_id, visited)
                .unwrap_or(member_sym_id);
        }

        Some(current_sym)
    }

    pub(in crate::checkers_domain::jsx) fn entity_name_text_in_arena(
        arena: &NodeArena,
        idx: NodeIndex,
    ) -> Option<String> {
        entity_name_text_in_arena(arena, idx)
    }

    // JSX Intrinsic Elements Type

    pub(in crate::checkers_domain::jsx) fn get_intrinsic_elements_symbol_id(
        &mut self,
    ) -> Option<SymbolId> {
        if let Some(cached) = self.ctx.jsx_intrinsic_elements_symbol_cache {
            return cached;
        }
        let resolved = self.get_jsx_namespace_export_symbol_id("IntrinsicElements");
        self.ctx.jsx_intrinsic_elements_symbol_cache = Some(resolved);
        resolved
    }

    /// Get the JSX.IntrinsicElements interface type (maps tag names to prop types).
    pub(crate) fn get_intrinsic_elements_type(&mut self) -> Option<TypeId> {
        if let Some(cached) = self.ctx.jsx_intrinsic_elements_type_cache {
            return cached;
        }
        let resolved = self
            .get_intrinsic_elements_symbol_id()
            .map(|intrinsic_elements_sym_id| {
                self.type_reference_symbol_type(intrinsic_elements_sym_id)
            });
        self.ctx.jsx_intrinsic_elements_type_cache = Some(resolved);
        resolved
    }

    /// Get the JSX.IntrinsicAttributes type (e.g. `{ key?: string }` in React).
    pub(in crate::checkers_domain::jsx) fn get_intrinsic_attributes_type(
        &mut self,
    ) -> Option<TypeId> {
        let ia_sym_id = self.get_jsx_namespace_export_symbol_id("IntrinsicAttributes")?;
        let ty = self.type_reference_symbol_type(ia_sym_id);
        let evaluated = self.evaluate_type_with_env(ty);
        if evaluated == TypeId::ANY || evaluated == TypeId::ERROR || evaluated == TypeId::UNKNOWN {
            return None;
        }
        Some(evaluated)
    }
    // JSX Element Type

    /// Get the JSX.Element type for fragments.
    ///
    /// Rule #36: Fragments resolve to JSX.Element type.
    pub(crate) fn get_jsx_element_type(&mut self, node_idx: NodeIndex) -> TypeId {
        self.check_jsx_factory_in_scope(node_idx);
        self.check_jsx_fragment_factory(node_idx);
        self.check_jsx_import_source(node_idx);

        // Try to resolve JSX.Element from the JSX namespace
        if let Some(element_sym_id) = self.get_jsx_namespace_export_symbol_id("Element") {
            return self.type_reference_symbol_type(element_sym_id);
        }
        // Note: tsc 6.0 never emits TS7026 about "JSX.Element" (0 occurrences).
        // TS7026 is only emitted about "JSX.IntrinsicElements" for intrinsic elements.
        // For fragments, tsc emits TS17016 (missing jsxFragmentFactory) instead.
        TypeId::ANY
    }
    /// Get JSX.Element type for return type checking (no factory diagnostics).
    pub(crate) fn get_jsx_element_type_for_check(&mut self) -> Option<TypeId> {
        let element_sym_id = self.get_jsx_namespace_export_symbol_id("Element")?;
        Some(self.type_reference_symbol_type(element_sym_id))
    }

    /// Get JSX.ElementClass type for class component return type checking.
    pub(in crate::checkers_domain::jsx) fn get_jsx_element_class_type(&mut self) -> Option<TypeId> {
        let element_class_sym_id = self.get_jsx_namespace_export_symbol_id("ElementClass")?;
        Some(self.type_reference_symbol_type(element_class_sym_id))
    }
    pub(in crate::checkers_domain::jsx) fn get_jsx_children_prop_name(&mut self) -> String {
        use tsz_common::checker_options::JsxMode;

        if matches!(
            self.effective_jsx_mode(),
            JsxMode::ReactJsx | JsxMode::ReactJsxDev
        ) {
            return "children".to_string();
        }

        let Some(eca_sym_id) = self.get_jsx_namespace_export_symbol_id("ElementChildrenAttribute")
        else {
            return "children".to_string();
        };

        let eca_type = self.type_reference_symbol_type(eca_sym_id);
        let evaluated = self.evaluate_type_with_env(eca_type);
        if evaluated == TypeId::UNKNOWN || evaluated == TypeId::ERROR {
            return "children".to_string();
        }

        let Some(shape) =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, evaluated)
        else {
            return "children".to_string();
        };

        shape
            .properties
            .first()
            .map(|prop| self.ctx.types.resolve_atom(prop.name))
            .unwrap_or_else(|| "children".to_string())
    }
    pub(crate) fn get_jsx_children_contextual_type(
        &mut self,
        opening_element_idx: NodeIndex,
    ) -> Option<TypeId> {
        let node = self.ctx.arena.get(opening_element_idx)?;
        let jsx_opening = self.ctx.arena.get_jsx_opening(node)?;
        let tag_name_idx = jsx_opening.tag_name;
        let tag_name_node = self.ctx.arena.get(tag_name_idx)?;

        // Determine if intrinsic (lowercase) or component (uppercase/property access)
        let is_intrinsic = if tag_name_node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
            self.ctx
                .arena
                .get_identifier(tag_name_node)
                .map(|id| id.escaped_text.as_str())
                .is_some_and(|n| n.chars().next().is_some_and(|c| c.is_ascii_lowercase()))
        } else {
            tag_name_node.kind == syntax_kind_ext::JSX_NAMESPACED_NAME
        };

        let props_type = if is_intrinsic {
            let tag_name = if tag_name_node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
                self.ctx
                    .arena
                    .get_identifier(tag_name_node)
                    .map(|id| id.escaped_text.as_str().to_string())
            } else {
                // Namespaced tag
                self.ctx
                    .arena
                    .get_jsx_namespaced_name(tag_name_node)
                    .and_then(|ns| {
                        let ns_id = self.ctx.arena.get(ns.namespace)?;
                        let ns_text = self.ctx.arena.get_identifier(ns_id)?.escaped_text.as_str();
                        let name_id = self.ctx.arena.get(ns.name)?;
                        let name_text = self
                            .ctx
                            .arena
                            .get_identifier(name_id)?
                            .escaped_text
                            .as_str();
                        Some(format!("{ns_text}:{name_text}"))
                    })
            }?;
            let props =
                self.get_jsx_intrinsic_props_for_tag(opening_element_idx, &tag_name, false)?;
            if props == TypeId::ERROR {
                return None;
            }
            props
        } else {
            // Component: resolve tag name to get component type, extract props
            let component_type = self.compute_type_of_node(tag_name_idx);
            let resolved_component_type =
                self.normalize_jsx_component_type_for_resolution(component_type);
            if let Some(tag) = self.get_jsx_specific_string_literal_component_tag_name(
                tag_name_idx,
                resolved_component_type,
            ) && let Some(props) =
                self.get_jsx_intrinsic_props_for_tag(opening_element_idx, &tag, false)
                && props != TypeId::ERROR
            {
                props
            } else if let Some((props, _raw_has_type_params)) = self
                .recover_jsx_component_props_type(
                    jsx_opening.attributes,
                    component_type,
                    None,
                    &TypingRequest::NONE,
                )
            {
                self.narrow_jsx_props_union_from_attributes(jsx_opening.attributes, props)
            } else if self.is_generic_jsx_component(resolved_component_type) {
                // Generic component: return ANY to avoid false implicit-any
                // diagnostics for callback and destructuring children.
                return Some(TypeId::ANY);
            } else {
                return None;
            }
        };

        let child_count = self
            .get_jsx_body_child_nodes(jsx_opening.attributes)
            .map_or(0, |children| children.len());

        self.get_jsx_children_prop_type(props_type)
            .map(|children_type| {
                self.jsx_children_contextual_type_for_body_shape(children_type, child_count)
            })
    }
    // JSX Attribute Name Extraction

    /// Extract the attribute name from a JSX attribute name node.
    ///
    /// Handles both simple identifiers (`name`) and namespaced names (`ns:name`).
    /// Returns `None` if the node is neither.
    pub(crate) fn get_jsx_attribute_name(
        &self,
        name_node: &tsz_parser::parser::node::Node,
    ) -> Option<String> {
        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            Some(ident.escaped_text.as_str().to_string())
        } else if let Some(ns) = self.ctx.arena.get_jsx_namespaced_name(name_node) {
            let ns_id = self.ctx.arena.get(ns.namespace)?;
            let ns_text = self.ctx.arena.get_identifier(ns_id)?.escaped_text.as_str();
            let name_id = self.ctx.arena.get(ns.name)?;
            let name_text = self
                .ctx
                .arena
                .get_identifier(name_id)?
                .escaped_text
                .as_str();
            Some(format!("{ns_text}:{name_text}"))
        } else {
            None
        }
    }

    /// Check if a specific attribute name exists as an EXPLICIT JSX attribute
    /// (not from a spread). Used for TS2710 double-specification detection.
    pub(in crate::checkers_domain::jsx) fn has_explicit_jsx_attribute(
        &self,
        attributes_idx: NodeIndex,
        name: &str,
    ) -> bool {
        self.find_explicit_jsx_attribute(attributes_idx, name)
            .is_some()
    }

    /// Find an explicit JSX attribute by name, returning the attribute's name node index.
    pub(in crate::checkers_domain::jsx) fn find_explicit_jsx_attribute(
        &self,
        attributes_idx: NodeIndex,
        name: &str,
    ) -> Option<NodeIndex> {
        let attrs_node = self.ctx.arena.get(attributes_idx)?;
        let attrs = self.ctx.arena.get_jsx_attributes(attrs_node)?;
        for &attr_idx in &attrs.properties.nodes {
            let attr_node = self.ctx.arena.get(attr_idx)?;
            if attr_node.kind == syntax_kind_ext::JSX_ATTRIBUTE {
                let attr_data = self.ctx.arena.get_jsx_attribute(attr_node)?;
                let name_node = self.ctx.arena.get(attr_data.name)?;
                if let Some(attr_name) = self.get_jsx_attribute_name(name_node)
                    && attr_name == name
                {
                    return Some(attr_data.name);
                }
            }
        }
        None
    }

    /// Instantiate a JSX component type with explicit type arguments.
    ///
    /// For function types (SFCs like `<SFC<string>>`), directly instantiates the
    /// function's type parameters to produce a concrete non-generic function.
    /// For other types (class components, type aliases), creates an Application type
    /// for normal evaluation.
    pub(super) fn instantiate_jsx_component_with_type_args(
        &mut self,
        component_type: TypeId,
        type_args: &[TypeId],
    ) -> TypeId {
        // Try Function types (single-signature SFCs) - use solver helper
        if let Some(instantiated) =
            crate::query_boundaries::common::instantiate_function_with_type_args(
                self.ctx.types,
                component_type,
                type_args,
            )
        {
            return instantiated;
        }

        // Fallback: create Application for class components, type aliases,
        // and overloaded SFCs (Callable types)
        self.ctx
            .types
            .application(component_type, type_args.to_vec())
    }
}
