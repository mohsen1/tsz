//! Type lowering: AST nodes → TypeId
//!
//! This module implements the "bridge" that converts raw AST nodes (Node)
//! into the structural type system (TypeId).
//!
//! Lowering is lazy - types are only computed when queried.

use crate::interner::Atom;
use crate::parser::NodeList;
use crate::parser::base::NodeIndex;
use crate::parser::node::{IndexSignatureData, NodeArena, SignatureData, TypeAliasData};
use crate::parser::syntax_kind_ext;
use crate::scanner::SyntaxKind;
use crate::solver::subtype::{SubtypeChecker, TypeResolver};
use crate::solver::types::*;
use crate::solver::{QueryDatabase, TypeDatabase};
use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::RefCell;

#[cfg(test)]
use crate::solver::TypeInterner;

/// Maximum number of type lowering operations to prevent infinite loops
pub const MAX_LOWERING_OPERATIONS: u32 = 100_000;

/// Type lowering context.
/// Converts AST type nodes into interned TypeIds.
pub struct TypeLowering<'a> {
    arena: &'a NodeArena,
    interner: &'a dyn TypeDatabase,
    /// Optional type resolver - resolves identifier nodes to SymbolIds.
    /// If provided, this enables correct abstract class detection.
    type_resolver: Option<&'a dyn Fn(NodeIndex) -> Option<u32>>,
    /// Optional value resolver for typeof queries.
    value_resolver: Option<&'a dyn Fn(NodeIndex) -> Option<u32>>,
    type_param_scopes: RefCell<Vec<Vec<(Atom, TypeId)>>>,
    /// Operation counter to prevent infinite loops
    operations: RefCell<u32>,
    /// Whether the operation limit has been exceeded
    limit_exceeded: RefCell<bool>,
}

struct InterfaceParts {
    properties: FxHashMap<Atom, PropertyMerge>,
    call_signatures: Vec<CallSignature>,
    construct_signatures: Vec<CallSignature>,
    string_index: Option<IndexSignature>,
    number_index: Option<IndexSignature>,
}

enum PropertyMerge {
    Property(PropertyInfo),
    Method(MethodOverloads),
    Conflict(PropertyInfo),
}

struct MethodOverloads {
    signatures: Vec<CallSignature>,
    optional: bool,
    readonly: bool,
}

struct IndexSignatureResolver;

impl TypeResolver for IndexSignatureResolver {
    fn resolve_ref(&self, _symbol: SymbolRef, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        Some(TypeId::ERROR) // Unresolved symbol during index signature checking - propagate error
    }
}

impl InterfaceParts {
    fn new() -> Self {
        InterfaceParts {
            properties: FxHashMap::default(),
            call_signatures: Vec::new(),
            construct_signatures: Vec::new(),
            string_index: None,
            number_index: None,
        }
    }

    fn merge_property(&mut self, prop: PropertyInfo) {
        use std::collections::hash_map::Entry;

        match self.properties.entry(prop.name) {
            Entry::Vacant(entry) => {
                entry.insert(PropertyMerge::Property(prop));
            }
            Entry::Occupied(mut entry) => match entry.get_mut() {
                PropertyMerge::Property(existing) => {
                    if existing.type_id == prop.type_id
                        && existing.write_type == prop.write_type
                        && existing.optional == prop.optional
                        && existing.readonly == prop.readonly
                        && existing.is_method == prop.is_method
                    {
                        return;
                    }
                    let conflict = PropertyInfo {
                        name: prop.name,
                        type_id: TypeId::ERROR,
                        write_type: TypeId::ERROR,
                        optional: existing.optional && prop.optional,
                        readonly: existing.readonly && prop.readonly,
                        is_method: false,
                    };
                    entry.insert(PropertyMerge::Conflict(conflict));
                }
                PropertyMerge::Method(methods) => {
                    let conflict = PropertyInfo {
                        name: prop.name,
                        type_id: TypeId::ERROR,
                        write_type: TypeId::ERROR,
                        optional: methods.optional && prop.optional,
                        readonly: false,
                        is_method: false,
                    };
                    entry.insert(PropertyMerge::Conflict(conflict));
                }
                PropertyMerge::Conflict(_) => {}
            },
        }
    }

    fn merge_method(
        &mut self,
        name: Atom,
        signature: CallSignature,
        optional: bool,
        readonly: bool,
    ) {
        use std::collections::hash_map::Entry;

        match self.properties.entry(name) {
            Entry::Vacant(entry) => {
                entry.insert(PropertyMerge::Method(MethodOverloads {
                    signatures: vec![signature],
                    optional,
                    readonly,
                }));
            }
            Entry::Occupied(mut entry) => match entry.get_mut() {
                PropertyMerge::Method(methods) => {
                    methods.signatures.push(signature);
                    methods.optional |= optional;
                    methods.readonly &= readonly;
                }
                PropertyMerge::Property(prop) => {
                    let conflict = PropertyInfo {
                        name,
                        type_id: TypeId::ERROR,
                        write_type: TypeId::ERROR,
                        optional: prop.optional && optional,
                        readonly: false,
                        is_method: false,
                    };
                    entry.insert(PropertyMerge::Conflict(conflict));
                }
                PropertyMerge::Conflict(_) => {}
            },
        }
    }

    fn merge_index_signature(&mut self, index: IndexSignature) {
        let target = if index.key_type == TypeId::NUMBER {
            &mut self.number_index
        } else {
            &mut self.string_index
        };

        if let Some(existing) = target.as_mut() {
            if existing.value_type != index.value_type || existing.readonly != index.readonly {
                existing.value_type = TypeId::ERROR;
                existing.readonly = false;
            }
        } else {
            *target = Some(index);
        }
    }
}

impl<'a> TypeLowering<'a> {
    /// Maximum iterations for tree-walking loops to prevent infinite loops.
    const MAX_TREE_WALK_ITERATIONS: usize = 10_000;

    pub fn new(arena: &'a NodeArena, interner: &'a dyn QueryDatabase) -> Self {
        TypeLowering {
            arena,
            interner: interner.as_type_database(),
            type_resolver: None,
            value_resolver: None,
            type_param_scopes: RefCell::new(Vec::new()),
            operations: RefCell::new(0),
            limit_exceeded: RefCell::new(false),
        }
    }

    /// Create a TypeLowering with a symbol resolver.
    /// The resolver converts identifier names to actual SymbolIds from the binder.
    pub fn with_resolver(
        arena: &'a NodeArena,
        interner: &'a dyn QueryDatabase,
        resolver: &'a dyn Fn(NodeIndex) -> Option<u32>,
    ) -> Self {
        TypeLowering {
            arena,
            interner: interner.as_type_database(),
            type_resolver: Some(resolver),
            value_resolver: Some(resolver),
            type_param_scopes: RefCell::new(Vec::new()),
            operations: RefCell::new(0),
            limit_exceeded: RefCell::new(false),
        }
    }

    /// Create a TypeLowering with separate type/value resolvers.
    pub fn with_resolvers(
        arena: &'a NodeArena,
        interner: &'a dyn QueryDatabase,
        type_resolver: &'a dyn Fn(NodeIndex) -> Option<u32>,
        value_resolver: &'a dyn Fn(NodeIndex) -> Option<u32>,
    ) -> Self {
        TypeLowering {
            arena,
            interner: interner.as_type_database(),
            type_resolver: Some(type_resolver),
            value_resolver: Some(value_resolver),
            type_param_scopes: RefCell::new(Vec::new()),
            operations: RefCell::new(0),
            limit_exceeded: RefCell::new(false),
        }
    }

    /// Check if the operation limit has been exceeded
    fn check_limit(&self) -> bool {
        if *self.limit_exceeded.borrow() {
            return true;
        }
        let mut ops = self.operations.borrow_mut();
        *ops += 1;
        if *ops > MAX_LOWERING_OPERATIONS {
            *self.limit_exceeded.borrow_mut() = true;
            return true;
        }
        false
    }

    /// Reset the operation counter (for testing purposes)
    #[cfg(test)]
    #[allow(dead_code)]
    fn reset_operations(&self) {
        *self.operations.borrow_mut() = 0;
        *self.limit_exceeded.borrow_mut() = false;
    }

    pub fn seed_type_params(&self, params: &[(Atom, TypeId)]) {
        if params.is_empty() {
            return;
        }
        self.push_type_param_scope();
        for (name, type_id) in params {
            self.add_type_param_binding(*name, *type_id);
        }
    }

    /// Initialize with existing type parameter bindings.
    /// These are added to a new scope that persists for the lifetime of the TypeLowering.
    pub fn with_type_param_bindings(mut self, bindings: Vec<(Atom, TypeId)>) -> Self {
        if !bindings.is_empty() {
            self.type_param_scopes = RefCell::new(vec![bindings]);
        }
        self
    }

    /// Resolve a node to a type symbol ID if a resolver is provided.
    fn resolve_type_symbol(&self, node_idx: NodeIndex) -> Option<u32> {
        self.type_resolver.and_then(|resolver| resolver(node_idx))
    }

    /// Resolve a node to a value symbol ID if a resolver is provided.
    fn resolve_value_symbol(&self, node_idx: NodeIndex) -> Option<u32> {
        if let Some(resolver) = self.value_resolver {
            resolver(node_idx)
        } else {
            self.resolve_type_symbol(node_idx)
        }
    }

    fn push_type_param_scope(&self) {
        self.type_param_scopes.borrow_mut().push(Vec::new());
    }

    fn pop_type_param_scope(&self) {
        let _ = self.type_param_scopes.borrow_mut().pop();
    }

    fn add_type_param_binding(&self, name: Atom, type_id: TypeId) {
        if let Some(scope) = self.type_param_scopes.borrow_mut().last_mut() {
            scope.push((name, type_id));
        }
    }

    fn lookup_type_param(&self, name: &str) -> Option<TypeId> {
        let atom = self.interner.intern_string(name);
        let scopes = self.type_param_scopes.borrow();
        for scope in scopes.iter().rev() {
            for (scope_name, type_id) in scope.iter().rev() {
                if *scope_name == atom {
                    return Some(*type_id);
                }
            }
        }
        None
    }

    /// Import type parameter bindings from an external scope (e.g., checker's type parameter scope).
    /// This allows TypeLowering to access type parameters that were defined outside of it.
    pub fn import_type_params<'b, I>(&self, bindings: I)
    where
        I: Iterator<Item = (&'b String, &'b TypeId)>,
    {
        self.push_type_param_scope();
        for (name, &type_id) in bindings {
            let atom = self.interner.intern_string(name);
            self.add_type_param_binding(atom, type_id);
        }
    }

    /// Lower a type node to a TypeId.
    /// This is the main entry point for type synthesis.
    pub fn lower_type(&self, node_idx: NodeIndex) -> TypeId {
        // Check operation limit to prevent infinite loops
        if self.check_limit() {
            return TypeId::ERROR;
        }

        if node_idx == NodeIndex::NONE {
            // Return ERROR for missing type annotations to prevent "Any poisoning".
            // This forces explicit type annotations and surfaces bugs early instead
            // of silently accepting invalid assignments via any/unknown defaults.
            // Per SOLVER.md Section 6.4: Error propagation prevents cascading noise.
            return TypeId::ERROR;
        }

        let node = match self.arena.get(node_idx) {
            Some(n) => n,
            None => return TypeId::ERROR,
        };

        match node.kind {
            // =========================================================================
            // Keyword types
            // =========================================================================
            k if k == SyntaxKind::AnyKeyword as u16 => TypeId::ANY,
            k if k == SyntaxKind::UnknownKeyword as u16 => TypeId::UNKNOWN,
            k if k == SyntaxKind::NeverKeyword as u16 => TypeId::NEVER,
            k if k == SyntaxKind::VoidKeyword as u16 => TypeId::VOID,
            k if k == SyntaxKind::UndefinedKeyword as u16 => TypeId::UNDEFINED,
            k if k == SyntaxKind::NullKeyword as u16 => TypeId::NULL,
            k if k == SyntaxKind::BooleanKeyword as u16 => TypeId::BOOLEAN,
            k if k == SyntaxKind::NumberKeyword as u16 => TypeId::NUMBER,
            k if k == SyntaxKind::StringKeyword as u16 => TypeId::STRING,
            k if k == SyntaxKind::BigIntKeyword as u16 => TypeId::BIGINT,
            k if k == SyntaxKind::SymbolKeyword as u16 => TypeId::SYMBOL,
            k if k == SyntaxKind::ObjectKeyword as u16 => TypeId::OBJECT,

            // =========================================================================
            // Literal types (true, false)
            // =========================================================================
            k if k == SyntaxKind::TrueKeyword as u16 => self.interner.literal_boolean(true),
            k if k == SyntaxKind::FalseKeyword as u16 => self.interner.literal_boolean(false),

            // =========================================================================
            // Composite types
            // =========================================================================
            k if k == syntax_kind_ext::UNION_TYPE => self.lower_union_type(node_idx),
            k if k == syntax_kind_ext::INTERSECTION_TYPE => self.lower_intersection_type(node_idx),

            // =========================================================================
            // Array and tuple types
            // =========================================================================
            k if k == syntax_kind_ext::ARRAY_TYPE => self.lower_array_type(node_idx),
            k if k == syntax_kind_ext::TUPLE_TYPE => self.lower_tuple_type(node_idx),

            // =========================================================================
            // Function type
            // =========================================================================
            k if k == syntax_kind_ext::FUNCTION_TYPE => self.lower_function_type(node_idx),

            // =========================================================================
            // Type literal (object type)
            // =========================================================================
            k if k == syntax_kind_ext::TYPE_LITERAL => self.lower_type_literal(node_idx),

            // =========================================================================
            // Conditional type
            // =========================================================================
            k if k == syntax_kind_ext::CONDITIONAL_TYPE => self.lower_conditional_type(node_idx),

            // =========================================================================
            // Mapped type
            // =========================================================================
            k if k == syntax_kind_ext::MAPPED_TYPE => self.lower_mapped_type(node_idx),

            // =========================================================================
            // Indexed access type
            // =========================================================================
            k if k == syntax_kind_ext::INDEXED_ACCESS_TYPE => {
                self.lower_indexed_access_type(node_idx)
            }

            // =========================================================================
            // Literal type (string literal, number literal in type position)
            // =========================================================================
            k if k == syntax_kind_ext::LITERAL_TYPE => self.lower_literal_type(node_idx),

            // =========================================================================
            // Type reference (NamedType or NamedType<Args>)
            // =========================================================================
            k if k == syntax_kind_ext::TYPE_REFERENCE => self.lower_type_reference(node_idx),

            // =========================================================================
            // Qualified name (A.B)
            // =========================================================================
            k if k == syntax_kind_ext::QUALIFIED_NAME => self.lower_qualified_name_type(node_idx),

            // =========================================================================
            // Identifier (simple type reference without type arguments)
            // =========================================================================
            k if k == SyntaxKind::Identifier as u16 => self.lower_identifier_type(node_idx),

            // =========================================================================
            // This type
            // =========================================================================
            k if k == SyntaxKind::ThisKeyword as u16 => self.interner.intern(TypeKey::ThisType),
            k if k == syntax_kind_ext::THIS_TYPE => self.interner.intern(TypeKey::ThisType),

            // =========================================================================
            // Parenthesized type
            // =========================================================================
            k if k == syntax_kind_ext::PARENTHESIZED_TYPE => {
                self.lower_parenthesized_type(node_idx)
            }

            // =========================================================================
            // Type query (typeof in type position)
            // =========================================================================
            k if k == syntax_kind_ext::TYPE_QUERY => self.lower_type_query(node_idx),

            // =========================================================================
            // Type predicate (x is T / asserts x is T)
            // =========================================================================
            k if k == syntax_kind_ext::TYPE_PREDICATE => self.lower_type_predicate(node_idx),

            // =========================================================================
            // Type operator (keyof, readonly, unique)
            // =========================================================================
            k if k == syntax_kind_ext::TYPE_OPERATOR => self.lower_type_operator(node_idx),

            // =========================================================================
            // Infer type (infer R)
            // =========================================================================
            k if k == syntax_kind_ext::INFER_TYPE => self.lower_infer_type(node_idx),

            // =========================================================================
            // Template literal type
            // =========================================================================
            k if k == syntax_kind_ext::TEMPLATE_LITERAL_TYPE => {
                self.lower_template_literal_type(node_idx)
            }

            // =========================================================================
            // Named tuple member
            // =========================================================================
            k if k == syntax_kind_ext::NAMED_TUPLE_MEMBER => {
                self.lower_named_tuple_member(node_idx)
            }

            // =========================================================================
            // Constructor type (new () => T)
            // =========================================================================
            k if k == syntax_kind_ext::CONSTRUCTOR_TYPE => self.lower_constructor_type(node_idx),

            // =========================================================================
            // Optional/Rest types (unwrap)
            // =========================================================================
            k if k == syntax_kind_ext::OPTIONAL_TYPE || k == syntax_kind_ext::REST_TYPE => {
                self.lower_wrapped_type(node_idx)
            }

            // =========================================================================
            // Unknown/unsupported - return ERROR to propagate type checking errors
            // This aligns with PROJECT_DIRECTION.md: errors should not be silently accepted
            // =========================================================================
            _ => TypeId::ERROR,
        }
    }

    /// Lower a union type (A | B | C)
    fn lower_union_type(&self, node_idx: NodeIndex) -> TypeId {
        let node = match self.arena.get(node_idx) {
            Some(n) => n,
            None => return TypeId::ERROR,
        };

        if let Some(data) = self.arena.get_composite_type(node) {
            let members: Vec<TypeId> = data
                .types
                .nodes
                .iter()
                .map(|&idx| self.lower_type(idx))
                .collect();
            self.interner.union(members)
        } else {
            TypeId::ERROR
        }
    }

    /// Lower an intersection type (A & B & C)
    fn lower_intersection_type(&self, node_idx: NodeIndex) -> TypeId {
        let node = match self.arena.get(node_idx) {
            Some(n) => n,
            None => return TypeId::ERROR,
        };

        if let Some(data) = self.arena.get_composite_type(node) {
            let members: Vec<TypeId> = data
                .types
                .nodes
                .iter()
                .map(|&idx| self.lower_type(idx))
                .collect();
            self.interner.intersection(members)
        } else {
            TypeId::ERROR
        }
    }

    /// Lower an array type (T[])
    fn lower_array_type(&self, node_idx: NodeIndex) -> TypeId {
        let node = match self.arena.get(node_idx) {
            Some(n) => n,
            None => return TypeId::ERROR,
        };

        if let Some(data) = self.arena.get_array_type(node) {
            let element_type = self.lower_type(data.element_type);
            self.interner.array(element_type)
        } else {
            TypeId::ERROR // Missing array type data - propagate error
        }
    }

    /// Lower a tuple type ([A, B, C])
    fn lower_tuple_type(&self, node_idx: NodeIndex) -> TypeId {
        let node = match self.arena.get(node_idx) {
            Some(n) => n,
            None => return TypeId::ERROR,
        };

        if let Some(data) = self.arena.get_tuple_type(node) {
            let elements: Vec<TupleElement> = data
                .elements
                .nodes
                .iter()
                .map(|&idx| self.lower_tuple_element(idx))
                .collect();
            self.interner.tuple(elements)
        } else {
            self.interner.tuple(vec![])
        }
    }

    /// Lower a tuple element, preserving name, optional, and rest metadata.
    fn lower_tuple_element(&self, node_idx: NodeIndex) -> TupleElement {
        let Some(node) = self.arena.get(node_idx) else {
            return TupleElement {
                type_id: TypeId::ERROR,
                name: None,
                optional: false,
                rest: false,
            };
        };

        if node.kind == syntax_kind_ext::NAMED_TUPLE_MEMBER
            && let Some(data) = self.arena.get_named_tuple_member(node)
        {
            let name = if let Some(name_node) = self.arena.get(data.name) {
                self.arena
                    .get_identifier(name_node)
                    .map(|id_data| self.interner.intern_string(&id_data.escaped_text))
            } else {
                None
            };

            return TupleElement {
                type_id: self.lower_type(data.type_node),
                name,
                optional: data.question_token,
                rest: data.dot_dot_dot_token,
            };
        }

        if node.kind == syntax_kind_ext::REST_TYPE || node.kind == syntax_kind_ext::OPTIONAL_TYPE {
            let wrapped = if let Some(data) = self.arena.get_wrapped_type(node) {
                Some(data.type_node)
            } else {
                self.arena
                    .type_operators
                    .get(node.data_index as usize)
                    .map(|data| data.type_node)
            };

            return TupleElement {
                type_id: wrapped
                    .map_or_else(|| self.lower_type(node_idx), |inner| self.lower_type(inner)),
                name: None,
                optional: node.kind == syntax_kind_ext::OPTIONAL_TYPE,
                rest: node.kind == syntax_kind_ext::REST_TYPE,
            };
        }

        TupleElement {
            type_id: self.lower_type(node_idx),
            name: None,
            optional: false,
            rest: false,
        }
    }

    fn with_type_params<R>(
        &self,
        type_params: &Option<NodeList>,
        f: impl FnOnce() -> R,
    ) -> (Vec<TypeParamInfo>, R) {
        let Some(list) = type_params else {
            return (Vec::new(), f());
        };

        if list.nodes.is_empty() {
            return (Vec::new(), f());
        }

        self.push_type_param_scope();
        let params = self.collect_type_parameters(list);
        let result = f();
        self.pop_type_param_scope();

        (params, result)
    }

    fn collect_type_parameters(&self, list: &NodeList) -> Vec<TypeParamInfo> {
        let mut params = Vec::with_capacity(list.nodes.len());
        for &idx in &list.nodes {
            if let Some(info) = self.lower_type_parameter(idx) {
                let type_id = self.interner.intern(TypeKey::TypeParameter(info.clone()));
                self.add_type_param_binding(info.name, type_id);
                params.push(info);
            }
        }
        params
    }

    fn lower_type_parameter(&self, node_idx: NodeIndex) -> Option<TypeParamInfo> {
        let node = self.arena.get(node_idx)?;
        let data = self.arena.get_type_parameter(node)?;

        let name = self
            .arena
            .get(data.name)
            .and_then(|name_node| self.arena.get_identifier(name_node))
            .map(|id_data| self.interner.intern_string(&id_data.escaped_text))
            .unwrap_or_else(|| self.interner.intern_string("T"));

        let constraint = if data.constraint != NodeIndex::NONE {
            Some(self.lower_type(data.constraint))
        } else {
            None
        };

        let default = if data.default != NodeIndex::NONE {
            Some(self.lower_type(data.default))
        } else {
            None
        };

        Some(TypeParamInfo {
            name,
            constraint,
            default,
        })
    }

    /// Extract a parameter name if it is an identifier.
    fn lower_parameter_name(&self, node_idx: NodeIndex) -> Option<crate::interner::Atom> {
        let node = self.arena.get(node_idx)?;
        self.arena
            .get_identifier(node)
            .map(|ident| self.interner.intern_string(&ident.escaped_text))
    }

    fn lower_params_with_this(&self, params: &NodeList) -> (Vec<ParamInfo>, Option<TypeId>) {
        let mut lowered = Vec::new();
        let mut this_type = None;

        for &idx in &params.nodes {
            let Some(param_node) = self.arena.get(idx) else {
                continue;
            };
            let Some(param_data) = self.arena.get_parameter(param_node) else {
                continue;
            };

            if let Some(name_node) = self.arena.get(param_data.name)
                && let Some(id_data) = self.arena.get_identifier(name_node)
                && id_data.escaped_text == "this"
            {
                if this_type.is_none() {
                    this_type = Some(self.lower_type(param_data.type_annotation));
                }
                continue;
            }

            lowered.push(ParamInfo {
                name: self.lower_parameter_name(param_data.name),
                type_id: self.lower_type(param_data.type_annotation),
                optional: param_data.question_token,
                rest: param_data.dot_dot_dot_token,
            });
        }

        (lowered, this_type)
    }

    fn lower_return_type(&self, node_idx: NodeIndex) -> (TypeId, Option<TypePredicate>) {
        if node_idx == NodeIndex::NONE {
            // Return ERROR for missing return type annotations to prevent "Any poisoning".
            // This forces explicit return type annotations and surfaces bugs early.
            // Per SOLVER.md Section 6.4: Error propagation prevents cascading noise.
            return (TypeId::ERROR, None);
        }

        let node = match self.arena.get(node_idx) {
            Some(n) => n,
            None => return (TypeId::ERROR, None),
        };

        if node.kind == syntax_kind_ext::TYPE_PREDICATE {
            return self.lower_type_predicate_return(node_idx);
        }

        (self.lower_type(node_idx), None)
    }

    /// Lower a function type ((a: T, b: U) => R)
    fn lower_function_type(&self, node_idx: NodeIndex) -> TypeId {
        let node = match self.arena.get(node_idx) {
            Some(n) => n,
            None => return TypeId::ERROR,
        };

        if let Some(data) = self.arena.get_function_type(node) {
            let (type_params, (params, this_type, return_type, type_predicate)) = self
                .with_type_params(&data.type_parameters, || {
                    let (params, this_type) = self.lower_params_with_this(&data.parameters);

                    let (return_type, type_predicate) =
                        self.lower_return_type(data.type_annotation);
                    (params, this_type, return_type, type_predicate)
                });

            let shape = FunctionShape {
                type_params,
                params,
                this_type,
                return_type,
                type_predicate,
                is_constructor: false,
                is_method: false,
            };

            self.interner.function(shape)
        } else {
            TypeId::ERROR
        }
    }

    /// Lower a type literal ({ x: T, y: U })
    fn lower_type_literal(&self, node_idx: NodeIndex) -> TypeId {
        let node = match self.arena.get(node_idx) {
            Some(n) => n,
            None => return TypeId::ERROR,
        };

        if let Some(data) = self.arena.get_type_literal(node) {
            let mut properties = Vec::new();
            let mut call_signatures = Vec::new();
            let mut construct_signatures = Vec::new();
            let mut string_index = None;
            let mut number_index = None;

            for &idx in &data.members.nodes {
                let Some(member) = self.arena.get(idx) else {
                    continue;
                };

                if let Some(sig) = self.arena.get_signature(member) {
                    match member.kind {
                        k if k == syntax_kind_ext::CALL_SIGNATURE => {
                            call_signatures.push(self.lower_call_signature(sig));
                        }
                        k if k == syntax_kind_ext::CONSTRUCT_SIGNATURE => {
                            construct_signatures.push(self.lower_call_signature(sig));
                        }
                        k if k == syntax_kind_ext::METHOD_SIGNATURE => {
                            if let Some(name) = self.lower_signature_name(sig.name) {
                                let type_id = self.lower_method_signature(sig);
                                properties.push(PropertyInfo {
                                    name,
                                    type_id,
                                    write_type: type_id,
                                    optional: sig.question_token,
                                    readonly: self.has_readonly_modifier(&sig.modifiers),
                                    is_method: true,
                                });
                            }
                        }
                        _ => {
                            if let Some(prop) = self.lower_type_element(idx) {
                                properties.push(prop);
                            }
                        }
                    }
                    continue;
                }

                if let Some(index_sig) = self.arena.get_index_signature(member)
                    && let Some(index_info) = self.lower_index_signature(index_sig)
                {
                    if index_info.key_type == TypeId::NUMBER {
                        number_index = Some(index_info);
                    } else {
                        string_index = Some(index_info);
                    }
                }
            }

            if !call_signatures.is_empty() || !construct_signatures.is_empty() {
                return self.interner.callable(CallableShape {
                    call_signatures,
                    construct_signatures,
                    properties,
                    string_index,
                    number_index,
                });
            }

            if string_index.is_some() || number_index.is_some() {
                if !self.index_signature_properties_compatible(
                    &properties,
                    string_index.as_ref(),
                    number_index.as_ref(),
                ) {
                    return TypeId::ERROR;
                }
                return self.interner.object_with_index(ObjectShape {
                    properties,
                    string_index,
                    number_index,
                });
            }

            self.interner.object(properties)
        } else {
            self.interner.object(vec![])
        }
    }

    pub fn lower_interface_declarations(&self, declarations: &[NodeIndex]) -> TypeId {
        if declarations.is_empty() {
            return TypeId::ERROR;
        }

        let mut parts = InterfaceParts::new();
        let mut type_params: Option<&NodeList> = None;
        let mut found = false;

        for &decl_idx in declarations {
            let Some(node) = self.arena.get(decl_idx) else {
                continue;
            };
            let Some(interface) = self.arena.get_interface(node) else {
                continue;
            };
            found = true;
            if type_params.is_none() {
                type_params = interface.type_parameters.as_ref();
            }
        }

        if !found {
            return TypeId::ERROR;
        }

        if let Some(params) = type_params {
            self.push_type_param_scope();
            let _ = self.collect_type_parameters(params);
        }

        for &decl_idx in declarations {
            let Some(node) = self.arena.get(decl_idx) else {
                continue;
            };
            let Some(interface) = self.arena.get_interface(node) else {
                continue;
            };
            self.collect_interface_members(&interface.members, &mut parts);
        }

        if type_params.is_some() {
            self.pop_type_param_scope();
        }

        self.finish_interface_parts(parts)
    }

    pub fn lower_type_alias_declaration(&self, alias: &TypeAliasData) -> TypeId {
        if let Some(params) = alias.type_parameters.as_ref()
            && !params.nodes.is_empty()
        {
            self.push_type_param_scope();
            let _ = self.collect_type_parameters(params);
            let result = self.lower_type(alias.type_node);
            self.pop_type_param_scope();
            return result;
        }

        self.lower_type(alias.type_node)
    }

    fn collect_interface_members(&self, members: &NodeList, parts: &mut InterfaceParts) {
        for &idx in &members.nodes {
            let Some(member) = self.arena.get(idx) else {
                continue;
            };

            if let Some(sig) = self.arena.get_signature(member) {
                match member.kind {
                    k if k == syntax_kind_ext::CALL_SIGNATURE => {
                        parts.call_signatures.push(self.lower_call_signature(sig));
                    }
                    k if k == syntax_kind_ext::CONSTRUCT_SIGNATURE => {
                        parts
                            .construct_signatures
                            .push(self.lower_call_signature(sig));
                    }
                    k if k == syntax_kind_ext::METHOD_SIGNATURE => {
                        if let Some(name) = self.lower_signature_name(sig.name) {
                            let signature = self.lower_call_signature(sig);
                            let readonly = self.has_readonly_modifier(&sig.modifiers);
                            parts.merge_method(name, signature, sig.question_token, readonly);
                        }
                    }
                    _ => {
                        if let Some(prop) = self.lower_type_element(idx) {
                            parts.merge_property(prop);
                        }
                    }
                }
                continue;
            }

            if let Some(index_sig) = self.arena.get_index_signature(member)
                && let Some(index_info) = self.lower_index_signature(index_sig)
            {
                parts.merge_index_signature(index_info);
            }
        }
    }

    fn finish_interface_parts(&self, parts: InterfaceParts) -> TypeId {
        let mut properties = Vec::with_capacity(parts.properties.len());
        for (name, entry) in parts.properties {
            match entry {
                PropertyMerge::Property(prop) => properties.push(prop),
                PropertyMerge::Method(methods) => {
                    let type_id = self.interner.callable(CallableShape {
                        call_signatures: methods.signatures,
                        construct_signatures: Vec::new(),
                        properties: Vec::new(),
                        ..Default::default()
                    });
                    properties.push(PropertyInfo {
                        name,
                        type_id,
                        write_type: type_id,
                        optional: methods.optional,
                        readonly: methods.readonly,
                        is_method: true,
                    });
                }
                PropertyMerge::Conflict(prop) => properties.push(prop),
            }
        }

        if !parts.call_signatures.is_empty() || !parts.construct_signatures.is_empty() {
            return self.interner.callable(CallableShape {
                call_signatures: parts.call_signatures,
                construct_signatures: parts.construct_signatures,
                properties,
                string_index: parts.string_index,
                number_index: parts.number_index,
            });
        }

        if parts.string_index.is_some() || parts.number_index.is_some() {
            if !self.index_signature_properties_compatible(
                &properties,
                parts.string_index.as_ref(),
                parts.number_index.as_ref(),
            ) {
                return TypeId::ERROR;
            }
            return self.interner.object_with_index(ObjectShape {
                properties,
                string_index: parts.string_index,
                number_index: parts.number_index,
            });
        }

        self.interner.object(properties)
    }

    fn lower_call_signature(&self, sig: &SignatureData) -> CallSignature {
        let (type_params, (params, this_type, return_type, type_predicate)) = self
            .with_type_params(&sig.type_parameters, || {
                let (params, this_type) = self.lower_signature_params(sig);
                let (return_type, type_predicate) = self.lower_return_type(sig.type_annotation);
                (params, this_type, return_type, type_predicate)
            });

        CallSignature {
            type_params,
            params,
            this_type,
            return_type,
            type_predicate,
        }
    }

    fn lower_method_signature(&self, sig: &SignatureData) -> TypeId {
        let (type_params, (params, this_type, return_type, type_predicate)) = self
            .with_type_params(&sig.type_parameters, || {
                let (params, this_type) = self.lower_signature_params(sig);
                let (return_type, type_predicate) = self.lower_return_type(sig.type_annotation);
                (params, this_type, return_type, type_predicate)
            });

        self.interner.function(FunctionShape {
            type_params,
            params,
            this_type,
            return_type,
            type_predicate,
            is_constructor: false,
            is_method: true,
        })
    }

    fn lower_signature_params(&self, sig: &SignatureData) -> (Vec<ParamInfo>, Option<TypeId>) {
        let Some(params) = &sig.parameters else {
            return (Vec::new(), None);
        };
        self.lower_params_with_this(params)
    }

    fn lower_signature_name(&self, node_idx: NodeIndex) -> Option<Atom> {
        let node = self.arena.get(node_idx)?;
        if let Some(id_data) = self.arena.get_identifier(node) {
            return Some(self.interner.intern_string(&id_data.escaped_text));
        }
        if let Some(lit_data) = self.arena.get_literal(node)
            && !lit_data.text.is_empty()
        {
            return Some(self.interner.intern_string(&lit_data.text));
        }
        None
    }

    fn lower_index_signature(&self, sig: &IndexSignatureData) -> Option<IndexSignature> {
        let param_idx = sig
            .parameters
            .nodes
            .first()
            .copied()
            .unwrap_or(NodeIndex::NONE);
        let param_node = self.arena.get(param_idx)?;
        let param_data = self.arena.get_parameter(param_node)?;
        let key_type = self.lower_type(param_data.type_annotation);
        let value_type = self.lower_type(sig.type_annotation);
        let readonly = self.has_readonly_modifier(&sig.modifiers);

        Some(IndexSignature {
            key_type,
            value_type,
            readonly,
        })
    }

    fn index_signature_properties_compatible(
        &self,
        properties: &[PropertyInfo],
        string_index: Option<&IndexSignature>,
        number_index: Option<&IndexSignature>,
    ) -> bool {
        if string_index.is_none() && number_index.is_none() {
            return true;
        }

        let skip_string = string_index
            .map(|idx| self.contains_meta_type(idx.value_type))
            .unwrap_or(false);
        let skip_number = number_index
            .map(|idx| self.contains_meta_type(idx.value_type))
            .unwrap_or(false);

        let resolver = IndexSignatureResolver;
        let mut checker = SubtypeChecker::with_resolver(self.interner, &resolver);

        for prop in properties {
            let prop_type = if prop.optional {
                self.interner.union2(prop.type_id, TypeId::UNDEFINED)
            } else {
                prop.type_id
            };

            if self.contains_meta_type(prop_type) {
                continue;
            }

            if let Some(number_idx) = number_index
                && !skip_number
            {
                let prop_name = self.interner.resolve_atom_ref(prop.name);
                let is_numeric = prop_name.as_ref().parse::<f64>().is_ok();
                if is_numeric && !checker.is_subtype_of(prop_type, number_idx.value_type) {
                    return false;
                }
            }

            if let Some(string_idx) = string_index
                && !skip_string
                && !checker.is_subtype_of(prop_type, string_idx.value_type)
            {
                return false;
            }
        }

        true
    }

    fn contains_meta_type(&self, type_id: TypeId) -> bool {
        let mut visited = FxHashSet::default();
        self.contains_meta_type_inner(type_id, &mut visited)
    }

    fn contains_meta_type_inner(&self, type_id: TypeId, visited: &mut FxHashSet<TypeId>) -> bool {
        if !visited.insert(type_id) {
            return false;
        }

        let key = match self.interner.lookup(type_id) {
            Some(key) => key,
            None => return false,
        };

        match key {
            TypeKey::TypeParameter(_)
            | TypeKey::Infer(_)
            | TypeKey::ThisType
            | TypeKey::TypeQuery(_)
            | TypeKey::Conditional(_)
            | TypeKey::Mapped(_)
            | TypeKey::IndexAccess(_, _)
            | TypeKey::KeyOf(_) => true,
            TypeKey::Union(members) | TypeKey::Intersection(members) => {
                let members = self.interner.type_list(members);
                members
                    .iter()
                    .any(|member| self.contains_meta_type_inner(*member, visited))
            }
            TypeKey::Array(elem) => self.contains_meta_type_inner(elem, visited),
            TypeKey::Tuple(elements) => {
                let elements = self.interner.tuple_list(elements);
                elements
                    .iter()
                    .any(|elem| self.contains_meta_type_inner(elem.type_id, visited))
            }
            TypeKey::Object(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                shape
                    .properties
                    .iter()
                    .any(|prop| self.contains_meta_type_inner(prop.type_id, visited))
            }
            TypeKey::ObjectWithIndex(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                if shape
                    .properties
                    .iter()
                    .any(|prop| self.contains_meta_type_inner(prop.type_id, visited))
                {
                    return true;
                }
                if let Some(index) = &shape.string_index
                    && (self.contains_meta_type_inner(index.value_type, visited)
                        || self.contains_meta_type_inner(index.key_type, visited))
                {
                    return true;
                }
                if let Some(index) = &shape.number_index
                    && (self.contains_meta_type_inner(index.value_type, visited)
                        || self.contains_meta_type_inner(index.key_type, visited))
                {
                    return true;
                }
                false
            }
            TypeKey::Function(shape_id) => {
                let shape = self.interner.function_shape(shape_id);
                if shape
                    .params
                    .iter()
                    .any(|param| self.contains_meta_type_inner(param.type_id, visited))
                {
                    return true;
                }
                if self.contains_meta_type_inner(shape.return_type, visited) {
                    return true;
                }
                for param in &shape.type_params {
                    if let Some(constraint) = param.constraint
                        && self.contains_meta_type_inner(constraint, visited)
                    {
                        return true;
                    }
                    if let Some(default) = param.default
                        && self.contains_meta_type_inner(default, visited)
                    {
                        return true;
                    }
                }
                false
            }
            TypeKey::Callable(shape_id) => {
                let shape = self.interner.callable_shape(shape_id);
                for sig in &shape.call_signatures {
                    if sig
                        .params
                        .iter()
                        .any(|param| self.contains_meta_type_inner(param.type_id, visited))
                    {
                        return true;
                    }
                    if self.contains_meta_type_inner(sig.return_type, visited) {
                        return true;
                    }
                    for param in &sig.type_params {
                        if let Some(constraint) = param.constraint
                            && self.contains_meta_type_inner(constraint, visited)
                        {
                            return true;
                        }
                        if let Some(default) = param.default
                            && self.contains_meta_type_inner(default, visited)
                        {
                            return true;
                        }
                    }
                }
                for sig in &shape.construct_signatures {
                    if sig
                        .params
                        .iter()
                        .any(|param| self.contains_meta_type_inner(param.type_id, visited))
                    {
                        return true;
                    }
                    if self.contains_meta_type_inner(sig.return_type, visited) {
                        return true;
                    }
                    for param in &sig.type_params {
                        if let Some(constraint) = param.constraint
                            && self.contains_meta_type_inner(constraint, visited)
                        {
                            return true;
                        }
                        if let Some(default) = param.default
                            && self.contains_meta_type_inner(default, visited)
                        {
                            return true;
                        }
                    }
                }
                shape
                    .properties
                    .iter()
                    .any(|prop| self.contains_meta_type_inner(prop.type_id, visited))
            }
            TypeKey::Application(app_id) => {
                let app = self.interner.type_application(app_id);
                if self.contains_meta_type_inner(app.base, visited) {
                    return true;
                }
                app.args
                    .iter()
                    .any(|arg| self.contains_meta_type_inner(*arg, visited))
            }
            TypeKey::ReadonlyType(inner) => self.contains_meta_type_inner(inner, visited),
            TypeKey::TemplateLiteral(spans) => {
                let spans = self.interner.template_list(spans);
                spans.iter().any(|span| match span {
                    TemplateSpan::Text(_) => false,
                    TemplateSpan::Type(inner) => self.contains_meta_type_inner(*inner, visited),
                })
            }
            TypeKey::StringIntrinsic { type_arg, .. } => {
                self.contains_meta_type_inner(type_arg, visited)
            }
            TypeKey::Ref(_)
            | TypeKey::Intrinsic(_)
            | TypeKey::Literal(_)
            | TypeKey::UniqueSymbol(_)
            | TypeKey::Error => false,
        }
    }

    /// Lower a type element (property signature, method signature, etc.)
    fn lower_type_element(&self, node_idx: NodeIndex) -> Option<PropertyInfo> {
        let node = self.arena.get(node_idx)?;

        // Check if it's a property or method signature
        if let Some(sig) = self.arena.get_signature(node) {
            // Get property name as Arc<str>
            let name = self.lower_signature_name(sig.name)?;

            // Check for readonly modifier
            let readonly = self.has_readonly_modifier(&sig.modifiers);

            Some(PropertyInfo {
                name,
                type_id: self.lower_type(sig.type_annotation),
                write_type: self.lower_type(sig.type_annotation),
                optional: sig.question_token,
                readonly,
                is_method: false,
            })
        } else {
            None
        }
    }

    /// Check if a modifiers list contains a readonly keyword
    fn has_readonly_modifier(&self, modifiers: &Option<NodeList>) -> bool {
        use crate::scanner::SyntaxKind;

        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.arena.get(mod_idx)
                    && mod_node.kind == SyntaxKind::ReadonlyKeyword as u16
                {
                    return true;
                }
            }
        }
        false
    }

    /// Lower a conditional type (T extends U ? X : Y)
    fn lower_conditional_type(&self, node_idx: NodeIndex) -> TypeId {
        let node = match self.arena.get(node_idx) {
            Some(n) => n,
            None => return TypeId::ERROR,
        };

        if let Some(data) = self.arena.get_conditional_type(node) {
            let is_distributive = self.is_naked_type_param(data.check_type);
            let check_type = self.lower_type(data.check_type);
            let extends_type = self.lower_type(data.extends_type);

            self.push_type_param_scope();
            let mut visited = FxHashSet::default();
            self.collect_infer_bindings(extends_type, &mut visited);
            let true_type = self.lower_type(data.true_type);
            let false_type = self.lower_type(data.false_type);
            self.pop_type_param_scope();

            let cond = ConditionalType {
                check_type,
                extends_type,
                true_type,
                false_type,
                is_distributive,
            };
            self.interner.conditional(cond)
        } else {
            TypeId::ERROR
        }
    }

    fn is_naked_type_param(&self, node_idx: NodeIndex) -> bool {
        let mut current = node_idx;
        let mut iterations = 0;
        loop {
            iterations += 1;
            if iterations > Self::MAX_TREE_WALK_ITERATIONS {
                // Safety limit reached - return false to prevent infinite loop
                return false;
            }
            let Some(node) = self.arena.get(current) else {
                return false;
            };
            match node.kind {
                k if k == syntax_kind_ext::PARENTHESIZED_TYPE => {
                    if let Some(data) = self.arena.get_wrapped_type(node) {
                        current = data.type_node;
                        continue;
                    }
                    return false;
                }
                k if k == syntax_kind_ext::TYPE_REFERENCE => {
                    let Some(data) = self.arena.get_type_ref(node) else {
                        return false;
                    };
                    if let Some(args) = &data.type_arguments
                        && !args.nodes.is_empty()
                    {
                        return false;
                    }
                    let Some(name_node) = self.arena.get(data.type_name) else {
                        return false;
                    };
                    if let Some(ident) = self.arena.get_identifier(name_node) {
                        return self.lookup_type_param(&ident.escaped_text).is_some();
                    }
                    return false;
                }
                k if k == SyntaxKind::Identifier as u16 => {
                    let Some(ident) = self.arena.get_identifier(node) else {
                        return false;
                    };
                    return self.lookup_type_param(&ident.escaped_text).is_some();
                }
                _ => return false,
            }
        }
    }

    fn collect_infer_bindings(&self, type_id: TypeId, visited: &mut FxHashSet<TypeId>) {
        if !visited.insert(type_id) {
            return;
        }

        let key = match self.interner.lookup(type_id) {
            Some(key) => key,
            None => return,
        };

        match key {
            TypeKey::Infer(info) => {
                self.add_type_param_binding(info.name, type_id);
                if let Some(constraint) = info.constraint {
                    self.collect_infer_bindings(constraint, visited);
                }
                if let Some(default) = info.default {
                    self.collect_infer_bindings(default, visited);
                }
            }
            TypeKey::Array(elem) => self.collect_infer_bindings(elem, visited),
            TypeKey::Tuple(elements) => {
                let elements = self.interner.tuple_list(elements);
                for element in elements.iter() {
                    self.collect_infer_bindings(element.type_id, visited);
                }
            }
            TypeKey::Union(members) | TypeKey::Intersection(members) => {
                let members = self.interner.type_list(members);
                for member in members.iter() {
                    self.collect_infer_bindings(*member, visited);
                }
            }
            TypeKey::Object(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                for prop in shape.properties.iter() {
                    self.collect_infer_bindings(prop.type_id, visited);
                }
            }
            TypeKey::ObjectWithIndex(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                for prop in &shape.properties {
                    self.collect_infer_bindings(prop.type_id, visited);
                }
                if let Some(index) = &shape.string_index {
                    self.collect_infer_bindings(index.key_type, visited);
                    self.collect_infer_bindings(index.value_type, visited);
                }
                if let Some(index) = &shape.number_index {
                    self.collect_infer_bindings(index.key_type, visited);
                    self.collect_infer_bindings(index.value_type, visited);
                }
            }
            TypeKey::Function(shape_id) => {
                let shape = self.interner.function_shape(shape_id);
                for param in &shape.params {
                    self.collect_infer_bindings(param.type_id, visited);
                }
                self.collect_infer_bindings(shape.return_type, visited);
                for param in &shape.type_params {
                    if let Some(constraint) = param.constraint {
                        self.collect_infer_bindings(constraint, visited);
                    }
                    if let Some(default) = param.default {
                        self.collect_infer_bindings(default, visited);
                    }
                }
            }
            TypeKey::Callable(shape_id) => {
                let shape = self.interner.callable_shape(shape_id);
                for sig in &shape.call_signatures {
                    for param in &sig.params {
                        self.collect_infer_bindings(param.type_id, visited);
                    }
                    self.collect_infer_bindings(sig.return_type, visited);
                    for param in &sig.type_params {
                        if let Some(constraint) = param.constraint {
                            self.collect_infer_bindings(constraint, visited);
                        }
                        if let Some(default) = param.default {
                            self.collect_infer_bindings(default, visited);
                        }
                    }
                }
                for sig in &shape.construct_signatures {
                    for param in &sig.params {
                        self.collect_infer_bindings(param.type_id, visited);
                    }
                    self.collect_infer_bindings(sig.return_type, visited);
                    for param in &sig.type_params {
                        if let Some(constraint) = param.constraint {
                            self.collect_infer_bindings(constraint, visited);
                        }
                        if let Some(default) = param.default {
                            self.collect_infer_bindings(default, visited);
                        }
                    }
                }
                for prop in &shape.properties {
                    self.collect_infer_bindings(prop.type_id, visited);
                }
            }
            TypeKey::TypeParameter(info) => {
                if let Some(constraint) = info.constraint {
                    self.collect_infer_bindings(constraint, visited);
                }
                if let Some(default) = info.default {
                    self.collect_infer_bindings(default, visited);
                }
            }
            TypeKey::Application(app_id) => {
                let app = self.interner.type_application(app_id);
                self.collect_infer_bindings(app.base, visited);
                for &arg in &app.args {
                    self.collect_infer_bindings(arg, visited);
                }
            }
            TypeKey::Conditional(cond_id) => {
                let cond = self.interner.conditional_type(cond_id);
                self.collect_infer_bindings(cond.check_type, visited);
                self.collect_infer_bindings(cond.extends_type, visited);
                self.collect_infer_bindings(cond.true_type, visited);
                self.collect_infer_bindings(cond.false_type, visited);
            }
            TypeKey::Mapped(mapped_id) => {
                let mapped = self.interner.mapped_type(mapped_id);
                if let Some(constraint) = mapped.type_param.constraint {
                    self.collect_infer_bindings(constraint, visited);
                }
                if let Some(default) = mapped.type_param.default {
                    self.collect_infer_bindings(default, visited);
                }
                self.collect_infer_bindings(mapped.constraint, visited);
                if let Some(name_type) = mapped.name_type {
                    self.collect_infer_bindings(name_type, visited);
                }
                self.collect_infer_bindings(mapped.template, visited);
            }
            TypeKey::IndexAccess(obj, idx) => {
                self.collect_infer_bindings(obj, visited);
                self.collect_infer_bindings(idx, visited);
            }
            TypeKey::KeyOf(inner) | TypeKey::ReadonlyType(inner) => {
                self.collect_infer_bindings(inner, visited);
            }
            TypeKey::TemplateLiteral(spans) => {
                let spans = self.interner.template_list(spans);
                for span in spans.iter() {
                    if let TemplateSpan::Type(inner) = span {
                        self.collect_infer_bindings(*inner, visited);
                    }
                }
            }
            TypeKey::StringIntrinsic { type_arg, .. } => {
                self.collect_infer_bindings(type_arg, visited);
            }
            TypeKey::Intrinsic(_)
            | TypeKey::Literal(_)
            | TypeKey::Ref(_)
            | TypeKey::TypeQuery(_)
            | TypeKey::UniqueSymbol(_)
            | TypeKey::ThisType
            | TypeKey::Error => {}
        }
    }

    /// Lower a mapped type ({ [K in Keys]: ValueType })
    fn lower_mapped_type(&self, node_idx: NodeIndex) -> TypeId {
        let node = match self.arena.get(node_idx) {
            Some(n) => n,
            None => return TypeId::ERROR,
        };

        if let Some(data) = self.arena.get_mapped_type(node) {
            let (type_param, constraint) = self.lower_mapped_type_param(data.type_parameter);
            self.push_type_param_scope();
            let type_param_id = self
                .interner
                .intern(TypeKey::TypeParameter(type_param.clone()));
            self.add_type_param_binding(type_param.name, type_param_id);
            let name_type = if data.name_type != NodeIndex::NONE {
                Some(self.lower_type(data.name_type))
            } else {
                None
            };
            let template = self.lower_type(data.type_node);
            self.pop_type_param_scope();
            let mapped = MappedType {
                type_param,
                constraint,
                name_type,
                template,
                readonly_modifier: self
                    .lower_mapped_modifier(data.readonly_token, SyntaxKind::ReadonlyKeyword as u16),
                optional_modifier: self
                    .lower_mapped_modifier(data.question_token, SyntaxKind::QuestionToken as u16),
            };
            self.interner.mapped(mapped)
        } else {
            TypeId::ERROR
        }
    }

    fn lower_mapped_type_param(&self, node_idx: NodeIndex) -> (TypeParamInfo, TypeId) {
        let node = match self.arena.get(node_idx) {
            Some(n) => n,
            None => {
                let name = self.interner.intern_string("K");
                return (
                    TypeParamInfo {
                        name,
                        constraint: None,
                        default: None,
                    },
                    TypeId::ERROR, // Missing node - propagate error
                );
            }
        };

        if let Some(param_data) = self.arena.get_type_parameter(node) {
            let name = self
                .arena
                .get(param_data.name)
                .and_then(|ident_node| self.arena.get_identifier(ident_node))
                .map(|ident| self.interner.intern_string(&ident.escaped_text))
                .unwrap_or_else(|| self.interner.intern_string("K"));

            let constraint = if param_data.constraint != NodeIndex::NONE {
                Some(self.lower_type(param_data.constraint))
            } else {
                None
            };

            let default = if param_data.default != NodeIndex::NONE {
                Some(self.lower_type(param_data.default))
            } else {
                None
            };

            // Use Unknown instead of Any for stricter type checking
            // When a generic parameter has no constraint, use Unknown to prevent
            // invalid values from being accepted
            let constraint_type = constraint.unwrap_or(TypeId::UNKNOWN);

            (
                TypeParamInfo {
                    name,
                    constraint,
                    default,
                },
                constraint_type,
            )
        } else {
            let name = self.interner.intern_string("K");
            (
                TypeParamInfo {
                    name,
                    constraint: None,
                    default: None,
                },
                TypeId::ERROR, // Missing type parameter data - propagate error
            )
        }
    }

    fn lower_mapped_modifier(
        &self,
        token_idx: NodeIndex,
        default_kind: u16,
    ) -> Option<MappedModifier> {
        use crate::scanner::SyntaxKind;

        if token_idx == NodeIndex::NONE {
            return None;
        }

        let kind = self.arena.get(token_idx).map(|node| node.kind)?;
        if kind == SyntaxKind::PlusToken as u16 || kind == default_kind {
            Some(MappedModifier::Add)
        } else if kind == SyntaxKind::MinusToken as u16 {
            Some(MappedModifier::Remove)
        } else {
            None
        }
    }

    /// Lower an indexed access type (T[K])
    fn lower_indexed_access_type(&self, node_idx: NodeIndex) -> TypeId {
        let node = match self.arena.get(node_idx) {
            Some(n) => n,
            None => return TypeId::ERROR,
        };

        if let Some(data) = self.arena.get_indexed_access_type(node) {
            let object_type = self.lower_type(data.object_type);
            let index_type = self.lower_type(data.index_type);
            self.interner
                .intern(TypeKey::IndexAccess(object_type, index_type))
        } else {
            TypeId::ERROR
        }
    }

    fn strip_numeric_separators<'b>(text: &'b str) -> std::borrow::Cow<'b, str> {
        if !text.as_bytes().contains(&b'_') {
            return std::borrow::Cow::Borrowed(text);
        }

        let mut out = String::with_capacity(text.len());
        for &byte in text.as_bytes() {
            if byte != b'_' {
                out.push(byte as char);
            }
        }
        std::borrow::Cow::Owned(out)
    }

    fn parse_numeric_literal_value(&self, value: Option<f64>, text: &str) -> Option<f64> {
        if let Some(value) = value {
            return Some(value);
        }

        if let Some(rest) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
            return Self::parse_radix_digits(rest, 16);
        }
        if let Some(rest) = text.strip_prefix("0b").or_else(|| text.strip_prefix("0B")) {
            return Self::parse_radix_digits(rest, 2);
        }
        if let Some(rest) = text.strip_prefix("0o").or_else(|| text.strip_prefix("0O")) {
            return Self::parse_radix_digits(rest, 8);
        }

        if text.as_bytes().contains(&b'_') {
            let cleaned = Self::strip_numeric_separators(text);
            return cleaned.as_ref().parse::<f64>().ok();
        }

        text.parse::<f64>().ok()
    }

    fn parse_radix_digits(text: &str, base: u32) -> Option<f64> {
        if text.is_empty() {
            return None;
        }

        let mut value = 0f64;
        let base_value = base as f64;
        let mut saw_digit = false;
        for &byte in text.as_bytes() {
            if byte == b'_' {
                continue;
            }

            let digit = match byte {
                b'0'..=b'9' => (byte - b'0') as u32,
                b'a'..=b'f' => (byte - b'a' + 10) as u32,
                b'A'..=b'F' => (byte - b'A' + 10) as u32,
                _ => return None,
            };
            if digit >= base {
                return None;
            }
            saw_digit = true;
            value = value * base_value + digit as f64;
        }

        if !saw_digit {
            return None;
        }

        Some(value)
    }

    fn normalize_bigint_literal<'b>(&self, text: &'b str) -> Option<std::borrow::Cow<'b, str>> {
        if let Some(rest) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
            return Self::bigint_base_to_decimal(rest, 16).map(std::borrow::Cow::Owned);
        }
        if let Some(rest) = text.strip_prefix("0b").or_else(|| text.strip_prefix("0B")) {
            return Self::bigint_base_to_decimal(rest, 2).map(std::borrow::Cow::Owned);
        }
        if let Some(rest) = text.strip_prefix("0o").or_else(|| text.strip_prefix("0O")) {
            return Self::bigint_base_to_decimal(rest, 8).map(std::borrow::Cow::Owned);
        }

        match Self::strip_numeric_separators(text) {
            std::borrow::Cow::Borrowed(cleaned) => {
                let trimmed = cleaned.trim_start_matches('0');
                if trimmed.is_empty() {
                    return Some(std::borrow::Cow::Borrowed("0"));
                }
                if trimmed.len() == cleaned.len() {
                    return Some(std::borrow::Cow::Borrowed(cleaned));
                }
                Some(std::borrow::Cow::Borrowed(trimmed))
            }
            std::borrow::Cow::Owned(mut cleaned) => {
                let cleaned_ref = cleaned.as_str();
                let trimmed = cleaned_ref.trim_start_matches('0');
                if trimmed.is_empty() {
                    return Some(std::borrow::Cow::Borrowed("0"));
                }
                if trimmed.len() == cleaned_ref.len() {
                    return Some(std::borrow::Cow::Owned(cleaned));
                }

                let trim_len = cleaned_ref.len() - trimmed.len();
                cleaned.drain(..trim_len);
                Some(std::borrow::Cow::Owned(cleaned))
            }
        }
    }

    fn bigint_base_to_decimal(text: &str, base: u32) -> Option<String> {
        if text.is_empty() {
            return None;
        }

        let mut digits: Vec<u8> = vec![0];
        let mut saw_digit = false;
        for &byte in text.as_bytes() {
            if byte == b'_' {
                continue;
            }

            let digit = match byte {
                b'0'..=b'9' => (byte - b'0') as u32,
                b'a'..=b'f' => (byte - b'a' + 10) as u32,
                b'A'..=b'F' => (byte - b'A' + 10) as u32,
                _ => return None,
            };
            if digit >= base {
                return None;
            }
            saw_digit = true;

            let mut carry = digit;
            for slot in &mut digits {
                let value = (*slot as u32) * base + carry;
                *slot = (value % 10) as u8;
                carry = value / 10;
            }
            while carry > 0 {
                digits.push((carry % 10) as u8);
                carry /= 10;
            }
        }

        if !saw_digit {
            return None;
        }

        while digits.len() > 1 && matches!(digits.last(), Some(&0)) {
            digits.pop();
        }

        let mut out = String::with_capacity(digits.len());
        for digit in digits.iter().rev() {
            out.push(char::from(b'0' + *digit));
        }
        Some(out)
    }

    /// Lower a literal type ("foo", 42, etc.)
    fn lower_literal_type(&self, node_idx: NodeIndex) -> TypeId {
        let node = match self.arena.get(node_idx) {
            Some(n) => n,
            None => return TypeId::ERROR,
        };

        if let Some(data) = self.arena.get_literal_type(node) {
            // The literal node contains the actual literal value
            if let Some(literal_node) = self.arena.get(data.literal) {
                match literal_node.kind {
                    k if k == SyntaxKind::StringLiteral as u16 => {
                        if let Some(lit_data) = self.arena.get_literal(literal_node) {
                            self.interner.literal_string(&lit_data.text)
                        } else {
                            TypeId::STRING
                        }
                    }
                    k if k == SyntaxKind::NumericLiteral as u16 => {
                        if let Some(lit_data) = self.arena.get_literal(literal_node) {
                            if let Some(value) =
                                self.parse_numeric_literal_value(lit_data.value, &lit_data.text)
                            {
                                self.interner.literal_number(value)
                            } else {
                                TypeId::NUMBER
                            }
                        } else {
                            TypeId::NUMBER
                        }
                    }
                    k if k == SyntaxKind::BigIntLiteral as u16 => {
                        if let Some(lit_data) = self.arena.get_literal(literal_node) {
                            let text = lit_data.text.strip_suffix('n').unwrap_or(&lit_data.text);
                            if let Some(normalized) = self.normalize_bigint_literal(text) {
                                self.interner.literal_bigint(normalized.as_ref())
                            } else {
                                TypeId::BIGINT
                            }
                        } else {
                            TypeId::BIGINT
                        }
                    }
                    k if k == SyntaxKind::TrueKeyword as u16 => self.interner.literal_boolean(true),
                    k if k == SyntaxKind::FalseKeyword as u16 => {
                        self.interner.literal_boolean(false)
                    }
                    k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                        if let Some(unary) = self.arena.get_unary_expr(literal_node) {
                            let op = unary.operator;
                            let Some(operand_node) = self.arena.get(unary.operand) else {
                                return TypeId::ERROR; // Propagate error for missing operand
                            };
                            match operand_node.kind {
                                k if k == SyntaxKind::NumericLiteral as u16 => {
                                    if let Some(lit_data) = self.arena.get_literal(operand_node) {
                                        if let Some(value) = self.parse_numeric_literal_value(
                                            lit_data.value,
                                            &lit_data.text,
                                        ) {
                                            let value = if op == SyntaxKind::MinusToken as u16 {
                                                -value
                                            } else {
                                                value
                                            };
                                            self.interner.literal_number(value)
                                        } else {
                                            TypeId::NUMBER
                                        }
                                    } else {
                                        TypeId::NUMBER
                                    }
                                }
                                k if k == SyntaxKind::BigIntLiteral as u16 => {
                                    if let Some(lit_data) = self.arena.get_literal(operand_node) {
                                        let text = lit_data
                                            .text
                                            .strip_suffix('n')
                                            .unwrap_or(&lit_data.text);
                                        let negative = op == SyntaxKind::MinusToken as u16;
                                        if let Some(normalized) =
                                            self.normalize_bigint_literal(text)
                                        {
                                            self.interner.literal_bigint_with_sign(
                                                negative,
                                                normalized.as_ref(),
                                            )
                                        } else {
                                            TypeId::BIGINT
                                        }
                                    } else {
                                        TypeId::BIGINT
                                    }
                                }
                                _ => TypeId::ERROR, // Propagate error for unknown operand kind
                            }
                        } else {
                            TypeId::ERROR // Propagate error for missing unary expression data
                        }
                    }
                    _ => TypeId::ERROR, // Propagate error for unknown literal kind
                }
            } else {
                TypeId::ERROR // Propagate error for missing literal node
            }
        } else {
            TypeId::ERROR // Propagate error for missing literal type data
        }
    }

    /// Lower a type reference (NamedType or NamedType<Args>)
    fn lower_type_reference(&self, node_idx: NodeIndex) -> TypeId {
        let node = match self.arena.get(node_idx) {
            Some(n) => n,
            None => return TypeId::ERROR,
        };

        if let Some(data) = self.arena.get_type_ref(node) {
            if let Some(name_node) = self.arena.get(data.type_name)
                && let Some(ident) = self.arena.get_identifier(name_node)
            {
                let name = ident.escaped_text.as_str();

                // Handle string manipulation intrinsic types
                if self.lookup_type_param(name).is_none()
                    && self.resolve_type_symbol(data.type_name).is_none()
                {
                    match name {
                        "Array" | "ReadonlyArray" => {
                            // Use Unknown instead of Any for stricter type checking
                            // Array/ReadonlyArray without type arguments defaults to unknown[]
                            // instead of any[] to prevent implicit any
                            let elem_type = data
                                .type_arguments
                                .as_ref()
                                .and_then(|args| args.nodes.first().copied())
                                .map(|idx| self.lower_type(idx))
                                .unwrap_or(TypeId::UNKNOWN);
                            let array_type = self.interner.array(elem_type);
                            if name == "ReadonlyArray" {
                                return self.interner.intern(TypeKey::ReadonlyType(array_type));
                            }
                            return array_type;
                        }
                        "Uppercase" => {
                            if let Some(args) = &data.type_arguments
                                && let Some(&first_arg) = args.nodes.first()
                            {
                                let type_arg = self.lower_type(first_arg);
                                return self.interner.intern(TypeKey::StringIntrinsic {
                                    kind: crate::solver::types::StringIntrinsicKind::Uppercase,
                                    type_arg,
                                });
                            }
                            return TypeId::ERROR;
                        }
                        "Lowercase" => {
                            if let Some(args) = &data.type_arguments
                                && let Some(&first_arg) = args.nodes.first()
                            {
                                let type_arg = self.lower_type(first_arg);
                                return self.interner.intern(TypeKey::StringIntrinsic {
                                    kind: crate::solver::types::StringIntrinsicKind::Lowercase,
                                    type_arg,
                                });
                            }
                            return TypeId::ERROR;
                        }
                        "Capitalize" => {
                            if let Some(args) = &data.type_arguments
                                && let Some(&first_arg) = args.nodes.first()
                            {
                                let type_arg = self.lower_type(first_arg);
                                return self.interner.intern(TypeKey::StringIntrinsic {
                                    kind: crate::solver::types::StringIntrinsicKind::Capitalize,
                                    type_arg,
                                });
                            }
                            return TypeId::ERROR;
                        }
                        "Uncapitalize" => {
                            if let Some(args) = &data.type_arguments
                                && let Some(&first_arg) = args.nodes.first()
                            {
                                let type_arg = self.lower_type(first_arg);
                                return self.interner.intern(TypeKey::StringIntrinsic {
                                    kind: crate::solver::types::StringIntrinsicKind::Uncapitalize,
                                    type_arg,
                                });
                            }
                            return TypeId::ERROR;
                        }
                        _ => {}
                    }
                }
            }

            // For now, just lower the type name as an identifier
            let base_type = self.lower_type(data.type_name);
            if let Some(args) = &data.type_arguments
                && !args.nodes.is_empty()
            {
                let type_args: Vec<TypeId> =
                    args.nodes.iter().map(|&idx| self.lower_type(idx)).collect();
                return self.interner.application(base_type, type_args);
            }
            base_type
        } else {
            TypeId::ERROR
        }
    }

    /// Lower a qualified name type (A.B).
    fn lower_qualified_name_type(&self, node_idx: NodeIndex) -> TypeId {
        if let Some(symbol_id) = self.resolve_type_symbol(node_idx) {
            return self.interner.reference(SymbolRef(symbol_id));
        }
        TypeId::ERROR
    }

    /// Lower an identifier as a type (simple type reference)
    fn lower_identifier_type(&self, node_idx: NodeIndex) -> TypeId {
        let node = match self.arena.get(node_idx) {
            Some(n) => n,
            None => return TypeId::ERROR,
        };

        if let Some(data) = self.arena.get_identifier(node) {
            let name = &data.escaped_text;

            if let Some(type_param) = self.lookup_type_param(name) {
                return type_param;
            }

            if let Some(symbol_id) = self.resolve_type_symbol(node_idx) {
                return self.interner.reference(SymbolRef(symbol_id));
            }

            // Check for built-in type names only if not resolved (shadowing-safe)
            match name.as_ref() {
                "any" => return TypeId::ANY,
                "unknown" => return TypeId::UNKNOWN,
                "never" => return TypeId::NEVER,
                "void" => return TypeId::VOID,
                "undefined" => return TypeId::UNDEFINED,
                "null" => return TypeId::NULL,
                "boolean" => return TypeId::BOOLEAN,
                "number" => return TypeId::NUMBER,
                "string" => return TypeId::STRING,
                "bigint" => return TypeId::BIGINT,
                "symbol" => return TypeId::SYMBOL,
                "object" => return TypeId::OBJECT,
                _ => {}
            }

            TypeId::ERROR
        } else {
            TypeId::ERROR
        }
    }

    /// Lower a parenthesized type
    fn lower_parenthesized_type(&self, node_idx: NodeIndex) -> TypeId {
        let node = match self.arena.get(node_idx) {
            Some(n) => n,
            None => return TypeId::ERROR,
        };

        // Parenthesized types just wrap another type
        if let Some(data) = self.arena.get_wrapped_type(node) {
            self.lower_type(data.type_node)
        } else {
            TypeId::ERROR
        }
    }

    /// Lower a type query (typeof expr in type position)
    fn lower_type_query(&self, node_idx: NodeIndex) -> TypeId {
        let node = match self.arena.get(node_idx) {
            Some(n) => n,
            None => return TypeId::ERROR,
        };

        if let Some(data) = self.arena.get_type_query(node) {
            // Create a symbol reference from the expression name
            if let Some(symbol_id) = self.resolve_value_symbol(data.expr_name) {
                let base = self
                    .interner
                    .intern(TypeKey::TypeQuery(SymbolRef(symbol_id)));
                if let Some(args) = &data.type_arguments
                    && !args.nodes.is_empty()
                {
                    let type_args: Vec<TypeId> =
                        args.nodes.iter().map(|&idx| self.lower_type(idx)).collect();
                    return self.interner.application(base, type_args);
                }
                return base;
            }
            TypeId::ERROR
        } else {
            TypeId::ERROR
        }
    }

    /// Lower a type operator (keyof, readonly, unique)
    fn lower_type_operator(&self, node_idx: NodeIndex) -> TypeId {
        let node = match self.arena.get(node_idx) {
            Some(n) => n,
            None => return TypeId::ERROR,
        };

        if let Some(data) = self.arena.get_type_operator(node) {
            let inner_type = self.lower_type(data.type_node);

            // Check which operator it is
            match data.operator {
                // KeyOfKeyword = 143
                143 => self.interner.intern(TypeKey::KeyOf(inner_type)),
                // ReadonlyKeyword = 148
                148 => self.interner.intern(TypeKey::ReadonlyType(inner_type)),
                // UniqueKeyword = 158 - unique symbol
                158 => {
                    // unique symbol creates a unique symbol type
                    // Use node index as unique identifier
                    self.interner
                        .intern(TypeKey::UniqueSymbol(SymbolRef(node_idx.0)))
                }
                _ => inner_type,
            }
        } else {
            TypeId::ERROR
        }
    }

    fn lower_type_predicate(&self, node_idx: NodeIndex) -> TypeId {
        self.lower_type_predicate_return(node_idx).0
    }

    fn lower_type_predicate_target(&self, node_idx: NodeIndex) -> Option<TypePredicateTarget> {
        let node = self.arena.get(node_idx)?;
        if node.kind == SyntaxKind::ThisKeyword as u16 || node.kind == syntax_kind_ext::THIS_TYPE {
            return Some(TypePredicateTarget::This);
        }

        self.arena.get_identifier(node).map(|ident| {
            TypePredicateTarget::Identifier(self.interner.intern_string(&ident.escaped_text))
        })
    }

    fn lower_type_predicate_return(&self, node_idx: NodeIndex) -> (TypeId, Option<TypePredicate>) {
        let node = match self.arena.get(node_idx) {
            Some(n) => n,
            None => return (TypeId::ERROR, None),
        };

        let Some(data) = self.arena.get_type_predicate(node) else {
            return (TypeId::BOOLEAN, None);
        };

        let return_type = if data.asserts_modifier {
            TypeId::VOID
        } else {
            TypeId::BOOLEAN
        };

        let target = match self.lower_type_predicate_target(data.parameter_name) {
            Some(target) => target,
            None => return (return_type, None),
        };

        let type_id = if data.type_node != NodeIndex::NONE {
            Some(self.lower_type(data.type_node))
        } else {
            None
        };

        let predicate = TypePredicate {
            asserts: data.asserts_modifier,
            target,
            type_id,
        };

        (return_type, Some(predicate))
    }

    /// Lower an infer type (infer R)
    fn lower_infer_type(&self, node_idx: NodeIndex) -> TypeId {
        let node = match self.arena.get(node_idx) {
            Some(n) => n,
            None => return TypeId::ERROR,
        };

        if let Some(data) = self.arena.get_infer_type(node) {
            if let Some(info) = self.lower_type_parameter(data.type_parameter) {
                return self.interner.intern(TypeKey::Infer(info));
            }

            // Fallback: synthesize a name if the node isn't a type parameter.
            let name = if let Some(tp_node) = self.arena.get(data.type_parameter) {
                if let Some(id_data) = self.arena.get_identifier(tp_node) {
                    self.interner.intern_string(&id_data.escaped_text)
                } else {
                    self.interner.intern_string("infer")
                }
            } else {
                self.interner.intern_string("infer")
            };

            self.interner.intern(TypeKey::Infer(TypeParamInfo {
                name,
                constraint: None,
                default: None,
            }))
        } else {
            TypeId::ERROR
        }
    }

    /// Lower a template literal type (`hello${T}world`)
    fn lower_template_literal_type(&self, node_idx: NodeIndex) -> TypeId {
        let node = match self.arena.get(node_idx) {
            Some(n) => n,
            None => return TypeId::ERROR,
        };

        if let Some(data) = self.arena.get_template_literal_type(node) {
            let mut spans: Vec<TemplateSpan> = Vec::new();

            // Add the head text if present, processing escape sequences
            if let Some(head_node) = self.arena.get(data.head)
                && let Some(head_lit) = self.arena.get_literal(head_node)
                && !head_lit.text.is_empty()
            {
                let processed =
                    crate::solver::types::process_template_escape_sequences(&head_lit.text);
                spans.push(TemplateSpan::Text(self.interner.intern_string(&processed)));
            }

            // Add template spans (type + text pairs)
            for &span_idx in &data.template_spans.nodes {
                if let Some(span_node) = self.arena.get(span_idx)
                    && span_node.kind == syntax_kind_ext::TEMPLATE_LITERAL_TYPE_SPAN
                    && let Some(span_data) =
                        self.arena.template_spans.get(span_node.data_index as usize)
                {
                    let type_id = self.lower_type(span_data.expression);
                    spans.push(TemplateSpan::Type(type_id));

                    if let Some(lit_node) = self.arena.get(span_data.literal)
                        && let Some(lit_data) = self.arena.get_literal(lit_node)
                        && !lit_data.text.is_empty()
                    {
                        // Process escape sequences in the text part
                        let processed =
                            crate::solver::types::process_template_escape_sequences(&lit_data.text);
                        spans.push(TemplateSpan::Text(self.interner.intern_string(&processed)));
                    }
                }
            }

            self.interner.template_literal(spans)
        } else {
            TypeId::STRING // Fallback to string
        }
    }

    /// Lower a named tuple member ([name: T])
    fn lower_named_tuple_member(&self, node_idx: NodeIndex) -> TypeId {
        let node = match self.arena.get(node_idx) {
            Some(n) => n,
            None => return TypeId::ERROR,
        };

        if let Some(data) = self.arena.get_named_tuple_member(node) {
            // Lower the type part
            self.lower_type(data.type_node)
        } else {
            TypeId::ERROR
        }
    }

    /// Lower a constructor type (new () => T)
    fn lower_constructor_type(&self, node_idx: NodeIndex) -> TypeId {
        let node = match self.arena.get(node_idx) {
            Some(n) => n,
            None => return TypeId::ERROR,
        };

        // Constructor types use the same data structure as function types
        if let Some(data) = self.arena.get_function_type(node) {
            let (type_params, (params, this_type, return_type, type_predicate)) = self
                .with_type_params(&data.type_parameters, || {
                    let (params, this_type) = self.lower_params_with_this(&data.parameters);

                    let (return_type, type_predicate) =
                        self.lower_return_type(data.type_annotation);
                    (params, this_type, return_type, type_predicate)
                });

            let shape = FunctionShape {
                type_params,
                params,
                this_type,
                return_type,
                type_predicate,
                is_constructor: true, // Mark as constructor
                is_method: false,
            };

            self.interner.function(shape)
        } else {
            TypeId::ERROR
        }
    }

    /// Lower a wrapped type (optional or rest type)
    fn lower_wrapped_type(&self, node_idx: NodeIndex) -> TypeId {
        let node = match self.arena.get(node_idx) {
            Some(n) => n,
            None => return TypeId::ERROR,
        };

        if let Some(data) = self.arena.get_wrapped_type(node) {
            return self.lower_type(data.type_node);
        }

        if let Some(data) = self.arena.type_operators.get(node.data_index as usize) {
            return self.lower_type(data.type_node);
        }

        TypeId::ERROR
    }
}

#[cfg(test)]
#[path = "lower_tests.rs"]
mod tests;
