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

use crate::interner::Atom;
use crate::solver::TypeDatabase;
use crate::solver::TypeVisitor;
use crate::solver::types::*;
use rustc_hash::FxHashSet;

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
/// ```rust
/// use crate::solver::variance::compute_variance;
/// use crate::solver::types::*;
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
pub fn compute_variance(db: &dyn TypeDatabase, type_id: TypeId, target_param: Atom) -> Variance {
    let visitor = VarianceVisitor::new(db, target_param);
    visitor.compute(type_id)
}

/// Visitor that computes variance for a specific type parameter.
///
/// The visitor tracks the current polarity (positive for covariant positions,
/// negative for contravariant positions) as it traverses the type graph.
/// When it encounters the target type parameter, it records the current polarity.
struct VarianceVisitor<'a> {
    /// The type database for looking up type structures.
    db: &'a dyn TypeDatabase,
    /// The name of the type parameter we're searching for (e.g., 'T').
    target_param: Atom,
    /// The accumulated variance result so far.
    result: Variance,
    /// Cycle detection: tracks (TypeId, Polarity) pairs.
    /// Polarity: true = Positive (Covariant), false = Negative (Contravariant)
    visiting: FxHashSet<(TypeId, bool)>,
    /// Stack of polarities to track current position in the type graph.
    /// true = Positive (Covariant), false = Negative (Contravariant)
    polarity_stack: Vec<bool>,
}

impl<'a> VarianceVisitor<'a> {
    /// Create a new VarianceVisitor.
    fn new(db: &'a dyn TypeDatabase, target_param: Atom) -> Self {
        Self {
            db,
            target_param,
            result: Variance::empty(),
            visiting: FxHashSet::default(),
            polarity_stack: vec![true], // Start with positive (covariant) polarity
        }
    }

    /// Entry point: computes the variance of target_param within type_id.
    fn compute(mut self, type_id: TypeId) -> Variance {
        self.visit_with_polarity(type_id, true);
        self.result
    }

    /// Visit a type while explicitly tracking polarity.
    ///
    /// This is used when we need to flip polarity for contravariant positions.
    fn visit_with_flipped_polarity(&mut self, type_id: TypeId) {
        let current = self.get_current_polarity();
        self.visit_with_polarity(type_id, !current);
    }

    /// Core recursive step with polarity tracking.
    fn visit_with_polarity(&mut self, type_id: TypeId, polarity: bool) {
        // Cycle detection: if we've seen this (type_id, polarity) pair, skip
        if !self.visiting.insert((type_id, polarity)) {
            return;
        }

        // Push new polarity onto stack
        self.polarity_stack.push(polarity);

        // Dispatch via TypeVisitor trait - the visitor implementations below
        // will use get_current_polarity() to get the current polarity
        self.visit_type(self.db, type_id);

        // Pop polarity from stack
        self.polarity_stack.pop();

        self.visiting.remove(&(type_id, polarity));
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
    }
}

impl<'a> TypeVisitor for VarianceVisitor<'a> {
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
        let members = self.db.type_list(TypeListId(list_id));
        // For unions, collect variance from all members
        // The union of covariant/contravariant gives us the overall variance
        for &member in members.iter() {
            // Polarity is preserved for union members
            self.visit_type(self.db, member);
        }
    }

    /// Intersection types: variance is the union of variances from all members.
    fn visit_intersection(&mut self, list_id: u32) {
        let members = self.db.type_list(TypeListId(list_id));
        // For intersections, collect variance from all members
        for &member in members.iter() {
            // Polarity is preserved for intersection members
            self.visit_type(self.db, member);
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
        let elements = self.db.tuple_list(TupleListId(list_id));
        let current_polarity = self.get_current_polarity();
        for element in elements.iter() {
            self.visit_with_polarity(element.type_id, current_polarity);
        }
    }

    /// Function types: parameters are contravariant, return type is covariant.
    fn visit_function(&mut self, shape_id: u32) {
        let shape = self.db.function_shape(FunctionShapeId(shape_id));
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

        // This type is CONTRAVARIANT: flip polarity (for method bivariance, this will be handled separately)
        if let Some(this_ty) = shape.this_type {
            self.visit_with_polarity(this_ty, !current_polarity);
        }
    }

    /// Callable types: same variance rules as functions.
    fn visit_callable(&mut self, shape_id: u32) {
        let callable = self.db.callable_shape(CallableShapeId(shape_id));
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
                self.visit_with_polarity(this_ty, !current_polarity);
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
        let shape = self.db.object_shape(ObjectShapeId(shape_id));
        let current_polarity = self.get_current_polarity();

        for prop in &shape.properties {
            // Read type is always checked at current polarity
            self.visit_with_polarity(prop.type_id, current_polarity);

            // CRITICAL FIX: Mutable properties are ALWAYS invariant
            // If write_type is different, use it; otherwise use type_id
            // This ensures { x: T } is invariant (not covariant!)
            if !prop.readonly {
                let write_ty = if prop.write_type != TypeId::NONE {
                    prop.write_type
                } else {
                    prop.type_id
                };
                self.visit_with_polarity(write_ty, !current_polarity);
            }
        }

        // Index signatures follow same rule as properties
        if let Some(ref idx) = shape.string_index {
            self.visit_with_polarity(idx.value_type, current_polarity);
            if !idx.readonly {
                self.visit_with_polarity(idx.value_type, !current_polarity);
            }
        }

        if let Some(ref idx) = shape.number_index {
            self.visit_with_polarity(idx.value_type, current_polarity);
            if !idx.readonly {
                self.visit_with_polarity(idx.value_type, !current_polarity);
            }
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

    /// Bound parameters: not handled in variance (used for canonicalization).
    fn visit_bound_parameter(&mut self, _de_bruijn_index: u32) {}

    /// Lazy types: resolve and continue.
    fn visit_lazy(&mut self, _def_id: u32) {
        // Lazy(DefId) types must be resolved to find variance
        // For now, we can't traverse into Lazy types without a resolver
        // This is handled at a higher level by resolving first
    }

    /// Recursive types: skip (already handled by cycle detection).
    fn visit_recursive(&mut self, _de_bruijn_index: u32) {}

    /// Enum types: check member type variance.
    fn visit_enum(&mut self, _def_id: u32, member_type: TypeId) {
        let current_polarity = self.get_current_polarity();
        self.visit_with_polarity(member_type, current_polarity);
    }

    /// Generic applications: variance depends on base type's variance.
    ///
    /// CRITICAL FIX: Since we can't resolve the base type's variance without
    /// a recursive query, we must assume invariance (the safe choice).
    /// Assuming covariance would be unsound!
    fn visit_application(&mut self, app_id: u32) {
        let app = self.db.type_application(TypeApplicationId(app_id));
        let current_polarity = self.get_current_polarity();

        // CONSERVATIVE: Assume all type parameters are invariant
        // This is safe (won't cause unsoundness) but may miss optimization opportunities
        // TODO: Look up variance mask of base type to get correct per-parameter variance
        for &arg in &app.args {
            // Visit with both polarities to indicate invariance
            self.visit_with_polarity(arg, current_polarity);
            self.visit_with_polarity(arg, !current_polarity);
        }
    }

    /// Conditional types: check_type is COVARIANT, extends_type is CONTRAVARIANT.
    fn visit_conditional(&mut self, cond_id: u32) {
        let cond = self.db.conditional_type(ConditionalTypeId(cond_id));
        let current_polarity = self.get_current_polarity();

        // FIX: check_type is COVARIANT (preserves polarity)
        self.visit_with_polarity(cond.check_type, current_polarity);

        // extends_type is CONTRAVARIANT (flips polarity)
        self.visit_with_polarity(cond.extends_type, !current_polarity);

        // True and false branches are COVARIANT (preserve polarity)
        self.visit_with_polarity(cond.true_type, current_polarity);
        self.visit_with_polarity(cond.false_type, current_polarity);
    }

    /// Mapped types: constraint is contravariant, template is covariant.
    fn visit_mapped(&mut self, mapped_id: u32) {
        let mapped = self.db.mapped_type(MappedTypeId(mapped_id));
        let current_polarity = self.get_current_polarity();

        // Type parameter constraint: check if it's our target
        if mapped.type_param.name == self.target_param {
            // The iteration variable K itself doesn't contribute to variance
            // It's a binder, not a usage of T
        }

        // Constraint (K in keyof T) is CONTRAVARIANT with respect to T
        self.visit_with_polarity(mapped.constraint, !current_polarity);

        // Template type is COVARIANT with respect to T
        self.visit_with_polarity(mapped.template, current_polarity);

        // Name type (if present) is COVARIANT
        if let Some(name_type) = mapped.name_type {
            self.visit_with_polarity(name_type, current_polarity);
        }
    }

    /// Index access: both object and key are at current polarity.
    fn visit_index_access(&mut self, object_type: TypeId, key_type: TypeId) {
        let current_polarity = self.get_current_polarity();
        self.visit_with_polarity(object_type, current_polarity);
        self.visit_with_polarity(key_type, current_polarity);
    }

    /// Template literals: types in spans are at current polarity.
    fn visit_template_literal(&mut self, template_id: u32) {
        let spans = self.db.template_list(TemplateLiteralId(template_id));
        let current_polarity = self.get_current_polarity();

        for span in spans.iter() {
            if let TemplateSpan::Type(type_id) = span {
                self.visit_with_polarity(*type_id, current_polarity);
            }
        }
    }

    /// Type query: not handled (would need symbol resolution).
    fn visit_type_query(&mut self, _symbol_ref: u32) {}

    /// Keyof: operand is at current polarity.
    fn visit_keyof(&mut self, type_id: TypeId) {
        let current_polarity = self.get_current_polarity();
        self.visit_with_polarity(type_id, current_polarity);
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
