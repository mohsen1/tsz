//! JSX type checking (element types, intrinsic elements, namespace types).
//! - JSX attribute type checking (TS2322 for type mismatches)
//!
//! This implements Rule #36: JSX type checking with case-sensitive tag lookup.

use crate::state::CheckerState;
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

// =============================================================================
// JSX Type Checking
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // JSX Opening Element Type
    // =========================================================================

    /// Get the type of a JSX opening element.
    ///
    /// Rule #36 (JSX Intrinsic Lookup): This implements the case-sensitive tag lookup:
    /// - Lowercase tags (e.g., `<div>`) look up `JSX.IntrinsicElements['div']`
    /// - Uppercase tags (e.g., `<MyComponent>`) resolve as variable expressions
    pub(crate) fn get_type_of_jsx_opening_element(&mut self, idx: NodeIndex) -> TypeId {
        self.check_jsx_factory_in_scope(idx);

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ANY;
        };

        // Get JSX opening data (works for both JSX_OPENING_ELEMENT and JSX_SELF_CLOSING_ELEMENT)
        let Some(jsx_opening) = self.ctx.arena.get_jsx_opening(node) else {
            return TypeId::ANY;
        };

        // Get the tag name
        let tag_name_idx = jsx_opening.tag_name;
        let Some(tag_name_node) = self.ctx.arena.get(tag_name_idx) else {
            return TypeId::ANY;
        };

        // Get tag name text and determine if intrinsic.
        // Namespaced tags (e.g., `svg:path`) are always intrinsic — TSC looks up
        // `JSX.IntrinsicElements["svg:path"]` using the full `namespace:name` string.
        let (tag_name, namespaced_tag_owned, is_intrinsic) =
            if tag_name_node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
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
                let ns_str = self
                    .ctx
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
                    use tsz_solver::operations::property::PropertyAccessResult;
                    let result = self.resolve_property_access_with_env(evaluated_ie, tag);
                    let props = match result {
                        PropertyAccessResult::Success { type_id, .. } => type_id,
                        PropertyAccessResult::PropertyNotFound { .. } => {
                            // TS2339: Property 'span' does not exist on type
                            // 'JSX.IntrinsicElements'.
                            // Use `idx` (the JSX element node) for the span — tsc
                            // points at `<tagName .../>`, not just the identifier.
                            // Format the type as "JSX.IntrinsicElements" (qualified name).
                            if let Some(loc) = self.get_source_location(idx) {
                                use crate::diagnostics::Diagnostic;
                                use tsz_common::diagnostics::diagnostic_codes;
                                let message = format!(
                                    "Property '{tag}' does not exist on type 'JSX.IntrinsicElements'."
                                );
                                self.ctx.push_diagnostic(Diagnostic::error(
                                    &self.ctx.file_name,
                                    loc.start,
                                    loc.length(),
                                    message,
                                    diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                                ));
                            }
                            TypeId::ERROR
                        }
                        _ => TypeId::ANY,
                    };
                    self.ctx.jsx_intrinsic_props_cache.insert(cache_key, props);
                    props
                };

                // Check JSX attributes against the resolved props type
                self.check_jsx_attributes_against_props(
                    jsx_opening.attributes,
                    evaluated_props,
                    jsx_opening.tag_name,
                    false, // intrinsic elements never have raw type params
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

            // TS2786: Check that the component's return/instance type is a valid JSX element.
            // This fires even for generic components where we skip attribute checking.
            self.check_jsx_component_return_type(evaluated, tag_name_idx);

            // Extract props type from the component and check attributes
            if let Some((props_type, raw_has_type_params)) =
                self.get_jsx_props_type_for_component(evaluated)
            {
                self.check_jsx_attributes_against_props(
                    jsx_opening.attributes,
                    props_type,
                    jsx_opening.tag_name,
                    raw_has_type_params,
                );
            } else {
                // TS2604: JSX element type does not have any construct or call signatures.
                // Emit when the component type is concrete but lacks call/construct signatures.
                self.check_jsx_element_has_signatures(evaluated, tag_name_idx);
            }

            // The type of a JSX component element expression is always JSX.Element
            // (i.e. React.ReactElement<any>), not the component constructor/function
            // type. Returning the component type causes false TS2322 errors when the
            // JSX expression is used in a position that expects JSX.Element (e.g. as
            // the return value of `render(): JSX.Element`).
            // We look up JSX.Element directly here instead of calling get_jsx_element_type()
            // to avoid re-running the factory-in-scope diagnostics that were already
            // emitted at the top of get_type_of_jsx_opening_element.
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
            // Fallback: return ANY when JSX.Element can't be resolved (e.g. no JSX types configured)
            TypeId::ANY
        }
    }

    /// Emit TS7026 for a JSX closing element if it refers to an intrinsic tag
    /// and no `JSX.IntrinsicElements` interface exists.
    ///
    /// tsc emits TS7026 independently for both the opening and closing tags of
    /// a JSX element (e.g., both `<input>` and `</input>`).  The opening tag is
    /// handled by `get_type_of_jsx_opening_element`; this method covers the
    /// closing tag.
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

    // =========================================================================
    // JSX Namespace Type
    // =========================================================================

    /// Get the global JSX namespace type.
    ///
    /// Rule #36: Resolves the global `JSX` namespace which contains type definitions
    /// for intrinsic elements and the Element type.
    pub(crate) fn get_jsx_namespace_type(&mut self) -> Option<SymbolId> {
        // First try file_locals (includes user-defined globals and merged lib symbols)
        if let Some(sym_id) = self.ctx.binder.file_locals.get("JSX") {
            return Some(sym_id);
        }

        // Then try using get_global_type to check lib binders
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

    // =========================================================================
    // JSX Intrinsic Elements Type
    // =========================================================================

    /// Get the JSX.IntrinsicElements interface type.
    ///
    /// Rule #36: Resolves `JSX.IntrinsicElements` which maps tag names to their prop types.
    /// Returns None if the JSX namespace or `IntrinsicElements` interface is not available.
    pub(crate) fn get_intrinsic_elements_type(&mut self) -> Option<TypeId> {
        // Get the JSX namespace symbol
        let jsx_sym_id = self.get_jsx_namespace_type()?;

        // Get lib binders for cross-arena symbol lookup
        let lib_binders = self.get_lib_binders();

        // Get the JSX namespace symbol data
        let symbol = self
            .ctx
            .binder
            .get_symbol_with_libs(jsx_sym_id, &lib_binders)?;

        // Look up IntrinsicElements in the JSX namespace exports
        let exports = symbol.exports.as_ref()?;
        let intrinsic_elements_sym_id = exports.get("IntrinsicElements")?;

        // Return the type reference for IntrinsicElements
        Some(self.type_reference_symbol_type(intrinsic_elements_sym_id))
    }

    // =========================================================================
    // JSX Element Type
    // =========================================================================

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

    // =========================================================================
    // JSX Component Props Extraction
    // =========================================================================

    /// Extract the props type from a JSX component type.
    ///
    /// TSC extracts props differently for function vs class components:
    /// - **SFC (Stateless Function Component)**: first parameter type of call signature
    /// - **Class component**: construct signature return type → property from
    ///   `JSX.ElementAttributesProperty` (or the full instance type if empty)
    ///
    /// Returns `(props_type, raw_has_type_params)`:
    /// - `props_type`: the evaluated props type for attribute type checking
    /// - `raw_has_type_params`: whether the pre-evaluation props type contained type
    ///   parameters (used to suppress excess property checking)
    fn get_jsx_props_type_for_component(
        &mut self,
        component_type: TypeId,
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
            // Don't check if the resolved props type is a type parameter
            if tsz_solver::type_queries::is_type_parameter_like(self.ctx.types, props) {
                return None;
            }
            // G5: Skip union-typed props — we don't do contextual union narrowing
            // for discriminated unions in JSX attribute checking
            if tsz_solver::is_union_type(self.ctx.types, props) {
                return None;
            }
            return Some((props, raw_has_tp));
        }

        // Try class component: get construct signatures → instance type → props
        if let Some(props) = self.get_class_component_props_type(component_type) {
            if tsz_solver::type_queries::is_type_parameter_like(self.ctx.types, props) {
                return None;
            }
            if tsz_solver::is_union_type(self.ctx.types, props) {
                return None;
            }
            return Some((props, false));
        }

        None
    }

    /// Emit TS2604 if the component type has no call or construct signatures.
    ///
    /// TSC emits this when a JSX element references a value that is neither:
    /// - A function (SFC) with call signatures, nor
    /// - A class with construct signatures, nor
    /// - `any`/error/unknown/type parameter (which are silently allowed).
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
        // Skip string types — dynamic tag names like `<CustomTag>` where CustomTag
        // is a string value are valid JSX (treated as intrinsic element lookups)
        if self.is_assignable_to(component_type, TypeId::STRING) {
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

    /// Check that a JSX component's return/instance type is a valid JSX element (TS2786).
    ///
    /// For function components (SFC): the call signature return type must be assignable
    /// to `JSX.Element`.
    /// For class components: the construct signature return type (instance type) must be
    /// assignable to `JSX.ElementClass`.
    ///
    /// tsc fires this at the JSX usage site, not the component definition.
    fn check_jsx_component_return_type(&mut self, component_type: TypeId, tag_name_idx: NodeIndex) {
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
        // Skip if file has parse errors (avoid cascading diagnostics)
        if self.ctx.has_parse_errors {
            return;
        }

        // Get JSX.Element and JSX.ElementClass from the JSX namespace
        let jsx_element_type_raw = self.get_jsx_element_type_for_check();
        let jsx_element_class_type_raw = self.get_jsx_element_class_type();

        // If we can't resolve any JSX types, skip the check
        if jsx_element_type_raw.is_none() && jsx_element_class_type_raw.is_none() {
            return;
        }

        // Evaluate to concrete types and skip if they resolve to any/error/unknown
        // (incomplete type resolution, e.g., cross-file React types not fully resolved)
        let jsx_element_type = jsx_element_type_raw
            .map(|t| self.evaluate_type_with_env(t))
            .filter(|&t| t != TypeId::ANY && t != TypeId::ERROR && t != TypeId::UNKNOWN);
        let jsx_element_class_type = jsx_element_class_type_raw
            .map(|t| self.evaluate_type_with_env(t))
            .filter(|&t| t != TypeId::ANY && t != TypeId::ERROR && t != TypeId::UNKNOWN);

        // If both resolve to non-concrete types, skip
        if jsx_element_type.is_none() && jsx_element_class_type.is_none() {
            return;
        }

        // Expand union types to check each member
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
            // Skip any/error/unknown members in unions
            if member_type == TypeId::ANY
                || member_type == TypeId::ERROR
                || member_type == TypeId::UNKNOWN
            {
                continue;
            }

            // Helper: check if a return type is "unresolved" (shouldn't trigger TS2786).
            // Includes ANY/ERROR/UNKNOWN and also Application types with unresolved Lazy
            // bases — these come from cross-file generic types (e.g., React.ReactElement<any>)
            // that couldn't be fully evaluated during type checking.
            let is_unresolved = |t: TypeId| -> bool {
                t == TypeId::ANY
                    || t == TypeId::ERROR
                    || t == TypeId::UNKNOWN
                    || tsz_solver::type_queries::needs_evaluation_for_merge(self.ctx.types, t)
            };

            // Helper: null is a valid SFC return type in JSX.
            // tsc allows `null` as a JSX component return even under strictNullChecks.
            // However, `undefined` and `void` are NOT valid under strictNullChecks
            // (see tsxSfcReturnUndefinedStrictNullChecks).
            let is_valid_null_like_return = |t: TypeId| -> bool { t == TypeId::NULL };

            // Try as function component (SFC): check call signature return type
            let mut is_sfc = false;
            if let Some(shape) =
                tsz_solver::type_queries::get_function_shape(self.ctx.types, member_type)
                && !shape.is_constructor
            {
                is_sfc = true;
                let return_type = self.evaluate_type_with_env(shape.return_type);
                // Skip check if return type is unresolved (e.g., cross-file type)
                // or is a valid null-like JSX return (null, undefined, void)
                if !is_unresolved(return_type) && !is_valid_null_like_return(return_type) {
                    any_checked = true;
                    if let Some(element_type) = jsx_element_type
                        && !self.is_assignable_to(return_type, element_type)
                    {
                        all_valid = false;
                    }
                }
            }

            if !is_sfc
                && let Some(sigs) =
                    tsz_solver::type_queries::get_call_signatures(self.ctx.types, member_type)
                && !sigs.is_empty()
            {
                // Check if ALL call signatures have invalid return types
                // (if any signature is valid, the component is valid)
                let mut any_concrete = false;
                let any_sig_valid = sigs.iter().any(|sig| {
                    let return_type = self.evaluate_type_with_env(sig.return_type);
                    if is_unresolved(return_type) || is_valid_null_like_return(return_type) {
                        return true; // Unresolved or null-like → assume valid
                    }
                    any_concrete = true;
                    if let Some(element_type) = jsx_element_type {
                        self.is_assignable_to(return_type, element_type)
                    } else {
                        true // No JSX.Element to check against
                    }
                });
                if any_concrete {
                    any_checked = true;
                }
                if any_concrete && !any_sig_valid {
                    all_valid = false;
                }
            }

            // NOTE: Class component checking (construct signatures vs JSX.ElementClass)
            // is deliberately skipped. Cross-file React.Component class heritage
            // resolution is incomplete — construct signature return types are often
            // partially resolved, causing widespread false TS2786 for valid class
            // components. This can be re-enabled once class heritage resolution
            // is more robust (see conformance.md Session 2026-02-27b).
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
    ///
    /// Handles simple identifiers (`FunctionComponent`), property access
    /// expressions (`obj.MemberFunctionComponent`), and keywords (`this`).
    /// tsc uses the exact source text for property access paths (preserving spaces).
    fn get_jsx_tag_name_text(&self, tag_name_idx: NodeIndex) -> String {
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

    /// Get JSX.Element type for return type checking (without emitting factory diagnostics).
    ///
    /// Unlike `get_jsx_element_type()` which also checks factory-in-scope,
    /// this just resolves the type for assignability checking.
    fn get_jsx_element_type_for_check(&mut self) -> Option<TypeId> {
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

    /// Extract props type from a Stateless Function Component (SFC).
    ///
    /// SFCs are functions where the first parameter is the props type:
    /// `function Comp(props: { x: number }) { return <div/>; }`
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
            let evaluated = self.evaluate_type_with_env(props);
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

    /// Extract props type from a class component via construct signatures.
    ///
    /// For class components, TSC:
    /// 1. Gets the construct signature return type (the instance type)
    /// 2. Looks up `JSX.ElementAttributesProperty` to find which instance
    ///    property holds the props
    /// 3. If `ElementAttributesProperty` is empty, the instance type IS the props
    /// 4. If it has a member (e.g., `{ props: {} }`), accesses that property
    fn get_class_component_props_type(&mut self, component_type: TypeId) -> Option<TypeId> {
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

        // Look up ElementAttributesProperty to know which instance property is props
        let prop_name = self.get_element_attributes_property_name();

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
                use tsz_solver::operations::property::PropertyAccessResult;
                match self.resolve_property_access_with_env(evaluated_instance, name) {
                    PropertyAccessResult::Success { type_id, .. } => {
                        let evaluated = self.evaluate_type_with_env(type_id);
                        Some(evaluated)
                    }
                    // If we can't resolve the attribute property, fall back to
                    // first construct param (like SFC) rather than instance type
                    _ => {
                        let props = sig.params.first().map(|p| p.type_id)?;
                        let evaluated = self.evaluate_type_with_env(props);
                        Some(evaluated)
                    }
                }
            }
        }
    }

    /// Get the property name from `JSX.ElementAttributesProperty`.
    ///
    /// Returns:
    /// - `None` if `ElementAttributesProperty` doesn't exist
    /// - `Some("")` if it exists but has no members (empty interface)
    /// - `Some("props")` if it has a member (e.g., `{ props: {} }`)
    fn get_element_attributes_property_name(&mut self) -> Option<String> {
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
            // Return the name of the first (and typically only) property
            if let Some(first_prop) = shape.properties.first() {
                return Some(self.ctx.types.resolve_atom(first_prop.name));
            }
        }

        Some(String::new()) // Default: empty (instance type is props)
    }

    // =========================================================================
    // JSX Children Contextual Typing
    // =========================================================================

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
            use tsz_solver::operations::property::PropertyAccessResult;
            match self.resolve_property_access_with_env(evaluated_ie, &tag_name) {
                PropertyAccessResult::Success { type_id, .. } => type_id,
                _ => return None,
            }
        } else {
            // Component: resolve tag name to get component type, extract props
            let component_type = self.compute_type_of_node(tag_name_idx);
            let evaluated = self.evaluate_type_with_env(component_type);
            self.get_jsx_props_type_for_children_contextual(evaluated)?
        };

        // Get 'children' property from the resolved props type
        let evaluated_props = self.evaluate_type_with_env(props_type);
        let resolved_props = self.resolve_type_for_property_access(evaluated_props);
        use tsz_solver::operations::property::PropertyAccessResult;
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

        // Try class component
        if let Some(props) = self.get_class_component_props_type(component_type) {
            return Some(props);
        }

        None
    }

    // =========================================================================
    // JSX Attribute Type Checking
    // =========================================================================

    /// Check JSX attributes against an already-evaluated props type.
    ///
    /// For each attribute, checks that the assigned value is assignable to the
    /// expected property type from the props interface. Emits:
    /// - TS2322 for type mismatches and excess properties
    /// - TS2741 for missing required properties
    fn check_jsx_attributes_against_props(
        &mut self,
        attributes_idx: NodeIndex,
        props_type: TypeId,
        tag_name_idx: NodeIndex,
        raw_props_has_type_params: bool,
    ) {
        // TS2698: Validate spread attribute types BEFORE props type resolution.
        // This check fires regardless of the props type — it's about whether the
        // spread source is a valid object type, not about the target props.
        // Must run before the `props_type == ANY` early return below, since
        // intrinsic elements with `[key: string]: any` resolve to ANY props.
        // Resolve Lazy(DefId) types to their concrete Object forms.
        // Normal property access calls resolve_type_for_property_access() before checking,
        // but JSX attribute checking skipped this step — interface-referenced props arrived
        // as Lazy(DefId) and the solver's PropertyAccessEvaluator couldn't resolve them
        // (QueryCache's resolve_lazy returns None), causing silent TypeId::ANY fallback.
        let props_type = self.resolve_type_for_property_access(props_type);

        // When props_type is any/error or contains error types, skip attribute-vs-props
        // checking but still validate spread types (TS2698) which is independent of props.
        let skip_prop_checks = props_type == TypeId::ANY
            || props_type == TypeId::ERROR
            || tsz_solver::contains_error_type(self.ctx.types, props_type);

        let Some(attrs_node) = self.ctx.arena.get(attributes_idx) else {
            return;
        };
        let Some(attrs) = self.ctx.arena.get_jsx_attributes(attrs_node) else {
            return;
        };

        // Check if the props type has a string index signature (e.g., [s: string]: any).
        // When it does, any attribute name is valid, so skip excess property checking.
        let has_string_index =
            tsz_solver::type_queries::get_object_shape(self.ctx.types, props_type)
                .is_some_and(|shape| shape.string_index.is_some());

        // When the props type contains unresolved type parameters (e.g. a generic component
        // `StatelessComponent<T>`), TypeScript suppresses *excess-property* errors because
        // a spread may satisfy the type parameter. We still check *type-mismatch* errors
        // for concrete properties that are found in the intersection. This flag is used
        // in the PropertyNotFound branch below.
        //
        // We check BOTH the evaluated props type AND the raw (pre-evaluation) type.
        // The raw check is important because evaluation may collapse type parameters:
        // e.g., `T & { children?: ReactNode }` evaluates to `{ children?: ReactNode; x: number }`
        // when T has constraint `{ x: number }`, losing the type parameter information.
        let props_has_type_params = raw_props_has_type_params
            || tsz_solver::contains_type_parameters(self.ctx.types, props_type);

        // Track provided attribute names for missing-required-property check
        let mut provided_attrs: Vec<String> = Vec::new();
        let mut spread_covers_all = false;
        let mut has_excess_property_error = false;

        // Track explicit attribute names → their name node indices for TS2783
        // (spread overwrite detection). When a spread contains a required property
        // that was already specified as an explicit attribute, we emit TS2783.
        let mut named_attr_nodes: rustc_hash::FxHashMap<String, NodeIndex> =
            rustc_hash::FxHashMap::default();

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

                // Get attribute name
                let Some(name_node) = self.ctx.arena.get(attr_data.name) else {
                    continue;
                };
                let attr_name = if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                    ident.escaped_text.as_str().to_string()
                } else {
                    continue;
                };

                // Skip 'key' and 'ref' — these are special JSX attributes that TypeScript
                // does not type-check against component props. 'key' is extracted by the
                // compiler (especially in react-jsx mode) and 'ref' is managed by
                // IntrinsicClassAttributes / React.RefObject, not by component props.
                // Checking them against the props type produces false positives when the
                // props type is an unevaluated application (e.g. DetailedHTMLProps<...>)
                // that would expose them through ClassAttributes/IntrinsicAttributes.
                // Note: the missing-required-props check already skips 'key' and 'ref'
                // from the required-property side, so not tracking them here is consistent.
                if attr_name == "key" || attr_name == "ref" {
                    continue;
                }

                // Only track valid identifiers for missing-prop checking
                provided_attrs.push(attr_name.clone());

                // Track for TS2783 spread-overwrite detection
                named_attr_nodes.insert(attr_name.clone(), attr_data.name);

                // Skip prop-type checking when props type is any/error/contains-error
                if skip_prop_checks {
                    continue;
                }

                // Get expected type from props
                use tsz_solver::operations::property::PropertyAccessResult;
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
                        // data-* and aria-* attributes are only type-checked when they're
                        // explicitly declared as named properties. When resolved via a string
                        // index signature, tsc skips them (HTML convention).
                        if is_data_or_aria && from_index_signature {
                            continue;
                        }
                        // Strip `undefined` from optional property types for write-position
                        // checking. When a prop is declared as `text?: string`, the solver's
                        // `optional_property_type` returns `string | undefined` (the read type).
                        // But providing a JSX attribute means the property IS present, so the
                        // target type for assignability should be `string` (the write type).
                        // This matches TSC's `removeMissingType` behavior for JSX attributes.
                        tsz_solver::remove_undefined(self.ctx.types, type_id)
                    }
                    PropertyAccessResult::PropertyNotFound { .. } => {
                        // Excess property: attribute doesn't exist in props type.
                        // Skip if:
                        //  - props has a string index signature (any attr is valid), or
                        //  - attr starts with "data-" or "aria-" (HTML convention), or
                        //  - props type contains unresolved type parameters (tsc suppresses
                        //    excess-property errors for generic components because a spread
                        //    may satisfy the type parameter).
                        if !has_string_index
                            && !props_has_type_params
                            && !attr_name.starts_with("data-")
                            && !attr_name.starts_with("aria-")
                        {
                            // Compute the attribute value type for the error message
                            let attr_type_name = if attr_data.initializer.is_none() {
                                "boolean".to_string()
                            } else if let Some(init_node) =
                                self.ctx.arena.get(attr_data.initializer)
                            {
                                let value_idx = if init_node.kind == syntax_kind_ext::JSX_EXPRESSION
                                {
                                    self.ctx
                                        .arena
                                        .get_jsx_expression(init_node)
                                        .map(|e| e.expression)
                                        .unwrap_or(attr_data.initializer)
                                } else {
                                    attr_data.initializer
                                };
                                let value_type = self.compute_type_of_node(value_idx);
                                self.format_type(value_type)
                            } else {
                                "any".to_string()
                            };
                            let props_name = self.format_type(props_type);
                            let message = format!(
                                "Type '{{ {attr_name}: {attr_type_name}; }}' is not assignable to type '{props_name}'.\n  \
                                     Object literal may only specify known properties, \
                                     and '{attr_name}' does not exist in type '{props_name}'."
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

                // Get actual type of the attribute value
                if attr_data.initializer.is_none() {
                    // Boolean attribute without value (e.g., <input disabled />)
                    // tsc treats shorthand JSX attributes as type 'true' for assignability
                    // since `<Foo x/>` is equivalent to `<Foo x={true}/>`.
                    // But tsc displays 'boolean' (not 'true') in error messages when
                    // comparing against non-boolean types (e.g., number).
                    // So we check assignability with BOOLEAN_TRUE (correct: `true` IS
                    // assignable to `true`) but report errors with BOOLEAN to match
                    // tsc's error message format for the common case.
                    if !self.is_assignable_to(TypeId::BOOLEAN_TRUE, expected_type) {
                        self.check_assignable_or_report_at(
                            TypeId::BOOLEAN,
                            expected_type,
                            attr_data.name,
                            attr_data.name,
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

                // TS2783: Check if a later spread attribute will overwrite this attribute.
                // e.g., `<Foo a={1} {...props}>` where `props` contains `a`.
                // IMPORTANT: Must check overwrite BEFORE assignability. If the attribute
                // will be overwritten by a later spread, tsc skips the type check
                // (emitting only TS2783, not TS2322).
                let overwritten = self.check_jsx_attr_overwritten_by_spread(
                    &attr_name,
                    attr_data.name,
                    attr_nodes,
                    attr_i,
                );

                if !overwritten {
                    // Set contextual type so attribute values preserve narrow literal
                    // types instead of widening. e.g., <Foo bar="A" /> where
                    // bar: "A" | "B" keeps "A" as literal, not widened to string.
                    let prev_contextual_type = self.ctx.contextual_type;
                    self.ctx.contextual_type = Some(expected_type);
                    let actual_type = self.compute_type_of_node(value_node_idx);
                    self.ctx.contextual_type = prev_contextual_type;

                    // Check assignability — tsc anchors JSX attribute errors at the
                    // attribute NAME (not the value expression)
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
                // Extract the spread type to track which properties it provides.
                // This is used for the TS2741 (missing required property) check below.
                let Some(spread_data) = self.ctx.arena.get_jsx_spread_attribute(attr_node) else {
                    continue;
                };
                let spread_expr_idx = spread_data.expression;
                // Set contextual type from props so inline object literals in spreads
                // preserve literal types (e.g., `{...{y: true}}` keeps `y: true`
                // instead of widening to `y: boolean`). tsc contextually types
                // spread expressions against the element's props type.
                let prev_contextual_type = self.ctx.contextual_type;
                if !skip_prop_checks {
                    self.ctx.contextual_type = Some(props_type);
                }
                let spread_type = self.compute_type_of_node(spread_expr_idx);
                self.ctx.contextual_type = prev_contextual_type;
                let spread_type = self.resolve_type_for_property_access(spread_type);

                // When spread type is any/error/unknown, it potentially provides all
                // properties, so we skip further checking.
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
                    // After TS2783 check, clear tracking only for required properties
                    // that the spread provides. Optional properties don't definitely
                    // overwrite, so the explicit attribute may still be overwritten by
                    // a later spread with the same property as required.
                    for sp in &spread_props {
                        if !sp.optional {
                            let sp_name = self.ctx.types.resolve_atom(sp.name).to_string();
                            named_attr_nodes.remove(&sp_name);
                        }
                    }
                }

                // Extract property names from the spread type for TS2741 tracking.
                // This allows the missing-required-property check to account for
                // properties provided via spread.
                if let Some(spread_shape) =
                    tsz_solver::type_queries::get_object_shape(self.ctx.types, spread_type)
                {
                    for prop in &spread_shape.properties {
                        let prop_name = self.ctx.types.resolve_atom(prop.name);
                        provided_attrs.push(prop_name.to_string());
                    }
                }

                // NOTE: Per-property spread type checking was removed. tsc checks
                // spreads as a whole type against the props type (via assignability),
                // not individual property-by-property. The TS2741 (missing required
                // property) check below handles the main spread validation case.
                // Full whole-spread assignability checking (TS2322 for mismatched
                // spread properties) is deferred until the assignability gate can
                // produce tsc-matching error messages for spread types.
            }
        }

        // Check for missing required properties (TS2741)
        // Skip if:
        // - we already reported excess property errors (tsc doesn't pile on with TS2741)
        // - a spread of type `any` covers all properties
        // - props type is any/error (skip_prop_checks)
        if !has_excess_property_error && !spread_covers_all && !skip_prop_checks {
            // tsc anchors TS2741 (missing required property) at the tag name
            self.check_missing_required_jsx_props(props_type, &provided_attrs, tag_name_idx);
        }
    }

    /// Check that all required properties in the props type are provided as JSX attributes.
    /// Emits TS2741 for each missing required property.
    fn check_missing_required_jsx_props(
        &mut self,
        props_type: TypeId,
        provided_attrs: &[String],
        attributes_idx: NodeIndex,
    ) {
        let Some(shape) = tsz_solver::type_queries::get_object_shape(self.ctx.types, props_type)
        else {
            return;
        };

        for prop in &shape.properties {
            if prop.optional {
                continue;
            }

            let prop_name = self.ctx.types.resolve_atom(prop.name);

            // Skip 'children' — TSC synthesizes children from JSX element body,
            // which we don't implement yet. Reporting 'children' as missing would
            // produce false positives.
            if prop_name == "children" {
                continue;
            }

            // Skip 'key' and 'ref' — these come from IntrinsicAttributes/
            // IntrinsicClassAttributes, not from component props directly
            if prop_name == "key" || prop_name == "ref" {
                continue;
            }

            if provided_attrs.iter().any(|a| a == &prop_name) {
                continue;
            }

            // Build the "source type" name from provided attributes
            let source_type = if provided_attrs.is_empty() {
                "{}".to_string()
            } else {
                format!("{{ {} }}", provided_attrs.join(", "))
            };
            let target_type = self.format_type(props_type);
            let message = format!(
                "Property '{prop_name}' is missing in type '{source_type}' but required in type '{target_type}'."
            );
            use crate::diagnostics::diagnostic_codes;
            self.error_at_node(
                attributes_idx,
                &message,
                diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
            );
        }
    }

    /// TS2783: Check if a later spread attribute will overwrite the current attribute.
    ///
    /// In JSX, `<Foo a={1} {...props}>` — if `props` has a required property `a`,
    /// the spread overwrites the explicit `a={1}`. TSC warns with TS2783:
    /// "'a' is specified more than once, so this usage will be overwritten."
    ///
    /// Only emitted under `strictNullChecks` (matching tsc behavior) and only for
    /// non-optional spread properties (optional properties may not overwrite).
    /// Returns `true` if the attribute is overwritten by a later spread (and
    /// optionally emits TS2783 when `strictNullChecks` is enabled).
    fn check_jsx_attr_overwritten_by_spread(
        &mut self,
        attr_name: &str,
        attr_name_idx: NodeIndex,
        attr_nodes: &[NodeIndex],
        current_idx: usize,
    ) -> bool {
        // Look at later siblings for spreads that contain this property
        for &later_idx in &attr_nodes[current_idx + 1..] {
            let Some(later_node) = self.ctx.arena.get(later_idx) else {
                continue;
            };
            if later_node.kind == syntax_kind_ext::JSX_SPREAD_ATTRIBUTE {
                let Some(spread_data) = self.ctx.arena.get_jsx_spread_attribute(later_node) else {
                    continue;
                };
                let spread_type = self.compute_type_of_node(spread_data.expression);
                let spread_type = self.resolve_type_for_property_access(spread_type);

                // Skip any/error/unknown — they might cover everything but we
                // can't tell which specific properties they contain.
                if spread_type == TypeId::ANY
                    || spread_type == TypeId::ERROR
                    || spread_type == TypeId::UNKNOWN
                {
                    continue;
                }

                // Check if the spread type has a non-optional property with this name
                if let Some(shape) =
                    tsz_solver::type_queries::get_object_shape(self.ctx.types, spread_type)
                {
                    let attr_atom = self.ctx.types.intern_string(attr_name);
                    let has_required_prop = shape
                        .properties
                        .iter()
                        .any(|p| p.name == attr_atom && !p.optional);
                    if has_required_prop {
                        // TS2783: only emitted under strictNullChecks (matching tsc)
                        if self.ctx.strict_null_checks() {
                            use tsz_common::diagnostics::{
                                diagnostic_codes, diagnostic_messages, format_message,
                            };
                            let message = format_message(
                                diagnostic_messages::IS_SPECIFIED_MORE_THAN_ONCE_SO_THIS_USAGE_WILL_BE_OVERWRITTEN,
                                &[attr_name],
                            );
                            self.error_at_node(
                                attr_name_idx,
                                &message,
                                diagnostic_codes::IS_SPECIFIED_MORE_THAN_ONCE_SO_THIS_USAGE_WILL_BE_OVERWRITTEN,
                            );
                        }
                        // Attribute is overwritten regardless of SNC
                        return true;
                    }
                }
            }
        }
        false
    }

    // =========================================================================
    // JSX Factory Check
    // =========================================================================

    /// Check that the JSX factory is in scope.
    /// Emits TS2874 if the factory root identifier cannot be found.
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

        // tsc 6.0 skips scope checking when jsxFactory is explicitly set
        if self.ctx.compiler_options.jsx_factory_from_config {
            return;
        }

        let factory = self.ctx.compiler_options.jsx_factory.clone();
        let root_ident = factory.split('.').next().unwrap_or(&factory);

        if root_ident.is_empty() {
            return;
        }

        // Check full scope chain at the JSX element location.
        // tsc's resolveName for factory checks includes ALL symbols (class members,
        // parameters, locals, imports, globals) — so we use an accept-all filter
        // rather than the checker's default filter that excludes class members.
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

    // =========================================================================
    // JSX Fragment Factory Check
    // =========================================================================

    /// Check that JSX fragments have a valid fragment factory when jsxFactory is set.
    /// Emits TS17016 if jsxFactory is explicitly set but jsxFragmentFactory is not.
    ///
    /// tsc 6.0 emits this at each fragment opening location (`<>` or `<React.Fragment>`).
    fn check_jsx_fragment_factory(&mut self, node_idx: NodeIndex) {
        use tsz_common::checker_options::JsxMode;

        // Only relevant in classic "react" mode with explicitly set jsxFactory
        if self.ctx.compiler_options.jsx_mode != JsxMode::React {
            return;
        }

        if !self.ctx.compiler_options.jsx_factory_from_config {
            return;
        }

        // If jsxFragmentFactory was explicitly set, no error needed
        if self.ctx.compiler_options.jsx_fragment_factory_from_config {
            return;
        }

        use crate::diagnostics::diagnostic_codes;
        self.error_at_node(
            node_idx,
            "The 'jsxFragmentFactory' compiler option must be provided to use JSX fragments with the 'jsxFactory' compiler option.",
            diagnostic_codes::THE_JSXFRAGMENTFACTORY_COMPILER_OPTION_MUST_BE_PROVIDED_TO_USE_JSX_FRAGMENTS_WIT,
        );
    }
}
