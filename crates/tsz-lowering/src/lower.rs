//! Type lowering: AST nodes → `TypeId`
//!
//! This module implements the "bridge" that converts raw AST nodes (Node)
//! into the structural type system (`TypeId`).
//!
//! Lowering is lazy - types are only computed when queried.

use indexmap::IndexMap;
use rustc_hash::FxHashSet;
use std::cell::RefCell;
use std::rc::Rc;
use tsz_common::interner::Atom;
use tsz_parser::parser::NodeList;
use tsz_parser::parser::base::NodeIndex;
use tsz_parser::parser::node::{IndexSignatureData, NodeArena, SignatureData, TypeAliasData};
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::def::DefId;
use tsz_solver::types::{
    CallSignature, CallableShape, ConditionalType, FunctionShape, IndexSignature, MappedModifier,
    MappedType, ObjectFlags, ObjectShape, ParamInfo, PropertyInfo, SymbolRef, TemplateSpan,
    TupleElement, TypeData, TypeId, TypeParamInfo, TypePredicate, TypePredicateTarget, Visibility,
};
use tsz_solver::{QueryDatabase, SubtypeChecker, TypeDatabase, TypeResolver};

#[path = "lower_advanced.rs"]
mod lower_advanced;

/// Maximum number of type lowering operations to prevent infinite loops
pub const MAX_LOWERING_OPERATIONS: u32 = 100_000;

type NodeIndexResolver<'a, T> = dyn Fn(NodeIndex) -> Option<T> + 'a;
type TypeIdResolver<'a> = dyn Fn(&str) -> Option<DefId> + 'a;
type TypeParamScopeStack = RefCell<Vec<Vec<(Atom, TypeId)>>>;

/// Type lowering context.
/// Converts AST type nodes into interned `TypeIds`.
pub struct TypeLowering<'a> {
    arena: &'a NodeArena,
    interner: &'a dyn TypeDatabase,
    /// Optional type resolver - resolves identifier nodes to `SymbolIds`.
    /// If provided, this enables correct abstract class detection.
    type_resolver: Option<&'a NodeIndexResolver<'a, u32>>,
    /// Optional `DefId` resolver - resolves identifier nodes to `DefIds`.
    /// Resolves identifier nodes to `DefId`s for type identity.
    def_id_resolver: Option<&'a NodeIndexResolver<'a, DefId>>,
    /// Optional value resolver for typeof queries.
    value_resolver: Option<&'a NodeIndexResolver<'a, u32>>,
    /// Optional name-based `DefId` resolver — fallback for cross-arena resolution.
    ///
    /// `NodeIndex` values are arena-specific: the same index means different things
    /// in different arenas. When `with_arena()` switches the working arena, the
    /// NodeIndex-based `def_id_resolver` can look up the wrong identifier because
    /// its closure captured arenas from the ORIGINAL context. This name-based
    /// resolver bypasses that problem by resolving directly from the identifier
    /// text (which `lower_identifier_type` already extracts from `self.arena`).
    name_def_id_resolver: Option<&'a TypeIdResolver<'a>>,
    /// Type parameter scopes - wrapped in Rc for sharing across arena contexts
    type_param_scopes: Rc<TypeParamScopeStack>,
    /// Operation counter to prevent infinite loops
    operations: Rc<RefCell<u32>>,
    /// Whether the operation limit has been exceeded
    limit_exceeded: Rc<RefCell<bool>>,
}

struct InterfaceParts {
    // Use IndexMap for deterministic property order - this ensures
    // the same interface produces the same TypeId on every lowering.
    // FxHashMap has undefined iteration order, causing non-determinism.
    properties: IndexMap<Atom, PropertyMerge>,
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
        Self {
            properties: IndexMap::new(),
            call_signatures: Vec::new(),
            construct_signatures: Vec::new(),
            string_index: None,
            number_index: None,
        }
    }

    fn merge_property(&mut self, prop: PropertyInfo) {
        use indexmap::map::Entry;

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
                        visibility: Visibility::Public,
                        parent_id: None,
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
                        visibility: Visibility::Public,
                        parent_id: None,
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
        use indexmap::map::Entry;

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
                        visibility: Visibility::Public,
                        parent_id: None,
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
            def_id_resolver: None,
            value_resolver: None,
            type_param_scopes: Rc::new(RefCell::new(Vec::new())),
            operations: Rc::new(RefCell::new(0)),
            limit_exceeded: Rc::new(RefCell::new(false)),
            name_def_id_resolver: None,
        }
    }

    /// Create a `TypeLowering` with a symbol resolver.
    /// The resolver converts identifier names to actual `SymbolIds` from the binder.
    pub fn with_resolver(
        arena: &'a NodeArena,
        interner: &'a dyn QueryDatabase,
        resolver: &'a dyn Fn(NodeIndex) -> Option<u32>,
    ) -> Self {
        TypeLowering {
            arena,
            interner: interner.as_type_database(),
            type_resolver: Some(resolver),
            def_id_resolver: None,
            value_resolver: Some(resolver),
            name_def_id_resolver: None,
            type_param_scopes: Rc::new(RefCell::new(Vec::new())),
            operations: Rc::new(RefCell::new(0)),
            limit_exceeded: Rc::new(RefCell::new(false)),
        }
    }

    /// Create a `TypeLowering` with separate type/value resolvers.
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
            def_id_resolver: None,
            value_resolver: Some(value_resolver),
            name_def_id_resolver: None,
            type_param_scopes: Rc::new(RefCell::new(Vec::new())),
            operations: Rc::new(RefCell::new(0)),
            limit_exceeded: Rc::new(RefCell::new(false)),
        }
    }

    /// Create a `TypeLowering` with a `DefId` resolver (Phase 1 migration).
    ///
    /// This is the migration path from `SymbolRef` to `DefId` for type identity.
    /// The `DefId` resolver resolves identifier nodes to Solver-owned `DefIds`
    /// instead of Binder-owned `SymbolIds`.
    pub fn with_def_id_resolver(
        arena: &'a NodeArena,
        interner: &'a dyn QueryDatabase,
        def_id_resolver: &'a dyn Fn(NodeIndex) -> Option<DefId>,
        value_resolver: &'a dyn Fn(NodeIndex) -> Option<u32>,
    ) -> Self {
        TypeLowering {
            arena,
            interner: interner.as_type_database(),
            type_resolver: None,
            def_id_resolver: Some(def_id_resolver),
            value_resolver: Some(value_resolver),
            name_def_id_resolver: None,
            type_param_scopes: Rc::new(RefCell::new(Vec::new())),
            operations: Rc::new(RefCell::new(0)),
            limit_exceeded: Rc::new(RefCell::new(false)),
        }
    }

    /// Create a `TypeLowering` with both type and `DefId` resolvers (Phase 2 migration).
    ///
    /// This allows `TypeLowering` to prefer `DefId` when available, but fall back
    /// to `SymbolId` for types that don't have a `DefId` yet.
    pub fn with_hybrid_resolver(
        arena: &'a NodeArena,
        interner: &'a dyn QueryDatabase,
        type_resolver: &'a dyn Fn(NodeIndex) -> Option<u32>,
        def_id_resolver: &'a dyn Fn(NodeIndex) -> Option<DefId>,
        value_resolver: &'a dyn Fn(NodeIndex) -> Option<u32>,
    ) -> Self {
        TypeLowering {
            arena,
            interner: interner.as_type_database(),
            type_resolver: Some(type_resolver),
            def_id_resolver: Some(def_id_resolver),
            value_resolver: Some(value_resolver),
            name_def_id_resolver: None,
            type_param_scopes: Rc::new(RefCell::new(Vec::new())),
            operations: Rc::new(RefCell::new(0)),
            limit_exceeded: Rc::new(RefCell::new(false)),
        }
    }

    /// Create a new `TypeLowering` sharing the same context/state but using a different arena.
    /// This is used for lowering merged interface declarations that span multiple lib files.
    pub fn with_arena<'b>(&'b self, arena: &'b NodeArena) -> TypeLowering<'b>
    where
        'a: 'b,
    {
        TypeLowering {
            arena,
            interner: self.interner,
            type_resolver: self.type_resolver,
            def_id_resolver: self.def_id_resolver,
            value_resolver: self.value_resolver,
            name_def_id_resolver: self.name_def_id_resolver,
            // Rc::clone() shares the underlying Rc instead of copying data
            type_param_scopes: Rc::clone(&self.type_param_scopes),
            operations: Rc::clone(&self.operations),
            limit_exceeded: Rc::clone(&self.limit_exceeded),
        }
    }

    /// Lower interface declarations that may span multiple arenas (lib files).
    ///
    /// For merged interfaces like `Array` which is declared in es5.d.ts, es2015.d.ts, etc.,
    /// each declaration may be in a different `NodeArena`. This method handles looking up
    /// each declaration in its correct arena.
    ///
    /// # Arguments
    /// * `declarations` - List of (`NodeIndex`, &`NodeArena`) pairs. Each declaration must be
    ///   paired with the `NodeArena` it belongs to.
    pub fn lower_merged_interface_declarations(
        &self,
        declarations: &[(NodeIndex, &NodeArena)],
    ) -> (TypeId, Vec<TypeParamInfo>) {
        if declarations.is_empty() {
            return (TypeId::ERROR, Vec::new());
        }

        let mut parts = InterfaceParts::new();
        let mut type_params_collected = false;
        let mut collected_params = Vec::new();

        for (decl_idx, decl_arena) in declarations {
            // Create a lowering context for this specific arena
            let lowerer = self.with_arena(decl_arena);

            let Some(node) = decl_arena.get(*decl_idx) else {
                continue;
            };
            let Some(interface) = decl_arena.get_interface(node) else {
                continue;
            };

            // If we haven't collected type params yet, do it now
            if !type_params_collected
                && let Some(params) = &interface.type_parameters
                && !params.nodes.is_empty()
            {
                // Push scope on the shared state
                self.push_type_param_scope();
                // Use the specific lowerer to resolve param nodes in that arena
                collected_params = lowerer.collect_type_parameters(params);
                type_params_collected = true;
            }

            // Collect members using the arena-specific lowerer
            lowerer.collect_interface_members(&interface.members, &mut parts);
        }

        let result = self.finish_interface_parts(parts, None);

        if type_params_collected {
            self.pop_type_param_scope();
        }

        (result, collected_params)
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
    /// These are added to a new scope that persists for the lifetime of the `TypeLowering`.
    pub fn with_type_param_bindings(self, bindings: Vec<(Atom, TypeId)>) -> Self {
        if !bindings.is_empty() {
            *self.type_param_scopes.borrow_mut() = vec![bindings];
        }
        self
    }

    /// Set the name-based `DefId` resolver for cross-arena resolution.
    pub fn with_name_def_id_resolver(
        mut self,
        resolver: &'a dyn Fn(&str) -> Option<DefId>,
    ) -> Self {
        self.name_def_id_resolver = Some(resolver);
        self
    }

    /// Resolve an identifier name to a `DefId` using the name-based resolver.
    fn resolve_def_id_by_name(&self, name: &str) -> Option<DefId> {
        self.name_def_id_resolver
            .and_then(|resolver| resolver(name))
    }

    /// Resolve a node to a type symbol ID if a resolver is provided.
    fn resolve_type_symbol(&self, node_idx: NodeIndex) -> Option<u32> {
        self.type_resolver.and_then(|resolver| resolver(node_idx))
    }

    /// Resolve a node to a `DefId` if a `DefId` resolver is provided.
    ///
    /// `DefIds` are Solver-owned identifiers that don't require Binder context.
    fn resolve_def_id(&self, node_idx: NodeIndex) -> Option<DefId> {
        self.def_id_resolver.and_then(|resolver| resolver(node_idx))
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
    /// This allows `TypeLowering` to access type parameters that were defined outside of it.
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

    /// Lower a type node to a `TypeId`.
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
            k if k == SyntaxKind::ThisKeyword as u16 => self.interner.this_type(),
            k if k == syntax_kind_ext::THIS_TYPE => self.interner.this_type(),

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
                let type_id = self.interner.type_param(info.clone());
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
            .map_or_else(
                || self.interner.intern_string("T"),
                |id_data| self.interner.intern_string(&id_data.escaped_text),
            );

        let constraint =
            (data.constraint != NodeIndex::NONE).then(|| self.lower_type(data.constraint));

        let default = (data.default != NodeIndex::NONE).then(|| self.lower_type(data.default));

        let is_const = self.has_const_modifier(&data.modifiers);

        Some(TypeParamInfo {
            is_const,
            name,
            constraint,
            default,
        })
    }

    /// Extract a parameter name if it is an identifier.
    fn lower_parameter_name(&self, node_idx: NodeIndex) -> Option<tsz_common::interner::Atom> {
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
                optional: param_data.question_token || param_data.initializer != NodeIndex::NONE,
                rest: param_data.dot_dot_dot_token,
            });
        }

        (lowered, this_type)
    }

    fn lower_return_type(
        &self,
        node_idx: NodeIndex,
        params: &[ParamInfo],
    ) -> (TypeId, Option<TypePredicate>) {
        if node_idx == NodeIndex::NONE {
            // Return ERROR for missing return type annotations to prevent "Any poisoning".
            // This forces explicit return type annotations and surfaces bugs early.
            // Per SOLVER.md Section 6.4: Error propagation prevents cascading noise.
            return (TypeId::ERROR, None);
        }

        if let Some(predicate_node_idx) = self.find_type_predicate_node(node_idx) {
            return self.lower_type_predicate_return(predicate_node_idx, params);
        }

        (self.lower_type(node_idx), None)
    }

    /// Recursively find a type predicate node within a type node (e.g., inside parentheses or intersections).
    fn find_type_predicate_node(&self, node_idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.arena.get(node_idx)?;
        match node.kind {
            k if k == syntax_kind_ext::TYPE_PREDICATE => Some(node_idx),
            k if k == syntax_kind_ext::PARENTHESIZED_TYPE => {
                let wrapped = self.arena.get_wrapped_type(node)?;
                self.find_type_predicate_node(wrapped.type_node)
            }
            k if k == syntax_kind_ext::INTERSECTION_TYPE => {
                let composite = self.arena.get_composite_type(node)?;
                for &member in &composite.types.nodes {
                    if let Some(found) = self.find_type_predicate_node(member) {
                        return Some(found);
                    }
                }
                None
            }
            _ => None,
        }
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
                        self.lower_return_type(data.type_annotation, &params);
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
                                    visibility: Visibility::Public,
                                    parent_id: None,
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
                    continue;
                }

                // Handle accessor declarations (get/set) in type literals
                if (member.kind == syntax_kind_ext::GET_ACCESSOR
                    || member.kind == syntax_kind_ext::SET_ACCESSOR)
                    && let Some(accessor) = self.arena.get_accessor(member)
                    && let Some(name) = self.lower_signature_name(accessor.name)
                {
                    let is_getter = member.kind == syntax_kind_ext::GET_ACCESSOR;
                    if is_getter {
                        let getter_type = self.lower_type(accessor.type_annotation);
                        if let Some(existing) = properties.iter_mut().find(|p| p.name == name) {
                            existing.type_id = getter_type;
                        } else {
                            properties.push(PropertyInfo {
                                name,
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
                            .and_then(|&param_idx| self.arena.get(param_idx))
                            .and_then(|param_node| self.arena.get_parameter(param_node))
                            .map_or(TypeId::UNKNOWN, |param| {
                                self.lower_type(param.type_annotation)
                            });
                        if let Some(existing) = properties.iter_mut().find(|p| p.name == name) {
                            existing.write_type = setter_type;
                            existing.readonly = false;
                        } else {
                            properties.push(PropertyInfo {
                                name,
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
                return self.interner.callable(CallableShape {
                    call_signatures,
                    construct_signatures,
                    properties,
                    string_index,
                    number_index,
                    symbol: None,
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
                    flags: ObjectFlags::empty(),
                    properties,
                    string_index,
                    number_index,
                    symbol: None,
                });
            }

            self.interner.object(properties)
        } else {
            self.interner.object(vec![])
        }
    }

    pub fn lower_interface_declarations(&self, declarations: &[NodeIndex]) -> TypeId {
        self.lower_interface_declarations_with_params(declarations)
            .0
    }

    /// Lower interface declarations and stamp the resulting type with a `SymbolId`.
    /// This is used by the type checker to preserve symbol information for import generation.
    /// The `SymbolId` allows `UsageAnalyzer` to trace which imported interfaces are used in exported APIs.
    pub fn lower_interface_declarations_with_symbol(
        &self,
        declarations: &[NodeIndex],
        sym_id: tsz_binder::SymbolId,
    ) -> TypeId {
        self.lower_interface_declarations_with_params_impl(declarations, Some(sym_id))
            .0
    }

    /// Lower interface declarations and also return the collected type parameters.
    /// This is needed when registering generic lib types (e.g. Array<T>) so that
    /// the actual type parameters from the interface definition are used rather
    /// than synthesizing fresh ones that may have different `TypeIds`.
    pub fn lower_interface_declarations_with_params(
        &self,
        declarations: &[NodeIndex],
    ) -> (TypeId, Vec<TypeParamInfo>) {
        self.lower_interface_declarations_with_params_impl(declarations, None)
    }

    /// Internal implementation that optionally stamps the interface type with a `SymbolId`.
    fn lower_interface_declarations_with_params_impl(
        &self,
        declarations: &[NodeIndex],
        symbol_id: Option<tsz_binder::SymbolId>,
    ) -> (TypeId, Vec<TypeParamInfo>) {
        if declarations.is_empty() {
            return (TypeId::ERROR, Vec::new());
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
            return (TypeId::ERROR, Vec::new());
        }

        let collected_params = if let Some(params) = type_params {
            self.push_type_param_scope();
            self.collect_type_parameters(params)
        } else {
            Vec::new()
        };

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

        (
            self.finish_interface_parts(parts, symbol_id),
            collected_params,
        )
    }

    pub fn lower_type_alias_declaration(
        &self,
        alias: &TypeAliasData,
    ) -> (TypeId, Vec<tsz_solver::TypeParamInfo>) {
        if let Some(params) = alias.type_parameters.as_ref()
            && !params.nodes.is_empty()
        {
            self.push_type_param_scope();
            let collected_params = self.collect_type_parameters(params);
            let result = self.lower_type(alias.type_node);
            self.pop_type_param_scope();
            return (result, collected_params);
        }

        (self.lower_type(alias.type_node), Vec::new())
    }

    /// Lower a function-like declaration (Method, Constructor, Function) to a `TypeId`.
    ///
    /// This is used for overload compatibility checking where we need the structural type
    /// of a specific declaration node, which might not be cached in the `node_types` map.
    ///
    /// # Arguments
    /// * `node_idx` - The declaration node index
    /// * `return_type_override` - Optional return type to use instead of the annotation.
    ///   (Useful for implementation signatures where return type is inferred from body)
    ///
    /// # Returns
    /// The `TypeId` of the function shape, or `TypeId::ERROR` if lowering fails.
    pub fn lower_signature_from_declaration(
        &self,
        node_idx: NodeIndex,
        return_type_override: Option<TypeId>,
    ) -> TypeId {
        use tsz_parser::parser::syntax_kind_ext;

        let Some(node) = self.arena.get(node_idx) else {
            return TypeId::ERROR;
        };

        match node.kind {
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                let Some(method) = self.arena.get_method_decl(node) else {
                    return TypeId::ERROR;
                };

                let (type_params, (params, this_type, return_type, type_predicate)) = self
                    .with_type_params(&method.type_parameters, || {
                        let (params, this_type) = self.lower_params_with_this(&method.parameters);

                        let (return_type, type_predicate) =
                            if let Some(override_type) = return_type_override {
                                (override_type, None)
                            } else {
                                self.lower_return_type(method.type_annotation, &params)
                            };

                        (params, this_type, return_type, type_predicate)
                    });

                self.interner.function(tsz_solver::FunctionShape {
                    type_params,
                    params,
                    this_type,
                    return_type,
                    type_predicate,
                    is_constructor: false,
                    is_method: true, // Methods are bivariant
                })
            }
            k if k == syntax_kind_ext::CONSTRUCTOR => {
                let Some(ctor) = self.arena.get_constructor(node) else {
                    return TypeId::ERROR;
                };

                let (params, this_type) = self.lower_params_with_this(&ctor.parameters);

                // Constructors return the instance type (or void/any implicitly)
                // For overload checking, we usually compare the function shapes.
                let return_type = return_type_override.unwrap_or(TypeId::VOID);

                self.interner.function(tsz_solver::FunctionShape {
                    type_params: Vec::new(), // Constructors don't have own type params
                    params,
                    this_type,
                    return_type,
                    type_predicate: None,
                    is_constructor: true,
                    is_method: false,
                })
            }
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                let Some(func) = self.arena.get_function(node) else {
                    return TypeId::ERROR;
                };

                let (type_params, (params, this_type, return_type, type_predicate)) = self
                    .with_type_params(&func.type_parameters, || {
                        let (params, this_type) = self.lower_params_with_this(&func.parameters);

                        let (return_type, type_predicate) =
                            if let Some(override_type) = return_type_override {
                                (override_type, None)
                            } else {
                                self.lower_return_type(func.type_annotation, &params)
                            };

                        (params, this_type, return_type, type_predicate)
                    });

                self.interner.function(tsz_solver::FunctionShape {
                    type_params,
                    params,
                    this_type,
                    return_type,
                    type_predicate,
                    is_constructor: false,
                    is_method: false, // Functions are contravariant (strict)
                })
            }
            _ => TypeId::ERROR,
        }
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
                            let mut signature = self.lower_call_signature(sig);
                            signature.is_method = true;
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
                continue;
            }

            // Handle accessor declarations (get/set) in interfaces and type literals
            if (member.kind == syntax_kind_ext::GET_ACCESSOR
                || member.kind == syntax_kind_ext::SET_ACCESSOR)
                && let Some(accessor) = self.arena.get_accessor(member)
                && let Some(name) = self.lower_signature_name(accessor.name)
            {
                let is_getter = member.kind == syntax_kind_ext::GET_ACCESSOR;
                if is_getter {
                    let getter_type = self.lower_type(accessor.type_annotation);
                    // Merge with existing accessor entry or create new one
                    match parts.properties.entry(name) {
                        indexmap::map::Entry::Occupied(mut entry) => {
                            // Update existing property with getter type as read type
                            if let PropertyMerge::Property(prop) = entry.get_mut() {
                                prop.type_id = getter_type;
                            }
                        }
                        indexmap::map::Entry::Vacant(entry) => {
                            entry.insert(PropertyMerge::Property(PropertyInfo {
                                name,
                                type_id: getter_type,
                                write_type: getter_type,
                                optional: false,
                                readonly: false,
                                is_method: false,
                                visibility: Visibility::Public,
                                parent_id: None,
                            }));
                        }
                    }
                } else {
                    // Set accessor - extract parameter type
                    let setter_type = accessor
                        .parameters
                        .nodes
                        .first()
                        .and_then(|&param_idx| self.arena.get(param_idx))
                        .and_then(|param_node| self.arena.get_parameter(param_node))
                        .map_or(TypeId::UNKNOWN, |param| {
                            self.lower_type(param.type_annotation)
                        });
                    match parts.properties.entry(name) {
                        indexmap::map::Entry::Occupied(mut entry) => {
                            // Update existing property with setter type as write type
                            if let PropertyMerge::Property(prop) = entry.get_mut() {
                                prop.write_type = setter_type;
                                prop.readonly = false;
                            }
                        }
                        indexmap::map::Entry::Vacant(entry) => {
                            entry.insert(PropertyMerge::Property(PropertyInfo {
                                name,
                                type_id: setter_type,
                                write_type: setter_type,
                                optional: false,
                                readonly: false,
                                is_method: false,
                                visibility: Visibility::Public,
                                parent_id: None,
                            }));
                        }
                    }
                }
            }
        }
    }

    fn finish_interface_parts(
        &self,
        parts: InterfaceParts,
        symbol_id: Option<tsz_binder::SymbolId>,
    ) -> TypeId {
        let mut properties = Vec::with_capacity(parts.properties.len());
        for (name, entry) in parts.properties {
            if let PropertyMerge::Method(methods) = entry {
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
                    visibility: Visibility::Public,
                    parent_id: None,
                });
            } else if let PropertyMerge::Property(prop) = entry {
                properties.push(prop);
            } else if let PropertyMerge::Conflict(prop) = entry {
                properties.push(prop);
            }
        }

        if !parts.call_signatures.is_empty() || !parts.construct_signatures.is_empty() {
            return self.interner.callable(CallableShape {
                call_signatures: parts.call_signatures,
                construct_signatures: parts.construct_signatures,
                properties,
                string_index: parts.string_index,
                number_index: parts.number_index,
                symbol: symbol_id,
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
                flags: ObjectFlags::empty(),
                properties,
                string_index: parts.string_index,
                number_index: parts.number_index,
                symbol: symbol_id,
            });
        }

        self.interner
            .object_with_flags_and_symbol(properties, ObjectFlags::empty(), symbol_id)
    }

    fn lower_call_signature(&self, sig: &SignatureData) -> CallSignature {
        let (type_params, (params, this_type, return_type, type_predicate)) = self
            .with_type_params(&sig.type_parameters, || {
                let (params, this_type) = self.lower_signature_params(sig);
                let (return_type, type_predicate) =
                    self.lower_return_type(sig.type_annotation, &params);
                (params, this_type, return_type, type_predicate)
            });

        CallSignature {
            type_params,
            params,
            this_type,
            return_type,
            type_predicate,
            is_method: false,
        }
    }

    fn lower_method_signature(&self, sig: &SignatureData) -> TypeId {
        let (type_params, (params, this_type, return_type, type_predicate)) = self
            .with_type_params(&sig.type_parameters, || {
                let (params, this_type) = self.lower_signature_params(sig);
                let (return_type, type_predicate) =
                    self.lower_return_type(sig.type_annotation, &params);
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
            // Canonicalize numeric property names (e.g. "1.", "1.0" -> "1")
            if node.kind == SyntaxKind::NumericLiteral as u16
                && let Some(canonical) =
                    tsz_solver::utils::canonicalize_numeric_name(&lit_data.text)
            {
                return Some(self.interner.intern_string(&canonical));
            }
            return Some(self.interner.intern_string(&lit_data.text));
        }
        // Handle computed property names like [Symbol.iterator]
        if node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
            && let Some(computed) = self.arena.get_computed_property(node)
            && let Some(symbol_name) = self.get_well_known_symbol_name(computed.expression)
        {
            return Some(self.interner.intern_string(&symbol_name));
        }
        None
    }

    /// Try to resolve a computed property expression to a well-known symbol name.
    /// Returns names like "[Symbol.iterator]", "[Symbol.asyncIterator]", etc.
    fn get_well_known_symbol_name(&self, expr_idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(expr_idx)?;

        // Handle Symbol.iterator (property access: Symbol.iterator)
        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let access = self.arena.get_access_expr(node)?;
            let base_node = self.arena.get(access.expression)?;
            let base_ident = self.arena.get_identifier(base_node)?;
            if base_ident.escaped_text == "Symbol" {
                let name_node = self.arena.get(access.name_or_argument)?;
                let name_ident = self.arena.get_identifier(name_node)?;
                return Some(format!("[Symbol.{}]", name_ident.escaped_text));
            }
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

        let skip_string = string_index.is_some_and(|idx| self.contains_meta_type(idx.value_type));
        let skip_number = number_index.is_some_and(|idx| self.contains_meta_type(idx.value_type));

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
            TypeData::TypeParameter(_)
            | TypeData::Infer(_)
            | TypeData::ThisType
            | TypeData::TypeQuery(_)
            | TypeData::Conditional(_)
            | TypeData::Mapped(_)
            | TypeData::IndexAccess(_, _)
            | TypeData::KeyOf(_) => true,
            TypeData::Union(members) | TypeData::Intersection(members) => {
                let members = self.interner.type_list(members);
                members
                    .iter()
                    .any(|member| self.contains_meta_type_inner(*member, visited))
            }
            TypeData::Array(elem) => self.contains_meta_type_inner(elem, visited),
            TypeData::Tuple(elements) => {
                let elements = self.interner.tuple_list(elements);
                elements
                    .iter()
                    .any(|elem| self.contains_meta_type_inner(elem.type_id, visited))
            }
            TypeData::Object(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                shape
                    .properties
                    .iter()
                    .any(|prop| self.contains_meta_type_inner(prop.type_id, visited))
            }
            TypeData::ObjectWithIndex(shape_id) => {
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
            TypeData::Function(shape_id) => {
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
            TypeData::Callable(shape_id) => {
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
            TypeData::Application(app_id) => {
                let app = self.interner.type_application(app_id);
                if self.contains_meta_type_inner(app.base, visited) {
                    return true;
                }
                app.args
                    .iter()
                    .any(|arg| self.contains_meta_type_inner(*arg, visited))
            }
            TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner) => {
                self.contains_meta_type_inner(inner, visited)
            }
            TypeData::TemplateLiteral(spans) => {
                let spans = self.interner.template_list(spans);
                spans.iter().any(|span| match span {
                    TemplateSpan::Text(_) => false,
                    TemplateSpan::Type(inner) => self.contains_meta_type_inner(*inner, visited),
                })
            }
            TypeData::StringIntrinsic { type_arg, .. } => {
                self.contains_meta_type_inner(type_arg, visited)
            }
            TypeData::Enum(_def_id, member_type) => {
                self.contains_meta_type_inner(member_type, visited)
            }
            TypeData::Lazy(_)
            | TypeData::Recursive(_)
            | TypeData::BoundParameter(_)
            | TypeData::Intrinsic(_)
            | TypeData::Literal(_)
            | TypeData::UniqueSymbol(_)
            | TypeData::ModuleNamespace(_)
            | TypeData::Error => false,
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

            // Get visibility (for type literals, always Public)
            let visibility = self.get_visibility_from_modifiers(&sig.modifiers);
            let type_id = self.lower_type(sig.type_annotation);
            let write_type = if readonly { TypeId::NONE } else { type_id };

            Some(PropertyInfo {
                name,
                type_id,
                write_type,
                optional: sig.question_token,
                readonly,
                is_method: false,
                visibility,
                parent_id: None, // Type literals don't have parent_id
            })
        } else {
            None
        }
    }

    /// Check if a modifiers list contains a readonly keyword
    fn has_readonly_modifier(&self, modifiers: &Option<NodeList>) -> bool {
        use tsz_scanner::SyntaxKind;

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

    /// Get visibility from modifiers list
    /// Returns Private, Protected, or Public (default)
    fn get_visibility_from_modifiers(&self, modifiers: &Option<NodeList>) -> Visibility {
        use tsz_scanner::SyntaxKind;

        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.arena.get(mod_idx) {
                    match mod_node.kind {
                        x if x == SyntaxKind::PrivateKeyword as u16 => return Visibility::Private,
                        x if x == SyntaxKind::ProtectedKeyword as u16 => {
                            return Visibility::Protected;
                        }
                        _ => continue,
                    }
                }
            }
        }
        Visibility::Public
    }

    /// Check if a modifiers list contains a const keyword (for const type parameters)
    fn has_const_modifier(&self, modifiers: &Option<NodeList>) -> bool {
        use tsz_scanner::SyntaxKind;

        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.arena.get(mod_idx)
                    && mod_node.kind == SyntaxKind::ConstKeyword as u16
                {
                    return true;
                }
            }
        }
        false
    }
}
