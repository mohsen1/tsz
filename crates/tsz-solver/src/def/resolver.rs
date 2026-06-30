//! Type resolution trait and environment.
//!
//! Defines `TypeResolver` — the trait for lazily resolving type references
//! (both legacy `SymbolRef` and modern `DefId`), and `TypeEnvironment` — the
//! standard implementation that maps identifiers to their resolved types.

use std::sync::Arc;

use crate::construction::TypeDatabase;
use crate::def::DefId;
use crate::def::core::DefinitionStore;
use crate::types::{IntrinsicKind, SymbolRef, TypeId, TypeParamInfo, Variance};
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_binder::SymbolId;

/// Trait for resolving type references to their structural types.
/// This allows the `SubtypeChecker` to lazily resolve Ref types
/// without being tightly coupled to the binder/checker.
pub trait TypeResolver {
    /// Monotonic generation for resolver-visible state.
    ///
    /// Narrowing and relation caches include this value when they depend on
    /// lazy `DefId` resolution. Resolver implementations should bump the
    /// generation whenever a later resolve call can return a different type.
    fn resolver_generation(&self) -> u64 {
        0
    }

    /// Whether this resolver carries no definition/symbol context (the
    /// [`NoopResolver`] sentinel used by `SubtypeChecker::new`).
    ///
    /// Relation rules that need to distinguish "no nominal context is
    /// available" (treat shapes structurally) from "a real resolver is present
    /// but a particular symbol/type is simply not mapped here" use this. It
    /// must stay `false` for every resolver that can answer any
    /// `symbol_to_def_id`/`def_for_type`/`get_def_kind` query.
    fn is_noop(&self) -> bool {
        false
    }

    /// Resolve a symbol reference to its structural type.
    /// Returns None if the symbol cannot be resolved.
    ///
    /// Deprecated: use `resolve_lazy` with `DefId` instead.
    fn resolve_ref(&self, symbol: SymbolRef, interner: &dyn TypeDatabase) -> Option<TypeId>;

    /// Resolve a symbol reference to a structural type, preferring DefId-based lazy paths.
    ///
    /// Prefers `resolve_lazy` via `DefId` when available, falling back to `resolve_ref`.
    fn resolve_symbol_ref(&self, symbol: SymbolRef, interner: &dyn TypeDatabase) -> Option<TypeId> {
        if let Some(def_id) = self.symbol_to_def_id(symbol) {
            self.resolve_lazy(def_id, interner)
        } else {
            self.resolve_ref(symbol, interner)
        }
    }

    /// Resolve a `TypeQuery` (`typeof X`) symbol to its value-space type.
    ///
    /// For classes, `resolve_lazy`/`resolve_symbol_ref` return the **instance** type
    /// Resolve a `TypeQuery` (`typeof X`) to the value-space type for a symbol.
    ///
    /// For classes, this must return the **constructor type** (with construct signatures
    /// and static members), NOT the instance type. This distinction is critical:
    /// `typeof MyClass` should give the constructor, not `MyClass` the instance.
    ///
    /// Default implementation delegates to `resolve_ref`. Implementations that store
    /// instance types under `SymbolRef` (like `TypeEnvironment`) should override this
    /// to return the constructor type via the `DefId` path.
    fn resolve_type_query(&self, symbol: SymbolRef, interner: &dyn TypeDatabase) -> Option<TypeId> {
        self.resolve_ref(symbol, interner)
    }

    /// Resolve a `DefId` reference to its structural type.
    ///
    /// This is the `DefId` equivalent of `resolve_ref`, used for `TypeData::Lazy(DefId)`.
    /// `DefIds` are Solver-owned identifiers that decouple type references from the Binder.
    ///
    /// Returns None by default; implementations should override to support Lazy type resolution.
    fn resolve_lazy(&self, _def_id: DefId, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        None
    }

    /// Pure lookup of a `DefId`'s already-registered body, with **no** on-demand
    /// side effects.
    ///
    /// `resolve_lazy` may, for a force-eligible lib interface, materialize and
    /// register a previously-unregistered body on a miss (issue #12101). Callers
    /// that treat a `resolve_lazy` *miss* as a stable signal — e.g. the variance
    /// fingerprint validity check, which asserts a stored mask's gap defs are
    /// *still* unresolved — must use this entry point instead, so miss-forcing
    /// never inverts that is-none signal. The default delegates to `resolve_lazy`
    /// for resolvers that have no forcing side effect.
    fn resolve_lazy_lookup_only(
        &self,
        def_id: DefId,
        interner: &dyn TypeDatabase,
    ) -> Option<TypeId> {
        self.resolve_lazy(def_id, interner)
    }

    /// Get type parameters for a symbol (for generic type aliases/interfaces).
    /// Returns None by default; implementations can override to support
    /// Application type expansion.
    fn get_type_params(&self, _symbol: SymbolRef) -> Option<Vec<TypeParamInfo>> {
        None
    }

    /// Get type parameters for a `DefId` (for generic type aliases/interfaces).
    ///
    /// This is the `DefId` equivalent of `get_type_params`.
    /// Returns None by default; implementations can override to support
    /// Application type expansion with Lazy types.
    fn get_lazy_type_params(&self, _def_id: DefId) -> Option<Vec<TypeParamInfo>> {
        None
    }

    /// Get the `SymbolId` for a `DefId` (bridge for `InheritanceGraph`).
    ///
    /// This enables DefId-based types to use the existing O(1) `InheritanceGraph`
    /// by mapping `DefIds` back to their corresponding `SymbolIds`. The mapping is
    /// maintained by the Binder/Checker during type resolution.
    ///
    /// Returns None if the `DefId` doesn't have a corresponding `SymbolId`.
    fn def_to_symbol_id(&self, _def_id: DefId) -> Option<SymbolId> {
        None
    }

    /// Resolve a `DefId` through import-alias forwarding to the declaring
    /// definition's `DefId`. Identity by default; resolvers backed by a
    /// `DefinitionStore` chase its alias-forward links so an alias-keyed
    /// `Lazy`/`Application` base and the declaring module's own key compare
    /// as the same definition.
    fn canonical_def_id(&self, def_id: DefId) -> DefId {
        def_id
    }

    /// Check whether two `DefIds` refer to the same declaration (same `DefId` or same `SymbolId`).
    ///
    /// Cross-context `DefId` aliasing can give the same interface different `DefIds`
    /// (e.g., lib file vs heritage clause lowering). This method handles that by
    /// falling back to `SymbolId` comparison when `DefIds` differ. Import-alias
    /// forwarding intentionally does NOT widen this predicate: its consumers
    /// (display aliasing, augmentation identity) must keep distinguishing an
    /// alias's def from its target's; relation-level same-definition detection
    /// canonicalizes through [`TypeResolver::canonical_def_id`] explicitly.
    fn defs_are_equivalent(&self, a: DefId, b: DefId) -> bool {
        a == b
            || self
                .def_to_symbol_id(a)
                .zip(self.def_to_symbol_id(b))
                .is_some_and(|(sa, sb)| sa == sb)
    }

    /// Get the `DefId` for a `SymbolRef` (Ref -> Lazy migration).
    ///
    /// This enables migrating Ref(SymbolRef) types to Lazy(DefId) resolution logic.
    /// When a `SymbolRef` has a corresponding `DefId`, we should use `resolve_lazy` instead
    /// of `resolve_ref` for consistent type identity.
    ///
    /// Returns None if the `SymbolRef` doesn't have a corresponding `DefId`.
    fn symbol_to_def_id(&self, _symbol: SymbolRef) -> Option<DefId> {
        None
    }

    /// Get the `DefKind` for a `DefId` (Task #32: Graph Isomorphism).
    ///
    /// This is used by the Canonicalizer to distinguish between structural types
    /// (`TypeAlias` - should be canonicalized with Recursive indices) and nominal
    /// types (Interface/Class/Enum - must remain as Lazy(DefId) for nominal identity).
    ///
    /// Returns None if the `DefId` doesn't exist or the implementation doesn't
    /// support `DefKind` lookup.
    fn get_def_kind(&self, _def_id: DefId) -> Option<crate::def::DefKind> {
        None
    }

    /// Get the source/display name for a `DefId` when available.
    fn get_def_name(&self, _def_id: DefId) -> Option<tsz_common::interner::Atom> {
        None
    }

    /// Whether this `DefId` is the standard library `ReadonlyArray` interface.
    fn is_builtin_readonly_array_def(&self, _def_id: DefId) -> bool {
        false
    }

    /// Whether this `DefId` originates from an actual or checker-cloned standard lib declaration.
    fn is_actual_or_cloned_lib_def(&self, _def_id: DefId) -> bool {
        false
    }

    /// Whether this `DefId` backs an `import` alias whose module failed to
    /// resolve (its `TS2307` was already reported).
    ///
    /// `tsc` substitutes the permissive `error`/`any` type for a reference
    /// whose target symbol could not be resolved, so applying type arguments
    /// to such a reference (`Gen<{...}>` from `import { Gen } from "missing"`)
    /// must collapse to `any` rather than survive as a live structural
    /// `Application` the relation layer then rejects. The default `false`
    /// keeps non-checker resolvers (which have no module-resolution surface)
    /// on the existing opaque-application behavior.
    fn is_unresolved_import_def(&self, _def_id: DefId) -> bool {
        false
    }

    /// Resolve an `UnresolvedTypeName(atom)` text to a `DefId`, when the
    /// resolver has access to a wider binder graph than the lowering pass
    /// did. Used by the type evaluator to recover from
    /// `Application(UnresolvedTypeName(name), args)` where the name now
    /// resolves cleanly through the merged binder. Returns `None` when the
    /// name still cannot be resolved.
    fn resolve_unresolved_type_name(&self, _name: &str) -> Option<DefId> {
        None
    }

    /// Resolve a canonical well-known symbol property name (for example
    /// `"[Symbol.iterator]"`) to its `SymbolRef` when available.
    ///
    /// This allows solver-only passes (like `keyof` evaluation) to recover
    /// unique-symbol key identity even when property names are carried as
    /// canonical string keys in object shapes.
    fn resolve_well_known_symbol_name(&self, _name: &str) -> Option<SymbolRef> {
        None
    }

    /// Reverse of [`TypeResolver::resolve_well_known_symbol_name`]: recover the
    /// canonical `[Symbol.xxx]` property name a well-known `SymbolRef` was
    /// registered under.
    ///
    /// Unique-symbol keys are modeled as `UniqueSymbol(SymbolRef)` in `keyof`
    /// and indexed-access types, but their object-shape members are stored under
    /// the canonical text key (e.g. `"[Symbol.iterator]"`). Converting a
    /// well-known `SymbolRef` back to that text — rather than the synthetic
    /// `__unique_N` placeholder used for user-authored unique symbols — lets
    /// member lookup and mapped-type materialization round-trip such keys.
    fn well_known_symbol_name_for_ref(&self, _symbol: SymbolRef) -> Option<&str> {
        None
    }

    /// Get the boxed interface type for a primitive intrinsic (Rule #33).
    /// For example, `IntrinsicKind::Number` -> `TypeId` of the Number interface.
    /// This enables primitives to be subtypes of their boxed interfaces.
    fn get_boxed_type(&self, _kind: IntrinsicKind) -> Option<TypeId> {
        None
    }

    /// Check if a `DefId` corresponds to a boxed type for the given intrinsic kind.
    fn is_boxed_def_id(&self, _def_id: DefId, _kind: IntrinsicKind) -> bool {
        false
    }

    /// Check if a `TypeId` is any known resolved form of a boxed type.
    ///
    /// The `Object` interface (and other boxed types) can have multiple `TypeId`s:
    /// one from `resolve_lib_type_by_name` and another from `type_reference_symbol_type`.
    /// This method checks all registered boxed `DefId`s and their resolved `TypeId`s.
    fn is_boxed_type_id(&self, _type_id: TypeId, _kind: IntrinsicKind) -> bool {
        false
    }

    /// Get the Array<T> interface type from lib.d.ts.
    fn get_array_base_type(&self) -> Option<TypeId> {
        None
    }

    /// Get the type parameters for the Array<T> interface.
    fn get_array_base_type_params(&self) -> &[TypeParamInfo] {
        &[]
    }

    /// Get the `ReadonlyArray<T>` interface type from lib.d.ts.
    ///
    /// Used by property access resolution to find only the non-mutating methods
    /// when resolving properties on `readonly T[]` or `readonly [...]` types.
    fn get_readonly_array_base_type(&self) -> Option<TypeId> {
        None
    }

    /// Check if a `DefId` corresponds to a numeric enum (not a string enum).
    ///
    /// Used for TypeScript's unsound Rule #7 (Open Numeric Enums) where
    /// number types are assignable to/from numeric enums.
    fn is_numeric_enum(&self, _def_id: DefId) -> bool {
        false
    }

    /// Check if a `TypeId` represents a full Enum type (not a specific member).
    fn is_enum_type(&self, _type_id: TypeId, _interner: &dyn TypeDatabase) -> bool {
        false
    }

    /// Get the parent Enum's `DefId` for an Enum Member's `DefId`.
    ///
    /// Used to check nominal relationships between enum members and their parent types.
    fn get_enum_parent_def_id(&self, _member_def_id: DefId) -> Option<DefId> {
        None
    }

    /// Get the registered member `DefIds` for a parent enum `DefId`.
    ///
    /// Returns the members in declaration order, or an empty `Vec` when the
    /// resolver has no record of an enum with this parent `DefId`. Used by
    /// control-flow narrowing to decompose `Enum(parent, lit_union)` into the
    /// union of its member-typed values, matching tsc's narrowing model.
    fn get_enum_member_def_ids(&self, _parent_def_id: DefId) -> Vec<DefId> {
        Vec::new()
    }

    /// Check if a `DefId` represents a user-defined enum (not an intrinsic type).
    fn is_user_enum_def(&self, _def_id: DefId) -> bool {
        false
    }

    /// Get the namespace object type for an enum (for `typeof Enum` / `keyof typeof Enum`).
    ///
    /// In TypeScript, `typeof Enum` returns the "enum object" — an object with member
    /// names as keys and member types as values (e.g., `{ Up: Direction.Up, Down: Direction.Down }`).
    /// The solver stores enums as `TypeData::Enum(DefId, union_of_values)` which only has
    /// member VALUES, not member NAMES. This method bridges that gap by letting the checker
    /// provide the pre-computed namespace object type.
    fn get_enum_namespace_type(&self, _def_id: DefId) -> Option<TypeId> {
        None
    }

    /// Get the parent class `DefId` for a class definition.
    ///
    /// Used by instanceof narrowing to check class hierarchy nominally,
    /// preventing structural subtype checks from incorrectly keeping
    /// unrelated class types in narrowed unions.
    fn get_class_extends(&self, _def_id: DefId) -> Option<DefId> {
        None
    }

    /// Resolve the concrete class/interface instance type for the current polymorphic `this`.
    ///
    /// When the caller is inside a class or interface member, this lets the solver
    /// substitute `ThisType` with the enclosing instance type for relation checks.
    fn resolve_this_type(&self, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        None
    }

    /// Reverse-lookup: get the class `DefId` for a resolved instance `TypeId`.
    ///
    /// When a class instance type (Object with properties) was registered via
    /// `insert_class_instance_type`, this returns the originating class's `DefId`.
    /// Used by instanceof narrowing to identify class types that have been
    /// resolved from `Lazy(DefId)` to their structural representation.
    fn class_def_for_instance_type(&self, _type_id: TypeId) -> Option<DefId> {
        None
    }

    /// Reverse-lookup: get the declaration `DefId` that produced a resolved `TypeId`.
    ///
    /// This is broader than `class_def_for_instance_type`: interface instance
    /// types and other named structural forms can also be backed by a
    /// declaration identity.
    fn def_for_type(&self, _type_id: TypeId) -> Option<DefId> {
        None
    }

    /// Get the base class type for a class/interface type.
    ///
    /// Used by the Best Common Type (BCT) algorithm to find common base classes.
    fn get_base_type(&self, _type_id: TypeId, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        None
    }

    /// #14351 lazy-reference relation: the instantiated base `TypeId` for a
    /// DIRECT `extends` edge `derived extends target`, as written in `derived`'s
    /// scope (e.g. `Functor1<F>` for `interface Apply1<F> extends Functor1<F>`).
    /// `None` if `target` is not a direct parent of `derived` (the first slice
    /// is single-hop). Unlike `get_base_type` this is keyed by the
    /// `(derived, target)` def pair (selects the specific parent, not
    /// `parents.first()`) and carries the heritage edge's type arguments.
    fn get_heritage_instantiation(&self, _derived: DefId, _target: DefId) -> Option<TypeId> {
        None
    }

    /// Get the variance mask for type parameters of a generic type (Task #41).
    ///
    /// Used by `check_application_to_application_subtype` to optimize generic
    /// assignability checks via variance annotations instead of full structural expansion.
    fn get_type_param_variance(
        &self,
        _def_id: DefId,
    ) -> Option<std::sync::Arc<[crate::types::Variance]>> {
        None
    }

    /// Get the raw structural body `TypeId` for a `DefId` directly from the
    /// definition store, bypassing evaluation caches, instance-type wrappers,
    /// and self-wrapper deferral logic present in the full `resolve_lazy` chain.
    ///
    /// Used by `is_conditional_alias_base_inline` to reliably detect whether a
    /// generic type alias has a `Conditional` body. `resolve_lazy` for generic
    /// aliases can return a cached `Application` or self-`Lazy` wrapper from
    /// `symbol_types`, hiding the real conditional body. This method provides a
    /// direct view of what was stored at alias-registration time.
    ///
    /// Returns `None` by default; implementations backed by a `DefinitionStore`
    /// should override to call `store.get_body(def_id)`.
    fn get_def_raw_body(&self, _def_id: DefId, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        None
    }

    /// Whether `def_id` names a generic type alias whose body is a *genuinely
    /// registered* `unknown` (`type C<T> = unknown`, or a utility alias that
    /// reduces to `unknown`), as opposed to a cross-file registration-window
    /// placeholder whose `unknown` is a not-yet-published-body sentinel.
    ///
    /// The two are indistinguishable from a resolved `unknown` alone: a genuine
    /// body is recorded in the definition store at alias-registration time
    /// (surfaced by [`Self::get_def_raw_body`]), whereas a placeholder `unknown`
    /// comes from an unresolved symbol-type fallback with no registered body.
    /// This is the single source of truth for that distinction, consumed by both
    /// the evaluator (whether to reduce `C<Args>` to canonical `unknown`) and the
    /// relation layer (whether a deferred `unknown`-returning member relates as
    /// `unknown`). See issues #14595 / #13212.
    fn is_genuine_unknown_alias_body(&self, def_id: DefId, interner: &dyn TypeDatabase) -> bool {
        if self.get_def_raw_body(def_id, interner) != Some(TypeId::UNKNOWN) {
            return false;
        }
        // tsc's lib declares NO `type X = unknown` utility alias — every lib
        // utility (`Omit`, `Pick`, `Exclude`, …) has a structural
        // (mapped/conditional/`Pick`) body — so a non-program (lib/ambient-binder)
        // def whose body currently reads `unknown` is always a not-yet-
        // materialized registration-window placeholder, never a genuine `unknown`
        // alias. Reducing `Omit<T, K>` to bare `unknown` because its body sentinel
        // has not materialized in a multi-file run drops the picked properties
        // (the ts-rest `params`/`body` TS2339 false positives, issue #14337).
        // Exclude such defs structurally by file origin (NOT by name); a
        // user-program `type C<T> = unknown` keeps a real program `file_id` and is
        // unaffected.
        !self.def_is_non_program(def_id)
    }

    /// Whether `def_id` originates in a non-program (lib/ambient-binder) file
    /// rather than user program source. Default `false`; resolvers backed by a
    /// `DefinitionStore` override this via the def's `file_id`. Used to keep a
    /// lib utility's not-yet-materialized `unknown` body from being mistaken for
    /// a genuine `unknown` alias (issue #14337).
    fn def_is_non_program(&self, _def_id: DefId) -> bool {
        false
    }
}

/// A no-op resolver that doesn't resolve any references.
/// Useful for tests or when symbol resolution isn't needed.
pub struct NoopResolver;

impl TypeResolver for NoopResolver {
    fn is_noop(&self) -> bool {
        true
    }

    fn resolve_ref(&self, _symbol: SymbolRef, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        None
    }
}

/// Blanket implementation of `TypeResolver` for references to resolver types.
///
/// This allows `&dyn TypeResolver` (which is Sized) to be used wherever
/// `R: TypeResolver` is expected.
impl<T: TypeResolver + ?Sized> TypeResolver for &T {
    fn resolver_generation(&self) -> u64 {
        (**self).resolver_generation()
    }

    fn is_noop(&self) -> bool {
        (**self).is_noop()
    }

    fn resolve_ref(&self, symbol: SymbolRef, interner: &dyn TypeDatabase) -> Option<TypeId> {
        (**self).resolve_ref(symbol, interner)
    }

    fn resolve_symbol_ref(&self, symbol: SymbolRef, interner: &dyn TypeDatabase) -> Option<TypeId> {
        (**self).resolve_symbol_ref(symbol, interner)
    }

    fn resolve_type_query(&self, symbol: SymbolRef, interner: &dyn TypeDatabase) -> Option<TypeId> {
        (**self).resolve_type_query(symbol, interner)
    }

    fn resolve_lazy(&self, def_id: DefId, interner: &dyn TypeDatabase) -> Option<TypeId> {
        (**self).resolve_lazy(def_id, interner)
    }

    fn def_is_non_program(&self, def_id: DefId) -> bool {
        // Forward so a `TypeEnvironment`'s store-backed file-origin check (lib
        // placeholder vs genuine `unknown`, issue #14337) is not shadowed by the
        // trait default when the evaluator holds `&R`.
        (**self).def_is_non_program(def_id)
    }

    fn get_type_params(&self, symbol: SymbolRef) -> Option<Vec<TypeParamInfo>> {
        (**self).get_type_params(symbol)
    }

    fn get_lazy_type_params(&self, def_id: DefId) -> Option<Vec<TypeParamInfo>> {
        (**self).get_lazy_type_params(def_id)
    }

    fn def_to_symbol_id(&self, def_id: DefId) -> Option<SymbolId> {
        (**self).def_to_symbol_id(def_id)
    }

    fn canonical_def_id(&self, def_id: DefId) -> DefId {
        (**self).canonical_def_id(def_id)
    }

    fn defs_are_equivalent(&self, a: DefId, b: DefId) -> bool {
        (**self).defs_are_equivalent(a, b)
    }

    fn symbol_to_def_id(&self, symbol: SymbolRef) -> Option<DefId> {
        (**self).symbol_to_def_id(symbol)
    }

    fn get_def_kind(&self, def_id: DefId) -> Option<crate::def::DefKind> {
        (**self).get_def_kind(def_id)
    }

    fn get_def_name(&self, def_id: DefId) -> Option<tsz_common::interner::Atom> {
        (**self).get_def_name(def_id)
    }

    fn is_builtin_readonly_array_def(&self, def_id: DefId) -> bool {
        (**self).is_builtin_readonly_array_def(def_id)
    }

    fn is_actual_or_cloned_lib_def(&self, def_id: DefId) -> bool {
        (**self).is_actual_or_cloned_lib_def(def_id)
    }

    fn get_boxed_type(&self, kind: IntrinsicKind) -> Option<TypeId> {
        (**self).get_boxed_type(kind)
    }

    fn is_boxed_def_id(&self, def_id: DefId, kind: IntrinsicKind) -> bool {
        (**self).is_boxed_def_id(def_id, kind)
    }

    fn is_boxed_type_id(&self, type_id: TypeId, kind: IntrinsicKind) -> bool {
        (**self).is_boxed_type_id(type_id, kind)
    }

    fn get_array_base_type(&self) -> Option<TypeId> {
        (**self).get_array_base_type()
    }

    fn get_array_base_type_params(&self) -> &[TypeParamInfo] {
        (**self).get_array_base_type_params()
    }

    fn get_readonly_array_base_type(&self) -> Option<TypeId> {
        (**self).get_readonly_array_base_type()
    }

    fn is_numeric_enum(&self, def_id: DefId) -> bool {
        (**self).is_numeric_enum(def_id)
    }

    fn is_enum_type(&self, type_id: TypeId, interner: &dyn TypeDatabase) -> bool {
        (**self).is_enum_type(type_id, interner)
    }

    fn get_enum_parent_def_id(&self, member_def_id: DefId) -> Option<DefId> {
        (**self).get_enum_parent_def_id(member_def_id)
    }

    fn get_enum_member_def_ids(&self, parent_def_id: DefId) -> Vec<DefId> {
        (**self).get_enum_member_def_ids(parent_def_id)
    }

    fn is_user_enum_def(&self, def_id: DefId) -> bool {
        (**self).is_user_enum_def(def_id)
    }

    fn get_enum_namespace_type(&self, def_id: DefId) -> Option<TypeId> {
        (**self).get_enum_namespace_type(def_id)
    }

    fn get_class_extends(&self, def_id: DefId) -> Option<DefId> {
        (**self).get_class_extends(def_id)
    }

    fn class_def_for_instance_type(&self, type_id: TypeId) -> Option<DefId> {
        (**self).class_def_for_instance_type(type_id)
    }

    fn def_for_type(&self, type_id: TypeId) -> Option<DefId> {
        (**self).def_for_type(type_id)
    }

    fn get_base_type(&self, type_id: TypeId, interner: &dyn TypeDatabase) -> Option<TypeId> {
        (**self).get_base_type(type_id, interner)
    }

    fn get_heritage_instantiation(&self, derived: DefId, target: DefId) -> Option<TypeId> {
        (**self).get_heritage_instantiation(derived, target)
    }

    fn get_type_param_variance(
        &self,
        def_id: DefId,
    ) -> Option<std::sync::Arc<[crate::types::Variance]>> {
        (**self).get_type_param_variance(def_id)
    }

    fn get_def_raw_body(&self, def_id: DefId, interner: &dyn TypeDatabase) -> Option<TypeId> {
        (**self).get_def_raw_body(def_id, interner)
    }
}

// =============================================================================
// TypeEnvironment
// =============================================================================

/// A type environment that maps symbol refs to their resolved types.
/// This is populated before type checking and passed to the `SubtypeChecker`.
#[derive(Clone, Debug, Default)]
pub struct TypeEnvironment {
    /// Monotonic revision for local resolver-visible mutations.
    generation: u64,
    /// Maps symbol references to their resolved structural types.
    types: FxHashMap<u32, TypeId>,
    /// Maps symbol references to their type parameters (for generic types).
    type_params: FxHashMap<u32, Vec<TypeParamInfo>>,
    /// Maps primitive intrinsic kinds to their boxed interface types (Rule #33).
    boxed_types: FxHashMap<IntrinsicKind, TypeId>,
    /// The Array<T> interface type from lib.d.ts.
    array_base_type: Option<TypeId>,
    /// Type parameters for the Array<T> interface (usually just [T]).
    array_base_type_params: Vec<TypeParamInfo>,
    /// The `ReadonlyArray<T>` interface type from lib.d.ts.
    readonly_array_base_type: Option<TypeId>,
    /// Maps `DefIds` to their resolved structural types.
    def_types: FxHashMap<u32, TypeId>,
    /// Maps `DefIds` to their type parameters (for generic types with Lazy refs).
    def_type_params: FxHashMap<u32, Vec<TypeParamInfo>>,
    /// Maps `DefIds` to explicit `in`/`out` variance annotations.
    declared_variances: FxHashMap<u32, Arc<[Variance]>>,
    /// Maps `DefIds` back to `SymbolIds` for `InheritanceGraph` lookups.
    def_to_symbol: FxHashMap<u32, SymbolId>,
    /// Maps `SymbolIds` to `DefIds` for Ref -> Lazy migration.
    symbol_to_def: FxHashMap<u32, DefId>,
    /// Set of `DefIds` that correspond to numeric enums.
    numeric_enums: FxHashSet<u32>,
    /// Maps `DefIds` to their `DefKind` (Task #32: Graph Isomorphism).
    def_kinds: FxHashMap<u32, crate::def::DefKind>,
    /// Maps enum `DefIds` to their namespace object types (for `typeof Enum`).
    enum_namespace_types: FxHashMap<u32, TypeId>,
    /// Maps enum member `DefIds` to their parent enum `DefId`.
    enum_parents: FxHashMap<u32, DefId>,
    /// Reverse of `enum_parents`: parent enum `DefId` -> ordered list of member
    /// `DefIds`. Iteration order follows declaration order via `Vec` push.
    /// Used by control-flow narrowing to decompose a whole-enum source into
    /// the union of its member-typed values (matching tsc's
    /// `getBaseTypeOfEnumType` narrowing model).
    enum_members: FxHashMap<u32, Vec<DefId>>,
    /// Maps class `DefIds` to their instance types.
    class_instance_types: FxHashMap<u32, TypeId>,
    /// Maps `IntrinsicKind` to all `DefIds` that correspond to that boxed type.
    boxed_def_ids: FxHashMap<IntrinsicKind, Vec<DefId>>,
    /// Maps class `DefIds` to their parent class `DefId` (for class hierarchy checks).
    class_extends: FxHashMap<u32, DefId>,
    /// Reverse map: instance `TypeId` → class `DefId` (for nominal instanceof narrowing).
    instance_type_to_class: FxHashMap<u32, DefId>,
    /// Shared `DefinitionStore` for fallback lookups (e.g., `DefKind` when `def_kinds`
    /// map wasn't populated due to `RefCell` borrow conflicts during recursive resolution).
    definition_store: Option<Arc<DefinitionStore>>,
    /// The concrete type that `ThisType` should resolve to in the current context.
    /// Set by the checker when performing relation checks inside a class scope.
    this_type: Option<TypeId>,
    /// Cache of `UnresolvedTypeName(name)` -> `DefId` resolutions populated by
    /// the checker once the merged binder graph is available. Lets the
    /// solver-side type evaluator reduce cross-file qualified-name residue
    /// (e.g. `Application(UnresolvedTypeName("util.OmitKeys"), args)`) without
    /// needing access to the full checker context.
    unresolved_name_resolutions: FxHashMap<String, DefId>,
    /// Canonical `[Symbol.xxx]` property name -> `SymbolRef` mapping.
    ///
    /// Populated by checker-side computed-property resolution and consumed by
    /// solver-side `keyof` evaluation to preserve unique-symbol key identity.
    well_known_symbol_name_to_ref: FxHashMap<String, SymbolRef>,
    /// Maps a merged interface+value `SymbolRef` to its VALUE-space type for
    /// `typeof` queries.
    ///
    /// A symbol declared as both an interface and a value (declaration merging,
    /// e.g. `interface Date {} declare var Date: DateConstructor`, or
    /// `interface Foo {} declare var Foo: {...}`) stores its TYPE-space
    /// (instance) type under the shared `SymbolRef`/`DefId`, because that is
    /// what type-position references (`x: Date`) need. A `typeof X` query on
    /// such a symbol needs the VALUE-space type (the var's type) instead. The
    /// checker computes that value type via its value-space identifier path and
    /// records it here so `resolve_type_query` returns it for the deferred
    /// `TypeQuery(SymbolRef)` shape produced by nested `typeof` positions
    /// (indexed-access, conditional, tuple). Consulted only by
    /// `resolve_type_query`, leaving `resolve_lazy`/`resolve_ref`
    /// (type-position) on the instance type.
    typeof_value_types: FxHashMap<u32, TypeId>,
}

impl TypeEnvironment {
    pub fn new() -> Self {
        Self {
            generation: 1,
            types: FxHashMap::default(),
            type_params: FxHashMap::default(),
            boxed_types: FxHashMap::default(),
            array_base_type: None,
            array_base_type_params: Vec::new(),
            readonly_array_base_type: None,
            def_types: FxHashMap::default(),
            def_type_params: FxHashMap::default(),
            declared_variances: FxHashMap::default(),
            def_to_symbol: FxHashMap::default(),
            symbol_to_def: FxHashMap::default(),
            numeric_enums: FxHashSet::default(),
            enum_namespace_types: FxHashMap::default(),
            def_kinds: FxHashMap::default(),
            enum_parents: FxHashMap::default(),
            enum_members: FxHashMap::default(),
            class_instance_types: FxHashMap::default(),
            boxed_def_ids: FxHashMap::default(),
            class_extends: FxHashMap::default(),
            instance_type_to_class: FxHashMap::default(),
            definition_store: None,
            this_type: None,
            unresolved_name_resolutions: FxHashMap::default(),
            well_known_symbol_name_to_ref: FxHashMap::default(),
            typeof_value_types: FxHashMap::default(),
        }
    }

    const fn bump_generation(&mut self) {
        self.generation = self.generation.saturating_add(1);
    }

    /// Current resolver-visible generation for this environment and shared store.
    pub fn generation(&self) -> u64 {
        self.generation.saturating_add(
            self.definition_store
                .as_ref()
                .map_or(0, |store| store.generation()),
        )
    }

    /// Record a `name -> DefId` mapping recovered by a wider resolver
    /// (typically `CheckerContext`) so the next solver-side evaluator
    /// pass can reduce `Application(UnresolvedTypeName(name), args)`.
    pub fn insert_unresolved_resolution(&mut self, name: String, def_id: DefId) {
        self.unresolved_name_resolutions.insert(name, def_id);
        self.bump_generation();
    }

    /// Look up a previously-recorded resolution for an `UnresolvedTypeName`
    /// name. Returns `None` when no mapping has been recorded yet.
    pub fn unresolved_resolution(&self, name: &str) -> Option<DefId> {
        self.unresolved_name_resolutions.get(name).copied()
    }

    /// Register the `SymbolRef` behind a canonical well-known symbol key name
    /// (e.g. `"[Symbol.iterator]"`).
    pub fn register_well_known_symbol_name(&mut self, name: String, symbol_ref: SymbolRef) {
        self.well_known_symbol_name_to_ref.insert(name, symbol_ref);
        self.bump_generation();
    }

    /// Look up a registered well-known symbol key name.
    pub fn get_well_known_symbol_ref(&self, name: &str) -> Option<SymbolRef> {
        self.well_known_symbol_name_to_ref.get(name).copied()
    }

    /// Reverse of [`TypeEnvironment::get_well_known_symbol_ref`]: the canonical
    /// `[Symbol.xxx]` name registered for a well-known symbol `SymbolRef`.
    ///
    /// The registry holds only the handful of well-known symbols, so a linear
    /// scan is cheaper than maintaining a second always-in-sync reverse map.
    pub fn lookup_well_known_symbol_name(&self, symbol: SymbolRef) -> Option<&str> {
        self.well_known_symbol_name_to_ref
            .iter()
            .find_map(|(name, &reg)| (reg == symbol).then_some(name.as_str()))
    }

    /// Set the concrete type that `ThisType` should resolve to.
    ///
    /// Called by the checker when performing relation checks inside a class
    /// scope so the solver can resolve `this` type references during
    /// subtype/identity comparisons.
    pub fn set_this_type(&mut self, this_type: Option<TypeId>) {
        if self.this_type == this_type {
            return;
        }
        self.this_type = this_type;
        self.bump_generation();
    }

    /// Set the shared `DefinitionStore` for fallback `DefKind` lookups.
    pub fn set_definition_store(&mut self, store: Arc<DefinitionStore>) {
        if self
            .definition_store
            .as_ref()
            .is_some_and(|s| Arc::ptr_eq(s, &store))
        {
            return;
        }
        self.definition_store = Some(store);
        self.bump_generation();
    }

    /// Register a symbol's resolved type.
    ///
    /// Re-inserting the mapping a symbol already has is a no-op and does not
    /// bump the generation: no later resolve call can return a different
    /// type because of it (see [`Self::generation`] consumers).
    pub fn insert(&mut self, symbol: SymbolRef, type_id: TypeId) {
        if self.types.get(&symbol.0) == Some(&type_id) {
            return;
        }
        self.types.insert(symbol.0, type_id);
        self.bump_generation();
    }

    /// Register a boxed type for a primitive (Rule #33).
    pub fn set_boxed_type(&mut self, kind: IntrinsicKind, type_id: TypeId) {
        self.boxed_types.insert(kind, type_id);
        self.bump_generation();
    }

    /// Get the boxed type for a primitive.
    pub fn get_boxed_type(&self, kind: IntrinsicKind) -> Option<TypeId> {
        self.boxed_types.get(&kind).copied()
    }

    /// Register a `DefId` as belonging to a boxed type.
    pub fn register_boxed_def_id(&mut self, kind: IntrinsicKind, def_id: DefId) {
        let def_ids = self.boxed_def_ids.entry(kind).or_default();
        if !def_ids.contains(&def_id) {
            def_ids.push(def_id);
            self.bump_generation();
        }
    }

    /// Check if a `DefId` corresponds to a boxed type of the given kind.
    pub fn is_boxed_def_id(&self, def_id: DefId, kind: IntrinsicKind) -> bool {
        self.boxed_def_ids
            .get(&kind)
            .is_some_and(|ids| ids.contains(&def_id))
    }

    /// Check if a `TypeId` is any known resolved form of a boxed type.
    pub fn is_boxed_type_id(&self, type_id: TypeId, kind: IntrinsicKind) -> bool {
        // First check the direct boxed type
        if self.boxed_types.get(&kind).is_some_and(|&t| t == type_id) {
            return true;
        }
        // Guard: if the type_id is registered as the direct boxed type for a
        // DIFFERENT kind, it cannot also be this kind's boxed type. This prevents
        // false matches when a DefId resolution points to a type that belongs to
        // another intrinsic kind (e.g., String DefId resolving to Function's
        // Object shape due to stale def_types entries).
        for (&other_kind, &other_ty) in &self.boxed_types {
            if other_kind != kind && other_ty == type_id {
                return false;
            }
        }
        // Check if any registered boxed DefId resolves to this TypeId
        if let Some(def_ids) = self.boxed_def_ids.get(&kind) {
            for &def_id in def_ids {
                if self.def_types.get(&def_id.0).is_some_and(|&t| t == type_id) {
                    return true;
                }
            }
        }
        false
    }

    /// Register the Array<T> interface type from lib.d.ts.
    pub fn set_array_base_type(&mut self, type_id: TypeId, type_params: Vec<TypeParamInfo>) {
        self.array_base_type = Some(type_id);
        self.array_base_type_params = type_params;
        self.bump_generation();
    }

    /// Get the Array<T> interface type.
    pub const fn get_array_base_type(&self) -> Option<TypeId> {
        self.array_base_type
    }

    /// Get the type parameters for the Array<T> interface.
    pub fn get_array_base_type_params(&self) -> &[TypeParamInfo] {
        &self.array_base_type_params
    }

    /// Register the `ReadonlyArray<T>` interface type from lib.d.ts.
    pub const fn set_readonly_array_base_type(&mut self, type_id: TypeId) {
        self.readonly_array_base_type = Some(type_id);
        self.bump_generation();
    }

    /// Get the `ReadonlyArray<T>` interface type.
    pub const fn get_readonly_array_base_type(&self) -> Option<TypeId> {
        self.readonly_array_base_type
    }

    /// Register a symbol's resolved type with type parameters.
    ///
    /// Same no-op rule as [`Self::insert`]: an identical re-registration does
    /// not bump the generation.
    pub fn insert_with_params(
        &mut self,
        symbol: SymbolRef,
        type_id: TypeId,
        params: Vec<TypeParamInfo>,
    ) {
        if self.types.get(&symbol.0) == Some(&type_id)
            && (params.is_empty() || self.type_params.get(&symbol.0) == Some(&params))
        {
            return;
        }
        self.types.insert(symbol.0, type_id);
        if !params.is_empty() {
            self.type_params.insert(symbol.0, params);
        }
        self.bump_generation();
    }

    /// Get a symbol's resolved type.
    pub fn get(&self, symbol: SymbolRef) -> Option<TypeId> {
        self.types.get(&symbol.0).copied()
    }

    /// Register the VALUE-space type a `typeof X` query should resolve to for a
    /// merged interface+value symbol. See `typeof_value_types`.
    pub fn insert_typeof_value_type(&mut self, symbol: SymbolRef, type_id: TypeId) {
        if let Some(ref store) = self.definition_store {
            store.register_typeof_value_literal_if_enabled(symbol.0, type_id); // #14345
        }
        if self.typeof_value_types.get(&symbol.0) == Some(&type_id) {
            return;
        }
        self.typeof_value_types.insert(symbol.0, type_id);
        self.bump_generation();
    }

    /// Get the registered `typeof` value-space type for a merged interface+value
    /// symbol, if any.
    pub fn get_typeof_value_type(&self, symbol: SymbolRef) -> Option<TypeId> {
        self.typeof_value_types.get(&symbol.0).copied()
    }

    /// Get a symbol's type parameters.
    pub fn get_params(&self, symbol: SymbolRef) -> Option<&[TypeParamInfo]> {
        self.type_params.get(&symbol.0).map(|v| v.as_slice())
    }

    /// Check if the environment contains a symbol.
    pub fn contains(&self, symbol: SymbolRef) -> bool {
        self.types.contains_key(&symbol.0)
    }

    // =========================================================================
    // DefId Resolution
    // =========================================================================

    /// Register a `DefId`'s resolved type.
    ///
    /// Writes to the local `def_types` cache and also to the shared
    /// `DefinitionStore` (if set) so cross-file delegation results are
    /// visible to parent checkers without explicit merge-back.
    pub fn insert_def(&mut self, def_id: DefId, type_id: TypeId) {
        self.insert_def_with_params(def_id, type_id, Vec::new());
    }

    /// Get a class `DefId`'s registered instance type.
    pub fn get_class_instance_type(&self, def_id: DefId) -> Option<TypeId> {
        self.class_instance_types.get(&def_id.0).copied()
    }

    /// Register a class `DefId`'s instance type.
    ///
    /// Writes to the local per-environment cache and, when a shared
    /// `DefinitionStore` is attached, also publishes the instance type into
    /// the shared `class_to_instance` slot so cross-file consumers can
    /// resolve `Lazy(class_def_id)` in type position without their own
    /// `class_instance_types` cache being warm.
    pub fn insert_class_instance_type(&mut self, def_id: DefId, instance_type: TypeId) {
        self.class_instance_types.insert(def_id.0, instance_type);
        // Reverse map: allow looking up which class a resolved instance type came from.
        // This is critical for instanceof narrowing to identify class types after
        // they've been resolved from Lazy(DefId) to Object types.
        self.instance_type_to_class.insert(instance_type.0, def_id);
        if let Some(ref store) = self.definition_store {
            store.register_class_instance_type(def_id, instance_type);
        }
        self.bump_generation();
    }

    /// Register a `DefId`'s resolved type with type parameters.
    ///
    /// Writes to the local cache and the shared `DefinitionStore` so
    /// cross-file delegation results are visible without merge-back.
    pub fn insert_def_with_params(
        &mut self,
        def_id: DefId,
        type_id: TypeId,
        params: Vec<TypeParamInfo>,
    ) {
        // Identical re-registration is a no-op (see `Self::insert`); checked
        // against both the local map and the shared store so the write-through
        // below is never skipped while either view is stale.
        if self.def_types.get(&def_id.0) == Some(&type_id)
            && (params.is_empty() || self.def_type_params.get(&def_id.0) == Some(&params))
            && self.definition_store.as_ref().is_none_or(|store| {
                store.body_and_params_published(
                    def_id,
                    type_id,
                    (!params.is_empty()).then_some(params.as_slice()),
                )
            })
        {
            return;
        }
        self.def_types.insert(def_id.0, type_id);
        if !params.is_empty() {
            self.def_type_params.insert(def_id.0, params.clone());
        }
        // Write through to shared store for cross-checker visibility.
        // Body and params go through the atomic single-entry-guard path:
        // publishing them in two separate writes lets a concurrent reader
        // observe a generic alias whose body is visible but whose parameter
        // list is still missing (see `set_body_with_params`).
        if let Some(ref store) = self.definition_store {
            let store_params = (!params.is_empty()).then_some(params);
            store.set_body_with_params(def_id, type_id, store_params);
        }
        self.bump_generation();
    }

    pub fn insert_declared_variances(&mut self, def_id: DefId, variances: Arc<[Variance]>) {
        if let Some(symbol) = self.def_to_symbol.get(&def_id.0).copied() {
            self.declared_variances
                .insert(symbol.0, Arc::clone(&variances));
        }
        self.declared_variances.insert(def_id.0, variances);
        self.bump_generation();
    }

    /// Get a `DefId`'s resolved type from the local cache or shared store fallback.
    pub fn get_def(&self, def_id: DefId) -> Option<TypeId> {
        self.def_types.get(&def_id.0).copied().or_else(|| {
            let body = self.definition_store.as_ref()?.get_body(def_id)?;
            tracing::debug!(
                target: "tsz::defstore_read",
                def_id = def_id.0,
                body = body.0,
                "store fallback body read (local env miss)"
            );
            Some(body)
        })
    }

    /// Resolve the redirect target for a `Lazy(DefId(N))` whose own
    /// body/params/variance are not directly available, by reinterpreting the
    /// numeric value `N` as a raw `SymbolId` and looking up the real `DefId`.
    ///
    /// This reinterpretation is sound ONLY for *zombie* `DefId`s minted via
    /// `interner.reference(SymbolRef(N))`, where `N` genuinely is a `SymbolId`.
    /// A store-registered `DefId` lives in the `DefId` number space, which is
    /// disjoint in meaning from the `SymbolId` space; resolving it through the
    /// file-agnostic symbol→def index returns whatever unrelated definition
    /// merely shares that raw numeric id. Lib symbols make this collision
    /// routine — every lib symbol keeps the `u32::MAX` declaration-file
    /// sentinel, so the index is first-writer-wins across lib binders. Concrete
    /// defect (#13862): `HTMLDivElement` resolves to `DefId(218)`; with its body
    /// not yet materialized, the old fallback re-read `218` as a `SymbolId` and
    /// answered with the def whose *symbol* id is `218` (`FileSystemEntry`),
    /// corrupting `HTMLElementTagNameMap["div"]`. Returning `None` for a
    /// registered `DefId` makes callers defer (the checker materializes the real
    /// body on demand) instead of resolving a collision.
    fn raw_symbol_fallback_def(&self, def_id: DefId) -> Option<DefId> {
        if let Some(store) = self.definition_store.as_ref()
            && store.contains(def_id)
        {
            // #14344 observability (measurement only — the early `return None`
            // below is the unchanged `#13862` behavior). A store-registered
            // `DefId(N)` here is one whose raw value `N`, reread as a `SymbolId`,
            // is *prevented* from redirecting to a colliding def. Count it only
            // when that reinterpretation would genuinely land on a DIFFERENT,
            // DIFFERENT-NAMED def (the `HTMLDivElement(218)` ->
            // `FileSystemEntry(symbol 218)` class), not on mere raw-`u32` overlap
            // (which is ~100% by construction and uninformative). Name identity
            // is interned `Atom` equality, so this is the content-difference test.
            if tsz_common::perf_counters::enabled_fast()
                && let Some(collision_def) = store.find_def_by_symbol(def_id.0)
                && collision_def != def_id
                && let (Some(canonical_name), Some(collision_name)) =
                    (store.get_name(def_id), store.get_name(collision_def))
                && canonical_name != collision_name
            {
                tsz_common::perf_counters::record_identity_collision_wrong_decl_suppressed();
            }
            return None;
        }
        self.symbol_to_def.get(&def_id.0).copied().or_else(|| {
            self.definition_store
                .as_ref()
                .and_then(|store| store.find_def_by_symbol(def_id.0))
        })
    }

    /// Get a `DefId`'s type parameters.
    ///
    /// Checks local `def_type_params` first, then falls back to `DefinitionStore`
    /// for cross-file visibility (analogous to `get_def` for type bodies).
    pub fn get_def_params(&self, def_id: DefId) -> Option<&[TypeParamInfo]> {
        self.def_type_params.get(&def_id.0).map(|v| v.as_slice())
    }

    /// Get a `DefId`'s type parameters, including from the `DefinitionStore`.
    ///
    /// This is the owned version that checks both local cache and the shared
    /// `DefinitionStore`, mirroring how `get_def` falls back to the store for
    /// type bodies. This ensures lib types like `Readonly<T>` whose params were
    /// registered in the `DefinitionStore` (but not in the local cache) are found.
    pub fn get_def_params_owned(&self, def_id: DefId) -> Option<Vec<TypeParamInfo>> {
        if let Some(local) = self.def_type_params.get(&def_id.0) {
            return Some(local.clone());
        }
        self.definition_store
            .as_ref()
            .and_then(|s| s.get_type_params(def_id))
    }

    /// Check if the environment contains a `DefId`.
    ///
    /// Checks local `def_types` first, then falls back to `DefinitionStore`.
    pub fn contains_def(&self, def_id: DefId) -> bool {
        self.def_types.contains_key(&def_id.0)
            || self
                .definition_store
                .as_ref()
                .is_some_and(|store| store.get_body(def_id).is_some())
    }

    /// Merge def entries (types and type params) from this environment into another.
    pub fn merge_defs_into(&self, target: &mut Self) {
        let mut changed = false;
        for (&key, &type_id) in &self.def_types {
            if let std::collections::hash_map::Entry::Vacant(entry) = target.def_types.entry(key) {
                entry.insert(type_id);
                changed = true;
            }
        }
        for (key, params) in &self.def_type_params {
            if let std::collections::hash_map::Entry::Vacant(entry) =
                target.def_type_params.entry(*key)
            {
                entry.insert(params.clone());
                changed = true;
            }
        }
        for (&child, &parent) in &self.class_extends {
            if let std::collections::hash_map::Entry::Vacant(entry) =
                target.class_extends.entry(child)
            {
                entry.insert(parent);
                changed = true;
            }
        }
        if changed {
            target.bump_generation();
        }
    }

    /// Vacancy-fill every local registration map from `source` into `self`.
    ///
    /// This is the comprehensive reconciliation used to make the flow-analyzer
    /// environment (`type_environment`) a complete mirror of the evaluator
    /// environment (`type_env`) after a file's type environment is built. Unlike
    /// a full `clone()` it never reallocates maps that are already populated and
    /// never discards entries that only exist in `self` (e.g. mappings written
    /// directly into the flow-analyzer env), so it is a strict superset merge:
    /// `self := self ∪ source`, keeping `self`'s value on key collisions.
    ///
    /// Entries are copied only where `self` lacks the key, so the common case
    /// (where `source` and `self` were kept in lock-step by the dual-write
    /// registration helpers) touches nothing. The shared `DefinitionStore`,
    /// `this_type`, and array-base scalars are filled only when `self` has no
    /// value, mirroring the previous full-snapshot behavior at the file
    /// preparation boundary where those scalars are otherwise unset.
    ///
    /// Maintenance contract: this overlay must list **every** local field so it
    /// can reconcile registrations that the checker writes into the evaluator
    /// env only (for example symbol-keyed `types`, `enum_parents`,
    /// `numeric_enums`, and `unresolved_name_resolutions`), which are never
    /// dual-written and therefore never reach the deferred-replay queue. When a
    /// new field is added to `TypeEnvironment`, add it here too or the
    /// flow-analyzer env will silently diverge from the evaluator env.
    pub fn overlay_missing_from(&mut self, source: &Self) {
        use std::collections::hash_map::Entry;
        let mut changed = false;

        // `copy` for `Copy` values, `clone` for owned (`Vec`/`Arc`) values.
        macro_rules! fill_map {
            ($field:ident, copy) => {
                for (key, value) in &source.$field {
                    if let Entry::Vacant(entry) = self.$field.entry(key.clone()) {
                        entry.insert(*value);
                        changed = true;
                    }
                }
            };
            ($field:ident, clone) => {
                for (key, value) in &source.$field {
                    if let Entry::Vacant(entry) = self.$field.entry(key.clone()) {
                        entry.insert(value.clone());
                        changed = true;
                    }
                }
            };
        }

        fill_map!(types, copy);
        fill_map!(type_params, clone);
        fill_map!(boxed_types, copy);
        fill_map!(def_types, copy);
        fill_map!(def_type_params, clone);
        fill_map!(declared_variances, clone);
        fill_map!(def_to_symbol, copy);
        fill_map!(symbol_to_def, copy);
        fill_map!(def_kinds, copy);
        fill_map!(enum_namespace_types, copy);
        fill_map!(enum_parents, copy);
        fill_map!(enum_members, clone);
        fill_map!(class_instance_types, copy);
        fill_map!(boxed_def_ids, clone);
        fill_map!(class_extends, copy);
        fill_map!(instance_type_to_class, copy);
        fill_map!(unresolved_name_resolutions, copy);
        fill_map!(well_known_symbol_name_to_ref, copy);
        fill_map!(typeof_value_types, copy);

        for value in &source.numeric_enums {
            if self.numeric_enums.insert(*value) {
                changed = true;
            }
        }

        if self.array_base_type.is_none() && source.array_base_type.is_some() {
            self.array_base_type = source.array_base_type;
            self.array_base_type_params = source.array_base_type_params.clone();
            changed = true;
        }
        if self.readonly_array_base_type.is_none() && source.readonly_array_base_type.is_some() {
            self.readonly_array_base_type = source.readonly_array_base_type;
            changed = true;
        }
        if self.this_type.is_none() && source.this_type.is_some() {
            self.this_type = source.this_type;
            changed = true;
        }
        if self.definition_store.is_none()
            && let Some(store) = source.definition_store.as_ref()
        {
            self.definition_store = Some(Arc::clone(store));
            changed = true;
        }

        if changed {
            self.bump_generation();
        }
    }

    /// Return the first `DefId`-keyed entry on which `self` and `other` disagree.
    ///
    /// The checker owns two `TypeEnvironment` instances with distinct
    /// lifecycles: the evaluator env (`type_env`, authoritative) and the
    /// flow-analyzer env (`type_environment`, reconciled from the evaluator env
    /// via [`Self::overlay_missing_from`] at the file-preparation boundary).
    /// Because `overlay_missing_from` is a vacancy-only superset merge that
    /// keeps `self`'s value on key collisions, after reconciliation the two
    /// envs must *agree* on every shared `DefId -> TypeId` (and the related
    /// `DefId`-keyed structural) entry. A disagreement means a mirror-write was
    /// applied to one env but not the other with a different value — exactly the
    /// silent `DefId -> TypeId` divergence that produces query-site-dependent
    /// wrong types.
    ///
    /// Returns `None` when the two envs are consistent. The returned tuple is
    /// `(map_name, raw_key, self_value, other_value)` for diagnostics. This is a
    /// read-only consistency probe used by the checker's debug-mode reconciliation
    /// assertion; it never mutates either env and is not on any hot path.
    #[must_use]
    pub fn first_def_divergence_from(&self, other: &Self) -> Option<(&'static str, u32, u32, u32)> {
        // Only keys present in *both* maps can disagree; vacancy-fill cannot
        // create a conflicting value, so missing-on-one-side is not divergence.
        for (&key, &value) in &self.def_types {
            if let Some(&other_value) = other.def_types.get(&key)
                && other_value != value
            {
                return Some(("def_types", key, value.0, other_value.0));
            }
        }
        for (&key, &value) in &self.class_instance_types {
            if let Some(&other_value) = other.class_instance_types.get(&key)
                && other_value != value
            {
                return Some(("class_instance_types", key, value.0, other_value.0));
            }
        }
        for (&key, &value) in &self.class_extends {
            if let Some(&other_value) = other.class_extends.get(&key)
                && other_value != value
            {
                return Some(("class_extends", key, value.0, other_value.0));
            }
        }
        None
    }

    /// Collect every shared `def_types` key whose value differs between `self`
    /// and `other`.
    ///
    /// Companion to [`Self::first_def_divergence_from`]: where that probe
    /// returns only the first disagreement for a debug assertion, this returns
    /// the full set so the checker can converge the benign (structurally
    /// identical) subset at the file-preparation reconciliation boundary before
    /// re-probing the residual. `overlay_missing_from` is a vacancy-only merge
    /// and therefore cannot touch a `DefId` that is present-but-different in
    /// both envs; that residual class is dominated by recursive
    /// self-referential interfaces whose self-reference is materialized at
    /// different resolution points and so interns to distinct — but
    /// coinductively equal — `TypeId`s per env (#13944).
    ///
    /// Each tuple is `(raw_def_key, self_value, other_value)`. Read-only; never
    /// mutates either env.
    #[must_use]
    pub fn collect_def_type_divergences_from(&self, other: &Self) -> Vec<(u32, TypeId, TypeId)> {
        let mut out = Vec::new();
        for (&key, &value) in &self.def_types {
            if let Some(&other_value) = other.def_types.get(&key)
                && other_value != value
            {
                out.push((key, value, other_value));
            }
        }
        out
    }

    /// Overwrite **only** the local `def_types` cache entry for `raw_def_key`,
    /// leaving the shared `DefinitionStore` write-through that
    /// [`Self::insert_def`] performs untouched.
    ///
    /// Used by flow/evaluator env reconciliation to canonicalize a
    /// flow-analyzer-env `def_types` entry onto the evaluator env's
    /// authoritative value when the two hold structurally identical recursive
    /// types interned at distinct `TypeId`s (#13944). The shared store already
    /// holds the authoritative body, so re-publishing it through `insert_def`
    /// would be redundant churn; this canonicalizes the divergent local cache
    /// entry in place.
    pub fn set_local_def_type(&mut self, raw_def_key: u32, type_id: TypeId) {
        if self.def_types.get(&raw_def_key) == Some(&type_id) {
            return;
        }
        self.def_types.insert(raw_def_key, type_id);
        self.bump_generation();
    }

    /// Snapshot the local DefId -> TypeId cache for downstream consumers like declaration emit.
    pub fn snapshot_def_types(&self) -> FxHashMap<u32, TypeId> {
        self.def_types.clone()
    }

    /// Snapshot the local class DefId -> instance TypeId cache for cross-checker merge-back.
    pub fn snapshot_class_instance_types(&self) -> FxHashMap<u32, TypeId> {
        self.class_instance_types.clone()
    }

    /// Snapshot the local class DefId -> parent class/interface DefId cache for cross-checker merge-back.
    pub fn snapshot_class_extends(&self) -> FxHashMap<u32, DefId> {
        self.class_extends.clone()
    }

    /// Snapshot the local DefId -> type params cache for downstream consumers like declaration emit.
    pub fn snapshot_def_type_params(&self) -> FxHashMap<u32, Vec<TypeParamInfo>> {
        self.def_type_params.clone()
    }

    /// Snapshot boxed primitive interface identities for downstream declaration emit.
    pub fn snapshot_boxed_types(&self) -> FxHashMap<IntrinsicKind, TypeId> {
        self.boxed_types.clone()
    }

    /// Snapshot boxed primitive `DefId` identities for downstream declaration emit.
    pub fn snapshot_boxed_def_ids(&self) -> FxHashMap<IntrinsicKind, Vec<DefId>> {
        self.boxed_def_ids.clone()
    }

    /// Snapshot canonical well-known symbol key names for downstream declaration emit.
    pub fn snapshot_well_known_symbol_names(&self) -> FxHashMap<String, SymbolRef> {
        self.well_known_symbol_name_to_ref.clone()
    }

    // =========================================================================
    // DefKind Storage (Task #32: Graph Isomorphism)
    // =========================================================================

    /// Register a `DefId`'s `DefKind`.
    pub fn insert_def_kind(&mut self, def_id: DefId, kind: crate::def::DefKind) {
        self.def_kinds.insert(def_id.0, kind);
        self.bump_generation();
    }

    /// Get a `DefId`'s `DefKind`.
    ///
    /// First checks the local `def_kinds` map, then falls back to the shared
    /// `DefinitionStore` if available. The fallback is needed because
    /// `insert_def_kind` can fail during recursive type resolution when the
    /// `TypeEnvironment` is behind a `RefCell` that's already borrowed.
    pub fn get_def_kind(&self, def_id: DefId) -> Option<crate::def::DefKind> {
        self.def_kinds
            .get(&def_id.0)
            .copied()
            .or_else(|| self.definition_store.as_ref()?.get_kind(def_id))
    }

    // =========================================================================
    // DefId <-> SymbolId Bridge
    // =========================================================================

    /// Register a mapping from `DefId` to `SymbolId` for `InheritanceGraph` lookups.
    ///
    /// Also registers the reverse mapping (`SymbolId` -> `DefId`).
    pub fn register_def_symbol_mapping(&mut self, def_id: DefId, sym_id: SymbolId) {
        self.def_to_symbol.insert(def_id.0, sym_id);
        self.symbol_to_def.insert(sym_id.0, def_id);
        self.bump_generation();
    }

    /// Register a `DefId` as a numeric enum.
    pub fn register_numeric_enum(&mut self, def_id: DefId) {
        self.numeric_enums.insert(def_id.0);
        self.bump_generation();
    }

    /// Check if a `DefId` is a numeric enum.
    pub fn is_numeric_enum(&self, def_id: DefId) -> bool {
        self.numeric_enums.contains(&def_id.0)
    }

    /// Register an enum's namespace object type (for `typeof Enum`).
    pub fn register_enum_namespace_type(&mut self, def_id: DefId, ns_type: TypeId) {
        self.enum_namespace_types.insert(def_id.0, ns_type);
        self.bump_generation();
    }

    /// Get an enum's namespace object type.
    pub fn get_enum_namespace_type(&self, def_id: DefId) -> Option<TypeId> {
        self.enum_namespace_types.get(&def_id.0).copied()
    }

    // =========================================================================
    // Enum Parent Relationships
    // =========================================================================

    /// Register an enum member's parent enum `DefId`.
    pub fn register_enum_parent(&mut self, member_def_id: DefId, parent_def_id: DefId) {
        // Only append to the reverse map on the first registration so repeated
        // calls (the computed-symbol pass registers each member twice) do not
        // duplicate members.
        if self
            .enum_parents
            .insert(member_def_id.0, parent_def_id)
            .is_none()
        {
            self.enum_members
                .entry(parent_def_id.0)
                .or_default()
                .push(member_def_id);
        }
        // Write through to the program-wide shared store. The per-file env reset
        // wipes `self.enum_parents` before a consuming file narrows on a
        // cross-file enum discriminant; the shared store survives that reset so
        // `get_enum_parent` can recover the member->parent edge at narrowing
        // time (see `DefinitionStore::enum_member_to_parent`).
        if let Some(store) = self.definition_store.as_ref() {
            store.register_enum_parent(member_def_id, parent_def_id);
        }
        self.bump_generation();
    }

    /// Get the parent enum `DefId` for an enum member `DefId`.
    ///
    /// Falls back to the shared program-wide `DefinitionStore` when the
    /// file-local `enum_parents` map lacks the entry. The local map is reset per
    /// file, so a consuming file's flow-analyzer env (which drives cross-file
    /// enum discriminant narrowing) would otherwise see `None` for a member
    /// declared in another file, collapsing the narrowed receiver to `never`.
    pub fn get_enum_parent(&self, member_def_id: DefId) -> Option<DefId> {
        self.enum_parents
            .get(&member_def_id.0)
            .copied()
            .or_else(|| {
                self.definition_store
                    .as_ref()
                    .and_then(|store| store.get_enum_parent(member_def_id))
            })
    }

    /// Get the registered member `DefIds` for a parent enum `DefId` (in
    /// declaration order). Returns an empty slice when the enum has no
    /// registered members.
    pub fn get_enum_member_defs(&self, parent_def_id: DefId) -> &[DefId] {
        self.enum_members
            .get(&parent_def_id.0)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    // =========================================================================
    // Class Extends Relationships
    // =========================================================================

    /// Register a class's parent class `DefId`.
    pub fn register_class_extends(&mut self, child_def_id: DefId, parent_def_id: DefId) {
        self.class_extends.insert(child_def_id.0, parent_def_id);
        self.bump_generation();
    }

    /// Get the parent class `DefId` for a class.
    pub fn get_class_extends_def(&self, def_id: DefId) -> Option<DefId> {
        self.class_extends.get(&def_id.0).copied()
    }

    /// Reverse-lookup: get the class `DefId` for a resolved instance `TypeId`.
    pub fn class_def_for_instance(&self, type_id: TypeId) -> Option<DefId> {
        self.instance_type_to_class.get(&type_id.0).copied()
    }
}

impl TypeResolver for TypeEnvironment {
    fn resolver_generation(&self) -> u64 {
        self.generation()
    }

    fn canonical_def_id(&self, def_id: DefId) -> DefId {
        self.definition_store
            .as_ref()
            .map_or(def_id, |store| store.canonical_def_id(def_id))
    }

    fn resolve_ref(&self, symbol: SymbolRef, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        self.get(symbol)
    }

    fn resolve_unresolved_type_name(&self, name: &str) -> Option<DefId> {
        self.unresolved_resolution(name)
    }

    fn resolve_well_known_symbol_name(&self, name: &str) -> Option<SymbolRef> {
        self.get_well_known_symbol_ref(name)
    }

    fn well_known_symbol_name_for_ref(&self, symbol: SymbolRef) -> Option<&str> {
        self.lookup_well_known_symbol_name(symbol)
    }

    fn resolve_type_query(&self, symbol: SymbolRef, interner: &dyn TypeDatabase) -> Option<TypeId> {
        // For TypeQuery (typeof X), we need the VALUE-space type:
        // - For classes: the constructor type (stored under DefId in the types map)
        // - For other symbols: same as resolve_ref
        //
        // The SymbolRef entry may contain the instance type (inserted by
        // type_reference_symbol_type via insert_type_env_symbol), but the DefId
        // entry always has the constructor type (inserted by get_type_of_symbol).
        // A merged interface+value symbol stores its instance (type-space) type
        // under the shared `DefId`. When the checker has recorded the distinct
        // value-space type for the `typeof` query, prefer it so nested
        // `typeof X` positions resolve to the value/constructor side.
        if let Some(&value_ty) = self.typeof_value_types.get(&symbol.0) {
            return Some(value_ty);
        }
        // Prefer the class `DefId` constructor type, else the `SymbolRef` lookup,
        // then #14345-substitute the program-wide literal on a self-loop body.
        let candidate = (self.symbol_to_def.get(&symbol.0))
            .and_then(|&def_id| self.get_def(DefId(def_id.0)))
            .or_else(|| self.get(symbol));
        (self.definition_store.as_ref())
            .and_then(|store| store.typeof_self_loop_literal(symbol, candidate, interner))
            .or(candidate)
    }

    fn resolve_lazy(&self, def_id: DefId, interner: &dyn TypeDatabase) -> Option<TypeId> {
        let augment = |def_id, ty| {
            self.definition_store.as_ref().map_or(ty, |store| {
                store.module_augmented_body_or_current(def_id, ty, interner)
            })
        };
        // For classes, return the instance type (type position) instead of the constructor type
        if let Some(&instance_type) = self.class_instance_types.get(&def_id.0) {
            return Some(instance_type);
        }
        if let Some(ty) = self.get_def(def_id) {
            return Some(augment(def_id, ty));
        }

        // Fallback: a zombie `Lazy(DefId(N))` may carry raw `SymbolId(N)`.
        // Registered `DefId`s never redirect through this path (#13862).
        let real_def = self.raw_symbol_fallback_def(def_id)?;
        tsz_common::perf_counters::record_type_environment_raw_symbol_lazy_fallback();
        tracing::trace!(
            target: "tsz::solver::def_id",
            raw_def_id = def_id.0,
            redirected_def_id = real_def.0,
            "resolved lazy type through raw SymbolRef fallback"
        );
        if let Some(&instance_type) = self.class_instance_types.get(&real_def.0) {
            return Some(instance_type);
        }
        self.get_def(real_def).map(|ty| augment(real_def, ty))
    }

    fn resolve_this_type(&self, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        self.this_type
    }

    fn get_type_params(&self, symbol: SymbolRef) -> Option<Vec<TypeParamInfo>> {
        self.get_params(symbol).map(|s| s.to_vec())
    }

    fn get_lazy_type_params(&self, def_id: DefId) -> Option<Vec<TypeParamInfo>> {
        // Use get_def_params_owned which includes DefinitionStore fallback,
        // ensuring lib types like Readonly<T> whose params were registered
        // in the shared store (not the local cache) are found.
        self.get_def_params_owned(def_id).or_else(|| {
            // Fallback: resolve a zombie raw-SymbolId `DefId` to the real def
            // (never a registered `DefId`; #13862).
            let real_def = self.raw_symbol_fallback_def(def_id)?;
            self.get_def_params_owned(real_def)
        })
    }

    fn get_type_param_variance(&self, def_id: DefId) -> Option<Arc<[Variance]>> {
        self.declared_variances.get(&def_id.0).cloned().or_else(|| {
            // Fallback: redirect a zombie raw-SymbolId `DefId` to the real def
            // (never a registered `DefId`; #13862).
            let real_def = self.raw_symbol_fallback_def(def_id)?;
            self.declared_variances.get(&real_def.0).cloned()
        })
    }

    fn get_boxed_type(&self, kind: IntrinsicKind) -> Option<TypeId> {
        Self::get_boxed_type(self, kind)
    }

    fn is_boxed_def_id(&self, def_id: DefId, kind: IntrinsicKind) -> bool {
        Self::is_boxed_def_id(self, def_id, kind)
    }

    fn is_boxed_type_id(&self, type_id: TypeId, kind: IntrinsicKind) -> bool {
        Self::is_boxed_type_id(self, type_id, kind)
    }

    fn get_array_base_type(&self) -> Option<TypeId> {
        Self::get_array_base_type(self)
    }

    fn get_array_base_type_params(&self) -> &[TypeParamInfo] {
        Self::get_array_base_type_params(self)
    }

    fn get_readonly_array_base_type(&self) -> Option<TypeId> {
        Self::get_readonly_array_base_type(self)
    }

    fn def_to_symbol_id(&self, def_id: DefId) -> Option<SymbolId> {
        self.def_to_symbol.get(&def_id.0).copied().or_else(|| {
            self.definition_store
                .as_ref()
                .and_then(|store| store.get(def_id))
                .and_then(|info| info.symbol_id)
                .map(SymbolId)
        })
    }

    fn symbol_to_def_id(&self, symbol: SymbolRef) -> Option<DefId> {
        self.symbol_to_def.get(&symbol.0).copied().or_else(|| {
            // Fallback: check the shared DefinitionStore for DefIds created in
            // other checker contexts (e.g., lib symbols resolved before this
            // TypeEnvironment was populated). This eliminates the need for
            // callers to fall back to `interner.reference(SymbolRef)` which
            // creates unregistered zombie DefIds.
            self.definition_store
                .as_ref()
                .and_then(|store| store.find_def_by_symbol(symbol.0))
        })
    }

    fn get_def_kind(&self, def_id: DefId) -> Option<crate::def::DefKind> {
        Self::get_def_kind(self, def_id)
    }

    fn get_def_name(&self, def_id: DefId) -> Option<tsz_common::interner::Atom> {
        self.definition_store
            .as_ref()
            .and_then(|store| store.get_name(def_id))
    }

    fn is_numeric_enum(&self, def_id: DefId) -> bool {
        Self::is_numeric_enum(self, def_id)
    }

    fn get_enum_parent_def_id(&self, member_def_id: DefId) -> Option<DefId> {
        Self::get_enum_parent(self, member_def_id)
    }

    fn get_enum_member_def_ids(&self, parent_def_id: DefId) -> Vec<DefId> {
        Self::get_enum_member_defs(self, parent_def_id).to_vec()
    }

    fn is_enum_type(&self, type_id: TypeId, interner: &dyn TypeDatabase) -> bool {
        use crate::visitors::visitor_extract::enum_components;
        if let Some((def_id, _)) = enum_components(interner, type_id) {
            // A full enum type's DefId is NOT registered as a member (key) in
            // enum_parents. Member DefIds ARE keys (mapping to their parent
            // DefId). So if the DefId has no parent, it's the parent enum type.
            // Use the store-fallback-aware `get_enum_parent` so a member
            // declared in another file (absent from the per-file `enum_parents`
            // map after the file-session reset) is not misclassified as a whole
            // enum type.
            Self::get_enum_parent(self, def_id).is_none()
        } else {
            false
        }
    }

    fn is_user_enum_def(&self, _def_id: DefId) -> bool {
        // TypeEnvironment doesn't have access to binder symbol information
        false
    }

    fn get_enum_namespace_type(&self, def_id: DefId) -> Option<TypeId> {
        Self::get_enum_namespace_type(self, def_id)
    }

    fn get_class_extends(&self, def_id: DefId) -> Option<DefId> {
        self.get_class_extends_def(def_id)
    }

    fn class_def_for_instance_type(&self, type_id: TypeId) -> Option<DefId> {
        self.class_def_for_instance(type_id).or_else(|| {
            let store = self.definition_store.as_ref()?;
            let def_id = store.find_def_for_type(type_id)?;
            matches!(self.get_def_kind(def_id), Some(crate::def::DefKind::Class)).then_some(def_id)
        })
    }

    fn def_for_type(&self, type_id: TypeId) -> Option<DefId> {
        self.definition_store
            .as_ref()
            .and_then(|store| store.find_def_for_type(type_id))
    }

    fn get_def_raw_body(&self, def_id: DefId, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        // Check the local def_types cache first, then the shared DefinitionStore.
        // Also handle zombie raw-SymbolId `DefId`s (from interner.reference) via
        // the same fallback used in resolve_lazy (never a registered `DefId`;
        // #13862).
        self.def_types
            .get(&def_id.0)
            .copied()
            .or_else(|| self.definition_store.as_ref()?.get_body(def_id))
            .or_else(|| {
                let real_def = self.raw_symbol_fallback_def(def_id)?;
                let store = self.definition_store.as_ref()?;
                self.def_types
                    .get(&real_def.0)
                    .copied()
                    .or_else(|| store.get_body(real_def))
            })
    }

    fn def_is_non_program(&self, def_id: DefId) -> bool {
        // A def is non-program (lib/ambient) when the shared `DefinitionStore`
        // records its `decl_file_idx` as the lib sentinel. Consulted by the
        // default `is_genuine_unknown_alias_body` to keep a lib utility's
        // not-yet-materialized `unknown` body from being mistaken for a genuine
        // `unknown` alias (issue #14337).
        self.definition_store
            .as_ref()
            .is_some_and(|store| store.def_is_non_program(def_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::def::DefinitionInfo;
    use crate::types::IntrinsicKind;
    use std::sync::Arc;

    /// Regression test: `is_boxed_type_id` must not match a TypeId that is
    /// registered as the direct boxed type for a DIFFERENT kind.
    ///
    /// Previously, a String DefId's resolved type in `def_types` could match
    /// Function's Object TypeId, causing `is_boxed_type_id(Function_TypeId`, String)
    /// to return true. This made `string` incorrectly assignable to `Function`.
    #[test]
    fn test_is_boxed_type_id_cross_kind_guard() {
        let mut env = TypeEnvironment::new();

        let string_type = TypeId(100);
        let function_type = TypeId(200);
        let string_def = DefId(50);

        // Register String boxed type (direct)
        env.set_boxed_type(IntrinsicKind::String, string_type);
        // Register Function boxed type (direct)
        env.set_boxed_type(IntrinsicKind::Function, function_type);
        // Register String DefId
        env.register_boxed_def_id(IntrinsicKind::String, string_def);
        // Simulate the bug: String DefId resolves to Function's TypeId
        env.insert_def(string_def, function_type);

        // Without the guard, this would return true (String DefId resolves to function_type)
        // With the guard, it should return false (function_type is registered for Function kind)
        assert!(
            !env.is_boxed_type_id(function_type, IntrinsicKind::String),
            "Function TypeId should NOT be identified as String's boxed type"
        );

        // Function TypeId should still match its own kind
        assert!(
            env.is_boxed_type_id(function_type, IntrinsicKind::Function),
            "Function TypeId should match Function kind"
        );

        // String TypeId should match its own kind
        assert!(
            env.is_boxed_type_id(string_type, IntrinsicKind::String),
            "String TypeId should match String kind"
        );
    }

    #[test]
    fn resolve_lazy_raw_symbol_fallback_redirects_to_real_def() {
        let mut env = TypeEnvironment::new();
        let interner = crate::construction::TypeInterner::new();

        let raw_symbol = SymbolRef(7);
        let real_def = DefId(42);
        let resolved_type = TypeId(99);

        env.symbol_to_def.insert(raw_symbol.0, real_def);
        env.insert_def(real_def, resolved_type);

        assert_eq!(
            env.resolve_lazy(DefId(raw_symbol.0), &interner),
            Some(resolved_type)
        );
    }

    /// Regression (#13862): `resolve_lazy` must never reinterpret a *registered*
    /// `DefId`'s numeric value as a `SymbolId`.
    ///
    /// `Lazy(DefId(N))` is created both for genuine registered definitions and
    /// for zombie references (`interner.reference(SymbolRef(N))`, where `N` is a
    /// raw `SymbolId`). The raw-symbol fallback is only sound for the zombie
    /// case. For a registered `DefId` whose body is not yet materialized,
    /// resolving it through `find_def_by_symbol(def_id.0)` collides with the
    /// unrelated definition that merely shares that raw numeric id — exactly the
    /// cross-lib-binder defect where `HTMLElementTagNameMap["div"]` resolved
    /// `HTMLDivElement` (= `DefId(218)`) to the def whose *symbol* id is `218`
    /// (`FileSystemEntry`). Lib symbols all carry the `u32::MAX` declaration-file
    /// sentinel, so the file-agnostic symbol→def index is first-writer-wins
    /// across lib binders, making the collision routine.
    #[test]
    fn resolve_lazy_does_not_symbol_conflate_a_registered_def() {
        let interner = crate::construction::TypeInterner::new();
        let store = Arc::new(DefinitionStore::new());

        // The "real" def (HTMLDivElement-like): registered, body NOT materialized.
        let real = store.register(DefinitionInfo::interface(
            interner.intern_string("ElementLike"),
            vec![],
            vec![],
        ));

        // A collider whose *SymbolId* equals `real`'s `DefId` numeric value and
        // which carries a concrete body, so `find_def_by_symbol(real.0)` resolves
        // to it.
        let mut collider_info =
            DefinitionInfo::type_alias(interner.intern_string("Collider"), vec![], TypeId::STRING);
        collider_info.symbol_id = Some(real.0);
        let collider = store.register(collider_info);
        store.set_body(collider, TypeId::STRING);
        assert_eq!(store.find_def_by_symbol(real.0), Some(collider));

        let mut env = TypeEnvironment::new();
        env.set_definition_store(Arc::clone(&store));

        // `real`'s body is unmaterialized. The pre-fix code returned the
        // collider's body via symbol conflation; the fix defers instead.
        assert_eq!(env.resolve_lazy(real, &interner), None);
    }

    /// #14344 identity-collision observability: the wrong-decl counter fires on
    /// a GENUINE content collision (the `#13862`-suppressed
    /// `HTMLDivElement(218)` -> `FileSystemEntry(symbol 218)` class) and stays
    /// silent for a store-registered def with no different-named collider. This
    /// is measurement only — `resolve_lazy` returns `None` either way (behavior
    /// unchanged). The counter is the migration's md5-stability regression
    /// signal.
    #[test]
    fn identity_collision_counter_fires_on_genuine_content_collision_only() {
        use tsz_common::perf_counters::{counters, force_enable_perf_counters_for_tests};

        // Force the gate on so we can observe `fetch_add` deltas regardless of
        // env / `OnceLock` state (the recorder short-circuits when disabled).
        force_enable_perf_counters_for_tests();

        let read = || {
            counters()
                .identity_collision_wrong_decl_suppressed
                .load(std::sync::atomic::Ordering::Relaxed)
        };

        let interner = crate::construction::TypeInterner::new();

        // --- Case 1: genuine content collision (different-named decls). ---
        let store = Arc::new(DefinitionStore::new());
        let real = store.register(DefinitionInfo::interface(
            interner.intern_string("ElementLike"),
            vec![],
            vec![],
        ));
        let mut collider_info =
            DefinitionInfo::type_alias(interner.intern_string("Collider"), vec![], TypeId::STRING);
        collider_info.symbol_id = Some(real.0);
        let collider = store.register(collider_info);
        store.set_body(collider, TypeId::STRING);
        // Precondition: the raw-`u32` reinterpretation lands on the differently
        // named collider, i.e. the content actually differs.
        assert_eq!(store.find_def_by_symbol(real.0), Some(collider));

        let mut env = TypeEnvironment::new();
        env.set_definition_store(Arc::clone(&store));

        let before = read();
        // Behavior is unchanged: the `#13862` guard still defers.
        assert_eq!(env.resolve_lazy(real, &interner), None);
        assert_eq!(
            read() - before,
            1,
            "a genuine different-named raw-u32 collision must be counted exactly once"
        );

        // --- Case 2: store-registered def with NO collider at its raw id. ---
        // A fresh store whose only registered def has no symbol sharing its
        // `DefId` numeric value: the fallback still defers, but there is no
        // content collision, so the counter must not move.
        let store2 = Arc::new(DefinitionStore::new());
        let lonely = store2.register(DefinitionInfo::interface(
            interner.intern_string("Lonely"),
            vec![],
            vec![],
        ));
        // No def carries `symbol_id == lonely.0`, so the raw reinterpretation
        // finds nothing to collide with.
        assert_eq!(store2.find_def_by_symbol(lonely.0), None);
        let mut env2 = TypeEnvironment::new();
        env2.set_definition_store(Arc::clone(&store2));

        let before2 = read();
        assert_eq!(env2.resolve_lazy(lonely, &interner), None);
        assert_eq!(
            read() - before2,
            0,
            "raw-u32 overlap with nothing (no different-named decl) must not be counted"
        );
    }

    /// The symbol-conflation fallback stays valid for *zombie* `DefId`s — those
    /// minted by `interner.reference(SymbolRef(N))` where `DefId(N)` is itself
    /// unregistered.
    #[test]
    fn resolve_lazy_store_zombie_fallback_still_redirects() {
        let interner = crate::construction::TypeInterner::new();
        let store = Arc::new(DefinitionStore::new());

        let mut info =
            DefinitionInfo::type_alias(interner.intern_string("Real"), vec![], TypeId::NUMBER);
        info.symbol_id = Some(9000); // raw SymbolId 9000
        let real = store.register(info);
        store.set_body(real, TypeId::NUMBER);

        let mut env = TypeEnvironment::new();
        env.set_definition_store(Arc::clone(&store));

        // DefId(9000) is unregistered, so the fallback legitimately reads 9000 as
        // a SymbolId and redirects to `real`'s body.
        assert!(store.get(DefId(9000)).is_none());
        assert_eq!(
            env.resolve_lazy(DefId(9000), &interner),
            Some(TypeId::NUMBER)
        );
    }

    #[test]
    fn resolve_lazy_raw_symbol_fallback_preserves_class_instance_type() {
        let mut env = TypeEnvironment::new();
        let interner = crate::construction::TypeInterner::new();

        let raw_symbol = SymbolRef(7);
        let real_def = DefId(42);
        let constructor_type = TypeId(99);
        let instance_type = TypeId(123);

        env.symbol_to_def.insert(raw_symbol.0, real_def);
        env.insert_def(real_def, constructor_type);
        env.class_instance_types.insert(real_def.0, instance_type);

        assert_eq!(
            env.resolve_lazy(DefId(raw_symbol.0), &interner),
            Some(instance_type)
        );
    }

    #[test]
    fn test_type_environment_generation_tracks_shared_store_mutations() {
        let store = Arc::new(DefinitionStore::new());
        let mut env = TypeEnvironment::new();

        let initial_generation = env.resolver_generation();
        env.set_definition_store(Arc::clone(&store));
        assert!(env.resolver_generation() > initial_generation);

        let before_store_write = env.resolver_generation();
        store.set_body(DefId(1), TypeId::STRING);
        assert!(env.resolver_generation() > before_store_write);

        let before_local_write = env.resolver_generation();
        env.insert_def(DefId(2), TypeId::NUMBER);
        assert!(env.resolver_generation() > before_local_write);
    }

    /// Pins the idempotency invariant introduced in #8269:
    /// repeated `set_definition_store` calls with the same `Arc` pointer must
    /// not bump the environment generation, while a subsequent store mutation
    /// must still be visible.
    #[test]
    fn set_definition_store_same_arc_is_generation_idempotent() {
        let store = Arc::new(DefinitionStore::new());
        let mut env = TypeEnvironment::new();

        // First install bumps generation.
        let gen_before_first = env.resolver_generation();
        env.set_definition_store(Arc::clone(&store));
        let gen_after_first = env.resolver_generation();
        assert!(
            gen_after_first > gen_before_first,
            "first set_definition_store must bump generation"
        );

        // Second install with the same Arc pointer must NOT bump generation.
        env.set_definition_store(Arc::clone(&store));
        assert_eq!(
            env.resolver_generation(),
            gen_after_first,
            "repeated set_definition_store with the same Arc must not bump generation"
        );

        // A subsequent store mutation is still visible through the generation sum.
        let gen_before_mutation = env.resolver_generation();
        store.set_body(DefId(99), TypeId::STRING);
        assert!(
            env.resolver_generation() > gen_before_mutation,
            "store mutation must still be visible after idempotent reinstall"
        );
    }

    #[test]
    fn boxed_def_id_registration_is_idempotent() {
        let mut env = TypeEnvironment::new();
        let def_id = DefId(7);

        env.register_boxed_def_id(IntrinsicKind::Function, def_id);
        let gen_after_first = env.resolver_generation();
        env.register_boxed_def_id(IntrinsicKind::Function, def_id);

        assert_eq!(
            env.resolver_generation(),
            gen_after_first,
            "re-registering the same boxed DefId must not invalidate env caches"
        );
        assert_eq!(
            env.snapshot_boxed_def_ids()
                .get(&IntrinsicKind::Function)
                .map(Vec::as_slice),
            Some(&[def_id][..])
        );
    }

    /// Issue #8720 infrastructure: `insert_class_instance_type` must write
    /// through to the shared `DefinitionStore::class_to_instance` slot when a
    /// store is attached, so any checker can observe the producer's instance
    /// type cross-file. This is the foundation for a follow-up that wires
    /// type-position class lookups through a boundary helper.
    ///
    /// The full `resolve_lazy` consumer path is intentionally out of scope
    /// for this PR — generic `resolve_lazy(class_def_id)` is read by both
    /// type-position and value-position callers, so the shared slot must be
    /// consulted only behind a position-aware boundary, not in the generic
    /// resolver.
    #[test]
    fn insert_class_instance_type_writes_through_to_shared_store() {
        let store = Arc::new(DefinitionStore::new());
        let class_def = DefId(8);
        let instance_type = TypeId(64);

        let mut env = TypeEnvironment::new();
        env.set_definition_store(Arc::clone(&store));
        env.insert_class_instance_type(class_def, instance_type);

        assert_eq!(
            store.get_class_instance_type(class_def),
            Some(instance_type)
        );
    }

    /// `insert_class_instance_type` on a store-less environment must still
    /// populate the local cache (used during checker setup before the store
    /// is wired). The local cache is consulted by `resolve_lazy` today; the
    /// shared cache write-through is purely additive.
    #[test]
    fn insert_class_instance_type_without_store_populates_local_only() {
        let class_def = DefId(8);
        let instance_type = TypeId(64);
        let interner = crate::construction::TypeInterner::new();

        let mut env = TypeEnvironment::new();
        env.insert_class_instance_type(class_def, instance_type);

        // Local cache is populated.
        assert_eq!(
            env.class_instance_types.get(&class_def.0),
            Some(&instance_type)
        );
        // `resolve_lazy` finds it through the local cache.
        assert_eq!(env.resolve_lazy(class_def, &interner), Some(instance_type));
    }

    /// The shared slot has independent producer/consumer semantics — a
    /// consumer that never received an `insert_class_instance_type` call
    /// (cross-file scenario) can still read the producer's published value
    /// via `DefinitionStore::get_class_instance_type`. This pins the
    /// infrastructure invariant the future boundary helper will rely on.
    #[test]
    fn shared_class_to_instance_visible_to_independent_environments() {
        let store = Arc::new(DefinitionStore::new());
        let class_def = DefId(101);
        let instance_type = TypeId(202);

        // Producer environment publishes via the write-through path.
        let mut producer_env = TypeEnvironment::new();
        producer_env.set_definition_store(Arc::clone(&store));
        producer_env.insert_class_instance_type(class_def, instance_type);

        // A separate consumer environment with no local entry can still
        // read the producer's instance type through the shared store.
        let consumer_env = TypeEnvironment::new();
        assert_eq!(
            consumer_env.class_instance_types.get(&class_def.0),
            None,
            "consumer's local cache must remain cold"
        );
        assert_eq!(
            store.get_class_instance_type(class_def),
            Some(instance_type),
            "shared slot must be visible to any checker holding the store"
        );
    }

    /// Pins the cross-module enum-parent residency fix: a producing env's
    /// `register_enum_parent` write-through publishes the member->parent edge to
    /// the shared `DefinitionStore`, so a *fresh* consumer env (modeling the
    /// per-file `TypeEnvironment::new()` session reset) recovers the parent via
    /// the store fallback even though its local `enum_parents` map is empty.
    ///
    /// Without the fallback, cross-file enum discriminant narrowing reads the
    /// consuming file's reset env, gets `None`, and collapses the receiver to
    /// `never` (false TS2339 in mobx's `IDerivationState_` cascade).
    #[test]
    fn enum_parent_survives_file_reset_via_shared_store() {
        let store = Arc::new(DefinitionStore::new());
        let member_def = DefId(501);
        let parent_def = DefId(500);

        // Producing file's env registers the member->parent edge and, because
        // it holds the shared store, writes through to it.
        let mut producer_env = TypeEnvironment::new();
        producer_env.set_definition_store(Arc::clone(&store));
        producer_env.register_enum_parent(member_def, parent_def);
        assert_eq!(
            store.get_enum_parent(member_def),
            Some(parent_def),
            "register_enum_parent must write through to the shared store"
        );

        // Consuming file's env is fresh (its local enum_parents is empty, as
        // after a file-session reset) but shares the same store.
        let mut consumer_env = TypeEnvironment::new();
        consumer_env.set_definition_store(Arc::clone(&store));
        assert!(
            !consumer_env.enum_parents.contains_key(&member_def.0),
            "consumer's local enum_parents must be cold"
        );
        assert_eq!(
            consumer_env.get_enum_parent(member_def),
            Some(parent_def),
            "consumer must recover the parent via the shared-store fallback"
        );
        assert_eq!(
            TypeResolver::get_enum_parent_def_id(&consumer_env, member_def),
            Some(parent_def),
            "resolver-trait accessor must use the same fallback path"
        );

        // An env with no store still returns None (no panic, no false edge).
        let bare_env = TypeEnvironment::new();
        assert_eq!(bare_env.get_enum_parent(member_def), None);
    }

    /// Pins the `get_def_kind` store-fallback path:
    /// an entry registered only in the `DefinitionStore` (not the local map)
    /// must be found once `set_definition_store` is called.
    #[test]
    fn get_def_kind_falls_back_to_definition_store() {
        use crate::TypeId;
        use crate::def::DefKind;
        use crate::def::core::DefinitionInfo;
        use tsz_common::interner::Atom;

        let store = Arc::new(DefinitionStore::new());
        let def_id = store.register(DefinitionInfo::type_alias(
            Atom::default(),
            vec![],
            TypeId::UNKNOWN,
        ));

        let mut env = TypeEnvironment::new();
        // No store wired → fallback returns None.
        assert_eq!(
            env.get_def_kind(def_id),
            None,
            "get_def_kind must return None when no store is wired"
        );

        // Wire the store → fallback finds the kind.
        env.set_definition_store(Arc::clone(&store));
        assert_eq!(
            env.get_def_kind(def_id),
            Some(DefKind::TypeAlias),
            "get_def_kind must find kind via store fallback after set_definition_store"
        );
    }

    /// `overlay_missing_from` must fill flow-analyzer-env-local maps that only
    /// the evaluator env carries (e.g. `enum_parents`, `class_extends`, and the
    /// symbol-keyed `types` map) while preserving entries unique to the target
    /// and not overwriting existing keys.
    #[test]
    fn overlay_missing_from_fills_evaluator_only_maps_without_clobbering() {
        let source_def = DefId(10);
        let parent_def = DefId(11);
        let child_def = DefId(12);
        let shared_def = DefId(13);

        let mut source = TypeEnvironment::new();
        source.insert_def(source_def, TypeId(100));
        source.register_enum_parent(child_def, parent_def);
        source.register_class_extends(child_def, parent_def);
        source.insert(SymbolRef(7), TypeId(200));
        // A key both envs already agree on, but with the target's own value kept.
        source.insert_def(shared_def, TypeId(300));

        let mut target = TypeEnvironment::new();
        // Target keeps a flow-analyzer-only mapping the evaluator never wrote.
        target.register_class_extends(DefId(99), DefId(98));
        // Target already has `shared_def` with a different value; overlay must
        // not clobber it.
        target.insert_def(shared_def, TypeId(301));

        target.overlay_missing_from(&source);

        assert_eq!(
            target.get_def(source_def),
            Some(TypeId(100)),
            "evaluator-only def body must be filled"
        );
        assert_eq!(
            target.get_enum_parent(child_def),
            Some(parent_def),
            "evaluator-only enum-parent must be filled"
        );
        assert_eq!(
            target.get_class_extends_def(child_def),
            Some(parent_def),
            "evaluator-only class-extends must be filled"
        );
        assert_eq!(
            target.get(SymbolRef(7)),
            Some(TypeId(200)),
            "evaluator-only symbol-keyed type must be filled"
        );
        assert_eq!(
            target.get_class_extends_def(DefId(99)),
            Some(DefId(98)),
            "flow-analyzer-only entry must be preserved"
        );
        assert_eq!(
            target.get_def(shared_def),
            Some(TypeId(301)),
            "existing target value must not be overwritten on key collision"
        );
    }

    #[test]
    fn first_def_divergence_from_detects_conflicting_shared_def_body() {
        let shared_def = DefId(42);

        let mut evaluator = TypeEnvironment::new();
        evaluator.insert_def(shared_def, TypeId(100));
        // An evaluator-only entry the flow env never received: not a divergence.
        evaluator.insert_def(DefId(7), TypeId(101));

        let mut flow = TypeEnvironment::new();
        flow.insert_def(shared_def, TypeId(200));

        // Probe is order-symmetric on the conflicting key.
        let divergence = flow.first_def_divergence_from(&evaluator);
        assert_eq!(
            divergence,
            Some(("def_types", shared_def.0, 200, 100)),
            "conflicting shared def body must be reported"
        );
    }

    #[test]
    fn first_def_divergence_from_clean_after_overlay() {
        let shared_def = DefId(42);
        let eval_only_def = DefId(7);
        let flow_only_class = DefId(99);

        let mut evaluator = TypeEnvironment::new();
        evaluator.insert_def(shared_def, TypeId(100));
        evaluator.insert_def(eval_only_def, TypeId(101));

        let mut flow = TypeEnvironment::new();
        // Flow-analyzer-only mapping the evaluator never wrote.
        flow.register_class_extends(flow_only_class, DefId(98));
        // After the production reconciliation step, the flow env is overlaid
        // from the evaluator env. With no conflicting writes this must leave the
        // two envs consistent on every shared key.
        flow.overlay_missing_from(&evaluator);

        assert_eq!(
            flow.first_def_divergence_from(&evaluator),
            None,
            "post-overlay envs must agree on every shared DefId entry"
        );
    }

    #[test]
    fn collect_def_type_divergences_reports_every_present_but_different_shared_def() {
        let conflict_a = DefId(42);
        let conflict_b = DefId(43);
        let agree = DefId(44);

        let mut evaluator = TypeEnvironment::new();
        evaluator.insert_def(conflict_a, TypeId(100));
        evaluator.insert_def(conflict_b, TypeId(110));
        evaluator.insert_def(agree, TypeId(120));
        // Evaluator-only entry: vacancy, never a divergence.
        evaluator.insert_def(DefId(7), TypeId(101));

        let mut flow = TypeEnvironment::new();
        flow.insert_def(conflict_a, TypeId(200));
        flow.insert_def(conflict_b, TypeId(210));
        flow.insert_def(agree, TypeId(120));

        let mut divergences = flow.collect_def_type_divergences_from(&evaluator);
        divergences.sort_by_key(|&(key, ..)| key);
        assert_eq!(
            divergences,
            vec![
                (conflict_a.0, TypeId(200), TypeId(100)),
                (conflict_b.0, TypeId(210), TypeId(110)),
            ],
            "only present-but-different shared defs are divergences; agreeing and \
             vacant keys are excluded"
        );
    }

    #[test]
    fn set_local_def_type_canonicalizes_without_store_write_through() {
        let store = Arc::new(DefinitionStore::default());
        let shared_def = DefId(42);

        let mut evaluator = TypeEnvironment::new();
        evaluator.set_definition_store(Arc::clone(&store));
        evaluator.insert_def(shared_def, TypeId(100)); // store body = 100 (authoritative)

        let mut flow = TypeEnvironment::new();
        flow.set_definition_store(Arc::clone(&store));
        // Simulate a local-only divergence: the flow env's local cache holds a
        // different `TypeId` than the authoritative store/evaluator (as happens
        // when a recursive interface materializes to distinct interned ids per
        // env). `set_local_def_type` is the only writer that touches the local
        // cache without store write-through, so use it to seed the divergence
        // too — proving the store is untouched in both directions.
        flow.set_local_def_type(shared_def.0, TypeId(200));
        assert_eq!(
            flow.first_def_divergence_from(&evaluator),
            Some(("def_types", shared_def.0, 200, 100)),
            "local-only flow cache must diverge from the authoritative evaluator value"
        );

        // Canonicalize the flow env's local cache onto the evaluator's value.
        flow.set_local_def_type(shared_def.0, TypeId(100));
        assert_eq!(
            flow.first_def_divergence_from(&evaluator),
            None,
            "set_local_def_type must converge the flow env onto the authoritative value"
        );
        // The shared store still holds the body the evaluator published; the
        // local-only setter never writes through it in either direction.
        assert_eq!(
            store.get_body(shared_def),
            Some(TypeId(100)),
            "set_local_def_type must not write through to the shared DefinitionStore"
        );
    }

    /// #14337: a lib/ambient utility alias (e.g. `Omit`) whose real
    /// `Pick<…>` body has not yet materialized reads the `unknown` sentinel as
    /// its registered body, exactly like a genuine `type C = unknown`. The
    /// genuine-unknown classifier must NOT treat the lib placeholder as genuine
    /// (which would reduce `Omit<T, K>` to bare `unknown`, dropping the picked
    /// properties — the ts-rest `params`/`body` TS2339 false positives), while a
    /// real user-program `type C = unknown` must still classify as genuine. The
    /// discriminator is the def's file origin, NOT its name.
    #[test]
    fn lib_placeholder_unknown_alias_is_not_genuine_unknown_issue_14337() {
        let interner = crate::construction::TypeInterner::new();
        let store = Arc::new(DefinitionStore::new());

        // Lib/ambient utility whose body sentinel is `unknown` (unmaterialized).
        // The binder name is varied to confirm the rule is structural, not a
        // name match.
        let mut lib_info =
            DefinitionInfo::type_alias(interner.intern_string("Strip"), vec![], TypeId::UNKNOWN);
        lib_info.file_id = Some(DefinitionStore::NON_PROGRAM_FILE_SENTINEL);
        let lib_def = store.register(lib_info);
        store.set_body(lib_def, TypeId::UNKNOWN);

        // User-program alias genuinely declared `type C = unknown`.
        let mut user_info = DefinitionInfo::type_alias(
            interner.intern_string("GenuineUnknown"),
            vec![],
            TypeId::UNKNOWN,
        );
        user_info.file_id = Some(0);
        let user_def = store.register(user_info);
        store.set_body(user_def, TypeId::UNKNOWN);

        let mut env = TypeEnvironment::new();
        env.set_definition_store(Arc::clone(&store));

        assert!(
            store.def_is_non_program(lib_def),
            "lib def must be classified non-program by its file sentinel"
        );
        assert!(
            !store.def_is_non_program(user_def),
            "user-program def must NOT be non-program"
        );

        assert!(
            !env.is_genuine_unknown_alias_body(lib_def, &interner),
            "a lib utility's not-yet-materialized `unknown` body must NOT be a \
             genuine `unknown` alias (#14337)"
        );
        assert!(
            env.is_genuine_unknown_alias_body(user_def, &interner),
            "a user-program `type C = unknown` must still be a genuine `unknown` alias"
        );
    }
}
