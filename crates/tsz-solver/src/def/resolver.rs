//! Type resolution trait and environment.
//!
//! Defines `TypeResolver` — the trait for lazily resolving type references
//! (both legacy `SymbolRef` and modern `DefId`), and `TypeEnvironment` — the
//! standard implementation that maps identifiers to their resolved types.

use std::sync::Arc;

use crate::TypeDatabase;
use crate::def::DefId;
use crate::def::core::DefinitionStore;
use crate::types::{IntrinsicKind, SymbolRef, TypeId, TypeParamInfo};
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_binder::SymbolId;

/// Trait for resolving type references to their structural types.
/// This allows the `SubtypeChecker` to lazily resolve Ref types
/// without being tightly coupled to the binder/checker.
pub trait TypeResolver {
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

    /// Check whether two `DefIds` refer to the same declaration (same `DefId` or same `SymbolId`).
    ///
    /// Cross-context `DefId` aliasing can give the same interface different `DefIds`
    /// (e.g., lib file vs heritage clause lowering). This method handles that by
    /// falling back to `SymbolId` comparison when `DefIds` differ.
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

    /// Get the base class type for a class/interface type.
    ///
    /// Used by the Best Common Type (BCT) algorithm to find common base classes.
    fn get_base_type(&self, _type_id: TypeId, _interner: &dyn TypeDatabase) -> Option<TypeId> {
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
}

/// A no-op resolver that doesn't resolve any references.
/// Useful for tests or when symbol resolution isn't needed.
pub struct NoopResolver;

impl TypeResolver for NoopResolver {
    fn resolve_ref(&self, _symbol: SymbolRef, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        None
    }
}

/// Blanket implementation of `TypeResolver` for references to resolver types.
///
/// This allows `&dyn TypeResolver` (which is Sized) to be used wherever
/// `R: TypeResolver` is expected.
impl<T: TypeResolver + ?Sized> TypeResolver for &T {
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

    fn get_type_params(&self, symbol: SymbolRef) -> Option<Vec<TypeParamInfo>> {
        (**self).get_type_params(symbol)
    }

    fn get_lazy_type_params(&self, def_id: DefId) -> Option<Vec<TypeParamInfo>> {
        (**self).get_lazy_type_params(def_id)
    }

    fn def_to_symbol_id(&self, def_id: DefId) -> Option<SymbolId> {
        (**self).def_to_symbol_id(def_id)
    }

    fn symbol_to_def_id(&self, symbol: SymbolRef) -> Option<DefId> {
        (**self).symbol_to_def_id(symbol)
    }

    fn get_def_kind(&self, def_id: DefId) -> Option<crate::def::DefKind> {
        (**self).get_def_kind(def_id)
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

    fn is_numeric_enum(&self, def_id: DefId) -> bool {
        (**self).is_numeric_enum(def_id)
    }

    fn is_enum_type(&self, type_id: TypeId, interner: &dyn TypeDatabase) -> bool {
        (**self).is_enum_type(type_id, interner)
    }

    fn get_enum_parent_def_id(&self, member_def_id: DefId) -> Option<DefId> {
        (**self).get_enum_parent_def_id(member_def_id)
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

    fn get_base_type(&self, type_id: TypeId, interner: &dyn TypeDatabase) -> Option<TypeId> {
        (**self).get_base_type(type_id, interner)
    }

    fn get_type_param_variance(
        &self,
        def_id: DefId,
    ) -> Option<std::sync::Arc<[crate::types::Variance]>> {
        (**self).get_type_param_variance(def_id)
    }
}

// =============================================================================
// TypeEnvironment
// =============================================================================

/// A type environment that maps symbol refs to their resolved types.
/// This is populated before type checking and passed to the `SubtypeChecker`.
#[derive(Clone, Debug, Default)]
pub struct TypeEnvironment {
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
    /// Maps `DefIds` to their resolved structural types.
    def_types: FxHashMap<u32, TypeId>,
    /// Maps `DefIds` to their type parameters (for generic types with Lazy refs).
    def_type_params: FxHashMap<u32, Vec<TypeParamInfo>>,
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
}

impl TypeEnvironment {
    pub fn new() -> Self {
        Self {
            types: FxHashMap::default(),
            type_params: FxHashMap::default(),
            boxed_types: FxHashMap::default(),
            array_base_type: None,
            array_base_type_params: Vec::new(),
            def_types: FxHashMap::default(),
            def_type_params: FxHashMap::default(),
            def_to_symbol: FxHashMap::default(),
            symbol_to_def: FxHashMap::default(),
            numeric_enums: FxHashSet::default(),
            enum_namespace_types: FxHashMap::default(),
            def_kinds: FxHashMap::default(),
            enum_parents: FxHashMap::default(),
            class_instance_types: FxHashMap::default(),
            boxed_def_ids: FxHashMap::default(),
            class_extends: FxHashMap::default(),
            instance_type_to_class: FxHashMap::default(),
            definition_store: None,
            this_type: None,
        }
    }

    /// Set the concrete type that `ThisType` should resolve to.
    ///
    /// Called by the checker when performing relation checks inside a class
    /// scope so the solver can resolve `this` type references during
    /// subtype/identity comparisons.
    pub const fn set_this_type(&mut self, this_type: Option<TypeId>) {
        self.this_type = this_type;
    }

    /// Set the shared `DefinitionStore` for fallback `DefKind` lookups.
    pub fn set_definition_store(&mut self, store: Arc<DefinitionStore>) {
        self.definition_store = Some(store);
    }

    /// Register a symbol's resolved type.
    pub fn insert(&mut self, symbol: SymbolRef, type_id: TypeId) {
        self.types.insert(symbol.0, type_id);
    }

    /// Register a boxed type for a primitive (Rule #33).
    pub fn set_boxed_type(&mut self, kind: IntrinsicKind, type_id: TypeId) {
        self.boxed_types.insert(kind, type_id);
    }

    /// Get the boxed type for a primitive.
    pub fn get_boxed_type(&self, kind: IntrinsicKind) -> Option<TypeId> {
        self.boxed_types.get(&kind).copied()
    }

    /// Register a `DefId` as belonging to a boxed type.
    pub fn register_boxed_def_id(&mut self, kind: IntrinsicKind, def_id: DefId) {
        self.boxed_def_ids.entry(kind).or_default().push(def_id);
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
    }

    /// Get the Array<T> interface type.
    pub const fn get_array_base_type(&self) -> Option<TypeId> {
        self.array_base_type
    }

    /// Get the type parameters for the Array<T> interface.
    pub fn get_array_base_type_params(&self) -> &[TypeParamInfo] {
        &self.array_base_type_params
    }

    /// Register a symbol's resolved type with type parameters.
    pub fn insert_with_params(
        &mut self,
        symbol: SymbolRef,
        type_id: TypeId,
        params: Vec<TypeParamInfo>,
    ) {
        self.types.insert(symbol.0, type_id);
        if !params.is_empty() {
            self.type_params.insert(symbol.0, params);
        }
    }

    /// Get a symbol's resolved type.
    pub fn get(&self, symbol: SymbolRef) -> Option<TypeId> {
        self.types.get(&symbol.0).copied()
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
        self.def_types.insert(def_id.0, type_id);
        // Write through to shared store for cross-checker visibility.
        if let Some(ref store) = self.definition_store {
            store.set_body(def_id, type_id);
        }
    }

    /// Get a class `DefId`'s registered instance type.
    pub fn get_class_instance_type(&self, def_id: DefId) -> Option<TypeId> {
        self.class_instance_types.get(&def_id.0).copied()
    }

    /// Register a class `DefId`'s instance type.
    pub fn insert_class_instance_type(&mut self, def_id: DefId, instance_type: TypeId) {
        self.class_instance_types.insert(def_id.0, instance_type);
        // Reverse map: allow looking up which class a resolved instance type came from.
        // This is critical for instanceof narrowing to identify class types after
        // they've been resolved from Lazy(DefId) to Object types.
        self.instance_type_to_class.insert(instance_type.0, def_id);
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
        self.def_types.insert(def_id.0, type_id);
        if !params.is_empty() {
            self.def_type_params.insert(def_id.0, params);
        }
        // Write through to shared store for cross-checker visibility.
        if let Some(ref store) = self.definition_store {
            store.set_body(def_id, type_id);
        }
    }

    /// Get a `DefId`'s resolved type.
    ///
    /// First checks the local `def_types` cache, then falls back to the shared
    /// `DefinitionStore.get_body()` if available. The fallback enables cross-file
    /// delegation results to be visible without explicit merge-back: the child
    /// checker writes to `DefinitionStore` and the parent reads via this fallback.
    pub fn get_def(&self, def_id: DefId) -> Option<TypeId> {
        self.def_types
            .get(&def_id.0)
            .copied()
            .or_else(|| self.definition_store.as_ref()?.get_body(def_id))
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
        for (&key, &type_id) in &self.def_types {
            target.def_types.entry(key).or_insert(type_id);
        }
        for (key, params) in &self.def_type_params {
            target
                .def_type_params
                .entry(*key)
                .or_insert_with(|| params.clone());
        }
    }

    // =========================================================================
    // DefKind Storage (Task #32: Graph Isomorphism)
    // =========================================================================

    /// Register a `DefId`'s `DefKind`.
    pub fn insert_def_kind(&mut self, def_id: DefId, kind: crate::def::DefKind) {
        self.def_kinds.insert(def_id.0, kind);
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
    }

    /// Register a `DefId` as a numeric enum.
    pub fn register_numeric_enum(&mut self, def_id: DefId) {
        self.numeric_enums.insert(def_id.0);
    }

    /// Check if a `DefId` is a numeric enum.
    pub fn is_numeric_enum(&self, def_id: DefId) -> bool {
        self.numeric_enums.contains(&def_id.0)
    }

    /// Register an enum's namespace object type (for `typeof Enum`).
    pub fn register_enum_namespace_type(&mut self, def_id: DefId, ns_type: TypeId) {
        self.enum_namespace_types.insert(def_id.0, ns_type);
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
        self.enum_parents.insert(member_def_id.0, parent_def_id);
    }

    /// Get the parent enum `DefId` for an enum member `DefId`.
    pub fn get_enum_parent(&self, member_def_id: DefId) -> Option<DefId> {
        self.enum_parents.get(&member_def_id.0).copied()
    }

    // =========================================================================
    // Class Extends Relationships
    // =========================================================================

    /// Register a class's parent class `DefId`.
    pub fn register_class_extends(&mut self, child_def_id: DefId, parent_def_id: DefId) {
        self.class_extends.insert(child_def_id.0, parent_def_id);
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
    fn resolve_ref(&self, symbol: SymbolRef, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        self.get(symbol)
    }

    fn resolve_type_query(
        &self,
        symbol: SymbolRef,
        _interner: &dyn TypeDatabase,
    ) -> Option<TypeId> {
        // For TypeQuery (typeof X), we need the VALUE-space type:
        // - For classes: the constructor type (stored under DefId in the types map)
        // - For other symbols: same as resolve_ref
        //
        // The SymbolRef entry may contain the instance type (inserted by
        // type_reference_symbol_type via insert_type_env_symbol), but the DefId
        // entry always has the constructor type (inserted by get_type_of_symbol).
        if let Some(&def_id) = self.symbol_to_def.get(&symbol.0)
            && let Some(ty) = self.get_def(DefId(def_id.0))
        {
            return Some(ty);
        }
        // Fallback to SymbolRef lookup for non-class symbols
        self.get(symbol)
    }

    fn resolve_lazy(&self, def_id: DefId, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        // For classes, return the instance type (type position) instead of the constructor type
        if let Some(&instance_type) = self.class_instance_types.get(&def_id.0) {
            return Some(instance_type);
        }
        self.get_def(def_id).or_else(|| {
            // Fallback: `interner.reference(SymbolRef(N))` creates `Lazy(DefId(N))`
            // where N is the raw SymbolId. Look up the real DefId via symbol_to_def.
            let real_def = self.symbol_to_def.get(&def_id.0)?;
            if let Some(&instance_type) = self.class_instance_types.get(&real_def.0) {
                return Some(instance_type);
            }
            self.get_def(*real_def)
        })
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
            // Fallback: resolve raw SymbolId-based DefIds to real DefIds
            let real_def = self.symbol_to_def.get(&def_id.0)?;
            self.get_def_params_owned(*real_def)
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

    fn def_to_symbol_id(&self, def_id: DefId) -> Option<SymbolId> {
        self.def_to_symbol.get(&def_id.0).copied()
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

    fn is_numeric_enum(&self, def_id: DefId) -> bool {
        Self::is_numeric_enum(self, def_id)
    }

    fn get_enum_parent_def_id(&self, member_def_id: DefId) -> Option<DefId> {
        Self::get_enum_parent(self, member_def_id)
    }

    fn is_enum_type(&self, type_id: TypeId, interner: &dyn TypeDatabase) -> bool {
        use crate::visitors::visitor_extract::enum_components;
        if let Some((def_id, _)) = enum_components(interner, type_id) {
            // A full enum type's DefId is NOT registered as a member (key) in enum_parents.
            // Member DefIds ARE keys in enum_parents (mapping to their parent DefId).
            // So if the DefId is NOT a member, it's the parent enum type.
            !self.enum_parents.contains_key(&def_id.0)
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
        self.class_def_for_instance(type_id)
    }
}
