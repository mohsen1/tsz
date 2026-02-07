//! Type Node Checking
//!
//! This module handles type resolution from AST type nodes (type annotations,
//! type references, union types, intersection types, etc.).
//!
//! It follows the "Check Fast, Explain Slow" pattern where we first
//! resolve types, then use the solver to explain any failures.

use super::context::CheckerContext;
use std::cell::Cell;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;
use tsz_solver::types::Visibility;

/// Maximum recursion depth for type node checking to prevent stack overflow
const MAX_TYPE_NODE_CHECK_DEPTH: u32 = 500;

/// Type node checker that operates on the shared context.
///
/// This is a stateless checker that borrows the context mutably.
/// All type resolution for type nodes goes through this checker.
pub struct TypeNodeChecker<'a, 'ctx> {
    pub ctx: &'a mut CheckerContext<'ctx>,
    /// Recursion depth counter for stack overflow protection
    depth: Cell<u32>,
}

impl<'a, 'ctx> TypeNodeChecker<'a, 'ctx> {
    /// Create a new type node checker with a mutable context reference.
    pub fn new(ctx: &'a mut CheckerContext<'ctx>) -> Self {
        Self {
            ctx,
            depth: Cell::new(0),
        }
    }

    /// Check a type node and return its type.
    ///
    /// This is the main entry point for type node resolution.
    /// It handles caching and dispatches to specific type node handlers.
    pub fn check(&mut self, idx: NodeIndex) -> TypeId {
        // Stack overflow protection
        let current_depth = self.depth.get();
        if current_depth >= MAX_TYPE_NODE_CHECK_DEPTH {
            return TypeId::ERROR;
        }
        self.depth.set(current_depth + 1);

        // Check cache first
        if let Some(&cached) = self.ctx.node_types.get(&idx.0) {
            if cached == TypeId::ERROR {
                // Always use cached ERROR to prevent duplicate emissions
                self.depth.set(current_depth);
                return cached;
            }

            // For non-ERROR cached results, check if we're in a generic context
            // If we're not in a generic context (type params are empty), the cache is valid
            if self.ctx.type_parameter_scope.is_empty() {
                // No type parameters in scope - cache is valid
                self.depth.set(current_depth);
                return cached;
            }
            // If we have type parameters in scope, we need to be more careful
            // For now, recompute to ensure correctness
            // TODO: Add cache key based on type param hash for smarter caching
        }

        // Compute and cache
        let result = self.compute_type(idx);
        self.ctx.node_types.insert(idx.0, result);

        self.depth.set(current_depth);
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

            // Fall back to TypeLowering for type nodes not handled above
            // (conditional types, mapped types, indexed access types, etc.)
            _ => {
                use tsz_binder::symbol_flags;
                use tsz_solver::TypeLowering;
                use tsz_solver::types::is_compiler_managed_type;

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
                        if let Some(sym_id) = lib_ctx.binder.file_locals.get(name) {
                            let symbol = lib_ctx.binder.get_symbol(sym_id)?;
                            if (symbol.flags
                                & (symbol_flags::TYPE
                                    | symbol_flags::REGULAR_ENUM
                                    | symbol_flags::CONST_ENUM))
                                != 0
                            {
                                return Some(sym_id.0);
                            }
                        }
                    }

                    None
                };

                let value_resolver = |node_idx: NodeIndex| -> Option<u32> {
                    let ident = self.ctx.arena.get_identifier_at(node_idx)?;
                    let name = ident.escaped_text.as_str();

                    if let Some(sym_id) = self.ctx.binder.file_locals.get(name) {
                        if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                            if (symbol.flags
                                & (symbol_flags::VALUE
                                    | symbol_flags::ALIAS
                                    | symbol_flags::REGULAR_ENUM
                                    | symbol_flags::CONST_ENUM))
                                != 0
                            {
                                return Some(sym_id.0);
                            }
                        }
                    }

                    for lib_ctx in &self.ctx.lib_contexts {
                        if let Some(sym_id) = lib_ctx.binder.file_locals.get(name) {
                            if let Some(symbol) = lib_ctx.binder.get_symbol(sym_id) {
                                if (symbol.flags
                                    & (symbol_flags::VALUE
                                        | symbol_flags::ALIAS
                                        | symbol_flags::REGULAR_ENUM
                                        | symbol_flags::CONST_ENUM))
                                    != 0
                                {
                                    return Some(sym_id.0);
                                }
                            }
                        }
                    }

                    None
                };

                // Create def_id_resolver to prefer Lazy(DefId) over Ref(SymbolRef)
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
        }
    }

    // =========================================================================
    // Type Reference Resolution
    // =========================================================================

    /// Get type from a type reference node (e.g., "number", "string", "MyType").
    fn get_type_from_type_reference(&mut self, idx: NodeIndex) -> TypeId {
        use tsz_binder::symbol_flags;
        use tsz_solver::TypeLowering;
        use tsz_solver::types::is_compiler_managed_type;

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
                if let Some(sym_id) = lib_ctx.binder.file_locals.get(name) {
                    let symbol = lib_ctx.binder.get_symbol(sym_id)?;
                    // Check for TYPE flag or ENUM flag (enums can be used as types)
                    if (symbol.flags
                        & (symbol_flags::TYPE
                            | symbol_flags::REGULAR_ENUM
                            | symbol_flags::CONST_ENUM))
                        != 0
                    {
                        return Some(sym_id.0);
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

            return self.ctx.types.union(member_types);
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
            if member_types.len() == 1 {
                return member_types[0];
            }

            return self.ctx.types.intersection(member_types);
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

        if let Some(array_type) = self.ctx.arena.get_array_type(node) {
            let elem_type = self.check(array_type.element_type);
            return self.ctx.types.array(elem_type);
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

            return self.ctx.types.tuple(elements);
        }

        TypeId::ERROR
    }

    // =========================================================================
    // Type Operators
    // =========================================================================

    /// Get type from a type operator node (readonly T[], readonly [T, U], unique symbol).
    ///
    /// Handles type modifiers like:
    /// - `readonly T[]` - Creates ReadonlyType wrapper
    /// - `unique symbol` - Special marker for unique symbols
    fn get_type_from_type_operator(&mut self, idx: NodeIndex) -> TypeId {
        use tsz_scanner::SyntaxKind;

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };

        if let Some(type_op) = self.ctx.arena.get_type_operator(node) {
            let operator = type_op.operator;
            let inner_type = self.check(type_op.type_node);

            // Handle readonly operator
            if operator == SyntaxKind::ReadonlyKeyword as u16 {
                // Wrap the inner type in ReadonlyType
                return self
                    .ctx
                    .types
                    .intern(tsz_solver::TypeKey::ReadonlyType(inner_type));
            }

            // Handle keyof operator
            if operator == SyntaxKind::KeyOfKeyword as u16 {
                return self
                    .ctx
                    .types
                    .intern(tsz_solver::TypeKey::KeyOf(inner_type));
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

        if let Some(indexed_access) = self.ctx.arena.get_indexed_access_type(node) {
            let object_type = self.check(indexed_access.object_type);
            let index_type = self.check(indexed_access.index_type);
            self.ctx
                .types
                .intern(tsz_solver::TypeKey::IndexAccess(object_type, index_type))
        } else {
            TypeId::ERROR
        }
    }

    // =========================================================================
    // Function and Callable Types
    // =========================================================================

    /// Get type from a function type node (e.g., () => number, (x: string) => void).
    fn get_type_from_function_type(&mut self, idx: NodeIndex) -> TypeId {
        use tsz_binder::symbol_flags;
        use tsz_solver::TypeLowering;
        use tsz_solver::types::is_compiler_managed_type;

        // EXPLICIT WALK: Visit the return type annotation to emit diagnostics.
        // This ensures TS2304 "Cannot find name" errors are emitted for undefined
        // type references in function return types (e.g., `(x) => UndefinedType`).
        // TypeLowering computes the type but does not emit diagnostics.
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };
        let Some(func_data) = self.ctx.arena.get_function_type(node) else {
            return TypeId::ERROR;
        };
        // Explicitly walk the return type to trigger TS2304 errors.
        // Must use self.check() not self.compute_type() to go through caching and proper dispatch.
        if !func_data.type_annotation.is_none() {
            let _ = self.check(func_data.type_annotation);
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
                if let Some(sym_id) = lib_ctx.binder.file_locals.get(name) {
                    let symbol = lib_ctx.binder.get_symbol(sym_id)?;
                    // Check for TYPE flag or ENUM flag (enums can be used as types)
                    if (symbol.flags
                        & (symbol_flags::TYPE
                            | symbol_flags::REGULAR_ENUM
                            | symbol_flags::CONST_ENUM))
                        != 0
                    {
                        return Some(sym_id.0);
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

        let mut lowering = TypeLowering::with_resolvers(
            self.ctx.arena,
            self.ctx.types,
            &type_resolver,
            &value_resolver,
        );
        if !type_param_bindings.is_empty() {
            lowering = lowering.with_type_param_bindings(type_param_bindings);
        }

        lowering.lower_type(idx)
    }

    /// Get type from a type literal node ({ a: number; b(): string; }).
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
                            let method_type = self.ctx.types.function(shape);
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
            }
        }

        if !call_signatures.is_empty() || !construct_signatures.is_empty() {
            return self.ctx.types.callable(CallableShape {
                call_signatures,
                construct_signatures,
                properties,
                string_index,
                number_index,
                symbol: None,
            });
        }

        if string_index.is_some() || number_index.is_some() {
            return self.ctx.types.object_with_index(ObjectShape {
                flags: ObjectFlags::empty(),
                properties,
                string_index,
                number_index,
                symbol: None,
            });
        }

        self.ctx.types.object(properties)
    }

    // =========================================================================
    // Type Query (typeof)
    // =========================================================================

    /// Get type from a type query node (typeof X).
    ///
    /// Creates a TypeQuery type that captures the type of a value.
    fn get_type_from_type_query(&mut self, idx: NodeIndex) -> TypeId {
        use tsz_solver::TypeLowering;

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };

        let Some(_type_query) = self.ctx.arena.get_type_query(node) else {
            return TypeId::ERROR;
        };

        // For now, delegate to TypeLowering
        // This will be expanded as we move more logic here
        let type_resolver = |_node_idx: NodeIndex| -> Option<u32> { None };
        let value_resolver = |_node_idx: NodeIndex| -> Option<u32> { None };
        let lowering = TypeLowering::with_resolvers(
            self.ctx.arena,
            self.ctx.types,
            &type_resolver,
            &value_resolver,
        );

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
                    this_type = if !param_data.type_annotation.is_none() {
                        Some(self.check(param_data.type_annotation))
                    } else {
                        None
                    };
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
            return Some(lit.text.clone());
        }

        None
    }

    /// Check if a modifier list contains the readonly modifier.
    fn has_readonly_modifier(&self, modifiers: &Option<tsz_parser::parser::NodeList>) -> bool {
        use tsz_scanner::SyntaxKind;
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.ctx.arena.get(mod_idx) {
                    if mod_node.kind == SyntaxKind::ReadonlyKeyword as u16 {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Get the context reference (for read-only access).
    pub fn context(&self) -> &CheckerContext<'ctx> {
        self.ctx
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsz_binder::BinderState;
    use tsz_parser::parser::ParserState;
    use tsz_solver::TypeInterner;

    #[test]
    fn test_type_node_checker_number_keyword() {
        let source = "let x: number;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let types = TypeInterner::new();
        let mut ctx = CheckerContext::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            crate::context::CheckerOptions::default(),
        );

        // Just verify the checker can be created - actual type checking
        // requires more complex setup
        let _checker = TypeNodeChecker::new(&mut ctx);
    }
}
