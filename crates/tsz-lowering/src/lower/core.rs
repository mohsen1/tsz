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
use std::sync::Arc;
use tsz_common::interner::Atom;
use tsz_parser::parser::NodeList;
use tsz_parser::parser::base::NodeIndex;
use tsz_parser::parser::node::{IndexSignatureData, NodeArena, SignatureData, TypeAliasData};
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::construction::{QueryDatabase, TypeDatabase};
use tsz_solver::def::DefId;
use tsz_solver::types::{
    CallSignature, CallableShape, FunctionShape, IndexSignature, ObjectFlags, ObjectShape,
    ParamInfo, PropertyInfo, TupleElement, TypeId, TypeParamInfo, TypeParamOrigin, TypePredicate,
    Visibility,
};

use super::host::{ClosureLoweringHost, LoweringHost};

/// #14344 STEP-B flag (default-OFF). When `TSZ_TYPEPARAM_DECL_IDENTITY=1`, the
/// dominant lowering construction path (`collect_type_parameters`) stamps each
/// user type parameter's origin with its declaration site `(file, name_node)`,
/// so distinct declarations sharing identical surface info intern distinctly —
/// the activation that reaches the fp-ts self-ref-guard collapse (the 257).
/// Measure-only until the activation gate holds; flag-OFF = `User` (byte-parity).
fn decl_identity_activation() -> bool {
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("TSZ_TYPEPARAM_DECL_IDENTITY").is_ok_and(|v| v == "1"))
}

mod signature_members;

#[cfg(test)]
mod constructor_parity_tests;

/// Fetch an arena node and, optionally, its typed payload, short-circuiting the
/// enclosing handler with a caller-supplied fallback when either is absent.
///
/// Every `lower_*` handler opens with the same prologue: fetch the node from
/// the arena (or return early), then fetch its node-specific typed data (or
/// return early). This macro is the single home for that guard so call sites
/// express only the expected variant and the per-node work. The fallback is
/// explicit at each call so the rare non-`ERROR` policies (e.g. an empty tuple
/// or object literal) stay deliberate instead of drifting silently.
///
/// Two forms:
/// - `lower_node_data!(self, idx; fallback)` yields the raw `&Node`.
/// - `lower_node_data!(self, idx, getter, fallback)` yields the typed payload
///   produced by `self.arena.getter(node)`.
macro_rules! lower_node_data {
    ($self:ident, $node_idx:expr; $fallback:expr $(,)?) => {{
        let Some(node) = $self.arena.get($node_idx) else {
            return $fallback;
        };
        node
    }};
    ($self:ident, $node_idx:expr, $getter:ident, $fallback:expr $(,)?) => {{
        let Some(node) = $self.arena.get($node_idx) else {
            return $fallback;
        };
        let Some(data) = $self.arena.$getter(node) else {
            return $fallback;
        };
        data
    }};
}

pub(super) use lower_node_data;

/// Maximum number of type lowering operations to prevent infinite loops
pub const MAX_LOWERING_OPERATIONS: u32 = 100_000;

pub(super) type NodeIndexResolver<'a, T> = dyn Fn(NodeIndex) -> Option<T> + 'a;
pub(super) type TypeIdResolver<'a> = dyn Fn(&str) -> Option<DefId> + 'a;
pub(super) type LazyTypeParamsResolver<'a> = dyn Fn(DefId) -> Option<Vec<TypeParamInfo>> + 'a;
#[derive(Clone)]
pub(super) struct TypeParamBinding {
    type_id: TypeId,
}

pub(super) type TypeParamScope = IndexMap<Arc<str>, TypeParamBinding>;
pub(super) type TypeParamScopeStack = RefCell<Vec<TypeParamScope>>;
pub(super) type TypeofParamScopeStack = RefCell<Vec<Vec<(Atom, TypeId)>>>;
pub type LoweredInterfaceMemberTypes = (Vec<TypeParamInfo>, Vec<(NodeIndex, TypeId)>);

/// Type lowering context.
/// Converts AST type nodes into interned `TypeIds`.
pub struct TypeLowering<'a> {
    pub(super) arena: &'a NodeArena,
    pub(super) interner: &'a dyn TypeDatabase,
    /// Single resolver boundary. Replaces the former twelve `Option<&dyn Fn>`
    /// callback fields: name/symbol/`DefId`/computed-name/lazy-type-param/
    /// type-query/import-type resolution all dispatch through this host, so the
    /// active capability set is a property of the host value rather than of each
    /// construction site. The public `new`/`with_*` constructors and `with_*`
    /// builders populate a `ClosureLoweringHost`.
    pub(super) host: ClosureLoweringHost<'a>,
    /// Optional compiler-controlled intrinsic replacement for the lib-only
    /// `BuiltinIteratorReturn` alias.
    pub(super) builtin_iterator_return_type: Option<TypeId>,
    /// When true, prefer identifier-text `DefId` resolution over raw NodeIndex-based
    /// resolution. This is needed for cross-arena lowering where the same `NodeIndex`
    /// may refer to different identifiers in different arenas.
    pub(super) prefer_name_def_id_resolution: bool,
    /// Optional direct self-reference for merged interface lowering.
    pub(super) preferred_self_name: Option<String>,
    pub(super) preferred_self_def_id: Option<DefId>,
    /// Type parameter scopes - wrapped in Rc for sharing across arena contexts
    pub(super) type_param_scopes: Rc<TypeParamScopeStack>,
    /// Value-parameter scopes for `typeof paramName` in signature return types.
    pub(super) typeof_param_scopes: Rc<TypeofParamScopeStack>,
    /// Whether strictNullChecks is enabled. When true, optional parameters
    /// in function types include `| undefined` in their type.
    pub(super) strict_null_checks: bool,
    /// Opt-in gate for the non-strict nullish union reduction (`nonstrict_nullish`).
    pub(super) nonstrict_nullish_union_reduction: bool,
    /// Operation counter to prevent infinite loops
    pub(super) operations: Rc<RefCell<u32>>,
    /// Whether the operation limit has been exceeded
    pub(super) limit_exceeded: Rc<RefCell<bool>>,
}

pub(super) struct ObjectTypeParts {
    // Use IndexMap for deterministic property order - this ensures
    // the same object type produces the same TypeId on every lowering.
    // FxHashMap has undefined iteration order, causing non-determinism.
    pub(super) properties: IndexMap<Atom, PropertyMerge>,
    pub(super) call_signatures: Vec<CallSignature>,
    pub(super) construct_signatures: Vec<CallSignature>,
    pub(super) string_index: Option<IndexSignature>,
    /// Additional string-keyed index signatures whose key type differs from
    /// `string_index.key_type`. Merged into `string_index` via key-type union
    /// in `finish_object_type_parts`, where the type interner is available.
    pub(super) extra_string_indices: Vec<IndexSignature>,
    pub(super) number_index: Option<IndexSignature>,
    /// Symbol-keyed index signature (`[k: symbol]: V`). Tracked separately so a
    /// type can carry a `symbol` index alongside `string`/`number` ones
    /// (e.g. `{ [k: string]: A; [k: symbol]: B }`) without the two colliding.
    pub(super) symbol_index: Option<IndexSignature>,
    /// Additional symbol-keyed index signatures beyond the first, collected for
    /// a deferred value-type union in `finish_object_type_parts` (the type
    /// interner is unavailable at merge time). tsc folds several computed
    /// members keyed by a plain `symbol` (`interface T { [s1]: A; [s2]: B }`)
    /// into one `[key: symbol]` index whose value is the UNION of their value
    /// types (`[key: symbol]: A | B`) — mirroring the `extra_string_indices`
    /// deferral for distinct string-key patterns. Only the implicit-from-
    /// computed-name form routes here (via `merge_implicit_symbol_index`); two
    /// *explicit* `[k: symbol]: T` declarations keep the duplicate-index
    /// error-collapse in `merge_index_signature` that pairs with TS2374.
    pub(super) extra_symbol_indices: Vec<IndexSignature>,
    /// True when at least one member has a computed property name that could not
    /// be resolved to a literal string/symbol key (e.g. `[sym]` where `sym` has
    /// type `symbol` rather than a unique-symbol type).  The resulting object
    /// type must carry `ObjectFlags::HAS_LATE_BOUND_MEMBERS` so that indexed
    /// access via a `symbol`-typed key correctly returns `any` instead of
    /// `undefined`.
    pub(super) has_late_bound_members: bool,
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
    pub(super) is_symbol_named: bool,
    pub(super) is_string_named: bool,
    pub(super) single_quoted_name: bool,
    /// Declaration order of the first occurrence of this method, for diagnostic ordering.
    pub(super) declaration_order: u32,
}

impl ObjectTypeParts {
    /// Stride between declaration passes. Must be larger than the maximum number
    /// of properties any single interface declaration contributes.
    const DECL_ORDER_STRIDE: u32 = 10_000;

    pub(super) fn new() -> Self {
        Self {
            properties: IndexMap::new(),
            call_signatures: Vec::new(),
            construct_signatures: Vec::new(),
            string_index: None,
            extra_string_indices: Vec::new(),
            number_index: None,
            symbol_index: None,
            extra_symbol_indices: Vec::new(),
            has_late_bound_members: false,
            current_pass_base: 0,
            // 1-based: declaration_order 0 is the interner constructors'
            // "unset" sentinel (they backfill it from vec order).
            pass_local_counter: 1,
            declaration_orders: rustc_hash::FxHashMap::default(),
        }
    }

    /// Set the declaration pass base for the next batch of properties.
    ///
    /// `forward_decl_index` is the 0-based index of the declaration in
    /// forward (source) order, so the earliest declaration gets index 0.
    /// Pass-local orders are 1-based so no property keeps `declaration_order`
    /// of 0, which the interner constructors treat as "unset" and backfill.
    pub(super) const fn set_declaration_pass(&mut self, forward_decl_index: usize) {
        self.current_pass_base = (forward_decl_index as u32) * Self::DECL_ORDER_STRIDE;
        self.pass_local_counter = 1;
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
                        is_symbol_named: false,
                        single_quoted_name: false,
                        non_widening: false,
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
                        is_symbol_named: false,
                        single_quoted_name: false,
                        non_widening: false,
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
        is_symbol_named: bool,
        is_string_named: bool,
        single_quoted_name: bool,
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
                    is_symbol_named,
                    is_string_named,
                    single_quoted_name,
                    declaration_order: next_order,
                }));
            }
            Entry::Occupied(mut entry) => match entry.get_mut() {
                PropertyMerge::Method(methods) => {
                    methods.signatures.push(signature);
                    methods.optional |= optional;
                    methods.readonly &= readonly;
                    methods.is_symbol_named |= is_symbol_named;
                    methods.is_string_named |= is_string_named;
                    methods.single_quoted_name |= single_quoted_name;
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
                        is_symbol_named: false,
                        single_quoted_name: false,
                        non_widening: false,
                    };
                    entry.insert(PropertyMerge::Conflict(conflict));
                }
                PropertyMerge::Conflict(_) => {}
            },
        }
    }

    pub(super) fn merge_index_signature(&mut self, index: IndexSignature) {
        if index.key_type == TypeId::NUMBER {
            if let Some(existing) = self.number_index.as_mut() {
                if existing.value_type != index.value_type || existing.readonly != index.readonly {
                    existing.value_type = TypeId::ERROR;
                    existing.readonly = false;
                }
            } else {
                self.number_index = Some(index);
            }
            return;
        }

        if index.key_type == TypeId::SYMBOL {
            if let Some(existing) = self.symbol_index.as_mut() {
                if existing.value_type != index.value_type || existing.readonly != index.readonly {
                    existing.value_type = TypeId::ERROR;
                    existing.readonly = false;
                }
            } else {
                self.symbol_index = Some(index);
            }
            return;
        }

        if let Some(existing) = self.string_index.as_mut() {
            if existing.key_type == index.key_type {
                if existing.value_type != index.value_type || existing.readonly != index.readonly {
                    existing.value_type = TypeId::ERROR;
                    existing.readonly = false;
                }
            } else {
                // Distinct pattern: defer key-type union to finish_object_type_parts
                // where the type interner is available.
                self.extra_string_indices.push(index);
            }
        } else {
            self.string_index = Some(index);
        }
    }

    /// Merge an *implicit* symbol index signature — one synthesized from a
    /// computed member whose key is a plain `symbol` (`[s]: V`). Unlike an
    /// explicit `[k: symbol]: T` declaration (which goes through
    /// `merge_index_signature` and error-collapses on a duplicate), tsc folds
    /// several such members into one `[key: symbol]` index by UNIONING their
    /// value types. The interner is not available at merge time, so extras are
    /// collected and unioned in `finish_object_type_parts` — the same deferral
    /// `extra_string_indices` uses for distinct string-key patterns.
    pub(super) fn merge_implicit_symbol_index(&mut self, value_type: TypeId, readonly: bool) {
        let index = IndexSignature {
            key_type: TypeId::SYMBOL,
            value_type,
            readonly,
            param_name: None,
        };
        if self.symbol_index.is_none() {
            self.symbol_index = Some(index);
        } else {
            self.extra_symbol_indices.push(index);
        }
    }
}

/// Merge missing defaults/constraints from a subsequent declaration's type
/// parameters into the already-collected parameters. Interface merging keeps
/// the first declaration's parameter list but later declarations may carry the
/// default or constraint (e.g. `Uint8Array` declares its default in
/// lib.es5.d.ts while es2015.iterable.d.ts omits it).
fn merge_type_param_metadata(collected: &mut [TypeParamInfo], extra: Vec<TypeParamInfo>) {
    for (i, ep) in extra.into_iter().enumerate() {
        let Some(cp) = collected.get_mut(i) else {
            break;
        };
        cp.default = cp.default.or(ep.default);
        cp.constraint = cp.constraint.or(ep.constraint);
    }
}

/// Resolver bundle threaded into the various `TypeLowering` constructors.
///
/// Only the resolver fields differ across the public entry points (`new`,
/// `with_resolver`, `with_resolvers`, `with_def_id_resolver`,
/// `with_hybrid_resolver`); every other field is initialized identically.
/// This bundle lets the public constructors share the single private
/// `from_resolvers` builder below, eliminating five copies of the same
/// 17-field literal.
#[derive(Default)]
pub(super) struct LoweringResolvers<'a> {
    pub(super) type_resolver: Option<&'a NodeIndexResolver<'a, u32>>,
    pub(super) def_id_resolver: Option<&'a NodeIndexResolver<'a, DefId>>,
    pub(super) value_resolver: Option<&'a NodeIndexResolver<'a, u32>>,
}

impl<'a> TypeLowering<'a> {
    /// Single private builder used by all public constructors. The five
    /// `pub fn` entry points only differ in which resolver fields they
    /// populate; everything else (interning, scope stack, operation
    /// counter, limit flag, `None`/`false` defaults) is initialized here
    /// once.
    fn from_resolvers(
        arena: &'a NodeArena,
        interner: &'a dyn QueryDatabase,
        resolvers: LoweringResolvers<'a>,
    ) -> Self {
        TypeLowering {
            arena,
            interner: interner.as_type_database(),
            host: ClosureLoweringHost {
                type_resolver: resolvers.type_resolver,
                def_id_resolver: resolvers.def_id_resolver,
                value_resolver: resolvers.value_resolver,
                ..ClosureLoweringHost::default()
            },
            builtin_iterator_return_type: None,
            prefer_name_def_id_resolution: false,
            preferred_self_name: None,
            preferred_self_def_id: None,
            strict_null_checks: false,
            nonstrict_nullish_union_reduction: false,
            type_param_scopes: Rc::new(RefCell::new(Vec::new())),
            typeof_param_scopes: Rc::new(RefCell::new(Vec::new())),
            operations: Rc::new(RefCell::new(0)),
            limit_exceeded: Rc::new(RefCell::new(false)),
        }
    }

    pub fn new(arena: &'a NodeArena, interner: &'a dyn QueryDatabase) -> Self {
        Self::from_resolvers(arena, interner, LoweringResolvers::default())
    }

    /// Create a `TypeLowering` with a symbol resolver.
    /// The resolver converts identifier names to actual `SymbolIds` from the binder.
    pub fn with_resolver(
        arena: &'a NodeArena,
        interner: &'a dyn QueryDatabase,
        resolver: &'a dyn Fn(NodeIndex) -> Option<u32>,
    ) -> Self {
        Self::from_resolvers(
            arena,
            interner,
            LoweringResolvers {
                type_resolver: Some(resolver),
                def_id_resolver: None,
                value_resolver: Some(resolver),
            },
        )
    }

    /// Create a `TypeLowering` with separate type/value resolvers.
    pub fn with_resolvers(
        arena: &'a NodeArena,
        interner: &'a dyn QueryDatabase,
        type_resolver: &'a dyn Fn(NodeIndex) -> Option<u32>,
        value_resolver: &'a dyn Fn(NodeIndex) -> Option<u32>,
    ) -> Self {
        Self::from_resolvers(
            arena,
            interner,
            LoweringResolvers {
                type_resolver: Some(type_resolver),
                def_id_resolver: None,
                value_resolver: Some(value_resolver),
            },
        )
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
        Self::from_resolvers(
            arena,
            interner,
            LoweringResolvers {
                type_resolver: None,
                def_id_resolver: Some(def_id_resolver),
                value_resolver: Some(value_resolver),
            },
        )
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
        Self::from_resolvers(
            arena,
            interner,
            LoweringResolvers {
                type_resolver: Some(type_resolver),
                def_id_resolver: Some(def_id_resolver),
                value_resolver: Some(value_resolver),
            },
        )
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
            // The whole resolver set travels as one copy of the host.
            host: self.host.clone(),
            builtin_iterator_return_type: self.builtin_iterator_return_type,
            prefer_name_def_id_resolution: self.prefer_name_def_id_resolution,
            preferred_self_name: self.preferred_self_name.clone(),
            preferred_self_def_id: self.preferred_self_def_id,
            strict_null_checks: self.strict_null_checks,
            nonstrict_nullish_union_reduction: self.nonstrict_nullish_union_reduction,
            // Rc::clone() shares the underlying Rc instead of copying data
            type_param_scopes: Rc::clone(&self.type_param_scopes),
            typeof_param_scopes: Rc::clone(&self.typeof_param_scopes),
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

        let mut parts = ObjectTypeParts::new();
        let mut type_params_collected = false;
        let mut collected_params = Vec::new();
        let mut lowered_interface_decls = 0usize;

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

        // Process declarations in FORWARD (lib-load / source) order: TypeScript 7
        // preserves declaration order for merged-interface overload sets, so an
        // earlier declaration's overloads render before a later declaration's
        // (e.g. Array.toLocaleString renders the es5 `(): string` overload before
        // the es2015.core `(locales, options): string` overload).
        let num_declarations = declarations.len();
        for (forward_decl_index, (decl_idx, decl_arena)) in declarations.iter().enumerate() {
            // Set the declaration pass base so that properties from earlier
            // (forward) declarations get lower declaration_order values.
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
            lowered_interface_decls += 1;

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
                    merge_type_param_metadata(&mut collected_params, extra);
                }
            }

            // Collect members using the arena-specific lowerer
            lowerer.collect_object_type_members(&interface.members, &mut parts);
        }

        // No declaration actually lowered as an interface: every pair either
        // pointed at a missing node or at a non-interface node (foreign-arena
        // `NodeIndex` collisions). Synthesizing an empty object here would
        // manufacture a memberless body for a real interface — that body then
        // leaks into shared definition state and produces false "property
        // does not exist" diagnostics (issue #13255). A genuinely empty
        // `interface Empty {}` still lowers normally: its declaration parses
        // as an interface and increments the counter.
        if lowered_interface_decls == 0 {
            tracing::debug!(
                num_declarations,
                symbol_id = ?symbol_id,
                "no merged interface declaration lowered; returning error type"
            );
            if type_params_collected {
                self.pop_type_param_scope();
            }
            return (TypeId::ERROR, collected_params);
        }

        // Assign declaration_order in FORWARD declaration order for diagnostics.
        self.assign_forward_declaration_order_cross_file(&mut parts, declarations);

        let result = self.finish_object_type_parts(parts, symbol_id);

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
                merge_type_param_metadata(&mut collected, extra);
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
            let scope = bindings
                .into_iter()
                .map(|(name, type_id)| {
                    (
                        self.interner.resolve_atom_ref(name),
                        TypeParamBinding { type_id },
                    )
                })
                .collect();
            *self.type_param_scopes.borrow_mut() = vec![scope];
        }
        self
    }

    /// Enable strictNullChecks behavior. When set, optional parameters in
    /// function types include `| undefined` in their type.
    pub const fn with_strict_null_checks(mut self, enabled: bool) -> Self {
        self.strict_null_checks = enabled;
        self
    }

    /// Opt this lowering into tsc's non-strict-mode `null`/`undefined` union
    /// reduction, wiring the real `strictNullChecks` in the same call so it
    /// can never fire under `--strict`. See `lower_union_type`.
    pub const fn with_nonstrict_nullish_union_reduction(
        mut self,
        strict_null_checks: bool,
    ) -> Self {
        self.strict_null_checks = strict_null_checks;
        self.nonstrict_nullish_union_reduction = true;
        self
    }

    /// Set the computed property name resolver for resolving computed property
    /// names like `[k]` where k is a unique symbol variable.
    pub fn with_computed_name_resolver(
        mut self,
        resolver: &'a dyn Fn(NodeIndex) -> Option<Atom>,
    ) -> Self {
        self.host.computed_name_resolver = Some(resolver);
        self
    }

    /// Arena-aware computed property name resolver. Receives `(expr_idx,
    /// arena_ptr)` where `arena_ptr` is the currently-active declaration
    /// arena. Use this instead of `with_computed_name_resolver` when
    /// processing cross-arena merged interfaces, where the same `NodeIndex`
    /// value can refer to different nodes in different arenas.
    pub fn with_computed_name_resolver_with_arena(
        mut self,
        resolver: &'a dyn Fn(NodeIndex, *const NodeArena) -> Option<Atom>,
    ) -> Self {
        self.host.computed_name_resolver_with_arena = Some(resolver);
        self
    }

    /// Set metadata for computed property names that are symbol-valued.
    pub fn with_computed_symbol_name_resolver(
        mut self,
        resolver: &'a dyn Fn(NodeIndex) -> bool,
    ) -> Self {
        self.host.computed_symbol_name_resolver = Some(resolver);
        self
    }

    /// Arena-aware variant of `with_computed_symbol_name_resolver`. Receives
    /// `(expr_idx, arena_ptr)` alongside the node index.
    pub fn with_computed_symbol_name_resolver_with_arena(
        mut self,
        resolver: &'a dyn Fn(NodeIndex, *const NodeArena) -> bool,
    ) -> Self {
        self.host.computed_symbol_name_resolver_with_arena = Some(resolver);
        self
    }

    /// Set the resolver for computed property names that are keyed by a
    /// plain (non-unique) `symbol`-typed binding. Such members route into
    /// the containing type's symbol index signature instead of minting a
    /// named member — see `LoweringHost::computed_name_is_wide_symbol`.
    pub fn with_computed_wide_symbol_name_resolver(
        mut self,
        resolver: &'a dyn Fn(NodeIndex) -> bool,
    ) -> Self {
        self.host.computed_wide_symbol_name_resolver = Some(resolver);
        self
    }

    /// Arena-aware variant of `with_computed_wide_symbol_name_resolver`.
    pub fn with_computed_wide_symbol_name_resolver_with_arena(
        mut self,
        resolver: &'a dyn Fn(NodeIndex, *const NodeArena) -> bool,
    ) -> Self {
        self.host.computed_wide_symbol_name_resolver_with_arena = Some(resolver);
        self
    }

    /// Set the lazy type parameter resolver for applying omitted defaulted type arguments
    /// when lowering lazy references from interface members.
    pub fn with_lazy_type_params_resolver(
        mut self,
        resolver: &'a dyn Fn(DefId) -> Option<Vec<TypeParamInfo>>,
    ) -> Self {
        self.host.lazy_type_params_resolver = Some(resolver);
        self
    }

    /// Replace lib `BuiltinIteratorReturn` references while lowering in
    /// checker-controlled lib/source-file direct paths. Normal user lowering
    /// leaves this unset so a user alias with the same name can still shadow it.
    pub const fn with_builtin_iterator_return_type(mut self, ty: TypeId) -> Self {
        self.builtin_iterator_return_type = Some(ty);
        self
    }

    /// Set the name-based `DefId` resolver for cross-arena resolution.
    pub fn with_name_def_id_resolver(
        mut self,
        resolver: &'a dyn Fn(&str) -> Option<DefId>,
    ) -> Self {
        self.host.name_def_id_resolver = Some(resolver);
        self
    }

    /// Set the local-shadow `DefId` resolver. It is consulted before the
    /// name-first resolution (see `prefer_name_def_id_resolution`) and returns a
    /// `DefId` only when a simple identifier resolves to a function- or
    /// block-local declaration shadowing a same-named file-level type.
    pub fn with_local_shadow_def_id_resolver(
        mut self,
        resolver: &'a dyn Fn(NodeIndex) -> Option<DefId>,
    ) -> Self {
        self.host.local_shadow_def_id_resolver = Some(resolver);
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
        self.host.type_query_override = Some(resolver);
        self
    }

    /// Attach an import-type resolver so that `TYPE_REFERENCE` nodes whose `type_name`
    /// is or starts with an `import()` call can be resolved during lowering.
    ///
    /// The checker pre-resolves such references (which require module resolution) and
    /// supplies this callback. Without it, `import("./x").Foo` in type position
    /// (e.g. the extends clause of a conditional type) would lower to `TypeId::ERROR`.
    pub fn with_import_type_resolver(
        mut self,
        resolver: &'a NodeIndexResolver<'a, TypeId>,
    ) -> Self {
        self.host.import_type_resolver = Some(resolver);
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
        self.host.resolve_def_id_by_name(name)
    }

    /// Build the full text of an entity-name type node from the current arena.
    ///
    /// This is used by cross-arena lowering to resolve qualified names (e.g.
    /// `Intl.NumberFormatOptions`) through the name-based `DefId` resolver
    /// instead of relying on arena-local `NodeIndex` values.
    pub(super) fn type_name_text(&self, node_idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(node_idx)?;

        if node.is_identifier() {
            return self
                .arena
                .get_identifier(node)
                .map(|ident| ident.escaped_text.to_string());
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
        if !node.is_identifier() {
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
                && name_node.is_identifier()
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
        self.host.resolve_type_symbol(node_idx)
    }

    /// Resolve a node to a `DefId` if a `DefId` resolver is provided.
    ///
    /// `DefIds` are Solver-owned identifiers that don't require Binder context.
    pub(super) fn resolve_def_id(&self, node_idx: NodeIndex) -> Option<DefId> {
        self.host.resolve_def_id(node_idx)
    }

    /// Resolve a node to the `DefId` of a function- or block-local declaration
    /// that shadows a same-named file-level type, if such a resolver is provided.
    /// Returns `None` for every non-shadowing reference.
    pub(super) fn resolve_local_shadow_def_id(&self, node_idx: NodeIndex) -> Option<DefId> {
        self.host.resolve_local_shadow_def_id(node_idx)
    }

    /// Resolve a node to a value symbol ID if a resolver is provided.
    pub(super) fn resolve_value_symbol(&self, node_idx: NodeIndex) -> Option<u32> {
        self.host.resolve_value_symbol(node_idx)
    }

    pub(super) fn push_type_param_scope(&self) {
        self.type_param_scopes.borrow_mut().push(IndexMap::new());
    }

    pub(super) fn pop_type_param_scope(&self) {
        let _ = self.type_param_scopes.borrow_mut().pop();
    }

    pub(super) fn add_type_param_binding(&self, name: Atom, type_id: TypeId) {
        if let Some(scope) = self.type_param_scopes.borrow_mut().last_mut() {
            scope.insert(
                self.interner.resolve_atom_ref(name),
                TypeParamBinding { type_id },
            );
        }
    }

    pub(super) fn update_type_param_binding(&self, name: Atom, type_id: TypeId) {
        if let Some(scope) = self.type_param_scopes.borrow_mut().last_mut()
            && let Some(existing) = scope.get_mut(self.interner.resolve_atom_ref(name).as_ref())
        {
            existing.type_id = type_id;
        }
    }

    pub(super) fn lookup_type_param(&self, name: &str) -> Option<TypeId> {
        let scopes = self.type_param_scopes.borrow();
        for scope in scopes.iter().rev() {
            if let Some(binding) = scope.get(name) {
                return Some(binding.type_id);
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

        let node = lower_node_data!(self, node_idx; TypeId::ERROR);

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
        let data = lower_node_data!(self, node_idx, get_composite_type, TypeId::ERROR);
        let members: Vec<TypeId> = data
            .types
            .nodes
            .iter()
            .map(|&idx| self.lower_type(idx))
            .collect();
        // Non-strict-mode `null`/`undefined` union reduction, applied only
        // when a site opted in via `with_nonstrict_nullish_union_reduction` —
        // which also wires the real `strictNullChecks`, so an unwired site
        // (whose `strict_null_checks` silently defaults to `false`) can never
        // collapse a union under `--strict`. The `!strict_null_checks` gate
        // makes strict builds skip both solver calls entirely. Solver-owned
        // and identical to the rule `tsz-checker`'s type-node resolvers apply;
        // this is the seam for object/interface members (index signatures, and
        // heritage-bearing or multiply-declared interfaces) that fall through
        // to this lowering rather than the checker fast-path. See #16620.
        let members = if self.nonstrict_nullish_union_reduction && !self.strict_null_checks {
            match tsz_solver::narrowing::collapse_pure_nullish_union_nonstrict(
                self.strict_null_checks,
                &members,
            ) {
                // Pure `null | undefined` resolves to a bare nullish scalar —
                // no `Union`, so no origin to record.
                Some(collapsed) => return collapsed,
                None => tsz_solver::narrowing::nonstrict_union_members_absorb_nullish_scalars(
                    self.strict_null_checks,
                    &members,
                )
                .unwrap_or(members),
            }
        } else {
            members
        };
        // Mirror tsc's `UnionType.origin`: record the as-written input
        // member list so the printer can render `0 | 1 | 2` in source
        // order even when the canonical sort uses non-deterministic
        // alloc-order for non-zero number literals.
        let result = self.interner.union_literal_reduce(members.clone());
        self.interner.store_union_origin(result, members);
        result
    }

    /// Lower an intersection type (A & B & C)
    fn lower_intersection_type(&self, node_idx: NodeIndex) -> TypeId {
        let data = lower_node_data!(self, node_idx, get_composite_type, TypeId::ERROR);
        let members: Vec<TypeId> = data
            .types
            .nodes
            .iter()
            .map(|&idx| self.lower_type(idx))
            .collect();
        self.interner.intersection(members)
    }

    /// Lower an array type (T[])
    fn lower_array_type(&self, node_idx: NodeIndex) -> TypeId {
        // Missing node or array type data - propagate error.
        let data = lower_node_data!(self, node_idx, get_array_type, TypeId::ERROR);
        let element_type = self.lower_type(data.element_type);
        self.interner.array(element_type)
    }

    /// Lower a tuple type ([A, B, C])
    fn lower_tuple_type(&self, node_idx: NodeIndex) -> TypeId {
        // Deliberate divergence from the `TypeId::ERROR` sibling fallback: a
        // tuple node with missing typed data lowers to the empty tuple `[]`
        // rather than an error type, preserving long-standing tuple-lowering
        // behavior. This is now an explicit policy rather than an accidental
        // hand-written else-branch.
        let data = lower_node_data!(self, node_idx, get_tuple_type, self.interner.tuple(vec![]));
        let elements: Vec<TupleElement> = data
            .elements
            .nodes
            .iter()
            .map(|&idx| self.lower_tuple_element(idx))
            .collect();
        self.interner.tuple(elements)
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

            let type_id = if data.dot_dot_dot_token {
                self.lower_rest_position_type(data.type_node)
            } else {
                self.lower_type(data.type_node)
            };
            return TupleElement {
                type_id,
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

            let is_rest = node.kind == syntax_kind_ext::REST_TYPE;
            let type_id = match wrapped {
                None => self.lower_type(node_idx),
                Some(inner) if is_rest => self.lower_rest_position_type(inner),
                Some(inner) => self.lower_type(inner),
            };
            return TupleElement {
                type_id,
                name: None,
                optional: node.kind == syntax_kind_ext::OPTIONAL_TYPE,
                rest: is_rest,
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

    /// #14344 dispatch-split: a zero-cost dispatcher that reads the activation
    /// flag once and tail-calls the matching leaf. The split is a CODEGEN
    /// requirement, not just organization: prepending the flag-branch inside the
    /// recursive body pulled the decl-scoped path into the same mutual-recursion
    /// SCC as `lower_type_parameter`, defeating the baseline inlining of the
    /// flag-OFF path and growing its hot frame enough to overflow fp-ts's
    /// at-the-limit recursion (even at a 2 GB stack). With `inline(always)` here
    /// and `inline(never)` on both leaves, the flag-OFF leaf
    /// (`collect_type_parameters_user`) keeps codegen byte-identical to the
    /// pre-`#14344` baseline (no flag, no branch, no `DeclScoped`), so its frame
    /// matches baseline and the overflow does not occur; the flag-ON leaf carries
    /// the decl-identity logic in isolation.
    #[inline(always)]
    pub fn collect_type_parameters(&self, list: &NodeList) -> Vec<TypeParamInfo> {
        if decl_identity_activation() {
            self.collect_type_parameters_decl_scoped(list)
        } else {
            self.collect_type_parameters_user(list)
        }
    }

    /// Flag-OFF leaf: byte-identical to the pre-`#14344` `collect_type_parameters`
    /// body (no flag read, no branch, no `DeclScoped`). `inline(never)` keeps it
    /// out of the decl-scoped path's SCC so its codegen — and thus its stack
    /// frame across the deep `lower_type_parameter` recursion — matches baseline.
    #[inline(never)]
    fn collect_type_parameters_user(&self, list: &NodeList) -> Vec<TypeParamInfo> {
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
                origin: TypeParamOrigin::User,
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
                self.update_type_param_binding(info.name, type_id);
                params.push(info);
            }
        }
        params
    }

    /// #14344 STEP-B (flag-ON only): like `collect_type_parameters` but stamps
    /// each param's origin with its declaration site `(file, name_node)` so two
    /// distinct declarations sharing identical surface info intern distinctly.
    /// Separate from the flag-OFF path so the hot recursion there is unchanged.
    /// `inline(never)` keeps this flag-ON leaf out of the flag-OFF leaf's codegen
    /// (the dispatch-split's other half — see `collect_type_parameters`).
    #[inline(never)]
    fn collect_type_parameters_decl_scoped(&self, list: &NodeList) -> Vec<TypeParamInfo> {
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
            let origin = self.decl_scoped_origin(data.name);
            let placeholder = TypeParamInfo {
                is_const,
                name,
                constraint: None,
                default: None,
                origin,
            };
            self.add_type_param_binding(name, self.interner.type_param(placeholder));
            param_names.push((idx, name, is_const, origin));
        }

        let mut params = Vec::with_capacity(param_names.len());
        for (idx, name, is_const, origin) in param_names {
            if let Some(mut info) = self.lower_type_parameter(idx) {
                info.name = name;
                info.is_const = is_const;
                info.origin = origin;
                let type_id = self.interner.type_param(info);
                self.update_type_param_binding(info.name, type_id);
                params.push(info);
            }
        }
        params
    }

    /// #14344 STEP-B: the decl-scoped origin for a type-param name node — stamps
    /// `(file, name_node)` under the activation flag, else `User` (byte-parity).
    ///
    /// `file` is the interned source-file name (deterministic + stable across
    /// re-lowerings of the same declaration), recovered by walking `name_node`
    /// up to its root source-file node — mirroring the checker's existing
    /// `intern_string(&ctx.file_name)` decl-node key. `node` is the parse-stable
    /// name `NodeIndex`. Two distinct declarations differ in `(file, node)` so
    /// they intern distinctly; the SAME declaration lowered repeatedly yields the
    /// SAME `(file, node)` so it stays a single identity (no over-split).
    fn decl_scoped_origin(&self, name_node: NodeIndex) -> TypeParamOrigin {
        if decl_identity_activation() {
            let file = self.source_file_atom_for(name_node);
            TypeParamOrigin::DeclScoped {
                file,
                node: name_node.0,
            }
        } else {
            TypeParamOrigin::User
        }
    }

    /// The interned source-file-name `Atom` for a node, by walking up the
    /// extended-parent chain to the root source-file node. Deterministic and
    /// stable across re-lowerings (the file name is the canonical compile path),
    /// unlike a per-run arena pointer. Falls back to a fixed sentinel atom when
    /// no source-file root is reachable (synthetic/detached arenas), which keeps
    /// the `(file, node)` key well-defined without leaking a heap address.
    fn source_file_atom_for(&self, start: NodeIndex) -> Atom {
        let mut current = start;
        while let Some(ext) = self.arena.get_extended(current) {
            if ext.parent.is_none() {
                break;
            }
            current = ext.parent;
        }
        self.arena
            .get(current)
            .and_then(|node| self.arena.get_source_file(node))
            .map_or_else(
                || self.interner.intern_string("__no_source_file"),
                |source| self.interner.intern_string(&source.file_name),
            )
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
            origin: TypeParamOrigin::User,
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

            // Check for `this` parameter — both as a ThisKeyword node
            // (the parser's normal representation) and as an identifier
            // named "this" (fallback for edge cases).
            if let Some(name_node) = self.arena.get(param_data.name) {
                let is_this = name_node.kind == tsz_scanner::SyntaxKind::ThisKeyword as u16
                    || self
                        .arena
                        .get_identifier(name_node)
                        .is_some_and(|id_data| id_data.escaped_text == "this");
                if is_this {
                    if this_type.is_none() {
                        this_type = Some(self.lower_type(param_data.type_annotation));
                    }
                    continue;
                }
            }

            // An unannotated parameter in a function/constructor *type* (as opposed
            // to a function declaration/expression, which infers from context) has
            // no contextual source to infer from, so tsc gives it the implicit `any`
            // type unconditionally. `lower_type` returns `TypeId::ERROR` for a
            // missing node to prevent "any poisoning" elsewhere; that guard is wrong
            // here specifically because a signature-type parameter position always
            // needs *some* type, and `any` (not `ERROR`) is the one tsc assigns.
            let type_id = if param_data.type_annotation.is_none() {
                TypeId::ANY
            } else {
                self.with_typeof_param_bindings(&lowered, || {
                    self.lower_type(param_data.type_annotation)
                })
            };
            let optional = param_data.question_token || param_data.initializer != NodeIndex::NONE;
            // For `?`-optional params, tsc includes `| undefined` in the
            // signature type unconditionally (for display). Default-value
            // params keep the base type.
            let sig_type_id = if param_data.question_token
                && type_id != TypeId::ANY
                && type_id != TypeId::UNKNOWN
                && type_id != TypeId::ERROR
                && !tsz_solver::narrowing::type_contains_undefined(self.interner, type_id)
            {
                self.interner.union2(type_id, TypeId::UNDEFINED)
            } else {
                type_id
            };
            lowered.push(ParamInfo {
                suppress_display_optional: false,
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

        self.with_typeof_param_bindings(params, || {
            if let Some(predicate_node_idx) = self.find_type_predicate_node(node_idx) {
                return self.lower_type_predicate_return(predicate_node_idx, params);
            }

            (self.lower_type(node_idx), None)
        })
    }

    fn with_typeof_param_bindings<T>(&self, params: &[ParamInfo], f: impl FnOnce() -> T) -> T {
        let scope: Vec<_> = params
            .iter()
            .filter_map(|param| param.name.map(|name| (name, param.type_id)))
            .collect();

        if scope.is_empty() {
            return f();
        }

        self.typeof_param_scopes.borrow_mut().push(scope);
        let result = f();
        self.typeof_param_scopes.borrow_mut().pop();
        result
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
        let data = lower_node_data!(self, node_idx, get_function_type, TypeId::ERROR);
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
    }

    /// Lower a type literal ({ x: T, y: U })
    ///
    /// Routes through the same member-collection pipeline as interface
    /// lowering (`collect_object_type_members` / `finish_object_type_parts`)
    /// so structurally equivalent `interface I { ... }` and `type T = { ... }`
    /// produce identical types: method overloads accumulate into one member,
    /// index signatures merge (conflicting value types poison to error,
    /// distinct string-key patterns union their key types), and duplicate
    /// member conflicts are detected instead of producing duplicate
    /// properties.
    fn lower_type_literal(&self, node_idx: NodeIndex) -> TypeId {
        // Node-missing and data-missing diverge here: a missing node propagates
        // `TypeId::ERROR`, while a present node lacking type-literal data lowers
        // to the empty object literal `{}`. The node fetch shares the common
        // guard; the data fallback stays inline because it is distinct.
        let node = lower_node_data!(self, node_idx; TypeId::ERROR);

        let Some(data) = self.arena.get_type_literal(node) else {
            return self.interner.object(vec![]);
        };

        // A type literal is a single declaration pass, so the 1-based
        // pass-local counters already produce forward source order; the
        // separate forward-order walk is only needed when merged interface
        // declarations are collected in reverse.
        let mut parts = ObjectTypeParts::new();
        self.collect_object_type_members(&data.members, &mut parts);
        self.finish_object_type_parts(parts, None)
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

        let mut parts = ObjectTypeParts::new();
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
            let collected = self.collect_type_parameters(params);
            self.pop_type_param_scope();
            collected
        } else {
            Vec::new()
        };

        let saved_type_param_scopes = self.type_param_scopes.borrow().clone();
        *self.type_param_scopes.borrow_mut() = Vec::new();

        // Process declarations in FORWARD (source) order: TypeScript 7 preserves
        // declaration order for merged-interface overload sets, so an earlier
        // interface declaration's overloads render before a later one's. This
        // matches the multi-arena path in
        // `lower_merged_interface_declarations_with_symbol`.
        for (forward_decl_index, &decl_idx) in declarations.iter().enumerate() {
            parts.set_declaration_pass(forward_decl_index);

            let Some(node) = self.arena.get(decl_idx) else {
                continue;
            };
            let Some(interface) = self.arena.get_interface(node) else {
                continue;
            };
            if let Some(params) = &interface.type_parameters
                && !params.nodes.is_empty()
            {
                self.push_type_param_scope();
                let _ = self.collect_type_parameters(params);
                self.collect_object_type_members(&interface.members, &mut parts);
                self.pop_type_param_scope();
            } else {
                self.collect_object_type_members(&interface.members, &mut parts);
            }
        }

        *self.type_param_scopes.borrow_mut() = saved_type_param_scopes;

        // Assign declaration_order in FORWARD declaration order for diagnostics.
        // The reverse iteration above is needed for overload resolution priority,
        // but TS2740 "missing properties" messages should list properties in the
        // order they first appear across declarations (earliest declaration first).
        self.assign_forward_declaration_order(&mut parts, declarations.iter().copied());

        (
            self.finish_object_type_parts(parts, symbol_id),
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
}
