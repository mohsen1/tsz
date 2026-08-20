//! Type Visitor Pattern
//!
//! This module implements the Visitor pattern for `TypeData` operations,
//! providing a clean alternative to repetitive match statements.
//!
//! # Benefits
//!
//! - **Centralized type logic**: All type handling in one place
//! - **Easier to extend**: Add new visitors without modifying existing code
//! - **Type-safe**: Compiler ensures all variants are handled
//! - **Composable**: Visitors can be combined and chained
//!
//! # Usage
//!
//! ```text
//! use crate::visitor::{TypeVisitor, TypeKind, is_type_kind};
//! use crate::types::{IntrinsicKind, LiteralValue};
//!
//! struct MyVisitor;
//!
//! impl TypeVisitor for MyVisitor {
//!     type Output = bool;
//!
//!     fn visit_intrinsic(&mut self, kind: IntrinsicKind) -> Self::Output {
//!         matches!(kind, IntrinsicKind::Any)
//!     }
//!
//!     fn visit_literal(&mut self, value: &LiteralValue) -> Self::Output {
//!         matches!(value, LiteralValue::Boolean(true))
//!     }
//!
//!     fn default_output() -> Self::Output {
//!         false
//!     }
//! }
//!
//! // Or use convenience functions:
//! let is_object = is_type_kind(&types, type_id, TypeKind::Object);
//! ```

use crate::construction::TypeDatabase;
use crate::def::DefId;
use crate::relations::subtype::TypeResolver;
use crate::types::{IntrinsicKind, StringIntrinsicKind, TupleElement, TypeParamInfo};
use crate::{LiteralValue, SymbolRef, TypeData, TypeId};
use rustc_hash::FxHashSet;
use std::cell::RefCell;

use super::child_policy::{ChildPolicy, for_each_child_with_policy, has_policy_children};

// Re-export type data extraction helpers (extracted to visitor_extract.rs)
pub use super::visitor_extract::*;

// Re-export type predicate functions (extracted to visitor_predicates.rs)
pub use super::visitor_predicates::*;

// =============================================================================
// Type Visitor Trait
// =============================================================================

/// Visitor pattern for `TypeData` traversal and transformation.
///
/// Implement this trait to perform custom operations on types without
/// writing repetitive match statements. Each method corresponds to a
/// `TypeData` variant and receives the relevant data for that type.
pub trait TypeVisitor: Sized {
    /// The output type produced by visiting.
    type Output;

    // =========================================================================
    // Core Type Visitors
    // =========================================================================

    /// Visit an intrinsic type (any, unknown, never, void, etc.).
    fn visit_intrinsic(&mut self, kind: IntrinsicKind) -> Self::Output;

    /// Visit a literal type (string, number, boolean, bigint literals).
    fn visit_literal(&mut self, value: &LiteralValue) -> Self::Output;

    // =========================================================================
    // Composite Types - Default implementations provided
    // =========================================================================

    /// Visit an object type with properties.
    fn visit_object(&mut self, _shape_id: u32) -> Self::Output {
        Self::default_output()
    }

    /// Visit an object type with index signatures.
    fn visit_object_with_index(&mut self, _shape_id: u32) -> Self::Output {
        Self::default_output()
    }

    /// Visit a union type (A | B | C).
    fn visit_union(&mut self, _list_id: u32) -> Self::Output {
        Self::default_output()
    }

    /// Visit an intersection type (A & B & C).
    fn visit_intersection(&mut self, _list_id: u32) -> Self::Output {
        Self::default_output()
    }

    /// Visit an array type T[].
    fn visit_array(&mut self, _element_type: TypeId) -> Self::Output {
        Self::default_output()
    }

    /// Visit a tuple type [T, U, V].
    fn visit_tuple(&mut self, _list_id: u32) -> Self::Output {
        Self::default_output()
    }

    /// Visit a function type.
    fn visit_function(&mut self, _shape_id: u32) -> Self::Output {
        Self::default_output()
    }

    /// Visit a callable type with call/construct signatures.
    fn visit_callable(&mut self, _shape_id: u32) -> Self::Output {
        Self::default_output()
    }

    /// Visit a type parameter (generic type variable).
    fn visit_type_parameter(&mut self, _param_info: &TypeParamInfo) -> Self::Output {
        Self::default_output()
    }

    /// Visit a bound type parameter using De Bruijn index for alpha-equivalence.
    ///
    /// This is used for canonicalizing generic types to achieve structural identity,
    /// where `type F<T> = T` and `type G<U> = U` are considered identical.
    /// The index represents which parameter in the binding scope (0 = innermost).
    fn visit_bound_parameter(&mut self, _de_bruijn_index: u32) -> Self::Output {
        Self::default_output()
    }

    /// Visit a named type reference (interface, class, type alias).
    fn visit_ref(&mut self, _symbol_ref: u32) -> Self::Output {
        Self::default_output()
    }

    /// Visit an enum type with nominal identity and structural member types.
    fn visit_enum(&mut self, _def_id: u32, _member_type: TypeId) -> Self::Output {
        Self::default_output()
    }

    /// Visit a lazy type reference using `DefId`.
    fn visit_lazy(&mut self, _def_id: u32) -> Self::Output {
        Self::default_output()
    }

    /// Visit a recursive type reference using De Bruijn index.
    ///
    /// This is used for canonicalizing recursive types to achieve O(1) equality.
    /// The index represents how many levels up the nesting chain to refer to.
    fn visit_recursive(&mut self, _de_bruijn_index: u32) -> Self::Output {
        Self::default_output()
    }

    /// Visit a generic type application Base<Args>.
    fn visit_application(&mut self, _app_id: u32) -> Self::Output {
        Self::default_output()
    }

    /// Visit a generic type application while preserving the already-interned
    /// `TypeId` that carried the application.
    fn visit_application_type(&mut self, _type_id: TypeId, app_id: u32) -> Self::Output {
        self.visit_application(app_id)
    }

    /// Visit a conditional type T extends U ? X : Y.
    fn visit_conditional(&mut self, _cond_id: u32) -> Self::Output {
        Self::default_output()
    }

    /// Visit a mapped type { [K in Keys]: V }.
    fn visit_mapped(&mut self, _mapped_id: u32) -> Self::Output {
        Self::default_output()
    }

    /// Visit an indexed access type T[K].
    fn visit_index_access(&mut self, _object_type: TypeId, _key_type: TypeId) -> Self::Output {
        Self::default_output()
    }

    /// Visit a template literal type `hello${x}world`.
    fn visit_template_literal(&mut self, _template_id: u32) -> Self::Output {
        Self::default_output()
    }

    /// Visit a type query (typeof expr).
    fn visit_type_query(&mut self, _symbol_ref: u32) -> Self::Output {
        Self::default_output()
    }

    /// Visit a keyof type.
    fn visit_keyof(&mut self, _type_id: TypeId) -> Self::Output {
        Self::default_output()
    }

    /// Visit a readonly type modifier.
    fn visit_readonly_type(&mut self, _inner_type: TypeId) -> Self::Output {
        Self::default_output()
    }

    /// Visit a unique symbol type.
    fn visit_unique_symbol(&mut self, _symbol_ref: u32) -> Self::Output {
        Self::default_output()
    }

    /// Visit an infer type (for type inference in conditional types).
    fn visit_infer(&mut self, _param_info: &TypeParamInfo) -> Self::Output {
        Self::default_output()
    }

    /// Visit a this type (polymorphic this parameter).
    fn visit_this_type(&mut self) -> Self::Output {
        Self::default_output()
    }

    /// Visit a string manipulation intrinsic type.
    fn visit_string_intrinsic(
        &mut self,
        _kind: StringIntrinsicKind,
        _type_arg: TypeId,
    ) -> Self::Output {
        Self::default_output()
    }

    /// Visit an error type.
    fn visit_error(&mut self) -> Self::Output {
        Self::default_output()
    }

    /// Visit a `NoInfer`<T> type (TypeScript 5.4+).
    /// Traverses the inner type (`NoInfer` is transparent for traversal).
    fn visit_no_infer(&mut self, _inner: TypeId) -> Self::Output {
        Self::default_output()
    }

    /// Visit a substitution type (`base_type` narrowed by `constraint`).
    /// Substitution is transparent for traversal; visitors that care about the
    /// surface identity look through to `base_type`.
    fn visit_substitution(&mut self, _base_type: TypeId, _constraint: TypeId) -> Self::Output {
        Self::default_output()
    }

    /// Visit a module namespace type (import * as ns).
    fn visit_module_namespace(&mut self, _symbol_ref: u32) -> Self::Output {
        Self::default_output()
    }

    // =========================================================================
    // Helper Methods
    // =========================================================================

    /// Default output for unimplemented variants.
    fn default_output() -> Self::Output;

    /// Visit a type by dispatching to the appropriate method.
    ///
    /// This is the main entry point for using the visitor.
    fn visit_type(&mut self, types: &dyn TypeDatabase, type_id: TypeId) -> Self::Output {
        match types.lookup(type_id) {
            Some(ref type_key) => self.visit_type_key(types, type_key),
            None => Self::default_output(),
        }
    }

    /// Visit a `TypeData` by dispatching to the appropriate method.
    fn visit_type_key(&mut self, _types: &dyn TypeDatabase, type_key: &TypeData) -> Self::Output {
        match type_key {
            TypeData::Intrinsic(kind) => self.visit_intrinsic(*kind),
            TypeData::Literal(value) => self.visit_literal(value),
            TypeData::Object(id) => self.visit_object(id.0),
            TypeData::ObjectWithIndex(id) => self.visit_object_with_index(id.0),
            TypeData::Union(id) => self.visit_union(id.0),
            TypeData::Intersection(id) => self.visit_intersection(id.0),
            TypeData::Array(element_type) => self.visit_array(*element_type),
            TypeData::Tuple(id) => self.visit_tuple(id.0),
            TypeData::Function(id) => self.visit_function(id.0),
            TypeData::Callable(id) => self.visit_callable(id.0),
            TypeData::TypeParameter(info) => self.visit_type_parameter(info),
            TypeData::BoundParameter(index) => self.visit_bound_parameter(*index),
            TypeData::Lazy(def_id) => self.visit_lazy(def_id.0),
            TypeData::Recursive(index) => self.visit_recursive(*index),
            TypeData::Enum(def_id, member_type) => self.visit_enum(def_id.0, *member_type),
            TypeData::Application(id) => self.visit_application(id.0),
            TypeData::Conditional(id) => self.visit_conditional(id.0),
            TypeData::Mapped(id) => self.visit_mapped(id.0),
            TypeData::IndexAccess(obj, key) => self.visit_index_access(*obj, *key),
            TypeData::TemplateLiteral(id) => self.visit_template_literal(id.0),
            TypeData::TypeQuery(sym_ref) => self.visit_type_query(sym_ref.0),
            TypeData::KeyOf(type_id) => self.visit_keyof(*type_id),
            TypeData::ReadonlyType(inner) => self.visit_readonly_type(*inner),
            TypeData::UniqueSymbol(sym_ref) => self.visit_unique_symbol(sym_ref.0),
            TypeData::Infer(info) => self.visit_infer(info),
            TypeData::ThisType => self.visit_this_type(),
            TypeData::StringIntrinsic { kind, type_arg } => {
                self.visit_string_intrinsic(*kind, *type_arg)
            }
            TypeData::ModuleNamespace(sym_ref) => self.visit_module_namespace(sym_ref.0),
            TypeData::NoInfer(inner) => self.visit_no_infer(*inner),
            TypeData::Substitution {
                base_type,
                constraint,
            } => self.visit_substitution(*base_type, *constraint),
            TypeData::UnresolvedTypeName(_) | TypeData::Error => self.visit_error(),
        }
    }
}

// =============================================================================
// Type Traversal Helpers
// =============================================================================

/// Invoke a function on each immediate child `TypeId` of a `TypeData`.
///
/// This function provides a simple way to traverse the type graph without
/// requiring the full Visitor pattern. It's useful for operations like:
/// - Populating caches (ensuring all nested types are resolved)
/// - Collecting dependencies
/// - Type environment population
///
/// # Parameters
///
/// * `db` - The type database to look up type structures
/// * `key` - The `TypeData` whose children should be visited
/// * `f` - Function to call for each child `TypeId`
///
/// # Examples
///
/// ```text
/// use crate::visitor::for_each_child;
///
/// for_each_child(types, &type_key, |child_id| {
///     // Process each nested type
/// });
/// ```
///
/// # `TypeData` Variants Handled
///
/// This is the [`ChildPolicy::FULL`] traversal of the canonical
/// policy-parameterized enumerator: every `TypeData` variant is handled, with
/// the full structural surface visited (application bases, property write
/// types, index-signature keys, signature `this`/predicate/type-parameter
/// metadata). Walkers that need a different child set drive
/// [`crate::visitors::child_policy::try_for_each_child_with_policy`] with
/// their own [`ChildPolicy`].
pub fn for_each_child<F>(db: &dyn TypeDatabase, key: &TypeData, f: F)
where
    F: FnMut(TypeId),
{
    for_each_child_with_policy(db, key, &ChildPolicy::FULL, f);
}

/// Walk all transitively referenced type IDs from `root`.
///
/// Convenience wrapper around [`for_each_child`] that takes a `TypeId` instead of `&TypeData`.
///
/// Looks up the type data for `type_id` and visits its direct children.
/// If the type cannot be resolved, this is a no-op.
pub fn for_each_child_by_id<F>(db: &dyn TypeDatabase, type_id: TypeId, f: F)
where
    F: FnMut(TypeId),
{
    // Fast path: intrinsic types have no children. `is_intrinsic()` is a
    // free `TypeId`-range check; skipping the `TypeData` lookup and
    // `for_each_child` match dispatch saves wasted work on every leaf
    // visit. Mirrors #2001 / #2005 / #2008.
    if type_id.is_intrinsic() {
        return;
    }
    if let Some(type_data) = db.lookup(type_id) {
        for_each_child(db, &type_data, f);
    }
}

// Reusable scratch buffers for `walk_referenced_types`. The visited-set and
// stack are both keyed by `TypeId` and have no per-call state to preserve, so
// pool them across calls to avoid one fresh `FxHashSet` + `Vec` allocation
// per invocation. Reentrant calls (when `f` itself calls
// `walk_referenced_types`) fall through to fresh allocations because `take()`
// has already emptied the slot. Per docs/plan/PERFORMANCE_PLAN.md §6.3.
type WalkPool = (FxHashSet<TypeId>, Vec<TypeId>);

thread_local! {
    static WALK_POOL: RefCell<Option<WalkPool>> = const { RefCell::new(None) };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReferencedTypeWalkState {
    Entered,
    AlreadyVisited,
}

impl ReferencedTypeWalkState {
    fn enter(visited: &mut FxHashSet<TypeId>, current: TypeId) -> Self {
        if visited.insert(current) {
            Self::Entered
        } else {
            Self::AlreadyVisited
        }
    }
}

/// The callback is invoked once per unique reachable type (including `root`).
pub fn walk_referenced_types<F>(types: &dyn TypeDatabase, root: TypeId, f: F)
where
    F: FnMut(TypeId),
{
    walk_referenced_types_with_policy(types, root, &ChildPolicy::FULL, f);
}

/// [`walk_referenced_types`] for declaration-emit portability checks
/// (TS2883 and friends): mapped key positions (`constraint`/`name_type`)
/// are not treated as references — declaration emit serializes a mapped
/// type's keys as property names, never as printed type references. Value
/// positions (`template`) are still walked.
pub fn walk_declaration_portability_referenced_types<F>(
    types: &dyn TypeDatabase,
    root: TypeId,
    f: F,
) where
    F: FnMut(TypeId),
{
    walk_referenced_types_with_policy(types, root, &ChildPolicy::DECLARATION_PORTABILITY, f);
}

/// [`walk_referenced_types`] with an explicit [`ChildPolicy`] selecting which
/// child positions count as references. The callback is invoked once per
/// unique reachable type (including `root`).
pub(crate) fn walk_referenced_types_with_policy<F>(
    types: &dyn TypeDatabase,
    root: TypeId,
    policy: &ChildPolicy,
    mut f: F,
) where
    F: FnMut(TypeId),
{
    let mut pool = WALK_POOL
        .with(|p| p.borrow_mut().take())
        .unwrap_or_else(|| (FxHashSet::default(), Vec::new()));
    let (visited, stack) = &mut pool;
    visited.clear();
    stack.clear();
    stack.push(root);

    while let Some(current) = stack.pop() {
        match ReferencedTypeWalkState::enter(visited, current) {
            ReferencedTypeWalkState::Entered => {}
            ReferencedTypeWalkState::AlreadyVisited => continue,
        }
        f(current);

        // Fast path: intrinsic types have no children. `is_intrinsic()`
        // is a free `TypeId`-range check, so skip the `TypeData` lookup
        // and the `for_each_child` match dispatch when we already know
        // there is nothing to push onto the stack. Mirrors #2001's
        // pattern for `ShapeExtractor::extract`.
        if current.is_intrinsic() {
            continue;
        }

        let Some(key) = types.lookup(current) else {
            continue;
        };
        for_each_child_with_policy(types, &key, policy, |child| stack.push(child));
    }

    // Return the pool, keeping whichever allocation has the larger visited-set
    // capacity (a proxy for "saw the bigger graph"). Reentrant inner calls
    // race with us here; if they have already deposited their smaller pool,
    // we still win.
    WALK_POOL.with(|p| {
        let mut slot = p.borrow_mut();
        let keep = match &*slot {
            None => true,
            Some((existing, _)) => pool.0.capacity() >= existing.capacity(),
        };
        if keep {
            *slot = Some(pool);
        }
    });
}

/// Invoke `f` for every lazy `DefId` reachable from `root`, in walk order and
/// **without** de-duplication — a `DefId` reachable by more than one path is
/// reported once per occurrence.
///
/// This is the allocation-free core of [`collect_lazy_def_ids`]. A caller that
/// already maintains its own visited set (the eval-cache dependency collector
/// does) feeds this directly instead of materializing an intermediate deduped
/// `Vec` only to re-deduplicate it, which is redundant work on a hot memoization
/// path.
pub fn for_each_lazy_def_id(types: &dyn TypeDatabase, root: TypeId, mut f: impl FnMut(DefId)) {
    walk_referenced_types(types, root, |type_id| {
        if type_id.is_intrinsic() {
            return;
        }
        if let Some(TypeData::Lazy(def_id)) = types.lookup(type_id) {
            f(def_id);
        }
    });
}

/// Collect all unique lazy `DefIds` reachable from `root`.
pub fn collect_lazy_def_ids(types: &dyn TypeDatabase, root: TypeId) -> Vec<DefId> {
    let mut out = Vec::new();
    let mut seen = FxHashSet::default();

    for_each_lazy_def_id(types, root, |def_id| {
        if seen.insert(def_id) {
            out.push(def_id);
        }
    });

    out
}

/// If every type reachable from `root` is an intrinsic, a [`TypeData::Union`]
/// container, or a *bare* [`TypeData::Lazy`] reference — i.e. `root` is a
/// (possibly nested) union of intrinsics and bare lazy references with no other
/// structure ([`TypeData::Application`], conditional, function, object, type
/// parameter, index-access, mapped, ...) — return the de-duplicated `DefId`s of
/// those lazy references. Otherwise return `None`.
///
/// Used by relation-input readiness to recognise a call return type whose
/// referenced interfaces may be deferred to on-demand forcing: the caller
/// additionally requires every returned `DefId` to be a force-eligible simple
/// lib interface and the set to be non-empty (a resolution-*dependent* return —
/// anything that is not a plain union of intrinsics and bare lib refs — is read
/// structurally by downstream computation and must not be deferred).
pub fn union_of_bare_lazy_def_ids(types: &dyn TypeDatabase, root: TypeId) -> Option<Vec<DefId>> {
    let mut out = Vec::new();
    let mut seen = FxHashSet::default();
    let mut only_union_and_lazy = true;

    walk_referenced_types(types, root, |type_id| {
        if !only_union_and_lazy || type_id.is_intrinsic() {
            return;
        }
        match types.lookup(type_id) {
            Some(TypeData::Union(_)) => {}
            Some(TypeData::Lazy(def_id)) => {
                if seen.insert(def_id) {
                    out.push(def_id);
                }
            }
            _ => only_union_and_lazy = false,
        }
    });

    only_union_and_lazy.then_some(out)
}

/// Return whether `root` contains `Lazy(target_def_id)`.
pub fn contains_lazy_def_id(types: &dyn TypeDatabase, root: TypeId, target_def_id: DefId) -> bool {
    let mut found = false;

    walk_referenced_types(types, root, |type_id| {
        if found || type_id.is_intrinsic() {
            return;
        }
        if let Some(TypeData::Lazy(def_id)) = types.lookup(type_id)
            && def_id == target_def_id
        {
            found = true;
        }
    });

    found
}

/// Whether `type_id` is itself an `Application` whose base is
/// `Lazy(target_def_id)` and whose arguments are all concrete (no free type
/// parameters or infer types). A concrete self-referential `Application` that
/// survives evaluation is the residual shape the TS2589 convergence checks look
/// for. Shared by the three walkers below so the match stays in one place.
fn is_concrete_application_of(
    types: &dyn TypeDatabase,
    type_id: TypeId,
    target_def_id: DefId,
) -> bool {
    let Some(TypeData::Application(app_id)) = types.lookup(type_id) else {
        return false;
    };
    let app = types.type_application(app_id);
    if types.lookup(app.base) != Some(TypeData::Lazy(target_def_id)) {
        return false;
    }
    !app.args
        .iter()
        .any(|&arg| super::visitor_predicates::contains_type_parameters(types, arg))
}

/// Check if `root` contains an Application whose base is `Lazy(target_def_id)`
/// and whose arguments are all concrete (no type parameters or infer types).
/// A concrete self-referential Application that survives evaluation indicates
/// infinite recursion (TS2589).
pub fn contains_concrete_application_with_def(
    types: &dyn TypeDatabase,
    root: TypeId,
    target_def_id: DefId,
) -> bool {
    let mut found = false;
    walk_referenced_types(types, root, |type_id| {
        found = found || is_concrete_application_of(types, type_id, target_def_id);
    });
    found
}

/// Collect every unique concrete `Application` of `target_def_id` reachable
/// from `root` (the application's arguments contain no free type parameters).
///
/// This is the collecting counterpart of [`contains_concrete_application_with_def`].
/// It lets callers re-drive a recursive alias's residual self-applications —
/// which the evaluator may leave deferred in non-tail positions such as a
/// function return type or object property — to a fixpoint, so a *terminating*
/// recursion is not mistaken for an infinite one.
pub fn collect_concrete_applications_with_def(
    types: &dyn TypeDatabase,
    root: TypeId,
    target_def_id: DefId,
) -> Vec<TypeId> {
    let mut out = Vec::new();
    let mut seen = FxHashSet::default();
    walk_referenced_types(types, root, |type_id| {
        if is_concrete_application_of(types, type_id, target_def_id) && seen.insert(type_id) {
            out.push(type_id);
        }
    });
    out
}

/// Like [`collect_concrete_applications_with_def`], but only collects residual
/// self-applications reachable through *eager* type positions — it prunes the
/// structural-deferral boundaries `tsc` never eagerly instantiates: object and
/// callable property/index value types, function/constructor signature bodies,
/// and a mapped type's template and key metadata.
///
/// The use-site TS2589 convergence check treats a residual whose argument
/// weight grew as evidence of infinite instantiation. That is only sound when
/// the residual sits at a position `tsc` eagerly expands — a tuple/array
/// element, a union/intersection member, an indexed-access/`keyof` operand, a
/// resolved conditional branch, or an application argument. A residual left in
/// a *deferred* position is exactly how `tsc` ties a finite knot for recursive
/// object/function/mapped types: `type Rec<T> = { a: Rec<[T]> }` and
/// `type Nest<N extends unknown[]> = N["length"] extends 60 ? number : { a:
/// Nest<[unknown, ...N]> }` both resolve to a concrete object with the
/// recursive call deferred in a property, and `tsc` reports no error at the
/// definition/use site whether or not that recursion is bounded. Counting such
/// a deferred residual as divergence produced a spurious `TS2589` (#17028).
pub fn collect_eager_concrete_applications_with_def(
    types: &dyn TypeDatabase,
    root: TypeId,
    target_def_id: DefId,
) -> Vec<TypeId> {
    let mut out = Vec::new();
    let mut seen = FxHashSet::default();
    let mut stack = vec![root];
    while let Some(type_id) = stack.pop() {
        if type_id.is_intrinsic() || !seen.insert(type_id) {
            continue;
        }
        let Some(data) = types.lookup(type_id) else {
            continue;
        };
        // `seen` already dedups each `type_id` on pop, so collect unconditionally.
        if is_concrete_application_of(types, type_id, target_def_id) {
            out.push(type_id);
        }
        push_eager_children(types, &data, &mut stack);
    }
    out
}

/// Push the children of `data` that occupy *eager* type positions onto `stack`,
/// pruning the structural-deferral boundaries `tsc` never eagerly instantiates.
/// The eager set is the child surface of [`collect_concrete_applications_with_def`]
/// minus object/callable member value types, function/constructor signature
/// bodies, and mapped-type template/key metadata (see
/// [`collect_eager_concrete_applications_with_def`]).
///
/// This deliberately does not route through the central
/// `try_for_each_child_with_policy` enumerator: no `ChildPolicy` flag can
/// express this prune. Object/callable member *value* types and non-generic
/// signature bodies are always visited (no gating flag), and a mapped type's
/// `template` is gated only by `deferred_operations` — which also controls the
/// conditional/indexed-access/`keyof`/template descent the eager walk must keep,
/// so mapped cannot be dropped without dropping those too. The arms below are
/// exhaustive (no `_`) so a new `TypeData` variant is a compile error here, not
/// a silent misclassification.
fn push_eager_children(types: &dyn TypeDatabase, data: &TypeData, stack: &mut Vec<TypeId>) {
    match data {
        TypeData::Array(inner) | TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner) => {
            stack.push(*inner);
        }
        TypeData::Substitution {
            base_type,
            constraint,
        } => {
            stack.push(*base_type);
            stack.push(*constraint);
        }
        TypeData::KeyOf(inner) => stack.push(*inner),
        TypeData::Union(list_id) | TypeData::Intersection(list_id) => {
            for &member in types.type_list(*list_id).iter() {
                stack.push(member);
            }
        }
        TypeData::Tuple(tuple_id) => {
            for elem in types.tuple_list(*tuple_id).iter() {
                stack.push(elem.type_id);
            }
        }
        TypeData::Application(app_id) => {
            let app = types.type_application(*app_id);
            for &arg in &app.args {
                stack.push(arg);
            }
        }
        TypeData::Conditional(cond_id) => {
            let cond = types.get_conditional(*cond_id);
            stack.push(cond.check_type);
            stack.push(cond.extends_type);
            stack.push(cond.true_type);
            stack.push(cond.false_type);
        }
        TypeData::IndexAccess(obj, idx) => {
            stack.push(*obj);
            stack.push(*idx);
        }
        TypeData::TemplateLiteral(template_id) => {
            for span in types.template_list(*template_id).iter() {
                if let crate::types::TemplateSpan::Type(type_id) = span {
                    stack.push(*type_id);
                }
            }
        }
        TypeData::StringIntrinsic { type_arg, .. } => stack.push(*type_arg),
        TypeData::Enum(_def_id, member_type) => stack.push(*member_type),
        // Deferred boundaries: `tsc` never eagerly instantiates object/callable
        // member value types, function/constructor signatures, or a mapped
        // template, so a residual reachable only through them is not divergence
        // evidence. Everything else below is a leaf or a bound position.
        TypeData::Object(_)
        | TypeData::ObjectWithIndex(_)
        | TypeData::Function(_)
        | TypeData::Callable(_)
        | TypeData::Mapped(_)
        | TypeData::TypeParameter(_)
        | TypeData::Infer(_)
        | TypeData::Intrinsic(_)
        | TypeData::Literal(_)
        | TypeData::Lazy(_)
        | TypeData::Recursive(_)
        | TypeData::BoundParameter(_)
        | TypeData::TypeQuery(_)
        | TypeData::UniqueSymbol(_)
        | TypeData::ThisType
        | TypeData::ModuleNamespace(_)
        | TypeData::UnresolvedTypeName(_)
        | TypeData::Error => {}
    }
}

/// Cheap structural-weight estimate of a single recursive type argument — the
/// shared divergent-growth metric used both by the evaluator (to bound
/// tail-recursive expansion) and by the checker's TS2589 convergence check.
///
/// Measures the dimensions along which recursive arguments shrink or grow
/// between recursion steps: concrete string-literal length, generic
/// template-literal span count, tuple arity, and union/intersection arity. Other
/// shapes count as a single unit. Intentionally shallow — one level for
/// lists/spans — so the estimate stays O(arity) and never walks an exploding
/// type tree.
pub fn recursive_growth_weight(types: &dyn TypeDatabase, type_id: TypeId) -> u64 {
    match types.lookup(type_id) {
        Some(TypeData::Literal(LiteralValue::String(atom))) => {
            types.resolve_atom_ref(atom).as_ref().len() as u64
        }
        Some(TypeData::TemplateLiteral(spans)) => types.template_list(spans).len() as u64,
        Some(TypeData::Tuple(list)) => types.tuple_list(list).len() as u64,
        Some(TypeData::Union(list) | TypeData::Intersection(list)) => {
            types.type_list(list).len() as u64
        }
        _ => 1,
    }
}

/// Per-argument weight along the dimensions that genuinely grow *without bound*
/// across recursion steps — concrete string-literal length, generic
/// template-literal span count, and tuple arity. Used by the checker's use-site
/// TS2589 convergence check to decide whether a residual self-application is
/// diverging.
///
/// Unlike [`recursive_growth_weight`], it deliberately does **not** count
/// union/intersection arity: a homomorphic mapped type over an object with
/// optional properties reintroduces a *bounded* `| undefined` on each structural
/// descent (`{ [K in keyof T]: Rec<T[K]> }`), which is not evidence of
/// divergence — `tsc` ties a finite knot for such recursive object/mapped types.
/// Genuine distributive union/intersection blow-ups are still caught earlier, in
/// the first-pass evaluation, by the evaluator's `detect_recursive_growth` (which
/// uses the full metric) and by the per-`DefId` instantiation-depth limit.
/// Counting only the unbounded eager-growth dimensions here keeps real
/// string/tuple/template builders flagged while leaving knot-tying object and
/// counter-driven recursions (`DeepObject<T, N>`) alone.
fn unbounded_growth_weight(types: &dyn TypeDatabase, type_id: TypeId) -> u64 {
    match types.lookup(type_id) {
        Some(TypeData::Literal(LiteralValue::String(atom))) => {
            types.resolve_atom_ref(atom).as_ref().len() as u64
        }
        Some(TypeData::TemplateLiteral(spans)) => types.template_list(spans).len() as u64,
        Some(TypeData::Tuple(list)) => types.tuple_list(list).len() as u64,
        _ => 0,
    }
}

/// Follow a `Lazy(DefId)` reference (bounded) to the underlying type so the
/// growth metric measures the *resolved* shape rather than treating the
/// reference as a single opaque unit.
///
/// A named alias argument such as `type TN = [0, 0]` must weigh the same as the
/// inline `[0, 0]` it stands for. Without this, the use-site convergence check
/// (see [`self_application_arg_weight`]) compares an unresolved `Lazy` arg on the
/// *input* side (scored as a single unit) against the resolved tuple / string /
/// template that the evaluator left in a *residual* self-application, so a
/// recursion that is actually shrinking (`Nest<T, TN>` -> `Nest<T, [...]>` with a
/// shorter tail) is misread as growing and trips a spurious TS2589. The bounded
/// hop count mirrors the alias-resolution cap in
/// `evaluate_rules::keyof::resolve_index_signature_key_alias`.
fn resolve_lazy_for_growth_weight<R: TypeResolver>(
    types: &dyn TypeDatabase,
    resolver: &R,
    type_id: TypeId,
) -> TypeId {
    let mut current = type_id;
    for _ in 0..8 {
        let Some(TypeData::Lazy(def_id)) = types.lookup(current) else {
            return current;
        };
        match resolver.resolve_lazy(def_id, types) {
            Some(next) if next != current => current = next,
            _ => return current,
        }
    }
    current
}

/// Total structural weight of the arguments of a concrete `Application` of
/// `target_def_id`, or `None` if `type_id` is not such an application.
///
/// Lets callers compare a recursive alias's input application against the
/// residual self-applications left in its evaluated result: a residual whose
/// argument weight is strictly smaller is making progress toward the base case
/// (e.g. a variadic tuple tail that loses an element each step) and is not, on
/// its own, evidence of an infinite instantiation.
///
/// Each argument is first resolved through any `Lazy(DefId)` alias indirection
/// (see [`resolve_lazy_for_growth_weight`]) so that a named-alias argument and
/// its inline expansion weigh identically — the input and residual sides of the
/// comparison must measure the same shape regardless of whether the argument was
/// written as an alias reference or spelled out inline.
pub fn self_application_arg_weight<R: TypeResolver>(
    types: &dyn TypeDatabase,
    resolver: &R,
    type_id: TypeId,
    target_def_id: DefId,
) -> Option<u64> {
    if let Some(TypeData::Application(app_id)) = types.lookup(type_id) {
        let app = types.type_application(app_id);
        if let Some(TypeData::Lazy(def_id)) = types.lookup(app.base)
            && def_id == target_def_id
        {
            return Some(
                app.args
                    .iter()
                    .map(|&a| {
                        let resolved = resolve_lazy_for_growth_weight(types, resolver, a);
                        unbounded_growth_weight(types, resolved)
                    })
                    .sum(),
            );
        }
    }
    None
}

/// Collect all unique enum `DefIds` reachable from `root`.
pub fn collect_enum_def_ids(types: &dyn TypeDatabase, root: TypeId) -> Vec<DefId> {
    let mut out = Vec::new();
    let mut seen = FxHashSet::default();

    walk_referenced_types(types, root, |type_id| {
        if type_id.is_intrinsic() {
            return;
        }
        if let Some(TypeData::Enum(def_id, _)) = types.lookup(type_id)
            && seen.insert(def_id)
        {
            out.push(def_id);
        }
    });

    out
}

/// Collect all unique type-query symbol references reachable from `root`.
///
/// Most types reachable on the hot symbol-resolution and relation-closure
/// paths contain no `typeof X` at all (lib-interface closures, instantiated
/// object shapes), yet callers re-walk the full closure on every resolution.
/// The memoized [`contains_type_query_full_db`](crate::type_queries::contains_type_query_full_db)
/// predicate answers "is there any `TypeQuery` under `root`?" in O(1) after
/// the first walk (project-wide per-node cache), so gate the unmemoized full
/// traversal on it: when no `TypeQuery` exists the returned set is necessarily
/// empty and every caller's loop body is a no-op. This defers the eager
/// closure walk to the rare types that actually carry `typeof` references,
/// matching tsc's demand-driven `typeof` resolution.
///
/// The gate uses the *full*-reachability predicate, not the narrower
/// `contains_type_query_db` (which uses `ChildPolicy::CONTENT_PREDICATE` for
/// eval-cache suppression and skips e.g. `Application` bases). The gate must
/// agree with this function's [`walk_referenced_types`] walk (`ChildPolicy::FULL`)
/// or a `typeof X` reachable only through a skipped position — as in
/// `InstanceType<typeof Anon<T>>`, where `typeof Anon` is an `Application`
/// base — would be silently dropped, breaking `typeof` resolution downstream.
pub fn collect_type_queries(types: &dyn TypeDatabase, root: TypeId) -> Vec<SymbolRef> {
    if !crate::type_queries::contains_type_query_full_db(types, root) {
        return Vec::new();
    }

    let mut out = Vec::new();
    let mut seen = FxHashSet::default();

    walk_referenced_types(types, root, |type_id| {
        if type_id.is_intrinsic() {
            return;
        }
        if let Some(TypeData::TypeQuery(symbol_ref)) = types.lookup(type_id)
            && seen.insert(symbol_ref)
        {
            out.push(symbol_ref);
        }
    });

    out
}

// =============================================================================
// Common Visitor Implementations
// =============================================================================

/// Classification of types into broad categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TypeKind {
    /// Primitive types (string, number, boolean, etc.)
    Primitive,
    /// Literal types ("hello", 42, true)
    Literal,
    /// Object types
    Object,
    /// Array types
    Array,
    /// Tuple types
    Tuple,
    /// Union types
    Union,
    /// Intersection types
    Intersection,
    /// Function/callable types
    Function,
    /// Generic types (type applications)
    Generic,
    /// Type parameters (T, K, etc.)
    TypeParameter,
    /// Conditional types (T extends U ? X : Y)
    Conditional,
    /// Mapped types ({ [K in Keys]: V })
    Mapped,
    /// Index access types (T[K])
    IndexAccess,
    /// Template literal types (`hello${T}`)
    TemplateLiteral,
    /// Type query (typeof expr)
    TypeQuery,
    /// `KeyOf` types (keyof T)
    KeyOf,
    /// Named type references (interfaces, type aliases)
    Reference,
    /// Error types
    Error,
    /// Other/unknown
    Other,
}

/// Visitor that checks if a type is a specific `TypeKind`.
pub struct TypeKindVisitor {
    /// The kind to check for.
    pub target_kind: TypeKind,
}

impl TypeKindVisitor {
    /// Create a new `TypeKindVisitor`.
    pub const fn new(target_kind: TypeKind) -> Self {
        Self { target_kind }
    }

    /// Get the kind of a type from its `TypeData`.
    pub const fn get_kind(type_key: &TypeData) -> TypeKind {
        match type_key {
            TypeData::Intrinsic(_)
            | TypeData::Enum(_, _)
            | TypeData::UniqueSymbol(_)
            | TypeData::StringIntrinsic { .. } => TypeKind::Primitive,
            TypeData::Literal(_) => TypeKind::Literal,
            TypeData::Object(_) | TypeData::ObjectWithIndex(_) | TypeData::ModuleNamespace(_) => {
                TypeKind::Object
            }
            TypeData::Array(_) => TypeKind::Array,
            TypeData::Tuple(_) => TypeKind::Tuple,
            TypeData::Union(_) => TypeKind::Union,
            TypeData::Intersection(_) => TypeKind::Intersection,
            TypeData::Function(_) | TypeData::Callable(_) => TypeKind::Function,
            TypeData::Application(_) => TypeKind::Generic,
            TypeData::TypeParameter(_) | TypeData::Infer(_) | TypeData::BoundParameter(_) => {
                TypeKind::TypeParameter
            }
            TypeData::Conditional(_) => TypeKind::Conditional,
            TypeData::Lazy(_) | TypeData::Recursive(_) => TypeKind::Reference,
            TypeData::Mapped(_) => TypeKind::Mapped,
            TypeData::IndexAccess(_, _) => TypeKind::IndexAccess,
            TypeData::TemplateLiteral(_) => TypeKind::TemplateLiteral,
            TypeData::TypeQuery(_) => TypeKind::TypeQuery,
            TypeData::KeyOf(_) => TypeKind::KeyOf,
            TypeData::ReadonlyType(_inner) => {
                // Readonly doesn't change the kind - look through it
                // Note: This requires lookup which we don't have here
                // For now, return Other and let callers handle it
                TypeKind::Other
            }
            TypeData::NoInfer(_inner) => {
                // NoInfer doesn't change the kind - look through it
                TypeKind::Other
            }
            TypeData::Substitution { .. } => {
                // Substitution presents its base type-variable surface.
                TypeKind::TypeParameter
            }
            TypeData::ThisType => TypeKind::TypeParameter, // this is type-parameter-like
            TypeData::Error | TypeData::UnresolvedTypeName(_) => TypeKind::Error,
        }
    }

    /// Get the kind of a type by `TypeId`.
    pub fn get_kind_of(types: &dyn TypeDatabase, type_id: TypeId) -> TypeKind {
        match types.lookup(type_id) {
            Some(ref type_key) => Self::get_kind(type_key),
            None => TypeKind::Other,
        }
    }
}

impl TypeVisitor for TypeKindVisitor {
    type Output = bool;

    fn visit_intrinsic(&mut self, _kind: IntrinsicKind) -> Self::Output {
        self.target_kind == TypeKind::Primitive
    }

    fn visit_literal(&mut self, _value: &LiteralValue) -> Self::Output {
        self.target_kind == TypeKind::Literal
    }

    fn visit_type_key(&mut self, _types: &dyn TypeDatabase, type_key: &TypeData) -> Self::Output {
        Self::get_kind(type_key) == self.target_kind
    }

    fn default_output() -> Self::Output {
        false
    }
}

/// Visitor that checks if a type matches a specific predicate.
pub struct TypePredicateVisitor<F>
where
    F: Fn(&TypeData) -> bool,
{
    /// Predicate function to test against `TypeData`.
    pub predicate: F,
}

impl<F> TypePredicateVisitor<F>
where
    F: Fn(&TypeData) -> bool,
{
    /// Create a new `TypePredicateVisitor`.
    pub const fn new(predicate: F) -> Self {
        Self { predicate }
    }
}

impl<F> TypeVisitor for TypePredicateVisitor<F>
where
    F: Fn(&TypeData) -> bool,
{
    type Output = bool;

    fn visit_type_key(&mut self, _types: &dyn TypeDatabase, type_key: &TypeData) -> Self::Output {
        (self.predicate)(type_key)
    }

    fn visit_intrinsic(&mut self, _kind: IntrinsicKind) -> Self::Output {
        false
    }

    fn visit_literal(&mut self, _value: &LiteralValue) -> Self::Output {
        false
    }

    fn default_output() -> Self::Output {
        false
    }
}

// =============================================================================
// Convenience Functions
// =============================================================================

/// Check if a type is a specific kind using the `TypeKindVisitor`.
///
/// # Example
///
/// ```text
/// use crate::visitor::{is_type_kind, TypeKind};
///
/// let is_object = is_type_kind(&types, type_id, TypeKind::Object);
/// ```
pub fn is_type_kind(types: &dyn TypeDatabase, type_id: TypeId, kind: TypeKind) -> bool {
    let mut visitor = TypeKindVisitor::new(kind);
    visitor.visit_type(types, type_id)
}

/// Collect all types referenced by a type.
///
/// # Example
///
/// ```text
/// use crate::visitor::collect_referenced_types;
///
/// let types = collect_referenced_types(&type_interner, type_id);
/// ```
pub fn collect_referenced_types(types: &dyn TypeDatabase, type_id: TypeId) -> FxHashSet<TypeId> {
    collect_all_types(types, type_id)
}

/// Test a type against a predicate function.
///
/// # Example
///
/// ```text
/// use crate::{TypeData, LiteralValue, visitor::test_type};
///
/// let is_string_literal = test_type(&types, type_id, |key| {
///     matches!(key, TypeData::Literal(LiteralValue::String(_)))
/// });
/// ```
pub fn test_type<F>(types: &dyn TypeDatabase, type_id: TypeId, predicate: F) -> bool
where
    F: Fn(&TypeData) -> bool,
{
    let mut visitor = TypePredicateVisitor::new(predicate);
    visitor.visit_type(types, type_id)
}

// =============================================================================
// Recursive Type Visitor - Traverses into nested types
// =============================================================================

/// A visitor that recursively collects all types referenced by a root type.
/// Properly traverses into nested structures (objects, callables, tuples, etc.).
pub struct RecursiveTypeCollector<'a> {
    types: &'a dyn TypeDatabase,
    collected: FxHashSet<TypeId>,
    guard: crate::recursion::RecursionGuard<TypeId>,
}

impl<'a> RecursiveTypeCollector<'a> {
    pub fn new(types: &'a dyn TypeDatabase) -> Self {
        Self {
            types,
            collected: FxHashSet::default(),
            guard: crate::recursion::RecursionGuard::with_profile(
                crate::recursion::RecursionProfile::ShallowTraversal,
            ),
        }
    }

    /// Collect all types reachable from the given type.
    pub fn collect(&mut self, type_id: TypeId) -> FxHashSet<TypeId> {
        self.visit(type_id);
        std::mem::take(&mut self.collected)
    }

    fn visit(&mut self, type_id: TypeId) {
        // Already collected
        if self.collected.contains(&type_id) {
            return;
        }

        // Look up before entering the guard so we can short-circuit
        // terminal kinds without paying the guard's HashSet round-trip.
        let Some(key) = self.types.lookup(type_id) else {
            self.collected.insert(type_id);
            return;
        };

        // Terminal-kind fast path: variants with no children under the
        // collector's policy make `visit_key` a no-op. Skip the recursion
        // guard's enter/leave HashSet bookkeeping — there is no recursion,
        // no cycle, and no depth growth. Mirrors the same fast path used in
        // `DeepContainsChecker` (#1988) and the sibling visitor predicates.
        if !has_policy_children(&key, &ChildPolicy::EVERYTHING) {
            self.collected.insert(type_id);
            return;
        }

        // Non-terminal: enter the guard for cycle/depth/iteration safety
        // and recurse via `visit_key`.
        match self.guard.enter(type_id) {
            crate::recursion::RecursionResult::Entered => {}
            _ => return,
        }

        self.collected.insert(type_id);
        // Shared cross-operation stack-frame breaker (issue #7574). When the
        // combined solver recursion budget is exhausted `with_solver_frame`
        // returns `None` and skips the descent; the node itself is already
        // recorded in `collected`.
        let _ = crate::recursion::with_solver_frame(|| self.visit_key(&key));
        self.guard.leave(type_id);
    }

    fn visit_key(&mut self, key: &TypeData) {
        let types = self.types;
        for_each_child_with_policy(types, key, &ChildPolicy::EVERYTHING, |child| {
            self.visit(child);
        });
    }
}

/// Collect all types recursively reachable from a root type.
pub fn collect_all_types(types: &dyn TypeDatabase, type_id: TypeId) -> FxHashSet<TypeId> {
    let mut collector = RecursiveTypeCollector::new(types);
    collector.collect(type_id)
}

// =============================================================================
// Const Assertion Visitor
// =============================================================================

/// Visitor that applies `as const` transformation to a type.
///
/// This visitor implements the const assertion logic from TypeScript:
/// - Literals: Preserved as-is
/// - Arrays: Converted to readonly tuples
/// - Tuples: Marked readonly, elements recursively const-asserted
/// - Objects: All properties marked readonly, recursively const-asserted
/// - Other types: Preserved as-is (any, unknown, primitives, etc.)
pub struct ConstAssertionVisitor<'a> {
    /// The type database/interner.
    pub db: &'a dyn TypeDatabase,
    /// Unified recursion guard for cycle detection.
    pub guard: crate::recursion::RecursionGuard<TypeId>,
}

impl<'a> ConstAssertionVisitor<'a> {
    /// Create a new `ConstAssertionVisitor`.
    pub fn new(db: &'a dyn TypeDatabase) -> Self {
        Self {
            db,
            guard: crate::recursion::RecursionGuard::with_profile(
                crate::recursion::RecursionProfile::ConstAssertion,
            ),
        }
    }

    /// Apply const assertion to a type, returning the transformed type ID.
    pub fn apply_const_assertion(&mut self, type_id: TypeId) -> TypeId {
        // Terminal-kind fast path: kinds that hit the `_ => type_id` arm
        // below have nothing to recurse into and produce the input type
        // unchanged. Skip the `RecursionGuard::enter`/`leave` `FxHashSet`
        // round-trip for those — the guard only matters when there are
        // children to walk. Mirrors #1988/#1993 (the deep contains walkers)
        // and #1996 (RecursiveTypeCollector).
        let lookup = self.db.lookup(type_id);
        let needs_recursion = matches!(
            lookup,
            Some(
                TypeData::Array(_)
                    | TypeData::Tuple(_)
                    | TypeData::Object(_)
                    | TypeData::ObjectWithIndex(_)
                    | TypeData::ReadonlyType(_)
                    | TypeData::Union(_)
                    | TypeData::Intersection(_)
            )
        );
        if !needs_recursion {
            return type_id;
        }

        // Prevent infinite recursion
        match self.guard.enter(type_id) {
            crate::recursion::RecursionResult::Entered => {}
            _ => return type_id,
        }

        let result = match lookup {
            // Arrays whose type reached const assertion as an array, rather than
            // an array-literal tuple, become readonly arrays.
            Some(TypeData::Array(element_type)) => {
                let const_element = self.apply_const_assertion(element_type);
                self.db.readonly_type(self.db.array(const_element))
            }

            // Tuples: Mark readonly and recurse on elements
            Some(TypeData::Tuple(list_id)) => {
                let elements = self.db.tuple_list(list_id);
                let const_elements: Vec<TupleElement> = elements
                    .iter()
                    .map(|elem| {
                        let const_type = if elem.rest {
                            self.apply_const_assertion_to_tuple_rest_type(elem.type_id)
                        } else {
                            self.apply_const_assertion(elem.type_id)
                        };
                        TupleElement {
                            type_id: const_type,
                            name: elem.name,
                            optional: elem.optional,
                            rest: elem.rest,
                        }
                    })
                    .collect();
                let tuple_type = self.db.tuple(const_elements);
                self.db.readonly_type(tuple_type)
            }

            // Objects: Mark all properties readonly and recurse
            Some(TypeData::Object(shape_id)) => {
                let shape = self.db.object_shape(shape_id);
                let mut new_props = Vec::with_capacity(shape.properties.len());

                for prop in &shape.properties {
                    let const_prop_type = self.apply_const_assertion(prop.type_id);
                    let const_write_type = self.apply_const_assertion(prop.write_type);
                    new_props.push(crate::types::PropertyInfo {
                        name: prop.name,
                        type_id: const_prop_type,
                        write_type: const_write_type,
                        optional: prop.optional,
                        readonly: true, // Mark as readonly
                        is_method: prop.is_method,
                        is_class_prototype: prop.is_class_prototype,
                        visibility: prop.visibility,
                        parent_id: prop.parent_id,
                        declaration_order: prop.declaration_order,
                        is_string_named: prop.is_string_named,
                        is_symbol_named: prop.is_symbol_named,
                        single_quoted_name: prop.single_quoted_name,
                        non_widening: false,
                    });
                }

                // Rebuild preserving the shape's flags (`FRESH_LITERAL` in
                // particular) and declaring symbol: a const assertion keeps
                // the operand's fresh object-literal identity in tsc, so the
                // readonly rebuild must not launder it into an anonymous
                // non-fresh object. Display provenance is carried forward
                // with the assertion applied — the recorded properties are
                // pre-widened display types of the MUTABLE literal, and
                // display surfaces that read them would otherwise drop the
                // `readonly` modifiers the asserted type gained.
                let rebuilt =
                    self.db
                        .object_with_flags_and_symbol(new_props, shape.flags, shape.symbol);
                if rebuilt != type_id
                    && let Some(display_props) = self.db.get_display_properties(type_id)
                {
                    let const_display_props: Vec<crate::types::PropertyInfo> = display_props
                        .iter()
                        .map(|prop| {
                            let mut const_prop = prop.clone();
                            const_prop.type_id = self.apply_const_assertion(prop.type_id);
                            const_prop.write_type = self.apply_const_assertion(prop.write_type);
                            const_prop.readonly = true;
                            const_prop
                        })
                        .collect();
                    crate::diagnostics::display_provenance::record_fresh_object_literal_display(
                        self.db,
                        crate::diagnostics::display_provenance::FreshObjectLiteralDisplayProvenance {
                            type_id: rebuilt,
                            properties: const_display_props,
                        },
                    );
                }
                rebuilt
            }

            // Objects with index signatures
            Some(TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.db.object_shape(shape_id);
                let mut new_props = Vec::with_capacity(shape.properties.len());

                for prop in &shape.properties {
                    let const_prop_type = self.apply_const_assertion(prop.type_id);
                    let const_write_type = self.apply_const_assertion(prop.write_type);
                    new_props.push(crate::types::PropertyInfo {
                        name: prop.name,
                        type_id: const_prop_type,
                        write_type: const_write_type,
                        optional: prop.optional,
                        readonly: true, // Mark as readonly
                        is_method: prop.is_method,
                        is_class_prototype: prop.is_class_prototype,
                        visibility: prop.visibility,
                        parent_id: prop.parent_id,
                        declaration_order: prop.declaration_order,
                        is_string_named: prop.is_string_named,
                        is_symbol_named: prop.is_symbol_named,
                        single_quoted_name: prop.single_quoted_name,
                        non_widening: false,
                    });
                }

                // Mark index signatures as readonly
                let string_index =
                    shape
                        .string_index
                        .as_ref()
                        .map(|idx| crate::types::IndexSignature {
                            key_type: idx.key_type,
                            value_type: self
                                .apply_const_assertion_to_index_signature_value(idx.value_type),
                            readonly: true,
                            param_name: idx.param_name,
                        });

                let number_index =
                    shape
                        .number_index
                        .as_ref()
                        .map(|idx| crate::types::IndexSignature {
                            key_type: idx.key_type,
                            value_type: self
                                .apply_const_assertion_to_index_signature_value(idx.value_type),
                            readonly: true,
                            param_name: idx.param_name,
                        });

                let mut new_shape = (*shape).clone();
                new_shape.properties = new_props;
                new_shape.string_index = string_index;
                new_shape.number_index = number_index;

                self.db.object_with_index(new_shape)
            }

            // Readonly types: Unwrap, process, re-wrap
            Some(TypeData::ReadonlyType(inner)) => {
                let const_inner = self.apply_const_assertion(inner);
                self.db.readonly_type(const_inner)
            }

            // Unions: Recursively apply to all members
            Some(TypeData::Union(list_id)) => {
                let members = self.db.type_list(list_id);
                let const_members: Vec<TypeId> = members
                    .iter()
                    .map(|&m| self.apply_const_assertion(m))
                    .collect();
                self.db.union_preserve_members(const_members)
            }

            // Intersections: Recursively apply to all members
            Some(TypeData::Intersection(list_id)) => {
                let members = self.db.type_list(list_id);
                let const_members: Vec<TypeId> = members
                    .iter()
                    .map(|&m| self.apply_const_assertion(m))
                    .collect();
                self.db.intersection(const_members)
            }

            // All other types: preserved as-is
            _ => type_id,
        };

        self.guard.leave(type_id);
        result
    }

    fn apply_const_assertion_to_tuple_rest_type(&mut self, type_id: TypeId) -> TypeId {
        if let Some(TypeData::Array(element_type)) = self.db.lookup(type_id) {
            let const_element = self.apply_const_assertion(element_type);
            self.db.array(const_element)
        } else {
            self.apply_const_assertion(type_id)
        }
    }

    fn apply_const_assertion_to_index_signature_value(&mut self, type_id: TypeId) -> TypeId {
        if let Some(TypeData::Union(list_id)) = self.db.lookup(type_id) {
            let members = self.db.type_list(list_id);
            let mut const_members: Vec<TypeId> = members
                .iter()
                .map(|&m| self.apply_const_assertion(m))
                .collect();
            let mut seen = FxHashSet::default();
            const_members.retain(|id| *id != TypeId::NEVER && seen.insert(*id));
            return self.db.union_from_sorted_vec(const_members);
        }
        self.apply_const_assertion(type_id)
    }
}

#[cfg(test)]
mod referenced_type_walk_state_tests {
    use super::{ReferencedTypeWalkState, walk_referenced_types};
    use crate::construction::TypeInterner;
    use crate::types::TupleElement;
    use rustc_hash::FxHashSet;

    #[test]
    fn referenced_type_walk_state_names_entered_and_revisit() {
        let db = TypeInterner::new();
        let type_id = db.object(vec![]);
        let mut visited = FxHashSet::default();

        assert_eq!(
            ReferencedTypeWalkState::enter(&mut visited, type_id),
            ReferencedTypeWalkState::Entered
        );
        assert_eq!(
            ReferencedTypeWalkState::enter(&mut visited, type_id),
            ReferencedTypeWalkState::AlreadyVisited
        );
    }

    #[test]
    fn walk_referenced_types_visits_shared_child_once() {
        let db = TypeInterner::new();
        let child = db.object(vec![]);
        let root = db.tuple(vec![TupleElement::fixed(child), TupleElement::fixed(child)]);
        let mut visits = Vec::new();

        walk_referenced_types(&db, root, |type_id| visits.push(type_id));

        assert_eq!(
            visits.iter().filter(|&&type_id| type_id == child).count(),
            1
        );
        assert!(visits.contains(&root));
    }
}

#[cfg(test)]
mod union_of_bare_lazy_def_ids_tests {
    use super::union_of_bare_lazy_def_ids;
    use crate::construction::TypeInterner;
    use crate::def::DefId;
    use crate::types::{FunctionShape, TypeId};

    #[test]
    fn bare_lazy_yields_its_def() {
        let db = TypeInterner::new();
        let lazy = db.lazy(DefId(11));
        assert_eq!(union_of_bare_lazy_def_ids(&db, lazy), Some(vec![DefId(11)]));
    }

    #[test]
    fn union_of_lazies_and_intrinsics_yields_the_lazy_defs() {
        let db = TypeInterner::new();
        // `Lazy(11) | Lazy(12) | null` — the `getElementById`-style return shape.
        let u = db.union(vec![db.lazy(DefId(11)), db.lazy(DefId(12)), TypeId::NULL]);
        let got = union_of_bare_lazy_def_ids(&db, u).expect("union of bare lazies is classified");
        assert!(got.contains(&DefId(11)) && got.contains(&DefId(12)) && got.len() == 2);
    }

    #[test]
    fn pure_intrinsic_is_classified_with_no_defs() {
        let db = TypeInterner::new();
        // Deferrable-shape-wise valid, but the caller requires a non-empty set.
        assert_eq!(
            union_of_bare_lazy_def_ids(&db, TypeId::STRING),
            Some(vec![])
        );
    }

    #[test]
    fn application_is_not_classified() {
        let db = TypeInterner::new();
        // `Lazy(11)<string>` (e.g. `Promise<string>`) is resolution-dependent.
        let app = db.application(db.lazy(DefId(11)), vec![TypeId::STRING]);
        assert_eq!(union_of_bare_lazy_def_ids(&db, app), None);
    }

    #[test]
    fn union_containing_a_non_bare_member_is_not_classified() {
        let db = TypeInterner::new();
        let app = db.application(db.lazy(DefId(12)), vec![TypeId::NUMBER]);
        let u = db.union(vec![db.lazy(DefId(11)), app]);
        assert_eq!(union_of_bare_lazy_def_ids(&db, u), None);
    }

    #[test]
    fn function_type_is_not_classified() {
        let db = TypeInterner::new();
        let func = db.function(FunctionShape {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: db.lazy(DefId(11)),
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });
        assert_eq!(union_of_bare_lazy_def_ids(&db, func), None);
    }
}
