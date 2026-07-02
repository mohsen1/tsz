//! Type Query Functions — Core Implementation
//!
//! This module contains the implementation of type query functions.
//! The parent `mod.rs` re-exports everything; callers should use `type_queries::*`.

use crate::construction::{QueryDatabase, TypeDatabase};
use crate::def::DefinitionStore;
use crate::evaluation::evaluate::evaluate_type;
use crate::types::{IntrinsicKind, LiteralValue};
use crate::{TypeData, TypeId, TypeParamInfo};

use super::classifiers::get_lazy_def_id;
use super::traversal::collect_property_name_atoms_for_diagnostics;

/// tsc-equivalent `isGenericType` for conditional-type branch decisions.
///
/// `getConditionalType` in tsc never resolves a conditional whose (effective)
/// check type is still generic: `isGenericType` = instantiable flags
/// (`TypeParameter` / `infer` / `this` / indexed access / `keyof` /
/// string-mapping / deferred conditional) plus object types whose *type
/// arguments* are generic (type references, tuples, arrays, and
/// unions/intersections of those, and generic mapped types). Crucially it does
/// NOT recurse through object members or function signatures — an anonymous
/// object/function type containing a type parameter in a member position is
/// not "generic" for deferral purposes, so `(x: T) => void extends Function`
/// still resolves eagerly.
///
/// A `KeyOf` / `IndexAccess` / `StringIntrinsic` / `Conditional` node that
/// *survived* evaluation is by construction still deferred (either its operand
/// is generic or its reference could not be resolved yet), so those count as
/// generic without inspecting the operand. This also keeps the decision
/// schedule-independent under parallel fresh checking: an unresolved
/// `keyof Lazy(D)` defers instead of feeding a definitive false branch.
pub fn is_generic_conditional_check_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    // Worklist traversal over the same generic-marker child set the recursive
    // form walked (see the per-`TypeData` arms below). A `visited` set collapses
    // the type DAG — typebox's recursive conditional/template builders reach
    // shared `Application`/`Union`/`TemplateLiteral` subtrees through many
    // parents, so the old depth-recursive `.any()` form re-walked each shared
    // node once per parent path (super-linear, and combined with the large
    // partial types #13652's instantiation-depth bail now surfaces it became a
    // non-terminating re-walk: typebox canary, ~300s CPU-bound timeout). The
    // worklist visits each reachable node at most once, so the cost is
    // O(distinct reachable nodes) per call and the deep recursion is gone.
    //
    // Behavior-preserving: the result is "is a generic-marker node reachable
    // through the marker child set". The `visited` set only prevents re-deriving
    // the same node's contribution — `any`-style reachability is monotone, so
    // collapsing duplicates yields the identical boolean. The recursive form's
    // `depth > 64` cap was a defensive bound against exactly this re-walk (added
    // with the permissive-instantiation gate, #13205); the `visited` set bounds
    // the walk structurally instead, so no reachable marker is ever truncated.
    if type_id.is_intrinsic() {
        return false;
    }
    // Pooled scratch buffers (per #4722/#4790 pattern) so the per-call worklist
    // does not allocate; this predicate runs once per generic-conditional
    // deferral decision (hot on typebox/kysely query builders).
    with_generic_check_buffers(|visited, stack| {
        stack.push(type_id);
        while let Some(current) = stack.pop() {
            if current.is_intrinsic() || !visited.insert(current) {
                continue;
            }
            match db.lookup(current) {
                Some(
                    TypeData::TypeParameter(_)
                    | TypeData::Infer(_)
                    | TypeData::BoundParameter(_)
                    | TypeData::ThisType
                    | TypeData::KeyOf(_)
                    | TypeData::IndexAccess(_, _)
                    | TypeData::StringIntrinsic { .. }
                    | TypeData::Conditional(_),
                ) => return true,
                Some(TypeData::TemplateLiteral(spans)) => {
                    for span in db.template_list(spans).iter() {
                        if let crate::types::TemplateSpan::Type(t) = span {
                            stack.push(*t);
                        }
                    }
                }
                Some(TypeData::Application(app_id)) => {
                    let app = db.type_application(app_id);
                    stack.extend(app.args.iter().copied());
                }
                Some(TypeData::Tuple(elements)) => {
                    stack.extend(db.tuple_list(elements).iter().map(|el| el.type_id));
                }
                Some(
                    TypeData::Array(elem) | TypeData::ReadonlyType(elem) | TypeData::NoInfer(elem),
                ) => {
                    stack.push(elem);
                }
                Some(TypeData::Substitution {
                    base_type,
                    constraint,
                }) => {
                    stack.push(base_type);
                    stack.push(constraint);
                }
                Some(TypeData::Union(list) | TypeData::Intersection(list)) => {
                    stack.extend(db.type_list(list).iter().copied());
                }
                Some(TypeData::Mapped(mapped_id)) => {
                    // Generic mapped type: tsc's `isGenericMappedType` keys off
                    // a still-generic constraint (`[K in keyof T]`).
                    let mapped = db.get_mapped(mapped_id);
                    stack.push(mapped.constraint);
                }
                _ => {}
            }
        }
        false
    })
}

// Reusable scratch buffers for `is_generic_conditional_check_type`'s worklist
// traversal, so the hot per-deferral-decision predicate does not allocate a
// fresh `visited` set + `stack` on every call (#4722/#4790 pool pattern).
type GenericCheckBuffers = (rustc_hash::FxHashSet<TypeId>, Vec<TypeId>);

thread_local! {
    static GENERIC_CHECK_BUFFERS: std::cell::RefCell<Option<GenericCheckBuffers>> =
        const { std::cell::RefCell::new(None) };
}

#[inline]
fn with_generic_check_buffers<R>(
    f: impl FnOnce(&mut rustc_hash::FxHashSet<TypeId>, &mut Vec<TypeId>) -> R,
) -> R {
    let mut bufs = GENERIC_CHECK_BUFFERS
        .with(|p| p.borrow_mut().take())
        .unwrap_or_default();
    bufs.0.clear();
    bufs.1.clear();
    let r = f(&mut bufs.0, &mut bufs.1);
    GENERIC_CHECK_BUFFERS.with(|p| {
        let mut slot = p.borrow_mut();
        let keep = match &*slot {
            None => true,
            Some((existing, _)) => bufs.0.capacity() >= existing.capacity(),
        };
        if keep {
            *slot = Some(bufs);
        }
    });
    r
}

pub fn get_allowed_keys(db: &dyn TypeDatabase, type_id: TypeId) -> rustc_hash::FxHashSet<String> {
    if let Some(exact) = super::data::collect_exact_literal_property_keys(db, type_id) {
        return exact.into_iter().map(|a| db.resolve_atom(a)).collect();
    }
    let atoms = collect_property_name_atoms_for_diagnostics(db, type_id, 10);
    atoms.into_iter().map(|a| db.resolve_atom(a)).collect()
}

pub fn application_base_has_conditional_alias_body(
    db: &dyn TypeDatabase,
    def_store: &DefinitionStore,
    type_id: TypeId,
) -> bool {
    let Some(TypeData::Application(app_id)) = db.lookup(type_id) else {
        return false;
    };
    let app = db.type_application(app_id);
    let Some(def_id) =
        get_lazy_def_id(db, app.base).or_else(|| def_store.find_def_for_type(app.base))
    else {
        return false;
    };
    def_store
        .get(def_id)
        .and_then(|def| def.body)
        .is_some_and(|body| matches!(db.lookup(body), Some(TypeData::Conditional(_))))
}

/// The reducing operator a type alias body bottoms out at, if any.
///
/// tsc's `aliasSymbol` policy hinges on whether instantiating an alias
/// application *resolves the type away*: a conditional takes a branch, an
/// indexed access resolves to the member type, and `keyof` resolves to the key
/// union — none of these constructors stamp the enclosing alias onto their
/// result, so the resolved type is rendered structurally. A mapped, object,
/// union, or intersection body survives instantiation and keeps the alias.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReducingAliasBodyKind {
    Conditional,
    IndexAccess,
    KeyOf,
}

/// Classify whether `type_id` is an application of a type alias whose declared
/// body bottoms out at a reducing operator (conditional / indexed access /
/// `keyof`), following alias-forwarding chains (`type A<T> = B<T>` where `B`'s
/// body reduces, or `type A = B` alias references) to a bounded depth.
///
/// Returns the terminal operator kind, or `None` when the base is not a type
/// alias or its body chain ends at a surviving constructor.
pub fn application_base_reducing_alias_body_kind(
    db: &dyn TypeDatabase,
    def_store: &DefinitionStore,
    type_id: TypeId,
) -> Option<ReducingAliasBodyKind> {
    let Some(TypeData::Application(app_id)) = db.lookup(type_id) else {
        return None;
    };
    let app = db.type_application(app_id);
    let def_id = get_lazy_def_id(db, app.base).or_else(|| def_store.find_def_for_type(app.base))?;
    alias_def_reducing_body_kind(db, def_store, def_id, 0)
}

/// Whether `type_id` is an application of a *distributive* conditional type
/// alias whose check argument is a union (or `boolean`, or a non-generic
/// union-alias reference) — i.e. the application distributes per member and
/// its diagnostic display renders the distributed branch union rather than
/// the alias surface or the collapsed evaluated union.
pub fn application_distributes_over_union_check_arg(
    db: &dyn TypeDatabase,
    def_store: &DefinitionStore,
    type_id: TypeId,
) -> bool {
    let Some(TypeData::Application(app_id)) = db.lookup(type_id) else {
        return false;
    };
    let app = db.type_application(app_id);
    let Some(def_id) =
        get_lazy_def_id(db, app.base).or_else(|| def_store.find_def_for_type(app.base))
    else {
        return false;
    };
    let Some(def) = def_store.get(def_id) else {
        return false;
    };
    if def.kind != crate::def::DefKind::TypeAlias {
        return false;
    }
    let Some(body) = def.body else {
        return false;
    };
    let Some(TypeData::Conditional(cond_id)) = db.lookup(body) else {
        return false;
    };
    let cond = db.conditional_type(cond_id);
    if !cond.is_distributive {
        return false;
    }
    let Some(TypeData::TypeParameter(check_tp)) = db.lookup(cond.check_type) else {
        return false;
    };
    let Some(check_index) = def
        .type_params
        .iter()
        .position(|param| param.name == check_tp.name)
    else {
        return false;
    };
    let Some(&check_arg) = app.args.get(check_index) else {
        return false;
    };
    let check_arg = resolve_distributive_union_check_arg(db, def_store, check_arg);
    if check_arg == TypeId::BOOLEAN {
        return true;
    }
    matches!(db.lookup(check_arg), Some(TypeData::Union(list_id)) if db.type_list(list_id).len() >= 2)
}

/// Resolve a distributive conditional's check argument to the union it
/// distributes over. A directly-written union (or `boolean`) passes through; a
/// non-generic union-alias reference (`NoC<U>` where `type U = A | B`) arrives
/// as `Lazy(def)` and resolves to the alias body, mirroring tsc, which
/// instantiates the reference before distributing. Every other shape returns
/// unchanged (and fails the caller's union check).
pub fn resolve_distributive_union_check_arg(
    db: &dyn TypeDatabase,
    def_store: &DefinitionStore,
    check_arg: TypeId,
) -> TypeId {
    if check_arg == TypeId::BOOLEAN || matches!(db.lookup(check_arg), Some(TypeData::Union(_))) {
        return check_arg;
    }
    let Some(TypeData::Lazy(def_id)) = db.lookup(check_arg) else {
        return check_arg;
    };
    def_store
        .get(def_id)
        .filter(|def| def.kind == crate::def::DefKind::TypeAlias && def.type_params.is_empty())
        .and_then(|def| def.body)
        .filter(|&body| matches!(db.lookup(body), Some(TypeData::Union(_))))
        .unwrap_or(check_arg)
}

fn alias_def_reducing_body_kind(
    db: &dyn TypeDatabase,
    def_store: &DefinitionStore,
    def_id: crate::def::DefId,
    depth: usize,
) -> Option<ReducingAliasBodyKind> {
    if depth > 8 {
        return None;
    }
    let def = def_store.get(def_id)?;
    if def.kind != crate::def::DefKind::TypeAlias {
        return None;
    }
    match db.lookup(def.body?)? {
        TypeData::Conditional(_) => Some(ReducingAliasBodyKind::Conditional),
        TypeData::IndexAccess(_, _) => Some(ReducingAliasBodyKind::IndexAccess),
        TypeData::KeyOf(_) => Some(ReducingAliasBodyKind::KeyOf),
        TypeData::Application(app_id) => {
            let app = db.type_application(app_id);
            let next = get_lazy_def_id(db, app.base)?;
            alias_def_reducing_body_kind(db, def_store, next, depth + 1)
        }
        TypeData::Lazy(next) => alias_def_reducing_body_kind(db, def_store, next, depth + 1),
        _ => None,
    }
}

/// When `type_id` is a plain mutable array of a boolean literal element
/// (`Array<true>` / `Array<false>`), return `Array<boolean>` (which renders as
/// `boolean[]`); otherwise return `None`.
pub fn boolean_literal_array_display_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<TypeId> {
    let TypeData::Array(element) = db.lookup(type_id)? else {
        return None;
    };
    let widened = super::widen_literal_to_primitive(db, element);
    (widened == TypeId::BOOLEAN && widened != element).then(|| db.array(TypeId::BOOLEAN))
}

// =============================================================================
// Core Type Queries
// =============================================================================

/// Check if a type is a callable type (function or callable with signatures).
///
/// Returns true for `TypeData::Callable`, `TypeData::Function`, and the
/// intrinsic `TypeId::FUNCTION` (the global `Function` interface).
pub fn is_callable_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id == TypeId::FUNCTION {
        return true;
    }
    if type_id.is_intrinsic() {
        return false;
    }
    matches!(
        db.lookup(type_id),
        Some(TypeData::Callable(_) | TypeData::Function(_))
    )
}

/// Check whether a constraint is, or evaluates to, a union whose members all
/// carry call or construct signatures.
///
/// Application aliases such as `ComponentType<any>` can evaluate to unions
/// whose members are still application-shaped. Evaluate both the constraint and
/// each union member before deciding, so TS2344 callers can treat the
/// constraint as callable without owning the structural walk in checker code.
pub fn constraint_expands_to_callable_union(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    let Some(members) = union_members_or_evaluated_union_members(db, type_id) else {
        return false;
    };
    !members.is_empty()
        && members
            .iter()
            .all(|&member| type_has_call_or_construct_signature_after_eval(db, member))
}

fn union_members_or_evaluated_union_members(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<Vec<TypeId>> {
    if let Some(TypeData::Union(list_id)) = db.lookup(type_id) {
        let members = db.type_list(list_id);
        return (!members.is_empty()).then(|| members.to_vec());
    }

    let evaluated = evaluate_type(db, type_id);
    if evaluated == type_id {
        return None;
    }
    let Some(TypeData::Union(list_id)) = db.lookup(evaluated) else {
        return None;
    };
    let members = db.type_list(list_id);
    (!members.is_empty()).then(|| members.to_vec())
}

fn type_has_call_or_construct_signature_after_eval(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_has_call_or_construct_signature(db, type_id) {
        return true;
    }
    let evaluated = evaluate_type(db, type_id);
    evaluated != type_id && type_has_call_or_construct_signature(db, evaluated)
}

fn type_has_call_or_construct_signature(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    is_callable_type(db, type_id) || super::data::get_callable_shape_for_type(db, type_id).is_some()
}

/// Check if a type has call signatures (not just construct signatures).
///
/// Returns `true` for `Function` types and `Callable` types that have at least
/// one call signature. Returns `false` for `Callable` types that only have
/// construct signatures (e.g., class constructor types like `typeof MyClass`).
///
/// This is important for distinguishing callable types from constructable types
/// when checking constraints like `T extends (...args: any) => any` which
/// requires call signatures, not construct signatures.
pub fn has_call_signatures(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id == TypeId::FUNCTION {
        return true;
    }
    if type_id.is_intrinsic() {
        return false;
    }
    match db.lookup(type_id) {
        Some(TypeData::Function(_)) => true,
        Some(TypeData::Callable(shape_id)) => {
            let shape = db.callable_shape(shape_id);
            !shape.call_signatures.is_empty()
        }
        _ => false,
    }
}

/// Check if a type is the Function interface from lib.d.ts.
///
/// Delegates to the canonical query in [`super::global_interfaces`]:
/// boxed-registry identity first, then the shared structural fallback
/// (the Function interface may be lowered as an `Object` without call
/// signatures due to cross-arena declaration splitting).
pub fn is_function_interface_structural(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    super::global_interfaces::is_global_function_interface(db, type_id)
}

pub fn type_may_display_iterator_protocol(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    type_may_display_iterator_protocol_inner(db, type_id, 0)
}

fn type_may_display_iterator_protocol_inner(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    depth: usize,
) -> bool {
    if depth > 4 {
        return true;
    }
    if let Some(alias) = db.get_display_alias(type_id)
        && alias != type_id
        && type_may_display_iterator_protocol_inner(db, alias, depth + 1)
    {
        return true;
    }
    // Intrinsics never match any of the iterator-protocol-relevant variants.
    if type_id.is_intrinsic() {
        return false;
    }

    match db.lookup(type_id) {
        Some(TypeData::Application(_))
        | Some(TypeData::Function(_))
        | Some(TypeData::Callable(_))
        | Some(TypeData::Lazy(_))
        | Some(TypeData::Recursive(_))
        | Some(TypeData::Conditional(_))
        | Some(TypeData::Mapped(_))
        | Some(TypeData::IndexAccess(_, _))
        | Some(TypeData::TypeParameter(_))
        | Some(TypeData::BoundParameter(_))
        | Some(TypeData::Infer(_))
        | Some(TypeData::ThisType)
        | Some(TypeData::NoInfer(_)) => true,
        Some(TypeData::Object(_)) | Some(TypeData::ObjectWithIndex(_)) => {
            super::data::type_has_property_by_str(db, type_id, "next")
        }
        Some(TypeData::Union(list_id)) | Some(TypeData::Intersection(list_id)) => db
            .type_list(list_id)
            .iter()
            .any(|member| type_may_display_iterator_protocol_inner(db, *member, depth + 1)),
        _ => false,
    }
}

/// Get the number of elements in a fixed-length tuple type.
///
/// Returns `Some(len)` for tuple types with no rest elements, `None` otherwise
/// (arrays, non-tuples, variadic tuples with rest elements).
pub fn get_fixed_tuple_length(db: &dyn TypeDatabase, type_id: TypeId) -> Option<usize> {
    if type_id.is_intrinsic() {
        return None;
    }
    if let Some(TypeData::Tuple(tuple_list_id)) = db.lookup(type_id) {
        let elements = db.tuple_list(tuple_list_id);
        if elements.iter().all(|e| !e.rest) {
            return Some(elements.len());
        }
    }
    if let Some(TypeData::Substitution { constraint, .. }) = db.lookup(type_id) {
        return get_fixed_tuple_length(db, constraint);
    }
    None
}

/// Positional offset of the first *variable-length* rest element in a tuple
/// spread, or `None` when the tuple is fully fixed-length (or not a tuple).
///
/// A tuple is "open-ended" when, after flattening nested fixed-length tuple
/// rests, it still contains a rest element whose source is an array (e.g.
/// `[number, ...string[]]`) or another open-ended tuple. Such a tuple has an
/// indeterminate length, exactly like a bare array spread.
///
/// The returned offset counts the fixed positional slots that precede the first
/// variable rest (fixed elements contribute one slot; a fully-fixed nested tuple
/// rest contributes its flattened length). tsc treats an open-ended tuple spread
/// like an array spread: the variable portion must land on a rest parameter, so
/// this offset is the argument position that must be a rest-parameter position
/// for the spread to be valid. When it is not, the call site reports TS2556.
///
/// Readonly wrappers, type-parameter/`infer` constraints, and alias
/// applications are seen through via [`get_tuple_elements`].
pub fn tuple_variable_rest_offset(db: &dyn TypeDatabase, type_id: TypeId) -> Option<usize> {
    let elements = super::data::get_tuple_elements(db, type_id)?;
    tuple_slice_variable_rest_offset(db, &elements)
}

/// Slice-taking form of [`tuple_variable_rest_offset`], for callers that have
/// already fetched the tuple's elements (e.g. the call-argument spread
/// expansion) and want to avoid a second [`get_tuple_elements`] lookup.
pub fn tuple_slice_variable_rest_offset(
    db: &dyn TypeDatabase,
    elements: &[crate::types::TupleElement],
) -> Option<usize> {
    let mut offset = 0usize;
    tuple_elements_variable_rest_offset(db, elements, &mut offset)
}

/// Recursive worker for [`tuple_variable_rest_offset`].
///
/// Advances `offset` past fixed positional slots (including fully-fixed nested
/// tuple rests) and returns `Some(absolute_offset)` at the first variable rest.
/// Returns `None` for a fully fixed-length element list, leaving `offset` at the
/// total flattened fixed-slot count so an enclosing tuple can continue counting.
fn tuple_elements_variable_rest_offset(
    db: &dyn TypeDatabase,
    elements: &[crate::types::TupleElement],
    offset: &mut usize,
) -> Option<usize> {
    for element in elements {
        if !element.rest {
            *offset += 1;
            continue;
        }
        match super::data::get_tuple_elements(db, element.type_id) {
            // Nested tuple rest: a fully-fixed nested tuple just contributes its
            // own fixed slots; a nested open-ended tuple surfaces its variable
            // rest at the combined offset.
            Some(inner) => {
                if let Some(found) = tuple_elements_variable_rest_offset(db, &inner, offset) {
                    return Some(found);
                }
            }
            // Array-backed (or otherwise non-tuple) rest: variable length.
            None => return Some(*offset),
        }
    }
    None
}

/// Check if a type is invokable (has call signatures, not just construct signatures).
///
/// This is more specific than `is_callable_type` - it ensures the type can be called
/// as a function (not just constructed with `new`).
///
/// # Arguments
///
/// * `db` - The type database/interner
/// * `type_id` - The type to check
///
/// # Returns
///
/// * `true` - If the type has call signatures
/// * `false` - Otherwise
///
/// # Examples
///
/// ```text
/// // Functions are invokable
/// assert!(is_invokable_type(&db, function_type));
///
/// // Callables with call signatures are invokable
/// assert!(is_invokable_type(&db, callable_with_call_sigs));
///
/// // Callables with ONLY construct signatures are NOT invokable
/// assert!(!is_invokable_type(&db, class_constructor_only));
/// ```
pub fn is_invokable_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    match db.lookup(type_id) {
        Some(TypeData::Function(_)) => true,
        Some(TypeData::Callable(shape_id)) => {
            let shape = db.callable_shape(shape_id);
            // Must have at least one call signature (not just construct signatures)
            !shape.call_signatures.is_empty()
        }
        // Intersections might contain a callable
        Some(TypeData::Intersection(list_id)) => {
            let members = db.type_list(list_id);
            members.iter().any(|&m| is_invokable_type(db, m))
        }
        _ => false,
    }
}

/// Check if a type is an object type (with or without index signatures).
///
/// Returns true for `TypeData::Object` and `TypeData::ObjectWithIndex`.
pub fn is_object_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    matches!(
        db.lookup(type_id),
        Some(TypeData::Object(_) | TypeData::ObjectWithIndex(_))
    )
}

/// `true` when `type_id` is an anonymous object/mapped shape — an object,
/// object-with-index, or mapped type. Used as the "renders structurally" gate
/// for reducing-bodied alias applications (issue #10914).
pub fn is_object_or_mapped_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    matches!(
        db.lookup(type_id),
        Some(TypeData::Object(_) | TypeData::ObjectWithIndex(_) | TypeData::Mapped(_))
    )
}

/// `true` when `type_id` is an anonymous object type, or a union / intersection
/// that contains one (recursing only through nested unions / intersections).
///
/// Used to keep the "computed body" structural display off shared object shapes:
/// such a shape is painted through the reverse `find_def_for_type` lookup, which
/// can repaint it with an unrelated alias name. Both the checker's computed-body
/// marking and the diagnostic formatter consult this so the exclusion stays
/// single-sourced.
pub fn union_or_intersection_mentions_object(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if is_object_type(db, type_id) {
        return true;
    }
    match db.lookup(type_id) {
        Some(TypeData::Union(list) | TypeData::Intersection(list)) => db
            .type_list(list)
            .iter()
            .any(|&member| union_or_intersection_mentions_object(db, member)),
        _ => false,
    }
}

/// Check if a type has named properties (non-empty property list).
///
/// Returns true for object types with at least one named property.
/// Used to determine if a contextual type can provide property-level
/// type information for class expressions.
pub fn has_properties(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    match db.lookup(type_id) {
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
            !db.object_shape(shape_id).properties.is_empty()
        }
        Some(TypeData::Union(members)) => {
            // A union has properties if any non-undefined/null member does.
            let members = db.type_list(members);
            members
                .iter()
                .any(|&m| m != TypeId::UNDEFINED && m != TypeId::NULL && has_properties(db, m))
        }
        _ => false,
    }
}

/// Check if an object type has a nominal symbol (class/interface instance).
///
/// Returns true when the type is an Object or `ObjectWithIndex` with a
/// non-None `symbol` field, indicating it was created from a named class
/// or interface declaration.
pub fn has_nominal_symbol(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    match db.lookup(type_id) {
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
            db.object_shape(shape_id).symbol.is_some()
        }
        _ => false,
    }
}

/// Check if a type is a generic type application (Base<Args>).
///
/// Returns true for `TypeData::Application`.
pub fn is_generic_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    matches!(db.lookup(type_id), Some(TypeData::Application(_)))
}

/// Check if a type is a named type reference.
///
/// Returns true for `TypeData::Lazy(DefId)` (interfaces, classes, type aliases).
pub fn is_type_reference(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    matches!(
        db.lookup(type_id),
        Some(TypeData::Lazy(_) | TypeData::Recursive(_) | TypeData::BoundParameter(_))
    )
}

/// Returns true for `TypeData::TypeParameter`, `TypeData::BoundParameter`,
/// and `TypeData::Infer`. `BoundParameter` is included because it represents
/// a type parameter that has been bound to a specific index in a generic
/// signature — it should still be treated as "unresolved" for purposes like
/// excess property checking and constraint validation.
///
/// Use this instead of `visitor_predicates::is_type_parameter` when you need
/// to treat bound (de Bruijn indexed) parameters as type-parameter-like.
pub fn is_type_parameter_like(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    // Fast path: intrinsic kinds (any / unknown / never / void / null /
    // undefined plus the reserved PrimitiveX kinds) cannot be a
    // TypeParameter / BoundParameter / Infer. Skip the `db.lookup` virtual
    // call for them. is_intrinsic() is a free TypeId-range check.
    if type_id.is_intrinsic() {
        return false;
    }
    matches!(
        db.lookup(type_id),
        Some(TypeData::TypeParameter(_) | TypeData::BoundParameter(_) | TypeData::Infer(_))
    )
}

/// True for bare `TypeData::TypeParameter` only.
///
/// Stricter than [`is_type_parameter_like`]: excludes `TypeData::Infer`
/// (in-flight `infer T` placeholders inside conditional types) and
/// `TypeData::BoundParameter` (de-Bruijn indexed bound parameters).
///
/// Used by callers that need to detect a *named, finalized* type parameter
/// — e.g. `T` declared on an enclosing function — as opposed to an
/// inference placeholder that further inference might still substitute.
pub fn is_bare_named_type_parameter(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    matches!(db.lookup(type_id), Some(TypeData::TypeParameter(_)))
}

/// Return metadata for a bare named `TypeData::TypeParameter` only.
///
/// This deliberately excludes `Infer` and `BoundParameter`, matching
/// [`is_bare_named_type_parameter`] for callers that need an enclosing
/// declaration's finalized type parameter metadata.
pub fn named_type_param_info(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeParamInfo> {
    if type_id.is_intrinsic() {
        return None;
    }
    match db.lookup(type_id) {
        Some(TypeData::TypeParameter(info)) => Some(info),
        _ => None,
    }
}

/// Check if a type is a keyof type.
///
/// Returns true for `TypeData::KeyOf`.
pub fn is_keyof_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    matches!(db.lookup(type_id), Some(TypeData::KeyOf(_)))
}

/// Check if a type is a readonly type modifier.
///
/// Returns true for `TypeData::ReadonlyType`.
pub fn is_readonly_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    matches!(db.lookup(type_id), Some(TypeData::ReadonlyType(_)))
}

/// Check if a type has readonly members (properties or is a `ReadonlyType` wrapper).
///
/// Returns true if the type is a `ReadonlyType` wrapper, or an object type
/// with at least one readonly property. Used to detect types derived from
/// `const` type parameters, which always produce readonly members.
pub fn type_has_readonly_members(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    match db.lookup(type_id) {
        Some(TypeData::ReadonlyType(_)) => true,
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
            let shape = db.object_shape(shape_id);
            shape.properties.iter().any(|p| p.readonly)
        }
        Some(TypeData::Union(members) | TypeData::Intersection(members)) => {
            let members = db.type_list(members);
            members.iter().any(|&m| type_has_readonly_members(db, m))
        }
        _ => false,
    }
}

/// Check if a spread source object type carries any readonly member at the top
/// level — either a readonly property or a readonly index signature, including
/// through a `ReadonlyType` wrapper, union, or intersection.
///
/// Object spread (`{ ...x }`) always produces *mutable* members, so when the
/// spread source has readonly members the resulting type cannot be reconstructed
/// verbatim from the source AST in declaration emit; the solver-computed spread
/// type must be used instead.
pub fn object_spread_source_has_readonly_member(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    match db.lookup(type_id) {
        Some(TypeData::ReadonlyType(_)) => true,
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
            let shape = db.object_shape(shape_id);
            shape.properties.iter().any(|p| p.readonly)
                || shape.string_index.is_some_and(|s| s.readonly)
                || shape.number_index.is_some_and(|s| s.readonly)
        }
        Some(TypeData::Union(members) | TypeData::Intersection(members)) => {
            let members = db.type_list(members);
            members
                .iter()
                .any(|&m| object_spread_source_has_readonly_member(db, m))
        }
        _ => false,
    }
}

/// Check if a type is the polymorphic `this` type.
///
/// `ThisType` represents `this` in class methods and needs to be resolved
/// to the concrete class type before property access.
pub fn is_this_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    matches!(db.lookup(type_id), Some(TypeData::ThisType))
}

/// Check if a type is `symbol` or a `unique symbol` type.
///
/// Returns true for the built-in `symbol` type and for `TypeData::UniqueSymbol`.
/// Check if a type is a unique symbol (not plain `symbol`).
///
/// Returns true only for `TypeData::UniqueSymbol` types, which represent
/// individual `typeof sym` types created for const symbol declarations.
pub fn is_unique_symbol_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    matches!(db.lookup(type_id), Some(TypeData::UniqueSymbol(_)))
}

pub fn is_symbol_or_unique_symbol_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id == TypeId::SYMBOL {
        return true;
    }
    if type_id.is_intrinsic() {
        return false;
    }
    matches!(
        db.lookup(type_id),
        Some(TypeData::UniqueSymbol(_) | TypeData::Intrinsic(crate::IntrinsicKind::Symbol))
    )
}

/// Check if a type is usable as a property name (TS1166/TS1165/TS1169).
///
/// Returns true for string literals, number literals, and unique symbol types.
/// This corresponds to TypeScript's `isTypeUsableAsPropertyName` check.
pub fn is_type_usable_as_property_name(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    matches!(
        db.lookup(type_id),
        Some(
            TypeData::Literal(crate::LiteralValue::String(_))
                | TypeData::Literal(crate::LiteralValue::Number(_))
                | TypeData::UniqueSymbol(_)
        )
    )
}

/// Check if a type needs evaluation before interface merging.
///
/// Returns true for Application and Lazy types, which are meta-types that
/// may resolve to Object/Callable types when evaluated. Used before
/// `classify_for_interface_merge` to ensure that type-alias-based heritage
/// (e.g., `interface X extends TypeAlias<T>`) is properly resolved.
pub fn needs_evaluation_for_merge(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    matches!(
        db.lookup(type_id),
        Some(TypeData::Application(_) | TypeData::Lazy(_))
    )
}

/// Get the return type of a function type.
///
/// Returns `TypeId::ERROR` if the type is not a Function.
pub fn get_function_return_type(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    if type_id.is_intrinsic() {
        return TypeId::ERROR;
    }
    match db.lookup(type_id) {
        Some(TypeData::Function(shape_id)) => db.function_shape(shape_id).return_type,
        _ => TypeId::ERROR,
    }
}

/// Get the parameter types of a function type.
///
/// Returns an empty vector if the type is not a Function.
pub fn get_function_parameter_types(db: &dyn TypeDatabase, type_id: TypeId) -> Vec<TypeId> {
    if type_id.is_intrinsic() {
        return Vec::new();
    }
    match db.lookup(type_id) {
        Some(TypeData::Function(shape_id)) => db
            .function_shape(shape_id)
            .params
            .iter()
            .map(|p| p.type_id)
            .collect(),
        _ => Vec::new(),
    }
}

// =============================================================================
// Intrinsic Type Queries
// =============================================================================
//
// These functions provide TypeData-free checking for intrinsic types.
// Checker code should use these instead of matching on TypeData::Intrinsic.
//
// ## Important Usage Notes
//
// These are TYPE IDENTITY checks, NOT compatibility checks:
//
// - Identity: `is_string_type(TypeId::STRING)` -> TRUE
// - Identity: `is_string_type(literal "hello")` -> FALSE (literal, not intrinsic)
// - Identity: `is_string_type(string & {tag: 1})` -> FALSE (intersection, not intrinsic)
//
// For assignability/compatibility checks, use Solver subtyping:
// - `solver.is_subtype_of(literal, TypeId::STRING)` -> TRUE
// - `solver.is_subtype_of(branded, TypeId::STRING)` -> TRUE (if assignable)
//
// ### When to use these helpers
// - Checking if a type annotation is explicitly the intrinsic keyword
// - Validating type constructor arguments
// - Distinguishing `void` from `undefined` in return types
//
// ### When NOT to use these helpers
// - Assignment/compatibility checks -> Use `is_subtype_of` instead
// - Type narrowing -> Use Solver's narrowing analysis
// - Checking if a value IS a string (not literal) -> Use `is_subtype_of`
//
// ## Implementation Notes
// - Shallow queries: do NOT resolve Lazy/Ref (caller's responsibility)
// - Defensive pattern: check both TypeId constants AND TypeData::Intrinsic
// - Fast-path O(1) using TypeId integer comparison

/// Generate an intrinsic type checker function that checks both the well-known
/// `TypeId` constant and the `TypeData::Intrinsic` variant.
macro_rules! define_intrinsic_check {
    ($fn_name:ident, $type_id:ident, $kind:ident) => {
        pub fn $fn_name(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
            type_id == TypeId::$type_id
                || matches!(
                    db.lookup(type_id),
                    Some(TypeData::Intrinsic(IntrinsicKind::$kind))
                )
        }
    };
}

define_intrinsic_check!(is_any_type, ANY, Any);
define_intrinsic_check!(is_unknown_type, UNKNOWN, Unknown);
define_intrinsic_check!(is_never_type, NEVER, Never);
define_intrinsic_check!(is_void_type, VOID, Void);
define_intrinsic_check!(is_undefined_type, UNDEFINED, Undefined);
define_intrinsic_check!(is_null_type, NULL, Null);
define_intrinsic_check!(is_string_type, STRING, String);
define_intrinsic_check!(is_number_type, NUMBER, Number);
define_intrinsic_check!(is_bigint_type, BIGINT, Bigint);
define_intrinsic_check!(is_boolean_type, BOOLEAN, Boolean);
define_intrinsic_check!(is_symbol_type, SYMBOL, Symbol);

/// Check if a type is `symbol` or a `unique symbol`.
pub fn is_symbol_or_unique_symbol(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id == TypeId::SYMBOL {
        return true;
    }
    if type_id.is_intrinsic() {
        return false;
    }
    matches!(
        db.lookup(type_id),
        Some(TypeData::UniqueSymbol(_)) | Some(TypeData::Intrinsic(crate::IntrinsicKind::Symbol))
    )
}

// =============================================================================
// Composite Type Queries
// =============================================================================

/// Check if a type is valid for object spreading (`{...x}`).
///
/// Matches tsc's `isValidSpreadType()` behavior:
/// 1. Resolve type parameters to their base constraints
/// 2. Remove definitely-falsy types (false, 0, "", null, undefined, void, never)
/// 3. Check if remaining type is an object-like/any/instantiable type
///
/// Returns `true` for types that can be spread into an object literal:
/// - `any`, `never`, `error` (always spreadable)
/// - Object types, arrays, tuples, functions, callables, mapped types
/// - `object` intrinsic (non-primitive)
/// - Type parameters whose constraint is spreadable
/// - Indexed access types whose evaluated base constraint is spreadable
/// - Unions where non-falsy members are all spreadable
/// - Intersections where all members are spreadable
///
/// Returns `false` for primitive types (`number`, `string`, `boolean`, etc.),
/// literals that aren't definitely-falsy, `keyof` types, `unknown`, and types
/// that resolve to these after constraint resolution.
pub fn is_valid_spread_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    is_valid_spread_type_impl(db, type_id, 0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectSpreadDtsProjection {
    InvalidSpread,
    EmptyObject,
    PreserveSource,
}

/// Classifies how declaration emit should spell `{ ...value }` when the object
/// literal has no own members. This mirrors the same definitely-falsy filtering
/// used by spread validity, while leaving generic operands nameable by source.
pub fn classify_object_spread_dts_projection(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> ObjectSpreadDtsProjection {
    if !is_valid_spread_type(db, type_id) {
        return ObjectSpreadDtsProjection::InvalidSpread;
    }
    if is_object_intrinsic_type(db, type_id) {
        return ObjectSpreadDtsProjection::EmptyObject;
    }
    let Some(TypeData::Union(members)) = db.lookup(type_id) else {
        return ObjectSpreadDtsProjection::PreserveSource;
    };
    let non_falsy: Vec<TypeId> = db
        .type_list(members)
        .iter()
        .copied()
        .filter(|&member| !is_definitely_falsy_type(db, member))
        .collect();
    if non_falsy.len() == 1 && is_object_intrinsic_type(db, non_falsy[0]) {
        ObjectSpreadDtsProjection::EmptyObject
    } else {
        ObjectSpreadDtsProjection::PreserveSource
    }
}

fn is_object_intrinsic_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    type_id == TypeId::OBJECT
        || matches!(
            db.lookup(type_id),
            Some(TypeData::Intrinsic(IntrinsicKind::Object))
        )
}

/// Check if a type is definitely falsy (always falsy at runtime).
///
/// Definitely-falsy types: `null`, `undefined`, `void`, `never`,
/// literal `false`, literal `0`/`-0`/`NaN`, literal `""`, literal `0n`.
fn is_definitely_falsy_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    // null, undefined, void, never are always falsy
    if type_id.is_nullable() || type_id == TypeId::NEVER {
        return true;
    }
    match db.lookup(type_id) {
        Some(TypeData::Intrinsic(
            IntrinsicKind::Void | IntrinsicKind::Null | IntrinsicKind::Undefined,
        )) => true,
        Some(TypeData::Literal(lit)) => match lit {
            LiteralValue::Boolean(b) => !b,
            LiteralValue::Number(n) => n.0 == 0.0 || n.0.is_nan(),
            LiteralValue::String(atom) => db.resolve_atom_ref(atom).is_empty(),
            LiteralValue::BigInt(atom) => db.resolve_atom_ref(atom).as_ref() == "0",
        },
        // Intersection: if ANY member is definitely falsy, the intersection is falsy.
        // e.g., `T & undefined` is always falsy because the value must be undefined.
        Some(TypeData::Intersection(members)) => {
            let members = db.type_list(members);
            members.iter().any(|&m| is_definitely_falsy_type(db, m))
        }
        // Type parameters: check if the constraint is definitely falsy.
        // e.g., `T extends undefined` is definitely falsy.
        Some(TypeData::TypeParameter(info)) => info
            .constraint
            .is_some_and(|c| is_definitely_falsy_type(db, c)),
        _ => false,
    }
}

/// Resolve a type parameter to its base constraint, or return the type itself.
///
/// Matches tsc's `getResolvedBaseConstraint()` for the type-parameter case:
/// - Walks nested type parameter chains (`U extends T extends number` →
///   `number`) until reaching a non-instantiable type.
/// - Stops and returns the current type when an unconstrained parameter
///   is reached, mirroring tsc's `noConstraintType` fallback.
/// - Normalizes an explicit `extends any` constraint to `unknown` along the
///   way, mirroring tsc's `getConstraintFromTypeParameter` (which rewrites
///   `T extends any` to `T extends unknown` outside mapped-type contexts).
pub fn get_base_constraint_or_type(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    let mut current = type_id;
    let mut depth: u32 = 0;
    loop {
        if depth > 50 {
            return current;
        }
        match db.lookup(current) {
            Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => match info.constraint {
                Some(c) => {
                    // tsc: an explicit `extends any` is treated as `extends unknown`
                    // for constraint reads (outside mapped-type contexts).
                    current = if c == TypeId::ANY { TypeId::UNKNOWN } else { c };
                    depth += 1;
                }
                None => return current,
            },
            _ => return current,
        }
    }
}

/// Whether `type_id` is "instantiable" — a type whose concrete shape depends on
/// a pending type-parameter substitution. Mirrors the subset of tsc's
/// `TypeFlags.Instantiable` that tsz models structurally.
fn is_instantiable_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    matches!(
        db.lookup(type_id),
        Some(
            TypeData::TypeParameter(_)
                | TypeData::Infer(_)
                | TypeData::BoundParameter(_)
                | TypeData::KeyOf(_)
                | TypeData::IndexAccess(_, _)
                | TypeData::Conditional(_)
                | TypeData::TemplateLiteral(_)
        )
    )
}

/// True when `type_id` is or contains (top-level union) `null`/`undefined`.
///
/// Mirrors tsc's `maybeTypeOfKind(type, TypeFlags.Nullable)` for the cases that
/// matter to constraint-position substitution.
fn maybe_nullable(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id == TypeId::NULL || type_id == TypeId::UNDEFINED {
        return true;
    }
    match db.lookup(type_id) {
        Some(TypeData::Union(list)) => db
            .type_list(list)
            .iter()
            .any(|&member| member == TypeId::NULL || member == TypeId::UNDEFINED),
        _ => false,
    }
}

/// tsc's `isGenericTypeWithUnionConstraint`, combined with the `someType`
/// distribution applied at its call sites.
///
/// Returns true when `type_id` (or, distributing over union/intersection
/// members, some constituent) is an instantiable type whose base constraint is
/// a union or includes `null`/`undefined`. Such references are substituted with
/// their base constraint when they appear in a constraint position, so that a
/// `T extends X | undefined` reference is seen as possibly-undefined at a
/// property/element access or call target.
pub fn is_generic_type_with_union_constraint(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    // Intrinsics can never be instantiable or a union/intersection of such, so
    // short-circuit before the memo lookup (cheaper than a cache probe).
    if type_id.is_intrinsic() {
        return false;
    }
    // The answer is a pure function of the type structure (instantiable kinds and
    // their structural base constraints), so memoize it root-wide. Constraint
    // positions re-ask this for every reference to the same symbol, which for a
    // wide N-member union turns into an O(N) member scan per reference — O(N^2)
    // over an N-arm discriminated-union switch (#13598). The memo collapses the
    // repeated scans of the same union `TypeId` to O(1).
    if let Some(cached) = db.is_generic_with_union_constraint_cached(type_id) {
        return cached;
    }
    let result = match db.lookup(type_id) {
        Some(TypeData::Union(list) | TypeData::Intersection(list)) => db
            .type_list(list)
            .iter()
            .any(|&member| is_generic_type_with_union_constraint(db, member)),
        _ => {
            if is_instantiable_type(db, type_id) {
                let base = get_base_constraint_or_type(db, type_id);
                base != type_id
                    && (matches!(db.lookup(base), Some(TypeData::Union(_)))
                        || maybe_nullable(db, base))
            } else {
                false
            }
        }
    };
    db.set_is_generic_with_union_constraint_cache(type_id, result);
    result
}

/// tsc's `isGenericTypeWithoutNullableConstraint`, combined with the `someType`
/// distribution applied at its call site.
///
/// Returns true when `type_id` (or some union/intersection constituent) is an
/// instantiable type whose base constraint does not include `null`/`undefined`.
/// Used to recognize the `obj[key]` exception to constraint-position
/// substitution: when both the object is a generic type without a nullable
/// constraint and the index is a generic index type, the access keeps its
/// deferred `T[K]` form instead of being substituted.
pub fn is_generic_type_without_nullable_constraint(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    // Pure function of type structure; memoized root-wide for the same reason as
    // `is_generic_type_with_union_constraint` — the `obj[key]` constraint-position
    // exception re-asks it per reference over potentially wide unions (#13598).
    if let Some(cached) = db.is_generic_without_nullable_constraint_cached(type_id) {
        return cached;
    }
    let result = match db.lookup(type_id) {
        Some(TypeData::Union(list) | TypeData::Intersection(list)) => db
            .type_list(list)
            .iter()
            .any(|&member| is_generic_type_without_nullable_constraint(db, member)),
        _ => {
            is_instantiable_type(db, type_id)
                && !maybe_nullable(db, get_base_constraint_or_type(db, type_id))
        }
    };
    db.set_is_generic_without_nullable_constraint_cache(type_id, result);
    result
}

/// Substitute a constraint-position reference's type with its base constraint,
/// distributing over union members (tsc's `mapType(type, getBaseConstraintOrType)`).
pub fn substitute_reference_base_constraints(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    if let Some(TypeData::Union(list)) = db.lookup(type_id) {
        let mapped: Vec<TypeId> = db
            .type_list(list)
            .iter()
            .map(|&member| get_base_constraint_or_type(db, member))
            .collect();
        crate::utils::union_or_single(db, mapped)
    } else {
        get_base_constraint_or_type(db, type_id)
    }
}

const MAX_VALID_SPREAD_DEPTH: u32 = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ValidSpreadDepthState {
    Continue,
    LimitExceeded,
}

impl ValidSpreadDepthState {
    const fn from_depth(depth: u32) -> Self {
        if depth > MAX_VALID_SPREAD_DEPTH {
            Self::LimitExceeded
        } else {
            Self::Continue
        }
    }
}

fn is_valid_spread_type_impl(db: &dyn TypeDatabase, type_id: TypeId, depth: u32) -> bool {
    match ValidSpreadDepthState::from_depth(depth) {
        ValidSpreadDepthState::Continue => {}
        ValidSpreadDepthState::LimitExceeded => return true,
    }

    // Step 1: Resolve type parameter to its base constraint (like tsc's getBaseConstraintOrType)
    let resolved = get_base_constraint_or_type(db, type_id);

    match resolved {
        // `any` and our internal error sentinel are permissive. `never` is
        // explicitly NOT spreadable in tsc — `removeDefinitelyFalsyTypes`
        // strips it before the validity flags check, leaving an empty type
        // that fails the Any|NonPrimitive|Object|InstantiableNonPrimitive
        // gate (TS2698).
        TypeId::ANY | TypeId::ERROR => return true,
        TypeId::NEVER => return false,
        _ => {}
    }

    match db.lookup(resolved) {
        // Primitives, null/undefined/void, literals, template literals, string
        // intrinsics, and `keyof T` (which is structurally a property-key
        // primitive `string | number | symbol` whether evaluated or deferred):
        // not spreadable on their own.
        // (Definitely-falsy members are filtered out in the union branch instead.)
        Some(
            TypeData::Intrinsic(
                IntrinsicKind::String
                | IntrinsicKind::Number
                | IntrinsicKind::Boolean
                | IntrinsicKind::Bigint
                | IntrinsicKind::Symbol
                | IntrinsicKind::Unknown
                | IntrinsicKind::Void
                | IntrinsicKind::Null
                | IntrinsicKind::Undefined,
            )
            | TypeData::Literal(_)
            | TypeData::TemplateLiteral(_)
            | TypeData::StringIntrinsic { .. }
            | TypeData::Enum(_, _)
            | TypeData::KeyOf(_),
        ) => false,
        // Union: remove definitely-falsy members, then check remaining.
        // Matches tsc's removeDefinitelyFalsyTypes before checking.
        Some(TypeData::Union(members)) => {
            let members = db.type_list(members);
            // Filter out definitely-falsy types, then check if all remaining are valid
            let non_falsy: Vec<TypeId> = members
                .iter()
                .copied()
                .filter(|&m| !is_definitely_falsy_type(db, m))
                .collect();
            // If nothing remains after removing falsy types, the spread is invalid
            // (entirely falsy union like `false | null`)
            if non_falsy.is_empty() {
                return false;
            }
            non_falsy
                .iter()
                .all(|&m| is_valid_spread_type_impl(db, m, depth + 1))
        }
        // Intersection: all members must be spreadable (after constraint resolution)
        Some(TypeData::Intersection(members)) => {
            let members = db.type_list(members);
            members
                .iter()
                .all(|&m| is_valid_spread_type_impl(db, m, depth + 1))
        }
        Some(TypeData::ReadonlyType(inner)) => is_valid_spread_type_impl(db, inner, depth + 1),
        // Mirrors tsc's `isValidSpreadType`:
        //   if (type.flags & Instantiable) {
        //       const constraint = getBaseConstraintOfType(type);
        //       if (constraint !== undefined) return isValidSpreadType(constraint);
        //   }
        //   return !!(type.flags & (Any | NonPrimitive | Object | InstantiableNonPrimitive) | …);
        //
        // For an `IndexAccess` we first ask the evaluator to reduce through
        // any usable constraint. If reduction succeeds the result is
        // validated like any other type. If it does not, the deferred form
        // itself carries `InstantiableNonPrimitive` (an indexed access could
        // be an object at runtime), so the flag-check arm accepts the
        // spread without recursing — the unchanged `resolved` carries no new
        // information for a recursive call.
        Some(TypeData::IndexAccess(_, _)) => {
            let evaluated = evaluate_type(db, resolved);
            evaluated == resolved || is_valid_spread_type_impl(db, evaluated, depth + 1)
        }
        // Everything else is spreadable: object types, arrays, tuples, functions,
        // callables, mapped types, type parameters (unconstrained ones reach here
        // and are valid per tsc's InstantiableNonPrimitive), lazy refs, applications, etc.
        _ => true,
    }
}

#[cfg(test)]
mod valid_spread_depth_state_tests {
    use super::*;

    #[test]
    fn valid_spread_depth_state_continues_at_limit() {
        assert_eq!(
            ValidSpreadDepthState::from_depth(MAX_VALID_SPREAD_DEPTH),
            ValidSpreadDepthState::Continue
        );
    }

    #[test]
    fn valid_spread_depth_state_limits_past_limit() {
        assert_eq!(
            ValidSpreadDepthState::from_depth(MAX_VALID_SPREAD_DEPTH + 1),
            ValidSpreadDepthState::LimitExceeded
        );
    }
}

// =============================================================================
// Constructor Type Collection Helpers
// =============================================================================

/// Result of classifying a type for constructor collection.
///
/// This enum tells the caller what kind of type this is and how to proceed
/// when collecting constructor types from a composite type structure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConstructorTypeKind {
    /// This is a Callable type - always a constructor type
    Callable,
    /// This is a Function type - check `is_constructor` flag on the shape
    Function(crate::types::FunctionShapeId),
    /// Recurse into these member types (Union, Intersection)
    Members(Vec<TypeId>),
    /// Recurse into the inner type (`ReadonlyType`)
    Inner(TypeId),
    /// Recurse into the constraint (`TypeParameter`, Infer)
    Constraint(Option<TypeId>),
    /// This type needs full type evaluation (Conditional, Mapped, `IndexAccess`, `KeyOf`)
    NeedsTypeEvaluation,
    /// This is a generic application that needs instantiation
    NeedsApplicationEvaluation,
    /// This is a `TypeQuery` - resolve the symbol reference to get its type
    TypeQuery(crate::types::SymbolRef),
    /// This type cannot be a constructor (primitives, literals, etc.)
    NotConstructor,
}

/// Classify a type for constructor type collection.
///
/// This function examines a `TypeData` and returns information about how to handle it
/// when collecting constructor types. The caller is responsible for:
/// - Checking the `is_constructor` flag for Function types
/// - Evaluating types when `NeedsTypeEvaluation` or `NeedsApplicationEvaluation` is returned
/// - Resolving symbol references for `TypeQuery`
/// - Recursing into members/inner types
pub fn classify_constructor_type(db: &dyn TypeDatabase, type_id: TypeId) -> ConstructorTypeKind {
    // Fast path: intrinsics (and the BOOLEAN_TRUE/FALSE intrinsic IDs that
    // lookup as `Literal(Boolean)`) all fall to the `NotConstructor` arm.
    if type_id.is_intrinsic() {
        return ConstructorTypeKind::NotConstructor;
    }
    let Some(key) = db.lookup(type_id) else {
        return ConstructorTypeKind::NotConstructor;
    };

    match key {
        TypeData::Callable(_) => ConstructorTypeKind::Callable,
        TypeData::Function(shape_id) => ConstructorTypeKind::Function(shape_id),
        TypeData::Intersection(members_id) | TypeData::Union(members_id) => {
            let members = db.type_list(members_id);
            ConstructorTypeKind::Members(members.to_vec())
        }
        TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner) => {
            ConstructorTypeKind::Inner(inner)
        }
        TypeData::Substitution { base_type, .. } => ConstructorTypeKind::Inner(base_type),
        TypeData::TypeParameter(info) | TypeData::Infer(info) => {
            ConstructorTypeKind::Constraint(info.constraint)
        }
        TypeData::Conditional(_)
        | TypeData::Mapped(_)
        | TypeData::IndexAccess(_, _)
        | TypeData::KeyOf(_) => ConstructorTypeKind::NeedsTypeEvaluation,
        TypeData::Application(_) => ConstructorTypeKind::NeedsApplicationEvaluation,
        TypeData::TypeQuery(sym_ref) => ConstructorTypeKind::TypeQuery(sym_ref),
        // All other types cannot be constructors
        TypeData::Enum(_, _)
        | TypeData::BoundParameter(_)
        | TypeData::Intrinsic(_)
        | TypeData::Literal(_)
        | TypeData::Object(_)
        | TypeData::ObjectWithIndex(_)
        | TypeData::Array(_)
        | TypeData::Tuple(_)
        | TypeData::Lazy(_)
        | TypeData::Recursive(_)
        | TypeData::TemplateLiteral(_)
        | TypeData::UniqueSymbol(_)
        | TypeData::ThisType
        | TypeData::StringIntrinsic { .. }
        | TypeData::ModuleNamespace(_)
        | TypeData::UnresolvedTypeName(_)
        | TypeData::Error => ConstructorTypeKind::NotConstructor,
    }
}

// =============================================================================
// Static Property Collection Helpers
// =============================================================================

/// Result of extracting static properties from a type.
///
/// This enum allows the caller to handle recursion and type evaluation
/// while keeping the `TypeData` matching logic in the solver layer.
#[derive(Debug, Clone)]
pub enum StaticPropertySource {
    /// Direct properties from Callable, Object, or `ObjectWithIndex` types.
    Properties(Vec<crate::PropertyInfo>),
    /// Member types that should be recursively processed (Union/Intersection).
    RecurseMembers(Vec<TypeId>),
    /// Single type to recurse into (`TypeParameter` constraint, `ReadonlyType` inner).
    RecurseSingle(TypeId),
    /// Type that needs evaluation before property extraction (Conditional, Mapped, etc.).
    NeedsEvaluation,
    /// Type that needs application evaluation (Application type).
    NeedsApplicationEvaluation,
    /// No properties available (primitives, error types, etc.).
    None,
}

/// Extract static property information from a type.
///
/// This function handles the `TypeData` matching for property collection,
/// returning a `StaticPropertySource` that tells the caller how to proceed.
/// The caller is responsible for:
/// - Handling recursion for `RecurseMembers` and `RecurseSingle` cases
/// - Evaluating types for `NeedsEvaluation` and `NeedsApplicationEvaluation` cases
/// - Tracking visited types to prevent infinite loops
pub fn get_static_property_source(db: &dyn TypeDatabase, type_id: TypeId) -> StaticPropertySource {
    if type_id.is_intrinsic() {
        return StaticPropertySource::None;
    }
    let Some(key) = db.lookup(type_id) else {
        return StaticPropertySource::None;
    };

    match key {
        TypeData::Callable(shape_id) => {
            let shape = db.callable_shape(shape_id);
            StaticPropertySource::Properties(shape.properties.to_vec())
        }
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            let shape = db.object_shape(shape_id);
            StaticPropertySource::Properties(shape.properties.to_vec())
        }
        TypeData::Intersection(members_id) | TypeData::Union(members_id) => {
            let members = db.type_list(members_id);
            StaticPropertySource::RecurseMembers(members.to_vec())
        }
        TypeData::TypeParameter(info) | TypeData::Infer(info) => {
            if let Some(constraint) = info.constraint {
                StaticPropertySource::RecurseSingle(constraint)
            } else {
                StaticPropertySource::None
            }
        }
        TypeData::ReadonlyType(inner) => StaticPropertySource::RecurseSingle(inner),
        TypeData::Conditional(_)
        | TypeData::Mapped(_)
        | TypeData::IndexAccess(_, _)
        | TypeData::KeyOf(_) => StaticPropertySource::NeedsEvaluation,
        TypeData::Application(_) => StaticPropertySource::NeedsApplicationEvaluation,
        _ => StaticPropertySource::None,
    }
}

// =============================================================================
// Construct Signature Queries
// =============================================================================

/// Check if a Callable type has construct signatures.
///
/// Returns true only for Callable types that have non-empty `construct_signatures`.
/// This is a direct check and does not resolve through Ref or `TypeQuery` types.
pub fn has_construct_signatures(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    match db.lookup(type_id) {
        Some(TypeData::Callable(shape_id)) => {
            let shape = db.callable_shape(shape_id);
            !shape.construct_signatures.is_empty()
        }
        _ => false,
    }
}

// =============================================================================
// Signature Classification
// =============================================================================

/// Classification for types when extracting call/construct signatures.
#[derive(Debug, Clone)]
pub enum SignatureTypeKind {
    /// Callable type with `shape_id` - has `call_signatures` and `construct_signatures`
    Callable(crate::types::CallableShapeId),
    /// Function type with `shape_id` - has single signature
    Function(crate::types::FunctionShapeId),
    /// Union type - get signatures from each member
    Union(Vec<TypeId>),
    /// Intersection type - get signatures from each member
    Intersection(Vec<TypeId>),
    /// Readonly wrapper - unwrap and get signatures from inner type
    ReadonlyType(TypeId),
    /// Type parameter with optional constraint - may need to check constraint
    TypeParameter { constraint: Option<TypeId> },
    /// Types that need evaluation before signature extraction (Conditional, Mapped, `IndexAccess`, `KeyOf`)
    NeedsEvaluation(TypeId),
    /// Types without signatures (Intrinsic, Literal, Object without callable, etc.)
    NoSignatures,
}

/// Classify a type for signature extraction.
pub fn classify_for_signatures(db: &dyn TypeDatabase, type_id: TypeId) -> SignatureTypeKind {
    // Handle special TypeIds first
    if type_id == TypeId::ERROR || type_id == TypeId::NEVER {
        return SignatureTypeKind::NoSignatures;
    }
    if type_id == TypeId::ANY {
        // any is callable but has no concrete signatures
        return SignatureTypeKind::NoSignatures;
    }
    // Other intrinsics resolve to TypeData::Intrinsic / Literal, which the
    // match below classifies as NoSignatures. Skip the dyn-dispatched lookup.
    if type_id.is_intrinsic() {
        return SignatureTypeKind::NoSignatures;
    }

    let Some(key) = db.lookup(type_id) else {
        return SignatureTypeKind::NoSignatures;
    };

    match key {
        // Callable types - have call_signatures and construct_signatures
        TypeData::Callable(shape_id) => SignatureTypeKind::Callable(shape_id),

        // Function types - have a single signature
        TypeData::Function(shape_id) => SignatureTypeKind::Function(shape_id),

        // Union type - get signatures from each member
        TypeData::Union(members_id) => {
            let members = db.type_list(members_id);
            SignatureTypeKind::Union(members.to_vec())
        }

        // Intersection type - get signatures from each member
        TypeData::Intersection(members_id) => {
            let members = db.type_list(members_id);
            SignatureTypeKind::Intersection(members.to_vec())
        }

        // Readonly wrapper - unwrap and recurse
        TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner) => {
            SignatureTypeKind::ReadonlyType(inner)
        }

        // Substitution presents its base variable's signatures.
        TypeData::Substitution { base_type, .. } => SignatureTypeKind::ReadonlyType(base_type),

        // Type parameter - may have constraint with signatures
        TypeData::TypeParameter(info) | TypeData::Infer(info) => SignatureTypeKind::TypeParameter {
            constraint: info.constraint,
        },

        // Complex types that need evaluation before signature extraction
        TypeData::Conditional(_)
        | TypeData::Mapped(_)
        | TypeData::IndexAccess(_, _)
        | TypeData::KeyOf(_) => SignatureTypeKind::NeedsEvaluation(type_id),

        // All other types don't have callable signatures
        TypeData::BoundParameter(_)
        | TypeData::Intrinsic(_)
        | TypeData::Literal(_)
        | TypeData::Object(_)
        | TypeData::ObjectWithIndex(_)
        | TypeData::Array(_)
        | TypeData::Tuple(_)
        | TypeData::Lazy(_)
        | TypeData::Recursive(_)
        | TypeData::Application(_)
        | TypeData::TemplateLiteral(_)
        | TypeData::UniqueSymbol(_)
        | TypeData::ThisType
        | TypeData::TypeQuery(_)
        | TypeData::StringIntrinsic { .. }
        | TypeData::ModuleNamespace(_)
        | TypeData::Enum(_, _)
        | TypeData::UnresolvedTypeName(_)
        | TypeData::Error => SignatureTypeKind::NoSignatures,
    }
}

// =============================================================================
// EvaluationNeeded - Classification for types that need evaluation
// =============================================================================

/// Classification for types that need evaluation before use.
#[derive(Debug, Clone)]
pub enum EvaluationNeeded {
    /// Already resolved, no evaluation needed
    Resolved(TypeId),
    /// Symbol reference - resolve symbol first
    SymbolRef(crate::types::SymbolRef),
    /// Type query (typeof) - evaluate first
    TypeQuery(crate::types::SymbolRef),
    /// Generic application - instantiate first
    Application {
        app_id: crate::types::TypeApplicationId,
    },
    /// Index access T[K] - evaluate with environment
    IndexAccess { object: TypeId, index: TypeId },
    /// `KeyOf` type - evaluate
    KeyOf(TypeId),
    /// Mapped type - evaluate
    Mapped {
        mapped_id: crate::types::MappedTypeId,
    },
    /// Conditional type - evaluate
    Conditional {
        cond_id: crate::types::ConditionalTypeId,
    },
    /// Callable type (for contextual typing checks)
    Callable(crate::types::CallableShapeId),
    /// Function type
    Function(crate::types::FunctionShapeId),
    /// Union - may need per-member evaluation
    Union(Vec<TypeId>),
    /// Intersection - may need per-member evaluation
    Intersection(Vec<TypeId>),
    /// Type parameter with constraint
    TypeParameter { constraint: Option<TypeId> },
    /// Readonly wrapper - unwrap
    Readonly(TypeId),
}

/// Classify a type for what kind of evaluation it needs.
pub fn classify_for_evaluation(db: &dyn TypeDatabase, type_id: TypeId) -> EvaluationNeeded {
    let Some(key) = db.lookup(type_id) else {
        return EvaluationNeeded::Resolved(type_id);
    };

    match key {
        TypeData::TypeQuery(sym_ref) => EvaluationNeeded::TypeQuery(sym_ref),
        TypeData::Application(app_id) => EvaluationNeeded::Application { app_id },
        TypeData::IndexAccess(object, index) => EvaluationNeeded::IndexAccess { object, index },
        TypeData::KeyOf(inner) => EvaluationNeeded::KeyOf(inner),
        TypeData::Mapped(mapped_id) => EvaluationNeeded::Mapped { mapped_id },
        TypeData::Conditional(cond_id) => EvaluationNeeded::Conditional { cond_id },
        TypeData::Callable(shape_id) => EvaluationNeeded::Callable(shape_id),
        TypeData::Function(shape_id) => EvaluationNeeded::Function(shape_id),
        TypeData::Union(list_id) => {
            let members = db.type_list(list_id);
            EvaluationNeeded::Union(members.to_vec())
        }
        TypeData::Intersection(list_id) => {
            let members = db.type_list(list_id);
            EvaluationNeeded::Intersection(members.to_vec())
        }
        TypeData::TypeParameter(info) | TypeData::Infer(info) => EvaluationNeeded::TypeParameter {
            constraint: info.constraint,
        },
        TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner) => {
            EvaluationNeeded::Readonly(inner)
        }
        TypeData::Substitution { base_type, .. } => EvaluationNeeded::Readonly(base_type),
        // Already resolved types (Lazy needs special handling when DefId lookup is implemented)
        TypeData::BoundParameter(_)
        | TypeData::Intrinsic(_)
        | TypeData::Literal(_)
        | TypeData::Object(_)
        | TypeData::ObjectWithIndex(_)
        | TypeData::Array(_)
        | TypeData::Tuple(_)
        | TypeData::Lazy(_)
        | TypeData::Recursive(_)
        | TypeData::TemplateLiteral(_)
        | TypeData::UniqueSymbol(_)
        | TypeData::ThisType
        | TypeData::StringIntrinsic { .. }
        | TypeData::ModuleNamespace(_)
        | TypeData::Enum(_, _)
        | TypeData::UnresolvedTypeName(_)
        | TypeData::Error => EvaluationNeeded::Resolved(type_id),
    }
}

/// Evaluate contextual wrapper structure while delegating leaf evaluation.
///
/// Solver owns traversal over semantic type shape; caller provides the concrete
/// leaf evaluator (for example checker's judge-based environment evaluation).
pub fn evaluate_contextual_structure_with(
    db: &dyn QueryDatabase,
    type_id: TypeId,
    evaluate_leaf: &mut dyn FnMut(TypeId) -> TypeId,
) -> TypeId {
    fn visit(
        db: &dyn QueryDatabase,
        type_id: TypeId,
        evaluate_leaf: &mut dyn FnMut(TypeId) -> TypeId,
    ) -> TypeId {
        match classify_for_evaluation(db, type_id) {
            EvaluationNeeded::Union(members) => {
                let mut changed = false;
                let evaluated: Vec<TypeId> = members
                    .iter()
                    .map(|&member| {
                        let ev = visit(db, member, evaluate_leaf);
                        if ev != member {
                            changed = true;
                        }
                        ev
                    })
                    .collect();
                if changed {
                    db.factory().union(evaluated)
                } else {
                    type_id
                }
            }
            EvaluationNeeded::Intersection(members) => {
                let mut changed = false;
                let evaluated: Vec<TypeId> = members
                    .iter()
                    .map(|&member| {
                        let ev = visit(db, member, evaluate_leaf);
                        if ev != member {
                            changed = true;
                        }
                        ev
                    })
                    .collect();
                if changed {
                    db.factory().intersection(evaluated)
                } else {
                    type_id
                }
            }
            EvaluationNeeded::Application { .. }
            | EvaluationNeeded::Mapped { .. }
            | EvaluationNeeded::Conditional { .. } => {
                let evaluated = evaluate_leaf(type_id);
                if evaluated != type_id {
                    evaluated
                } else {
                    type_id
                }
            }
            _ if get_lazy_def_id(db, type_id).is_some() => {
                let evaluated = evaluate_leaf(type_id);
                if evaluated != type_id {
                    evaluated
                } else {
                    type_id
                }
            }
            _ => type_id,
        }
    }

    visit(db, type_id, evaluate_leaf)
}

// =============================================================================
// Compound Type Classification Queries
// =============================================================================

/// Check if a type is a type parameter at the top level, or is an intersection
/// that contains a type parameter member.
///
/// Used by generic call inference to determine whether excess property checking
/// should be skipped for a parameter position (because the type parameter
/// captures the full object type).
pub fn is_type_parameter_or_intersection_with_type_parameter(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    match db.lookup(type_id) {
        Some(TypeData::TypeParameter(_) | TypeData::BoundParameter(_) | TypeData::Infer(_)) => true,
        Some(TypeData::Intersection(list_id)) => {
            let members = db.type_list(list_id);
            members.iter().any(|&m| {
                matches!(
                    db.lookup(m),
                    Some(
                        TypeData::TypeParameter(_)
                            | TypeData::BoundParameter(_)
                            | TypeData::Infer(_)
                    )
                )
            })
        }
        _ => false,
    }
}

/// Check if both types are application (generic instantiation) types and the
/// parameter type contains type parameters.
///
/// When true, the parameter type should be preserved without evaluation during
/// generic inference, because evaluating it would lose the type parameter
/// information needed for inference against the argument type.
pub fn should_preserve_application_for_inference(
    db: &dyn TypeDatabase,
    param_type: TypeId,
    arg_type: TypeId,
) -> bool {
    matches!(db.lookup(param_type), Some(TypeData::Application(_)))
        && matches!(db.lookup(arg_type), Some(TypeData::Application(_)))
        && super::data::contains_type_parameters_db(db, param_type)
}

/// Check if a type represents an unresolved inference result.
///
/// Returns true if the type is `error`, contains infer types, or transitively
/// references `error`. Used to detect provisional inference results from
/// Round 1 of generic call resolution that should not pollute outer inference.
pub fn is_unresolved_inference_result(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    type_id == TypeId::ERROR
        || super::data::contains_infer_types_db(db, type_id)
        || crate::visitor::collect_referenced_types(db, type_id).contains(&TypeId::ERROR)
}
#[cfg(test)]
mod boolean_literal_array_display_tests {
    use super::boolean_literal_array_display_type;
    use crate::TypeId;
    use crate::construction::TypeInterner;

    #[test]
    fn widens_mutable_boolean_literal_array_to_boolean_array() {
        let db = TypeInterner::new();
        let boolean_array = db.array(TypeId::BOOLEAN);

        for value in [true, false] {
            let literal = db.literal_boolean(value);
            let literal_array = db.array(literal);
            assert_eq!(
                boolean_literal_array_display_type(&db, literal_array),
                Some(boolean_array),
                "Array<{value}> should widen to boolean[]"
            );
        }
    }

    #[test]
    fn leaves_non_boolean_literal_arrays_untouched() {
        let db = TypeInterner::new();
        assert_eq!(
            boolean_literal_array_display_type(&db, db.array(TypeId::BOOLEAN)),
            None
        );
        assert_eq!(
            boolean_literal_array_display_type(&db, db.array(db.literal_number(1.0))),
            None
        );
        assert_eq!(
            boolean_literal_array_display_type(&db, db.array(TypeId::STRING)),
            None
        );
        assert_eq!(
            boolean_literal_array_display_type(&db, TypeId::BOOLEAN),
            None
        );
        assert_eq!(
            boolean_literal_array_display_type(&db, db.literal_boolean(true)),
            None
        );
    }
}
