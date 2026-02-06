//! Structural subtype checking.
//!
//! This module implements the core logic engine for TypeScript's structural
//! subtyping. It uses coinductive semantics to handle recursive types.
//!
//! Key features:
//! - O(1) equality check via TypeId comparison
//! - Cycle detection for recursive types (coinductive)
//! - Set-theoretic operations for unions and intersections
//! - TypeResolver trait for lazy symbol resolution
//! - Tracer pattern for zero-cost diagnostic abstraction

use crate::binder::SymbolId;
use crate::limits;
use crate::solver::AssignabilityChecker;
use crate::solver::TypeDatabase;
use crate::solver::db::QueryDatabase;
use crate::solver::def::DefId;
use crate::solver::diagnostics::{DynSubtypeTracer, SubtypeFailureReason};
use crate::solver::types::*;
use crate::solver::utils;
use crate::solver::visitor::{
    TypeVisitor, application_id, array_element_type, callable_shape_id, conditional_type_id,
    enum_components, function_shape_id, intersection_list_id, intrinsic_kind, is_this_type,
    keyof_inner_type, lazy_def_id, literal_value, mapped_type_id, object_shape_id,
    object_with_index_shape_id, readonly_inner_type, ref_symbol, template_literal_id,
    tuple_list_id, type_param_info, type_query_symbol, union_list_id, unique_symbol_ref,
};
use rustc_hash::{FxHashMap, FxHashSet};

#[cfg(test)]
use crate::solver::TypeInterner;

/// Maximum recursion depth for subtype checking.
/// This prevents OOM/stack overflow from infinitely expanding recursive types.
/// Examples: `interface AA<T extends AA<T>>`, `interface List<T> { next: List<T> }`
pub(crate) const MAX_SUBTYPE_DEPTH: u32 = limits::MAX_SUBTYPE_DEPTH;

/// Result of a subtype check
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SubtypeResult {
    /// The relationship is definitely true
    True,
    /// The relationship is definitely false
    False,
    /// We're in a valid cycle (coinductive recursion)
    ///
    /// This represents finite/cyclic recursion like `interface List { next: List }`.
    /// The type graph forms a closed loop, which is valid in TypeScript.
    CycleDetected,
    /// We've exceeded the recursion depth limit
    ///
    /// This represents expansive recursion that grows indefinitely like
    /// `type T<X> = T<Box<X>>`. TypeScript rejects these as "excessively deep".
    ///
    /// This is treated as `false` for soundness - if we can't prove subtyping within
    /// reasonable limits, we reject the relationship rather than accepting unsoundly.
    DepthExceeded,
}

impl SubtypeResult {
    pub fn is_true(self) -> bool {
        matches!(self, SubtypeResult::True | SubtypeResult::CycleDetected)
    }

    pub fn is_false(self) -> bool {
        matches!(self, SubtypeResult::False)
    }
}

/// Returns true for unit types where `source != target` implies disjointness.
///
/// This intentionally excludes:
/// - null/undefined/void/never (special-cased assignability semantics)
/// - Tuples (labeled tuples like [a: 1] vs [b: 1] are compatible despite different TypeIds)
///
/// Only safe for primitives where identity implies structural equality.
fn is_disjoint_unit_type(types: &dyn TypeDatabase, ty: TypeId) -> bool {
    match types.lookup(ty) {
        Some(TypeKey::Literal(_)) | Some(TypeKey::UniqueSymbol(_)) => true,
        // Note: Tuples removed to avoid labeled tuple bug
        // TypeScript treats [a: 1] and [b: 1] as compatible even though they have different TypeIds
        _ => false,
    }
}

/// Controls how `any` is treated during subtype checks.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AnyPropagationMode {
    /// `any` is treated as top/bottom everywhere (TypeScript default).
    All,
    /// `any` is treated as top/bottom only at the top-level comparison.
    TopLevelOnly,
}

impl AnyPropagationMode {
    #[inline]
    fn allows_any_at_depth(self, depth: u32) -> bool {
        match self {
            AnyPropagationMode::All => true,
            AnyPropagationMode::TopLevelOnly => depth == 0,
        }
    }
}

/// Trait for resolving type references to their structural types.
/// This allows the SubtypeChecker to lazily resolve Ref types
/// without being tightly coupled to the binder/checker.
pub trait TypeResolver {
    /// Resolve a symbol reference to its structural type.
    /// Returns None if the symbol cannot be resolved.
    ///
    /// **Phase 3.4**: Deprecated - use `resolve_lazy` with DefId instead.
    /// This method is being phased out as part of the migration to DefId-based type identity.
    #[deprecated(
        note = "Use resolve_lazy with DefId instead. This method is being phased out as part of Issue #12."
    )]
    fn resolve_ref(&self, symbol: SymbolRef, interner: &dyn TypeDatabase) -> Option<TypeId>;

    /// Resolve a DefId reference to its structural type.
    ///
    /// This is the DefId equivalent of `resolve_ref`, used for `TypeKey::Lazy(DefId)`.
    /// DefIds are Solver-owned identifiers that decouple type references from the Binder.
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

    /// Get type parameters for a DefId (for generic type aliases/interfaces).
    ///
    /// This is the DefId equivalent of `get_type_params`.
    /// Returns None by default; implementations can override to support
    /// Application type expansion with Lazy types.
    fn get_lazy_type_params(&self, _def_id: DefId) -> Option<Vec<TypeParamInfo>> {
        None
    }

    /// Get the SymbolId for a DefId (Phase 3.2: bridge for InheritanceGraph).
    ///
    /// This enables DefId-based types to use the existing O(1) InheritanceGraph
    /// by mapping DefIds back to their corresponding SymbolIds. The mapping is
    /// maintained by the Binder/Checker during type resolution.
    ///
    /// Returns None if the DefId doesn't have a corresponding SymbolId.
    fn def_to_symbol_id(&self, _def_id: DefId) -> Option<SymbolId> {
        None
    }

    /// Get the DefId for a SymbolRef (Phase 3.4: Ref -> Lazy migration).
    ///
    /// This enables migrating Ref(SymbolRef) types to Lazy(DefId) resolution logic.
    /// When a SymbolRef has a corresponding DefId, we should use resolve_lazy instead
    /// of resolve_ref for consistent type identity.
    ///
    /// Returns None if the SymbolRef doesn't have a corresponding DefId.
    fn symbol_to_def_id(&self, _symbol: SymbolRef) -> Option<DefId> {
        None
    }

    /// Get the DefKind for a DefId (Task #32: Graph Isomorphism).
    ///
    /// This is used by the Canonicalizer to distinguish between structural types
    /// (TypeAlias - should be canonicalized with Recursive indices) and nominal
    /// types (Interface/Class/Enum - must remain as Lazy(DefId) for nominal identity).
    ///
    /// ## Structural vs Nominal Types
    ///
    /// - **TypeAlias**: Structural - `type A = { x: A }` and `type B = { x: B }`
    ///   should canonicalize to the same type with Recursive(0)
    /// - **Interface/Class**: Nominal - Different interfaces are incompatible even
    ///   if structurally identical, so they must keep their Lazy(DefId) reference
    ///
    /// Returns None if the DefId doesn't exist or the implementation doesn't
    /// support DefKind lookup. Implementations should override this to support
    /// canonicalization of recursive types.
    fn get_def_kind(&self, _def_id: DefId) -> Option<crate::solver::def::DefKind> {
        None
    }

    /// Get the boxed interface type for a primitive intrinsic (Rule #33).
    /// For example, IntrinsicKind::Number -> TypeId of the Number interface.
    /// This enables primitives to be subtypes of their boxed interfaces.
    fn get_boxed_type(&self, _kind: IntrinsicKind) -> Option<TypeId> {
        None
    }

    /// Get the Array<T> interface type from lib.d.ts.
    /// This is used to resolve array methods via the official interface
    /// instead of hardcoding. Returns the generic Array interface type.
    fn get_array_base_type(&self) -> Option<TypeId> {
        None
    }

    /// Get the type parameters for the Array<T> interface.
    /// Used together with get_array_base_type to instantiate Array<T> with a concrete element type.
    fn get_array_base_type_params(&self) -> &[TypeParamInfo] {
        &[]
    }

    /// Get an export from a namespace/module by name.
    ///
    /// Used for qualified name resolution: `namespace.member`.
    /// Returns None by default; implementations should override to support
    /// namespace member access with Lazy types.
    fn get_lazy_export(&self, _def_id: DefId, _name: crate::interner::Atom) -> Option<TypeId> {
        None
    }

    /// Get enum member type by name from an enum DefId.
    ///
    /// Used for enum member access: `Enum.Member`.
    /// Returns None by default; implementations should override to support
    /// enum member access with Lazy types.
    fn get_lazy_enum_member(&self, _def_id: DefId, _name: crate::interner::Atom) -> Option<TypeId> {
        None
    }

    /// Check if a DefId corresponds to a numeric enum (not a string enum).
    ///
    /// Used for TypeScript's unsound Rule #7 (Open Numeric Enums) where
    /// number types are assignable to/from numeric enums.
    fn is_numeric_enum(&self, _def_id: DefId) -> bool {
        false
    }

    /// Check if a TypeId represents a full Enum type (not a specific member).
    ///
    /// Used to distinguish between `enum E` (type) and `enum E.A` (member) for
    /// assignability rules. Specifically, `number` is assignable to numeric enum
    /// types but NOT to enum members.
    ///
    /// Returns true if the TypeId is:
    /// - A TypeKey::Enum where the Symbol has ENUM flag but not ENUM_MEMBER flag
    /// - A Union of TypeKey::Enum members from the same parent enum
    ///
    /// Returns false for enum members or non-enum types.
    fn is_enum_type(&self, _type_id: TypeId, _interner: &dyn TypeDatabase) -> bool {
        false
    }

    /// Get the parent Enum's DefId for an Enum Member's DefId.
    ///
    /// Used to check nominal relationships between enum members and their parent types.
    /// For example, to determine if `E.A` (member) can be assigned to `E` (parent type).
    ///
    /// Returns Some(parent_def_id) if the DefId is an enum member.
    /// Returns None if the DefId is not an enum member (e.g., it's the enum type itself).
    fn get_enum_parent_def_id(&self, _member_def_id: DefId) -> Option<DefId> {
        None
    }

    /// Check if a DefId represents a user-defined enum (not an intrinsic type).
    ///
    /// This is used to distinguish between user-defined enums (like `enum E { A, B }`)
    /// and intrinsic types from lib.d.ts (like `type string = ...`) that are stored
    /// as TypeKey::Enum for definition store purposes.
    ///
    /// Returns true if the DefId is a user-defined enum.
    /// Returns false for intrinsic types, type aliases, interfaces, etc.
    fn is_user_enum_def(&self, _def_id: DefId) -> bool {
        false
    }

    /// Get the base class type for a class/interface type.
    ///
    /// This is used by the Best Common Type (BCT) algorithm to find common base classes.
    /// For example, given Dog and Cat that both extend Animal, this returns Animal.
    ///
    /// Returns None if the type doesn't have a base class (e.g., interfaces don't extend classes).
    ///
    /// **Architecture**: This bridges the Solver (which computes BCT) to the Binder (which stores extends clauses).
    fn get_base_type(&self, _type_id: TypeId, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        None
    }

    /// Get the variance mask for type parameters of a generic type (Task #41).
    ///
    /// This is used by `check_application_to_application_subtype` to optimize generic
    /// assignability checks. Instead of expanding the entire type structure, we use
    /// variance annotations to check type arguments in O(1) time.
    ///
    /// # Parameters
    ///
    /// * `def_id` - The DefId of the generic type (e.g., Array, Promise, Map)
    ///
    /// # Returns
    ///
    /// - `Some(variances)` - A slice of Variance bitflags, one per type parameter
    /// - `None` - Variance unavailable (fall back to structural expansion)
    ///
    /// # Example
    ///
    /// For `type ReadonlyArray<T> = { readonly [index: number]: T }`:
    /// - Returns `Some([Variance::COVARIANT])` because T is only used in read position
    ///
    /// For `type Box<T> = { get(): T; set(x: T): void }`:
    /// - Returns `Some([Variance::INVARIANT])` because T is used in both read and write
    ///
    /// Returns None by default; implementations should override to support variance queries.
    fn get_type_param_variance(
        &self,
        _def_id: DefId,
    ) -> Option<std::sync::Arc<[crate::solver::types::Variance]>> {
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

    fn symbol_to_def_id(&self, _symbol: SymbolRef) -> Option<DefId> {
        None
    }
}

/// Blanket implementation of TypeResolver for references to resolver types.
///
/// This allows `&dyn TypeResolver` (which is Sized) to be used wherever
/// `R: TypeResolver` is expected. This is critical for passing resolvers
/// through contexts like `NarrowingContext` that store `Option<&dyn TypeResolver>`.
///
/// # Example
/// ```rust
/// let env: TypeEnvironment = ...;
/// let resolver: &dyn TypeResolver = &env;
/// // resolver can now be passed to functions expecting R: TypeResolver
/// ```
impl<T: TypeResolver + ?Sized> TypeResolver for &T {
    fn resolve_ref(&self, _symbol: SymbolRef, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        // This method is deprecated - use resolve_lazy instead
        None
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

    fn symbol_to_def_id(&self, symbol: SymbolRef) -> Option<DefId> {
        (**self).symbol_to_def_id(symbol)
    }
}

/// A type environment that maps symbol refs to their resolved types.
/// This is populated before type checking and passed to the SubtypeChecker.
#[derive(Clone, Debug, Default)]
pub struct TypeEnvironment {
    /// Maps symbol references to their resolved structural types.
    types: std::collections::HashMap<u32, TypeId>,
    /// Maps symbol references to their type parameters (for generic types).
    type_params: std::collections::HashMap<u32, Vec<TypeParamInfo>>,
    /// Maps primitive intrinsic kinds to their boxed interface types (Rule #33).
    /// e.g., IntrinsicKind::Number -> TypeId of the Number interface
    boxed_types: std::collections::HashMap<IntrinsicKind, TypeId>,
    /// The Array<T> interface type from lib.d.ts.
    /// Used to resolve array methods via the official interface.
    array_base_type: Option<TypeId>,
    /// Type parameters for the Array<T> interface (usually just [T]).
    array_base_type_params: Vec<TypeParamInfo>,
    /// Maps DefIds to their resolved structural types (Phase 4.3 migration).
    /// This enables `TypeKey::Lazy(DefId)` resolution.
    def_types: std::collections::HashMap<u32, TypeId>,
    /// Maps DefIds to their type parameters (for generic types with Lazy refs).
    def_type_params: std::collections::HashMap<u32, Vec<TypeParamInfo>>,
    /// Maps DefIds back to SymbolIds for InheritanceGraph lookups (Phase 3.2).
    /// This bridge enables Lazy(DefId) types to use the O(1) InheritanceGraph
    /// by mapping DefIds back to their corresponding SymbolIds.
    def_to_symbol: std::collections::HashMap<u32, SymbolId>,
    /// Maps SymbolIds to DefIds for Ref -> Lazy migration (Phase 3.4).
    /// This reverse mapping enables migrating Ref(SymbolRef) types to use
    /// DefId-based resolution via resolve_lazy instead of resolve_ref.
    symbol_to_def: std::collections::HashMap<u32, DefId>,
    /// Set of DefIds that correspond to numeric enums.
    /// Used for Rule #7 (Open Numeric Enums) where number types are assignable to/from numeric enums.
    numeric_enums: std::collections::HashSet<u32>,
    /// Maps DefIds to their DefKind (Task #32: Graph Isomorphism).
    /// Used by the Canonicalizer to distinguish structural types (TypeAlias)
    /// from nominal types (Interface/Class/Enum).
    def_kinds: std::collections::HashMap<u32, crate::solver::def::DefKind>,
    /// Maps enum member DefIds to their parent enum DefId.
    /// Used for member-to-parent assignability (e.g., E.A -> E).
    enum_parents: std::collections::HashMap<u32, DefId>,
}

impl TypeEnvironment {
    pub fn new() -> Self {
        TypeEnvironment {
            types: std::collections::HashMap::new(),
            type_params: std::collections::HashMap::new(),
            boxed_types: std::collections::HashMap::new(),
            array_base_type: None,
            array_base_type_params: Vec::new(),
            def_types: std::collections::HashMap::new(),
            def_type_params: std::collections::HashMap::new(),
            def_to_symbol: std::collections::HashMap::new(),
            symbol_to_def: std::collections::HashMap::new(),
            numeric_enums: std::collections::HashSet::new(),
            def_kinds: std::collections::HashMap::new(),
            enum_parents: std::collections::HashMap::new(),
        }
    }

    /// Register a symbol's resolved type.
    pub fn insert(&mut self, symbol: SymbolRef, type_id: TypeId) {
        self.types.insert(symbol.0, type_id);
    }

    /// Register a boxed type for a primitive (Rule #33).
    /// e.g., set_boxed_type(IntrinsicKind::Number, type_id_of_Number_interface)
    pub fn set_boxed_type(&mut self, kind: IntrinsicKind, type_id: TypeId) {
        self.boxed_types.insert(kind, type_id);
    }

    /// Get the boxed type for a primitive.
    pub fn get_boxed_type(&self, kind: IntrinsicKind) -> Option<TypeId> {
        self.boxed_types.get(&kind).copied()
    }

    /// Register the Array<T> interface type from lib.d.ts.
    /// This enables array property access to use lib.d.ts definitions.
    /// `type_params` should contain the type parameters of the Array interface (usually just [T]).
    pub fn set_array_base_type(&mut self, type_id: TypeId, type_params: Vec<TypeParamInfo>) {
        self.array_base_type = Some(type_id);
        self.array_base_type_params = type_params;
    }

    /// Get the Array<T> interface type.
    pub fn get_array_base_type(&self) -> Option<TypeId> {
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
    pub fn get_params(&self, symbol: SymbolRef) -> Option<&Vec<TypeParamInfo>> {
        self.type_params.get(&symbol.0)
    }

    /// Check if the environment contains a symbol.
    pub fn contains(&self, symbol: SymbolRef) -> bool {
        self.types.contains_key(&symbol.0)
    }

    /// Number of resolved types.
    pub fn len(&self) -> usize {
        self.types.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.types.is_empty()
    }

    // =========================================================================
    // DefId Resolution (Phase 4.3 migration)
    // =========================================================================

    /// Register a DefId's resolved type.
    pub fn insert_def(&mut self, def_id: DefId, type_id: TypeId) {
        self.def_types.insert(def_id.0, type_id);
    }

    /// Register a DefId's resolved type with type parameters.
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
    }

    /// Get a DefId's resolved type.
    pub fn get_def(&self, def_id: DefId) -> Option<TypeId> {
        self.def_types.get(&def_id.0).copied()
    }

    /// Get a DefId's type parameters.
    pub fn get_def_params(&self, def_id: DefId) -> Option<&Vec<TypeParamInfo>> {
        self.def_type_params.get(&def_id.0)
    }

    /// Check if the environment contains a DefId.
    pub fn contains_def(&self, def_id: DefId) -> bool {
        self.def_types.contains_key(&def_id.0)
    }

    // =========================================================================
    // DefKind Storage (Task #32: Graph Isomorphism)
    // =========================================================================

    /// Register a DefId's DefKind.
    ///
    /// Used by the Canonicalizer to distinguish structural types (TypeAlias)
    /// from nominal types (Interface/Class/Enum).
    pub fn insert_def_kind(&mut self, def_id: DefId, kind: crate::solver::def::DefKind) {
        self.def_kinds.insert(def_id.0, kind);
    }

    /// Get a DefId's DefKind.
    pub fn get_def_kind(&self, def_id: DefId) -> Option<crate::solver::def::DefKind> {
        self.def_kinds.get(&def_id.0).copied()
    }

    // =========================================================================
    // DefId <-> SymbolId Bridge (Phase 3.2, 3.4)
    // =========================================================================

    /// Register a mapping from DefId to SymbolId for InheritanceGraph lookups.
    ///
    /// This bridge enables Lazy(DefId) types to use the O(1) InheritanceGraph
    /// by mapping DefIds back to their corresponding SymbolIds. The mapping
    /// is maintained by the Binder/Checker during type resolution.
    ///
    /// Phase 3.4: Also registers the reverse mapping (SymbolId -> DefId) to support
    /// migrating Ref types to DefId resolution.
    pub fn register_def_symbol_mapping(&mut self, def_id: DefId, sym_id: SymbolId) {
        self.def_to_symbol.insert(def_id.0, sym_id);
        self.symbol_to_def.insert(sym_id.0, def_id); // Populate reverse map
    }

    /// Register a DefId as a numeric enum.
    /// Used for Rule #7 (Open Numeric Enums) where number types are assignable to/from numeric enums.
    pub fn register_numeric_enum(&mut self, def_id: DefId) {
        self.numeric_enums.insert(def_id.0);
    }

    /// Check if a DefId is a numeric enum.
    pub fn is_numeric_enum(&self, def_id: DefId) -> bool {
        self.numeric_enums.contains(&def_id.0)
    }

    // =========================================================================
    // Enum Parent Relationships (Task #17: Enum Type Resolution)
    // =========================================================================

    /// Register an enum member's parent enum DefId.
    ///
    /// Used for member-to-parent assignability (e.g., E.A -> E).
    pub fn register_enum_parent(&mut self, member_def_id: DefId, parent_def_id: DefId) {
        self.enum_parents.insert(member_def_id.0, parent_def_id);
    }

    /// Get the parent enum DefId for an enum member DefId.
    ///
    /// Returns Some(parent_def_id) if the DefId is an enum member.
    /// Returns None if the DefId is not an enum member (e.g., it's the enum type itself).
    pub fn get_enum_parent(&self, member_def_id: DefId) -> Option<DefId> {
        self.enum_parents.get(&member_def_id.0).copied()
    }
}

impl TypeResolver for TypeEnvironment {
    fn resolve_ref(&self, symbol: SymbolRef, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        self.get(symbol)
    }

    fn resolve_lazy(&self, def_id: DefId, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        self.get_def(def_id)
    }

    fn get_type_params(&self, symbol: SymbolRef) -> Option<Vec<TypeParamInfo>> {
        self.get_params(symbol).cloned()
    }

    fn get_lazy_type_params(&self, def_id: DefId) -> Option<Vec<TypeParamInfo>> {
        self.get_def_params(def_id).cloned()
    }

    fn get_boxed_type(&self, kind: IntrinsicKind) -> Option<TypeId> {
        TypeEnvironment::get_boxed_type(self, kind)
    }

    fn get_array_base_type(&self) -> Option<TypeId> {
        TypeEnvironment::get_array_base_type(self)
    }

    fn get_array_base_type_params(&self) -> &[TypeParamInfo] {
        TypeEnvironment::get_array_base_type_params(self)
    }

    fn def_to_symbol_id(&self, def_id: DefId) -> Option<SymbolId> {
        self.def_to_symbol.get(&def_id.0).copied()
    }

    fn symbol_to_def_id(&self, symbol: SymbolRef) -> Option<DefId> {
        self.symbol_to_def.get(&symbol.0).copied()
    }

    fn get_def_kind(&self, def_id: DefId) -> Option<crate::solver::def::DefKind> {
        TypeEnvironment::get_def_kind(self, def_id)
    }

    fn is_numeric_enum(&self, def_id: DefId) -> bool {
        TypeEnvironment::is_numeric_enum(self, def_id)
    }

    fn get_enum_parent_def_id(&self, member_def_id: DefId) -> Option<DefId> {
        TypeEnvironment::get_enum_parent(self, member_def_id)
    }

    fn is_user_enum_def(&self, _def_id: DefId) -> bool {
        // TypeEnvironment doesn't have access to binder symbol information
        // Default to false (conservative approach)
        false
    }
}

/// Maximum number of unique type pairs to track in cycle detection.
/// Prevents unbounded memory growth in pathological cases.
pub const MAX_IN_PROGRESS_PAIRS: usize = limits::MAX_IN_PROGRESS_PAIRS as usize;

// =============================================================================
// Task #48: SubtypeVisitor - Visitor Pattern for Subtype Checking
// =============================================================================

/// Visitor for structural subtype checking.
///
/// This visitor implements the North Star Rule 2 (Visitor Pattern for type operations).
/// It wraps a mutable reference to SubtypeChecker and the target type, dispatching
/// to the appropriate checker methods based on the source type's structure.
///
/// ## Design
///
/// - **Binary Relation**: Subtyping is binary (A <: B), but visitor is unary (visits A).
///   The target type B is stored as a field.
/// - **Double Dispatch**: Many visitor methods must inspect both source and target kinds
///   to determine which checker method to call (e.g., tuple-to-tuple vs tuple-to-array).
/// - **Coinduction**: All recursive checks MUST go through `self.checker.check_subtype()`
///   to ensure cycle detection works correctly.
/// - **Pre-checks**: Special cases (apparent shapes, target-is-union) remain in
///   `check_subtype_inner` before dispatching to the visitor.
pub struct SubtypeVisitor<'a, 'b, R: TypeResolver> {
    /// Reference to the parent checker (for recursive checks and state).
    pub checker: &'a mut SubtypeChecker<'b, R>,
    /// The source type being visited (the "A" in "A <: B").
    /// Stored because some delegation methods need the full TypeId, not just unpacked data.
    pub source: TypeId,
    /// The target type we're checking against (the "B" in "A <: B").
    pub target: TypeId,
}

impl<'a, 'b, R: TypeResolver> TypeVisitor for SubtypeVisitor<'a, 'b, R> {
    type Output = SubtypeResult;

    // Default: return False for unimplemented variants
    fn default_output() -> Self::Output {
        SubtypeResult::False
    }

    // Core intrinsics - delegate to checker
    fn visit_intrinsic(&mut self, kind: IntrinsicKind) -> Self::Output {
        if let Some(t_kind) = intrinsic_kind(self.checker.interner, self.target) {
            return self.checker.check_intrinsic_subtype(kind, t_kind);
        }
        if self.checker.is_boxed_primitive_subtype(kind, self.target) {
            SubtypeResult::True
        } else {
            SubtypeResult::False
        }
    }

    fn visit_literal(&mut self, value: &LiteralValue) -> Self::Output {
        if let Some(t_kind) = intrinsic_kind(self.checker.interner, self.target) {
            return self.checker.check_literal_to_intrinsic(value, t_kind);
        }
        if let Some(t_lit) = literal_value(self.checker.interner, self.target) {
            return if value == &t_lit {
                SubtypeResult::True
            } else {
                SubtypeResult::False
            };
        }
        SubtypeResult::False
    }

    fn visit_array(&mut self, element_type: TypeId) -> Self::Output {
        if let Some(t_elem) = array_element_type(self.checker.interner, self.target) {
            self.checker.check_subtype(element_type, t_elem)
        } else {
            SubtypeResult::False
        }
    }

    fn visit_tuple(&mut self, list_id: u32) -> Self::Output {
        // Double dispatch: check target type to determine which helper to call
        // Tuple <: Tuple, Tuple <: Array, Array <: Tuple
        let s_tuple_id = TupleListId(list_id);

        if let Some(t_list) = tuple_list_id(self.checker.interner, self.target) {
            // Tuple <: Tuple
            let s_elems = self.checker.interner.tuple_list(s_tuple_id);
            let t_elems = self.checker.interner.tuple_list(t_list);
            self.checker.check_tuple_subtype(&s_elems, &t_elems)
        } else if let Some(t_elem) = array_element_type(self.checker.interner, self.target) {
            // Tuple <: Array
            self.checker
                .check_tuple_to_array_subtype(s_tuple_id, t_elem)
        } else {
            SubtypeResult::False
        }
    }

    fn visit_union(&mut self, list_id: u32) -> Self::Output {
        // Union <: Target requires ALL members to be subtypes
        let member_list = self.checker.interner.type_list(TypeListId(list_id));
        for &member in member_list.iter() {
            if !self.checker.check_subtype(member, self.target).is_true() {
                return SubtypeResult::False;
            }
        }
        SubtypeResult::True
    }

    fn visit_intersection(&mut self, list_id: u32) -> Self::Output {
        // Intersection <: Target requires AT LEAST ONE member to be subtype
        let member_list = self.checker.interner.type_list(TypeListId(list_id));
        for &member in member_list.iter() {
            if self.checker.check_subtype(member, self.target).is_true() {
                return SubtypeResult::True;
            }
        }

        // Special case: If target is an object type, check if MERGED properties satisfy it
        // This handles cases like: { a: string } & { b: number } <: { a: string; b: number }
        if object_shape_id(self.checker.interner, self.target).is_some()
            || object_with_index_shape_id(self.checker.interner, self.target).is_some()
        {
            use crate::solver::objects::{PropertyCollectionResult, collect_properties};

            match collect_properties(self.source, self.checker.interner, self.checker.resolver) {
                PropertyCollectionResult::Any => {
                    // any & T = any, so check if any is subtype of target
                    return self.checker.check_subtype(TypeId::ANY, self.target);
                }
                PropertyCollectionResult::NonObject => {
                    // No object properties to check
                }
                PropertyCollectionResult::Properties {
                    properties,
                    string_index,
                    number_index,
                } => {
                    if !properties.is_empty() || string_index.is_some() || number_index.is_some() {
                        let merged_type = if string_index.is_some() || number_index.is_some() {
                            self.checker.interner.object_with_index(ObjectShape {
                                flags: ObjectFlags::empty(),
                                properties,
                                string_index,
                                number_index,
                                symbol: None,
                            })
                        } else {
                            self.checker.interner.object(properties)
                        };
                        if self
                            .checker
                            .check_subtype(merged_type, self.target)
                            .is_true()
                        {
                            return SubtypeResult::True;
                        }
                    }
                }
            }
        }

        SubtypeResult::False
    }

    fn visit_type_parameter(&mut self, param_info: &TypeParamInfo) -> Self::Output {
        self.checker
            .check_type_parameter_subtype(param_info, self.target)
    }

    fn visit_recursive(&mut self, _de_bruijn_index: u32) -> Self::Output {
        // Recursive references are valid in coinductive semantics
        SubtypeResult::True
    }

    fn visit_lazy(&mut self, def_id: u32) -> Self::Output {
        // Resolve the Lazy(DefId) type using the resolver
        let resolved = self
            .checker
            .resolver
            .resolve_lazy(DefId(def_id), self.checker.interner)
            .unwrap_or(self.source);

        // If resolution succeeded and changed the type, restart the check
        // This is critical for coinductive cycle detection to work correctly
        if resolved != self.source {
            self.checker.check_subtype(resolved, self.target)
        } else {
            // Resolution failed or returned the same type - fall through
            SubtypeResult::False
        }
    }

    #[allow(deprecated)]
    fn visit_ref(&mut self, symbol_ref: u32) -> Self::Output {
        // Resolve the legacy Ref(SymbolRef) type using the resolver
        #[allow(deprecated)]
        let resolved = self
            .checker
            .resolver
            .resolve_ref(SymbolRef(symbol_ref), self.checker.interner)
            .unwrap_or(self.source);

        // If resolution succeeded and changed the type, restart the check
        // This is critical for coinductive cycle detection to work correctly
        if resolved != self.source {
            self.checker.check_subtype(resolved, self.target)
        } else {
            // Resolution failed or returned the same type - fall through
            SubtypeResult::False
        }
    }

    fn visit_readonly_type(&mut self, inner_type: TypeId) -> Self::Output {
        // Readonly types have specific subtyping rules:
        // - Readonly<T> <: Readonly<U> if T <: U
        // - Readonly<T> is NOT assignable to mutable T (safety)
        // - T <: Readonly<T> is allowed (can add readonly) - handled by target peeling in check_subtype_inner

        // Case: Readonly<S> <: Readonly<T>
        // If target is also Readonly, compare inner types
        if let Some(t_inner) = readonly_inner_type(self.checker.interner, self.target) {
            return self.checker.check_subtype(inner_type, t_inner);
        }

        // Case: Readonly<S> <: Mutable<T>
        // Readonly source cannot be assigned to mutable target for safety reasons.
        // Exception: target is any/unknown (handled by fast paths in check_subtype_inner).
        SubtypeResult::False
    }

    fn visit_string_intrinsic(
        &mut self,
        _kind: StringIntrinsicKind,
        _type_arg: TypeId,
    ) -> Self::Output {
        // String intrinsics are handled by evaluation
        SubtypeResult::False
    }

    fn visit_enum(&mut self, def_id: u32, member_type: TypeId) -> Self::Output {
        // Enums are nominal types - nominal identity matters for enum-to-enum
        if let Some((t_def, _t_members)) = enum_components(self.checker.interner, self.target) {
            // Enum to Enum: Nominal check - DefIds must match
            return if DefId(def_id) == t_def {
                SubtypeResult::True
            } else {
                SubtypeResult::False
            };
        }

        // Enum to non-Enum: Structural check on member type
        // e.g., Enum(1, 2, 3) <: number
        self.checker.check_subtype(member_type, self.target)
    }

    // Double dispatch implementations for structural types
    // These check the target type to determine which helper method to call

    fn visit_object(&mut self, shape_id: u32) -> Self::Output {
        // Double dispatch: check target type to determine which helper to call
        let s_shape = self.checker.interner.object_shape(ObjectShapeId(shape_id));

        if let Some(t_shape_id) = object_shape_id(self.checker.interner, self.target) {
            // Object <: Object
            let t_shape = self.checker.interner.object_shape(t_shape_id);
            self.checker
                .check_object_subtype(&s_shape, Some(ObjectShapeId(shape_id)), &t_shape)
        } else if let Some(t_shape_id) =
            object_with_index_shape_id(self.checker.interner, self.target)
        {
            // Object <: ObjectWithIndex
            let t_shape = self.checker.interner.object_shape(t_shape_id);
            self.checker.check_object_to_indexed(
                &s_shape.properties,
                Some(ObjectShapeId(shape_id)),
                &t_shape,
            )
        } else {
            SubtypeResult::False
        }
    }

    fn visit_object_with_index(&mut self, shape_id: u32) -> Self::Output {
        // Double dispatch: check target type to determine which helper to call
        let s_shape = self.checker.interner.object_shape(ObjectShapeId(shape_id));

        if let Some(t_shape_id) = object_with_index_shape_id(self.checker.interner, self.target) {
            // ObjectWithIndex <: ObjectWithIndex
            let t_shape = self.checker.interner.object_shape(t_shape_id);
            self.checker.check_object_with_index_subtype(
                &s_shape,
                Some(ObjectShapeId(shape_id)),
                &t_shape,
            )
        } else if let Some(t_shape_id) = object_shape_id(self.checker.interner, self.target) {
            // ObjectWithIndex <: Object
            let t_shape = self.checker.interner.object_shape(t_shape_id);
            self.checker.check_object_with_index_to_object(
                &s_shape,
                ObjectShapeId(shape_id),
                &t_shape.properties,
            )
        } else {
            SubtypeResult::False
        }
    }
    fn visit_function(&mut self, shape_id: u32) -> Self::Output {
        // Double dispatch: check target type to determine which helper to call
        if let Some(t_fn_id) = function_shape_id(self.checker.interner, self.target) {
            // Function <: Function
            let s_fn = self
                .checker
                .interner
                .function_shape(FunctionShapeId(shape_id));
            let t_fn = self.checker.interner.function_shape(t_fn_id);
            self.checker.check_function_subtype(&s_fn, &t_fn)
        } else if let Some(t_callable_id) = callable_shape_id(self.checker.interner, self.target) {
            // Function <: Callable
            self.checker
                .check_function_to_callable_subtype(FunctionShapeId(shape_id), t_callable_id)
        } else {
            SubtypeResult::False
        }
    }

    fn visit_callable(&mut self, shape_id: u32) -> Self::Output {
        // Double dispatch: check target type to determine which helper to call
        if let Some(t_callable_id) = callable_shape_id(self.checker.interner, self.target) {
            // Callable <: Callable
            let s_callable = self
                .checker
                .interner
                .callable_shape(CallableShapeId(shape_id));
            let t_callable = self.checker.interner.callable_shape(t_callable_id);
            self.checker
                .check_callable_subtype(&s_callable, &t_callable)
        } else if let Some(t_fn_id) = function_shape_id(self.checker.interner, self.target) {
            // Callable <: Function
            self.checker
                .check_callable_to_function_subtype(CallableShapeId(shape_id), t_fn_id)
        } else {
            SubtypeResult::False
        }
    }
    fn visit_bound_parameter(&mut self, _de_bruijn_index: u32) -> Self::Output {
        SubtypeResult::False
    }
    fn visit_application(&mut self, app_id: u32) -> Self::Output {
        // Application types require the original source TypeId for proper expansion
        self.checker.check_application_expansion_target(
            self.source,
            self.target,
            TypeApplicationId(app_id),
        )
    }
    fn visit_conditional(&mut self, cond_id: u32) -> Self::Output {
        // Conditional types require special handling
        self.checker.conditional_branches_subtype(
            self.checker
                .interner
                .conditional_type(ConditionalTypeId(cond_id))
                .as_ref(),
            self.target,
        )
    }

    fn visit_mapped(&mut self, mapped_id: u32) -> Self::Output {
        // Mapped types require the original source TypeId for proper expansion
        self.checker.check_mapped_expansion_target(
            self.source,
            self.target,
            MappedTypeId(mapped_id),
        )
    }
    fn visit_index_access(&mut self, object_type: TypeId, key_type: TypeId) -> Self::Output {
        use crate::solver::visitor::index_access_parts;

        // S[I] <: T[J]  <=>  S <: T  AND  I <: J
        // This handles deferred index access types (usually involving type parameters).
        if let Some((t_obj, t_idx)) = index_access_parts(self.checker.interner, self.target) {
            // Coinductive check: delegate back to check_subtype for both parts
            if self.checker.check_subtype(object_type, t_obj).is_true()
                && self.checker.check_subtype(key_type, t_idx).is_true()
            {
                return SubtypeResult::True;
            }
        }

        // If target is not an IndexAccess, we cannot prove subtyping.
        // Note: If S[I] could have been simplified to a concrete type that matches the target,
        // evaluate_type() in the caller (check_subtype) would have already handled it.
        SubtypeResult::False
    }
    fn visit_template_literal(&mut self, template_id: u32) -> Self::Output {
        use crate::solver::types::IntrinsicKind;
        use crate::solver::types::TemplateLiteralId;
        use crate::solver::types::TemplateSpan;
        use crate::solver::visitor::{intrinsic_kind, template_literal_id};

        // Template literal <: string is always true
        if intrinsic_kind(self.checker.interner, self.target) == Some(IntrinsicKind::String) {
            return SubtypeResult::True;
        }

        // Template literal <: Template literal
        // Compare spans: Text must match exactly, Type must satisfy subtype
        if let Some(t_template_id) = template_literal_id(self.checker.interner, self.target) {
            let s_id = TemplateLiteralId(template_id);

            // Fast path: same template literal
            if s_id == t_template_id {
                return SubtypeResult::True;
            }

            let s_list = self.checker.interner.template_list(s_id);
            let t_list = self.checker.interner.template_list(t_template_id);

            // Different number of spans - not compatible
            if s_list.len() != t_list.len() {
                return SubtypeResult::False;
            }

            // Compare each span
            for (s_span, t_span) in s_list.iter().zip(t_list.iter()) {
                match (s_span, t_span) {
                    (TemplateSpan::Text(s_text), TemplateSpan::Text(t_text)) => {
                        if s_text != t_text {
                            return SubtypeResult::False;
                        }
                    }
                    (TemplateSpan::Type(s_type), TemplateSpan::Type(t_type)) => {
                        if !self.checker.check_subtype(*s_type, *t_type).is_true() {
                            return SubtypeResult::False;
                        }
                    }
                    _ => {
                        // Mismatched span types (Text vs Type)
                        return SubtypeResult::False;
                    }
                }
            }

            return SubtypeResult::True;
        }

        SubtypeResult::False
    }
    fn visit_type_query(&mut self, symbol_ref: u32) -> Self::Output {
        use crate::solver::types::SymbolRef;

        // TypeQuery (typeof X) is a reference to a value symbol.
        // We need to resolve it to its structural type before comparing.
        let sym = SymbolRef(symbol_ref);

        // Attempt to resolve the symbol to its structural type.
        // Prioritize DefId-based resolution (Lazy) over legacy SymbolRef (Ref).
        let resolved = if let Some(def_id) = self.checker.resolver.symbol_to_def_id(sym) {
            self.checker
                .resolver
                .resolve_lazy(def_id, self.checker.interner)
        } else {
            #[allow(deprecated)]
            self.checker
                .resolver
                .resolve_ref(sym, self.checker.interner)
        }
        .unwrap_or(self.source);

        // If resolution succeeded and gave us a different type, restart the check.
        // This recursion is critical for coinductive cycle detection.
        if resolved != self.source {
            self.checker.check_subtype(resolved, self.target)
        } else {
            // If resolution failed or returned the same ID, we cannot prove subtyping.
            SubtypeResult::False
        }
    }
    fn visit_keyof(&mut self, inner_type: TypeId) -> Self::Output {
        use crate::solver::types::IntrinsicKind;
        use crate::solver::visitor::{keyof_inner_type, union_list_id};

        // keyof S <: keyof T  <=>  T <: S (Contravariant)
        // If target is also a keyof type, check inner types in reverse
        if let Some(t_inner) = keyof_inner_type(self.checker.interner, self.target) {
            return self.checker.check_subtype(t_inner, inner_type);
        }

        // If inner_type is a TypeParameter, keyof T is NOT a subtype of primitives
        // (deferred keyof - we don't know what keys T has)
        if matches!(
            self.checker.interner.lookup(inner_type),
            Some(TypeKey::TypeParameter(_))
        ) {
            return SubtypeResult::False;
        }

        // keyof T is always a subtype of string | number | symbol
        // Check if target is a union that matches this pattern
        if let Some(union_id) = union_list_id(self.checker.interner, self.target) {
            let members = self.checker.interner.type_list(union_id);
            // Check if all members are string, number, or symbol
            let all_primitive = members.iter().all(|&m| {
                matches!(
                    self.checker.interner.lookup(m),
                    Some(TypeKey::Intrinsic(
                        IntrinsicKind::String | IntrinsicKind::Number | IntrinsicKind::Symbol
                    ))
                )
            });
            if all_primitive && !members.is_empty() {
                return SubtypeResult::True;
            }
        }

        // keyof is also subtype of the specific primitive if it matches
        if let Some(TypeKey::Intrinsic(
            IntrinsicKind::String | IntrinsicKind::Number | IntrinsicKind::Symbol,
        )) = self.checker.interner.lookup(self.target)
        {
            return SubtypeResult::True;
        }

        SubtypeResult::False
    }
    fn visit_this_type(&mut self) -> Self::Output {
        use crate::solver::visitor::is_this_type;

        // If target is also a 'this' type, they are compatible.
        // This handles cases like comparing two uninstantiated generic methods.
        if is_this_type(self.checker.interner, self.target) {
            return SubtypeResult::True;
        }

        // If we reach here, 'this' is being compared against a non-this type.
        // In most cases, check_subtype_inner's apparent_primitive_shape_for_type
        // would have resolved 'this' to its containing class/interface.
        // If that didn't happen or didn't result in 'True', we return False.
        SubtypeResult::False
    }
    fn visit_infer(&mut self, param_info: &TypeParamInfo) -> Self::Output {
        // 'infer R' behaves like a type parameter during structural subtyping.
        // It is a subtype of the target if its constraint satisfies the target.
        self.checker
            .check_type_parameter_subtype(param_info, self.target)
    }
    fn visit_unique_symbol(&mut self, symbol_ref: u32) -> Self::Output {
        use crate::solver::visitor::unique_symbol_ref;

        // unique symbol has nominal identity - same symbol ref is subtype
        if let Some(t_symbol_ref) = unique_symbol_ref(self.checker.interner, self.target) {
            return if symbol_ref == t_symbol_ref.0 {
                SubtypeResult::True
            } else {
                SubtypeResult::False
            };
        }

        // unique symbol is always a subtype of symbol
        if let Some(TypeKey::Intrinsic(IntrinsicKind::Symbol)) =
            self.checker.interner.lookup(self.target)
        {
            return SubtypeResult::True;
        }

        SubtypeResult::False
    }
    fn visit_module_namespace(&mut self, _symbol_ref: u32) -> Self::Output {
        SubtypeResult::False
    }
    fn visit_error(&mut self) -> Self::Output {
        SubtypeResult::False
    }
}

/// Subtype checking context.
/// Maintains the "seen" set for cycle detection.
pub struct SubtypeChecker<'a, R: TypeResolver = NoopResolver> {
    pub(crate) interner: &'a dyn TypeDatabase,
    /// Optional query database for Salsa-backed memoization.
    /// When set, Phase 2/3 will route evaluate_type and is_subtype_of through Salsa.
    pub(crate) query_db: Option<&'a dyn QueryDatabase>,
    pub(crate) resolver: &'a R,
    /// Active subtype pairs being checked (for cycle detection at TypeId level)
    pub(crate) in_progress: FxHashSet<(TypeId, TypeId)>,
    /// Active SymbolRef pairs being checked (for DefId-level cycle detection)
    /// This catches cycles in Ref types before they're resolved, preventing
    /// infinite expansion of recursive type aliases and interfaces.
    pub(crate) seen_refs: FxHashSet<(SymbolRef, SymbolRef)>,
    /// Active DefId pairs being checked (for DefId-level cycle detection)
    /// Phase 3.1: Catches cycles in Lazy(DefId) types before they're resolved.
    /// This mirrors seen_refs but for the new DefId-based type identity system.
    pub(crate) seen_defs: FxHashSet<(DefId, DefId)>,
    /// Current recursion depth (for stack overflow prevention)
    pub(crate) depth: u32,
    /// Total number of check_subtype calls (iteration limit)
    pub(crate) total_checks: u32,
    /// Whether the recursion depth limit was exceeded (for TS2589 diagnostic)
    pub depth_exceeded: bool,
    /// Whether to use strict function types (contravariant parameters).
    /// Default: true (sound, correct behavior)
    pub strict_function_types: bool,
    /// Whether to allow any return type when the target return is void.
    pub allow_void_return: bool,
    /// Whether rest parameters of any/unknown should be treated as bivariant.
    /// See https://github.com/microsoft/TypeScript/issues/20007.
    pub allow_bivariant_rest: bool,
    /// When true, skip the evaluate_type() call in check_subtype.
    /// This prevents infinite recursion when TypeEvaluator calls SubtypeChecker
    /// for simplification, since TypeEvaluator has already evaluated the types.
    pub bypass_evaluation: bool,
    /// Maximum recursion depth for subtype checking.
    /// Used by TypeEvaluator simplification to prevent stack overflow.
    /// Default: MAX_SUBTYPE_DEPTH (100)
    pub max_depth: u32,
    /// Whether required parameter count mismatches are allowed for bivariant methods.
    pub allow_bivariant_param_count: bool,
    /// Whether optional properties are exact (exclude implicit `undefined`).
    /// Default: false (legacy TS behavior).
    pub exact_optional_property_types: bool,
    /// Whether null/undefined are treated as separate types.
    /// Default: true (strict null checks).
    pub strict_null_checks: bool,
    /// Whether indexed access includes `undefined`.
    /// Default: false (legacy TS behavior).
    pub no_unchecked_indexed_access: bool,
    // When true, disables method bivariance (methods use contravariance).
    // Default: false (methods are bivariant in TypeScript for compatibility).
    pub disable_method_bivariance: bool,
    /// Optional inheritance graph for O(1) nominal class subtype checking.
    /// When provided, enables fast nominal checks for class inheritance.
    pub inheritance_graph: Option<&'a crate::solver::inheritance::InheritanceGraph>,
    /// Optional callback to check if a symbol is a class (for nominal subtyping).
    /// Returns true if the symbol has the CLASS flag set.
    pub is_class_symbol: Option<&'a dyn Fn(SymbolRef) -> bool>,
    /// Controls how `any` is treated during subtype checks.
    pub any_propagation: AnyPropagationMode,
    /// Cache for evaluate_type results within this SubtypeChecker's lifetime.
    /// This prevents O(n²) behavior when the same type (e.g., a large union) is
    /// evaluated multiple times across different subtype checks.
    /// Key is (TypeId, no_unchecked_indexed_access) since that flag affects evaluation.
    pub(crate) eval_cache: FxHashMap<(TypeId, bool), TypeId>,
    /// Optional tracer for collecting subtype failure diagnostics.
    /// When `Some`, enables detailed failure reason collection for error messages.
    /// When `None`, disables tracing for maximum performance (default).
    pub tracer: Option<&'a mut dyn DynSubtypeTracer>,
}

/// Maximum total subtype checks allowed per SubtypeChecker instance.
/// Prevents infinite loops in pathological type comparison scenarios.
pub const MAX_TOTAL_SUBTYPE_CHECKS: u32 = 100_000;

impl<'a> SubtypeChecker<'a, NoopResolver> {
    /// Create a new SubtypeChecker without a resolver (basic mode).
    pub fn new(interner: &'a dyn TypeDatabase) -> SubtypeChecker<'a, NoopResolver> {
        static NOOP: NoopResolver = NoopResolver;
        SubtypeChecker {
            interner,
            query_db: None,
            resolver: &NOOP,
            in_progress: FxHashSet::default(),
            seen_refs: FxHashSet::default(),
            seen_defs: FxHashSet::default(),
            depth: 0,
            total_checks: 0,
            depth_exceeded: false,
            strict_function_types: true, // Default to strict (sound) behavior
            allow_void_return: false,
            allow_bivariant_rest: false,
            allow_bivariant_param_count: false,
            exact_optional_property_types: false,
            strict_null_checks: true,
            no_unchecked_indexed_access: false,
            disable_method_bivariance: false,
            inheritance_graph: None,
            is_class_symbol: None,
            any_propagation: AnyPropagationMode::All,
            bypass_evaluation: false,
            max_depth: MAX_SUBTYPE_DEPTH,
            eval_cache: FxHashMap::default(),
            tracer: None,
        }
    }
}

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    /// Create a new SubtypeChecker with a custom resolver.
    pub fn with_resolver(interner: &'a dyn TypeDatabase, resolver: &'a R) -> Self {
        SubtypeChecker {
            interner,
            query_db: None,
            resolver,
            in_progress: FxHashSet::default(),
            seen_refs: FxHashSet::default(),
            seen_defs: FxHashSet::default(),
            depth: 0,
            total_checks: 0,
            depth_exceeded: false,
            strict_function_types: true,
            allow_void_return: false,
            allow_bivariant_rest: false,
            allow_bivariant_param_count: false,
            exact_optional_property_types: false,
            strict_null_checks: true,
            no_unchecked_indexed_access: false,
            disable_method_bivariance: false,
            inheritance_graph: None,
            is_class_symbol: None,
            any_propagation: AnyPropagationMode::All,
            bypass_evaluation: false,
            max_depth: MAX_SUBTYPE_DEPTH,
            eval_cache: FxHashMap::default(),
            tracer: None,
        }
    }

    /// Set the inheritance graph for O(1) nominal class subtype checking.
    pub fn with_inheritance_graph(
        mut self,
        graph: &'a crate::solver::inheritance::InheritanceGraph,
    ) -> Self {
        self.inheritance_graph = Some(graph);
        self
    }

    /// Set the callback to check if a symbol is a class.
    pub fn with_class_check(mut self, check: &'a dyn Fn(SymbolRef) -> bool) -> Self {
        self.is_class_symbol = Some(check);
        self
    }

    /// Configure how `any` is treated during subtype checks.
    pub fn with_any_propagation_mode(mut self, mode: AnyPropagationMode) -> Self {
        self.any_propagation = mode;
        self
    }

    /// Set the tracer for collecting subtype failure diagnostics.
    /// When set, enables detailed failure reason collection for error messages.
    pub fn with_tracer(mut self, tracer: &'a mut dyn DynSubtypeTracer) -> Self {
        self.tracer = Some(tracer);
        self
    }

    /// Set the query database for Salsa-backed memoization.
    /// When set, Phase 2/3 will route evaluate_type and is_subtype_of through Salsa.
    pub fn with_query_db(mut self, db: &'a dyn QueryDatabase) -> Self {
        self.query_db = Some(db);
        self
    }

    /// Get the query database, if available.
    #[inline]
    #[allow(dead_code)]
    pub(crate) fn query_db(&self) -> Option<&'a dyn QueryDatabase> {
        self.query_db
    }

    /// Set whether strict null checks are enabled.
    /// When false, null and undefined are assignable to any type.
    pub fn with_strict_null_checks(mut self, strict_null_checks: bool) -> Self {
        self.strict_null_checks = strict_null_checks;
        self
    }

    /// Apply compiler flags from a packed u16 bitmask.
    ///
    /// This unpacks the flags used by `RelationCacheKey` and applies them to the checker.
    /// The bit layout matches the cache key definition in types.rs:
    /// - bit 0: strict_null_checks
    /// - bit 1: strict_function_types
    /// - bit 2: exact_optional_property_types
    /// - bit 3: no_unchecked_indexed_access
    /// - bit 4: disable_method_bivariance
    /// - bit 5: allow_void_return
    /// - bit 6: allow_bivariant_rest
    /// - bit 7: allow_bivariant_param_count
    pub(crate) fn apply_flags(mut self, flags: u16) -> Self {
        self.strict_null_checks = (flags & (1 << 0)) != 0;
        self.strict_function_types = (flags & (1 << 1)) != 0;
        self.exact_optional_property_types = (flags & (1 << 2)) != 0;
        self.no_unchecked_indexed_access = (flags & (1 << 3)) != 0;
        self.disable_method_bivariance = (flags & (1 << 4)) != 0;
        self.allow_void_return = (flags & (1 << 5)) != 0;
        self.allow_bivariant_rest = (flags & (1 << 6)) != 0;
        self.allow_bivariant_param_count = (flags & (1 << 7)) != 0;
        self
    }

    pub(crate) fn resolve_ref_type(&self, type_id: TypeId) -> TypeId {
        // Handle DefId-based Lazy types (new API)
        if let Some(def_id) = lazy_def_id(self.interner, type_id) {
            return self
                .resolver
                .resolve_lazy(def_id, self.interner)
                .unwrap_or(type_id);
        }

        // Handle legacy SymbolRef-based types (old API)
        if let Some(symbol) = ref_symbol(self.interner, type_id) {
            if let Some(def_id) = self.resolver.symbol_to_def_id(symbol) {
                self.resolver
                    .resolve_lazy(def_id, self.interner)
                    .unwrap_or(type_id)
            } else {
                #[allow(deprecated)]
                self.resolver
                    .resolve_ref(symbol, self.interner)
                    .unwrap_or(type_id)
            }
        } else {
            type_id
        }
    }

    pub(crate) fn resolve_lazy_type(&self, type_id: TypeId) -> TypeId {
        if let Some(def_id) = lazy_def_id(self.interner, type_id) {
            self.resolver
                .resolve_lazy(def_id, self.interner)
                .unwrap_or(type_id)
        } else {
            type_id
        }
    }

    /// Check if two types have any overlap (non-empty intersection).
    ///
    /// This is used for TS2367: "This condition will always return 'false' since the types 'X' and 'Y' have no overlap."
    ///
    /// Returns true if there exists at least one type that is a subtype of both a and b.
    /// Returns false if a & b would be the `never` type (zero overlap).
    ///
    /// # MVP Implementation (Phase 1)
    ///
    /// This catches OBVIOUS non-overlaps:
    /// - Different primitives (string vs number, boolean vs bigint, etc.)
    /// - Different literals of same primitive ("a" vs "b", 1 vs 2)
    /// - Object property type mismatches ({ a: string } vs { a: number })
    ///
    /// For complex types (unions, intersections, generics), we conservatively return true
    /// to avoid false positives. Phase 2 will add more sophisticated overlap detection.
    ///
    /// # Examples
    /// - `are_types_overlapping(string, number)` -> false (different primitives)
    /// - `are_types_overlapping(1, 2)` -> false (different number literals)
    /// - `are_types_overlapping({ a: string }, { a: number })` -> false (property type mismatch)
    /// - `are_types_overlapping({ a: 1 }, { b: 2 })` -> true (can have { a: 1, b: 2 })
    /// - `are_types_overlapping(string, "hello")` -> true (literal is subtype of primitive)
    pub(crate) fn are_types_overlapping(&self, a: TypeId, b: TypeId) -> bool {
        // Fast path: identical types overlap (unless never)
        if a == b {
            return a != TypeId::NEVER;
        }

        // Top types: any/unknown overlap with everything except never
        if a == TypeId::ANY || a == TypeId::UNKNOWN {
            return b != TypeId::NEVER;
        }
        if b == TypeId::ANY || b == TypeId::UNKNOWN {
            return a != TypeId::NEVER;
        }

        // Bottom type: never overlaps with nothing
        if a == TypeId::NEVER || b == TypeId::NEVER {
            return false;
        }

        // Resolve Lazy/Ref types before checking
        let a_resolved = self.resolve_ref_type(a);
        let b_resolved = self.resolve_ref_type(b);

        // Check if either is subtype of the other (sufficient condition, not necessary)
        // This catches: literal <: primitive, object <: interface, etc.
        // Note: check_subtype returns SubtypeResult, but we need &mut self for it
        // For now, we'll use a simpler approach that doesn't require mutation
        if self.are_types_in_subtype_relation(a_resolved, b_resolved) {
            return true;
        }

        // Check for different primitive types
        if let (Some(a_kind), Some(b_kind)) = (
            intrinsic_kind(self.interner, a_resolved),
            intrinsic_kind(self.interner, b_resolved),
        ) {
            // 1. Handle strictNullChecks
            if !self.strict_null_checks {
                // If strict null checks is OFF, null/undefined overlap with everything
                if matches!(a_kind, IntrinsicKind::Null | IntrinsicKind::Undefined)
                    || matches!(b_kind, IntrinsicKind::Null | IntrinsicKind::Undefined)
                {
                    return true;
                }
            }

            // 2. Handle Void vs Undefined (always overlap)
            if (a_kind == IntrinsicKind::Void && b_kind == IntrinsicKind::Undefined)
                || (a_kind == IntrinsicKind::Undefined && b_kind == IntrinsicKind::Void)
            {
                return true;
            }

            // 3. Compare primitives
            match (a_kind, b_kind) {
                (IntrinsicKind::String, IntrinsicKind::String)
                | (IntrinsicKind::Number, IntrinsicKind::Number)
                | (IntrinsicKind::Boolean, IntrinsicKind::Boolean)
                | (IntrinsicKind::Bigint, IntrinsicKind::Bigint)
                | (IntrinsicKind::Symbol, IntrinsicKind::Symbol) => {
                    // Same primitive type - check if they're different literals
                    return self.are_literals_overlapping(a_resolved, b_resolved);
                }
                // Distinct primitives do not overlap
                (IntrinsicKind::String, _)
                | (IntrinsicKind::Number, _)
                | (IntrinsicKind::Boolean, _)
                | (IntrinsicKind::Bigint, _)
                | (IntrinsicKind::Symbol, _)
                | (IntrinsicKind::Null, _)
                | (IntrinsicKind::Undefined, _)
                | (IntrinsicKind::Void, _) => {
                    return false;
                }
                // Handle Object keyword vs Primitives (Disjoint)
                (IntrinsicKind::Object, _) | (_, IntrinsicKind::Object) => {
                    // 'object' (non-primitive) does not overlap with primitives
                    // Note: It DOES overlap with Object (interface), but that is handled
                    // by object_shape_id, not intrinsic_kind.
                    return false;
                }
                // Fallback for any new intrinsics added later
                _ => return true,
            }
        }

        // Check for different literal values of the same primitive type
        if let (Some(a_lit), Some(b_lit)) = (
            literal_value(self.interner, a_resolved),
            literal_value(self.interner, b_resolved),
        ) {
            // Different literal values never overlap
            return a_lit == b_lit;
        }

        // For object-like types, use refined overlap detection with PropertyCollector
        // This handles: objects, objects with index signatures, and intersections
        // This replaces the simplified check that only handled direct object-to-object
        let is_a_obj = self.is_object_like(a_resolved);
        let is_b_obj = self.is_object_like(b_resolved);

        if is_a_obj && is_b_obj {
            return self.do_refined_object_overlap_check(a_resolved, b_resolved);
        }

        // Template literal disjointness detection
        // Two template literals with different starting/ending text are disjoint
        if let (Some(a_spans), Some(b_spans)) = (
            template_literal_id(self.interner, a_resolved),
            template_literal_id(self.interner, b_resolved),
        ) {
            return self.are_template_literals_overlapping(a_spans, b_spans);
        }

        // Conservative: assume overlap for complex types we haven't fully handled yet
        // (unions, intersections, generics, etc.)
        // Better to miss some TS2367 errors than to emit them incorrectly
        true
    }

    /// Check if one type is a subtype of the other without mutation.
    ///
    /// This is a simplified version that checks obvious subtype relationships
    /// without needing to call the full check_subtype which requires &mut self.
    fn are_types_in_subtype_relation(&self, a: TypeId, b: TypeId) -> bool {
        // Check identity first
        if a == b {
            return true;
        }

        // Check for literal-to-primitive relationships
        if let (Some(a_lit), Some(b_kind)) = (
            literal_value(self.interner, a),
            intrinsic_kind(self.interner, b),
        ) {
            return match (a_lit, b_kind) {
                (LiteralValue::String(_), IntrinsicKind::String) => true,
                (LiteralValue::Number(_), IntrinsicKind::Number) => true,
                (LiteralValue::BigInt(_), IntrinsicKind::Bigint) => true,
                (LiteralValue::Boolean(_), IntrinsicKind::Boolean) => true,
                _ => false,
            };
        }

        if let (Some(a_kind), Some(b_lit)) = (
            intrinsic_kind(self.interner, a),
            literal_value(self.interner, b),
        ) {
            return match (a_kind, b_lit) {
                (IntrinsicKind::String, LiteralValue::String(_)) => true,
                (IntrinsicKind::Number, LiteralValue::Number(_)) => true,
                (IntrinsicKind::Bigint, LiteralValue::BigInt(_)) => true,
                (IntrinsicKind::Boolean, LiteralValue::Boolean(_)) => true,
                _ => false,
            };
        }

        false
    }

    /// Check if two literal types have overlapping values.
    ///
    /// Returns false if they're different literals of the same primitive type.
    /// Returns true if they're the same literal or if we can't determine.
    fn are_literals_overlapping(&self, a: TypeId, b: TypeId) -> bool {
        if let (Some(a_lit), Some(b_lit)) = (
            literal_value(self.interner, a),
            literal_value(self.interner, b),
        ) {
            // Different literal values of the same primitive type never overlap
            a_lit == b_lit
        } else {
            // At least one isn't a literal, so they overlap
            true
        }
    }

    /// Check if two template literal types have any overlap.
    ///
    /// Template literals are disjoint if they have incompatible fixed text spans.
    /// For example:
    /// - `foo${string}` and `bar${string}` are disjoint (different prefixes)
    /// - `foo${string}` and `foo${number}` may overlap (same prefix, compatible types)
    /// - `a${string}b` and `a${string}c` are disjoint (different suffixes)
    ///
    /// Returns false if types are guaranteed disjoint, true otherwise.
    fn are_template_literals_overlapping(
        &self,
        a: TemplateLiteralId,
        b: TemplateLiteralId,
    ) -> bool {
        // Fast path: same template literal definitely overlaps
        if a == b {
            return true;
        }

        let a_spans = self.interner.template_list(a);
        let b_spans = self.interner.template_list(b);

        // Templates with different numbers of spans might still overlap
        // if the type holes are wide enough (e.g., string)
        // We need to check if there's any possible string that matches both patterns

        // For simplicity, we check if there are incompatible fixed text spans
        let a_len = a_spans.len();
        let b_len = b_spans.len();

        // Collect fixed text patterns from both templates
        // Two templates are disjoint if they have incompatible fixed text at any position
        let mut a_idx = 0;
        let mut b_idx = 0;

        loop {
            // Skip type holes in both templates
            while a_idx < a_len && matches!(a_spans[a_idx], TemplateSpan::Type(_)) {
                a_idx += 1;
            }
            while b_idx < b_len && matches!(b_spans[b_idx], TemplateSpan::Type(_)) {
                b_idx += 1;
            }

            // If both reached the end, they overlap (both can match empty string after all type holes)
            if a_idx >= a_len && b_idx >= b_len {
                return true;
            }

            // If only one reached the end, check if the remaining can be empty
            if a_idx >= a_len {
                // A exhausted, B has more content
                // They overlap only if B's remaining content is all type holes
                return b_spans[b_idx..]
                    .iter()
                    .all(|s| matches!(s, TemplateSpan::Type(_)));
            }
            if b_idx >= b_len {
                // B exhausted, A has more content
                return a_spans[a_idx..]
                    .iter()
                    .all(|s| matches!(s, TemplateSpan::Type(_)));
            }

            // Both have text spans - check if they match
            match (&a_spans[a_idx], &b_spans[b_idx]) {
                (TemplateSpan::Text(a_text), TemplateSpan::Text(b_text)) => {
                    let a_str = self.interner.resolve_atom(*a_text);
                    let b_str = self.interner.resolve_atom(*b_text);

                    // Check if the text spans can match
                    // They must have at least one common prefix
                    let min_len = a_str.len().min(b_str.len());
                    if a_str[..min_len] != b_str[..min_len] {
                        // Incompatible prefixes - templates are disjoint
                        return false;
                    }

                    // Advance past the common prefix
                    let advance = min_len;
                    a_idx += 1;
                    b_idx += 1;

                    // If one text span is exhausted, the other must have type holes to continue
                    if a_str.len() > advance {
                        // A's text is longer - B needs a type hole to consume the rest
                        if b_idx >= b_len || !matches!(b_spans[b_idx], TemplateSpan::Type(_)) {
                            // B can't consume the rest of A's text - disjoint unless A's extra text is a prefix
                            // that B's type hole can match
                            return a_str[advance..].is_empty();
                        }
                    }
                    if b_str.len() > advance {
                        // B's text is longer - A needs a type hole to consume the rest
                        if a_idx >= a_len || !matches!(a_spans[a_idx], TemplateSpan::Type(_)) {
                            return b_str[advance..].is_empty();
                        }
                    }
                }
                _ => {
                    // One is text, one is type - they're compatible
                    // The type can match any string, so we advance both
                    a_idx += 1;
                    b_idx += 1;
                }
            }
        }
    }

    /// Check if two types are "object-like" (should use PropertyCollector for overlap detection).
    ///
    /// Object-like types include:
    /// - Plain objects with properties
    /// - Objects with index signatures
    /// - Intersections (which may contain objects)
    fn is_object_like(&self, type_id: TypeId) -> bool {
        use crate::solver::visitor::{
            intersection_list_id, object_shape_id, object_with_index_shape_id,
        };

        object_shape_id(self.interner, type_id).is_some()
            || object_with_index_shape_id(self.interner, type_id).is_some()
            || intersection_list_id(self.interner, type_id).is_some()
    }

    /// Check if two object-like types have overlapping properties and index signatures.
    ///
    /// This is the refined implementation using PropertyCollector to handle:
    /// - Intersections (flattened property collection)
    /// - Index signatures (both string and number)
    /// - Optional properties (correct undefined handling via optional_property_type)
    /// - Discriminant detection (common property with disjoint literal types)
    ///
    /// Returns false if types have zero overlap, true otherwise.
    fn do_refined_object_overlap_check(&self, a: TypeId, b: TypeId) -> bool {
        use crate::solver::objects::{PropertyCollectionResult, collect_properties};

        // Collect properties and index signatures from both types
        let res_a = collect_properties(a, self.interner, self.resolver);
        let res_b = collect_properties(b, self.interner, self.resolver);

        // Extract properties and index signatures from results
        let (props_a, s_idx_a, _n_idx_a) = match res_a {
            PropertyCollectionResult::Any => return true, // Any overlaps with everything
            PropertyCollectionResult::NonObject => return true, // Conservatively overlap
            PropertyCollectionResult::Properties {
                properties,
                string_index,
                number_index,
            } => (properties, string_index, number_index),
        };

        let (props_b, s_idx_b, _n_idx_b) = match res_b {
            PropertyCollectionResult::Any => return true,
            PropertyCollectionResult::NonObject => return true,
            PropertyCollectionResult::Properties {
                properties,
                string_index,
                number_index,
            } => (properties, string_index, number_index),
        };

        // 1. Check Common Properties for overlap
        // If a property exists in both objects, their types must overlap
        for p_a in &props_a {
            if let Some(p_b) = props_b.iter().find(|p| p.name == p_a.name) {
                // Use optional_property_type for correct undefined handling
                let type_a = self.optional_property_type(p_a);
                let type_b = self.optional_property_type(p_b);

                if !self.are_types_overlapping(type_a, type_b) {
                    return false; // Hard conflict - no overlap
                }
            }
        }

        // 2. Check Required Properties A against Index Signatures B
        // Only REQUIRED properties must be compatible with B's string index.
        // Optional properties can be missing (undefined) so they don't conflict with index signatures.
        // Example: { a?: string } and { [k: string]: number } DO overlap because {} satisfies both.
        if let Some(ref idx_b) = s_idx_b {
            for p_a in &props_a {
                if !p_a.optional {
                    // Only check required properties
                    if !self.are_types_overlapping(p_a.type_id, idx_b.value_type) {
                        return false;
                    }
                }
            }
        }

        // 3. Check Required Properties B against Index Signatures A
        // Only REQUIRED properties must be compatible with A's string index.
        if let Some(ref idx_a) = s_idx_a {
            for p_b in &props_b {
                if !p_b.optional {
                    // Only check required properties
                    if !self.are_types_overlapping(p_b.type_id, idx_a.value_type) {
                        return false;
                    }
                }
            }
        }

        // 4. Index Signature Compatibility Check
        // NOTE: Index signatures do NOT prevent overlap even if their value types are disjoint
        // because the empty object {} satisfies both index signatures.
        // Example: { [k: string]: string } and { [k: string]: number } DO overlap.
        // So NO CHECK needed here - index signatures never cause disjointness.

        // All checks passed - types overlap
        true
    }

    /// Check if two object types have overlapping properties.
    ///
    /// Returns false if any common property has non-overlapping types.
    /// Returns true if all common properties have overlapping types.
    ///
    /// This is a simplified check - Phase 2 will use PropertyCollector
    /// for full intersection-type-aware checking.
    #[allow(dead_code)]
    fn do_object_properties_overlap(&self, a_shape: ObjectShapeId, b_shape: ObjectShapeId) -> bool {
        let a_props = self.interner.object_shape(a_shape);
        let b_props = self.interner.object_shape(b_shape);

        // Check each common property
        for a_prop in &a_props.properties {
            if let Some(b_prop) = b_props.properties.iter().find(|p| p.name == a_prop.name) {
                // If the common property types don't overlap, the objects don't overlap
                if !self.are_types_overlapping(a_prop.type_id, b_prop.type_id) {
                    return false;
                }
            }
        }

        // All common properties have overlapping types
        // (Note: this allows { a: string } & { b: number } to overlap, which is correct)
        true
    }

    /// Construct a `RelationCacheKey` for the current checker configuration.
    ///
    /// This packs the Lawyer-layer flags into a compact cache key to ensure that
    /// results computed under different rules (strict vs non-strict) don't contaminate each other.
    fn make_cache_key(&self, source: TypeId, target: TypeId) -> RelationCacheKey {
        // Pack boolean flags into a u16 bitmask:
        // bit 0: strict_null_checks
        // bit 1: strict_function_types
        // bit 2: exact_optional_property_types
        // bit 3: no_unchecked_indexed_access
        // bit 4: disable_method_bivariance
        // bit 5: allow_void_return
        // bit 6: allow_bivariant_rest
        // bit 7: allow_bivariant_param_count
        let mut flags: u16 = 0;
        if self.strict_null_checks {
            flags |= 1 << 0;
        }
        if self.strict_function_types {
            flags |= 1 << 1;
        }
        if self.exact_optional_property_types {
            flags |= 1 << 2;
        }
        if self.no_unchecked_indexed_access {
            flags |= 1 << 3;
        }
        if self.disable_method_bivariance {
            flags |= 1 << 4;
        }
        if self.allow_void_return {
            flags |= 1 << 5;
        }
        if self.allow_bivariant_rest {
            flags |= 1 << 6;
        }
        if self.allow_bivariant_param_count {
            flags |= 1 << 7;
        }

        // CRITICAL: Calculate effective `any_mode` based on depth.
        // If `any_propagation` is `TopLevelOnly` but `depth > 0`, the effective mode is "None".
        // This ensures that top-level checks don't incorrectly hit cached results from nested checks.
        let any_mode = match self.any_propagation {
            AnyPropagationMode::All => 0,
            AnyPropagationMode::TopLevelOnly if self.depth == 0 => 1,
            AnyPropagationMode::TopLevelOnly => 2, // Disabled at depth > 0
        };

        RelationCacheKey::subtype(source, target, flags, any_mode)
    }

    /// Check if `source` is a subtype of `target`.
    /// This is the main entry point for subtype checking.
    ///
    /// When a QueryDatabase is available (via `with_query_db`), fast-path checks
    /// (identity, any, unknown, never) are done locally, then the full structural
    /// check is delegated to the internal `check_subtype` which may use Salsa
    /// memoization for evaluate_type calls.
    pub fn is_subtype_of(&mut self, source: TypeId, target: TypeId) -> bool {
        self.check_subtype(source, target).is_true()
    }

    /// Check if `source` is assignable to `target`.
    /// This is a strict structural check; use CompatChecker for TypeScript assignability rules.
    pub fn is_assignable_to(&mut self, source: TypeId, target: TypeId) -> bool {
        self.is_subtype_of(source, target)
    }

    /// Internal subtype check with cycle detection
    ///
    /// # Cycle Detection Strategy (Coinductive Semantics)
    ///
    /// This function implements coinductive cycle handling for recursive types.
    /// The key insight is that we must check for cycles BEFORE evaluation to handle
    /// "expansive" types like `type Deep<T> = { next: Deep<Box<T>> }` that produce
    /// fresh TypeIds on each evaluation.
    ///
    /// The algorithm:
    /// 1. Fast paths (identity, any, unknown, never)
    /// 2. **Cycle detection FIRST** (before evaluation!)
    /// 3. Meta-type evaluation (keyof, conditional, mapped, etc.)
    /// 4. Structural comparison
    ///
    /// When a cycle is detected, we return `CycleDetected` (coinductive semantics)
    /// which implements greatest fixed point semantics - the correct behavior for
    /// recursive type checking. When depth/iteration limits are exceeded, we return
    /// `DepthExceeded` (conservative false) for soundness.
    pub fn check_subtype(&mut self, source: TypeId, target: TypeId) -> SubtypeResult {
        // =========================================================================
        // Fast paths (no cycle tracking needed)
        // =========================================================================

        let allow_any = self.any_propagation.allows_any_at_depth(self.depth);
        let mut source = source;
        let mut target = target;
        if !allow_any {
            if source == TypeId::ANY {
                source = TypeId::UNKNOWN;
            }
            if target == TypeId::ANY {
                target = TypeId::UNKNOWN;
            }
        }

        // Same type is always a subtype of itself
        if source == target {
            return SubtypeResult::True;
        }

        // Any is assignable to anything (when allowed)
        if allow_any && source == TypeId::ANY {
            return SubtypeResult::True;
        }

        // Everything is assignable to any (when allowed)
        if allow_any && target == TypeId::ANY {
            return SubtypeResult::True;
        }

        // Everything is assignable to unknown
        if target == TypeId::UNKNOWN {
            return SubtypeResult::True;
        }

        // Never is assignable to everything
        if source == TypeId::NEVER {
            return SubtypeResult::True;
        }

        // Error types are only compatible with themselves.
        // Error suppression belongs in the compatibility layer (CompatChecker),
        // not in the strict subtype engine.
        if source == TypeId::ERROR || target == TypeId::ERROR {
            return SubtypeResult::False;
        }

        // Fast path: distinct disjoint unit types are never subtypes.
        // This avoids expensive structural checks for large unions of literals/enum members.
        if is_disjoint_unit_type(self.interner, source)
            && is_disjoint_unit_type(self.interner, target)
        {
            return SubtypeResult::False;
        }

        // =========================================================================
        // Cross-checker memoization (QueryCache lookup)
        // =========================================================================
        // Check the shared cache for a previously computed result.
        // This avoids re-doing expensive structural checks for type pairs
        // already resolved by a prior SubtypeChecker instance.
        if let Some(db) = self.query_db {
            let key = self.make_cache_key(source, target);
            if let Some(cached) = db.lookup_subtype_cache(key) {
                return if cached {
                    SubtypeResult::True
                } else {
                    SubtypeResult::False
                };
            }
        }

        // =========================================================================
        // Iteration limit check (timeout prevention)
        // =========================================================================

        self.total_checks += 1;
        if self.total_checks > MAX_TOTAL_SUBTYPE_CHECKS {
            // Too many checks - likely in an infinite expansion scenario
            // Return DepthExceeded to treat as false (soundness fix)
            self.depth_exceeded = true;
            return SubtypeResult::DepthExceeded;
        }

        // =========================================================================
        // Depth Check (stack overflow prevention)
        // =========================================================================

        if self.depth > self.max_depth {
            // Recursion too deep - return DepthExceeded (treat as false for soundness)
            // This prevents incorrectly accepting unsound expansive recursive types
            // Valid finite cyclic types won't hit this limit
            self.depth_exceeded = true;
            return SubtypeResult::DepthExceeded;
        }

        // =========================================================================
        // Cycle detection FIRST (coinduction) - BEFORE evaluation!
        //
        // Critical: This must happen BEFORE evaluate_type() to catch cycles
        // in expansive types that produce fresh TypeIds on each evaluation.
        // See docs/architecture/SOLVER_REFACTORING_PROPOSAL.md Section 2.1
        // =========================================================================

        let pair = (source, target);
        if self.in_progress.contains(&pair) {
            // We're in a cycle - return provisional true
            // This implements coinductive semantics for recursive types
            return SubtypeResult::CycleDetected;
        }

        // Also check the reversed pair to detect cycles in bivariant parameter checking.
        // When checking bivariant parameters, we check both (A, B) and (B, A), which can
        // create cross-recursion that the normal cycle detection doesn't catch.
        let reversed_pair = (target, source);
        if self.in_progress.contains(&reversed_pair) {
            // We're in a cross-recursion cycle from bivariant checking
            return SubtypeResult::CycleDetected;
        }

        // Memory safety: limit the number of in-progress pairs to prevent unbounded growth
        if self.in_progress.len() >= MAX_IN_PROGRESS_PAIRS {
            // Too many pairs being tracked - likely pathological case
            // Return DepthExceeded (treat as false for soundness)
            self.depth_exceeded = true;
            return SubtypeResult::DepthExceeded;
        }

        // =======================================================================
        // DEFD-LEVEL CYCLE DETECTION (before evaluation!)
        // =======================================================================
        // This catches cycles in recursive type aliases BEFORE they expand,
        // preventing infinite recursion. For example:
        // - `type T = Box<T>` produces new TypeId on each evaluation
        // - Current in_progress check (TypeId-level) fails: T[] ≠ T
        // - DefId-level check catches: (DefId_T, DefId_T) is same pair
        //
        // CRITICAL: We only apply this check to non-generic types.
        // If the type is an Application (has type args like Box<string>),
        // we CANNOT use pure DefId equality because Box<string> ≠ Box<number>
        // even though both have DefId(Box).
        //
        // This implements coinductive semantics: assume subtypes, verify consistency.
        // =======================================================================

        // Helper to check if it's safe to use DefId cycle detection
        // Only safe if the type is NOT an Application (no generic arguments)
        let is_safe_for_defid_check = |type_id: TypeId| -> bool {
            // Check if it's an Application. If so, UNSAFE to check purely by DefId.
            application_id(self.interner, type_id).is_none()
        };

        let def_pair = if is_safe_for_defid_check(source) && is_safe_for_defid_check(target) {
            if let (Some(s_def), Some(t_def)) = (
                lazy_def_id(self.interner, source)
                    .or_else(|| enum_components(self.interner, source).map(|(def_id, _)| def_id)),
                lazy_def_id(self.interner, target)
                    .or_else(|| enum_components(self.interner, target).map(|(def_id, _)| def_id)),
            ) {
                Some((s_def, t_def))
            } else {
                None
            }
        } else {
            None
        };

        // Check for DefId-level cycles BEFORE evaluation
        let inserted_seen_defs = if let Some((s_def, t_def)) = def_pair {
            // Check forward pair
            if self.seen_defs.contains(&(s_def, t_def)) {
                // We're in a cycle at the DefId level - return CycleDetected
                // This implements coinductive semantics for recursive types
                return SubtypeResult::CycleDetected;
            }

            // Check reversed pair for bivariant cross-recursion
            if self.seen_defs.contains(&(t_def, s_def)) {
                return SubtypeResult::CycleDetected;
            }

            // Mark this DefId pair as being checked BEFORE evaluation
            self.seen_defs.insert((s_def, t_def));
            true
        } else {
            false
        };

        // Mark as in-progress BEFORE evaluation to catch expansive type cycles
        self.in_progress.insert(pair);
        self.depth += 1;

        // =========================================================================
        // Meta-type evaluation (after cycle detection is set up)
        // =========================================================================
        // Evaluate meta-types (KeyOf, Conditional, etc.)
        // Note: This happens AFTER cycle detection is set up, so expansive types
        // that produce fresh TypeIds will be caught by the cycle detection above.
        //
        // When bypass_evaluation is true (TypeEvaluator simplification mode),
        // skip evaluation to prevent infinite recursion. TypeEvaluator has already
        // evaluated all members before calling the simplifier.
        let result = if self.bypass_evaluation {
            // Skip evaluation - go straight to structural check
            if target == TypeId::NEVER {
                SubtypeResult::False
            } else {
                self.check_subtype_inner(source, target)
            }
        } else {
            let source_eval = self.evaluate_type(source);
            let target_eval = self.evaluate_type(target);

            // If evaluation changed anything, recurse with the simplified types
            // The cycle detection is already set up for the original pair
            if source_eval != source || target_eval != target {
                self.check_subtype(source_eval, target_eval)
            } else {
                // =========================================================================
                // Post-evaluation fast paths
                // =========================================================================

                // Nothing (except never) is assignable to never
                if target == TypeId::NEVER {
                    SubtypeResult::False
                } else {
                    // Do the actual structural check
                    self.check_subtype_inner(source, target)
                }
            }
        };

        // Remove from in-progress and decrement depth
        self.depth -= 1;
        self.in_progress.remove(&pair);

        // Remove from seen_defs if we inserted (DefId-level cycle cleanup)
        if inserted_seen_defs {
            if let Some((s_def, t_def)) = def_pair {
                self.seen_defs.remove(&(s_def, t_def));
            }
        }

        // Cache definitive results in the shared QueryCache for cross-checker memoization.
        // Only cache True/False, not non-definitive results (cycle detection artifacts).
        if let Some(db) = self.query_db {
            let key = self.make_cache_key(source, target);
            match result {
                SubtypeResult::True => db.insert_subtype_cache(key, true),
                SubtypeResult::False => db.insert_subtype_cache(key, false),
                SubtypeResult::CycleDetected | SubtypeResult::DepthExceeded => {} // Don't cache non-definitive results
            }
        }

        result
    }

    /// Inner subtype check (after cycle detection and type evaluation)
    fn check_subtype_inner(&mut self, source: TypeId, target: TypeId) -> SubtypeResult {
        // Types are already evaluated in check_subtype, so no need to re-evaluate here

        if !self.strict_null_checks && (source == TypeId::NULL || source == TypeId::UNDEFINED) {
            return SubtypeResult::True;
        }

        // Note: Canonicalization-based structural identity (Task #36) was previously
        // called here as a "fast path", but it was actually SLOWER than the normal path
        // because it allocated a fresh Canonicalizer per call (FxHashMap + Vecs) and
        // triggered O(n²) union reduction via interner.union(). The existing QueryCache
        // already provides O(1) memoization for repeated subtype checks.
        // The Canonicalizer remains available for its intended purpose: detecting
        // structural identity of recursive type aliases (graph isomorphism).
        // See: are_types_structurally_identical() and isomorphism_tests.rs

        // Note: Weak type checking is handled by CompatChecker (compat.rs:167-170).
        // Removed redundant check here to avoid double-checking which caused false positives.

        if let Some(shape) = self.apparent_primitive_shape_for_type(source) {
            if let Some(t_shape_id) = object_shape_id(self.interner, target) {
                let t_shape = self.interner.object_shape(t_shape_id);
                return self.check_object_subtype(&shape, None, &t_shape);
            }
            if let Some(t_shape_id) = object_with_index_shape_id(self.interner, target) {
                let t_shape = self.interner.object_shape(t_shape_id);
                return self.check_object_with_index_subtype(&shape, None, &t_shape);
            }
        }

        if let Some(source_cond_id) = conditional_type_id(self.interner, source) {
            if let Some(target_cond_id) = conditional_type_id(self.interner, target) {
                let source_cond = self.interner.conditional_type(source_cond_id);
                let target_cond = self.interner.conditional_type(target_cond_id);
                return self.check_conditional_subtype(source_cond.as_ref(), target_cond.as_ref());
            }

            let source_cond = self.interner.conditional_type(source_cond_id);
            return self.conditional_branches_subtype(source_cond.as_ref(), target);
        }

        if let Some(target_cond_id) = conditional_type_id(self.interner, target) {
            let target_cond = self.interner.conditional_type(target_cond_id);
            return self.subtype_of_conditional_target(source, target_cond.as_ref());
        }

        if let Some(members) = union_list_id(self.interner, source) {
            let member_list = self.interner.type_list(members);
            for &member in member_list.iter() {
                if !self.check_subtype(member, target).is_true() {
                    // Trace: No union member matches target
                    if let Some(tracer) = &mut self.tracer {
                        if !tracer.on_mismatch_dyn(SubtypeFailureReason::NoUnionMemberMatches {
                            source_type: source,
                            target_union_members: vec![target],
                        }) {
                            return SubtypeResult::False;
                        }
                    }
                    return SubtypeResult::False;
                }
            }
            return SubtypeResult::True;
        }

        if let Some(members) = union_list_id(self.interner, target) {
            if keyof_inner_type(self.interner, source).is_some()
                && self.is_keyof_subtype_of_string_number_symbol_union(members)
            {
                return SubtypeResult::True;
            }

            // Rule #7: Open Numeric Enums - number is assignable to unions containing numeric enums
            if source == TypeId::NUMBER {
                let member_list = self.interner.type_list(members);
                for &member in member_list.iter() {
                    if let Some(def_id) = lazy_def_id(self.interner, member) {
                        if self.resolver.is_numeric_enum(def_id) {
                            return SubtypeResult::True;
                        }
                    }
                }
            }

            let member_list = self.interner.type_list(members);

            // Fast path: TypeId equality pre-scan before expensive structural checks.
            // If source has the same TypeId as any union member, it's trivially a subtype.
            // This avoids O(n × cost) structural comparisons when the match is by identity.
            for &member in member_list.iter() {
                if source == member {
                    return SubtypeResult::True;
                }
            }

            for &member in member_list.iter() {
                if self.check_subtype(source, member).is_true() {
                    return SubtypeResult::True;
                }
            }
            // Trace: Source is not a subtype of any union member
            if let Some(tracer) = &mut self.tracer {
                if !tracer.on_mismatch_dyn(SubtypeFailureReason::NoUnionMemberMatches {
                    source_type: source,
                    target_union_members: member_list.iter().copied().collect(),
                }) {
                    return SubtypeResult::False;
                }
            }
            return SubtypeResult::False;
        }

        if let Some(members) = intersection_list_id(self.interner, source) {
            let member_list = self.interner.type_list(members);

            for &member in member_list.iter() {
                if self.check_subtype(member, target).is_true() {
                    return SubtypeResult::True;
                }
            }

            if object_shape_id(self.interner, target).is_some()
                || object_with_index_shape_id(self.interner, target).is_some()
            {
                // Use PropertyCollector to merge all properties from intersection members
                // This handles Lazy/Ref resolution and avoids infinite recursion
                use crate::solver::objects::{PropertyCollectionResult, collect_properties};

                match collect_properties(source, self.interner, self.resolver) {
                    PropertyCollectionResult::Any => {
                        // any & T = any, so check if any is subtype of target
                        return self.check_subtype(TypeId::ANY, target);
                    }
                    PropertyCollectionResult::NonObject => {
                        // No object properties to check, fall through to other checks
                    }
                    PropertyCollectionResult::Properties {
                        properties,
                        string_index,
                        number_index,
                    } => {
                        if !properties.is_empty()
                            || string_index.is_some()
                            || number_index.is_some()
                        {
                            let merged_type = if string_index.is_some() || number_index.is_some() {
                                self.interner.object_with_index(ObjectShape {
                                    flags: ObjectFlags::empty(),
                                    properties,
                                    string_index,
                                    number_index,
                                    symbol: None,
                                })
                            } else {
                                self.interner.object(properties)
                            };
                            if self.check_subtype(merged_type, target).is_true() {
                                return SubtypeResult::True;
                            }
                        }
                    }
                }
            }

            return SubtypeResult::False;
        }

        if let Some(members) = intersection_list_id(self.interner, target) {
            let member_list = self.interner.type_list(members);
            for &member in member_list.iter() {
                if !self.check_subtype(source, member).is_true() {
                    return SubtypeResult::False;
                }
            }
            return SubtypeResult::True;
        }

        if let (Some(s_kind), Some(t_kind)) = (
            intrinsic_kind(self.interner, source),
            intrinsic_kind(self.interner, target),
        ) {
            return self.check_intrinsic_subtype(s_kind, t_kind);
        }

        // Type parameter checks BEFORE boxed primitive check
        // Unconstrained type parameters should be handled before other checks
        if let Some(s_info) = type_param_info(self.interner, source) {
            return self.check_type_parameter_subtype(&s_info, target);
        }

        if let Some(_t_info) = type_param_info(self.interner, target) {
            // A concrete type is never a subtype of an opaque type parameter.
            // The type parameter T could be instantiated as any type satisfying its constraint,
            // so we cannot guarantee that source <: T unless source is never/any (handled above).
            //
            // This is the correct TypeScript behavior:
            // - "hello" is NOT assignable to T extends string (T could be "world")
            // - { value: number } is NOT assignable to unconstrained T (T defaults to unknown)
            //
            // Note: When the type parameter is the SOURCE (e.g., T <: string), we check
            // against its constraint. But as TARGET, we return False.

            // Trace: Concrete type not assignable to type parameter
            if let Some(tracer) = &mut self.tracer {
                if !tracer.on_mismatch_dyn(SubtypeFailureReason::TypeMismatch {
                    source_type: source,
                    target_type: target,
                }) {
                    return SubtypeResult::False;
                }
            }
            return SubtypeResult::False;
        }

        if let Some(s_kind) = intrinsic_kind(self.interner, source) {
            if self.is_boxed_primitive_subtype(s_kind, target) {
                return SubtypeResult::True;
            } else {
                // Trace: Intrinsic type mismatch (boxed primitive check failed)
                if let Some(tracer) = &mut self.tracer {
                    if !tracer.on_mismatch_dyn(SubtypeFailureReason::TypeMismatch {
                        source_type: source,
                        target_type: target,
                    }) {
                        return SubtypeResult::False;
                    }
                }
                return SubtypeResult::False;
            }
        }

        if let (Some(lit), Some(t_kind)) = (
            literal_value(self.interner, source),
            intrinsic_kind(self.interner, target),
        ) {
            return self.check_literal_to_intrinsic(&lit, t_kind);
        }

        if let (Some(s_lit), Some(t_lit)) = (
            literal_value(self.interner, source),
            literal_value(self.interner, target),
        ) {
            if s_lit == t_lit {
                return SubtypeResult::True;
            } else {
                // Trace: Literal type mismatch
                if let Some(tracer) = &mut self.tracer {
                    if !tracer.on_mismatch_dyn(SubtypeFailureReason::LiteralTypeMismatch {
                        source_type: source,
                        target_type: target,
                    }) {
                        return SubtypeResult::False;
                    }
                }
                return SubtypeResult::False;
            }
        }

        if let (Some(LiteralValue::String(s_lit)), Some(t_spans)) = (
            literal_value(self.interner, source),
            template_literal_id(self.interner, target),
        ) {
            return self.check_literal_matches_template_literal(s_lit, t_spans);
        }

        if intrinsic_kind(self.interner, target) == Some(IntrinsicKind::Object) {
            if self.is_object_keyword_type(source) {
                return SubtypeResult::True;
            } else {
                // Trace: Source is not object-compatible
                if let Some(tracer) = &mut self.tracer {
                    if !tracer.on_mismatch_dyn(SubtypeFailureReason::TypeMismatch {
                        source_type: source,
                        target_type: target,
                    }) {
                        return SubtypeResult::False;
                    }
                }
                return SubtypeResult::False;
            }
        }

        if intrinsic_kind(self.interner, target) == Some(IntrinsicKind::Function) {
            if self.is_callable_type(source) {
                return SubtypeResult::True;
            } else {
                // Trace: Source is not function-compatible
                if let Some(tracer) = &mut self.tracer {
                    if !tracer.on_mismatch_dyn(SubtypeFailureReason::TypeMismatch {
                        source_type: source,
                        target_type: target,
                    }) {
                        return SubtypeResult::False;
                    }
                }
                return SubtypeResult::False;
            }
        }

        if let (Some(s_elem), Some(t_elem)) = (
            array_element_type(self.interner, source),
            array_element_type(self.interner, target),
        ) {
            return self.check_subtype(s_elem, t_elem);
        }

        if let (Some(s_elems), Some(t_elems)) = (
            tuple_list_id(self.interner, source),
            tuple_list_id(self.interner, target),
        ) {
            // OPTIMIZATION: Unit-tuple disjointness fast-path (O(1) cached lookup)
            // Two different unit tuples (tuples of literals/enums only) are guaranteed disjoint.
            // Since we already checked source == target at the top and returned True,
            // reaching here means source != target. If both are unit tuples, they're disjoint.
            // This avoids O(N) structural recursion for each comparison in BCT's O(N²) loop.
            if self.interner.is_unit_type(source) && self.interner.is_unit_type(target) {
                return SubtypeResult::False;
            }
            let s_elems = self.interner.tuple_list(s_elems);
            let t_elems = self.interner.tuple_list(t_elems);
            return self.check_tuple_subtype(&s_elems, &t_elems);
        }

        if let (Some(s_elems), Some(t_elem)) = (
            tuple_list_id(self.interner, source),
            array_element_type(self.interner, target),
        ) {
            return self.check_tuple_to_array_subtype(s_elems, t_elem);
        }

        if let (Some(s_elem), Some(t_elems)) = (
            array_element_type(self.interner, source),
            tuple_list_id(self.interner, target),
        ) {
            let t_elems = self.interner.tuple_list(t_elems);
            return self.check_array_to_tuple_subtype(s_elem, &t_elems);
        }

        if let (Some(s_shape_id), Some(t_shape_id)) = (
            object_shape_id(self.interner, source),
            object_shape_id(self.interner, target),
        ) {
            let s_shape = self.interner.object_shape(s_shape_id);
            let t_shape = self.interner.object_shape(t_shape_id);
            return self.check_object_subtype(&s_shape, Some(s_shape_id), &t_shape);
        }

        if let (Some(s_shape_id), Some(t_shape_id)) = (
            object_with_index_shape_id(self.interner, source),
            object_with_index_shape_id(self.interner, target),
        ) {
            let s_shape = self.interner.object_shape(s_shape_id);
            let t_shape = self.interner.object_shape(t_shape_id);
            return self.check_object_with_index_subtype(&s_shape, Some(s_shape_id), &t_shape);
        }

        // Nominal type checking for class instances
        // Before structural checks, verify that classes with different symbols have proper inheritance relationship
        if let (Some(s_shape_id), Some(t_shape_id)) = (
            object_with_index_shape_id(self.interner, source),
            object_with_index_shape_id(self.interner, target),
        ) {
            let s_shape = self.interner.object_shape(s_shape_id);
            let t_shape = self.interner.object_shape(t_shape_id);

            // If both have nominal identity (class symbols), check inheritance relationship
            if let (Some(_s_sym), Some(_t_sym)) = (s_shape.symbol, t_shape.symbol) {
                // Both have symbols - they're both class instances
                // Check if source extends target through nominal inheritance
                let source_extends_target = self.check_nominal_inheritance(source, target);
                if !source_extends_target {
                    return SubtypeResult::False;
                }
                // Valid inheritance - continue to structural check below
            }
        }

        if let (Some(s_shape_id), Some(t_shape_id)) = (
            object_with_index_shape_id(self.interner, source),
            object_shape_id(self.interner, target),
        ) {
            let s_shape = self.interner.object_shape(s_shape_id);
            let t_shape = self.interner.object_shape(t_shape_id);
            return self.check_object_with_index_to_object(
                &s_shape,
                s_shape_id,
                &t_shape.properties,
            );
        }

        if let (Some(s_shape_id), Some(t_shape_id)) = (
            object_shape_id(self.interner, source),
            object_with_index_shape_id(self.interner, target),
        ) {
            let s_shape = self.interner.object_shape(s_shape_id);
            let t_shape = self.interner.object_shape(t_shape_id);
            return self.check_object_to_indexed(&s_shape.properties, Some(s_shape_id), &t_shape);
        }

        if let (Some(s_fn_id), Some(t_fn_id)) = (
            function_shape_id(self.interner, source),
            function_shape_id(self.interner, target),
        ) {
            let s_fn = self.interner.function_shape(s_fn_id);
            let t_fn = self.interner.function_shape(t_fn_id);
            return self.check_function_subtype(&s_fn, &t_fn);
        }

        if let (Some(s_callable_id), Some(t_callable_id)) = (
            callable_shape_id(self.interner, source),
            callable_shape_id(self.interner, target),
        ) {
            let s_callable = self.interner.callable_shape(s_callable_id);
            let t_callable = self.interner.callable_shape(t_callable_id);
            return self.check_callable_subtype(&s_callable, &t_callable);
        }

        if let (Some(s_fn_id), Some(t_callable_id)) = (
            function_shape_id(self.interner, source),
            callable_shape_id(self.interner, target),
        ) {
            return self.check_function_to_callable_subtype(s_fn_id, t_callable_id);
        }

        if let (Some(s_callable_id), Some(t_fn_id)) = (
            callable_shape_id(self.interner, source),
            function_shape_id(self.interner, target),
        ) {
            return self.check_callable_to_function_subtype(s_callable_id, t_fn_id);
        }

        if let (Some(s_app_id), Some(t_app_id)) = (
            application_id(self.interner, source),
            application_id(self.interner, target),
        ) {
            return self.check_application_to_application_subtype(s_app_id, t_app_id);
        }

        if let Some(app_id) = application_id(self.interner, source) {
            return self.check_application_expansion_target(source, target, app_id);
        }

        if let Some(app_id) = application_id(self.interner, target) {
            return self.check_source_to_application_expansion(source, target, app_id);
        }

        if let Some(mapped_id) = mapped_type_id(self.interner, source) {
            return self.check_mapped_expansion_target(source, target, mapped_id);
        }

        if let Some(mapped_id) = mapped_type_id(self.interner, target) {
            return self.check_source_to_mapped_expansion(source, target, mapped_id);
        }

        // =======================================================================
        // ENUM TYPE CHECKING (Nominal Identity)
        // =======================================================================
        // Enums are nominal types - two different enums with the same member types
        // are NOT compatible. Enum(DefId, MemberType) preserves both:
        // - DefId: For nominal identity (E1 != E2)
        // - MemberType: For structural assignability to primitives (E1 <: number)
        // =======================================================================

        if let (Some((s_def_id, _s_members)), Some((t_def_id, _t_members))) = (
            enum_components(self.interner, source),
            enum_components(self.interner, target),
        ) {
            // Enum to Enum: Nominal check - DefIds must match
            if s_def_id == t_def_id {
                return SubtypeResult::True;
            }

            // Check for member-to-parent relationship (e.g., E.A -> E)
            // If source is a member of the target enum, it is a subtype
            if self.resolver.get_enum_parent_def_id(s_def_id) == Some(t_def_id) {
                // Source is a member of target enum
                // Only allow if target is the full enum type (not a different member)
                if self.resolver.is_enum_type(target, self.interner) {
                    return SubtypeResult::True;
                }
            }

            // Different enums are NOT compatible (nominal typing)
            // Trace: Enum nominal mismatch
            if let Some(tracer) = &mut self.tracer {
                if !tracer.on_mismatch_dyn(SubtypeFailureReason::TypeMismatch {
                    source_type: source,
                    target_type: target,
                }) {
                    return SubtypeResult::False;
                }
            }
            return SubtypeResult::False;
        }

        // Source is Enum, Target is not - check structural member type
        if let Some((_s_def_id, s_members)) = enum_components(self.interner, source) {
            return self.check_subtype(s_members, target);
        }

        // Target is Enum, Source is not - check structural member type
        if let Some((_t_def_id, t_members)) = enum_components(self.interner, target) {
            return self.check_subtype(source, t_members);
        }

        // =======================================================================
        // PHASE 3.2: PRIORITIZE DefId (Lazy) OVER SymbolRef (Ref)
        // =======================================================================
        // We now check Lazy(DefId) types before Ref(SymbolRef) types to establish
        // DefId as the primary type identity system. The InheritanceGraph bridge
        // enables Lazy types to use O(1) nominal subtype checking.
        // =======================================================================

        if let (Some(s_def), Some(t_def)) = (
            lazy_def_id(self.interner, source),
            lazy_def_id(self.interner, target),
        ) {
            // Phase 3.1: Use proper DefId-level cycle detection
            // Phase 3.2: Now checked before Ref types (priority)
            return self.check_lazy_lazy_subtype(source, target, &s_def, &t_def);
        }

        // =======================================================================
        // Rule #7: Open Numeric Enums - Number <-> Numeric Enum Assignability
        // =======================================================================
        // In TypeScript, numeric enums are "open" - they allow bidirectional
        // assignability with the number type. This is unsound but matches tsc behavior.
        // See docs/specs/TS_UNSOUNDNESS_CATALOG.md Item #7.

        // Helper to extract DefId from Enum or Lazy types
        let get_enum_def_id = |type_id: TypeId| -> Option<DefId> {
            match self.interner.lookup(type_id) {
                Some(TypeKey::Enum(def_id, _)) => Some(def_id),
                Some(TypeKey::Lazy(def_id)) => Some(def_id),
                _ => None,
            }
        };

        // Check: source is numeric enum, target is Number
        if let Some(s_def) = get_enum_def_id(source) {
            if target == TypeId::NUMBER && self.resolver.is_numeric_enum(s_def) {
                return SubtypeResult::True;
            }
        }

        // Check: source is Number (or numeric literal), target is numeric enum
        if let Some(t_def) = get_enum_def_id(target) {
            if source == TypeId::NUMBER && self.resolver.is_numeric_enum(t_def) {
                return SubtypeResult::True;
            }
            // Also check for numeric literals (subtypes of number)
            if matches!(
                self.interner.lookup(source),
                Some(TypeKey::Literal(LiteralValue::Number(_)))
            ) {
                if self.resolver.is_numeric_enum(t_def) {
                    // For numeric literals, we need to check if they're assignable to the enum
                    // Fall through to structural check (e.g., 0 -> E.A might succeed if E.A = 0)
                    return self.check_subtype(source, self.resolve_lazy_type(target));
                }
            }
        }

        if lazy_def_id(self.interner, source).is_some() {
            let resolved = self.resolve_lazy_type(source);
            return if resolved != source {
                self.check_subtype(resolved, target)
            } else {
                SubtypeResult::False
            };
        }

        if lazy_def_id(self.interner, target).is_some() {
            let resolved = self.resolve_lazy_type(target);
            return if resolved != target {
                self.check_subtype(source, resolved)
            } else {
                SubtypeResult::False
            };
        }

        // =======================================================================
        // Ref(SymbolRef) checks - now secondary to Lazy(DefId)
        // =======================================================================

        if let (Some(s_sym), Some(t_sym)) = (
            ref_symbol(self.interner, source),
            ref_symbol(self.interner, target),
        ) {
            return self.check_ref_ref_subtype(source, target, &s_sym, &t_sym);
        }

        if let Some(s_sym) = ref_symbol(self.interner, source) {
            return self.check_ref_subtype(source, target, &s_sym);
        }

        if let Some(t_sym) = ref_symbol(self.interner, target) {
            return self.check_to_ref_subtype(source, target, &t_sym);
        }

        if let (Some(s_sym), Some(t_sym)) = (
            type_query_symbol(self.interner, source),
            type_query_symbol(self.interner, target),
        ) {
            return self.check_typequery_typequery_subtype(source, target, &s_sym, &t_sym);
        }

        if let Some(s_sym) = type_query_symbol(self.interner, source) {
            return self.check_typequery_subtype(source, target, &s_sym);
        }

        if let Some(t_sym) = type_query_symbol(self.interner, target) {
            return self.check_to_typequery_subtype(source, target, &t_sym);
        }

        if let (Some(s_inner), Some(t_inner)) = (
            keyof_inner_type(self.interner, source),
            keyof_inner_type(self.interner, target),
        ) {
            return self.check_subtype(t_inner, s_inner);
        }

        if let (Some(s_inner), Some(t_inner)) = (
            readonly_inner_type(self.interner, source),
            readonly_inner_type(self.interner, target),
        ) {
            return self.check_subtype(s_inner, t_inner);
        }

        // Readonly target peeling: T <: Readonly<U> if T <: U
        // A mutable type can always be treated as readonly (readonly is a supertype)
        // CRITICAL: Only peel if source is NOT Readonly. If source IS Readonly, we must
        // fall through to the visitor to compare Readonly<S> vs Readonly<T>.
        if let Some(t_inner) = readonly_inner_type(self.interner, target) {
            if readonly_inner_type(self.interner, source).is_none() {
                return self.check_subtype(source, t_inner);
            }
        }

        // Readonly source to mutable target case is handled by SubtypeVisitor::visit_readonly_type
        // which returns False (correctly, because Readonly is not assignable to Mutable)

        if let (Some(s_sym), Some(t_sym)) = (
            unique_symbol_ref(self.interner, source),
            unique_symbol_ref(self.interner, target),
        ) {
            return if s_sym == t_sym {
                SubtypeResult::True
            } else {
                SubtypeResult::False
            };
        }

        if unique_symbol_ref(self.interner, source).is_some()
            && intrinsic_kind(self.interner, target) == Some(IntrinsicKind::Symbol)
        {
            return SubtypeResult::True;
        }

        if is_this_type(self.interner, source) && is_this_type(self.interner, target) {
            return SubtypeResult::True;
        }

        if let (Some(s_spans), Some(t_spans)) = (
            template_literal_id(self.interner, source),
            template_literal_id(self.interner, target),
        ) {
            if s_spans == t_spans {
                return SubtypeResult::True;
            }
            let s_list = self.interner.template_list(s_spans);
            let t_list = self.interner.template_list(t_spans);
            if s_list.len() != t_list.len() {
                // Trace: Template literal length mismatch
                if let Some(tracer) = &mut self.tracer {
                    if !tracer.on_mismatch_dyn(SubtypeFailureReason::TypeMismatch {
                        source_type: source,
                        target_type: target,
                    }) {
                        return SubtypeResult::False;
                    }
                }
                return SubtypeResult::False;
            }
            for (s_span, t_span) in s_list.iter().zip(t_list.iter()) {
                match (s_span, t_span) {
                    (TemplateSpan::Text(s_text), TemplateSpan::Text(t_text)) => {
                        if s_text != t_text {
                            // Trace: Template literal text part mismatch
                            if let Some(tracer) = &mut self.tracer {
                                if !tracer.on_mismatch_dyn(SubtypeFailureReason::TypeMismatch {
                                    source_type: source,
                                    target_type: target,
                                }) {
                                    return SubtypeResult::False;
                                }
                            }
                            return SubtypeResult::False;
                        }
                    }
                    (TemplateSpan::Type(s_type), TemplateSpan::Type(t_type)) => {
                        if !self.check_subtype(*s_type, *t_type).is_true() {
                            return SubtypeResult::False;
                        }
                    }
                    _ => {
                        // Trace: Template literal span kind mismatch
                        if let Some(tracer) = &mut self.tracer {
                            if !tracer.on_mismatch_dyn(SubtypeFailureReason::TypeMismatch {
                                source_type: source,
                                target_type: target,
                            }) {
                                return SubtypeResult::False;
                            }
                        }
                        return SubtypeResult::False;
                    }
                }
            }
            return SubtypeResult::True;
        }

        if template_literal_id(self.interner, source).is_some()
            && intrinsic_kind(self.interner, target) == Some(IntrinsicKind::String)
        {
            return SubtypeResult::True;
        }

        let source_is_callable = function_shape_id(self.interner, source).is_some()
            || callable_shape_id(self.interner, source).is_some();
        if source_is_callable {
            if let Some(t_shape_id) = object_shape_id(self.interner, target) {
                let t_shape = self.interner.object_shape(t_shape_id);
                if t_shape.properties.is_empty() {
                    return SubtypeResult::True;
                } else {
                    // Trace: Callable not assignable to object with non-empty properties
                    if let Some(tracer) = &mut self.tracer {
                        if !tracer.on_mismatch_dyn(SubtypeFailureReason::TypeMismatch {
                            source_type: source,
                            target_type: target,
                        }) {
                            return SubtypeResult::False;
                        }
                    }
                    return SubtypeResult::False;
                }
            }
            if let Some(t_shape_id) = object_with_index_shape_id(self.interner, target) {
                let t_shape = self.interner.object_shape(t_shape_id);
                if t_shape.properties.is_empty() {
                    return SubtypeResult::True;
                } else {
                    // Trace: Callable not assignable to indexed object with non-empty properties
                    if let Some(tracer) = &mut self.tracer {
                        if !tracer.on_mismatch_dyn(SubtypeFailureReason::TypeMismatch {
                            source_type: source,
                            target_type: target,
                        }) {
                            return SubtypeResult::False;
                        }
                    }
                    return SubtypeResult::False;
                }
            }
        }

        let source_is_array_or_tuple = array_element_type(self.interner, source).is_some()
            || tuple_list_id(self.interner, source).is_some();
        if source_is_array_or_tuple {
            if let Some(t_shape_id) = object_shape_id(self.interner, target) {
                let t_shape = self.interner.object_shape(t_shape_id);
                if t_shape.properties.is_empty() {
                    return SubtypeResult::True;
                }
                let mut all_ok = true;
                for t_prop in &t_shape.properties {
                    let prop_name = self.interner.resolve_atom(t_prop.name);
                    if prop_name == "length" {
                        if !self.check_subtype(TypeId::NUMBER, t_prop.type_id).is_true() {
                            all_ok = false;
                            break;
                        }
                    } else {
                        all_ok = false;
                        break;
                    }
                }
                if all_ok {
                    return SubtypeResult::True;
                } else {
                    // Trace: Array/tuple not compatible with object
                    if let Some(tracer) = &mut self.tracer {
                        if !tracer.on_mismatch_dyn(SubtypeFailureReason::TypeMismatch {
                            source_type: source,
                            target_type: target,
                        }) {
                            return SubtypeResult::False;
                        }
                    }
                    return SubtypeResult::False;
                }
            }
            if let Some(t_shape_id) = object_with_index_shape_id(self.interner, target) {
                let t_shape = self.interner.object_shape(t_shape_id);
                if t_shape.properties.is_empty() {
                    if let Some(ref num_idx) = t_shape.number_index {
                        let elem_type =
                            array_element_type(self.interner, source).unwrap_or(TypeId::ANY);
                        if !self.check_subtype(elem_type, num_idx.value_type).is_true() {
                            // Trace: Array element type mismatch with index signature
                            if let Some(tracer) = &mut self.tracer {
                                if !tracer.on_mismatch_dyn(
                                    SubtypeFailureReason::IndexSignatureMismatch {
                                        index_kind: "number",
                                        source_value_type: elem_type,
                                        target_value_type: num_idx.value_type,
                                    },
                                ) {
                                    return SubtypeResult::False;
                                }
                            }
                            return SubtypeResult::False;
                        }
                    }
                    return SubtypeResult::True;
                }
                // Trace: Array/tuple not compatible with indexed object with non-empty properties
                if let Some(tracer) = &mut self.tracer {
                    if !tracer.on_mismatch_dyn(SubtypeFailureReason::TypeMismatch {
                        source_type: source,
                        target_type: target,
                    }) {
                        return SubtypeResult::False;
                    }
                }
                return SubtypeResult::False;
            }
        }

        // =======================================================================
        // VISITOR PATTERN DISPATCH (Task #48.4)
        // =======================================================================
        // After all special-case checks above, dispatch to the visitor for
        // general structural type checking. The visitor implements double-
        // dispatch pattern to handle source type variants and their interaction
        // with the target type.
        // =======================================================================

        // Extract the interner reference FIRST (Copy trait)
        // This must happen before creating the visitor which mutably borrows self
        let interner = self.interner;

        // Create the visitor with a mutable reborrow of self
        let mut visitor = SubtypeVisitor {
            checker: self,
            source,
            target,
        };

        // Dispatch to the visitor using the extracted interner
        let result = visitor.visit_type(interner, source);

        // Trace: Generic fallback type mismatch (no specific reason matched above)
        if result == SubtypeResult::False {
            if let Some(tracer) = &mut self.tracer {
                if !tracer.on_mismatch_dyn(SubtypeFailureReason::TypeMismatch {
                    source_type: source,
                    target_type: target,
                }) {
                    return SubtypeResult::False;
                }
            }
        }

        result
    }

    /// Check if a deferred keyof type is a subtype of string | number | symbol.
    /// This handles the case where `keyof T` (T is a type parameter) should be
    /// considered a subtype of `string | number | symbol` because in TypeScript,
    /// keyof always produces a subtype of those three types.
    fn is_keyof_subtype_of_string_number_symbol_union(&self, members: TypeListId) -> bool {
        let member_list = self.interner.type_list(members);
        // Check if the union contains string, number, and symbol
        let mut has_string = false;
        let mut has_number = false;
        let mut has_symbol = false;
        for &member in member_list.iter() {
            if member == TypeId::STRING {
                has_string = true;
            } else if member == TypeId::NUMBER {
                has_number = true;
            } else if member == TypeId::SYMBOL {
                has_symbol = true;
            }
        }
        has_string && has_number && has_symbol
    }

    /// Check if source type extends target type through nominal class inheritance.
    ///
    /// This implements nominal type checking for class instances. If two class instances
    /// have different nominal identities, they are only compatible if one extends the
    /// other through the class hierarchy (not just structural similarity).
    ///
    /// # Arguments
    /// * `source` - TypeId of the source class instance
    /// * `target` - TypeId of the target class instance
    ///
    /// # Returns
    /// * `true` if source extends target (directly or through inheritance chain)
    /// * `false` if source does not extend target
    fn check_nominal_inheritance(&self, source: TypeId, target: TypeId) -> bool {
        use crate::solver::visitor::object_with_index_shape_id;

        // Check if target has nominal identity (is a class instance)
        let target_has_symbol =
            if let Some(target_shape_id) = object_with_index_shape_id(self.interner, target) {
                let target_shape = self.interner.object_shape(target_shape_id);
                target_shape.symbol.is_some()
            } else {
                false
            };

        // If target doesn't have nominal identity, use structural typing
        if !target_has_symbol {
            return true;
        }

        // Target has nominal identity - walk source's inheritance chain to see if we reach target
        let mut current_type = source;
        let mut visited = vec![current_type];

        loop {
            // Check if we've reached the target
            if current_type == target {
                return true;
            }

            // Get base class using TypeResolver's get_base_type
            if let Some(base_type) = self.resolver.get_base_type(current_type, self.interner) {
                // Prevent infinite loops in case of circular inheritance
                if visited.contains(&base_type) {
                    break;
                }
                visited.push(base_type);
                current_type = base_type;
            } else {
                // No more base classes in the chain
                break;
            }
        }

        false
    }
}

// =============================================================================
// Error Explanation API
// =============================================================================

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    /// Explain why `source` is not assignable to `target`.
    ///
    /// This is the "slow path" - called only when `is_assignable_to` returns false
    /// and we need to generate an error message. Re-runs the subtype logic with
    /// tracing enabled to produce a structured failure reason.
    ///
    /// Returns `None` if the types are actually compatible (shouldn't happen
    /// if called correctly after a failed check).
    pub fn explain_failure(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<SubtypeFailureReason> {
        // Fast path: if types are equal, no failure
        if source == target {
            return None;
        }

        if !self.strict_null_checks && (source == TypeId::NULL || source == TypeId::UNDEFINED) {
            return None;
        }

        // Check for any/unknown/never special cases
        if source == TypeId::ANY || target == TypeId::ANY || target == TypeId::UNKNOWN {
            return None;
        }
        if source == TypeId::NEVER {
            return None;
        }
        // ERROR types should produce ErrorType failure reason
        if source == TypeId::ERROR || target == TypeId::ERROR {
            return Some(SubtypeFailureReason::ErrorType {
                source_type: source,
                target_type: target,
            });
        }

        // Note: Weak type checking is handled by CompatChecker (compat.rs:167-170).
        // Removed redundant check here to avoid double-checking which caused false positives.

        self.explain_failure_inner(source, target)
    }

    fn explain_failure_inner(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<SubtypeFailureReason> {
        if let Some(shape) = self.apparent_primitive_shape_for_type(source) {
            if let Some(t_shape_id) = object_shape_id(self.interner, target) {
                let t_shape = self.interner.object_shape(t_shape_id);
                return self.explain_object_failure(
                    source,
                    target,
                    &shape.properties,
                    None,
                    &t_shape.properties,
                );
            }
            if let Some(t_shape_id) = object_with_index_shape_id(self.interner, target) {
                let t_shape = self.interner.object_shape(t_shape_id);
                return self.explain_indexed_object_failure(source, target, &shape, None, &t_shape);
            }
        }

        if let (Some(s_shape_id), Some(t_shape_id)) = (
            object_shape_id(self.interner, source),
            object_shape_id(self.interner, target),
        ) {
            let s_shape = self.interner.object_shape(s_shape_id);
            let t_shape = self.interner.object_shape(t_shape_id);
            return self.explain_object_failure(
                source,
                target,
                &s_shape.properties,
                Some(s_shape_id),
                &t_shape.properties,
            );
        }

        if let (Some(s_shape_id), Some(t_shape_id)) = (
            object_with_index_shape_id(self.interner, source),
            object_with_index_shape_id(self.interner, target),
        ) {
            let s_shape = self.interner.object_shape(s_shape_id);
            let t_shape = self.interner.object_shape(t_shape_id);
            return self.explain_indexed_object_failure(
                source,
                target,
                &s_shape,
                Some(s_shape_id),
                &t_shape,
            );
        }

        if let (Some(s_shape_id), Some(t_shape_id)) = (
            object_with_index_shape_id(self.interner, source),
            object_shape_id(self.interner, target),
        ) {
            let s_shape = self.interner.object_shape(s_shape_id);
            let t_shape = self.interner.object_shape(t_shape_id);
            return self.explain_object_with_index_to_object_failure(
                source,
                target,
                &s_shape,
                s_shape_id,
                &t_shape.properties,
            );
        }

        if let (Some(s_shape_id), Some(t_shape_id)) = (
            object_shape_id(self.interner, source),
            object_with_index_shape_id(self.interner, target),
        ) {
            let s_shape = self.interner.object_shape(s_shape_id);
            let t_shape = self.interner.object_shape(t_shape_id);
            if let Some(reason) = self.explain_object_failure(
                source,
                target,
                &s_shape.properties,
                Some(s_shape_id),
                &t_shape.properties,
            ) {
                return Some(reason);
            }
            if let Some(ref string_idx) = t_shape.string_index {
                for prop in &s_shape.properties {
                    let prop_type = self.optional_property_type(prop);
                    if !self
                        .check_subtype(prop_type, string_idx.value_type)
                        .is_true()
                    {
                        return Some(SubtypeFailureReason::IndexSignatureMismatch {
                            index_kind: "string",
                            source_value_type: prop_type,
                            target_value_type: string_idx.value_type,
                        });
                    }
                }
            }
            return None;
        }

        if let (Some(s_fn_id), Some(t_fn_id)) = (
            function_shape_id(self.interner, source),
            function_shape_id(self.interner, target),
        ) {
            let s_fn = self.interner.function_shape(s_fn_id);
            let t_fn = self.interner.function_shape(t_fn_id);
            return self.explain_function_failure(&s_fn, &t_fn);
        }

        if let (Some(s_elem), Some(t_elem)) = (
            array_element_type(self.interner, source),
            array_element_type(self.interner, target),
        ) {
            if !self.check_subtype(s_elem, t_elem).is_true() {
                return Some(SubtypeFailureReason::ArrayElementMismatch {
                    source_element: s_elem,
                    target_element: t_elem,
                });
            }
            return None;
        }

        if let (Some(s_elems), Some(t_elems)) = (
            tuple_list_id(self.interner, source),
            tuple_list_id(self.interner, target),
        ) {
            let s_elems = self.interner.tuple_list(s_elems);
            let t_elems = self.interner.tuple_list(t_elems);
            return self.explain_tuple_failure(&s_elems, &t_elems);
        }

        if let Some(members) = union_list_id(self.interner, target) {
            let members = self.interner.type_list(members);
            return Some(SubtypeFailureReason::NoUnionMemberMatches {
                source_type: source,
                target_union_members: members.as_ref().to_vec(),
            });
        }

        if let (Some(s_kind), Some(t_kind)) = (
            intrinsic_kind(self.interner, source),
            intrinsic_kind(self.interner, target),
        ) {
            if s_kind != t_kind {
                return Some(SubtypeFailureReason::IntrinsicTypeMismatch {
                    source_type: source,
                    target_type: target,
                });
            }
            return None;
        }

        if literal_value(self.interner, source).is_some()
            && literal_value(self.interner, target).is_some()
        {
            return Some(SubtypeFailureReason::LiteralTypeMismatch {
                source_type: source,
                target_type: target,
            });
        }

        if let (Some(lit), Some(t_kind)) = (
            literal_value(self.interner, source),
            intrinsic_kind(self.interner, target),
        ) {
            let compatible = match lit {
                LiteralValue::String(_) => t_kind == IntrinsicKind::String,
                LiteralValue::Number(_) => t_kind == IntrinsicKind::Number,
                LiteralValue::BigInt(_) => t_kind == IntrinsicKind::Bigint,
                LiteralValue::Boolean(_) => t_kind == IntrinsicKind::Boolean,
            };
            if !compatible {
                return Some(SubtypeFailureReason::LiteralTypeMismatch {
                    source_type: source,
                    target_type: target,
                });
            }
            return None;
        }

        if intrinsic_kind(self.interner, source).is_some()
            && literal_value(self.interner, target).is_some()
        {
            return Some(SubtypeFailureReason::TypeMismatch {
                source_type: source,
                target_type: target,
            });
        }

        Some(SubtypeFailureReason::TypeMismatch {
            source_type: source,
            target_type: target,
        })
    }

    /// Explain why an object type assignment failed.
    fn explain_object_failure(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_props: &[PropertyInfo],
        source_shape_id: Option<ObjectShapeId>,
        target_props: &[PropertyInfo],
    ) -> Option<SubtypeFailureReason> {
        for t_prop in target_props {
            let s_prop = self.lookup_property(source_props, source_shape_id, t_prop.name);

            match s_prop {
                Some(sp) => {
                    // Check nominal identity for private/protected properties
                    // Private and protected members are nominally typed - they must
                    // originate from the same declaration (same parent_id)
                    if t_prop.visibility != Visibility::Public {
                        if sp.parent_id != t_prop.parent_id {
                            return Some(SubtypeFailureReason::PropertyNominalMismatch {
                                property_name: t_prop.name,
                            });
                        }
                    }
                    // Cannot assign private/protected source to public target
                    else if sp.visibility != Visibility::Public {
                        return Some(SubtypeFailureReason::PropertyVisibilityMismatch {
                            property_name: t_prop.name,
                            source_visibility: sp.visibility,
                            target_visibility: t_prop.visibility,
                        });
                    }

                    // Check optional/required mismatch
                    if sp.optional && !t_prop.optional {
                        return Some(SubtypeFailureReason::OptionalPropertyRequired {
                            property_name: t_prop.name,
                        });
                    }
                    // NOTE: TypeScript allows readonly source to satisfy mutable target
                    // (readonly is a constraint on the reference, not structural compatibility)

                    // Check property type compatibility
                    let source_type = self.optional_property_type(sp);
                    let target_type = self.optional_property_type(t_prop);
                    let allow_bivariant = sp.is_method || t_prop.is_method;
                    if !self
                        .check_subtype_with_method_variance(
                            source_type,
                            target_type,
                            allow_bivariant,
                        )
                        .is_true()
                    {
                        // Recursively explain the nested failure
                        let nested = self.explain_failure_with_method_variance(
                            source_type,
                            target_type,
                            allow_bivariant,
                        );
                        return Some(SubtypeFailureReason::PropertyTypeMismatch {
                            property_name: t_prop.name,
                            source_property_type: source_type,
                            target_property_type: target_type,
                            nested_reason: nested.map(Box::new),
                        });
                    }
                    if !t_prop.readonly
                        && (sp.write_type != sp.type_id || t_prop.write_type != t_prop.type_id)
                    {
                        let source_write = self.optional_property_write_type(sp);
                        let target_write = self.optional_property_write_type(t_prop);
                        if !self
                            .check_subtype_with_method_variance(
                                target_write,
                                source_write,
                                allow_bivariant,
                            )
                            .is_true()
                        {
                            let nested = self.explain_failure_with_method_variance(
                                target_write,
                                source_write,
                                allow_bivariant,
                            );
                            return Some(SubtypeFailureReason::PropertyTypeMismatch {
                                property_name: t_prop.name,
                                source_property_type: source_write,
                                target_property_type: target_write,
                                nested_reason: nested.map(Box::new),
                            });
                        }
                    }
                }
                None => {
                    // Required property is missing
                    if !t_prop.optional {
                        return Some(SubtypeFailureReason::MissingProperty {
                            property_name: t_prop.name,
                            source_type: source,
                            target_type: target,
                        });
                    }
                }
            }
        }

        None
    }

    /// Explain why an indexed object type assignment failed.
    fn explain_indexed_object_failure(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_shape: &ObjectShape,
        source_shape_id: Option<ObjectShapeId>,
        target_shape: &ObjectShape,
    ) -> Option<SubtypeFailureReason> {
        // First check properties
        if let Some(reason) = self.explain_object_failure(
            source,
            target,
            &source_shape.properties,
            source_shape_id,
            &target_shape.properties,
        ) {
            return Some(reason);
        }

        // Check string index signature
        if let Some(ref t_string_idx) = target_shape.string_index {
            match &source_shape.string_index {
                Some(s_string_idx) => {
                    if s_string_idx.readonly && !t_string_idx.readonly {
                        return Some(SubtypeFailureReason::TypeMismatch {
                            source_type: source,
                            target_type: target,
                        });
                    }
                    if !self
                        .check_subtype(s_string_idx.value_type, t_string_idx.value_type)
                        .is_true()
                    {
                        return Some(SubtypeFailureReason::IndexSignatureMismatch {
                            index_kind: "string",
                            source_value_type: s_string_idx.value_type,
                            target_value_type: t_string_idx.value_type,
                        });
                    }
                }
                None => {
                    for prop in &source_shape.properties {
                        let prop_type = self.optional_property_type(prop);
                        if !self
                            .check_subtype(prop_type, t_string_idx.value_type)
                            .is_true()
                        {
                            return Some(SubtypeFailureReason::IndexSignatureMismatch {
                                index_kind: "string",
                                source_value_type: prop_type,
                                target_value_type: t_string_idx.value_type,
                            });
                        }
                    }
                }
            }
        }

        // Check number index signature
        if let Some(ref t_number_idx) = target_shape.number_index
            && let Some(ref s_number_idx) = source_shape.number_index
        {
            if s_number_idx.readonly && !t_number_idx.readonly {
                return Some(SubtypeFailureReason::TypeMismatch {
                    source_type: source,
                    target_type: target,
                });
            }
            if !self
                .check_subtype(s_number_idx.value_type, t_number_idx.value_type)
                .is_true()
            {
                return Some(SubtypeFailureReason::IndexSignatureMismatch {
                    index_kind: "number",
                    source_value_type: s_number_idx.value_type,
                    target_value_type: t_number_idx.value_type,
                });
            }
        }

        if let Some(reason) =
            self.explain_properties_against_index_signatures(&source_shape.properties, target_shape)
        {
            return Some(reason);
        }

        None
    }

    fn explain_object_with_index_to_object_failure(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_shape: &ObjectShape,
        source_shape_id: ObjectShapeId,
        target_props: &[PropertyInfo],
    ) -> Option<SubtypeFailureReason> {
        for t_prop in target_props {
            if let Some(sp) =
                self.lookup_property(&source_shape.properties, Some(source_shape_id), t_prop.name)
            {
                // Check nominal identity for private/protected properties
                // Private and protected members are nominally typed - they must
                // originate from the same declaration (same parent_id)
                if t_prop.visibility != Visibility::Public {
                    if sp.parent_id != t_prop.parent_id {
                        return Some(SubtypeFailureReason::PropertyNominalMismatch {
                            property_name: t_prop.name,
                        });
                    }
                }
                // Cannot assign private/protected source to public target
                else if sp.visibility != Visibility::Public {
                    return Some(SubtypeFailureReason::PropertyVisibilityMismatch {
                        property_name: t_prop.name,
                        source_visibility: sp.visibility,
                        target_visibility: t_prop.visibility,
                    });
                }

                if sp.optional && !t_prop.optional {
                    return Some(SubtypeFailureReason::OptionalPropertyRequired {
                        property_name: t_prop.name,
                    });
                }
                // NOTE: TypeScript allows readonly source to satisfy mutable target
                // (readonly is a constraint on the reference, not structural compatibility)

                let source_type = self.optional_property_type(sp);
                let target_type = self.optional_property_type(t_prop);
                let allow_bivariant = sp.is_method || t_prop.is_method;
                if !self
                    .check_subtype_with_method_variance(source_type, target_type, allow_bivariant)
                    .is_true()
                {
                    let nested = self.explain_failure_with_method_variance(
                        source_type,
                        target_type,
                        allow_bivariant,
                    );
                    return Some(SubtypeFailureReason::PropertyTypeMismatch {
                        property_name: t_prop.name,
                        source_property_type: source_type,
                        target_property_type: target_type,
                        nested_reason: nested.map(Box::new),
                    });
                }
                if !t_prop.readonly
                    && (sp.write_type != sp.type_id || t_prop.write_type != t_prop.type_id)
                {
                    let source_write = self.optional_property_write_type(sp);
                    let target_write = self.optional_property_write_type(t_prop);
                    if !self
                        .check_subtype_with_method_variance(
                            target_write,
                            source_write,
                            allow_bivariant,
                        )
                        .is_true()
                    {
                        let nested = self.explain_failure_with_method_variance(
                            target_write,
                            source_write,
                            allow_bivariant,
                        );
                        return Some(SubtypeFailureReason::PropertyTypeMismatch {
                            property_name: t_prop.name,
                            source_property_type: source_write,
                            target_property_type: target_write,
                            nested_reason: nested.map(Box::new),
                        });
                    }
                }
                continue;
            }

            let mut checked = false;
            let target_type = self.optional_property_type(t_prop);

            if utils::is_numeric_property_name(self.interner, t_prop.name)
                && let Some(number_idx) = &source_shape.number_index
            {
                checked = true;
                if number_idx.readonly && !t_prop.readonly {
                    return Some(SubtypeFailureReason::ReadonlyPropertyMismatch {
                        property_name: t_prop.name,
                    });
                }
                if !self
                    .check_subtype_with_method_variance(
                        number_idx.value_type,
                        target_type,
                        t_prop.is_method,
                    )
                    .is_true()
                {
                    return Some(SubtypeFailureReason::IndexSignatureMismatch {
                        index_kind: "number",
                        source_value_type: number_idx.value_type,
                        target_value_type: target_type,
                    });
                }
            }

            if let Some(string_idx) = &source_shape.string_index {
                checked = true;
                if string_idx.readonly && !t_prop.readonly {
                    return Some(SubtypeFailureReason::ReadonlyPropertyMismatch {
                        property_name: t_prop.name,
                    });
                }
                if !self
                    .check_subtype_with_method_variance(
                        string_idx.value_type,
                        target_type,
                        t_prop.is_method,
                    )
                    .is_true()
                {
                    return Some(SubtypeFailureReason::IndexSignatureMismatch {
                        index_kind: "string",
                        source_value_type: string_idx.value_type,
                        target_value_type: target_type,
                    });
                }
            }

            if !checked && !t_prop.optional {
                return Some(SubtypeFailureReason::MissingProperty {
                    property_name: t_prop.name,
                    source_type: source,
                    target_type: target,
                });
            }
        }

        None
    }

    fn explain_properties_against_index_signatures(
        &mut self,
        source: &[PropertyInfo],
        target: &ObjectShape,
    ) -> Option<SubtypeFailureReason> {
        let string_index = target.string_index.as_ref();
        let number_index = target.number_index.as_ref();

        if string_index.is_none() && number_index.is_none() {
            return None;
        }

        for prop in source {
            let prop_type = self.optional_property_type(prop);
            let allow_bivariant = prop.is_method;

            if let Some(number_idx) = number_index {
                let is_numeric = utils::is_numeric_property_name(self.interner, prop.name);
                if is_numeric {
                    if !number_idx.readonly && prop.readonly {
                        return Some(SubtypeFailureReason::ReadonlyPropertyMismatch {
                            property_name: prop.name,
                        });
                    }
                    if !self
                        .check_subtype_with_method_variance(
                            prop_type,
                            number_idx.value_type,
                            allow_bivariant,
                        )
                        .is_true()
                    {
                        return Some(SubtypeFailureReason::IndexSignatureMismatch {
                            index_kind: "number",
                            source_value_type: prop_type,
                            target_value_type: number_idx.value_type,
                        });
                    }
                }
            }

            if let Some(string_idx) = string_index {
                if !string_idx.readonly && prop.readonly {
                    return Some(SubtypeFailureReason::ReadonlyPropertyMismatch {
                        property_name: prop.name,
                    });
                }
                if !self
                    .check_subtype_with_method_variance(
                        prop_type,
                        string_idx.value_type,
                        allow_bivariant,
                    )
                    .is_true()
                {
                    return Some(SubtypeFailureReason::IndexSignatureMismatch {
                        index_kind: "string",
                        source_value_type: prop_type,
                        target_value_type: string_idx.value_type,
                    });
                }
            }
        }

        None
    }

    /// Explain why a function type assignment failed.
    fn explain_function_failure(
        &mut self,
        source: &FunctionShape,
        target: &FunctionShape,
    ) -> Option<SubtypeFailureReason> {
        // Check return type
        if !(self
            .check_subtype(source.return_type, target.return_type)
            .is_true()
            || self.allow_void_return && target.return_type == TypeId::VOID)
        {
            let nested = self.explain_failure(source.return_type, target.return_type);
            return Some(SubtypeFailureReason::ReturnTypeMismatch {
                source_return: source.return_type,
                target_return: target.return_type,
                nested_reason: nested.map(Box::new),
            });
        }

        // Check parameter count
        let target_has_rest = target.params.last().is_some_and(|p| p.rest);
        let rest_elem_type = if target_has_rest {
            target
                .params
                .last()
                .map(|param| self.get_array_element_type(param.type_id))
        } else {
            None
        };
        let rest_is_top = self.allow_bivariant_rest
            && matches!(rest_elem_type, Some(TypeId::ANY | TypeId::UNKNOWN));
        let source_required = self.required_param_count(&source.params);
        let target_required = self.required_param_count(&target.params);
        let extra_required_ok = target_has_rest
            && source_required > target_required
            && self.extra_required_accepts_undefined(
                &source.params,
                target_required,
                source_required,
            );
        let too_many_params = !self.allow_bivariant_param_count
            && !rest_is_top
            && source_required > target_required
            && (!target_has_rest || !extra_required_ok);
        if !target_has_rest && too_many_params {
            return Some(SubtypeFailureReason::TooManyParameters {
                source_count: source_required,
                target_count: target_required,
            });
        }

        // Check parameter types
        let source_has_rest = source.params.last().is_some_and(|p| p.rest);
        let target_fixed_count = if target_has_rest {
            target.params.len().saturating_sub(1)
        } else {
            target.params.len()
        };
        let source_fixed_count = if source_has_rest {
            source.params.len().saturating_sub(1)
        } else {
            source.params.len()
        };
        let fixed_compare_count = std::cmp::min(source_fixed_count, target_fixed_count);
        for i in 0..fixed_compare_count {
            let s_param = &source.params[i];
            let t_param = &target.params[i];
            // Check parameter compatibility (contravariant in strict mode, bivariant in legacy)
            if !self.are_parameters_compatible(s_param.type_id, t_param.type_id) {
                return Some(SubtypeFailureReason::ParameterTypeMismatch {
                    param_index: i,
                    source_param: s_param.type_id,
                    target_param: t_param.type_id,
                });
            }
        }

        if target_has_rest {
            let Some(rest_elem_type) = rest_elem_type else {
                return None; // Invalid rest parameter
            };
            if rest_is_top {
                return None;
            }

            for i in target_fixed_count..source_fixed_count {
                let s_param = &source.params[i];
                if !self.are_parameters_compatible(s_param.type_id, rest_elem_type) {
                    return Some(SubtypeFailureReason::ParameterTypeMismatch {
                        param_index: i,
                        source_param: s_param.type_id,
                        target_param: rest_elem_type,
                    });
                }
            }

            if source_has_rest {
                let Some(s_rest_param) = source.params.last() else {
                    return None;
                };
                let s_rest_elem = self.get_array_element_type(s_rest_param.type_id);
                if !self.are_parameters_compatible(s_rest_elem, rest_elem_type) {
                    return Some(SubtypeFailureReason::ParameterTypeMismatch {
                        param_index: source_fixed_count,
                        source_param: s_rest_elem,
                        target_param: rest_elem_type,
                    });
                }
            }
        }

        if source_has_rest {
            let rest_param = source.params.last()?;
            let rest_elem_type = self.get_array_element_type(rest_param.type_id);
            let rest_is_top = self.allow_bivariant_rest
                && (rest_elem_type == TypeId::ANY || rest_elem_type == TypeId::UNKNOWN);

            if !rest_is_top {
                for i in source_fixed_count..target_fixed_count {
                    let t_param = &target.params[i];
                    if !self.are_parameters_compatible(rest_elem_type, t_param.type_id) {
                        return Some(SubtypeFailureReason::ParameterTypeMismatch {
                            param_index: i,
                            source_param: rest_elem_type,
                            target_param: t_param.type_id,
                        });
                    }
                }
            }
        }

        if target_has_rest && too_many_params {
            return Some(SubtypeFailureReason::TooManyParameters {
                source_count: source_required,
                target_count: target_required,
            });
        }

        None
    }

    /// Explain why a tuple type assignment failed.
    fn explain_tuple_failure(
        &mut self,
        source: &[TupleElement],
        target: &[TupleElement],
    ) -> Option<SubtypeFailureReason> {
        let source_required = source.iter().filter(|e| !e.optional && !e.rest).count();
        let target_required = target.iter().filter(|e| !e.optional && !e.rest).count();

        if source_required < target_required {
            return Some(SubtypeFailureReason::TupleElementMismatch {
                source_count: source.len(),
                target_count: target.len(),
            });
        }

        for (i, t_elem) in target.iter().enumerate() {
            if t_elem.rest {
                let expansion = self.expand_tuple_rest(t_elem.type_id);
                let outer_tail = &target[i + 1..];
                // Combined suffix = expansion.tail + outer_tail
                let combined_suffix: Vec<_> = expansion
                    .tail
                    .iter()
                    .chain(outer_tail.iter())
                    .cloned()
                    .collect();

                let mut source_end = source.len();
                for tail_elem in combined_suffix.iter().rev() {
                    if source_end <= i {
                        if !tail_elem.optional {
                            return Some(SubtypeFailureReason::TupleElementMismatch {
                                source_count: source.len(),
                                target_count: target.len(),
                            });
                        }
                        break;
                    }
                    let s_elem = &source[source_end - 1];
                    if s_elem.rest {
                        if !tail_elem.optional {
                            return Some(SubtypeFailureReason::TupleElementMismatch {
                                source_count: source.len(),
                                target_count: target.len(),
                            });
                        }
                        break;
                    }
                    let assignable = self
                        .check_subtype(s_elem.type_id, tail_elem.type_id)
                        .is_true();
                    if tail_elem.optional && !assignable {
                        break;
                    }
                    if !assignable {
                        return Some(SubtypeFailureReason::TupleElementTypeMismatch {
                            index: source_end - 1,
                            source_element: s_elem.type_id,
                            target_element: tail_elem.type_id,
                        });
                    }
                    source_end -= 1;
                }

                let mut source_iter = source.iter().enumerate().take(source_end).skip(i);

                for t_fixed in &expansion.fixed {
                    match source_iter.next() {
                        Some((j, s_elem)) => {
                            if s_elem.rest {
                                return Some(SubtypeFailureReason::TupleElementMismatch {
                                    source_count: source.len(),
                                    target_count: target.len(),
                                });
                            }
                            if !self
                                .check_subtype(s_elem.type_id, t_fixed.type_id)
                                .is_true()
                            {
                                return Some(SubtypeFailureReason::TupleElementTypeMismatch {
                                    index: j,
                                    source_element: s_elem.type_id,
                                    target_element: t_fixed.type_id,
                                });
                            }
                        }
                        None => {
                            if !t_fixed.optional {
                                return Some(SubtypeFailureReason::TupleElementMismatch {
                                    source_count: source.len(),
                                    target_count: target.len(),
                                });
                            }
                        }
                    }
                }

                if let Some(variadic) = expansion.variadic {
                    let variadic_array = self.interner.array(variadic);
                    for (j, s_elem) in source_iter {
                        let target_type = if s_elem.rest {
                            variadic_array
                        } else {
                            variadic
                        };
                        if !self.check_subtype(s_elem.type_id, target_type).is_true() {
                            return Some(SubtypeFailureReason::TupleElementTypeMismatch {
                                index: j,
                                source_element: s_elem.type_id,
                                target_element: target_type,
                            });
                        }
                    }
                    return None;
                }

                if source_iter.next().is_some() {
                    return Some(SubtypeFailureReason::TupleElementMismatch {
                        source_count: source.len(),
                        target_count: target.len(),
                    });
                }
                return None;
            }

            if let Some(s_elem) = source.get(i) {
                if s_elem.rest {
                    // Source has rest but target expects fixed element
                    return Some(SubtypeFailureReason::TupleElementMismatch {
                        source_count: source.len(), // Approximate "infinity"
                        target_count: target.len(),
                    });
                }

                if !self.check_subtype(s_elem.type_id, t_elem.type_id).is_true() {
                    return Some(SubtypeFailureReason::TupleElementTypeMismatch {
                        index: i,
                        source_element: s_elem.type_id,
                        target_element: t_elem.type_id,
                    });
                }
            } else if !t_elem.optional {
                return Some(SubtypeFailureReason::TupleElementMismatch {
                    source_count: source.len(),
                    target_count: target.len(),
                });
            }
        }

        // Target is closed. Check for extra elements in source.
        if source.len() > target.len() {
            return Some(SubtypeFailureReason::TupleElementMismatch {
                source_count: source.len(),
                target_count: target.len(),
            });
        }

        for s_elem in source {
            if s_elem.rest {
                return Some(SubtypeFailureReason::TupleElementMismatch {
                    source_count: source.len(), // implies open
                    target_count: target.len(),
                });
            }
        }

        None
    }

    /// Check if two types are structurally identical using De Bruijn indices for cycles.
    ///
    /// This is the O(1) alternative to bidirectional subtyping for identity checks.
    /// It transforms cyclic graphs into trees to solve the Graph Isomorphism problem.
    pub fn are_types_structurally_identical(&self, a: TypeId, b: TypeId) -> bool {
        if a == b {
            return true;
        }

        // Task #49: Use cached canonical_id when query_db is available (O(1) path)
        if let Some(db) = self.query_db {
            return db.canonical_id(a) == db.canonical_id(b);
        }

        // Fallback for cases without query_db: compute directly (O(N) path)
        let mut canonicalizer =
            crate::solver::canonicalize::Canonicalizer::new(self.interner, self.resolver);
        let canon_a = canonicalizer.canonicalize(a);
        let canon_b = canonicalizer.canonicalize(b);

        // After canonicalization, structural identity reduces to TypeId equality
        canon_a == canon_b
    }
}

/// Convenience function for one-off subtype checks (without resolver)
pub fn is_subtype_of(interner: &dyn TypeDatabase, source: TypeId, target: TypeId) -> bool {
    let mut checker = SubtypeChecker::new(interner);
    checker.is_subtype_of(source, target)
}

impl<'a, R: TypeResolver> AssignabilityChecker for SubtypeChecker<'a, R> {
    fn is_assignable_to(&mut self, source: TypeId, target: TypeId) -> bool {
        SubtypeChecker::is_assignable_to(self, source, target)
    }

    fn is_assignable_to_bivariant_callback(&mut self, source: TypeId, target: TypeId) -> bool {
        let prev_strict = self.strict_function_types;
        let prev_param_count = self.allow_bivariant_param_count;
        self.strict_function_types = false;
        self.allow_bivariant_param_count = true;
        let result = SubtypeChecker::is_assignable_to(self, source, target);
        self.allow_bivariant_param_count = prev_param_count;
        self.strict_function_types = prev_strict;
        result
    }
}

/// Convenience function for one-off subtype checks with a resolver
pub fn is_subtype_of_with_resolver<R: TypeResolver>(
    interner: &dyn TypeDatabase,
    resolver: &R,
    source: TypeId,
    target: TypeId,
) -> bool {
    let mut checker = SubtypeChecker::with_resolver(interner, resolver);
    checker.is_subtype_of(source, target)
}

/// Check if two types are structurally identical using De Bruijn indices for cycles.
///
/// This is the O(1) alternative to bidirectional subtyping for identity checks.
/// It transforms cyclic graphs into trees to solve the Graph Isomorphism problem.
pub fn are_types_structurally_identical<R: TypeResolver>(
    interner: &dyn TypeDatabase,
    resolver: &R,
    a: TypeId,
    b: TypeId,
) -> bool {
    if a == b {
        return true;
    }
    let mut canonicalizer = crate::solver::canonicalize::Canonicalizer::new(interner, resolver);
    let canon_a = canonicalizer.canonicalize(a);
    let canon_b = canonicalizer.canonicalize(b);

    // After canonicalization, structural identity reduces to TypeId equality
    canon_a == canon_b
}

/// Convenience function for one-off subtype checks routed through a QueryDatabase.
/// The QueryDatabase enables Salsa memoization when available.
pub fn is_subtype_of_with_db(db: &dyn QueryDatabase, source: TypeId, target: TypeId) -> bool {
    let mut checker = SubtypeChecker::new(db.as_type_database()).with_query_db(db);
    checker.is_subtype_of(source, target)
}

/// Convenience function for one-off subtype checks with compiler flags.
/// The flags are a packed u16 bitmask matching RelationCacheKey.flags.
pub fn is_subtype_of_with_flags(
    interner: &dyn TypeDatabase,
    source: TypeId,
    target: TypeId,
    flags: u16,
) -> bool {
    let mut checker = SubtypeChecker::new(interner).apply_flags(flags);
    checker.is_subtype_of(source, target)
}

/// Convenience function for one-off subtype checks with a resolver, routed through a QueryDatabase.
pub fn is_subtype_of_with_resolver_and_db<R: TypeResolver>(
    db: &dyn QueryDatabase,
    resolver: &R,
    source: TypeId,
    target: TypeId,
) -> bool {
    let mut checker =
        SubtypeChecker::with_resolver(db.as_type_database(), resolver).with_query_db(db);
    checker.is_subtype_of(source, target)
}

// Re-enabled subtype tests - verifying API compatibility
#[cfg(test)]
#[path = "tests/subtype_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/index_signature_tests.rs"]
mod index_signature_tests;

#[cfg(test)]
#[path = "tests/generics_rules_tests.rs"]
mod generics_rules_tests;

#[cfg(test)]
#[path = "tests/callable_tests.rs"]
mod callable_tests;

#[cfg(test)]
#[path = "tests/union_tests.rs"]
mod union_tests;

#[cfg(test)]
#[path = "tests/typescript_quirks_tests.rs"]
mod typescript_quirks_tests;

#[cfg(test)]
#[path = "tests/type_predicate_tests.rs"]
mod type_predicate_tests;

#[cfg(test)]
#[path = "tests/overlap_tests.rs"]
mod overlap_tests;
