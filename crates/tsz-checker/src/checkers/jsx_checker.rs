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
                );

                let factory = self.ctx.types.factory();
                let tag_literal = factory.literal_string(tag);
                return factory.index_access(intrinsic_elements_type, tag_literal);
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
            TypeId::ANY
        } else {
            // Component: resolve as variable expression
            // The tag name is a reference to a component (function or class)
            let component_type = self.compute_type_of_node(tag_name_idx);
            let evaluated = self.evaluate_type_with_env(component_type);

            // Extract props type from the component and check attributes
            if let Some(props_type) = self.get_jsx_props_type_for_component(evaluated) {
                self.check_jsx_attributes_against_props(
                    jsx_opening.attributes,
                    props_type,
                    jsx_opening.tag_name,
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
    fn get_jsx_props_type_for_component(&mut self, component_type: TypeId) -> Option<TypeId> {
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
        if let Some(props) = self.get_sfc_props_type(component_type) {
            // Don't check if the resolved props type is a type parameter
            if tsz_solver::type_queries::is_type_parameter_like(self.ctx.types, props) {
                return None;
            }
            // G5: Skip union-typed props — we don't do contextual union narrowing
            // for discriminated unions in JSX attribute checking
            if tsz_solver::is_union_type(self.ctx.types, props) {
                return None;
            }
            return Some(props);
        }

        // Try class component: get construct signatures → instance type → props
        if let Some(props) = self.get_class_component_props_type(component_type) {
            if tsz_solver::type_queries::is_type_parameter_like(self.ctx.types, props) {
                return None;
            }
            if tsz_solver::is_union_type(self.ctx.types, props) {
                return None;
            }
            return Some(props);
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

    /// Extract props type from a Stateless Function Component (SFC).
    ///
    /// SFCs are functions where the first parameter is the props type:
    /// `function Comp(props: { x: number }) { return <div/>; }`
    fn get_sfc_props_type(&mut self, component_type: TypeId) -> Option<TypeId> {
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
            let evaluated = self.evaluate_type_with_env(props);
            return Some(evaluated);
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
            let evaluated = self.evaluate_type_with_env(props);
            return Some(evaluated);
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
    ) {
        // Resolve Lazy(DefId) types to their concrete Object forms.
        // Normal property access calls resolve_type_for_property_access() before checking,
        // but JSX attribute checking skipped this step — interface-referenced props arrived
        // as Lazy(DefId) and the solver's PropertyAccessEvaluator couldn't resolve them
        // (QueryCache's resolve_lazy returns None), causing silent TypeId::ANY fallback.
        let props_type = self.resolve_type_for_property_access(props_type);

        // Skip if evaluation resulted in any or error
        if props_type == TypeId::ANY || props_type == TypeId::ERROR {
            return;
        }

        // Skip if the props type contains error types. This happens when generic type alias
        // instantiation fails (e.g. TS2589 "Type instantiation is excessively deep") and the
        // solver produces an Application node whose base is TypeData::Error. Checking attributes
        // against such a type produces false-positive TS2322 excess-property errors because
        // no properties can be found in an error type. tsc never emits TS2322 in this situation
        // because it successfully evaluates the type.
        if tsz_solver::contains_error_type(self.ctx.types, props_type) {
            return;
        }

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
        let props_has_type_params =
            tsz_solver::contains_type_parameters(self.ctx.types, props_type);

        // Track provided attribute names for missing-required-property check
        let mut provided_attrs: Vec<String> = Vec::new();
        let mut spread_covers_all = false;
        let mut has_excess_property_error = false;

        // Check each attribute
        for &attr_idx in &attrs.properties.nodes {
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
                        type_id
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
                    // TypeScript treats this as type 'boolean' and checks assignability
                    let bool_type = TypeId::BOOLEAN;
                    self.check_assignable_or_report_at(
                        bool_type,
                        expected_type,
                        attr_data.name,
                        attr_data.name,
                    );
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

                let actual_type = self.compute_type_of_node(value_node_idx);

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
            } else if attr_node.kind == syntax_kind_ext::JSX_SPREAD_ATTRIBUTE {
                // Extract the spread type to track which properties it provides.
                // This is used for the TS2741 (missing required property) check below.
                let Some(spread_data) = self.ctx.arena.get_jsx_spread_attribute(attr_node) else {
                    continue;
                };
                let spread_expr_idx = spread_data.expression;
                let spread_type = self.compute_type_of_node(spread_expr_idx);
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

                // Check assignability of each spread property against the corresponding
                // props property. Only check properties that exist in the target props
                // AND are NOT overridden by explicit attributes after the spread.
                // Note: We don't check the spread as a whole against props because
                // explicit attributes can supplement/override spread properties.
                if let Some(spread_shape) =
                    tsz_solver::type_queries::get_object_shape(self.ctx.types, spread_type)
                {
                    use tsz_solver::operations::property::PropertyAccessResult;
                    for prop in &spread_shape.properties {
                        let prop_name = self.ctx.types.resolve_atom(prop.name);

                        // Skip props that will be overridden by later explicit attributes.
                        // We check if an explicit attribute with the same name appears in the
                        // remaining attrs list.
                        let overridden = attrs.properties.nodes.iter().any(|&later_idx| {
                            if let Some(later_node) = self.ctx.arena.get(later_idx)
                                && later_node.kind == syntax_kind_ext::JSX_ATTRIBUTE
                                && let Some(later_attr) =
                                    self.ctx.arena.get_jsx_attribute(later_node)
                                && let Some(name_node) = self.ctx.arena.get(later_attr.name)
                                && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                            {
                                return ident.escaped_text.as_str() == prop_name;
                            }
                            false
                        });
                        if overridden {
                            continue;
                        }

                        // Look up the expected type for this property in the props type
                        let expected_type =
                            match self.resolve_property_access_with_env(props_type, &prop_name) {
                                PropertyAccessResult::Success { type_id, .. } => type_id,
                                _ => continue, // Property not in props, skip
                            };

                        // Check if the spread property's type is assignable to the expected type
                        if prop.type_id != TypeId::ANY && prop.type_id != TypeId::ERROR {
                            self.check_assignable_or_report_at(
                                prop.type_id,
                                expected_type,
                                spread_expr_idx,
                                tag_name_idx,
                            );
                        }
                    }
                }
            }
        }

        // Check for missing required properties (TS2741)
        // Skip if:
        // - we already reported excess property errors (tsc doesn't pile on with TS2741)
        // - a spread of type `any` covers all properties
        if !has_excess_property_error && !spread_covers_all {
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
