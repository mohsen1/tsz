//! Type lowering: AST nodes → `TypeId`
//!
//! This module implements the "bridge" that converts raw AST nodes (Node)
//! into the structural type system (`TypeId`).
//!
//! Lowering is lazy - types are only computed when queried.

use indexmap::IndexMap;
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use tsz_common::interner::Atom;
use tsz_parser::parser::NodeList;
use tsz_parser::parser::base::NodeIndex;
use tsz_parser::parser::node::{IndexSignatureData, NodeArena, SignatureData, TypeAliasData};
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::def::DefId;
use tsz_solver::types::{
    CallSignature, CallableShape, FunctionShape, IndexSignature, ObjectShape, ParamInfo,
    PropertyInfo, TupleElement, TypeId, TypeParamInfo, TypePredicate, Visibility,
};
use tsz_solver::{QueryDatabase, TypeDatabase};

/// Maximum number of type lowering operations to prevent infinite loops
pub const MAX_LOWERING_OPERATIONS: u32 = 100_000;

pub(super) type NodeIndexResolver<'a, T> = dyn Fn(NodeIndex) -> Option<T> + 'a;
pub(super) type TypeIdResolver<'a> = dyn Fn(&str) -> Option<DefId> + 'a;
pub(super) type LazyTypeParamsResolver<'a> = dyn Fn(DefId) -> Option<Vec<TypeParamInfo>> + 'a;
pub(super) type TypeParamScopeStack = RefCell<Vec<Vec<(Atom, TypeId)>>>;

/// Type lowering context.
/// Converts AST type nodes into interned `TypeIds`.
pub struct TypeLowering<'a> {
    pub(super) arena: &'a NodeArena,
    pub(super) interner: &'a dyn TypeDatabase,
    /// Optional type resolver - resolves identifier nodes to `SymbolIds`.
    /// If provided, this enables correct abstract class detection.
    pub(super) type_resolver: Option<&'a NodeIndexResolver<'a, u32>>,
    /// Optional `DefId` resolver - resolves identifier nodes to `DefIds`.
    /// Resolves identifier nodes to `DefId`s for type identity.
    pub(super) def_id_resolver: Option<&'a NodeIndexResolver<'a, DefId>>,
    /// Optional value resolver for typeof queries.
    pub(super) value_resolver: Option<&'a NodeIndexResolver<'a, u32>>,
    /// Optional name-based `DefId` resolver — fallback for cross-arena resolution.
    ///
    /// `NodeIndex` values are arena-specific: the same index means different things
    /// in different arenas. When `with_arena()` switches the working arena, the
    /// NodeIndex-based `def_id_resolver` can look up the wrong identifier because
    /// its closure captured arenas from the ORIGINAL context. This name-based
    /// resolver bypasses that problem by resolving directly from the identifier
    /// text (which `lower_identifier_type` already extracts from `self.arena`).
    pub(super) name_def_id_resolver: Option<&'a TypeIdResolver<'a>>,
    /// Optional computed property name resolver — resolves computed property
    /// expressions (e.g., `[k]` where k is a unique symbol) to property name atoms.
    /// Used when the lowering can't determine the name from AST alone.
    pub(super) computed_name_resolver: Option<&'a NodeIndexResolver<'a, Atom>>,
    /// Optional resolver for lazy type parameter metadata. This is used when
    /// a lowered lazy reference omits type arguments but all parameters have defaults.
    pub(super) lazy_type_params_resolver: Option<&'a LazyTypeParamsResolver<'a>>,
    /// When true, prefer identifier-text `DefId` resolution over raw NodeIndex-based
    /// resolution. This is needed for cross-arena lowering where the same `NodeIndex`
    /// may refer to different identifiers in different arenas.
    pub(super) prefer_name_def_id_resolution: bool,
    /// Optional direct self-reference for merged interface lowering.
    pub(super) preferred_self_name: Option<String>,
    pub(super) preferred_self_def_id: Option<DefId>,
    /// Type parameter scopes - wrapped in Rc for sharing across arena contexts
    pub(super) type_param_scopes: Rc<TypeParamScopeStack>,
    /// Whether strictNullChecks is enabled. When true, optional parameters
    /// in function types include `| undefined` in their type.
    pub(super) strict_null_checks: bool,
    /// Optional override for type query resolution. When provided, this callback
    /// is consulted before creating a `TypeQuery` type. If it returns `Some(type_id)`,
    /// that type is used directly instead of creating a deferred `TypeQuery`.
    /// This enables flow-sensitive narrowing for `typeof expr` in type positions
    /// (e.g., inside type alias bodies where flow narrowing has already been computed).
    pub(super) type_query_override: Option<&'a NodeIndexResolver<'a, TypeId>>,
    /// Operation counter to prevent infinite loops
    pub(super) operations: Rc<RefCell<u32>>,
    /// Whether the operation limit has been exceeded
    pub(super) limit_exceeded: Rc<RefCell<bool>>,
}

pub(super) struct InterfaceParts {
    // Use IndexMap for deterministic property order - this ensures
    // the same interface produces the same TypeId on every lowering.
    // FxHashMap has undefined iteration order, causing non-determinism.
    pub(super) properties: IndexMap<Atom, PropertyMerge>,
    pub(super) call_signatures: Vec<CallSignature>,
    pub(super) construct_signatures: Vec<CallSignature>,
    pub(super) string_index: Option<IndexSignature>,
    pub(super) number_index: Option<IndexSignature>,
    /// Base `declaration_order` for the current declaration pass.
    current_pass_base: u32,
    /// Counter within the current declaration pass.
    pass_local_counter: u32,
    /// Forward declaration order for properties. Populated after reverse iteration
    /// to give earlier declarations lower order numbers (matching tsc's property
    /// enumeration for diagnostics like TS2740 "missing properties" lists).
    pub(super) declaration_orders: rustc_hash::FxHashMap<Atom, u32>,
}

pub(super) enum PropertyMerge {
    Property(PropertyInfo),
    Method(MethodOverloads),
    Conflict(PropertyInfo),
}

pub(super) struct MethodOverloads {
    pub(super) signatures: Vec<CallSignature>,
    pub(super) optional: bool,
    pub(super) readonly: bool,
    /// Declaration order of the first occurrence of this method, for diagnostic ordering.
    pub(super) declaration_order: u32,
}

impl InterfaceParts {
    /// Stride between declaration passes. Must be larger than the maximum number
    /// of properties any single interface declaration contributes.
    const DECL_ORDER_STRIDE: u32 = 10_000;

    pub(super) fn new() -> Self {
        Self {
            properties: IndexMap::new(),
            call_signatures: Vec::new(),
            construct_signatures: Vec::new(),
            string_index: None,
            number_index: None,
            current_pass_base: 0,
            pass_local_counter: 0,
            declaration_orders: rustc_hash::FxHashMap::default(),
        }
    }

    /// Set the declaration pass base for the next batch of properties.
    ///
    /// `forward_decl_index` is the 0-based index of the declaration in
    /// forward (source) order, so the earliest declaration gets index 0.
    pub(super) const fn set_declaration_pass(&mut self, forward_decl_index: usize) {
        self.current_pass_base = (forward_decl_index as u32) * Self::DECL_ORDER_STRIDE;
        self.pass_local_counter = 0;
    }

    /// Get the next `declaration_order` value for a property being added in
    /// the current declaration pass.
    pub(super) const fn next_declaration_order(&mut self) -> u32 {
        let order = self.current_pass_base + self.pass_local_counter;
        self.pass_local_counter += 1;
        order
    }

    pub(super) fn merge_property(&mut self, prop: PropertyInfo) {
        use indexmap::map::Entry;

        let next_order = self.current_pass_base + self.pass_local_counter;
        match self.properties.entry(prop.name) {
            Entry::Vacant(entry) => {
                self.pass_local_counter += 1;
                let mut prop = prop;
                prop.declaration_order = next_order;
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
                    let order = existing.declaration_order;
                    let conflict = PropertyInfo {
                        name: prop.name,
                        type_id: TypeId::ERROR,
                        write_type: TypeId::ERROR,
                        optional: existing.optional && prop.optional,
                        readonly: existing.readonly && prop.readonly,
                        is_method: false,
                        is_class_prototype: false,
                        visibility: Visibility::Public,
                        parent_id: None,
                        declaration_order: order,
                        is_string_named: false,
                    };
                    entry.insert(PropertyMerge::Conflict(conflict));
                }
                PropertyMerge::Method(methods) => {
                    let order = methods.declaration_order;
                    let conflict = PropertyInfo {
                        name: prop.name,
                        type_id: TypeId::ERROR,
                        write_type: TypeId::ERROR,
                        optional: methods.optional && prop.optional,
                        readonly: false,
                        is_method: false,
                        is_class_prototype: false,
                        visibility: Visibility::Public,
                        parent_id: None,
                        declaration_order: order,
                        is_string_named: false,
                    };
                    entry.insert(PropertyMerge::Conflict(conflict));
                }
                PropertyMerge::Conflict(_) => {}
            },
        }
    }

    pub(super) fn merge_method(
        &mut self,
        name: Atom,
        signature: CallSignature,
        optional: bool,
        readonly: bool,
    ) {
        use indexmap::map::Entry;

        let next_order = self.current_pass_base + self.pass_local_counter;
        match self.properties.entry(name) {
            Entry::Vacant(entry) => {
                self.pass_local_counter += 1;
                entry.insert(PropertyMerge::Method(MethodOverloads {
                    signatures: vec![signature],
                    optional,
                    readonly,
                    declaration_order: next_order,
                }));
            }
            Entry::Occupied(mut entry) => match entry.get_mut() {
                PropertyMerge::Method(methods) => {
                    methods.signatures.push(signature);
                    methods.optional |= optional;
                    methods.readonly &= readonly;
                }
                PropertyMerge::Property(prop) => {
                    let order = prop.declaration_order;
                    let conflict = PropertyInfo {
                        name,
                        type_id: TypeId::ERROR,
                        write_type: TypeId::ERROR,
                        optional: prop.optional && optional,
                        readonly: false,
                        is_method: false,
                        is_class_prototype: false,
                        visibility: Visibility::Public,
                        parent_id: None,
                        declaration_order: order,
                        is_string_named: false,
                    };
                    entry.insert(PropertyMerge::Conflict(conflict));
                }
                PropertyMerge::Conflict(_) => {}
            },
        }
    }

    pub(super) fn merge_index_signature(&mut self, index: IndexSignature) {
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
    pub fn new(arena: &'a NodeArena, interner: &'a dyn QueryDatabase) -> Self {
        TypeLowering {
            arena,
            interner: interner.as_type_database(),
            type_resolver: None,
            def_id_resolver: None,
            value_resolver: None,
            computed_name_resolver: None,
            lazy_type_params_resolver: None,
            prefer_name_def_id_resolution: false,
            preferred_self_name: None,
            preferred_self_def_id: None,
            type_param_scopes: Rc::new(RefCell::new(Vec::new())),
            operations: Rc::new(RefCell::new(0)),
            limit_exceeded: Rc::new(RefCell::new(false)),
            name_def_id_resolver: None,
            strict_null_checks: false,
            type_query_override: None,
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
            computed_name_resolver: None,
            lazy_type_params_resolver: None,
            prefer_name_def_id_resolution: false,
            preferred_self_name: None,
            preferred_self_def_id: None,
            name_def_id_resolver: None,
            strict_null_checks: false,
            type_param_scopes: Rc::new(RefCell::new(Vec::new())),
            operations: Rc::new(RefCell::new(0)),
            limit_exceeded: Rc::new(RefCell::new(false)),
            type_query_override: None,
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
            computed_name_resolver: None,
            lazy_type_params_resolver: None,
            prefer_name_def_id_resolution: false,
            preferred_self_name: None,
            preferred_self_def_id: None,
            name_def_id_resolver: None,
            strict_null_checks: false,
            type_param_scopes: Rc::new(RefCell::new(Vec::new())),
            operations: Rc::new(RefCell::new(0)),
            limit_exceeded: Rc::new(RefCell::new(false)),
            type_query_override: None,
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
            computed_name_resolver: None,
            lazy_type_params_resolver: None,
            prefer_name_def_id_resolution: false,
            preferred_self_name: None,
            preferred_self_def_id: None,
            name_def_id_resolver: None,
            strict_null_checks: false,
            type_param_scopes: Rc::new(RefCell::new(Vec::new())),
            operations: Rc::new(RefCell::new(0)),
            limit_exceeded: Rc::new(RefCell::new(false)),
            type_query_override: None,
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
            computed_name_resolver: None,
            lazy_type_params_resolver: None,
            prefer_name_def_id_resolution: false,
            preferred_self_name: None,
            preferred_self_def_id: None,
            name_def_id_resolver: None,
            strict_null_checks: false,
            type_param_scopes: Rc::new(RefCell::new(Vec::new())),
            operations: Rc::new(RefCell::new(0)),
            limit_exceeded: Rc::new(RefCell::new(false)),
            type_query_override: None,
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
            computed_name_resolver: self.computed_name_resolver,
            lazy_type_params_resolver: self.lazy_type_params_resolver,
            prefer_name_def_id_resolution: self.prefer_name_def_id_resolution,
            preferred_self_name: self.preferred_self_name.clone(),
            preferred_self_def_id: self.preferred_self_def_id,
            name_def_id_resolver: self.name_def_id_resolver,
            strict_null_checks: self.strict_null_checks,
            type_query_override: self.type_query_override,
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
        self.lower_merged_interface_declarations_with_symbol(declarations, None)
    }

    /// Lower merged interface declarations and optionally stamp the resulting type
    /// with the originating interface symbol.
    pub fn lower_merged_interface_declarations_with_symbol(
        &self,
        declarations: &[(NodeIndex, &NodeArena)],
        symbol_id: Option<tsz_binder::SymbolId>,
    ) -> (TypeId, Vec<TypeParamInfo>) {
        if declarations.is_empty() {
            return (TypeId::ERROR, Vec::new());
        }

        let mut parts = InterfaceParts::new();
        let mut type_params_collected = false;
        let mut collected_params = Vec::new();

        let is_lib_decl = |arena: &NodeArena, idx: NodeIndex| {
            let mut current = idx;
            while let Some(ext) = arena.get_extended(current) {
                if ext.parent.is_none() {
                    break;
                }
                current = ext.parent;
            }

            arena
                .get(current)
                .and_then(|node| arena.get_source_file(node))
                .is_some_and(|source| {
                    let file_name = Path::new(&source.file_name)
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or(source.file_name.as_str());
                    source.is_declaration_file
                        && file_name.starts_with("lib.")
                        && file_name.ends_with(".d.ts")
                })
        };

        // Process declarations in reverse order: TypeScript's interface merging
        // rule puts later declarations' members first for overload resolution.
        let num_declarations = declarations.len();
        for (rev_i, (decl_idx, decl_arena)) in declarations.iter().rev().enumerate() {
            // Set the declaration pass base so that properties from earlier
            // (forward) declarations get lower declaration_order values.
            // rev_i=0 processes the last declaration (forward index = num-1),
            // rev_i=num-1 processes the first declaration (forward index = 0).
            let forward_decl_index = num_declarations - 1 - rev_i;
            parts.set_declaration_pass(forward_decl_index);

            // Merged lib declarations share NodeIndex values across arenas. Even when the
            // current declaration uses the fallback arena, raw NodeIndex-based lookup can
            // still pick an identifier text from a sibling lib declaration first and corrupt
            // references like Iterable<T> during merged static interface lowering.
            let lowerer = if is_lib_decl(decl_arena, *decl_idx) {
                self.with_arena(decl_arena).prefer_name_def_id_resolution()
            } else {
                self.with_arena(decl_arena)
            };

            let Some(node) = decl_arena.get(*decl_idx) else {
                continue;
            };
            let Some(interface) = decl_arena.get_interface(node) else {
                continue;
            };

            // Collect or merge type parameters from this declaration
            if let Some(params) = &interface.type_parameters
                && !params.nodes.is_empty()
            {
                if !type_params_collected {
                    // First declaration with type params: collect them
                    self.push_type_param_scope();
                    collected_params = lowerer.collect_type_parameters(params);
                    type_params_collected = true;
                } else {
                    // Subsequent declaration: merge missing defaults/constraints
                    // from this declaration into the already-collected params.
                    // This handles cases like Uint8Array where the default is
                    // declared in lib.es5.d.ts but other declarations in
                    // es2015.iterable.d.ts etc. omit it.
                    let extra = lowerer.collect_type_parameters_raw(params);
                    for (i, ep) in extra.into_iter().enumerate() {
                        if i < collected_params.len() {
                            if collected_params[i].default.is_none() && ep.default.is_some() {
                                collected_params[i].default = ep.default;
                            }
                            if collected_params[i].constraint.is_none() && ep.constraint.is_some() {
                                collected_params[i].constraint = ep.constraint;
                            }
                        }
                    }
                }
            }

            // Collect members using the arena-specific lowerer
            lowerer.collect_interface_members(&interface.members, &mut parts);
        }

        // Assign declaration_order in FORWARD declaration order for diagnostics.
        self.assign_forward_declaration_order_cross_file(&mut parts, declarations);

        let result = self.finish_interface_parts(parts, symbol_id);

        if type_params_collected {
            self.pop_type_param_scope();
        }

        (result, collected_params)
    }

    /// Collect type parameters from merged interface declarations without lowering members.
    ///
    /// This is a lightweight path used when callers only need generic parameter metadata
    /// (names/constraints/defaults) and not the full interface body.
    pub fn collect_merged_interface_type_parameters(
        &self,
        declarations: &[(NodeIndex, &NodeArena)],
    ) -> Vec<TypeParamInfo> {
        let mut collected = Vec::new();
        let mut scope_pushed = false;

        for (decl_idx, decl_arena) in declarations {
            let Some(node) = decl_arena.get(*decl_idx) else {
                continue;
            };
            let Some(interface) = decl_arena.get_interface(node) else {
                continue;
            };
            let Some(params) = &interface.type_parameters else {
                continue;
            };
            if params.nodes.is_empty() {
                continue;
            }

            let lowerer = self.with_arena(decl_arena);
            if !scope_pushed {
                self.push_type_param_scope();
                collected = lowerer.collect_type_parameters(params);
                scope_pushed = true;
            } else {
                // Merge missing defaults/constraints from subsequent declarations
                let extra = lowerer.collect_type_parameters_raw(params);
                for (i, ep) in extra.into_iter().enumerate() {
                    if i < collected.len() {
                        if collected[i].default.is_none() && ep.default.is_some() {
                            collected[i].default = ep.default;
                        }
                        if collected[i].constraint.is_none() && ep.constraint.is_some() {
                            collected[i].constraint = ep.constraint;
                        }
                    }
                }
            }
        }

        if scope_pushed {
            self.pop_type_param_scope();
        }
        collected
    }

    /// Collect type parameters for a type alias declaration without lowering the alias body.
    pub fn collect_type_alias_type_parameters(&self, alias: &TypeAliasData) -> Vec<TypeParamInfo> {
        let Some(params) = alias.type_parameters.as_ref() else {
            return Vec::new();
        };
        if params.nodes.is_empty() {
            return Vec::new();
        }

        self.push_type_param_scope();
        let collected = self.collect_type_parameters(params);
        self.pop_type_param_scope();
        collected
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

    /// Enable strictNullChecks behavior. When set, optional parameters in
    /// function types include `| undefined` in their type.
    pub const fn with_strict_null_checks(mut self, enabled: bool) -> Self {
        self.strict_null_checks = enabled;
        self
    }

    /// Set the computed property name resolver for resolving computed property
    /// names like `[k]` where k is a unique symbol variable.
    pub fn with_computed_name_resolver(
        mut self,
        resolver: &'a dyn Fn(NodeIndex) -> Option<Atom>,
    ) -> Self {
        self.computed_name_resolver = Some(resolver);
        self
    }

    /// Set the lazy type parameter resolver for applying omitted defaulted type arguments
    /// when lowering lazy references from interface members.
    pub fn with_lazy_type_params_resolver(
        mut self,
        resolver: &'a dyn Fn(DefId) -> Option<Vec<TypeParamInfo>>,
    ) -> Self {
        self.lazy_type_params_resolver = Some(resolver);
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

    /// Prefer identifier-text DefId resolution over raw NodeIndex-based resolution.
    ///
    /// This should only be enabled in cross-arena lowering contexts where `NodeIndex`
    /// collisions between declaration arenas are possible.
    pub const fn prefer_name_def_id_resolution(mut self) -> Self {
        self.prefer_name_def_id_resolution = true;
        self
    }

    /// Set a type query override callback for flow-sensitive `typeof` resolution.
    ///
    /// When lowering encounters `typeof expr`, this callback is consulted first.
    /// If it returns `Some(type_id)`, that type is used directly instead of
    /// creating a deferred `TypeQuery` type. This enables the checker to inject
    /// flow-narrowed types for `typeof` expressions in type alias bodies.
    pub fn with_type_query_override(
        mut self,
        resolver: &'a dyn Fn(NodeIndex) -> Option<TypeId>,
    ) -> Self {
        self.type_query_override = Some(resolver);
        self
    }

    /// Resolve merged interface self-references directly to the merged symbol.
    pub fn with_preferred_self_reference(mut self, name: String, def_id: DefId) -> Self {
        self.preferred_self_name = Some(name);
        self.preferred_self_def_id = Some(def_id);
        self
    }

    /// Resolve an identifier name to a `DefId` using the name-based resolver.
    pub(super) fn resolve_def_id_by_name(&self, name: &str) -> Option<DefId> {
        self.name_def_id_resolver
            .and_then(|resolver| resolver(name))
    }

    /// Build the full text of an entity-name type node from the current arena.
    ///
    /// This is used by cross-arena lowering to resolve qualified names (e.g.
    /// `Intl.NumberFormatOptions`) through the name-based `DefId` resolver
    /// instead of relying on arena-local `NodeIndex` values.
    pub(super) fn type_name_text(&self, node_idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(node_idx)?;

        if node.kind == SyntaxKind::Identifier as u16 {
            return self
                .arena
                .get_identifier(node)
                .map(|ident| ident.escaped_text.clone());
        }

        if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            let qn = self.arena.get_qualified_name(node)?;
            let left = self.type_name_text(qn.left)?;
            let right = self.type_name_text(qn.right)?;
            let mut combined = String::with_capacity(left.len() + 1 + right.len());
            combined.push_str(&left);
            combined.push('.');
            combined.push_str(&right);
            return Some(combined);
        }

        None
    }

    /// Build a namespace-qualified name for a simple identifier when it appears
    /// inside nested `namespace`/`module` declarations.
    ///
    /// Cross-arena lib lowering often encounters unqualified references to
    /// sibling declarations within a namespace, e.g. `NumberFormatOptionsStyle`
    /// inside `declare namespace Intl`. In those cases the current arena can
    /// recover the lexical namespace path even when the cross-arena
    /// `NodeIndex`-based resolver cannot.
    pub(super) fn scoped_identifier_name_text(&self, node_idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(node_idx)?;
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        let ident = self.arena.get_identifier(node)?;
        let mut prefixes = Vec::new();
        let mut parent = self
            .arena
            .get_extended(node_idx)
            .map_or(NodeIndex::NONE, |info| info.parent);

        while parent.is_some() {
            let parent_node = self.arena.get(parent)?;
            if parent_node.kind == syntax_kind_ext::MODULE_DECLARATION
                && let Some(module) = self.arena.get_module(parent_node)
                && let Some(name_node) = self.arena.get(module.name)
                && name_node.kind == SyntaxKind::Identifier as u16
                && let Some(name_ident) = self.arena.get_identifier(name_node)
            {
                prefixes.push(name_ident.escaped_text.clone());
            }

            parent = self
                .arena
                .get_extended(parent)
                .map_or(NodeIndex::NONE, |info| info.parent);
        }

        if prefixes.is_empty() {
            return None;
        }

        let mut combined = String::new();
        for prefix in prefixes.iter().rev() {
            combined.push_str(prefix);
            combined.push('.');
        }
        combined.push_str(&ident.escaped_text);
        Some(combined)
    }

    /// Resolve a node to a type symbol ID if a resolver is provided.
    pub(super) fn resolve_type_symbol(&self, node_idx: NodeIndex) -> Option<u32> {
        self.type_resolver.and_then(|resolver| resolver(node_idx))
    }

    /// Resolve a node to a `DefId` if a `DefId` resolver is provided.
    ///
    /// `DefIds` are Solver-owned identifiers that don't require Binder context.
    pub(super) fn resolve_def_id(&self, node_idx: NodeIndex) -> Option<DefId> {
        self.def_id_resolver.and_then(|resolver| resolver(node_idx))
    }

    /// Resolve a node to a value symbol ID if a resolver is provided.
    pub(super) fn resolve_value_symbol(&self, node_idx: NodeIndex) -> Option<u32> {
        if let Some(resolver) = self.value_resolver {
            resolver(node_idx)
        } else {
            self.resolve_type_symbol(node_idx)
        }
    }

    pub(super) fn push_type_param_scope(&self) {
        self.type_param_scopes.borrow_mut().push(Vec::new());
    }

    pub(super) fn pop_type_param_scope(&self) {
        let _ = self.type_param_scopes.borrow_mut().pop();
    }

    pub(super) fn add_type_param_binding(&self, name: Atom, type_id: TypeId) {
        if let Some(scope) = self.type_param_scopes.borrow_mut().last_mut() {
            scope.push((name, type_id));
        }
    }

    pub(super) fn lookup_type_param(&self, name: &str) -> Option<TypeId> {
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

    pub(super) fn with_type_params<R>(
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

    pub(super) fn collect_type_parameters(&self, list: &NodeList) -> Vec<TypeParamInfo> {
        let mut param_names = Vec::with_capacity(list.nodes.len());
        for &idx in &list.nodes {
            let Some(node) = self.arena.get(idx) else {
                continue;
            };
            let Some(data) = self.arena.get_type_parameter(node) else {
                continue;
            };
            let name = self
                .arena
                .get(data.name)
                .and_then(|name_node| self.arena.get_identifier(name_node))
                .map_or_else(
                    || self.interner.intern_string("T"),
                    |id_data| self.interner.intern_string(&id_data.escaped_text),
                );
            let is_const = self
                .arena
                .has_modifier(&data.modifiers, tsz_scanner::SyntaxKind::ConstKeyword);

            // Bind all local type parameters before lowering constraints/defaults so
            // self-referential constraints like `Exclude<keyof P, ...>` can resolve P.
            let placeholder = TypeParamInfo {
                is_const,
                name,
                constraint: None,
                default: None,
            };
            self.add_type_param_binding(name, self.interner.type_param(placeholder));
            param_names.push((idx, name, is_const));
        }

        let mut params = Vec::with_capacity(param_names.len());
        for (idx, name, is_const) in param_names {
            if let Some(mut info) = self.lower_type_parameter(idx) {
                info.name = name;
                info.is_const = is_const;
                let type_id = self.interner.type_param(info);
                self.add_type_param_binding(info.name, type_id);
                params.push(info);
            }
        }
        params
    }

    /// Collect type parameters without adding scope bindings.
    /// Used for merging defaults/constraints from additional declarations
    /// when the type params are already in scope from a prior declaration.
    pub(super) fn collect_type_parameters_raw(&self, list: &NodeList) -> Vec<TypeParamInfo> {
        let mut params = Vec::with_capacity(list.nodes.len());
        for &idx in &list.nodes {
            if let Some(info) = self.lower_type_parameter(idx) {
                params.push(info);
            }
        }
        params
    }

    pub(super) fn lower_type_parameter(&self, node_idx: NodeIndex) -> Option<TypeParamInfo> {
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

        let is_const = self
            .arena
            .has_modifier(&data.modifiers, tsz_scanner::SyntaxKind::ConstKeyword);

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

    pub(super) fn lower_params_with_this(
        &self,
        params: &NodeList,
    ) -> (Vec<ParamInfo>, Option<TypeId>) {
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

            let type_id = self.lower_type(param_data.type_annotation);
            let optional = param_data.question_token || param_data.initializer != NodeIndex::NONE;
            // For `?`-optional params, tsc includes `| undefined` in the
            // signature type unconditionally (for display). Default-value
            // params keep the base type.
            let sig_type_id = if param_data.question_token
                && type_id != TypeId::ANY
                && type_id != TypeId::UNKNOWN
                && type_id != TypeId::ERROR
                && !tsz_solver::type_contains_undefined(self.interner, type_id)
            {
                self.interner.union2(type_id, TypeId::UNDEFINED)
            } else {
                type_id
            };
            lowered.push(ParamInfo {
                name: self.lower_parameter_name(param_data.name),
                type_id: sig_type_id,
                optional,
                rest: param_data.dot_dot_dot_token,
            });
        }

        (lowered, this_type)
    }

    pub(super) fn lower_return_type(
        &self,
        node_idx: NodeIndex,
        params: &[ParamInfo],
    ) -> (TypeId, Option<TypePredicate>) {
        if node_idx == NodeIndex::NONE {
            // Return ANY for missing return type annotations to match TypeScript behavior,
            // especially for type literals and signatures without bodies.
            return (TypeId::ANY, None);
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
                                    readonly: self.arena.has_modifier(
                                        &sig.modifiers,
                                        tsz_scanner::SyntaxKind::ReadonlyKeyword,
                                    ),
                                    is_method: true,
                                    is_class_prototype: false,
                                    visibility: Visibility::Public,
                                    parent_id: None,
                                    declaration_order: 0,
                                    is_string_named: false,
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
                                readonly: true,
                                is_method: false,
                                is_class_prototype: false,
                                visibility: Visibility::Public,
                                parent_id: None,
                                declaration_order: 0,
                                is_string_named: false,
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
                                is_class_prototype: false,
                                visibility: Visibility::Public,
                                parent_id: None,
                                declaration_order: 0,
                                is_string_named: false,
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
                    is_abstract: false,
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
                    ..ObjectShape::default()
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

        // Process declarations in reverse order: TypeScript's interface merging
        // rule puts later declarations' members first for overload resolution.
        // E.g., PromiseConstructor from es2015.iterable (earlier) and es2015.promise
        // (later) — the tuple overload from es2015.promise should be tried first.
        let num_declarations = declarations.len();
        for (rev_i, &decl_idx) in declarations.iter().rev().enumerate() {
            let forward_decl_index = num_declarations - 1 - rev_i;
            parts.set_declaration_pass(forward_decl_index);

            let Some(node) = self.arena.get(decl_idx) else {
                continue;
            };
            let Some(interface) = self.arena.get_interface(node) else {
                continue;
            };
            self.collect_interface_members(&interface.members, &mut parts);
        }

        // Assign declaration_order in FORWARD declaration order for diagnostics.
        // The reverse iteration above is needed for overload resolution priority,
        // but TS2740 "missing properties" messages should list properties in the
        // order they first appear across declarations (earliest declaration first).
        self.assign_forward_declaration_order(&mut parts, declarations.iter().copied());

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

    pub(super) fn collect_interface_members(&self, members: &NodeList, parts: &mut InterfaceParts) {
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
                            let readonly = self.arena.has_modifier(
                                &sig.modifiers,
                                tsz_scanner::SyntaxKind::ReadonlyKeyword,
                            );
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
                    let order = parts.next_declaration_order();
                    // Merge with existing accessor entry or create new one
                    match parts.properties.entry(name) {
                        indexmap::map::Entry::Occupied(mut entry) => {
                            // Update existing property with getter type as read type
                            if let PropertyMerge::Property(prop) = entry.get_mut() {
                                prop.type_id = getter_type;
                                // Getter-only means readonly; if a setter was already
                                // merged, its branch will have set readonly=false already
                                // and we preserve that (both accessor present = not readonly).
                            }
                        }
                        indexmap::map::Entry::Vacant(entry) => {
                            entry.insert(PropertyMerge::Property(PropertyInfo {
                                name,
                                type_id: getter_type,
                                write_type: getter_type,
                                optional: false,
                                readonly: true,
                                is_method: false,
                                is_class_prototype: false,
                                visibility: Visibility::Public,
                                parent_id: None,
                                declaration_order: order,
                                is_string_named: false,
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
                    let order = parts.next_declaration_order();
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
                                is_class_prototype: false,
                                visibility: Visibility::Public,
                                parent_id: None,
                                declaration_order: order,
                                is_string_named: false,
                            }));
                        }
                    }
                }
            }
        }
    }

    /// Assign `declaration_order` values by iterating declarations in FORWARD order.
    /// This gives earlier declarations lower order numbers, matching tsc's property
    /// enumeration for diagnostics like TS2740 "missing properties: length, pop, ...".
    fn assign_forward_declaration_order(
        &self,
        parts: &mut InterfaceParts,
        declarations: impl Iterator<Item = NodeIndex>,
    ) {
        let mut counter: u32 = 0;
        for decl_idx in declarations {
            let Some(node) = self.arena.get(decl_idx) else {
                continue;
            };
            let Some(interface) = self.arena.get_interface(node) else {
                continue;
            };
            for &idx in &interface.members.nodes {
                if let Some(name) = self.get_interface_member_name(idx) {
                    parts.declaration_orders.entry(name).or_insert_with(|| {
                        counter += 1;
                        counter
                    });
                }
            }
        }
    }

    /// Cross-file variant of `assign_forward_declaration_order`.
    fn assign_forward_declaration_order_cross_file(
        &self,
        parts: &mut InterfaceParts,
        declarations: &[(NodeIndex, &NodeArena)],
    ) {
        let mut counter: u32 = 0;
        for &(decl_idx, decl_arena) in declarations {
            let Some(node) = decl_arena.get(decl_idx) else {
                continue;
            };
            let Some(interface) = decl_arena.get_interface(node) else {
                continue;
            };
            let lowerer = self.with_arena(decl_arena);
            for &idx in &interface.members.nodes {
                if let Some(name) = lowerer.get_interface_member_name(idx) {
                    parts.declaration_orders.entry(name).or_insert_with(|| {
                        counter += 1;
                        counter
                    });
                }
            }
        }
    }

    /// Extract the property/method name from an interface member node.
    fn get_interface_member_name(&self, idx: NodeIndex) -> Option<Atom> {
        let member = self.arena.get(idx)?;
        if let Some(sig) = self.arena.get_signature(member) {
            return self.lower_signature_name(sig.name);
        }
        if (member.kind == syntax_kind_ext::GET_ACCESSOR
            || member.kind == syntax_kind_ext::SET_ACCESSOR)
            && let Some(accessor) = self.arena.get_accessor(member)
        {
            return self.lower_signature_name(accessor.name);
        }
        None
    }

    fn finish_interface_parts(
        &self,
        parts: InterfaceParts,
        symbol_id: Option<tsz_binder::SymbolId>,
    ) -> TypeId {
        let mut properties = Vec::with_capacity(parts.properties.len());
        for (name, entry) in parts.properties {
            // Use forward declaration order when available (corrects reverse iteration order)
            let forward_order = parts.declaration_orders.get(&name).copied();
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
                    is_class_prototype: false,
                    visibility: Visibility::Public,
                    parent_id: None,
                    declaration_order: forward_order.unwrap_or(methods.declaration_order),
                    is_string_named: false,
                });
            } else if let PropertyMerge::Property(mut prop) = entry {
                if let Some(order) = forward_order {
                    prop.declaration_order = order;
                }
                properties.push(prop);
            } else if let PropertyMerge::Conflict(mut prop) = entry {
                if let Some(order) = forward_order {
                    prop.declaration_order = order;
                }
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
                is_abstract: false,
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
                symbol: symbol_id,
                ..ObjectShape::default()
            });
        }

        self.interner
            .object_with_flags_and_symbol(properties, Default::default(), symbol_id)
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
        {
            if let Some(symbol_name) = self.get_well_known_symbol_name(computed.expression) {
                return Some(self.interner.intern_string(&symbol_name));
            }
            // Try the computed name resolver for user-defined computed properties
            // (e.g., [k] where k is a unique symbol variable)
            if let Some(resolver) = self.computed_name_resolver
                && let Some(name) = resolver(computed.expression)
            {
                return Some(name);
            }
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
        let readonly = self
            .arena
            .has_modifier(&sig.modifiers, tsz_scanner::SyntaxKind::ReadonlyKeyword);

        let param_name = self
            .arena
            .get(param_data.name)
            .and_then(|name_node| self.arena.get_identifier(name_node))
            .map(|name_ident| self.interner.intern_string(&name_ident.escaped_text));

        Some(IndexSignature {
            key_type,
            value_type,
            readonly,
            param_name,
        })
    }

    const fn index_signature_properties_compatible(
        &self,
        _properties: &[PropertyInfo],
        _string_index: Option<&IndexSignature>,
        _number_index: Option<&IndexSignature>,
    ) -> bool {
        true
    }

    /// Lower a type element (property signature, method signature, etc.)
    fn lower_type_element(&self, node_idx: NodeIndex) -> Option<PropertyInfo> {
        let node = self.arena.get(node_idx)?;

        // Check if it's a property or method signature
        if let Some(sig) = self.arena.get_signature(node) {
            // Get property name as Arc<str>
            let name = self.lower_signature_name(sig.name)?;

            // Check for readonly modifier
            let readonly = self
                .arena
                .has_modifier(&sig.modifiers, tsz_scanner::SyntaxKind::ReadonlyKeyword);

            // Get visibility (for type literals, always Public)
            let visibility = self.arena.get_visibility_from_modifiers(&sig.modifiers);
            let type_id = self.lower_type(sig.type_annotation);
            let write_type = if readonly { TypeId::NONE } else { type_id };

            Some(PropertyInfo {
                name,
                type_id,
                write_type,
                optional: sig.question_token,
                readonly,
                is_method: false,
                is_class_prototype: false,
                visibility,
                parent_id: None, // Type literals don't have parent_id
                declaration_order: 0,
                is_string_named: false,
            })
        } else {
            None
        }
    }
}
