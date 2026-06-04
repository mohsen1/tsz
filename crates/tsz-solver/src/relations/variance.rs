//! Variance calculation for type parameters.
//!
//! This module implements variance analysis for generic type parameters,
//! enabling O(1) generic assignability checks by determining whether type
//! parameters are covariant, contravariant, invariant, or independent.
//!
//! ## Variance (Task #41)
//!
//! Variance determines how subtyping of generic types relates to subtyping
//! of their type arguments:
//!
//! - **Covariant**: `Box<Dog>` <: `Box<Animal>` if `Dog` <: `Animal`
//! - **Contravariant**: `Writer<Animal>` <: `Writer<Dog>` if `Dog` <: `Animal`
//! - **Invariant**: `MutableBox<Dog>` <: `MutableBox<Animal>` only if `Dog === Animal`
//! - **Independent**: Type parameter not used - can be skipped in checks
//!
//! ## Implementation
//!
//! The `VarianceVisitor` traverses types while tracking polarity:
//! - **Positive polarity** (covariant positions): function returns, array elements
//! - **Negative polarity** (contravariant positions): function parameters
//! - **Both polarity** (invariant): mutable properties with different read/write types
//!
//! Cycle detection uses `(TypeId, Polarity)` pairs to allow correct variance
//! calculation for recursive types like `type List<T> = { head: T; tail: List<T> }`.
//!
//! Also supports lazy type resolution, recursive variance composition,
//! and Ref(SymbolRef) type handling.

use crate::caches::db::QueryDatabase;
use crate::construction::TypeDatabase;
use crate::def::DefId;
use crate::def::resolver::TypeResolver;
use crate::types::{
    CallableShapeId, ConditionalTypeId, FunctionShapeId, IntrinsicKind, LiteralValue, MappedTypeId,
    ObjectShapeId, StringIntrinsicKind, SymbolRef, TemplateLiteralId, TemplateSpan, TupleListId,
    TypeApplicationId, TypeData, TypeId, TypeListId, TypeParamInfo, Variance,
};
use crate::visitor::lazy_def_id;
use crate::visitors::visitor::TypeVisitor;

use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use tsz_common::interner::Atom;

/// Compute the variance of a type parameter within a type.
///
/// This is the main entry point for variance calculation. It analyzes how
/// a specific type parameter (identified by its name) is used within a type
/// to determine whether it's covariant, contravariant, invariant, or independent.
///
/// # Parameters
///
/// * `db` - The type database for looking up type structures
/// * `type_id` - The type to analyze (e.g., the body of a generic type)
/// * `target_param` - The name of the type parameter to find (e.g., "T")
///
/// # Returns
///
/// A `Variance` bitmask indicating how the type parameter is used:
///
/// # Examples
///
/// ```text
/// use crate::relations::variance::compute_variance;
/// use crate::types::*;
///
/// // For type ReadonlyArray<T> = { readonly [index: number]: T }
/// // T is in a covariant position (array element)
/// let variance = compute_variance(db, array_body, "T");
/// assert!(variance.is_covariant());
///
/// // For type Writer<T> = { write(x: T): void }
/// // T is in a contravariant position (function parameter)
/// let variance = compute_variance(db, writer_body, "T");
/// assert!(variance.is_contravariant());
///
/// // For type Box<T> = { get(): T; set(x: T): void }
/// // T is in both positions -> invariant
/// let variance = compute_variance(db, box_body, "T");
/// assert!(variance.is_invariant());
/// ```
pub fn compute_variance(db: &dyn QueryDatabase, type_id: TypeId, target_param: Atom) -> Variance {
    let mut computer = VarianceComputer::new(db.as_type_database(), db.as_type_resolver());
    computer.compute(type_id, target_param)
}

/// Compute the variance of a type parameter using an explicit resolver.
///
/// This is the resolver-aware equivalent of `compute_variance`. It is used by
/// relation checks that need to preserve local alias identity even when the
/// shared query cache cannot resolve those alias definitions.
pub fn compute_variance_with_resolver(
    db: &dyn TypeDatabase,
    resolver: &dyn TypeResolver,
    type_id: TypeId,
    target_param: Atom,
) -> Variance {
    let mut computer = VarianceComputer::new(db, resolver);
    computer.compute(type_id, target_param)
}

/// Compute the full variance mask for a generic definition using an explicit resolver.
///
/// Returns `None` when the definition cannot be resolved to a generic body.
pub fn compute_type_param_variances_with_resolver(
    db: &dyn TypeDatabase,
    resolver: &dyn TypeResolver,
    def_id: DefId,
) -> Option<Arc<[Variance]>> {
    let mut computer = VarianceComputer::new(db, resolver);
    computer.compute_def_variances(def_id)
}

pub fn compute_actual_type_param_variances_with_resolver(
    db: &dyn TypeDatabase,
    resolver: &dyn TypeResolver,
    def_id: DefId,
) -> Option<Arc<[Variance]>> {
    let mut computer = VarianceComputer::new_actual(db, resolver);
    computer.compute_def_variances(def_id)
}

/// Session-cached form of [`compute_type_param_variances_with_resolver`].
///
/// Variance of a generic `DefId` is a pure function of that definition's
/// resolved body, so for a fixed checking session the declared-variance mask is
/// stable across every reference to the generic. Without memoization, each type
/// reference that validates its type arguments rebuilds a fresh
/// [`VarianceComputer`] and re-walks the entire (possibly deep, lazy-ref-heavy)
/// type graph from scratch — the dominant cost when checking large
/// generic-alias-heavy projects.
///
/// This helper computes the mask **using the supplied resolver** (never the
/// query database's own resolver, which may not see local alias bodies) but
/// threads the session-level variance cache exposed by [`QueryDatabase`] into
/// the [`VarianceComputer`] so that *every* `DefId` resolved at a context-free
/// top-level entry — both the queried def and any nested generic reached
/// through `visit_application` whose own walk starts with an empty active-def
/// set — is read from / written to one persistent map. The cache key is the
/// `DefId` alone: the stored value is the declared-variance mask, identical to
/// what the uncached call would return, so consulting/populating the cache
/// cannot change any diagnostic.
///
/// ## Soundness gate (the empty-active-def entry rule)
///
/// A [`VarianceComputer`] tracks `active_defs` to truncate cyclic
/// self-references (returning the "independent" placeholder mask). The mask a
/// `DefId` produces is therefore a pure function of its resolved body **only
/// when its `compute_def_variances` walk begins with no other def already on the
/// recursion stack**: if an outer def is active, a back-edge into it would be
/// truncated, yielding a context-dependent mask. The visitor-level nesting
/// counters (`mapped_depth`, `method_bivariant_depth`, `inside_unreliable`,
/// `bound_type_params`) never cross a `compute_def_variances` boundary — each
/// nested def starts fresh visitors at base context — so an empty `active_defs`
/// at entry is the exact, complete condition for context-freedom.
///
/// The cache is consulted and populated **only** for `compute_def_variances`
/// calls observed with an empty `active_defs` set at entry. The handful of
/// genuinely context-sensitive defs (those reached only through a nested
/// application while an outer def is active) never satisfy this gate and are
/// never cached, so they recompute on every reference exactly as before.
///
/// Only fully-resolved (`Some`) results are memoized. A `None` (unresolved or
/// non-generic) result is not cached, so a later reference made after the
/// definition's body becomes resolvable still recomputes.
pub fn compute_type_param_variances_with_resolver_cached(
    db: &dyn TypeDatabase,
    resolver: &dyn TypeResolver,
    query_db: Option<&dyn QueryDatabase>,
    def_id: DefId,
) -> Option<Arc<[Variance]>> {
    let session_cache = query_db.filter(|_| variance_cache_enabled());
    let mut computer = VarianceComputer::new(db, resolver);
    computer.session_cache = session_cache;
    computer.compute_def_variances(def_id)
}

/// Debug kill-switch for the session-level computed-variance cache.
///
/// Set `TSZ_DISABLE_VARIANCE_CACHE=1` to bypass both reads and writes so the
/// resolver-aware variance walk runs uncached on every reference. Used to
/// bisect regressions and prove byte-identical diagnostics; defaults to enabled.
fn variance_cache_enabled() -> bool {
    use std::sync::OnceLock;
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("TSZ_DISABLE_VARIANCE_CACHE").is_err())
}

struct VarianceComputer<'a> {
    db: &'a dyn TypeDatabase,
    resolver: &'a dyn TypeResolver,
    use_declared_variance: bool,
    active_defs: FxHashSet<DefId>,
    cached_def_variances: FxHashMap<DefId, Option<Arc<[Variance]>>>,
    /// Optional session-persistent declared-variance cache.
    ///
    /// When present, `compute_def_variances` reads from and writes to this map
    /// for every def whose walk begins with an empty `active_defs` set (the
    /// context-free top-level entry — see
    /// [`compute_type_param_variances_with_resolver_cached`]). Only wired in for
    /// `use_declared_variance` computers: the map stores declared masks, so the
    /// `new_actual` computer must never touch it.
    session_cache: Option<&'a dyn QueryDatabase>,
}

impl<'a> VarianceComputer<'a> {
    fn new(db: &'a dyn TypeDatabase, resolver: &'a dyn TypeResolver) -> Self {
        Self {
            db,
            resolver,
            use_declared_variance: true,
            active_defs: FxHashSet::default(),
            cached_def_variances: FxHashMap::default(),
            session_cache: None,
        }
    }

    fn new_actual(db: &'a dyn TypeDatabase, resolver: &'a dyn TypeResolver) -> Self {
        Self {
            db,
            resolver,
            use_declared_variance: false,
            active_defs: FxHashSet::default(),
            cached_def_variances: FxHashMap::default(),
            session_cache: None,
        }
    }

    fn compute(&mut self, type_id: TypeId, target_param: Atom) -> Variance {
        let variances = VarianceVisitor::new(self, &[target_param]).compute(type_id);
        variances.into_iter().next().unwrap_or_else(Variance::empty)
    }

    fn compute_def_variances(&mut self, def_id: DefId) -> Option<Arc<[Variance]>> {
        if self.use_declared_variance
            && let Some(declared) = self.resolver.get_type_param_variance(def_id)
        {
            return Some(declared);
        }

        if let Some(cached) = self.cached_def_variances.get(&def_id) {
            return cached.clone();
        }

        // Context-free top-level entry: no other def is on the recursion stack,
        // so the mask this walk produces is a pure function of the resolved
        // body and is safe to share project-wide. Consult the session cache
        // before doing any work. The visitor-level nesting counters never cross
        // this boundary, so an empty `active_defs` is the exact gate (see
        // `compute_type_param_variances_with_resolver_cached`).
        let context_free_entry = self.active_defs.is_empty();
        if context_free_entry
            && let Some(qdb) = self.session_cache
            && let Some(cached) = qdb.get_cached_type_param_variance(def_id)
        {
            self.cached_def_variances
                .insert(def_id, Some(cached.clone()));
            return Some(cached);
        }

        if !self.active_defs.insert(def_id) {
            // Recursive self-reference: return independent (empty) variance for
            // each type parameter. This tells visit_application to skip the
            // recursive arguments entirely, so only non-recursive appearances of
            // the type parameter determine the variance. This avoids the previous
            // behavior of returning None which caused NEEDS_STRUCTURAL_FALLBACK
            // to be set, incorrectly forcing structural comparison for types like
            // Promise<T> that are clearly covariant from their direct usages.
            let params = self.resolver.get_lazy_type_params(def_id);
            return params.map(|p| Arc::from(vec![Variance::empty(); p.len()]));
        }

        let result: Option<Arc<[Variance]>> = (|| {
            let params = self.resolver.get_lazy_type_params(def_id)?;
            if params.is_empty() {
                return None;
            }

            let body = self.resolver.resolve_lazy(def_id, self.db)?;
            let param_names: Vec<_> = params.iter().map(|param| param.name).collect();
            let variances = VarianceVisitor::new(self, &param_names).compute(body);
            Some(Arc::from(variances))
        })();

        self.active_defs.remove(&def_id);

        // Promote to the session cache only when this walk was a context-free
        // top-level entry (empty `active_defs` at entry) and produced a fully
        // resolved mask. A `None` (unresolved/non-generic) is not cached so a
        // later reference after the body resolves still recomputes. The stored
        // mask equals the uncached result, so replaying it cannot change a
        // diagnostic.
        if context_free_entry
            && let Some(qdb) = self.session_cache
            && let Some(variances) = result.as_ref()
        {
            qdb.insert_type_param_variance(def_id, variances.clone());
        }

        self.cached_def_variances.insert(def_id, result.clone());
        result
    }
}

/// Visitor that computes variance for one or more type parameters.
///
/// The visitor tracks the current polarity (positive for covariant positions,
/// negative for contravariant positions) as it traverses the type graph.
/// When it encounters a target type parameter, it records the current polarity.
struct VarianceVisitor<'a, 'b> {
    /// Shared variance computation host.
    computer: &'b mut VarianceComputer<'a>,
    /// Target type parameters keyed by name, pointing into `results`.
    target_indices: FxHashMap<Atom, usize>,
    /// The accumulated variance results, one per target parameter.
    results: Vec<Variance>,
    /// Active (`TypeId`, Polarity) stack for cycle detection.
    active_stack: smallvec::SmallVec<[(TypeId, bool); 64]>,
    /// Total attempted guarded entries for the current variance walk.
    iterations: u32,
    /// Stack of polarities to track current position in the type graph.
    /// true = Positive (Covariant), false = Negative (Contravariant)
    polarity_stack: Vec<bool>,
    /// Names of bound type parameters (mapped type iteration variables) whose
    /// constraints should be skipped during variance computation. In a mapped
    /// type `{ [K in keyof S]: S[K] }`, K is a bound variable. Its constraint
    /// `keyof S` is already accounted for by visiting `mapped.constraint`.
    /// Without this, visiting K's constraint again through the template would
    /// double-count S's variance contribution (adding a spurious contravariant
    /// occurrence through the keyof reversal).
    bound_type_params: smallvec::SmallVec<[Atom; 2]>,
    /// Whether each target parameter was seen as the object of an indexed access.
    /// Used to detect when indexed access can normalize away type argument differences.
    seen_target_in_index_access: Vec<bool>,
    /// Depth counter for mapped type nesting. When > 0, occurrences of the target
    /// parameter are inside a mapped type and should not set `DIRECT_USAGE`.
    inside_mapped_depth: u32,
    /// Depth counter for method-bivariant traversal. When > 0, we're inside
    /// method parameter types. TypeScript methods have bivariant parameter
    /// checking, and tsc's variance computation (via marker types) always
    /// produces BIVARIANT for type params found through method params, which
    /// is then checked using the COVARIANT direction. To match this, we
    /// record all target param occurrences as COVARIANT when inside method
    /// params, regardless of actual nesting depth/polarity.
    method_bivariant_depth: u32,
    /// When true, suppress `method_bivariant_depth` from being set. This is
    /// used inside indexed access types where `{ m(x: T): any }['m']`
    /// extracts the method as a plain function, stripping method-ness.
    suppress_method_bivariance: bool,
    /// True if the target parameter was found at a strictly-checked position —
    /// outside any method-bivariant context AND outside any application visit
    /// that already inherited `REJECTION_UNRELIABLE` from a nested generic.
    /// A strict occurrence provides a reliable variance signal that overrides
    /// the unreliability set by sibling method-bivariant occurrences: e.g.
    /// `{ m(x: T, cb: (x: T) => void) }` should be COVARIANT, not bivariant,
    /// because the callback occurrence pins the variance.
    strict_occurrence_seen: Vec<bool>,
    /// Depth counter for visiting type arguments of an Application whose base
    /// generic has `REJECTION_UNRELIABLE` in its variance. Inside such a visit
    /// we do not treat the leaf occurrence as a strict signal — the
    /// unreliability has already been inherited from the wrapping application
    /// (e.g. `{ container: C1<T> }` should remain bivariant when `C1` is
    /// bivariant).
    inside_unreliable_application: u32,
}

impl<'a, 'b> VarianceVisitor<'a, 'b> {
    /// Create a new `VarianceVisitor`.
    fn new(computer: &'b mut VarianceComputer<'a>, target_params: &[Atom]) -> Self {
        let mut target_indices = FxHashMap::default();
        for (index, &param) in target_params.iter().enumerate() {
            target_indices.entry(param).or_insert(index);
        }
        let target_count = target_params.len();
        Self {
            computer,
            target_indices,
            results: vec![Variance::empty(); target_count],
            active_stack: smallvec::SmallVec::new(),
            iterations: 0,
            polarity_stack: vec![true], // Start with positive (covariant) polarity
            bound_type_params: smallvec::SmallVec::new(),
            seen_target_in_index_access: vec![false; target_count],
            inside_mapped_depth: 0,
            method_bivariant_depth: 0,
            suppress_method_bivariance: false,
            strict_occurrence_seen: vec![false; target_count],
            inside_unreliable_application: 0,
        }
    }

    /// Entry point: computes the variance of each target parameter within `type_id`.
    fn compute(mut self, type_id: TypeId) -> Vec<Variance> {
        self.visit_with_polarity(type_id, true);
        for index in 0..self.results.len() {
            // Indexed access plus structural fallback can normalize away type
            // argument differences, making variance rejection unreliable.
            if self.seen_target_in_index_access[index]
                && self.results[index].needs_structural_fallback()
            {
                self.results[index] |= Variance::REJECTION_UNRELIABLE;
            }
            // Strict occurrences pin method-bivariant unreliability, except for
            // indexed-access normalization unreliability.
            if self.strict_occurrence_seen[index] && !self.seen_target_in_index_access[index] {
                self.results[index].remove(Variance::REJECTION_UNRELIABLE);
            }
        }
        self.results
    }

    /// Core recursive step with polarity tracking.
    fn visit_with_polarity(&mut self, type_id: TypeId, polarity: bool) {
        // Fast path: intrinsic types contribute no variance information —
        // they have no nested type parameters anywhere inside them. The
        // visitor's `visit_intrinsic` handler is `{}`, so the guard
        // enter/leave, polarity-stack push/pop, and dispatch are all
        // wasted work for them. `TypeId::is_intrinsic` is a free range
        // check. Mirrors #2001 / #2005 / #2008 / #2009.
        if type_id.is_intrinsic() {
            return;
        }

        let key = (type_id, polarity);
        self.iterations = self.iterations.saturating_add(1);
        if self.iterations > crate::recursion::RecursionProfile::Variance.max_iterations()
            || self.active_stack.len()
                >= crate::recursion::RecursionProfile::Variance.max_depth() as usize
            || self.active_stack.contains(&key)
        {
            return;
        }
        self.active_stack.push(key);

        // Push new polarity onto stack
        self.polarity_stack.push(polarity);

        // Dispatch via TypeVisitor trait - the visitor implementations below
        // will use get_current_polarity() to get the current polarity
        self.visit_type(self.computer.db, type_id);

        // Pop polarity from stack
        self.polarity_stack.pop();

        debug_assert_eq!(self.active_stack.pop(), Some(key));
    }

    /// Get the current polarity from the stack.
    fn get_current_polarity(&self) -> bool {
        *self.polarity_stack.last().unwrap_or(&true)
    }

    /// Record an occurrence of the target parameter at the current polarity.
    fn add_occurrence(&mut self, target_index: usize, polarity: bool) {
        let result = &mut self.results[target_index];
        if self.method_bivariant_depth > 0 {
            // Inside method parameter types, always record as COVARIANT.
            // This matches tsc behavior: method bivariance makes T appear in
            // both co and contra positions (BIVARIANT), but tsc checks bivariant
            // type args using the covariant direction first. The net effect is
            // that method-param occurrences act as covariant for variance checking.
            *result |= Variance::COVARIANT;
            *result |= Variance::REJECTION_UNRELIABLE;
        } else if polarity {
            *result |= Variance::COVARIANT;
        } else {
            *result |= Variance::CONTRAVARIANT;
        }
        // Mark as direct usage when outside mapped type contexts.
        // Direct usage (function params, return types, properties) provides
        // reliable variance signal, unlike mapped type keyof/template positions.
        if self.inside_mapped_depth == 0 {
            *result |= Variance::DIRECT_USAGE;
        }
        // Track whether we've found T at a strict position. A strict occurrence
        // is one that's outside method bivariance AND outside an application
        // visit that already inherited unreliability. Such an occurrence pins
        // the variance signal — see `compute()` for how this is consumed.
        if self.method_bivariant_depth == 0 && self.inside_unreliable_application == 0 {
            self.strict_occurrence_seen[target_index] = true;
        }
    }

    fn mark_all_results(&mut self, variance: Variance) {
        for result in &mut self.results {
            *result |= variance;
        }
    }

    /// Check if a constraint type uses `keyof` of the target type parameter.
    /// For mapped types like `{ [K in keyof S]: Template }`, the key set depends
    /// on S via keyof, so the variance shortcut is unreliable even without modifiers.
    fn mark_keyof_constraint_fallback(&mut self, constraint: TypeId) {
        if let Some(crate::types::TypeData::KeyOf(inner)) = self.computer.db.lookup(constraint) {
            let mut indices = smallvec::SmallVec::<[usize; 4]>::new();
            self.target_indices_referenced_by(inner, &mut indices);
            for index in indices {
                self.results[index] |= Variance::NEEDS_STRUCTURAL_FALLBACK;
            }
        }
    }

    /// Collect target parameters referenced by a type (directly or nested).
    fn target_indices_referenced_by(
        &self,
        type_id: TypeId,
        out: &mut smallvec::SmallVec<[usize; 4]>,
    ) {
        if type_id.is_intrinsic() {
            return;
        }
        match self.computer.db.lookup(type_id) {
            Some(crate::types::TypeData::TypeParameter(info)) => {
                if let Some(&index) = self.target_indices.get(&info.name)
                    && !out.contains(&index)
                {
                    out.push(index);
                }
            }
            Some(crate::types::TypeData::KeyOf(inner)) => {
                self.target_indices_referenced_by(inner, out);
            }
            Some(crate::types::TypeData::IndexAccess(obj, idx)) => {
                self.target_indices_referenced_by(obj, out);
                self.target_indices_referenced_by(idx, out);
            }
            _ => {}
        }
    }
}

impl<'a, 'b> TypeVisitor for VarianceVisitor<'a, 'b> {
    type Output = ();

    fn default_output() -> Self::Output {}

    // ===== Intrinsic types (no type parameters) =====
    fn visit_intrinsic(&mut self, _kind: IntrinsicKind) {}

    fn visit_literal(&mut self, _value: &LiteralValue) {}

    fn visit_unique_symbol(&mut self, _symbol_ref: u32) {}

    fn visit_error(&mut self) {}

    fn visit_this_type(&mut self) {}

    // ===== Composite types =====

    /// Union types: variance is the union of variances from all members.
    fn visit_union(&mut self, list_id: u32) {
        let members = self.computer.db.type_list(TypeListId(list_id));
        // For unions, collect variance from all members
        // The union of covariant/contravariant gives us the overall variance
        for &member in members.iter() {
            // Polarity is preserved for union members
            self.visit_type(self.computer.db, member);
        }
    }

    /// Intersection types: variance is the union of variances from all members.
    fn visit_intersection(&mut self, list_id: u32) {
        let members = self.computer.db.type_list(TypeListId(list_id));
        // For intersections, collect variance from all members
        for &member in members.iter() {
            // Polarity is preserved for intersection members
            self.visit_type(self.computer.db, member);
        }
    }

    /// Array types: element type is in covariant position.
    fn visit_array(&mut self, element_type: TypeId) {
        // Array<T> is covariant in T
        // Current polarity preserved
        let current_polarity = self.get_current_polarity();
        self.visit_with_polarity(element_type, current_polarity);
    }

    /// Tuple types: element types are in covariant position.
    fn visit_tuple(&mut self, list_id: u32) {
        let elements = self.computer.db.tuple_list(TupleListId(list_id));
        let current_polarity = self.get_current_polarity();
        for element in elements.iter() {
            self.visit_with_polarity(element.type_id, current_polarity);
        }
    }

    /// Function types: parameters are contravariant, return type is covariant.
    fn visit_function(&mut self, shape_id: u32) {
        let shape = self.computer.db.function_shape(FunctionShapeId(shape_id));
        let current_polarity = self.get_current_polarity();

        let saved_method_depth = self.method_bivariant_depth;

        // Method parameters are bivariant at assignability time, but variance
        // probing still has to see type parameters used only in method
        // parameters. Treat those occurrences as covariant-first so generic
        // application checks do not classify Promise-like interfaces as
        // independent.
        if shape.is_method {
            if !self.suppress_method_bivariance {
                self.method_bivariant_depth = saved_method_depth + 1;
                for param in &shape.params {
                    self.visit_with_polarity(param.type_id, !current_polarity);
                }
                self.method_bivariant_depth = saved_method_depth;
            }
            // Method return type stays at the outer (saved) depth — strict
            // covariant position (matches tsc, where `interface C<T> { m():
            // T }` is COVARIANT).
            self.visit_with_polarity(shape.return_type, current_polarity);
            if let Some(this_ty) = shape.this_type {
                self.visit_with_polarity(this_ty, current_polarity);
            }
        } else {
            // Nested non-method function: T occurrences inside its
            // parameters, return type, and `this` are NOT method-bivariant,
            // even if this function value is itself a parameter of a
            // surrounding method. Reset `method_bivariant_depth` for ALL
            // child visits so leaf occurrences record their actual variance
            // polarity (e.g. `Promise<T>.then(cb: (x: T) => T)` records the
            // return-position T as COVARIANT, not bivariant).
            self.method_bivariant_depth = 0;
            for param in &shape.params {
                self.visit_with_polarity(param.type_id, !current_polarity);
            }
            self.visit_with_polarity(shape.return_type, current_polarity);
            if let Some(this_ty) = shape.this_type {
                self.visit_with_polarity(this_ty, !current_polarity);
            }
            self.method_bivariant_depth = saved_method_depth;
        }
    }

    /// Callable types: same variance rules as functions.
    fn visit_callable(&mut self, shape_id: u32) {
        let callable = self.computer.db.callable_shape(CallableShapeId(shape_id));
        let current_polarity = self.get_current_polarity();
        let saved_method_depth = self.method_bivariant_depth;

        // Call signatures
        for sig in &callable.call_signatures {
            // For methods (see visit_function for full rationale).
            if sig.is_method {
                if !self.suppress_method_bivariance {
                    self.method_bivariant_depth = saved_method_depth + 1;
                    for param in &sig.params {
                        self.visit_with_polarity(param.type_id, !current_polarity);
                    }
                    self.method_bivariant_depth = saved_method_depth;
                }
                // Method return type stays at the outer (saved) depth.
                self.visit_with_polarity(sig.return_type, current_polarity);
                if let Some(this_ty) = sig.this_type {
                    self.visit_with_polarity(this_ty, current_polarity);
                }
            } else {
                // Non-method call signature: reset method bivariance for the
                // entire signature — parameters, return type, and `this` —
                // matching `visit_function` for non-method shapes.
                self.method_bivariant_depth = 0;
                for param in &sig.params {
                    self.visit_with_polarity(param.type_id, !current_polarity);
                }
                self.visit_with_polarity(sig.return_type, current_polarity);
                if let Some(this_ty) = sig.this_type {
                    self.visit_with_polarity(this_ty, !current_polarity);
                }
                self.method_bivariant_depth = saved_method_depth;
            }
        }

        // Construct signatures follow the same rules. We deliberately do NOT
        // reset `method_bivariant_depth` here: a constructor signature inside
        // a generic interface (e.g. `interface ObjectContaining<T> { new
        // (sample: Partial<T>): Partial<T> }`) is not a callback in a
        // surrounding method, so the legacy depth propagation is the safe
        // behaviour. Changing this changed several `Partial<T>` /
        // `nongenericPartialInstantiations*` baselines without a clear win.
        for sig in &callable.construct_signatures {
            for param in &sig.params {
                self.visit_with_polarity(param.type_id, !current_polarity);
            }
            self.visit_with_polarity(sig.return_type, current_polarity);
            if let Some(this_ty) = sig.this_type {
                self.visit_with_polarity(this_ty, !current_polarity);
            }
        }

        // Properties follow the same rules as regular objects
        for prop in &callable.properties {
            // Read type is always checked at current polarity
            self.visit_with_polarity(prop.type_id, current_polarity);

            // CRITICAL FIX: Mutable properties are ALWAYS invariant
            if !prop.readonly {
                let write_ty = if prop.write_type != TypeId::NONE {
                    prop.write_type
                } else {
                    prop.type_id
                };
                self.visit_with_polarity(write_ty, !current_polarity);
            }
        }
    }

    /// Object types: properties are covariant (readonly) or invariant (mutable).
    fn visit_object(&mut self, shape_id: u32) {
        let shape = self.computer.db.object_shape(ObjectShapeId(shape_id));
        let current_polarity = self.get_current_polarity();

        for prop in &shape.properties {
            // TypeScript treats all properties as covariant for variance inference,
            // regardless of mutability. This matches tsc behavior where `{ x: T }`
            // is covariant in T even though the property is mutable (a well-known
            // unsoundness in TS for usability). Only explicit write_type differences
            // (set accessors with different types) contribute contravariant position.
            self.visit_with_polarity(prop.type_id, current_polarity);

            if prop.has_split_accessor() {
                self.visit_with_polarity(prop.write_type, !current_polarity);
            }
        }

        // Index signatures: same covariant-only rule for tsc parity
        if let Some(ref idx) = shape.string_index {
            self.visit_with_polarity(idx.value_type, current_polarity);
        }

        if let Some(ref idx) = shape.number_index {
            self.visit_with_polarity(idx.value_type, current_polarity);
        }
    }

    /// Object with index signatures: same variance rules as regular objects.
    fn visit_object_with_index(&mut self, shape_id: u32) {
        self.visit_object(shape_id);
    }

    /// Type parameters: check if this is our target.
    fn visit_type_parameter(&mut self, info: &TypeParamInfo) {
        if let Some(&target_index) = self.target_indices.get(&info.name) {
            let current_polarity = self.get_current_polarity();
            self.add_occurrence(target_index, current_polarity);
        }

        // Skip constraint/default for bound type parameters (mapped type iteration
        // variables like K in `{ [K in keyof S]: S[K] }`). Their constraints are
        // already accounted for by visit_mapped visiting mapped.constraint directly.
        let is_bound = self.bound_type_params.contains(&info.name);
        if !is_bound {
            // Also check constraint (at current polarity).
            // Constraints affect structural shape: `<U extends T>` means T
            // constrains what U can be, so T's variance is affected.
            if let Some(constraint) = info.constraint {
                let current_polarity = self.get_current_polarity();
                self.visit_with_polarity(constraint, current_polarity);
            }

            // Type parameter defaults are NOT visited for variance: a default
            // like `<TResult1 = T>` is only used when the caller omits the
            // type argument, and even then it expresses an *instantiation
            // rule*, not an occurrence of T in the generic body. Counting it
            // as an occurrence over-constrains variance — for example,
            // `Promise<T>.then<TResult1 = T>(cb: (v: T) => TResult1):
            // Promise<TResult1>` would record T as both contravariant (via
            // the cb return's default) and covariant (via the Promise<TR1>
            // return's default), making T invariant and rejecting valid
            // `Promise<never>` → `Promise<X>` assignments.
            //
            // Constraints (`<U extends T>`) ARE visited — those genuinely
            // constrain U's structural shape and propagate T's variance.
        }
    }

    /// Bound parameters: not handled in variance (used for canonicalization).
    fn visit_bound_parameter(&mut self, _de_bruijn_index: u32) {}

    /// Resolve Lazy(DefId) types to analyze variance of the underlying type.
    fn visit_lazy(&mut self, def_id: u32) {
        // Resolve the Lazy(DefId) to its underlying TypeId
        let def_id = DefId(def_id);
        if let Some(resolved) = self
            .computer
            .resolver
            .resolve_lazy(def_id, self.computer.db)
        {
            let current_polarity = self.get_current_polarity();
            self.visit_with_polarity(resolved, current_polarity);
        }
    }

    /// Resolve Ref(SymbolRef) types to analyze variance (legacy path).
    fn visit_ref(&mut self, symbol_ref: u32) {
        let symbol_ref = SymbolRef(symbol_ref);

        // Try to convert Ref to DefId (migration path)
        if let Some(def_id) = self.computer.resolver.symbol_to_def_id(symbol_ref) {
            // Convert to Lazy and resolve
            if let Some(resolved) = self
                .computer
                .resolver
                .resolve_lazy(def_id, self.computer.db)
            {
                let current_polarity = self.get_current_polarity();
                self.visit_with_polarity(resolved, current_polarity);
                return;
            }
        }

        // Fallback: resolve legacy symbols when DefId is unavailable.
        if let Some(resolved) = self
            .computer
            .resolver
            .resolve_symbol_ref(symbol_ref, self.computer.db)
        {
            let current_polarity = self.get_current_polarity();
            self.visit_with_polarity(resolved, current_polarity);
        }
    }

    /// Recursive types: skip (already handled by cycle detection).
    fn visit_recursive(&mut self, _de_bruijn_index: u32) {}

    /// Enum types: check member type variance.
    fn visit_enum(&mut self, _def_id: u32, member_type: TypeId) {
        let current_polarity = self.get_current_polarity();
        self.visit_with_polarity(member_type, current_polarity);
    }

    /// Look up the base type's variance and compose it with current polarity.
    /// This enables recursive variance calculation for nested generics like
    /// `type Wrapper<T> = Box<T>` where `Box` is covariant, so `Wrapper` should also be covariant.
    fn visit_application(&mut self, app_id: u32) {
        let app = self.computer.db.type_application(TypeApplicationId(app_id));
        let current_polarity = self.get_current_polarity();

        // 1. Extract DefId from the base type
        let base_def_id = lazy_def_id(self.computer.db, app.base);
        let variances = base_def_id.and_then(|def_id| self.computer.compute_def_variances(def_id));

        if let Some(variances) = variances {
            // 3. Compose variance: for each argument, apply base param's variance rules
            for (i, &arg) in app.args.iter().enumerate() {
                // Default to invariance if base type has more args than variance entries
                let base_param_variance = variances
                    .get(i)
                    .copied()
                    .unwrap_or(Variance::COVARIANT | Variance::CONTRAVARIANT);

                // Propagate NEEDS_STRUCTURAL_FALLBACK and REJECTION_UNRELIABLE
                // from nested applications. If Required<T> needs structural fallback
                // due to modifiers, then Foo<T> = { a: Required<T> } also needs it.
                if base_param_variance.needs_structural_fallback() {
                    self.mark_all_results(Variance::NEEDS_STRUCTURAL_FALLBACK);
                }
                let inherits_unreliable = base_param_variance.rejection_unreliable();
                if inherits_unreliable {
                    self.mark_all_results(Variance::REJECTION_UNRELIABLE);
                }

                // Composition Rules:
                // - Covariant base param: Argument inherits current polarity
                // - Contravariant base param: Argument flips current polarity
                // While visiting `arg`, mark that any leaf occurrence of T
                // appearing through this application should not count as a
                // "strict signal": the wrapping application has already
                // decided this position is unreliable, so the leaf merely
                // re-emits that unreliability. Without this, a structurally
                // bivariant wrapper such as `{ container: C1<T> }` would be
                // incorrectly demoted to strict covariance once we clear
                // `REJECTION_UNRELIABLE` based on `strict_occurrence_seen`.
                if inherits_unreliable {
                    self.inside_unreliable_application += 1;
                }
                if base_param_variance.contains(Variance::COVARIANT) {
                    self.visit_with_polarity(arg, current_polarity);
                }
                if base_param_variance.contains(Variance::CONTRAVARIANT) {
                    self.visit_with_polarity(arg, !current_polarity);
                }
                if inherits_unreliable {
                    self.inside_unreliable_application -= 1;
                }
                // Note: Invariant (both bits) visits both. Independent (no bits) visits neither.
            }
        } else if base_def_id.is_some() {
            // Can't compute — assume invariance + structural fallback.
            // We have a DefId but can't resolve the body/params, so we
            // can't verify whether the inner type has mapped type modifiers
            // that would make the variance shortcut unsound.
            self.mark_all_results(Variance::NEEDS_STRUCTURAL_FALLBACK);
            for &arg in &app.args {
                self.visit_with_polarity(arg, current_polarity);
                self.visit_with_polarity(arg, !current_polarity);
            }
        } else {
            // No DefId available — assume invariance (safest choice)
            for &arg in &app.args {
                self.visit_with_polarity(arg, current_polarity);
                self.visit_with_polarity(arg, !current_polarity);
            }
        }
    }

    /// Conditional types: `check_type` is COVARIANT, `extends_type` is CONTRAVARIANT.
    fn visit_conditional(&mut self, cond_id: u32) {
        let cond = self.computer.db.get_conditional(ConditionalTypeId(cond_id));
        let current_polarity = self.get_current_polarity();

        // In TypeScript, conditional types `T extends U ? X : Y` determine variance
        // solely from the branch types X and Y. The check_type T acts as a guard
        // condition, not a usage position, so it doesn't contribute to variance.
        // Similarly, extends_type U is a bound, not a variance contributor.
        // This matches tsc's probe-based variance inference behavior.

        // True and false branches preserve polarity (covariant positions)
        self.visit_with_polarity(cond.true_type, current_polarity);
        self.visit_with_polarity(cond.false_type, current_polarity);
    }

    /// Mapped types: constraint is contravariant, template is covariant.
    fn visit_mapped(&mut self, mapped_id: u32) {
        let mapped = self.computer.db.get_mapped(MappedTypeId(mapped_id));
        let current_polarity = self.get_current_polarity();

        // Mapped types with modifiers (-?/+?/-readonly/+readonly) require structural
        // fallback because mutually-assignable type arguments can produce structurally
        // incompatible results after modifier application (e.g., Required<{a?; x}> vs
        // Required<{b?; x}> — the args are assignable but the results differ).
        //
        // Additionally, mapped types whose constraint uses `keyof` of the target
        // type parameter (e.g., `{ [K in keyof S]: Type<S[K]> }`) need structural
        // fallback because the key set depends on S via `keyof S`, making the
        // variance check insufficient: a variance failure (e.g., invariant check
        // fails because `{a: 1} <: {}` but not `{} <: {a: 1}`) doesn't mean the
        // expanded mapped types are incompatible (`{ a: Type<1> }` IS assignable to `{}`).
        //
        // Plain mapped types like `Record<P, T> = { [K in P]: T }` do NOT need
        // fallback because the key set P is a direct type argument, not derived
        // through `keyof`, so variance correctly captures the relationship.
        if mapped.optional_modifier.is_some() || mapped.readonly_modifier.is_some() {
            self.mark_all_results(Variance::NEEDS_STRUCTURAL_FALLBACK);
        }
        self.mark_keyof_constraint_fallback(mapped.constraint);

        // Homomorphic mapped types with non-identity templates need structural
        // fallback. For identity mapped types (`{ [K in keyof S]: S[K] }`), the
        // variance is purely covariant and reliable. But for non-identity templates
        // like `{ [K in keyof S]: Type<S[K]> }`, the template may introduce
        // contravariant positions (e.g., Type<A> with A in function parameter
        // position), making the variance invariant. However, the STRUCTURAL result
        // can still be compatible: `ToA<{x:n}>` is assignable to `ToA<{}>`
        // because `ToA<{}>` evaluates to `{}` (no keys, so structurally empty).
        //
        // This matches tsc's variance probing behavior: when probing gives
        // unreliable results for complex mapped types, tsc falls through to
        // structural comparison rather than definitively rejecting.
        {
            use crate::types::TypeData;
            if let Some(TypeData::KeyOf(source)) = self.computer.db.lookup(mapped.constraint) {
                // Check if the template is identity: T[K] where T is the keyof source
                // and K is the iteration variable.
                let is_identity = if let Some(TypeData::IndexAccess(obj, idx)) =
                    self.computer.db.lookup(mapped.template)
                {
                    obj == source
                        && matches!(
                            self.computer.db.lookup(idx),
                            Some(TypeData::TypeParameter(tp)) if tp.name == mapped.type_param.name
                        )
                } else {
                    false
                };
                if !is_identity {
                    self.mark_all_results(Variance::NEEDS_STRUCTURAL_FALLBACK);
                }
            }
        }

        // Track that we're inside a mapped type so occurrences are not
        // marked as DIRECT_USAGE. Mapped type positions (keyof constraint,
        // template) can give unreliable variance signals.
        self.inside_mapped_depth += 1;

        // Constraint (K in keyof T) is CONTRAVARIANT with respect to T
        self.visit_with_polarity(mapped.constraint, !current_polarity);

        // Mark the iteration variable K as bound. When visiting the template,
        // encountering K should NOT trigger visiting K's constraint again —
        // the constraint is already accounted for above. Without this,
        // `{ [K in keyof S]: S[K] }` would give S an invariant variance
        // because K's constraint `keyof S` would add a spurious contravariant
        // contribution through the keyof reversal.
        let iter_var_name = mapped.type_param.name;
        self.bound_type_params.push(iter_var_name);

        // Template type is COVARIANT with respect to T.
        self.visit_with_polarity(mapped.template, current_polarity);

        // Name type (if present) is COVARIANT
        if let Some(name_type) = mapped.name_type {
            self.visit_with_polarity(name_type, current_polarity);
        }

        // Remove the bound variable
        self.bound_type_params.pop();

        self.inside_mapped_depth -= 1;
    }

    /// Index access: both object and key are at current polarity.
    ///
    /// When the target type parameter appears inside an indexed access (either as
    /// the object or the key), we mark the variance as needing structural fallback.
    /// This matches tsc's behavior where indexed access through a type parameter
    /// produces "unmeasurable" variance — the relationship between the type argument
    /// and the indexed access result is too complex for static variance analysis.
    ///
    /// Example: `S["base"] & S["new"]` in `DerivedTable<S>` — even though S is used
    /// covariantly, different instantiations like `{base: B, new: N}` and
    /// `{base: B, new: N & B}` can produce structurally equivalent indexed access
    /// results despite the type arguments not being subtypes of each other.
    fn visit_index_access(&mut self, object_type: TypeId, key_type: TypeId) {
        let current_polarity = self.get_current_polarity();
        // Track when the target parameter appears as the object of an indexed
        // access. This indicates that the type mapping S → S["key"] may
        // normalize away differences between type arguments.
        if let Some(TypeData::TypeParameter(tp)) = self.computer.db.lookup(object_type)
            && let Some(&target_index) = self.target_indices.get(&tp.name)
        {
            self.seen_target_in_index_access[target_index] = true;
        }
        let before = self.results.clone();
        // Suppress method bivariance inside indexed access. When a type uses
        // the `bivarianceHack` pattern like `{ m(x: T): any }['m']`, the
        // indexed access extracts the method as a plain function, stripping
        // method-ness. Method params should be skipped here (INDEPENDENT),
        // matching the original behavior that allowed structural comparison
        // with proper method bivariance at the assignability level.
        let saved_smb = self.suppress_method_bivariance;
        self.suppress_method_bivariance = true;
        self.visit_with_polarity(object_type, current_polarity);
        self.visit_with_polarity(key_type, current_polarity);
        self.suppress_method_bivariance = saved_smb;
        // If the target parameter was found inside this indexed access,
        // the variance shortcut may be unreliable — require structural fallback.
        //
        // However, for simple literal-key indexed accesses like A['witness'],
        // the access is a straightforward property projection that preserves
        // variance reliability. The variance of A['key'] directly follows from
        // A's variance for that property. Only non-literal keys (type params,
        // keyof, unions, etc.) can cause non-obvious normalization that makes
        // the variance shortcut unreliable.
        if self.results != before {
            let is_literal_key = matches!(
                self.computer.db.lookup(key_type),
                Some(TypeData::Literal(_))
            );
            if !is_literal_key {
                for (index, before_result) in before.iter().enumerate() {
                    if self.results[index] != *before_result {
                        self.results[index] |= Variance::NEEDS_STRUCTURAL_FALLBACK;
                    }
                }
            }
        }
    }

    /// Template literals: types in spans are at current polarity.
    fn visit_template_literal(&mut self, template_id: u32) {
        let before = self.results.clone();
        let spans = self
            .computer
            .db
            .template_list(TemplateLiteralId(template_id));
        let current_polarity = self.get_current_polarity();
        self.inside_unreliable_application += 1;

        for span in spans.iter() {
            if let TemplateSpan::Type(type_id) = span {
                self.visit_with_polarity(*type_id, current_polarity);
            }
        }
        self.inside_unreliable_application -= 1;
        if self.results != before {
            for (index, before_result) in before.iter().enumerate() {
                if self.results[index] != *before_result {
                    self.results[index] |= Variance::REJECTION_UNRELIABLE;
                }
            }
        }
    }

    /// Type query: not handled (would need symbol resolution).
    fn visit_type_query(&mut self, _symbol_ref: u32) {}

    /// Keyof: operand is CONTRAVARIANT.
    ///
    /// keyof reverses the variance relationship:
    /// - If T <: U (T is subtype of U), then keyof T has MORE properties than keyof U
    /// - Therefore keyof T is NOT a subtype of keyof U (it's a supertype)
    /// - Example: { a: 1, b: 2 } <: { a: 1 }, but "a" | "b" is NOT <: "a"
    fn visit_keyof(&mut self, type_id: TypeId) {
        let current_polarity = self.get_current_polarity();
        // keyof T reverses the variance (contravariant position)
        self.visit_with_polarity(type_id, !current_polarity);
    }

    /// Readonly types: inner type is at current polarity.
    fn visit_readonly_type(&mut self, inner_type: TypeId) {
        let current_polarity = self.get_current_polarity();
        self.visit_with_polarity(inner_type, current_polarity);
    }

    /// Infer types: declaration is not a usage.
    fn visit_infer(&mut self, info: &TypeParamInfo) {
        // FIX: Do not check info.name against target parameters.
        // 'infer X' declares X, it doesn't use the outer target param.
        // If 'infer T' shadows outer 'T', it's still a declaration, not a usage.

        // Check constraint
        if let Some(constraint) = info.constraint {
            let current_polarity = self.get_current_polarity();
            self.visit_with_polarity(constraint, current_polarity);
        }
    }

    /// String intrinsics: type argument is at current polarity.
    fn visit_string_intrinsic(&mut self, _kind: StringIntrinsicKind, type_arg: TypeId) {
        let current_polarity = self.get_current_polarity();
        self.visit_with_polarity(type_arg, current_polarity);
    }

    /// Module namespace: not handled.
    fn visit_module_namespace(&mut self, _symbol_ref: u32) {}
}
