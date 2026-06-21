//! Canonical, policy-parameterized child enumeration for `TypeData`.
//!
//! Historically the solver carried several hand-rolled enumerations of all
//! `TypeData` variants (`for_each_child`, the deep content-predicate walkers,
//! the free-parameter walkers, the cached content walker, the error-containment
//! walker), each visiting a slightly different child set and kept in sync only
//! by comments. This module replaces them with a single enumerator,
//! [`try_for_each_child_with_policy`], parameterized by an explicit
//! [`ChildPolicy`] that makes each walker's deliberate child-set differences
//! visible at the type level instead of buried in copy-pasted match arms.
//!
//! Every walker is a thin driver over this enumerator: memoization, recursion
//! guards, and short-circuiting stay per-driver; the child set is policy.

use std::ops::ControlFlow;

use crate::construction::TypeDatabase;
use crate::types::{IndexSignature, ParamInfo, PropertyInfo, TypeParamInfo};
use crate::{TypeData, TypeId};

/// Which child `TypeId`s a traversal descends into for each `TypeData` variant.
///
/// Members and structural children that every walker agrees on (union and
/// intersection members, tuple elements, array/`KeyOf`/`ReadonlyType`/`NoInfer`
/// inners, conditional branches, mapped `constraint`/`template`/`name_type`,
/// indexed-access operands, template-literal type spans, string-intrinsic
/// arguments, enum member types, application arguments, object property read
/// types, object index-signature value types, signature parameter and return
/// types) are always visited. The flags below gate the child positions where
/// the walkers deliberately differ.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChildPolicy {
    /// Visit an `Application`'s base type (arguments are always visited).
    ///
    /// Predicate walkers skip the base: the base definition's own type
    /// parameters are bound by the application's arguments, so e.g.
    /// `A<number>` must not count as "containing type parameters".
    pub application_base: bool,
    /// Visit a bare `TypeParameter`/`Infer` node's `constraint`.
    pub type_param_constraint: bool,
    /// Visit a bare `TypeParameter`/`Infer` node's `default`.
    pub type_param_default: bool,
    /// Visit a mapped type's iteration-variable `constraint`/`default`
    /// metadata (`mapped.type_param`); the mapped `constraint`, `template`,
    /// and `name_type` are always visited.
    pub mapped_type_param_metadata: bool,
    /// Skip the entire body (params, return, `this`, predicate, metadata) of a
    /// *generic* signature. Free-occurrence walkers use this: a generic
    /// signature binds its own type parameters, so references inside its body
    /// are not free occurrences from the enclosing scope.
    pub skip_generic_signature_bodies: bool,
    /// Visit property `write_type`s (read types are always visited).
    pub property_write_types: bool,
    /// Visit index-signature `key_type`s (value types are always visited).
    pub index_key_types: bool,
    /// Visit a `Callable`'s string/number index signatures at all.
    pub callable_index_signatures: bool,
    /// Visit signature `this` parameter types.
    pub signature_this_type: bool,
    /// Visit signature type-predicate types (`x is T`).
    pub signature_type_predicate: bool,
    /// Visit signature type-parameter `constraint`/`default` metadata.
    pub signature_type_param_metadata: bool,
    /// Visit the operand structure of deferred type-level operations:
    /// conditional check/extends/branches, mapped types, indexed-access
    /// operands, `keyof`, template-literal spans, and string intrinsics.
    ///
    /// Error containment turns this off: those operands are unevaluated
    /// alternatives, not committed structure — an error inside an unselected
    /// conditional branch (e.g. React's `LibraryManagedAttributes` internals)
    /// is not an error in the type until evaluation selects it.
    pub deferred_operations: bool,
}

impl ChildPolicy {
    /// The full structural surface as visited by [`super::visitor::for_each_child`].
    ///
    /// Note the historical asymmetry preserved here: a bare `TypeParameter`'s
    /// `default` is not visited, while signature and mapped type-parameter
    /// defaults are. Callers that need defaults too use [`Self::EVERYTHING`].
    pub const FULL: Self = Self {
        application_base: true,
        type_param_constraint: true,
        type_param_default: false,
        mapped_type_param_metadata: true,
        skip_generic_signature_bodies: false,
        property_write_types: true,
        index_key_types: true,
        callable_index_signatures: true,
        signature_this_type: true,
        signature_type_predicate: true,
        signature_type_param_metadata: true,
        deferred_operations: true,
    };

    /// Every child position, including bare type-parameter defaults. Used by
    /// the recursive type collector, where "reachable anywhere" is the
    /// correct notion.
    pub const EVERYTHING: Self = Self {
        type_param_default: true,
        ..Self::FULL
    };

    /// Child set of the deep content-predicate walkers
    /// (`contains_type_matching` and the project-cached content walker):
    /// bare type-parameter `constraint` and `default` are both visited.
    ///
    /// `application_base: false` is semantic: the base definition's own type
    /// parameters are bound by the application's arguments. The remaining
    /// exclusions (write types, index keys, callable index signatures,
    /// signature predicate/metadata) are the historical child set of the old
    /// hand-rolled predicate walkers, preserved verbatim under this change's
    /// behavior-preservation constraint — they are not known to be semantic.
    /// Note the resulting asymmetry: an occurrence in an *object* index
    /// signature value is found, the same occurrence in a *callable* index
    /// signature value is not.
    pub const CONTENT_PREDICATE: Self = Self {
        application_base: false,
        type_param_constraint: true,
        type_param_default: true,
        mapped_type_param_metadata: true,
        skip_generic_signature_bodies: false,
        property_write_types: false,
        index_key_types: false,
        callable_index_signatures: false,
        signature_this_type: true,
        signature_type_predicate: false,
        signature_type_param_metadata: false,
        deferred_operations: true,
    };

    /// [`Self::CONTENT_PREDICATE`] for free-type-parameter checks: generic
    /// signature bodies bind their own parameters and are skipped wholesale.
    pub const FREE_TYPE_PARAMS: Self = Self {
        skip_generic_signature_bodies: true,
        ..Self::CONTENT_PREDICATE
    };

    /// Free-`infer` checks: like [`Self::CONTENT_PREDICATE`] but a bare
    /// `TypeParameter`/`Infer` is a leaf — structural `infer` patterns inside a
    /// parameter's `constraint`/`default` are definitional, not live inference
    /// variables.
    pub const FREE_INFER: Self = Self {
        type_param_constraint: false,
        type_param_default: false,
        ..Self::CONTENT_PREDICATE
    };

    /// Free-type-parameter *collection*: generic signature bodies are skipped
    /// and all type-parameter declaration metadata (bare and mapped) is
    /// treated as bound by the host, not as free uses.
    pub const FREE_PARAM_COLLECT: Self = Self {
        type_param_constraint: false,
        type_param_default: false,
        mapped_type_param_metadata: false,
        skip_generic_signature_bodies: true,
        ..Self::CONTENT_PREDICATE
    };

    /// Structural *uses* of types: the full surface minus type-parameter
    /// declaration metadata on mapped types and signatures. Used by
    /// free-occurrence walks that must not treat parameter-declaration
    /// metadata as uses by the enclosing type.
    pub const STRUCTURAL_USES: Self = Self {
        mapped_type_param_metadata: false,
        signature_type_param_metadata: false,
        ..Self::FULL
    };

    /// Error containment: every *committed* structural use position,
    /// including `Application` bases. Two classes of children are not uses:
    /// type-parameter declaration metadata (an unresolved name in a
    /// parameter's `constraint`/`default` is diagnosed at the declaration and
    /// must not poison every contextual use of the parameter itself), and the
    /// operands of deferred type-level operations (an error inside an
    /// unselected conditional/mapped/indexed-access branch is only real once
    /// evaluation selects it — see `deferred_operations`).
    pub const ERROR_CONTAINMENT: Self = Self {
        type_param_constraint: false,
        type_param_default: false,
        deferred_operations: false,
        ..Self::STRUCTURAL_USES
    };

    /// Shallow occurrence checks: the full surface, but a bare
    /// `TypeParameter`/`Infer` is a leaf — its `constraint`/`default` carry
    /// the parameter's declaration metadata, and descending them turns
    /// `{ [K in keyof T]: T[K] }`-style self-references into false cycles.
    pub const SHALLOW: Self = Self {
        type_param_constraint: false,
        type_param_default: false,
        ..Self::FULL
    };

    /// [`Self::STRUCTURAL_USES`] with bare `TypeParameter`/`Infer` as leaves:
    /// no type-parameter declaration metadata anywhere (bare, mapped
    /// iteration variable, or signature). Used by free-occurrence checks that
    /// classify every parameter-declaration position as bound by the host.
    pub const STRUCTURAL_USES_SHALLOW: Self = Self {
        type_param_constraint: false,
        type_param_default: false,
        ..Self::STRUCTURAL_USES
    };
}

/// Whether `key` has any children to enumerate under `policy`.
///
/// This is the shared terminal-kind fast path: drivers use it to skip
/// recursion-guard and memo bookkeeping for leaves. It must stay in lockstep
/// with [`try_for_each_child_with_policy`] by construction: a variant is
/// terminal exactly when the enumerator would invoke the callback zero times
/// for every possible shape of that variant.
pub const fn has_policy_children(key: &TypeData, policy: &ChildPolicy) -> bool {
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
        TypeData::TypeParameter(info) | TypeData::Infer(info) => {
            (policy.type_param_constraint && info.constraint.is_some())
                || (policy.type_param_default && info.default.is_some())
        }
        TypeData::Conditional(_)
        | TypeData::Mapped(_)
        | TypeData::IndexAccess(_, _)
        | TypeData::KeyOf(_)
        | TypeData::TemplateLiteral(_)
        | TypeData::StringIntrinsic { .. } => policy.deferred_operations,
        _ => true,
    }
}

#[inline]
fn visit_signature<B, F: FnMut(TypeId) -> ControlFlow<B>>(
    policy: &ChildPolicy,
    type_params: &[TypeParamInfo],
    params: &[ParamInfo],
    this_type: Option<TypeId>,
    return_type: TypeId,
    type_predicate: Option<TypeId>,
    f: &mut F,
) -> ControlFlow<B> {
    if policy.skip_generic_signature_bodies && !type_params.is_empty() {
        return ControlFlow::Continue(());
    }
    f(return_type)?;
    if policy.signature_this_type
        && let Some(this_type) = this_type
    {
        f(this_type)?;
    }
    if policy.signature_type_predicate
        && let Some(predicate_type) = type_predicate
    {
        f(predicate_type)?;
    }
    for param in params {
        f(param.type_id)?;
    }
    if policy.signature_type_param_metadata {
        for type_param in type_params {
            if let Some(constraint) = type_param.constraint {
                f(constraint)?;
            }
            if let Some(default) = type_param.default {
                f(default)?;
            }
        }
    }
    ControlFlow::Continue(())
}

#[inline]
fn visit_props_and_indexes<B, F: FnMut(TypeId) -> ControlFlow<B>>(
    policy: &ChildPolicy,
    properties: &[PropertyInfo],
    string_index: Option<&IndexSignature>,
    number_index: Option<&IndexSignature>,
    include_index_signatures: bool,
    f: &mut F,
) -> ControlFlow<B> {
    for prop in properties {
        f(prop.type_id)?;
        if policy.property_write_types {
            f(prop.write_type)?;
        }
    }
    if include_index_signatures {
        for sig in [string_index, number_index].into_iter().flatten() {
            if policy.index_key_types {
                f(sig.key_type)?;
            }
            f(sig.value_type)?;
        }
    }
    ControlFlow::Continue(())
}

/// Invoke `f` on each immediate child `TypeId` of `key` selected by `policy`,
/// stopping early when `f` breaks.
///
/// This is the single canonical enumeration of the `TypeData` child graph; all
/// traversal helpers and predicate walkers drive it with their own policy.
#[inline]
pub fn try_for_each_child_with_policy<B, F: FnMut(TypeId) -> ControlFlow<B>>(
    db: &dyn TypeDatabase,
    key: &TypeData,
    policy: &ChildPolicy,
    f: &mut F,
) -> ControlFlow<B> {
    match key {
        // Single nested type
        TypeData::Array(inner) | TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner) => {
            f(*inner)
        }

        // Substitution: traverse both the underlying variable and its implied
        // constraint so generic-content predicates observe the narrowing.
        TypeData::Substitution {
            base_type,
            constraint,
        } => {
            f(*base_type)?;
            f(*constraint)
        }

        TypeData::KeyOf(inner) => {
            if policy.deferred_operations {
                f(*inner)?;
            }
            ControlFlow::Continue(())
        }

        // Composite types with multiple members
        TypeData::Union(list_id) | TypeData::Intersection(list_id) => {
            for &member in db.type_list(*list_id).iter() {
                f(member)?;
            }
            ControlFlow::Continue(())
        }

        // Object types with properties and index signatures
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            let shape = db.object_shape(*shape_id);
            visit_props_and_indexes(
                policy,
                &shape.properties,
                shape.string_index.as_ref(),
                shape.number_index.as_ref(),
                true,
                f,
            )
        }

        TypeData::Tuple(tuple_id) => {
            for elem in db.tuple_list(*tuple_id).iter() {
                f(elem.type_id)?;
            }
            ControlFlow::Continue(())
        }

        TypeData::Function(func_id) => {
            let sig = db.function_shape(*func_id);
            visit_signature(
                policy,
                &sig.type_params,
                &sig.params,
                sig.this_type,
                sig.return_type,
                sig.type_predicate.as_ref().and_then(|p| p.type_id),
                f,
            )
        }

        TypeData::Callable(callable_id) => {
            let callable = db.callable_shape(*callable_id);
            for sig in callable
                .call_signatures
                .iter()
                .chain(callable.construct_signatures.iter())
            {
                visit_signature(
                    policy,
                    &sig.type_params,
                    &sig.params,
                    sig.this_type,
                    sig.return_type,
                    sig.type_predicate.as_ref().and_then(|p| p.type_id),
                    f,
                )?;
            }
            visit_props_and_indexes(
                policy,
                &callable.properties,
                callable.string_index.as_ref(),
                callable.number_index.as_ref(),
                policy.callable_index_signatures,
                f,
            )
        }

        TypeData::Application(app_id) => {
            let app = db.type_application(*app_id);
            if policy.application_base {
                f(app.base)?;
            }
            for &arg in &app.args {
                f(arg)?;
            }
            ControlFlow::Continue(())
        }

        TypeData::Conditional(cond_id) => {
            if !policy.deferred_operations {
                return ControlFlow::Continue(());
            }
            let cond = db.get_conditional(*cond_id);
            f(cond.check_type)?;
            f(cond.extends_type)?;
            f(cond.true_type)?;
            f(cond.false_type)
        }

        TypeData::Mapped(mapped_id) => {
            if !policy.deferred_operations {
                return ControlFlow::Continue(());
            }
            let mapped = db.get_mapped(*mapped_id);
            if policy.mapped_type_param_metadata {
                if let Some(constraint) = mapped.type_param.constraint {
                    f(constraint)?;
                }
                if let Some(default) = mapped.type_param.default {
                    f(default)?;
                }
            }
            f(mapped.constraint)?;
            f(mapped.template)?;
            if let Some(name_type) = mapped.name_type {
                f(name_type)?;
            }
            ControlFlow::Continue(())
        }

        TypeData::IndexAccess(obj, idx) => {
            if !policy.deferred_operations {
                return ControlFlow::Continue(());
            }
            f(*obj)?;
            f(*idx)
        }

        TypeData::TemplateLiteral(template_id) => {
            if !policy.deferred_operations {
                return ControlFlow::Continue(());
            }
            for span in db.template_list(*template_id).iter() {
                if let crate::types::TemplateSpan::Type(type_id) = span {
                    f(*type_id)?;
                }
            }
            ControlFlow::Continue(())
        }

        TypeData::StringIntrinsic { type_arg, .. } => {
            if policy.deferred_operations {
                f(*type_arg)?;
            }
            ControlFlow::Continue(())
        }

        TypeData::TypeParameter(info) | TypeData::Infer(info) => {
            if policy.type_param_constraint
                && let Some(constraint) = info.constraint
            {
                f(constraint)?;
            }
            if policy.type_param_default
                && let Some(default) = info.default
            {
                f(default)?;
            }
            ControlFlow::Continue(())
        }

        TypeData::Enum(_def_id, member_type) => f(*member_type),

        // Leaf types - no children to visit
        TypeData::Intrinsic(_)
        | TypeData::Literal(_)
        | TypeData::Lazy(_)
        | TypeData::Recursive(_)
        | TypeData::BoundParameter(_)
        | TypeData::TypeQuery(_)
        | TypeData::UniqueSymbol(_)
        | TypeData::ThisType
        | TypeData::ModuleNamespace(_)
        | TypeData::UnresolvedTypeName(_)
        | TypeData::Error => ControlFlow::Continue(()),
    }
}

/// Non-early-exit convenience wrapper over [`try_for_each_child_with_policy`].
pub fn for_each_child_with_policy<F>(
    db: &dyn TypeDatabase,
    key: &TypeData,
    policy: &ChildPolicy,
    mut f: F,
) where
    F: FnMut(TypeId),
{
    let _ = try_for_each_child_with_policy::<std::convert::Infallible, _>(
        db,
        key,
        policy,
        &mut |child| {
            f(child);
            ControlFlow::Continue(())
        },
    );
}
