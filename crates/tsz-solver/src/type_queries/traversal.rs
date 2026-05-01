//! Type traversal and property access classification helpers.
//!
//! This module provides classification enums and functions for traversing
//! type structures. These are used by the checker to determine how to walk
//! into nested types for property access resolution, symbol resolution,
//! and diagnostic property name collection — without directly matching on
//! `TypeData` variants.

use crate::def::DefId;
use crate::type_queries::data::{get_callable_shape, get_object_shape};
use crate::types::{IntrinsicKind, TemplateSpan, TypeData, TypeId};
use crate::{TypeDatabase, TypeResolver, TypeSubstitution, instantiate_type};
use rustc_hash::FxHashSet;
use tsz_common::interner::Atom;

// =============================================================================
// TypeTraversalKind - Classification for type structure traversal
// =============================================================================

/// Classification for traversing type structure to resolve symbols.
///
/// This enum is used by `ensure_application_symbols_resolved_inner` to
/// determine how to traverse into nested types without directly matching
/// on `TypeData` in the checker layer.
#[derive(Debug, Clone)]
pub enum TypeTraversalKind {
    /// Application type - resolve base symbol and recurse into base and args
    Application {
        app_id: crate::types::TypeApplicationId,
        base: TypeId,
        args: Vec<TypeId>,
    },
    /// Symbol reference - resolve the symbol
    SymbolRef(crate::types::SymbolRef),
    /// Lazy type reference (`DefId`) - needs resolution before traversal
    Lazy(crate::def::DefId),
    /// Type query (typeof X) - value-space reference that needs resolution
    TypeQuery(crate::types::SymbolRef),
    /// Type parameter - recurse into constraint and default if present
    TypeParameter {
        constraint: Option<TypeId>,
        default: Option<TypeId>,
    },
    /// Union or intersection - recurse into members
    Members(Vec<TypeId>),
    /// Function type - recurse into type params, params, return type, etc.
    Function(crate::types::FunctionShapeId),
    /// Callable type - recurse into signatures and properties
    Callable(crate::types::CallableShapeId),
    /// Object type - recurse into properties and index signatures
    Object(crate::types::ObjectShapeId),
    /// Array type - recurse into element type
    Array(TypeId),
    /// Tuple type - recurse into element types
    Tuple(crate::types::TupleListId),
    /// Conditional type - recurse into check, extends, true, and false types
    Conditional(crate::types::ConditionalTypeId),
    /// Mapped type - recurse into constraint, template, and name type
    Mapped(crate::types::MappedTypeId),
    /// Readonly wrapper - recurse into inner type
    Readonly(TypeId),
    /// Index access - recurse into object and index types
    IndexAccess { object: TypeId, index: TypeId },
    /// `KeyOf` - recurse into inner type
    KeyOf(TypeId),
    /// Template literal - extract types from spans
    TemplateLiteral(Vec<TypeId>),
    /// String intrinsic - traverse the type argument
    StringIntrinsic(TypeId),
    /// Terminal type - no further traversal needed
    Terminal,
}

/// Classify a type for structure traversal (symbol resolution).
///
/// This function examines a type and returns information about how to
/// traverse into its nested types. Used by `ensure_application_symbols_resolved_inner`.
pub fn classify_for_traversal(db: &dyn TypeDatabase, type_id: TypeId) -> TypeTraversalKind {
    if type_id.is_intrinsic() {
        return TypeTraversalKind::Terminal;
    }
    let Some(key) = db.lookup(type_id) else {
        return TypeTraversalKind::Terminal;
    };

    match key {
        TypeData::Application(app_id) => {
            let app = db.type_application(app_id);
            TypeTraversalKind::Application {
                app_id,
                base: app.base,
                args: app.args.clone(),
            }
        }
        TypeData::TypeParameter(info) | TypeData::Infer(info) => TypeTraversalKind::TypeParameter {
            constraint: info.constraint,
            default: info.default,
        },
        TypeData::Union(list_id) | TypeData::Intersection(list_id) => {
            let members = db.type_list(list_id);
            TypeTraversalKind::Members(members.to_vec())
        }
        TypeData::Function(shape_id) => TypeTraversalKind::Function(shape_id),
        TypeData::Callable(shape_id) => TypeTraversalKind::Callable(shape_id),
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            TypeTraversalKind::Object(shape_id)
        }
        TypeData::Array(elem) => TypeTraversalKind::Array(elem),
        TypeData::Tuple(list_id) => TypeTraversalKind::Tuple(list_id),
        TypeData::Conditional(cond_id) => TypeTraversalKind::Conditional(cond_id),
        TypeData::Mapped(mapped_id) => TypeTraversalKind::Mapped(mapped_id),
        TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner) => {
            TypeTraversalKind::Readonly(inner)
        }
        TypeData::IndexAccess(object, index) => TypeTraversalKind::IndexAccess { object, index },
        TypeData::KeyOf(inner) => TypeTraversalKind::KeyOf(inner),
        // Template literal - extract types from spans for traversal
        TypeData::TemplateLiteral(list_id) => {
            let spans = db.template_list(list_id);
            let types: Vec<TypeId> = spans
                .iter()
                .filter_map(|span| match span {
                    TemplateSpan::Type(id) => Some(*id),
                    _ => None,
                })
                .collect();
            if types.is_empty() {
                TypeTraversalKind::Terminal
            } else {
                TypeTraversalKind::TemplateLiteral(types)
            }
        }
        // String intrinsic - traverse the type argument
        TypeData::StringIntrinsic { type_arg, .. } => TypeTraversalKind::StringIntrinsic(type_arg),
        // Lazy type reference - needs resolution before traversal
        TypeData::Lazy(def_id) => TypeTraversalKind::Lazy(def_id),
        // Type query (typeof X) - value-space reference
        TypeData::TypeQuery(symbol_ref) => TypeTraversalKind::TypeQuery(symbol_ref),
        // Terminal types - no nested types to traverse
        TypeData::BoundParameter(_)
        | TypeData::Intrinsic(_)
        | TypeData::Literal(_)
        | TypeData::Recursive(_)
        | TypeData::UniqueSymbol(_)
        | TypeData::ThisType
        | TypeData::ModuleNamespace(_)
        | TypeData::UnresolvedTypeName(_)
        | TypeData::Error
        | TypeData::Enum(_, _) => TypeTraversalKind::Terminal,
    }
}

/// High-level property traversal classification for diagnostics/reporting.
///
/// This keeps traversal-shape branching inside solver queries so checker code
/// can remain thin orchestration.
#[derive(Debug, Clone)]
pub enum PropertyTraversalKind {
    Object(std::sync::Arc<crate::types::ObjectShape>),
    Callable(std::sync::Arc<crate::types::CallableShape>),
    Members(Vec<TypeId>),
    Other,
}

/// Classify a type into a property traversal shape for checker diagnostics.
pub fn classify_property_traversal(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> PropertyTraversalKind {
    match classify_for_traversal(db, type_id) {
        TypeTraversalKind::Object(_) => get_object_shape(db, type_id)
            .map_or(PropertyTraversalKind::Other, PropertyTraversalKind::Object),
        TypeTraversalKind::Callable(_) => get_callable_shape(db, type_id).map_or(
            PropertyTraversalKind::Other,
            PropertyTraversalKind::Callable,
        ),
        TypeTraversalKind::Members(members) => PropertyTraversalKind::Members(members),
        _ => PropertyTraversalKind::Other,
    }
}

/// Collect property names reachable from a type for diagnostics/suggestions.
///
/// Traversal shape decisions stay in solver so checker can remain orchestration-only.
pub fn collect_property_name_atoms_for_diagnostics(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    max_depth: usize,
) -> Vec<Atom> {
    fn collect_inner(
        db: &dyn TypeDatabase,
        type_id: TypeId,
        out: &mut Vec<Atom>,
        depth: usize,
        max_depth: usize,
    ) {
        if depth > max_depth {
            return;
        }
        match classify_property_traversal(db, type_id) {
            PropertyTraversalKind::Object(shape) => {
                for prop in &shape.properties {
                    out.push(prop.name);
                }
            }
            PropertyTraversalKind::Callable(shape) => {
                for prop in &shape.properties {
                    out.push(prop.name);
                }
            }
            PropertyTraversalKind::Members(members) => {
                for member in members {
                    collect_inner(db, member, out, depth + 1, max_depth);
                }
            }
            PropertyTraversalKind::Other => {}
        }
    }

    let mut atoms = Vec::new();
    collect_inner(db, type_id, &mut atoms, 0, max_depth);
    atoms.sort_unstable();
    atoms.dedup();
    atoms
}

pub trait DeclarationTypeCycleHost: TypeResolver {
    fn evaluate_application_for_serialization(&mut self, type_id: TypeId) -> TypeId;

    /// Return true for aliases whose application can be referenced by name in
    /// declaration emit without serializing their source body, such as standard
    /// library aliases.
    fn is_application_alias_serialization_exempt(&self, _base_def_id: DefId) -> bool {
        false
    }
}

fn application_base_def_id(db: &dyn TypeDatabase, type_id: TypeId) -> Option<DefId> {
    let Some(TypeData::Application(app_id)) = db.lookup(type_id) else {
        return None;
    };
    let app = db.type_application(app_id);
    let Some(TypeData::Lazy(def_id)) = db.lookup(app.base) else {
        return None;
    };
    Some(def_id)
}

fn application_contains_nonserializable_recursive_alias<H>(
    db: &dyn TypeDatabase,
    host: &H,
    type_id: TypeId,
) -> bool
where
    H: DeclarationTypeCycleHost,
{
    let Some(target_def_id) = application_base_def_id(db, type_id) else {
        return false;
    };
    if host.is_application_alias_serialization_exempt(target_def_id) {
        return false;
    }
    let Some(TypeData::Application(app_id)) = db.lookup(type_id) else {
        return false;
    };
    let app = db.type_application(app_id);
    let Some(body) = host.resolve_lazy(target_def_id, db) else {
        return false;
    };
    let Some(type_params) = host.get_lazy_type_params(target_def_id) else {
        return false;
    };
    if type_params.is_empty() || body == type_id {
        return false;
    }

    let subst = TypeSubstitution::from_args(db, &type_params, &app.args);
    let instantiated = instantiate_type(db, body, &subst);
    let mut visited = FxHashSet::default();
    contains_recursive_alias_application_in_conditional_branch(
        db,
        host,
        instantiated,
        target_def_id,
        false,
        &mut visited,
    )
}

fn contains_recursive_alias_application_in_conditional_branch<H>(
    db: &dyn TypeDatabase,
    host: &H,
    type_id: TypeId,
    target_def_id: DefId,
    in_conditional_branch: bool,
    visited: &mut FxHashSet<(TypeId, bool)>,
) -> bool
where
    H: DeclarationTypeCycleHost,
{
    let mut stack = vec![(type_id, in_conditional_branch)];
    while let Some((current, in_branch)) = stack.pop() {
        if current == TypeId::ERROR || current == TypeId::ANY {
            continue;
        }
        if !visited.insert((current, in_branch)) {
            continue;
        }

        let Some(key) = db.lookup(current) else {
            continue;
        };
        if let TypeData::Application(app_id) = &key {
            let app = db.type_application(*app_id);
            if in_branch
                && let Some(TypeData::Lazy(def_id)) = db.lookup(app.base)
                && host.defs_are_equivalent(def_id, target_def_id)
            {
                return true;
            }
        }

        if let TypeData::Conditional(cond_id) = &key {
            let cond = db.conditional_type(*cond_id);
            stack.push((cond.check_type, in_branch));
            stack.push((cond.extends_type, in_branch));
            stack.push((cond.true_type, true));
            stack.push((cond.false_type, true));
        } else {
            crate::visitor::for_each_child(db, &key, |child| stack.push((child, in_branch)));
        }
    }
    false
}

/// Check whether a declaration type contains a cyclic structure that cannot be
/// trivially serialized for `.d.ts` emit.
pub fn declaration_type_references_cyclic_structure<H>(
    db: &dyn TypeDatabase,
    host: &mut H,
    type_id: TypeId,
) -> bool
where
    H: DeclarationTypeCycleHost,
{
    fn visit<H>(
        db: &dyn TypeDatabase,
        host: &mut H,
        type_id: TypeId,
        active: &mut FxHashSet<TypeId>,
        finished: &mut FxHashSet<TypeId>,
        in_cond_branch: bool,
    ) -> bool
    where
        H: DeclarationTypeCycleHost,
    {
        if type_id == TypeId::ERROR || type_id == TypeId::ANY {
            return false;
        }
        if finished.contains(&type_id) {
            return false;
        }
        if !active.insert(type_id) {
            // Only report cycle as non-serializable (TS5088) if the path
            // went through a conditional type branch. Cycles through
            // object/function/union types are serializable by the declaration
            // emitter (tsc elides them with a symbol depth limit, no error).
            return in_cond_branch;
        }

        let result = match db.lookup(type_id) {
            Some(TypeData::Recursive(_)) => in_cond_branch,
            Some(TypeData::Lazy(def_id)) => {
                // When the alias is defined outside the file currently being
                // declaration-emitted, the .d.ts emitter can reference it by
                // name and never needs to inline-walk its body for cycle
                // detection. Skip the walk entirely; this is what the
                // `is_application_alias_serialization_exempt` short-circuit
                // does for the Application path, and the same property
                // applies here when Lazy is encountered structurally rather
                // than via an Application boundary.
                if host.is_application_alias_serialization_exempt(def_id) {
                    return false;
                }
                host.resolve_lazy(def_id, db).is_some_and(|resolved| {
                    // Reset in_cond_branch when crossing a Lazy(DefId) boundary.
                    // Lazy types represent named type definitions (interfaces, classes,
                    // type aliases) that the declaration emitter can reference by name
                    // without inlining. When a cycle passes through a named reference,
                    // the emitter can break the cycle by emitting the name instead of
                    // expanding. Only truly inline cycles through conditional branches
                    // (detected via Recursive nodes) are non-serializable.
                    resolved != type_id && visit(db, host, resolved, active, finished, false)
                })
            }
            Some(TypeData::Application(app_id)) => {
                let evaluated = host.evaluate_application_for_serialization(type_id);
                if application_contains_nonserializable_recursive_alias(db, host, type_id)
                    || (evaluated != type_id
                        && application_contains_nonserializable_recursive_alias(
                            db, host, evaluated,
                        ))
                {
                    true
                } else if evaluated != type_id {
                    visit(db, host, evaluated, active, finished, in_cond_branch)
                } else {
                    let app = db.type_application(app_id);
                    visit(db, host, app.base, active, finished, in_cond_branch)
                        || app
                            .args
                            .iter()
                            .copied()
                            .any(|arg| visit(db, host, arg, active, finished, in_cond_branch))
                }
            }
            Some(TypeData::Array(elem))
            | Some(TypeData::ReadonlyType(elem))
            | Some(TypeData::NoInfer(elem))
            | Some(TypeData::KeyOf(elem)) => {
                visit(db, host, elem, active, finished, in_cond_branch)
            }
            Some(TypeData::IndexAccess(object, index)) => {
                visit(db, host, object, active, finished, in_cond_branch)
                    || visit(db, host, index, active, finished, in_cond_branch)
            }
            Some(TypeData::Union(list_id)) | Some(TypeData::Intersection(list_id)) => db
                .type_list(list_id)
                .iter()
                .copied()
                .any(|member| visit(db, host, member, active, finished, in_cond_branch)),
            Some(TypeData::Tuple(list_id)) => db
                .tuple_list(list_id)
                .iter()
                .any(|elem| visit(db, host, elem.type_id, active, finished, in_cond_branch)),
            Some(TypeData::Function(shape_id)) => {
                let shape = db.function_shape(shape_id);
                shape.this_type.is_some_and(|this_type| {
                    visit(db, host, this_type, active, finished, in_cond_branch)
                }) || shape
                    .params
                    .iter()
                    .any(|param| visit(db, host, param.type_id, active, finished, in_cond_branch))
                    || shape.type_params.iter().any(|tp| {
                        tp.constraint.is_some_and(|constraint| {
                            visit(db, host, constraint, active, finished, in_cond_branch)
                        }) || tp.default.is_some_and(|default| {
                            visit(db, host, default, active, finished, in_cond_branch)
                        })
                    })
                    || shape.type_predicate.as_ref().is_some_and(|pred| {
                        pred.type_id.is_some_and(|pred_type| {
                            visit(db, host, pred_type, active, finished, in_cond_branch)
                        })
                    })
                    || visit(
                        db,
                        host,
                        shape.return_type,
                        active,
                        finished,
                        in_cond_branch,
                    )
            }
            Some(TypeData::Callable(shape_id)) => {
                let shape = db.callable_shape(shape_id);
                let mut visit_sig = |sig: &crate::CallSignature,
                                     active: &mut FxHashSet<TypeId>,
                                     finished: &mut FxHashSet<TypeId>|
                 -> bool {
                    sig.this_type.is_some_and(|this_type| {
                        visit(db, host, this_type, active, finished, in_cond_branch)
                    }) || sig.params.iter().any(|param| {
                        visit(db, host, param.type_id, active, finished, in_cond_branch)
                    }) || sig.type_params.iter().any(|tp| {
                        tp.constraint.is_some_and(|constraint| {
                            visit(db, host, constraint, active, finished, in_cond_branch)
                        }) || tp.default.is_some_and(|default| {
                            visit(db, host, default, active, finished, in_cond_branch)
                        })
                    }) || sig.type_predicate.as_ref().is_some_and(|pred| {
                        pred.type_id.is_some_and(|pred_type| {
                            visit(db, host, pred_type, active, finished, in_cond_branch)
                        })
                    }) || visit(db, host, sig.return_type, active, finished, in_cond_branch)
                };

                shape
                    .call_signatures
                    .iter()
                    .any(|sig| visit_sig(sig, active, finished))
                    || shape
                        .construct_signatures
                        .iter()
                        .any(|sig| visit_sig(sig, active, finished))
                    || shape.properties.iter().any(|prop| {
                        visit(db, host, prop.type_id, active, finished, in_cond_branch)
                            || visit(db, host, prop.write_type, active, finished, in_cond_branch)
                    })
                    || shape.string_index.as_ref().is_some_and(|index| {
                        visit(db, host, index.value_type, active, finished, in_cond_branch)
                    })
                    || shape.number_index.as_ref().is_some_and(|index| {
                        visit(db, host, index.value_type, active, finished, in_cond_branch)
                    })
            }
            Some(TypeData::Object(shape_id)) | Some(TypeData::ObjectWithIndex(shape_id)) => {
                let shape = db.object_shape(shape_id);
                shape.properties.iter().any(|prop| {
                    visit(db, host, prop.type_id, active, finished, in_cond_branch)
                        || visit(db, host, prop.write_type, active, finished, in_cond_branch)
                }) || shape.string_index.as_ref().is_some_and(|index| {
                    visit(db, host, index.value_type, active, finished, in_cond_branch)
                }) || shape.number_index.as_ref().is_some_and(|index| {
                    visit(db, host, index.value_type, active, finished, in_cond_branch)
                })
            }
            Some(TypeData::Conditional(cond_id)) => {
                let cond = db.conditional_type(cond_id);
                // Check/extends use current flag; true/false branches set
                // in_cond_branch=true so cycles through conditional branches
                // are reported as TS5088 (matching tsc behavior).
                visit(db, host, cond.check_type, active, finished, in_cond_branch)
                    || visit(
                        db,
                        host,
                        cond.extends_type,
                        active,
                        finished,
                        in_cond_branch,
                    )
                    || visit(db, host, cond.true_type, active, finished, true)
                    || visit(db, host, cond.false_type, active, finished, true)
            }
            Some(TypeData::Mapped(mapped_id)) => {
                let mapped = db.mapped_type(mapped_id);
                visit(
                    db,
                    host,
                    mapped.constraint,
                    active,
                    finished,
                    in_cond_branch,
                ) || visit(db, host, mapped.template, active, finished, in_cond_branch)
                    || mapped.name_type.is_some_and(|name_type| {
                        visit(db, host, name_type, active, finished, in_cond_branch)
                    })
                    || mapped.type_param.constraint.is_some_and(|constraint| {
                        visit(db, host, constraint, active, finished, in_cond_branch)
                    })
                    || mapped.type_param.default.is_some_and(|default| {
                        visit(db, host, default, active, finished, in_cond_branch)
                    })
            }
            Some(TypeData::TypeParameter(info)) | Some(TypeData::Infer(info)) => {
                info.constraint.is_some_and(|constraint| {
                    visit(db, host, constraint, active, finished, in_cond_branch)
                }) || info.default.is_some_and(|default| {
                    visit(db, host, default, active, finished, in_cond_branch)
                })
            }
            Some(TypeData::TemplateLiteral(list_id)) => {
                db.template_list(list_id).iter().any(|span| match span {
                    TemplateSpan::Type(inner) => {
                        visit(db, host, *inner, active, finished, in_cond_branch)
                    }
                    _ => false,
                })
            }
            Some(TypeData::StringIntrinsic { type_arg, .. }) => {
                visit(db, host, type_arg, active, finished, in_cond_branch)
            }
            _ => false,
        };

        active.remove(&type_id);
        if !result {
            finished.insert(type_id);
        }
        result
    }

    let mut active = FxHashSet::default();
    let mut finished = FxHashSet::default();
    visit(db, host, type_id, &mut active, &mut finished, false)
}

/// Collect property names accessible on a type for spelling suggestions.
///
/// For union types, only properties present in ALL members are returned (intersection).
/// This matches tsc: "did you mean" for union access uses only common/accessible properties.
pub fn collect_accessible_property_names_for_suggestion(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    max_depth: usize,
) -> Vec<Atom> {
    if let Some(TypeData::Union(list_id)) = db.lookup(type_id) {
        let members = db.type_list(list_id).to_vec();
        if members.is_empty() {
            return vec![];
        }
        let mut common = collect_property_name_atoms_for_diagnostics(db, members[0], max_depth);
        common.sort_unstable();
        common.dedup();
        for &member in &members[1..] {
            let mut member_props =
                collect_property_name_atoms_for_diagnostics(db, member, max_depth);
            member_props.sort_unstable();
            member_props.dedup();
            common.retain(|a| member_props.binary_search(a).is_ok());
            if common.is_empty() {
                return vec![];
            }
        }
        return common;
    }
    collect_property_name_atoms_for_diagnostics(db, type_id, max_depth)
}

/// Checks if a type is exclusively `null`, `undefined`, or a union of both.
pub fn is_only_null_or_undefined(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id == TypeId::NULL || type_id == TypeId::UNDEFINED {
        return true;
    }
    if type_id.is_intrinsic() {
        return false;
    }
    match db.lookup(type_id) {
        Some(TypeData::Intrinsic(IntrinsicKind::Null | IntrinsicKind::Undefined)) => true,
        Some(TypeData::Union(list_id)) => {
            let members = db.type_list(list_id);
            members.iter().all(|&m| is_only_null_or_undefined(db, m))
        }
        _ => false,
    }
}
