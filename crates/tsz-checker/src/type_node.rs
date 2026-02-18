//! Type Node Checking
//!
//! This module handles type resolution from AST type nodes (type annotations,
//! type references, union types, intersection types, etc.).
//!
//! It follows the "Check Fast, Explain Slow" pattern where we first
//! resolve types, then use the solver to explain any failures.

use super::context::CheckerContext;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
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
        self.ctx.node_types.insert(idx.0, result);

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
            k if k == syntax_kind_ext::FUNCTION_TYPE => self.get_type_from_function_type(idx),

            // Constructor type (e.g., new () => number, new (x: string) => any)
            k if k == syntax_kind_ext::CONSTRUCTOR_TYPE => self.get_type_from_function_type(idx),

            // Type literal ({ a: number; b(): string; })
            k if k == syntax_kind_ext::TYPE_LITERAL => self.get_type_from_type_literal(idx),

            // Type query (typeof X) - returns the type of X
            k if k == syntax_kind_ext::TYPE_QUERY => self.get_type_from_type_query(idx),

            // Mapped type ({ [P in K]: T })
            // Check for TS7039 before TypeLowering since TypeLowering doesn't emit diagnostics
            k if k == syntax_kind_ext::MAPPED_TYPE => self.get_type_from_mapped_type(idx),

            // Fall back to TypeLowering for type nodes not handled above
            // (conditional types, indexed access types, etc.)
            _ => {
                use tsz_binder::symbol_flags;
                use tsz_lowering::TypeLowering;
                use tsz_parser::parser::syntax_kind_ext;
                use tsz_solver::is_compiler_managed_type;

                let type_param_bindings: Vec<(tsz_common::interner::Atom, TypeId)> = self
                    .ctx
                    .type_parameter_scope
                    .iter()
                    .map(|(name, &type_id)| (self.ctx.types.intern_string(name), type_id))
                    .collect();

                // Create proper type/value resolvers that look up symbols in the binder
                // This is needed for mapped types, conditional types, and other complex types
                let type_resolver = |node_idx: NodeIndex| -> Option<u32> {
                    let ident = self.ctx.arena.get_identifier_at(node_idx)?;
                    let name = ident.escaped_text.as_str();

                    // Skip built-in types that have special handling in TypeLowering
                    if is_compiler_managed_type(name) {
                        return None;
                    }

                    // Look up the symbol in file_locals
                    if let Some(sym_id) = self.ctx.binder.file_locals.get(name) {
                        let symbol = self.ctx.binder.get_symbol(sym_id)?;
                        if (symbol.flags
                            & (symbol_flags::TYPE
                                | symbol_flags::REGULAR_ENUM
                                | symbol_flags::CONST_ENUM))
                            != 0
                        {
                            return Some(sym_id.0);
                        }
                    }

                    // Also check lib_contexts if available
                    for lib_ctx in &self.ctx.lib_contexts {
                        if let Some(lib_sym_id) = lib_ctx.binder.file_locals.get(name) {
                            let symbol = lib_ctx.binder.get_symbol(lib_sym_id)?;
                            if (symbol.flags
                                & (symbol_flags::TYPE
                                    | symbol_flags::REGULAR_ENUM
                                    | symbol_flags::CONST_ENUM))
                                != 0
                            {
                                // Use file binder's sym_id for correct ID space after lib merge
                                let file_sym_id =
                                    self.ctx.binder.file_locals.get(name).unwrap_or(lib_sym_id);
                                return Some(file_sym_id.0);
                            }
                        }
                    }

                    None
                };

                let value_resolver = |node_idx: NodeIndex| -> Option<u32> {
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
                            // Use file binder's sym_id for correct ID space after lib merge
                            let file_sym_id =
                                self.ctx.binder.file_locals.get(name).unwrap_or(lib_sym_id);
                            return Some(file_sym_id.0);
                        }
                    }

                    None
                };

                // Create def_id_resolver to prefer Lazy(DefId) over Ref(SymbolRef)
                let def_id_resolver = |node_idx: NodeIndex| -> Option<tsz_solver::def::DefId> {
                    // Try identifier resolution first (existing path)
                    if let Some(sym_id) = type_resolver(node_idx) {
                        return Some(self.ctx.get_or_create_def_id(tsz_binder::SymbolId(sym_id)));
                    }

                    // Handle qualified names (e.g., AnimalType.cat in template literal types).
                    // When a qualified name appears inside `${...}` in a template literal type,
                    // we need to resolve the left side to a symbol and look up the right member.
                    let node = self.ctx.arena.get(node_idx)?;
                    if node.kind == syntax_kind_ext::QUALIFIED_NAME {
                        let qn = self.ctx.arena.get_qualified_name(node)?;

                        // Resolve the left identifier to a symbol
                        let left_sym_raw = type_resolver(qn.left)?;
                        let left_sym_id = tsz_binder::SymbolId(left_sym_raw);
                        let left_symbol = self.ctx.binder.get_symbol(left_sym_id)?;

                        // Get the right identifier name
                        let right_node = self.ctx.arena.get(qn.right)?;
                        let right_ident = self.ctx.arena.get_identifier(right_node)?;
                        let right_name = right_ident.escaped_text.as_str();

                        // Look up the member in exports (handles enum members, namespace exports)
                        let member_sym_id = left_symbol.exports.as_ref()?.get(right_name)?;
                        return Some(self.ctx.get_or_create_def_id(member_sym_id));
                    }

                    None
                };

                let mut lowering = TypeLowering::with_hybrid_resolver(
                    self.ctx.arena,
                    self.ctx.types,
                    &type_resolver,
                    &def_id_resolver,
                    &value_resolver,
                );
                if !type_param_bindings.is_empty() {
                    lowering = lowering.with_type_param_bindings(type_param_bindings);
                }
                lowering.lower_type(idx)
            }
        }
    }

    // =========================================================================
    // Type Reference Resolution
    // =========================================================================

    /// Get type from a type reference node (e.g., "number", "string", "`MyType`").
    fn get_type_from_type_reference(&mut self, idx: NodeIndex) -> TypeId {
        use tsz_binder::symbol_flags;
        use tsz_lowering::TypeLowering;
        use tsz_solver::is_compiler_managed_type;

        // Create a type resolver that looks up symbols in the binder
        let type_resolver = |node_idx: NodeIndex| -> Option<u32> {
            let ident = self.ctx.arena.get_identifier_at(node_idx)?;
            let name = ident.escaped_text.as_str();

            // Skip built-in types that have special handling in TypeLowering
            if is_compiler_managed_type(name) {
                return None;
            }

            // Look up the symbol in file_locals
            if let Some(sym_id) = self.ctx.binder.file_locals.get(name) {
                let symbol = self.ctx.binder.get_symbol(sym_id)?;
                // Check for TYPE flag or ENUM flag (enums can be used as types)
                if (symbol.flags
                    & (symbol_flags::TYPE | symbol_flags::REGULAR_ENUM | symbol_flags::CONST_ENUM))
                    != 0
                {
                    return Some(sym_id.0);
                }
            }

            // Also check lib_contexts if available
            for lib_ctx in &self.ctx.lib_contexts {
                if let Some(lib_sym_id) = lib_ctx.binder.file_locals.get(name) {
                    let symbol = lib_ctx.binder.get_symbol(lib_sym_id)?;
                    // Check for TYPE flag or ENUM flag (enums can be used as types)
                    if (symbol.flags
                        & (symbol_flags::TYPE
                            | symbol_flags::REGULAR_ENUM
                            | symbol_flags::CONST_ENUM))
                        != 0
                    {
                        // Use file binder's sym_id for correct ID space after lib merge
                        let file_sym_id =
                            self.ctx.binder.file_locals.get(name).unwrap_or(lib_sym_id);
                        return Some(file_sym_id.0);
                    }
                }
            }

            None
        };

        let value_resolver = |node_idx: NodeIndex| -> Option<u32> {
            let ident = self.ctx.arena.get_identifier_at(node_idx)?;
            let name = ident.escaped_text.as_str();

            // Look up the symbol in file_locals
            if let Some(sym_id) = self.ctx.binder.file_locals.get(name) {
                let symbol = self.ctx.binder.get_symbol(sym_id)?;
                if (symbol.flags & (symbol_flags::VALUE | symbol_flags::ALIAS)) != 0 {
                    return Some(sym_id.0);
                }
            }

            None
        };

        // Get type parameter bindings from the context
        let type_param_bindings: Vec<(tsz_common::interner::Atom, TypeId)> = self
            .ctx
            .type_parameter_scope
            .iter()
            .map(|(name, &type_id)| (self.ctx.types.intern_string(name), type_id))
            .collect();

        // Create a def_id_resolver that converts symbol IDs to DefIds
        // This is needed for enums and other types that use DefId-based identity
        let def_id_resolver = |node_idx: NodeIndex| -> Option<tsz_solver::def::DefId> {
            let sym_id = type_resolver(node_idx)?;
            Some(self.ctx.get_or_create_def_id(tsz_binder::SymbolId(sym_id)))
        };

        let mut lowering = TypeLowering::with_hybrid_resolver(
            self.ctx.arena,
            self.ctx.types,
            &type_resolver,
            &def_id_resolver,
            &value_resolver,
        );
        if !type_param_bindings.is_empty() {
            lowering = lowering.with_type_param_bindings(type_param_bindings);
        }

        lowering.lower_type(idx)
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
        let factory = self.ctx.types.factory();

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
            if member_types.len() == 1 {
                return member_types[0];
            }

            return factory.union(member_types);
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
        let factory = self.ctx.types.factory();

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
            if member_types.len() == 1 {
                return member_types[0];
            }

            return factory.intersection(member_types);
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

        if let Some(tuple_type) = self.ctx.arena.get_tuple_type(node) {
            let mut elements = Vec::new();

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
                    // Rest element (e.g., `...string[]`)
                    if let Some(wrapped) = self.ctx.arena.get_wrapped_type(elem_node) {
                        let elem_type = self.check(wrapped.type_node);
                        elements.push(TupleElement {
                            type_id: elem_type,
                            name: None,
                            optional: false,
                            rest: true,
                        });
                    }
                } else {
                    // Regular element
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

    // =========================================================================
    // Type Operators
    // =========================================================================

    /// Get type from a type operator node (readonly T[], readonly [T, U], unique symbol).
    ///
    /// Handles type modifiers like:
    /// - `readonly T[]` - Creates `ReadonlyType` wrapper
    /// - `unique symbol` - Special marker for unique symbols
    fn get_type_from_type_operator(&mut self, idx: NodeIndex) -> TypeId {
        use tsz_scanner::SyntaxKind;
        let factory = self.ctx.types.factory();

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };

        if let Some(type_op) = self.ctx.arena.get_type_operator(node) {
            let operator = type_op.operator;
            let inner_type = self.check(type_op.type_node);

            // Handle readonly operator
            if operator == SyntaxKind::ReadonlyKeyword as u16 {
                return factory.readonly_type(inner_type);
            }

            // Handle keyof operator
            if operator == SyntaxKind::KeyOfKeyword as u16 {
                return factory.keyof(inner_type);
            }

            // Handle unique operator
            if operator == SyntaxKind::UniqueKeyword as u16 {
                // unique is handled differently - it's a type modifier for symbols
                // For now, just return the inner type
                return inner_type;
            }

            // Unknown operator - return inner type
            inner_type
        } else {
            TypeId::ERROR
        }
    }

    // =========================================================================
    // Indexed Access Types
    // =========================================================================

    /// Handle indexed access type nodes (e.g., `Person["name"]`, `T[K]`).
    fn get_type_from_indexed_access_type(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };
        let factory = self.ctx.types.factory();

        if let Some(indexed_access) = self.ctx.arena.get_indexed_access_type(node) {
            let object_type = self.check(indexed_access.object_type);
            let index_type = self.check(indexed_access.index_type);

            // TS2538: Check if the index type is valid (string, number, symbol, or literal thereof)
            if self.is_invalid_index_type(index_type)
                && let Some(inode) = self.ctx.arena.get(indexed_access.index_type)
            {
                self.ctx.error(
                    inode.pos,
                    inode.end - inode.pos,
                    "Type cannot be used as an index type.".to_string(),
                    2538,
                );
            }

            factory.index_access(object_type, index_type)
        } else {
            TypeId::ERROR
        }
    }

    /// Check if a type cannot be used as an index type (TS2538).
    fn is_invalid_index_type(&self, type_id: TypeId) -> bool {
        tsz_solver::type_queries::is_invalid_index_type(self.ctx.types, type_id)
    }

    // =========================================================================
    // Function and Callable Types
    // =========================================================================

    /// Get type from a function type node (e.g., () => number, (x: string) => void).
    fn get_type_from_function_type(&mut self, idx: NodeIndex) -> TypeId {
        use tsz_binder::symbol_flags;
        use tsz_lowering::TypeLowering;
        use tsz_solver::is_compiler_managed_type;

        let Some(_node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };
        let Some(func_data) = self.ctx.arena.get_function_type(_node) else {
            return TypeId::ERROR;
        };

        // EXPLICIT VALIDATION: Check type references in parameters and return type for TS2304.
        // We must do this before TypeLowering because TypeLowering doesn't emit diagnostics.
        // This ensures errors like "Cannot find name 'C'" are emitted for: (x: T) => C
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
                    && !binding.property_name.is_none()
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
        if !func_data.type_annotation.is_none()
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
                && !param_data.type_annotation.is_none()
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

        // Now emit all the TS2304 errors
        for (error_idx, name) in undefined_types {
            if renamed_binding_aliases.contains(&name) {
                continue;
            }
            if let Some(node) = self.ctx.arena.get(error_idx) {
                let message = format!("Cannot find name '{name}'.");
                self.ctx.error(node.pos, node.end - node.pos, message, 2304);
            }
        }

        // Create a type resolver that looks up symbols in the binder
        let type_resolver = |node_idx: NodeIndex| -> Option<u32> {
            let ident = self.ctx.arena.get_identifier_at(node_idx)?;
            let name = ident.escaped_text.as_str();

            // Skip built-in types that have special handling in TypeLowering
            if is_compiler_managed_type(name) {
                return None;
            }

            // Look up the symbol in file_locals
            if let Some(sym_id) = self.ctx.binder.file_locals.get(name) {
                let symbol = self.ctx.binder.get_symbol(sym_id)?;
                // Check for TYPE flag or ENUM flag (enums can be used as types)
                if (symbol.flags
                    & (symbol_flags::TYPE | symbol_flags::REGULAR_ENUM | symbol_flags::CONST_ENUM))
                    != 0
                {
                    return Some(sym_id.0);
                }
            }

            // Also check lib_contexts if available
            for lib_ctx in &self.ctx.lib_contexts {
                if let Some(lib_sym_id) = lib_ctx.binder.file_locals.get(name) {
                    let symbol = lib_ctx.binder.get_symbol(lib_sym_id)?;
                    // Check for TYPE flag or ENUM flag (enums can be used as types)
                    if (symbol.flags
                        & (symbol_flags::TYPE
                            | symbol_flags::REGULAR_ENUM
                            | symbol_flags::CONST_ENUM))
                        != 0
                    {
                        // Use file binder's sym_id for correct ID space after lib merge
                        let file_sym_id =
                            self.ctx.binder.file_locals.get(name).unwrap_or(lib_sym_id);
                        return Some(file_sym_id.0);
                    }
                }
            }

            None
        };

        let value_resolver = |node_idx: NodeIndex| -> Option<u32> {
            let ident = self.ctx.arena.get_identifier_at(node_idx)?;
            let name = ident.escaped_text.as_str();

            // Look up the symbol in file_locals
            if let Some(sym_id) = self.ctx.binder.file_locals.get(name) {
                let symbol = self.ctx.binder.get_symbol(sym_id)?;
                if (symbol.flags & (symbol_flags::VALUE | symbol_flags::ALIAS)) != 0 {
                    return Some(sym_id.0);
                }
            }

            None
        };

        // DefId resolver: converts binder SymbolIds to solver DefIds so that
        // TypeLowering can create Lazy(DefId) for user-defined type references
        // (e.g., type aliases like `Values`).  Without this, TypeLowering only
        // has the SymbolId-based `type_resolver` which is used as a guard but
        // not for actual type creation, causing user types to resolve as ERROR.
        let def_id_resolver = |node_idx: NodeIndex| -> Option<tsz_solver::def::DefId> {
            let ident = self.ctx.arena.get_identifier_at(node_idx)?;
            let name = ident.escaped_text.as_str();
            if is_compiler_managed_type(name) {
                return None;
            }
            if let Some(sym_id) = self.ctx.binder.file_locals.get(name) {
                let symbol = self.ctx.binder.get_symbol(sym_id)?;
                if (symbol.flags
                    & (symbol_flags::TYPE | symbol_flags::REGULAR_ENUM | symbol_flags::CONST_ENUM))
                    != 0
                {
                    return Some(self.ctx.get_or_create_def_id(sym_id));
                }
            }
            for lib_ctx in &self.ctx.lib_contexts {
                if let Some(lib_sym_id) = lib_ctx.binder.file_locals.get(name) {
                    let symbol = lib_ctx.binder.get_symbol(lib_sym_id)?;
                    if (symbol.flags
                        & (symbol_flags::TYPE
                            | symbol_flags::REGULAR_ENUM
                            | symbol_flags::CONST_ENUM))
                        != 0
                    {
                        let file_sym_id =
                            self.ctx.binder.file_locals.get(name).unwrap_or(lib_sym_id);
                        return Some(self.ctx.get_or_create_def_id(file_sym_id));
                    }
                }
            }
            None
        };

        // Get type parameter bindings from the context
        let type_param_bindings: Vec<(tsz_common::interner::Atom, TypeId)> = self
            .ctx
            .type_parameter_scope
            .iter()
            .map(|(name, &type_id)| (self.ctx.types.intern_string(name), type_id))
            .collect();

        let mut lowering = TypeLowering::with_hybrid_resolver(
            self.ctx.arena,
            self.ctx.types,
            &type_resolver,
            &def_id_resolver,
            &value_resolver,
        );
        if !type_param_bindings.is_empty() {
            lowering = lowering.with_type_param_bindings(type_param_bindings);
        }

        lowering.lower_type(idx)
    }

    /// Get type from a type literal node ({ a: number; `b()`: string; }).
    fn get_type_from_type_literal(&mut self, idx: NodeIndex) -> TypeId {
        use tsz_parser::parser::syntax_kind_ext::{
            CALL_SIGNATURE, CONSTRUCT_SIGNATURE, METHOD_SIGNATURE, PROPERTY_SIGNATURE,
        };
        use tsz_solver::{
            CallSignature, CallableShape, FunctionShape, IndexSignature, ObjectFlags, ObjectShape,
            PropertyInfo,
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
                        let (params, this_type) = self.extract_params_from_signature(sig);
                        let return_type = if !sig.type_annotation.is_none() {
                            self.check(sig.type_annotation)
                        } else {
                            TypeId::ANY
                        };
                        call_signatures.push(CallSignature {
                            type_params: Vec::new(),
                            params,
                            this_type,
                            return_type,
                            type_predicate: None,
                            is_method: false,
                        });
                    }
                    CONSTRUCT_SIGNATURE => {
                        let (params, this_type) = self.extract_params_from_signature(sig);
                        let return_type = if !sig.type_annotation.is_none() {
                            self.check(sig.type_annotation)
                        } else {
                            TypeId::ANY
                        };
                        construct_signatures.push(CallSignature {
                            type_params: Vec::new(),
                            params,
                            this_type,
                            return_type,
                            type_predicate: None,
                            is_method: false,
                        });
                    }
                    METHOD_SIGNATURE | PROPERTY_SIGNATURE => {
                        let Some(name) = self.get_property_name(sig.name) else {
                            continue;
                        };
                        let name_atom = self.ctx.types.intern_string(&name);

                        if member.kind == METHOD_SIGNATURE {
                            let (params, this_type) = self.extract_params_from_signature(sig);
                            let return_type = if !sig.type_annotation.is_none() {
                                self.check(sig.type_annotation)
                            } else {
                                TypeId::ANY
                            };
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
                            properties.push(PropertyInfo {
                                name: name_atom,
                                type_id: method_type,
                                write_type: method_type,
                                optional: sig.question_token,
                                readonly: self.has_readonly_modifier(&sig.modifiers),
                                is_method: true,
                                visibility: Visibility::Public,
                                parent_id: None,
                            });
                        } else {
                            let type_id = if !sig.type_annotation.is_none() {
                                self.check(sig.type_annotation)
                            } else {
                                TypeId::ANY
                            };
                            properties.push(PropertyInfo {
                                name: name_atom,
                                type_id,
                                write_type: type_id,
                                optional: sig.question_token,
                                readonly: self.has_readonly_modifier(&sig.modifiers),
                                is_method: false,
                                visibility: Visibility::Public,
                                parent_id: None,
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
                let key_type = if !param_data.type_annotation.is_none() {
                    self.check(param_data.type_annotation)
                } else {
                    TypeId::ANY
                };

                // TS1268: An index signature parameter type must be 'string', 'number',
                // 'symbol', or a template literal type.
                // Suppress when the parameter already has grammar errors (rest/optional) — matches tsc.
                let has_param_grammar_error =
                    param_data.dot_dot_dot_token || param_data.question_token;
                let is_valid_index_type = key_type == TypeId::STRING
                    || key_type == TypeId::NUMBER
                    || key_type == TypeId::SYMBOL
                    || tsz_solver::visitor::is_template_literal_type(self.ctx.types, key_type);
                if !is_valid_index_type
                    && !has_param_grammar_error
                    && let Some(pnode) = self.ctx.arena.get(param_idx)
                {
                    self.ctx.error(
                            pnode.pos,
                            pnode.end - pnode.pos,
                            "An index signature parameter type must be 'string', 'number', 'symbol', or a template literal type.".to_string(),
                            1268,
                        );
                }

                let value_type = if !index_sig.type_annotation.is_none() {
                    self.check(index_sig.type_annotation)
                } else {
                    TypeId::ANY
                };
                let readonly = self.has_readonly_modifier(&index_sig.modifiers);
                let info = IndexSignature {
                    key_type,
                    value_type,
                    readonly,
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
                && let Some(name) = self.get_property_name(accessor.name)
            {
                let name_atom = self.ctx.types.intern_string(&name);
                let is_getter = member.kind == tsz_parser::parser::syntax_kind_ext::GET_ACCESSOR;
                if is_getter {
                    let getter_type = if !accessor.type_annotation.is_none() {
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
                            visibility: Visibility::Public,
                            parent_id: None,
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
                            (!param.type_annotation.is_none())
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
                            visibility: Visibility::Public,
                            parent_id: None,
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
            });
        }

        if string_index.is_some() || number_index.is_some() {
            let factory = self.ctx.types.factory();

            return factory.object_with_index(ObjectShape {
                flags: ObjectFlags::empty(),
                properties,
                string_index,
                number_index,
                symbol: None,
            });
        }

        let factory = self.ctx.types.factory();
        factory.object(properties)
    }

    // =========================================================================
    // Type Query (typeof)
    // =========================================================================

    /// Get type from a type query node (typeof X).
    ///
    /// Creates a `TypeQuery` type that captures the type of a value.
    fn get_type_from_type_query(&mut self, idx: NodeIndex) -> TypeId {
        use tsz_lowering::TypeLowering;

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };

        let Some(type_query) = self.ctx.arena.get_type_query(node) else {
            return TypeId::ERROR;
        };

        // Prefer the already-computed value-space type at this query site when available.
        // This preserves flow-sensitive narrowing for `typeof expr` in type positions.
        if let Some(&expr_type) = self.ctx.node_types.get(&type_query.expr_name.0)
            && expr_type != TypeId::ERROR
        {
            return expr_type;
        }

        // For qualified names (e.g., typeof M.F2), resolve the symbol through
        // the binder's export tables. Simple identifiers are already handled by
        // the node_types cache above, but qualified names need member resolution.
        if let Some(sym_id) = self.resolve_type_query_symbol(type_query.expr_name) {
            let factory = self.ctx.types.factory();
            return factory.type_query(tsz_solver::SymbolRef(sym_id.0));
        }

        // Fall back to TypeLowering with proper value resolvers
        let value_resolver = |node_idx: NodeIndex| -> Option<u32> {
            let ident = self.ctx.arena.get_identifier_at(node_idx)?;
            let name = ident.escaped_text.as_str();
            let sym_id = self.ctx.binder.file_locals.get(name)?;
            Some(sym_id.0)
        };
        let type_resolver = |_node_idx: NodeIndex| -> Option<u32> { None };
        let lowering = TypeLowering::with_resolvers(
            self.ctx.arena,
            self.ctx.types,
            &type_resolver,
            &value_resolver,
        );

        lowering.lower_type(idx)
    }

    /// Resolve the symbol for a type query expression name.
    ///
    /// Handles both simple identifiers and qualified names (e.g., `M.F2`).
    /// For qualified names, walks through namespace exports to find the member.
    fn resolve_type_query_symbol(&self, expr_name: NodeIndex) -> Option<tsz_binder::SymbolId> {
        use tsz_parser::parser::syntax_kind_ext;

        let node = self.ctx.arena.get(expr_name)?;

        if node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
            let ident = self.ctx.arena.get_identifier(node)?;
            let name = ident.escaped_text.as_str();
            let sym_id = self.ctx.binder.file_locals.get(name)?;
            return Some(sym_id);
        }

        if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            let qn = self.ctx.arena.get_qualified_name(node)?;
            // Recursively resolve the left side
            let left_sym = self.resolve_type_query_symbol(qn.left)?;

            // Get the right name
            let right_node = self.ctx.arena.get(qn.right)?;
            let right_ident = self.ctx.arena.get_identifier(right_node)?;
            let right_name = right_ident.escaped_text.as_str();

            // Look through binder + libs for the left symbol's exports
            let lib_binders: Vec<std::sync::Arc<tsz_binder::BinderState>> = self
                .ctx
                .lib_contexts
                .iter()
                .map(|lc| std::sync::Arc::clone(&lc.binder))
                .collect();
            let left_symbol = self
                .ctx
                .binder
                .get_symbol_with_libs(left_sym, &lib_binders)?;

            if let Some(exports) = left_symbol.exports.as_ref()
                && let Some(member_sym) = exports.get(right_name)
            {
                return Some(member_sym);
            }
        }

        None
    }

    /// Check a mapped type ({ [P in K]: T }).
    ///
    /// This function validates the mapped type and emits TS7039 if the type expression
    /// after the colon is missing (e.g., `{[P in "bar"]}` instead of `{[P in "bar"]: string}`).
    fn get_type_from_mapped_type(&mut self, idx: NodeIndex) -> TypeId {
        use tsz_lowering::TypeLowering;
        use tsz_parser::parser::NodeIndex as ParserNodeIndex;

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };

        let Some(data) = self.ctx.arena.get_mapped_type(node) else {
            return TypeId::ERROR;
        };

        // TS7039: Mapped object type implicitly has an 'any' template type.
        // This error occurs when the type expression after the colon is missing.
        // Example: type Foo = {[P in "bar"]};  // Missing ": T" after "bar"]
        if data.type_node == ParserNodeIndex::NONE {
            let message = "Mapped object type implicitly has an 'any' template type.";
            self.ctx
                .error(node.pos, node.end - node.pos, message.to_string(), 7039);
            // Return ANY since the template type is implicitly any
            return TypeId::ANY;
        }

        // Delegate to TypeLowering for normal mapped type processing
        let type_param_bindings: Vec<(tsz_common::interner::Atom, TypeId)> = self
            .ctx
            .type_parameter_scope
            .iter()
            .map(|(name, &type_id)| (self.ctx.types.intern_string(name), type_id))
            .collect();

        // Create type and value resolvers (similar to the fallback case)
        let type_resolver = |node_idx: ParserNodeIndex| -> Option<u32> {
            let ident = self.ctx.arena.get_identifier_at(node_idx)?;
            let name = ident.escaped_text.as_str();

            if tsz_solver::is_compiler_managed_type(name) {
                return None;
            }

            if let Some(sym_id) = self.ctx.binder.file_locals.get(name) {
                let symbol = self.ctx.binder.get_symbol(sym_id)?;
                if (symbol.flags
                    & (tsz_binder::symbol_flags::TYPE
                        | tsz_binder::symbol_flags::REGULAR_ENUM
                        | tsz_binder::symbol_flags::CONST_ENUM))
                    != 0
                {
                    return Some(sym_id.0);
                }
            }

            for lib_ctx in &self.ctx.lib_contexts {
                if let Some(lib_sym_id) = lib_ctx.binder.file_locals.get(name) {
                    let symbol = lib_ctx.binder.get_symbol(lib_sym_id)?;
                    if (symbol.flags
                        & (tsz_binder::symbol_flags::TYPE
                            | tsz_binder::symbol_flags::REGULAR_ENUM
                            | tsz_binder::symbol_flags::CONST_ENUM))
                        != 0
                    {
                        let file_sym_id =
                            self.ctx.binder.file_locals.get(name).unwrap_or(lib_sym_id);
                        return Some(file_sym_id.0);
                    }
                }
            }

            None
        };

        let value_resolver = |node_idx: ParserNodeIndex| -> Option<u32> {
            let ident = self.ctx.arena.get_identifier_at(node_idx)?;
            let name = ident.escaped_text.as_str();

            if let Some(sym_id) = self.ctx.binder.file_locals.get(name)
                && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                && (symbol.flags
                    & (tsz_binder::symbol_flags::VALUE
                        | tsz_binder::symbol_flags::ALIAS
                        | tsz_binder::symbol_flags::REGULAR_ENUM
                        | tsz_binder::symbol_flags::CONST_ENUM))
                    != 0
            {
                return Some(sym_id.0);
            }

            for lib_ctx in &self.ctx.lib_contexts {
                if let Some(lib_sym_id) = lib_ctx.binder.file_locals.get(name)
                    && let Some(symbol) = lib_ctx.binder.get_symbol(lib_sym_id)
                    && (symbol.flags
                        & (tsz_binder::symbol_flags::VALUE
                            | tsz_binder::symbol_flags::ALIAS
                            | tsz_binder::symbol_flags::REGULAR_ENUM
                            | tsz_binder::symbol_flags::CONST_ENUM))
                        != 0
                {
                    let file_sym_id = self.ctx.binder.file_locals.get(name).unwrap_or(lib_sym_id);
                    return Some(file_sym_id.0);
                }
            }

            None
        };

        let def_id_resolver = |node_idx: ParserNodeIndex| -> Option<tsz_solver::def::DefId> {
            let sym_id = type_resolver(node_idx)?;
            Some(self.ctx.get_or_create_def_id(tsz_binder::SymbolId(sym_id)))
        };

        let mut lowering = TypeLowering::with_hybrid_resolver(
            self.ctx.arena,
            self.ctx.types,
            &type_resolver,
            &def_id_resolver,
            &value_resolver,
        );
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
                    this_type = (!param_data.type_annotation.is_none())
                        .then(|| self.check(param_data.type_annotation));
                    continue;
                }

                // Get parameter type
                let type_id = if !param_data.type_annotation.is_none() {
                    self.check(param_data.type_annotation)
                } else {
                    TypeId::ANY
                };

                let optional = param_data.question_token || !param_data.initializer.is_none();
                let rest = param_data.dot_dot_dot_token;

                // Under strictNullChecks, optional parameters (with `?`) get
                // `undefined` added to their type.
                let effective_type = if param_data.question_token
                    && self.ctx.strict_null_checks()
                    && type_id != TypeId::ANY
                    && type_id != TypeId::ERROR
                    && type_id != TypeId::UNDEFINED
                {
                    let factory = self.ctx.types.factory();
                    factory.union(vec![type_id, TypeId::UNDEFINED])
                } else {
                    type_id
                };

                params.push(ParamInfo {
                    name: Some(self.ctx.types.intern_string(&name)),
                    type_id: effective_type,
                    optional,
                    rest,
                });
            }
        }

        (params, this_type)
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
        use tsz_scanner::SyntaxKind;

        let name_node = self.ctx.arena.get(name_idx)?;

        // Identifier
        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            return Some(ident.escaped_text.clone());
        }

        // String literal, no-substitution template literal, or numeric literal
        if matches!(
            name_node.kind,
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                || k == SyntaxKind::NumericLiteral as u16
        ) && let Some(lit) = self.ctx.arena.get_literal(name_node)
        {
            // Canonicalize numeric property names (e.g. "1.", "1.0" -> "1")
            if name_node.kind == SyntaxKind::NumericLiteral as u16
                && let Some(canonical) = tsz_solver::utils::canonicalize_numeric_name(&lit.text)
            {
                return Some(canonical);
            }
            return Some(lit.text.clone());
        }

        None
    }

    /// Check if a modifier list contains the readonly modifier.
    fn has_readonly_modifier(&self, modifiers: &Option<tsz_parser::parser::NodeList>) -> bool {
        use tsz_scanner::SyntaxKind;
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.ctx.arena.get(mod_idx)
                    && mod_node.kind == SyntaxKind::ReadonlyKeyword as u16
                {
                    return true;
                }
            }
        }
        false
    }

    /// Get the context reference (for read-only access).
    pub const fn context(&self) -> &CheckerContext<'ctx> {
        self.ctx
    }
}

#[cfg(test)]
#[path = "../tests/type_node.rs"]
mod tests;
