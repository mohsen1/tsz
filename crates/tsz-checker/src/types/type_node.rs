//! Type Node Checking
//!
//! This module handles type resolution from AST type nodes (type annotations,
//! type references, union types, intersection types, etc.).
//!
//! It follows the "Check Fast, Explain Slow" pattern where we first
//! resolve types, then use the solver to explain any failures.

use super::type_node_helpers::{
    check_duplicate_parameters_in_type, check_parameter_initializers_in_type,
};
use crate::context::CheckerContext;
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;
use tsz_solver::Visibility;
use tsz_solver::recursion::{DepthCounter, RecursionProfile};

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
        use tsz_scanner::SyntaxKind;

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };

        match node.kind {
            // Keyword types - use compile-time constant TypeIds
            k if k == SyntaxKind::NumberKeyword as u16 => TypeId::NUMBER,
            k if k == SyntaxKind::StringKeyword as u16 => TypeId::STRING,
            k if k == SyntaxKind::BooleanKeyword as u16 => TypeId::BOOLEAN,
            k if k == SyntaxKind::VoidKeyword as u16 => TypeId::VOID,
            k if k == SyntaxKind::AnyKeyword as u16 => TypeId::ANY,
            k if k == SyntaxKind::NeverKeyword as u16 => TypeId::NEVER,
            k if k == SyntaxKind::UnknownKeyword as u16 => TypeId::UNKNOWN,
            k if k == SyntaxKind::UndefinedKeyword as u16 => TypeId::UNDEFINED,
            k if k == SyntaxKind::NullKeyword as u16 => TypeId::NULL,
            k if k == SyntaxKind::ObjectKeyword as u16 => TypeId::OBJECT,
            k if k == SyntaxKind::BigIntKeyword as u16 => TypeId::BIGINT,
            k if k == SyntaxKind::SymbolKeyword as u16 => TypeId::SYMBOL,

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

    // =========================================================================
    // Type Reference Resolution
    // =========================================================================

    /// Get type from a type reference node (e.g., "number", "string", "`MyType`").
    fn get_type_from_type_reference(&mut self, idx: NodeIndex) -> TypeId {
        self.lower_with_resolvers(idx, false, true)
    }

    // =========================================================================
    // Composite Type Resolution
    // =========================================================================

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
            return tsz_solver::utils::union_or_single_literal_reduce(self.ctx.types, member_types);
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

                // Check if this is an optional/rest type or a regular type
                use tsz_parser::parser::syntax_kind_ext;
                if elem_node.kind == syntax_kind_ext::OPTIONAL_TYPE {
                    // Optional element (e.g., `string?`)
                    // TS1266: An optional element cannot follow a rest element
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
                        let elem_type = self.check(wrapped.type_node);
                        elements.push(TupleElement {
                            type_id: elem_type,
                            name: None,
                            optional: true,
                            rest: false,
                        });
                    }
                } else if elem_node.kind == syntax_kind_ext::REST_TYPE {
                    // Rest element (e.g., `...string[]` or `...T`)
                    if let Some(wrapped) = self.ctx.arena.get_wrapped_type(elem_node) {
                        let elem_type = self.check(wrapped.type_node);
                        // Only track seen_rest for concrete array/tuple rest elements.
                        // Variadic type parameter spreads (...T) don't count as "rest"
                        // for TS1265/TS1266 purposes — they represent variadic tuples.
                        let is_concrete_rest = self.is_array_or_tuple_type(elem_type);
                        if is_concrete_rest {
                            // TS1265: A rest element cannot follow another rest element
                            if seen_rest {
                                self.ctx.error(
                                    elem_node.pos,
                                    elem_node.end - elem_node.pos,
                                    crate::diagnostics::diagnostic_messages::A_REST_ELEMENT_CANNOT_FOLLOW_ANOTHER_REST_ELEMENT.to_string(),
                                    crate::diagnostics::diagnostic_codes::A_REST_ELEMENT_CANNOT_FOLLOW_ANOTHER_REST_ELEMENT,
                                );
                            }
                            seen_rest = true;
                        }
                        elements.push(TupleElement {
                            type_id: elem_type,
                            name: None,
                            optional: false,
                            rest: true,
                        });
                    }
                } else if elem_node.kind == syntax_kind_ext::NAMED_TUPLE_MEMBER {
                    // Named tuple element (e.g., `[x: number, y?: string, ...rest: boolean[]]`)
                    if let Some(data) = self.ctx.arena.get_named_tuple_member(elem_node) {
                        let elem_type = self.check(data.type_node);
                        let name = self
                            .ctx
                            .arena
                            .get(data.name)
                            .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                            .map(|id_data| self.ctx.types.intern_string(&id_data.escaped_text));

                        if data.dot_dot_dot_token {
                            let is_concrete_rest = self.is_array_or_tuple_type(elem_type);
                            if is_concrete_rest {
                                // TS1265: A rest element cannot follow another rest element
                                if seen_rest {
                                    self.ctx.error(
                                        elem_node.pos,
                                        elem_node.end - elem_node.pos,
                                        crate::diagnostics::diagnostic_messages::A_REST_ELEMENT_CANNOT_FOLLOW_ANOTHER_REST_ELEMENT.to_string(),
                                        crate::diagnostics::diagnostic_codes::A_REST_ELEMENT_CANNOT_FOLLOW_ANOTHER_REST_ELEMENT,
                                    );
                                }
                                seen_rest = true;
                            }
                        } else if data.question_token {
                            // TS1266: An optional element cannot follow a rest element
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
                            optional: data.question_token,
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
                && self
                    .ctx
                    .binder
                    .get_symbol(sym_id)
                    .is_some_and(|symbol| symbol.flags & tsz_binder::symbol_flags::TYPE_ALIAS != 0)
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

    // =========================================================================
    // Function and Callable Types
    // =========================================================================

    /// Get type from a function type node (e.g., () => number, (x: string) => void).
    fn get_type_from_function_type(&mut self, idx: NodeIndex) -> TypeId {
        let Some(_node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };
        let Some(func_data) = self.ctx.arena.get_function_type(_node) else {
            return TypeId::ERROR;
        };

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

        // Delegate to TypeLowering with standard resolvers.
        // Enable qualified name resolution so return types like `Ns.Type<T>`
        // resolve correctly (QUALIFIED_NAME nodes need the extended resolver).
        let result = self.lower_with_resolvers(idx, false, true);

        // TS2677: Check that a type predicate's type is assignable to its parameter's type.
        self.check_type_predicate_assignability(idx, func_data.type_annotation, result);

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

        let predicate_type = self.check(pred_data.type_node);
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
                    param_type = (!param_data.type_annotation.is_none())
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
        let resolved_predicate =
            if crate::query_boundaries::common::is_type_parameter_like(self.ctx.types, predicate_type) {
                match crate::query_boundaries::common::type_param_info(self.ctx.types, predicate_type)
                    .and_then(|info| info.constraint)
                {
                    Some(_) => return, // Constrained type param: always assignable to its constraint
                    None => TypeId::UNKNOWN,
                }
            } else {
                predicate_type
            };
        let resolved_param =
            if crate::query_boundaries::common::is_type_parameter_like(self.ctx.types, param_type) {
                match crate::query_boundaries::common::type_param_info(self.ctx.types, param_type)
                    .and_then(|info| info.constraint)
                {
                    Some(c) => c,
                    None => TypeId::UNKNOWN,
                }
            } else {
                param_type
            };

        if !self
            .ctx
            .types
            .is_assignable_to(resolved_predicate, resolved_param)
        {
            if let Some(type_node) = self.ctx.arena.get(pred_data.type_node) {
                self.ctx.error(
                    type_node.pos,
                    type_node.end - type_node.pos,
                    "A type predicate's type must be assignable to its parameter's type."
                        .to_string(),
                    2677,
                );
            }
        }
    }

    /// Check if a type contains an Application that can't be evaluated (e.g., `NonNullable<T>`
    /// where the resolver doesn't know about the base type's definition). In such cases,
    /// the Application stays opaque and assignability checks may give incorrect results.
    fn predicate_type_contains_unevaluable_application(&self, type_id: TypeId) -> bool {
        if tsz_solver::type_queries::get_application_info(self.ctx.types, type_id).is_some() {
            // If evaluate_type returns the same TypeId, the Application couldn't be resolved
            let evaluated = self.ctx.types.evaluate_type(type_id);
            return evaluated == type_id;
        }
        if let Some(members) =
            tsz_solver::type_queries::get_intersection_members(self.ctx.types, type_id)
        {
            return members
                .iter()
                .any(|&m| self.predicate_type_contains_unevaluable_application(m));
        }
        if let Some(members) = crate::query_boundaries::common::union_members(self.ctx.types, type_id)
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

                        if member.kind == METHOD_SIGNATURE {
                            let (_type_params, type_param_updates) = self
                                .push_type_parameters_for_type_literal_signature(
                                    &sig.type_parameters,
                                );
                            let (params, this_type) = self.extract_params_from_signature(sig);
                            let return_type = self.resolve_return_type_with_params_in_scope(
                                sig.type_annotation,
                                &params,
                            );
                            let shape = FunctionShape {
                                type_params: Vec::new(),
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
                        let is_valid_index_type = key_type == TypeId::STRING
                            || key_type == TypeId::NUMBER
                            || key_type == TypeId::SYMBOL
                            || tsz_solver::visitor::is_template_literal_type(
                                self.ctx.types,
                                key_type,
                            );
                        if !is_valid_index_type && let Some(pnode) = self.ctx.arena.get(param_idx) {
                            self.ctx.error(
                                    pnode.pos,
                                    pnode.end - pnode.pos,
                                    "An index signature parameter type must be 'string', 'number', 'symbol', or a template literal type.".to_string(),
                                    1268,
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
                if key_type == TypeId::NUMBER {
                    number_index = Some(info);
                } else {
                    string_index = Some(info);
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

    // =========================================================================
    // Symbol Resolution Helpers
    // =========================================================================

    /// Resolve a type symbol from a node index.
    ///
    /// Looks up the identifier in `file_locals` and `lib_contexts` for symbols with
    /// TYPE, `REGULAR_ENUM`, or `CONST_ENUM` flags. Returns the raw symbol ID (u32).
    /// Skips compiler-managed types (Array, ReadonlyArray, etc.) that `TypeLowering`
    /// handles specially.
    pub(crate) fn resolve_type_symbol(&self, node_idx: NodeIndex) -> Option<u32> {
        use tsz_binder::symbol_flags;
        use tsz_solver::is_compiler_managed_type;

        let ident = self.ctx.arena.get_identifier_at(node_idx)?;
        let name = ident.escaped_text.as_str();

        if is_compiler_managed_type(name) {
            return None;
        }

        // Prefer lexical scope resolution so local type parameters shadow outer
        // file-level aliases/types with the same name.
        if let Some(sym_id) = self.ctx.binder.resolve_identifier(self.ctx.arena, node_idx) {
            let symbol = self.ctx.binder.get_symbol(sym_id)?;
            if (symbol.flags
                & (symbol_flags::TYPE | symbol_flags::REGULAR_ENUM | symbol_flags::CONST_ENUM))
                != 0
            {
                return Some(sym_id.0);
            }
        }

        if let Some(sym_id) = self.ctx.binder.file_locals.get(name) {
            let symbol = self.ctx.binder.get_symbol(sym_id)?;
            if (symbol.flags
                & (symbol_flags::TYPE | symbol_flags::REGULAR_ENUM | symbol_flags::CONST_ENUM))
                != 0
            {
                return Some(sym_id.0);
            }
        }

        for lib_ctx in &self.ctx.lib_contexts {
            if let Some(lib_sym_id) = lib_ctx.binder.file_locals.get(name) {
                let symbol = lib_ctx.binder.get_symbol(lib_sym_id)?;
                if (symbol.flags
                    & (symbol_flags::TYPE | symbol_flags::REGULAR_ENUM | symbol_flags::CONST_ENUM))
                    != 0
                {
                    let file_sym_id = self.ctx.binder.file_locals.get(name).unwrap_or(lib_sym_id);
                    return Some(file_sym_id.0);
                }
            }
        }

        None
    }

    /// Resolve a value symbol from a node index (`file_locals` only).
    ///
    /// Looks for symbols with VALUE or ALIAS flags. Used by `type_reference` and
    /// `function_type` resolvers.
    fn resolve_value_symbol(&self, node_idx: NodeIndex) -> Option<u32> {
        use tsz_binder::symbol_flags;

        let ident = self.ctx.arena.get_identifier_at(node_idx)?;
        let name = ident.escaped_text.as_str();

        if let Some(sym_id) = self.ctx.binder.file_locals.get(name) {
            let symbol = self.ctx.binder.get_symbol(sym_id)?;
            if (symbol.flags & (symbol_flags::VALUE | symbol_flags::ALIAS)) != 0 {
                return Some(sym_id.0);
            }
        }

        None
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

        for lib_ctx in &self.ctx.lib_contexts {
            if let Some(lib_sym_id) = lib_ctx.binder.file_locals.get(name)
                && let Some(symbol) = lib_ctx.binder.get_symbol(lib_sym_id)
                && (symbol.flags
                    & (symbol_flags::VALUE
                        | symbol_flags::ALIAS
                        | symbol_flags::REGULAR_ENUM
                        | symbol_flags::CONST_ENUM))
                    != 0
            {
                let file_sym_id = self.ctx.binder.file_locals.get(name).unwrap_or(lib_sym_id);
                return Some(file_sym_id.0);
            }
        }

        None
    }

    /// Resolve a DefId from a node index via the type resolver.
    ///
    /// Uses the stable-identity helper `ensure_def_id_with_alias` to mint
    /// the DefId and ensure type alias body+params are registered.
    fn resolve_def_id(&self, node_idx: NodeIndex) -> Option<tsz_solver::def::DefId> {
        let sym_id_raw = self.resolve_type_symbol(node_idx)?;
        let sym_id = tsz_binder::SymbolId(sym_id_raw);
        Some(self.ensure_def_id_with_alias(sym_id))
    }

    /// Collect type parameter bindings from the current scope.
    fn collect_type_param_bindings(&self) -> Vec<(tsz_common::interner::Atom, TypeId)> {
        self.ctx
            .type_parameter_scope
            .iter()
            .map(|(name, &type_id)| (self.ctx.types.intern_string(name), type_id))
            .collect()
    }

    /// Run `TypeLowering` with the standard resolvers (type + value + `def_id`).
    ///
    /// This is the common path used by `compute_type` fallback, `type_reference`,
    /// `function_type`, and `mapped_type`. The `use_extended_value_resolver` flag
    /// controls whether enum flags and lib search are included in value resolution.
    /// The `use_qualified_names` flag enables qualified name support in `def_id` resolution.
    pub(crate) fn lower_with_resolvers(
        &self,
        idx: NodeIndex,
        use_extended_value_resolver: bool,
        use_qualified_names: bool,
    ) -> TypeId {
        use tsz_lowering::TypeLowering;

        let type_param_bindings = self.collect_type_param_bindings();

        let type_resolver =
            |node_idx: NodeIndex| -> Option<u32> { self.resolve_type_symbol(node_idx) };

        let value_resolver = |node_idx: NodeIndex| -> Option<u32> {
            if use_extended_value_resolver {
                self.resolve_value_symbol_with_libs(node_idx)
            } else {
                self.resolve_value_symbol(node_idx)
            }
        };

        let def_id_resolver = |node_idx: NodeIndex| -> Option<tsz_solver::def::DefId> {
            if use_qualified_names {
                self.resolve_def_id_with_qualified_names(node_idx)
            } else {
                self.resolve_def_id(node_idx)
            }
        };

        let mut lowering = TypeLowering::with_hybrid_resolver(
            self.ctx.arena,
            self.ctx.types,
            &type_resolver,
            &def_id_resolver,
            &value_resolver,
        )
        .with_strict_null_checks(self.ctx.strict_null_checks());
        if !type_param_bindings.is_empty() {
            lowering = lowering.with_type_param_bindings(type_param_bindings);
        }
        lowering.lower_type(idx)
    }

    // =========================================================================
    // Helper Methods
    // =========================================================================

    /// Extract parameter information from a signature.
    fn extract_params_from_signature(
        &mut self,
        sig: &tsz_parser::parser::node::SignatureData,
    ) -> (Vec<tsz_solver::ParamInfo>, Option<TypeId>) {
        use tsz_solver::ParamInfo;

        let mut params = Vec::new();
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

                // Get parameter type
                let type_id = if param_data.type_annotation.is_some() {
                    self.check(param_data.type_annotation)
                } else {
                    TypeId::ANY
                };

                let optional = param_data.question_token || param_data.initializer.is_some();
                let rest = param_data.dot_dot_dot_token;

                // Store the declared base type in the signature, NOT `type | undefined`.
                // The `optional: true` flag conveys optionality.  Adding `| undefined`
                // to the stored type is redundant and prevents union subtype reduction
                // (e.g., `(x: string | undefined) => void | (x?: string) => void`
                // should reduce, but both params end up with the same TypeId when we
                // eagerly widen here, blocking the shallow subtype guard).
                // This matches tsc's behavior where the signature stores the declared
                // type and `| undefined` is added at the use site.
                params.push(ParamInfo {
                    name: Some(self.ctx.types.intern_string(&name)),
                    type_id,
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
        if let Some(ident) = self.ctx.arena.get_identifier_at(name_idx) {
            return ident.escaped_text.to_string();
        }
        "_".to_string()
    }

    /// Get property name from a property name node.
    fn get_property_name(&self, name_idx: NodeIndex) -> Option<String> {
        crate::types_domain::queries::core::get_literal_property_name(self.ctx.arena, name_idx)
    }

    /// Resolve a property name, including computed names backed by unique symbols.
    fn get_property_name_resolved(&self, name_idx: NodeIndex) -> Option<String> {
        let name_node = self.ctx.arena.get(name_idx)?;

        if let Some(name) = self.get_property_name(name_idx) {
            return Some(name);
        }

        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return None;
        }

        let computed = self.ctx.arena.get_computed_property(name_node)?;

        if let Some(symbol_name) = self.get_well_known_symbol_property_name(computed.expression) {
            return Some(symbol_name);
        }

        let sym_id = self.resolve_computed_property_symbol(computed.expression)?;
        self.symbol_refers_to_unique_symbol(sym_id)
            .then(|| format!("__unique_{}", sym_id.0))
    }

    fn get_well_known_symbol_property_name(&self, expr_idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(expr_idx)?;

        if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            let paren = self.ctx.arena.get_parenthesized(node)?;
            return self.get_well_known_symbol_property_name(paren.expression);
        }

        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            return None;
        }

        let access = self.ctx.arena.get_access_expr(node)?;
        let base_node = self.ctx.arena.get(access.expression)?;
        let base_ident = self.ctx.arena.get_identifier(base_node)?;
        if base_ident.escaped_text != "Symbol" {
            return None;
        }

        let name_node = self.ctx.arena.get(access.name_or_argument)?;
        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            return Some(format!("[Symbol.{}]", ident.escaped_text));
        }

        if matches!(
            name_node.kind,
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
        ) && let Some(lit) = self.ctx.arena.get_literal(name_node)
            && !lit.text.is_empty()
        {
            return Some(format!("[Symbol.{}]", lit.text));
        }

        None
    }

    fn resolve_computed_property_symbol(&self, expr_idx: NodeIndex) -> Option<SymbolId> {
        let node = self.ctx.arena.get(expr_idx)?;

        if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            let paren = self.ctx.arena.get_parenthesized(node)?;
            return self.resolve_computed_property_symbol(paren.expression);
        }

        if node.kind == SyntaxKind::Identifier as u16 {
            return self.resolve_value_symbol_with_libs(expr_idx).map(SymbolId);
        }

        let qualified = self.expression_name_text(expr_idx)?;
        self.resolve_entity_name_text_symbol(&qualified)
    }

    fn expression_name_text(&self, idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(idx)?;

        if node.kind == SyntaxKind::Identifier as u16 {
            return self
                .ctx
                .arena
                .get_identifier(node)
                .map(|ident| ident.escaped_text.clone());
        }

        if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            return self.entity_name_text(idx);
        }

        if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            let paren = self.ctx.arena.get_parenthesized(node)?;
            return self.expression_name_text(paren.expression);
        }

        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(access) = self.ctx.arena.get_access_expr(node)
        {
            let left = self.expression_name_text(access.expression)?;
            let right_node = self.ctx.arena.get(access.name_or_argument)?;
            let right = self.ctx.arena.get_identifier(right_node)?;
            return Some(format!("{left}.{}", right.escaped_text));
        }

        None
    }

    fn symbol_refers_to_unique_symbol(&self, sym_id: SymbolId) -> bool {
        let lib_binders: Vec<_> = self
            .ctx
            .lib_contexts
            .iter()
            .map(|ctx| std::sync::Arc::clone(&ctx.binder))
            .collect();
        let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders) else {
            return false;
        };

        let mut decl_candidates = symbol.declarations.clone();
        if symbol.value_declaration.is_some()
            && !decl_candidates.contains(&symbol.value_declaration)
        {
            decl_candidates.push(symbol.value_declaration);
        }

        decl_candidates
            .into_iter()
            .any(|decl_idx| self.declaration_is_unique_symbol(sym_id, decl_idx))
    }

    fn declaration_is_unique_symbol(&self, sym_id: SymbolId, decl_idx: NodeIndex) -> bool {
        let mut candidate_arenas: Vec<&tsz_parser::parser::node::NodeArena> = Vec::new();
        if let Some(arenas) = self.ctx.binder.declaration_arenas.get(&(sym_id, decl_idx)) {
            candidate_arenas.extend(arenas.iter().map(std::convert::AsRef::as_ref));
        }
        if let Some(symbol_arena) = self.ctx.binder.symbol_arenas.get(&sym_id) {
            candidate_arenas.push(symbol_arena.as_ref());
        }
        candidate_arenas.push(self.ctx.arena);

        candidate_arenas.into_iter().any(|arena| {
            let Some(node) = arena.get(decl_idx) else {
                return false;
            };
            if node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
                return false;
            }

            let Some(var_decl) = arena.get_variable_declaration(node) else {
                return false;
            };

            (var_decl.type_annotation.is_some()
                && self.is_unique_symbol_type_annotation_in_arena(arena, var_decl.type_annotation))
                || self.is_symbol_call_initializer_in_arena(arena, var_decl.initializer)
        })
    }

    fn is_unique_symbol_type_annotation_in_arena(
        &self,
        arena: &tsz_parser::parser::node::NodeArena,
        type_annotation: NodeIndex,
    ) -> bool {
        let Some(type_node) = arena.get(type_annotation) else {
            return false;
        };

        match type_node.kind {
            k if k == syntax_kind_ext::TYPE_OPERATOR => {
                arena.get_type_operator(type_node).is_some_and(|op| {
                    op.operator == SyntaxKind::UniqueKeyword as u16
                        && self.is_symbol_type_node_in_arena(arena, op.type_node)
                })
            }
            _ => false,
        }
    }

    fn is_symbol_type_node_in_arena(
        &self,
        arena: &tsz_parser::parser::node::NodeArena,
        type_annotation: NodeIndex,
    ) -> bool {
        let Some(type_node) = arena.get(type_annotation) else {
            return false;
        };
        if type_node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return false;
        }

        let Some(type_ref) = arena.get_type_ref(type_node) else {
            return false;
        };

        let Some(name_node) = arena.get(type_ref.type_name) else {
            return false;
        };

        arena
            .get_identifier(name_node)
            .is_some_and(|ident| ident.escaped_text == "symbol")
    }

    fn is_symbol_call_initializer_in_arena(
        &self,
        arena: &tsz_parser::parser::node::NodeArena,
        init_idx: NodeIndex,
    ) -> bool {
        let Some(node) = arena.get(init_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return false;
        }

        let Some(call) = arena.get_call_expr(node) else {
            return false;
        };
        let Some(expr_node) = arena.get(call.expression) else {
            return false;
        };

        arena
            .get_identifier(expr_node)
            .is_some_and(|ident| ident.escaped_text == "Symbol")
    }

    /// Get the context reference (for read-only access).
    pub const fn context(&self) -> &CheckerContext<'ctx> {
        self.ctx
    }
}

#[cfg(test)]
#[path = "../../tests/type_node.rs"]
mod tests;
