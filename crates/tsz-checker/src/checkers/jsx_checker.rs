//! JSX type checking (element types, intrinsic elements, namespace types).
//! - JSX attribute type checking (TS2322 for type mismatches)
//!
//! This implements Rule #36: JSX type checking with case-sensitive tag lookup.

use crate::context::TypingRequest;
use crate::state::CheckerState;
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Get the type of a JSX opening element (Rule #36: case-sensitive tag lookup).
    #[allow(dead_code)]
    pub(crate) fn get_type_of_jsx_opening_element(&mut self, idx: NodeIndex) -> TypeId {
        self.get_type_of_jsx_opening_element_with_request(idx, &TypingRequest::NONE)
    }

    pub(crate) fn get_type_of_jsx_opening_element_with_request(
        &mut self,
        idx: NodeIndex,
        request: &TypingRequest,
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
                            let jsx_mode = self.ctx.compiler_options.jsx_mode;
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
                && let Some(intrinsic_elements_type) = ie_type
            {
                // Evaluate IntrinsicElements from Lazy(DefId) to its concrete Object form.
                // The solver's PropertyAccessEvaluator can't resolve Lazy types without
                // the checker's type environment, so we must evaluate before property access.
                let evaluated_ie = self.evaluate_type_with_env(intrinsic_elements_type);

                let tag_atom = self.ctx.types.intern_string(tag);
                let cache_key = (intrinsic_elements_type, tag_atom);

                // Use cached result if available
                let evaluated_props = if let Some(&cached) =
                    self.ctx.jsx_intrinsic_props_cache.get(&cache_key)
                {
                    cached
                } else {
                    // Resolve the tag name as a property on the evaluated IntrinsicElements
                    use crate::query_boundaries::common::PropertyAccessResult;
                    let result = self.resolve_property_access_with_env(evaluated_ie, tag);
                    let props = match result {
                        PropertyAccessResult::Success { type_id, .. } => type_id,
                        PropertyAccessResult::PropertyNotFound { .. } => {
                            // TS2339: Property 'span' does not exist on type
                            // 'JSX.IntrinsicElements'.
                            // Use `idx` (the JSX element node) for the span — tsc
                            // points at `<tagName .../>`, not just the identifier.
                            // Format the type as "JSX.IntrinsicElements" (qualified name).
                            {
                                use tsz_common::diagnostics::diagnostic_codes;
                                let message = format!(
                                    "Property '{tag}' does not exist on type 'JSX.IntrinsicElements'."
                                );
                                self.error_at_node(
                                    idx,
                                    &message,
                                    diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                                );
                            }
                            TypeId::ERROR
                        }
                        _ => TypeId::ANY,
                    };
                    self.ctx.jsx_intrinsic_props_cache.insert(cache_key, props);
                    props
                };

                // Check JSX attributes against the resolved props type.
                // For intrinsic elements, the display target is just the props type
                // (tsc doesn't wrap intrinsic element props in IntrinsicAttributes).
                let display_target = self.build_jsx_display_target(evaluated_props, None);
                self.check_jsx_attributes_against_props(
                    jsx_opening.attributes,
                    evaluated_props,
                    jsx_opening.tag_name,
                    false, // intrinsic elements never have raw type params
                    display_target,
                    request,
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
            // 2. When the file has parser-level errors (e.g. malformed JSX attributes → TS1145),
            //    tsc suppresses TS7026 to avoid double-reporting in error-recovery situations.
            use tsz_common::checker_options::JsxMode;
            let jsx_mode = self.ctx.compiler_options.jsx_mode;
            let uses_import_source =
                jsx_mode == JsxMode::ReactJsx || jsx_mode == JsxMode::ReactJsxDev;
            if !uses_import_source && !self.ctx.has_parse_errors {
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
                            && !attr_data.initializer.is_none()
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
            let evaluated = self.evaluate_type_with_env(component_type);

            // If the resolved type is string-like or a keyof type (e.g., `keyof ReactHTML`),
            // treat it as an intrinsic element. tsc allows `<Tag>` where Tag has a string
            // type without emitting TS2604.
            if self.is_jsx_string_tag_type(evaluated)
                || tsz_solver::type_queries::is_keyof_type(self.ctx.types, evaluated)
            {
                self.check_grammar_jsx_element(jsx_opening.attributes);
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

            // TS2786: component return type must be valid JSX element
            self.check_jsx_component_return_type(evaluated, tag_name_idx);

            // Extract props type from the component and check attributes.
            // TS2607/TS2608 are emitted within props extraction when applicable.
            // Build display target with IntrinsicAttributes intersection for TS2322 messages.
            if let Some((props_type, raw_has_type_params)) =
                self.get_jsx_props_type_for_component(evaluated, Some(idx))
            {
                let display_target = self.build_jsx_display_target(props_type, Some(evaluated));
                self.check_jsx_attributes_against_props(
                    jsx_opening.attributes,
                    props_type,
                    jsx_opening.tag_name,
                    raw_has_type_params,
                    display_target,
                    request,
                );
            } else if self.is_overloaded_sfc(evaluated) {
                // JSX overload resolution: try each non-generic call signature against
                // the provided attributes. If no overload matches, emit TS2769.
                self.check_jsx_overloaded_sfc(
                    evaluated,
                    jsx_opening.attributes,
                    jsx_opening.tag_name,
                );
            } else {
                // Grammar check: TS17000 for empty expressions in JSX attributes.
                self.check_grammar_jsx_element(jsx_opening.attributes);

                // TS2604: JSX element type does not have any construct or call signatures.
                // Emit when the component type is concrete but lacks call/construct signatures.
                self.check_jsx_element_has_signatures(evaluated, tag_name_idx);

                // Even when we can't extract component props (e.g., no ElementAttributesProperty),
                // check IntrinsicAttributes for required properties (e.g., required `key`).
                // tsc checks IntrinsicAttributes independently of component props extraction.
                self.check_jsx_intrinsic_attributes_only(
                    jsx_opening.attributes,
                    jsx_opening.tag_name,
                );

                // For generic SFCs (e.g., `Component<T>(props: T)`), we can't infer
                // type arguments, but we CAN check that spread attributes satisfy
                // IntrinsicAttributes. tsc checks spreads against
                // `IntrinsicAttributes & inferred_props` and emits TS2322 when an
                // unconstrained type parameter doesn't satisfy IntrinsicAttributes.
                self.check_generic_sfc_spread_intrinsic_attrs(
                    evaluated,
                    jsx_opening.attributes,
                    jsx_opening.tag_name,
                );

                // Evaluate attribute values to trigger nested JSX processing and
                // definite-assignment checks, even when props type is unknown.
                // For generic components, set UNKNOWN contextual type to prevent
                // false TS7006 on callback parameters in JSX attributes.
                let gen_ctx = self.is_generic_jsx_component(evaluated);
                let attr_request = if gen_ctx {
                    request.read().normal_origin().contextual(TypeId::UNKNOWN)
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
                                    self.compute_type_of_node_with_request(
                                        spread_data.expression,
                                        &attr_request,
                                    );
                                }
                            } else if attr_node.kind == syntax_kind_ext::JSX_ATTRIBUTE
                                && let Some(attr_data) = self.ctx.arena.get_jsx_attribute(attr_node)
                                && !attr_data.initializer.is_none()
                            {
                                self.compute_type_of_node_with_request(
                                    attr_data.initializer,
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
        // Same suppression rules as the opening-element TS7026 check:
        // - ReactJsx/ReactJsxDev use jsxImportSource (no global IntrinsicElements needed)
        // - File has parse errors → suppress to avoid double-reporting
        use tsz_common::checker_options::JsxMode;
        let jsx_mode = self.ctx.compiler_options.jsx_mode;
        let uses_import_source = jsx_mode == JsxMode::ReactJsx || jsx_mode == JsxMode::ReactJsxDev;
        if is_intrinsic
            && self.get_intrinsic_elements_type().is_none()
            && !uses_import_source
            && !self.ctx.has_parse_errors
        {
            use crate::diagnostics::diagnostic_codes;
            self.error_at_node_msg(
                idx,
                diagnostic_codes::JSX_ELEMENT_IMPLICITLY_HAS_TYPE_ANY_BECAUSE_NO_INTERFACE_JSX_EXISTS,
                &["IntrinsicElements"],
            );
        }
    }

    /// Get the global JSX namespace type (resolves factory-scoped then global `JSX`).
    pub(crate) fn get_jsx_namespace_type(&mut self) -> Option<SymbolId> {
        if let Some(jsx_sym) = self.resolve_jsx_namespace_from_factory() {
            return Some(jsx_sym);
        }
        if let Some(sym_id) = self.ctx.binder.file_locals.get("JSX") {
            return Some(sym_id);
        }
        let lib_binders = self.get_lib_binders();
        if let Some(sym_id) = self
            .ctx
            .binder
            .get_global_type_with_libs("JSX", &lib_binders)
        {
            return Some(sym_id);
        }

        None
    }

    // JSX Intrinsic Elements Type

    /// Get the JSX.IntrinsicElements interface type (maps tag names to prop types).
    pub(crate) fn get_intrinsic_elements_type(&mut self) -> Option<TypeId> {
        let jsx_sym_id = self.get_jsx_namespace_type()?;
        let lib_binders = self.get_lib_binders();
        let symbol = self
            .ctx
            .binder
            .get_symbol_with_libs(jsx_sym_id, &lib_binders)?;
        let exports = symbol.exports.as_ref()?;
        let intrinsic_elements_sym_id = exports.get("IntrinsicElements")?;
        Some(self.type_reference_symbol_type(intrinsic_elements_sym_id))
    }

    /// Get the JSX.IntrinsicAttributes type (e.g. `{ key?: string }` in React).
    pub(super) fn get_intrinsic_attributes_type(&mut self) -> Option<TypeId> {
        let jsx_sym_id = self.get_jsx_namespace_type()?;
        let lib_binders = self.get_lib_binders();
        let symbol = self
            .ctx
            .binder
            .get_symbol_with_libs(jsx_sym_id, &lib_binders)?;
        let exports = symbol.exports.as_ref()?;
        let ia_sym_id = exports.get("IntrinsicAttributes")?;
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

        // Try to resolve JSX.Element from the JSX namespace
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
        // Note: tsc 6.0 never emits TS7026 about "JSX.Element" (0 occurrences).
        // TS7026 is only emitted about "JSX.IntrinsicElements" for intrinsic elements.
        // For fragments, tsc emits TS17016 (missing jsxFragmentFactory) instead.
        TypeId::ANY
    }

    // JSX Component Props Extraction

    /// Extract props type from a JSX component (SFC: first param of call sig;
    /// class: construct sig return → `JSX.ElementAttributesProperty` member).
    /// Returns `(props_type, raw_has_type_params)` where `raw_has_type_params`
    /// suppresses excess property checking when true.
    fn get_jsx_props_type_for_component(
        &mut self,
        component_type: TypeId,
        element_idx: Option<NodeIndex>,
    ) -> Option<(TypeId, bool)> {
        if component_type == TypeId::ANY
            || component_type == TypeId::ERROR
            || component_type == TypeId::UNKNOWN
        {
            return None;
        }

        // G1: Skip component attribute checking when the file has parse errors.
        // Parse errors can cause incorrect AST that leads to false-positive type
        // errors. TSC similarly suppresses some JSX checking in error-recovery
        // situations (e.g., tsxStatelessFunctionComponents1 with TS1005).
        if self.ctx.has_parse_errors {
            return None;
        }

        // Skip type parameters — we can't check attributes against unresolved generics
        if tsz_solver::type_queries::is_type_parameter_like(self.ctx.types, component_type) {
            return None;
        }

        // Try SFC first: get call signatures → first parameter is props type
        if let Some((props, raw_has_tp)) = self.get_sfc_props_type(component_type) {
            return Some((props, raw_has_tp));
        }

        // Try class component: get construct signatures → instance type → props
        if let Some(props) = self.get_class_component_props_type(component_type, element_idx) {
            return Some((props, false));
        }

        None
    }

    /// Emit TS2604 if the component type has no call or construct signatures.
    fn check_jsx_element_has_signatures(
        &mut self,
        component_type: TypeId,
        tag_name_idx: NodeIndex,
    ) {
        // Skip for types that are inherently allowed in JSX position
        if component_type == TypeId::ANY
            || component_type == TypeId::ERROR
            || component_type == TypeId::UNKNOWN
            || component_type == TypeId::NEVER
        {
            return;
        }
        // Skip type parameters — they may resolve to callable types
        if tsz_solver::type_queries::is_type_parameter_like(self.ctx.types, component_type) {
            return;
        }
        // Skip string-like tag values without going through full assignability.
        // Dynamic tag names like `<Tag>` where `Tag` is `string` or a union of
        // string literals are valid JSX and should be treated like intrinsic
        // element lookups. A structural relation check here is unnecessarily
        // heavy for `React.ReactType`-style unions.
        if self.is_jsx_string_tag_type(component_type) {
            return;
        }
        // Skip if file has parse errors (avoid cascading diagnostics)
        if self.ctx.has_parse_errors {
            return;
        }

        // Check if the type (or any union member) has call/construct signatures
        let types_to_check = if let Some(members) =
            tsz_solver::type_queries::get_union_members(self.ctx.types, component_type)
        {
            members
        } else {
            vec![component_type]
        };

        let has_signatures = types_to_check.iter().any(|&ty| {
            tsz_solver::type_queries::get_call_signatures(self.ctx.types, ty)
                .is_some_and(|sigs| !sigs.is_empty())
                || tsz_solver::type_queries::get_construct_signatures(self.ctx.types, ty)
                    .is_some_and(|sigs| !sigs.is_empty())
                || tsz_solver::type_queries::get_function_shape(self.ctx.types, ty).is_some()
        });

        if !has_signatures {
            // TSC uses the tag name (variable name) in the message, not the resolved type
            let tag_text = self
                .ctx
                .arena
                .get(tag_name_idx)
                .and_then(|n| self.ctx.arena.get_identifier(n))
                .map(|id| id.escaped_text.as_str().to_owned())
                .unwrap_or_else(|| self.format_type(component_type));
            use crate::diagnostics::diagnostic_codes;
            self.error_at_node_msg(
                tag_name_idx,
                diagnostic_codes::JSX_ELEMENT_TYPE_DOES_NOT_HAVE_ANY_CONSTRUCT_OR_CALL_SIGNATURES,
                &[&tag_text],
            );
        }
    }

    fn is_jsx_string_tag_type(&self, type_id: TypeId) -> bool {
        if tsz_solver::type_queries::is_string_type(self.ctx.types, type_id)
            || tsz_solver::type_queries::is_string_literal(self.ctx.types, type_id)
        {
            return true;
        }

        if let Some(members) = tsz_solver::type_queries::get_union_members(self.ctx.types, type_id)
        {
            return members
                .iter()
                .all(|&member| self.is_jsx_string_tag_type(member));
        }

        if let Some(members) =
            tsz_solver::type_queries::get_intersection_members(self.ctx.types, type_id)
        {
            return members
                .iter()
                .any(|&member| self.is_jsx_string_tag_type(member));
        }

        false
    }

    /// TS2786: Check that a JSX component's return type is assignable to
    /// `JSX.Element` (SFC) or `JSX.ElementClass` (class component).
    fn check_jsx_component_return_type(&mut self, component_type: TypeId, tag_name_idx: NodeIndex) {
        if component_type == TypeId::ANY
            || component_type == TypeId::ERROR
            || component_type == TypeId::UNKNOWN
            || component_type == TypeId::NEVER
        {
            return;
        }
        if tsz_solver::type_queries::is_type_parameter_like(self.ctx.types, component_type) {
            return;
        }
        if self.ctx.has_parse_errors {
            return;
        }

        let jsx_element_type_raw = self.get_jsx_element_type_for_check();
        let jsx_element_class_type_raw = self.get_jsx_element_class_type();
        if jsx_element_type_raw.is_none() && jsx_element_class_type_raw.is_none() {
            return;
        }

        let is_concrete =
            |t: TypeId| t != TypeId::ANY && t != TypeId::ERROR && t != TypeId::UNKNOWN;
        let jsx_element_type = jsx_element_type_raw
            .map(|t| self.evaluate_type_with_env(t))
            .filter(|&t| is_concrete(t));
        let jsx_element_class_type = jsx_element_class_type_raw
            .map(|t| self.evaluate_type_with_env(t))
            .filter(|&t| is_concrete(t));
        if jsx_element_type.is_none() && jsx_element_class_type.is_none() {
            return;
        }

        let types_to_check = if let Some(members) =
            tsz_solver::type_queries::get_union_members(self.ctx.types, component_type)
        {
            members
        } else {
            vec![component_type]
        };

        let mut any_checked = false;
        let mut all_valid = true;

        for &member_type in &types_to_check {
            if !is_concrete(member_type) {
                continue;
            }
            // Skip unresolved Application/Lazy member types (e.g. ComponentClass<any>)
            if tsz_solver::type_queries::needs_evaluation_for_merge(self.ctx.types, member_type) {
                continue;
            }

            let is_unresolved = |t: TypeId| -> bool {
                !is_concrete(t)
                    || tsz_solver::type_queries::needs_evaluation_for_merge(self.ctx.types, t)
            };
            let is_valid_null_like_return = |t: TypeId| -> bool { t == TypeId::NULL };

            let mut is_sfc = false;
            if let Some(shape) =
                tsz_solver::type_queries::get_function_shape(self.ctx.types, member_type)
                && !shape.is_constructor
            {
                is_sfc = true;
                let return_type = self.evaluate_type_with_env(shape.return_type);
                if !is_unresolved(return_type) && !is_valid_null_like_return(return_type) {
                    any_checked = true;
                    if let Some(element_type) = jsx_element_type {
                        // TSC allows null/undefined in SFC return types
                        // (e.g., `() => Element | null` is valid).
                        // Strip null/undefined before checking against JSX.Element.
                        let non_null_return =
                            tsz_solver::remove_nullish(self.ctx.types, return_type);
                        if non_null_return == TypeId::NEVER
                            || !self.is_assignable_to(non_null_return, element_type)
                        {
                            all_valid = false;
                        }
                    }
                }
            }

            // Check call/construct signatures against JSX.Element/ElementClass.
            for (get_sigs_fn, target, is_call_sig) in [
                (
                    tsz_solver::type_queries::get_call_signatures as fn(_, _) -> _,
                    jsx_element_type,
                    true,
                ),
                (
                    tsz_solver::type_queries::get_construct_signatures,
                    jsx_element_class_type,
                    false,
                ),
            ] {
                if !is_sfc
                    && let Some(sigs) = get_sigs_fn(self.ctx.types, member_type)
                    && !sigs.is_empty()
                {
                    let mut any_concrete = false;
                    let any_valid = sigs.iter().any(|sig| {
                        // Skip generic construct signatures — return type has
                        // unresolved type params that can't be checked until
                        // instantiation. Call sigs (SFCs) are still checked.
                        if !is_call_sig && !sig.type_params.is_empty() {
                            return true;
                        }
                        let ret = self.evaluate_type_with_env(sig.return_type);
                        if is_unresolved(ret) || is_valid_null_like_return(ret) {
                            return true;
                        }
                        // For construct sigs, skip if the return type still
                        // contains type parameters (from outer scopes). The
                        // instance type is incomplete until instantiation.
                        if !is_call_sig
                            && tsz_solver::type_queries::data::contains_type_parameters_db(
                                self.ctx.types,
                                ret,
                            )
                        {
                            return true;
                        }
                        any_concrete = true;
                        target.is_none_or(|t| {
                            let check_ret = if is_call_sig {
                                let stripped = tsz_solver::remove_nullish(self.ctx.types, ret);
                                if stripped == TypeId::NEVER {
                                    return true;
                                }
                                stripped
                            } else {
                                ret
                            };
                            self.is_assignable_to(check_ret, t)
                        })
                    });
                    if any_concrete {
                        any_checked = true;
                    }
                    if any_concrete && !any_valid {
                        all_valid = false;
                    }
                }
            }
        }

        if any_checked && !all_valid {
            let tag_text = self.get_jsx_tag_name_text(tag_name_idx);
            use crate::diagnostics::diagnostic_codes;
            self.error_at_node_msg(
                tag_name_idx,
                diagnostic_codes::CANNOT_BE_USED_AS_A_JSX_COMPONENT,
                &[&tag_text],
            );
        }
    }

    /// Get the text of a JSX tag name for error messages.
    pub(crate) fn get_jsx_tag_name_text(&self, tag_name_idx: NodeIndex) -> String {
        let Some(tag_name_node) = self.ctx.arena.get(tag_name_idx) else {
            return "unknown".to_string();
        };

        // Simple identifier
        if let Some(ident) = self.ctx.arena.get_identifier(tag_name_node) {
            return ident.escaped_text.as_str().to_owned();
        }

        // `this` keyword
        if tag_name_node.kind == tsz_scanner::SyntaxKind::ThisKeyword as u16 {
            return "this".to_string();
        }

        // Property access expression — reconstruct from the access expression structure
        // to preserve exact formatting (e.g., `obj. MemberClassComponent` with the space).
        // We can't use node_text() directly because the parser's PROPERTY_ACCESS_EXPRESSION
        // node span in JSX tag position may extend into trailing JSX tokens (` />`).
        if let Some(access) = self.ctx.arena.get_access_expr(tag_name_node) {
            let expr_text = self.get_jsx_tag_name_text(access.expression);
            let name_text = self
                .ctx
                .arena
                .get(access.name_or_argument)
                .and_then(|n| self.ctx.arena.get_identifier(n))
                .map(|id| id.escaped_text.as_str().to_owned())
                .unwrap_or_default();

            // Preserve whitespace between expression end and name start (includes dot + spaces)
            // get_node_span returns (start, end) — we need end of expression, start of name
            if let Some((_, expr_end)) = self.get_node_span(access.expression)
                && let Some((name_start, _)) = self.get_node_span(access.name_or_argument)
            {
                let source = self.ctx.arena.source_files.first().map(|f| f.text.as_ref());
                if let Some(src) = source {
                    let between =
                        &src[expr_end as usize..std::cmp::min(name_start as usize, src.len())];
                    return format!("{expr_text}{between}{name_text}");
                }
            }

            return format!("{expr_text}.{name_text}");
        }

        // Fallback: use raw source text, trimming trailing JSX tokens
        self.node_text(tag_name_idx)
            .map(|t| t.trim_end().to_string())
            .unwrap_or_else(|| "unknown".to_string())
    }

    /// Get JSX.Element type for return type checking (no factory diagnostics).
    pub(crate) fn get_jsx_element_type_for_check(&mut self) -> Option<TypeId> {
        let jsx_sym_id = self.get_jsx_namespace_type()?;
        let lib_binders = self.get_lib_binders();
        let symbol = self
            .ctx
            .binder
            .get_symbol_with_libs(jsx_sym_id, &lib_binders)?;
        let exports = symbol.exports.as_ref()?;
        let element_sym_id = exports.get("Element")?;
        Some(self.type_reference_symbol_type(element_sym_id))
    }

    /// Get JSX.ElementClass type for class component return type checking.
    fn get_jsx_element_class_type(&mut self) -> Option<TypeId> {
        let jsx_sym_id = self.get_jsx_namespace_type()?;
        let lib_binders = self.get_lib_binders();
        let symbol = self
            .ctx
            .binder
            .get_symbol_with_libs(jsx_sym_id, &lib_binders)?;
        let exports = symbol.exports.as_ref()?;
        let element_class_sym_id = exports.get("ElementClass")?;
        Some(self.type_reference_symbol_type(element_class_sym_id))
    }

    /// Extract props type from a Stateless Function Component (first param of call sig).
    fn get_sfc_props_type(&mut self, component_type: TypeId) -> Option<(TypeId, bool)> {
        // Check Function type (single signature)
        if let Some(shape) =
            tsz_solver::type_queries::get_function_shape(self.ctx.types, component_type)
            && !shape.is_constructor
        {
            // Skip generic SFCs — we can't infer type args without full inference
            if !shape.type_params.is_empty() {
                return None;
            }
            let props = shape
                .params
                .first()
                .map(|p| p.type_id)
                .unwrap_or_else(|| self.ctx.types.factory().object(vec![]));
            // Check for type parameters BEFORE evaluation, since evaluation may
            // collapse `T & {children?: ReactNode}` into a concrete object type
            // that loses the type parameter information.
            let raw_has_type_params = tsz_solver::contains_type_parameters(self.ctx.types, props);
            // When the raw props type is already a union (e.g., discriminated unions like
            // `{ variant: Avatar } | { variant: OneLine }`), skip full evaluation.
            // The type evaluator may incorrectly merge union members with the same
            // property names into a single object, losing the discriminated union
            // structure needed for correct assignability checking.
            let evaluated = if tsz_solver::is_union_type(self.ctx.types, props) {
                props
            } else {
                self.evaluate_type_with_env(props)
            };
            return Some((evaluated, raw_has_type_params));
        }

        // Check Callable type (overloaded signatures)
        if let Some(sigs) =
            tsz_solver::type_queries::get_call_signatures(self.ctx.types, component_type)
            && !sigs.is_empty()
        {
            // G4: Skip overloaded SFCs — we don't do JSX overload resolution.
            // Picking the first non-generic overload would produce wrong errors
            // when later overloads match the provided attributes.
            let non_generic: Vec<_> = sigs.iter().filter(|s| s.type_params.is_empty()).collect();
            if non_generic.len() != 1 {
                return None;
            }
            let sig = non_generic[0];
            let props = sig
                .params
                .first()
                .map(|p| p.type_id)
                .unwrap_or_else(|| self.ctx.types.factory().object(vec![]));
            let raw_has_type_params = tsz_solver::contains_type_parameters(self.ctx.types, props);
            let evaluated = self.evaluate_type_with_env(props);
            return Some((evaluated, raw_has_type_params));
        }

        None
    }

    /// Check if a component type is an overloaded SFC (>= 2 non-generic call signatures).
    fn is_overloaded_sfc(&self, component_type: TypeId) -> bool {
        let Some(sigs) =
            tsz_solver::type_queries::get_call_signatures(self.ctx.types, component_type)
        else {
            return false;
        };
        let non_generic_count = sigs.iter().filter(|s| s.type_params.is_empty()).count();
        non_generic_count >= 2
    }

    /// Check if a component type has generic call or construct signatures.
    fn is_generic_jsx_component(&self, component_type: TypeId) -> bool {
        if let Some(shape) =
            tsz_solver::type_queries::get_function_shape(self.ctx.types, component_type)
            && !shape.is_constructor
            && !shape.type_params.is_empty()
        {
            return true;
        }
        if let Some(sigs) =
            tsz_solver::type_queries::get_call_signatures(self.ctx.types, component_type)
            && sigs.iter().any(|s| !s.type_params.is_empty())
        {
            return true;
        }
        if let Some(sigs) =
            tsz_solver::type_queries::get_construct_signatures(self.ctx.types, component_type)
            && sigs.iter().any(|s| !s.type_params.is_empty())
        {
            return true;
        }
        false
    }

    /// Extract props type from a class component via construct signatures.
    fn get_class_component_props_type(
        &mut self,
        component_type: TypeId,
        element_idx: Option<NodeIndex>,
    ) -> Option<TypeId> {
        let sigs =
            tsz_solver::type_queries::get_construct_signatures(self.ctx.types, component_type)?;
        if sigs.is_empty() {
            return None;
        }

        // Get instance type from the first construct signature
        let sig = sigs.first()?;

        // G3: Skip generic class components — we can't infer type arguments
        // without full generic type inference for JSX elements
        if !sig.type_params.is_empty() {
            return None;
        }

        let instance_type = sig.return_type;
        if instance_type == TypeId::ANY || instance_type == TypeId::ERROR {
            return None;
        }

        // Evaluate Application/Lazy instance types to their structural form.
        // e.g. `Component<{reqd: any}, any>` is an Application that evaluates
        // to a concrete object. Only skip if evaluation still yields a type
        // with unresolved type parameters (outer generic context).
        let instance_type = if tsz_solver::type_queries::needs_evaluation_for_merge(
            self.ctx.types,
            instance_type,
        ) {
            let evaluated = self.evaluate_type_with_env(instance_type);
            // After evaluation, if the type still contains type parameters,
            // we can't resolve it further — bail out.
            if tsz_solver::contains_type_parameters(self.ctx.types, evaluated) {
                return None;
            }
            evaluated
        } else {
            instance_type
        };

        // Look up ElementAttributesProperty to know which instance property is props
        // Pass element_idx so TS2608 can be emitted if >1 property
        let prop_name = self.get_element_attributes_property_name_with_check(element_idx);

        match prop_name {
            None => {
                // G2: No ElementAttributesProperty → no JSX infrastructure.
                // TSC skips attribute checking when JSX types aren't configured.
                None
            }
            Some(ref name) if name.is_empty() => {
                // Empty ElementAttributesProperty → instance type IS the props
                let evaluated_instance = self.evaluate_type_with_env(instance_type);
                Some(evaluated_instance)
            }
            Some(ref name) => {
                // ElementAttributesProperty has a member → access that property on instance
                let evaluated_instance = self.evaluate_type_with_env(instance_type);
                use crate::query_boundaries::common::PropertyAccessResult;
                match self.resolve_property_access_with_env(evaluated_instance, name) {
                    PropertyAccessResult::Success { type_id, .. } => {
                        let evaluated = self.evaluate_type_with_env(type_id);
                        Some(evaluated)
                    }
                    // Instance type doesn't have the ElementAttributesProperty member.
                    // This can happen when class inheritance doesn't include inherited
                    // members in the construct signature return type.
                    // Fall back to the first construct parameter as props type (the
                    // common React pattern: `new(props: P)`). If no suitable fallback,
                    // emit TS2607.
                    _ => {
                        // Try first construct param as fallback (React-style: new(props: P))
                        if let Some(first_param) = sig.params.first() {
                            let param_type = self.evaluate_type_with_env(first_param.type_id);
                            if param_type != TypeId::ANY
                                && param_type != TypeId::ERROR
                                && param_type != TypeId::STRING
                                && param_type != TypeId::NUMBER
                            {
                                return Some(param_type);
                            }
                        }
                        if let Some(elem_idx) = element_idx {
                            use crate::diagnostics::diagnostic_codes;
                            self.error_at_node_msg(
                                elem_idx,
                                diagnostic_codes::JSX_ELEMENT_CLASS_DOES_NOT_SUPPORT_ATTRIBUTES_BECAUSE_IT_DOES_NOT_HAVE_A_PROPERT,
                                &[name],
                            );
                        }
                        None
                    }
                }
            }
        }
    }

    /// Get the property name from `JSX.ElementAttributesProperty`.
    /// Returns None/Some("")/Some("name"); emits TS2608 if >1 property.
    fn get_element_attributes_property_name_with_check(
        &mut self,
        element_idx: Option<NodeIndex>,
    ) -> Option<String> {
        let jsx_sym_id = self.get_jsx_namespace_type()?;
        let lib_binders = self.get_lib_binders();
        let symbol = self
            .ctx
            .binder
            .get_symbol_with_libs(jsx_sym_id, &lib_binders)?;
        let exports = symbol.exports.as_ref()?;
        let eap_sym_id = exports.get("ElementAttributesProperty")?;

        // Get the type of ElementAttributesProperty
        let eap_type = self.type_reference_symbol_type(eap_sym_id);
        let evaluated = self.evaluate_type_with_env(eap_type);

        // If the type couldn't be resolved (unknown/error), the symbol's declarations
        // are likely in a different file's arena (cross-file project mode). Fall back to
        // "props" as the standard JSX convention — all React types and most JSX configs
        // use `{ props: {} }` as the ElementAttributesProperty interface.
        if evaluated == TypeId::UNKNOWN || evaluated == TypeId::ERROR {
            return Some("props".to_string());
        }

        // Check if it has any properties
        if let Some(shape) = tsz_solver::type_queries::get_object_shape(self.ctx.types, evaluated) {
            if shape.properties.is_empty() {
                return Some(String::new()); // Empty interface
            }
            // TS2608: ElementAttributesProperty may not have more than one property
            if shape.properties.len() > 1 {
                if let Some(elem_idx) = element_idx {
                    use crate::diagnostics::diagnostic_codes;
                    self.error_at_node_msg(
                        elem_idx,
                        diagnostic_codes::THE_GLOBAL_TYPE_JSX_MAY_NOT_HAVE_MORE_THAN_ONE_PROPERTY,
                        &["ElementAttributesProperty"],
                    );
                }
                return Some(String::new());
            }
            // Return the name of the first (and typically only) property
            if let Some(first_prop) = shape.properties.first() {
                return Some(self.ctx.types.resolve_atom(first_prop.name));
            }
        }

        Some(String::new()) // Default: empty (instance type is props)
    }

    // JSX Children Contextual Typing

    fn collect_jsx_children_discriminant_attrs(
        &mut self,
        attributes_idx: NodeIndex,
    ) -> Vec<(String, TypeId)> {
        let Some(attrs_node) = self.ctx.arena.get(attributes_idx) else {
            return Vec::new();
        };
        let Some(attrs) = self.ctx.arena.get_jsx_attributes(attrs_node) else {
            return Vec::new();
        };

        let mut provided = Vec::new();
        for &attr_idx in &attrs.properties.nodes {
            let Some(attr_node) = self.ctx.arena.get(attr_idx) else {
                continue;
            };
            if attr_node.kind != syntax_kind_ext::JSX_ATTRIBUTE {
                continue;
            }
            let Some(attr_data) = self.ctx.arena.get_jsx_attribute(attr_node) else {
                continue;
            };
            let Some(name_node) = self.ctx.arena.get(attr_data.name) else {
                continue;
            };
            let Some(attr_name) = self.get_jsx_attribute_name(name_node) else {
                continue;
            };
            if matches!(attr_name.as_str(), "key" | "ref" | "children") {
                continue;
            }

            let attr_type = if attr_data.initializer.is_none() {
                TypeId::BOOLEAN_TRUE
            } else if let Some(init_node) = self.ctx.arena.get(attr_data.initializer) {
                let value_idx = if init_node.kind == syntax_kind_ext::JSX_EXPRESSION {
                    self.ctx
                        .arena
                        .get_jsx_expression(init_node)
                        .map(|expr| expr.expression)
                        .unwrap_or(attr_data.initializer)
                } else {
                    attr_data.initializer
                };
                if let Some(value_node) = self.ctx.arena.get(value_idx)
                    && matches!(
                        value_node.kind,
                        syntax_kind_ext::ARROW_FUNCTION | syntax_kind_ext::FUNCTION_EXPRESSION
                    )
                {
                    continue;
                }
                let prev = self.ctx.preserve_literal_types;
                self.ctx.preserve_literal_types = true;
                let ty = self.compute_type_of_node(value_idx);
                self.ctx.preserve_literal_types = prev;
                ty
            } else {
                TypeId::ANY
            };

            provided.push((attr_name, attr_type));
        }

        provided
    }

    fn narrow_jsx_props_union_for_children(
        &mut self,
        attributes_idx: NodeIndex,
        props_type: TypeId,
    ) -> TypeId {
        let Some(members) = tsz_solver::type_queries::get_union_members(self.ctx.types, props_type)
        else {
            return props_type;
        };

        let provided_attrs = self.collect_jsx_children_discriminant_attrs(attributes_idx);
        let provided_names: rustc_hash::FxHashSet<&str> = provided_attrs
            .iter()
            .map(|(name, _)| name.as_str())
            .collect();

        let compatible: Vec<TypeId> = members
            .into_iter()
            .filter(|&member| {
                let member = self.resolve_type_for_property_access(member);

                let attrs_match = provided_attrs.iter().all(|(name, attr_type)| {
                    use crate::query_boundaries::common::PropertyAccessResult;
                    match self.resolve_property_access_with_env(member, name) {
                        PropertyAccessResult::Success { type_id, .. } => {
                            let expected = tsz_solver::remove_undefined(self.ctx.types, type_id);
                            *attr_type == TypeId::ANY
                                || *attr_type == TypeId::ERROR
                                || self.is_assignable_to(*attr_type, expected)
                        }
                        _ => false,
                    }
                });
                if !attrs_match {
                    return false;
                }

                if let Some(shape) =
                    tsz_solver::type_queries::get_object_shape(self.ctx.types, member)
                {
                    shape.properties.iter().all(|prop| {
                        if prop.optional {
                            return true;
                        }
                        let prop_name = self.ctx.types.resolve_atom(prop.name);
                        prop_name.as_str() == "children"
                            || provided_names.contains(prop_name.as_str())
                    })
                } else {
                    true
                }
            })
            .collect();

        if compatible.len() == 1 {
            compatible[0]
        } else {
            props_type
        }
    }

    /// Extract the contextual type for JSX children from the opening element's
    /// props type. Used to provide contextual typing for children expressions
    /// like `<Comp>{(arg) => ...}</Comp>` where `arg` should get its type from
    /// the `children` prop.
    ///
    /// This must be called BEFORE children are type-checked, since
    /// `get_type_of_node` caches results and won't benefit from contextual
    /// typing if the children are already computed.
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
            // Intrinsic: look up IntrinsicElements[tag]
            let ie_type = self.get_intrinsic_elements_type()?;
            let evaluated_ie = self.evaluate_type_with_env(ie_type);
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
            use crate::query_boundaries::common::PropertyAccessResult;
            match self.resolve_property_access_with_env(evaluated_ie, &tag_name) {
                PropertyAccessResult::Success { type_id, .. } => type_id,
                _ => return None,
            }
        } else {
            // Component: resolve tag name to get component type, extract props
            let component_type = self.compute_type_of_node(tag_name_idx);
            let evaluated = self.evaluate_type_with_env(component_type);
            if let Some(props) = self.get_jsx_props_type_for_children_contextual(evaluated) {
                self.narrow_jsx_props_union_for_children(jsx_opening.attributes, props)
            } else if self.is_generic_jsx_component(evaluated) {
                // Generic component: return UNKNOWN to prevent false TS7006
                return Some(TypeId::UNKNOWN);
            } else {
                return None;
            }
        };

        // Get 'children' property from the resolved props type
        let evaluated_props = self.evaluate_type_with_env(props_type);
        let resolved_props = self.resolve_type_for_property_access(evaluated_props);
        use crate::query_boundaries::common::PropertyAccessResult;
        match self.resolve_property_access_with_env(resolved_props, "children") {
            PropertyAccessResult::Success { type_id, .. } => {
                // Don't use ANY or ERROR as contextual type — it provides no information
                if type_id == TypeId::ANY || type_id == TypeId::ERROR {
                    None
                } else {
                    Some(type_id)
                }
            }
            _ => None,
        }
    }

    /// Extract props type for contextual typing of children.
    ///
    /// Like `get_jsx_props_type_for_component` but more permissive:
    /// - Allows union props types (contextual typing works across union members)
    /// - Skips generic SFCs (can't resolve type params for contextual typing)
    fn get_jsx_props_type_for_children_contextual(
        &mut self,
        component_type: TypeId,
    ) -> Option<TypeId> {
        if component_type == TypeId::ANY
            || component_type == TypeId::ERROR
            || component_type == TypeId::UNKNOWN
        {
            return None;
        }
        if tsz_solver::type_queries::is_type_parameter_like(self.ctx.types, component_type) {
            return None;
        }

        // Try SFC: get call signatures → first parameter is props type
        if let Some(shape) =
            tsz_solver::type_queries::get_function_shape(self.ctx.types, component_type)
            && !shape.is_constructor
        {
            if !shape.type_params.is_empty() {
                return None; // Can't resolve generic type params for contextual typing
            }
            let props = shape
                .params
                .first()
                .map(|p| p.type_id)
                .unwrap_or_else(|| self.ctx.types.factory().object(vec![]));
            return Some(self.evaluate_type_with_env(props));
        }

        // Try Callable (overloaded): pick first non-generic signature
        if let Some(sigs) =
            tsz_solver::type_queries::get_call_signatures(self.ctx.types, component_type)
            && !sigs.is_empty()
        {
            let non_generic: Vec<_> = sigs.iter().filter(|s| s.type_params.is_empty()).collect();
            if non_generic.len() != 1 {
                return None;
            }
            let sig = non_generic[0];
            let props = sig
                .params
                .first()
                .map(|p| p.type_id)
                .unwrap_or_else(|| self.ctx.types.factory().object(vec![]));
            return Some(self.evaluate_type_with_env(props));
        }

        // Try class component (no element_idx — don't emit TS2607 from contextual typing)
        if let Some(props) = self.get_class_component_props_type(component_type, None) {
            return Some(props);
        }

        None
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
    fn has_explicit_jsx_attribute(&self, attributes_idx: NodeIndex, name: &str) -> bool {
        self.find_explicit_jsx_attribute(attributes_idx, name)
            .is_some()
    }

    /// Find an explicit JSX attribute by name, returning the attribute's name node index.
    fn find_explicit_jsx_attribute(
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

    /// Check JSX attributes against an already-evaluated props type.
    ///
    /// For each attribute, checks that the assigned value is assignable to the
    /// expected property type from the props interface. Emits:
    /// - TS2322 for type mismatches and excess properties
    /// - TS2741 for missing required properties
    ///
    /// `display_target` is the pre-formatted string shown in TS2322 error messages
    /// for excess properties. tsc uses `IntrinsicAttributes & PropsType` (or
    /// `IntrinsicAttributes & IntrinsicClassAttributes<T> & PropsType`) rather
    /// than just `PropsType`.
    pub(crate) fn check_jsx_attributes_against_props(
        &mut self,
        attributes_idx: NodeIndex,
        props_type: TypeId,
        tag_name_idx: NodeIndex,
        raw_props_has_type_params: bool,
        display_target: String,
        request: &TypingRequest,
    ) {
        // Grammar check: TS17000 for empty expressions in JSX attributes.
        // Matches tsc: only the first empty expression per element is reported.
        self.check_grammar_jsx_element(attributes_idx);

        // Take children_info EARLY — nested JSX in attribute values would steal it.
        let children_info = self.ctx.jsx_children_info.take();

        // Resolve Lazy(DefId) props before any checks (TS2698 needs this too).
        let props_type = self.resolve_type_for_property_access(props_type);

        // Union props: delegate to whole-object assignability checking.
        if tsz_solver::is_union_type(self.ctx.types, props_type) {
            // Restore children_info for the union path which takes it independently
            self.ctx.jsx_children_info = children_info;
            self.check_jsx_union_props(attributes_idx, props_type, tag_name_idx);
            return;
        }
        // Skip attribute-vs-props checking for any/error props.
        let skip_prop_checks = props_type == TypeId::ANY
            || props_type == TypeId::ERROR
            || tsz_solver::contains_error_type(self.ctx.types, props_type);

        let Some(attrs_node) = self.ctx.arena.get(attributes_idx) else {
            return;
        };
        let Some(attrs) = self.ctx.arena.get_jsx_attributes(attrs_node) else {
            return;
        };

        // String index signature → any attribute name is valid.
        let has_string_index =
            tsz_solver::type_queries::get_object_shape(self.ctx.types, props_type)
                .is_some_and(|shape| shape.string_index.is_some());

        // Suppress excess-property errors when props has unresolved type params.
        // Check both raw and evaluated props (evaluation may collapse type params).
        let props_has_type_params = raw_props_has_type_params
            || tsz_solver::contains_type_parameters(self.ctx.types, props_type);

        let mut provided_attrs: Vec<(String, TypeId)> = Vec::new();
        let mut spread_covers_all = false;
        let mut has_excess_property_error = false;

        // TS2783: track explicit attr names for spread overwrite detection.
        let mut named_attr_nodes: rustc_hash::FxHashMap<String, NodeIndex> =
            rustc_hash::FxHashMap::default();

        // Deferred spread entries: (spread_type, expr_idx, attr_index) for TS2322.
        let mut spread_entries: Vec<(TypeId, NodeIndex, usize)> = Vec::new();

        // Check each attribute
        let attr_nodes = &attrs.properties.nodes;
        for (attr_i, &attr_idx) in attr_nodes.iter().enumerate() {
            let Some(attr_node) = self.ctx.arena.get(attr_idx) else {
                continue;
            };

            if attr_node.kind == syntax_kind_ext::JSX_ATTRIBUTE {
                // Regular JSX attribute: name={value}
                let Some(attr_data) = self.ctx.arena.get_jsx_attribute(attr_node) else {
                    continue;
                };

                // Get attribute name (handles both simple and namespaced names like `ns:attr`)
                let Some(name_node) = self.ctx.arena.get(attr_data.name) else {
                    continue;
                };
                let Some(attr_name) = self.get_jsx_attribute_name(name_node) else {
                    continue;
                };

                // Track all attributes for missing-prop checking (including key/ref).
                // Even though key/ref are not checked against component props for TYPE
                // compatibility (they come from IntrinsicAttributes/IntrinsicClassAttributes),
                // they still need to be tracked as "provided" so the IntrinsicAttributes
                // missing-required-property check knows they were given.
                // Type will be filled in later after compute_type_of_node is called.
                provided_attrs.push((attr_name.clone(), TypeId::ANY));

                // Skip type-checking 'key' and 'ref' against component props.
                // These are special JSX attributes managed by IntrinsicAttributes /
                // IntrinsicClassAttributes, not by component props directly.
                // Checking them against the props type produces false positives when the
                // props type is an unevaluated application (e.g. DetailedHTMLProps<...>).
                if attr_name == "key" || attr_name == "ref" {
                    continue;
                }

                // Track for TS2783 spread-overwrite detection
                named_attr_nodes.insert(attr_name.clone(), attr_data.name);

                // Skip prop-type checking when props type is any/error/contains-error
                if skip_prop_checks {
                    continue;
                }

                // Get expected type from props
                use crate::query_boundaries::common::PropertyAccessResult;
                let is_data_or_aria =
                    attr_name.starts_with("data-") || attr_name.starts_with("aria-");
                let expected_type = match self
                    .resolve_property_access_with_env(props_type, &attr_name)
                {
                    PropertyAccessResult::Success {
                        type_id,
                        from_index_signature,
                        ..
                    } => {
                        // data-*/aria-* via index signature: skip (HTML convention).
                        if is_data_or_aria && from_index_signature {
                            continue;
                        }
                        // Strip undefined from optional props (write-position checking).
                        tsz_solver::remove_undefined(self.ctx.types, type_id)
                    }
                    PropertyAccessResult::PropertyNotFound { .. } => {
                        // Compute actual value type (replacing ANY placeholder) for error messages.
                        let attr_value_type = if attr_data.initializer.is_none() {
                            TypeId::BOOLEAN_TRUE // shorthand boolean literal
                        } else if let Some(init_node) = self.ctx.arena.get(attr_data.initializer) {
                            let value_idx = if init_node.kind == syntax_kind_ext::JSX_EXPRESSION {
                                self.ctx
                                    .arena
                                    .get_jsx_expression(init_node)
                                    .map(|e| e.expression)
                                    .unwrap_or(attr_data.initializer)
                            } else {
                                attr_data.initializer
                            };
                            self.compute_type_of_node(value_idx)
                        } else {
                            TypeId::ANY
                        };
                        if let Some(entry) = provided_attrs.last_mut() {
                            entry.1 = attr_value_type;
                        }

                        if !has_string_index // excess property check
                            && !props_has_type_params
                            && !attr_name.starts_with("data-")
                            && !attr_name.starts_with("aria-")
                        {
                            let attr_type_name = if attr_data.initializer.is_none() {
                                "true".to_string()
                            } else {
                                self.format_type(attr_value_type)
                            };
                            let message = format!(
                                "Type '{{ {attr_name}: {attr_type_name}; }}' is not assignable to type '{display_target}'.\n  \
                                     Object literal may only specify known properties, \
                                     and '{attr_name}' does not exist in type '{display_target}'."
                            );
                            use crate::diagnostics::diagnostic_codes;
                            self.error_at_node(
                                attr_idx,
                                &message,
                                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                            );
                            has_excess_property_error = true;
                        }
                        continue;
                    }
                    _ => continue,
                };

                // Check attribute value assignability
                if attr_data.initializer.is_none() {
                    // Shorthand boolean: tsc uses literal `true` for both assignability
                    // and error messages in the per-attribute path.
                    if let Some(entry) = provided_attrs.last_mut() {
                        entry.1 = TypeId::BOOLEAN_TRUE;
                    }
                    if !self.is_assignable_to(TypeId::BOOLEAN_TRUE, expected_type) {
                        use crate::diagnostics::{
                            diagnostic_codes, diagnostic_messages, format_message,
                        };
                        let target_str = self.format_type(expected_type);
                        let message = format_message(
                            diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                            &["true", &target_str],
                        );
                        self.error_at_node(
                            attr_data.name,
                            &message,
                            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        );
                    }
                    continue;
                }

                // The initializer might be a JSX expression wrapper or a string literal
                let value_node_idx =
                    if let Some(init_node) = self.ctx.arena.get(attr_data.initializer) {
                        if init_node.kind == syntax_kind_ext::JSX_EXPRESSION {
                            // Unwrap JSX expression to get the actual expression
                            if let Some(jsx_expr) = self.ctx.arena.get_jsx_expression(init_node) {
                                jsx_expr.expression
                            } else {
                                continue;
                            }
                        } else {
                            // String literal or other expression
                            attr_data.initializer
                        }
                    } else {
                        continue;
                    };

                // TS2783: Check if a later spread overwrites this attr (skip type check if so).
                let overwritten = self.check_jsx_attr_overwritten_by_spread(
                    &attr_name,
                    attr_data.name,
                    attr_nodes,
                    attr_i,
                );

                if !overwritten {
                    // Set contextual type to preserve narrow literal types.
                    let actual_type = self.compute_type_of_node_with_request(
                        value_node_idx,
                        &request.read().normal_origin().contextual(expected_type),
                    );

                    if let Some(entry) = provided_attrs.last_mut() {
                        entry.1 = actual_type;
                    }
                    // Assignability check — tsc anchors at the attribute NAME.
                    if actual_type != TypeId::ANY && actual_type != TypeId::ERROR {
                        self.check_assignable_or_report_at(
                            actual_type,
                            expected_type,
                            value_node_idx,
                            attr_data.name,
                        );
                    }
                }
            } else if attr_node.kind == syntax_kind_ext::JSX_SPREAD_ATTRIBUTE {
                let Some(spread_data) = self.ctx.arena.get_jsx_spread_attribute(attr_node) else {
                    continue;
                };
                let spread_expr_idx = spread_data.expression;
                // Set contextual type so spread literals preserve narrow types.
                let spread_request = if !skip_prop_checks {
                    request.read().normal_origin().contextual(props_type)
                } else {
                    request.read().normal_origin().contextual_opt(None)
                };
                let spread_type =
                    self.compute_type_of_node_with_request(spread_expr_idx, &spread_request);
                let spread_type = self.resolve_type_for_property_access(spread_type);

                // any/error/unknown spread covers all properties.
                if spread_type == TypeId::ANY
                    || spread_type == TypeId::ERROR
                    || spread_type == TypeId::UNKNOWN
                {
                    // Mark all required props as provided (any spread covers everything)
                    spread_covers_all = true;
                    continue;
                }

                // TS2698: Validate spread type is object-like.
                // tsc rejects spreading `null`, `undefined`, `never`, primitives in JSX.
                // This runs regardless of skip_prop_checks — it's independent of props type.
                let resolved = self.resolve_lazy_type(spread_type);
                if resolved == TypeId::NEVER
                    || !crate::query_boundaries::type_computation::access::is_valid_spread_type(
                        self.ctx.types,
                        resolved,
                    )
                {
                    self.report_spread_not_object_type(spread_expr_idx);
                    continue;
                }

                // TS2783: Check if any earlier explicit attributes will be
                // overwritten by required (non-optional) properties from this spread.
                // Only when strict null checks are enabled (matches tsc behavior).
                if self.ctx.strict_null_checks() && !named_attr_nodes.is_empty() {
                    let spread_props = self.collect_object_spread_properties(spread_type);
                    for sp in &spread_props {
                        if !sp.optional {
                            let sp_name = self.ctx.types.resolve_atom(sp.name).to_string();
                            if let Some(&attr_name_idx) = named_attr_nodes.get(&sp_name) {
                                use crate::diagnostics::{
                                    diagnostic_codes, diagnostic_messages, format_message,
                                };
                                let message = format_message(
                                    diagnostic_messages::IS_SPECIFIED_MORE_THAN_ONCE_SO_THIS_USAGE_WILL_BE_OVERWRITTEN,
                                    &[&sp_name],
                                );
                                self.error_at_node(
                                    attr_name_idx,
                                    &message,
                                    diagnostic_codes::IS_SPECIFIED_MORE_THAN_ONCE_SO_THIS_USAGE_WILL_BE_OVERWRITTEN,
                                );
                            }
                        }
                    }
                    // Clear required spread props from tracking.
                    for sp in &spread_props {
                        if !sp.optional {
                            let sp_name = self.ctx.types.resolve_atom(sp.name).to_string();
                            named_attr_nodes.remove(&sp_name);
                        }
                    }
                }

                // Extract spread props for TS2741 tracking.
                if let Some(spread_shape) =
                    tsz_solver::type_queries::get_object_shape(self.ctx.types, spread_type)
                {
                    for prop in &spread_shape.properties {
                        let prop_name = self.ctx.types.resolve_atom(prop.name);
                        provided_attrs.push((prop_name.to_string(), prop.type_id));
                    }
                }

                // Defer TS2322 spread checking until after attribute override tracking.
                if !skip_prop_checks {
                    spread_entries.push((spread_type, spread_expr_idx, attr_i));
                }
            }
        }

        // TS2322: Check spread props against expected types (deferred to account for overrides).
        if !spread_entries.is_empty() {
            let mut explicit_attr_names_with_pos: Vec<(usize, String)> = Vec::new();
            for (i, &node_idx) in attr_nodes.iter().enumerate() {
                let Some(node) = self.ctx.arena.get(node_idx) else {
                    continue;
                };
                if node.kind == syntax_kind_ext::JSX_ATTRIBUTE
                    && let Some(attr_data) = self.ctx.arena.get_jsx_attribute(node)
                    && let Some(name_node) = self.ctx.arena.get(attr_data.name)
                    && let Some(attr_name) = self.get_jsx_attribute_name(name_node)
                {
                    explicit_attr_names_with_pos.push((i, attr_name));
                }
            }

            for &(spread_type, _spread_expr_idx, _spread_pos) in &spread_entries {
                // Skip props that also appear as explicit attributes.
                let overridden: rustc_hash::FxHashSet<&str> = explicit_attr_names_with_pos
                    .iter()
                    .map(|(_, name)| name.as_str())
                    .collect();

                self.check_spread_property_types(
                    spread_type,
                    props_type,
                    tag_name_idx,
                    &overridden,
                    &display_target,
                );
            }
        }

        // JSX children synthesis: incorporate body children into provided props.
        if let Some((child_count, has_text_child, synthesized_type, text_child_indices)) =
            children_info
        {
            // TS2710: explicit children attr + body children = double specification.
            // Error location: the first JSX attribute (matching tsc's span).
            let has_explicit_children_attr =
                self.has_explicit_jsx_attribute(attributes_idx, "children");
            if has_explicit_children_attr && !skip_prop_checks {
                // tsc reports TS2710 on the JsxAttributes node, which spans from
                // the first attribute to the closing >. Our parser sets JsxAttributes.pos
                // to the first token after the tag name, matching tsc's behavior.
                use crate::diagnostics::diagnostic_codes;
                self.error_at_node_msg(
                    attributes_idx,
                    diagnostic_codes::ARE_SPECIFIED_TWICE_THE_ATTRIBUTE_NAMED_WILL_BE_OVERWRITTEN,
                    &["children"],
                );
            }

            provided_attrs.push(("children".to_string(), synthesized_type));
            // TS2745/TS2746: route JSX body children through one normalized
            // classifier so union/tuple shapes don't drift by component path.
            if child_count > 0 && !skip_prop_checks {
                self.check_jsx_children_shape(
                    props_type,
                    attributes_idx,
                    child_count,
                    has_text_child,
                    tag_name_idx,
                );
            }
            // TS2747: text children not accepted by component.
            if has_text_child && !skip_prop_checks {
                self.check_jsx_text_children_accepted(
                    props_type,
                    tag_name_idx,
                    &text_child_indices,
                );
            }
        }

        // TS2741: missing required properties.
        if !has_excess_property_error && !spread_covers_all && !skip_prop_checks {
            self.check_missing_required_jsx_props(props_type, &provided_attrs, tag_name_idx);
        }

        // Also check required IntrinsicAttributes.
        if !has_excess_property_error
            && !spread_covers_all
            && let Some(intrinsic_attrs_type) = self.get_intrinsic_attributes_type()
        {
            self.check_missing_required_jsx_props(
                intrinsic_attrs_type,
                &provided_attrs,
                tag_name_idx,
            );
        }
    }

    /// Mark a JSX factory or fragment factory name as referenced for
    /// unused-import checking (TS6192). The name may be dotted (e.g.,
    /// `React.createElement`); we resolve only the root identifier.
    pub(crate) fn mark_jsx_name_as_referenced(&mut self, name: &str, node_idx: NodeIndex) {
        let root_ident = name.split('.').next().unwrap_or(name);
        if root_ident.is_empty() {
            return;
        }
        let lib_binders = self.get_lib_binders();
        if let Some(sym_id) = self.ctx.binder.resolve_name_with_filter(
            root_ident,
            self.ctx.arena,
            node_idx,
            &lib_binders,
            |_| true,
        ) {
            self.ctx.referenced_symbols.borrow_mut().insert(sym_id);
        }
    }

    /// Check that the JSX factory is in scope (TS2874).
    ///
    /// tsc 6.0 behavior:
    /// - Only classic "react" mode requires the factory in scope.
    /// - When `jsxFactory` compiler option is explicitly set, tsc skips scope
    ///   checking (the option is a name hint, not a scope requirement).
    /// - When using default (`React.createElement`) or `reactNamespace`, tsc
    ///   checks the full scope chain (local, imports, namespace, global).
    pub(crate) fn check_jsx_factory_in_scope(&mut self, node_idx: NodeIndex) {
        use tsz_common::checker_options::JsxMode;

        // Only classic "react" mode requires the factory in scope
        if self.ctx.compiler_options.jsx_mode != JsxMode::React {
            return;
        }

        // When @jsxImportSource pragma is present, it overrides react mode
        // to react-jsx behavior, so the factory scope check doesn't apply.
        if self.extract_jsx_import_source_pragma().is_some() {
            return;
        }

        // tsc 6.0 skips scope checking when jsxFactory is explicitly set.
        // However, we still need to mark the factory symbol as referenced
        // so that unused-import checking (TS6192) doesn't flag it.
        if self.ctx.compiler_options.jsx_factory_from_config {
            self.mark_jsx_name_as_referenced(
                &self.ctx.compiler_options.jsx_factory.clone(),
                node_idx,
            );
            return;
        }

        // Check for per-file /** @jsx factory */ pragma
        let pragma_factory = self
            .ctx
            .arena
            .source_files
            .first()
            .and_then(|sf| super::jsx_checker_attrs::extract_jsx_pragma(&sf.text));

        let factory =
            pragma_factory.unwrap_or_else(|| self.ctx.compiler_options.jsx_factory.clone());
        let root_ident = factory.split('.').next().unwrap_or(&factory);

        if root_ident.is_empty() {
            return;
        }

        // Check full scope chain (accept-all filter to include class members)
        let lib_binders = self.get_lib_binders();
        let found = self.ctx.binder.resolve_name_with_filter(
            root_ident,
            self.ctx.arena,
            node_idx,
            &lib_binders,
            |_| true, // Accept any symbol, including class members
        );
        if found.is_some() {
            return;
        }

        // Also check global scope as fallback (for lib-loaded symbols)
        if self.resolve_global_value_symbol(root_ident).is_some() {
            return;
        }

        // If not found, emit TS2874 at the tag name (tsc points at the tag name, not `<`)
        let error_node = self
            .ctx
            .arena
            .get(node_idx)
            .and_then(|node| self.ctx.arena.get_jsx_opening(node))
            .map(|jsx| jsx.tag_name)
            .unwrap_or(node_idx);
        use crate::diagnostics::diagnostic_codes;
        self.error_at_node_msg(
            error_node,
            diagnostic_codes::THIS_JSX_TAG_REQUIRES_TO_BE_IN_SCOPE_BUT_IT_COULD_NOT_BE_FOUND,
            &[root_ident],
        );
    }
}
