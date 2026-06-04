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
    ParamInfo, PropertyInfo, TupleElement, TypeId, TypeParamInfo, TypePredicate, Visibility,
};

mod signature_members;

#[cfg(test)]
mod constructor_parity_tests;

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
    /// Optional type resolver - resolves identifier nodes to `SymbolIds`.
    /// If provided, this enables correct abstract class detection.
    pub(super) type_resolver: Option<&'a NodeIndexResolver<'a, u32>>,
    /// Optional `DefId` resolver - resolves identifier nodes to `DefIds`.
    /// Resolves identifier nodes to `DefId`s for type identity.
    pub(super) def_id_resolver: Option<&'a NodeIndexResolver<'a, DefId>>,
    /// Optional resolver that returns a `DefId` only when a simple identifier
    /// lexically resolves to a function- or block-local declaration shadowing a
    /// same-named file-level type. Consulted before the name-first resolution in
    /// `prefer_name_def_id_resolution` mode so local shadows win without
    /// disturbing imported/lib name resolution. Returns `None` otherwise.
    pub(super) local_shadow_def_id_resolver: Option<&'a NodeIndexResolver<'a, DefId>>,
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
    /// Arena-aware variant of `computed_name_resolver`. When set, receives
    /// `(expr_idx, arena_ptr)` so the resolver can distinguish the same `NodeIndex`
    /// value from different arenas (cross-arena merged-interface lowering).
    /// Takes precedence over `computed_name_resolver` when both are set.
    pub(super) computed_name_resolver_with_arena:
        Option<&'a dyn Fn(NodeIndex, *const NodeArena) -> Option<Atom>>,
    /// Optional metadata resolver for computed property expressions whose resolved
    /// property name came from a symbol-valued computed name.
    pub(super) computed_symbol_name_resolver: Option<&'a dyn Fn(NodeIndex) -> bool>,
    /// Arena-aware variant of `computed_symbol_name_resolver`. Takes precedence
    /// over `computed_symbol_name_resolver` when both are set.
    pub(super) computed_symbol_name_resolver_with_arena:
        Option<&'a dyn Fn(NodeIndex, *const NodeArena) -> bool>,
    /// Optional resolver for lazy type parameter metadata. This is used when
    /// a lowered lazy reference omits type arguments but all parameters have defaults.
    pub(super) lazy_type_params_resolver: Option<&'a LazyTypeParamsResolver<'a>>,
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
    /// Optional override for type query resolution. When provided, this callback
    /// is consulted before creating a `TypeQuery` type. If it returns `Some(type_id)`,
    /// that type is used directly instead of creating a deferred `TypeQuery`.
    /// This enables flow-sensitive narrowing for `typeof expr` in type positions
    /// (e.g., inside type alias bodies where flow narrowing has already been computed).
    pub(super) type_query_override: Option<&'a NodeIndexResolver<'a, TypeId>>,
    /// Optional import type resolver — resolves `TYPE_REFERENCE` nodes whose `type_name`
    /// is or starts with an `import()` `CALL_EXPRESSION` to a pre-computed `TypeId`.
    ///
    /// `TypeLowering` cannot perform module resolution; the checker pre-resolves import
    /// type references and supplies them through this callback. The argument is the full
    /// `type_name` `NodeIndex` (either the `CALL_EXPRESSION` itself or the `QUALIFIED_NAME`
    /// rooted in it). Returns `Some` when pre-resolved, `None` to fall through to `ERROR`.
    pub(super) import_type_resolver: Option<&'a NodeIndexResolver<'a, TypeId>>,
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
    /// Additional string-keyed index signatures whose key type differs from
    /// `string_index.key_type`. Merged into `string_index` via key-type union
    /// in `finish_interface_parts`, where the type interner is available.
    pub(super) extra_string_indices: Vec<IndexSignature>,
    pub(super) number_index: Option<IndexSignature>,
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
            extra_string_indices: Vec::new(),
            number_index: None,
            has_late_bound_members: false,
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
                        is_symbol_named: false,
                        single_quoted_name: false,
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

        if let Some(existing) = self.string_index.as_mut() {
            if existing.key_type == index.key_type {
                if existing.value_type != index.value_type || existing.readonly != index.readonly {
                    existing.value_type = TypeId::ERROR;
                    existing.readonly = false;
                }
            } else {
                // Distinct pattern: defer key-type union to finish_interface_parts
                // where the type interner is available.
                self.extra_string_indices.push(index);
            }
        } else {
            self.string_index = Some(index);
        }
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

include!("core_parts/part1.rs");
include!("core_parts/part2.rs");
