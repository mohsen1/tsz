//! Deep content predicate walkers for solver `TypeData` graphs.

use crate::construction::TypeDatabase;
use crate::types::IntrinsicKind;
use crate::{TypeData, TypeId};
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_common::Atom;

use super::predicate_pool::with_predicate_buffers;

/// Check if a type contains any type parameters.
///
/// The depth-limited walk always starts from a fresh recursion guard, so the
/// answer is a pure function of the root `TypeId`. Recursive conditional
/// evaluation re-asks this for the same check/extends roots constantly, so the
/// root result is memoized project-wide on the interner.
pub fn contains_type_parameters(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    if let Some(cached) = types.contains_param_or_infer_root_cached(type_id) {
        return cached;
    }
    let result = contains_type_matching(types, type_id, |key| {
        matches!(key, TypeData::TypeParameter(_) | TypeData::Infer(_))
    });
    types.set_contains_param_or_infer_root_cache(type_id, result);
    result
}

/// Check if a type contains free type parameters, excluding those bound by
/// enclosing function/callable signatures. See `contains_free_type_parameters_db`
/// in `content_predicates` for the full doc.
pub fn contains_free_type_parameters(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    let mut checker = FreeTypeParamChecker {
        types,
        memo: FxHashMap::default(),
        guard: crate::recursion::RecursionGuard::with_profile(
            crate::recursion::RecursionProfile::ShallowTraversal,
        ),
    };
    checker.check(type_id)
}

/// Check if a type contains any `infer` types.
pub fn contains_infer_types(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    contains_type_matching(types, type_id, |key| matches!(key, TypeData::Infer(_)))
}

/// Check if a type contains any "free" `infer` types — inference placeholders
/// that are NOT buried inside a `TypeParameter`'s constraint or default.
///
/// `TypeParameter` constraints/defaults are definitional (e.g., `T extends Foo`
/// where `Foo = X extends Bar<infer V> ? V : never`). The `infer V` there is
/// structural and already resolved at the definition site. Walking into it
/// produces false positives when used to decide whether to suppress diagnostics.
///
/// This variant is used by `should_suppress_assignability_diagnostic` to avoid
/// suppressing real errors like TS2322 when the only `infer` types are in
/// type parameter constraint chains.
pub fn contains_free_infer_types(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    let mut checker = FreeInferChecker {
        types,
        memo: FxHashMap::default(),
        guard: crate::recursion::RecursionGuard::with_profile(
            crate::recursion::RecursionProfile::ShallowTraversal,
        ),
    };
    checker.check(type_id)
}

/// Check if a type contains the `any` intrinsic anywhere.
pub fn contains_any_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id == TypeId::ANY {
        return true;
    }
    contains_type_matching(types, type_id, |key| {
        matches!(key, TypeData::Intrinsic(IntrinsicKind::Any))
    })
}

/// Check if a type contains the error type.
///
/// This handles `TypeId::ERROR` directly and also detects error types nested
/// inside Application types (e.g., `Application(Error, args)` which displays
/// as `error<args>`). The generic `contains_type_matching` visitor can't catch
/// these because (a) its intrinsic fast-path skips `TypeId::ERROR` and (b) it
/// doesn't check Application bases.
pub fn contains_error_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id == TypeId::ERROR {
        return true;
    }
    contains_error_type_recursive(types, type_id, &mut FxHashMap::default())
}

fn contains_error_type_recursive(
    types: &dyn TypeDatabase,
    type_id: TypeId,
    memo: &mut FxHashMap<TypeId, bool>,
) -> bool {
    if type_id == TypeId::ERROR {
        return true;
    }
    if type_id.is_intrinsic() {
        return false;
    }
    if let Some(&cached) = memo.get(&type_id) {
        return cached;
    }
    // Mark as false to break cycles
    memo.insert(type_id, false);

    let Some(key) = types.lookup(type_id) else {
        return false;
    };
    if matches!(key, TypeData::Error | TypeData::UnresolvedTypeName(_)) {
        memo.insert(type_id, true);
        return true;
    }

    // Terminal-kind fast path. These variants have no children to recurse
    // into and fall through the match below to `_ => false`. Short-circuiting
    // here skips the eight-arm dispatch and the trailing memo write (we
    // already inserted `false` at line 462 for cycle prevention, and the
    // match's `_ => false` would just rewrite the same value).
    if matches!(
        key,
        TypeData::Literal(_)
            | TypeData::ThisType
            | TypeData::BoundParameter(_)
            | TypeData::Lazy(_)
            | TypeData::Recursive(_)
            | TypeData::TypeQuery(_)
            | TypeData::UniqueSymbol(_)
            | TypeData::ModuleNamespace(_)
            | TypeData::Intrinsic(_)
    ) {
        return false;
    }

    let result = match key {
        TypeData::Application(app_id) => {
            let app = types.type_application(app_id);
            // Check both base AND args for error types. Unlike the generic
            // contains_type_matching which skips bases to avoid false positives
            // with type parameters, error types in the base are always wrong.
            contains_error_type_recursive(types, app.base, memo)
                || app
                    .args
                    .iter()
                    .any(|&a| contains_error_type_recursive(types, a, memo))
        }
        TypeData::Union(list_id) | TypeData::Intersection(list_id) => {
            let members = types.type_list(list_id);
            members
                .iter()
                .any(|&m| contains_error_type_recursive(types, m, memo))
        }
        TypeData::Tuple(tuple_list_id) => {
            let elements = types.tuple_list(tuple_list_id);
            elements
                .iter()
                .any(|elem| contains_error_type_recursive(types, elem.type_id, memo))
        }
        TypeData::Array(element_type) => contains_error_type_recursive(types, element_type, memo),
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            let shape = types.object_shape(shape_id);
            shape.properties.iter().any(|prop| {
                contains_error_type_recursive(types, prop.type_id, memo)
                    || contains_error_type_recursive(types, prop.write_type, memo)
            }) || shape.string_index.as_ref().is_some_and(|index| {
                contains_error_type_recursive(types, index.key_type, memo)
                    || contains_error_type_recursive(types, index.value_type, memo)
            }) || shape.number_index.as_ref().is_some_and(|index| {
                contains_error_type_recursive(types, index.key_type, memo)
                    || contains_error_type_recursive(types, index.value_type, memo)
            })
        }
        TypeData::Function(shape_id) => {
            let shape = types.function_shape(shape_id);
            contains_error_type_recursive(types, shape.return_type, memo)
                || shape
                    .params
                    .iter()
                    .any(|p| contains_error_type_recursive(types, p.type_id, memo))
        }
        TypeData::Callable(shape_id) => {
            let shape = types.callable_shape(shape_id);
            shape.call_signatures.iter().any(|sig| {
                sig.params
                    .iter()
                    .any(|param| contains_error_type_recursive(types, param.type_id, memo))
                    || contains_error_type_recursive(types, sig.return_type, memo)
                    || sig.this_type.is_some_and(|this_type| {
                        contains_error_type_recursive(types, this_type, memo)
                    })
            }) || shape.construct_signatures.iter().any(|sig| {
                sig.params
                    .iter()
                    .any(|param| contains_error_type_recursive(types, param.type_id, memo))
                    || contains_error_type_recursive(types, sig.return_type, memo)
                    || sig.this_type.is_some_and(|this_type| {
                        contains_error_type_recursive(types, this_type, memo)
                    })
            }) || shape.properties.iter().any(|prop| {
                contains_error_type_recursive(types, prop.type_id, memo)
                    || contains_error_type_recursive(types, prop.write_type, memo)
            }) || shape.string_index.as_ref().is_some_and(|index| {
                contains_error_type_recursive(types, index.key_type, memo)
                    || contains_error_type_recursive(types, index.value_type, memo)
            }) || shape.number_index.as_ref().is_some_and(|index| {
                contains_error_type_recursive(types, index.key_type, memo)
                    || contains_error_type_recursive(types, index.value_type, memo)
            })
        }
        _ => false,
    };
    memo.insert(type_id, result);
    result
}

/// Check if a type contains the `this` type anywhere.
///
/// The result is stable per `TypeId` within a single `TypeInterner`, so we
/// memoize in a project-wide `DashMap` on the interner to avoid the repeated
/// recursive walk that profiled at ~5% of total CPU on multi-file workloads.
#[inline]
pub fn contains_this_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    // Fast path: intrinsic types never contain ThisType
    if type_id.is_intrinsic() {
        return false;
    }
    // The deep walk is memoized per node in the shared `contains_this` cache,
    // so repeated checks over the same large recursive shapes stay O(1).
    crate::type_queries::contains_this_type_db(types, type_id)
}

/// Check if a type contains any type matching a predicate.
pub fn contains_type_matching<F>(types: &dyn TypeDatabase, type_id: TypeId, predicate: F) -> bool
where
    F: Fn(&TypeData) -> bool,
{
    let mut checker = ContainsTypeChecker {
        types,
        predicate,
        memo: FxHashMap::default(),
        guard: crate::recursion::RecursionGuard::with_profile(
            crate::recursion::RecursionProfile::ShallowTraversal,
        ),
    };
    checker.check(type_id)
}

/// Check if a type contains a type parameter with the given name.
///
/// This is a convenience wrapper around `contains_type_matching` that avoids
/// requiring callers to match on `TypeData` internals directly.
pub fn contains_type_parameter_named(
    types: &dyn TypeDatabase,
    type_id: TypeId,
    name: Atom,
) -> bool {
    contains_type_matching(
        types,
        type_id,
        |td| matches!(td, TypeData::TypeParameter(info) if info.name == name),
    )
}

/// Check if a type contains a type parameter with the given name, WITHOUT
/// walking into other type parameters' constraints.
///
/// Unlike `contains_type_parameter_named`, this does not descend into
/// `TypeParameter.constraint` or `TypeParameter.default`. This is important
/// for mapped type circular-constraint detection: in `{ [K in keyof T]: T[K] }`,
/// `K`'s constraint is `keyof T`. The deep check would walk into `T`'s own
/// constraint (which may contain `K`), falsely reporting a cycle.
pub fn contains_type_parameter_named_shallow(
    types: &dyn TypeDatabase,
    type_id: TypeId,
    name: Atom,
) -> bool {
    with_predicate_buffers(|visited, stack| {
        stack.push(type_id);
        while let Some(current) = stack.pop() {
            if current.is_intrinsic() || !visited.insert(current) {
                continue;
            }

            let Some(data) = types.lookup(current) else {
                continue;
            };

            // Check predicate
            if matches!(&data, TypeData::TypeParameter(info) if info.name == name) {
                return true;
            }

            // Visit children but skip TypeParameter/Infer constraints/defaults.
            // For TypeParameter/Infer, we only care about identity (name match),
            // not what their constraints contain.
            if matches!(&data, TypeData::TypeParameter(_) | TypeData::Infer(_)) {
                continue;
            }
            // Terminal kinds have no children to enumerate. Skipping
            // `for_each_child_by_id` (which would iterate an empty child set)
            // saves the closure setup and visitor dispatch on the very common
            // input shape where the predicate is the entry-point lookup result.
            // The kinds listed here match the leaf arms of every other walker
            // that returns `false` for them — see `ContainsTypeChecker.check_key`,
            // `FreeTypeParamChecker.check_key`, and `FreeInferChecker.check_key`.
            if matches!(
                &data,
                TypeData::Literal(_)
                    | TypeData::Error
                    | TypeData::ThisType
                    | TypeData::BoundParameter(_)
                    | TypeData::Lazy(_)
                    | TypeData::Recursive(_)
                    | TypeData::TypeQuery(_)
                    | TypeData::UniqueSymbol(_)
                    | TypeData::ModuleNamespace(_)
                    | TypeData::UnresolvedTypeName(_)
            ) {
                continue;
            }
            // For all other types, use the generic child visitor.
            crate::visitors::visitor::for_each_child_by_id(types, current, |child| {
                if !visited.contains(&child) {
                    stack.push(child);
                }
            });
        }
        false
    })
}

fn type_parameter_identity_matches(
    def_store: &crate::def::DefinitionStore,
    candidate: TypeId,
    target: TypeId,
) -> bool {
    candidate == target
        || def_store
            .find_def_for_type(candidate)
            .zip(def_store.find_def_for_type(target))
            .is_some_and(|(candidate_def, target_def)| candidate_def == target_def)
}

/// Check if a type contains the target type parameter identity, without walking
/// into other type parameters' constraints/defaults.
pub fn contains_type_parameter_identity_shallow(
    types: &dyn TypeDatabase,
    def_store: &crate::def::DefinitionStore,
    type_id: TypeId,
    target: TypeId,
) -> bool {
    with_predicate_buffers(|visited, stack| {
        stack.push(type_id);
        while let Some(current) = stack.pop() {
            if current.is_intrinsic() || !visited.insert(current) {
                continue;
            }

            if type_parameter_identity_matches(def_store, current, target) {
                return true;
            }

            let Some(data) = types.lookup(current) else {
                continue;
            };

            if matches!(&data, TypeData::TypeParameter(_) | TypeData::Infer(_)) {
                continue;
            }
            if matches!(
                &data,
                TypeData::Literal(_)
                    | TypeData::Error
                    | TypeData::ThisType
                    | TypeData::BoundParameter(_)
                    | TypeData::Lazy(_)
                    | TypeData::Recursive(_)
                    | TypeData::TypeQuery(_)
                    | TypeData::UniqueSymbol(_)
                    | TypeData::ModuleNamespace(_)
                    | TypeData::UnresolvedTypeName(_)
            ) {
                continue;
            }
            crate::visitors::visitor::for_each_child_by_id(types, current, |child| {
                if !visited.contains(&child) {
                    stack.push(child);
                }
            });
        }
        false
    })
}

/// Check if a type transitively references any type parameter whose name
/// is in the given set.
///
/// This is more efficient than `collect_referenced_types` followed by
/// per-element `type_param_info` checks, because it short-circuits on
/// the first match.
pub fn references_any_type_param_named(
    types: &dyn TypeDatabase,
    type_id: TypeId,
    names: &rustc_hash::FxHashSet<Atom>,
) -> bool {
    contains_type_matching(
        types,
        type_id,
        |td| matches!(td, TypeData::TypeParameter(info) if names.contains(&info.name)),
    )
}

/// Check if a constraint type references a type parameter along the base-constraint
/// resolution path. This mimics tsc's `getBaseConstraint` recursion, which only
/// follows certain structural paths:
///
/// Descended into (these require resolving sub-constraints):
/// - Union/intersection members
/// - Mapped type constraint (the key source)
/// - Conditional check/extends types
/// - Index access object/index
/// - `KeyOf` operand
///
/// NOT descended into (these are type references/wrappers — tsc treats them as opaque):
/// - Type application arguments (e.g. `Foo<T>`)
/// - Array/Tuple/ReadonlyType/NoInfer inner types (these are effectively type references)
/// - Object property types
/// - Function parameter/return types
///
/// This avoids false positives: `T extends Array<T>` is NOT circular,
/// but `T extends { [P in T]: number }` IS circular.
pub fn constraint_references_type_param_in_resolution_path(
    types: &dyn TypeDatabase,
    type_id: TypeId,
    param_name: Atom,
) -> bool {
    with_predicate_buffers(|visited, stack| {
        stack.push(type_id);
        while let Some(current) = stack.pop() {
            if current.is_intrinsic() || !visited.insert(current) {
                continue;
            }

            let Some(data) = types.lookup(current) else {
                continue;
            };

            // Found the type parameter we're looking for
            if matches!(&data, TypeData::TypeParameter(info) if info.name == param_name) {
                return true;
            }

            // Follow only resolution-path children (not type reference args)
            match &data {
                // Union/intersection: descend into all members
                TypeData::Union(list_id) | TypeData::Intersection(list_id) => {
                    for &member in types.type_list(*list_id).iter() {
                        stack.push(member);
                    }
                }
                // Mapped type: descend into the constraint (key source) only.
                // This catches `T extends { [P in T]: number }` (genuinely circular)
                // while NOT false-positiving on `T extends { [K in keyof T]: V }`
                // because we don't follow through KeyOf (see below).
                TypeData::Mapped(mapped_id) => {
                    let mapped = types.get_mapped(*mapped_id);
                    stack.push(mapped.constraint);
                }
                // Index access: descend into object and index.
                // Catches `T extends Foo | T["hello"]` (circular through index access).
                TypeData::IndexAccess(obj, idx) => {
                    stack.push(*obj);
                    stack.push(*idx);
                }
                // KeyOf, Conditional, and everything else (Application, Object,
                // Function, Array, Tuple, ReadonlyType, NoInfer, etc.) are opaque
                // at the constraint-resolution level. `T extends { [K in keyof T]: V }`
                // is NOT circular in tsc, and neither is `T extends null extends T ? any : never`.
                _ => {}
            }
        }
        false
    })
}

/// Identity-based variant of `constraint_references_type_param_in_resolution_path`.
pub fn constraint_references_type_param_identity_in_resolution_path(
    types: &dyn TypeDatabase,
    def_store: &crate::def::DefinitionStore,
    type_id: TypeId,
    target: TypeId,
) -> bool {
    with_predicate_buffers(|visited, stack| {
        stack.push(type_id);
        while let Some(current) = stack.pop() {
            if current.is_intrinsic() || !visited.insert(current) {
                continue;
            }

            if type_parameter_identity_matches(def_store, current, target) {
                return true;
            }

            let Some(data) = types.lookup(current) else {
                continue;
            };

            match &data {
                TypeData::Union(list_id) | TypeData::Intersection(list_id) => {
                    stack.extend(types.type_list(*list_id).iter().copied());
                }
                TypeData::Mapped(mapped_id) => {
                    stack.push(types.get_mapped(*mapped_id).constraint);
                }
                TypeData::IndexAccess(obj, idx) => {
                    stack.push(*obj);
                    stack.push(*idx);
                }
                _ => {}
            }
        }
        false
    })
}

/// Check if a type transitively contains a specific `TypeId`.
///
/// This is more efficient than `collect_referenced_types(…).contains(&target)`
/// because it short-circuits as soon as the target is found.
pub fn contains_type_by_id(types: &dyn TypeDatabase, root: TypeId, target: TypeId) -> bool {
    if root == target {
        return true;
    }
    let mut visited = FxHashMap::default();
    let mut stack = vec![root];
    while let Some(current) = stack.pop() {
        if current == target {
            return true;
        }
        if visited.contains_key(&current) {
            continue;
        }
        visited.insert(current, true);
        crate::visitors::visitor::for_each_child_by_id(types, current, |child| {
            if !visited.contains_key(&child) {
                stack.push(child);
            }
        });
    }
    false
}

struct ContainsTypeChecker<'a, F>
where
    F: Fn(&TypeData) -> bool,
{
    types: &'a dyn TypeDatabase,
    predicate: F,
    memo: FxHashMap<TypeId, bool>,
    guard: crate::recursion::RecursionGuard<TypeId>,
}

impl<'a, F> ContainsTypeChecker<'a, F>
where
    F: Fn(&TypeData) -> bool,
{
    #[cfg(test)]
    fn memo_entries(&self) -> usize {
        self.memo.len()
    }

    fn check(&mut self, type_id: TypeId) -> bool {
        // Fast path: intrinsic types (primitives, any, never, etc.) have no subtypes
        // and can never contain nested type structures.
        if type_id.is_intrinsic() {
            return false;
        }

        if let Some(&cached) = self.memo.get(&type_id) {
            return cached;
        }

        let Some(key) = self.types.lookup(type_id) else {
            return false;
        };

        if (self.predicate)(&key) {
            self.memo.insert(type_id, true);
            return true;
        }

        // Terminal-kind fast path: types with no children to walk and no
        // cycle risk. The recursive `check_key` below would dispatch to its
        // leaf arm and immediately return `false` for these kinds, so
        // skipping the `guard.enter`/`guard.leave` HashSet round-trip is a
        // pure win. Memo is still updated so repeat visits of the same
        // type within one `contains_type_matching` call stay O(1).
        //
        // `Intrinsic` is already handled by the entry-level `is_intrinsic`
        // check above. The remaining terminal kinds match the recursive
        // walker's leaf arm in `check_key`.
        if matches!(
            key,
            TypeData::Literal(_)
                | TypeData::Error
                | TypeData::ThisType
                | TypeData::BoundParameter(_)
                | TypeData::Lazy(_)
                | TypeData::Recursive(_)
                | TypeData::TypeQuery(_)
                | TypeData::UniqueSymbol(_)
                | TypeData::ModuleNamespace(_)
                | TypeData::UnresolvedTypeName(_)
        ) {
            self.memo.insert(type_id, false);
            return false;
        }

        match self.guard.enter(type_id) {
            crate::recursion::RecursionResult::Entered => {}
            _ => return false,
        }

        let result = self.check_key(&key);

        self.guard.leave(type_id);
        self.memo.insert(type_id, result);

        result
    }

    fn check_key(&mut self, key: &TypeData) -> bool {
        match key {
            TypeData::Intrinsic(_)
            | TypeData::Literal(_)
            | TypeData::Error
            | TypeData::ThisType
            | TypeData::BoundParameter(_)
            | TypeData::Lazy(_)
            | TypeData::Recursive(_)
            | TypeData::TypeQuery(_)
            | TypeData::UniqueSymbol(_)
            | TypeData::ModuleNamespace(_)
            | TypeData::UnresolvedTypeName(_) => false,
            TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
                let shape = self.types.object_shape(*shape_id);
                shape.properties.iter().any(|p| self.check(p.type_id))
                    || shape
                        .string_index
                        .as_ref()
                        .is_some_and(|i| self.check(i.value_type))
                    || shape
                        .number_index
                        .as_ref()
                        .is_some_and(|i| self.check(i.value_type))
            }
            TypeData::Union(list_id) | TypeData::Intersection(list_id) => {
                let members = self.types.type_list(*list_id);
                members.iter().any(|&m| self.check(m))
            }
            TypeData::Array(elem) => self.check(*elem),
            TypeData::Tuple(list_id) => {
                let elements = self.types.tuple_list(*list_id);
                elements.iter().any(|e| self.check(e.type_id))
            }
            TypeData::Function(shape_id) => {
                let shape = self.types.function_shape(*shape_id);
                shape.params.iter().any(|p| self.check(p.type_id))
                    || self.check(shape.return_type)
                    || shape.this_type.is_some_and(|t| self.check(t))
            }
            TypeData::Callable(shape_id) => {
                let shape = self.types.callable_shape(*shape_id);
                shape.call_signatures.iter().any(|s| {
                    s.params.iter().any(|p| self.check(p.type_id))
                        || self.check(s.return_type)
                        || s.this_type.is_some_and(|t| self.check(t))
                }) || shape.construct_signatures.iter().any(|s| {
                    s.params.iter().any(|p| self.check(p.type_id))
                        || self.check(s.return_type)
                        || s.this_type.is_some_and(|t| self.check(t))
                }) || shape.properties.iter().any(|p| self.check(p.type_id))
            }
            TypeData::TypeParameter(info) | TypeData::Infer(info) => {
                info.constraint.is_some_and(|c| self.check(c))
                    || info.default.is_some_and(|d| self.check(d))
            }
            TypeData::Application(app_id) => {
                // Only check args, not base. The base type's own type parameters
                // are bound by the application arguments and should not count as
                // "containing type parameters". E.g., `A<number>` is concrete even
                // though `A`'s definition contains `TypeParameter T`.
                let app = self.types.type_application(*app_id);
                app.args.iter().any(|&a| self.check(a))
            }
            TypeData::Conditional(cond_id) => {
                let cond = self.types.get_conditional(*cond_id);
                self.check(cond.check_type)
                    || self.check(cond.extends_type)
                    || self.check(cond.true_type)
                    || self.check(cond.false_type)
            }
            TypeData::Mapped(mapped_id) => {
                let mapped = self.types.get_mapped(*mapped_id);
                mapped.type_param.constraint.is_some_and(|c| self.check(c))
                    || mapped.type_param.default.is_some_and(|d| self.check(d))
                    || self.check(mapped.constraint)
                    || self.check(mapped.template)
                    || mapped.name_type.is_some_and(|n| self.check(n))
            }
            TypeData::IndexAccess(obj, idx) => self.check(*obj) || self.check(*idx),
            TypeData::TemplateLiteral(list_id) => {
                let spans = self.types.template_list(*list_id);
                spans.iter().any(|span| {
                    if let crate::types::TemplateSpan::Type(type_id) = span {
                        self.check(*type_id)
                    } else {
                        false
                    }
                })
            }
            TypeData::KeyOf(inner) | TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner) => {
                self.check(*inner)
            }
            TypeData::StringIntrinsic { type_arg, .. } => self.check(*type_arg),
            TypeData::Enum(_def_id, member_type) => self.check(*member_type),
        }
    }
}

// =============================================================================
// FreeTypeParamChecker — like ContainsTypeChecker but skips bound type params
// in function/callable signatures
// =============================================================================

struct FreeTypeParamChecker<'a> {
    types: &'a dyn TypeDatabase,
    memo: FxHashMap<TypeId, bool>,
    guard: crate::recursion::RecursionGuard<TypeId>,
}

impl<'a> FreeTypeParamChecker<'a> {
    #[cfg(test)]
    fn memo_entries(&self) -> usize {
        self.memo.len()
    }

    fn check(&mut self, type_id: TypeId) -> bool {
        if type_id.is_intrinsic() {
            return false;
        }
        if let Some(&cached) = self.memo.get(&type_id) {
            return cached;
        }
        let Some(key) = self.types.lookup(type_id) else {
            return false;
        };
        if matches!(
            key,
            TypeData::TypeParameter(_)
                | TypeData::Infer(_)
                | TypeData::ThisType
                | TypeData::BoundParameter(_)
        ) {
            self.memo.insert(type_id, true);
            return true;
        }
        // Terminal-kind fast path: same set that `check_key` returns `false`
        // for unconditionally. Short-circuit before the recursion-guard
        // enter/leave so common terminals (`Lazy(DefId)`, `TypeQuery`, etc.)
        // skip the per-call `FxHashSet` insert + remove. Mirrors #1978/#1990.
        if matches!(
            key,
            TypeData::Intrinsic(_)
                | TypeData::Literal(_)
                | TypeData::Error
                | TypeData::Lazy(_)
                | TypeData::Recursive(_)
                | TypeData::TypeQuery(_)
                | TypeData::UniqueSymbol(_)
                | TypeData::ModuleNamespace(_)
                | TypeData::UnresolvedTypeName(_)
        ) {
            self.memo.insert(type_id, false);
            return false;
        }
        match self.guard.enter(type_id) {
            crate::recursion::RecursionResult::Entered => {}
            _ => return false,
        }
        let result = self.check_key(&key);
        self.guard.leave(type_id);
        self.memo.insert(type_id, result);
        result
    }

    fn check_key(&mut self, key: &TypeData) -> bool {
        match key {
            TypeData::Intrinsic(_)
            | TypeData::Literal(_)
            | TypeData::Error
            | TypeData::ThisType
            | TypeData::BoundParameter(_)
            | TypeData::Lazy(_)
            | TypeData::Recursive(_)
            | TypeData::TypeQuery(_)
            | TypeData::UniqueSymbol(_)
            | TypeData::ModuleNamespace(_)
            | TypeData::UnresolvedTypeName(_) => false,
            TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
                let shape = self.types.object_shape(*shape_id);
                shape.properties.iter().any(|p| self.check(p.type_id))
                    || shape
                        .string_index
                        .as_ref()
                        .is_some_and(|i| self.check(i.value_type))
                    || shape
                        .number_index
                        .as_ref()
                        .is_some_and(|i| self.check(i.value_type))
            }
            TypeData::Union(list_id) | TypeData::Intersection(list_id) => {
                let members = self.types.type_list(*list_id);
                members.iter().any(|&m| self.check(m))
            }
            TypeData::Array(elem) => self.check(*elem),
            TypeData::Tuple(list_id) => {
                let elements = self.types.tuple_list(*list_id);
                elements.iter().any(|e| self.check(e.type_id))
            }
            TypeData::Function(shape_id) => {
                let shape = self.types.function_shape(*shape_id);
                if !shape.type_params.is_empty() {
                    // Generic function: type params in body are bound, not free.
                    // Skip body traversal to avoid counting bound params.
                    return false;
                }
                shape.params.iter().any(|p| self.check(p.type_id))
                    || self.check(shape.return_type)
                    || shape.this_type.is_some_and(|t| self.check(t))
            }
            TypeData::Callable(shape_id) => {
                let shape = self.types.callable_shape(*shape_id);
                shape.call_signatures.iter().any(|s| {
                    if !s.type_params.is_empty() {
                        return false;
                    }
                    s.params.iter().any(|p| self.check(p.type_id))
                        || self.check(s.return_type)
                        || s.this_type.is_some_and(|t| self.check(t))
                }) || shape.construct_signatures.iter().any(|s| {
                    if !s.type_params.is_empty() {
                        return false;
                    }
                    s.params.iter().any(|p| self.check(p.type_id))
                        || self.check(s.return_type)
                        || s.this_type.is_some_and(|t| self.check(t))
                }) || shape.properties.iter().any(|p| self.check(p.type_id))
            }
            TypeData::TypeParameter(info) | TypeData::Infer(info) => {
                info.constraint.is_some_and(|c| self.check(c))
                    || info.default.is_some_and(|d| self.check(d))
            }
            TypeData::Application(app_id) => {
                let app = self.types.type_application(*app_id);
                app.args.iter().any(|&a| self.check(a))
            }
            TypeData::Conditional(cond_id) => {
                let cond = self.types.get_conditional(*cond_id);
                self.check(cond.check_type)
                    || self.check(cond.extends_type)
                    || self.check(cond.true_type)
                    || self.check(cond.false_type)
            }
            TypeData::Mapped(mapped_id) => {
                let mapped = self.types.get_mapped(*mapped_id);
                mapped.type_param.constraint.is_some_and(|c| self.check(c))
                    || mapped.type_param.default.is_some_and(|d| self.check(d))
                    || self.check(mapped.constraint)
                    || self.check(mapped.template)
                    || mapped.name_type.is_some_and(|n| self.check(n))
            }
            TypeData::IndexAccess(obj, idx) => self.check(*obj) || self.check(*idx),
            TypeData::TemplateLiteral(list_id) => {
                let spans = self.types.template_list(*list_id);
                spans.iter().any(|span| {
                    if let crate::types::TemplateSpan::Type(type_id) = span {
                        self.check(*type_id)
                    } else {
                        false
                    }
                })
            }
            TypeData::KeyOf(inner) | TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner) => {
                self.check(*inner)
            }
            TypeData::StringIntrinsic { type_arg, .. } => self.check(*type_arg),
            TypeData::Enum(_def_id, member_type) => self.check(*member_type),
        }
    }
}

/// Collect the `TypeId`s of every `TypeParameter`/`Infer` that occurs *free*
/// across `roots`, i.e. not bound by an enclosing generic function/callable
/// signature.
///
/// Type parameters in tsz are interned structurally by name, so two unrelated
/// `T`s share a `TypeId`. Callers that compare against a specific parameter's
/// identity must therefore only consider free occurrences: a parameter *bound*
/// by a nested generic signature in the traversed type is a distinct
/// declaration that merely shares an interned name, and must not be reported.
///
/// This mirrors [`FreeTypeParamChecker`]'s binder handling — the body of a
/// nested generic signature is skipped wholesale rather than re-scoped — but
/// returns the set of free parameter ids instead of a boolean. The result is a
/// pure function of the input types and is memoized across all `roots` in a
/// single pass (so e.g. a signature's parameters and return type share the
/// walk); the walk is `stacker`-guarded because it runs inside the (already
/// deep) subtype relation.
///
/// A `TypeParameter`'s `constraint`/`default` are metadata, not free uses, so
/// they are not traversed — matching the rest of the free-parameter walkers.
pub fn free_type_parameter_ids_in(
    types: &dyn TypeDatabase,
    roots: impl IntoIterator<Item = TypeId>,
) -> FxHashSet<TypeId> {
    let mut collector = FreeTypeParamCollector {
        types,
        memo: FxHashMap::default(),
        guard: crate::recursion::RecursionGuard::with_profile(
            crate::recursion::RecursionProfile::ShallowTraversal,
        ),
    };
    let mut out = FxHashSet::default();
    for root in roots {
        out.extend(collector.free(root));
    }
    out
}

struct FreeTypeParamCollector<'a> {
    types: &'a dyn TypeDatabase,
    /// Memoized free-parameter set per `TypeId`. Freeness is a pure function of
    /// the type (a generic signature contributes nothing; everything else is the
    /// union of its children), so it can be cached without an ambient scope.
    /// Memoization also keeps the walk linear over a shared type DAG — without it
    /// a signature reused across many positions would be re-expanded and can
    /// overflow the stack on real-world types.
    memo: FxHashMap<TypeId, FxHashSet<TypeId>>,
    guard: crate::recursion::RecursionGuard<TypeId>,
}

impl<'a> FreeTypeParamCollector<'a> {
    fn free(&mut self, type_id: TypeId) -> FxHashSet<TypeId> {
        if type_id.is_intrinsic() {
            return FxHashSet::default();
        }
        if let Some(cached) = self.memo.get(&type_id) {
            return cached.clone();
        }
        let Some(key) = self.types.lookup(type_id) else {
            return FxHashSet::default();
        };
        // Terminal-kind fast path: these variants have no children and contribute
        // no free parameters, so skip the recursion-guard/memo bookkeeping
        // entirely. Mirrors `FreeTypeParamChecker::check`. `TypeParameter`/`Infer`
        // are intentionally excluded — they are the leaves we collect.
        if matches!(
            key,
            TypeData::Intrinsic(_)
                | TypeData::Literal(_)
                | TypeData::Error
                | TypeData::ThisType
                | TypeData::BoundParameter(_)
                | TypeData::Lazy(_)
                | TypeData::Recursive(_)
                | TypeData::TypeQuery(_)
                | TypeData::UniqueSymbol(_)
                | TypeData::ModuleNamespace(_)
                | TypeData::UnresolvedTypeName(_)
        ) {
            return FxHashSet::default();
        }
        // Cycle back-edges contribute no new free parameters (the parameter is
        // already reachable from the ancestor on the stack), so return empty on
        // re-entry and do not memoize the partial result.
        match self.guard.enter(type_id) {
            crate::recursion::RecursionResult::Entered => {}
            _ => return FxHashSet::default(),
        }
        // Grow the native stack on demand: this walk runs *inside* the already
        // deep subtype-relation recursion, so a deeply nested type can otherwise
        // overflow. Mirrors `RecursiveTypeCollector::visit`.
        let result =
            stacker::maybe_grow(256 * 1024, 2 * 1024 * 1024, || self.free_key(type_id, &key));
        self.guard.leave(type_id);
        self.memo.insert(type_id, result.clone());
        result
    }

    /// Free parameters of a signature. A *generic* signature binds its own type
    /// parameters, so its body is skipped entirely — mirroring
    /// [`FreeTypeParamChecker`]. This intentionally does not descend into a
    /// generic signature to recover an outer parameter threaded through it; that
    /// extra precision is unnecessary for the identity-sharing decision this
    /// helper drives, and descending makes the walk dramatically deeper on
    /// real-world recursive signature graphs. A *non-generic* signature binds
    /// nothing, so its children's free parameters pass through.
    fn free_signature(
        &mut self,
        is_generic: bool,
        params: impl Iterator<Item = TypeId>,
        return_type: TypeId,
        this_type: Option<TypeId>,
    ) -> FxHashSet<TypeId> {
        let mut set = FxHashSet::default();
        if is_generic {
            return set;
        }
        for p in params {
            set.extend(self.free(p));
        }
        set.extend(self.free(return_type));
        if let Some(t) = this_type {
            set.extend(self.free(t));
        }
        set
    }

    fn free_key(&mut self, type_id: TypeId, key: &TypeData) -> FxHashSet<TypeId> {
        let mut set = FxHashSet::default();
        match key {
            TypeData::TypeParameter(_) | TypeData::Infer(_) => {
                // A free occurrence. Its constraint/default are metadata, not
                // free uses, so they are intentionally not traversed.
                set.insert(type_id);
            }
            TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
                // `object_shape` returns an owned `Arc`, so the iteration borrow
                // is independent of `&mut self` and needs no intermediate Vec.
                let shape = self.types.object_shape(*shape_id);
                for child in shape
                    .properties
                    .iter()
                    .map(|p| p.type_id)
                    .chain(shape.string_index.as_ref().map(|i| i.value_type))
                    .chain(shape.number_index.as_ref().map(|i| i.value_type))
                {
                    set.extend(self.free(child));
                }
            }
            TypeData::Union(list_id) | TypeData::Intersection(list_id) => {
                let members = self.types.type_list(*list_id);
                for &m in members.iter() {
                    set.extend(self.free(m));
                }
            }
            TypeData::Array(elem) => set.extend(self.free(*elem)),
            TypeData::Tuple(list_id) => {
                let elems = self.types.tuple_list(*list_id);
                for e in elems.iter() {
                    set.extend(self.free(e.type_id));
                }
            }
            TypeData::Function(shape_id) => {
                let shape = self.types.function_shape(*shape_id);
                set = self.free_signature(
                    !shape.type_params.is_empty(),
                    shape.params.iter().map(|p| p.type_id),
                    shape.return_type,
                    shape.this_type,
                );
            }
            TypeData::Callable(shape_id) => {
                let shape = self.types.callable_shape(*shape_id);
                for s in shape
                    .call_signatures
                    .iter()
                    .chain(shape.construct_signatures.iter())
                {
                    set.extend(self.free_signature(
                        !s.type_params.is_empty(),
                        s.params.iter().map(|p| p.type_id),
                        s.return_type,
                        s.this_type,
                    ));
                }
                for p in shape.properties.iter() {
                    set.extend(self.free(p.type_id));
                }
            }
            TypeData::Application(app_id) => {
                let app = self.types.type_application(*app_id);
                for &a in app.args.iter() {
                    set.extend(self.free(a));
                }
            }
            TypeData::Conditional(cond_id) => {
                let cond = self.types.get_conditional(*cond_id);
                let parts = [
                    cond.check_type,
                    cond.extends_type,
                    cond.true_type,
                    cond.false_type,
                ];
                for part in parts {
                    set.extend(self.free(part));
                }
            }
            TypeData::Mapped(mapped_id) => {
                let mapped = self.types.get_mapped(*mapped_id);
                for part in [
                    Some(mapped.constraint),
                    Some(mapped.template),
                    mapped.name_type,
                ]
                .into_iter()
                .flatten()
                {
                    set.extend(self.free(part));
                }
            }
            TypeData::IndexAccess(obj, idx) => {
                set.extend(self.free(*obj));
                set.extend(self.free(*idx));
            }
            TypeData::TemplateLiteral(list_id) => {
                let spans = self.types.template_list(*list_id);
                for span in spans.iter() {
                    if let crate::types::TemplateSpan::Type(t) = span {
                        set.extend(self.free(*t));
                    }
                }
            }
            TypeData::KeyOf(inner) | TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner) => {
                set.extend(self.free(*inner));
            }
            TypeData::StringIntrinsic { type_arg, .. } => set.extend(self.free(*type_arg)),
            TypeData::Enum(_def_id, member_type) => set.extend(self.free(*member_type)),
            TypeData::Intrinsic(_)
            | TypeData::Literal(_)
            | TypeData::Error
            | TypeData::ThisType
            | TypeData::BoundParameter(_)
            | TypeData::Lazy(_)
            | TypeData::Recursive(_)
            | TypeData::TypeQuery(_)
            | TypeData::UniqueSymbol(_)
            | TypeData::ModuleNamespace(_)
            | TypeData::UnresolvedTypeName(_) => {}
        }
        set
    }
}

// =============================================================================
// FreeInferChecker — like ContainsTypeChecker but skips TypeParameter constraints
// =============================================================================

struct FreeInferChecker<'a> {
    types: &'a dyn TypeDatabase,
    memo: FxHashMap<TypeId, bool>,
    guard: crate::recursion::RecursionGuard<TypeId>,
}

impl<'a> FreeInferChecker<'a> {
    #[cfg(test)]
    fn memo_entries(&self) -> usize {
        self.memo.len()
    }

    fn check(&mut self, type_id: TypeId) -> bool {
        if type_id.is_intrinsic() {
            return false;
        }
        if let Some(&cached) = self.memo.get(&type_id) {
            return cached;
        }
        let Some(key) = self.types.lookup(type_id) else {
            return false;
        };
        if matches!(key, TypeData::Infer(_)) {
            self.memo.insert(type_id, true);
            return true;
        }
        // Terminal-kind fast path: same set that `check_key` returns `false`
        // for unconditionally (TypeParameter is included here because this
        // walker, by design, does not descend into TypeParameter
        // constraints/defaults). Short-circuit before the recursion-guard
        // enter/leave dance. Mirrors #1978/#1990.
        if matches!(
            key,
            TypeData::Intrinsic(_)
                | TypeData::Literal(_)
                | TypeData::Error
                | TypeData::ThisType
                | TypeData::BoundParameter(_)
                | TypeData::Lazy(_)
                | TypeData::Recursive(_)
                | TypeData::TypeQuery(_)
                | TypeData::UniqueSymbol(_)
                | TypeData::ModuleNamespace(_)
                | TypeData::TypeParameter(_)
                | TypeData::UnresolvedTypeName(_)
        ) {
            self.memo.insert(type_id, false);
            return false;
        }
        match self.guard.enter(type_id) {
            crate::recursion::RecursionResult::Entered => {}
            _ => return false,
        }
        let result = self.check_key(&key);
        self.guard.leave(type_id);
        self.memo.insert(type_id, result);
        result
    }

    fn check_key(&mut self, key: &TypeData) -> bool {
        match key {
            TypeData::Intrinsic(_)
            | TypeData::Literal(_)
            | TypeData::Error
            | TypeData::ThisType
            | TypeData::BoundParameter(_)
            | TypeData::Lazy(_)
            | TypeData::Recursive(_)
            | TypeData::TypeQuery(_)
            | TypeData::UniqueSymbol(_)
            | TypeData::ModuleNamespace(_)
            // TypeParameter/Infer: do NOT walk into constraints/defaults.
            // Structural `infer` patterns in constraints (e.g., from type alias
            // definitions like `type Foo = X extends Bar<infer V> ? V : never`)
            // are definitional, not live inference variables.
            | TypeData::TypeParameter(_)
            | TypeData::Infer(_)
            | TypeData::UnresolvedTypeName(_) => false,
            TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
                let shape = self.types.object_shape(*shape_id);
                shape.properties.iter().any(|p| self.check(p.type_id))
                    || shape
                        .string_index
                        .as_ref()
                        .is_some_and(|i| self.check(i.value_type))
                    || shape
                        .number_index
                        .as_ref()
                        .is_some_and(|i| self.check(i.value_type))
            }
            TypeData::Union(list_id) | TypeData::Intersection(list_id) => {
                let members = self.types.type_list(*list_id);
                members.iter().any(|&m| self.check(m))
            }
            TypeData::Array(elem) => self.check(*elem),
            TypeData::Tuple(list_id) => {
                let elements = self.types.tuple_list(*list_id);
                elements.iter().any(|e| self.check(e.type_id))
            }
            TypeData::Function(shape_id) => {
                let shape = self.types.function_shape(*shape_id);
                shape.params.iter().any(|p| self.check(p.type_id))
                    || self.check(shape.return_type)
                    || shape.this_type.is_some_and(|t| self.check(t))
            }
            TypeData::Callable(shape_id) => {
                let shape = self.types.callable_shape(*shape_id);
                shape.call_signatures.iter().any(|s| {
                    s.params.iter().any(|p| self.check(p.type_id))
                        || self.check(s.return_type)
                        || s.this_type.is_some_and(|t| self.check(t))
                }) || shape.construct_signatures.iter().any(|s| {
                    s.params.iter().any(|p| self.check(p.type_id))
                        || self.check(s.return_type)
                        || s.this_type.is_some_and(|t| self.check(t))
                }) || shape.properties.iter().any(|p| self.check(p.type_id))
            }
            TypeData::Application(app_id) => {
                let app = self.types.type_application(*app_id);
                app.args.iter().any(|&a| self.check(a))
            }
            TypeData::Conditional(cond_id) => {
                let cond = self.types.get_conditional(*cond_id);
                self.check(cond.check_type)
                    || self.check(cond.extends_type)
                    || self.check(cond.true_type)
                    || self.check(cond.false_type)
            }
            TypeData::Mapped(mapped_id) => {
                let mapped = self.types.get_mapped(*mapped_id);
                mapped.type_param.constraint.is_some_and(|c| self.check(c))
                    || mapped.type_param.default.is_some_and(|d| self.check(d))
                    || self.check(mapped.constraint)
                    || self.check(mapped.template)
                    || mapped.name_type.is_some_and(|n| self.check(n))
            }
            TypeData::IndexAccess(obj, idx) => self.check(*obj) || self.check(*idx),
            TypeData::TemplateLiteral(list_id) => {
                let spans = self.types.template_list(*list_id);
                spans.iter().any(|span| {
                    if let crate::types::TemplateSpan::Type(type_id) = span {
                        self.check(*type_id)
                    } else {
                        false
                    }
                })
            }
            TypeData::KeyOf(inner) | TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner) => {
                self.check(*inner)
            }
            TypeData::StringIntrinsic { type_arg, .. } => self.check(*type_arg),
            TypeData::Enum(_def_id, member_type) => self.check(*member_type),
        }
    }
}

/// Check whether `type_id` contains a *free* reference to a type parameter
/// other than `excluded_name`, treating each `TypeParameter`/`Infer` as a leaf.
///
/// A `TypeParameter`'s `constraint`/`default` are metadata, not uses:
/// the iteration variable `K` in `{ [K in keyof T as ...]: T[K] }` carries
/// a stale `keyof T` constraint after `T` is substituted, since the `K`
/// instances inside the body still reference the pre-substitution record.
pub fn contains_free_type_parameters_except_name(
    types: &dyn TypeDatabase,
    type_id: TypeId,
    excluded_name: Atom,
) -> bool {
    with_predicate_buffers(|visited, stack| {
        stack.push(type_id);
        while let Some(current) = stack.pop() {
            if current.is_intrinsic() || !visited.insert(current) {
                continue;
            }
            let Some(data) = types.lookup(current) else {
                continue;
            };
            match &data {
                TypeData::TypeParameter(info) | TypeData::Infer(info) => {
                    if info.name != excluded_name {
                        return true;
                    }
                    // Skip the parameter's `constraint`/`default` — those are
                    // metadata for the parameter, not uses by the enclosing
                    // type. Same reason applies to Mapped/Function/Callable
                    // type-param lists handled in the visit_structural_children
                    // path below.
                    continue;
                }
                TypeData::ThisType | TypeData::BoundParameter(_) => return true,
                _ => {}
            }
            visit_structural_children(types, current, &data, |child| {
                if !visited.contains(&child) {
                    stack.push(child);
                }
            });
        }
        false
    })
}

/// Variant of [`crate::visitors::visitor::for_each_child_by_id`] that skips type-
/// parameter `constraint`/`default` metadata on `Mapped`, `Function`, and
/// `Callable` types. Used by free-type-parameter checks that must treat
/// parameter-declaration metadata as bound by the host, not as free uses.
fn visit_structural_children<F>(db: &dyn TypeDatabase, type_id: TypeId, data: &TypeData, mut f: F)
where
    F: FnMut(TypeId),
{
    match data {
        TypeData::Mapped(mapped_id) => {
            let mapped = db.get_mapped(*mapped_id);
            f(mapped.constraint);
            f(mapped.template);
            if let Some(name_type) = mapped.name_type {
                f(name_type);
            }
        }
        TypeData::Function(func_id) => {
            let sig = db.function_shape(*func_id);
            f(sig.return_type);
            if let Some(this_type) = sig.this_type {
                f(this_type);
            }
            if let Some(predicate) = sig.type_predicate.as_ref()
                && let Some(predicate_type) = predicate.type_id
            {
                f(predicate_type);
            }
            for param in &sig.params {
                f(param.type_id);
            }
        }
        TypeData::Callable(callable_id) => {
            let callable = db.callable_shape(*callable_id);
            for sig in callable
                .call_signatures
                .iter()
                .chain(callable.construct_signatures.iter())
            {
                f(sig.return_type);
                if let Some(this_type) = sig.this_type {
                    f(this_type);
                }
                if let Some(predicate) = sig.type_predicate.as_ref()
                    && let Some(predicate_type) = predicate.type_id
                {
                    f(predicate_type);
                }
                for param in &sig.params {
                    f(param.type_id);
                }
            }
            for prop in &callable.properties {
                f(prop.type_id);
                f(prop.write_type);
            }
            if let Some(sig) = callable.string_index.as_ref() {
                f(sig.key_type);
                f(sig.value_type);
            }
            if let Some(sig) = callable.number_index.as_ref() {
                f(sig.key_type);
                f(sig.value_type);
            }
        }
        _ => crate::visitors::visitor::for_each_child_by_id(db, type_id, f),
    }
}

// =============================================================================
// ShallowContainsTypeChecker — checks type parameter name without traversing
// into type parameter constraints/defaults (prevents false circularity detection)
// =============================================================================

#[allow(dead_code)]
struct ShallowContainsTypeChecker<'a> {
    types: &'a dyn TypeDatabase,
    name: Atom,
    memo: FxHashMap<TypeId, bool>,
    guard: crate::recursion::RecursionGuard<TypeId>,
}

#[allow(dead_code)]
impl<'a> ShallowContainsTypeChecker<'a> {
    #[cfg(test)]
    fn memo_entries(&self) -> usize {
        self.memo.len()
    }

    fn check(&mut self, type_id: TypeId) -> bool {
        if type_id.is_intrinsic() {
            return false;
        }
        if let Some(&cached) = self.memo.get(&type_id) {
            return cached;
        }
        let Some(key) = self.types.lookup(type_id) else {
            return false;
        };
        // Direct match: is this type parameter the one we're looking for?
        if matches!(&key, TypeData::TypeParameter(info) if info.name == self.name) {
            self.memo.insert(type_id, true);
            return true;
        }
        // Terminal-kind fast path: same set that `check_key` returns `false`
        // for unconditionally. Note: `TypeParameter(_)` is also a terminal
        // here — by design "shallow" does not descend into constraints —
        // but we exclude it from this short-circuit because the positive
        // match above already drained the matching name. Any remaining
        // `TypeParameter` is a non-match terminal. Mirrors #1978/#1990.
        if matches!(
            key,
            TypeData::Intrinsic(_)
                | TypeData::Literal(_)
                | TypeData::Error
                | TypeData::ThisType
                | TypeData::BoundParameter(_)
                | TypeData::Lazy(_)
                | TypeData::Recursive(_)
                | TypeData::TypeQuery(_)
                | TypeData::UniqueSymbol(_)
                | TypeData::ModuleNamespace(_)
                | TypeData::TypeParameter(_)
                | TypeData::Infer(_)
                | TypeData::UnresolvedTypeName(_)
        ) {
            self.memo.insert(type_id, false);
            return false;
        }
        match self.guard.enter(type_id) {
            crate::recursion::RecursionResult::Entered => {}
            _ => return false,
        }
        let result = self.check_key(&key);
        self.guard.leave(type_id);
        self.memo.insert(type_id, result);
        result
    }

    fn check_key(&mut self, key: &TypeData) -> bool {
        match key {
            TypeData::Intrinsic(_)
            | TypeData::Literal(_)
            | TypeData::Error
            | TypeData::ThisType
            | TypeData::BoundParameter(_)
            | TypeData::Lazy(_)
            | TypeData::Recursive(_)
            | TypeData::TypeQuery(_)
            | TypeData::UniqueSymbol(_)
            | TypeData::ModuleNamespace(_)
            // Do NOT traverse into TypeParameter constraints/defaults — that's
            // the whole point of the "shallow" variant. We only check if the
            // type parameter itself matches, not what its constraint contains.
            | TypeData::TypeParameter(_)
            | TypeData::Infer(_)
            | TypeData::UnresolvedTypeName(_) => false,
            TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
                let shape = self.types.object_shape(*shape_id);
                shape.properties.iter().any(|p| self.check(p.type_id))
                    || shape
                        .string_index
                        .as_ref()
                        .is_some_and(|i| self.check(i.value_type))
                    || shape
                        .number_index
                        .as_ref()
                        .is_some_and(|i| self.check(i.value_type))
            }
            TypeData::Union(list_id) | TypeData::Intersection(list_id) => {
                let members = self.types.type_list(*list_id);
                members.iter().any(|&m| self.check(m))
            }
            TypeData::Array(elem) => self.check(*elem),
            TypeData::Tuple(list_id) => {
                let elements = self.types.tuple_list(*list_id);
                elements.iter().any(|e| self.check(e.type_id))
            }
            TypeData::Function(shape_id) => {
                let shape = self.types.function_shape(*shape_id);
                shape.params.iter().any(|p| self.check(p.type_id))
                    || self.check(shape.return_type)
                    || shape.this_type.is_some_and(|t| self.check(t))
            }
            TypeData::Callable(shape_id) => {
                let shape = self.types.callable_shape(*shape_id);
                shape.call_signatures.iter().any(|s| {
                    s.params.iter().any(|p| self.check(p.type_id))
                        || self.check(s.return_type)
                        || s.this_type.is_some_and(|t| self.check(t))
                }) || shape.construct_signatures.iter().any(|s| {
                    s.params.iter().any(|p| self.check(p.type_id))
                        || self.check(s.return_type)
                        || s.this_type.is_some_and(|t| self.check(t))
                }) || shape.properties.iter().any(|p| self.check(p.type_id))
            }
            TypeData::Application(app_id) => {
                let app = self.types.type_application(*app_id);
                app.args.iter().any(|&a| self.check(a))
            }
            TypeData::Conditional(cond_id) => {
                let cond = self.types.get_conditional(*cond_id);
                self.check(cond.check_type)
                    || self.check(cond.extends_type)
                    || self.check(cond.true_type)
                    || self.check(cond.false_type)
            }
            TypeData::Mapped(mapped_id) => {
                let mapped = self.types.get_mapped(*mapped_id);
                mapped.type_param.constraint.is_some_and(|c| self.check(c))
                    || mapped.type_param.default.is_some_and(|d| self.check(d))
                    || self.check(mapped.constraint)
                    || self.check(mapped.template)
                    || mapped.name_type.is_some_and(|n| self.check(n))
            }
            TypeData::IndexAccess(obj, idx) => self.check(*obj) || self.check(*idx),
            TypeData::TemplateLiteral(list_id) => {
                let spans = self.types.template_list(*list_id);
                spans.iter().any(|span| {
                    if let crate::types::TemplateSpan::Type(type_id) = span {
                        self.check(*type_id)
                    } else {
                        false
                    }
                })
            }
            TypeData::KeyOf(inner) | TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner) => {
                self.check(*inner)
            }
            TypeData::StringIntrinsic { type_arg, .. } => self.check(*type_arg),
            TypeData::Enum(_def_id, member_type) => self.check(*member_type),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intern::TypeInterner;
    use crate::types::TypeParamInfo;

    fn traversal_guard() -> crate::recursion::RecursionGuard<TypeId> {
        crate::recursion::RecursionGuard::with_profile(
            crate::recursion::RecursionProfile::ShallowTraversal,
        )
    }

    #[test]
    fn predicate_checker_memo_entry_counts_are_observable() {
        let interner = TypeInterner::new();
        let t_name = interner.intern_string("T");
        let u_name = interner.intern_string("U");
        let t_param = interner.type_param(TypeParamInfo::simple(t_name));
        let u_infer = interner.infer(TypeParamInfo::simple(u_name));
        let wrapper = interner.readonly_type(t_param);

        let mut contains_checker = ContainsTypeChecker {
            types: &interner,
            predicate: |key| matches!(key, TypeData::TypeParameter(_)),
            memo: FxHashMap::default(),
            guard: traversal_guard(),
        };
        assert!(contains_checker.check(wrapper));
        assert!(contains_checker.memo_entries() > 0);

        let mut free_type_param_checker = FreeTypeParamChecker {
            types: &interner,
            memo: FxHashMap::default(),
            guard: traversal_guard(),
        };
        assert!(free_type_param_checker.check(wrapper));
        assert!(free_type_param_checker.memo_entries() > 0);

        let mut free_infer_checker = FreeInferChecker {
            types: &interner,
            memo: FxHashMap::default(),
            guard: traversal_guard(),
        };
        assert!(free_infer_checker.check(u_infer));
        assert!(free_infer_checker.memo_entries() > 0);

        let mut shallow_checker = ShallowContainsTypeChecker {
            types: &interner,
            name: t_name,
            memo: FxHashMap::default(),
            guard: traversal_guard(),
        };
        assert!(shallow_checker.check(wrapper));
        assert!(shallow_checker.memo_entries() > 0);
    }
}
