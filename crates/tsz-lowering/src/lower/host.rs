//! `LoweringHost`: the single capability boundary `TypeLowering` consults to
//! resolve names, symbols, `DefId`s, computed property keys, lazy type-param
//! metadata, type-query overrides, and import-type references.
//!
//! Historically `TypeLowering` carried twelve independent `Option<&dyn Fn>`
//! resolver callbacks, wired ad hoc at each construction site. Which resolvers
//! were active became a property of every call site rather than of the type,
//! and the precedence between arena-aware and arena-blind variants lived only
//! in comments. This module collapses that surface into one trait.
//!
//! [`LoweringHost`] declares every capability as a default-`None`/`false`
//! method, so a host only overrides what it actually provides. Arena-aware
//! capabilities take `&NodeArena` directly instead of a raw `*const NodeArena`.
//!
//! [`ClosureLoweringHost`] is the closure-backed adapter the existing checker
//! construction sites build through the public `TypeLowering` constructors and
//! `with_*` builders. It owns the twelve optional closures and implements the
//! precedence rules (arena-aware variant first, value-resolver falling back to
//! the type resolver) exactly as the previous inline field reads did, so the
//! refactor is behavior-preserving.

use tsz_common::interner::Atom;
use tsz_parser::parser::base::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_solver::def::DefId;
use tsz_solver::types::{TypeId, TypeParamInfo};

use super::core::{LazyTypeParamsResolver, NodeIndexResolver, TypeIdResolver};

/// Single resolver boundary consulted by `TypeLowering`.
///
/// Every method defaults to "not provided" (`None`/`false`) so a host only
/// implements the capabilities it actually backs. `TypeLowering` never reads a
/// resolver field directly; it asks the host, so the active capability set is a
/// property of the host value rather than of each construction site.
pub trait LoweringHost {
    /// Resolve an identifier node to a binder `SymbolId` (type position).
    fn resolve_type_symbol(&self, _node: NodeIndex) -> Option<u32> {
        None
    }

    /// Resolve an identifier node to a solver-owned `DefId`.
    fn resolve_def_id(&self, _node: NodeIndex) -> Option<DefId> {
        None
    }

    /// Resolve an identifier node to the `DefId` of a function- or block-local
    /// declaration that shadows a same-named file-level type. Returns `None` for
    /// every non-shadowing reference.
    fn resolve_local_shadow_def_id(&self, _node: NodeIndex) -> Option<DefId> {
        None
    }

    /// Resolve an identifier node to a binder `SymbolId` (value position, for
    /// `typeof` queries). When no value resolver is provided, callers fall back
    /// to [`LoweringHost::resolve_type_symbol`]; that fallback is implemented by
    /// the concrete host so it can honor a shared type/value closure.
    fn resolve_value_symbol(&self, _node: NodeIndex) -> Option<u32> {
        None
    }

    /// Resolve an identifier's text to a `DefId` (cross-arena name-based path).
    fn resolve_def_id_by_name(&self, _name: &str) -> Option<DefId> {
        None
    }

    /// Resolve a computed property expression to a property-name atom, using the
    /// active declaration `arena` to disambiguate cross-arena `NodeIndex` values.
    fn resolve_computed_name(&self, _expr: NodeIndex, _arena: &NodeArena) -> Option<Atom> {
        None
    }

    /// Whether a computed property expression resolves to a symbol-valued key,
    /// using the active declaration `arena` for cross-arena disambiguation.
    ///
    /// Returns `None` when this host wires no symbol-name resolver, so the
    /// caller falls back to its syntax-only well-known-`Symbol` check. A host
    /// that does wire a resolver returns `Some(answer)` and that answer wins
    /// outright (no fallback), matching the previous inline precedence.
    fn computed_name_is_symbol(&self, _expr: NodeIndex, _arena: &NodeArena) -> Option<bool> {
        None
    }

    /// Whether a computed property expression resolves to a plain
    /// (non-unique) `symbol`-valued key — a binding annotated `: symbol`
    /// rather than `: unique symbol`. Such a key does not mint a named
    /// member: tsc routes it into the containing type's symbol index
    /// signature instead, so two declarations keyed by independent
    /// `symbol`-typed bindings still describe the same member set.
    ///
    /// Returns `None` when this host wires no such resolver, in which case
    /// the caller treats the key as an ordinary named member (the previous
    /// behavior). A host that does wire a resolver returns `Some(answer)`
    /// and that answer wins outright.
    fn computed_name_is_wide_symbol(&self, _expr: NodeIndex, _arena: &NodeArena) -> Option<bool> {
        None
    }

    /// Resolve lazy type-parameter metadata for a `DefId` (used to apply omitted
    /// defaulted type arguments).
    fn resolve_lazy_type_params(&self, _def_id: DefId) -> Option<Vec<TypeParamInfo>> {
        None
    }

    /// Flow-sensitive override for `typeof expr` in type position. When it
    /// returns `Some`, the result is used instead of a deferred `TypeQuery`.
    fn resolve_type_query_override(&self, _node: NodeIndex) -> Option<TypeId> {
        None
    }

    /// Resolve a `TYPE_REFERENCE` whose name is rooted in an `import()` call to a
    /// pre-computed `TypeId` (module resolution is performed by the checker).
    fn resolve_import_type(&self, _node: NodeIndex) -> Option<TypeId> {
        None
    }
}

/// Closure-backed [`LoweringHost`] used by the public `TypeLowering`
/// constructors and `with_*` builders.
///
/// Each field mirrors one of the historical resolver callbacks. The two
/// computed-name capabilities keep both an arena-aware and an arena-blind
/// closure; the arena-aware variant takes precedence when both are set, exactly
/// as the previous inline reads did.
#[derive(Default, Clone)]
pub struct ClosureLoweringHost<'a> {
    pub(super) type_resolver: Option<&'a NodeIndexResolver<'a, u32>>,
    pub(super) def_id_resolver: Option<&'a NodeIndexResolver<'a, DefId>>,
    pub(super) local_shadow_def_id_resolver: Option<&'a NodeIndexResolver<'a, DefId>>,
    pub(super) value_resolver: Option<&'a NodeIndexResolver<'a, u32>>,
    pub(super) name_def_id_resolver: Option<&'a TypeIdResolver<'a>>,
    pub(super) computed_name_resolver: Option<&'a NodeIndexResolver<'a, Atom>>,
    pub(super) computed_name_resolver_with_arena:
        Option<&'a dyn Fn(NodeIndex, *const NodeArena) -> Option<Atom>>,
    pub(super) computed_symbol_name_resolver: Option<&'a dyn Fn(NodeIndex) -> bool>,
    pub(super) computed_symbol_name_resolver_with_arena:
        Option<&'a dyn Fn(NodeIndex, *const NodeArena) -> bool>,
    pub(super) computed_wide_symbol_name_resolver: Option<&'a dyn Fn(NodeIndex) -> bool>,
    pub(super) computed_wide_symbol_name_resolver_with_arena:
        Option<&'a dyn Fn(NodeIndex, *const NodeArena) -> bool>,
    pub(super) lazy_type_params_resolver: Option<&'a LazyTypeParamsResolver<'a>>,
    pub(super) type_query_override: Option<&'a NodeIndexResolver<'a, TypeId>>,
    pub(super) import_type_resolver: Option<&'a NodeIndexResolver<'a, TypeId>>,
}

impl LoweringHost for ClosureLoweringHost<'_> {
    fn resolve_type_symbol(&self, node: NodeIndex) -> Option<u32> {
        self.type_resolver.and_then(|resolver| resolver(node))
    }

    fn resolve_def_id(&self, node: NodeIndex) -> Option<DefId> {
        self.def_id_resolver.and_then(|resolver| resolver(node))
    }

    fn resolve_local_shadow_def_id(&self, node: NodeIndex) -> Option<DefId> {
        self.local_shadow_def_id_resolver
            .and_then(|resolver| resolver(node))
    }

    fn resolve_value_symbol(&self, node: NodeIndex) -> Option<u32> {
        // A value resolver wins; otherwise fall back to the type resolver,
        // preserving the previous `resolve_value_symbol` behavior.
        if let Some(resolver) = self.value_resolver {
            resolver(node)
        } else {
            self.resolve_type_symbol(node)
        }
    }

    fn resolve_def_id_by_name(&self, name: &str) -> Option<DefId> {
        self.name_def_id_resolver
            .and_then(|resolver| resolver(name))
    }

    fn resolve_computed_name(&self, expr: NodeIndex, arena: &NodeArena) -> Option<Atom> {
        // The arena-aware variant takes precedence: it distinguishes the same
        // NodeIndex value across different arenas (cross-arena lowering).
        let arena_ptr: *const NodeArena = arena;
        if let Some(resolver) = self.computed_name_resolver_with_arena
            && let Some(name) = resolver(expr, arena_ptr)
        {
            return Some(name);
        }
        if let Some(resolver) = self.computed_name_resolver
            && let Some(name) = resolver(expr)
        {
            return Some(name);
        }
        None
    }

    fn computed_name_is_symbol(&self, expr: NodeIndex, arena: &NodeArena) -> Option<bool> {
        // The arena-aware variant takes precedence; either resolver's answer
        // wins outright (no well-known fallback), exactly as the previous
        // inline reads did. `None` means no symbol resolver is wired.
        let arena_ptr: *const NodeArena = arena;
        if let Some(resolver) = self.computed_symbol_name_resolver_with_arena {
            return Some(resolver(expr, arena_ptr));
        }
        if let Some(resolver) = self.computed_symbol_name_resolver {
            return Some(resolver(expr));
        }
        None
    }

    fn computed_name_is_wide_symbol(&self, expr: NodeIndex, arena: &NodeArena) -> Option<bool> {
        let arena_ptr: *const NodeArena = arena;
        if let Some(resolver) = self.computed_wide_symbol_name_resolver_with_arena {
            return Some(resolver(expr, arena_ptr));
        }
        if let Some(resolver) = self.computed_wide_symbol_name_resolver {
            return Some(resolver(expr));
        }
        None
    }

    fn resolve_lazy_type_params(&self, def_id: DefId) -> Option<Vec<TypeParamInfo>> {
        self.lazy_type_params_resolver
            .and_then(|resolver| resolver(def_id))
    }

    fn resolve_type_query_override(&self, node: NodeIndex) -> Option<TypeId> {
        self.type_query_override.and_then(|resolver| resolver(node))
    }

    fn resolve_import_type(&self, node: NodeIndex) -> Option<TypeId> {
        self.import_type_resolver
            .and_then(|resolver| resolver(node))
    }
}
