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

use crate::TypeDatabase;
use crate::caches::db::QueryDatabase;
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

struct VarianceComputer<'a> {
    db: &'a dyn TypeDatabase,
    resolver: &'a dyn TypeResolver,
    active_defs: FxHashSet<DefId>,
    cached_def_variances: FxHashMap<DefId, Option<Arc<[Variance]>>>,
}

impl<'a> VarianceComputer<'a> {
    fn new(db: &'a dyn TypeDatabase, resolver: &'a dyn TypeResolver) -> Self {
        Self {
            db,
            resolver,
            active_defs: FxHashSet::default(),
            cached_def_variances: FxHashMap::default(),
        }
    }

    fn compute(&mut self, type_id: TypeId, target_param: Atom) -> Variance {
        let visitor = VarianceVisitor::new(self, target_param);
        visitor.compute(type_id)
    }

    fn compute_def_variances(&mut self, def_id: DefId) -> Option<Arc<[Variance]>> {
        if let Some(cached) = self.cached_def_variances.get(&def_id) {
            return cached.clone();
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

        let result = (|| {
            let params = self.resolver.get_lazy_type_params(def_id)?;
            if params.is_empty() {
                return None;
            }

            let body = self.resolver.resolve_lazy(def_id, self.db)?;
            let mut variances = Vec::with_capacity(params.len());
            for param in &params {
                variances.push(self.compute(body, param.name));
            }
            Some(Arc::from(variances))
        })();

        self.active_defs.remove(&def_id);
        self.cached_def_variances.insert(def_id, result.clone());
        result
    }
}

/// Visitor that computes variance for a specific type parameter.
///
/// The visitor tracks the current polarity (positive for covariant positions,
/// negative for contravariant positions) as it traverses the type graph.
/// When it encounters the target type parameter, it records the current polarity.
struct VarianceVisitor<'a, 'b> {
    /// Shared variance computation host.
    computer: &'b mut VarianceComputer<'a>,
    /// The name of the type parameter we're searching for (e.g., 'T').
    target_param: Atom,
    /// The accumulated variance result so far.
    result: Variance,
    /// Unified recursion guard for (`TypeId`, Polarity) cycle detection.
    guard: crate::recursion::RecursionGuard<(TypeId, bool)>,
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
    /// Whether the target parameter was seen as the object of an indexed access.
    /// Used to detect when indexed access can normalize away type argument differences.
    seen_target_in_index_access: bool,
    /// Depth counter for mapped type nesting. When > 0, occurrences of the target
    /// parameter are inside a mapped type and should not set `DIRECT_USAGE`.
    inside_mapped_depth: u32,
}

impl<'a, 'b> VarianceVisitor<'a, 'b> {
    /// Create a new `VarianceVisitor`.
    fn new(computer: &'b mut VarianceComputer<'a>, target_param: Atom) -> Self {
        Self {
            computer,
            target_param,
            result: Variance::empty(),
            guard: crate::recursion::RecursionGuard::with_profile(
                crate::recursion::RecursionProfile::Variance,
            ),
            polarity_stack: vec![true], // Start with positive (covariant) polarity
            bound_type_params: smallvec::SmallVec::new(),
            seen_target_in_index_access: false,
            inside_mapped_depth: 0,
        }
    }

    /// Entry point: computes the variance of `target_param` within `type_id`.
    fn compute(mut self, type_id: TypeId) -> Variance {
        self.visit_with_polarity(type_id, true);
        // When the type parameter is used as the object of an indexed access
        // AND a mapped type with modifiers is present (NEEDS_STRUCTURAL_FALLBACK),
        // the variance-based rejection becomes unreliable. Indexed access types
        // combined with intersections can normalize away differences between type
        // arguments, producing structurally equivalent instantiations even when
        // the type arguments themselves are not assignable.
        if self.seen_target_in_index_access && self.result.needs_structural_fallback() {
            self.result |= Variance::REJECTION_UNRELIABLE;
        }
        self.result
    }

    /// Core recursive step with polarity tracking.
    fn visit_with_polarity(&mut self, type_id: TypeId, polarity: bool) {
        // Unified enter: cycle detection + depth/iteration limits
        let key = (type_id, polarity);
        match self.guard.enter(key) {
            crate::recursion::RecursionResult::Entered => {}
            _ => return, // Cycle or limits exceeded
        }

        // Push new polarity onto stack
        self.polarity_stack.push(polarity);

        // Dispatch via TypeVisitor trait - the visitor implementations below
        // will use get_current_polarity() to get the current polarity
        self.visit_type(self.computer.db, type_id);

        // Pop polarity from stack
        self.polarity_stack.pop();

        self.guard.leave(key);
    }

    /// Get the current polarity from the stack.
    fn get_current_polarity(&self) -> bool {
        *self.polarity_stack.last().unwrap_or(&true)
    }

    /// Record an occurrence of the target parameter at the current polarity.
    fn add_occurrence(&mut self, polarity: bool) {
        if polarity {
            self.result |= Variance::COVARIANT;
        } else {
            self.result |= Variance::CONTRAVARIANT;
        }
        // Mark as direct usage when outside mapped type contexts.
        // Direct usage (function params, return types, properties) provides
        // reliable variance signal, unlike mapped type keyof/template positions.
        if self.inside_mapped_depth == 0 {
            self.result |= Variance::DIRECT_USAGE;
        }
    }

    /// Check if a constraint type uses `keyof` of the target type parameter.
    /// For mapped types like `{ [K in keyof S]: Template }`, the key set depends
    /// on S via keyof, so the variance shortcut is unreliable even without modifiers.
    fn constraint_uses_keyof_of_target(&self, constraint: TypeId) -> bool {
        if let Some(crate::types::TypeData::KeyOf(inner)) = self.computer.db.lookup(constraint) {
            self.type_references_target_param(inner)
        } else {
            false
        }
    }

    /// Check if a type references the target type parameter (directly or nested).
    fn type_references_target_param(&self, type_id: TypeId) -> bool {
        match self.computer.db.lookup(type_id) {
            Some(crate::types::TypeData::TypeParameter(info)) => info.name == self.target_param,
            Some(crate::types::TypeData::KeyOf(inner)) => self.type_references_target_param(inner),
            Some(crate::types::TypeData::IndexAccess(obj, idx)) => {
                self.type_references_target_param(obj) || self.type_references_target_param(idx)
            }
            _ => false,
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

        // CRITICAL FIX: Method bivariance - methods have bivariant parameters
        // If is_method is true, skip parameter variance (bivariant doesn't constrain)
        // Otherwise, parameters are CONTRAVARIANT (flip polarity)
        if !shape.is_method {
            for param in &shape.params {
                self.visit_with_polarity(param.type_id, !current_polarity);
            }
        }

        // Return type is COVARIANT: preserve polarity
        self.visit_with_polarity(shape.return_type, current_polarity);

        // `this` parameter behaves like a parameter for plain functions.
        // For methods, keep bivariance behavior and avoid forcing contravariant variance.
        if let Some(this_ty) = shape.this_type {
            let polarity = if shape.is_method {
                current_polarity
            } else {
                !current_polarity
            };
            self.visit_with_polarity(this_ty, polarity);
        }
    }

    /// Callable types: same variance rules as functions.
    fn visit_callable(&mut self, shape_id: u32) {
        let callable = self.computer.db.callable_shape(CallableShapeId(shape_id));
        let current_polarity = self.get_current_polarity();

        // Call signatures
        for sig in &callable.call_signatures {
            // CRITICAL FIX: Method bivariance - skip parameter variance if is_method
            if !sig.is_method {
                for param in &sig.params {
                    self.visit_with_polarity(param.type_id, !current_polarity);
                }
            }
            // Return type is covariant
            self.visit_with_polarity(sig.return_type, current_polarity);
            if let Some(this_ty) = sig.this_type {
                let polarity = if sig.is_method {
                    current_polarity
                } else {
                    !current_polarity
                };
                self.visit_with_polarity(this_ty, polarity);
            }
        }

        // Construct signatures follow same rules
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

            if prop.write_type != TypeId::NONE && prop.write_type != prop.type_id {
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
        if info.name == self.target_param {
            let current_polarity = self.get_current_polarity();
            self.add_occurrence(current_polarity);
        }

        // Skip constraint/default for bound type parameters (mapped type iteration
        // variables like K in `{ [K in keyof S]: S[K] }`). Their constraints are
        // already accounted for by visit_mapped visiting mapped.constraint directly.
        let is_bound = self.bound_type_params.contains(&info.name);
        if !is_bound {
            // Also check constraint (at current polarity)
            if let Some(constraint) = info.constraint {
                let current_polarity = self.get_current_polarity();
                self.visit_with_polarity(constraint, current_polarity);
            }

            // Default type (at current polarity)
            if let Some(default) = info.default {
                let current_polarity = self.get_current_polarity();
                self.visit_with_polarity(default, current_polarity);
            }
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
                    self.result |= Variance::NEEDS_STRUCTURAL_FALLBACK;
                }
                if base_param_variance.rejection_unreliable() {
                    self.result |= Variance::REJECTION_UNRELIABLE;
                }

                // Composition Rules:
                // - Covariant base param: Argument inherits current polarity
                if base_param_variance.contains(Variance::COVARIANT) {
                    self.visit_with_polarity(arg, current_polarity);
                }
                // - Contravariant base param: Argument flips current polarity
                if base_param_variance.contains(Variance::CONTRAVARIANT) {
                    self.visit_with_polarity(arg, !current_polarity);
                }
                // Note: Invariant (both bits) visits both. Independent (no bits) visits neither.
            }
        } else if base_def_id.is_some() {
            // Can't compute — assume invariance + structural fallback.
            // We have a DefId but can't resolve the body/params, so we
            // can't verify whether the inner type has mapped type modifiers
            // that would make the variance shortcut unsound.
            self.result |= Variance::NEEDS_STRUCTURAL_FALLBACK;
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
        if mapped.optional_modifier.is_some()
            || mapped.readonly_modifier.is_some()
            || self.constraint_uses_keyof_of_target(mapped.constraint)
        {
            self.result |= Variance::NEEDS_STRUCTURAL_FALLBACK;
        }

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
                    self.result |= Variance::NEEDS_STRUCTURAL_FALLBACK;
                }
            }
        }

        // Type parameter constraint: check if it's our target
        if mapped.type_param.name == self.target_param {
            // The iteration variable K itself doesn't contribute to variance
            // It's a binder, not a usage of T
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
            && tp.name == self.target_param
        {
            self.seen_target_in_index_access = true;
        }
        let before = self.result;
        self.visit_with_polarity(object_type, current_polarity);
        self.visit_with_polarity(key_type, current_polarity);
        // If the target parameter was found inside this indexed access,
        // the variance shortcut is unreliable — require structural fallback.
        if self.result != before {
            self.result |= Variance::NEEDS_STRUCTURAL_FALLBACK;
        }
    }

    /// Template literals: types in spans are at current polarity.
    fn visit_template_literal(&mut self, template_id: u32) {
        let spans = self
            .computer
            .db
            .template_list(TemplateLiteralId(template_id));
        let current_polarity = self.get_current_polarity();

        for span in spans.iter() {
            if let TemplateSpan::Type(type_id) = span {
                self.visit_with_polarity(*type_id, current_polarity);
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
        // FIX: Do not check info.name == self.target_param.
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
