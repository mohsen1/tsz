//! Type Node Checking
use super::queries::lib_resolution::keyword_syntax_to_type_id;
use super::type_node_helpers::{
    check_duplicate_parameters_in_type, check_parameter_initializers_in_type,
};
use crate::context::CheckerContext;
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::recursion::{DepthCounter, RecursionProfile};
use tsz_solver::{TypeId, Visibility};
/// Type node checker that operates on the shared context.
///
/// This is a stateless checker that borrows the context mutably.
/// All type resolution for type nodes goes through this checker.
pub struct TypeNodeChecker<'a, 'ctx> {
    pub ctx: &'a mut CheckerContext<'ctx>,
    /// Recursion depth counter for stack overflow protection.
    depth: DepthCounter,
}

pub(super) type TypeLiteralSignatureScopeUpdates = Vec<(String, Option<TypeId>)>;
impl<'a, 'ctx> TypeNodeChecker<'a, 'ctx> {
    /// Create a new type node checker with a mutable context reference.
    pub const fn new(ctx: &'a mut CheckerContext<'ctx>) -> Self {
        Self {
            ctx,
            depth: DepthCounter::with_profile(RecursionProfile::TypeNodeCheck),
        }
    }

    /// Check a type node and return its type.
    ///
    /// This is the main entry point for type node resolution.
    /// It handles caching and dispatches to specific type node handlers.
    pub fn check(&mut self, idx: NodeIndex) -> TypeId {
        // Stack overflow protection
        if !self.depth.enter() {
            return TypeId::ERROR;
        }

        // Check cache first
        if let Some(&cached) = self.ctx.node_types.get(&idx.0) {
            if cached == TypeId::ERROR {
                // Always use cached ERROR to prevent duplicate emissions
                self.depth.leave();
                return cached;
            }

            // For non-ERROR cached results, check if we're in a generic context
            // If we're not in a generic context (type params are empty), the cache is valid
            if self.ctx.type_parameter_scope.is_empty() {
                // No type parameters in scope - cache is valid
                self.depth.leave();
                return cached;
            }
            // If we have type parameters in scope, we need to be more careful
            // For now, recompute to ensure correctness
            // TODO: Add cache key based on type param hash for smarter caching
        }
        // Compute and cache
        let result = self.compute_type(idx);
        // Don't cache TYPE_REFERENCE results here — CheckerState's
        // get_type_from_type_node has its own TYPE_REFERENCE handler that
        // calls get_type_from_type_reference() which emits diagnostics
        // (TS2314, TS2304, etc.). If we cache here, the checker's handler
        // finds the cached result and skips the diagnostic-emitting path.
        let is_type_ref = self
            .ctx
            .arena
            .get(idx)
            .is_some_and(|n| n.kind == tsz_parser::parser::syntax_kind_ext::TYPE_REFERENCE);
        if !is_type_ref {
            self.ctx.node_types.insert(idx.0, result);
        }

        self.depth.leave();
        result
    }

    /// Compute the type of a type node (internal, not cached).
    fn compute_type(&mut self, idx: NodeIndex) -> TypeId {
        use tsz_parser::parser::syntax_kind_ext;

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };

        if let Some(builtin) = keyword_syntax_to_type_id(node.kind) {
            return builtin;
        }

        match node.kind {
            k if k == SyntaxKind::TrueKeyword as u16 => self.ctx.types.literal_boolean(true),

            k if k == SyntaxKind::FalseKeyword as u16 => self.ctx.types.literal_boolean(false),

            // Type reference (e.g., "MyType", "Array<T>")
            k if k == syntax_kind_ext::TYPE_REFERENCE => self.get_type_from_type_reference(idx),

            // Union type (A | B)
            k if k == syntax_kind_ext::UNION_TYPE => self.get_type_from_union_type(idx),

            // Intersection type (A & B)
            k if k == syntax_kind_ext::INTERSECTION_TYPE => {
                self.get_type_from_intersection_type(idx)
            }

            // Array type (T[])
            k if k == syntax_kind_ext::ARRAY_TYPE => self.get_type_from_array_type(idx),

            // Tuple type ([T, U, ...V[]])
            k if k == syntax_kind_ext::TUPLE_TYPE => self.get_type_from_tuple_type(idx),

            // Type operator (readonly, unique, keyof)
            k if k == syntax_kind_ext::TYPE_OPERATOR => self.get_type_from_type_operator(idx),

            // Indexed access type (T[K], Person["name"])
            k if k == syntax_kind_ext::INDEXED_ACCESS_TYPE => {
                self.get_type_from_indexed_access_type(idx)
            }

            // Function type (e.g., () => number, (x: string) => void)
            k if k == syntax_kind_ext::FUNCTION_TYPE => {
                // TS1385/TS1387: Function type notation must be parenthesized
                // when used in a union or intersection type.
                self.check_grammar_function_type_in_union_or_intersection(idx);
                self.get_type_from_function_type(idx)
            }

            // Constructor type (e.g., new () => number, new (x: string) => any)
            k if k == syntax_kind_ext::CONSTRUCTOR_TYPE => {
                // TS1386/TS1388: Constructor type notation must be parenthesized
                // when used in a union or intersection type.
                self.check_grammar_constructor_type_in_union_or_intersection(idx);
                self.get_type_from_function_type(idx)
            }

            // Type literal ({ a: number; b(): string; })
            k if k == syntax_kind_ext::TYPE_LITERAL => self.get_type_from_type_literal(idx),

            // Type query (typeof X) - returns the type of X
            k if k == syntax_kind_ext::TYPE_QUERY => self.get_type_from_type_query(idx),

            // Mapped type ({ [P in K]: T })
            // Check for TS7039 before TypeLowering since TypeLowering doesn't emit diagnostics
            k if k == syntax_kind_ext::MAPPED_TYPE => self.get_type_from_mapped_type(idx),

            k if k == syntax_kind_ext::THIS_TYPE
                || k == tsz_scanner::SyntaxKind::ThisKeyword as u16 =>
            {
                if !self.is_this_type_allowed(idx) {
                    use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                    self.ctx.error(
                        node.pos,
                        node.end.saturating_sub(node.pos),
                        diagnostic_messages::A_THIS_TYPE_IS_AVAILABLE_ONLY_IN_A_NON_STATIC_MEMBER_OF_A_CLASS_OR_INTERFACE.to_string(),
                        diagnostic_codes::A_THIS_TYPE_IS_AVAILABLE_ONLY_IN_A_NON_STATIC_MEMBER_OF_A_CLASS_OR_INTERFACE,
                    );
                    TypeId::ERROR
                } else {
                    self.ctx.types.this_type()
                }
            }

            // Fall back to TypeLowering for type nodes not handled above
            // (conditional types, indexed access types, etc.)
            _ => self.lower_with_resolvers(idx, true, true),
        }
    }

    /// Get type from a type reference node (e.g., "number", "string", "`MyType`").
    fn get_type_from_type_reference(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };
        if let Some(type_ref) = self.ctx.arena.get_type_ref(node)
            && let Some(mut resolved) = self.import_call_type_reference(type_ref.type_name)
        {
            if let Some(args) = &type_ref.type_arguments
                && !args.nodes.is_empty()
            {
                let type_args = args
                    .nodes
                    .iter()
                    .map(|&arg_idx| self.check(arg_idx))
                    .collect();
                resolved = self.ctx.types.application(resolved, type_args);
            }
            return resolved;
        }

        self.lower_with_resolvers(idx, false, true)
    }

    /// Get type from a union type node (A | B).
    ///
    /// Parses a union type expression and creates a Union type with all members.
    ///
    /// ## Type Normalization:
    /// - Empty union -> NEVER (the empty type)
    /// - Single member -> the member itself (no union wrapper)
    /// - Multiple members -> Union type with all members
    fn get_type_from_union_type(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };

        // UnionType uses CompositeTypeData which has a types list
        if let Some(composite) = self.ctx.arena.get_composite_type(node) {
            let mut member_types = Vec::new();
            for &type_idx in &composite.types.nodes {
                // Recursively resolve each member type
                member_types.push(self.check(type_idx));
            }

            if member_types.is_empty() {
                return TypeId::NEVER;
            }

            // Use literal-only reduction for type annotation unions to match tsc's
            // UnionReduction.Literal behavior. This preserves the union structure
            // (e.g., C | D stays as C | D even when D extends C) which is important
            // for TS2403 redeclaration checks and type display in diagnostics.
            let result = tsz_solver::utils::union_or_single_literal_reduce(
                self.ctx.types,
                member_types.clone(),
            );

            // Mirror tsc's `UnionType.origin`: record the as-written input
            // member list so the diagnostic printer can preserve top-level
            // alias names that union flattening would otherwise dissolve
            // (see `TypeInterner::store_union_origin`).
            self.ctx.types.store_union_origin(result, member_types);
            return result;
        }

        TypeId::ERROR
    }

    /// Get type from an intersection type node (A & B).
    ///
    /// Parses an intersection type expression and creates an Intersection type with all members.
    ///
    /// ## Type Normalization:
    /// - Empty intersection -> UNKNOWN (the top type for intersections)
    /// - Single member -> the member itself (no intersection wrapper)
    /// - Multiple members -> Intersection type with all members
    fn get_type_from_intersection_type(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };

        if super::window_global_this_annotation::is_window_and_typeof_global_this_type_node(
            self.ctx.arena,
            idx,
        ) {
            return TypeId::ANY;
        }

        // IntersectionType uses CompositeTypeData which has a types list
        if let Some(composite) = self.ctx.arena.get_composite_type(node) {
            let mut member_types = Vec::new();
            for &type_idx in &composite.types.nodes {
                // Recursively resolve each member type
                member_types.push(self.check(type_idx));
            }
            if member_types.is_empty() {
                return TypeId::UNKNOWN; // Empty intersection is unknown
            }

            return tsz_solver::utils::intersection_or_single(self.ctx.types, member_types);
        }

        TypeId::ERROR
    }
    /// Get type from an array type node (string[]).
    ///
    /// Parses an array type expression and creates an Array type.
    fn get_type_from_array_type(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };
        let factory = self.ctx.types.factory();

        if let Some(array_type) = self.ctx.arena.get_array_type(node) {
            let elem_type = self.check(array_type.element_type);
            return factory.array(elem_type);
        }

        TypeId::ERROR
    }

    /// Get type from a tuple type node ([T, U, ...V[]]).
    ///
    /// Parses a tuple type expression and creates a Tuple type with proper handling of:
    /// - Regular elements (e.g., `[number, string]`)
    /// - Optional elements (e.g., `[number, string?]`)
    /// - Rest elements (e.g., `[number, ...string[]]`)
    /// - Named elements (e.g., `[x: number, y: string]`)
    fn get_type_from_tuple_type(&mut self, idx: NodeIndex) -> TypeId {
        use tsz_solver::TupleElement;

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };
        let factory = self.ctx.types.factory();

        if self.tuple_type_directly_references_resolving_alias(idx) {
            self.ctx.error(
                node.pos,
                node.end - node.pos,
                crate::diagnostics::diagnostic_messages::TUPLE_TYPE_ARGUMENTS_CIRCULARLY_REFERENCE_THEMSELVES.to_string(),
                crate::diagnostics::diagnostic_codes::TUPLE_TYPE_ARGUMENTS_CIRCULARLY_REFERENCE_THEMSELVES,
            );
        }

        if let Some(tuple_type) = self.ctx.arena.get_tuple_type(node) {
            let mut elements = Vec::new();
            let mut seen_optional = false;
            let mut seen_rest = false;

            for &elem_idx in &tuple_type.elements.nodes {
                if elem_idx.is_none() {
                    continue;
                }

                let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                    continue;
                };

                use tsz_parser::parser::syntax_kind_ext;
                if elem_node.kind == syntax_kind_ext::OPTIONAL_TYPE {
                    if seen_rest {
                        self.ctx.error(
                            elem_node.pos,
                            elem_node.end - elem_node.pos,
                            crate::diagnostics::diagnostic_messages::AN_OPTIONAL_ELEMENT_CANNOT_FOLLOW_A_REST_ELEMENT.to_string(),
                            crate::diagnostics::diagnostic_codes::AN_OPTIONAL_ELEMENT_CANNOT_FOLLOW_A_REST_ELEMENT,
                        );
                    }
                    seen_optional = true;
                    if let Some(wrapped) = self.ctx.arena.get_wrapped_type(elem_node) {
                        let (inner_idx, is_rest_optional) = if let Some(inner_node) =
                            self.ctx.arena.get(wrapped.type_node)
                            && inner_node.kind == syntax_kind_ext::REST_TYPE
                            && let Some(inner_wrapped) = self.ctx.arena.get_wrapped_type(inner_node)
                        {
                            (inner_wrapped.type_node, true)
                        } else {
                            (wrapped.type_node, false)
                        };
                        let elem_type =
                            self.check_tuple_rest_type_node(inner_idx, is_rest_optional);
                        if is_rest_optional
                            && !self.is_array_or_tuple_type(elem_type)
                            && Self::ast_kind_is_obviously_non_array(self.ctx.arena, inner_idx)
                        {
                            self.emit_rest_element_type_must_be_array(elem_node.pos, elem_node.end);
                        }
                        elements.push(TupleElement {
                            type_id: elem_type,
                            name: None,
                            optional: true,
                            rest: is_rest_optional,
                        });
                    }
                } else if elem_node.kind == syntax_kind_ext::REST_TYPE {
                    if let Some(wrapped) = self.ctx.arena.get_wrapped_type(elem_node) {
                        let elem_type = self.check_tuple_rest_type_node(wrapped.type_node, true);
                        if let Some(spread_elements) = self.fixed_tuple_spread_elements(elem_type) {
                            elements.extend(spread_elements);
                            continue;
                        }
                        let is_concrete_rest = self.is_variadic_array_or_tuple(elem_type)
                            || Self::ast_kind_is_obviously_array_or_tuple(
                                self.ctx.arena,
                                wrapped.type_node,
                            );
                        if is_concrete_rest {
                            if seen_rest {
                                self.ctx.error(
                                    elem_node.pos,
                                    elem_node.end - elem_node.pos,
                                    crate::diagnostics::diagnostic_messages::A_REST_ELEMENT_CANNOT_FOLLOW_ANOTHER_REST_ELEMENT.to_string(),
                                    crate::diagnostics::diagnostic_codes::A_REST_ELEMENT_CANNOT_FOLLOW_ANOTHER_REST_ELEMENT,
                                );
                            }
                            seen_rest = true;
                        } else if Self::ast_kind_is_obviously_non_array(
                            self.ctx.arena,
                            wrapped.type_node,
                        ) {
                            self.emit_rest_element_type_must_be_array(elem_node.pos, elem_node.end);
                        }
                        elements.push(TupleElement {
                            type_id: elem_type,
                            name: None,
                            optional: false,
                            rest: true,
                        });
                    }
                } else if elem_node.kind == syntax_kind_ext::NAMED_TUPLE_MEMBER {
                    if let Some(data) = self.ctx.arena.get_named_tuple_member(elem_node) {
                        let elem_type =
                            self.check_tuple_rest_type_node(data.type_node, data.dot_dot_dot_token);
                        let misplaced_optional_marker =
                            !data.question_token
                                && self.ctx.arena.get(data.type_node).is_some_and(|node| {
                                    node.kind == syntax_kind_ext::OPTIONAL_TYPE
                                });
                        let name = self
                            .ctx
                            .arena
                            .get(data.name)
                            .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                            .map(|id_data| self.ctx.types.intern_string(&id_data.escaped_text));

                        if data.dot_dot_dot_token {
                            if let Some(spread_elements) =
                                self.fixed_tuple_spread_elements(elem_type)
                            {
                                elements.extend(spread_elements);
                                continue;
                            }
                            let is_concrete_rest = self.is_variadic_array_or_tuple(elem_type)
                                || Self::ast_kind_is_obviously_array_or_tuple(
                                    self.ctx.arena,
                                    data.type_node,
                                );
                            if is_concrete_rest {
                                if seen_rest {
                                    self.ctx.error(
                                        elem_node.pos,
                                        elem_node.end - elem_node.pos,
                                        crate::diagnostics::diagnostic_messages::A_REST_ELEMENT_CANNOT_FOLLOW_ANOTHER_REST_ELEMENT.to_string(),
                                        crate::diagnostics::diagnostic_codes::A_REST_ELEMENT_CANNOT_FOLLOW_ANOTHER_REST_ELEMENT,
                                    );
                                }
                                seen_rest = true;
                            } else if Self::ast_kind_is_obviously_non_array(
                                self.ctx.arena,
                                data.type_node,
                            ) {
                                self.emit_rest_element_type_must_be_array(
                                    elem_node.pos,
                                    elem_node.end,
                                );
                            }
                        } else if data.question_token || misplaced_optional_marker {
                            if misplaced_optional_marker {
                                self.ctx.error(
                                    elem_node.pos,
                                    elem_node.end - elem_node.pos,
                                    crate::diagnostics::diagnostic_messages::A_LABELED_TUPLE_ELEMENT_IS_DECLARED_AS_OPTIONAL_WITH_A_QUESTION_MARK_AFTER_THE_N.to_string(),
                                    crate::diagnostics::diagnostic_codes::A_LABELED_TUPLE_ELEMENT_IS_DECLARED_AS_OPTIONAL_WITH_A_QUESTION_MARK_AFTER_THE_N,
                                );
                            }
                            if seen_rest {
                                self.ctx.error(
                                    elem_node.pos,
                                    elem_node.end - elem_node.pos,
                                    crate::diagnostics::diagnostic_messages::AN_OPTIONAL_ELEMENT_CANNOT_FOLLOW_A_REST_ELEMENT.to_string(),
                                    crate::diagnostics::diagnostic_codes::AN_OPTIONAL_ELEMENT_CANNOT_FOLLOW_A_REST_ELEMENT,
                                );
                            }
                            seen_optional = true;
                        } else if seen_optional {
                            self.ctx.error(
                                elem_node.pos,
                                elem_node.end - elem_node.pos,
                                crate::diagnostics::diagnostic_messages::A_REQUIRED_ELEMENT_CANNOT_FOLLOW_AN_OPTIONAL_ELEMENT.to_string(),
                                crate::diagnostics::diagnostic_codes::A_REQUIRED_ELEMENT_CANNOT_FOLLOW_AN_OPTIONAL_ELEMENT,
                            );
                        }

                        elements.push(TupleElement {
                            type_id: elem_type,
                            name,
                            optional: data.question_token || misplaced_optional_marker,
                            rest: data.dot_dot_dot_token,
                        });
                    }
                } else {
                    // Regular element
                    // TS1257: A required element cannot follow an optional element
                    if seen_optional {
                        self.ctx.error(
                            elem_node.pos,
                            elem_node.end - elem_node.pos,
                            crate::diagnostics::diagnostic_messages::A_REQUIRED_ELEMENT_CANNOT_FOLLOW_AN_OPTIONAL_ELEMENT.to_string(),
                            crate::diagnostics::diagnostic_codes::A_REQUIRED_ELEMENT_CANNOT_FOLLOW_AN_OPTIONAL_ELEMENT,
                        );
                    }
                    let elem_type = self.check(elem_idx);
                    elements.push(TupleElement {
                        type_id: elem_type,
                        name: None,
                        optional: false,
                        rest: false,
                    });
                }
            }

            return factory.tuple(elements);
        }

        TypeId::ERROR
    }

    fn tuple_type_directly_references_resolving_alias(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        let Some(tuple_type) = self.ctx.arena.get_tuple_type(node) else {
            return false;
        };

        tuple_type
            .elements
            .nodes
            .iter()
            .copied()
            .any(|elem_idx| self.type_node_references_resolving_alias(elem_idx, true, false))
    }

    /// Check whether a type node references a type alias currently being resolved,
    /// in a way that creates a true circularity for TS4110.
    ///
    /// In TSC, TS4110 fires only when resolving a tuple element requires evaluating
    /// the alias itself (via `pushTypeResolution`/`popTypeResolution`).  A bare
    /// `TypeReference` to the alias (e.g. `type T = [string, T]`) does NOT trigger
    /// TS4110 because type references produce deferred (lazy) types.  Only when the
    /// alias appears inside a computation context that forces immediate evaluation --
    /// such as indexed access (`T[0]`) -- does the circularity fire.
    ///
    /// `inside_computation` tracks whether we are inside a node that requires
    /// immediate type evaluation (indexed access type, conditional type, etc.).
    fn type_node_references_resolving_alias(
        &self,
        node_idx: NodeIndex,
        stop_at_nested_tuple: bool,
        inside_computation: bool,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };

        if stop_at_nested_tuple && node.kind == syntax_kind_ext::TUPLE_TYPE {
            return false;
        }

        // Array types, function types, and other type constructs create deferred type
        // references that break circularity.  In TSC, these are "deferred type reference
        // nodes" and self-references through them do NOT trigger TS4110.  For example,
        // `type T = ["or", T[]]` is valid because `T[]` is deferred.
        if matches!(
            node.kind,
            k if k == syntax_kind_ext::ARRAY_TYPE
                || k == syntax_kind_ext::FUNCTION_TYPE
                || k == syntax_kind_ext::CONSTRUCTOR_TYPE
                || k == syntax_kind_ext::TYPE_LITERAL
                || k == syntax_kind_ext::MAPPED_TYPE
                || k == syntax_kind_ext::TYPE_QUERY
        ) {
            return false;
        }

        if node.kind == SyntaxKind::Identifier as u16
            || node.kind == syntax_kind_ext::TYPE_REFERENCE
        {
            let sym_id = if node.kind == syntax_kind_ext::TYPE_REFERENCE {
                self.ctx
                    .arena
                    .get_type_ref(node)
                    .and_then(|type_ref| self.resolve_type_symbol(type_ref.type_name))
                    .map(SymbolId)
            } else {
                self.resolve_type_symbol(node_idx).map(SymbolId)
            };

            if let Some(sym_id) = sym_id
                && self.ctx.symbol_resolution_set.contains(&sym_id)
                && self.ctx.binder.get_symbol(sym_id).is_some_and(|symbol| {
                    symbol.has_any_flags(tsz_binder::symbol_flags::TYPE_ALIAS)
                })
            {
                // A TypeReference with type arguments (e.g. `C1<T>`) creates a new
                // instantiation boundary -- not circular even inside computation.
                if node.kind == syntax_kind_ext::TYPE_REFERENCE {
                    let has_args = self
                        .ctx
                        .arena
                        .get_type_ref(node)
                        .is_some_and(|tr| tr.type_arguments.is_some());
                    if has_args {
                        return false;
                    }
                }
                // Only flag as circular if we are inside a computation context
                // (indexed access, conditional type).  A bare TypeReference to the
                // alias is deferred in TSC and does not cause circularity.
                return inside_computation;
            }

            // A TypeReference to a different type is a deferred boundary -- do not
            // recurse into its type arguments.
            if node.kind == syntax_kind_ext::TYPE_REFERENCE {
                return false;
            }
        }

        // Indexed access types and conditional types are "computation" contexts:
        // resolving them forces immediate evaluation of the alias.
        let enters_computation = matches!(
            node.kind,
            k if k == syntax_kind_ext::INDEXED_ACCESS_TYPE
                || k == syntax_kind_ext::CONDITIONAL_TYPE
        );
        let child_inside = inside_computation || enters_computation;

        for child_idx in self.ctx.arena.get_children(node_idx) {
            if self.type_node_references_resolving_alias(child_idx, false, child_inside) {
                return true;
            }
        }

        false
    }

    /// Get type from a function type node (e.g., () => number, (x: string) => void).
    fn get_type_from_function_type(&mut self, idx: NodeIndex) -> TypeId {
        let Some(_node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };
        let Some(func_data) = self.ctx.arena.get_function_type(_node) else {
            return TypeId::ERROR;
        };

        let (_type_params, type_param_updates) =
            self.push_type_parameters_for_type_literal_signature(&func_data.type_parameters);

        // EXPLICIT VALIDATION: Check type references in parameters and return type for TS2304.
        // We must do this before TypeLowering because TypeLowering doesn't emit diagnostics.
        // This ensures errors like "Cannot find name 'C'" are emitted for: (x: T) => C
        check_duplicate_parameters_in_type(self.ctx, &func_data.parameters);
        check_parameter_initializers_in_type(self.ctx, &func_data.parameters);

        use tsz_parser::parser::syntax_kind_ext;

        // Collect type parameter names from this function type (e.g., <T> in <T>(x: T) => T)
        let mut local_type_params: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        if let Some(ref type_params) = func_data.type_parameters {
            for &tp_idx in &type_params.nodes {
                if let Some(tp_node) = self.ctx.arena.get(tp_idx)
                    && let Some(tp_data) = self.ctx.arena.get_type_parameter(tp_node)
                    && let Some(name_node) = self.ctx.arena.get(tp_data.name)
                    && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                {
                    local_type_params.insert(ident.escaped_text.clone());
                }
            }
        }

        // Helper to check if a type name is a built-in TypeScript type
        let is_builtin_type = |name: &str| -> bool {
            matches!(
                name,
                // Primitive types
                "void" | "null" | "undefined" | "any" | "unknown" | "never" |
                "number" | "bigint" | "boolean" | "string" | "symbol" | "object" |
                // Special types
                "Function" | "Object" | "String" | "Number" | "Boolean" | "Symbol" |
                // Compiler-managed
                "Array" | "ReadonlyArray" | "Uppercase" | "Lowercase" | "Capitalize" | "Uncapitalize"
            )
        };

        // Collect undefined type names first (to avoid borrow checker issues)
        let mut undefined_types: Vec<(NodeIndex, String)> = Vec::new();
        let mut renamed_binding_aliases: std::collections::HashSet<String> =
            std::collections::HashSet::new();

        for &param_idx in &func_data.parameters.nodes {
            let mut stack = vec![param_idx];
            while let Some(node_idx) = stack.pop() {
                let Some(binding_node) = self.ctx.arena.get(node_idx) else {
                    continue;
                };
                if binding_node.kind == syntax_kind_ext::BINDING_ELEMENT
                    && let Some(binding) = self.ctx.arena.get_binding_element(binding_node)
                    && binding.property_name.is_some()
                    && binding.name.is_some()
                    && let Some(alias_name) = self.ctx.arena.get_identifier_text(binding.name)
                {
                    renamed_binding_aliases.insert(alias_name.to_string());
                }
                stack.extend(self.ctx.arena.get_children(node_idx));
            }
        }

        // Helper: check if a type name is resolvable in any scope (file locals,
        // lib contexts, enclosing namespace scopes via binder identifier resolution).
        let is_name_resolvable =
            |ctx: &CheckerContext, name: &str, name_node_idx: NodeIndex| -> bool {
                // Check file-level declarations
                if ctx.binder.file_locals.get(name).is_some() {
                    return true;
                }
                // Check lib declarations
                if ctx
                    .lib_contexts
                    .iter()
                    .any(|lib_ctx| lib_ctx.binder.file_locals.get(name).is_some())
                {
                    return true;
                }
                // Check scope-based resolution (handles namespace-scoped names)
                if ctx
                    .binder
                    .resolve_identifier(ctx.arena, name_node_idx)
                    .is_some()
                {
                    return true;
                }
                false
            };

        // Check return type annotation
        if func_data.type_annotation.is_some()
            && let Some(tn) = self.ctx.arena.get(func_data.type_annotation)
            && tn.kind == syntax_kind_ext::TYPE_REFERENCE
            && let Some(tr) = self.ctx.arena.get_type_ref(tn)
            && let Some(name_node) = self.ctx.arena.get(tr.type_name)
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
        {
            let name = &ident.escaped_text;
            let is_builtin = is_builtin_type(name);
            let is_local_type_param = local_type_params.contains(name);
            let is_type_param = self.ctx.type_parameter_scope.contains_key(name);
            let in_scope = is_name_resolvable(self.ctx, name, tr.type_name);

            if !is_builtin && !is_local_type_param && !is_type_param && !in_scope {
                undefined_types.push((tr.type_name, name.clone()));
            }
        }

        super::type_node_helpers::report_type_predicate_in_constructor_type(
            self.ctx,
            _node.kind,
            func_data.type_annotation,
        );

        // Check parameter type annotations
        for param_idx in &func_data.parameters.nodes {
            if let Some(param_node) = self.ctx.arena.get(*param_idx)
                && let Some(param_data) = self.ctx.arena.get_parameter(param_node)
                && param_data.type_annotation.is_some()
                && let Some(tn) = self.ctx.arena.get(param_data.type_annotation)
                && tn.kind == syntax_kind_ext::TYPE_REFERENCE
                && let Some(tr) = self.ctx.arena.get_type_ref(tn)
                && let Some(name_node) = self.ctx.arena.get(tr.type_name)
                && let Some(ident) = self.ctx.arena.get_identifier(name_node)
            {
                let name = &ident.escaped_text;
                let is_builtin = is_builtin_type(name);
                let is_local_type_param = local_type_params.contains(name);
                let is_type_param = self.ctx.type_parameter_scope.contains_key(name);
                let in_scope = is_name_resolvable(self.ctx, name, tr.type_name);

                if !is_builtin && !is_local_type_param && !is_type_param && !in_scope {
                    undefined_types.push((tr.type_name, name.clone()));
                }
            }
        }

        // Now emit all the TS2304 errors.
        // In JS files, suppress TS2304 for names inside syntactic type annotations.
        // tsc emits TS8010 for these but does NOT attempt name resolution.
        let suppress_for_js = self.ctx.is_js_file();
        if !suppress_for_js {
            for (error_idx, name) in undefined_types {
                if renamed_binding_aliases.contains(&name) {
                    continue;
                }
                if let Some(node) = self.ctx.arena.get(error_idx) {
                    let message = format!("Cannot find name '{name}'.");
                    self.ctx.error(node.pos, node.end - node.pos, message, 2304);
                }
            }
        }

        // The return type of a function type is processed through TypeLowering,
        // which doesn't trigger grammar checks. Recursively scan the return type
        // subtree for TS1385/TS1387 (unparenthesized function/constructor types in
        // union/intersection contexts) to match tsc's parser-level detection.
        self.check_nested_function_types_in_type(func_data.type_annotation);

        // Delegate to TypeLowering with standard resolvers.
        // Enable qualified name resolution so return types like `Ns.Type<T>`
        // resolve correctly (QUALIFIED_NAME nodes need the extended resolver).
        let result = self.lower_with_resolvers(idx, true, true);

        // TS2677: Check that a type predicate's type is assignable to its parameter's type.
        self.check_type_predicate_assignability(idx, func_data.type_annotation, result);

        self.pop_type_parameters_for_type_literal_signature(type_param_updates);

        result
    }

    /// TS2677: A type predicate's type must be assignable to its parameter's type.
    fn check_type_predicate_assignability(
        &mut self,
        function_type_idx: NodeIndex,
        type_annotation: NodeIndex,
        lowered_type: TypeId,
    ) {
        if type_annotation.is_none() {
            return;
        }
        let predicate_node_idx = match self.find_type_predicate_in_type(type_annotation) {
            Some(idx) => idx,
            None => return,
        };
        let Some(pred_node) = self.ctx.arena.get(predicate_node_idx) else {
            return;
        };
        let Some(pred_data) = self.ctx.arena.get_type_predicate(pred_node) else {
            return;
        };
        if pred_data.type_node.is_none() {
            return;
        }
        let Some(predicate_name) = self.ctx.arena.get_identifier_text(pred_data.parameter_name)
        else {
            return;
        };

        let mut predicate_type = self.check(pred_data.type_node);

        // When the predicate type was parsed from `?T` (prefix ?), the parser recovers
        // just `T` but tsc semantically treats it as `T | null | undefined`. Detect this
        // by checking if the type node's position matches a nullable-type parse error.
        // Only `?`-related errors (TS17019/TS17020) trigger widening; `!`-related errors
        // should not widen since the recovered type is already correct.
        if let Some(type_node) = self.ctx.arena.get(pred_data.type_node) {
            let type_pos = type_node.pos;
            if self
                .ctx
                .nullable_type_parse_error_positions
                .contains(&type_pos)
            {
                // Widen predicate type to T | null | undefined to match tsc behavior
                predicate_type = self.ctx.types.factory().union(vec![
                    predicate_type,
                    TypeId::NULL,
                    TypeId::UNDEFINED,
                ]);
            }
        }

        let mut param_type = None;

        if let Some(function_node) = self.ctx.arena.get(function_type_idx)
            && let Some(function_data) = self.ctx.arena.get_function_type(function_node)
        {
            for &param_idx in &function_data.parameters.nodes {
                let Some(param_node) = self.ctx.arena.get(param_idx) else {
                    continue;
                };
                let Some(param_data) = self.ctx.arena.get_parameter(param_node) else {
                    continue;
                };
                if self.ctx.arena.get_identifier_text(param_data.name) == Some(predicate_name) {
                    param_type = (param_data.type_annotation.is_some())
                        .then(|| self.check(param_data.type_annotation));
                    break;
                }
            }
        }

        let (predicate_type, param_type) = if let Some(param_type) = param_type {
            (predicate_type, param_type)
        } else {
            let Some(shape) = crate::query_boundaries::common::function_shape_for_type(
                self.ctx.types,
                lowered_type,
            ) else {
                return;
            };
            let Some(ref predicate) = shape.type_predicate else {
                return;
            };
            let Some(predicate_type) = predicate.type_id else {
                return;
            };
            let Some(param_index) = predicate.parameter_index else {
                return;
            };
            let Some(param) = shape.params.get(param_index) else {
                return;
            };
            (predicate_type, param.type_id)
        };
        // Skip the check when the predicate type is an unevaluable Application
        // (e.g., NonNullable<T> where T is a free type parameter). Our evaluator
        // can't resolve all lib.d.ts type aliases yet, so the Application stays
        // opaque and fails the assignability check even when it's structurally sound
        // (e.g., NonNullable<T> = T & {} which is always assignable to T).
        // TSC resolves these and succeeds; we defer to avoid false TS2677 errors.
        if self.predicate_type_contains_unevaluable_application(predicate_type) {
            return;
        }
        // TSC checks: checkTypeAssignableTo(predicateType, paramType).
        // For type parameters with an explicit constraint (`T extends X`), the
        // constraint is by definition assignable to the param type when the param
        // type IS that constraint. Skip the check for constrained type parameters
        // to avoid false positives from TypeId dedup issues with recursive types.
        // For unconstrained type parameters, use `unknown` as the implicit constraint.
        let resolved_predicate = if crate::query_boundaries::common::is_type_parameter_like(
            self.ctx.types,
            predicate_type,
        ) {
            match crate::query_boundaries::common::type_param_info(self.ctx.types, predicate_type)
                .and_then(|info| info.constraint)
            {
                Some(_) => return, // Constrained type param: always assignable to its constraint
                None => TypeId::UNKNOWN,
            }
        } else {
            predicate_type
        };
        let resolved_param = if crate::query_boundaries::common::is_type_parameter_like(
            self.ctx.types,
            param_type,
        ) {
            match crate::query_boundaries::common::type_param_info(self.ctx.types, param_type)
                .and_then(|info| info.constraint)
            {
                Some(c) => c,
                None => TypeId::UNKNOWN,
            }
        } else {
            param_type
        };

        let types = self.ctx.types;
        if !crate::query_boundaries::type_predicates::type_predicate_type_assignable_to_parameter_with(
            types,
            resolved_predicate,
            resolved_param,
            |source, target| types.is_assignable_to(source, target),
        ) && let Some(type_node) = self.ctx.arena.get(pred_data.type_node)
        {
            self.ctx.error(
                type_node.pos,
                type_node.end - type_node.pos,
                "A type predicate's type must be assignable to its parameter's type.".to_string(),
                2677,
            );
        }
    }

    /// Check if a type contains an Application that can't be evaluated (e.g., `NonNullable<T>`
    /// where the resolver doesn't know about the base type's definition). In such cases,
    /// the Application stays opaque and assignability checks may give incorrect results.
    fn predicate_type_contains_unevaluable_application(&self, type_id: TypeId) -> bool {
        if crate::query_boundaries::common::application_info(self.ctx.types, type_id).is_some() {
            // If evaluate_type returns the same TypeId, the Application couldn't be resolved
            let evaluated = self.ctx.types.evaluate_type(type_id);
            return evaluated == type_id;
        }
        if let Some(members) =
            crate::query_boundaries::common::intersection_members(self.ctx.types, type_id)
        {
            return members
                .iter()
                .any(|&m| self.predicate_type_contains_unevaluable_application(m));
        }
        if let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, type_id)
        {
            return members
                .iter()
                .any(|&m| self.predicate_type_contains_unevaluable_application(m));
        }
        false
    }

    fn find_type_predicate_in_type(&self, node_idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.ctx.arena.get(node_idx)?;
        match node.kind {
            k if k == syntax_kind_ext::TYPE_PREDICATE => Some(node_idx),
            k if k == syntax_kind_ext::PARENTHESIZED_TYPE => {
                let wrapped = self.ctx.arena.get_wrapped_type(node)?;
                self.find_type_predicate_in_type(wrapped.type_node)
            }
            k if k == syntax_kind_ext::INTERSECTION_TYPE => {
                let composite = self.ctx.arena.get_composite_type(node)?;
                for &member in &composite.types.nodes {
                    if let Some(found) = self.find_type_predicate_in_type(member) {
                        return Some(found);
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Get type from a type literal node ({ a: number; `b()`: string; }).
    fn get_type_from_type_literal(&mut self, idx: NodeIndex) -> TypeId {
        use tsz_parser::parser::syntax_kind_ext::{
            CALL_SIGNATURE, CONSTRUCT_SIGNATURE, METHOD_SIGNATURE, PROPERTY_SIGNATURE,
        };
        use tsz_solver::{
            CallSignature, CallableShape, FunctionShape, IndexSignature, ObjectShape, PropertyInfo,
        };

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };

        let Some(data) = self.ctx.arena.get_type_literal(node) else {
            return TypeId::ERROR;
        };

        let mut properties = Vec::new();
        let mut call_signatures = Vec::new();
        let mut construct_signatures = Vec::new();
        let mut string_index = None;
        let mut number_index = None;

        for &member_idx in &data.members.nodes {
            let Some(member) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            if let Some(sig) = self.ctx.arena.get_signature(member) {
                match member.kind {
                    CALL_SIGNATURE => {
                        let (type_params, type_param_updates) = self
                            .push_type_parameters_for_type_literal_signature(&sig.type_parameters);
                        let (params, this_type) = self.extract_params_from_signature(sig);
                        let return_type = self
                            .resolve_return_type_with_params_in_scope(sig.type_annotation, &params);
                        call_signatures.push(CallSignature {
                            type_params,
                            params,
                            this_type,
                            return_type,
                            type_predicate: None,
                            is_method: false,
                        });
                        self.pop_type_parameters_for_type_literal_signature(type_param_updates);
                    }
                    CONSTRUCT_SIGNATURE => {
                        let (type_params, type_param_updates) = self
                            .push_type_parameters_for_type_literal_signature(&sig.type_parameters);
                        let (params, this_type) = self.extract_params_from_signature(sig);
                        let return_type = self
                            .resolve_return_type_with_params_in_scope(sig.type_annotation, &params);
                        construct_signatures.push(CallSignature {
                            type_params,
                            params,
                            this_type,
                            return_type,
                            type_predicate: None,
                            is_method: false,
                        });
                        self.pop_type_parameters_for_type_literal_signature(type_param_updates);
                    }
                    METHOD_SIGNATURE | PROPERTY_SIGNATURE => {
                        let Some(name) = self.get_property_name_resolved(sig.name) else {
                            continue;
                        };
                        let name_atom = self.ctx.types.intern_string(&name);
                        let is_symbol_named = self.is_symbol_property_name(sig.name);

                        if member.kind == METHOD_SIGNATURE {
                            let (type_params, type_param_updates) = self
                                .push_type_parameters_for_type_literal_signature(
                                    &sig.type_parameters,
                                );
                            let (params, this_type) = self.extract_params_from_signature(sig);
                            let return_type = self.resolve_return_type_with_params_in_scope(
                                sig.type_annotation,
                                &params,
                            );
                            let shape = FunctionShape {
                                type_params,
                                params,
                                this_type,
                                return_type,
                                type_predicate: None,
                                is_constructor: false,
                                is_method: true,
                            };
                            let factory = self.ctx.types.factory();
                            let method_type = factory.function(shape);
                            self.pop_type_parameters_for_type_literal_signature(type_param_updates);
                            properties.push(PropertyInfo {
                                name: name_atom,
                                type_id: method_type,
                                write_type: method_type,
                                optional: sig.question_token,
                                readonly: self.ctx.arena.has_modifier(
                                    &sig.modifiers,
                                    tsz_scanner::SyntaxKind::ReadonlyKeyword,
                                ),
                                is_method: true,
                                is_class_prototype: false,
                                visibility: Visibility::Public,
                                parent_id: None,
                                declaration_order: (properties.len() + 1) as u32,
                                is_string_named: false,
                                is_symbol_named,
                                single_quoted_name: false,
                            });
                        } else {
                            let type_id = if sig.type_annotation.is_some() {
                                self.check(sig.type_annotation)
                            } else {
                                TypeId::ANY
                            };
                            properties.push(PropertyInfo {
                                name: name_atom,
                                type_id,
                                write_type: type_id,
                                optional: sig.question_token,
                                readonly: self.ctx.arena.has_modifier(
                                    &sig.modifiers,
                                    tsz_scanner::SyntaxKind::ReadonlyKeyword,
                                ),
                                is_method: false,
                                is_class_prototype: false,
                                visibility: Visibility::Public,
                                parent_id: None,
                                declaration_order: (properties.len() + 1) as u32,
                                is_string_named: false,
                                is_symbol_named,
                                single_quoted_name: false,
                            });
                        }
                    }
                    _ => {}
                }
                continue;
            }

            if let Some(index_sig) = self.ctx.arena.get_index_signature(member) {
                let param_idx = index_sig
                    .parameters
                    .nodes
                    .first()
                    .copied()
                    .unwrap_or(NodeIndex::NONE);
                let Some(param_node) = self.ctx.arena.get(param_idx) else {
                    continue;
                };
                let Some(param_data) = self.ctx.arena.get_parameter(param_node) else {
                    continue;
                };
                let key_type = if param_data.type_annotation.is_some() {
                    self.check(param_data.type_annotation)
                } else {
                    TypeId::ANY
                };

                // TS1337 / TS1268: Validate index signature parameter type.
                // Suppress when the parameter already has grammar errors (rest/optional) — matches tsc.
                let has_param_grammar_error =
                    param_data.dot_dot_dot_token || param_data.question_token;
                let mut is_valid_index_type = false;
                let mut is_valid_via_ast = false;
                if !has_param_grammar_error && param_data.type_annotation.is_some() {
                    // Check AST node kind to detect type parameters and literals (TS1337)
                    // before the resolved-type check. Type params like `T extends string`
                    // resolve to STRING but are still invalid as index sig param types.
                    let is_generic_or_literal =
                        self.is_type_param_or_literal_in_index_sig(param_data.type_annotation);
                    if is_generic_or_literal {
                        if let Some(pnode) = self.ctx.arena.get(param_idx) {
                            self.ctx.error(
                                pnode.pos,
                                pnode.end - pnode.pos,
                                "An index signature parameter type cannot be a literal type or generic type. Consider using a mapped object type instead.".to_string(),
                                1337,
                            );
                        }
                    } else {
                        is_valid_index_type = key_type == TypeId::STRING
                            || key_type == TypeId::NUMBER
                            || key_type == TypeId::SYMBOL
                            || crate::query_boundaries::common::is_template_literal_type(
                                self.ctx.types,
                                key_type,
                            );
                        // AST fallback: unions of valid types and non-generic
                        // intersections (`string | number`, `string & Tag`)
                        // resolve to composite TypeIds that don't match the
                        // primitive checks above.
                        is_valid_via_ast = !is_valid_index_type
                            && crate::query_boundaries::index_signature::is_valid_index_sig_param_type_ast(
                                self.ctx.arena,
                                self.ctx.binder,
                                param_data.type_annotation,
                            );
                        if !is_valid_index_type
                            && !is_valid_via_ast
                            && let Some(pnode) = self.ctx.arena.get(param_idx)
                        {
                            self.ctx.error(
                                    pnode.pos,
                                    pnode.end - pnode.pos,
                                    "An index signature parameter type must be 'string', 'number', 'symbol', or a template literal type.".to_string(),
                                    1268,
                                );
                        }
                    }
                }

                // TS2693: Check if parameter name without type annotation
                // refers to a type (e.g., `[K]: number` where `K` is a type alias).
                if !has_param_grammar_error
                    && param_data.type_annotation.is_none()
                    && let Some(name_node) = self.ctx.arena.get(param_data.name)
                    && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                {
                    let name = &ident.escaped_text;
                    // Check if this identifier resolves to a type symbol
                    if let Some(sym_id) = self
                        .ctx
                        .binder
                        .resolve_identifier(self.ctx.arena, param_data.name)
                        && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                    {
                        let has_type = symbol.has_any_flags(
                            tsz_binder::symbol_flags::TYPE
                                | tsz_binder::symbol_flags::TYPE_ALIAS
                                | tsz_binder::symbol_flags::INTERFACE,
                        );
                        let has_value = symbol.has_any_flags(tsz_binder::symbol_flags::VALUE);
                        if has_type && !has_value {
                            // The identifier refers to a type-only symbol
                            // Emit TS2693: Type only used as value
                            use crate::diagnostics::{
                                diagnostic_codes, diagnostic_messages, format_message,
                            };
                            let message = format_message(
                                            diagnostic_messages::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE,
                                            &[name],
                                        );
                            self.ctx.error(
                                            name_node.pos,
                                            name_node.end - name_node.pos,
                                            message,
                                            diagnostic_codes::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE,
                                        );
                        }
                    }
                }

                let value_type = if index_sig.type_annotation.is_some() {
                    self.check(index_sig.type_annotation)
                } else {
                    TypeId::ANY
                };
                let readonly = self.ctx.arena.has_modifier(
                    &index_sig.modifiers,
                    tsz_scanner::SyntaxKind::ReadonlyKeyword,
                );
                let param_name = self
                    .ctx
                    .arena
                    .get(param_data.name)
                    .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                    .map(|name_ident| self.ctx.types.intern_string(&name_ident.escaped_text));
                let info = IndexSignature {
                    key_type,
                    value_type,
                    readonly,
                    param_name,
                };
                if is_valid_index_type || is_valid_via_ast {
                    if key_type == TypeId::NUMBER {
                        number_index = Some(info);
                    } else {
                        string_index = Some(info);
                    }
                }
                continue;
            }

            // Handle accessor declarations (get/set) in type literals
            if (member.kind == tsz_parser::parser::syntax_kind_ext::GET_ACCESSOR
                || member.kind == tsz_parser::parser::syntax_kind_ext::SET_ACCESSOR)
                && let Some(accessor) = self.ctx.arena.get_accessor(member)
                && let Some(name) = self.get_property_name_resolved(accessor.name)
            {
                let name_atom = self.ctx.types.intern_string(&name);
                let is_symbol_named = self.is_symbol_property_name(accessor.name);
                let is_getter = member.kind == tsz_parser::parser::syntax_kind_ext::GET_ACCESSOR;
                if is_getter {
                    let getter_type = if accessor.type_annotation.is_some() {
                        self.check(accessor.type_annotation)
                    } else {
                        TypeId::ANY
                    };
                    if let Some(existing) = properties.iter_mut().find(|p| p.name == name_atom) {
                        existing.type_id = getter_type;
                    } else {
                        properties.push(PropertyInfo {
                            name: name_atom,
                            type_id: getter_type,
                            write_type: getter_type,
                            optional: false,
                            readonly: false,
                            is_method: false,
                            is_class_prototype: false,
                            visibility: Visibility::Public,
                            parent_id: None,
                            declaration_order: (properties.len() + 1) as u32,
                            is_string_named: false,
                            is_symbol_named,
                            single_quoted_name: false,
                        });
                    }
                } else {
                    let setter_type = accessor
                        .parameters
                        .nodes
                        .first()
                        .and_then(|&param_idx| self.ctx.arena.get(param_idx))
                        .and_then(|param_node| self.ctx.arena.get_parameter(param_node))
                        .and_then(|param| {
                            (param.type_annotation.is_some())
                                .then(|| self.check(param.type_annotation))
                        })
                        .unwrap_or(TypeId::UNKNOWN);
                    if let Some(existing) = properties.iter_mut().find(|p| p.name == name_atom) {
                        existing.write_type = setter_type;
                        existing.readonly = false;
                    } else {
                        properties.push(PropertyInfo {
                            name: name_atom,
                            type_id: setter_type,
                            write_type: setter_type,
                            optional: false,
                            readonly: false,
                            is_method: false,
                            is_class_prototype: false,
                            visibility: Visibility::Public,
                            parent_id: None,
                            declaration_order: (properties.len() + 1) as u32,
                            is_string_named: false,
                            is_symbol_named,
                            single_quoted_name: false,
                        });
                    }
                }
            }
        }

        if !call_signatures.is_empty() || !construct_signatures.is_empty() {
            let factory = self.ctx.types.factory();

            return factory.callable(CallableShape {
                call_signatures,
                construct_signatures,
                properties,
                string_index,
                number_index,
                symbol: None,
                is_abstract: false,
            });
        }

        if string_index.is_some() || number_index.is_some() {
            let factory = self.ctx.types.factory();

            return factory.object_with_index(ObjectShape {
                properties,
                string_index,
                number_index,
                ..ObjectShape::default()
            });
        }

        let factory = self.ctx.types.factory();
        factory.object(properties)
    }

    /// Resolve a type symbol from a node index.
    /// Looks up the identifier in `file_locals` and `lib_contexts` for symbols with
    /// TYPE, `REGULAR_ENUM`, or `CONST_ENUM` flags. Returns the raw symbol ID (u32).
    /// Skips unshadowed compiler-managed types handled specially by `TypeLowering`.
    pub(crate) fn resolve_type_symbol(&self, node_idx: NodeIndex) -> Option<u32> {
        use tsz_binder::symbol_flags;
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_solver::is_compiler_managed_type;

        let ident = self.ctx.arena.get_identifier_at(node_idx)?;
        let name = ident.escaped_text.as_str();

        if self.ctx.type_parameter_scope.contains_key(name) {
            return None;
        }

        if is_compiler_managed_type(name) && !self.ctx.file_local_type_shadow_for_lib_name(name) {
            return None;
        }

        let scoped_name = {
            let node = self.ctx.arena.get(node_idx)?;
            if node.kind != SyntaxKind::Identifier as u16 {
                None
            } else {
                let mut prefixes = Vec::new();
                let mut parent = self
                    .ctx
                    .arena
                    .get_extended(node_idx)
                    .map_or(NodeIndex::NONE, |info| info.parent);

                while parent.is_some() {
                    let parent_node = self.ctx.arena.get(parent)?;
                    if parent_node.kind == syntax_kind_ext::MODULE_DECLARATION
                        && let Some(module) = self.ctx.arena.get_module(parent_node)
                        && let Some(name_node) = self.ctx.arena.get(module.name)
                        && name_node.kind == SyntaxKind::Identifier as u16
                        && let Some(name_ident) = self.ctx.arena.get_identifier(name_node)
                    {
                        prefixes.push(name_ident.escaped_text.clone());
                    }

                    parent = self
                        .ctx
                        .arena
                        .get_extended(parent)
                        .map_or(NodeIndex::NONE, |info| info.parent);
                }

                if prefixes.is_empty() {
                    None
                } else {
                    prefixes.reverse();
                    prefixes.push(name.to_string());
                    Some(prefixes.join("."))
                }
            }
        };

        let scoped_sym_id = scoped_name
            .as_deref()
            .and_then(|qualified| self.resolve_entity_name_text_symbol(qualified));

        // Prefer lexical scope resolution so local type parameters shadow outer
        // file-level aliases/types with the same name.
        if let Some(sym_id) = self.ctx.binder.resolve_identifier(self.ctx.arena, node_idx) {
            let symbol = self.ctx.binder.get_symbol(sym_id)?;
            if symbol.escaped_name != name {
                // NodeIndex values are arena-local. During cross-file type-node
                // lowering, a raw node id can accidentally find an unrelated
                // symbol in the current binder; ignore that collision and fall
                // through to name-based file/lib lookup.
            } else {
                if let Some(target_sym_id) = self.resolve_import_alias_type_target_symbol(sym_id) {
                    return Some(target_sym_id.0);
                }
                if let Some(scoped_sym_id) = scoped_sym_id
                    && scoped_sym_id != sym_id
                    && let Some(scoped_symbol) = self.get_symbol_from_any_context(scoped_sym_id)
                    && scoped_symbol.has_any_flags(
                        symbol_flags::TYPE | symbol_flags::REGULAR_ENUM | symbol_flags::CONST_ENUM,
                    )
                    && scoped_symbol.has_any_flags(symbol_flags::TYPE_ALIAS)
                    && !symbol.has_any_flags(symbol_flags::TYPE_ALIAS)
                {
                    return Some(scoped_sym_id.0);
                }
                if symbol.has_any_flags(
                    symbol_flags::TYPE | symbol_flags::REGULAR_ENUM | symbol_flags::CONST_ENUM,
                ) {
                    return Some(sym_id.0);
                }
            }
        }

        if let Some(scoped_sym_id) = scoped_sym_id
            && let Some(scoped_symbol) = self.get_symbol_from_any_context(scoped_sym_id)
            && (scoped_symbol.flags
                & (symbol_flags::TYPE | symbol_flags::REGULAR_ENUM | symbol_flags::CONST_ENUM))
                != 0
        {
            self.ctx
                .register_symbol_file_target(scoped_sym_id, scoped_symbol.decl_file_idx as usize);
            return Some(scoped_sym_id.0);
        }

        if let Some(sym_id) = self.ctx.binder.file_locals.get(name) {
            let symbol = self.ctx.binder.get_symbol(sym_id)?;
            if let Some(target_sym_id) = self.resolve_import_alias_type_target_symbol(sym_id) {
                return Some(target_sym_id.0);
            }
            if symbol.escaped_name == name
                && (symbol.flags
                    & (symbol_flags::TYPE | symbol_flags::REGULAR_ENUM | symbol_flags::CONST_ENUM))
                    != 0
            {
                return Some(sym_id.0);
            }
        }

        for lib_ctx in self.ctx.lib_contexts.iter() {
            if let Some(lib_sym_id) = lib_ctx.binder.file_locals.get(name) {
                let symbol = lib_ctx.binder.get_symbol(lib_sym_id)?;
                if (symbol.flags
                    & (symbol_flags::TYPE | symbol_flags::REGULAR_ENUM | symbol_flags::CONST_ENUM))
                    != 0
                {
                    self.ctx
                        .register_symbol_file_target(lib_sym_id, symbol.decl_file_idx as usize);
                    return Some(lib_sym_id.0);
                }
            }
        }

        None
    }

    /// Resolve a value symbol from a node index (`file_locals` only).
    ///
    /// Looks for symbols with VALUE or ALIAS flags. Used by `type_reference` and
    /// `function_type` resolvers.
    pub(super) fn resolve_value_symbol(&self, node_idx: NodeIndex) -> Option<u32> {
        self.resolve_value_symbol_in_scope(node_idx)
            .map(|sym_id| sym_id.0)
    }

    pub(super) fn resolve_value_symbol_in_scope(
        &self,
        node_idx: NodeIndex,
    ) -> Option<tsz_binder::SymbolId> {
        use tsz_binder::symbol_flags;

        let ident = self.ctx.arena.get_identifier_at(node_idx)?;
        let name = ident.escaped_text.as_str();

        if let Some(sym_id) = self.ctx.binder.resolve_identifier(self.ctx.arena, node_idx)
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            && symbol.escaped_name == name
            && (symbol.flags & (symbol_flags::VALUE | symbol_flags::ALIAS)) != 0
        {
            return Some(sym_id);
        }

        if let Some(sym_id) = self.ctx.binder.file_locals.get(name) {
            let symbol = self.ctx.binder.get_symbol(sym_id)?;
            if (symbol.flags & (symbol_flags::VALUE | symbol_flags::ALIAS)) != 0 {
                return Some(sym_id);
            }
        }

        None
    }

    pub(super) fn declared_type_annotation_for_value_symbol(
        &self,
        sym_id: tsz_binder::SymbolId,
    ) -> Option<NodeIndex> {
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let mut decl = symbol.value_declaration;
        if decl.is_none() {
            decl = symbol.primary_declaration()?;
        }
        let decl_node = self.ctx.arena.get(decl)?;
        if decl_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            let var_decl = self.ctx.arena.get_variable_declaration(decl_node)?;
            return var_decl
                .type_annotation
                .is_some()
                .then_some(var_decl.type_annotation);
        }
        if decl_node.kind == syntax_kind_ext::PARAMETER {
            let param = self.ctx.arena.get_parameter(decl_node)?;
            return param
                .type_annotation
                .is_some()
                .then_some(param.type_annotation);
        }
        if decl_node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
            let parent = self.ctx.arena.get_extended(decl)?.parent;
            let parent_node = self.ctx.arena.get(parent)?;
            if parent_node.kind == syntax_kind_ext::PARAMETER {
                let param = self.ctx.arena.get_parameter(parent_node)?;
                return (param.name == decl && param.type_annotation.is_some())
                    .then_some(param.type_annotation);
            }
            if parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
                let var_decl = self.ctx.arena.get_variable_declaration(parent_node)?;
                return (var_decl.name == decl && var_decl.type_annotation.is_some())
                    .then_some(var_decl.type_annotation);
            }
        }
        None
    }

    pub(super) fn is_direct_typeof_annotation_for_symbol(
        &self,
        annotation_idx: NodeIndex,
        sym_id: tsz_binder::SymbolId,
    ) -> bool {
        let Some(annotation_node) = self.ctx.arena.get(annotation_idx) else {
            return false;
        };
        if annotation_node.kind != syntax_kind_ext::TYPE_QUERY {
            return false;
        }
        let Some(type_query) = self.ctx.arena.get_type_query(annotation_node) else {
            return false;
        };
        self.ctx
            .binder
            .get_node_symbol(type_query.expr_name)
            .or_else(|| {
                self.ctx
                    .binder
                    .resolve_identifier(self.ctx.arena, type_query.expr_name)
            })
            == Some(sym_id)
    }

    /// Resolve a value symbol from a node index (`file_locals` + libs, with enum flags).
    ///
    /// Extended variant used by `compute_type` fallback and `mapped_type` resolvers
    /// that also checks `lib_contexts` and includes `REGULAR_ENUM/CONST_ENUM` flags.
    pub(crate) fn resolve_value_symbol_with_libs(&self, node_idx: NodeIndex) -> Option<u32> {
        use tsz_binder::symbol_flags;

        let ident = self.ctx.arena.get_identifier_at(node_idx)?;
        let name = ident.escaped_text.as_str();

        if let Some(sym_id) = self.ctx.binder.file_locals.get(name)
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            && (symbol.flags
                & (symbol_flags::VALUE
                    | symbol_flags::ALIAS
                    | symbol_flags::REGULAR_ENUM
                    | symbol_flags::CONST_ENUM))
                != 0
        {
            return Some(sym_id.0);
        }

        for lib_ctx in self.ctx.lib_contexts.iter() {
            if let Some(lib_sym_id) = lib_ctx.binder.file_locals.get(name)
                && let Some(symbol) = lib_ctx.binder.get_symbol(lib_sym_id)
                && (symbol.flags
                    & (symbol_flags::VALUE
                        | symbol_flags::ALIAS
                        | symbol_flags::REGULAR_ENUM
                        | symbol_flags::CONST_ENUM))
                    != 0
            {
                self.ctx
                    .register_symbol_file_target(lib_sym_id, symbol.decl_file_idx as usize);
                return Some(lib_sym_id.0);
            }
        }

        None
    }

    /// Extract parameter information from a signature.
    fn extract_params_from_signature(
        &mut self,
        sig: &tsz_parser::parser::node::SignatureData,
    ) -> (Vec<tsz_solver::ParamInfo>, Option<TypeId>) {
        use tsz_solver::ParamInfo;

        let mut params: Vec<ParamInfo> = Vec::new();
        let mut this_type = None;

        if let Some(ref param_list) = sig.parameters {
            for &param_idx in &param_list.nodes {
                let Some(param_node) = self.ctx.arena.get(param_idx) else {
                    continue;
                };
                let Some(param_data) = self.ctx.arena.get_parameter(param_node) else {
                    continue;
                };

                // Get parameter name
                let name = self.get_param_name(param_data.name);

                // Check for 'this' parameter
                if name == "this" {
                    this_type = (param_data.type_annotation.is_some())
                        .then(|| self.check(param_data.type_annotation));
                    continue;
                }

                // Later parameter annotations can reference earlier value
                // parameters via `typeof`.
                for param in &params {
                    if let Some(name_atom) = param.name {
                        let name = self.ctx.types.resolve_atom(name_atom);
                        self.ctx.typeof_param_scope.insert(name, param.type_id);
                    }
                }
                let type_id = if param_data.type_annotation.is_some() {
                    self.check(param_data.type_annotation)
                } else {
                    TypeId::ANY
                };
                for param in &params {
                    if let Some(name_atom) = param.name {
                        let name = self.ctx.types.resolve_atom(name_atom);
                        self.ctx.typeof_param_scope.remove(&name);
                    }
                }

                let optional = param_data.question_token || param_data.initializer.is_some();
                let rest = param_data.dot_dot_dot_token;

                let sig_type_id = if param_data.question_token
                    && type_id != TypeId::ANY
                    && type_id != TypeId::UNKNOWN
                    && type_id != TypeId::ERROR
                    && !crate::query_boundaries::common::type_contains_undefined(
                        self.ctx.types,
                        type_id,
                    ) {
                    self.ctx.types.factory().union2(type_id, TypeId::UNDEFINED)
                } else {
                    type_id
                };
                params.push(ParamInfo {
                    name: Some(self.ctx.types.intern_string(&name)),
                    type_id: sig_type_id,
                    optional,
                    rest,
                });
            }
        }

        (params, this_type)
    }

    /// Resolve return type annotation with parameter names in scope for `typeof`.
    ///
    /// Pushes parameter names into `typeof_param_scope` so that `typeof paramName`
    /// in the return type annotation resolves to the parameter's declared type.
    fn resolve_return_type_with_params_in_scope(
        &mut self,
        type_annotation: NodeIndex,
        params: &[tsz_solver::ParamInfo],
    ) -> TypeId {
        if type_annotation.is_none() {
            return TypeId::ANY;
        }

        // Push param names into typeof_param_scope
        for param in params {
            if let Some(name_atom) = param.name {
                let name = self.ctx.types.resolve_atom(name_atom);
                self.ctx.typeof_param_scope.insert(name, param.type_id);
            }
        }

        let return_type = self.check(type_annotation);

        // Clear typeof_param_scope
        for param in params {
            if let Some(name_atom) = param.name {
                let name = self.ctx.types.resolve_atom(name_atom);
                self.ctx.typeof_param_scope.remove(&name);
            }
        }

        return_type
    }

    /// Get parameter name from a binding name node.
    fn get_param_name(&self, name_idx: NodeIndex) -> String {
        if self
            .ctx
            .arena
            .get(name_idx)
            .is_some_and(|node| node.kind == SyntaxKind::ThisKeyword as u16)
        {
            return "this".to_string();
        }
        if let Some(ident) = self.ctx.arena.get_identifier_at(name_idx) {
            return ident.escaped_text.to_string();
        }
        "_".to_string()
    }
}
#[cfg(test)]
#[path = "../../tests/type_node.rs"]
mod tests;
